use crate::types::LocalListener;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(not(target_os = "linux"))]
use anyhow::anyhow;
#[cfg(not(target_os = "linux"))]
use std::process::Command;

pub fn scan_listeners() -> Result<Vec<LocalListener>> {
    #[cfg(target_os = "linux")]
    {
        let mut listeners = linux::scan_listeners()?;
        listeners.sort_by_key(|l| (l.port, l.pid));
        Ok(listeners)
    }

    #[cfg(not(target_os = "linux"))]
    {
        scan_listeners_lsof()
    }
}

#[cfg(not(target_os = "linux"))]
fn scan_listeners_lsof() -> Result<Vec<LocalListener>> {
    let raw = run_lsof_listen()?;
    let mut listeners = parse_lsof_listen(&raw);
    fill_cwds(&mut listeners)?;
    listeners.sort_by_key(|l| (l.port, l.pid));
    Ok(listeners)
}

#[cfg(not(target_os = "linux"))]
fn run_lsof_listen() -> Result<String> {
    let output = Command::new("lsof")
        .args([
            "-iTCP",
            "-sTCP:LISTEN",
            "-P", // numeric ports
            "-n", // numeric hosts
            "-F",
            "pgcnPL",
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

#[cfg(not(target_os = "linux"))]
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

#[cfg(not(target_os = "linux"))]
fn parse_port(name: &str) -> Option<u16> {
    let port_str = name.rsplit(':').next()?;
    port_str.parse().ok()
}

#[cfg(not(target_os = "linux"))]
fn dedup_by_port_pid(items: Vec<LocalListener>) -> Vec<LocalListener> {
    let mut seen: HashMap<(u32, u16), LocalListener> = HashMap::new();
    for item in items {
        seen.entry((item.pid, item.port)).or_insert(item);
    }
    seen.into_values().collect()
}

#[cfg(not(target_os = "linux"))]
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

#[cfg(not(target_os = "linux"))]
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

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use anyhow::Context;
    use std::collections::HashSet;
    use std::fs;
    use std::io;
    use std::path::Path;

    pub fn scan_listeners() -> Result<Vec<LocalListener>> {
        let sockets = listening_tcp_sockets()?;
        let mut out = Vec::new();
        let mut seen: HashSet<(u32, u16)> = HashSet::new();

        for entry in fs::read_dir("/proc").context("read /proc")? {
            let Ok(entry) = entry else {
                continue;
            };
            let Some(pid) = pid_from_proc_entry(&entry) else {
                continue;
            };
            let fd_dir = entry.path().join("fd");
            let Ok(fds) = fs::read_dir(fd_dir) else {
                continue;
            };
            for fd in fds.flatten() {
                let Ok(target) = fs::read_link(fd.path()) else {
                    continue;
                };
                let Some(inode) = socket_inode(&target) else {
                    continue;
                };
                let Some(port) = sockets.get(&inode).copied() else {
                    continue;
                };
                if !seen.insert((pid, port)) {
                    continue;
                }
                out.push(LocalListener {
                    pid,
                    pgid: pgid_for_pid(pid).unwrap_or(pid as i32),
                    port,
                    command_name: command_name(pid).unwrap_or_else(|| pid.to_string()),
                    cwd: cwd_for_pid(pid),
                });
            }
        }

        Ok(out)
    }

    fn listening_tcp_sockets() -> Result<HashMap<u64, u16>> {
        let mut out = HashMap::new();
        read_proc_net_tcp("/proc/net/tcp", &mut out)?;
        if let Err(err) = read_proc_net_tcp("/proc/net/tcp6", &mut out) {
            if err.downcast_ref::<io::Error>().map(io::Error::kind) != Some(io::ErrorKind::NotFound)
            {
                return Err(err);
            }
        }
        Ok(out)
    }

    fn read_proc_net_tcp(path: &str, out: &mut HashMap<u64, u16>) -> Result<()> {
        let text = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        for line in text.lines().skip(1) {
            if let Some((inode, port)) = parse_proc_net_tcp_line(line) {
                out.insert(inode, port);
            }
        }
        Ok(())
    }

    fn parse_proc_net_tcp_line(line: &str) -> Option<(u64, u16)> {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() <= 9 || fields[3] != "0A" {
            return None;
        }
        let local = fields[1];
        let (_, port_hex) = local.rsplit_once(':')?;
        let port = u16::from_str_radix(port_hex, 16).ok()?;
        if port == 0 {
            return None;
        }
        let inode = fields[9].parse().ok()?;
        Some((inode, port))
    }

    fn pid_from_proc_entry(entry: &fs::DirEntry) -> Option<u32> {
        entry.file_name().to_string_lossy().parse().ok()
    }

    fn socket_inode(path: &Path) -> Option<u64> {
        let text = path.to_string_lossy();
        let inner = text.strip_prefix("socket:[")?.strip_suffix(']')?;
        inner.parse().ok()
    }

    fn pgid_for_pid(pid: u32) -> Option<i32> {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let after_comm = stat.rsplit_once(") ")?.1;
        let mut fields = after_comm.split_whitespace();
        let _state = fields.next()?;
        let _ppid = fields.next()?;
        fields.next()?.parse().ok()
    }

    fn command_name(pid: u32) -> Option<String> {
        if let Ok(comm) = fs::read_to_string(format!("/proc/{pid}/comm")) {
            let trimmed = comm.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        let cmdline = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let first = cmdline.split(|b| *b == 0).next()?;
        if first.is_empty() {
            return None;
        }
        let path = Path::new(std::str::from_utf8(first).ok()?);
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
    }

    fn cwd_for_pid(pid: u32) -> Option<PathBuf> {
        fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::net::TcpListener;

        #[test]
        fn parses_listening_proc_net_tcp_line() {
            let line = "   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 424242 1 0000000000000000 100 0 0 10 0";
            assert_eq!(parse_proc_net_tcp_line(line), Some((424242, 8080)));
        }

        #[test]
        fn ignores_non_listening_proc_net_tcp_line() {
            let line = "   1: 0100007F:1F90 0100007F:C001 01 00000000:00000000 00:00000000 00000000  1000        0 424242 1 0000000000000000 100 0 0 10 0";
            assert_eq!(parse_proc_net_tcp_line(line), None);
        }

        #[test]
        fn extracts_socket_inode_from_proc_fd_target() {
            assert_eq!(socket_inode(Path::new("socket:[12345]")), Some(12345));
            assert_eq!(socket_inode(Path::new("/tmp/file")), None);
        }

        #[test]
        fn scanner_finds_current_process_listener() {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let port = listener.local_addr().unwrap().port();
            let pid = std::process::id();

            let listeners = scan_listeners().unwrap();

            assert!(
                listeners
                    .iter()
                    .any(|listener| listener.pid == pid && listener.port == port),
                "expected scanner to find pid {pid} listening on port {port}; got {listeners:?}"
            );
        }
    }
}

#[cfg(all(test, not(target_os = "linux")))]
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
