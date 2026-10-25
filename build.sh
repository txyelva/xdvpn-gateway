#!/bin/bash
# 构建插件离线安装包：corpline.tar.gz（在沙盒虚拟机或任意有 Rust + musl 目标的 Linux 上执行）
set -e
cd "$(dirname "$0")"
. ~/.cargo/env 2>/dev/null || true
( cd corpline && cargo build --release --target aarch64-unknown-linux-musl )
install -m 755 corpline/target/aarch64-unknown-linux-musl/release/corpline plugin/corpline/bin/corpline
# 软件中心离线安装（ks_tar_install.sh）的硬性检查，先在这里查一遍
P=plugin/corpline
[ "$(cat $P/.valid 2>/dev/null)" = hnd ] || { echo "缺少 $P/.valid（内容 hnd），软件中心会拒装"; exit 1; }
[ -f $P/install.sh ] && [ -d $P/scripts ] && [ -f $P/webs/Module_corpline.asp ] || { echo "缺少 install.sh / scripts / webs/Module_corpline.asp"; exit 1; }
! grep -q "ks_tar_install\|detect_package" $P/install.sh || { echo "install.sh 里不能出现 ks_tar_install / detect_package"; exit 1; }
# 中国服务名单随安装包带上：装好就能用，不依赖路由器第一次下载成功（之后路由器每周自动更新）
L=$P/configs/china-domains.conf
mkdir -p $P/configs
for u in https://cdn.jsdelivr.net/gh/felixonmars/dnsmasq-china-list@master/accelerated-domains.china.conf \
	https://raw.githubusercontent.com/felixonmars/dnsmasq-china-list/master/accelerated-domains.china.conf; do
	curl -fsSL -m 120 "$u" 2>/dev/null | sed -n 's#^server=/\([^/]*\)/.*#\1#p' >$L.tmp || true
	[ "$(wc -l <$L.tmp)" -ge 50000 ] && mv -f $L.tmp $L && break
done
rm -f $L.tmp
[ -f $L ] && [ "$(wc -l <$L)" -ge 50000 ] || { echo "中国服务名单下载失败，且没有可用的旧名单"; exit 1; }
echo "中国服务名单 $(wc -l <$L) 条"
# 公开的恶意 IP 名单（安全防护用，路由器之后每周自动更新）
B=$P/configs/bad-ips.netset
for u in https://cdn.jsdelivr.net/gh/firehol/blocklist-ipsets@master/firehol_level1.netset \
	https://raw.githubusercontent.com/firehol/blocklist-ipsets/master/firehol_level1.netset; do
	curl -fsSL -m 120 "$u" 2>/dev/null | grep -E '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+(/[0-9]+)?$' >$B.tmp || true
	[ "$(wc -l <$B.tmp)" -ge 1000 ] && mv -f $B.tmp $B && break
done
rm -f $B.tmp
[ -f $B ] && [ "$(wc -l <$B)" -ge 1000 ] || { echo "恶意地址名单下载失败，且没有可用的旧名单"; exit 1; }
echo "恶意地址名单 $(wc -l <$B) 条"
rm -f corpline.tar.gz
tar -C plugin --exclude='.DS_Store' --exclude='icon-corpline-b.png' -czf corpline.tar.gz corpline
ls -la corpline.tar.gz
