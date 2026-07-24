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
// The BPF_JMP class alone (without an OP subcode) decodes as BPF_JA —
// an *unconditional* jump that treats `k` as a raw forward offset, not a
// jump-if-equal comparison. Every conditional jump in this file must OR in
// BPF_JEQ, or the kernel's classic-BPF validator sees a JA instruction
// whose `k` (an arch constant or syscall number, not a valid offset) jumps
// out of the program's bounds and rejects the whole filter with EINVAL —
// confirmed by hand: `prctl(PR_SET_SECCOMP, ...)` returns EINVAL for any
// program containing a bare `BPF_JMP | BPF_K` comparison, and succeeds
// once BPF_JEQ is included.
const BPF_JEQ: u16 = 0x10;

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
        BPF_JMP | BPF_JEQ | BPF_K,
        AUDIT_ARCH,
        1, // si arch ok, saute le kill (jt=1)
        0, // si arch != AUDIT_ARCH, tombe dans le kill (jf=0)
    ));
    // Si arch invalide → tue le processus (bad arch)
    prog.push(bpf_stmt(BPF_RET | BPF_K, libc::SECCOMP_RET_KILL_PROCESS));

    // 2. Vérifier le numéro de syscall (offset 0)
    prog.push(bpf_stmt(BPF_LD | BPF_W | BPF_ABS, 0));

    // 3. Pour chaque syscall autorisé, comparer et autoriser si match
    for syscall in syscalls {
        prog.push(bpf_jump(
            BPF_JMP | BPF_JEQ | BPF_K,
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

/// Construit un programme BPF "denylist" : autorise tout par défaut, sauf
/// la liste de syscalls donnée qui retournent `errno`. Contrairement à
/// `build_bpf_filter` (allowlist stricte, réservée aux binaires whitelistés
/// dont on connaît l'empreinte syscall exacte), cette variante reste
/// utilisable comme profil actif *par défaut* pour des commandes arbitraires
/// (ls, cat, grep, cargo, ...) sans avoir à énumérer chaque syscall requis
/// par la libc/l'éditeur de liens dynamique — elle bloque uniquement les
/// syscalls à fort impact (namespaces, modules noyau, ptrace, horloge
/// système, ...) qu'une commande shell légitime n'a jamais besoin d'appeler.
#[allow(clippy::vec_init_then_push)]
fn build_bpf_denylist_filter(syscalls: &[i64], errno: u32) -> Vec<SockFilter> {
    let mut prog = Vec::new();

    prog.push(bpf_stmt(BPF_LD | BPF_W | BPF_ABS, 4));
    prog.push(bpf_jump(BPF_JMP | BPF_JEQ | BPF_K, AUDIT_ARCH, 1, 0));
    prog.push(bpf_stmt(BPF_RET | BPF_K, libc::SECCOMP_RET_KILL_PROCESS));

    prog.push(bpf_stmt(BPF_LD | BPF_W | BPF_ABS, 0));

    for syscall in syscalls {
        prog.push(bpf_jump(
            BPF_JMP | BPF_JEQ | BPF_K,
            *syscall as u32,
            0, // si match → instruction suivante (deny)
            1, // si pas match → skip le deny, essaie le suivant
        ));
        prog.push(bpf_stmt(
            BPF_RET | BPF_K,
            SECCOMP_RET_ERRNO | (errno & 0xFFFF),
        ));
    }

    // Fallback: tout le reste (non listé ci-dessus) → autorisé.
    prog.push(bpf_stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW));

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
    match profile {
        "strict" => {
            let prog = build_bpf_filter(STRICT_SYSCALLS, libc::EPERM as u32);
            load_bpf(&prog)
        }
        "allowlist-default" => {
            let prog = build_bpf_filter(DEFAULT_SYSCALLS, libc::EPERM as u32);
            load_bpf(&prog)
        }
        // The mandatory, always-on profile for `SandboxPolicy::default()`.
        // A strict per-syscall allowlist cannot support arbitrary shell
        // commands (cargo, rustc, grep, find, ...) without enumerating
        // every syscall their dynamic linker/runtime needs, so this
        // profile instead denies a fixed set of high-impact syscalls
        // (namespaces, kernel modules, ptrace, raw device/clock control,
        // ...) that no legitimate sandboxed command requires, and allows
        // everything else.
        "default" => {
            let prog = build_bpf_denylist_filter(DANGEROUS_SYSCALLS, libc::EPERM as u32);
            load_bpf(&prog)
        }
        "unconfined" => Ok(()),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "unknown seccomp profile",
        )),
    }
}

// ── Syscall lists ───────────────────────────────────────────

// NOTE: both allowlists below MUST include SYS_execve — the seccomp filter
// is installed in `pre_exec`, i.e. *before* std::process::Command performs
// the execve() that actually starts the target program. A filter missing
// execve blocks its own exec and every sandboxed command fails instantly.
// (This was previously the case here — neither list had ever actually been
// exercised via `SandboxPolicy::default()`, which set `seccomp_profile:
// None`, so the omission was latent rather than caught by a test.)
const STRICT_SYSCALLS: &[i64] = &[
    libc::SYS_execve,
    libc::SYS_execveat,
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
    libc::SYS_execve,
    libc::SYS_execveat,
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

/// High-impact syscalls denied by the mandatory `"default"` profile:
/// kernel-module control, raw device/IO privilege, namespace/mount
/// manipulation, process tracing, and system clock/accounting control.
/// No ordinary sandboxed shell command (ls, cat, grep, cargo, rustc, ...)
/// legitimately needs any of these; a command that calls one is either
/// misbehaving or attempting a container/sandbox escape.
const DANGEROUS_SYSCALLS: &[i64] = &[
    libc::SYS_ptrace,
    libc::SYS_process_vm_readv,
    libc::SYS_process_vm_writev,
    libc::SYS_mount,
    libc::SYS_umount2,
    libc::SYS_pivot_root,
    libc::SYS_chroot,
    libc::SYS_unshare,
    libc::SYS_setns,
    libc::SYS_reboot,
    libc::SYS_kexec_load,
    libc::SYS_init_module,
    libc::SYS_finit_module,
    libc::SYS_delete_module,
    libc::SYS_iopl,
    libc::SYS_ioperm,
    libc::SYS_acct,
    libc::SYS_swapon,
    libc::SYS_swapoff,
    libc::SYS_settimeofday,
    libc::SYS_clock_settime,
    libc::SYS_clock_adjtime,
    libc::SYS_adjtimex,
    libc::SYS_syslog,
    libc::SYS_add_key,
    libc::SYS_request_key,
    libc::SYS_keyctl,
    libc::SYS_bpf,
    libc::SYS_perf_event_open,
    libc::SYS_quotactl,
];
