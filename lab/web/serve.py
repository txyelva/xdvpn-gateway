#!/usr/bin/env python3
"""公司专线网页的本机预览服务器（不连路由器，只用标准库）。

    python3 lab/web/serve.py            # http://127.0.0.1:8765/Module_corpline.asp?s=active
    python3 lab/web/serve.py --port 9000

页面参数（只在预览里有效）：
    s=active|watching|fallback|autherr|paused|connecting|off|setup|stale   切换状态并重置设置
    open=r-account,r-devices,r-global,r-notify,r-adv                     预先展开设置行
    log=1                                                                预先展开运行日志
    click=元素id或CSS选择器|…                                             加载后依次点击（等元素出现且可点）
    js=脚本                                                              加载 2.5 秒后执行，结果写到 <html data-js>（配合 --dump-dom）
    noclients=1                                                          模拟读不到在线设备列表
    /_temp/corpline_domains.json                                         最近访问的网站（fixtures/domains.json；插件没运行时 404）
    sec=busy|quiet|error|starting                                        安全防护：有封锁和记录 / 什么都没发生 / 没生效 / 刚启动
    /_temp/corpline_devices.json                                         守护程序认出来的设备名字（fixtures/devices.json）
    /_temp/corpline_security.json                                        安全防护状态（fixtures/security.json；插件没运行时是关闭后的样子）
    net=login|down                                                       模拟登录过期（状态和 dbus 回登录页）/ 路由器连不上（503）
    /_lab/net?m=ok|login|down                                            页面开着时切换上面的模拟
    /_lab/slow?post=秒&status=秒                                          让 POST /_api/ 或状态文件晚回复（测请求进行中的界面）
    POST /_lab/patch {…}                                                 把这些字段盖到之后的状态 JSON 上（{} 清除）

页面里的操作会改内存里的 dbus（和软件中心一样只写收到的键，值为空即删除），并模拟 corpline_config.sh：
  [1] 应用 / [3] 重新启用：旧进程的状态再保留 3 秒（模拟重启耗时），然后新进程（新的 started）
      正在连接 8 秒 → 正在使用公司专线；关闭 → 已关闭（关闭记录里没有 started）
  [0] 关闭：同时清掉 corpline_paused        [2] 立即重试：不换进程，started 不变
  [7] 解封（corpline_sec_unblock=IP 或 all）   [8] 更新恶意地址名单（6 秒后完成）
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
RESTART_DELAY = 3
LOGIN_HTML = '<html><head><script>top.location.href="/Main_Login.asp";</script></head><body></body></html>'

STATE = {"name": "active", "dbus": {}, "sim": None, "noclients": False, "net": "ok",
         "slow_post": 0, "slow_status": 0, "patch": {},
         "sec": "busy", "unblocked": set()}


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
    for k in ("corpline_bad_time",):
        if db.get(k, "").startswith("-"):
            db[k] = str(int(time.time()) + int(db[k]))
    STATE.update(name=name, dbus=db, sim=None, slow_post=0, slow_status=0, patch={}, t0=int(time.time()))


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
    if sim and now < sim["start_at"] and sim.get("prev"):
        # 旧进程还没退出：继续写旧状态（旧的 started）
        return dict(sim["prev"], updated=now)
    if sim:
        events = list(sim["events"]) + (status or {}).get("events", [])
        if sim["state"] == "off":
            status = {"v": 1, "version": "0.1.0", "state": "off", "updated": 0, "events": events}
        else:
            at = sim["start_at"]
            elapsed = now - at
            if elapsed < 8:
                status = copy.deepcopy(load_fixture("connecting")["status"])
                status["since"] = at - now
            else:
                status = copy.deepcopy(load_fixture("active")["status"])
                status["since"] = at + 8 - now
                status["cn_direct"] = STATE["dbus"].get("corpline_cn_direct") == "1"
                events = [{"t": at + 8 - now, "l": "ok", "m": "已连上公司专线，全家流量改走公司线路"}] + events
            status["updated"] = 0
            status["events"] = events
            if sim.get("started"):
                status["started"] = sim["started"]
            else:
                status.pop("started", None)
    if status is None:
        return None
    out = absolutize(status, now)
    # started 是进程启动时刻，整个进程生命期不变：夹具里的相对值按重置时刻换算，不随每次请求变
    if isinstance(out.get("started"), (int, float)) and out["started"] <= 0:
        out["started"] = STATE["t0"] + out["started"]
    return out


def current_domains():
    """最近访问的网站：插件没在运行（没有状态或状态是 off）时没有这个文件；c 按当前已保存的规则重新算。"""
    s = current_status()
    if not s or s.get("state") == "off":
        return None
    with open(os.path.join(FIX, "domains.json"), encoding="utf-8") as f:
        fx = json.load(f)
    db = STATE["dbus"]
    rules = [(d, "direct") for d in re.split(r"[,\s]+", db.get("corpline_direct_domains", "")) if d] + \
            [(d, "overseas") for d in re.split(r"[,\s]+", db.get("corpline_overseas_domains", "")) if d]
    for it in fx["items"]:
        hit = [r for r in rules if it["d"] == r[0] or it["d"].endswith("." + r[0])]
        if hit:
            it["c"] = max(hit, key=lambda r: len(r[0]))[1]
        elif it["c"] != "internal":
            it["c"] = "corp"
    return absolutize(fx, int(time.time()))


def current_security():
    """安全防护状态：时间字段是相对现在的秒数；插件没在运行时守护程序退出前写的是 active=false。"""
    now = int(time.time())
    s = current_status()
    with open(os.path.join(FIX, "security.json"), encoding="utf-8") as f:
        fx = json.load(f)
    kind = STATE["sec"]
    for b in fx["blocked"]:
        b["since"] += now
        b["expires"] += now
    for e in fx["events"]:
        e["t"] += now
    fx["updated"] = now
    fx["enable"] = STATE["dbus"].get("corpline_sec_enable") != "0"
    fx["block_hours"] = int(STATE["dbus"].get("corpline_sec_block_hours") or 24)
    if kind == "quiet":
        fx.update(blocked=[], watch=[], events=[])
        fx["stats"].update(blocked=0, blocks_today=0, alerts_today=0, dropped=0)
    elif kind == "error":
        fx.update(active=False, error="路由器的防火墙不支持按次数计数（缺少 hashlimit）", blocked=[], watch=[])
    elif kind == "starting":
        fx.update(active=False)
    fx["blocked"] = [b for b in fx["blocked"] if b["ip"] not in STATE["unblocked"]]
    fx["stats"]["blocked"] = len(fx["blocked"])
    if not s or s.get("state") == "off" or not fx["enable"]:
        fx.update(active=False, blocked=[], watch=[])
    return fx


def merge(a, b):
    out = dict(a)
    for k, v in b.items():
        out[k] = merge(out[k], v) if isinstance(v, dict) and isinstance(out.get(k), dict) else v
    return out


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


# 中国服务名单状态（corpline_cn_*）：?cn=bundle|download|downloading|failed|nolist|none，配合 cn_direct=1
def set_cn(kind):
    db, now = STATE["dbus"], int(time.time())
    for k in [k for k in db if k.startswith("corpline_cn_") and k != "corpline_cn_direct"]:
        db.pop(k)
    db["corpline_cn_direct"] = "1"
    base = {"corpline_cn_count": "109513", "corpline_cnip_count": "3881", "corpline_cn_time": str(now - 86400 * 3), "corpline_cn_src": "bundle", "corpline_cn_state": "ok"}
    if kind == "bundle":
        db.update(base)
    elif kind == "download":
        db.update(base, corpline_cn_src="download", corpline_cn_time=str(now - 3600 * 5))
    elif kind == "downloading":
        db.update(base, corpline_cn_state="downloading", corpline_cn_tried=str(now))
    elif kind == "failed":
        db.update(base, corpline_cn_state="failed", corpline_cn_tried=str(now - 600), corpline_cn_error="下载超时")
    elif kind == "nolist":
        db.update(corpline_cn_state="failed", corpline_cn_tried=str(now - 120), corpline_cn_error="连不上下载服务器")


def cn_tick():
    db = STATE["dbus"]
    if db.get("corpline_bad_state") == "downloading" and time.time() >= STATE.get("bad_done_at", 0):
        db.update(corpline_bad_state="ok", corpline_bad_count="4652", corpline_bad_time=str(int(time.time())))
    if db.get("corpline_cn_state") == "downloading" and time.time() >= STATE.get("cn_done_at", 0):
        db.update(corpline_cn_state="ok", corpline_cn_count="109795", corpline_cn_time=str(int(time.time())), corpline_cn_src="download")
        db.pop("corpline_cn_error", None)


def simulate(action):
    db, now = STATE["dbus"], int(time.time())
    prev_status = current_status()
    ev = []
    if action == 0:
        db.pop("corpline_paused", None)
        db.pop("corpline_paused_reason", None)
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
    # 只有还在按时写状态的旧进程才会“撑一会儿”；状态过期（进程已经死了）就直接换新进程
    live_prev = prev_status if prev_status and prev_status.get("state") != "off" and now - prev_status.get("updated", 0) <= 30 else None
    if state == "off" or action == 2:
        start_at, started, keep = now, (live_prev or {}).get("started") if action == 2 else None, None
    else:
        # 重启：旧进程（如果还在写状态）再撑 RESTART_DELAY 秒，新进程带新的 started
        start_at = now + (RESTART_DELAY if live_prev else 0)
        started, keep = start_at, live_prev
    STATE["sim"] = {"state": state, "at": now, "start_at": start_at, "started": started, "prev": keep,
                    "events": [{"t": e["t"] - now, "l": e["l"], "m": e["m"]} for e in ev] + prev}


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
            if "cn" in q:
                set_cn(q["cn"][0])
                STATE["cn_done_at"] = time.time() + 3600
            STATE["noclients"] = q.get("noclients", ["0"])[0] == "1"
            if "sec" in q or "s" in q:
                STATE["sec"] = q.get("sec", ["busy"])[0]
                STATE["unblocked"] = set()
            STATE["net"] = q.get("net", ["ok"])[0]
            with open(ASP, encoding="utf-8") as f:
                html = f.read()
            html = html.replace("<% login_ip_str(); %>", ADMIN_IP)
            html = re.sub(r"<%.*?%>", "", html, flags=re.S)
            hooks = []
            for rid in (q.get("open", [""])[0]).split(","):
                if rid:
                    hooks.append("CL.openRow(%s,true,true);" % json.dumps(rid))
            if "click" in q:
                # 交互验证：页面加载后依次点指定按钮（元素 id 或 CSS 选择器，| 分隔）
                hooks.append("window.addEventListener('load',function(){var a=%s.split('|'),i=0,t=0;(function n(){if(i>=a.length)return;var s=a[i],e=document.getElementById(s)||document.querySelector(s);if(e&&!e.disabled){i++;t=0;e.click();setTimeout(n,300);}else if(++t<40)setTimeout(n,200);})();});" % json.dumps(q["click"][0]))
            if "js" in q:
                # 交互验证：页面加载 2.5 秒后执行一段脚本，返回值或异常写到 <html data-js>，用 --dump-dom 读
                hooks.append("window.addEventListener('load',function(){setTimeout(function(){var r;try{r=String(eval(%s));}catch(x){r='ERR '+x;}document.documentElement.setAttribute('data-js',r);},2500);});" % json.dumps(q["js"][0]))
            if q.get("log", ["0"])[0] == "1":
                hooks.append("window.addEventListener('load',function(){setTimeout(function(){document.getElementById('clLogBtn').click();},50);});")
            if hooks:
                # 同步执行：在首帧之前展开，截图时不依赖过渡动画
                html = html.replace("</body>", "<script>%s</script>\n</body>" % "".join(hooks))
            return self.send(200, html, "text/html; charset=utf-8")
        if p == "/_lab/net":
            STATE["net"] = q.get("m", ["ok"])[0]
            return self.json({"net": STATE["net"]})
        if p in ("/_api/corpline_", "/_temp/corpline_status.json") and self.net_down():
            return
        if p == "/_lab/slow":
            STATE["slow_post"] = float(q.get("post", ["0"])[0])
            STATE["slow_status"] = float(q.get("status", ["0"])[0])
            return self.json({"post": STATE["slow_post"], "status": STATE["slow_status"]})
        if p == "/_api/corpline_":
            cn_tick()
            return self.json({"result": [STATE["dbus"]]})
        if p == "/_temp/corpline_status.json":
            if STATE["slow_status"]:
                time.sleep(STATE["slow_status"])
            s = current_status()
            if s is not None and STATE["patch"]:
                s = merge(s, STATE["patch"])
            return self.json(s) if s is not None else self.send(404, "not found")
        if p == "/_temp/corpline_domains.json":
            if self.net_down():
                return
            d = current_domains()
            return self.json(d) if d is not None else self.send(404, "not found")
        if p == "/_temp/corpline_security.json":
            if self.net_down():
                return
            return self.json(current_security())
        if p == "/_temp/corpline_devices.json":
            if STATE["noclients"]:
                return self.send(404, "not found")
            return self.file(os.path.join(FIX, "devices.json"), "application/json")
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

    def net_down(self):
        """登录过期：httpd 回 200 的登录页；连不上：503。"""
        if STATE["net"] == "login":
            self.send(200, LOGIN_HTML, "text/html; charset=utf-8")
            return True
        if STATE["net"] == "down":
            self.send(503, "unavailable")
            return True
        return False

    def do_POST(self):
        path = urlparse(self.path).path
        if path == "/_lab/patch":
            n = int(self.headers.get("Content-Length") or 0)
            STATE["patch"] = json.loads(self.rfile.read(n).decode("utf-8") or "{}")
            return self.json({"patch": STATE["patch"]})
        if path != "/_api/":
            return self.send(404, "not found")
        if STATE["slow_post"]:
            time.sleep(STATE["slow_post"])
        if self.net_down():
            return
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
        if action == 6:
            db.update(corpline_cn_state="downloading", corpline_cn_tried=str(int(time.time())))
            STATE["cn_done_at"] = time.time() + 6
            return self.json({"result": req.get("id")})
        if action == 7:
            ip = db.pop("corpline_sec_unblock", "")
            if ip == "all":
                with open(os.path.join(FIX, "security.json"), encoding="utf-8") as f:
                    STATE["unblocked"] |= {b["ip"] for b in json.load(f)["blocked"]}
            elif ip:
                STATE["unblocked"].add(ip)
            return self.json({"result": req.get("id")})
        if action == 8:
            db.update(corpline_bad_state="downloading")
            STATE["bad_done_at"] = time.time() + 6
            return self.json({"result": req.get("id")})
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
