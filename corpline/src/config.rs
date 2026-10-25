//! 配置：一个 key=value 文本文件（由插件外壳脚本从软件中心 dbus 生成，放在内存盘 /tmp）。
//! 守护程序不直接依赖 dbus，便于在沙盒里测试。

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub server: String,
    pub protocol: String,
    pub user: String,
    pub password: String,
    pub openconnect: PathBuf,
    pub oc_ld_library_path: String,
    pub cafile: String,
    pub servercert: String,
    pub oc_extra: Vec<String>,
    pub no_dtls: bool,

    pub lan_if: String,
    pub tun_if: String,
    pub table: u32,
    pub mark: u32,
    pub rule_pref: u32,
    pub bypass_nets: Vec<String>,
    pub direct_macs: Vec<String>,
    /// 走公司线路的设备里，从这些本机端口发出的连接直连：(MAC, 起, 止)
    pub direct_ports: Vec<(String, u16, u16)>,

    pub overseas_domains: Vec<String>,
    /// 用户单独设为直连的网站（解析走运营商 DNS，连接不进隧道）
    pub direct_domains: Vec<String>,
    pub overseas_dns: Vec<Ipv4Addr>,
    pub company_dns_override: Vec<Ipv4Addr>,
    pub internal_domains: Vec<String>,
    pub isp_dns: Vec<Ipv4Addr>,
    pub isp_dns_file: PathBuf,
    pub cn_direct: bool,
    pub cn_list: PathBuf,
    pub cn_ip_list: PathBuf,
    pub filter_aaaa: bool,
    pub probe_domain: String,
    pub dns_listen: SocketAddrV4,

    pub dnsmasq_conf: PathBuf,
    pub dnsmasq_restart: String,
    pub dnsmasq_pidfile: PathBuf,
    pub manage_dnsmasq: bool,
    pub ipv6_guard: bool,

    pub relaxed: bool,
    pub run_dir: PathBuf,
    pub status_dir: PathBuf,
    pub hook_cmd: String,
    pub notify_cmd: String,
    pub daily_cap: u32,

    pub sec: Sec,
}

/// 安全防护：外来的连接按来源 IP 计数，超过阈值自动封锁；家里设备异常外连只提醒。
/// 计数与拦截都由内核（hashlimit / ipset）完成，守护程序只定时读结果、写事件。
#[derive(Clone, Debug)]
pub struct Sec {
    pub enable: bool,
    /// 外网网卡；空 = 按主路由表默认路由的网卡（PPPoE 是 ppp0）
    pub wan_if: String,
    /// 白名单（IP 或网段）：不计数、不封锁、不提醒
    pub allow: Vec<String>,
    pub block_secs: u32,
    /// 家里的管理类端口（端口转发后的内网端口）：连得太频繁按"猜密码"处理
    pub admin_ports: Vec<u16>,
    /// 家里设备连外网这些端口太频繁 = 可能中毒在扫描别人
    pub risky_ports: Vec<u16>,
    /// 每分钟阈值：连路由器本身（扫描）/ 连管理端口 / 任意转发 / 家里设备扫外网 / 家里设备狂连同一地址
    pub scan_per_min: u32,
    pub brute_per_min: u32,
    pub flood_per_min: u32,
    pub out_scan_per_min: u32,
    pub out_flood_per_min: u32,
    /// 不算"外网"的目的地（家里设备访问它们不提醒）
    pub private_nets: Vec<String>,
    /// 公开的恶意 IP 名单（每行一个 IP 或网段）
    pub bad_list: PathBuf,
    pub leases_file: PathBuf,
    /// "MAC<TAB>名字" 的设备名单（外壳脚本从网页的设备名单生成）
    pub names_file: PathBuf,
    pub poll_secs: u32,
}

fn list(v: Option<&String>) -> Vec<String> {
    v.map(|s| {
        s.split(|c: char| c == ',' || c.is_whitespace())
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

fn ips(v: Option<&String>) -> Vec<Ipv4Addr> {
    list(v).iter().filter_map(|s| s.parse().ok()).collect()
}

fn is_mac(s: &str) -> bool {
    let p: Vec<&str> = s.split(':').collect();
    p.len() == 6 && p.iter().all(|x| x.len() == 2 && x.chars().all(|c| c.is_ascii_hexdigit()))
}

/// "MAC@端口" 或 "MAC@起-止"，逗号分隔；格式不对的条目丢掉
fn port_rules(v: Option<&String>) -> Vec<(String, u16, u16)> {
    list(v)
        .iter()
        .filter_map(|e| {
            let (mac, p) = e.split_once('@')?;
            if !is_mac(mac) {
                return None;
            }
            let (a, b) = p.split_once('-').unwrap_or((p, p));
            let (a, b): (u16, u16) = (a.parse().ok()?, b.parse().ok()?);
            (a >= 1 && a <= b).then(|| (mac.to_ascii_uppercase(), a, b))
        })
        .collect()
}

fn ports(v: Option<&String>, default: &[u16]) -> Vec<u16> {
    match v {
        None => default.to_vec(),
        Some(_) => list(v).iter().filter_map(|x| x.parse().ok()).filter(|p| *p > 0).collect(),
    }
}

/// IPv4 地址或网段（a.b.c.d 或 a.b.c.d/n），不合法的丢掉，防止写进 ipset 时整批失败
pub fn is_net(s: &str) -> bool {
    let (ip, len) = s.split_once('/').unwrap_or((s, "32"));
    ip.parse::<Ipv4Addr>().is_ok() && len.parse::<u8>().map(|n| n <= 32).unwrap_or(false)
}

pub fn nets(v: Option<&String>) -> Vec<String> {
    list(v).into_iter().filter(|x| is_net(x)).collect()
}

fn flag(v: Option<&String>, default: bool) -> bool {
    match v.map(|s| s.trim()) {
        Some("1") | Some("true") | Some("yes") | Some("on") => true,
        Some("0") | Some("false") | Some("no") | Some("off") => false,
        _ => default,
    }
}

fn num(v: Option<&String>, default: u32) -> u32 {
    v.and_then(|s| {
        let s = s.trim();
        if let Some(h) = s.strip_prefix("0x") {
            u32::from_str_radix(h, 16).ok()
        } else {
            s.parse().ok()
        }
    })
    .unwrap_or(default)
}

pub fn parse_kv(text: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            // 密码原样保留（首尾空格也可能是密码的一部分）
            let v = if k == "password" { raw.trim_start().split_once('=').map(|x| x.1).unwrap_or("").trim_end_matches('\r') } else { v.trim() };
            m.insert(k.to_string(), v.to_string());
        }
    }
    m
}

/// 域名统一成小写、去首尾点。
pub fn norm_domain(s: &str) -> String {
    s.trim().trim_matches('.').to_ascii_lowercase()
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("读取配置 {} 失败：{}", path.display(), e))?;
        Self::from_map(&parse_kv(&text))
    }

    pub fn from_map(m: &HashMap<String, String>) -> Result<Config, String> {
        let g = |k: &str| m.get(k);
        let s = |k: &str, d: &str| m.get(k).cloned().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string());
        let self_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("/koolshare/bin/corpline"));
        let mut overseas_domains: Vec<String> = list(g("overseas_domains")).iter().map(|d| norm_domain(d)).collect();
        if g("overseas_domains").is_none() {
            overseas_domains = vec!["apple.com".into(), "mzstatic.com".into()];
        }
        let mut overseas_dns = ips(g("overseas_dns"));
        if overseas_dns.is_empty() {
            overseas_dns = vec![Ipv4Addr::new(8, 8, 8, 8), Ipv4Addr::new(1, 1, 1, 1)];
        }
        let cfg = Config {
            server: s("server", ""),
            protocol: s("protocol", "anyconnect"),
            user: s("user", ""),
            password: m.get("password").cloned().unwrap_or_default(),
            openconnect: PathBuf::from(s("openconnect", "/opt/sbin/openconnect")),
            oc_ld_library_path: s("oc_ld_library_path", "/opt/lib:/opt/usr/lib"),
            cafile: s("cafile", ""),
            servercert: s("servercert", ""),
            oc_extra: list(g("oc_extra")),
            no_dtls: flag(g("no_dtls"), false),

            lan_if: s("lan_if", "br0"),
            tun_if: s("tun_if", "corp0"),
            table: num(g("table"), 88),
            mark: num(g("mark"), 0x5000),
            rule_pref: num(g("rule_pref"), 8850),
            bypass_nets: list(g("bypass_nets")),
            direct_macs: list(g("direct_macs")).iter().map(|x| x.to_ascii_uppercase()).collect(),
            direct_ports: port_rules(g("direct_ports")),

            overseas_domains,
            direct_domains: list(g("direct_domains")).iter().map(|d| norm_domain(d)).collect(),
            overseas_dns,
            company_dns_override: ips(g("company_dns")),
            internal_domains: list(g("internal_domains")).iter().map(|d| norm_domain(d)).collect(),
            isp_dns: ips(g("isp_dns")),
            isp_dns_file: PathBuf::from(s("isp_dns_file", "/tmp/resolv.dnsmasq")),
            cn_direct: flag(g("cn_direct"), false),
            cn_list: PathBuf::from(s("cn_list", "/koolshare/configs/corpline/china-domains.conf")),
            cn_ip_list: PathBuf::from(s("cn_ip_list", "/koolshare/configs/corpline/china-ips.txt")),
            filter_aaaa: flag(g("filter_aaaa"), true),
            probe_domain: s("probe_domain", ""),
            dns_listen: s("dns_listen", "127.0.0.1:5354").parse().map_err(|_| "dns_listen 格式不对".to_string())?,

            dnsmasq_conf: PathBuf::from(s("dnsmasq_conf", "/jffs/configs/dnsmasq.d/corpline.conf")),
            dnsmasq_restart: s("dnsmasq_restart", "service restart_dnsmasq"),
            dnsmasq_pidfile: PathBuf::from(s("dnsmasq_pidfile", "/var/run/dnsmasq.pid")),
            manage_dnsmasq: flag(g("manage_dnsmasq"), true),
            ipv6_guard: flag(g("ipv6_guard"), true),

            relaxed: s("sensitivity", "normal") == "relaxed",
            run_dir: PathBuf::from(s("run_dir", "/var/run/corpline")),
            status_dir: PathBuf::from(s("status_dir", "/tmp/upload")),
            hook_cmd: s("hook_cmd", &format!("{} hook", self_exe.display())),
            notify_cmd: s("notify_cmd", ""),
            daily_cap: num(g("daily_cap"), 60),

            sec: Sec {
                enable: flag(g("sec_enable"), true),
                wan_if: s("wan_if", ""),
                allow: nets(g("sec_allow")),
                block_secs: num(g("sec_block_hours"), 24).clamp(1, 24 * 30) * 3600,
                admin_ports: ports(g("sec_admin_ports"), &[22, 23, 80, 443, 3389, 5000, 5001, 6690, 8080, 8443, 8085, 13000]),
                risky_ports: ports(g("sec_risky_ports"), &[22, 23, 445, 1433, 2323, 3306, 3389, 5555, 6379, 7547, 8291, 37215, 52869]),
                scan_per_min: num(g("sec_scan_per_min"), 20).max(1),
                brute_per_min: num(g("sec_brute_per_min"), 40).max(1),
                flood_per_min: num(g("sec_flood_per_min"), 300).max(1),
                out_scan_per_min: num(g("sec_out_scan_per_min"), 30).max(1),
                out_flood_per_min: num(g("sec_out_flood_per_min"), 600).max(1),
                private_nets: match g("sec_private_nets") {
                    Some(_) => nets(g("sec_private_nets")),
                    // 私有与保留地址：198.18.0.0/15 常被代理软件当"假 IP"用，文档示例网段也不会出现在公网
                    None => [
                        "0.0.0.0/8", "10.0.0.0/8", "100.64.0.0/10", "127.0.0.0/8", "169.254.0.0/16", "172.16.0.0/12", "192.0.0.0/24", "192.0.2.0/24",
                        "192.168.0.0/16", "198.18.0.0/15", "198.51.100.0/24", "203.0.113.0/24", "224.0.0.0/3",
                    ]
                        .iter()
                        .map(|x| x.to_string())
                        .collect(),
                },
                bad_list: PathBuf::from(s("sec_bad_list", "/koolshare/configs/corpline/bad-ips.netset")),
                leases_file: PathBuf::from(s("leases_file", "/var/lib/misc/dnsmasq.leases")),
                names_file: PathBuf::from(s("names_file", "/var/run/corpline/devices.tsv")),
                poll_secs: num(g("sec_poll_secs"), 30).clamp(1, 600),
            },
        };
        if cfg.server.is_empty() {
            return Err("还没有填写公司线路服务器地址".into());
        }
        if cfg.user.is_empty() || cfg.password.is_empty() {
            return Err("还没有填写公司账号或密码".into());
        }
        // 0 / 253–255 是内核的 unspec/default/main/local 表；mark 为 0 等于不打标记；
        // rule_pref 还要给 oif 规则留出 rule_pref-1，且不能抢到 local 表（pref 0）前面
        if cfg.table == 0 || cfg.table >= 253 {
            return Err(format!("路由表编号 {} 不可用（不能是 0 或 253–255）", cfg.table));
        }
        if cfg.mark == 0 {
            return Err("连接标记 mark 不能是 0".into());
        }
        if cfg.rule_pref < 2 || cfg.rule_pref >= 32766 {
            return Err(format!("规则优先级 {} 不可用（应在 2–32765 之间）", cfg.rule_pref));
        }
        Ok(cfg)
    }

    /// 服务器主机名（去掉端口与路径），用于自己解析服务器地址。
    pub fn server_host(&self) -> String {
        let s = self.server.trim_start_matches("https://");
        let s = s.split('/').next().unwrap_or(s);
        match s.rsplit_once(':') {
            Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => h.to_string(),
            _ => s.to_string(),
        }
    }

    pub fn tunnel_file(&self) -> PathBuf {
        self.run_dir.join("tunnel.env")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_numbers() {
        let base = "server=v\nuser=u\npassword=p\n";
        for bad in ["table=0", "table=254", "mark=0", "rule_pref=1", "rule_pref=40000"] {
            assert!(Config::from_map(&parse_kv(&format!("{}{}\n", base, bad))).is_err(), "{}", bad);
        }
        assert!(Config::from_map(&parse_kv(base)).is_ok());
    }

    fn base() -> HashMap<String, String> {
        parse_kv("server=vpn.example.com:8443\nuser=u\npassword=p=q\n")
    }

    #[test]
    fn defaults() {
        let c = Config::from_map(&base()).unwrap();
        assert_eq!(c.password, "p=q");
        let m = parse_kv("server=s\nuser=u\npassword= a b \n");
        assert_eq!(m.get("password").unwrap(), " a b ");
        assert_eq!(c.mark, 0x5000);
        assert_eq!(c.overseas_domains, vec!["apple.com", "mzstatic.com"]);
        assert_eq!(c.server_host(), "vpn.example.com");
        assert!(!c.cn_direct);
    }

    #[test]
    fn missing_account() {
        let m = parse_kv("server=x\n");
        assert!(Config::from_map(&m).is_err());
    }

    #[test]
    fn lists() {
        let mut m = base();
        m.insert("overseas_domains".into(), "Apple.com, .sony.com.".into());
        m.insert("direct_macs".into(), "aa:bb:cc:dd:ee:ff".into());
        m.insert("mark".into(), "0x8000".into());
        let c = Config::from_map(&m).unwrap();
        assert_eq!(c.overseas_domains, vec!["apple.com", "sony.com"]);
        assert_eq!(c.direct_macs, vec!["AA:BB:CC:DD:EE:FF"]);
        assert_eq!(c.mark, 0x8000);
    }

    #[test]
    fn security() {
        let c = Config::from_map(&base()).unwrap();
        assert!(c.sec.enable);
        assert_eq!(c.sec.block_secs, 86400);
        assert!(c.sec.admin_ports.contains(&5001));
        assert!(c.sec.private_nets.contains(&"192.168.0.0/16".to_string()));
        let mut m = base();
        m.insert("sec_allow".into(), "203.0.113.0/24, 1.2.3.4,bad,1.2.3.4/40".into());
        m.insert("sec_admin_ports".into(), "".into());
        m.insert("sec_block_hours".into(), "0".into());
        m.insert("sec_private_nets".into(), "10.0.0.0/8".into());
        let c = Config::from_map(&m).unwrap();
        assert_eq!(c.sec.allow, vec!["203.0.113.0/24", "1.2.3.4"]);
        assert!(c.sec.admin_ports.is_empty());
        assert_eq!(c.sec.block_secs, 3600);
        assert_eq!(c.sec.private_nets, vec!["10.0.0.0/8"]);
    }

    #[test]
    fn port_exceptions() {
        let mut m = base();
        m.insert("direct_ports".into(), "aa:bb:cc:dd:ee:ff@32400,AA:BB:CC:DD:EE:FF@3982-4081,x@1,AA:BB:CC:DD:EE:FF@0,AA:BB:CC:DD:EE:FF@9-3,AA:BB:CC:DD:EE:FF@70000".into());
        let c = Config::from_map(&m).unwrap();
        let mac = "AA:BB:CC:DD:EE:FF".to_string();
        assert_eq!(c.direct_ports, vec![(mac.clone(), 32400, 32400), (mac, 3982, 4081)]);
    }
}
