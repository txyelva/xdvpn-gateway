#!/bin/sh
# 公司专线（corpline）安装脚本 —— 由软件中心离线安装调用；非 0 退出 = 安装失败。
# 安装只复制文件、写默认设置，不改任何网络配置；默认"未启用"，对全家网络零影响。

. /koolshare/scripts/base.sh
DIR=$(cd $(dirname $0) && pwd)
module=corpline
TITLE="公司专线"
DESCR="全家走公司线路，异常时自动回到直连"
PLVER=$(cat $DIR/version 2>/dev/null)

fail() {
	echo_date "$1"
	echo_date "安装未完成，没有改动你的网络。"
	exit 1
}

platform_test() {
	local kv=$(uname -r | awk -F. '{print $1*100+$2}')
	[ -d /koolshare ] || fail "没有找到软件中心目录 /koolshare，本插件只适用于 koolcenter 固件。"
	[ "$kv" -ge 401 ] || fail "内核版本过旧（$(uname -r)），本插件适用于 hnd/axhnd 平台。"
	case "$(uname -m)" in
	aarch64 | armv8*) ;;
	*) fail "本插件的程序为 aarch64 编译，你的机型是 $(uname -m)，暂不支持。" ;;
	esac
	$DIR/bin/corpline version >/dev/null 2>&1 || fail "程序无法在本机运行（$(uname -m)）。"
}

# 先复制成临时文件再改名：正在运行的程序不会被写坏，复制到一半失败也不会留下半个文件
put() {
	cp -f "$1" "$2.new" && chmod "$3" "$2.new" && mv -f "$2.new" "$2" || fail "复制 $(basename $1) 失败（存储空间不足？）"
}

install_now() {
	if [ -f /koolshare/scripts/corpline_config.sh ]; then
		echo_date "检测到旧版本，先停止运行中的服务…"
		/bin/sh /koolshare/scripts/corpline_config.sh stop >/dev/null 2>&1
	fi
	echo_date "复制插件文件…"
	mkdir -p /koolshare/bin /koolshare/scripts /koolshare/webs /koolshare/res /koolshare/configs/corpline
	put $DIR/bin/corpline /koolshare/bin/corpline 755
	put $DIR/scripts/corpline_config.sh /koolshare/scripts/corpline_config.sh 755
	put $DIR/webs/Module_corpline.asp /koolshare/webs/Module_corpline.asp 644
	put $DIR/res/icon-corpline.png /koolshare/res/icon-corpline.png 644
	[ -f $DIR/res/corpline-bg.png ] && put $DIR/res/corpline-bg.png /koolshare/res/corpline-bg.png 644
	[ -f $DIR/configs/china-domains.conf ] && put $DIR/configs/china-domains.conf /koolshare/configs/corpline/china-domains.conf 644
	put $DIR/uninstall.sh /koolshare/scripts/uninstall_corpline.sh 755

	# 默认值只在第一次安装时写：之后用户清空的项（例如"走海外 DNS 的域名"）升级时不会被填回去
	if [ -z "$(dbus get corpline_inited)" ]; then
		echo_date "首次安装，写入默认设置…"
		[ -z "$(dbus get corpline_enable)" ] && dbus set corpline_enable=0
		[ -z "$(dbus get corpline_server)" ] && dbus set corpline_server="vpn.xindong.com:8443"
		[ -z "$(dbus get corpline_overseas_domains)" ] && dbus set corpline_overseas_domains="apple.com,mzstatic.com"
		[ -z "$(dbus get corpline_cn_direct)" ] && dbus set corpline_cn_direct=0
		[ -z "$(dbus get corpline_sensitivity)" ] && dbus set corpline_sensitivity=normal
		[ -z "$(dbus get corpline_notify)" ] && dbus set corpline_notify=off
		dbus set corpline_inited=1
	fi

	dbus set corpline_version="$PLVER"
	dbus set softcenter_module_${module}_version="$PLVER"
	dbus set softcenter_module_${module}_install=1
	dbus set softcenter_module_${module}_name="$module"
	dbus set softcenter_module_${module}_title="$TITLE"
	dbus set softcenter_module_${module}_description="$DESCR"

	if [ "$(dbus get corpline_enable)" = "1" ]; then
		# 在这里等它启动完（约几秒）：软件中心随后会删掉 /tmp 下的安装包，但守护程序与运行目录不在那里
		echo_date "插件原本处于启用状态，按原设置重新启动…"
		/bin/sh /koolshare/scripts/corpline_config.sh restart >/dev/null 2>&1
	fi
	echo_date "安装完成。在软件中心打开「$TITLE」填写公司账号后再启用。"
}

platform_test
install_now
rm -rf /tmp/${module}*.tar.gz /tmp/${module} >/dev/null 2>&1
exit 0
