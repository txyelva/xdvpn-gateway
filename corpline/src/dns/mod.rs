//! 内置 DNS 转发（127.0.0.1:5354，dnsmasq 的上游）。单线程 poll 循环，无缓存（缓存交给 dnsmasq）。
//!
//! - 隧道在用时：普通域名问公司 DNS，300 ms 没回就并行问海外公共 DNS；例外名单（Apple）反过来；
//!   公司内网域名只问公司 DNS。发往隧道上游的套接字绑定隧道网卡，绝不从宽带发出。
//! - 直连期间：只问运营商 DNS，并把 TTL 压到 30 s，切回公司线路后旧解析很快过期。
//! - 国内网站直连开启时：中国服务名单问运营商 DNS，应答 IP 写入 ipset 后再回给客户端。

pub mod domains;
pub mod msg;
pub mod plan;
pub mod recent;

use domains::SuffixSet;
use plan::{Class, Kind, Tier, Verdict};
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::machine::Level;
use crate::sys;

const QUERY_DEADLINE_MS: u64 = 2500;
const UPSTREAM_TIMEOUT_MS: u64 = 1500;
const FALLBACK_TTL_CAP: u32 = 30;
const MAX_PENDING: usize = 2048;
const MAX_TCP: usize = 16;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub tunneled: bool,
    pub tun_dev: Option<String>,
    pub company: Vec<Ipv4Addr>,
    pub overseas: Vec<Ipv4Addr>,
    pub isp: Vec<Ipv4Addr>,
    pub internal: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Health {
    pub company: Option<bool>,
    pub overseas: Option<bool>,
    pub isp: Option<bool>,
    pub queries: u64,
}

pub type Notify = Box<dyn Fn(Level, String) + Send + Sync>;

pub struct Shared {
    view: Mutex<View>,
    gen: AtomicU64,
    pub health: Mutex<Health>,
}

impl Shared {
    pub fn new(v: View) -> Arc<Shared> {
        Arc::new(Shared { view: Mutex::new(v), gen: AtomicU64::new(1), health: Mutex::new(Health::default()) })
    }
    pub fn set(&self, v: View) {
        let mut cur = self.view.lock().unwrap();
        if *cur != v {
            *cur = v;
            self.gen.fetch_add(1, Ordering::SeqCst);
        }
    }
    /// 上游不变也强制重建套接字（宽带重拨换了地址后，旧套接字绑着已经不存在的源地址）
    pub fn bump(&self) {
        self.gen.fetch_add(1, Ordering::SeqCst);
    }
    pub fn get(&self) -> View {
        self.view.lock().unwrap().clone()
    }
}

pub struct Static {
    pub overseas: SuffixSet,
    /// 用户单独设为直连的网站
    pub direct: SuffixSet,
    /// 最近访问域名的输出文件（内存盘）
    pub recent_file: Option<std::path::PathBuf>,
    pub cn: Option<SuffixSet>,
    pub filter_aaaa: bool,
    pub cn_ipset: String,
}

struct Up {
    kind: Kind,
    addr: Ipv4Addr,
    dev: Option<String>,
    sock: Option<UdpSocket>,
    /// 套接字发送出错（如源地址已失效）或没建起来时，最早何时再重建
    reopen_at: u64,
    healthy: bool,
    fails: u32,
    oks: u32,
    out: HashMap<u16, u64>,
}

impl Up {
    fn ok(&mut self) -> Option<bool> {
        self.fails = 0;
        self.oks += 1;
        if !self.healthy && self.oks >= 3 {
            self.healthy = true;
            return Some(true);
        }
        None
    }
    fn fail(&mut self) -> Option<bool> {
        self.oks = 0;
        self.fails += 1;
        if self.healthy && self.fails >= 3 {
            self.healthy = false;
            return Some(false);
        }
        None
    }
}

struct Pending {
    client: SocketAddr,
    orig_id: u16,
    query: Vec<u8>,
    qname: String,
    qtype: u16,
    class: Class,
    tunneled: bool,
    tiers: Vec<Tier>,
    next_tier: usize,
    start: u64,
    waiting: Vec<usize>,
    held: Option<Vec<u8>>,
    isp_tried: bool,
}

struct Job {
    ips: Vec<(Ipv4Addr, u32)>,
    resp: Vec<u8>,
    client: SocketAddr,
}

pub struct Engine {
    shared: Arc<Shared>,
    st: Arc<Static>,
    notify: Arc<Notify>,
    udp: UdpSocket,
    tcp: TcpListener,
    ups: Vec<Up>,
    internal: SuffixSet,
    view: View,
    gen: u64,
    pending: HashMap<u16, Pending>,
    rng: u32,
    ipset_tx: mpsc::Sender<Job>,
    tcp_busy: Arc<AtomicUsize>,
    kind_health: HashMap<Kind, bool>,
    recent: recent::Recent,
}

impl Engine {
    pub fn bind(listen: SocketAddrV4, shared: Arc<Shared>, st: Static, notify: Notify) -> std::io::Result<Engine> {
        let udp = UdpSocket::bind(listen)?;
        udp.set_nonblocking(true)?;
        let tcp = TcpListener::bind(listen)?;
        tcp.set_nonblocking(true)?;
        let st = Arc::new(st);
        let (tx, rx) = mpsc::channel::<Job>();
        let out = udp.try_clone()?;
        let set = st.cn_ipset.clone();
        let recent = recent::Recent::new(st.recent_file.clone());
        std::thread::Builder::new().name("ipset".into()).stack_size(64 * 1024).spawn(move || ipset_worker(rx, out, set))?;
        Ok(Engine {
            shared,
            st,
            notify: Arc::new(notify),
            udp,
            tcp,
            ups: Vec::new(),
            internal: SuffixSet::empty(),
            view: View::default(),
            gen: 0,
            pending: HashMap::new(),
            rng: seed(),
            ipset_tx: tx,
            tcp_busy: Arc::new(AtomicUsize::new(0)),
            kind_health: HashMap::new(),
            recent,
        })
    }

    fn rebuild(&mut self) {
        let v = self.shared.get();
        let mut old: HashMap<(Kind, Ipv4Addr), (bool, u32, u32)> =
            self.ups.iter().map(|u| ((u.kind, u.addr), (u.healthy, u.fails, u.oks))).collect();
        let mut ups = Vec::new();
        let tun = v.tun_dev.as_deref();
        let mut add = |kind: Kind, list: &[Ipv4Addr]| {
            for &a in list {
                if ups.iter().any(|u: &Up| u.kind == kind && u.addr == a) {
                    continue;
                }
                let dev = if kind.via_tunnel() { tun } else { None };
                let sock = if kind.via_tunnel() && dev.is_none() {
                    None
                } else {
                    sys::udp_connected(SocketAddrV4::new(a, 53), dev, true).ok()
                };
                let (healthy, fails, oks) = old.remove(&(kind, a)).unwrap_or((true, 0, 0));
                ups.push(Up { kind, addr: a, dev: dev.map(String::from), sock, reopen_at: 0, healthy, fails, oks, out: HashMap::new() });
            }
        };
        add(Kind::Company, &v.company);
        add(Kind::Overseas, &v.overseas);
        add(Kind::Isp, &v.isp);
        self.ups = ups;
        self.internal = SuffixSet::new(v.internal.clone());
        // 换了上游，旧的等待中的查询按各自截止时间自然收尾
        for p in self.pending.values_mut() {
            p.waiting.clear();
        }
        self.view = v;
        self.publish_health();
    }

    fn publish_health(&mut self) {
        let mut h = self.shared.health.lock().unwrap();
        let k = |kind: Kind| {
            let v: Vec<&Up> = self.ups.iter().filter(|u| u.kind == kind && u.sock.is_some()).collect();
            if v.is_empty() {
                None
            } else {
                Some(v.iter().any(|u| u.healthy))
            }
        };
        h.company = k(Kind::Company);
        h.overseas = k(Kind::Overseas);
        h.isp = k(Kind::Isp);
    }

    fn health_changed(&mut self, kind: Kind) {
        let now_ok = self.ups.iter().filter(|u| u.kind == kind && u.sock.is_some()).any(|u| u.healthy);
        let prev = self.kind_health.insert(kind, now_ok).unwrap_or(true);
        self.publish_health();
        if prev == now_ok {
            return;
        }
        let relevant = match kind {
            Kind::Isp => !self.view.tunneled || self.st.cn.is_some(),
            _ => self.view.tunneled,
        };
        if !relevant {
            return;
        }
        let msg = match (kind, now_ok) {
            (Kind::Company, false) => (Level::Warn, "公司 DNS 无响应，已改用备用 DNS（海外公共 DNS，经公司线路）".to_string()),
            (Kind::Company, true) => (Level::Ok, "公司 DNS 恢复正常".to_string()),
            (Kind::Overseas, false) => (Level::Info, "海外公共 DNS 无响应，Apple 等例外改问公司 DNS".to_string()),
            (Kind::Overseas, true) => (Level::Ok, "海外公共 DNS 恢复正常".to_string()),
            (Kind::Isp, false) => (Level::Warn, "运营商 DNS 无响应".to_string()),
            (Kind::Isp, true) => (Level::Ok, "运营商 DNS 恢复正常".to_string()),
        };
        (self.notify)(msg.0, msg.1);
    }

    /// 随机查询 ID（不可预测，配合各上游已 connect 的套接字，抵御伪造应答）
    fn alloc_id(&mut self) -> u16 {
        loop {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 17;
            self.rng ^= self.rng << 5;
            let id = (self.rng >> 8) as u16;
            if !self.pending.contains_key(&id) {
                return id;
            }
        }
    }

    /// 重建一个上游套接字（限频）：宽带重拨换了地址、或当初没建起来时用
    fn reopen(&mut self, i: usize, now: u64) {
        let u = &mut self.ups[i];
        if now < u.reopen_at || (u.kind.via_tunnel() && u.dev.is_none()) {
            return;
        }
        u.reopen_at = now + 2000;
        // 旧套接字上还没回的查询留在 out 里，按超时计失败、照常换下一个上游
        u.sock = sys::udp_connected(SocketAddrV4::new(u.addr, 53), u.dev.as_deref(), true).ok();
    }

    pub fn run(mut self) {
        let mut buf = vec![0u8; 4096];
        loop {
            let g = self.shared.gen.load(Ordering::SeqCst);
            if g != self.gen {
                self.gen = g;
                self.rebuild();
            }
            let now = sys::now_ms();
            let timeout = self.next_deadline(now).map(|d| d.saturating_sub(now).max(1) as i32).unwrap_or(500).min(500);
            let mut fds = vec![sys::raw(&self.udp), sys::raw(&self.tcp)];
            let mut map = Vec::new();
            for (i, u) in self.ups.iter().enumerate() {
                if let Some(s) = &u.sock {
                    fds.push(sys::raw(s));
                    map.push(i);
                }
            }
            let ready = match sys::poll_readable(&fds, timeout) {
                Ok(r) => r,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
            };
            if ready[0] {
                for _ in 0..64 {
                    match self.udp.recv_from(&mut buf) {
                        Ok((n, from)) => {
                            let q = buf[..n].to_vec();
                            self.on_query(q, from);
                        }
                        Err(_) => break,
                    }
                }
            }
            if ready[1] {
                self.accept_tcp();
            }
            self.recent.flush(sys::now_ms());
            for (k, &i) in map.iter().enumerate() {
                if !ready[k + 2] {
                    continue;
                }
                for _ in 0..64 {
                    let r = match &self.ups[i].sock {
                        Some(s) => s.recv(&mut buf),
                        None => break,
                    };
                    match r {
                        Ok(n) => {
                            let resp = buf[..n].to_vec();
                            self.on_reply(i, resp);
                        }
                        Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                        Err(_) => {
                            // ICMP 不可达等：算一次失败（connected 套接字才能收到这种错误）
                            if self.ups[i].fail().is_some() {
                                let kind = self.ups[i].kind;
                                self.health_changed(kind);
                            }
                            break;
                        }
                    }
                }
            }
            self.on_timers(sys::now_ms());
        }
    }

    fn next_deadline(&self, now: u64) -> Option<u64> {
        let mut d: Option<u64> = None;
        let mut take = |t: u64| d = Some(d.map_or(t, |x: u64| x.min(t)));
        for p in self.pending.values() {
            take(p.start + QUERY_DEADLINE_MS);
            if let Some(t) = p.tiers.get(p.next_tier) {
                take(p.start + t.delay_ms);
            }
        }
        for u in &self.ups {
            if let Some(&t) = u.out.values().min() {
                take(t + UPSTREAM_TIMEOUT_MS);
            }
        }
        d.map(|x| x.max(now))
    }

    fn on_query(&mut self, mut q: Vec<u8>, client: SocketAddr) {
        let question = match msg::question(&q) {
            Some(x) if !msg::is_response(&q) => x,
            _ => return,
        };
        self.shared.health.lock().unwrap().queries += 1;
        if self.st.filter_aaaa && question.qtype == msg::TYPE_AAAA {
            // V1 不走 IPv6：直接回空，设备马上用 IPv4
            if let Some(r) = msg::reply_empty(&q, 0) {
                let _ = self.udp.send_to(&r, client);
            }
            return;
        }
        if self.pending.len() >= MAX_PENDING {
            if let Some(r) = msg::reply_empty(&q, msg::RCODE_SERVFAIL) {
                let _ = self.udp.send_to(&r, client);
            }
            return;
        }
        let tunneled = self.view.tunneled;
        let class = plan::classify(&question.name, &self.internal, &self.st.direct, &self.st.overseas, self.st.cn.as_ref());
        self.recent.record(&question.name, class, sys::wall_secs());
        let mut tiers = plan::plan(class, tunneled);
        if tunneled && !self.ups.iter().any(|u| u.kind.via_tunnel() && u.sock.is_some()) {
            // 隧道刚断、状态还没切过来：直接问运营商
            tiers = plan::plan(class, false);
        }
        let orig_id = msg::id(&q).unwrap_or(0);
        let id = self.alloc_id();
        msg::set_id(&mut q, id);
        let now = sys::now_ms();
        self.pending.insert(
            id,
            Pending {
                client,
                orig_id,
                query: q,
                qname: question.name,
                qtype: question.qtype,
                class,
                tunneled,
                tiers,
                next_tier: 0,
                start: now,
                waiting: Vec::new(),
                held: None,
                isp_tried: false,
            },
        );
        self.advance(id, now);
    }

    /// 发出所有已到时间的层级；某层全部不健康或发不出去时立即发下一层
    fn advance(&mut self, id: u16, now: u64) {
        let mut force = false;
        loop {
            let (kind, due) = match self.pending.get(&id) {
                Some(p) => match p.tiers.get(p.next_tier) {
                    Some(t) => (t.kind, force || p.start + t.delay_ms <= now || p.waiting.is_empty()),
                    None => break,
                },
                None => return,
            };
            if !due {
                break;
            }
            let (sent, any_healthy) = self.send_kind(id, kind, now);
            if let Some(p) = self.pending.get_mut(&id) {
                p.next_tier += 1;
                if kind == Kind::Isp {
                    p.isp_tried = true;
                }
            }
            // 这一层的上游全都不健康：不等 300 ms，立刻并行问下一层
            force = sent > 0 && !any_healthy;
        }
        self.maybe_exhausted(id);
    }

    fn send_kind(&mut self, id: u16, kind: Kind, now: u64) -> (usize, bool) {
        let q = match self.pending.get(&id) {
            Some(p) => p.query.clone(),
            None => return (0, false),
        };
        let mut sent = 0;
        let mut healthy = false;
        let mut changed = false;
        let mut idx = Vec::new();
        let mut reopen = Vec::new();
        for (i, u) in self.ups.iter_mut().enumerate() {
            if u.kind != kind {
                continue;
            }
            let ok = match &u.sock {
                Some(s) => s.send(&q).is_ok(),
                None => false,
            };
            if !ok {
                reopen.push(i);
            }
            if ok {
                u.out.insert(id, now);
                sent += 1;
                healthy |= u.healthy;
                idx.push(i);
            } else if u.sock.is_some() && u.fail().is_some() {
                changed = true;
            }
        }
        if changed {
            self.health_changed(kind);
        }
        for i in reopen {
            self.reopen(i, now);
        }
        if let Some(p) = self.pending.get_mut(&id) {
            p.waiting.extend(idx);
        }
        (sent, healthy)
    }

    /// 没有在等的上游、也没有剩余层级：用留着的答案，或隧道全失败时紧急改问运营商，否则回 SERVFAIL
    fn maybe_exhausted(&mut self, id: u16) {
        let (done, held, need_isp) = match self.pending.get(&id) {
            Some(p) => (
                p.waiting.is_empty() && p.next_tier >= p.tiers.len(),
                p.held.is_some(),
                p.tunneled && !p.isp_tried && p.class != Class::Internal,
            ),
            None => return,
        };
        if !done {
            return;
        }
        if held {
            let p = self.pending.remove(&id).unwrap();
            let r = p.held.clone().unwrap();
            self.finish(p, r, Kind::Overseas);
            return;
        }
        let tunnel_dead = !self.ups.iter().any(|u| u.kind.via_tunnel() && u.sock.is_some() && u.healthy);
        if need_isp && tunnel_dead {
            if let Some(p) = self.pending.get_mut(&id) {
                p.tiers.push(Tier { kind: Kind::Isp, delay_ms: 0 });
            }
            let now = sys::now_ms();
            self.advance(id, now);
            return;
        }
        let p = self.pending.remove(&id).unwrap();
        self.reply_fail(&p);
    }

    fn reply_fail(&self, p: &Pending) {
        let mut q = p.query.clone();
        msg::set_id(&mut q, p.orig_id);
        if let Some(r) = msg::reply_empty(&q, msg::RCODE_SERVFAIL) {
            let _ = self.udp.send_to(&r, p.client);
        }
    }

    fn on_reply(&mut self, ui: usize, resp: Vec<u8>) {
        let id = match msg::id(&resp) {
            Some(x) if msg::is_response(&resp) => x,
            _ => return,
        };
        if !self.ups[ui].out.contains_key(&id) {
            return; // 迟到、重复或伪造
        }
        let kind = self.ups[ui].kind;
        let rc = msg::rcode(&resp);
        // 先核对问题再占用这个等待位：一个对不上的包不能顶掉随后到的真应答
        let valid_q = match (self.pending.get(&id), msg::question(&resp)) {
            (Some(p), Some(q)) => q.name == p.qname && q.qtype == p.qtype,
            (_, None) => rc != 0, // 不带问题的错误应答（如 FORMERR）：算这个上游失败
            (None, _) => true,
        };
        if !valid_q {
            return;
        }
        self.ups[ui].out.remove(&id);
        let bad = rc == msg::RCODE_SERVFAIL || rc == 5 || msg::question(&resp).is_none();
        let health = if bad { self.ups[ui].fail() } else { self.ups[ui].ok() };
        if health.is_some() {
            self.health_changed(kind);
        }
        let (class, tunneled, company_pending) = match self.pending.get_mut(&id) {
            Some(p) => {
                p.waiting.retain(|&w| w != ui);
                let cp = p.waiting.iter().any(|&w| self.ups.get(w).map(|u| u.kind) == Some(Kind::Company))
                    || p.tiers[p.next_tier..].iter().any(|t| t.kind == Kind::Company);
                (p.class, p.tunneled, cp)
            }
            None => return,
        };
        let company_pending = company_pending && self.ups.iter().any(|u| u.kind == Kind::Company && u.sock.is_some() && u.healthy);
        if msg::question(&resp).is_none() {
            self.advance(id, sys::now_ms());
            return;
        }
        match plan::judge(class, tunneled, kind, rc, msg::ancount(&resp), company_pending) {
            Verdict::Accept => {
                let p = self.pending.remove(&id).unwrap();
                self.finish(p, resp, kind);
            }
            Verdict::Hold => {
                if let Some(p) = self.pending.get_mut(&id) {
                    p.held = Some(resp);
                }
                self.advance(id, sys::now_ms());
            }
            Verdict::Fail => self.advance(id, sys::now_ms()),
        }
    }

    fn finish(&mut self, p: Pending, mut resp: Vec<u8>, from: Kind) {
        msg::set_id(&mut resp, p.orig_id);
        // 直连期间、以及隧道全挂时紧急改问运营商得到的答案，都可能被污染：TTL 压短，恢复后很快过期
        if !p.tunneled || (from == Kind::Isp && p.class != Class::Cn) {
            msg::cap_ttl(&mut resp, FALLBACK_TTL_CAP);
        }
        if p.tunneled && p.class == Class::Cn && from == Kind::Isp {
            let ips = msg::a_records(&resp);
            if !ips.is_empty() {
                let _ = self.ipset_tx.send(Job { ips, resp, client: p.client });
                return;
            }
        }
        let _ = self.udp.send_to(&resp, p.client);
    }

    fn on_timers(&mut self, now: u64) {
        for i in 0..self.ups.len() {
            if self.ups[i].sock.is_none() {
                self.reopen(i, now);
            }
        }
        let mut changed = Vec::new();
        for u in self.ups.iter_mut() {
            let late: Vec<u16> = u.out.iter().filter(|(_, &t)| now >= t + UPSTREAM_TIMEOUT_MS).map(|(&i, _)| i).collect();
            for i in late {
                u.out.remove(&i);
                if u.fail().is_some() {
                    changed.push(u.kind);
                }
            }
        }
        changed.dedup();
        for k in changed {
            self.health_changed(k);
        }
        let ids: Vec<u16> = self.pending.keys().copied().collect();
        for id in ids {
            let expired = self.pending.get(&id).map(|p| now >= p.start + QUERY_DEADLINE_MS).unwrap_or(false);
            if expired {
                let p = self.pending.remove(&id).unwrap();
                if let Some(h) = p.held.clone() {
                    self.finish(p, h, Kind::Overseas);
                } else {
                    self.reply_fail(&p);
                }
                continue;
            }
            // 等待中的上游已超时（不再在 out 表里）：从 waiting 里去掉
            if let Some(p) = self.pending.get_mut(&id) {
                let ups = &self.ups;
                p.waiting.retain(|&w| ups.get(w).map(|u| u.out.contains_key(&id)).unwrap_or(false));
            }
            self.advance(id, now);
        }
    }

    fn accept_tcp(&mut self) {
        loop {
            match self.tcp.accept() {
                Ok((s, _)) => {
                    if self.tcp_busy.load(Ordering::SeqCst) >= MAX_TCP {
                        continue; // 直接关闭，dnsmasq 会改问下一个
                    }
                    self.tcp_busy.fetch_add(1, Ordering::SeqCst);
                    let busy = self.tcp_busy.clone();
                    let ctx = TcpCtx {
                        view: self.view.clone(),
                        internal: SuffixSet::new(self.view.internal.clone()),
                        st: self.st.clone(),
                        healthy: self.ups.iter().map(|u| ((u.kind, u.addr), u.healthy)).collect(),
                    };
                    let r = std::thread::Builder::new().name("dns-tcp".into()).stack_size(128 * 1024).spawn(move || {
                        let _ = serve_tcp(s, &ctx);
                        busy.fetch_sub(1, Ordering::SeqCst);
                    });
                    if r.is_err() {
                        self.tcp_busy.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                Err(_) => break,
            }
        }
    }
}

struct TcpCtx {
    view: View,
    internal: SuffixSet,
    st: Arc<Static>,
    healthy: HashMap<(Kind, Ipv4Addr), bool>,
}

fn read_frame(s: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut l = [0u8; 2];
    s.read_exact(&mut l)?;
    let n = u16::from_be_bytes(l) as usize;
    let mut b = vec![0u8; n];
    s.read_exact(&mut b)?;
    Ok(b)
}

fn write_frame(s: &mut TcpStream, b: &[u8]) -> std::io::Result<()> {
    let mut out = (b.len() as u16).to_be_bytes().to_vec();
    out.extend_from_slice(b);
    s.write_all(&out)
}

fn serve_tcp(mut client: TcpStream, ctx: &TcpCtx) -> std::io::Result<()> {
    client.set_nonblocking(false)?;
    client.set_read_timeout(Some(Duration::from_secs(3)))?;
    client.set_write_timeout(Some(Duration::from_secs(3)))?;
    let q = read_frame(&mut client)?;
    let question = msg::question(&q).ok_or(ErrorKind::InvalidData)?;
    if ctx.st.filter_aaaa && question.qtype == msg::TYPE_AAAA {
        return write_frame(&mut client, &msg::reply_empty(&q, 0).unwrap_or_default());
    }
    let class = plan::classify(&question.name, &ctx.internal, &ctx.st.direct, &ctx.st.overseas, ctx.st.cn.as_ref());
    let v = &ctx.view;
    let mut tiers = plan::plan(class, v.tunneled);
    if v.tunneled && v.tun_dev.is_none() {
        tiers = plan::plan(class, false);
    }
    let mut held: Option<Vec<u8>> = None;
    for t in &tiers {
        let list = match t.kind {
            Kind::Company => &v.company,
            Kind::Overseas => &v.overseas,
            Kind::Isp => &v.isp,
        };
        let mut order: Vec<&Ipv4Addr> = list.iter().collect();
        order.sort_by_key(|a| !ctx.healthy.get(&(t.kind, **a)).copied().unwrap_or(true));
        for a in order {
            let dev = if t.kind.via_tunnel() { v.tun_dev.as_deref() } else { None };
            let Ok(mut up) = sys::tcp_connect(SocketAddrV4::new(*a, 53), dev, Duration::from_millis(1500)) else { continue };
            if write_frame(&mut up, &q).is_err() {
                continue;
            }
            let Ok(mut r) = read_frame(&mut up) else { continue };
            let company_pending = t.kind != Kind::Company && tiers.iter().any(|x| x.kind == Kind::Company);
            match plan::judge(class, v.tunneled, t.kind, msg::rcode(&r), msg::ancount(&r), company_pending) {
                Verdict::Accept => {
                    if !v.tunneled {
                        msg::cap_ttl(&mut r, FALLBACK_TTL_CAP);
                    }
                    if v.tunneled && class == Class::Cn && t.kind == Kind::Isp {
                        ipset_add(&ctx.st.cn_ipset, &msg::a_records(&r), &mut HashMap::new());
                    }
                    return write_frame(&mut client, &r);
                }
                Verdict::Hold => held = Some(r),
                Verdict::Fail => {}
            }
        }
    }
    let r = held.unwrap_or_else(|| msg::reply_empty(&q, msg::RCODE_SERVFAIL).unwrap_or_default());
    write_frame(&mut client, &r)
}

/// 返回是否执行成功；cache 记录已加入的 IP 与到期时间，避免重复调用
fn ipset_add(set: &str, ips: &[(Ipv4Addr, u32)], cache: &mut HashMap<Ipv4Addr, u64>) -> bool {
    let now = sys::now_ms();
    let mut input = String::new();
    for (ip, ttl) in ips {
        let t = (*ttl).clamp(300, 86400);
        if let Some(&exp) = cache.get(ip) {
            if exp > now + 120_000 {
                continue;
            }
        }
        cache.insert(*ip, now + t as u64 * 1000);
        input.push_str(&format!("add {} {} timeout {}\n", set, ip, t));
    }
    if input.is_empty() {
        return true;
    }
    if cache.len() > 20_000 {
        cache.retain(|_, &mut e| e > now);
    }
    sys::run_stdin("ipset", &["restore", "-!"], &input)
}

fn ipset_worker(rx: mpsc::Receiver<Job>, out: UdpSocket, set: String) {
    let mut cache = HashMap::new();
    while let Ok(first) = rx.recv() {
        let mut jobs = vec![first];
        while let Ok(j) = rx.try_recv() {
            jobs.push(j);
            if jobs.len() >= 64 {
                break;
            }
        }
        let ips: Vec<(Ipv4Addr, u32)> = jobs.iter().flat_map(|j| j.ips.iter().copied()).collect();
        ipset_add(&set, &ips, &mut cache);
        for j in jobs {
            let _ = out.send_to(&j.resp, j.client);
        }
    }
}

/// 查询 ID 随机数的种子
fn seed() -> u32 {
    let mut b = [0u8; 4];
    let ok = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_ok();
    let x = if ok { u32::from_ne_bytes(b) } else { (sys::now_ms() as u32) ^ (std::process::id() << 16) ^ sys::wall_secs() as u32 };
    x | 1 // xorshift 的状态不能为 0
}

/// 一次性解析（供探测与解析服务器地址用）：阻塞、可绑定网卡。返回 (A 记录, 往返毫秒)
pub fn resolve_once(server: Ipv4Addr, dev: Option<&str>, name: &str, timeout: Duration) -> Result<(Vec<Ipv4Addr>, u32), String> {
    let s = sys::udp_connected(SocketAddrV4::new(server, 53), dev, false).map_err(|e| format!("建立查询失败：{}", e))?;
    s.set_read_timeout(Some(timeout)).ok();
    let id = (sys::now_ms() as u16) ^ 0x5a5a;
    let q = msg::build_query(id, name, msg::TYPE_A);
    let t0 = sys::now_ms();
    s.send(&q).map_err(|e| format!("发送失败：{}", e))?;
    let mut buf = [0u8; 1500];
    loop {
        let n = s.recv(&mut buf).map_err(|e| {
            if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut {
                "超时无应答".to_string()
            } else {
                format!("接收失败：{}", e)
            }
        })?;
        let r = &buf[..n];
        if msg::id(r) != Some(id) || !msg::is_response(r) {
            continue;
        }
        let rc = msg::rcode(r);
        if rc == msg::RCODE_SERVFAIL || rc == 5 {
            return Err(format!("服务器拒绝（rcode {}）", rc));
        }
        let rtt = (sys::now_ms() - t0) as u32;
        return Ok((msg::a_records(r).into_iter().map(|x| x.0).collect(), rtt));
    }
}
