#!/usr/bin/env python3
"""支持 HTTP Range 的静态文件服务器，只给 verify-pack.sh 用。

Python 自带的 http.server 不支持 Range——它会把整个文件发回来，于是「只取一段」
根本没被测到。这个脚本补上 Range，顺便把每次请求实际发出的字节数记进 bytes.log，
用来核对传输量。

不用于生产：线上是 Caddy 的 file_server，它本来就支持 Range。
"""
import http.server, os, re, sys, pathlib

ROOT = pathlib.Path(sys.argv[2]).resolve()
LOG = ROOT / "bytes.log"

class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        path = (ROOT / self.path.lstrip("/")).resolve()
        if not path.is_file() or ROOT not in path.parents and path.parent != ROOT:
            self.send_error(404); return
        size = path.stat().st_size
        rng = self.headers.get("Range")
        start, end = 0, size - 1
        status = 200
        if rng and (m := re.match(r"bytes=(\d+)-(\d*)", rng)):
            start = int(m.group(1))
            end = int(m.group(2)) if m.group(2) else size - 1
            status = 206
        length = end - start + 1
        with LOG.open("a") as f:
            f.write(f"{self.path} {length}\n")
        self.send_response(status)
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(length))
        if status == 206:
            self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
        self.end_headers()
        with path.open("rb") as f:
            f.seek(start)
            self.wfile.write(f.read(length))

http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
