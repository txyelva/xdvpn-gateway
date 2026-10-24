//! 系统小工具：时钟、套接字绑定网卡、poll、信号、外部命令、/proc 读数。

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream, UdpSocket};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// 单调毫秒（进程内）
pub fn now_ms() -> u64 {
    T0.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

pub fn wall_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// 本地日期编号（按东八区；路由器时区固定在中国/新加坡，均为 UTC+8）
pub fn day_number() -> u64 {
    (wall_secs() + 8 * 3600) / 86400
}

pub fn hhmmss(epoch: u64) -> String {
    let s = (epoch + 8 * 3600) % 86400;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

fn cvt(r: libc::c_int) -> io::Result<libc::c_int> {
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(r)
    }
}

fn bind_device(fd: RawFd, dev: &str) -> io::Result<()> {
    let mut name = dev.as_bytes().to_vec();
    name.push(0);
    cvt(unsafe {
        libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_BINDTODEVICE, name.as_ptr() as *const libc::c_void, name.len() as libc::socklen_t)
    })?;
    Ok(())
}

fn sockaddr(a: SocketAddrV4) -> libc::sockaddr_in {
    libc::sockaddr_in {
        sin_family: libc::AF_INET as libc::sa_family_t,
        sin_port: a.port().to_be(),
        sin_addr: libc::in_addr { s_addr: u32::from_ne_bytes(a.ip().octets()) },
        sin_zero: [0; 8],
    }
}

/// 已 connect 的非阻塞 UDP 套接字；dev 不为空时绑定到该网卡（只经隧道发出，网卡不在就失败）
pub fn udp_connected(dst: SocketAddrV4, dev: Option<&str>, nonblock: bool) -> io::Result<UdpSocket> {
    let mut ty = libc::SOCK_DGRAM | libc::SOCK_CLOEXEC;
    if nonblock {
        ty |= libc::SOCK_NONBLOCK;
    }
    let fd = cvt(unsafe { libc::socket(libc::AF_INET, ty, 0) })?;
    let sock = unsafe { UdpSocket::from_raw_fd(fd) };
    if let Some(d) = dev {
        bind_device(fd, d)?;
    }
    let sa = sockaddr(dst);
    cvt(unsafe { libc::connect(fd, &sa as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_in>() as u32) })?;
    Ok(sock)
}

/// 阻塞 TCP 连接（带超时），可绑定网卡
pub fn tcp_connect(dst: SocketAddrV4, dev: Option<&str>, timeout: Duration) -> io::Result<TcpStream> {
    let fd = cvt(unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK, 0) })?;
    let s = unsafe { TcpStream::from_raw_fd(fd) };
    if let Some(d) = dev {
        bind_device(fd, d)?;
    }
    let sa = sockaddr(dst);
    let r = unsafe { libc::connect(fd, &sa as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_in>() as u32) };
    if r < 0 {
        let e = io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(e);
        }
        let mut p = libc::pollfd { fd, events: libc::POLLOUT, revents: 0 };
        let n = cvt(unsafe { libc::poll(&mut p, 1, timeout.as_millis() as i32) })?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "connect timeout"));
        }
        let mut err: libc::c_int = 0;
        let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        unsafe { libc::getsockopt(fd, libc::SOL_SOCKET, libc::SO_ERROR, &mut err as *mut _ as *mut libc::c_void, &mut len) };
        if err != 0 {
            return Err(io::Error::from_raw_os_error(err));
        }
    }
    s.set_nonblocking(false)?;
    s.set_read_timeout(Some(timeout))?;
    s.set_write_timeout(Some(timeout))?;
    Ok(s)
}

/// poll 一组 fd 的可读事件；返回每个 fd 是否可读
pub fn poll_readable(fds: &[RawFd], timeout_ms: i32) -> io::Result<Vec<bool>> {
    let mut p: Vec<libc::pollfd> = fds.iter().map(|&fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 }).collect();
    let r = unsafe { libc::poll(p.as_mut_ptr(), p.len() as libc::nfds_t, timeout_ms) };
    if r < 0 {
        let e = io::Error::last_os_error();
        if e.kind() == io::ErrorKind::Interrupted {
            return Ok(vec![false; fds.len()]);
        }
        return Err(e);
    }
    Ok(p.iter().map(|x| x.revents & (libc::POLLIN | libc::POLLERR | libc::POLLHUP) != 0).collect())
}

pub fn raw<T: AsRawFd>(x: &T) -> RawFd {
    x.as_raw_fd()
}

// ---------- 信号 ----------

pub static SIG_TERM: AtomicBool = AtomicBool::new(false);
pub static SIG_HUP: AtomicBool = AtomicBool::new(false);
pub static SIG_USR1: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(sig: libc::c_int) {
    match sig {
        libc::SIGTERM | libc::SIGINT => SIG_TERM.store(true, Ordering::SeqCst),
        libc::SIGHUP => SIG_HUP.store(true, Ordering::SeqCst),
        libc::SIGUSR1 => SIG_USR1.store(true, Ordering::SeqCst),
        _ => {}
    }
}

pub fn install_signals() {
    unsafe {
        for s in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP, libc::SIGUSR1] {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as usize;
            sa.sa_flags = 0; // 不设 SA_RESTART：让阻塞调用尽快返回
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(s, &sa, std::ptr::null_mut());
        }
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
}

pub fn take(flag: &AtomicBool) -> bool {
    flag.swap(false, Ordering::SeqCst)
}

pub fn kill(pid: i32, sig: libc::c_int) -> bool {
    pid > 0 && unsafe { libc::kill(pid, sig) } == 0
}

pub fn alive(pid: i32) -> bool {
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

// ---------- 外部命令 ----------

/// 执行命令，返回 (成功, 合并后的输出)。PATH 由外壳脚本保证。
pub fn run(prog: &str, args: &[&str]) -> (bool, String) {
    match Command::new(prog).args(args).stdin(Stdio::null()).output() {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.success(), s)
        }
        Err(e) => (false, e.to_string()),
    }
}

pub fn run_ok(prog: &str, args: &[&str]) -> bool {
    run(prog, args).0
}

/// 通过 /bin/sh 执行一条命令行（用绝对路径：该固件上裸 sh 是内存读写工具）
pub fn sh(cmdline: &str) -> (bool, String) {
    run("/bin/sh", &["-c", cmdline])
}

pub fn run_stdin(prog: &str, args: &[&str], input: &str) -> bool {
    use std::io::Write;
    let child = Command::new(prog).args(args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    match child {
        Ok(mut c) => {
            if let Some(mut i) = c.stdin.take() {
                let _ = i.write_all(input.as_bytes());
            }
            c.wait().map(|s| s.success()).unwrap_or(false)
        }
        Err(_) => false,
    }
}

// ---------- /proc ----------

/// 进程常驻内存（KB）
pub fn rss_kb(pid: i32) -> Option<u64> {
    let s = std::fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
    s.lines().find(|l| l.starts_with("VmRSS:"))?.split_whitespace().nth(1)?.parse().ok()
}

/// 进程累计 CPU 时间（时钟滴答）
pub fn cpu_ticks(pid: i32) -> Option<u64> {
    let s = std::fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;
    let rest = &s[s.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some(f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?)
}

pub fn clk_tck() -> u64 {
    let v = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if v > 0 {
        v as u64
    } else {
        100
    }
}

pub fn if_counter(dev: &str, name: &str) -> Option<u64> {
    std::fs::read_to_string(format!("/sys/class/net/{}/statistics/{}", dev, name)).ok()?.trim().parse().ok()
}

/// 原子写文件（先写临时文件再改名），避免网页读到半截内容
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

pub fn ip4(s: &str) -> Option<Ipv4Addr> {
    s.trim().parse().ok()
}
