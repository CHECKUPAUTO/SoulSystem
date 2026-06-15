//! Module seccomp — filtre BPF direct sans dépendance externe.
//!
//! Implémente un filtre seccomp mode 2 (SECCOMP_SET_MODE_FILTER) via
//! `prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &prog)` avec instructions BPF.
//! Plus de dépendance sur le crate `seccomp` v0.1 abandonné depuis 2018.
//!
//! Références: Documentation du noyau Linux — `prctl(2)`, `seccomp(2)`.

// ── BPF instruction constants ───────────────────────────────

const BPF_LD: u16 = 0x00;
const BPF_JMP: u16 = 0x05;
const BPF_RET: u16 = 0x06;
const BPF_W: u16 = 0x00;
const BPF_ABS: u16 = 0x20;
const BPF_K: u16 = 0x00;

// ── BPF instruction ─────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
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

// ── BPF helpers ─────────────────────────────────────────────

fn bpf_stmt(code: u16, k: u32) -> SockFilter {
    SockFilter {
        code,
        jt: 0,
        jf: 0,
        k,
    }
}

fn bpf_jump(code: u16, k: u32, jt: u8, jf: u8) -> SockFilter {
    SockFilter { code, jt, jf, k }
}

// ── Seccomp constants ───────────────────────────────────────

const SECCOMP_RET_ALLOW: u32 = 0x7FFF_0000;
const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;

#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH: u32 = 0xC000_003E;
#[cfg(target_arch = "aarch64")]
const AUDIT_ARCH: u32 = 0xC000_00B7;

// ── Filter builder ──────────────────────────────────────────

/// Construit un programme BPF qui autorise la liste de syscalls donnée
/// et bloque tout le reste avec `errno`.
#[allow(clippy::vec_init_then_push)]
fn build_bpf_filter(syscalls: &[i64], errno: u32) -> Vec<SockFilter> {
    let mut prog = Vec::new();

    // 1. Valider l'architecture — charge arch à l'offset 4 du `seccomp_data`
    prog.push(bpf_stmt(BPF_LD | BPF_W | BPF_ABS, 4));
    prog.push(bpf_jump(
        BPF_JMP | BPF_K,
        AUDIT_ARCH,
        0, // si arch ok, continue (jt=0 saute le kill)
        1, // si arch != AUDIT_ARCH, saute au kill
    ));
    // Si arch invalide → tue le processus (bad arch)
    prog.push(bpf_stmt(BPF_RET | BPF_K, libc::SECCOMP_RET_KILL_PROCESS));

    // 2. Vérifier le numéro de syscall (offset 0)
    prog.push(bpf_stmt(BPF_LD | BPF_W | BPF_ABS, 0));

    // 3. Pour chaque syscall autorisé, comparer et autoriser si match
    for syscall in syscalls {
        prog.push(bpf_jump(
            BPF_JMP | BPF_K,
            *syscall as u32,
            0, // si match → instruction suivante (allow)
            1, // si pas match → skip le allow
        ));
        prog.push(bpf_stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW));
    }

    // 4. Fallback: tout le reste → ERRNO
    prog.push(bpf_stmt(
        BPF_RET | BPF_K,
        SECCOMP_RET_ERRNO | (errno & 0xFFFF),
    ));

    prog
}

/// Charge un filtre BPF dans le noyau via prctl.
fn load_bpf(prog: &[SockFilter]) -> Result<(), std::io::Error> {
    let fprog = SockFprog {
        len: prog.len() as u16,
        filter: prog.as_ptr(),
    };

    // PR_SET_SECCOMP + SECCOMP_MODE_FILTER
    let ret = unsafe {
        libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &fprog as *const SockFprog,
        )
    };

    if ret != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

// ── Public API ──────────────────────────────────────────────

pub fn install_filter(profile: &str) -> Result<(), std::io::Error> {
    let syscalls: &[i64] = match profile {
        "strict" => STRICT_SYSCALLS,
        "default" => DEFAULT_SYSCALLS,
        "unconfined" => return Ok(()),
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "unknown seccomp profile",
            ))
        }
    };

    let prog = build_bpf_filter(syscalls, libc::EPERM as u32);
    load_bpf(&prog)
}

// ── Syscall lists ───────────────────────────────────────────

const STRICT_SYSCALLS: &[i64] = &[
    libc::SYS_read,
    libc::SYS_write,
    libc::SYS_exit,
    libc::SYS_exit_group,
    libc::SYS_rt_sigreturn,
    libc::SYS_futex,
    libc::SYS_mmap,
    libc::SYS_munmap,
    libc::SYS_brk,
    libc::SYS_close,
    libc::SYS_fcntl,
    libc::SYS_getpid,
    libc::SYS_getgid,
];

const DEFAULT_SYSCALLS: &[i64] = &[
    libc::SYS_read,
    libc::SYS_write,
    libc::SYS_openat,
    libc::SYS_close,
    libc::SYS_exit,
    libc::SYS_exit_group,
    libc::SYS_rt_sigreturn,
    libc::SYS_futex,
    libc::SYS_mmap,
    libc::SYS_munmap,
    libc::SYS_brk,
    libc::SYS_fcntl,
    libc::SYS_getpid,
    libc::SYS_getgid,
    libc::SYS_statx,
    libc::SYS_lseek,
    libc::SYS_ioctl,
    libc::SYS_getuid,
];
