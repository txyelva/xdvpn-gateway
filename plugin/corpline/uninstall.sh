#!/bin/sh
# 公司专线（corpline）卸载脚本 —— 安装时复制为 /koolshare/scripts/uninstall_corpline.sh
# 先撤网络改动、再删文件：即使中途出错，也不会留下把流量导向已不存在隧道的规则。

. /koolshare/scripts/base.sh
module=corpline
RUN=/var/run/corpline

echo_date "停止公司专线并撤掉网络改动…"
/bin/sh /koolshare/scripts/corpline_config.sh stop >/dev/null 2>&1
# 兜底：外壳脚本已损坏或缺失时也要停干净
mkdir -p $RUN && echo stopped >$RUN/gen # 守护程序外层的看护 sh 见此不再拉起
killall -q corpline 2>/dev/null
p=$(cat $RUN/openconnect.pid 2>/dev/null)
[ -n "$p" ] && grep -q openconnect /proc/$p/comm 2>/dev/null && kill -9 $p 2>/dev/null
# 再按固定名字清一遍（配置文件不在时用默认值；守护程序已不在也能执行）
[ -x /koolshare/bin/corpline ] && /koolshare/bin/corpline cleanup -c $RUN/corpline.conf >/dev/null 2>&1
cru d corpline_ensure >/dev/null 2>&1
cru d corpline_cnlist >/dev/null 2>&1
if [ -f /jffs/configs/dnsmasq.d/corpline.conf ]; then
	rm -f /jffs/configs/dnsmasq.d/corpline.conf
	service restart_dnsmasq >/dev/null 2>&1
fi

echo_date "删除插件文件…"
rm -f /koolshare/init.d/S96corpline.sh /koolshare/init.d/N96corpline.sh
rm -f /koolshare/bin/corpline /koolshare/scripts/corpline_config.sh
rm -f /koolshare/webs/Module_corpline.asp /koolshare/res/icon-corpline.png /koolshare/res/corpline-bg.png
rm -rf /koolshare/configs/corpline $RUN /var/lock/corpline.lock
rm -f /tmp/upload/corpline_status.json /tmp/upload/corpline_log.txt /tmp/upload/corpline_log.txt.1

echo_date "清理设置…"
for v in $(dbus list corpline_ | cut -d= -f1); do dbus remove $v; done
for v in $(dbus list softcenter_module_${module} | cut -d= -f1); do dbus remove $v; done

echo_date "卸载完成，网络为正常直连。"
rm -f /koolshare/scripts/uninstall_corpline.sh
