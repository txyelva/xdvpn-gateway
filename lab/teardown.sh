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
echo "沙盒已拆除"
