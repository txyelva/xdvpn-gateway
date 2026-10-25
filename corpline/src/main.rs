//! corpline —— 公司专线守护程序
//!
//! 用法：
//!   corpline run [-c 配置文件]      前台运行守护程序（由插件脚本放到后台）
//!   corpline hook                    openconnect 连接脚本（openconnect 调用）
//!   corpline fw [-c 配置文件]        重建防火墙规则（固件重建防火墙后调用）
//!   corpline cleanup [-c 配置文件]   撤掉所有网络改动（守护程序不在也可执行）
//!   corpline version

mod config;
mod devices;
mod dns;
mod guard;
mod machine;
mod net;
mod oc;
mod report;
mod shaper;
mod sys;

use config::Config;
use machine::{Action, Event, Level, Machine, Oc as OcState, Params};
use report::{Json, Timeline};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");
// 运行目录不能叫 /tmp/corpline*：软件中心安装/升级完会 rm -rf /tmp/<插件名>*
const DEFAULT_CONF: &str = "/var/run/corpline/corpline.conf";

enum Msg {
    Oc(oc::OcMsg),
    Probe { ok: bool, rtt: u32, detail: String },
    Note(Level, String),
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");
    let conf = args
        .iter()
        .position(|a| a == "-c")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONF));
    let code = match cmd {
        "run" => run(&conf),
        "hook" => hook(),
        "fw" => with_cfg(&conf, |c| {
            if let Err(e) = net::apply_fw(c) {
                eprintln!("{}", e);
                return 1;
            }
            0
        }),
        "cleanup" => {
            let c = Config::load(&conf).ok().unwrap_or_else(fallback_cfg);
            // 先让 dnsmasq 不再问已经不在的转发端口，再撤路由与防火墙
            net::dnsmasq_remove(&c);
            net::cleanup(&c);
            0
        }
        "devices" => {
            // 只读：列出每台设备认出来的名字，排查"为什么叫这个名字"用
            let c = Config::load(&conf).ok().unwrap_or_else(fallback_cfg);
            print!("{}", devices::Names::load(&c).table());
            0
        }
        "version" | "-V" | "--version" => {
            println!("corpline {}", VERSION);
            0
        }
        _ => {
            eprintln!("用法：corpline run|hook|fw|cleanup|devices|version [-c 配置文件]");
            2
        }
    };
    std::process::exit(code);
}

/// 配置文件缺失时 cleanup 仍要能用：用默认值
fn fallback_cfg() -> Config {
    let mut m = config::parse_kv("server=-\nuser=-\npassword=-\n");
    m.insert("manage_dnsmasq".into(), "1".into());
    Config::from_map(&m).expect("默认配置")
}

fn with_cfg(p: &Path, f: impl FnOnce(&Config) -> i32) -> i32 {
    match Config::load(p) {
        Ok(c) => f(&c),
        Err(e) => {
            eprintln!("{}", e);
            1
        }
    }
}

// ---------------- openconnect 连接脚本 ----------------

fn hook() -> i32 {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let reason = env("reason");
    let run_dir = PathBuf::from(std::env::var("CORPLINE_RUN_DIR").unwrap_or_else(|_| "/var/run/corpline".into()));
    let table: u32 = env("CORPLINE_TABLE").parse().unwrap_or(88);
    let dev = env("TUNDEV");
    let file = run_dir.join("tunnel.env");
    match reason.as_str() {
        "connect" | "reconnect" => {
            let addr = env("INTERNAL_IP4_ADDRESS");
            let ok = net::tun_up(&dev, &addr, &env("INTERNAL_IP4_MTU"), table);
            // 隧道地址变了，马上换掉"按回程地址认隧道连接"的规则（不等每分钟的自检）
            let mark = u32::from_str_radix(env("CORPLINE_MARK").trim_start_matches("0x"), 16).unwrap_or(0x5000);
            net::set_flow(&run_dir, mark, addr.parse().ok());
            let mut s = String::new();
            for k in ["TUNDEV", "INTERNAL_IP4_ADDRESS", "INTERNAL_IP4_MTU", "INTERNAL_IP4_DNS", "CISCO_DEF_DOMAIN", "VPNGATEWAY", "reason"] {
                s.push_str(&format!("{}={}\n", k, env(k).replace('\n', " ")));
            }
            s.push_str(&format!("CONFIGURED={}\nAT={}\n", if ok { 1 } else { 0 }, sys::wall_secs()));
            let _ = std::fs::create_dir_all(&run_dir);
            let _ = sys::write_atomic(&file, s.as_bytes());
        }
        "disconnect" => {
            let _ = std::fs::remove_file(&file);
            sys::run_ok("ip", &["route", "flush", "table", &table.to_string()]);
        }
        _ => {}
    }
    0 // 永远返回成功：脚本失败会让 openconnect 放弃整个连接
}

// ---------------- 守护程序 ----------------

#[derive(Default, Clone)]
struct Tunnel {
    dev: String,
    ip: String,
    mtu: String,
    dns: Vec<Ipv4Addr>,
    domain: String,
    gateway: String,
    up_since: u64,
}

fn parse_tunnel(path: &Path) -> Option<Tunnel> {
    let s = std::fs::read_to_string(path).ok()?;
    let m = config::parse_kv(&s);
    let g = |k: &str| m.get(k).cloned().unwrap_or_default();
    if g("CONFIGURED") != "1" {
        return None;
    }
    Some(Tunnel {
        dev: g("TUNDEV"),
        ip: g("INTERNAL_IP4_ADDRESS"),
        mtu: g("INTERNAL_IP4_MTU"),
        dns: g("INTERNAL_IP4_DNS").split_whitespace().filter_map(sys::ip4).collect(),
        domain: g("CISCO_DEF_DOMAIN").split_whitespace().next().unwrap_or("").to_string(),
        gateway: g("VPNGATEWAY"),
        up_since: g("AT").parse().unwrap_or_else(|_| sys::wall_secs()),
    })
}

struct Daemon {
    cfg: Config,
    m: Machine,
    tl: Timeline,
    oc: Option<oc::Oc>,
    tunnel: Option<Tunnel>,
    tunnel_mtime: Option<std::time::SystemTime>,
    dns: Arc<dns::Shared>,
    isp: Vec<Ipv4Addr>,
    server_ips: Vec<Ipv4Addr>,
    tx: mpsc::Sender<Msg>,
    oc_tx: mpsc::Sender<oc::OcMsg>,
    day: u64,
    traffic: (u64, u64),
    counters: Option<(u64, u64)>,
    self_cpu: (u64, u64),
    cpu_pct: f64,
    dirty: bool,
    long_fallback_noted: bool,
    bypass: Vec<String>,
    wan_up: bool,
    /// 总阀门上次没切成功（ipset 偶发出错）：定时重试，直到内核状态与状态机一致
    live_pending: Option<bool>,
    /// 本进程启动时刻（墙钟秒）：网页靠它区分"新启动的守护程序"与"即将被停掉的旧进程"
    started: u64,
    guard: guard::Guard,
    /// 中国 IP 段名单文件的修改时间与条数（国内直连开启时装进内核；每周更新后自动换上）
    cn_ips_mtime: Option<std::time::SystemTime>,
    cn_ips: usize,
    cn_ips_warned: bool,
    /// 公司线路上行排队控制（见 shaper.rs）
    shaper: shaper::Shaper,
    shape_failed: bool,
}

fn run(conf: &Path) -> i32 {
    let cfg = match Config::load(conf) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };
    let _ = std::fs::create_dir_all(&cfg.run_dir);
    let _ = std::fs::create_dir_all(&cfg.status_dir);
    report::init_log(cfg.status_dir.join("corpline_log.txt"));
    sys::install_signals();

    // 只允许一个实例
    let pidf = cfg.run_dir.join("corpline.pid");
    if let Ok(s) = std::fs::read_to_string(&pidf) {
        if let Ok(pid) = s.trim().parse::<i32>() {
            let comm = std::fs::read_to_string(format!("/proc/{}/comm", pid)).unwrap_or_default();
            if pid != std::process::id() as i32 && sys::alive(pid) && comm.trim() == "corpline" {
                eprintln!("corpline 已在运行（pid {}）", pid);
                return 3;
            }
        }
    }
    let mut tl = Timeline::load(cfg.run_dir.join("events.txt"));

    let isp = net::isp_dns(&cfg);
    let shared = dns::Shared::new(dns::View { isp: isp.clone(), overseas: cfg.overseas_dns.clone(), ..Default::default() });
    // 上次启动时名单缺失、这次有了：在"最近发生了什么"里补一条，让人知道它已经生效
    let cn_missing = cfg.run_dir.join("cn_missing");
    let cn = if cfg.cn_direct {
        let miss = |tl: &mut Timeline, text: &str| {
            tl.push(Level::Warn, text);
            let _ = std::fs::write(&cn_missing, b"");
        };
        match dns::domains::SuffixSet::load_file(&cfg.cn_list) {
            Ok(s) if s.len() > 1000 => {
                report::log(&format!("中国服务名单 {} 条（{} KB）", s.len(), s.bytes() / 1024));
                if std::fs::remove_file(&cn_missing).is_ok() {
                    tl.push(Level::Ok, &format!("中国服务名单已就绪（{} 个网站），国内网站直连已生效", s.len()));
                }
                Some(s)
            }
            Ok(s) => {
                miss(&mut tl, &format!("中国服务名单只有 {} 条，像是不完整，国内网站直连暂不生效", s.len()));
                None
            }
            Err(_) => {
                miss(&mut tl, "没有找到中国服务名单，国内网站直连暂不生效（下载进度见设置里的「国内网站直连」）");
                None
            }
        }
    } else {
        None
    };
    let st = dns::Static {
        overseas: dns::domains::SuffixSet::new(cfg.overseas_domains.clone()),
        direct: dns::domains::SuffixSet::new(cfg.direct_domains.clone()),
        recent_file: Some(cfg.status_dir.join("corpline_domains.json")),
        cn,
        filter_aaaa: cfg.filter_aaaa,
        cn_ipset: net::SET_CN.to_string(),
        ovs_ipset: if cfg.cn_direct { Some(net::SET_OVS.to_string()) } else { None },
    };
    let (tx, rx) = mpsc::channel::<Msg>();
    let ntx = Mutex::new(tx.clone());
    let notify: dns::Notify = Box::new(move |l, s| {
        let _ = ntx.lock().unwrap().send(Msg::Note(l, s));
    });
    // 先占 DNS 转发端口，再动网络：端口被占（多半是另一个 corpline 还在跑，pid 文件丢了也算）
    // 就直接退出，不碰正在工作的那一份的规则
    let engine = match dns::Engine::bind(cfg.dns_listen, shared.clone(), st, notify) {
        Ok(e) => e,
        Err(e) => {
            tl.push(Level::Err, &format!("DNS 转发端口 {} 被占用（{}），可能已有一份在运行；本次不启动", cfg.dns_listen, e));
            return 3;
        }
    };

    let _ = std::fs::write(&pidf, std::process::id().to_string());
    report::log(&format!("corpline {} 启动", VERSION));
    oc::kill_orphan(&cfg);
    let _ = std::fs::remove_file(cfg.tunnel_file());
    if let Err(e) = net::setup(&cfg) {
        tl.push(Level::Err, &format!("网络准备失败：{}，保持直连", e));
        net::cleanup(&cfg);
        let _ = std::fs::remove_file(&pidf);
        return 4;
    }
    if let Err(e) = std::thread::Builder::new().name("dns".into()).stack_size(256 * 1024).spawn(move || engine.run()) {
        // 没有 DNS 转发就不能装 dnsmasq 配置，撤掉改动退出（外层会按崩溃处理、稍后重启）
        tl.push(Level::Err, &format!("DNS 转发线程启动失败：{}，保持直连", e));
        net::cleanup(&cfg);
        let _ = std::fs::remove_file(&pidf);
        return 4;
    }
    if !net::dnsmasq_install(&cfg) {
        report::log("dnsmasq 配置写入或重启失败");
    }

    let (oc_tx, oc_rx) = mpsc::channel::<oc::OcMsg>();
    {
        let tx = tx.clone();
        let _ = std::thread::Builder::new().name("oc-fwd".into()).stack_size(64 * 1024).spawn(move || {
            while let Ok(m) = oc_rx.recv() {
                if tx.send(Msg::Oc(m)).is_err() {
                    break;
                }
            }
        });
    }

    let day = sys::day_number();
    let attempts = load_attempts(&cfg, day);
    let params = if cfg.relaxed { Params::relaxed(cfg.daily_cap) } else { Params::normal(cfg.daily_cap) };
    let traffic = load_traffic(&cfg, day);
    let guard = guard::Guard::new(&cfg, conf);
    let mut d = Daemon {
        m: Machine::new(params, attempts),
        cfg,
        tl,
        oc: None,
        tunnel: None,
        tunnel_mtime: None,
        dns: shared,
        isp,
        server_ips: Vec::new(),
        tx,
        oc_tx,
        day,
        traffic,
        counters: None,
        self_cpu: (0, 0),
        cpu_pct: 0.0,
        dirty: true,
        long_fallback_noted: false,
        bypass: Vec::new(),
        wan_up: net::has_default_route(),
        live_pending: None,
        started: sys::wall_secs(),
        guard,
        cn_ips_mtime: None,
        cn_ips: 0,
        cn_ips_warned: false,
        shaper: shaper::Shaper::new(),
        shape_failed: false,
    };
    d.load_cn_ips();
    let acts = d.m.start(sys::now_ms());
    d.apply(acts);
    d.main_loop(rx);
    d.shutdown();
    let _ = std::fs::remove_file(&pidf);
    0
}

use std::sync::Mutex;

fn load_attempts(cfg: &Config, day: u64) -> u32 {
    std::fs::read_to_string(cfg.run_dir.join("attempts"))
        .ok()
        .and_then(|s| {
            let (d, n) = s.trim().split_once(' ')?;
            (d.parse::<u64>().ok()? == day).then(|| n.parse().ok())?
        })
        .unwrap_or(0)
}

fn load_traffic(cfg: &Config, day: u64) -> (u64, u64) {
    std::fs::read_to_string(cfg.run_dir.join("traffic"))
        .ok()
        .and_then(|s| {
            let v: Vec<u64> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (v.len() == 3 && v[0] == day).then(|| (v[1], v[2]))
        })
        .unwrap_or((0, 0))
}

impl Daemon {
    fn main_loop(&mut self, rx: mpsc::Receiver<Msg>) {
        let mut last_tick = 0u64;
        let mut last_status = 0u64;
        let mut last_res = 0u64;
        let mut last_fw = sys::now_ms();
        let mut last_isp = sys::now_ms();
        loop {
            match rx.recv_timeout(Duration::from_millis(1000)) {
                Ok(m) => self.on_msg(m),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
            while let Ok(m) = rx.try_recv() {
                self.on_msg(m);
            }
            if sys::take(&sys::SIG_TERM) {
                break;
            }
            if sys::take(&sys::SIG_USR1) {
                let a = self.m.handle(Event::UserRetry, sys::now_ms());
                self.apply(a);
            }
            let now = sys::now_ms();
            if now.saturating_sub(last_tick) < 900 {
                continue;
            }
            last_tick = now;
            self.check_oc();
            self.shape(now);
            self.check_tunnel_file();
            let a = self.m.handle(Event::Tick, now);
            self.apply(a);
            let day = sys::day_number();
            if day != self.day {
                self.day = day;
                self.traffic = (0, 0);
                let a = self.m.new_day(now);
                self.apply(a);
            }
            if self.m.state == machine::State::Fallback && now.saturating_sub(self.m.since) >= 1_800_000 {
                if !self.long_fallback_noted {
                    self.long_fallback_noted = true;
                    let r = self.m.reason.clone();
                    self.notify(&format!("已直连超过 30 分钟（{}），全家网络正常但没走公司线路", r));
                }
            } else if self.m.state.tunneled() {
                self.long_fallback_noted = false;
            }
            for e in self.guard.tick(&self.cfg, now) {
                // 要马上知道的（有人猜密码、家里设备异常）才推送；一般扫描在网页和每日汇总里看
                if e.level == "alert" {
                    self.notify(&format!("安全提醒：{}", e.text));
                }
            }
            if now.saturating_sub(last_res) >= 5000 {
                let dt = now.saturating_sub(last_res);
                last_res = now;
                self.resources(dt);
            }
            if now.saturating_sub(last_fw) >= 60_000 {
                last_fw = now;
                if !net::fw_present(&self.cfg) || !net::rules_present(&self.cfg) {
                    self.tl.push(Level::Info, "防火墙或路由规则被固件重建过，已恢复本插件规则");
                    if let Err(e) = net::apply_fw(&self.cfg) {
                        report::log(&format!("恢复防火墙规则失败：{}", e));
                    }
                    net::apply_rules(&self.cfg);
                }
            }
            if now.saturating_sub(last_isp) >= 60_000 {
                last_isp = now;
                self.load_cn_ips();
                self.refresh_bypass();
                let isp = net::isp_dns(&self.cfg);
                if isp != self.isp {
                    self.isp = isp;
                    self.push_dns_view();
                }
            }
            if self.dirty || now.saturating_sub(last_status) >= 5000 {
                last_status = now;
                self.dirty = false;
                self.write_status();
            }
        }
    }

    fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Oc(oc::OcMsg::Line(l)) => {
                report::log(&format!("openconnect: {}", l));
                if let Some(o) = self.oc.as_mut() {
                    let before = o.dtls;
                    o.on_line(&l);
                    if before != o.dtls {
                        self.dirty = true;
                    }
                }
            }
            Msg::Probe { ok, rtt, detail } => {
                if !ok {
                    report::log(&format!("探测失败：{}", detail));
                }
                let a = self.m.handle(Event::Probe { ok, rtt_ms: rtt }, sys::now_ms());
                self.apply(a);
                self.dirty = true;
            }
            Msg::Note(l, s) => {
                self.tl.push(l, &s);
                self.dirty = true;
            }
        }
    }

    fn apply(&mut self, acts: Vec<Action>) {
        for a in acts {
            self.dirty = true;
            match a {
                Action::StartOc => self.start_oc(),
                Action::StopOc(why) => {
                    report::log(&format!("停止 openconnect：{}", why));
                    if let Some(o) = self.oc.as_mut() {
                        o.last_error = why;
                        o.stop();
                    }
                }
                Action::SetLive(on) => {
                    if net::set_live(on) {
                        self.live_pending = None;
                    } else {
                        self.live_pending = Some(on);
                        self.tl.push(Level::Err, "切换线路失败（ipset 执行出错），稍后自动重试");
                    }
                    report::log(if on { "总阀门：打开（新连接走公司线路）" } else { "总阀门：关闭（新连接直连）" });
                    self.push_dns_view();
                }
                Action::FlushDnsCache => net::dnsmasq_flush(&self.cfg),
                Action::Probe => self.probe(),
                Action::Note(l, s) => {
                    if l == Level::Err {
                        self.notify(&s);
                    }
                    self.tl.push(l, &s)
                }
            }
        }
        let _ = std::fs::write(self.cfg.run_dir.join("attempts"), format!("{} {}", self.day, self.m.attempts_today));
    }

    /// 异常通知（飞书 / Bark）：交给外壳脚本发，后台执行不阻塞
    fn notify(&self, text: &str) {
        if self.cfg.notify_cmd.trim().is_empty() {
            return;
        }
        // 文字作为独立参数传给外壳脚本，不经 shell 拼接
        let mut argv: Vec<String> = self.cfg.notify_cmd.split_whitespace().map(String::from).collect();
        argv.push(text.to_string());
        let _ = std::thread::Builder::new().name("notify".into()).stack_size(64 * 1024).spawn(move || {
            let args: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
            sys::run(&argv[0], &args);
        });
    }

    fn resolve_server(&mut self) -> Option<Ipv4Addr> {
        let host = self.cfg.server_host();
        if let Ok(ip) = host.parse::<Ipv4Addr>() {
            return Some(ip);
        }
        for s in self.isp.clone() {
            if let Ok((ips, _)) = dns::resolve_once(s, None, &host, Duration::from_millis(2000)) {
                if !ips.is_empty() {
                    return Some(ips[0]);
                }
            }
        }
        None
    }

    fn start_oc(&mut self) {
        let ip = self.resolve_server();
        if let Some(ip) = ip {
            if !self.server_ips.contains(&ip) {
                self.server_ips.push(ip);
                if self.server_ips.len() > 8 {
                    self.server_ips.remove(0);
                }
                self.refresh_bypass();
            }
        }
        report::log(&format!("连接公司线路（第 {} 次）", self.m.attempts_today));
        // 上一个 openconnect 若是被强杀的，它的断开钩子没跑，隧道信息文件还在；
        // 不清掉会被当成"新隧道已就绪"，连接超时就失效了
        let _ = std::fs::remove_file(self.cfg.tunnel_file());
        self.tunnel_mtime = None;
        match oc::Oc::spawn(&self.cfg, ip, self.oc_tx.clone()) {
            Ok(o) => self.oc = Some(o),
            Err(e) => {
                let a = self.m.handle(Event::OcExited { auth_failed: false, detail: e }, sys::now_ms());
                self.apply(a);
            }
        }
    }

    fn check_oc(&mut self) {
        let exited = match self.oc.as_mut() {
            Some(o) => o.poll_exit(),
            None => false,
        };
        if exited {
            let o = self.oc.take().unwrap();
            let detail = if o.last_error.is_empty() { "连接中断".to_string() } else { o.last_error.clone() };
            report::log(&format!("openconnect 已退出（{}）", detail));
            let _ = std::fs::remove_file(self.cfg.run_dir.join("openconnect.pid"));
            let _ = std::fs::remove_file(self.cfg.tunnel_file());
            self.tunnel = None;
            self.tunnel_mtime = None;
            self.counters = None;
            self.shaper.reset_counters();
            self.push_dns_view();
            let a = self.m.handle(Event::OcExited { auth_failed: o.auth_failed, detail }, sys::now_ms());
            self.apply(a);
        }
    }

    fn check_tunnel_file(&mut self) {
        if self.oc.is_none() {
            return;
        }
        let p = self.cfg.tunnel_file();
        let mt = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
        if mt == self.tunnel_mtime {
            return;
        }
        self.tunnel_mtime = mt;
        match parse_tunnel(&p) {
            Some(t) => {
                report::log(&format!("隧道就绪：{} 地址 {} MTU {} DNS {:?}", t.dev, t.ip, t.mtu, t.dns));
                // 新网卡：沿用学到的上限（线路能力一般不会因重连而变）
                self.shaper.reset_counters();
                if let Some(l) = self.shaper.limit.filter(|_| self.cfg.up_shaper) {
                    net::shape_set(&t.dev, l);
                }
                self.tunnel = Some(t);
                self.push_dns_view();
                let a = self.m.handle(Event::TunnelUp, sys::now_ms());
                self.apply(a);
            }
            None => {
                if mt.is_some() {
                    report::log("隧道网卡配置失败");
                }
            }
        }
    }

    fn company_dns(&self) -> Vec<Ipv4Addr> {
        if !self.cfg.company_dns_override.is_empty() {
            return self.cfg.company_dns_override.clone();
        }
        self.tunnel.as_ref().map(|t| t.dns.clone()).unwrap_or_default()
    }

    fn push_dns_view(&self) {
        let mut internal = self.cfg.internal_domains.clone();
        if let Some(t) = &self.tunnel {
            if !t.domain.is_empty() {
                internal.push(config::norm_domain(&t.domain));
            }
        }
        self.dns.set(dns::View {
            tunneled: self.m.live(),
            tun_dev: self.tunnel.as_ref().map(|t| t.dev.clone()),
            company: self.company_dns(),
            overseas: self.cfg.overseas_dns.clone(),
            isp: self.isp.clone(),
            internal,
        });
    }

    /// 经隧道问一次公司 DNS（没有公司 DNS 时问海外公共 DNS），在单独线程里做，不阻塞主循环
    fn probe(&mut self) {
        let Some(t) = self.tunnel.clone() else {
            let _ = self.tx.send(Msg::Probe { ok: false, rtt: 0, detail: "隧道不在".into() });
            return;
        };
        // 先问公司 DNS；它不应答时再经隧道问海外公共 DNS —— 只是公司 DNS 坏了不算线路坏了
        let company_name = if !self.cfg.probe_domain.is_empty() {
            self.cfg.probe_domain.clone()
        } else if !t.domain.is_empty() {
            t.domain.clone()
        } else {
            self.cfg.server_host()
        };
        let overseas_name = if self.cfg.probe_domain.is_empty() { "www.google.com".to_string() } else { self.cfg.probe_domain.clone() };
        let mut targets: Vec<(Ipv4Addr, String)> = self.company_dns().into_iter().map(|s| (s, company_name.clone())).collect();
        targets.extend(self.cfg.overseas_dns.iter().take(1).map(|s| (*s, overseas_name.clone())));
        let tx = self.tx.clone();
        let fail_tx = self.tx.clone();
        let spawned = std::thread::Builder::new().name("probe".into()).stack_size(64 * 1024).spawn(move || {
            let mut detail = String::new();
            for (s, name) in targets {
                match dns::resolve_once(s, Some(&t.dev), &name, Duration::from_millis(2500)) {
                    Ok((_, rtt)) => {
                        let _ = tx.send(Msg::Probe { ok: true, rtt, detail: String::new() });
                        return;
                    }
                    Err(e) => detail = format!("{} → {}", s, e),
                }
            }
            let _ = tx.send(Msg::Probe { ok: false, rtt: 0, detail });
        });
        if spawned.is_err() {
            // 线程起不来也要给状态机一个结果，否则"探测中"标记一直挂着，之后再也不探测
            let _ = fail_tx.send(Msg::Probe { ok: false, rtt: 0, detail: "探测线程启动失败".into() });
        }
    }

    /// 国内直连开启时，把中国 IP 段装进内核；文件没变不重装，文件坏了保留已装的那份
    fn load_cn_ips(&mut self) {
        if !self.cfg.cn_direct {
            return;
        }
        let m = std::fs::metadata(&self.cfg.cn_ip_list).and_then(|x| x.modified()).ok();
        if m.is_some() && m == self.cn_ips_mtime {
            return;
        }
        let nets = net::parse_cn_ips(&std::fs::read_to_string(&self.cfg.cn_ip_list).unwrap_or_default());
        if nets.len() < 1000 {
            // 名单缺失或不完整：已装进内核的那份照用；一份都没有时在时间线说一次，不每分钟刷
            if self.cn_ips == 0 && !self.cn_ips_warned {
                self.cn_ips_warned = true;
                self.tl.push(Level::Warn, "没有找到中国 IP 段名单，国内 App 直接连 IP 时仍走公司线路（国内网站照常直连）");
            }
            self.cn_ips_mtime = m;
            return;
        }
        if net::set_cn_ips(&nets) {
            report::log(&format!("中国 IP 段 {} 条", nets.len()));
            self.cn_ips = nets.len();
            self.cn_ips_mtime = m;
        }
    }

    /// 每秒看一眼隧道上行有没有堵住（读两个 /proc 小文件），需要时调整限速
    fn shape(&mut self, now: u64) {
        if !self.cfg.up_shaper {
            return;
        }
        let (Some(t), Some(o)) = (&self.tunnel, &self.oc) else { return };
        let Some(tx) = sys::if_counter(&t.dev, "tx_bytes") else { return };
        let mbit = |k: u32| format!("{:.1} Mbit/s", k as f64 / 1000.0);
        match self.shaper.observe(now, tx, net::oc_sendq(o.pid)) {
            Some(shaper::Change::Set(l, first)) => {
                if !net::shape_set(&t.dev, l) {
                    // 没装上就当没限：下次占满再试，也让"开始限速"的提醒在真正生效时才出现；失败只记一次
                    self.shaper.limit = None;
                    if !self.shape_failed {
                        self.shape_failed = true;
                        report::log("设置公司线路上传限速失败（tc 不可用？）");
                    }
                    return;
                }
                let (cap, lim) = (mbit(self.shaper.cap), mbit(l));
                if first {
                    self.tl.push(Level::Info, &format!("有设备在往公司线路大量上传，把线路占满了（实际约 {}）：已把上传限到 {}、各设备轮流发，其它设备不再被拖慢", cap, lim));
                    self.dirty = true;
                }
                report::log(&format!("公司线路上传占满（实际约 {}），限到 {}", cap, lim));
            }
            None => {}
            Some(shaper::Change::Off) => {
                net::shape_off(&t.dev);
                report::log("公司线路上传不再限速");
            }
        }
    }

    fn refresh_bypass(&mut self) {
        let nets = net::bypass_nets(&self.cfg, &self.server_ips);
        if nets != self.bypass && net::set_bypass(&nets) {
            self.bypass = nets;
        }
    }

    fn resources(&mut self, dt: u64) {
        let wan = net::has_default_route();
        if wan != self.wan_up {
            self.wan_up = wan;
            report::log(if wan { "宽带默认路由恢复" } else { "宽带默认路由消失" });
            if wan {
                self.refresh_bypass();
                self.isp = net::isp_dns(&self.cfg);
                self.push_dns_view();
                // 重拨后源地址可能变了：上游列表没变也要重建 DNS 套接字
                self.dns.bump();
                let a = self.m.handle(Event::WanBack, sys::now_ms());
                self.apply(a);
            }
        }
        // 反向路径过滤被固件改回会让隧道回包被丢，探测随即失败：每 5 秒看一眼（只读两个小文件）
        if net::ensure_rp_filter(&self.cfg) {
            report::log("反向路径过滤被改回，已重新关闭");
        }
        if let Some(on) = self.live_pending {
            if net::set_live(on) {
                self.live_pending = None;
                report::log("总阀门重试切换成功");
            }
        }
        let why = self.oc.as_mut().and_then(|o| o.resource_check(dt.max(1)));
        if let Some(w) = why {
            self.tl.push(Level::Warn, &format!("{}，已重启它", w));
            let a = self.m.handle(Event::OcUnhealthy(w), sys::now_ms());
            self.apply(a);
        }
        if let Some(t) = &self.tunnel {
            if let (Some(rx), Some(tx)) = (sys::if_counter(&t.dev, "rx_bytes"), sys::if_counter(&t.dev, "tx_bytes")) {
                if let Some((prx, ptx)) = self.counters {
                    self.traffic.0 += rx.saturating_sub(prx);
                    self.traffic.1 += tx.saturating_sub(ptx);
                }
                self.counters = Some((rx, tx));
                let _ = std::fs::write(self.cfg.run_dir.join("traffic"), format!("{} {} {}", self.day, self.traffic.0, self.traffic.1));
            }
        }
        let me = std::process::id() as i32;
        if let Some(t) = sys::cpu_ticks(me) {
            let now = sys::now_ms();
            if self.self_cpu.1 > 0 {
                let used = t.saturating_sub(self.self_cpu.0) as f64 / sys::clk_tck() as f64;
                self.cpu_pct = used * 100.0 / (now.saturating_sub(self.self_cpu.1).max(1) as f64 / 1000.0);
            }
            self.self_cpu = (t, now);
        }
    }

    fn write_status(&self) {
        let now_ms = sys::now_ms();
        let wall = sys::wall_secs();
        let to_wall = |mono: u64| wall.saturating_sub(now_ms.saturating_sub(mono) / 1000);
        let h = self.dns.health.lock().unwrap().clone();
        let oc = self.oc.as_ref();
        let tunnel = match &self.tunnel {
            Some(t) => Json::obj()
                .b("up", true)
                .s("ip", &t.ip)
                .s("dev", &t.dev)
                .s("mtu", &t.mtu)
                .s("gateway", &t.gateway)
                .s("domain", &t.domain)
                .ob("dtls", oc.and_then(|o| o.dtls))
                .n("up_limit_kbit", self.shaper.limit.unwrap_or(0) as u64)
                .n("up_since", t.up_since),
            None => Json::obj().b("up", false),
        };
        let next_in = match (self.m.oc, self.m.next_attempt) {
            (OcState::Down, Some(t)) => t.saturating_sub(now_ms) / 1000,
            _ => 0,
        };
        let s = Json::obj()
            .n("v", 1)
            .s("version", VERSION)
            .s("state", self.m.state.key())
            .n("since", to_wall(self.m.since))
            .s("reason", &self.m.reason)
            .b("live", self.m.live())
            .s("line", if self.m.state.tunneled() { "corp" } else { "direct" })
            .o("tunnel", tunnel)
            .n("started", self.started)
            .raw("rtt_ms", self.m.last_rtt.map(|x| x.to_string()).unwrap_or_else(|| "null".into()))
            .o("traffic_today", Json::obj().n("rx", self.traffic.0).n("tx", self.traffic.1))
            .o(
                "dns",
                Json::obj()
                    .ob("company", h.company)
                    .ob("overseas", h.overseas)
                    .ob("isp", h.isp)
                    .n("queries", h.queries)
                    .s("mode", if self.m.live() { "company" } else { "isp" }),
            )
            .o(
                "retry",
                Json::obj()
                    .n("attempts_today", self.m.attempts_today)
                    .n("daily_cap", self.cfg.daily_cap)
                    .n("next_in", next_in)
                    .b("connecting", matches!(self.m.oc, OcState::Starting(_))),
            )
            .o(
                "res",
                Json::obj()
                    .n("mem_kb", sys::rss_kb(std::process::id() as i32).unwrap_or(0))
                    .n("oc_mem_kb", oc.and_then(|o| sys::rss_kb(o.pid)).unwrap_or(0))
                    .n("cpu_pct", format!("{:.1}", self.cpu_pct)),
            )
            .b("cn_direct", self.cfg.cn_direct)
            .n("cn_ips", self.cn_ips)
            .n("updated", wall)
            .raw("events", report::events_json(&self.tl.items, 30))
            .done();
        let _ = sys::write_atomic(&self.cfg.status_dir.join("corpline_status.json"), s.as_bytes());
    }

    fn shutdown(&mut self) {
        report::log("corpline 停止：撤掉网络改动");
        net::set_live(false);
        if let Some(mut o) = self.oc.take() {
            o.wait_blocking(5000);
        }
        net::dnsmasq_remove(&self.cfg);
        net::cleanup(&self.cfg);
        self.guard.stopped(&self.cfg);
        let _ = std::fs::remove_file(self.cfg.tunnel_file());
        let _ = std::fs::remove_file(self.cfg.run_dir.join("openconnect.pid"));
        let _ = std::fs::remove_file(self.cfg.status_dir.join("corpline_domains.json"));
        let s = Json::obj().n("v", 1).s("version", VERSION).s("state", "off").n("updated", sys::wall_secs()).raw("events", report::events_json(&self.tl.items, 30)).done();
        let _ = sys::write_atomic(&self.cfg.status_dir.join("corpline_status.json"), s.as_bytes());
    }
}
