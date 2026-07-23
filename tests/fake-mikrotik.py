#!/usr/bin/env python3
"""Minimal in-memory fake of the RouterOS v7 REST API — just the WireGuard
interface + peer endpoints wg-vpng's mikrotik-api client touches, with HTTP
Basic auth. Used by the mikrotik NixOS backend test in place of a real router.
"""
import base64
import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import parse_qs, urlparse

USER, PASS = "admin", "testpass"
interfaces = {}   # id -> obj
peers = {}        # id -> obj
ip4 = {}          # id -> obj (/ip/address)
ip6 = {}          # id -> obj (/ipv6/address) — a separate table, as on RouterOS
_counter = {"n": 0}


def new_id():
    _counter["n"] += 1
    return "*" + str(_counter["n"])


class Handler(BaseHTTPRequestHandler):
    def _authorized(self):
        h = self.headers.get("Authorization", "")
        if not h.startswith("Basic "):
            return False
        try:
            user, pw = base64.b64decode(h[6:]).decode().split(":", 1)
        except Exception:
            return False
        return user == USER and pw == PASS

    def _send(self, code, obj=None):
        body = b"" if obj is None else json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if body:
            self.wfile.write(body)

    def _body(self):
        n = int(self.headers.get("Content-Length", "0") or 0)
        return json.loads(self.rfile.read(n) or b"{}") if n else {}

    def _route(self, method):
        if not self._authorized():
            return self._send(401, {"error": "unauthorized"})
        parsed = urlparse(self.path)
        path = parsed.path.rstrip("/")
        query = parse_qs(parsed.query)

        # ── interface collection ─────────────────────────────────
        if path == "/rest/interface/wireguard":
            if method == "GET":
                name = query.get("name", [None])[0]
                return self._send(200, [v for v in interfaces.values()
                                        if name is None or v.get("name") == name])
            if method == "PUT":
                obj = self._body()
                obj[".id"] = new_id()
                interfaces[obj[".id"]] = obj
                return self._send(201, obj)

        # ── interface item ───────────────────────────────────────
        if path.startswith("/rest/interface/wireguard/") and "/peers" not in path:
            iid = path.rsplit("/", 1)[-1]
            if method == "PATCH":
                interfaces.setdefault(iid, {".id": iid}).update(self._body())
                return self._send(200, interfaces[iid])
            if method == "DELETE":
                interfaces.pop(iid, None)
                return self._send(200)

        # ── peer collection ──────────────────────────────────────
        if path == "/rest/interface/wireguard/peers":
            if method == "GET":
                iface = query.get("interface", [None])[0]
                return self._send(200, [v for v in peers.values()
                                        if iface is None or v.get("interface") == iface])
            if method == "PUT":
                obj = self._body()
                obj[".id"] = new_id()
                peers[obj[".id"]] = obj
                return self._send(201, obj)

        # ── peer item ────────────────────────────────────────────
        if path.startswith("/rest/interface/wireguard/peers/"):
            pid = path.rsplit("/", 1)[-1]
            if method == "PATCH":
                peers.setdefault(pid, {".id": pid}).update(self._body())
                return self._send(200, peers[pid])
            if method == "DELETE":
                peers.pop(pid, None)
                return self._send(200)

        # ── ip / ipv6 address collection + item (separate tables) ─
        store = ip6 if path.startswith("/rest/ipv6/address") else ip4
        if path in ("/rest/ip/address", "/rest/ipv6/address"):
            if method == "GET":
                iface = query.get("interface", [None])[0]
                return self._send(200, [v for v in store.values()
                                        if iface is None or v.get("interface") == iface])
            if method == "PUT":
                obj = self._body()
                obj[".id"] = new_id()
                store[obj[".id"]] = obj
                return self._send(201, obj)
        if path.startswith("/rest/ip/address/") or path.startswith("/rest/ipv6/address/"):
            aid = path.rsplit("/", 1)[-1]
            if method == "PATCH":
                store.setdefault(aid, {".id": aid}).update(self._body())
                return self._send(200, store[aid])
            if method == "DELETE":
                store.pop(aid, None)
                return self._send(200)

        return self._send(404, {"error": "not found", "path": path})

    def do_GET(self):
        self._route("GET")

    def do_PUT(self):
        self._route("PUT")

    def do_PATCH(self):
        self._route("PATCH")

    def do_DELETE(self):
        self._route("DELETE")

    def log_message(self, *_):
        pass


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8081
    HTTPServer(("127.0.0.1", port), Handler).serve_forever()
