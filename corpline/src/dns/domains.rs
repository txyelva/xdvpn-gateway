//! 紧凑的域名后缀集合：所有域名排序后拼在一块连续内存里，用偏移数组二分查找。
//! 11 万条中国服务名单约占 2 MB，比 HashSet<String> 省一半以上。

pub struct SuffixSet {
    buf: Vec<u8>,
    offs: Vec<u32>,
}

impl SuffixSet {
    pub fn new<I: IntoIterator<Item = String>>(items: I) -> SuffixSet {
        let mut v: Vec<String> = items
            .into_iter()
            .map(|s| s.trim().trim_matches('.').to_ascii_lowercase())
            .filter(|s| !s.is_empty() && s.len() <= 253)
            .collect();
        v.sort_unstable();
        v.dedup();
        let mut buf = Vec::with_capacity(v.iter().map(|s| s.len()).sum());
        let mut offs = Vec::with_capacity(v.len() + 1);
        for s in &v {
            offs.push(buf.len() as u32);
            buf.extend_from_slice(s.as_bytes());
        }
        offs.push(buf.len() as u32);
        buf.shrink_to_fit();
        SuffixSet { buf, offs }
    }

    pub fn empty() -> SuffixSet {
        SuffixSet::new(Vec::new())
    }

    pub fn len(&self) -> usize {
        self.offs.len() - 1
    }

    pub fn bytes(&self) -> usize {
        self.buf.len() + self.offs.len() * 4
    }

    fn get(&self, i: usize) -> &[u8] {
        &self.buf[self.offs[i] as usize..self.offs[i + 1] as usize]
    }

    fn contains_exact(&self, s: &[u8]) -> bool {
        let (mut lo, mut hi) = (0usize, self.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.get(mid).cmp(s) {
                std::cmp::Ordering::Equal => return true,
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        false
    }

    /// name 本身或它的任一上级域名在集合里（name 已是小写）
    pub fn matches(&self, name: &str) -> bool {
        if self.len() == 0 {
            return false;
        }
        let b = name.as_bytes();
        let mut start = 0;
        loop {
            if self.contains_exact(&b[start..]) {
                return true;
            }
            match b[start..].iter().position(|&c| c == b'.') {
                Some(p) => start += p + 1,
                None => return false,
            }
        }
    }

    /// 读取 dnsmasq-china-list 格式（server=/域名/IP）或每行一个域名
    pub fn load_file(path: &std::path::Path) -> std::io::Result<SuffixSet> {
        let text = std::fs::read_to_string(path)?;
        Ok(SuffixSet::new(text.lines().filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            if let Some(rest) = l.strip_prefix("server=/") {
                return rest.split('/').next().map(|s| s.to_string());
            }
            Some(l.to_string())
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_match() {
        let s = SuffixSet::new(vec!["apple.com".into(), "MZSTATIC.com.".into(), "baidu.com".into()]);
        assert_eq!(s.len(), 3);
        assert!(s.matches("apple.com"));
        assert!(s.matches("apps.apple.com"));
        assert!(s.matches("is1-ssl.mzstatic.com"));
        assert!(!s.matches("notapple.com"));
        assert!(!s.matches("com"));
        assert!(!s.matches("apple.com.cn"));
        assert!(!SuffixSet::empty().matches("a.b"));
    }

    #[test]
    fn china_list_format() {
        let dir = std::env::temp_dir().join(format!("corpline-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("l.conf");
        std::fs::write(&p, "#x\nserver=/0-100.com/114.114.114.114\nserver=/qq.com/114.114.114.114\n\n").unwrap();
        let s = SuffixSet::load_file(&p).unwrap();
        assert!(s.matches("im.qq.com"));
        assert!(s.matches("0-100.com"));
        std::fs::remove_dir_all(dir).ok();
    }
}
