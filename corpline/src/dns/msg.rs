//! 最小 DNS 报文处理：只解析转发决策需要的部分，不做完整的报文模型。

use std::net::Ipv4Addr;

pub const TYPE_A: u16 = 1;
pub const TYPE_AAAA: u16 = 28;
pub const TYPE_OPT: u16 = 41;
pub const RCODE_SERVFAIL: u8 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
    /// 问题段结束位置
    pub end: usize,
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?]))
}

pub fn id(b: &[u8]) -> Option<u16> {
    u16_at(b, 0)
}

pub fn set_id(b: &mut [u8], id: u16) {
    if b.len() >= 2 {
        b[..2].copy_from_slice(&id.to_be_bytes());
    }
}

pub fn is_response(b: &[u8]) -> bool {
    b.len() >= 12 && b[2] & 0x80 != 0
}

pub fn rcode(b: &[u8]) -> u8 {
    if b.len() >= 4 {
        b[3] & 0x0f
    } else {
        RCODE_SERVFAIL
    }
}

pub fn ancount(b: &[u8]) -> u16 {
    u16_at(b, 6).unwrap_or(0)
}

/// 读名字（处理压缩指针），返回 (小写名字, 名字在报文中占用的字节后的位置)
fn read_name(b: &[u8], mut i: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut end = None;
    let mut jumps = 0;
    loop {
        let len = *b.get(i)? as usize;
        if len == 0 {
            i += 1;
            break;
        }
        if len & 0xc0 == 0xc0 {
            let ptr = (u16_at(b, i)? & 0x3fff) as usize;
            if end.is_none() {
                end = Some(i + 2);
            }
            jumps += 1;
            if jumps > 16 {
                return None;
            }
            i = ptr;
            continue;
        }
        if len > 63 {
            return None;
        }
        let label = b.get(i + 1..i + 1 + len)?;
        if !name.is_empty() {
            name.push('.');
        }
        for &c in label {
            name.push((c as char).to_ascii_lowercase());
        }
        if name.len() > 255 {
            return None;
        }
        i += 1 + len;
    }
    Some((name, end.unwrap_or(i)))
}

fn skip_name(b: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let len = *b.get(i)? as usize;
        if len == 0 {
            return Some(i + 1);
        }
        if len & 0xc0 == 0xc0 {
            b.get(i + 1)?;
            return Some(i + 2);
        }
        i += 1 + len;
    }
}

pub fn question(b: &[u8]) -> Option<Question> {
    if b.len() < 12 || u16_at(b, 4)? != 1 {
        return None;
    }
    let (name, i) = read_name(b, 12)?;
    let qtype = u16_at(b, i)?;
    let qclass = u16_at(b, i + 2)?;
    Some(Question { name, qtype, qclass, end: i + 4 })
}

/// 遍历全部资源记录：回调 (段号 0=answer 1=authority 2=additional, 类型, TTL 字段位置, rdata 起止)
fn for_each_rr(b: &[u8], mut f: impl FnMut(usize, u16, usize, usize, usize)) -> Option<()> {
    let q = question(b)?;
    let counts = [u16_at(b, 6)?, u16_at(b, 8)?, u16_at(b, 10)?];
    let mut i = q.end;
    for (sec, &n) in counts.iter().enumerate() {
        for _ in 0..n {
            i = skip_name(b, i)?;
            let t = u16_at(b, i)?;
            let ttl_at = i + 4;
            let rdlen = u16_at(b, i + 8)? as usize;
            let rd = i + 10;
            if rd + rdlen > b.len() {
                return None;
            }
            f(sec, t, ttl_at, rd, rd + rdlen);
            i = rd + rdlen;
        }
    }
    Some(())
}

/// 应答里的 A 记录 (地址, TTL)
pub fn a_records(b: &[u8]) -> Vec<(Ipv4Addr, u32)> {
    let mut out = Vec::new();
    let _ = for_each_rr(b, |sec, t, ttl_at, s, e| {
        if sec == 0 && t == TYPE_A && e - s == 4 {
            let ttl = u32_at(b, ttl_at).unwrap_or(0);
            out.push((Ipv4Addr::new(b[s], b[s + 1], b[s + 2], b[s + 3]), ttl));
        }
    });
    out
}

/// 把所有记录的 TTL 压到不超过 cap（OPT 记录的 TTL 字段是扩展标志，跳过）
pub fn cap_ttl(b: &mut [u8], cap: u32) {
    let mut spots = Vec::new();
    let _ = for_each_rr(b, |_, t, ttl_at, _, _| {
        if t != TYPE_OPT {
            spots.push(ttl_at);
        }
    });
    for at in spots {
        if let Some(v) = u32_at(b, at) {
            if v > cap {
                b[at..at + 4].copy_from_slice(&cap.to_be_bytes());
            }
        }
    }
}

/// 用查询报文构造一个无记录的应答（rcode 指定），只保留问题段
pub fn reply_empty(query: &[u8], rc: u8) -> Option<Vec<u8>> {
    let q = question(query)?;
    let mut r = query[..q.end].to_vec();
    r[2] = 0x80 | (query[2] & 0x01); // QR + 保留 RD
    r[3] = 0x80 | (rc & 0x0f); // RA + rcode
    r[6..12].copy_from_slice(&[0, 0, 0, 0, 0, 0]);
    Some(r)
}

pub fn build_query(id: u16, name: &str, qtype: u16) -> Vec<u8> {
    let mut b = Vec::with_capacity(32 + name.len());
    b.extend_from_slice(&id.to_be_bytes());
    b.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.trim_matches('.').split('.') {
        if label.is_empty() {
            continue;
        }
        let l = &label.as_bytes()[..label.len().min(63)];
        b.push(l.len() as u8);
        b.extend_from_slice(l);
    }
    b.push(0);
    b.extend_from_slice(&qtype.to_be_bytes());
    b.extend_from_slice(&1u16.to_be_bytes());
    b
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// 构造一个带 A 记录与 OPT 的应答
    pub fn answer(id: u16, name: &str, ips: &[([u8; 4], u32)]) -> Vec<u8> {
        let mut b = build_query(id, name, TYPE_A);
        b[2] = 0x81;
        b[3] = 0x80;
        b[6..8].copy_from_slice(&(ips.len() as u16).to_be_bytes());
        b[10..12].copy_from_slice(&1u16.to_be_bytes());
        for (ip, ttl) in ips {
            b.extend_from_slice(&[0xc0, 12, 0, 1, 0, 1]);
            b.extend_from_slice(&ttl.to_be_bytes());
            b.extend_from_slice(&[0, 4]);
            b.extend_from_slice(ip);
        }
        // OPT：TTL 字段 = 扩展标志（DO 位），不能被改写
        b.extend_from_slice(&[0, 0, 41, 0x10, 0, 0, 0, 0x80, 0, 0, 0]);
        b
    }

    #[test]
    fn roundtrip_question() {
        let q = build_query(7, "Www.Example.COM.", TYPE_AAAA);
        let p = question(&q).unwrap();
        assert_eq!(p.name, "www.example.com");
        assert_eq!(p.qtype, TYPE_AAAA);
        assert_eq!(p.end, q.len());
        assert!(!is_response(&q));
    }

    #[test]
    fn parse_answers_and_cap() {
        let mut a = answer(9, "a.cn", &[([1, 2, 3, 4], 3600), ([5, 6, 7, 8], 20)]);
        assert_eq!(a_records(&a), vec![(Ipv4Addr::new(1, 2, 3, 4), 3600), (Ipv4Addr::new(5, 6, 7, 8), 20)]);
        cap_ttl(&mut a, 30);
        assert_eq!(a_records(&a), vec![(Ipv4Addr::new(1, 2, 3, 4), 30), (Ipv4Addr::new(5, 6, 7, 8), 20)]);
        let n = a.len();
        assert_eq!(&a[n - 6..n - 2], &[0, 0, 0x80, 0], "OPT 扩展标志不变");
    }

    #[test]
    fn empty_reply() {
        let q = build_query(0x1234, "x.y", TYPE_AAAA);
        let r = reply_empty(&q, 0).unwrap();
        assert!(is_response(&r));
        assert_eq!(id(&r), Some(0x1234));
        assert_eq!(rcode(&r), 0);
        assert_eq!(ancount(&r), 0);
        assert_eq!(question(&r).unwrap().name, "x.y");
    }

    #[test]
    fn garbage_is_safe() {
        assert!(question(&[0u8; 5]).is_none());
        let mut bad = build_query(1, "a.b", 1);
        bad[12] = 0xc0;
        bad[13] = 12; // 自指针循环
        assert!(question(&bad).is_none());
        assert!(a_records(&[0xffu8; 40]).is_empty());
        let mut x = vec![0xffu8; 40];
        cap_ttl(&mut x, 1);
    }
}
