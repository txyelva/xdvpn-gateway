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
rm -f corpline.tar.gz
tar -C plugin --exclude='.DS_Store' --exclude='icon-corpline-b.png' -czf corpline.tar.gz corpline
ls -la corpline.tar.gz
