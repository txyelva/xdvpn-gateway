#!/usr/bin/env python3
"""用无头 Chrome 给每个状态截 2x 图，并裁掉页面下方空白。需要先运行 serve.py。

    python3 lab/web/shoot.py                 # 页面全部状态 → lab/web/shots/
    python3 lab/web/shoot.py --proto         # 同时截原型 → lab/web/shots/proto/
"""
import os, subprocess, sys
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "shots")
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
BASE = "http://127.0.0.1:8765/Module_corpline.asp"
PROTO = "file:///Users/chris/Projects/Designs/xdvpn-gateway/prototype/index.html"
STATES = ["active", "watching", "fallback", "autherr", "paused", "connecting", "off", "setup", "stale"]
EXTRA = {
    "active-open": "s=active&open=r-account,r-devices,r-global,r-notify,r-adv",
    "active-log": "s=active&log=1",
    "setup-open": "s=setup&open=r-account,r-devices",
    "active-noclients": "s=active&noclients=1&open=r-devices",
}

def shot(url, path, h=1700):
    subprocess.run([CHROME, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=2", "--force-prefers-reduced-motion",
                    "--window-size=1040,%d" % h, "--virtual-time-budget=4000",
                    "--user-data-dir=/private/tmp/claude-501/chrome-corpline", "--screenshot=" + path, url],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    crop(path)

def crop(path):
    im = Image.open(path).convert("RGB")
    w, h = im.size
    bg = im.getpixel((w - 4, h - 4))
    px = im.load()
    bottom = h
    for y in range(h - 1, 0, -2):
        if any(sum(abs(a - b) for a, b in zip(px[x, y], bg)) > 12 for x in range(0, w, 6)):
            bottom = min(h, y + 60)
            break
    im.crop((0, 0, w, bottom)).save(path)

os.makedirs(os.path.join(OUT, "proto"), exist_ok=True)
for s in STATES:
    shot(BASE + "?s=" + s, os.path.join(OUT, s + ".png"))
for k, q in EXTRA.items():
    shot(BASE + "?" + q, os.path.join(OUT, k + ".png"), 2600)
if "--proto" in sys.argv:
    for s in STATES[:-1]:
        shot(PROTO + "?s=" + s, os.path.join(OUT, "proto", s + ".png"))
    shot(PROTO + "?s=active&open=r-account,r-devices,r-global,r-notify,r-adv", os.path.join(OUT, "proto", "active-open.png"), 2600)
print("done")
