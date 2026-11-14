"""Local HTTPS stand-in for a Nexus raw hosted repository (test fixture only).

Behaves the way rust-mirror's publisher expects a "no redeploy" raw repository
to behave:
- HEAD and GET are anonymous (Cargo clients read without credentials);
- PUT needs HTTP Basic credentials, and refuses with 412 when the path already
  exists and the request carries `If-None-Match: *`;
- files are stored under ROOT/<url path>.

Every request is appended to LOG as one JSON line (method, path, status). The
Authorization header and credentials are never logged.

Usage: python -I nexus_fixture.py ROOT CERT KEY PORT LOG
Credentials come from FIXTURE_USER and FIXTURE_PASSWORD in the environment.
Python 3.9+ standard library only.
"""

import base64
import http.server
import json
import os
import ssl
import sys
import threading
import urllib.parse

ROOT, CERT, KEY, PORT, LOG = sys.argv[1:6]
EXPECTED = "Basic " + base64.b64encode(
    f"{os.environ['FIXTURE_USER']}:{os.environ['FIXTURE_PASSWORD']}".encode()
).decode()
LOCK = threading.Lock()


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
        with LOCK, open(LOG, "a", encoding="utf-8") as f:
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


server = http.server.ThreadingHTTPServer(("127.0.0.1", int(PORT)), Handler)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(CERT, KEY)
server.socket = context.wrap_socket(server.socket, server_side=True)
print(f"fixture listening on https://127.0.0.1:{PORT}/", flush=True)
server.serve_forever()
