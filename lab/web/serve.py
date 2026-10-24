#!/usr/bin/env python3
"""公司专线网页的本机预览服务器（不连路由器，只用标准库）。

    python3 lab/web/serve.py            # http://127.0.0.1:8765/Module_corpline.asp?s=active
    python3 lab/web/serve.py --port 9000

页面参数（只在预览里有效）：
    s=active|watching|fallback|autherr|paused|connecting|off|setup|stale   切换状态并重置设置
    open=r-account,r-devices,r-global,r-notify,r-adv                     预先展开设置行
    log=1                                                                预先展开运行日志
    noclients=1                                                          模拟读不到在线设备列表

页面里的操作会改内存里的 dbus，并模拟 corpline_config.sh：
  [1] 应用：开启/保存/重试 → 正在连接 8 秒 → 正在使用公司专线；关闭 → 已关闭
  [4] 只保存，不改状态     [5] 测试通知：通知关闭或没地址回 nourl，地址里含 "fail" 回 fail
  [1]/[4] 保存过密码 → corpline_has_password=1；按 admin_ip 查 MAC（用客户端列表代替 /proc/net/arp），
          没有就在设备列表最前加一行锁定的「管理这台路由器的电脑」。
"""
import argparse
import base64
import copy
import json
import os
import re
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
ASP = os.path.join(ROOT, "plugin", "corpline", "webs", "Module_corpline.asp")
RES = os.path.join(ROOT, "plugin", "corpline", "res")
STUB = os.path.join(HERE, "stub")
FIX = os.path.join(HERE, "fixtures")
ADMIN_IP = "192.168.50.198"
TIME_KEYS = {"since", "up_since", "updated", "t"}

STATE = {"name": "active", "dbus": {}, "sim": None, "noclients": False}


def load_fixture(name):
    with open(os.path.join(FIX, "state-%s.json" % name), encoding="utf-8") as f:
        return json.load(f)


def encode_devices(lines):
    return base64.b64encode("\n".join(lines).encode()).decode() if lines else ""


def reset(name):
    with open(os.path.join(FIX, "dbus-base.json"), encoding="utf-8") as f:
        db = json.load(f)
    fx = load_fixture(name)
    db.update(fx["dbus"])
    db["corpline_devices"] = encode_devices(db.pop("corpline_devices_lines", []))
    db = {k: v for k, v in db.items() if v != ""}
    STATE.update(name=name, dbus=db, sim=None)


def absolutize(o, now):
    if isinstance(o, dict):
        return {k: (now + v if k in TIME_KEYS and isinstance(v, (int, float)) and v <= 0 else absolutize(v, now)) for k, v in o.items()}
    if isinstance(o, list):
        return [absolutize(x, now) for x in o]
    return o


def current_status():
    now = int(time.time())
    fx = load_fixture(STATE["name"])
    status = fx["status"]
    sim = STATE["sim"]
    if sim:
        events = list(sim["events"]) + (status or {}).get("events", [])
        if sim["state"] == "off":
            status = {"v": 1, "version": "0.1.0", "state": "off", "updated": 0, "events": events}
        else:
            elapsed = now - sim["at"]
            if elapsed < 8:
                status = copy.deepcopy(load_fixture("connecting")["status"])
                status["since"] = sim["at"] - now
            else:
                status = copy.deepcopy(load_fixture("active")["status"])
                status["since"] = sim["at"] + 8 - now
                status["cn_direct"] = STATE["dbus"].get("corpline_cn_direct") == "1"
                events = [{"t": sim["at"] + 8 - now, "l": "ok", "m": "已连上公司专线，全家流量改走公司线路"}] + events
            status["updated"] = 0
            status["events"] = events
    if status is None:
        return None
    return absolutize(status, now)


def save_admin_device():
    """模拟后端 save_admin_device：admin_ip → MAC，缺了就在最前面加锁定行（按 MAC 不分大小写去重）。"""
    db = STATE["dbus"]
    ip = db.get("corpline_admin_ip")
    if not ip:
        return
    with open(os.path.join(FIX, "clientlist.json"), encoding="utf-8") as f:
        cl = json.load(f)["get_clientlist"]
    mac = next((k.upper() for k, v in cl.items() if isinstance(v, dict) and v.get("ip") == ip), None)
    if not mac:
        return
    cur = base64.b64decode(db.get("corpline_devices", "")).decode() if db.get("corpline_devices") else ""
    if any(l.upper().startswith(mac) for l in cur.split("\n")):
        return
    lines = ["%s\t管理这台路由器的电脑\t1" % mac] + [l for l in cur.split("\n") if l]
    db["corpline_devices"] = encode_devices(lines)


def simulate(action):
    db, now = STATE["dbus"], int(time.time())
    ev = []
    if action == 0 or (action == 1 and db.get("corpline_enable") != "1"):
        state = "off"
        ev.append({"t": 0, "l": "info", "m": "已关闭公司专线，全家恢复直连"})
    else:
        if action == 3:
            db["corpline_paused"] = "0"
            db.pop("corpline_paused_reason", None)
        if db.get("corpline_paused") == "1":
            return
        state = "connecting"
        ev.append({"t": 0, "l": "info", "m": "手动重试连接公司线路" if action == 2 else "开始连接公司线路"})
    for e in ev:
        e["t"] = now
    prev = STATE["sim"]["events"] if STATE["sim"] else []
    STATE["sim"] = {"state": state, "at": now, "events": [{"t": e["t"] - now, "l": e["l"], "m": e["m"]} for e in ev] + prev}


class H(BaseHTTPRequestHandler):
    def log_message(self, fmt, *a):
        pass

    def send(self, code, body, ctype="text/plain; charset=utf-8"):
        if isinstance(body, str):
            body = body.encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def json(self, o, code=200):
        self.send(code, json.dumps(o, ensure_ascii=False), "application/json; charset=utf-8")

    def file(self, path, ctype):
        if not os.path.isfile(path):
            return self.send(404, "not found")
        with open(path, "rb") as f:
            self.send(200, f.read(), ctype)

    def do_GET(self):
        u = urlparse(self.path)
        q = parse_qs(u.query)
        p = u.path
        if p in ("/", "/index.html"):
            self.send_response(302)
            self.send_header("Location", "/Module_corpline.asp")
            return self.end_headers()
        if p == "/Module_corpline.asp":
            if "s" in q:
                reset(q["s"][0])
            STATE["noclients"] = q.get("noclients", ["0"])[0] == "1"
            with open(ASP, encoding="utf-8") as f:
                html = f.read()
            html = html.replace("<% login_ip_str(); %>", ADMIN_IP)
            html = re.sub(r"<%.*?%>", "", html, flags=re.S)
            hooks = []
            for rid in (q.get("open", [""])[0]).split(","):
                if rid:
                    hooks.append("CL.openRow(%s,true,true);" % json.dumps(rid))
            if q.get("log", ["0"])[0] == "1":
                hooks.append("window.addEventListener('load',function(){setTimeout(function(){document.getElementById('clLogBtn').click();},50);});")
            if hooks:
                # 同步执行：在首帧之前展开，截图时不依赖过渡动画
                html = html.replace("</body>", "<script>%s</script>\n</body>" % "".join(hooks))
            return self.send(200, html, "text/html; charset=utf-8")
        if p == "/_api/corpline_":
            return self.json({"result": [STATE["dbus"]]})
        if p == "/_temp/corpline_status.json":
            s = current_status()
            return self.json(s) if s is not None else self.send(404, "not found")
        if p == "/_temp/corpline_log.txt":
            return self.file(os.path.join(FIX, "corpline_log.txt"), "text/plain; charset=utf-8")
        if p == "/appGet.cgi":
            if STATE["noclients"]:
                return self.send(500, "error")
            return self.file(os.path.join(FIX, "clientlist.json"), "application/json")
        if p.startswith("/res/icon-corpline"):
            return self.file(os.path.join(RES, os.path.basename(p)), "image/png")
        name = os.path.basename(p)
        if name.endswith(".js") or name.endswith(".css"):
            path = os.path.join(STUB, name)
            ctype = "application/javascript" if name.endswith(".js") else "text/css"
            return self.file(path, ctype) if os.path.isfile(path) else self.send(200, "", ctype)
        return self.send(404, "not found")

    def do_POST(self):
        if urlparse(self.path).path != "/_api/":
            return self.send(404, "not found")
        n = int(self.headers.get("Content-Length") or 0)
        try:
            req = json.loads(self.rfile.read(n).decode("utf-8"))
        except ValueError:
            return self.json({"result": "bad json"}, 400)
        fields = req.get("fields") or {}
        if isinstance(fields, dict):
            for k, v in fields.items():
                if k == "corpline_password":
                    # 和 corpline_config.sh 一样：密码转存后从 dbus 删除，只留 has_password
                    print("password received (base64, %d chars)" % len(v))
                    STATE["dbus"]["corpline_has_password"] = "1"
                    continue
                if v == "":
                    STATE["dbus"].pop(k, None)
                else:
                    STATE["dbus"][k] = v
        action = (req.get("params") or [None])[0]
        print("POST %s params=%s fields=%s" % (req.get("method"), req.get("params"), sorted(fields) if isinstance(fields, dict) else fields))
        db = STATE["dbus"]
        if action == 5:
            if db.get("corpline_notify", "off") == "off" or not db.get("corpline_notify_url"):
                return self.json({"result": "nourl"})
            return self.json({"result": "fail" if "fail" in db["corpline_notify_url"] else req.get("id")})
        if action in (1, 4):
            save_admin_device()
        if action in (0, 1, 2, 3):
            simulate(action)
        return self.json({"result": req.get("id")})


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8765)
    a = ap.parse_args()
    reset("active")
    srv = ThreadingHTTPServer(("127.0.0.1", a.port), H)
    print("公司专线预览：http://127.0.0.1:%d/Module_corpline.asp?s=active" % a.port)
    srv.serve_forever()


if __name__ == "__main__":
    main()
