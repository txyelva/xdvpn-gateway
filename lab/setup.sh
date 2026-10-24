#!/bin/bash
# 沙盒网络：在 Linux 虚拟机里用网络命名空间搭一个"家 + 路由器 + 运营商 + 公司"的完整环境。
# 与真实路由器、Mac 的网络完全隔离；全部可以 teardown.sh 一键拆掉。
#
#   lan1 (普通设备 192.168.50.100)  ─┐
#   lan2 (直连设备 192.168.50.101)  ─┴─ br0 [rt 路由器 192.168.50.1] eth0 100.64.0.2
#                                                         │
#                              [wan 运营商/互联网 100.64.0.1]  运营商 DNS 100.64.0.53（会"污染" google）
#                                │  国内/直连网站 100.64.2.80、100.64.2.81
#                              [corp 公司 100.64.1.10]  ocserv（假的公司 VPN，vpn.corp.test:8443）
#                                   公司 DNS 172.24.4.79、海外 DNS 8.8.8.8 / 1.1.1.1、
#                                   只有经公司线路才能到的网站 203.0.113.80/81、内网 172.24.10.10
set -e
cd "$(dirname "$0")"
LAB=$(pwd)
S=/tmp/lab
bash "$LAB/teardown.sh" >/dev/null 2>&1 || true
mkdir -p $S/pki $S/ocserv $S/dns /etc/netns/rt /etc/netns/lan1 /etc/netns/lan2 /etc/netns/corp /etc/netns/wan

modprobe -a tun ip_set xt_set ip_set_hash_net ip_set_hash_ip xt_connmark xt_mark xt_mac xt_TCPMSS xt_state sch_netem 2>/dev/null || true

for ns in wan corp rt lan1 lan2; do ip netns add $ns; ip -n $ns link set lo up; done

veth() { # veth <ns1> <if1> <ns2> <if2>
	ip link add $2 netns $1 type veth peer name $4 netns $3
	ip -n $1 link set $2 up
	ip -n $3 link set $4 up
}

# ---- wan：运营商 + 互联网 ----
veth wan wan-rt rt eth0
veth wan wan-corp corp eth0
ip -n wan addr add 100.64.0.1/24 dev wan-rt
ip -n wan addr add 100.64.1.1/24 dev wan-corp
ip -n wan link add dummy0 type dummy; ip -n wan link set dummy0 up
for a in 100.64.0.53 100.64.2.80 100.64.2.81; do ip -n wan addr add $a/32 dev dummy0; done
ip netns exec wan sysctl -qw net.ipv4.ip_forward=1

# ---- corp：公司网络（VPN 服务器 + 公司 DNS + 公司出口）----
ip -n corp addr add 100.64.1.10/24 dev eth0
ip -n corp route add default via 100.64.1.1
ip -n corp link add dummy0 type dummy; ip -n corp link set dummy0 up
for a in 172.24.4.79 8.8.8.8 1.1.1.1 203.0.113.80 203.0.113.81 172.24.10.10; do ip -n corp addr add $a/32 dev dummy0; done
ip netns exec corp sysctl -qw net.ipv4.ip_forward=1
ip netns exec corp iptables -t nat -A POSTROUTING -s 10.235.128.0/18 -o eth0 -j MASQUERADE

# ---- rt：路由器 ----
ip -n rt addr add 100.64.0.2/24 dev eth0
ip -n rt route add default via 100.64.0.1
ip -n rt link add br0 type bridge; ip -n rt link set br0 up
ip -n rt addr add 192.168.50.1/24 dev br0
ip netns exec rt sysctl -qw net.ipv4.ip_forward=1
# 与华硕固件一致：严格反向路径过滤默认开着
ip netns exec rt sysctl -qw net.ipv4.conf.all.rp_filter=1
ip netns exec rt sysctl -qw net.ipv4.conf.default.rp_filter=1
ip netns exec rt sysctl -qw net.ipv4.conf.br0.rp_filter=1
for n in 1 2; do
	ip link add lan$n-p netns rt type veth peer name eth0 netns lan$n
	ip -n rt link set lan$n-p master br0 up
	ip -n lan$n link set eth0 address 02:00:00:00:0$n:00
	ip -n lan$n link set eth0 up
	ip -n lan$n addr add 192.168.50.10$((n - 1))/24 dev eth0
	ip -n lan$n route add default via 192.168.50.1
	echo "nameserver 192.168.50.1" >/etc/netns/lan$n/resolv.conf
done
echo "nameserver 127.0.0.1" >/etc/netns/rt/resolv.conf
echo "nameserver 100.64.0.53" >/etc/netns/corp/resolv.conf
echo "nameserver 100.64.0.53" >/etc/netns/wan/resolv.conf

# 华硕风格防火墙：转发默认丢弃，只放行已建立连接与来自 LAN 的连接
R="ip netns exec rt"
$R iptables -P FORWARD DROP
$R iptables -A FORWARD -m state --state RELATED,ESTABLISHED -j ACCEPT
$R iptables -A FORWARD -i br0 -j ACCEPT
$R iptables -P INPUT DROP
$R iptables -A INPUT -m state --state RELATED,ESTABLISHED -j ACCEPT
$R iptables -A INPUT -i lo -j ACCEPT
$R iptables -A INPUT -i br0 -j ACCEPT
$R iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
$R ip6tables -P FORWARD ACCEPT
$R iptables -t mangle -S >/dev/null   # 华硕固件的 mangle 表本来就在用（QoS 等）

# ---- 假 DNS 服务器 ----
fake_dns() { # fake_dns <ns> <name> <listen-ips> <records...>
	local ns=$1 name=$2 listen=$3
	shift 3
	local conf=$S/dns/$name.conf
	{
		echo "no-resolv"
		echo "no-hosts"
		echo "bind-interfaces"
		for a in $listen; do echo "listen-address=$a"; done
		echo "except-interface=lo"
		echo "log-queries"
		echo "local-ttl=600"
		echo "log-facility=$S/dns/$name.log"
		echo "pid-file=$S/dns/$name.pid"
		for r in "$@"; do echo "$r"; done
	} >$conf
	ip netns exec $ns dnsmasq -C $conf
}
fake_dns wan isp 100.64.0.53 \
	address=/vpn.corp.test/100.64.1.10 address=/www.google.com/100.64.9.9 address=/www.baidu.com/100.64.2.80 \
	address=/www.qq.com/100.64.2.80 address=/apps.apple.com/100.64.2.81 address=/which.test/100.64.2.80 \
	address=/whoami.test/100.64.2.80 local=/corp.test/
fake_dns corp company 172.24.4.79 \
	address=/www.google.com/203.0.113.80 address=/www.baidu.com/100.64.2.80 address=/www.qq.com/100.64.2.80 \
	address=/apps.apple.com/100.64.2.81 address=/which.test/203.0.113.80 address=/git.corp.test/172.24.10.10 \
	address=/corp.test/172.24.4.79 address=/vpn.corp.test/100.64.1.10 address=/whoami.test/203.0.113.80
fake_dns corp overseas "8.8.8.8 1.1.1.1" \
	address=/www.google.com/203.0.113.80 address=/apps.apple.com/203.0.113.81 address=/which.test/203.0.113.81 \
	address=/www.baidu.com/100.64.2.80 local=/corp.test/

# ---- 网站：回显"我是谁、你从哪来"，用来判断走的是哪条线 ----
for ns in wan corp; do
	ip netns exec $ns nohup python3 -I "$LAB/whoami.py" >$S/web-$ns.log 2>&1 &
done
ip netns exec wan nohup python3 -I "$LAB/hook.py" >$S/hook-web.log 2>&1 &

# ---- ocserv：假的公司 VPN（自签 CA，openconnect 用 --cafile 校验，与真实 CA 校验流程一致）----
cd $S/pki
openssl req -x509 -newkey rsa:2048 -nodes -days 30 -subj "/CN=Lab CA" -keyout ca.key -out ca.pem 2>/dev/null
openssl req -newkey rsa:2048 -nodes -subj "/CN=vpn.corp.test" -keyout server.key -out server.csr 2>/dev/null
printf "subjectAltName=DNS:vpn.corp.test\nextendedKeyUsage=serverAuth\n" >ext.cnf
openssl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -CAcreateserial -days 30 -extfile ext.cnf -out server.pem 2>/dev/null
cd "$LAB"
printf 's3cret\ns3cret\n' | ocpasswd -c $S/ocserv/passwd tester
cat >$S/ocserv/ocserv.conf <<EOF
auth = "plain[passwd=$S/ocserv/passwd]"
listen-host = 100.64.1.10
tcp-port = 8443
udp-port = 8443
run-as-user = nobody
run-as-group = nogroup
socket-file = $S/ocserv/ocserv.sock
server-cert = $S/pki/server.pem
server-key = $S/pki/server.key
isolate-workers = false
max-clients = 16
max-same-clients = 4
keepalive = 300
dpd = 10
mobile-dpd = 10
try-mtu-discovery = false
auth-timeout = 240
cookie-timeout = 300
deny-roaming = false
rekey-time = 172800
rekey-method = ssl
use-occtl = true
occtl-socket-file = $S/ocserv/occtl.sock
pid-file = $S/ocserv/ocserv.pid
device = vpns
predictable-ips = true
default-domain = corp.test
ipv4-network = 10.235.128.0
ipv4-netmask = 255.255.192.0
dns = 172.24.4.79
ping-leases = false
cisco-client-compat = true
dtls-legacy = true
EOF
ip netns exec corp nohup ocserv -f -c $S/ocserv/ocserv.conf >$S/ocserv/ocserv.log 2>&1 &

# ---- 模拟软件中心环境（dbus / cru / service / base.sh）----
bash "$LAB/mock/install-mock.sh"

# ---- 路由器上的 dnsmasq（和华硕一样：上游写在 servers-file，另读 dnsmasq.d）----
echo "server=100.64.0.53" >$S/resolv.dnsmasq
cat >$S/dnsmasq-rt.conf <<EOF
interface=br0
interface=lo
bind-interfaces
no-resolv
no-hosts
servers-file=$S/resolv.dnsmasq
conf-dir=/jffs/configs/dnsmasq.d/,*.conf
pid-file=/var/run/dnsmasq.pid
cache-size=1500
log-queries
log-facility=$S/dnsmasq-rt.log
EOF
ip netns exec rt dnsmasq -C $S/dnsmasq-rt.conf

# 看护定时器模拟（cru 的 */1 任务每 60 秒执行一次）
ip netns exec rt nohup bash "$LAB/mock/crond.sh" >$S/crond.log 2>&1 &

sleep 2
echo "沙盒已就绪"
