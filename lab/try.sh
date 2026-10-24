#!/bin/bash
# 手动试跑：安装 → 填账号 → 启用
cd "$(dirname "$0")"; . ./lib.sh
mkdir -p $S/snap
snapshot >$S/snap/s0.txt
ks_install
snapshot >$S/snap/s1.txt
mkdir -p /koolshare/configs/corpline
cat >/koolshare/configs/corpline/lab.conf <<EOC
cafile=$S/pki/ca.pem
oc_ld_library_path=
isp_dns_file=$S/resolv.dnsmasq
EOC
dbset corpline_enable=1
dbset corpline_server=vpn.corp.test:8443
dbset corpline_user=tester
dbset corpline_password=$(printf s3cret | base64)
dbset corpline_devices=$(printf '02:00:00:00:02:00\tMac Studio\t1\n' | base64 -w0)
api 1
