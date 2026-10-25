//! 公司线路上行排队控制。
//!
//! 公司 VPN 服务器只走 TLS（TCP）时，有人往公司线路上大量上传，数据就堆在 openconnect 到服务器的那条
//! TCP 连接里（Linux 默认最多 4 MB），全家走公司线路的其它连接都得跟在后面排几秒的队。隧道网卡本身
//! 不会让包排队（满了直接丢），所以只有把上传限到略低于线路实际能力，队列才会留在路由器这边，再用
//! sfq 让各连接轮流发：视频会议、网页这些小流量就不再被一个大上传拖慢。
//!
//! 线路能力靠观察得出，不做测速：那条 TLS 连接里待发的数据持续积压，就说明线路满了，此时隧道实际
//! 发出的速率就是线路能力，限到它的 80%。之后有人贴着上限传、又不积压，就每隔一段时间放宽 10%，
//! 线路变快了也能跟上。没人大量上传时不限速、不改任何东西。

use std::collections::VecDeque;

/// 最低限到 1 Mbit/s（再低公司线路就没法用了）
const MIN_KBIT: u32 = 1000;
/// 放宽到这么高等于不限，撤掉
const MAX_KBIT: u32 = 200_000;
/// 待发数据超过 64 KB、且按当前速率要排 150 ms 以上，算积压
const FULL_BYTES: u64 = 64 * 1024;
const FULL_MS: u64 = 150;
/// 连续积压 3 秒才动手（一阵突发不算）
const FULL_SECS: u32 = 3;
/// 调整后至少 10 秒内不再调
const HOLD_MS: u64 = 10_000;

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
    last: Option<(u64, u64)>,
    rates: VecDeque<u32>,
    full: u32,
    calm: u32,
    hold_until: u64,
}

impl Shaper {
    pub fn new() -> Self {
        Shaper { limit: None, cap: 0, last: None, rates: VecDeque::new(), full: 0, calm: 0, hold_until: 0 }
    }

    /// 隧道重建（网卡计数从头开始）后调用
    pub fn reset_counters(&mut self) {
        self.last = None;
        self.rates.clear();
        self.full = 0;
        self.calm = 0;
    }

    /// 约每秒一次：tx = 隧道网卡累计发出的字节，queued = TLS 连接里待发的字节（走 DTLS 时没有）
    pub fn observe(&mut self, now: u64, tx: u64, queued: Option<u64>) -> Option<Change> {
        let (t0, b0) = self.last.replace((now, tx))?;
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
        let q = queued.unwrap_or(0);
        let full = avg >= 500 && q >= FULL_BYTES && q * 8 / avg as u64 >= FULL_MS;
        self.full = if full { self.full + 1 } else { 0 };
        if self.full >= FULL_SECS && now >= self.hold_until {
            self.full = 0;
            self.calm = 0;
            self.hold_until = now + HOLD_MS;
            self.cap = avg;
            let first = self.limit.is_none();
            let l = (avg / 10 * 8).max(MIN_KBIT);
            self.limit = Some(l);
            return Some(Change::Set(l, first));
        }
        let l = self.limit?;
        if avg as u64 * 100 >= l as u64 * 85 && q < 16 * 1024 {
            self.calm += 1;
        } else if (avg as u64) * 2 < l as u64 {
            self.calm = 0;
        }
        // 离上次占满时的速率还远就快些放宽（30 秒），接近了就慢慢试（5 分钟）
        let next = l as u64 * 11 / 10;
        let need = if next < self.cap as u64 * 9 / 10 { 30 } else { 300 };
        if self.calm >= need && now >= self.hold_until {
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
            if let Some(c) = s.observe(*t, *tx, q) {
                last = Some(c);
            }
        }
        last
    }

    #[test]
    fn idle_or_busy_without_backlog_never_limits() {
        let (mut s, mut t, mut tx) = (Shaper::new(), 0, 0);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 20_000, Some(8 * 1024), 120), None);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 50, Some(200 * 1024), 120), None); // 速率很低：不是被大上传堵的
        assert_eq!(feed(&mut s, &mut t, &mut tx, 5000, None, 120), None); // DTLS：没有 TLS 队列
        assert_eq!(s.limit, None);
    }

    #[test]
    fn saturation_limits_to_80_percent() {
        let (mut s, mut t, mut tx) = (Shaper::new(), 0, 0);
        feed(&mut s, &mut t, &mut tx, 5000, Some(0), 2);
        // 2 秒积压不动手，第 3 秒限到 4000
        assert_eq!(feed(&mut s, &mut t, &mut tx, 5000, Some(2_000_000), 2), None);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 5000, Some(2_000_000), 1), Some(Change::Set(4000, true)));
        assert_eq!(s.cap, 5000);
        // 限速后仍积压（线路又变慢了）：10 秒内不再调，之后按新速率再降
        assert_eq!(feed(&mut s, &mut t, &mut tx, 3000, Some(2_000_000), 9), None);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 3000, Some(2_000_000), 3), Some(Change::Set(2400, false)));
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
        // 贴着 8000 跑、不积压：30 秒后放宽到 8800（离 10000 的九成还远）
        assert_eq!(feed(&mut s, &mut t, &mut tx, 8000, Some(0), 29), None);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 8000, Some(0), 1), Some(Change::Set(8800, false)));
        // 再往上就接近上次占满的速率了：要 5 分钟
        assert_eq!(feed(&mut s, &mut t, &mut tx, 8800, Some(0), 299), None);
        assert_eq!(feed(&mut s, &mut t, &mut tx, 8800, Some(0), 1), Some(Change::Set(9680, false)));
        // 没人用时不放宽，也不撤
        assert_eq!(feed(&mut s, &mut t, &mut tx, 0, Some(0), 1000), None);
        assert_eq!(s.limit, Some(9680));
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
}
