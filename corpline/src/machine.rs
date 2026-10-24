//! 状态机：只做判断，不碰网络。输入事件 + 当前时间（毫秒，单调时钟），输出要执行的动作。
//! 所有时间都由调用方注入，单元测试用模拟时钟覆盖每一条状态转换。

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// 刚启用，第一次连接公司线路（流量直连）
    Connecting,
    /// 正在使用公司专线
    Active,
    /// 公司线路有点慢或偶发无响应，仍走公司线路，观察中
    Watching,
    /// 已临时切到直连，后台继续重连
    Fallback,
    /// 账号或密码不对，停止重试（终态，等用户改密码）
    AuthErr,
}

impl State {
    pub fn key(self) -> &'static str {
        match self {
            State::Connecting => "connecting",
            State::Active => "active",
            State::Watching => "watching",
            State::Fallback => "fallback",
            State::AuthErr => "autherr",
        }
    }
    /// 这个状态下家里流量是否走公司线路
    pub fn tunneled(self) -> bool {
        matches!(self, State::Active | State::Watching)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Ok,
    Info,
    Warn,
    Err,
}

impl Level {
    pub fn key(self) -> &'static str {
        match self {
            Level::Ok => "ok",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Err => "err",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Tick,
    TunnelUp,
    OcExited { auth_failed: bool, detail: String },
    Probe { ok: bool, rtt_ms: u32 },
    /// openconnect 资源异常或卡死（由守护程序检测），需要重启它
    OcUnhealthy(String),
    UserRetry,
    /// 宽带恢复（主路由表的默认路由重新出现）：不必等退避，立即重连
    WanBack,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    StartOc,
    StopOc(String),
    /// 打开/关闭"走公司线路"的总阀门
    SetLive(bool),
    /// 切回公司线路后清掉 dnsmasq 里直连期间的解析缓存
    FlushDnsCache,
    Probe,
    Note(Level, String),
}

#[derive(Clone, Debug)]
pub struct Params {
    pub probe_every: u64,
    pub fail_threshold: u32,
    pub slow_ms: u32,
    pub recover_hold: u64,
    pub connect_timeout: u64,
    pub backoff: Vec<u64>,
    pub backoff_tail: u64,
    pub daily_cap: u32,
    /// 已直连且隧道仍在、但探测又连续失败这么多次 → 重建隧道
    pub stuck_failures: u32,
    /// 公司线路稳定这么久后，重连间隔从头算
    pub stable_reset: u64,
}

impl Params {
    pub fn normal(daily_cap: u32) -> Params {
        Params {
            probe_every: 15_000,
            fail_threshold: 3,
            slow_ms: 800,
            recover_hold: 120_000,
            connect_timeout: 45_000,
            backoff: vec![5_000, 10_000, 30_000, 60_000, 120_000, 300_000],
            backoff_tail: 900_000,
            daily_cap,
            stuck_failures: 4,
            stable_reset: 600_000,
        }
    }
    /// "宽松"灵敏度：网络本身抖动大的家庭用，少切换
    pub fn relaxed(daily_cap: u32) -> Params {
        Params { fail_threshold: 5, slow_ms: 1500, recover_hold: 180_000, ..Params::normal(daily_cap) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Oc {
    Down,
    Starting(u64),
    Up(u64),
    Stopping,
}

pub struct Machine {
    pub p: Params,
    pub state: State,
    pub since: u64,
    pub reason: String,
    pub oc: Oc,
    pub fails: u32,
    pub last_rtt: Option<u32>,
    pub attempts_today: u32,
    pub next_attempt: Option<u64>,
    healthy_since: Option<u64>,
    active_since: Option<u64>,
    backoff_idx: usize,
    next_probe: u64,
    probing: bool,
    ever_active: bool,
    live: bool,
}

impl Machine {
    pub fn new(p: Params, attempts_today: u32) -> Machine {
        Machine {
            p,
            state: State::Connecting,
            since: 0,
            reason: String::new(),
            oc: Oc::Down,
            fails: 0,
            last_rtt: None,
            attempts_today,
            next_attempt: None,
            healthy_since: None,
            active_since: None,
            backoff_idx: 0,
            next_probe: 0,
            probing: false,
            ever_active: false,
            live: false,
        }
    }

    pub fn live(&self) -> bool {
        self.live
    }

    pub fn start(&mut self, now: u64) -> Vec<Action> {
        let mut a = Vec::new();
        self.since = now;
        self.goto(State::Connecting, now, "正在连接公司线路", &mut a);
        self.attempt(now, &mut a);
        a
    }

    /// 新的一天：每日重连次数清零；若此前因到上限停手，立刻恢复重连。
    pub fn new_day(&mut self, now: u64) -> Vec<Action> {
        let mut a = Vec::new();
        let capped = self.attempts_today >= self.p.daily_cap;
        self.attempts_today = 0;
        if capped && self.oc == Oc::Down && self.state != State::AuthErr {
            self.next_attempt = Some(now);
        }
        a.extend(self.handle(Event::Tick, now));
        a
    }

    pub fn backoff_delay(&self) -> u64 {
        *self.p.backoff.get(self.backoff_idx).unwrap_or(&self.p.backoff_tail)
    }

    fn goto(&mut self, s: State, now: u64, reason: &str, a: &mut Vec<Action>) {
        if self.state != s || self.since == 0 {
            self.state = s;
            self.since = now;
        }
        self.reason = reason.to_string();
        let want = s.tunneled();
        if want != self.live {
            self.live = want;
            a.push(Action::SetLive(want));
            if want {
                a.push(Action::FlushDnsCache);
            }
        }
        if s == State::Active && self.active_since.is_none() {
            self.active_since = Some(now);
        }
        if s != State::Active {
            self.active_since = None;
        }
    }

    fn attempt(&mut self, now: u64, a: &mut Vec<Action>) {
        if self.attempts_today >= self.p.daily_cap {
            self.next_attempt = None;
            if self.state != State::Fallback {
                self.goto(State::Fallback, now, "今天重连次数已到上限", a);
            } else {
                self.reason = "今天重连次数已到上限".into();
            }
            a.push(Action::Note(
                Level::Warn,
                format!("今天已重连 {} 次，先保持直连，明天自动再试（也可以点\"立即重试\"）", self.attempts_today),
            ));
            return;
        }
        self.attempts_today += 1;
        self.next_attempt = None;
        self.oc = Oc::Starting(now);
        self.fails = 0;
        self.probing = false;
        self.healthy_since = None;
        a.push(Action::StartOc);
    }

    fn schedule_retry(&mut self, now: u64) -> u64 {
        let d = self.backoff_delay();
        self.backoff_idx += 1;
        self.next_attempt = Some(now + d);
        d
    }

    pub fn handle(&mut self, ev: Event, now: u64) -> Vec<Action> {
        let mut a = Vec::new();
        match ev {
            Event::Tick => self.tick(now, &mut a),
            Event::TunnelUp => {
                if matches!(self.oc, Oc::Starting(_) | Oc::Up(_)) {
                    if !matches!(self.oc, Oc::Up(_)) {
                        self.oc = Oc::Up(now);
                    }
                    self.fails = 0;
                    self.probing = true;
                    self.next_probe = now + self.p.probe_every;
                    a.push(Action::Probe);
                }
            }
            Event::Probe { ok, rtt_ms } => self.on_probe(ok, rtt_ms, now, &mut a),
            Event::OcExited { auth_failed, detail } => self.on_exit(auth_failed, &detail, now, &mut a),
            Event::OcUnhealthy(why) => {
                if matches!(self.oc, Oc::Starting(_) | Oc::Up(_)) {
                    self.oc = Oc::Stopping;
                    a.push(Action::StopOc(why));
                }
            }
            Event::WanBack => {
                if self.oc == Oc::Down && self.state != State::AuthErr && self.attempts_today < self.p.daily_cap {
                    self.backoff_idx = 0;
                    a.push(Action::Note(Level::Info, "宽带恢复，立即重连公司线路".into()));
                    self.attempt(now, &mut a);
                }
            }
            Event::UserRetry => {
                self.backoff_idx = 0;
                match self.oc {
                    Oc::Down => {
                        if self.state == State::AuthErr {
                            self.goto(State::Connecting, now, "正在连接公司线路", &mut a);
                        }
                        if self.attempts_today >= self.p.daily_cap {
                            // 用户主动重试不受每日上限约束，但仍计数
                            self.attempts_today = self.p.daily_cap.saturating_sub(1);
                        }
                        a.push(Action::Note(Level::Info, "手动重试连接公司线路".into()));
                        self.attempt(now, &mut a);
                    }
                    Oc::Up(_) if !self.probing => {
                        self.probing = true;
                        a.push(Action::Probe);
                    }
                    _ => {}
                }
            }
        }
        a
    }

    fn tick(&mut self, now: u64, a: &mut Vec<Action>) {
        match self.oc {
            Oc::Starting(at) if now.saturating_sub(at) >= self.p.connect_timeout => {
                self.oc = Oc::Stopping;
                a.push(Action::StopOc(format!("{} 秒内没连上", self.p.connect_timeout / 1000)));
            }
            Oc::Down => {
                if let Some(t) = self.next_attempt {
                    if now >= t {
                        self.attempt(now, a);
                    }
                }
            }
            Oc::Up(_) => {
                if !self.probing && now >= self.next_probe {
                    self.probing = true;
                    a.push(Action::Probe);
                }
            }
            _ => {}
        }
        if let Some(t) = self.active_since {
            if self.state == State::Active && now.saturating_sub(t) >= self.p.stable_reset {
                self.backoff_idx = 0;
            }
        }
    }

    fn on_probe(&mut self, ok: bool, rtt: u32, now: u64, a: &mut Vec<Action>) {
        self.probing = false;
        if !matches!(self.oc, Oc::Up(_)) {
            return; // 迟到的探测结果，隧道已不在
        }
        self.next_probe = now + self.p.probe_every;
        if ok {
            self.last_rtt = Some(rtt);
            let slow = rtt > self.p.slow_ms;
            self.fails = 0;
            match self.state {
                State::Connecting => {
                    self.ever_active = true;
                    a.push(Action::Note(Level::Ok, "已连上公司专线，全家流量改走公司线路".into()));
                    if slow {
                        self.goto(State::Watching, now, &format!("延迟 {} ms，偏慢", rtt), a);
                    } else {
                        self.goto(State::Active, now, "", a);
                    }
                }
                State::Active | State::Watching => {
                    if slow {
                        if self.state == State::Active {
                            a.push(Action::Note(Level::Info, format!("公司线路变慢（延迟 {} ms），继续观察", rtt)));
                        }
                        self.goto(State::Watching, now, &format!("延迟 {} ms，偏慢", rtt), a);
                    } else {
                        if self.state == State::Watching {
                            a.push(Action::Note(Level::Ok, "公司线路恢复正常".into()));
                        }
                        self.goto(State::Active, now, "", a);
                    }
                }
                State::Fallback => {
                    let first = *self.healthy_since.get_or_insert(now);
                    if !self.ever_active || now.saturating_sub(first) >= self.p.recover_hold {
                        let msg = if self.ever_active {
                            format!("公司线路已稳定 {} 秒，切回公司专线", now.saturating_sub(first) / 1000)
                        } else {
                            "已连上公司专线，全家流量改走公司线路".into()
                        };
                        self.ever_active = true;
                        a.push(Action::Note(Level::Ok, msg));
                        if slow {
                            self.goto(State::Watching, now, &format!("延迟 {} ms，偏慢", rtt), a);
                        } else {
                            self.goto(State::Active, now, "", a);
                        }
                    } else {
                        self.reason = "公司线路已恢复，稳定一段时间后切回".into();
                    }
                }
                State::AuthErr => {}
            }
        } else {
            self.fails += 1;
            self.healthy_since = None;
            match self.state {
                State::Active | State::Watching | State::Connecting => {
                    if self.fails >= self.p.fail_threshold {
                        a.push(Action::Note(
                            Level::Warn,
                            format!("公司线路连续 {} 次无响应，已临时切到直连，后台继续尝试", self.fails),
                        ));
                        self.goto(State::Fallback, now, "公司线路无响应", a);
                    } else if self.state != State::Connecting {
                        if self.state == State::Active {
                            a.push(Action::Note(Level::Info, "公司线路一次无响应，继续观察".into()));
                        }
                        self.goto(State::Watching, now, &format!("连续 {} 次无响应", self.fails), a);
                    }
                }
                State::Fallback => {
                    if self.fails >= self.p.fail_threshold + self.p.stuck_failures {
                        self.oc = Oc::Stopping;
                        a.push(Action::StopOc("公司线路长时间无响应，重建连接".into()));
                    }
                }
                State::AuthErr => {}
            }
        }
    }

    fn on_exit(&mut self, auth_failed: bool, detail: &str, now: u64, a: &mut Vec<Action>) {
        let requested = self.oc == Oc::Stopping;
        self.oc = Oc::Down;
        self.probing = false;
        self.healthy_since = None;
        if auth_failed {
            self.next_attempt = None;
            a.push(Action::Note(Level::Err, "公司账号登录失败（账号或密码不对），已停止重试，全家保持直连".into()));
            self.goto(State::AuthErr, now, "账号或密码不对", a);
            return;
        }
        let was_tunneled = self.state.tunneled();
        let d = self.schedule_retry(now);
        let wait = human_secs(d / 1000);
        if was_tunneled {
            a.push(Action::Note(Level::Warn, format!("公司线路断开（{}），已临时切到直连，{}后重连", detail, wait)));
            self.goto(State::Fallback, now, "公司线路断开", a);
        } else if requested {
            a.push(Action::Note(Level::Info, format!("{}，{}后重新连接", detail, wait)));
            if self.state == State::Connecting {
                self.goto(State::Fallback, now, "暂时连不上公司线路", a);
            }
        } else {
            a.push(Action::Note(Level::Info, format!("连接未成功（{}），{}后再试", detail, wait)));
            if self.state == State::Connecting {
                self.goto(State::Fallback, now, "暂时连不上公司线路", a);
            } else {
                self.reason = "暂时连不上公司线路".into();
            }
        }
    }
}

pub fn human_secs(s: u64) -> String {
    if s < 60 {
        format!("{} 秒", s)
    } else if s % 60 == 0 {
        format!("{} 分钟", s / 60)
    } else {
        format!("{} 分 {} 秒", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m() -> Machine {
        Machine::new(Params::normal(60), 0)
    }

    fn has(a: &[Action], x: &Action) -> bool {
        a.iter().any(|y| y == x)
    }

    fn up(m: &mut Machine, now: u64) {
        let a = m.handle(Event::TunnelUp, now);
        assert!(has(&a, &Action::Probe));
    }

    fn active(now: u64) -> Machine {
        let mut x = m();
        assert!(has(&x.start(now), &Action::StartOc));
        up(&mut x, now + 2000);
        let a = x.handle(Event::Probe { ok: true, rtt_ms: 80 }, now + 2100);
        assert!(has(&a, &Action::SetLive(true)));
        assert_eq!(x.state, State::Active);
        x
    }

    #[test]
    fn start_connect_active() {
        let x = active(1000);
        assert!(x.live());
    }

    #[test]
    fn no_live_until_probe_passes() {
        let mut x = m();
        let a = x.start(1);
        assert!(!a.iter().any(|y| matches!(y, Action::SetLive(true))));
        up(&mut x, 10);
        assert!(!x.live());
    }

    #[test]
    fn three_failures_fallback_then_hold_before_return() {
        let mut x = active(1000);
        let mut t = 20_000;
        let a = x.handle(Event::Probe { ok: false, rtt_ms: 0 }, t);
        assert_eq!(x.state, State::Watching);
        assert!(!has(&a, &Action::SetLive(false)));
        t += 15_000;
        x.handle(Event::Probe { ok: false, rtt_ms: 0 }, t);
        assert_eq!(x.state, State::Watching);
        t += 15_000;
        let a = x.handle(Event::Probe { ok: false, rtt_ms: 0 }, t);
        assert_eq!(x.state, State::Fallback);
        assert!(has(&a, &Action::SetLive(false)));
        // 恢复：需要连续健康 120 s
        t += 15_000;
        x.handle(Event::Probe { ok: true, rtt_ms: 50 }, t);
        assert_eq!(x.state, State::Fallback);
        let first = t;
        while t < first + 120_000 {
            t += 15_000;
            x.handle(Event::Probe { ok: true, rtt_ms: 50 }, t);
        }
        assert_eq!(x.state, State::Active);
        assert!(x.live());
    }

    #[test]
    fn flap_during_recovery_resets_hold() {
        let mut x = active(0);
        for i in 0..3 {
            x.handle(Event::Probe { ok: false, rtt_ms: 0 }, 15_000 * (i + 1));
        }
        assert_eq!(x.state, State::Fallback);
        let mut t = 60_000;
        for _ in 0..6 {
            t += 15_000;
            x.handle(Event::Probe { ok: true, rtt_ms: 50 }, t);
        }
        x.handle(Event::Probe { ok: false, rtt_ms: 0 }, t + 15_000);
        t += 15_000;
        for _ in 0..5 {
            t += 15_000;
            x.handle(Event::Probe { ok: true, rtt_ms: 50 }, t);
        }
        assert_eq!(x.state, State::Fallback, "抖动后应重新计时");
    }

    #[test]
    fn slow_means_watching_not_fallback() {
        let mut x = active(0);
        x.handle(Event::Probe { ok: true, rtt_ms: 1200 }, 20_000);
        assert_eq!(x.state, State::Watching);
        assert!(x.live());
        x.handle(Event::Probe { ok: true, rtt_ms: 100 }, 35_000);
        assert_eq!(x.state, State::Active);
    }

    #[test]
    fn oc_exit_backoff_chain() {
        let mut x = active(0);
        let a = x.handle(Event::OcExited { auth_failed: false, detail: "x".into() }, 10_000);
        assert!(has(&a, &Action::SetLive(false)));
        assert_eq!(x.state, State::Fallback);
        assert_eq!(x.next_attempt, Some(15_000));
        assert!(x.handle(Event::Tick, 14_000).is_empty());
        assert!(has(&x.handle(Event::Tick, 15_000), &Action::StartOc));
        let mut t = 15_000;
        let mut gaps = Vec::new();
        for _ in 0..8 {
            t += 1000;
            x.handle(Event::OcExited { auth_failed: false, detail: "x".into() }, t);
            let n = x.next_attempt.unwrap();
            gaps.push((n - t) / 1000);
            t = n;
            x.handle(Event::Tick, t);
        }
        assert_eq!(gaps, vec![10, 30, 60, 120, 300, 900, 900, 900]);
    }

    #[test]
    fn auth_failure_is_terminal() {
        let mut x = m();
        x.start(0);
        let a = x.handle(Event::OcExited { auth_failed: true, detail: "auth".into() }, 3000);
        assert_eq!(x.state, State::AuthErr);
        assert!(!a.iter().any(|y| matches!(y, Action::StartOc)));
        for t in [10_000u64, 100_000, 10_000_000] {
            assert!(!x.handle(Event::Tick, t).iter().any(|y| matches!(y, Action::StartOc)));
        }
        assert!(has(&x.handle(Event::UserRetry, 20_000_000), &Action::StartOc));
    }

    #[test]
    fn daily_cap_stops_until_new_day() {
        let mut x = Machine::new(Params::normal(3), 0);
        x.start(0);
        let mut t = 0;
        for _ in 0..3 {
            t += 1000;
            x.handle(Event::OcExited { auth_failed: false, detail: "x".into() }, t);
            if let Some(n) = x.next_attempt {
                t = n;
                x.handle(Event::Tick, t);
            }
        }
        assert_eq!(x.attempts_today, 3);
        assert_eq!(x.oc, Oc::Down);
        assert_eq!(x.next_attempt, None);
        assert!(x.reason.contains("上限"));
        assert!(!x.handle(Event::Tick, t + 10_000_000).iter().any(|y| matches!(y, Action::StartOc)));
        assert!(has(&x.new_day(t + 20_000_000), &Action::StartOc));
    }

    #[test]
    fn connect_timeout_kills_oc() {
        let mut x = m();
        x.start(0);
        assert!(x.handle(Event::Tick, 44_000).is_empty());
        let a = x.handle(Event::Tick, 45_000);
        assert!(a.iter().any(|y| matches!(y, Action::StopOc(_))));
        x.handle(Event::OcExited { auth_failed: false, detail: "killed".into() }, 46_000);
        assert_eq!(x.state, State::Fallback);
        assert!(x.next_attempt.is_some());
    }

    #[test]
    fn stuck_tunnel_in_fallback_rebuilt() {
        let mut x = active(0);
        let mut t = 0;
        let mut stopped = false;
        for _ in 0..7 {
            t += 15_000;
            let a = x.handle(Event::Probe { ok: false, rtt_ms: 0 }, t);
            stopped |= a.iter().any(|y| matches!(y, Action::StopOc(_)));
        }
        assert!(stopped);
        assert_eq!(x.oc, Oc::Stopping);
    }

    #[test]
    fn late_probe_after_exit_ignored() {
        let mut x = active(0);
        x.handle(Event::Tick, 20_000);
        x.handle(Event::OcExited { auth_failed: false, detail: "x".into() }, 21_000);
        let a = x.handle(Event::Probe { ok: true, rtt_ms: 10 }, 22_000);
        assert!(a.is_empty());
        assert_eq!(x.state, State::Fallback);
    }

    #[test]
    fn first_connect_failures_never_live() {
        let mut x = m();
        x.start(0);
        up(&mut x, 1000);
        for i in 0..3 {
            let a = x.handle(Event::Probe { ok: false, rtt_ms: 0 }, 2000 + i * 15_000);
            assert!(!a.iter().any(|y| matches!(y, Action::SetLive(true))));
        }
        assert_eq!(x.state, State::Fallback);
        // 从未成功过：第一次健康就切过去，不必等 120 s
        let a = x.handle(Event::Probe { ok: true, rtt_ms: 30 }, 60_000);
        assert!(has(&a, &Action::SetLive(true)));
    }

    #[test]
    fn probes_scheduled_by_tick() {
        let mut x = active(0);
        assert!(x.handle(Event::Tick, 10_000).is_empty());
        assert!(has(&x.handle(Event::Tick, 17_200), &Action::Probe));
        // 探测进行中不重复发起
        assert!(!has(&x.handle(Event::Tick, 18_000), &Action::Probe));
    }

    #[test]
    fn stable_resets_backoff() {
        let mut x = active(0);
        x.handle(Event::OcExited { auth_failed: false, detail: "x".into() }, 1000);
        let t = x.next_attempt.unwrap();
        x.handle(Event::Tick, t);
        x.handle(Event::TunnelUp, t + 1000);
        x.handle(Event::Probe { ok: true, rtt_ms: 10 }, t + 2000);
        assert_eq!(x.state, State::Fallback); // ever_active → 需要稳定
        let mut now = t + 2000;
        while x.state != State::Active {
            now += 15_000;
            x.handle(Event::Tick, now);
            x.handle(Event::Probe { ok: true, rtt_ms: 10 }, now);
        }
        assert_eq!(x.backoff_delay(), 10_000);
        now += 600_000;
        x.handle(Event::Tick, now);
        assert_eq!(x.backoff_delay(), 5_000);
    }

    #[test]
    fn wan_back_skips_backoff() {
        let mut x = active(0);
        let mut t = 1000;
        for _ in 0..4 {
            x.handle(Event::OcExited { auth_failed: false, detail: "x".into() }, t);
            t = x.next_attempt.unwrap();
            x.handle(Event::Tick, t);
        }
        x.handle(Event::OcExited { auth_failed: false, detail: "x".into() }, t + 1);
        assert!(x.next_attempt.unwrap() - t >= 120_000);
        assert!(has(&x.handle(Event::WanBack, t + 5000), &Action::StartOc));
        // 正在连接时不重复发起
        assert!(!has(&x.handle(Event::WanBack, t + 6000), &Action::StartOc));
    }

    #[test]
    fn human() {
        assert_eq!(human_secs(5), "5 秒");
        assert_eq!(human_secs(120), "2 分钟");
        assert_eq!(human_secs(90), "1 分 30 秒");
    }
}
