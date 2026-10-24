# 假的飞书 / Bark 推送接口：/feishu/ok 回 code 0，/feishu/bad 回 HTTP 200 + 错误码（与飞书一致），
# /bark/... 回 code 200。收到的内容追加到 /tmp/lab/hook.log。
import http.server, json

class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length") or 0)).decode("utf-8", "replace")
        with open("/tmp/lab/hook.log", "a") as f:
            f.write(self.path + " " + body + "\n")
        if self.path.startswith("/feishu/ok"):
            r = {"StatusCode": 0, "StatusMessage": "success", "code": 0, "data": {}, "msg": "success"}
        elif self.path.startswith("/feishu/"):
            r = {"code": 19021, "data": {}, "msg": "sign match fail or timestamp is not within one hour from current time"}
        else:
            r = {"code": 200, "message": "success"}
        b = json.dumps(r).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def log_message(self, *a):
        pass

http.server.ThreadingHTTPServer(("100.64.2.81", 8080), H).serve_forever()
