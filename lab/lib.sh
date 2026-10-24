# 沙盒测试的公共函数（由 run.sh source）
S=/tmp/lab
ST=/tmp/upload/corpline_status.json
rt() { ip netns exec rt "$@"; }
lan1() { ip netns exec lan1 "$@"; }
lan2() { ip netns exec lan2 "$@"; }
corp() { ip netns exec corp "$@"; }
wan() { ip netns exec wan "$@"; }

state() { jq -r '.state // "none"' $ST 2>/dev/null || echo none; }
# 等到状态变成 $1（可写多个，用 | 分隔），最多 $2 秒；返回用时
wait_state() {
	local want=$1 max=$2 t0=$(date +%s)
	while :; do
		local s=$(state)
		if echo "|$want|" | grep -q "|$s|"; then echo $(($(date +%s) - t0)); return 0; fi
		[ $(($(date +%s) - t0)) -ge $max ] && { echo "超时(当前 $s)"; return 1; }
		sleep 1
	done
}
# 从设备访问 $2，返回走的线路：tunnel / direct / fail
path() {
	local out=$(ip netns exec $1 curl -s -m ${3:-4} http://$2/ 2>/dev/null)
	case "$out" in
	*" 100.64.1.10") echo tunnel ;;
	*" 100.64.0.2") echo direct ;;
	*" 10.235."*) echo tunnel ;;
	"") echo fail ;;
	*) echo "other:$out" ;;
	esac
}
# 设备经路由器解析 $2，返回 IP
resolve() { ip netns exec ${3:-lan1} dig +short +time=3 +tries=1 $2 ${4:-A} @192.168.50.1 | tail -n1; }
# which.test 由哪个 DNS 回答：company / overseas / isp（每次用新的子域名，避开 dnsmasq 缓存）
which_dns() {
	case "$(resolve x r$RANDOM$RANDOM.which.test ${1:-lan1})" in
	203.0.113.80) echo company ;; 203.0.113.81) echo overseas ;; 100.64.2.80) echo isp ;; *) echo none ;;
	esac
}
api() { # 模拟网页调用：api <动作>
	rt busybox sh /koolshare/scripts/corpline_config.sh $RANDOM $1 >>$S/api.log 2>&1
}
dbset() { /koolshare/bin/dbus set "$1"; }
snapshot() { # 网络与文件状态快照（用于"零残留"对比）
	{
		echo "## iptables"; rt iptables-save | grep -v '^#' | sed 's/\[[0-9:]*\]//'
		echo "## ip6tables"; rt ip6tables-save | grep -v '^#' | sed 's/\[[0-9:]*\]//'
		echo "## ip rule"; rt ip rule
		echo "## routes"; rt ip route show table all | grep -v "^local\|^broadcast\|fe80\|ff00\|::1\|^multicast"
		echo "## ipset"; rt ipset list -n
		echo "## rp_filter"; for d in all default br0 eth0; do echo "$d=$(rt cat /proc/sys/net/ipv4/conf/$d/rp_filter)"; done
		echo "## dnsmasq.d"; ls /jffs/configs/dnsmasq.d/
		echo "## processes"; pgrep -a "corpline|openconnect" | sed 's/^[0-9]* //' || true
		echo "## cron"; cat $S/cron 2>/dev/null
		echo "## koolshare"; find /koolshare -type f -o -type l | sort
		echo "## tmp"; ls /tmp/upload/ 2>/dev/null | grep -i corpline; ls -d /tmp/corpline* 2>/dev/null
	} 2>&1
}
