#!/usr/bin/env python3
"""Static server with the cross-origin isolation headers SharedArrayBuffer needs."""
import http.server, sys, functools
class H(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Cache-Control", "no-cache")
        super().end_headers()
    def log_message(self, *a): pass
H.extensions_map.update({".js": "text/javascript", ".wasm": "application/wasm", ".mjs": "text/javascript"})
port = int(sys.argv[2]) if len(sys.argv) > 2 else 8123
http.server.ThreadingHTTPServer(("127.0.0.1", port), functools.partial(H, directory=sys.argv[1])).serve_forever()
