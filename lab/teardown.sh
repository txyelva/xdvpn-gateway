#!/bin/bash
# 拆掉整个沙盒（只影响虚拟机里的网络命名空间与 /tmp/lab）
S=/tmp/lab
for ns in rt lan1 lan2 corp wan; do
	for p in $(ip netns pids $ns 2>/dev/null); do kill -9 $p 2>/dev/null; done
done
pkill -9 -f "lab/mock/crond.sh" 2>/dev/null
for ns in rt lan1 lan2 corp wan; do ip netns del $ns 2>/dev/null; done
rm -rf $S /etc/netns/rt /etc/netns/lan1 /etc/netns/lan2 /etc/netns/corp /etc/netns/wan
rm -f /var/run/dnsmasq.pid /jffs/configs/dnsmasq.d/corpline.conf
rm -rf /var/run/corpline /var/lock/corpline.lock /tmp/corpline*
# 上一轮没跑到卸载时留在模拟软件中心里的插件文件（否则下一轮"安装前快照"就不干净）
rm -f /koolshare/bin/corpline /koolshare/scripts/corpline_config.sh /koolshare/scripts/uninstall_corpline.sh \
	/koolshare/webs/Module_corpline.asp /koolshare/res/icon-corpline.png /koolshare/res/corpline-bg.png /koolshare/init.d/*corpline*
rm -rf /koolshare/configs/corpline
rm -f /tmp/upload/corpline_*
echo "沙盒已拆除"
