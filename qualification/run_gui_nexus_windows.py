#!/usr/bin/env python3
"""rust-mirror-gui crate workflow against an owned, native Windows Nexus OSS.

Reuses rust-mirror's Windows Nexus harness unchanged (scripts/ of
rustcommons/rust-mirror, checked out at a pinned commit and passed with
--rust-mirror-scripts):
- nexus(): downloads the official Nexus OSS 3.76.1 win64 bundle, checks its
  published SHA-256, runs it with its bundled JRE on 127.0.0.1, and deletes the
  owned instance afterwards;
- certificates(): a disposable CA, a signed CRL served over local HTTP, and a
  127.0.0.1 server certificate whose CRL distribution point names it;
- tls_proxy(): HTTPS in front of Nexus with that certificate.

This script sets up the same least-privilege layout as rust-mirror's proof (a
raw hosted repository with ALLOW_ONCE, a publisher role limited to it,
anonymous read-only access, and a random replacement admin password). Then it:
1. runs tests/windows_qualification.rs: the crate workflow through
   rust-mirror-gui --worker, publishing to that repository;
2. has a fresh Cargo home fetch and build qualification/demo --locked
   --offline from the published mirror, with TLS and revocation checks on;
3. checks that Nexus itself refuses an overwrite and an anonymous write.

Evidence goes to --evidence: summary.json, requests.json (method, path,
status through the TLS front; no headers), revocation-requests.json, the
worker step evidence and the client output. Credentials are random per run,
passed only through child environments, and never written.

Usage (native Windows only):
  python run_gui_nexus_windows.py --rust-mirror-scripts DIR --root DIR --evidence DIR
"""
import argparse
import hashlib
import json
import os
import platform
import secrets
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent


def run(args, env, log, cwd=REPO, success=True, timeout=3600):
    completed = subprocess.run([str(a) for a in args], cwd=cwd, env=env, capture_output=True,
                               text=True, timeout=timeout)
    log.with_suffix(".stdout").write_text(completed.stdout)
    log.with_suffix(".stderr").write_text(completed.stderr)
    print(completed.stdout[-6000:], completed.stderr[-6000:], sep="\n", flush=True)
    if success != (completed.returncode == 0):
        raise RuntimeError(f"{log.name}: unexpected exit {completed.returncode}")
    return completed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust-mirror-scripts", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Windows":
        parser.error("native Windows only")
    sys.path.insert(0, str(args.rust_mirror_scripts.resolve()))
    from run_nexus_windows import nexus, tls_proxy
    from prove_crates_https import certificates
    from prove_nexus import credentials

    root = args.root.resolve()
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    result = {"status": "starting", "cases": []}
    requests = []

    def case(name, **values):
        result["cases"].append({"case": name, "passed": True, **values})
        print("passed:", name, flush=True)

    try:
        with nexus(root, evidence) as (port, admin_password, identity):
            result["instance"] = identity
            private = root / "private-tmp"
            private.mkdir()
            tempfile.tempdir = str(private)  # TLS keys live inside the owned root
            with certificates() as pki, tls_proxy(port, pki, requests) as tls_port:
                import http.client
                import ssl
                trust = ssl.create_default_context(cafile=str(pki / "ca.pem"))
                admin = {"auth": credentials("admin", admin_password)}

                def request(method, path, data=None, auth=None, text=False):
                    conn = http.client.HTTPSConnection("127.0.0.1", tls_port, context=trust, timeout=120)
                    body = data if isinstance(data, bytes) else data.encode() if text else json.dumps(data).encode() if data is not None else None
                    headers = {"Content-Type": "application/octet-stream" if isinstance(data, bytes) else "text/plain" if text else "application/json"}
                    if auth:
                        headers["Authorization"] = auth
                    try:
                        conn.request(method, path, body=body, headers=headers)
                        response = conn.getresponse()
                        payload = response.read(4 * 1024 * 1024)
                        if response.status == 403 and b"eula" in payload.lower():
                            raise RuntimeError("EULA acceptance required; stopping, nothing accepted")
                        return response.status, payload
                    finally:
                        conn.close()

                def api(method, path, data=None, text=False):
                    status, body = request(method, "/service/rest/v1/" + path, data, admin["auth"], text)
                    assert 200 <= status < 300, (method, path, status)
                    return json.loads(body) if body.strip() else None

                suffix = secrets.token_hex(6)
                repo = "rust-mirror-gui-win-" + suffix
                api("POST", "repositories/raw/hosted", {
                    "name": repo, "online": True,
                    "storage": {"blobStoreName": "default", "strictContentTypeValidation": True, "writePolicy": "ALLOW_ONCE"},
                    "cleanup": {"policyNames": []}, "raw": {"contentDisposition": "ATTACHMENT"}})
                for role, actions in [("publish-" + suffix, ["browse", "read", "add", "edit"]), ("read-" + suffix, ["browse", "read"])]:
                    api("POST", "security/roles", {"id": role, "name": role, "description": "Owned rust-mirror-gui Windows qualification",
                                                   "privileges": [f"nx-repository-view-raw-{repo}-{a}" for a in actions], "roles": []})
                publisher, publisher_password = "publisher-" + suffix, secrets.token_urlsafe(32)
                reader = "reader-" + suffix
                for name, role, password in [(publisher, "publish-" + suffix, publisher_password), (reader, "read-" + suffix, secrets.token_urlsafe(32))]:
                    api("POST", "security/users", {"userId": name, "firstName": "Windows", "lastName": "Qualification",
                                                   "emailAddress": "mirror@example.invalid", "password": password,
                                                   "status": "active", "roles": [role]})
                api("PUT", "security/anonymous", {"enabled": True, "userId": reader, "realmName": "NexusAuthorizingRealm"})
                rotated = secrets.token_urlsafe(32)
                api("PUT", "security/users/admin/change-password", rotated, text=True)
                admin["auth"] = credentials("admin", rotated)
                config = api("GET", "repositories/raw/hosted/" + repo)
                assert config["storage"]["writePolicy"].lower() == "allow_once"
                case("owned_nexus_allow_once_repository_scoped_publisher_anonymous_read_only",
                     repository=repo, write_policy=config["storage"]["writePolicy"])

                ca_file = root / "public-ca.pem"
                shutil.copyfile(pki / "ca.pem", ca_file)
                base = f"https://127.0.0.1:{tls_port}/repository/{repo}/crate-generation/"
                env = {**os.environ, "QUAL_DIR": str(evidence / "work"),
                       "QUAL_LOCK": str(REPO / "qualification/demo/Cargo.lock"),
                       "QUAL_PUBLISH_URL": base, "QUAL_CA_FILE": str(ca_file),
                       "QUAL_PUBLISH_USER": publisher, "QUAL_PUBLISH_PASSWORD": publisher_password}
                run(["cargo", "test", "--release", "--locked", "--test", "windows_qualification", "--",
                     "--ignored", "--nocapture"], env, evidence / "gui-worker-workflow")
                text = (evidence / "work/evidence.json").read_text()
                assert publisher_password not in text and rotated not in text
                case("gui_worker_crate_workflow_published_to_native_nexus", base_url=base)

                # Nexus itself refuses an overwrite (ALLOW_ONCE) and anonymous
                # writes. The overwrite re-sends a published .crate's own bytes,
                # so content-type validation cannot be the reason for refusing.
                mirror = evidence / "work/ws/run-002/mirror"
                crate = next(mirror.glob("cargo/crates/*/*.crate"))
                rel = crate.relative_to(mirror).as_posix()
                original = crate.read_bytes()
                status, _ = request("PUT", f"/repository/{repo}/crate-generation/{rel}", original, credentials(publisher, publisher_password))
                assert status == 400, ("overwrite", status)
                status, body = request("GET", f"/repository/{repo}/crate-generation/{rel}")
                assert status == 200 and body == original, ("original kept", status)
                status, _ = request("PUT", f"/repository/{repo}/anonymous-write", "x", None, text=True)
                assert status in (401, 403), ("anonymous write", status)
                case("nexus_refuses_overwrite_and_anonymous_write_original_kept", overwrite_status=400, path=rel)

                # A fresh Cargo home builds offline from the published mirror.
                client = root / "client"
                (client / "home").mkdir(parents=True)
                shutil.copytree(REPO / "qualification/demo", client / "demo")
                shutil.copyfile(evidence / "work/cargo-config.toml", client / "home/config.toml")
                lock = client / "demo/Cargo.lock"
                before = hashlib.sha256(lock.read_bytes()).hexdigest()
                cenv = {k: v for k, v in os.environ.items() if not k.startswith("CARGO_")}
                cenv.update(CARGO_HOME=str(client / "home"), CARGO_TARGET_DIR=str(client / "target"),
                            CARGO_HTTP_CAINFO=str(ca_file), CARGO_HTTP_SSL_VERIFY="true", CARGO_NET_RETRY="0")
                run(["cargo", "fetch", "--locked"], cenv, evidence / "client-fetch", cwd=client / "demo")
                run(["cargo", "build", "--locked", "--offline"], cenv, evidence / "client-build", cwd=client / "demo")
                ran = run([client / "target/debug/qualification-demo.exe"], cenv, evidence / "client-run", cwd=client / "demo")
                after = hashlib.sha256(lock.read_bytes()).hexdigest()
                assert before == after, "Cargo.lock changed"
                fetched = (evidence / "client-fetch.stderr").read_text().count("(registry `org-mirror`)")
                case("fresh_windows_cargo_home_fetch_locked_build_offline", crates_downloaded=fetched,
                     lock_sha256=after, output=ran.stdout.strip())
                crl = json.loads((pki / "crl-requests.json").read_text())
                (evidence / "revocation-requests.json").write_text(json.dumps(crl) + "\n")
                assert crl, "Windows did not fetch the CRL"
                case("windows_revocation_checked_through_served_crl", crl_fetches=len(crl))
            result["status"] = "passed"
    except Exception as error:
        result["status"] = "failed"
        result["failure"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        (evidence / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
        (evidence / "requests.json").write_text(json.dumps(requests, indent=2) + "\n")


if __name__ == "__main__":
    main()
