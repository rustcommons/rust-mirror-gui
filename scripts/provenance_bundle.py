#!/usr/bin/env python3
"""Assemble and check a release's provenance bundle file.

The release attaches `<name>-<version>.intoto.jsonl`: JSON Lines, one Sigstore
bundle per line, as written by actions/attest-build-provenance (its SLSA
build-provenance attestation; the same design as rust-mirror). Each bundle
holds a DSSE envelope with an in-toto statement. Scorecard's Signed-Releases check recognizes the `.intoto.jsonl`
suffix, and users can verify offline with
`gh attestation verify FILE --bundle <name>-<version>.intoto.jsonl ...`.

  provenance_bundle.py assemble OUT BUNDLE...   one compact line per bundle
  provenance_bundle.py check JSONL FILE...      the provenance subjects are
                                                exactly these FILEs (by SHA-256)
  provenance_bundle.py match JSONL STORED...    every bundle in JSONL is one of
                                                GitHub's stored attestations
                                                (`gh attestation download` files)

Standard library only.
"""
import base64
import hashlib
import json
import sys

PROVENANCE = "https://slsa.dev/provenance/v1"
IN_TOTO = "application/vnd.in-toto+json"


def statement(bundle):
    env = bundle.get("dsseEnvelope") or {}
    if env.get("payloadType") != IN_TOTO:
        raise ValueError("not an in-toto DSSE envelope")
    if not env.get("signatures"):
        raise ValueError("DSSE envelope without signatures")
    if "verificationMaterial" not in bundle:
        raise ValueError("bundle without verification material")
    return json.loads(base64.b64decode(env["payload"]))


def read_bundles(path):
    with open(path, encoding="utf-8") as f:
        text = f.read()
    try:  # one bundle, possibly pretty-printed
        return [json.loads(text)]
    except json.JSONDecodeError:
        return [json.loads(line) for line in text.splitlines() if line.strip()]


def assemble(out, paths):
    lines, seen = [], set()
    for path in paths:
        for bundle in read_bundles(path):
            st = statement(bundle)
            if st["predicateType"] != PROVENANCE:
                raise ValueError(f"{path}: not a build-provenance bundle")
            if key(bundle) in seen:  # the same attestation from several files
                continue
            seen.add(key(bundle))
            print(f"{path}: {st['predicateType']}, {len(st['subject'])} subjects")
            lines.append(json.dumps(bundle, separators=(",", ":"), sort_keys=True))
    if not lines:
        sys.exit("no bundles")
    # Bytes with "\n" only: the same file on every OS (no CRLF on Windows).
    with open(out, "wb") as f:
        f.write(("\n".join(lines) + "\n").encode("utf-8"))
    print(f"wrote {out}: {len(lines)} bundles")


def check(jsonl, files):
    covered = {}
    with open(jsonl, encoding="utf-8") as f:
        lines = [l for l in f.read().splitlines() if l.strip()]
    for line in lines:
        st = statement(json.loads(line))  # one JSON object per line
        if st["predicateType"] != PROVENANCE:
            sys.exit(f"{jsonl}: a bundle that isn't build provenance ({st['predicateType']})")
        for s in st["subject"]:
            covered[s["digest"]["sha256"]] = s["name"]
    wanted = {}
    for path in files:
        with open(path, "rb") as f:
            wanted[hashlib.sha256(f.read()).hexdigest()] = path
    missing = sorted(p for d, p in wanted.items() if d not in covered)
    extra = sorted(n for d, n in covered.items() if d not in wanted)
    if missing or extra:
        sys.exit(f"{jsonl}: subjects differ from the files; not covered: {missing}; not a file here: {extra}")
    for d, p in sorted(wanted.items(), key=lambda x: x[1]):
        print(f"subject: {p} ({d[:16]}...)")
    print(f"{jsonl}: {len(lines)} bundle(s); subjects are exactly the {len(files)} files")


def key(bundle):
    env = bundle["dsseEnvelope"]
    return (env["payload"], tuple(sig["sig"] for sig in env["signatures"]))


def match(jsonl, stored_paths):
    # gh writes "sha256:<digest>.jsonl" (Linux, macOS) or "sha256-<digest>.jsonl"
    # (Windows, where ":" would make an NTFS stream); callers pass a glob.
    import os
    found = [p for p in stored_paths if os.path.isfile(p)]
    if not found:
        sys.exit("no stored attestations were downloaded (nothing to match against)")
    stored = {key(b) for p in found for b in read_bundles(p)}
    attached = read_bundles(jsonl)
    unknown = [n for n, b in enumerate(attached, 1) if key(b) not in stored]
    if unknown:
        sys.exit(f"{jsonl}: bundle(s) {unknown} are not among GitHub's stored attestations")
    print(f"{jsonl}: all {len(attached)} bundles are GitHub's stored attestations")


if __name__ == "__main__":
    commands = {"assemble": assemble, "check": check, "match": match}
    if len(sys.argv) < 4 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    commands[sys.argv[1]](sys.argv[2], sys.argv[3:])
