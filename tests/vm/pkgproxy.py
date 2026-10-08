"""Caching pacman mirror for VM tests: http://10.0.2.2:8766/$repo/os/$arch.

Package files are cached under the given directory (they're immutable);
databases and signatures for databases are always fetched fresh.
Usage: pkgproxy.py <cache-dir> <port> [upstream]
"""
import os
import shutil
import sys
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

CACHE, PORT = sys.argv[1], int(sys.argv[2])
UPSTREAM = sys.argv[3] if len(sys.argv) > 3 else "https://geo.mirror.pkgbuild.com"
os.makedirs(CACHE, exist_ok=True)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        sys.stderr.write("%s %s\n" % (self.command, self.path))

    def _serve(self, head: bool) -> None:
        name = os.path.basename(self.path)
        cacheable = ".pkg.tar." in name and "/" in self.path and ".." not in self.path
        cached = os.path.join(CACHE, name)
        if cacheable and os.path.isfile(cached):
            self.send_response(200)
            self.send_header("Content-Length", str(os.path.getsize(cached)))
            self.end_headers()
            if not head:
                with open(cached, "rb") as f:
                    shutil.copyfileobj(f, self.wfile)
            return
        try:
            req = urllib.request.Request(UPSTREAM + self.path, method="HEAD" if head else "GET")
            with urllib.request.urlopen(req, timeout=60) as up:
                self.send_response(up.status)
                if up.headers.get("Content-Length"):
                    self.send_header("Content-Length", up.headers["Content-Length"])
                self.end_headers()
                if head:
                    return
                tmp = cached + f".part{os.getpid()}-{id(self)}" if cacheable else None
                out = open(tmp, "wb") if tmp else None
                while chunk := up.read(1 << 16):
                    self.wfile.write(chunk)
                    if out:
                        out.write(chunk)
                if out:
                    out.close()
                    os.replace(tmp, cached)
        except urllib.error.HTTPError as e:
            self.send_error(e.code)
        except Exception:
            self.send_error(502)

    def do_GET(self):
        self._serve(False)

    def do_HEAD(self):
        self._serve(True)


ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
