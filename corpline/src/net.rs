//! 网络改动（全部可逆、幂等）。启用时建好一次，之后切换线路只改 ipset `corpline_live` 的内容：
//!
//! - 新连接在建立那一刻决定走哪条线，并记在连接跟踪上（connmark）；之后这条连接一直走原线路。
//!   所以切换线路不会打断正在进行的连接（视频会议、下载）。
//! - 打了标记的包查路由表 88：`default dev corp0`。隧道网卡一消失，这条路由跟着消失，
//!   内核自动落回主路由表直连 —— 即使守护程序卡死，全家也不会断网。

use crate::config::Config;
use crate::sys::{self, run, run_ok, run_stdin};
use std::net::Ipv4Addr;

pub const SET_LIVE: &str = "corpline_live";
pub const SET_BYPASS: &str = "corpline_bypass";
pub const SET_CN: &str = "corpline_cn";
const CH_MARK: &str = "CORPLINE";
const CH_NEW: &str = "CORPLINE_NEW";
const CH_NAT: &str = "CORPLINE_NAT";
const CH_MSS: &str = "CORPLINE_MSS";
const CH_GUARD: &str = "CORPLINE_GUARD";
const CH_V6: &str = "CORPLINE6";

/// 执行 iptables / ip6tables；固件的 iptables 没有 -w，遇到 xtables 锁冲突稍等重试
fn xt(bin: &str, args: &[&str]) -> bool {
    for _ in 0..5 {
        let (ok, out) = run(bin, args);
        if ok {
            return true;
        }
        if !out.contains("temporarily unavailable") && !out.contains("xtables lock") && !out.contains("Resource") {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

fn ipt(table: &str, args: &[&str]) -> bool {
    let mut a = vec!["-t", table];
    a.extend_from_slice(args);
    xt("iptables", &a)
}

fn ensure_chain(bin: &str, table: &str, chain: &str) {
    if !xt(bin, &["-t", table, "-N", chain]) {
        xt(bin, &["-t", table, "-F", chain]);
    }
}

/// 改防火墙的互斥锁：守护程序的定时自检与 N96 钩子（corpline fw）、cleanup 可能同时动同一组链，
/// 交错执行会把规则顺序搅乱（例如"直连目的地 RETURN"排到"打标记"之后）。进程退出时自动释放。
pub fn fw_lock(cfg: &Config) -> Option<std::fs::File> {
    let _ = std::fs::create_dir_all(&cfg.run_dir);
    let f = std::fs::OpenOptions::new().create(true).write(true).open(cfg.run_dir.join("fw.lock")).ok()?;
    use std::os::unix::io::AsRawFd;
    unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) };
    Some(f)
}

/// 在内建链最前面挂上跳转（已存在则不重复）
fn ensure_jump(bin: &str, table: &str, builtin: &str, spec: &[&str]) {
    let mut check = vec!["-t", table, "-C", builtin];
    check.extend_from_slice(spec);
    if !xt(bin, &check) {
        let mut ins = vec!["-t", table, "-I", builtin, "1"];
        ins.extend_from_slice(spec);
        xt(bin, &ins);
    }
}

fn drop_jump(bin: &str, table: &str, builtin: &str, spec: &[&str]) {
    let mut del = vec!["-t", table, "-D", builtin];
    del.extend_from_slice(spec);
    for _ in 0..8 {
        if !xt(bin, &del) {
            break;
        }
    }
}

fn drop_chain(bin: &str, table: &str, chain: &str) {
    xt(bin, &["-t", table, "-F", chain]);
    xt(bin, &["-t", table, "-X", chain]);
}

/// 一次性原子地重写 mangle 里本插件的两条链（iptables-restore --noflush：只换这两条链，不碰别的）
fn restore_mangle(rules: &str) -> bool {
    for _ in 0..5 {
        if run_stdin("iptables-restore", &["--noflush"], rules) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

fn mark(cfg: &Config) -> String {
    format!("0x{:x}/0x{:x}", cfg.mark, cfg.mark)
}

pub fn create_sets() -> bool {
    let a = run_ok("ipset", &["-exist", "create", SET_LIVE, "hash:net", "family", "inet", "hashsize", "64", "maxelem", "16"]);
    let b = run_ok("ipset", &["-exist", "create", SET_BYPASS, "hash:net", "family", "inet", "hashsize", "64", "maxelem", "1024"]);
    let c = run_ok("ipset", &["-exist", "create", SET_CN, "hash:ip", "family", "inet", "timeout", "0", "maxelem", "65536"]);
    a && b && c
}

/// 建立/重建防火墙规则（幂等；固件重建防火墙后再调一次即可恢复）
pub fn apply_fw(cfg: &Config) -> Result<(), String> {
    let _g = fw_lock(cfg);
    if !create_sets() {
        return Err("创建 ipset 失败（固件缺少 ipset 支持？）".into());
    }
    let m = mark(cfg);
    let lan = cfg.lan_if.as_str();
    let tun = cfg.tun_if.as_str();

    // mangle：只在新连接时判断，结果记到 connmark；后续包按 connmark 恢复标记。
    // 两条链用一次 iptables-restore 原子写入，规则顺序不会被并发执行打乱。
    let body = |local: bool| {
        let mut r = format!("*mangle\n:{} - [0:0]\n:{} - [0:0]\n", CH_NEW, CH_MARK);
        // 发往路由器自己的地址（含 PPPoE 的公网 IP：从家里按公网地址访问自己的端口转发）不走隧道
        if local {
            r.push_str(&format!("-A {} -m addrtype --dst-type LOCAL -j RETURN\n", CH_NEW));
        }
        r.push_str(&format!("-A {} -m set --match-set {} dst -j RETURN\n", CH_NEW, SET_BYPASS));
        for mac in &cfg.direct_macs {
            r.push_str(&format!("-A {} -m mac --mac-source {} -j RETURN\n", CH_NEW, mac));
        }
        r.push_str(&format!("-A {} -m set --match-set {} dst -j RETURN\n", CH_NEW, SET_CN));
        r.push_str(&format!("-A {} -m set --match-set {} dst -j CONNMARK --set-xmark {}\n", CH_NEW, SET_LIVE, m));
        r.push_str(&format!("-A {} -m state --state NEW -j {}\n", CH_MARK, CH_NEW));
        r.push_str(&format!("-A {} -m connmark --mark {} -j MARK --set-xmark {}\n", CH_MARK, m, m));
        r.push_str("COMMIT\n");
        r
    };
    // 固件若没有 addrtype 模块，退回不带这条（公网 IP 已由 bypass 集合里的直连网段大体覆盖）
    if !restore_mangle(&body(true)) && !restore_mangle(&body(false)) {
        return Err("写入 mangle 规则失败".into());
    }
    ensure_jump("iptables", "mangle", "PREROUTING", &["-i", lan, "-j", CH_MARK]);

    // 走隧道的 TCP 按隧道 MTU 调整 MSS，避免大包卡住
    ensure_chain("iptables", "mangle", CH_MSS);
    ipt("mangle", &["-A", CH_MSS, "-o", tun, "-p", "tcp", "--tcp-flags", "SYN,RST", "SYN", "-j", "TCPMSS", "--clamp-mss-to-pmtu"]);
    ensure_jump("iptables", "mangle", "FORWARD", &["-j", CH_MSS]);

    // nat：家里设备经隧道出去时换成隧道地址
    ensure_chain("iptables", "nat", CH_NAT);
    ipt("nat", &["-A", CH_NAT, "-o", tun, "-j", "MASQUERADE"]);
    ensure_jump("iptables", "nat", "POSTROUTING", &["-j", CH_NAT]);

    // filter：不允许从公司网络主动连进家里或路由器（只加拒绝，不加放行，不绕过家长控制等固件规则）
    ensure_chain("iptables", "filter", CH_GUARD);
    ipt("filter", &["-A", CH_GUARD, "-i", tun, "-m", "state", "--state", "NEW,INVALID", "-j", "DROP"]);
    ensure_jump("iptables", "filter", "INPUT", &["-j", CH_GUARD]);
    ensure_jump("iptables", "filter", "FORWARD", &["-j", CH_GUARD]);

    // IPv6：V1 不经隧道转发 IPv6，家里设备的 IPv6 出网被拒后立刻改用 IPv4（DNS 也不回 AAAA）
    if cfg.ipv6_guard && run_ok("ip6tables", &["-t", "filter", "-L", "FORWARD", "-n"]) {
        ensure_chain("ip6tables", "filter", CH_V6);
        run_ok("ip6tables", &["-t", "filter", "-A", CH_V6, "-i", lan, "!", "-o", lan, "-j", "REJECT", "--reject-with", "icmp6-adm-prohibited"]);
        ensure_jump("ip6tables", "filter", "FORWARD", &["-j", CH_V6]);
    }
    Ok(())
}

/// 防火墙规则是否还在（固件重建防火墙会清掉它们；只重建 filter 或 IPv6 的情况也要发现）
pub fn fw_present(cfg: &Config) -> bool {
    let v6 = !cfg.ipv6_guard
        || !run_ok("ip6tables", &["-t", "filter", "-L", "FORWARD", "-n"])
        || run_ok("ip6tables", &["-t", "filter", "-C", "FORWARD", "-j", CH_V6]);
    run_ok("iptables", &["-t", "mangle", "-C", "PREROUTING", "-i", &cfg.lan_if, "-j", CH_MARK])
        && run_ok("iptables", &["-t", "mangle", "-C", "FORWARD", "-j", CH_MSS])
        && run_ok("iptables", &["-t", "nat", "-C", "POSTROUTING", "-j", CH_NAT])
        && run_ok("iptables", &["-t", "filter", "-C", "INPUT", "-j", CH_GUARD])
        && run_ok("iptables", &["-t", "filter", "-C", "FORWARD", "-j", CH_GUARD])
        && v6
}

/// 两条 ip rule 是否都在（丢了的话页面显示公司专线、流量却在直连）
pub fn rules_present(cfg: &Config) -> bool {
    let (_, out) = run("ip", &["rule", "show"]);
    let has = |pref: u32| out.lines().any(|l| l.starts_with(&format!("{}:", pref)) && l.contains(&format!("lookup {}", cfg.table)));
    has(cfg.rule_pref) && has(cfg.rule_pref - 1)
}

fn rule_args(cfg: &Config, kind: &str) -> Vec<String> {
    let t = cfg.table.to_string();
    match kind {
        "mark" => vec!["fwmark".into(), mark(cfg), "lookup".into(), t, "pref".into(), cfg.rule_pref.to_string()],
        _ => vec!["oif".into(), cfg.tun_if.clone(), "lookup".into(), t, "pref".into(), (cfg.rule_pref - 1).to_string()],
    }
}

fn ip_rule(op: &str, args: &[String]) -> bool {
    let mut a = vec!["rule", op];
    a.extend(args.iter().map(|s| s.as_str()));
    run_ok("ip", &a)
}

pub fn apply_rules(cfg: &Config) {
    for kind in ["mark", "oif"] {
        let args = rule_args(cfg, kind);
        // 先删再加，保证只有一条
        while ip_rule("del", &args) {}
        ip_rule("add", &args);
    }
}

pub fn set_rp_filter(cfg: &Config) {
    let bak = cfg.run_dir.join("rp_filter.bak");
    if !bak.exists() {
        let mut s = String::new();
        for dev in ["all", cfg.lan_if.as_str()] {
            if let Ok(v) = std::fs::read_to_string(format!("/proc/sys/net/ipv4/conf/{}/rp_filter", dev)) {
                s.push_str(&format!("{}={}\n", dev, v.trim()));
            }
        }
        let _ = std::fs::write(&bak, s);
    }
    ensure_rp_filter(cfg);
}

/// 隧道回包从 corp0 进来，严格反向路径过滤会把它当伪造包丢掉（旧版真机实测的转发不通根因）。
/// 固件重启 WAN / 防火墙时可能改回 1，定时检查；返回是否改动过。
pub fn ensure_rp_filter(cfg: &Config) -> bool {
    let mut changed = false;
    for dev in ["all", cfg.lan_if.as_str()] {
        let p = format!("/proc/sys/net/ipv4/conf/{}/rp_filter", dev);
        if std::fs::read_to_string(&p).map(|v| v.trim() != "0").unwrap_or(false) {
            let _ = std::fs::write(&p, "0");
            changed = true;
        }
    }
    changed
}

fn restore_rp_filter(cfg: &Config) {
    let bak = cfg.run_dir.join("rp_filter.bak");
    if let Ok(s) = std::fs::read_to_string(&bak) {
        for l in s.lines() {
            if let Some((dev, v)) = l.split_once('=') {
                let _ = std::fs::write(format!("/proc/sys/net/ipv4/conf/{}/rp_filter", dev), v);
            }
        }
        let _ = std::fs::remove_file(bak);
    }
}

/// 总阀门：打开 = 新连接走公司线路；关闭 = 新连接直连
pub fn set_live(on: bool) -> bool {
    let mut s = format!("flush {}\n", SET_LIVE);
    if on {
        s.push_str(&format!("add {} 0.0.0.0/1\nadd {} 128.0.0.0/1\n", SET_LIVE, SET_LIVE));
    }
    run_stdin("ipset", &["restore", "-!"], &s)
}

/// 不走隧道的目的地：本机网段、组播、链路本地，以及 VPN 服务器本身（防环路，也让家里电脑自己的 VPN 直连）
pub fn bypass_nets(cfg: &Config, server_ips: &[Ipv4Addr]) -> Vec<String> {
    let mut nets: Vec<String> = vec![
        "0.0.0.0/8".into(),
        "127.0.0.0/8".into(),
        "169.254.0.0/16".into(),
        "224.0.0.0/4".into(),
        "255.255.255.255/32".into(),
    ];
    // 主路由表里直连的网段（家里 LAN、光猫管理网段等），隧道网卡除外
    let (_, out) = run("ip", &["-4", "route", "show", "table", "main", "scope", "link"]);
    for l in out.lines() {
        let mut it = l.split_whitespace();
        let Some(net) = it.next() else { continue };
        if l.contains(&format!("dev {}", cfg.tun_if)) || net == "default" {
            continue;
        }
        if net.parse::<Ipv4Addr>().is_ok() || net.contains('/') {
            nets.push(net.to_string());
        }
    }
    for ip in server_ips {
        nets.push(format!("{}/32", ip));
    }
    nets.extend(cfg.bypass_nets.iter().cloned());
    nets
}

pub fn set_bypass(nets: &[String]) -> bool {
    let tmp = format!("{}_tmp", SET_BYPASS);
    let mut s = format!("create {} hash:net family inet hashsize 64 maxelem 1024 -exist\nflush {}\n", tmp, tmp);
    for n in nets {
        s.push_str(&format!("add {} {}\n", tmp, n));
    }
    s.push_str(&format!("swap {} {}\ndestroy {}\n", tmp, SET_BYPASS, tmp));
    run_stdin("ipset", &["restore", "-!"], &s)
}

/// 主路由表是否有默认路由（宽带是否在线）
pub fn has_default_route() -> bool {
    run("ip", &["-4", "route", "show", "table", "main", "default"]).1.contains("default")
}

pub fn flush_cn() {
    run_ok("ipset", &["flush", SET_CN]);
}

/// 启用时的一次性准备
pub fn setup(cfg: &Config) -> Result<(), String> {
    set_live(false);
    apply_fw(cfg)?;
    set_live(false);
    set_bypass(&bypass_nets(cfg, &[]));
    apply_rules(cfg);
    set_rp_filter(cfg);
    if !cfg.cn_direct {
        flush_cn();
    }
    Ok(())
}

/// 撤掉本插件的一切网络改动（守护程序不在也能执行）
pub fn cleanup(cfg: &Config) {
    let _g = fw_lock(cfg);
    let lan = cfg.lan_if.as_str();
    set_live(false);
    drop_jump("iptables", "mangle", "PREROUTING", &["-i", lan, "-j", CH_MARK]);
    drop_jump("iptables", "mangle", "FORWARD", &["-j", CH_MSS]);
    drop_jump("iptables", "nat", "POSTROUTING", &["-j", CH_NAT]);
    drop_jump("iptables", "filter", "INPUT", &["-j", CH_GUARD]);
    drop_jump("iptables", "filter", "FORWARD", &["-j", CH_GUARD]);
    drop_chain("iptables", "mangle", CH_MARK);
    drop_chain("iptables", "mangle", CH_NEW);
    drop_chain("iptables", "mangle", CH_MSS);
    drop_chain("iptables", "nat", CH_NAT);
    drop_chain("iptables", "filter", CH_GUARD);
    if xt("ip6tables", &["-t", "filter", "-L", "FORWARD", "-n"]) {
        drop_jump("ip6tables", "filter", "FORWARD", &["-j", CH_V6]);
        drop_chain("ip6tables", "filter", CH_V6);
    }
    for kind in ["mark", "oif"] {
        let args = rule_args(cfg, kind);
        while ip_rule("del", &args) {}
    }
    run_ok("ip", &["route", "flush", "table", &cfg.table.to_string()]);
    for s in [SET_LIVE, SET_BYPASS, SET_CN, "corpline_bypass_tmp"] {
        run_ok("ipset", &["destroy", s]);
    }
    restore_rp_filter(cfg);
}

/// openconnect 连接脚本里配置隧道网卡
pub fn tun_up(dev: &str, addr: &str, mtu: &str, table: u32) -> bool {
    let mtu = if mtu.parse::<u32>().map(|m| (576..=1500).contains(&m)).unwrap_or(false) { mtu } else { "1300" };
    let t = table.to_string();
    let a = run_ok("ip", &["link", "set", "dev", dev, "up", "mtu", mtu]);
    let b = run_ok("ip", &["addr", "replace", &format!("{}/32", addr), "dev", dev]);
    let _ = std::fs::write(format!("/proc/sys/net/ipv4/conf/{}/rp_filter", dev), "0");
    let c = run_ok("ip", &["route", "replace", "default", "dev", dev, "table", &t]);
    a && b && c
}

pub fn dnsmasq_conf(cfg: &Config) -> String {
    format!(
        "# 公司专线（corpline）自动生成，停用插件时自动删除。\n\
         # 先问公司专线的 DNS 转发；它不在时（插件停止/崩溃）按顺序改问运营商 DNS。\n\
         server={}#{}\n\
         strict-order\n",
        cfg.dns_listen.ip(),
        cfg.dns_listen.port()
    )
}

/// 写入 dnsmasq 配置并重启 dnsmasq（只在启用/停用时各一次）
pub fn dnsmasq_install(cfg: &Config) -> bool {
    if !cfg.manage_dnsmasq {
        return true;
    }
    let want = dnsmasq_conf(cfg);
    if std::fs::read_to_string(&cfg.dnsmasq_conf).map(|s| s == want).unwrap_or(false) {
        return true;
    }
    if let Some(d) = cfg.dnsmasq_conf.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if sys::write_atomic(&cfg.dnsmasq_conf, want.as_bytes()).is_err() {
        return false;
    }
    sys::sh(&cfg.dnsmasq_restart).0
}

pub fn dnsmasq_remove(cfg: &Config) {
    if !cfg.manage_dnsmasq {
        return;
    }
    if cfg.dnsmasq_conf.exists() {
        let _ = std::fs::remove_file(&cfg.dnsmasq_conf);
        sys::sh(&cfg.dnsmasq_restart);
    }
}

/// 清掉 dnsmasq 缓存（不重启）：直连期间运营商给的解析可能被污染，切回公司线路后不再使用
pub fn dnsmasq_flush(cfg: &Config) {
    if let Ok(p) = std::fs::read_to_string(&cfg.dnsmasq_pidfile) {
        if let Ok(pid) = p.trim().parse::<i32>() {
            // pid 文件可能是旧的、已被别的进程复用：确认是 dnsmasq 再发信号
            let comm = std::fs::read_to_string(format!("/proc/{}/comm", pid)).unwrap_or_default();
            if comm.trim() == "dnsmasq" && sys::kill(pid, libc::SIGHUP) {
                return;
            }
        }
    }
    run_ok("killall", &["-HUP", "dnsmasq"]);
}

/// 运营商 DNS：华硕固件把它写在 /tmp/resolv.dnsmasq（server=x.x.x.x），退而读 /etc/resolv.conf
pub fn isp_dns(cfg: &Config) -> Vec<Ipv4Addr> {
    if !cfg.isp_dns.is_empty() {
        return cfg.isp_dns.clone();
    }
    let mut v = Vec::new();
    if let Ok(s) = std::fs::read_to_string(&cfg.isp_dns_file) {
        for l in s.lines() {
            let l = l.trim();
            let x = l.strip_prefix("server=").or_else(|| l.strip_prefix("nameserver "));
            if let Some(ip) = x.and_then(|x| x.split('#').next()).and_then(sys::ip4) {
                if !ip.is_loopback() && !v.contains(&ip) {
                    v.push(ip);
                }
            }
        }
    }
    v
}
