//! 最近解析过的域名（网页「网站单独设置」里的「最近访问」）：只在内存里，最多 300 条，
//! 每 10 秒有变化才写一次到内存盘的 JSON，不写闪存。

use super::plan::Class;
use crate::report::esc;
use crate::sys;
use std::collections::HashMap;
use std::path::PathBuf;

const CAP: usize = 300;
const FLUSH_MS: u64 = 10_000;

pub struct Recent {
    path: Option<PathBuf>,
    items: HashMap<String, (u64, u32, Class)>,
    dirty: bool,
    last_flush: u64,
}

fn route(c: Class) -> &'static str {
    match c {
        Class::Internal => "internal",
        Class::Overseas => "overseas",
        Class::Cn => "direct",
        Class::Default => "corp",
    }
}

impl Recent {
    pub fn new(path: Option<PathBuf>) -> Recent {
        Recent { path, items: HashMap::new(), dirty: false, last_flush: 0 }
    }

    pub fn record(&mut self, name: &str, class: Class, wall: u64) {
        if self.path.is_none() || !name.contains('.') || name.ends_with(".arpa") || name.len() > 253 {
            return;
        }
        if let Some(e) = self.items.get_mut(name) {
            e.0 = wall;
            e.1 = e.1.saturating_add(1);
            e.2 = class;
        } else {
            if self.items.len() >= CAP {
                // 挤掉最久没出现的那条（300 条线性找一遍，只在新域名进来时发生）
                if let Some(k) = self.items.iter().min_by_key(|(_, v)| v.0).map(|(k, _)| k.clone()) {
                    self.items.remove(&k);
                }
            }
            self.items.insert(name.to_string(), (wall, 1, class));
        }
        self.dirty = true;
    }

    pub fn json(&self, wall: u64) -> String {
        let mut v: Vec<(&String, &(u64, u32, Class))> = self.items.iter().collect();
        v.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
        let mut s = format!("{{\"v\":1,\"updated\":{},\"items\":[", wall);
        for (i, (d, (t, n, c))) in v.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("{{\"d\":{},\"t\":{},\"n\":{},\"c\":\"{}\"}}", esc(d), t, n, route(*c)));
        }
        s.push_str("]}");
        s
    }

    pub fn flush(&mut self, now_ms: u64) {
        if !self.dirty || now_ms.saturating_sub(self.last_flush) < FLUSH_MS {
            return;
        }
        if let Some(p) = &self.path {
            let _ = sys::write_atomic(p, self.json(sys::wall_secs()).as_bytes());
        }
        self.dirty = false;
        self.last_flush = now_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_capped_escaped() {
        let mut r = Recent::new(Some(PathBuf::from("/dev/null")));
        for i in 0..(CAP + 5) {
            r.record(&format!("d{}.example.com", i), Class::Default, 1000 + i as u64);
        }
        assert_eq!(r.items.len(), CAP);
        assert!(!r.items.contains_key("d0.example.com"));
        r.record("d10.example.com", Class::Cn, 5000);
        r.record("x.in-addr.arpa", Class::Default, 6000);
        r.record("localhost", Class::Default, 6000);
        r.record("a\"b.com", Class::Overseas, 4000);
        let j = r.json(7000);
        assert!(j.starts_with("{\"v\":1,\"updated\":7000,\"items\":[{\"d\":\"d10.example.com\",\"t\":5000,\"n\":2,\"c\":\"direct\"}"));
        assert!(j.contains("\"a\\\"b.com\""));
        assert!(!j.contains("arpa") && !j.contains("localhost"));
    }
}
