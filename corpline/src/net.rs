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

// 安全防护（filter 表）：外来新连接进 CORPLINE_SEC，家里设备的新外连进 CORPLINE_SECOUT
pub const SET_BLK: [&str; 3] = ["corpline_blk_scan", "corpline_blk_brute", "corpline_blk_flood"];
pub const SET_AL: [&str; 3] = ["corpline_al_bad", "corpline_al_scan", "corpline_al_flood"];
pub const SET_BAD: &str = "corpline_bad";
pub const SET_OK: &str = "corpline_secok";
pub const SET_HIT: &str = "corpline_sechit";
const SET_PRIV: &str = "corpline_secpriv";
const SET_ADM: &str = "corpline_secadm";
const SET_RISK: &str = "corpline_secrisk";
// 家里设备最近 1 分钟连过的（外网地址, 高危端口）：只数"新目标"，反复连同一台服务器（git、SSH）不算扫描
const SET_ODST: &str = "corpline_secodst";
const CH_SEC: &str = "CORPLINE_SEC";
const CH_SECOUT: &str = "CORPLINE_SECOUT";
const CH_SECDROP: &str = "CORPLINE_SECDROP";
const CH_ODST: &str = "CORPLINE_O_NEWDST";
const CH_BLK: [&str; 3] = ["CORPLINE_B_SCAN", "CORPLINE_B_BRUTE", "CORPLINE_B_FLOOD"];
const CH_AL: [&str; 3] = ["CORPLINE_A_BAD", "CORPLINE_A_SCAN", "CORPLINE_A_FLOOD"];

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
        // 设备走公司线路、个别本机端口直连（例如 NAS 上 BT 的收发端口），TCP 和 UDP 都算
        for (mac, a, b) in &cfg.direct_ports {
            for proto in ["tcp", "udp"] {
                r.push_str(&format!("-A {} -m mac --mac-source {} -p {} --sport {}:{} -j RETURN\n", CH_NEW, mac, proto, a, b));
            }
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
    // 安全防护出问题不影响公司线路：记下原因，线路照常
    if let Err(e) = apply_sec(cfg) {
        crate::report::log(&format!("安全防护没有生效：{}", e));
    }
    Ok(())
}

/// 外网网卡：配置里写了就用，否则看主路由表默认路由走哪张网卡（PPPoE 拨号是 ppp0）
pub fn wan_if(cfg: &Config) -> Option<String> {
    if !cfg.sec.wan_if.is_empty() {
        return Some(cfg.sec.wan_if.clone());
    }
    let (_, out) = run("ip", &["-4", "route", "show", "table", "main", "default"]);
    let mut it = out.split_whitespace();
    while let Some(w) = it.next() {
        if w == "dev" {
            return it.next().filter(|d| *d != cfg.tun_if).map(String::from);
        }
    }
    None
}

/// 建集合；固件的 ipset 不支持 counters 时退回不带计数
fn create_set(name: &str, spec: &[&str]) -> bool {
    let mut a = vec!["-exist", "create", name];
    a.extend_from_slice(spec);
    if run_ok("ipset", &a) {
        return true;
    }
    let plain: Vec<&str> = a.iter().copied().filter(|x| *x != "counters").collect();
    plain.len() != a.len() && run_ok("ipset", &plain)
}

/// 把内容整体换成 items（先写临时集合再 swap，不出现空窗）
fn replace_set(name: &str, kind: &str, opts: &str, items: &[String]) -> bool {
    let tmp = format!("{}_t", name);
    let mut s = format!("create {} {} {} -exist\nflush {}\n", tmp, kind, opts, tmp);
    for i in items {
        s.push_str(&format!("add {} {}\n", tmp, i));
    }
    s.push_str(&format!("swap {} {}\ndestroy {}\n", tmp, name, tmp));
    run_stdin("ipset", &["restore", "-!"], &s)
}

fn sec_save_file(cfg: &Config) -> std::path::PathBuf {
    cfg.run_dir.join("sec-blocks.save")
}

fn sec_create_sets(cfg: &Config) -> bool {
    let t = cfg.sec.block_secs.to_string();
    let mut ok = true;
    for s in SET_BLK {
        ok &= create_set(s, &["hash:ip", "family", "inet", "timeout", &t, "maxelem", "65536", "counters"]);
    }
    for s in SET_AL {
        ok &= create_set(s, &["hash:ip", "family", "inet", "timeout", "3600", "maxelem", "1024"]);
    }
    ok &= create_set(SET_HIT, &["hash:ip,port,ip", "family", "inet", "timeout", "3600", "maxelem", "16384"]);
    ok &= create_set(SET_BAD, &["hash:net", "family", "inet", "maxelem", "131072"]);
    ok &= create_set(SET_OK, &["hash:net", "family", "inet", "maxelem", "1024"]);
    ok &= create_set(SET_PRIV, &["hash:net", "family", "inet", "maxelem", "256"]);
    ok &= create_set(SET_ADM, &["bitmap:port", "range", "0-65535"]);
    ok &= create_set(SET_RISK, &["bitmap:port", "range", "0-65535"]);
    ok &= create_set(SET_ODST, &["hash:ip,port,ip", "family", "inet", "timeout", "60", "maxelem", "65536"]);
    if !ok {
        return false;
    }
    set_allow(&cfg.sec.allow);
    replace_set(SET_PRIV, "hash:net", "family inet maxelem 256", &cfg.sec.private_nets);
    let p = |v: &[u16]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    replace_set(SET_ADM, "bitmap:port", "range 0-65535", &p(&cfg.sec.admin_ports));
    replace_set(SET_RISK, "bitmap:port", "range 0-65535", &p(&cfg.sec.risky_ports));
    // 守护程序重启前保存的封锁名单（剩余时间照旧），重启路由器后自然清空
    if let Ok(saved) = std::fs::read_to_string(sec_save_file(cfg)) {
        let adds: String = saved.lines().filter(|l| l.starts_with("add ")).map(|l| format!("{}\n", l)).collect();
        run_stdin("ipset", &["restore", "-!"], &adds);
        let _ = std::fs::remove_file(sec_save_file(cfg));
    }
    true
}

pub fn set_allow(nets: &[String]) -> bool {
    replace_set(SET_OK, "hash:net", "family inet maxelem 1024", nets)
}

pub fn set_bad(nets: &[String]) -> bool {
    replace_set(SET_BAD, "hash:net", "family inet maxelem 131072", nets)
}

/// 安全防护的 filter 规则（一次 iptables-restore 原子写入）。只加"丢弃"和"记录"，从不放行，
/// 所以不会绕过固件自己的防火墙；外网扫描/猜密码/狂连只看 TCP 握手包，游戏和 BT 的 UDP 不计数。
fn sec_rules(cfg: &Config) -> String {
    let c = &cfg.sec;
    let mut r = String::from("*filter\n");
    for ch in [CH_SEC, CH_SECOUT, CH_SECDROP, CH_ODST].iter().chain(CH_BLK.iter()).chain(CH_AL.iter()) {
        r.push_str(&format!(":{} - [0:0]\n", ch));
    }
    let a = |r: &mut String, ch: &str, rule: String| r.push_str(&format!("-A {} {}\n", ch, rule));
    let hl = |name: &str, per_min: u32, mode: &str| {
        format!(
            "-m hashlimit --hashlimit-above {n}/min --hashlimit-burst {n} --hashlimit-mode {mode} --hashlimit-name {name} --hashlimit-htable-expire 120000 --hashlimit-htable-max 16384",
            n = per_min.min(10000),
            mode = mode,
            name = name
        )
    };
    // 被封的地址：记下它碰了哪个端口，然后丢弃
    a(&mut r, CH_SECDROP, format!("-j SET --add-set {} src,dst,dst", SET_HIT));
    a(&mut r, CH_SECDROP, "-j DROP".into());
    for (ch, set) in CH_BLK.iter().zip(SET_BLK) {
        a(&mut r, ch, format!("-j SET --add-set {} src", set));
        a(&mut r, ch, format!("-j {}", CH_SECDROP));
    }
    for (ch, set) in CH_AL.iter().zip(SET_AL) {
        a(&mut r, ch, format!("-j SET --add-set {} src", set));
        a(&mut r, ch, format!("-j SET --add-set {} src,dst,dst", SET_HIT));
    }

    // 外来
    a(&mut r, CH_SEC, format!("-m set --match-set {} src -j RETURN", SET_OK));
    for set in SET_BLK {
        a(&mut r, CH_SEC, format!("-m set --match-set {} src -j {}", set, CH_SECDROP));
    }
    a(&mut r, CH_SEC, format!("-m set --match-set {} src -j DROP", SET_BAD));
    // 连路由器自己（外网管理已关，主动连进来的都是扫描）
    a(&mut r, CH_SEC, format!("-p tcp --syn -m addrtype --dst-type LOCAL {} -j {}", hl("corpline_scan", c.scan_per_min, "srcip"), CH_BLK[0]));
    a(&mut r, CH_SEC, "-m addrtype --dst-type LOCAL -j RETURN".into());
    // 经端口转发连家里设备：管理端口按猜密码算，其余按狂连算
    a(&mut r, CH_SEC, format!("-p tcp --syn -m set --match-set {} dst {} -j {}", SET_ADM, hl("corpline_brute", c.brute_per_min, "srcip"), CH_BLK[1]));
    a(&mut r, CH_SEC, format!("-p tcp --syn {} -j {}", hl("corpline_flood", c.flood_per_min, "srcip"), CH_BLK[2]));

    // 家里设备往外：只记录、提醒，不拦
    a(&mut r, CH_SECOUT, format!("-m set --match-set {} dst -j RETURN", SET_PRIV));
    // 设备设了「直连端口」的程序（NAS 上的 BT、Plex）本来就和全世界的陌生地址收发，不算异常外连
    for (mac, lo, hi) in &cfg.direct_ports {
        for proto in ["tcp", "udp"] {
            a(&mut r, CH_SECOUT, format!("-m mac --mac-source {} -p {} --sport {}:{} -j RETURN", mac, proto, lo, hi));
        }
    }
    a(&mut r, CH_SECOUT, format!("-m set --match-set {} dst -j RETURN", SET_OK));
    a(&mut r, CH_SECOUT, format!("-m set --match-set {} dst -j {}", SET_BAD, CH_AL[0]));
    // 扫描外网：1 分钟内连了很多个不同的（地址, 高危端口）；同一目标重复连接不计数
    a(&mut r, CH_SECOUT, format!("-p tcp --syn -m set --match-set {} dst -m set ! --match-set {} src,dst,dst -j {}", SET_RISK, SET_ODST, CH_ODST));
    a(&mut r, CH_ODST, format!("-j SET --add-set {} src,dst,dst", SET_ODST));
    a(&mut r, CH_ODST, format!("{} -j {}", hl("corpline_oscan", c.out_scan_per_min, "srcip"), CH_AL[1]));
    a(&mut r, CH_SECOUT, format!("-p tcp --syn {} -j {}", hl("corpline_oflood", c.out_flood_per_min, "srcip,dstip"), CH_AL[2]));
    a(&mut r, CH_SECOUT, format!("-p udp -m multiport ! --dports 53,123,443,853 {} -j {}", hl("corpline_oudp", c.out_flood_per_min, "srcip,dstip"), CH_AL[2]));
    r.push_str("COMMIT\n");
    r
}

fn sec_jumps(cfg: &Config, wan: &str) -> Vec<(&'static str, Vec<String>)> {
    let v = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    vec![
        ("INPUT", v(&["-i", wan, "-m", "state", "--state", "NEW", "-j", CH_SEC])),
        ("FORWARD", v(&["-i", wan, "-m", "state", "--state", "NEW", "-j", CH_SEC])),
        ("FORWARD", v(&["-i", &cfg.lan_if, "!", "-o", &cfg.lan_if, "-m", "state", "--state", "NEW", "-j", CH_SECOUT])),
    ]
}

/// 外网网卡变了（例如从 ppp0 换成 eth0）时，旧的跳转要先摘掉
fn sec_drop_jumps() {
    for builtin in ["INPUT", "FORWARD"] {
        let (_, out) = run("iptables", &["-t", "filter", "-S", builtin]);
        for l in out.lines() {
            if l.ends_with(&format!("-j {}", CH_SEC)) || l.ends_with(&format!("-j {}", CH_SECOUT)) {
                let spec: Vec<&str> = l.split_whitespace().skip(2).collect();
                drop_jump("iptables", "filter", builtin, &spec);
            }
        }
    }
}

/// 上次建安全防护规则失败的原因（网页上显示）
pub static SEC_ERR: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn apply_sec(cfg: &Config) -> Result<(), String> {
    let r = apply_sec_inner(cfg);
    *SEC_ERR.lock().unwrap() = r.as_ref().err().cloned();
    r
}

fn apply_sec_inner(cfg: &Config) -> Result<(), String> {
    if !cfg.sec.enable {
        return Ok(());
    }
    let wan = wan_if(cfg).ok_or("找不到外网网卡（宽带没连上？）")?;
    if !sec_create_sets(cfg) {
        return Err("创建安全防护用的 ipset 失败".into());
    }
    if !run_stdin("iptables-restore", &["--noflush"], &sec_rules(cfg)) {
        return Err("写入安全防护规则失败（固件缺少 hashlimit 模块？）".into());
    }
    let have: Vec<String> = sec_jumps(cfg, &wan).iter().map(|(b, s)| format!("-A {} {}", b, s.join(" "))).collect();
    let (_, cur) = run("iptables", &["-t", "filter", "-S"]);
    let stale = cur.lines().any(|l| (l.ends_with(&format!("-j {}", CH_SEC)) || l.ends_with(&format!("-j {}", CH_SECOUT))) && !have.iter().any(|h| h == l));
    if stale {
        sec_drop_jumps();
    }
    for (b, spec) in sec_jumps(cfg, &wan) {
        let spec: Vec<&str> = spec.iter().map(String::as_str).collect();
        ensure_jump("iptables", "filter", b, &spec);
    }
    Ok(())
}

pub fn sec_present(cfg: &Config) -> bool {
    if !cfg.sec.enable {
        return true;
    }
    let Some(wan) = wan_if(cfg) else { return true };
    sec_jumps(cfg, &wan).iter().all(|(b, spec)| {
        let mut a = vec!["-t", "filter", "-C", b];
        a.extend(spec.iter().map(String::as_str));
        run_ok("iptables", &a)
    })
}

fn sec_cleanup(cfg: &Config) {
    sec_drop_jumps();
    for ch in [CH_SEC, CH_SECOUT, CH_ODST].iter().chain(CH_BLK.iter()).chain(CH_AL.iter()).chain([CH_SECDROP].iter()) {
        xt("iptables", &["-t", "filter", "-F", ch]);
    }
    for ch in [CH_SEC, CH_SECOUT, CH_ODST].iter().chain(CH_BLK.iter()).chain(CH_AL.iter()).chain([CH_SECDROP].iter()) {
        xt("iptables", &["-t", "filter", "-X", ch]);
    }
    // 封锁名单留给下次启动（只在内存盘，重启路由器即清空）
    let mut save = String::new();
    for s in SET_BLK {
        let (ok, out) = run("ipset", &["save", s]);
        if ok {
            save.push_str(&out);
        }
    }
    if save.contains("\nadd ") || save.starts_with("add ") {
        let _ = std::fs::create_dir_all(&cfg.run_dir);
        let _ = sys::write_atomic(&sec_save_file(cfg), save.as_bytes());
    }
    for s in SET_BLK.iter().chain(SET_AL.iter()).chain([SET_HIT, SET_BAD, SET_OK, SET_PRIV, SET_ADM, SET_RISK, SET_ODST].iter()) {
        run_ok("ipset", &["destroy", s]);
        run_ok("ipset", &["destroy", &format!("{}_t", s)]);
    }
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
        && sec_present(cfg)
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
    sec_cleanup(cfg);
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
