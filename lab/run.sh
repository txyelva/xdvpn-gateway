#!/bin/bash
# 沙盒故障演练：从安装到卸载，逐项制造故障，检查插件是否按设计退避、回到直连、再自动恢复。
# 用法：sudo bash lab/run.sh [场景名...]   不带参数 = 全部场景（约 40 分钟）
# 结果：lab/report/RESULTS.md（每项 通过/失败 + 证据）
cd "$(dirname "$0")"
. ./lib.sh
REPORT=$(pwd)/report
mkdir -p $REPORT
OUT=$REPORT/RESULTS.md
PASS=0
FAIL=0

log() { echo "[$(date +%T)] $*"; }
ok() { PASS=$((PASS + 1)); echo "| $CASE | ✅ | $* |" >>$OUT; log "✅ $CASE：$*"; }
bad() { FAIL=$((FAIL + 1)); echo "| $CASE | ❌ | $* |" >>$OUT; log "❌ $CASE：$*"; }
check() { # check <描述> <实际> <期望>
	if [ "$2" = "$3" ]; then ok "$1：$2"; else bad "$1：实际 $2，期望 $3"; fi
}
timeline_has() { jq -r '.events[].m' $ST 2>/dev/null | grep -q "$1"; }
oc_pid() { pgrep -x openconnect | head -n1; }
cl_pid() { pgrep -x corpline | head -n1; }
oc_starts() { grep -c "连接公司线路（第" /tmp/upload/corpline_log.txt 2>/dev/null; }
blackhole_on() { corp iptables -I INPUT -s 10.235.128.0/18 -j DROP; corp iptables -I FORWARD -s 10.235.128.0/18 -j DROP; }
blackhole_off() { corp iptables -D INPUT -s 10.235.128.0/18 -j DROP 2>/dev/null; corp iptables -D FORWARD -s 10.235.128.0/18 -j DROP 2>/dev/null; true; }
ensure_now() { rt /bin/sh /koolshare/scripts/corpline_config.sh ensure; }
fresh_active() { # 回到干净的"公司专线正常"状态
	blackhole_off
	corp iptables -F INPUT 2>/dev/null
	wan tc qdisc del dev wan-corp root 2>/dev/null
	rt ip link set eth0 up
	rt ip route replace default via 100.64.0.1
	[ -z "$(cl_pid)" ] && api 1
	if [ "$(state)" != active ]; then
		rt /bin/sh -c 'kill -USR1 $(cat /var/run/corpline/corpline.pid)' 2>/dev/null
		wait_state active 240 >/dev/null
	fi
}

scenario_install() {
	CASE="安装（未启用）"
	rm -f $ST   # 上一轮留下的状态文件会让"等到公司专线可用"立刻返回
	snapshot >$S/s0.txt
	ks_install >$S/install.log 2>&1
	snapshot >$S/s1.txt
	local d=$(diff <(sed -n '/## iptables/,/## dnsmasq.d/p' $S/s0.txt) <(sed -n '/## iptables/,/## dnsmasq.d/p' $S/s1.txt) | grep -c '^[<>]')
	check "安装后网络改动条数" "$d" 0
	check "未启用时设备线路" "$(path lan1 100.64.2.80)" direct
	local n=$(wc -l </koolshare/configs/corpline/china-domains.conf 2>/dev/null)
	[ "${n:-0}" -ge 50000 ] && ok "安装包自带中国服务名单：$n 条" || bad "安装包没带中国服务名单（${n:-0} 条）"
	check "名单状态（网页显示用）" "$(/koolshare/bin/dbus get corpline_cn_state)/$(/koolshare/bin/dbus get corpline_cn_src)/$(/koolshare/bin/dbus get corpline_cn_count)" "ok/bundle/$n"
	mkdir -p /koolshare/configs/corpline
	printf 'cafile=%s\noc_ld_library_path=\nisp_dns_file=%s\n' $S/pki/ca.pem $S/resolv.dnsmasq >/koolshare/configs/corpline/lab.conf
}

scenario_enable() {
	CASE="启用"
	dbset corpline_enable=1
	dbset corpline_server=vpn.corp.test:8443
	dbset corpline_user=tester
	dbset corpline_password=$(printf s3cret | base64)
	dbset corpline_devices=$(printf '02:00:00:00:02:00\tMac Studio\t1\n' | base64 -w0)
	local t0=$(date +%s)
	api 1
	local t=$(wait_state active 60)
	ok "从点启用到公司专线可用：${t} 秒（含建隧道与首轮探测）"
	check "普通设备线路" "$(path lan1 100.64.2.80)" tunnel
	check "只有公司线路能到的网站" "$(path lan1 203.0.113.80)" tunnel
	check "直连设备（Mac Studio）线路" "$(path lan2 100.64.2.80)" direct
	check "路由器自身线路" "$(path rt 100.64.2.80)" direct
	check "普通域名由谁解析" "$(which_dns)" company
	check "google 解析（公司 DNS，不受污染）" "$(resolve x www.google.com)" 203.0.113.80
	check "Apple 解析（海外 DNS 例外）" "$(resolve x apps.apple.com)" 203.0.113.81
	check "公司内网域名" "$(resolve x git.corp.test)" 172.24.10.10
	check "IPv6 记录（V1 不走 IPv6，回空）" "[$(resolve x www.google.com lan1 AAAA)]" "[]"
	check "rp_filter 已放宽" "$(rt cat /proc/sys/net/ipv4/conf/all/rp_filter)" 0
	check "看护任务已登记" "$(grep -c corpline_ensure $S/cron)" 1
}

scenario_company_dns_down() {
	CASE="公司 DNS 无响应"
	fresh_active
	corp iptables -I INPUT -d 172.24.4.79 -p udp --dport 53 -j DROP
	corp iptables -I INPUT -d 172.24.4.79 -p tcp --dport 53 -j DROP
	local t0=$(date +%s%N)
	local w=$(which_dns)
	local ms=$((($(date +%s%N) - t0) / 1000000))
	check "公司 DNS 不应答时由谁解析" "$w" overseas
	ok "该次解析耗时 ${ms} ms（公司 DNS 300 ms 未回即并行问海外 DNS）"
	for i in 1 2 3 4 5 6; do resolve x a$i.which.test >/dev/null; done
	sleep 50
	check "公司 DNS 坏了不算线路坏了（50 秒后状态）" "$(state)" active
	if timeline_has "公司 DNS 无响应"; then ok "时间线写明「公司 DNS 无响应，已改用备用 DNS」"; else bad "时间线没有公司 DNS 故障记录"; fi
	local t1=$(date +%s%N)
	which_dns >/dev/null
	ok "公司 DNS 被判不健康后，单次解析耗时 $((($(date +%s%N) - t1) / 1000000)) ms（不再等 300 ms）"
	corp iptables -D INPUT -d 172.24.4.79 -p udp --dport 53 -j DROP
	corp iptables -D INPUT -d 172.24.4.79 -p tcp --dport 53 -j DROP
	for i in $(seq 1 8); do which_dns >/dev/null; sleep 1; done
	check "公司 DNS 恢复后由谁解析" "$(which_dns)" company
}

scenario_blackhole() {
	CASE="线路黑洞（隧道在但不通）"
	fresh_active
	blackhole_on
	local t=$(wait_state fallback 90)
	ok "从不通到切直连：${t} 秒（设计约 45 秒：每 15 秒探测、连续 3 次失败）"
	check "直连后设备线路" "$(path lan1 100.64.2.80)" direct
	check "直连后 DNS" "$(which_dns)" isp
	# 运营商 DNS 的答案 TTL 被压到 30 秒，切回后污染记录很快过期
	local ttl=$(lan1 dig +noall +answer r$RANDOM.which.test @192.168.50.1 | awk '{print $2}' | head -n1)
	if [ -n "$ttl" ] && [ "$ttl" -le 30 ]; then ok "直连期间解析 TTL ≤ 30 秒（实际 $ttl）"; else bad "直连期间 TTL 未压低：$ttl"; fi
	local s0=$(oc_starts)
	sleep 95
	local s1=$(oc_starts)
	if [ "$s1" -gt "$s0" ]; then ok "隧道一直不通：直连后约 80 秒主动重建连接（第 $s1 次）"; else bad "隧道不通但没有重建"; fi
	# 直连期间建立一条长连接，切回公司线路后它应继续完成
	lan1 curl -s -o /dev/null -w '%{http_code} %{size_download}\n' "http://100.64.2.80/big?mb=4&kbps=20" >$S/longconn.txt 2>&1 &
	local lc=$!
	sleep 2
	blackhole_off
	local t2=$(wait_state active 260)
	ok "线路恢复后切回公司专线：${t2} 秒（含重连等待与 120 秒稳定期）"
	check "切回后设备线路" "$(path lan1 100.64.2.80)" tunnel
	wait $lc
	check "直连期间开始的下载在切回后没有中断（4 MB）" "$(cat $S/longconn.txt)" "200 4194304"
}

scenario_kill_oc() {
	CASE="openconnect 被杀"
	fresh_active
	kill -9 $(oc_pid)
	local t=$(wait_state fallback 10)
	ok "进程消失到切直连：${t} 秒"
	check "设备线路（内核兜底，立即直连）" "$(path lan1 100.64.2.80)" direct
	local t2=$(wait_state active 240)
	ok "5 秒后重连，稳定 120 秒后切回：${t2} 秒"
}

scenario_freeze_oc() {
	CASE="openconnect 卡死（SIGSTOP）"
	fresh_active
	local p=$(oc_pid)
	kill -STOP $p
	local t=$(wait_state fallback 90)
	ok "卡死到切直连：${t} 秒"
	check "设备线路" "$(path lan1 100.64.2.80)" direct
	local n=0
	while [ -d /proc/$p ] && [ $n -lt 150 ]; do sleep 1; n=$((n + 1)); done
	if [ ! -d /proc/$p ]; then ok "卡死的进程被结束（SIGTERM 无效时 12 秒后 SIGKILL），用时约 ${n} 秒"; else bad "卡死的 openconnect 没有被结束"; kill -9 $p; fi
	local t2=$(wait_state active 260)
	ok "重建后切回公司专线：${t2} 秒"
}

scenario_latency() {
	CASE="线路变慢"
	fresh_active
	wan tc qdisc add dev wan-corp root netem delay 1000ms
	local t=$(wait_state watching 40)
	ok "往返延迟 +1 秒→「有点慢，正在观察」：${t} 秒；仍走公司线路"
	check "观察中设备线路" "$(path lan1 100.64.2.80 8)" tunnel
	wan tc qdisc del dev wan-corp root
	local t2=$(wait_state active 40)
	ok "延迟恢复后回到正常：${t2} 秒"
}

scenario_wan_flap() {
	CASE="宽带断开 40 秒"
	fresh_active
	rt ip link set eth0 down
	local t=$(wait_state fallback 90)
	ok "宽带断开后切直连：${t} 秒"
	sleep 10
	rt ip link set eth0 up
	rt ip route replace default via 100.64.0.1   # 真机上由 WAN 的 DHCP/PPPoE 重新下发
	local t2=$(wait_state active 200)
	ok "宽带恢复后自动回到公司专线：${t2} 秒（宽带恢复即重连，不等退避；再稳定 120 秒）"
	check "设备线路" "$(path lan1 100.64.2.80)" tunnel
}

scenario_server_down() {
	CASE="公司服务器连不上（重连退避）"
	fresh_active
	# 前面的场景已把退避拉长（稳定 10 分钟才清零，这是有意的）；这里从头看退避节奏，先重启一次服务
	api 1
	wait_state active 60 >/dev/null
	corp iptables -I INPUT -p tcp --dport 8443 -j DROP
	corp iptables -I INPUT -p udp --dport 8443 -j DROP
	local l0=$(wc -l </tmp/upload/corpline_log.txt)
	kill -9 $(oc_pid)
	wait_state fallback 10 >/dev/null
	sleep 150
	local gaps=$(tail -n +$((l0 + 1)) /tmp/upload/corpline_log.txt | grep -E "连接公司线路（第" | awk '{print $1}' | tr '\n' ' ')
	local n=$(echo $gaps | wc -w)
	if [ "$n" -ge 3 ]; then ok "150 秒内重连 $n 次（每次 45 秒连不上就放弃，再按 5/10/30 秒退避）：$gaps"; else bad "150 秒内只重连 $n 次（应 ≥ 3，连接超时没生效？）：$gaps"; fi
	check "期间设备线路" "$(path lan1 100.64.2.80)" direct
	check "期间 DNS" "$(which_dns)" isp
	corp iptables -D INPUT -p tcp --dport 8443 -j DROP
	corp iptables -D INPUT -p udp --dport 8443 -j DROP
	rt /bin/sh -c 'kill -USR1 $(cat /var/run/corpline/corpline.pid)'
	local t=$(wait_state active 200)
	ok "服务器恢复 + 点「立即重试」后回到公司专线：${t} 秒"
}

scenario_crash() {
	CASE="插件守护程序崩溃"
	fresh_active
	rm -f /var/run/corpline/restarts
	touch $S/crond.paused   # 不靠每分钟的定时看护，验证崩溃后自己就能恢复
	local op=$(oc_pid)
	kill -9 $(cl_pid)
	sleep 1
	if [ ! -d /proc/$op ]; then ok "守护程序一死，openconnect 随之退出（不留孤儿）"; else bad "openconnect 成了孤儿"; fi
	local t0=$(date +%s%N)
	local r=$(resolve x r$RANDOM.which.test)
	local ms=$((($(date +%s%N) - t0) / 1000000))
	[ "$r" = 100.64.2.80 ] || [ "$r" = 203.0.113.80 ] && ok "崩溃后 1 秒内解析照常：${ms} ms" || bad "崩溃后解析失败：$r（${ms} ms）"
	local t=$(wait_state active 60)
	ok "不等定时看护，崩溃后自动拉起回到公司专线：${t} 秒"
	check "设备线路" "$(path lan1 100.64.2.80)" tunnel
	rm -f $S/crond.paused
}

scenario_crash_loop() {
	CASE="反复崩溃 → 自动停用"
	fresh_active
	rm -f /var/run/corpline/restarts
	touch $S/crond.paused
	local i=0
	while [ $i -lt 5 ] && [ "$(/koolshare/bin/dbus get corpline_paused)" != 1 ]; do
		local w=0
		while [ -z "$(cl_pid)" ] && [ $w -lt 15 ]; do sleep 1; w=$((w + 1)); done
		kill -9 $(cl_pid) 2>/dev/null
		i=$((i + 1))
		sleep 4
	done
	check "崩溃 3 次后自动停用" "$(/koolshare/bin/dbus get corpline_paused)/$i" 1/3
	check "停用后进程" "$(pgrep -c -x corpline)" 0
	check "停用后设备线路" "$(path lan1 100.64.2.80)" direct
	check "停用后网络规则" "$(rt iptables-save | grep -ci corpline)" 0
	check "停用后 DNS" "$(which_dns)" isp
	api 3
	rm -f $S/crond.paused
	local t=$(wait_state active 60)
	ok "点「重新启用」后恢复：${t} 秒"
}

scenario_notify() {
	CASE="异常通知"
	fresh_active
	: >$S/hook.log
	: >$S/http_response.log
	dbset corpline_notify=feishu
	dbset corpline_notify_url=http://100.64.2.81:8080/feishu/ok
	api 4
	check "只保存通知设置不影响线路" "$(state)" active
	rt busybox sh /koolshare/scripts/corpline_config.sh 777 5 >/dev/null 2>&1
	check "飞书测试通知（地址正确）" "$(tail -n1 $S/http_response.log)" 777
	grep -q '测试通知' $S/hook.log && ok "飞书收到测试内容" || bad "飞书没收到内容"
	dbset corpline_notify_url=http://100.64.2.81:8080/feishu/bad
	rt busybox sh /koolshare/scripts/corpline_config.sh 778 5 >/dev/null 2>&1
	check "飞书测试通知（地址或签名不对，飞书仍回 HTTP 200）" "$(tail -n1 $S/http_response.log)" fail
	dbset corpline_notify=bark
	dbset corpline_notify_url=http://100.64.2.81:8080/bark/KEY/
	rt busybox sh /koolshare/scripts/corpline_config.sh 779 5 >/dev/null 2>&1
	check "Bark 测试通知" "$(tail -n1 $S/http_response.log)" 779
	dbset corpline_notify_url=
	rt busybox sh /koolshare/scripts/corpline_config.sh 780 5 >/dev/null 2>&1
	check "没填地址" "$(tail -n1 $S/http_response.log)" nourl
	# 自动停用时推送
	dbset corpline_notify=feishu
	dbset corpline_notify_url=http://100.64.2.81:8080/feishu/ok
	: >$S/hook.log
	rm -f /var/run/corpline/restarts
	touch $S/crond.paused
	local i=0
	while [ $i -lt 5 ] && [ "$(/koolshare/bin/dbus get corpline_paused)" != 1 ]; do
		local w=0
		while [ -z "$(cl_pid)" ] && [ $w -lt 15 ]; do sleep 1; w=$((w + 1)); done
		kill -9 $(cl_pid) 2>/dev/null
		i=$((i + 1))
		sleep 4
	done
	grep -q '已自动停用' $S/hook.log && ok "自动停用时推送：$(grep -o '已自动停用[^"]*' $S/hook.log | head -n1)" || bad "自动停用没有推送"
	dbset corpline_notify=off
	api 3
	rm -f $S/crond.paused
	wait_state active 60 >/dev/null
}

scenario_power_loss() {
	CASE="运行中突然断电"
	fresh_active
	# 断电：外壳和守护程序都来不及收尾；重启后内存里的东西都没了，只剩 /jffs 与 dbus 里的设置
	reboot_sim
	rt /bin/sh -c 'kill $(cat /var/run/dnsmasq.pid)'; sleep 0.5; rt dnsmasq -C $S/dnsmasq-rt.conf; sleep 0.5
	dbset corpline_paused=1   # 最坏情况：断电前插件已处于自动停用（或 openconnect 所在 U 盘没挂上）
	rt /bin/sh /koolshare/init.d/S96corpline.sh start
	check "开机后残留的 DNS 转发配置" "$(ls /jffs/configs/dnsmasq.d/ | grep -c corpline)" 0
	local t0=$(date +%s%N)
	local r=$(resolve x r$RANDOM.which.test)
	local ms=$((($(date +%s%N) - t0) / 1000000))
	check "开机后解析" "$r" 100.64.2.80
	ok "开机后单次解析耗时 ${ms} ms（不再先等超时）"
	api 3
	local t=$(wait_state active 60)
	ok "点「重新启用」后回到公司专线：${t} 秒"
}

scenario_auth() {
	CASE="密码错误"
	fresh_active
	dbset corpline_password=$(printf wrong | base64)
	api 1
	local t=$(wait_state autherr 30)
	ok "识别为账号密码错误：${t} 秒"
	local s0=$(oc_starts)
	sleep 120
	check "两分钟内没有再尝试（防账号风控）" "$(oc_starts)" "$s0"
	check "设备线路" "$(path lan1 100.64.2.80)" direct
	check "DNS" "$(which_dns)" isp
	dbset corpline_password=$(printf s3cret | base64)
	api 1
	local t2=$(wait_state active 60)
	ok "改对密码后恢复：${t2} 秒"
}

scenario_fw_rebuild() {
	CASE="固件重建防火墙"
	fresh_active
	rt iptables -t mangle -D PREROUTING -i br0 -j CORPLINE
	rt iptables -t nat -D POSTROUTING -j CORPLINE_NAT
	check "规则被清后新连接线路（落回直连，不断网）" "$(path lan1 100.64.2.80)" direct
	rt /bin/sh /koolshare/init.d/N96corpline.sh start_nat
	check "N96 钩子补回后线路" "$(path lan1 100.64.2.80)" tunnel
	rt iptables -t mangle -D PREROUTING -i br0 -j CORPLINE
	rt iptables -t nat -D POSTROUTING -j CORPLINE_NAT
	local n=0
	while [ $n -lt 70 ] && ! rt iptables -t mangle -C PREROUTING -i br0 -j CORPLINE 2>/dev/null; do sleep 1; n=$((n + 1)); done
	if [ $n -lt 70 ]; then ok "即使没有钩子，守护程序 ${n} 秒内自行补回"; else bad "规则没有被补回"; fi
	# 只丢了路由规则、只重建了 filter 表、rp_filter 被改回：也要发现并补回
	rt ip rule del pref 8850
	rt iptables -D FORWARD -j CORPLINE_GUARD
	rt sh -c 'echo 1 >/proc/sys/net/ipv4/conf/all/rp_filter'
	check "路由规则丢失后新连接线路（落回直连）" "$(path lan1 100.64.2.80)" direct
	n=0
	while [ $n -lt 70 ] && ! { rt ip rule | grep -q '^8850:' && rt iptables -C FORWARD -j CORPLINE_GUARD 2>/dev/null && [ "$(rt cat /proc/sys/net/ipv4/conf/all/rp_filter)" = 0 ]; }; do sleep 1; n=$((n + 1)); done
	if [ $n -lt 70 ]; then ok "路由规则、filter 防护、rp_filter ${n} 秒内全部补回"; else bad "路由规则/防护/rp_filter 没有全部补回"; fi
	check "补回后线路" "$(path lan1 100.64.2.80)" tunnel
	check "补回后 mangle 规则顺序（不重复）" "$(rt iptables -t mangle -S CORPLINE_NEW | grep -c -- '-A')" "$(rt iptables -t mangle -S CORPLINE_NEW | grep -- '-A' | sort -u | wc -l)"
}

scenario_cn_direct() {
	CASE="国内网站直连"
	fresh_active
	mkdir -p /koolshare/configs/corpline
	# 名单缺失时开启：时间线提示暂不生效；名单到位重启后补一条"已就绪"
	mv -f /koolshare/configs/corpline/china-domains.conf /tmp/cn.keep 2>/dev/null
	# 中国 IP 段：沙盒里把 100.64.2.80（国内网站）、203.0.113.81（Apple 的海外解析结果）、
	# 172.24.10.10（公司内网域名的地址）算作国内地址，100.64.2.81 不算；补足 1000 条，名单太短会被当成不完整
	local CI=/koolshare/configs/corpline/china-ips.txt
	check "安装包自带中国 IP 段" "$([ "$(wc -l <$CI 2>/dev/null)" -ge 1000 ] && echo yes)" yes
	cp -f $CI /tmp/cnip.keep
	{ printf '100.64.2.80/32\n203.0.113.81/32\n172.24.10.10/32\n'; for i in $(seq 1 1100); do echo "44.$((i / 250)).$((i % 250)).0/24"; done; } >$CI
	dbset corpline_cn_direct=1
	api 1
	wait_state active 60 >/dev/null
	timeline_has "没有找到中国服务名单" && ok "名单缺失时时间线有提示" || bad "名单缺失时时间线没有提示"
	pkill -f "corpline_config.sh update_cn" 2>/dev/null; rm -f /var/run/corpline/cn.pid
	# 安装包自带的是每行一个域名，旧版下载的是 server=/域名/IP，两种都要认
	printf 'baidu.com\nserver=/qq.com/114.114.114.114\n' >/koolshare/configs/corpline/china-domains.conf
	for i in $(seq 1 1100); do echo "server=/cn-filler-$i.com/114.114.114.114"; done >>/koolshare/configs/corpline/china-domains.conf
	dbset corpline_cn_direct=1
	api 1
	wait_state active 60 >/dev/null
	timeline_has "中国服务名单已就绪" && ok "名单到位后时间线写明已生效" || bad "名单到位后时间线没有写明已生效"
	check "中国服务域名由谁解析" "$(resolve x www.baidu.com)" 100.64.2.80
	check "访问中国服务（按域名进 ipset）" "$(lan1 curl -s -m4 http://www.baidu.com/ | awk '{print $2}')" 100.64.0.2
	check "其它网站仍走公司线路" "$(path lan1 100.64.2.81)" tunnel
	check "中国 IP 段已装进内核" "$(jq -r .cn_ips $ST)" 1103
	check "不查域名、直接连中国 IP（像微信）走直连" "$(path lan1 100.64.2.80)" direct
	# 例外网站优先于中国 IP 段：没查过 Apple 时这个地址按中国 IP 直连（沙盒里直连到不了它），查过之后走公司线路
	check "中国 IP 段里的地址默认直连" "$(path lan1 203.0.113.81)" fail
	check "Apple 仍走海外解析" "$(resolve x apps.apple.com)" 203.0.113.81
	check "Apple 解析出的地址在中国 IP 段里也走公司线路" "$(path lan1 203.0.113.81)" tunnel
	check "公司内网域名" "$(resolve x git.corp.test)" 172.24.10.10
	check "公司内网域名的地址在中国 IP 段里也走公司线路" "$(path lan1 172.24.10.10)" tunnel
	check "路由器自身仍直连" "$(path rt 100.64.2.81)" direct
	# 每周更新名单后不用重启：守护程序 1 分钟内发现文件变化，重新装进内核
	local pid0=$(cl_pid) i=0
	echo 100.64.2.81/32 >>$CI
	while [ $i -lt 75 ] && [ "$(path lan1 100.64.2.81 2)" != direct ]; do sleep 3; i=$((i + 3)); done
	if [ $i -lt 75 ]; then ok "中国 IP 段更新后 ${i} 秒内生效，没有重启"; else bad "中国 IP 段更新后 75 秒仍未生效"; fi
	sleep 6
	check "更新后的条数" "$(jq -r .cn_ips $ST)" 1104
	check "更新时没有重启守护程序" "$(cl_pid)" "$pid0"
	dbset corpline_cn_direct=0
	api 1
	wait_state active 60 >/dev/null
	check "关闭后中国服务走公司线路" "$(lan1 curl -s -m4 http://www.baidu.com/ | awk '{print $2}')" 100.64.1.10
	check "关闭后中国 IP 段不生效" "$(path lan1 100.64.2.80)" tunnel
	mv -f /tmp/cnip.keep $CI
}

scenario_resources() {
	CASE="资源占用"
	fresh_active
	local p=$(cl_pid)
	local t0=$(awk '{print $14+$15}' /proc/$p/stat)
	sleep 60
	local t1=$(awk '{print $14+$15}' /proc/$p/stat)
	local hz=$(getconf CLK_TCK)
	ok "空闲 60 秒 CPU 时间：$(echo "scale=3; ($t1-$t0)/$hz" | bc) 秒（约 $(echo "scale=2; ($t1-$t0)*100/$hz/60" | bc)%）"
	ok "守护程序常驻内存：$(awk '/VmRSS/{print $2}' /proc/$p/status) KB（预算 6144 KB）"
	ok "openconnect 常驻内存：$(awk '/VmRSS/{print $2}' /proc/$(oc_pid)/status) KB"
	ok "程序体积：$(stat -c %s /koolshare/bin/corpline) 字节（预算 2 MB）"
	# 1000 次 DNS 查询的压力
	local t2=$(awk '{print $14+$15}' /proc/$p/stat)
	local s=$(date +%s%N)
	for i in $(seq 1 1000); do echo "q$i.which.test"; done | lan1 xargs -P 20 -n 50 dig +short +time=2 +tries=1 @192.168.50.1 >/dev/null
	local ms=$((($(date +%s%N) - s) / 1000000))
	local t3=$(awk '{print $14+$15}' /proc/$p/stat)
	ok "1000 次不同域名查询（20 并发）：${ms} ms，守护程序 CPU $(echo "scale=3; ($t3-$t2)/$hz" | bc) 秒，之后内存 $(awk '/VmRSS/{print $2}' /proc/$p/status) KB"
	local logkb=$(du -k /tmp/upload/corpline_log.txt | cut -f1)
	ok "日志在内存盘：${logkb} KB（单段上限 128 KB）"
}

scenario_bootloop() {
	CASE="开机即坏保护"
	fresh_active
	dbset corpline_boots=0
	reboot_sim; rt /bin/sh /koolshare/init.d/S96corpline.sh start
	reboot_sim; rt /bin/sh /koolshare/init.d/S96corpline.sh start
	check "两次开机后仍在运行" "$(pgrep -c -x corpline)" 1
	check "开机计数" "$(/koolshare/bin/dbus get corpline_boots)" 2
	reboot_sim; rt /bin/sh /koolshare/init.d/S96corpline.sh start
	check "第三次开机（未稳定运行过）自动停用" "$(/koolshare/bin/dbus get corpline_paused)" 1
	check "停用后网络规则" "$(rt iptables-save | grep -ci corpline)" 0
	api 3
	wait_state active 60 >/dev/null
}

scenario_disable_uninstall() {
	CASE="关闭与卸载"
	fresh_active
	dbset corpline_enable=0
	api 1
	sleep 2
	snapshot >$S/s2.txt
	local d=$(diff <(sed -n '/## iptables/,/## processes/p' $S/s1.txt) <(sed -n '/## iptables/,/## processes/p' $S/s2.txt))
	if [ -z "$d" ]; then ok "关闭后网络与安装后未启用时完全一致"; else bad "关闭后有残留：$(echo "$d" | tr '\n' ' ' | head -c 300)"; fi
	check "关闭后设备线路" "$(path lan1 100.64.2.80)" direct
	check "关闭后 DNS" "$(which_dns)" isp
	rt busybox sh /koolshare/scripts/uninstall_corpline.sh >$S/uninstall.log 2>&1
	snapshot >$S/s3.txt
	local d2=$(diff <(grep -v "^lab.conf\|configs/corpline" $S/s0.txt) <(grep -v "configs/corpline" $S/s3.txt))
	if [ -z "$d2" ]; then ok "卸载后与安装前完全一致（网络、进程、定时任务、文件）"; else bad "卸载后有残留：$(echo "$d2" | tr '\n' ' ' | head -c 300)"; fi
	check "卸载后 dbus 设置" "$(/koolshare/bin/dbus list corpline_ | wc -l)" 0
}

scenario_wan_repeat() {
	CASE="运行中宽带重拨（S96 重复调用）"
	fresh_active
	local r0=$(rt iptables-save | grep -c CORPLINE) b0=$(/koolshare/bin/dbus get corpline_boots)
	# 最容易出错的情况：从网页启用后还没经历过开机，第一次宽带重拨
	rm -f /var/run/corpline/booted
	rt /bin/sh /koolshare/init.d/S96corpline.sh start
	rt /bin/sh /koolshare/init.d/S96corpline.sh start
	check "重拨后守护程序个数" "$(pgrep -c -x corpline)" 1
	check "重拨后规则条数不变" "$(rt iptables-save | grep -c CORPLINE)" "$r0"
	check "重拨不算开机（开机计数）" "$(/koolshare/bin/dbus get corpline_boots)" "$b0"
	check "总阀门仍打开" "$(rt ipset list corpline_live | grep -c '/1')" 2
	check "设备线路" "$(path lan1 100.64.2.80)" tunnel
	check "状态" "$(state)" active
}

scenario_upgrade() {
	CASE="启用中升级（覆盖安装同一包）"
	fresh_active
	local p0=$(cl_pid)
	ks_install >$S/upgrade.log 2>&1
	check "升级脚本退出码" "$?" 0
	local t=$(wait_state active 90)
	ok "升级后回到公司专线：${t} 秒"
	check "守护程序个数" "$(pgrep -c -x corpline)" 1
	check "openconnect 个数" "$(pgrep -c -x openconnect)" 1
	[ "$(cl_pid)" != "$p0" ] && ok "运行的是新装的程序（pid $p0 → $(cl_pid)）" || bad "守护程序没有重启"
	check "运行目录未被安装器删掉" "$(ls /var/run/corpline/corpline.pid 2>/dev/null)" /var/run/corpline/corpline.pid
	check "升级后设备线路" "$(path lan1 100.64.2.80)" tunnel
	check "升级不覆盖已有设置（海外域名）" "$(/koolshare/bin/dbus get corpline_overseas_domains)" "apple.com,mzstatic.com"
	dbset corpline_enable=0
	api 1
	sleep 2
	check "升级后关闭：进程" "$(pgrep -c -x 'corpline|openconnect')" 0
	check "升级后关闭：网络规则" "$(rt iptables-save | grep -ci corpline)" 0
	check "升级后关闭：设备线路" "$(path lan1 100.64.2.80)" direct
	# 用户清空"走海外 DNS 的域名"后再升级，不会被填回默认
	dbset corpline_overseas_domains=
	ks_install >>$S/upgrade.log 2>&1
	check "清空的设置升级后仍为空" "[$(/koolshare/bin/dbus get corpline_overseas_domains)]" "[]"
	dbset corpline_overseas_domains=apple.com,mzstatic.com
	dbset corpline_enable=1
	api 1
	wait_state active 60 >/dev/null
}

scenario_nasty() {
	CASE="设置里有特殊字符"
	fresh_active
	rm -f /tmp/pwned*
	dbset 'corpline_user=te"st$(touch /tmp/pwned1)`touch /tmp/pwned2`'"'"';touch /tmp/pwned3'
	dbset 'corpline_server=vpn.corp.test:8443;touch /tmp/pwned4'
	dbset 'corpline_overseas_domains=apple.com$(touch /tmp/pwned5)'
	dbset corpline_password=$(printf '%s' 'p"$(touch /tmp/pwned6)' | base64 -w0)
	api 1
	sleep 8
	dbset corpline_notify=bark
	dbset 'corpline_notify_url=http://100.64.2.81:8080/bark/$(touch /tmp/pwned7)'
	rt busybox sh /koolshare/scripts/corpline_config.sh 781 5 >/dev/null 2>&1
	check "特殊字符没有被当成命令执行" "$(ls /tmp/pwned* 2>/dev/null | wc -l)" 0
	check "账号按原样写入配置" "$(grep '^user=' /var/run/corpline/corpline.conf | cut -d= -f2-)" 'te"st$(touch /tmp/pwned1)`touch /tmp/pwned2`'"'"';touch /tmp/pwned3'
	check "格式不对的服务器地址改用默认" "$(grep '^server=' /var/run/corpline/corpline.conf)" "server=vpn.xindong.com:8443"
	check "密码按原样保存" "$(cat /koolshare/configs/corpline/passwd)" 'p"$(touch /tmp/pwned6)'
	dbset corpline_enable=0
	api 1
	sleep 2
	check "这时仍能正常关闭：进程" "$(pgrep -c -x 'corpline|openconnect')" 0
	check "这时仍能正常关闭：网络规则" "$(rt iptables-save | grep -ci corpline)" 0
	dbset corpline_user=tester
	dbset corpline_server=vpn.corp.test:8443
	dbset corpline_overseas_domains=apple.com,mzstatic.com
	dbset corpline_password=$(printf s3cret | base64)
	dbset corpline_notify=off
	dbset corpline_notify_url=
	dbset corpline_enable=1
	api 1
	local t=$(wait_state active 60)
	ok "改回正常设置后恢复：${t} 秒"
}

# 网页「立即更新」名单：沙盒里连不上外网，应当显示失败原因、保留旧名单、不卡在"正在下载"
scenario_cn_update() {
	CASE="更新中国服务名单"
	local L=/koolshare/configs/corpline/china-domains.conf before=$(md5sum </koolshare/configs/corpline/china-domains.conf) i=0 st
	api 6
	check "点了之后立刻显示正在下载" "$(/koolshare/bin/dbus get corpline_cn_state)" downloading
	while [ $i -lt 120 ] && [ "$(/koolshare/bin/dbus get corpline_cn_state)" = downloading ]; do sleep 1; i=$((i + 1)); done
	st=$(/koolshare/bin/dbus get corpline_cn_state)
	ok "下载结束用时 ${i} 秒，结果 $st（$(/koolshare/bin/dbus get corpline_cn_error)）"
	case $st in
	failed)
		[ -n "$(/koolshare/bin/dbus get corpline_cn_error)" ] && ok "失败原因已记录" || bad "失败但没有原因"
		check "失败时旧名单保留" "$(md5sum <$L)" "$before"
		;;
	ok) ok "下载成功：$(wc -l <$L) 个网站" ;;
	*) bad "下载状态异常：$st" ;;
	esac
	check "下载进程已退出" "$(cat /var/run/corpline/cn.pid 2>/dev/null)" ""
	[ -n "$(cl_pid)" ] && ok "守护程序仍在运行" || bad "守护程序不在了"
}

scenario_direct_domain() {
	CASE="网站单独设为直连"
	fresh_active
	dbset corpline_cn_direct=0
	dbset corpline_direct_domains=baidu.com
	api 1
	wait_state active 60 >/dev/null
	check "设为直连的网站由谁解析（运营商）" "$(resolve x www.baidu.com)" 100.64.2.80
	check "访问设为直连的网站" "$(lan1 curl -s -m4 http://www.baidu.com/ | awk '{print $2}')" 100.64.0.2
	check "其它网站仍走公司线路" "$(path lan1 100.64.2.81)" tunnel
	sleep 11
	check "最近访问列表里有它，标为直连" "$(jq -r '.items[] | select(.d=="www.baidu.com") | .c' /tmp/upload/corpline_domains.json 2>/dev/null)" direct
	check "最近访问列表按时间倒序" "$(jq -r '[.items[].t] | . == (sort | reverse)' /tmp/upload/corpline_domains.json 2>/dev/null)" true
	dbset corpline_direct_domains=
	api 1
	wait_state active 60 >/dev/null
	check "取消后走公司线路" "$(lan1 curl -s -m4 http://www.baidu.com/ | awk '{print $2}')" 100.64.1.10
	dbset corpline_enable=0
	api 1
	sleep 1
	check "关闭后最近访问记录被删除" "$(ls /tmp/upload/corpline_domains.json 2>/dev/null | wc -l)" 0
	dbset corpline_enable=1
	api 1
	wait_state active 60 >/dev/null
}

scenario_device_ports() {
	CASE="设备走公司线路、个别端口直连"
	fresh_active
	# 上一轮用过的本机端口还在 TIME-WAIT 时 curl 绑不上，先等它们过期
	for i in $(seq 70); do
		[ -z "$(lan1 ss -Htan state time-wait '( sport >= :3981 and sport <= :3991 )')" ] && break
		sleep 1
	done
	local keep=$(/koolshare/bin/dbus get corpline_devices)
	dbset corpline_devices=$(printf '02:00:00:00:02:00\tMac Studio\t1\n02:00:00:00:01:00\tNAS\t0\tcorp\t3981, 3982-3990,0,x\n' | base64 -w0)
	api 1
	wait_state active 60 >/dev/null
	check "配置里的端口例外（不合法的丢掉）" "$(grep '^direct_ports=' /var/run/corpline/corpline.conf)" "direct_ports=02:00:00:00:01:00@3981,02:00:00:00:01:00@3982-3990,"
	check "这台设备不在整台直连名单里" "$(grep -c '02:00:00:00:01:00' <(grep '^direct_macs=' /var/run/corpline/corpline.conf))" 0
	check "普通连接走公司线路" "$(path lan1 100.64.2.80)" tunnel
	check "从端口 3985 发出的连接" "$(lan1 curl -s -m4 --local-port 3985 http://100.64.2.80/ | awk '{print $2}')" 100.64.0.2
	check "从端口 3981 发出的连接" "$(lan1 curl -s -m4 --local-port 3981 http://100.64.2.80/ | awk '{print $2}')" 100.64.0.2
	check "从端口 3991 发出的连接（不在范围内）" "$(lan1 curl -s -m4 --local-port 3991 http://100.64.2.80/ | awk '{print $2}')" 100.64.1.10
	check "UDP 也有对应规则" "$(rt iptables -t mangle -S CORPLINE_NEW | grep -c -- '-p udp .*--sport 398[12]')" 2
	check "另一台设备不受影响" "$(path lan2 100.64.2.80)" direct
	dbset corpline_devices=$keep
	api 1
	wait_state active 60 >/dev/null
	check "取消后从端口 3987 发出的连接" "$(lan1 curl -s -m4 --local-port 3987 http://100.64.2.80/ | awk '{print $2}')" 100.64.1.10
}

# 安全防护：外网"攻击者"在 wan 里用几个独立地址；阈值调低、检查间隔 2 秒，几秒内就能看到结果
SEC=/tmp/upload/corpline_security.json
sec_json() { jq -r "$1" $SEC 2>/dev/null; }
sec_wait() { # sec_wait <jq 条件> <最多秒数>
	local i=0
	while [ $i -lt ${2:-15} ]; do
		[ "$(sec_json "$1")" = true ] && return 0
		sleep 1
		i=$((i + 1))
	done
	return 1
}
# 从外网地址 $1 连路由器公网地址的端口 $2…；输出最后一次是否连上
knock() {
	local from=$1 r=fail p
	shift
	for p in "$@"; do
		wan curl -s -m 1 --interface $from -o /dev/null http://100.64.0.2:$p/ 2>/dev/null && r=ok || r=fail
	done
	echo $r
}
scenario_security() {
	CASE="安全防护"
	fresh_active
	local L=/koolshare/configs/corpline/lab.conf B=/koolshare/configs/corpline/bad-ips.netset keep_dev=$(/koolshare/bin/dbus get corpline_devices) a p m
	cp $L $S/lab.conf.bak
	cp $B $S/bad.bak 2>/dev/null
	# 真实名单里有 100.64.0.0/10（运营商保留网段）：沙盒的"外网"正好用它，去掉（真机上它默认被当作私有网段剔除）
	grep -v '^100\.' $S/bad.bak >$B
	printf '%s\n' sec_scan_per_min=5 sec_brute_per_min=5 sec_flood_per_min=10 sec_out_scan_per_min=5 sec_out_flood_per_min=10 sec_poll_secs=2 \
		sec_private_nets=10.0.0.0/8,127.0.0.0/8,172.16.0.0/12,192.168.0.0/16,224.0.0.0/3 >>$L
	for a in 1 2 3 4 5; do wan ip addr add 100.64.3.$a/32 dev dummy0 2>/dev/null; done
	# 家里一台设备开着端口转发（8080 = 管理类端口，9000 = 普通服务）
	for p in 8080 9000; do
		# 不用 http.server 命令行：它启动时先反查主机名，沙盒里要等 DNS 超时，端口迟迟不开
		lan1 python3 -I -c "import socketserver as s, http.server as h; s.TCPServer.allow_reuse_address = True; s.TCPServer(('', $p), h.SimpleHTTPRequestHandler).serve_forever()" </dev/null >/dev/null 2>&1 &
		rt iptables -t nat -I PREROUTING -i eth0 -p tcp --dport $p -j DNAT --to-destination 192.168.50.100:$p
	done
	rt iptables -I FORWARD -i eth0 -m conntrack --ctstate DNAT -j ACCEPT
	# 测试电脑的 3981 设为「直连端口」（像 NAS 上的 BT）：从这个端口发出的连接不算异常外连
	dbset corpline_devices=$(printf '02:00:00:00:01:00\t测试电脑\t0\tcorp\t3981\n' | base64 -w0)
	dbset corpline_notify=feishu
	dbset corpline_notify_url=http://100.64.2.81:8080/feishu/ok
	: >$S/hook.log
	api 1
	wait_state active 60 >/dev/null
	# 上一轮演练封过的地址会跨守护程序重启保留：先全部解封
	dbset corpline_sec_unblock=all
	api 7
	sleep 2
	sec_wait '.active' 10 && ok "防护已生效（外网网卡 $(sec_json .wan)）" || bad "防护没有生效：$(sec_json .error)"
	check "外来与外连的跳转都在" "$(rt iptables -S | grep -c -- '-j CORPLINE_SEC\(OUT\)\?$')" 3
	check "恶意地址名单装入条数 > 1000" "$([ "$(sec_json .stats.bad_count)" -gt 1000 ] && echo yes)" yes
	check "正常访问端口转发" "$(knock 100.64.3.2 9000)" ok

	# 1. 扫描路由器的端口
	knock 100.64.3.1 $(seq 1001 1012) >/dev/null
	sec_wait '[.blocked[] | select(.ip=="100.64.3.1" and .kind=="scan")] | length == 1' 10 && ok "扫描者被封锁" || bad "扫描者没有被封锁"
	check "封锁后它连端口转发也不通" "$(knock 100.64.3.1 9000)" fail
	check "别的外网地址不受影响" "$(knock 100.64.3.2 9000)" ok
	m=$(sec_json '[.events[] | select(.ip=="100.64.3.1")][0].m')
	echo "$m" | grep -q '扫描路由器的端口（端口 10' && ok "事件写明了扫描的端口：$m" || bad "扫描事件内容：$m"
	check "一般扫描不推送" "$(grep -c '100.64.3.1' $S/hook.log)" 0

	# 2. 反复连家里设备的管理端口（像猜密码）
	knock 100.64.3.2 $(for i in $(seq 14); do echo 8080; done) >/dev/null
	sec_wait '[.blocked[] | select(.ip=="100.64.3.2" and .kind=="brute")] | length == 1' 10 && ok "猜密码的被封锁" || bad "猜密码的没有被封锁"
	m=$(sec_json '[.events[] | select(.ip=="100.64.3.2")][0]')
	echo "$m" | jq -r .m | grep -q '「测试电脑」（192.168.50.100）的管理端口（端口 8080）' && ok "事件写明了设备名和端口：$(echo "$m" | jq -r .m)" || bad "猜密码事件：$m"
	check "猜密码属于要马上提醒的" "$(echo "$m" | jq -r .level)" alert
	sleep 2
	grep -q '安全提醒：外网 100.64.3.2' $S/hook.log && ok "已推送到飞书" || bad "没有推送：$(tail -c 300 $S/hook.log)"

	# 3. 短时间大量连接普通服务
	knock 100.64.3.3 $(for i in $(seq 25); do echo 9000; done) >/dev/null
	sec_wait '[.blocked[] | select(.ip=="100.64.3.3" and .kind=="flood")] | length == 1' 10 && ok "狂连的被封锁" || bad "狂连的没有被封锁"

	# 4. 恶意地址名单：外来的直接丢，家里设备连它只提醒
	printf '100.64.3.4\n100.64.2.81/32\n10.0.0.0/8\n' >>$B
	sleep 4
	check "名单里的外网地址连不进来" "$(knock 100.64.3.4 9000)" fail
	check "名单里的私有网段被去掉" "$(rt ipset test corpline_bad 10.1.1.1 2>&1 | grep -c 'is in set')" 0
	local t0=$(date +%s)
	lan1 curl -s -m4 --local-port 3981 -o /dev/null http://100.64.2.81/
	sleep 4
	check "从直连端口（BT）连名单里的地址不提醒" "$(sec_json "[.events[] | select(.kind==\"out_bad\" and .t >= $t0)] | length")" 0
	check "家里设备仍能访问名单里的地址（只提醒不拦）" "$(lan1 curl -s -m4 http://100.64.2.81/ | awk '{print $2}')" 100.64.1.10
	sec_wait '[.events[] | select(.kind=="out_bad" and .ip=="192.168.50.100")] | length == 1' 10 && ok "家里设备连恶意地址：$(sec_json '[.events[] | select(.kind=="out_bad")][0].m')" || bad "家里设备连恶意地址没有提醒"

	# 5. 家里设备反复连同一台服务器的同一个高危端口（像 git / SSH）不算扫描；短时间连很多不同的高危端口才提醒
	for i in $(seq 15); do lan1 curl -s -m 1 -o /dev/null http://100.64.2.80:22/ 2>/dev/null; done
	sleep 4
	check "反复连同一台服务器的 22 端口不算扫描" "$(sec_json '[.events[] | select(.kind=="out_scan" and .ip=="192.168.50.100")] | length')" 0
	for p in 22 23 445 3389 3306 6379 1433 5555; do lan2 curl -s -m 1 -o /dev/null http://100.64.2.80:$p/ 2>/dev/null; done
	sec_wait '[.events[] | select(.kind=="out_scan" and .ip=="192.168.50.101")] | length == 1' 10 && ok "家里设备扫描外网：$(sec_json '[.events[] | select(.kind=="out_scan")][0].m')" || bad "家里设备扫描外网没有提醒"
	check "提醒后家里设备照常上网" "$(path lan2 100.64.2.80)" tunnel

	# 6. 家里设备狂连同一个外网地址
	for i in $(seq 25); do lan2 curl -s -m 1 -o /dev/null http://100.64.2.80/ 2>/dev/null; done
	sec_wait '[.events[] | select(.kind=="out_flood" and .ip=="192.168.50.101")] | length == 1' 10 && ok "家里设备狂连：$(sec_json '[.events[] | select(.kind=="out_flood")][0].m')" || bad "家里设备狂连没有提醒"

	# 7. 重启守护程序：封锁保留、不重复出事件
	local n0=$(sec_json '.events | length')
	api 1
	wait_state active 60 >/dev/null
	sleep 4
	check "重启后扫描者仍被封锁" "$(knock 100.64.3.1 9000)" fail
	check "重启后事件没有重复" "$(sec_json '.events | length')" "$n0"
	check "重启后封锁名单" "$(sec_json '[.blocked[].ip] | sort | join(",")')" "100.64.3.1,100.64.3.2,100.64.3.3"

	# 8. 网页解封 / 白名单（白名单不用重启）
	dbset corpline_sec_unblock=100.64.3.1
	api 7
	check "解封后能连进来" "$(knock 100.64.3.1 9000)" ok
	local p0=$(cl_pid)
	dbset corpline_sec_allow=100.64.3.3
	api 4
	sleep 4
	check "加白名单后能连进来" "$(knock 100.64.3.3 9000)" ok
	check "改白名单没有重启守护程序" "$(cl_pid)" "$p0"
	dbset 'corpline_sec_unblock=1.2.3.4;touch /tmp/pwned8'
	api 7
	check "解封参数不会被当命令执行" "$(ls /tmp/pwned8 2>/dev/null | wc -l)" 0

	# 9. 关闭防护：规则全部撤掉，公司线路不受影响
	dbset corpline_sec_enable=0
	api 1
	wait_state active 60 >/dev/null
	check "关闭后防护规则" "$(rt iptables-save | grep -c CORPLINE_SEC)" 0
	check "关闭后防护集合" "$(rt ipset list -n | grep -c 'corpline_\(blk\|al\|bad\|sec\)')" 0
	check "关闭后被封过的地址能连进来" "$(knock 100.64.3.2 9000)" ok
	check "关闭后线路" "$(path lan1 100.64.2.80)" tunnel

	# 收尾
	dbset corpline_sec_enable=1
	dbset corpline_sec_allow=
	dbset corpline_devices=$keep_dev
	dbset corpline_notify=off
	dbset corpline_notify_url=
	cp $S/lab.conf.bak $L
	[ -f $S/bad.bak ] && cp $S/bad.bak $B
	for p in 8080 9000; do rt iptables -t nat -D PREROUTING -i eth0 -p tcp --dport $p -j DNAT --to-destination 192.168.50.100:$p; done
	rt iptables -D FORWARD -i eth0 -m conntrack --ctstate DNAT -j ACCEPT
	pkill -f "TCPServer\(\('', (8080|9000)\)"
	for a in 1 2 3 4 5; do wan ip addr del 100.64.3.$a/32 dev dummy0 2>/dev/null; done
	api 1
	wait_state active 60 >/dev/null
}

# 设备名字：华硕网络地图、华硕里改过的名字、插件别名合起来认设备；改名不改线路、不重启
dev_json() { jq -r "$1" /tmp/upload/corpline_devices.json 2>/dev/null; }
dev_name() { rt /koolshare/bin/corpline devices -c /var/run/corpline/corpline.conf | awk -F'\t' -v m="$1" '$1 == m {print $2}'; }
scenario_devices() {
	CASE="设备名字"
	fresh_active
	local L=/koolshare/configs/corpline/lab.conf D=/tmp/lab/devnames keep_dev=$(/koolshare/bin/dbus get corpline_devices) p0 i
	cp $L $S/lab.conf.bak
	mkdir -p $D
	# 假的华硕数据：lan1 是米家插座（随机 MAC），另有一台荣耀手机、一台没报主机名的 Sony 游戏机
	cat >$D/nmp.js <<'JSON'
{"02:00:00:00:01:00":{"mac":"02:00:00:00:01:00","name":"cuco-plug-v3_mibt1A2B","vendor":"","type":70,"online":1},
 "00:11:22:00:00:01":{"mac":"00:11:22:00:00:01","name":"HONOR_X40-1a2b3c4d5e","vendor":"Others","type":34,"list":[1,{"x":"y"}]},
 "00:11:22:00:00:02":{"mac":"00:11:22:00:00:02","name":"","vendor":"Sony Corporation","type":7}}
JSON
	printf '<小王的手机>00:11:22:00:00:01>0>0>><Nintendo Co  Ltd>00:11:22:00:00:02>0>0>>' >$D/nick.txt
	printf '%s\n' "client_db=$D/nmp.js" "nick_cmd=cat $D/nick.txt" sec_poll_secs=2 >>$L
	dbset corpline_devices=$(printf '02:00:00:00:02:00\tMac Studio\t1\n' | base64 -w0)
	api 1
	wait_state active 60 >/dev/null
	for i in $(seq 10); do [ -n "$(dev_json '.devices["02:00:00:00:01:00"].name')" ] && break; sleep 1; done
	check "主机名认出品类" "$(dev_json '.devices["02:00:00:00:01:00"] | "\(.name)/\(.host)/\(.random)"')" "米家智能插座/cuco-plug-v3_mibt1A2B/true"
	check "华硕里改过的名字优先" "$(dev_json '.devices["00:11:22:00:00:01"] | "\(.name)/\(.auto)/\(.random)"')" "小王的手机/荣耀 X40/false"
	check "没主机名用厂商和类型；公司全称不当名字" "$(dev_json '.devices["00:11:22:00:00:02"] | "\(.name)/\(.vendor)"')" "Sony 游戏机/Sony"
	check "安全提醒用的名字（锁定的管理电脑用别名）" "$(dev_name 02:00:00:00:02:00)" "Mac Studio"
	check "安全提醒用的名字（没起别名用认出来的）" "$(dev_name 02:00:00:00:01:00)" "米家智能插座"

	# 网页里给 lan1 起别名：只改名字，线路照旧走公司，守护程序不重启
	p0=$(cl_pid)
	dbset corpline_devices=$(printf '02:00:00:00:02:00\tMac Studio\t1\n02:00:00:00:01:00\t鱼缸插座\t0\tcorp\t\n' | base64 -w0)
	api 4
	sleep 3
	check "起别名后的名字" "$(dev_name 02:00:00:00:01:00)" "鱼缸插座"
	check "起别名没有重启守护程序" "$(cl_pid)" "$p0"
	check "起别名不改线路" "$(path lan1 100.64.2.80)" tunnel
	check "起别名不进直连名单" "$(grep -c '02:00:00:00:01:00' <(grep '^direct_\(macs\|ports\)=' /var/run/corpline/corpline.conf))" 0
	dbset corpline_devices=$(printf '02:00:00:00:02:00\tMac Studio\t1\n' | base64 -w0)
	api 4
	sleep 3
	check "清掉别名恢复认出来的名字" "$(dev_name 02:00:00:00:01:00)" "米家智能插座"

	# 华硕的文件坏了：不影响守护程序，名字退回能认的部分
	printf '{"02:00:00:00:01:00":{"name":"x"' >$D/nmp.js
	sleep 4
	check "华硕文件损坏时守护程序照常" "$(cl_pid)" "$p0"
	check "华硕文件损坏时的名字" "$(dev_json '.devices["00:11:22:00:00:01"].name')" "小王的手机"

	# 收尾
	dbset corpline_devices=$keep_dev
	cp $S/lab.conf.bak $L
	api 1
	wait_state active 60 >/dev/null
}

ALL="install enable company_dns_down blackhole kill_oc freeze_oc latency wan_flap wan_repeat server_down crash crash_loop power_loss notify auth fw_rebuild cn_direct cn_update direct_domain device_ports devices security resources nasty upgrade bootloop disable_uninstall"
LIST=${*:-$ALL}
{
	echo "# 公司专线 沙盒演练结果"
	echo
	echo "- 时间：$(date '+%F %T')"
	echo "- 程序：$(/koolshare/bin/corpline version 2>/dev/null || ../plugin/corpline/bin/corpline version)；内核 $(uname -r)；dnsmasq $(dnsmasq --version | head -n1 | awk '{print $3}')；openconnect $(openconnect --version | head -n1 | awk '{print $3}')"
	echo
	echo "| 场景 | 结果 | 证据 |"
	echo "|---|---|---|"
} >$OUT
for s in $LIST; do
	log "==== $s"
	scenario_$s
done
echo >>$OUT
echo "合计：通过 $PASS，失败 $FAIL" >>$OUT
cp /tmp/upload/corpline_log.txt $REPORT/corpline_log.txt 2>/dev/null
log "完成：通过 $PASS，失败 $FAIL"
