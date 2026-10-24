//! 运行日志与"最近发生了什么"时间线。全部写在内存盘（/tmp），不写闪存。
//! 日志两段轮转，每段 128 KB；时间线保留最近 200 条，守护程序重启后接着用。

use crate::machine::Level;
use crate::sys;
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const LOG_SEGMENT: u64 = 128 * 1024;
const MAX_EVENTS: usize = 200;

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init_log(path: PathBuf) {
    *LOG_PATH.lock().unwrap() = Some(path);
}

pub fn log(text: &str) {
    let line = format!("{} {}\n", sys::hhmmss(sys::wall_secs()), text.trim_end());
    let guard = LOG_PATH.lock().unwrap();
    match guard.as_ref() {
        Some(p) => {
            if std::fs::metadata(p).map(|m| m.len() > LOG_SEGMENT).unwrap_or(false) {
                let _ = std::fs::rename(p, p.with_extension("txt.1"));
            }
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
                let _ = f.write_all(line.as_bytes());
            }
        }
        None => eprint!("{}", line),
    }
}

#[derive(Clone, Debug)]
pub struct Ev {
    pub t: u64,
    pub level: Level,
    pub text: String,
}

pub struct Timeline {
    path: PathBuf,
    pub items: VecDeque<Ev>,
}

impl Timeline {
    pub fn load(path: PathBuf) -> Timeline {
        let mut items = VecDeque::new();
        if let Ok(s) = std::fs::read_to_string(&path) {
            for l in s.lines() {
                let mut it = l.splitn(3, '\t');
                let (Some(t), Some(lv), Some(text)) = (it.next(), it.next(), it.next()) else { continue };
                let level = match lv {
                    "ok" => Level::Ok,
                    "warn" => Level::Warn,
                    "err" => Level::Err,
                    _ => Level::Info,
                };
                if let Ok(t) = t.parse() {
                    items.push_back(Ev { t, level, text: text.to_string() });
                }
            }
        }
        while items.len() > MAX_EVENTS {
            items.pop_front();
        }
        Timeline { path, items }
    }

    pub fn push(&mut self, level: Level, text: &str) {
        log(&format!("[{}] {}", level.key(), text));
        let ev = Ev { t: sys::wall_secs(), level, text: text.replace(['\t', '\n'], " ") };
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = writeln!(f, "{}\t{}\t{}", ev.t, level.key(), ev.text);
        }
        self.items.push_back(ev);
        if self.items.len() > MAX_EVENTS {
            self.items.pop_front();
            self.compact();
        }
    }

    fn compact(&self) {
        let mut s = String::new();
        for e in &self.items {
            s.push_str(&format!("{}\t{}\t{}\n", e.t, e.level.key(), e.text));
        }
        let _ = sys::write_atomic(&self.path, s.as_bytes());
    }
}

/// 极简 JSON 构造（不引入 serde，控制体积）
pub struct Json(String);

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

impl Json {
    pub fn obj() -> Json {
        Json(String::from("{"))
    }
    fn key(&mut self, k: &str) {
        if self.0.len() > 1 {
            self.0.push(',');
        }
        self.0.push_str(&esc(k));
        self.0.push(':');
    }
    pub fn s(mut self, k: &str, v: &str) -> Json {
        self.key(k);
        self.0.push_str(&esc(v));
        self
    }
    pub fn n(mut self, k: &str, v: impl std::fmt::Display) -> Json {
        self.key(k);
        self.0.push_str(&v.to_string());
        self
    }
    pub fn b(mut self, k: &str, v: bool) -> Json {
        self.key(k);
        self.0.push_str(if v { "true" } else { "false" });
        self
    }
    pub fn ob(mut self, k: &str, v: Option<bool>) -> Json {
        self.key(k);
        self.0.push_str(match v {
            Some(true) => "true",
            Some(false) => "false",
            None => "null",
        });
        self
    }
    pub fn raw(mut self, k: &str, v: String) -> Json {
        self.key(k);
        self.0.push_str(&v);
        self
    }
    pub fn o(self, k: &str, v: Json) -> Json {
        self.raw(k, v.done())
    }
    pub fn done(mut self) -> String {
        self.0.push('}');
        self.0
    }
}

pub fn events_json(items: &VecDeque<Ev>, n: usize) -> String {
    let mut s = String::from("[");
    for (i, e) in items.iter().rev().take(n).enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&Json::obj().n("t", e.t).s("l", e.level.key()).s("m", &e.text).done());
    }
    s.push(']');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escape() {
        let j = Json::obj().s("a", "x\"y\n").n("n", 3).b("b", true).ob("c", None).done();
        assert_eq!(j, r#"{"a":"x\"y\n","n":3,"b":true,"c":null}"#);
    }

    #[test]
    fn timeline_persists() {
        let dir = std::env::temp_dir().join(format!("corpline-tl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("events.txt");
        let mut t = Timeline::load(p.clone());
        for i in 0..205 {
            t.push(Level::Info, &format!("e{}", i));
        }
        assert_eq!(t.items.len(), 200);
        let t2 = Timeline::load(p);
        assert_eq!(t2.items.len(), 200);
        assert_eq!(t2.items.back().unwrap().text, "e204");
        std::fs::remove_dir_all(dir).ok();
    }
}
