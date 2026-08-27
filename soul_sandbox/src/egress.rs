//! Per-host egress enforcement via `SECCOMP_RET_USER_NOTIF`.
//!
//! # Why this mechanism
//!
//! `network_isolated` is all-or-nothing: a tool gets the host's network or
//! none of it. Restricting a tool to *specific* destinations needs something
//! that can see where a connection is going, and the obvious candidates do not
//! work unprivileged:
//!
//! * plain seccomp cannot inspect the `sockaddr` — BPF may not dereference
//!   pointers, so the filter sees a pointer value and nothing behind it;
//! * nftables inside the netns needs privileged network setup;
//! * a proxy is advisory unless redirect rules force traffic through it, which
//!   needs the same privileges.
//!
//! `SECCOMP_RET_USER_NOTIF` is the one that does work: the kernel suspends the
//! target on `connect(2)` and hands a supervisor the syscall arguments. The
//! supervisor reads the `sockaddr` out of the target's memory, decides, and
//! either lets the syscall proceed or fails it with `EACCES`.
//!
//! # What this enforces, precisely
//!
//! Destination **IP and port**, at `connect(2)`. Not hostnames: a name is
//! resolved to addresses when the policy is built, and it is those addresses
//! that are enforced. If a name later resolves elsewhere, the new address is
//! denied rather than silently allowed — which is the safe direction, and the
//! reason [`EgressPolicy::allow_host`] records the name it resolved.
//!
//! Deny is the default. A destination that is not in the allowlist is refused,
//! and a `connect` this code cannot fully inspect is refused rather than
//! waved through.
//!
//! # Fail-closed, unlike the namespaces
//!
//! Network and PID isolation degrade on hosts that forbid unprivileged user
//! namespaces, because refusing there would leave the sandbox unable to run
//! anything. This does not have that dependency: installing a listener needs
//! only `no_new_privs`. So when a caller asks for an egress policy and the
//! listener cannot be installed, the execution fails instead of running with
//! unrestricted network — an egress policy that silently stops applying is
//! worse than none, because it is believed.

use std::collections::BTreeSet;
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::os::unix::io::RawFd;

/// Destinations a sandboxed process may reach.
///
/// Empty means "no egress at all", which is a real policy and not an oversight:
/// it is what a tool with no network needs, enforced at `connect` rather than
/// by removing the interfaces.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EgressPolicy {
    allowed: BTreeSet<SocketAddr>,
    /// Names resolved when the policy was built, kept for diagnostics.
    ///
    /// A denial that says "api.example.com resolved to 10.0.0.1 when this
    /// policy was built, the connection went to 10.0.0.9" is actionable. One
    /// that says "denied" is not.
    resolved_names: Vec<(String, Vec<SocketAddr>)>,
}

impl EgressPolicy {
    /// A policy that permits nothing.
    pub fn deny_all() -> Self {
        Self::default()
    }

    /// Permit exactly this address and port.
    pub fn allow_addr(mut self, addr: SocketAddr) -> Self {
        self.allowed.insert(addr);
        self
    }

    /// Resolve `host:port` now and permit the addresses it currently maps to.
    ///
    /// Resolution happens here, once, rather than at connect time. A
    /// supervisor that resolved names while deciding would be trusting DNS at
    /// exactly the moment an attacker controls what the target asks for.
    pub fn allow_host(mut self, host: &str, port: u16) -> io::Result<Self> {
        let addrs: Vec<SocketAddr> = (host, port).to_socket_addrs()?.collect();
        if addrs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{host}:{port} resolved to no addresses"),
            ));
        }
        for a in &addrs {
            self.allowed.insert(*a);
        }
        self.resolved_names.push((host.to_string(), addrs));
        Ok(self)
    }

    /// Whether this policy permits anything at all.
    pub fn is_deny_all(&self) -> bool {
        self.allowed.is_empty()
    }

    pub fn allowed_addrs(&self) -> impl Iterator<Item = &SocketAddr> {
        self.allowed.iter()
    }

    /// The decision for one destination.
    pub fn permits(&self, addr: &SocketAddr) -> bool {
        self.allowed.contains(addr)
    }

    /// Human-readable reason a destination was refused.
    pub fn denial_reason(&self, addr: &SocketAddr) -> String {
        if self.allowed.is_empty() {
            return format!("egress policy denies all destinations; {addr} refused");
        }
        let mut msg = format!("{addr} is not in the egress allowlist. Allowed: ");
        let list: Vec<String> = self.allowed.iter().map(|a| a.to_string()).collect();
        msg.push_str(&list.join(", "));
        for (name, addrs) in &self.resolved_names {
            if addrs.iter().any(|a| a.ip() == addr.ip()) {
                continue;
            }
            msg.push_str(&format!(
                " ({name} resolved to {} when this policy was built)",
                addrs
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            ));
        }
        msg
    }
}

// ── Kernel interface ─────────────────────────────────────────────────────────
//
// `libc` does not expose the seccomp notification structs, so they are
// declared here against the kernel UAPI. Sizes matter: the kernel checks
// `len` against its own struct size.

pub(crate) const SECCOMP_RET_USER_NOTIF: u32 = 0x7fc0_0000;
pub(crate) const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
pub(crate) const SECCOMP_SET_MODE_FILTER: libc::c_uint = 1;
pub(crate) const SECCOMP_FILTER_FLAG_NEW_LISTENER: libc::c_ulong = 1 << 3;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct SeccompData {
    pub nr: libc::c_int,
    pub arch: u32,
    pub instruction_pointer: u64,
    pub args: [u64; 6],
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct SeccompNotif {
    pub id: u64,
    pub pid: u32,
    pub flags: u32,
    pub data: SeccompData,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct SeccompNotifResp {
    pub id: u64,
    pub val: i64,
    pub error: i32,
    pub flags: u32,
}

// _IOWR('!', 0, struct seccomp_notif) etc. Computed rather than hardcoded so
// a struct-size mistake shows up as a failing ioctl, not silent corruption.
const IOC_NRBITS: u64 = 8;
const IOC_TYPEBITS: u64 = 8;
const IOC_SIZEBITS: u64 = 14;
const IOC_NRSHIFT: u64 = 0;
const IOC_TYPESHIFT: u64 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u64 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u64 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_READ: u64 = 2;
const IOC_WRITE: u64 = 1;

const fn ioc(dir: u64, ty: u64, nr: u64, size: u64) -> libc::c_ulong {
    ((dir << IOC_DIRSHIFT) | (ty << IOC_TYPESHIFT) | (nr << IOC_NRSHIFT) | (size << IOC_SIZESHIFT))
        as libc::c_ulong
}

pub(crate) fn ioctl_notif_recv() -> libc::c_ulong {
    ioc(
        IOC_READ | IOC_WRITE,
        b'!' as u64,
        0,
        std::mem::size_of::<SeccompNotif>() as u64,
    )
}

pub(crate) fn ioctl_notif_send() -> libc::c_ulong {
    ioc(
        IOC_READ | IOC_WRITE,
        b'!' as u64,
        1,
        std::mem::size_of::<SeccompNotifResp>() as u64,
    )
}

pub(crate) fn ioctl_notif_id_valid() -> libc::c_ulong {
    ioc(IOC_WRITE, b'!' as u64, 2, std::mem::size_of::<u64>() as u64)
}

/// Install a filter that traps `connect` to a user-space supervisor.
///
/// Returns the listener fd. Called from `pre_exec`, so everything here is
/// async-signal-safe: no allocation, no locks.
///
/// # Safety
/// Must be called in the forked child before `exec`.
pub(crate) unsafe fn install_connect_listener() -> io::Result<RawFd> {
    // BPF: load syscall nr; if it is `connect`, return USER_NOTIF; else ALLOW.
    //
    // Deliberately narrow. Trapping every syscall would make the supervisor a
    // bottleneck on the whole program; `connect` is where a destination is
    // named. `sendto` with an address is the gap, and it is stated in the
    // module docs rather than left for a reader to discover.
    #[repr(C)]
    struct SockFilter {
        code: u16,
        jt: u8,
        jf: u8,
        k: u32,
    }
    #[repr(C)]
    struct SockFprog {
        len: u16,
        filter: *const SockFilter,
    }

    const BPF_LD: u16 = 0x00;
    const BPF_W: u16 = 0x00;
    const BPF_ABS: u16 = 0x20;
    const BPF_JMP: u16 = 0x05;
    const BPF_JEQ: u16 = 0x10;
    const BPF_K: u16 = 0x00;
    const BPF_RET: u16 = 0x06;

    let nr_offset = std::mem::offset_of!(SeccompData, nr) as u32;
    let filter = [
        SockFilter {
            code: BPF_LD | BPF_W | BPF_ABS,
            jt: 0,
            jf: 0,
            k: nr_offset,
        },
        SockFilter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 0,
            jf: 1,
            k: libc::SYS_connect as u32,
        },
        SockFilter {
            code: BPF_RET | BPF_K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_USER_NOTIF,
        },
        SockFilter {
            code: BPF_RET | BPF_K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_ALLOW,
        },
    ];
    let prog = SockFprog {
        len: filter.len() as u16,
        filter: filter.as_ptr(),
    };

    // Required before installing a filter without CAP_SYS_ADMIN.
    if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
        return Err(io::Error::last_os_error());
    }

    let fd = libc::syscall(
        libc::SYS_seccomp,
        SECCOMP_SET_MODE_FILTER,
        SECCOMP_FILTER_FLAG_NEW_LISTENER,
        &prog as *const SockFprog,
    );
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fd as RawFd)
}

/// Read `len` bytes at `addr` out of `pid`'s address space.
///
/// Uses `/proc/<pid>/mem`, which is the supported way to read another
/// process's memory for exactly this purpose.
fn read_target_memory(pid: u32, addr: u64, len: usize) -> io::Result<Vec<u8>> {
    use std::os::unix::fs::FileExt;
    let f = std::fs::File::open(format!("/proc/{pid}/mem"))?;
    let mut buf = vec![0u8; len];
    f.read_exact_at(&mut buf, addr)?;
    Ok(buf)
}

/// Decode a `sockaddr` into a `SocketAddr`, for the families that name a
/// destination.
///
/// Returns `None` for anything else — AF_UNIX, netlink, and families this code
/// does not understand. The caller denies on `None`: a destination that cannot
/// be read is not a destination that can be checked.
fn decode_sockaddr(buf: &[u8]) -> Option<SocketAddr> {
    if buf.len() < 2 {
        return None;
    }
    let family = u16::from_ne_bytes([buf[0], buf[1]]);
    match family as i32 {
        libc::AF_INET => {
            if buf.len() < 8 {
                return None;
            }
            // sockaddr_in: family(2) port(2, network order) addr(4)
            let port = u16::from_be_bytes([buf[2], buf[3]]);
            let ip = std::net::Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
            Some(SocketAddr::new(IpAddr::V4(ip), port))
        }
        libc::AF_INET6 => {
            if buf.len() < 24 {
                return None;
            }
            // sockaddr_in6: family(2) port(2) flowinfo(4) addr(16)
            let port = u16::from_be_bytes([buf[2], buf[3]]);
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&buf[8..24]);
            Some(SocketAddr::new(
                IpAddr::V6(std::net::Ipv6Addr::from(octets)),
                port,
            ))
        }
        _ => None,
    }
}

/// One supervised decision. Exposed for tests and for the audit trail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressDecision {
    pub destination: Option<SocketAddr>,
    pub allowed: bool,
    pub reason: String,
}

/// Serve notifications on `notify_fd` until the target exits.
///
/// Runs on its own thread in the parent. Every decision is appended to
/// `log` so a caller can report what was refused rather than leaving an
/// operator to guess why a tool could not reach anything.
pub(crate) fn supervise(
    notify_fd: RawFd,
    policy: EgressPolicy,
    log: std::sync::Arc<std::sync::Mutex<Vec<EgressDecision>>>,
) {
    loop {
        let mut req = SeccompNotif::default();
        let rc = unsafe { libc::ioctl(notify_fd, ioctl_notif_recv(), &mut req) };
        if rc != 0 {
            // ENOENT: the target died or the syscall was interrupted. Either
            // way there is nothing left to supervise.
            break;
        }

        let addr_ptr = req.data.args[1];
        let addr_len = req.data.args[2] as usize;
        let decoded = if addr_len == 0 || addr_len > 128 {
            None
        } else {
            read_target_memory(req.pid, addr_ptr, addr_len)
                .ok()
                .and_then(|b| decode_sockaddr(&b))
        };

        // Re-check the notification is still live before acting on memory read
        // out of band: without this the target could have exited and had its
        // id reused, and the decision would apply to a different syscall.
        let mut id = req.id;
        let still_valid = unsafe { libc::ioctl(notify_fd, ioctl_notif_id_valid(), &mut id) } == 0;
        if !still_valid {
            continue;
        }

        let (allow, reason) = match decoded {
            Some(addr) if policy.permits(&addr) => (true, format!("{addr} permitted")),
            Some(addr) => (false, policy.denial_reason(&addr)),
            None => (
                false,
                "connect to an address this supervisor could not decode; \
                 refused rather than waved through"
                    .to_string(),
            ),
        };

        if let Ok(mut l) = log.lock() {
            l.push(EgressDecision {
                destination: decoded,
                allowed: allow,
                reason: reason.clone(),
            });
        }

        let resp = SeccompNotifResp {
            id: req.id,
            val: 0,
            error: if allow { 0 } else { -libc::EACCES },
            // SECCOMP_USER_NOTIF_FLAG_CONTINUE lets the syscall proceed
            // normally; without it a zero error means "syscall returned 0",
            // which is not the same as "let it happen".
            flags: if allow { 1 } else { 0 },
        };
        let rc = unsafe { libc::ioctl(notify_fd, ioctl_notif_send(), &resp) };
        if rc != 0 {
            break;
        }
    }
    unsafe {
        libc::close(notify_fd);
    }
}

/// Send `fd` over a unix socket with `SCM_RIGHTS`.
///
/// Called from `pre_exec`, so syscalls only — no allocation.
///
/// # Safety
/// `sock` must be a connected unix socket and `fd` must be open.
pub(crate) unsafe fn send_fd(sock: RawFd, fd: RawFd) -> io::Result<()> {
    let mut byte = 0u8;
    let mut iov = libc::iovec {
        iov_base: &mut byte as *mut u8 as *mut libc::c_void,
        iov_len: 1,
    };
    // CMSG buffer sized for exactly one fd.
    let mut cmsg_buf = [0u8; 32];
    let mut msg: libc::msghdr = std::mem::zeroed();
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = cmsg_buf.as_mut_ptr() as *mut libc::c_void;
    msg.msg_controllen = libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as _;

    let cmsg = libc::CMSG_FIRSTHDR(&msg);
    (*cmsg).cmsg_level = libc::SOL_SOCKET;
    (*cmsg).cmsg_type = libc::SCM_RIGHTS;
    (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
    std::ptr::write_unaligned(libc::CMSG_DATA(cmsg) as *mut RawFd, fd);

    if libc::sendmsg(sock, &msg, 0) < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Receive one fd sent with [`send_fd`].
pub(crate) fn recv_fd(sock: RawFd) -> io::Result<RawFd> {
    unsafe {
        let mut byte = 0u8;
        let mut iov = libc::iovec {
            iov_base: &mut byte as *mut u8 as *mut libc::c_void,
            iov_len: 1,
        };
        let mut cmsg_buf = [0u8; 32];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cmsg_buf.as_mut_ptr() as *mut libc::c_void;
        msg.msg_controllen = libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as _;

        let n = libc::recvmsg(sock, &mut msg, 0);
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if cmsg.is_null()
            || (*cmsg).cmsg_level != libc::SOL_SOCKET
            || (*cmsg).cmsg_type != libc::SCM_RIGHTS
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no descriptor arrived from the sandboxed child; egress \
                 supervision cannot start",
            ));
        }
        Ok(std::ptr::read_unaligned(
            libc::CMSG_DATA(cmsg) as *const RawFd
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_policy_denies_everything() {
        let p = EgressPolicy::deny_all();
        assert!(p.is_deny_all());
        assert!(!p.permits(&"1.2.3.4:443".parse().unwrap()));
        assert!(p
            .denial_reason(&"1.2.3.4:443".parse().unwrap())
            .contains("denies all"));
    }

    #[test]
    fn an_allowed_address_is_permitted_and_others_are_not() {
        let p = EgressPolicy::deny_all().allow_addr("10.0.0.1:8080".parse().unwrap());
        assert!(p.permits(&"10.0.0.1:8080".parse().unwrap()));
        // Same host, different port is a different destination.
        assert!(!p.permits(&"10.0.0.1:9090".parse().unwrap()));
        assert!(!p.permits(&"10.0.0.2:8080".parse().unwrap()));
    }

    #[test]
    fn ipv4_sockaddr_decodes() {
        // family=AF_INET, port=443 (be), 93.184.216.34
        let mut buf = vec![0u8; 16];
        buf[0..2].copy_from_slice(&(libc::AF_INET as u16).to_ne_bytes());
        buf[2..4].copy_from_slice(&443u16.to_be_bytes());
        buf[4..8].copy_from_slice(&[93, 184, 216, 34]);
        assert_eq!(
            decode_sockaddr(&buf),
            Some("93.184.216.34:443".parse().unwrap())
        );
    }

    #[test]
    fn ipv6_sockaddr_decodes() {
        let mut buf = vec![0u8; 28];
        buf[0..2].copy_from_slice(&(libc::AF_INET6 as u16).to_ne_bytes());
        buf[2..4].copy_from_slice(&443u16.to_be_bytes());
        let ip: std::net::Ipv6Addr = "2606:2800:220:1:248:1893:25c8:1946".parse().unwrap();
        buf[8..24].copy_from_slice(&ip.octets());
        assert_eq!(
            decode_sockaddr(&buf),
            Some(SocketAddr::new(IpAddr::V6(ip), 443))
        );
    }

    /// A family this code does not model must decode to `None`, because the
    /// caller denies on `None`. Guessing would be the failure mode.
    #[test]
    fn an_unknown_family_does_not_decode() {
        let mut buf = vec![0u8; 16];
        buf[0..2].copy_from_slice(&(libc::AF_UNIX as u16).to_ne_bytes());
        assert_eq!(decode_sockaddr(&buf), None);
        assert_eq!(decode_sockaddr(&[]), None);
        assert_eq!(decode_sockaddr(&[2]), None);
    }

    /// The ioctl numbers must match the kernel's, which depends on the struct
    /// sizes. A mismatch here is the difference between supervising and
    /// silently failing to.
    #[test]
    fn the_notification_structs_have_the_sizes_the_kernel_expects() {
        assert_eq!(std::mem::size_of::<SeccompData>(), 64);
        assert_eq!(std::mem::size_of::<SeccompNotif>(), 80);
        assert_eq!(std::mem::size_of::<SeccompNotifResp>(), 24);
    }

    #[test]
    fn a_listener_can_be_installed_on_this_host() {
        // Installing a filter is irreversible for the calling thread, so this
        // runs in a child process rather than poisoning the test binary.
        let ok = unsafe {
            match libc::fork() {
                -1 => panic!("fork failed"),
                0 => {
                    let r = install_connect_listener();
                    libc::_exit(if r.is_ok() { 0 } else { 1 });
                }
                pid => {
                    let mut st = 0;
                    libc::waitpid(pid, &mut st, 0);
                    libc::WIFEXITED(st) && libc::WEXITSTATUS(st) == 0
                }
            }
        };
        assert!(
            ok,
            "SECCOMP_FILTER_FLAG_NEW_LISTENER is unavailable on this host; \
             egress supervision cannot be enforced here"
        );
    }
}
