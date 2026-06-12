use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

use nix::sys::resource::{setrlimit, Resource};
use nix::unistd::Pid;

use crate::config::SandboxConfig;

/// Configure a [`Command`] to apply resource limits via [`CommandExt::pre_exec`].
/// This contains the ONLY unsafe block in the entire workspace.
#[allow(unsafe_code)]
pub fn configure_sandbox_command(cmd: &mut Command, config: &SandboxConfig) {
    let cfg = config.clone();

    unsafe {
        cmd.pre_exec(move || {
            // SAFETY: This closure runs in the child process after fork and
            // before exec. We are single-threaded; no other code is executing.

            // Create new process group
            nix::unistd::setpgid(Pid::from_raw(0), Pid::from_raw(0))
                .map_err(|e| io::Error::other(format!("setpgid: {e}")))?;

            // CPU time limit
            setrlimit(Resource::RLIMIT_CPU, cfg.cpu_limit_secs, cfg.cpu_limit_secs)
                .map_err(|e| io::Error::other(format!("setrlimit CPU: {e}")))?;

            // Address space (memory) limit
            setrlimit(
                Resource::RLIMIT_AS,
                cfg.mem_limit_bytes,
                cfg.mem_limit_bytes,
            )
            .map_err(|e| io::Error::other(format!("setrlimit AS: {e}")))?;

            // Number of processes limit
            setrlimit(Resource::RLIMIT_NPROC, cfg.nproc_limit, cfg.nproc_limit)
                .map_err(|e| io::Error::other(format!("setrlimit NPROC: {e}")))?;

            // File size limit
            setrlimit(
                Resource::RLIMIT_FSIZE,
                cfg.fsize_limit_bytes,
                cfg.fsize_limit_bytes,
            )
            .map_err(|e| io::Error::other(format!("setrlimit FSIZE: {e}")))?;

            // Number of open files limit
            setrlimit(Resource::RLIMIT_NOFILE, cfg.nofile_limit, cfg.nofile_limit)
                .map_err(|e| io::Error::other(format!("setrlimit NOFILE: {e}")))?;

            // No core dumps
            let ret = libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }

            // No new privileges
            let ret = libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }

            Ok(())
        });
    }
}
