//! 安全防护：计数与拦截都在内核里（见 net.rs 的 CORPLINE_SEC / CORPLINE_SECOUT），
//! 这里每隔一会儿读一次 ipset，把"新封锁的外网地址""家里设备的可疑外连"整理成事件，
//! 写到 /tmp/upload/corpline_security.json 给网页和 Mac 上的提醒任务读。

use crate::config::{self, Config};
use crate::net::{self, SET_AL, SET_BAD, SET_BLK, SET_HIT};
use crate::report::{self, Json};
use crate::sys;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_EVENTS: usize = 300;
const KINDS: [&str; 6] = ["scan", "brute", "flood", "out_bad", "out_scan", "out_flood"];

#[derive(Clone, Debug)]
pub struct SecEv {
    pub id: u64,
    pub t: u64,
    pub kind: String,
    /// info：一般扫描（已自动封锁，汇总提醒即可）；warn：狂连；alert：猜密码、家里设备异常（要马上告诉人）
    pub level: String,
    pub ip: String,
    pub dev: String,
    pub text: String,
}

/// ipset save 的一行：add <集合> <成员> [timeout N] [packets N bytes N]
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub key: String,
    pub timeout: u64,
    pub packets: Option<u64>,
}

pub fn parse_save(text: &str, set: &str) -> Vec<Member> {
    let mut v = Vec::new();
    for l in text.lines() {
        let mut it = l.split_whitespace();
        if it.next() != Some("add") || it.next() != Some(set) {
            continue;
        }
        let Some(key) = it.next() else { continue };
        let mut m = Member { key: key.to_string(), timeout: 0, packets: None };
        while let Some(k) = it.next() {
            let val = it.next().and_then(|x| x.parse().ok());
            match k {
                "timeout" => m.timeout = val.unwrap_or(0),
                "packets" => m.packets = val,
                _ => {}
            }
        }
        v.push(m);
    }
    v
}

/// sechit 的成员 "源,协议:端口,目的"
pub fn parse_hit(key: &str) -> Option<(Ipv4Addr, String, u16, Ipv4Addr)> {
    let mut it = key.split(',');
    let src = it.next()?.parse().ok()?;
    let (proto, port) = it.next()?.split_once(':')?;
    let dst = it.next()?.parse().ok()?;
    Some((src, proto.to_string(), port.parse().ok()?, dst))
}

fn net_of(s: &str) -> Option<(u32, u8)> {
    let (ip, len) = s.split_once('/').unwrap_or((s, "32"));
    let ip: Ipv4Addr = ip.parse().ok()?;
    let len: u8 = len.parse().ok().filter(|n| *n <= 32)?;
    Some((u32::from(ip), len))
}

fn overlaps(a: (u32, u8), b: (u32, u8)) -> bool {
    let l = a.1.min(b.1);
    let mask = if l == 0 { 0 } else { u32::MAX << (32 - l) };
    a.0 & mask == b.0 & mask
}

/// 恶意地址名单：去掉注释、私有/保留网段和白名单（名单里常带"bogon"网段，误伤家里和公司网络）
pub fn filter_bad(text: &str, keep_out: &[String]) -> Vec<String> {
    let outs: Vec<(u32, u8)> = keep_out.iter().filter_map(|s| net_of(s)).collect();
    let mut v = Vec::new();
    for l in text.lines() {
        let l = l.split('#').next().unwrap_or("").trim();
        if l.is_empty() || !config::is_net(l) {
            continue;
        }
        let Some(n) = net_of(l) else { continue };
        if n.1 < 8 || outs.iter().any(|o| overlaps(*o, n)) {
            continue;
        }
        v.push(l.to_string());
    }
    v
}

/// 端口列表写成"22、23、8080 等 15 个"
fn ports_text(ports: &BTreeSet<u16>) -> String {
    let v: Vec<String> = ports.iter().take(6).map(|p| p.to_string()).collect();
    if ports.len() > 6 {
        format!("{} 等 {} 个", v.join("、"), ports.len())
    } else {
        v.join("、")
    }
}

/// 局域网设备名：网页设备名单里的名字 > DHCP 主机名 > IP
pub struct Names {
    by_mac: HashMap<String, String>,
    ip_mac: HashMap<Ipv4Addr, String>,
    ip_host: HashMap<Ipv4Addr, String>,
}

impl Names {
    pub fn load(cfg: &Config) -> Names {
        let mut n = Names { by_mac: HashMap::new(), ip_mac: HashMap::new(), ip_host: HashMap::new() };
        if let Ok(s) = std::fs::read_to_string(&cfg.sec.names_file) {
            for l in s.lines() {
                if let Some((mac, name)) = l.split_once('\t') {
                    if !name.trim().is_empty() {
                        n.by_mac.insert(mac.to_ascii_uppercase(), name.trim().to_string());
                    }
                }
            }
        }
        if let Ok(s) = std::fs::read_to_string(&cfg.sec.leases_file) {
            for l in s.lines() {
                let f: Vec<&str> = l.split_whitespace().collect();
                if f.len() >= 4 {
                    if let Ok(ip) = f[2].parse() {
                        n.ip_mac.insert(ip, f[1].to_ascii_uppercase());
                        if f[3] != "*" {
                            n.ip_host.insert(ip, f[3].to_string());
                        }
                    }
                }
            }
        }
        if let Ok(s) = std::fs::read_to_string("/proc/net/arp") {
            for l in s.lines().skip(1) {
                let f: Vec<&str> = l.split_whitespace().collect();
                if f.len() >= 4 && f[3] != "00:00:00:00:00:00" {
                    if let Ok(ip) = f[0].parse() {
                        n.ip_mac.insert(ip, f[3].to_ascii_uppercase());
                    }
                }
            }
        }
        n
    }

    pub fn name(&self, ip: Ipv4Addr) -> Option<String> {
        self.ip_mac.get(&ip).and_then(|m| self.by_mac.get(m)).or_else(|| self.ip_host.get(&ip)).cloned()
    }

    /// "「NAS」（192.168.50.168）"；不认识写"设备 192.168.50.168 "（带空格，直接接在中文前后）
    pub fn label(&self, ip: Ipv4Addr) -> String {
        match self.name(ip) {
            Some(n) => format!("「{}」（{}）", n, ip),
            None => format!("设备 {} ", ip),
        }
    }
}

pub struct Guard {
    conf: PathBuf,
    events_file: PathBuf,
    out: PathBuf,
    events: VecDeque<SecEv>,
    next_id: u64,
    /// 已经出过事件的成员：(集合序号, IP) → 首次发现时间
    known: HashMap<(usize, Ipv4Addr), u64>,
    first: bool,
    last_poll: u64,
    bad_mtime: Option<SystemTime>,
    bad_count: usize,
    conf_mtime: Option<SystemTime>,
    allow: Vec<String>,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn save_all(sets: &[&str]) -> String {
    let mut s = String::new();
    for set in sets {
        let (ok, out) = sys::run("ipset", &["save", set]);
        if ok {
            s.push_str(&out);
        }
    }
    s
}

/// 规则计数：iptables -nvxL <链>，返回目标为 target 的那条规则的包数
fn rule_packets(chain: &str, needle: &str) -> u64 {
    let (_, out) = sys::run("iptables", &["-t", "filter", "-nvxL", chain]);
    out.lines().filter(|l| l.contains(needle)).filter_map(|l| l.split_whitespace().next()?.parse::<u64>().ok()).sum()
}

impl Guard {
    pub fn new(cfg: &Config, conf: &Path) -> Guard {
        let mut g = Guard {
            conf: conf.to_path_buf(),
            events_file: cfg.run_dir.join("sec-events.txt"),
            out: cfg.status_dir.join("corpline_security.json"),
            events: VecDeque::new(),
            next_id: 1,
            known: HashMap::new(),
            first: true,
            last_poll: 0,
            bad_mtime: None,
            bad_count: 0,
            conf_mtime: mtime(conf),
            allow: cfg.sec.allow.clone(),
        };
        // 守护程序重启后接着用之前的事件（编号连续，Mac 上的提醒任务靠编号判断新旧）
        if let Ok(s) = std::fs::read_to_string(&g.events_file) {
            for l in s.lines() {
                let f: Vec<&str> = l.splitn(7, '\t').collect();
                if f.len() == 7 {
                    if let (Ok(id), Ok(t)) = (f[0].parse(), f[1].parse()) {
                        g.events.push_back(SecEv { id, t, kind: f[2].into(), level: f[3].into(), ip: f[4].into(), dev: f[5].into(), text: f[6].into() });
                    }
                }
            }
        }
        while g.events.len() > MAX_EVENTS {
            g.events.pop_front();
        }
        g.next_id = g.events.back().map(|e| e.id + 1).unwrap_or_else(|| sys::wall_secs() * 1000);
        g
    }

    fn push(&mut self, kind: usize, level: &str, ip: Ipv4Addr, dev: &str, text: String) -> SecEv {
        let ev = SecEv { id: self.next_id, t: sys::wall_secs(), kind: KINDS[kind].into(), level: level.into(), ip: ip.to_string(), dev: dev.into(), text: text.replace(['\t', '\n'], " ") };
        self.next_id += 1;
        report::log(&format!("[安全] {}", ev.text));
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.events_file) {
            let _ = writeln!(f, "{}\t{}\t{}\t{}\t{}\t{}\t{}", ev.id, ev.t, ev.kind, ev.level, ev.ip, ev.dev, ev.text);
        }
        self.events.push_back(ev.clone());
        if self.events.len() > MAX_EVENTS {
            self.events.pop_front();
            let mut s = String::new();
            for e in &self.events {
                s.push_str(&format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\n", e.id, e.t, e.kind, e.level, e.ip, e.dev, e.text));
            }
            let _ = sys::write_atomic(&self.events_file, s.as_bytes());
        }
        ev
    }

    /// 名单文件变了就重新装进内核；白名单（配置文件里的 sec_allow）改了也立即生效，不用重启
    fn reload_lists(&mut self, cfg: &Config) {
        let m = mtime(&self.conf);
        if m != self.conf_mtime {
            self.conf_mtime = m;
            if let Ok(s) = std::fs::read_to_string(&self.conf) {
                let allow = config::nets(config::parse_kv(&s).get("sec_allow"));
                if allow != self.allow {
                    net::set_allow(&allow);
                    report::log(&format!("安全防护白名单已更新：{} 条", allow.len()));
                    self.allow = allow;
                    self.bad_mtime = None; // 白名单里的地址要从恶意名单里去掉
                }
            }
        }
        let m = mtime(&cfg.sec.bad_list);
        if m != self.bad_mtime {
            self.bad_mtime = m;
            let text = std::fs::read_to_string(&cfg.sec.bad_list).unwrap_or_default();
            let mut keep: Vec<String> = cfg.sec.private_nets.clone();
            keep.extend(self.allow.iter().cloned());
            let nets = filter_bad(&text, &keep);
            if net::set_bad(&nets) {
                self.bad_count = nets.len();
                report::log(&format!("恶意地址名单已装入：{} 条", nets.len()));
            }
        }
    }

    /// 每秒调用，按 sec_poll_secs 的间隔真正检查；返回新出现的事件（调用方决定要不要推送）
    pub fn tick(&mut self, cfg: &Config, now_ms: u64) -> Vec<SecEv> {
        if self.last_poll != 0 && now_ms.saturating_sub(self.last_poll) < cfg.sec.poll_secs as u64 * 1000 {
            return Vec::new();
        }
        self.last_poll = now_ms;
        self.poll(cfg)
    }

    pub fn poll(&mut self, cfg: &Config) -> Vec<SecEv> {
        if !cfg.sec.enable {
            self.write(cfg, &HashMap::new(), &Names::load(cfg));
            return Vec::new();
        }
        // 启动时宽带还没通、找不到外网网卡：等它通了再补建规则
        let pending = net::SEC_ERR.lock().unwrap().clone();
        if pending.is_some() && net::wan_if(cfg).is_some() && !net::sec_present(cfg) {
            if net::apply_fw(cfg).is_ok() && net::SEC_ERR.lock().unwrap().is_none() {
                report::log("安全防护规则已补建");
            }
        }
        self.reload_lists(cfg);
        let mut names_all: Vec<&str> = SET_BLK.to_vec();
        names_all.extend_from_slice(&SET_AL);
        names_all.push(SET_HIT);
        let saved = save_all(&names_all);
        let mut hits: HashMap<Ipv4Addr, Vec<(String, u16, Ipv4Addr)>> = HashMap::new();
        for m in parse_save(&saved, SET_HIT) {
            if let Some((s, p, port, d)) = parse_hit(&m.key) {
                hits.entry(s).or_default().push((p, port, d));
            }
        }
        let names = Names::load(cfg);
        let hours = cfg.sec.block_secs / 3600;
        let now = sys::wall_secs();
        let mut fresh = Vec::new();
        let mut current: HashMap<(usize, Ipv4Addr), Member> = HashMap::new();
        for (i, set) in SET_BLK.iter().chain(SET_AL.iter()).enumerate() {
            for m in parse_save(&saved, set) {
                if let Ok(ip) = m.key.parse::<Ipv4Addr>() {
                    current.insert((i, ip), m);
                }
            }
        }
        let mut keys: Vec<&(usize, Ipv4Addr)> = current.keys().collect();
        keys.sort();
        for &(i, ip) in keys {
            if self.known.contains_key(&(i, ip)) {
                continue;
            }
            // 守护程序重启前就有的封锁：事件记录里有它（同一类、封锁期内），沿用当时的时间，不再出事件
            if self.first && i < 3 {
                let ips = ip.to_string();
                let prev = self.events.iter().rev().find(|e| e.ip == ips && e.kind == KINDS[i] && e.t + cfg.sec.block_secs as u64 >= now);
                if let Some(e) = prev {
                    self.known.insert((i, ip), e.t);
                    continue;
                }
            }
            self.known.insert((i, ip), now);
            let h = hits.get(&ip).cloned().unwrap_or_default();
            // 碰得最多的那个目的地址（被扫的设备 / 家里设备狂连的对象）
            let mut per_dst: HashMap<Ipv4Addr, BTreeSet<u16>> = HashMap::new();
            for (_, port, d) in &h {
                per_dst.entry(*d).or_default().insert(*port);
            }
            let top = per_dst.iter().max_by_key(|(d, p)| (p.len(), std::cmp::Reverse(**d))).map(|(d, _)| *d);
            let ports: BTreeSet<u16> = h.iter().map(|x| x.1).collect();
            let top_ports = top.and_then(|d| per_dst.get(&d).cloned()).unwrap_or_default();
            let pt = |p: &BTreeSet<u16>| if p.is_empty() { String::new() } else { format!("（端口 {}）", ports_text(p)) };
            let tgt = |d: Option<Ipv4Addr>| d.map(|d| names.label(d)).unwrap_or_else(|| "家里的设备".into());
            let tdev = top.and_then(|d| names.name(d)).unwrap_or_default();
            let dsts: Vec<Ipv4Addr> = {
                let mut v: Vec<_> = per_dst.keys().copied().collect();
                v.sort();
                v
            };
            let dl: Vec<String> = dsts.iter().take(3).map(|d| d.to_string()).collect();
            let dtext = if dsts.len() > 3 { format!("{} 等 {} 个地址", dl.join("、"), dsts.len()) } else { dl.join("、") };
            let ev = match i {
                0 => self.push(i, "info", ip, "", format!("外网 {} 在扫描路由器的端口{}，已自动封锁 {} 小时", ip, pt(&ports), hours)),
                1 => self.push(i, "alert", ip, &tdev, format!("外网 {} 反复连接{}的管理端口{}，像在猜密码，已自动封锁 {} 小时", ip, tgt(top), pt(&top_ports), hours)),
                2 => self.push(i, "warn", ip, &tdev, format!("外网 {} 短时间大量连接{}{}，已自动封锁 {} 小时", ip, tgt(top), pt(&top_ports), hours)),
                _ => {
                    let dev = names.name(ip).unwrap_or_default();
                    let who = names.label(ip);
                    let text = match i {
                        3 => format!("家里的{}连接了公开恶意地址名单里的 {}{}，可能中了木马或广告软件，建议检查（只提醒，没有拦截）", who, dtext, pt(&ports)),
                        4 => format!("家里的{}短时间大量连接外网的高危端口（{}，端口 {}），像中毒后在扫描别人，建议检查（只提醒，没有拦截）", who, dtext, ports_text(&ports)),
                        _ => format!(
                            "家里的{}短时间向 {} 发起大量连接{}，可能被人利用发起攻击，建议检查（只提醒，没有拦截）",
                            who,
                            top.map(|d| d.to_string()).unwrap_or_default(),
                            pt(&top_ports)
                        ),
                    };
                    self.push(i, "alert", ip, &dev, text)
                }
            };
            fresh.push(ev);
        }
        // 过期或被手动解封的，下次再出现要重新出事件
        self.known.retain(|k, _| current.contains_key(k));
        self.first = false;
        self.write(cfg, &current, &names);
        fresh
    }

    /// 守护程序停止后：封锁已撤掉，网页显示"未生效"，历史事件保留
    pub fn stopped(&self, cfg: &Config) {
        self.write(cfg, &HashMap::new(), &Names::load(cfg));
    }

    fn write(&self, cfg: &Config, current: &HashMap<(usize, Ipv4Addr), Member>, names: &Names) {
        let now = sys::wall_secs();
        let today = sys::day_number();
        let day_of = |t: u64| (t + 8 * 3600) / 86400;
        let mut blocked: Vec<(&(usize, Ipv4Addr), &Member)> = current.iter().filter(|(k, _)| k.0 < 3).collect();
        blocked.sort_by_key(|(k, _)| std::cmp::Reverse(self.known.get(k).copied().unwrap_or(0)));
        let mut bl = String::from("[");
        for (n, ((i, ip), m)) in blocked.iter().take(100).enumerate() {
            if n > 0 {
                bl.push(',');
            }
            bl.push_str(
                &Json::obj()
                    .s("ip", &ip.to_string())
                    .s("kind", KINDS[*i])
                    .n("since", self.known.get(&(*i, *ip)).copied().unwrap_or(now))
                    .n("expires", now + m.timeout)
                    .raw("hits", m.packets.map(|p| p.to_string()).unwrap_or_else(|| "null".into()))
                    .done(),
            );
        }
        bl.push(']');
        let mut al = String::from("[");
        let alerts: Vec<_> = current.iter().filter(|(k, _)| k.0 >= 3).collect();
        for (n, ((i, ip), _)) in alerts.iter().enumerate() {
            if n > 0 {
                al.push(',');
            }
            al.push_str(&Json::obj().s("ip", &ip.to_string()).s("kind", KINDS[*i]).s("dev", &names.name(*ip).unwrap_or_default()).done());
        }
        al.push(']');
        let mut ev = String::from("[");
        for (n, e) in self.events.iter().rev().take(100).enumerate() {
            if n > 0 {
                ev.push(',');
            }
            ev.push_str(&Json::obj().n("id", e.id).n("t", e.t).s("kind", &e.kind).s("level", &e.level).s("ip", &e.ip).s("dev", &e.dev).s("m", &e.text).done());
        }
        ev.push(']');
        let td = |kinds: &[&str]| self.events.iter().filter(|e| day_of(e.t) == today && kinds.contains(&e.kind.as_str())).count();
        let err = net::SEC_ERR.lock().unwrap().clone().unwrap_or_default();
        let s = Json::obj()
            .n("v", 1)
            .n("updated", now)
            .b("enable", cfg.sec.enable)
            .b("active", cfg.sec.enable && err.is_empty() && net::sec_present(cfg))
            .s("error", &err)
            .s("wan", &net::wan_if(cfg).unwrap_or_default())
            .n("block_hours", cfg.sec.block_secs / 3600)
            .o(
                "stats",
                Json::obj()
                    .n("blocked", blocked.len())
                    .n("blocks_today", td(&KINDS[..3]))
                    .n("alerts_today", td(&KINDS[3..]))
                    .n("bad_count", self.bad_count)
                    .n("dropped", if cfg.sec.enable { rule_packets("CORPLINE_SECDROP", "DROP") + rule_packets("CORPLINE_SEC", SET_BAD) } else { 0 }),
            )
            .raw("blocked", bl)
            .raw("watch", al)
            .raw("events", ev)
            .done();
        let _ = sys::write_atomic(&self.out, s.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ipset_save() {
        let t = "create corpline_blk_scan hash:ip family inet hashsize 1024 maxelem 65536 timeout 86400 counters\n\
                 add corpline_blk_scan 100.64.0.1 timeout 86391 packets 3 bytes 180\n\
                 add corpline_blk_scan 1.2.3.4 timeout 5\n\
                 add corpline_sechit 1.2.3.4,tcp:22,100.64.0.2 timeout 3591\n";
        let v = parse_save(t, "corpline_blk_scan");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], Member { key: "100.64.0.1".into(), timeout: 86391, packets: Some(3) });
        assert_eq!(v[1].packets, None);
        let h = parse_save(t, "corpline_sechit");
        assert_eq!(parse_hit(&h[0].key), Some(("1.2.3.4".parse().unwrap(), "tcp".into(), 22, "100.64.0.2".parse().unwrap())));
    }

    #[test]
    fn bad_list_drops_private_and_allowed() {
        let text = "# firehol\n0.0.0.0/8\n10.0.0.0/8\n1.2.3.0/24\n198.51.0.0/16\n5.6.7.8\nbad\n192.168.0.0/16 # x\n2.0.0.0/4\n";
        let keep = vec!["10.0.0.0/8".to_string(), "192.168.0.0/16".into(), "0.0.0.0/8".into(), "198.51.100.0/24".into()];
        assert_eq!(filter_bad(text, &keep), vec!["1.2.3.0/24", "5.6.7.8"]);
    }

    #[test]
    fn port_text() {
        let s: BTreeSet<u16> = [22, 23].into_iter().collect();
        assert_eq!(ports_text(&s), "22、23");
        let s: BTreeSet<u16> = (1..=9).collect();
        assert_eq!(ports_text(&s), "1、2、3、4、5、6 等 9 个");
    }
}
