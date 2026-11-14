"""Local HTTPS stand-in for a Nexus raw hosted repository (test fixture only).

TLS material comes from rust-mirror's own fixture helper,
`certificates()` in rust-mirror's scripts/prove_crates_https.py (passed in by
path, pinned by the workflow). It makes a disposable CA, a signed CRL served
over local HTTP, and a 127.0.0.1 server certificate whose CRL distribution
point names that CRL, so Windows clients can keep revocation checking on.

Repository behavior, as rust-mirror's publisher expects from a "no redeploy"
raw repository:
- HEAD and GET are anonymous (Cargo clients read without credentials);
- PUT needs HTTP Basic credentials, and refuses with 412 when the path already
  exists and the request carries `If-None-Match: *`;
- files are stored under ROOT/<url path>.

Every request is appended to OUT/requests.jsonl (method, path, status); the
Authorization header and credentials are never logged. The CA certificate is
copied to OUT/ca.pem, and OUT/ready is written once the server listens.
Create OUT/stop to shut down cleanly; the CRL fetch log is then copied to
OUT/revocation-requests.json.

Usage: python -I nexus_fixture.py PROVE_CRATES_HTTPS_PY ROOT PORT OUT
Credentials come from FIXTURE_USER and FIXTURE_PASSWORD. Python 3.11+.
"""

import base64
import http.server
import importlib.util
import json
import os
import shutil
import ssl
import sys
import threading
import time
import urllib.parse
from pathlib import Path

HELPER, ROOT, PORT, OUT = sys.argv[1:5]
OUT = Path(OUT)
OUT.mkdir(parents=True, exist_ok=True)
EXPECTED = "Basic " + base64.b64encode(
    f"{os.environ['FIXTURE_USER']}:{os.environ['FIXTURE_PASSWORD']}".encode()
).decode()
LOCK = threading.Lock()

spec = importlib.util.spec_from_file_location("prove_crates_https", HELPER)
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


def local_path(url_path):
    parts = [p for p in urllib.parse.unquote(urllib.parse.urlsplit(url_path).path).split("/") if p]
    if any(p in (".", "..") or "\\" in p or ":" in p for p in parts):
        return None
    return os.path.join(ROOT, *parts)


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def record(self, status):
        with LOCK, open(OUT / "requests.jsonl", "a", encoding="utf-8") as f:
            f.write(json.dumps({"method": self.command, "path": self.path, "status": status}) + "\n")

    def reply(self, status, body=b"", content_type="text/plain"):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)
        self.record(status)

    def do_HEAD(self):
        self.do_GET()

    def do_GET(self):
        path = local_path(self.path)
        if path is None or not os.path.isfile(path):
            return self.reply(404, b"not found")
        with open(path, "rb") as f:
            data = f.read()
        self.reply(200, data, "application/octet-stream")

    def do_PUT(self):
        length = int(self.headers.get("Content-Length", "0"))
        data = self.rfile.read(length)
        if self.headers.get("Authorization") != EXPECTED:
            return self.reply(401, b"unauthorized")
        path = local_path(self.path)
        if path is None:
            return self.reply(400, b"bad path")
        with LOCK:
            exists = os.path.exists(path)
        if exists and self.headers.get("If-None-Match") == "*":
            return self.reply(412, b"exists")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(data)
        self.reply(201, b"created")


with helper.certificates() as pki:
    shutil.copyfile(pki / "ca.pem", OUT / "ca.pem")
    server = http.server.ThreadingHTTPServer(("127.0.0.1", int(PORT)), Handler)
    server.daemon_threads = True
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.load_cert_chain(pki / "server.pem", pki / "server.key")
    server.socket = context.wrap_socket(server.socket, server_side=True)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    (OUT / "ready").write_text(f"https://127.0.0.1:{PORT}/\n")
    print(f"fixture listening on https://127.0.0.1:{PORT}/", flush=True)
    try:
        while not (OUT / "stop").exists():
            time.sleep(0.5)
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=10)
        shutil.copyfile(pki / "crl-requests.json", OUT / "revocation-requests.json")
print("fixture stopped", flush=True)
