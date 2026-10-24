//! 转发决策（纯函数，可单测）：这个域名该问谁、按什么顺序、谁的回答可以直接用。

use super::domains::SuffixSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// 公司 DNS（经隧道）
    Company,
    /// 海外公共 DNS（经隧道）
    Overseas,
    /// 运营商 DNS（直连）
    Isp,
}

impl Kind {
    pub fn via_tunnel(self) -> bool {
        !matches!(self, Kind::Isp)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// 公司内网域名：只问公司 DNS
    Internal,
    /// 用海外解析的例外（默认 Apple）
    Overseas,
    /// 中国服务名单（仅"国内网站直连"开启时）
    Cn,
    /// 其余全部
    Default,
}

pub fn classify(name: &str, internal: &SuffixSet, overseas: &SuffixSet, cn: Option<&SuffixSet>) -> Class {
    if internal.matches(name) {
        return Class::Internal;
    }
    if overseas.matches(name) {
        return Class::Overseas;
    }
    if let Some(cn) = cn {
        if cn.matches(name) {
            return Class::Cn;
        }
    }
    Class::Default
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tier {
    pub kind: Kind,
    /// 距查询开始多少毫秒后发出（前一级不健康时提前到 0）
    pub delay_ms: u64,
}

pub const HEDGE_MS: u64 = 300;

/// tunneled = 当前家里流量走公司线路（总阀门打开）
pub fn plan(class: Class, tunneled: bool) -> Vec<Tier> {
    let t = |kind, delay_ms| Tier { kind, delay_ms };
    if !tunneled {
        return vec![t(Kind::Isp, 0)];
    }
    match class {
        Class::Internal => vec![t(Kind::Company, 0)],
        Class::Overseas => vec![t(Kind::Overseas, 0), t(Kind::Company, HEDGE_MS)],
        Class::Cn => vec![t(Kind::Isp, 0), t(Kind::Company, HEDGE_MS)],
        Class::Default => vec![t(Kind::Company, 0), t(Kind::Overseas, HEDGE_MS)],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// 直接回给客户端
    Accept,
    /// 先留着，等更优先的上游；都失败时再用
    Hold,
    /// 无效（SERVFAIL / REFUSED），算该上游一次失败
    Fail,
}

/// company_pending：公司 DNS 是否还有可能给出回答
pub fn judge(class: Class, tunneled: bool, from: Kind, rcode: u8, ancount: u16, company_pending: bool) -> Verdict {
    if rcode == 2 || rcode == 5 {
        return Verdict::Fail;
    }
    if !tunneled {
        return Verdict::Accept;
    }
    match (class, from) {
        // 普通域名的主力是公司 DNS；海外 DNS 只在它有正经答案、或公司 DNS 已无望时采用，
        // 避免公司内网未登记的域名被海外 DNS 抢先回 NXDOMAIN。
        (Class::Default, Kind::Overseas) => {
            if (rcode == 0 && ancount > 0) || !company_pending {
                Verdict::Accept
            } else {
                Verdict::Hold
            }
        }
        _ => Verdict::Accept,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sets() -> (SuffixSet, SuffixSet, SuffixSet) {
        (
            SuffixSet::new(vec!["xindong.com".into()]),
            SuffixSet::new(vec!["apple.com".into(), "mzstatic.com".into()]),
            SuffixSet::new(vec!["baidu.com".into(), "qq.com".into()]),
        )
    }

    #[test]
    fn classes() {
        let (i, o, c) = sets();
        assert_eq!(classify("git.xindong.com", &i, &o, Some(&c)), Class::Internal);
        assert_eq!(classify("apps.apple.com", &i, &o, Some(&c)), Class::Overseas);
        assert_eq!(classify("www.baidu.com", &i, &o, Some(&c)), Class::Cn);
        assert_eq!(classify("www.baidu.com", &i, &o, None), Class::Default, "国内直连关闭时名单不生效");
        assert_eq!(classify("www.google.com", &i, &o, Some(&c)), Class::Default);
    }

    #[test]
    fn plans() {
        assert_eq!(plan(Class::Default, false), vec![Tier { kind: Kind::Isp, delay_ms: 0 }]);
        assert_eq!(plan(Class::Overseas, false)[0].kind, Kind::Isp, "直连期间一律问运营商");
        let p = plan(Class::Default, true);
        assert_eq!(p[0].kind, Kind::Company);
        assert_eq!(p[1], Tier { kind: Kind::Overseas, delay_ms: HEDGE_MS });
        assert_eq!(plan(Class::Overseas, true)[0].kind, Kind::Overseas);
        assert_eq!(plan(Class::Cn, true)[0].kind, Kind::Isp);
        assert!(plan(Class::Internal, true).iter().all(|t| t.kind == Kind::Company));
        assert!(!plan(Class::Default, true).iter().any(|t| t.kind == Kind::Isp), "隧道在时不问运营商，防污染");
    }

    #[test]
    fn verdicts() {
        use Verdict::*;
        assert_eq!(judge(Class::Default, true, Kind::Company, 3, 0, false), Accept);
        assert_eq!(judge(Class::Default, true, Kind::Overseas, 3, 0, true), Hold);
        assert_eq!(judge(Class::Default, true, Kind::Overseas, 3, 0, false), Accept);
        assert_eq!(judge(Class::Default, true, Kind::Overseas, 0, 2, true), Accept);
        assert_eq!(judge(Class::Default, true, Kind::Company, 2, 0, false), Fail);
        assert_eq!(judge(Class::Overseas, true, Kind::Overseas, 0, 1, true), Accept);
        assert_eq!(judge(Class::Default, false, Kind::Isp, 5, 0, false), Fail);
    }
}
