use seccomp::{Action, Compare, Context, Op, Rule};

pub fn install_filter(profile: &str) -> Result<(), std::io::Error> {
    match profile {
        "strict" => {
            let mut ctx = Context::default(Action::Errno(libc::EPERM))
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            add_strict_rules(&mut ctx)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            ctx.load()
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            Ok(())
        }
        "default" => {
            let mut ctx = Context::default(Action::Errno(libc::EPERM))
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            add_default_rules(&mut ctx)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            ctx.load()
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            Ok(())
        }
        "unconfined" => Ok(()),
        _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "unknown profile")),
    }
}

fn add_strict_rules(ctx: &mut Context) -> Result<(), seccomp::SeccompError> {
    let syscalls = [
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
        libc::SYS_getuid,
        libc::SYS_getgid,
    ];
    for syscall in syscalls {
        ctx.add_rule(Rule::new(
            syscall as usize,
            Compare::arg(0).using(Op::Eq).with(0).build().unwrap(),
            Action::Allow,
        ))?;
    }
    Ok(())
}

fn add_default_rules(ctx: &mut Context) -> Result<(), seccomp::SeccompError> {
    let syscalls = [
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
        libc::SYS_getuid,
        libc::SYS_getgid,
        libc::SYS_statx,
        libc::SYS_lseek,
        libc::SYS_ioctl,
    ];
    for syscall in syscalls {
        ctx.add_rule(Rule::new(
            syscall as usize,
            Compare::arg(0).using(Op::Eq).with(0).build().unwrap(),
            Action::Allow,
        ))?;
    }
    Ok(())
}
