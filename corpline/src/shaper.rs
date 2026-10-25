//! 公司线路上行排队控制。
//!
//! 公司 VPN 服务器只走 TLS（TCP）时，有人往公司线路上大量上传，数据就堆在 openconnect 到服务器的那条
//! TCP 连接里（Linux 默认最多 4 MB），全家走公司线路的其它连接都得跟在后面排几秒的队。隧道网卡本身
//! 不会让包排队（满了直接丢），所以只有把上传限到略低于线路实际能力，队列才会留在路由器这边，再用
//! sfq 让各连接轮流发：视频会议、网页这些小流量就不再被一个大上传拖慢。
//!
//! 线路能力靠观察得出，不做测速：那条 TLS 连接里待发的数据持续积压，就说明线路满了，此时隧道实际
//! 发出的速率就是线路能力，限到它的 80%。之后有人贴着上限传、又不积压，就很快往上放（每 5 秒 10%），
//! 直到开始积压为止 —— 只有一台设备在传时也能用到接近线路的全部
//! 能力，不会因为一次测低了就一直卡着。没人上传满 30 分钟就撤掉限速，下次占满时重新测。
//! 没人大量上传时不限速、不改任何东西。

use std::collections::VecDeque;

/// 最低限到 1 Mbit/s（再低公司线路就没法用了）
const MIN_KBIT: u32 = 1000;
/// 放宽到这么高等于不限，撤掉
const MAX_KBIT: u32 = 200_000;
/// 待发数据（含已发出未确认的）超过 32 KB、且按当前速率要排 100 ms 以上，算积压。
/// 华硕上这条连接的发送缓冲只有 100 KB 左右，积压时在 70–100 KB 之间跳，阈值不能定得太高；
/// 按时间算是为了线路很快时不把正常在途的数据（约一个来回）当成积压
const FULL_BYTES: u64 = 32 * 1024;
const FULL_MS: u64 = 100;
/// 调整后至少 5 秒内不再调
const HOLD_MS: u64 = 5_000;
/// 贴着上限跑、不积压满这么多秒就放宽 10%
const RAISE_SECS: u32 = 5;
/// 没人往公司线路上传满这么久（秒）就撤掉限速
const IDLE_OFF_SECS: u32 = 1800;

#[derive(Debug, PartialEq)]
pub enum Change {
    /// 限到多少 kbit/s；第二项 = 本来没限速、这次开始限
    Set(u32, bool),
    Off,
}

pub struct Shaper {
    /// 当前上限（kbit/s），None = 不限
    pub limit: Option<u32>,
    /// 最近一次线路占满时实际发出的速率（kbit/s）
    pub cap: u32,
    /// 最近一次看到的速率（kbit/s，3 秒平均）与 TLS 连接待发字节，给状态文件排查用
    pub last_kbit: u32,
    pub last_queued: u64,
    last: Option<(u64, u64, u64)>,
    rates: VecDeque<u32>,
    /// 最近几秒是否积压（最低位是这一秒）；最近 3 秒里有 2 秒积压才动手，一阵突发不算
    full: u8,
    calm: u32,
    idle: u32,
    hold_until: u64,
}

impl Shaper {
    pub fn new() -> Self {
        Shaper { limit: None, cap: 0, last_kbit: 0, last_queued: 0, last: None, rates: VecDeque::new(), full: 0, calm: 0, idle: 0, hold_until: 0 }
    }

    /// 隧道重建（网卡计数从头开始）后调用
    pub fn reset_counters(&mut self) {
        self.last = None;
        self.rates.clear();
        self.full = 0;
        self.calm = 0;
    }

    /// 约每秒一次：tx / dropped = 隧道网卡累计发出、丢弃的包字节数与个数，
    /// queued = TLS 连接里待发的字节（走 DTLS 时没有）
    pub fn observe(&mut self, now: u64, tx: u64, dropped: u64, queued: Option<u64>) -> Option<Change> {
        let (t0, b0, d0) = self.last.replace((now, tx, dropped))?;
        let dt = now.saturating_sub(t0);
        if dt < 500 || tx < b0 {
            return None;
        }
        let r = ((tx - b0) * 8 / dt).min(u32::MAX as u64) as u32;
        self.rates.push_back(r);
        if self.rates.len() > 3 {
            self.rates.pop_front();
        }
        let avg = (self.rates.iter().map(|&x| x as u64).sum::<u64>() / self.rates.len() as u64) as u32;
        // 速率取最近 3 秒的中间值：刚开始时 TCP 还在加速、或者第一秒一下子灌进缓冲，都不算数
        let mut sorted: Vec<u32> = self.rates.iter().copied().collect();
        sorted.sort_unstable();
        let med = sorted[(sorted.len() - 1) / 2];
        let q = queued.unwrap_or(0);
        self.last_kbit = avg;
        self.last_queued = q;
        // 隧道网卡丢包 = openconnect 来不及读（它那条连接塞满了），也算积压
        let full = med >= 500 && ((q >= FULL_BYTES && q * 8 / med as u64 >= FULL_MS) || dropped > d0);
        self.full = (self.full << 1 | full as u8) & 0b111;
        if self.full.count_ones() >= 2 && self.rates.len() >= 3 && now >= self.hold_until {
            self.full = 0;
            self.calm = 0;
            self.hold_until = now + HOLD_MS;
            let avg = med;
            self.cap = avg;
        let first = self.limit.is_none();
            // 第一次限到八成；已经在限、往上放宽时又占满了，说明刚好碰到线路能力，退回九成就够
            let l = (avg / 10 * if first { 8 } else { 9 }).max(MIN_KBIT);
            self.limit = Some(l);
            return Some(Change::Set(l, first));
        }
        let l = self.limit?;
        if avg as u64 * 10 < l as u64 {
            self.idle += 1;
            if self.idle >= IDLE_OFF_SECS {
                self.idle = 0;
                self.calm = 0;
                self.limit = None;
                return Some(Change::Off);
            }
        } else {
            self.idle = 0;
        }
        // 贴着上限跑、又没开始积压：说明还能再快。积压一点（≥16 KB）就停在这里不再往上
        if avg as u64 * 100 >= l as u64 * 85 && q < 16 * 1024 {
            self.calm += 1;
        } else if (avg as u64) * 2 < l as u64 {
            self.calm = 0;
        }
        let next = l as u64 * 11 / 10;
        if self.calm >= RAISE_SECS && now >= self.hold_until {
            self.calm = 0;
            if next > MAX_KBIT as u64 {
                self.limit = None;
                return Some(Change::Off);
            }
            self.limit = Some(next as u32);
            return Some(Change::Set(next as u32, false));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 按 kbit/s 和待发字节喂 n 秒，返回最后一次调整
    fn feed(s: &mut Shaper, t: &mut u64, tx: &mut u64, kbit: u64, q: Option<u64>, n: u32) -> Option<Change> {
        let mut last = None;
        for _ in 0..n {
            *t += 1000;
            *tx += kbit * 1000 / 8;
            if let Some(c) = s.observe(*t, *tx, 0, q) {
                last = Some(c);
            }
        }
        last
    }

    #[test]
    fn idle_or_busy_without_backlog_never_limits() {
        let cases: [(u64, Option<u64>); 3] = [
            (20_000, Some(8 * 1024)),
            (50, Some(200 * 1024)), // 速率很低：不是被大上传堵的
            (5000, None),           // DTLS：没有 TLS 队列
        ];
        for (kbit, q) in cases {
            let (mut s, mut t, mut tx) = (Shaper::new(), 0, 0);
            assert_eq!(feed(&mut s, &mut t, &mut tx, kbit, q, 120), None);
            assert_eq!(s.limit, None);
        }
    }

    #[test]
    fn saturation_limits_to_80_percent() {
        let (mut s, mut t, mut tx) = (Shaper::new(), 0, 0);
        feed(&mut s, &mut t, &mut tx, 5000, Some(0), 2);
        // 1 秒积压不动手，第 2 秒限到 4000
        assert_eq!(feed(&mut s, &mut t, &mut tx, 5000, Some(2_000_000), 1), None);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 5000, Some(2_000_000), 1), Some(Change::Set(4000, true)));
        assert_eq!(s.cap, 5000);
        // 限速后仍积压（线路又变慢了）：5 秒内不再调，之后按新速率的九成再降
        assert_eq!(feed(&mut s, &mut t, &mut tx, 3000, Some(2_000_000), 4), None);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 3000, Some(2_000_000), 1), Some(Change::Set(2700, false)));
    }

    #[test]
    fn floor_is_one_mbit() {
        let (mut s, mut t, mut tx) = (Shaper::new(), 0, 0);
        feed(&mut s, &mut t, &mut tx, 800, Some(0), 1);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 800, Some(500_000), 5), Some(Change::Set(MIN_KBIT, true)));
    }

    #[test]
    fn raises_when_pinned_at_limit_without_backlog() {
        let (mut s, mut t, mut tx) = (Shaper::new(), 0, 0);
        feed(&mut s, &mut t, &mut tx, 10_000, Some(0), 1);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 10_000, Some(3_000_000), 3), Some(Change::Set(8000, true)));
        // 只有一台设备贴着 8000 跑、不积压：5 秒后放宽到 8800
        assert_eq!(feed(&mut s, &mut t, &mut tx, 8000, Some(0), 4), None);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 8000, Some(0), 1), Some(Change::Set(8800, false)));
        // 继续每 5 秒放宽一次
        assert_eq!(feed(&mut s, &mut t, &mut tx, 8800, Some(0), 4), None);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 8800, Some(0), 1), Some(Change::Set(9680, false)));
        // 开始积压一点就停住，不再往上
        assert_eq!(feed(&mut s, &mut t, &mut tx, 9680, Some(32 * 1024), 120), None);
        assert_eq!(s.limit, Some(9680));
    }

    #[test]
    fn a_low_estimate_recovers_quickly() {
        // 某次只测到 2000（线路其实能跑 3500）：一台设备单独传，一分钟内就能用满 3500
        let (mut s, mut t, mut tx) = (Shaper::new(), 0, 0);
        feed(&mut s, &mut t, &mut tx, 2000, Some(0), 1);
        feed(&mut s, &mut t, &mut tx, 2000, Some(500_000), 3);
        assert_eq!(s.limit, Some(1600));
        for _ in 0..60 {
            let l = s.limit.unwrap() as u64;
            feed(&mut s, &mut t, &mut tx, l.min(3500), Some(0), 1);
        }
        assert!(s.limit.unwrap() >= 3500, "{:?}", s.limit);
        // 线路就这么快：不会无限放宽
        for _ in 0..600 {
            let l = s.limit.unwrap() as u64;
            feed(&mut s, &mut t, &mut tx, l.min(3500), Some(0), 1);
        }
        assert!(s.limit.unwrap() < 4200, "{:?}", s.limit);
    }

    #[test]
    fn idle_for_half_an_hour_removes_the_limit() {
        let (mut s, mut t, mut tx) = (Shaper::new(), 0, 0);
        feed(&mut s, &mut t, &mut tx, 5000, Some(0), 1);
        feed(&mut s, &mut t, &mut tx, 5000, Some(2_000_000), 3);
        assert_eq!(s.limit, Some(4000));
        assert_eq!(feed(&mut s, &mut t, &mut tx, 50, Some(0), 1790), None);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 50, Some(0), 20), Some(Change::Off));
        assert_eq!(s.limit, None);
    }

    #[test]
    fn new_tunnel_counters_do_not_confuse_it() {
        let (mut s, mut t, mut tx) = (Shaper::new(), 0, 0);
        feed(&mut s, &mut t, &mut tx, 5000, Some(2_000_000), 2);
        s.reset_counters();
        tx = 0;
        assert_eq!(feed(&mut s, &mut t, &mut tx, 5000, Some(2_000_000), 3), None);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 5000, Some(2_000_000), 1), Some(Change::Set(4000, true)));
    }

    #[test]
    fn router_like_jumpy_queue_is_caught_quickly() {
        // 华硕上实测：积压时待发数据在 67–98 KB 之间跳，速率 4–5 Mbit/s
        let (mut s, mut t, mut tx) = (Shaper::new(), 0, 0);
        feed(&mut s, &mut t, &mut tx, 4800, Some(0), 1);
        let mut got = None;
        // 第一秒一下子灌进缓冲（10 Mbit/s），线路能力不能按它算
        tx += 10_000 * 1000 / 8;
        t += 1000;
        s.observe(t, tx, 0, Some(77_594));
        for (i, q) in [77_594u64, 76_960, 97_888, 66_976].iter().enumerate() {
            if let Some(c) = feed(&mut s, &mut t, &mut tx, 4500, Some(*q), 1) {
                got = Some((i, c));
                break;
            }
        }
        assert_eq!(got.map(|g| g.0), Some(1)); // 有 3 秒速率、2 秒积压就动手
        assert_eq!(s.limit, Some(3600)); // 按 4500 算，不按 10000
    }

    #[test]
    fn tun_drops_count_as_backlog_and_in_flight_data_does_not() {
        let mut s = Shaper::new();
        s.observe(0, 0, 0, Some(0));
        // 50 Mbit/s 下 60 KB 在途（约 10 ms）是正常的
        let mut tx = 0;
        for i in 1..20u64 {
            tx += 50_000 * 1000 / 8;
            assert_eq!(s.observe(i * 1000, tx, 0, Some(60_000)), None);
        }
        // 隧道网卡开始丢包：两秒内动手
        assert_eq!(s.observe(20_000, tx + 625_000, 5, Some(0)), None);
        assert!(matches!(s.observe(21_000, tx + 1_250_000, 9, Some(0)), Some(Change::Set(_, true))));
    }
}
