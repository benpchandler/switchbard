use crate::types::LocalListener;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

pub fn scan_listeners() -> Result<Vec<LocalListener>> {
    let raw = run_lsof_listen()?;
    let mut listeners = parse_lsof_listen(&raw);
    fill_cwds(&mut listeners)?;
    listeners.sort_by_key(|l| (l.port, l.pid));
    Ok(listeners)
}

fn run_lsof_listen() -> Result<String> {
    let output = Command::new("lsof")
        .args([
            "-iTCP",
            "-sTCP:LISTEN",
            "-P", // numeric ports
            "-n", // numeric hosts
            "-F", "pgcnPL",
        ])
        .output()
        .map_err(|e| anyhow!("failed to spawn lsof: {e}"))?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(anyhow!(
            "lsof exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_lsof_listen(raw: &str) -> Vec<LocalListener> {
    let mut out = Vec::new();
    let mut pid: Option<u32> = None;
    let mut pgid: Option<i32> = None;
    let mut cmd: Option<String> = None;
    let mut user_skip = false;

    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let (tag, rest) = (line.as_bytes()[0] as char, &line[1..]);
        match tag {
            'p' => {
                pid = rest.parse().ok();
                pgid = None;
                cmd = None;
                user_skip = false;
            }
            'g' => {
                pgid = rest.parse().ok();
            }
            'c' => {
                cmd = Some(rest.to_string());
            }
            'L' => {
                // Login name. Don't filter; we want everything the current user can see.
                // (running lsof unprivileged already restricts to processes we can read.)
                let _ = user_skip;
            }
            'n' => {
                // e.g. "*:54687" or "127.0.0.1:7768" or "[::1]:54687"
                if let (Some(p), Some(c)) = (pid, cmd.clone()) {
                    if let Some(port) = parse_port(rest) {
                        out.push(LocalListener {
                            pid: p,
                            pgid: pgid.unwrap_or(p as i32),
                            port,
                            command_name: c,
                            cwd: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    dedup_by_port_pid(out)
}

fn parse_port(name: &str) -> Option<u16> {
    let port_str = name.rsplit(':').next()?;
    port_str.parse().ok()
}

fn dedup_by_port_pid(items: Vec<LocalListener>) -> Vec<LocalListener> {
    let mut seen: HashMap<(u32, u16), LocalListener> = HashMap::new();
    for item in items {
        seen.entry((item.pid, item.port)).or_insert(item);
    }
    seen.into_values().collect()
}

fn fill_cwds(listeners: &mut [LocalListener]) -> Result<()> {
    let mut unique_pids: Vec<u32> = listeners.iter().map(|l| l.pid).collect();
    unique_pids.sort();
    unique_pids.dedup();
    let cwds = cwds_for_pids(&unique_pids);
    for l in listeners.iter_mut() {
        l.cwd = cwds.get(&l.pid).cloned();
    }
    Ok(())
}

fn cwds_for_pids(pids: &[u32]) -> HashMap<u32, PathBuf> {
    let mut out = HashMap::new();
    if pids.is_empty() {
        return out;
    }
    let pid_args: Vec<String> = pids.iter().map(|p| p.to_string()).collect();
    let mut args: Vec<&str> = vec!["-a", "-d", "cwd", "-F", "pn"];
    let joined = pid_args.join(",");
    args.push("-p");
    args.push(&joined);
    let Ok(output) = Command::new("lsof").args(&args).output() else {
        return out;
    };
    let raw = String::from_utf8_lossy(&output.stdout);
    let mut current_pid: Option<u32> = None;
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let tag = line.as_bytes()[0] as char;
        let rest = &line[1..];
        match tag {
            'p' => current_pid = rest.parse().ok(),
            'n' => {
                if let Some(p) = current_pid {
                    out.insert(p, PathBuf::from(rest));
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_lsof_record() {
        let raw = "\
p1218
g1218
clyon-bundle
f6
PTCP
n*:54687
";
        let out = parse_lsof_listen(raw);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].pid, 1218);
        assert_eq!(out[0].pgid, 1218);
        assert_eq!(out[0].port, 54687);
        assert_eq!(out[0].command_name, "lyon-bundle");
    }

    #[test]
    fn parses_multiple_processes_and_dedups_per_pid_port() {
        let raw = "\
p100
g99
cserver
f6
PTCP
n*:8080
f7
PTCP
n*:8080
p200
g200
cother
f6
PTCP
n127.0.0.1:9000
";
        let out = parse_lsof_listen(raw);
        assert_eq!(out.len(), 2);
        let ports: Vec<u16> = out.iter().map(|l| l.port).collect();
        assert!(ports.contains(&8080));
        assert!(ports.contains(&9000));
    }

    #[test]
    fn parses_ipv6_bracket_form() {
        let raw = "p1\ng1\ncfoo\nf6\nPTCP\nn[::1]:54687\n";
        let out = parse_lsof_listen(raw);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].port, 54687);
    }
}
