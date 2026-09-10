#!/usr/bin/env python3
"""SPIKE ONLY - not app code.

Stands up an origin that replicates crucible-web's own CSP, frames a "plugin"
on an opaque origin, and proxies /api/* to a real running `cru web`.

    python3 serve.py [--port 8977] [--upstream 127.0.0.1:3000]
"""

import argparse
import http.client
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))

# Copied verbatim from crates/crucible-web/src/server.rs CONTENT_SECURITY_POLICY.
APP_CSP = (
    "default-src 'self'; base-uri 'self'; object-src 'self'; "
    "frame-ancestors 'none'; form-action 'self'; "
    "script-src 'self' 'wasm-unsafe-eval'; worker-src 'self' blob:; "
    "style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https: http:; "
    "font-src 'self' data:; media-src 'self' data: blob:; "
    "frame-src https: http:; connect-src 'self' https:"
)

UPSTREAM = "127.0.0.1:3000"
PORT = 8977


def frame_csp(port):
    """The policy a plugin frame would be served under.

    `sandbox allow-scripts` = opaque origin, script still runs.
    `'self'` is USELESS here (an opaque origin matches no URL), so the script
    source names the serving authority explicitly.
    `connect-src 'none'` = the frame cannot reach the network at all.
    """
    return (
        f"sandbox allow-scripts; default-src 'none'; "
        f"script-src http://127.0.0.1:{port}; "
        f"style-src 'unsafe-inline'; connect-src 'none'"
    )


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def _send(self, body, ctype, csp, code=200):
        if isinstance(body, str):
            body = body.encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Content-Security-Policy", csp)
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _file(self, name, ctype, csp):
        with open(os.path.join(HERE, name), "rb") as f:
            self._send(f.read(), ctype, csp)

    def do_GET(self):
        path = self.path.split("?")[0]
        if path == "/":
            self._file("host.html", "text/html; charset=utf-8", APP_CSP)
        elif path == "/host.js":
            self._file("host.js", "text/javascript; charset=utf-8", APP_CSP)
        elif path == "/frame.html":
            self._file("frame.html", "text/html; charset=utf-8", frame_csp(PORT))
        elif path == "/frame-open.html":
            # Same opaque origin, but NO connect-src restriction: isolates what
            # the origin alone contains from what the CSP contains.
            self._file(
                "frame.html",
                "text/html; charset=utf-8",
                f"sandbox allow-scripts; script-src http://127.0.0.1:{PORT}; "
                "style-src 'unsafe-inline'",
            )
        elif path == "/quiet.html":
            self._file("quiet.html", "text/html; charset=utf-8", frame_csp(PORT))
        elif path == "/quiet.js":
            self._file("quiet.js", "text/javascript; charset=utf-8", APP_CSP)
        elif path == "/plugin.js":
            # Served under the app CSP; the DOCUMENT that loads it is what is
            # sandboxed, and a subresource does not re-derive an origin.
            self._file("plugin.js", "text/javascript; charset=utf-8", APP_CSP)
        elif path.startswith("/bulk"):
            n = int(self.path.split("n=")[-1]) if "n=" in self.path else 2000
            self._send(json.dumps(synthetic_graph(n)), "application/json", APP_CSP)
        elif path.startswith("/api/"):
            self._proxy()
        else:
            self._send("no", "text/plain", APP_CSP, 404)

    def _proxy(self):
        host, port = UPSTREAM.split(":")
        conn = http.client.HTTPConnection(host, int(port), timeout=10)
        conn.request("GET", self.path, headers={"X-Crucible-Plugin": "app"})
        r = conn.getresponse()
        body = r.read()
        self.send_response(r.status)
        self.send_header("Content-Type", r.getheader("Content-Type", "application/json"))
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Content-Security-Policy", APP_CSP)
        self.end_headers()
        self.wfile.write(body)
        conn.close()


def synthetic_graph(n):
    """Roughly the 2 000-note kiln from step 2 of the plan: 2 000 nodes, ~6
    edges each."""
    nodes = [{"path": f"notes/note-{i:05}.md", "title": f"Note {i}", "tags": ["a", "b"]}
             for i in range(n)]
    edges = [{"from": f"notes/note-{i:05}.md", "to": f"notes/note-{(i * 7 + k) % n:05}.md"}
             for i in range(n) for k in range(6)]
    return {"nodes": nodes, "edges": edges}


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8977)
    ap.add_argument("--upstream", default="127.0.0.1:3000")
    args = ap.parse_args()
    PORT = args.port
    UPSTREAM = args.upstream
    ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
