//! 家里的设备叫什么：插件里起的别名 > 华硕网络地图里的备注名 > 按主机名和厂商自动认出来的名字。
//! 名字都按 MAC 记；IP 只用来在提醒里找到是哪台。程序里只有通用的识别规则，
//! 每家的别名存在各自路由器的插件设置里（dbus corpline_devices），不写进程序。
//!
//! 来源（都只读）：插件设备名单 names_file、华硕网络地图 /jffs/nmp_cl_json.js（主机名、厂商、类型）、
//! nvram custom_clientlist（华硕里改过的名字）、dnsmasq 租约、/proc/net/arp。
//! 整理后写一份 corpline_devices.json 给网页显示用。

use crate::config::Config;
use crate::report::{esc, Json};
use crate::sys;
use std::collections::{BTreeMap, HashMap};
use std::net::Ipv4Addr;

/// 后端自动加锁定设备时用的占位名，不当成用户起的别名
const LOCK_NAME: &str = "管理这台路由器的电脑";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dev {
    /// 插件里起的别名
    pub alias: String,
    /// 华硕里改过的名字
    pub nick: String,
    /// 设备自己报的主机名
    pub host: String,
    /// 网卡厂商（华硕按 MAC 前缀查的原文）
    pub vendor: String,
    /// 华硕判断的设备类型编号（0 = 不知道）
    pub kind: u32,
    /// 按主机名 / 厂商自动认出来的名字
    pub auto: String,
}

impl Dev {
    pub fn name(&self) -> Option<&str> {
        let alias = if self.alias == LOCK_NAME { "" } else { self.alias.as_str() };
        let host = if self.host == "*" { "" } else { self.host.as_str() };
        [alias, &self.nick, &self.auto, host, &self.alias].into_iter().find(|s| !s.is_empty())
    }
}

pub struct Names {
    devs: BTreeMap<String, Dev>,
    ip_mac: HashMap<Ipv4Addr, String>,
}

impl Names {
    pub fn load(cfg: &Config) -> Names {
        let mut n = Names { devs: BTreeMap::new(), ip_mac: HashMap::new() };
        if let Ok(s) = std::fs::read_to_string(&cfg.sec.client_db) {
            for (mac, f) in parse_client_db(&s) {
                let d = n.dev(&mac);
                let get = |k: &str| f.get(k).map(|v| v.trim().to_string()).unwrap_or_default();
                d.host = get("name");
                d.vendor = get("vendor");
                d.kind = get("type").parse().unwrap_or(0);
            }
        }
        if !cfg.sec.nick_cmd.is_empty() {
            let mut a = cfg.sec.nick_cmd.split_whitespace();
            if let Some(prog) = a.next() {
                let args: Vec<&str> = a.collect();
                let (ok, out) = sys::run(prog, &args);
                if ok {
                    for (mac, nick) in parse_nicks(&out) {
                        n.dev(&mac).nick = nick;
                    }
                }
            }
        }
        if let Ok(s) = std::fs::read_to_string(&cfg.sec.leases_file) {
            for l in s.lines() {
                let f: Vec<&str> = l.split_whitespace().collect();
                if f.len() >= 4 && valid_mac(f[1]) {
                    let mac = f[1].to_ascii_uppercase();
                    if let Ok(ip) = f[2].parse() {
                        n.ip_mac.insert(ip, mac.clone());
                    }
                    let d = n.dev(&mac);
                    // 华硕记的名字可能是它查到的厂商名（设备没报主机名时），租约里的才是设备自己报的
                    if f[3] != "*" && (d.host.is_empty() || d.host == d.vendor) {
                        d.host = f[3].to_string();
                    }
                }
            }
        }
        if let Ok(s) = std::fs::read_to_string("/proc/net/arp") {
            for l in s.lines().skip(1) {
                let f: Vec<&str> = l.split_whitespace().collect();
                if f.len() >= 4 && f[3] != "00:00:00:00:00:00" && valid_mac(f[3]) {
                    if let Ok(ip) = f[0].parse() {
                        n.ip_mac.insert(ip, f[3].to_ascii_uppercase());
                    }
                }
            }
        }
        if let Ok(s) = std::fs::read_to_string(&cfg.sec.names_file) {
            for l in s.lines() {
                if let Some((mac, name)) = l.split_once('\t') {
                    if valid_mac(mac) && !name.trim().is_empty() {
                        n.dev(&mac.to_ascii_uppercase()).alias = name.trim().to_string();
                    }
                }
            }
        }
        for (mac, d) in n.devs.iter_mut() {
            // 随机 MAC 按前缀查出来的厂商是瞎猜的；苹果设备华硕另有办法认出来，留着
            if is_random(mac) && known_brand(&d.vendor) != Some("Apple") {
                d.vendor.clear();
            }
            d.nick = clean_nick(&d.nick, &d.vendor, d.kind);
            d.auto = auto_name(&d.host, &d.vendor, d.kind);
        }
        n
    }

    fn dev(&mut self, mac: &str) -> &mut Dev {
        self.devs.entry(mac.to_ascii_uppercase()).or_default()
    }

    pub fn name(&self, ip: Ipv4Addr) -> Option<String> {
        self.ip_mac.get(&ip).and_then(|m| self.devs.get(m)).and_then(|d| d.name()).map(|s| s.to_string())
    }

    /// "「NAS」（192.168.50.168）"；不认识写"设备 192.168.50.168 "（带空格，直接接在中文前后）
    pub fn label(&self, ip: Ipv4Addr) -> String {
        match self.name(ip) {
            Some(n) => format!("「{}」（{}）", n, ip),
            None => format!("设备 {} ", ip),
        }
    }

    /// 命令行排查用：MAC、显示的名字、自动识别、主机名、厂商、随机 MAC
    pub fn table(&self) -> String {
        let mut s = String::new();
        for (mac, d) in &self.devs {
            s.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                mac,
                d.name().unwrap_or("-"),
                d.auto,
                d.host,
                vendor_short(&d.vendor),
                if is_random(mac) { "随机MAC" } else { "" }
            ));
        }
        s
    }

    /// 网页用：每台设备自动认出来的名字、主机名、厂商、是否随机 MAC（别名和华硕备注名网页自己有）
    pub fn json(&self) -> String {
        let mut s = String::from("{");
        let mut first = true;
        for (mac, d) in &self.devs {
            if d.auto.is_empty() && d.nick.is_empty() && d.host.is_empty() && d.vendor.is_empty() {
                continue;
            }
            if !first {
                s.push(',');
            }
            first = false;
            s.push_str(&esc(mac));
            s.push(':');
            let shown = if d.nick.is_empty() { &d.auto } else { &d.nick };
            s.push_str(&Json::obj().s("name", shown).s("auto", &d.auto).s("host", &d.host).s("vendor", &vendor_short(&d.vendor)).b("random", is_random(mac)).done());
        }
        s.push('}');
        Json::obj().n("t", sys::wall_secs()).raw("devices", s).done()
    }
}

pub fn valid_mac(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 17 && (0..17).all(|i| if i % 3 == 2 { b[i] == b':' } else { b[i].is_ascii_hexdigit() })
}

/// 随机 MAC（手机的「私有 Wi‑Fi 地址」等）：第一个字节的"本地管理"位为 1。
/// 这种地址换网络、重置或定期轮换后会变，按 MAC 记的设置会跟着失效。
pub fn is_random(mac: &str) -> bool {
    u8::from_str_radix(mac.get(0..2).unwrap_or("00"), 16).map(|b| b & 2 != 0).unwrap_or(false)
}

/// nvram custom_clientlist："<名字>MAC>…<名字>MAC>…"
fn parse_nicks(s: &str) -> Vec<(String, String)> {
    s.trim()
        .split('<')
        .filter_map(|e| {
            let mut f = e.split('>');
            let name = f.next()?.trim();
            let mac = f.next()?.trim();
            (valid_mac(mac) && !name.is_empty()).then(|| (mac.to_ascii_uppercase(), name.to_string()))
        })
        .collect()
}

/// 华硕网络地图的 JSON：{"MAC":{"name":"…","vendor":"…","type":10,…},…}；只取一层的字符串和数字
fn parse_client_db(s: &str) -> Vec<(String, HashMap<String, String>)> {
    let mut p = Parser { b: s.as_bytes(), i: 0 };
    let mut out = Vec::new();
    if let Some(V::Obj(top)) = p.value() {
        for (k, v) in top {
            if let (true, V::Obj(fields)) = (valid_mac(&k), v) {
                let m = fields
                    .into_iter()
                    .filter_map(|(k, v)| match v {
                        V::Str(s) => Some((k, s)),
                        V::Num(n) => Some((k, n)),
                        _ => None,
                    })
                    .collect();
                out.push((k.to_ascii_uppercase(), m));
            }
        }
    }
    out
}

enum V {
    Str(String),
    Num(String),
    Obj(Vec<(String, V)>),
    Other,
}

/// 够读华硕那几个 JSON 文件的小解析器；格式不对就返回 None，不 panic
struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.b.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn value(&mut self) -> Option<V> {
        self.ws();
        match *self.b.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut o = Vec::new();
                if self.eat(b'}') {
                    return Some(V::Obj(o));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    if !self.eat(b':') {
                        return None;
                    }
                    o.push((k, self.value()?));
                    if self.eat(b',') {
                        continue;
                    }
                    return self.eat(b'}').then_some(V::Obj(o));
                }
            }
            b'[' => {
                self.i += 1;
                if self.eat(b']') {
                    return Some(V::Other);
                }
                loop {
                    self.value()?;
                    if self.eat(b',') {
                        continue;
                    }
                    return self.eat(b']').then_some(V::Other);
                }
            }
            b'"' => self.string().map(V::Str),
            _ => {
                let st = self.i;
                while self.i < self.b.len() && !matches!(self.b[self.i], b',' | b'}' | b']') && !self.b[self.i].is_ascii_whitespace() {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.b[st..self.i]).ok()?;
                if t.is_empty() {
                    None
                } else if t.parse::<f64>().is_ok() {
                    Some(V::Num(t.to_string()))
                } else {
                    Some(V::Other)
                }
            }
        }
    }
    fn string(&mut self) -> Option<String> {
        if self.b.get(self.i) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = *self.b.get(self.i)?;
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' | b'f' => out.push(b' '),
                        b'u' => {
                            let h = std::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
                            self.i += 4;
                            let ch = u32::from_str_radix(h, 16).ok().and_then(char::from_u32).unwrap_or(' ');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        e => out.push(e),
                    }
                }
                c => out.push(c),
            }
        }
    }
}

// ---- 自动识别 ----

/// 米家生态的主机名前缀（品牌-品类-型号_miapXXXX 这种格式）
const MI_BRANDS: &[&str] = &[
    "xiaomi", "mijia", "chuangmi", "cuco", "chunmi", "zhimi", "yeelink", "lumi", "dreame", "roborock", "viomi", "dmaker", "qmi", "isa", "mmgg", "deerma", "yunmi", "miaomiaoce", "zimi", "qjiang", "nwt", "careli", "hfjh", "ijai", "xiaovv", "mxiang",
];

/// 主机名里的品类词 → 中文；先长后短，长的优先（dehumidifier 在 humidifier 前）
const KINDS: &[(&str, &str)] = &[
    ("dehumidifier", "除湿机"),
    ("humidifier", "加湿器"),
    ("airpurifier", "空气净化器"),
    ("aircondition", "空调"),
    ("acpartner", "空调伴侣"),
    ("airrtc", "空调"),
    ("purifier", "净化器"),
    ("waterpuri", "净水器"),
    ("toothbrush", "电动牙刷"),
    ("doorbell", "门铃"),
    ("camera", "摄像头"),
    ("cooker", "电饭煲"),
    ("wetdryvac", "洗地机"),
    ("vacuum", "扫地机器人"),
    ("sweeper", "扫地机器人"),
    ("gateway", "网关"),
    ("speaker", "音箱"),
    ("washer", "洗衣机"),
    ("dryer", "干衣机"),
    ("heater", "取暖器"),
    ("curtain", "窗帘"),
    ("outlet", "智能插座"),
    ("plug", "智能插座"),
    ("switch", "开关"),
    ("feeder", "喂食器"),
    ("kettle", "水壶"),
    ("oven", "烤箱"),
    ("fridge", "冰箱"),
    ("sensor", "传感器"),
    ("printer", "打印机"),
    ("router", "路由器"),
    ("light", "灯"),
    ("lamp", "灯"),
    ("bulb", "灯"),
    ("fan", "风扇"),
    ("tv", "电视"),
    ("lock", "门锁"),
];

/// 主机名开头的品牌词 → 显示名（手机等，后面一般跟型号）
const HOST_BRANDS: &[(&str, &str)] = &[
    ("honor", "荣耀"),
    ("huawei", "华为"),
    ("redmi", "Redmi"),
    ("xiaomi", "小米"),
    ("oppo", "OPPO"),
    ("vivo", "vivo"),
    ("oneplus", "一加"),
    ("realme", "realme"),
    ("iqoo", "iQOO"),
    ("meizu", "魅族"),
    ("galaxy", "Galaxy"),
    ("samsung", "三星"),
    ("nintendo", "任天堂"),
];

/// 只做联网模块的厂商：主机名多半是模块默认名，看不出是什么设备
const MODULE_VENDORS: &[&str] = &["espressif", "mxchip", "tuya", "beken", "bouffalo", "realtek semiconductor", "hui zhou gaoshengda", "azurewave"];
/// 模块的默认主机名，等于没报
const GENERIC_HOSTS: &[&str] = &["mico", "localhost", "android", "unknown", "espressif", "wlan0", "none"];

fn word_has(t: &str, key: &str) -> bool {
    t == key || (key.len() >= 4 && t.contains(key)) || (t.starts_with(key) && t[key.len()..].bytes().all(|c| c.is_ascii_digit()))
}

fn kind_word(tokens: &[String]) -> Option<&'static str> {
    KINDS.iter().find(|(k, _)| tokens.iter().any(|t| word_has(t, k))).map(|(_, v)| *v)
}

/// 像序列号 / MAC 尾巴的段：纯数字 ≥6 位，或十六进制 ≥4 位且同时有字母和数字
fn is_serial(t: &str) -> bool {
    let hex = t.bytes().all(|c| c.is_ascii_hexdigit());
    let digits = t.bytes().filter(|c| c.is_ascii_digit()).count();
    (digits == t.len() && t.len() >= 6) || (hex && t.len() >= 4 && digits > 0 && digits < t.len())
}

fn split_host(h: &str) -> Vec<String> {
    h.split(|c: char| c == '-' || c == '_' || c == ' ' || c == '.').filter(|t| !t.is_empty()).map(|t| t.to_string()).collect()
}

/// 认识的品牌：厂商原文里含这些词就用后面的名字
const BRANDS: &[(&str, &str)] = &[
    ("apple", "Apple"),
    ("samsung", "三星"),
    ("sony", "Sony"),
    ("nintendo", "任天堂"),
    ("microsoft", "Microsoft"),
    ("google", "Google"),
    ("huawei", "华为"),
    ("honor", "荣耀"),
    ("xiaomi", "小米"),
    ("lumi united", "Aqara"),
    ("synology", "群晖"),
    ("roborock", "石头科技"),
    ("belkin", "Belkin"),
    ("tp-link", "TP-Link"),
    ("asustek", "ASUS"),
    ("amazon", "Amazon"),
    ("d-link", "D-Link"),
    ("intel", "Intel"),
    ("lenovo", "联想"),
    ("dell", "Dell"),
    ("hewlett", "HP"),
    ("oppo", "OPPO"),
    ("vivo", "vivo"),
    ("oneplus", "一加"),
    ("espressif", "乐鑫"),
    ("tuya", "涂鸦"),
    ("mxchip", "庆科"),
];

fn known_brand(v: &str) -> Option<&'static str> {
    let l = v.to_ascii_lowercase();
    BRANDS.iter().find(|(k, _)| l.contains(k)).map(|(_, b)| *b)
}

/// 厂商原文 → 简短的品牌名；"Others"、空 → ""
pub fn vendor_short(v: &str) -> String {
    let l = v.to_ascii_lowercase();
    if l.is_empty() || l == "others" || l == "unknown" {
        return String::new();
    }
    if let Some(b) = known_brand(v) {
        return b.to_string();
    }
    // 其它：去掉公司后缀，最多留两个词
    const SUFFIX: &[&str] = &[
        "inc", "inc.", "corporation", "corp", "corp.", "co", "co.", "co.,ltd", "co.,ltd.", "ltd", "ltd.", "limited", "group", "international", "incorporated", "technology",
        "technologies", "electronics", "information", "systems", "engineering", "gmbh", "ag", "llc", "s.a.", "communication", "communications",
    ];
    let head = v.split(',').next().unwrap_or(v);
    let mut w: Vec<&str> = head.split_whitespace().collect();
    while w.len() > 1 && SUFFIX.contains(&w[w.len() - 1].to_ascii_lowercase().as_str()) {
        w.pop();
    }
    w.truncate(2);
    w.join(" ")
}

/// 像公司全称（"Nintendo Co  Ltd"、"XIAOMI Electronics,CO., LTD."）：华硕没主机名时拿厂商名顶上的
fn looks_company(s: &str) -> bool {
    let w: Vec<String> = s.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).map(|t| t.trim_end_matches('.').to_ascii_lowercase()).collect();
    w.len() >= 2 && ["inc", "corporation", "corp", "co", "ltd", "limited", "incorporated", "gmbh", "llc", "ag", "s.a"].contains(&w[w.len() - 1].as_str())
}

/// 华硕里的备注名：公司全称当没起；机器生成的（"cuco-plug-v3_mibt5D6E"）换成认出来的名字
fn clean_nick(nick: &str, vendor: &str, kind: u32) -> String {
    let n = nick.trim();
    if n.is_empty() || looks_company(n) {
        return String::new();
    }
    let a = auto_name(n, vendor, kind);
    if a.is_empty() {
        n.to_string()
    } else {
        a
    }
}

fn phone_vendor(b: &str) -> bool {
    ["三星", "Google", "华为", "荣耀", "小米", "OPPO", "vivo", "一加"].contains(&b)
}

/// 只有厂商和类型时的名字："Apple iPhone"、"Sony 游戏机"、"Wistron Neweb 设备"
fn by_vendor(vendor: &str, kind: u32) -> String {
    let b = vendor_short(vendor);
    if b.is_empty() {
        return String::new();
    }
    let l = vendor.to_ascii_lowercase();
    if MODULE_VENDORS.iter().any(|m| l.contains(m)) {
        return "智能家居设备".into();
    }
    let what = match (b.as_str(), kind) {
        ("Apple", 10) => "iPhone",
        ("Apple", 21) => "iPad",
        ("Apple", 6) | ("Apple", 14) => "Mac",
        ("Apple", 11) => "Apple TV",
        ("Sony", 7) | ("任天堂", 7) | ("Microsoft", 7) => "游戏机",
        (x, 9) if phone_vendor(x) => "手机",
        _ => "设备",
    };
    // 中英文之间空一格，中文和中文之间不空（"Sony 游戏机"、"三星手机"）
    if b.is_ascii() || what.is_ascii() {
        format!("{} {}", b, what)
    } else {
        format!("{}{}", b, what)
    }
}

/// 按主机名、厂商、华硕类型猜一个让人看得懂的名字；什么都没有返回 ""
pub fn auto_name(host: &str, vendor: &str, kind: u32) -> String {
    let h = host.trim();
    let hl = h.to_ascii_lowercase();
    let vl = vendor.trim().to_ascii_lowercase();
    // 没报主机名：华硕会拿厂商名顶上；模块默认名也等于没报
    let no_host = h.is_empty()
        || h == "*"
        || (!vl.is_empty() && (hl == vl || vl.starts_with(&hl) && hl.len() >= 6))
        || looks_company(h)
        || GENERIC_HOSTS.contains(&hl.as_str())
        || hl.starts_with("esp_")
        || hl.starts_with("esp-");
    if no_host {
        return by_vendor(vendor, kind);
    }
    let lower: Vec<String> = split_host(&hl);
    let orig: Vec<String> = split_host(h);
    // 米家：品牌-品类-型号_miapXXXX / xiaomi_camera_xxxxxx
    let mi_tail = lower.iter().any(|t| {
        t.len() == 8 && t.starts_with("mi") && (t[2..4] == *"ap" || t[2..4] == *"bt" || t[2..4] == *"io") && t[4..].bytes().all(|c| c.is_ascii_hexdigit())
    });
    if mi_tail || lower.first().map(|t| MI_BRANDS.contains(&t.as_str())).unwrap_or(false) {
        // 有自己品牌的生态链厂商用自己的牌子，其它都叫米家
        let brand = match lower[0].as_str() {
            "roborock" => "石头",
            "dreame" => "追觅",
            "viomi" => "云米",
            "yeelink" => "Yeelight",
            "lumi" => "Aqara",
            _ => "米家",
        };
        let what = kind_word(&lower).unwrap_or("设备");
        return if brand.is_ascii() { format!("{} {}", brand, what) } else { format!("{}{}", brand, what) };
    }
    // Linksys12345（Velop 节点的默认名）
    if hl.starts_with("linksys") && hl.len() > 7 && hl[7..].bytes().all(|c| c.is_ascii_digit()) {
        return "Linksys 路由器".into();
    }
    if hl.starts_with("android") && lower.len() <= 2 {
        return "安卓设备".into();
    }
    // 去掉结尾的序列号 / MAC 段（第一段保留）
    let mut keep = orig.len();
    while keep > 1 && is_serial(&lower[keep - 1]) {
        keep -= 1;
    }
    if keep == orig.len() {
        // 主机名本来就是人起的（"Livingroom"、"Alex-MacBook-Pro"），原样用
        return h.to_string();
    }
    let mut words: Vec<String> = orig[..keep].to_vec();
    if let Some((_, b)) = HOST_BRANDS.iter().find(|(k, _)| lower[0] == *k) {
        words[0] = b.to_string();
    } else {
        // 机器生成的名字：前面补上厂商品牌（"Aqara Camera Hub G3"）
        let l = vendor.to_ascii_lowercase();
        if let Some(b) = known_brand(vendor) {
            if b.is_ascii() && !MODULE_VENDORS.iter().any(|m| l.contains(m)) && !words.iter().any(|w| w.eq_ignore_ascii_case(b)) {
                words.insert(0, b.to_string());
            }
        }
    }
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_from_hosts() {
        let a = |h: &str, v: &str, k: u32| auto_name(h, v, k);
        assert_eq!(a("cuco-plug-v3_mibt1A2B", "", 70), "米家智能插座");
        assert_eq!(a("chuangmi-plug-v3_miap3C4D", "", 1), "米家智能插座");
        assert_eq!(a("chunmi-cooker-normal2_miapA1B2", "XIAOMI Electronics,CO.,LTD", 52), "米家电饭煲");
        assert_eq!(a("xiaomi_camera_5e6f70", "D-Link Systems, Inc.", 5), "米家摄像头");
        assert_eq!(a("zhimi-airpurifier-ma4_miio1234", "", 0), "米家空气净化器");
        assert_eq!(a("HONOR_X40-1a2b3c4d5e", "Others", 34), "荣耀 X40");
        assert_eq!(a("Camera-Hub-G3-7B8C", "Lumi United Technology Co., Ltd", 5), "Aqara Camera Hub G3");
        assert_eq!(a("BeoPlay-A9-12345678", "StreamUnlimited Engineering GmbH", 5), "BeoPlay A9");
        assert_eq!(a("FC9000_A1B2", "Seoul Commtech", 7), "FC9000");
        assert_eq!(a("Linksys01234", "Belkin International Inc.", 5), "Linksys 路由器");
        assert_eq!(a("Livingroom", "Apple, Inc.", 10), "Livingroom");
        assert_eq!(a("Alex-de-Z-Fold7", "Samsung Group", 9), "Alex-de-Z-Fold7");
        assert_eq!(a("iPhone", "Apple, Inc.", 10), "iPhone");
        assert_eq!(a("my-smart-home", "Texas Instruments", 2), "my-smart-home");
        assert_eq!(a("Desk-2023", "", 0), "Desk-2023");
        assert_eq!(a("android-1a2b3c4d5e6f7a8b", "Google Inc.", 9), "安卓设备");
        assert_eq!(a("roborock-wetdryvac-b2c4", "Beijing Roborock Technology Co., Ltd.", 0), "石头洗地机");
        assert_eq!(a("lumi-gateway-v3_miio12345678", "", 0), "Aqara 网关");
        assert_eq!(a("XIAOMI Electronics,CO., LTD.", "XIAOMI Electronics,CO.,LTD", 0), "小米设备");
        assert_eq!(clean_nick("cuco-plug-v3_mibt5D6E", "", 70), "米家智能插座");
        assert_eq!(clean_nick("Nintendo Co  Ltd", "", 0), "");
        assert_eq!(clean_nick("PS5 Pro", "Sony Corporation", 7), "PS5 Pro");
        assert_eq!(clean_nick("Linksys Study", "Belkin International Inc.", 5), "Linksys Study");
    }

    #[test]
    fn names_without_hosts() {
        let a = |h: &str, v: &str, k: u32| auto_name(h, v, k);
        assert_eq!(a("", "Sony Corporation", 7), "Sony 游戏机");
        assert_eq!(a("*", "Apple, Inc.", 10), "Apple iPhone");
        assert_eq!(a("Wistron Neweb Corporation", "Wistron Neweb Corporation", 9), "Wistron Neweb 设备");
        assert_eq!(a("MiCO", "Shanghai MXCHIP Information Technology Co., Ltd.", 0), "智能家居设备");
        assert_eq!(a("", "Samsung Group", 9), "三星手机");
        assert_eq!(a("", "Others", 34), "");
        assert_eq!(a("", "", 0), "");
    }

    #[test]
    fn random_mac() {
        assert!(is_random("DA:00:00:11:22:33"));
        assert!(is_random("52:00:00:44:55:66"));
        assert!(!is_random("00:11:22:00:00:02"));
        assert!(!is_random("90:00:00:AA:BB:CC"));
    }

    #[test]
    fn reads_asus_files() {
        let db = r#"{"00:11:22:00:00:03":{"mac":"00:11:22:00:00:03","name":"iPhone","vendor":"Apple, Inc.","type":10,"os_type":5,"online":1},
            "bad":{"name":"x"},"0a:bb:22:00:00:02":{"name":"","vendor":"Sony Corporation","type":7,"list":[1,{"a":"b"}],"x":null,"y":"a\"b中"}}"#;
        let v = parse_client_db(db);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].0, "00:11:22:00:00:03");
        assert_eq!(v[0].1.get("type").map(|s| s.as_str()), Some("10"));
        assert_eq!(v[1].0, "0A:BB:22:00:00:02");
        assert_eq!(v[1].1.get("y").map(|s| s.as_str()), Some("a\"b中"));
        assert!(parse_client_db("{\"a\":").is_empty());
        let n = parse_nicks("<Game Console>00:11:22:00:00:04>0>74>>>><书房的路由器>00:11:22:00:00:05>0>0>><>11:22:33:44:55:66>0<bad>zz\n");
        assert_eq!(n, vec![("00:11:22:00:00:04".to_string(), "Game Console".to_string()), ("00:11:22:00:00:05".to_string(), "书房的路由器".to_string())]);
    }

    #[test]
    fn name_priority() {
        let mut d = Dev { host: "cuco-plug-v3_mibt1A2B".into(), auto: "米家智能插座".into(), ..Default::default() };
        assert_eq!(d.name(), Some("米家智能插座"));
        d.nick = "餐厅插座".into();
        assert_eq!(d.name(), Some("餐厅插座"));
        d.alias = "鱼缸插座".into();
        assert_eq!(d.name(), Some("鱼缸插座"));
        let mut m = Dev { alias: LOCK_NAME.into(), ..Default::default() };
        assert_eq!(m.name(), Some(LOCK_NAME));
        m.auto = "Mac".into();
        assert_eq!(m.name(), Some("Mac"));
    }
}
