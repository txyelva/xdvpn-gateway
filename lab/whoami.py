"""沙盒里的网站：回答"我是哪个地址、你从哪个地址来"，用来判断流量走的是哪条线。
/big?mb=N&kbps=M 慢速输出 N MB，用来测"切换线路时正在进行的连接会不会断"。"""
import http.server
import socketserver
import time
import urllib.parse


class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        local = self.connection.getsockname()[0]
        peer = self.client_address[0]
        if u.path == "/big":
            q = urllib.parse.parse_qs(u.query)
            mb = int(q.get("mb", ["5"])[0])
            kbps = int(q.get("kbps", ["200"])[0])
            self.send_response(200)
            self.send_header("Content-Length", str(mb * 1024 * 1024))
            self.end_headers()
            chunk = b"x" * 1024
            try:
                for i in range(mb * 1024):
                    self.wfile.write(chunk)
                    if i % 16 == 15:
                        time.sleep(16 / kbps)
            except (BrokenPipeError, ConnectionResetError):
                pass
            return
        body = f"{local} {peer}\n".encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class S(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True


S(("0.0.0.0", 80), H).serve_forever()
