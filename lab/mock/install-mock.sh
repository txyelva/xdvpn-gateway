#!/bin/bash
# 在虚拟机里模拟 koolcenter 软件中心的运行环境：/koolshare、skipd 的 dbus、cru、service、base.sh。
# 只模拟插件用到的那部分接口行为，便于在不碰真机的情况下跑安装、启用、看护、卸载全流程。
set -e
M=$(cd "$(dirname "$0")" && pwd)
mkdir -p /koolshare/bin /koolshare/scripts /koolshare/webs /koolshare/res /koolshare/init.d /koolshare/configs \
	/jffs/configs/dnsmasq.d /tmp/upload /tmp/lab
touch /usr/bin/skipd 2>/dev/null || true
install -m 755 "$M/dbus" /koolshare/bin/dbus
install -m 755 "$M/cru" /koolshare/bin/cru
install -m 755 "$M/service" /koolshare/bin/service
install -m 644 "$M/base.sh" /koolshare/scripts/base.sh
# 路由器用的是 busybox：让脚本里的常用命令命中 busybox 版本，尽量贴近真机行为
for a in awk sed grep date setsid killall wc tail head cut sleep mkdir mv rm cat tr sort uniq printf base64; do
	ln -sf /usr/bin/busybox /koolshare/bin/$a
done
[ -f /tmp/lab/dbus.db ] || : >/tmp/lab/dbus.db
