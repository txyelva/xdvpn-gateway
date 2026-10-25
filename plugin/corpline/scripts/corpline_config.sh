#!/bin/sh
# 公司专线（corpline）软件中心外壳脚本
#
# 只做三件事：把软件中心 dbus 里的设置翻译成守护程序的配置文件；启动/停止守护程序；
# 每分钟看护一次。所有网络判断都在 /koolshare/bin/corpline 里。
#
# 调用方式：
#   init.d：      S96corpline.sh start（每次 WAN 连通都会调用，不只是开机） | N96corpline.sh start_nat
#   网页 /_api/：  corpline_config.sh <请求id> <动作>
#                 动作：1 应用（重启服务）  2 立即重试  3 重新启用  4 只保存（不影响线路，如通知设置）  5 发测试通知  0 关闭
#   cru 看护：    corpline_config.sh ensure
#   守护程序通知：corpline_config.sh notify "<文字>"
# 注意：本固件上裸 sh 是内存读写工具，脚本互调一律用 /bin/sh。

export KSROOT=/koolshare
. $KSROOT/scripts/base.sh
export PATH=$KSROOT/bin:$KSROOT/scripts:/usr/sbin:/usr/bin:/sbin:/bin:$PATH

BIN=$KSROOT/bin/corpline
# 运行目录不能叫 /tmp/corpline*：软件中心安装/升级完会 rm -rf /tmp/<插件名>*，
# 会把正在运行的守护程序的 pid 文件等一起删掉，之后就找不到、停不掉它。
RUN=/var/run/corpline
CONF=$RUN/corpline.conf
PIDF=$RUN/corpline.pid
CFGDIR=$KSROOT/configs/corpline
PASSF=$CFGDIR/passwd
LOG=/tmp/upload/corpline_log.txt
LOCK=/var/lock/corpline.lock
DNSMASQ_CONF=/jffs/configs/dnsmasq.d/corpline.conf
NL='
'
CR=$(printf '\r')

say() {
	mkdir -p /tmp/upload
	echo "$(date '+%H:%M:%S') $*" >>$LOG
}

# 读设置：逐行解析 dbus list，不用 eval $(dbus export)。
# 网页填的账号、服务器、推送地址里若有引号、$()、反引号，只会被当成普通文字。
load_settings() {
	local l k
	for k in $(set | sed -n 's/^\(corpline_[a-z0-9_]*\)=.*/\1/p'); do unset $k; done
	while IFS= read -r l; do
		k=${l%%=*}
		case "$k" in corpline_*) ;; *) continue ;; esac
		case "$k" in *[!a-z0-9_]*) continue ;; esac
		eval "$k=\${l#*=}"
	done <<EOF
$(dbus list corpline_)
EOF
}

# 开机以来的秒数：比墙钟可靠（开机时 NTP 还没对时，墙钟会跳）
uptime_s() { cut -d. -f1 /proc/uptime; }

# 互斥：同一时刻只有一个操作在动网络。
#   lock <最多等几秒> [force]   force：等不到也照样执行（只用于"关闭"，关闭必须能生效）
HAVE_LOCK=
lock() {
	local max=${1:-30} i=0 nopid=0 p
	mkdir -p ${LOCK%/*}
	while ! mkdir $LOCK 2>/dev/null; do
		p=$(cat $LOCK/pid 2>/dev/null)
		if [ -n "$p" ]; then
			# 持锁进程已不在，或 pid 已被别的程序复用：陈旧锁
			if ! grep -q corpline /proc/$p/cmdline 2>/dev/null; then
				rm -rf $LOCK
				continue
			fi
		else
			# 刚建锁还没写 pid 是正常的；10 秒都没写上，说明持锁进程半路没了
			nopid=$((nopid + 1))
			[ $nopid -ge 10 ] && { rm -rf $LOCK; continue; }
		fi
		i=$((i + 1))
		if [ $i -ge $max ]; then
			[ "$2" = "force" ] || return 1
			say "等待其它操作超时，强制执行"
			rm -rf $LOCK
			continue
		fi
		sleep 1
	done
	echo $$ >$LOCK/pid
	HAVE_LOCK=1
	return 0
}
unlock() {
	# 只释放自己拿到的锁
	[ "$HAVE_LOCK" = "1" ] && [ "$(cat $LOCK/pid 2>/dev/null)" = "$$" ] && rm -rf $LOCK
	HAVE_LOCK=
}

daemon_pid() {
	local p=$(cat $PIDF 2>/dev/null)
	[ -n "$p" ] && [ -d /proc/$p ] && grep -q corpline /proc/$p/comm 2>/dev/null && echo $p
}

b64d() { echo "$1" | base64_decode 2>/dev/null; }
b64e() { base64_encode | tr -d '\n'; }

save_password() {
	# 网页把密码 base64 后放进 dbus；这里转存到 600 权限的文件，并从 dbus 删除
	local enc=$(dbus get corpline_password)
	[ -z "$enc" ] && return
	dbus remove corpline_password
	local p=$(b64d "$enc")
	[ -z "$p" ] && p="$enc"
	case "$p" in
	*"$NL"* | *"$CR"*)
		say "密码里不能有换行，未保存"
		return
		;;
	esac
	mkdir -p $CFGDIR
	(
		umask 077
		printf '%s' "$p" >$PASSF
	)
	chmod 600 $PASSF
	dbus set corpline_has_password=1
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

# 第一次启用时，把正在操作网页的这台电脑（按 IP 查 MAC）加为锁定的直连设备。只做一次：
# 加过（或列表里已有锁定设备）就删掉这个 IP，免得以后这个 IP 换给别的设备时把它误锁成直连。
save_admin_device() {
	[ -z "$corpline_admin_ip" ] && return
	local cur=$(b64d "$corpline_devices")
	if echo "$cur" | awk -F'\t' '$3 == "1" {f = 1} END {exit !f}'; then
		dbus remove corpline_admin_ip
		return
	fi
	local mac=$(awk -v ip="$corpline_admin_ip" -v dev="${corpline_lan_if:-br0}" \
		'$1 == ip && $6 == dev && $4 != "00:00:00:00:00:00" {print toupper($4)}' /proc/net/arp | head -n1)
	[ -z "$mac" ] && return
	if ! echo "$cur" | grep -qi "^$mac"; then
		local line=$(printf '%s\t%s\t1' "$mac" "管理这台路由器的电脑")
		corpline_devices=$(printf '%s\n%s' "$line" "$cur" | sed '/^$/d' | b64e)
		dbus set corpline_devices="$corpline_devices"
	fi
	dbus remove corpline_admin_ip
}

# 直连设备：dbus 里是 base64 的多行 "MAC<TAB>名字<TAB>锁定"，只取 MAC
direct_macs() {
	[ -z "$corpline_devices" ] && return
	b64d "$corpline_devices" | awk -F'\t' '$1 ~ /^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$/ {printf "%s,", toupper($1)}'
}

# 配置文件一行：值里的换行去掉，防止一个设置项"多写出"别的配置行
kv() { printf '%s=%s\n' "$1" "$(printf '%s' "$2" | tr -d '\r\n')"; }

server_value() {
	local s=${corpline_server:-vpn.xindong.com:8443}
	case "$s" in
	*[!A-Za-z0-9.:-]*)
		say "公司服务器地址格式不对（$s），改用默认 vpn.xindong.com:8443"
		s=vpn.xindong.com:8443
		;;
	esac
	echo "$s"
}

gen_conf() {
	mkdir -p $RUN /tmp/upload
	local oc=$(find_openconnect)
	local ca=$(find_cafile)
	local pass=""
	[ -f $PASSF ] && pass=$(cat $PASSF)
	(
		umask 077
		{
			kv server "$(server_value)"
			kv protocol "${corpline_protocol:-anyconnect}"
			kv user "$corpline_user"
			kv password "$pass"
			kv openconnect "${oc:-/opt/sbin/openconnect}"
			kv cafile "${corpline_cafile:-$ca}"
			[ -n "$corpline_servercert" ] && kv servercert "$corpline_servercert"
			[ "$corpline_no_dtls" = "1" ] && echo "no_dtls=1"
			kv lan_if "${corpline_lan_if:-br0}"
			kv direct_macs "$(direct_macs)"
			# 安装时已写入默认 apple.com,mzstatic.com；用户清空后这里就是空，不再回到默认
			kv overseas_domains "$corpline_overseas_domains"
			[ -n "$corpline_overseas_dns" ] && kv overseas_dns "$corpline_overseas_dns"
			[ -n "$corpline_isp_dns" ] && kv isp_dns "$corpline_isp_dns"
			[ -n "$corpline_internal_domains" ] && kv internal_domains "$corpline_internal_domains"
			kv cn_direct "${corpline_cn_direct:-0}"
			echo "cn_list=$CFGDIR/china-domains.conf"
			kv sensitivity "${corpline_sensitivity:-normal}"
			echo "hook_cmd=$BIN hook"
			# 通知设置在发送时才读取，改通知不需要重启
			echo "notify_cmd=/bin/sh $KSROOT/scripts/corpline_config.sh notify"
			echo "run_dir=$RUN"
			echo "status_dir=/tmp/upload"
			# 沙盒测试用的覆盖项（真机上不存在这个文件）
			[ -f $CFGDIR/lab.conf ] && cat $CFGDIR/lab.conf
		} >$CONF.tmp
	)
	mv -f $CONF.tmp $CONF
	chmod 600 $CONF
}

# 能不能启动：缺账号密码、缺 openconnect（例如 Entware 所在 U 盘开机时还没挂上）属于"还不能启动"，
# 不是守护程序崩溃，不计入崩溃次数。能启动则无输出。
not_ready() {
	if [ ! -s $PASSF ]; then
		[ "$corpline_has_password" = "1" ] && dbus set corpline_has_password=0
		echo "还没有填写公司密码"
	elif [ -z "$corpline_user" ]; then
		echo "还没有填写公司账号"
	elif [ ! -x "$(find_openconnect)" ]; then
		echo "找不到 openconnect，请先在 Entware 安装：opkg install openconnect"
	fi
}

start_daemon() {
	[ -n "$(daemon_pid)" ] && return 0
	local why=$(not_ready)
	if [ -n "$why" ]; then
		# 同样的原因只记一次，看护每分钟都会来问
		[ "$(cat $RUN/blocked 2>/dev/null)" = "$why" ] || { mkdir -p $RUN; echo "$why" >$RUN/blocked; say "$why，未启动"; }
		return 2
	fi
	rm -f $RUN/blocked
	# 拿到锁之前开关可能已被关掉：启动前最后再确认一次
	[ "$(dbus get corpline_enable)" = "1" ] && [ "$(dbus get corpline_paused)" != "1" ] || return 1
	gen_conf
	rm -f $RUN/stable
	uptime_s >$RUN/started
	touch $RUN/booted # 本次开机已经启动过：之后的 S96（宽带重拨）不再算开机
	# 子 shell + setsid：守护程序脱离本脚本，脚本退出不影响它。
	# 外面套一层 sh 等它退出（阻塞等待，不占 CPU）：不是我们主动停的（崩溃、被系统杀掉），
	# 立刻撤掉全部网络改动——尤其是 dnsmasq 的转发，否则全家每次解析都要多等 1–5 秒——
	# 再走一次看护（拉起 / 10 分钟内 3 次则自动停用），不必等每分钟的定时看护。
	# 每次启动一个编号：停止或重启会换掉编号，旧的外壳醒来对不上号就什么都不做。
	# 退出码 3 = 已有一份在运行（DNS 端口被占），不能去清别人的规则。
	local gen="$$.$(uptime_s)"
	echo $gen >$RUN/gen
	local w="$BIN run -c $CONF; rc=\$?; [ \$rc != 3 ] && [ \"\$(cat $RUN/gen 2>/dev/null)\" = $gen ] && { rm -f /tmp/upload/corpline_status.json; $BIN cleanup -c $CONF; /bin/sh $KSROOT/scripts/corpline_config.sh ensure; }"
	if command -v setsid >/dev/null 2>&1; then
		(setsid /bin/sh -c "$w" >/dev/null 2>&1 </dev/null &)
	else
		(/bin/sh -c "$w" >/dev/null 2>&1 </dev/null &)
	fi
	local i=0
	while [ $i -lt 10 ]; do
		[ -n "$(daemon_pid)" ] && break
		sleep 1
		i=$((i + 1))
	done
	[ -n "$(daemon_pid)" ] || { say "守护程序启动失败"; return 1; }
	return 0
}

# 按 pid 文件结束本插件的 openconnect（不误杀路由器上别的 openconnect）
kill_oc() {
	local p=$(cat $RUN/openconnect.pid 2>/dev/null)
	[ -n "$p" ] && grep -q openconnect /proc/$p/comm 2>/dev/null && kill -9 $p 2>/dev/null
	rm -f $RUN/openconnect.pid
}

stop_daemon() {
	mkdir -p $RUN
	echo stopped >$RUN/gen # 让守护程序外面那层 sh 知道这次退出是主动停的
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
	# pid 文件丢了的守护程序（旧版本留下的等）按名字兜底
	killall -q corpline 2>/dev/null
	kill_oc
	# 无论守护程序是否正常退出，都再清一遍（幂等），保证不留规则
	[ -f $CONF ] || gen_conf
	$BIN cleanup -c $CONF >/dev/null 2>&1
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
	corpline_paused=1
	say "已自动停用：$1"
	stop_daemon
	hooks_off
	# 开机钩子保留，便于用户在网页上"重新启用"后照常开机自启
	ln -sf $KSROOT/scripts/corpline_config.sh $KSROOT/init.d/S96corpline.sh
	do_notify "已自动停用（$1），全家已恢复直连。到路由器的插件页面点「重新启用」即可。"
}

# 本插件用到的内核模块：华硕固件默认不一定全加载（旧版 xdvpn-router 真机经验）
load_modules() {
	local os=$(uname -r) mod ko
	modprobe tun >/dev/null 2>&1
	for mod in ip_set ip_set_hash_net ip_set_hash_ip xt_set xt_mark xt_connmark xt_mac xt_state xt_TCPMSS xt_addrtype; do
		lsmod | grep -q "^$mod " && continue
		modprobe $mod >/dev/null 2>&1 && continue
		ko=$(find /lib/modules/$os -name "$mod.ko" 2>/dev/null | head -n1)
		[ -n "$ko" ] && insmod $ko >/dev/null 2>&1
	done
}

# 开机早期 nat 表可能还没就绪：等它就绪再动防火墙（最多 120 秒）
wait_nat_ready() {
	local i=0
	until iptables -t nat -L PREROUTING -n >/dev/null 2>&1; do
		i=$((i + 1))
		[ $i -ge 120 ] && { say "防火墙 nat 表 120 秒仍未就绪，本次不启动"; return 1; }
		sleep 1
	done
}

do_start() {
	[ "$corpline_enable" = "1" ] || return 0
	if [ "$corpline_paused" = "1" ]; then
		ln -sf $KSROOT/scripts/corpline_config.sh $KSROOT/init.d/S96corpline.sh
		say "插件处于自动停用状态，等待重新启用"
		return 0
	fi
	hooks_on
	if [ "$corpline_cn_direct" = "1" ] && [ ! -s $CFGDIR/china-domains.conf ]; then
		# 首次开启：后台下载名单，下载好后重启守护程序让它生效（期间照常走公司线路）
		(
			sleep 5
			/bin/sh $KSROOT/scripts/corpline_config.sh update_cn
		) >/dev/null 2>&1 &
	fi
	load_modules
	wait_nat_ready || return 1
	start_daemon
}

do_stop() {
	hooks_off
	stop_daemon
}

# 每分钟一次：守护程序在就什么都不做；不在就拉起；10 分钟内意外退出 3 次 → 自动停用
do_ensure() {
	if [ "$corpline_enable" != "1" ] || [ "$corpline_paused" = "1" ]; then
		cru d corpline_ensure >/dev/null 2>&1
		return
	fi
	local now=$(uptime_s)
	if [ -n "$(daemon_pid)" ]; then
		# 稳定运行 10 分钟后清零开机计数（防"开机即坏"循环用）
		if [ ! -f $RUN/stable ] && [ $((now - $(cat $RUN/started 2>/dev/null || echo $now))) -ge 600 ]; then
			touch $RUN/stable
		fi
		[ -f $RUN/stable ] && [ "${corpline_boots:-0}" != "0" ] && dbus set corpline_boots=0
		return
	fi
	if [ -n "$(not_ready)" ]; then
		start_daemon # 只记录原因，不计崩溃
		return
	fi
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

# S96：每次 WAN 连通都会调用。本次开机的第一次、且守护程序还没在跑，才算"开机"
# （标记在内存盘，重启即消失）。装好后从网页启用、之后第一次宽带重拨，不能当成开机去清规则。
do_wan_up() {
	if [ ! -f $RUN/booted ] && [ -z "$(daemon_pid)" ]; then
		mkdir -p $RUN
		touch $RUN/booted
		# 上次是断电/死机关掉的：/jffs 上的 dnsmasq 转发配置还在（其余改动重启后已自然消失）。
		# 先撤掉，再决定是否启动——否则停用状态或 openconnect 所在的 U 盘没挂上时，
		# 全家每次解析都要先等一次超时。
		[ -f $DNSMASQ_CONF ] && $BIN cleanup -c $CONF >/dev/null 2>&1
		[ "$corpline_enable" = "1" ] || return 0
		[ "$corpline_paused" = "1" ] && return 0
		local boots=$((${corpline_boots:-0} + 1))
		dbus set corpline_boots=$boots
		if [ $boots -ge 3 ]; then
			pause "路由器短时间内连续重启 $boots 次，为安全起见先停用"
			return
		fi
	fi
	do_start
}

# 中国服务名单（felixonmars/dnsmasq-china-list accelerated-domains）：首次开启"国内网站直连"时下载，之后每周更新。
# 条目数校验通过才原子替换；失败保留旧表。
CN_URLS="https://cdn.jsdelivr.net/gh/felixonmars/dnsmasq-china-list@master/accelerated-domains.china.conf https://raw.githubusercontent.com/felixonmars/dnsmasq-china-list/master/accelerated-domains.china.conf"
update_cn() {
	local dst=$CFGDIR/china-domains.conf tmp=$CFGDIR/china-domains.tmp
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
	local text="公司专线：$1" u="$corpline_notify_url"
	[ "${corpline_notify:-off}" = "off" ] && return 0
	case "$u" in
	https://* | http://*) ;;
	*) return 1 ;;
	esac
	case "$corpline_notify" in
	feishu)
		local t=$(printf '%s' "$text" | tr '\r\n\t' '   ' | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g')
		# 飞书地址或签名不对时也回 HTTP 200，要看返回里的 code 是否为 0
		curl -sf -m 10 -H 'Content-Type: application/json' -d "{\"msg_type\":\"text\",\"content\":{\"text\":\"$t\"}}" --url "$u" 2>/dev/null |
			grep -Eq '"code": *0[,}]'
		;;
	bark)
		curl -sf -m 10 --data-urlencode "body=$text" --data-urlencode "title=公司专线" --url "${u%/}" >/dev/null 2>&1
		;;
	esac
}

load_settings

case $1 in
start)
	# S96：WAN 连通（开机或宽带重拨）
	lock 60 && {
		load_settings
		do_wan_up
		unlock
	}
	exit 0
	;;
start_nat)
	# 防火墙被固件重建（N96）：只补回本插件的 iptables 规则，不动隧道。
	# 别的操作正在进行就跳过——守护程序自己每分钟也会检查补回。
	lock 5 && {
		[ -n "$(daemon_pid)" ] && $BIN fw -c $CONF >/dev/null 2>&1
		unlock
	}
	exit 0
	;;
stop)
	lock 60 force
	do_stop
	unlock
	exit 0
	;;
restart)
	lock 60 && {
		load_settings
		do_stop
		do_start
		unlock
	}
	exit 0
	;;
ensure)
	# 别的操作正在进行就跳过，下一分钟再来
	lock 5 && {
		load_settings
		do_ensure
		unlock
	}
	exit 0
	;;
notify)
	do_notify "$2"
	exit 0
	;;
update_cn)
	# 每周更新：名单换了才需要让守护程序重新加载（凌晨执行，重启只有几秒直连）
	if update_cn && [ -n "$(daemon_pid)" ]; then
		lock 60 && {
			load_settings
			do_stop
			do_start
			unlock
		}
	fi
	exit 0
	;;
esac

# 网页调用：$1 = 请求 id，$2 = 动作
case $2 in
1)
	# 应用：保存设置；开关打开则（重新）启动，关闭则停止
	http_response "$1"
	save_password
	load_settings
	save_admin_device
	if [ "$corpline_enable" = "1" ]; then
		lock 60 || {
			say "另一个操作还没完成，这次应用没有执行，请稍后再点一次"
			exit 0
		}
		load_settings
		say "应用设置"
		do_stop
		do_start
	else
		lock 60 force
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
	load_settings
	save_admin_device
	;;
5)
	# 发一条测试通知（用刚保存的通知设置）
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
		lock 60 && {
			load_settings
			do_start
			unlock
		}
	fi
	;;
3)
	# 自动停用后，用户点"重新启用"
	http_response "$1"
	lock 60 || exit 0
	dbus set corpline_paused=0
	dbus remove corpline_paused_reason
	dbus set corpline_boots=0
	rm -f $RUN/restarts $RUN/blocked
	load_settings
	say "重新启用"
	do_start
	unlock
	;;
0)
	# 关闭：必须能生效，等不到锁也照样执行
	http_response "$1"
	lock 60 force
	do_stop
	unlock
	;;
esac
exit 0
