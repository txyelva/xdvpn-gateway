#!/bin/bash
# 用真实的中国服务名单（约 11 万条）测内存与解析：sudo bash lab/mem_cn.sh <名单文件>
cd "$(dirname "$0")"
. ./lib.sh
LIST=${1:-.state/china.conf}
[ -f "$LIST" ] || { echo "缺少名单文件 $LIST"; exit 1; }
rm -rf /tmp/ks && mkdir -p /tmp/ks && tar -C /tmp/ks -xzf ../corpline.tar.gz
rt busybox sh /tmp/ks/corpline/install.sh >/dev/null 2>&1
mkdir -p /koolshare/configs/corpline
printf 'cafile=%s\noc_ld_library_path=\nisp_dns_file=%s\n' $S/pki/ca.pem $S/resolv.dnsmasq >/koolshare/configs/corpline/lab.conf
cp "$LIST" /koolshare/configs/corpline/china-domains.conf
# 沙盒里的运营商 DNS 只认识少数域名；把名单里前 300 个域名也登记进去，模拟真实解析
head -n 300 "$LIST" | sed -n 's#^server=/\([^/]*\)/.*#address=/\1/100.64.2.80#p' >>$S/dns/isp.conf
kill $(cat $S/dns/isp.pid); sleep 0.5; wan dnsmasq -C $S/dns/isp.conf
dbset corpline_enable=1
dbset corpline_server=vpn.corp.test:8443
dbset corpline_user=tester
dbset corpline_password=$(printf s3cret | base64)
dbset corpline_cn_direct=1
api 1
wait_state active 60 >/dev/null
p=$(pgrep -x corpline)
echo "名单条目：$(grep -c '^server=/' "$LIST")"
echo "加载名单后守护程序内存：$(awk '/VmRSS/{print $2}' /proc/$p/status) KB"
grep "中国服务名单" /tmp/upload/corpline_log.txt | tail -n1
s=$(date +%s%N)
head -n 300 "$LIST" | sed -n 's#^server=/\([^/]*\)/.*#www.\1#p' | lan1 xargs -P 10 -n 30 dig +short +time=3 +tries=1 @192.168.50.1 >/dev/null
echo "300 个中国服务域名解析：$((($(date +%s%N) - s) / 1000000)) ms"
echo "ipset 直连集合条目：$(rt ipset list corpline_cn | grep -c timeout)"
echo "解析后守护程序内存：$(awk '/VmRSS/{print $2}' /proc/$p/status) KB"
d=$(head -n 1 "$LIST" | sed -n 's#^server=/\([^/]*\)/.*#\1#p')
echo "名单内域名 $d 访问线路：$(lan1 curl -s -m4 http://$d/ | awk '{print ($2=="100.64.0.2")?"direct":$2}')"
echo "名单外网站访问线路：$(path lan1 100.64.2.81)"
rt busybox sh /koolshare/scripts/uninstall_corpline.sh >/dev/null 2>&1
echo "已卸载"
