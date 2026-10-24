#!/bin/sh
# 公司专线（corpline）软件中心外壳脚本
#
# 只做三件事：把软件中心 dbus 里的设置翻译成守护程序的配置文件；启动/停止守护程序；
# 每分钟看护一次。所有网络判断都在 /koolshare/bin/corpline 里。
#
# 调用方式：
#   init.d：      S96corpline.sh start | N96corpline.sh start_nat
#   网页 /_api/：  corpline_config.sh <请求id> <动作>
#                 动作：1 应用（重启服务）  2 立即重试  3 重新启用  4 只保存（不影响线路，如通知设置）  5 发测试通知  0 关闭
#   cru 看护：    corpline_config.sh ensure
#   守护程序通知：corpline_config.sh notify "<文字>"
# 注意：本固件上裸 sh 是内存读写工具，脚本互调一律用 /bin/sh。

export KSROOT=/koolshare
. $KSROOT/scripts/base.sh
export PATH=$KSROOT/bin:$KSROOT/scripts:/usr/sbin:/usr/bin:/sbin:/bin:$PATH

BIN=$KSROOT/bin/corpline
RUN=/tmp/corpline
CONF=$RUN/corpline.conf
PIDF=$RUN/corpline.pid
CFGDIR=$KSROOT/configs/corpline
PASSF=$CFGDIR/passwd
LOG=/tmp/upload/corpline_log.txt
LOCK=/tmp/corpline.lock

eval $(dbus export corpline_)

say() {
	mkdir -p /tmp/upload
	echo "$(date '+%H:%M:%S') $*" >>$LOG
}

# 简单互斥：网页连点、cru 与开机同时触发时排队，最多等 20 秒
lock() {
	local i=0
	while ! mkdir $LOCK 2>/dev/null; do
		i=$((i + 1))
		if [ $i -ge 20 ]; then
			# 持锁进程已不在：清掉陈旧锁
			local p=$(cat $LOCK/pid 2>/dev/null)
			if [ -z "$p" ] || [ ! -d /proc/$p ]; then
				rm -rf $LOCK
				continue
			fi
			return 1
		fi
		sleep 1
	done
	echo $$ >$LOCK/pid
	return 0
}
unlock() { rm -rf $LOCK; }

daemon_pid() {
	local p=$(cat $PIDF 2>/dev/null)
	[ -n "$p" ] && [ -d /proc/$p ] && grep -q corpline /proc/$p/comm 2>/dev/null && echo $p
}

b64d() { echo "$1" | base64_decode 2>/dev/null; }

save_password() {
	# 网页把密码 base64 后放进 dbus；这里转存到 600 权限的文件，并从 dbus 删除
	[ -z "$corpline_password" ] && return
	mkdir -p $CFGDIR
	umask 077
	local p=$(b64d "$corpline_password")
	[ -z "$p" ] && p="$corpline_password"
	printf '%s' "$p" >$PASSF
	chmod 600 $PASSF
	dbus set corpline_has_password=1
	dbus remove corpline_password
	unset corpline_password
}

find_openconnect() {
	for f in $KSROOT/bin/openconnect /opt/sbin/openconnect /opt/bin/openconnect /usr/sbin/openconnect; do
		[ -x "$f" ] && echo $f && return
	done
}

find_cafile() {
	for f in /opt/etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt /rom/etc/ssl/certs/ca-certificates.crt; do
		[ -f "$f" ] && echo $f && return
	done
}

# 第一次启用时，把正在操作网页的这台电脑（按 IP 查 MAC）加为锁定的直连设备
save_admin_device() {
	[ -z "$corpline_admin_ip" ] && return
	local mac=$(awk -v ip="$corpline_admin_ip" '$1 == ip && $4 != "00:00:00:00:00:00" {print toupper($4)}' /proc/net/arp | head -n1)
	[ -z "$mac" ] && return
	local cur=$(b64d "$corpline_devices")
	echo "$cur" | grep -qi "^$mac" && return
	local line=$(printf '%s\t%s\t1' "$mac" "管理这台路由器的电脑")
	local all=$(printf '%s\n%s' "$line" "$cur" | sed '/^$/d')
	corpline_devices=$(printf '%s' "$all" | base64_encode)
	dbus set corpline_devices="$corpline_devices"
}

# 直连设备：dbus 里是 base64 的多行 "MAC<TAB>名字<TAB>锁定"，只取 MAC
direct_macs() {
	[ -z "$corpline_devices" ] && return
	b64d "$corpline_devices" | awk -F'\t' '$1 ~ /^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$/ {printf "%s,", toupper($1)}'
}

gen_conf() {
	mkdir -p $RUN /tmp/upload
	umask 077
	local oc=$(find_openconnect)
	local ca=$(find_cafile)
	local pass=""
	[ -f $PASSF ] && pass=$(cat $PASSF)
	{
		echo "server=${corpline_server:-vpn.xindong.com:8443}"
		echo "protocol=${corpline_protocol:-anyconnect}"
		echo "user=$corpline_user"
		echo "password=$pass"
		echo "openconnect=${oc:-/opt/sbin/openconnect}"
		echo "cafile=${corpline_cafile:-$ca}"
		[ -n "$corpline_servercert" ] && echo "servercert=$corpline_servercert"
		[ "$corpline_no_dtls" = "1" ] && echo "no_dtls=1"
		echo "lan_if=${corpline_lan_if:-br0}"
		echo "direct_macs=$(direct_macs)"
		# 安装时已写入默认 apple.com,mzstatic.com；用户清空后这里就是空，不再回到默认
		echo "overseas_domains=$corpline_overseas_domains"
		[ -n "$corpline_overseas_dns" ] && echo "overseas_dns=$corpline_overseas_dns"
		[ -n "$corpline_isp_dns" ] && echo "isp_dns=$corpline_isp_dns"
		[ -n "$corpline_internal_domains" ] && echo "internal_domains=$corpline_internal_domains"
		echo "cn_direct=${corpline_cn_direct:-0}"
		echo "cn_list=$CFGDIR/china-domains.conf"
		echo "sensitivity=${corpline_sensitivity:-normal}"
		echo "hook_cmd=$BIN hook"
		# 通知设置在发送时才读取，改通知不需要重启
		echo "notify_cmd=/bin/sh $KSROOT/scripts/corpline_config.sh notify"
		echo "run_dir=$RUN"
		echo "status_dir=/tmp/upload"
		# 沙盒测试用的覆盖项（真机上不存在这个文件）
		[ -f $CFGDIR/lab.conf ] && cat $CFGDIR/lab.conf
	} >$CONF.tmp
	mv -f $CONF.tmp $CONF
	chmod 600 $CONF
}

start_daemon() {
	gen_conf
	if [ ! -f $PASSF ] || [ -z "$corpline_user" ]; then
		say "还没有填写公司账号或密码，未启动"
		return 1
	fi
	if [ ! -x "$(find_openconnect)" ]; then
		say "找不到 openconnect，请先在 Entware 安装：opkg install openconnect"
		return 1
	fi
	[ -n "$(daemon_pid)" ] && return 0
	rm -f $RUN/stable
	# 子 shell + setsid：守护程序脱离本脚本，脚本退出不影响它。
	# 外面套一层 sh 等它退出（阻塞等待，不占 CPU）：不是我们主动停的（崩溃、被系统杀掉），
	# 立刻撤掉全部网络改动——尤其是 dnsmasq 的转发，否则全家每次解析都要多等 1–5 秒——
	# 再走一次看护（拉起 / 10 分钟内 3 次则自动停用），不必等每分钟的定时看护。
	# 每次启动一个编号：停止或重启会换掉编号，旧的外壳醒来对不上号就什么都不做。
	mkdir -p $RUN
	local gen="$$.$(date +%s)"
	echo $gen >$RUN/gen
	local w="$BIN run -c $CONF; [ \"\$(cat $RUN/gen 2>/dev/null)\" = $gen ] && { rm -f /tmp/upload/corpline_status.json; $BIN cleanup -c $CONF; /bin/sh $KSROOT/scripts/corpline_config.sh ensure; }"
	( setsid /bin/sh -c "$w" >/dev/null 2>&1 </dev/null & ) 2>/dev/null || ( /bin/sh -c "$w" >/dev/null 2>&1 </dev/null & )
	local i=0
	while [ $i -lt 10 ]; do
		[ -n "$(daemon_pid)" ] && break
		sleep 1
		i=$((i + 1))
	done
	[ -n "$(daemon_pid)" ] || { say "守护程序启动失败"; return 1; }
	return 0
}

stop_daemon() {
	mkdir -p $RUN
	echo stopped >$RUN/gen   # 让守护程序外面那层 sh 知道这次退出是主动停的
	local p=$(daemon_pid)
	if [ -n "$p" ]; then
		kill -TERM $p
		local i=0
		while [ $i -lt 15 ] && [ -d /proc/$p ]; do
			sleep 1
			i=$((i + 1))
		done
		[ -d /proc/$p ] && kill -9 $p
	fi
	# 无论守护程序是否正常退出，都再清一遍（幂等），保证不留规则
	[ -f $CONF ] || gen_conf
	$BIN cleanup -c $CONF >/dev/null 2>&1
	killall -q openconnect 2>/dev/null
	rm -f $PIDF
}

hooks_on() {
	mkdir -p $KSROOT/init.d
	ln -sf $KSROOT/scripts/corpline_config.sh $KSROOT/init.d/S96corpline.sh
	ln -sf $KSROOT/scripts/corpline_config.sh $KSROOT/init.d/N96corpline.sh
	cru a corpline_ensure "*/1 * * * * /bin/sh $KSROOT/scripts/corpline_config.sh ensure"
	if [ "$corpline_cn_direct" = "1" ]; then
		cru a corpline_cnlist "17 4 * * 1 /bin/sh $KSROOT/scripts/corpline_config.sh update_cn"
	else
		cru d corpline_cnlist >/dev/null 2>&1
	fi
}

hooks_off() {
	cru d corpline_ensure >/dev/null 2>&1
	cru d corpline_cnlist >/dev/null 2>&1
	rm -f $KSROOT/init.d/S96corpline.sh $KSROOT/init.d/N96corpline.sh
}

pause() {
	dbus set corpline_paused=1
	dbus set corpline_paused_reason="$1"
	say "已自动停用：$1"
	stop_daemon
	do_notify "已自动停用（$1），全家已恢复直连。到路由器的插件页面点「重新启用」即可。"
	hooks_off
	# 开机钩子保留，便于用户在网页上"重新启用"后照常开机自启
	ln -sf $KSROOT/scripts/corpline_config.sh $KSROOT/init.d/S96corpline.sh
}

do_start() {
	[ "$corpline_enable" = "1" ] || return 0
	[ "$corpline_paused" = "1" ] && { say "插件处于自动停用状态，等待重新启用"; return 0; }
	hooks_on
	if [ "$corpline_cn_direct" = "1" ] && [ ! -s $CFGDIR/china-domains.conf ]; then
		# 首次开启：后台下载名单，下载好后重启守护程序让它生效（期间照常走公司线路）
		( sleep 5; /bin/sh $KSROOT/scripts/corpline_config.sh update_cn ) >/dev/null 2>&1 &
	fi
	start_daemon
}

do_stop() {
	hooks_off
	stop_daemon
}

# 每分钟一次：守护程序在就什么都不做；不在就拉起；10 分钟内拉起 3 次以上 → 自动停用
do_ensure() {
	if [ "$corpline_enable" != "1" ] || [ "$corpline_paused" = "1" ]; then
		cru d corpline_ensure >/dev/null 2>&1
		return
	fi
	if [ -n "$(daemon_pid)" ]; then
		# 稳定运行 10 分钟后清零开机计数（防"开机即坏"循环用）
		if [ -f $RUN/stable ] && [ "${corpline_boots:-0}" != "0" ]; then
			dbus set corpline_boots=0
		fi
		if [ ! -f $RUN/stable ] && [ -f $PIDF ]; then
			local age=$(($(date +%s) - $(date -r $PIDF +%s 2>/dev/null || date +%s)))
			[ $age -ge 600 ] && touch $RUN/stable
		fi
		return
	fi
	local now=$(date +%s)
	mkdir -p $RUN
	echo $now >>$RUN/restarts
	local n=$(awk -v t=$((now - 600)) '$1 >= t' $RUN/restarts | wc -l)
	tail -n 10 $RUN/restarts >$RUN/restarts.tmp && mv -f $RUN/restarts.tmp $RUN/restarts
	if [ "$n" -ge 3 ]; then
		pause "10 分钟内意外退出 $n 次"
		return
	fi
	say "守护程序不在运行，自动拉起（10 分钟内第 $n 次）"
	start_daemon
}

do_boot() {
	# 上次是断电/死机关掉的：/jffs 上的 dnsmasq 转发配置还在（其余改动重启后已自然消失）。
	# 先撤掉，再决定是否启动——否则停用状态或 openconnect 所在的 U 盘没挂上时，
	# 全家每次解析都要先等一次超时。
	[ -f /jffs/configs/dnsmasq.d/corpline.conf ] && $BIN cleanup -c $CONF >/dev/null 2>&1
	[ "$corpline_enable" = "1" ] || return 0
	local boots=$((${corpline_boots:-0} + 1))
	dbus set corpline_boots=$boots
	if [ $boots -ge 3 ]; then
		pause "路由器短时间内连续重启 $boots 次，为安全起见先停用"
		return
	fi
	do_start
}

# 中国服务名单（felixonmars/dnsmasq-china-list accelerated-domains）：首次开启"国内网站直连"时下载，之后每周更新。
# 条目数校验通过才原子替换；失败保留旧表。
CN_URLS="https://cdn.jsdelivr.net/gh/felixonmars/dnsmasq-china-list@master/accelerated-domains.china.conf https://raw.githubusercontent.com/felixonmars/dnsmasq-china-list/master/accelerated-domains.china.conf"
update_cn() {
	local dst=$CFGDIR/china-domains.conf tmp=/tmp/corpline-cn.tmp
	mkdir -p $CFGDIR
	for u in $CN_URLS; do
		rm -f $tmp
		curl -fsSL -m 120 -o $tmp "$u" 2>/dev/null || wget -q -T 120 -O $tmp "$u" 2>/dev/null || continue
		local n=$(grep -c '^server=/' $tmp 2>/dev/null)
		if [ "${n:-0}" -ge 50000 ]; then
			mv -f $tmp $dst
			say "中国服务名单已更新：$n 条"
			return 0
		fi
	done
	rm -f $tmp
	say "中国服务名单更新失败，继续用旧名单"
	return 1
}

do_notify() {
	local text="公司专线：$1"
	[ "${corpline_notify:-off}" = "off" ] && return 0
	[ -n "$corpline_notify_url" ] || return 0
	case "$corpline_notify" in
	feishu)
		local body=$(printf '{"msg_type":"text","content":{"text":"%s"}}' "$(echo "$text" | sed 's/"/\\"/g')")
		# 飞书地址或签名不对时也回 HTTP 200，要看返回里的 code 是否为 0
		curl -sf -m 10 -H 'Content-Type: application/json' -d "$body" "$corpline_notify_url" 2>/dev/null | grep -Eq '"code": *0[,}]'
		;;
	bark)
		curl -sf -m 10 --data-urlencode "body=$text" --data-urlencode "title=公司专线" "${corpline_notify_url%/}" >/dev/null 2>&1
		;;
	esac
}

case $1 in
start)
	# 开机（S96）
	lock && { do_boot; unlock; }
	exit 0
	;;
start_nat)
	# 防火墙被固件重建（N96）：只补回本插件的 iptables 规则，不动隧道
	[ -n "$(daemon_pid)" ] && $BIN fw -c $CONF >/dev/null 2>&1
	exit 0
	;;
stop)
	lock; do_stop; unlock
	exit 0
	;;
restart)
	lock; do_stop; do_start; unlock
	exit 0
	;;
ensure)
	lock && { do_ensure; unlock; }
	exit 0
	;;
notify)
	do_notify "$2"
	exit 0
	;;
update_cn)
	# 每周更新：名单换了才需要让守护程序重新加载（凌晨执行，重启只有几秒直连）
	if update_cn && [ -n "$(daemon_pid)" ] && [ "$3" != "norestart" ]; then
		lock && { do_stop; do_start; unlock; }
	fi
	exit 0
	;;
esac

# 网页调用：$1 = 请求 id，$2 = 动作
case $2 in
1)
	# 应用：保存设置；开关打开则（重新）启动，关闭则停止
	http_response "$1"
	lock
	save_password
	eval $(dbus export corpline_)
	save_admin_device
	if [ "$corpline_enable" = "1" ]; then
		say "应用设置"
		do_stop
		do_start
	else
		[ -n "$(daemon_pid)" ] && say "已关闭公司专线，全家恢复直连"
		do_stop
		dbus set corpline_boots=0
	fi
	unlock
	;;
4)
	# 只保存：dbus 已由软件中心写好，这里只处理密码与锁定设备，不重启线路
	http_response "$1"
	save_password
	eval $(dbus export corpline_)
	save_admin_device
	;;
5)
	# 发一条测试通知（用刚保存的通知设置）
	eval $(dbus export corpline_)
	if [ "${corpline_notify:-off}" = "off" ] || [ -z "$corpline_notify_url" ]; then
		http_response "nourl"
	else
		do_notify "这是一条测试通知，收到说明设置正确。" && http_response "$1" || http_response "fail"
	fi
	;;
2)
	# 立即重试
	http_response "$1"
	p=$(daemon_pid)
	if [ -n "$p" ]; then
		kill -USR1 $p
	else
		lock && { do_start; unlock; }
	fi
	;;
3)
	# 自动停用后，用户点"重新启用"
	http_response "$1"
	lock
	dbus set corpline_paused=0
	dbus remove corpline_paused_reason
	dbus set corpline_boots=0
	rm -f $RUN/restarts
	eval $(dbus export corpline_)
	say "重新启用"
	do_start
	unlock
	;;
0)
	http_response "$1"
	lock; do_stop; unlock
	;;
esac
exit 0
