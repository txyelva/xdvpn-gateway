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

    pub overseas_domains: Vec<String>,
    pub overseas_dns: Vec<Ipv4Addr>,
    pub company_dns_override: Vec<Ipv4Addr>,
    pub internal_domains: Vec<String>,
    pub isp_dns: Vec<Ipv4Addr>,
    pub isp_dns_file: PathBuf,
    pub cn_direct: bool,
    pub cn_list: PathBuf,
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

            overseas_domains,
            overseas_dns,
            company_dns_override: ips(g("company_dns")),
            internal_domains: list(g("internal_domains")).iter().map(|d| norm_domain(d)).collect(),
            isp_dns: ips(g("isp_dns")),
            isp_dns_file: PathBuf::from(s("isp_dns_file", "/tmp/resolv.dnsmasq")),
            cn_direct: flag(g("cn_direct"), false),
            cn_list: PathBuf::from(s("cn_list", "/koolshare/configs/corpline/china-domains.conf")),
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
}
