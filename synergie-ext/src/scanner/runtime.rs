//! Snapshot runtime : ports écoutés (ss), services systemd, processus.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeSnapshot {
    pub listeners: Vec<ListeningPort>,
    pub units: Vec<SystemdUnit>,
    pub processes: HashMap<u32, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListeningPort {
    pub proto: String,
    pub addr: String,
    pub port: u16,
    pub pid: Option<u32>,
    pub process: Option<String>,
    pub cwd: Option<PathBuf>,
    pub cmdline: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemdUnit {
    pub name: String,
    pub state: String,
    pub description: String,
}

pub fn snapshot() -> Result<RuntimeSnapshot> {
    let mut snap = RuntimeSnapshot::default();

    // ss : TCP + UDP listeners
    if let Ok(out) = Command::new("ss").args(["-Htnlp"]).output() {
        if out.status.success() {
            parse_ss(&String::from_utf8_lossy(&out.stdout), "tcp", &mut snap);
        }
    }
    if let Ok(out) = Command::new("ss").args(["-Hunlp"]).output() {
        if out.status.success() {
            parse_ss(&String::from_utf8_lossy(&out.stdout), "udp", &mut snap);
        }
    }

    // systemctl list-units
    if let Ok(out) = Command::new("systemctl")
        .args(["list-units", "--type=service", "--all", "--no-legend", "--plain"])
        .output()
    {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    snap.units.push(SystemdUnit {
                        name: parts[0].to_string(),
                        state: parts[2].to_string(),
                        description: parts[4..].join(" "),
                    });
                }
            }
        }
    }

    // Enrichir avec /proc/<pid>/cwd + cmdline
    for l in &mut snap.listeners {
        if let Some(pid) = l.pid {
            l.cwd = read_proc_link(pid, "cwd");
            l.cmdline = read_proc_cmdline(pid);
            if let Some(cmd) = &l.cmdline {
                snap.processes.insert(pid, cmd.clone());
            }
        }
    }

    Ok(snap)
}

fn parse_ss(text: &str, proto: &str, snap: &mut RuntimeSnapshot) {
    for line in text.lines() {
        // Format ss : State Recv-Q Send-Q Local Address:Port Peer Address:Port users:(("name",pid=N,fd=M))
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 { continue; }
        let local = parts[3];
        let (addr, port) = match local.rsplit_once(':') {
            Some((a, p)) => (a.to_string(), p.parse::<u16>().unwrap_or(0)),
            None => continue,
        };
        if port == 0 { continue; }

        let mut pid = None;
        let mut process = None;
        if let Some(users) = parts.iter().find(|p| p.starts_with("users:")) {
            // users:(("name",pid=N,fd=M))
            if let Some(name_start) = users.find('"') {
                if let Some(name_end) = users[name_start + 1..].find('"') {
                    process = Some(users[name_start + 1..name_start + 1 + name_end].to_string());
                }
            }
            if let Some(pid_idx) = users.find("pid=") {
                let tail = &users[pid_idx + 4..];
                let pid_str: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
                pid = pid_str.parse().ok();
            }
        }

        snap.listeners.push(ListeningPort {
            proto: proto.to_string(),
            addr,
            port,
            pid,
            process,
            cwd: None,
            cmdline: None,
        });
    }
}

fn read_proc_link(pid: u32, what: &str) -> Option<PathBuf> {
    let p = PathBuf::from(format!("/proc/{}/{}", pid, what));
    std::fs::read_link(&p).ok()
}

fn read_proc_cmdline(pid: u32) -> Option<String> {
    let p = PathBuf::from(format!("/proc/{}/cmdline", pid));
    let bytes = std::fs::read(&p).ok()?;
    Some(
        bytes
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect::<Vec<_>>()
            .join(" "),
    )
}
