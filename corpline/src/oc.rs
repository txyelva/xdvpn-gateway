//! openconnect 子进程：前台运行（不用 --background，旧版真机实测它会重建 tun 丢配置），
//! 密码经 stdin 传入；corpline 退出时内核给它发 SIGTERM（PR_SET_PDEATHSIG），不留孤儿进程。

use crate::config::Config;
use crate::sys;
use std::io::{BufRead, BufReader, Write};
use std::net::Ipv4Addr;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;

pub enum OcMsg {
    Line(String),
}

pub struct Oc {
    child: Child,
    pub pid: i32,
    term_at: Option<u64>,
    pub auth_failed: bool,
    pub last_error: String,
    pub dtls: Option<bool>,
    cpu_last: Option<u64>,
    pub cpu_hot: u32,
}

/// 根据一行输出判断：是否认证失败、是否值得记下的错误、DTLS 状态
pub fn classify_line(l: &str) -> (bool, Option<&'static str>, Option<bool>) {
    let low = l.to_ascii_lowercase();
    let auth = low.contains("login failed")
        || low.contains("authentication failed")
        || low.contains("username or password")
        || low.contains("invalid credentials")
        || low.contains("auth-failed");
    let err = if auth {
        Some("账号或密码不对")
    } else if low.contains("certificate") && (low.contains("verify") || low.contains("signer not found") || low.contains("invalid")) {
        Some("服务器证书校验失败")
    } else if low.contains("failed to connect") || low.contains("connection refused") || low.contains("failed to open https connection") {
        Some("连不上公司服务器")
    } else if low.contains("getaddrinfo failed") || low.contains("failed to resolve") || low.contains("name or service not known") {
        Some("解析不到公司服务器地址")
    } else if low.contains("too many") || low.contains("session limit") || low.contains("max sessions") {
        Some("公司账号在线数已满")
    } else if low.contains("tun device") || low.contains("failed to open tun") || low.contains("tun interface") && low.contains("fail") {
        Some("隧道网卡创建失败")
    } else if low.contains("cookie") && low.contains("rejected") || low.contains("server terminated") {
        Some("公司服务器断开了连接")
    } else {
        None
    };
    let dtls = if low.contains("established dtls connection") {
        Some(true)
    } else if low.contains("dtls dead peer") || low.contains("dtls handshake failed") || low.contains("dtls handshake timed out") {
        Some(false)
    } else {
        None
    };
    (auth, err, dtls)
}

impl Oc {
    pub fn spawn(cfg: &Config, server_ip: Option<Ipv4Addr>, tx: Sender<OcMsg>) -> Result<Oc, String> {
        if !cfg.openconnect.exists() {
            return Err(format!("找不到 openconnect（{}）", cfg.openconnect.display()));
        }
        let mut cmd = Command::new(&cfg.openconnect);
        cmd.env("LD_LIBRARY_PATH", &cfg.oc_ld_library_path)
            .env("CORPLINE_RUN_DIR", &cfg.run_dir)
            .env("CORPLINE_TABLE", cfg.table.to_string())
            .env("CORPLINE_MARK", format!("0x{:x}", cfg.mark))
            .arg(format!("--protocol={}", cfg.protocol))
            .arg(format!("--user={}", cfg.user))
            .arg("--passwd-on-stdin")
            .arg("--non-inter")
            .arg(format!("--interface={}", cfg.tun_if))
            .arg(format!("--script={}", cfg.hook_cmd))
            .arg("--force-dpd=10")
            .arg("--reconnect-timeout=60");
        if cfg.no_dtls {
            cmd.arg("--no-dtls");
        }
        if !cfg.cafile.is_empty() {
            cmd.arg(format!("--cafile={}", cfg.cafile));
        }
        if !cfg.servercert.is_empty() {
            cmd.arg(format!("--servercert={}", cfg.servercert));
        }
        if let Some(ip) = server_ip {
            let host = cfg.server_host();
            if host.parse::<Ipv4Addr>().is_err() {
                // 服务器地址由 corpline 经运营商 DNS 解析好，openconnect 不再依赖家里的 DNS 链路
                cmd.arg(format!("--resolve={}:{}", host, ip));
            }
        }
        for x in &cfg.oc_extra {
            cmd.arg(x);
        }
        cmd.arg("--").arg(&cfg.server);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                libc::setpgid(0, 0);
                Ok(())
            });
        }
        let mut child = cmd.spawn().map_err(|e| format!("启动 openconnect 失败：{}", e))?;
        if let Some(mut i) = child.stdin.take() {
            let _ = i.write_all(cfg.password.as_bytes());
            let _ = i.write_all(b"\n");
        }
        for out in [child.stdout.take().map(|x| Box::new(x) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|x| Box::new(x) as Box<dyn std::io::Read + Send>)]
            .into_iter()
            .flatten()
        {
            let tx = tx.clone();
            let _ = std::thread::Builder::new().name("oc-out".into()).stack_size(64 * 1024).spawn(move || {
                let r = BufReader::new(out);
                for l in r.split(b'\n').map_while(Result::ok) {
                    let s = String::from_utf8_lossy(&l).trim_end().to_string();
                    if !s.is_empty() && tx.send(OcMsg::Line(s)).is_err() {
                        break;
                    }
                }
            });
        }
        let pid = child.id() as i32;
        let _ = std::fs::write(cfg.run_dir.join("openconnect.pid"), pid.to_string());
        Ok(Oc { child, pid, term_at: None, auth_failed: false, last_error: String::new(), dtls: None, cpu_last: None, cpu_hot: 0 })
    }

    pub fn on_line(&mut self, l: &str) {
        let (auth, err, dtls) = classify_line(l);
        if auth {
            self.auth_failed = true;
        }
        if let Some(e) = err {
            self.last_error = e.to_string();
        }
        if let Some(d) = dtls {
            self.dtls = Some(d);
        }
    }

    /// 请求退出：先 SIGTERM（让连接脚本走 disconnect），12 s 后仍在就 SIGKILL
    pub fn stop(&mut self) {
        if self.term_at.is_none() {
            sys::kill(self.pid, libc::SIGTERM);
            self.term_at = Some(sys::now_ms());
        }
    }

    /// 已退出则返回 true
    pub fn poll_exit(&mut self) -> bool {
        if let Some(t) = self.term_at {
            if sys::now_ms() >= t + 12_000 {
                sys::kill(self.pid, libc::SIGKILL);
            }
        }
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    pub fn wait_blocking(&mut self, ms: u64) {
        self.stop();
        let end = sys::now_ms() + ms;
        while sys::now_ms() < end {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        sys::kill(self.pid, libc::SIGKILL);
        let _ = self.child.wait();
    }

    /// 每 5 s 调一次：内存超过 40 MB，或 CPU 连续 60 s 占满 → 返回原因
    pub fn resource_check(&mut self, interval_ms: u64) -> Option<String> {
        if let Some(kb) = sys::rss_kb(self.pid) {
            if kb > 40 * 1024 {
                return Some(format!("openconnect 内存异常（{} MB）", kb / 1024));
            }
        }
        if let Some(t) = sys::cpu_ticks(self.pid) {
            if let Some(last) = self.cpu_last {
                let used = t.saturating_sub(last) as f64 / sys::clk_tck() as f64;
                if used >= interval_ms as f64 / 1000.0 * 0.9 {
                    self.cpu_hot += 1;
                } else {
                    self.cpu_hot = 0;
                }
            }
            self.cpu_last = Some(t);
            if self.cpu_hot as u64 * interval_ms >= 60_000 {
                return Some("openconnect 持续占满 CPU".into());
            }
        }
        None
    }
}

/// 上次 corpline 异常退出留下的 openconnect：确认是它再结束掉
pub fn kill_orphan(cfg: &Config) {
    let p = cfg.run_dir.join("openconnect.pid");
    if let Ok(s) = std::fs::read_to_string(&p) {
        if let Ok(pid) = s.trim().parse::<i32>() {
            let comm = std::fs::read_to_string(format!("/proc/{}/comm", pid)).unwrap_or_default();
            if comm.trim() == "openconnect" {
                sys::kill(pid, libc::SIGTERM);
                for _ in 0..50 {
                    if !sys::alive(pid) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                sys::kill(pid, libc::SIGKILL);
            }
        }
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines() {
        assert!(classify_line("Login failed.").0);
        assert_eq!(classify_line("Server certificate verify failed: signer not found").1, Some("服务器证书校验失败"));
        assert_eq!(classify_line("Established DTLS connection (using GnuTLS).").2, Some(true));
        assert_eq!(classify_line("DTLS Dead Peer Detection detected dead peer!").2, Some(false));
        let (a, e, d) = classify_line("Connected as 10.235.152.121, using SSL, with DTLS in progress");
        assert!(!a && e.is_none() && d.is_none());
    }
}
