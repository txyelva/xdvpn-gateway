#!/bin/bash
# 构建插件离线安装包：corpline.tar.gz（在沙盒虚拟机或任意有 Rust + musl 目标的 Linux 上执行）
set -e
cd "$(dirname "$0")"
. ~/.cargo/env 2>/dev/null || true
( cd corpline && cargo build --release --target aarch64-unknown-linux-musl )
install -m 755 corpline/target/aarch64-unknown-linux-musl/release/corpline plugin/corpline/bin/corpline
rm -f corpline.tar.gz
tar -C plugin --exclude='.DS_Store' --exclude='icon-corpline-b.png' -czf corpline.tar.gz corpline
ls -la corpline.tar.gz
