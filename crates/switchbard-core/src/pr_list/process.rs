//! Bound both wall time and allocation for authenticated CLI reads.
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;
const POLLS: usize = 300;

pub(super) fn run_gh(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = command(repo, args);
    let mut child = command.spawn().map_err(|e| format!("Cannot run gh: {e}"))?;
    let stdout = reader(child.stdout.take().expect("piped stdout"));
    let stderr = reader(child.stderr.take().expect("piped stderr"));
    let status = wait(&mut child);
    let cleanup = cleanup(&mut child);
    let out = receive(stdout);
    let err = receive(stderr);
    let status = status?;
    cleanup?;
    let out = out?;
    let err = err?;
    if !status.success() {
        let detail: String = String::from_utf8_lossy(&err)
            .chars()
            .filter(|c| !c.is_control())
            .take(500)
            .collect();
        return Err(format!("GitHub read failed: {detail}"));
    }
    Ok(out)
}

fn command(repo: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("gh");
    command
        .current_dir(repo)
        .args(args)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GH_REPO");
    for var in [
        "GIT_DIR",
        "GIT_INDEX_FILE",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_NAMESPACE",
    ] {
        command.env_remove(var);
    }
    command
}

fn reader(stream: impl Read + Send + 'static) -> Receiver<Result<Vec<u8>, String>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stream
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Cannot read gh output: {e}"))
            .and_then(|_| {
                if bytes.len() as u64 > OUTPUT_LIMIT {
                    Err("GitHub output exceeded 4 MiB limit".to_owned())
                } else {
                    Ok(bytes)
                }
            });
        if sender.send(result).is_err() { /* caller already timed out */ }
    });
    receiver
}

fn receive(receiver: Receiver<Result<Vec<u8>, String>>) -> Result<Vec<u8>, String> {
    receiver
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| "GitHub output stream did not close".to_owned())?
}

fn wait(child: &mut Child) -> Result<ExitStatus, String> {
    for _ in 0..POLLS {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok(status);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("GitHub read timed out after 30 seconds".to_owned())
}

fn cleanup(child: &mut Child) -> Result<(), String> {
    if child.try_wait().map_err(|e| e.to_string())?.is_none() {
        child.kill().map_err(|e| format!("Cannot stop gh: {e}"))?;
        child.wait().map_err(|e| format!("Cannot reap gh: {e}"))?;
    }
    let group = i32::try_from(child.id()).map_err(|e| e.to_string())?;
    crate::kill_pgid(group, Duration::ZERO)
        .map_err(|e| format!("Cannot clean up gh group: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_bounded_and_reports_overflow() {
        let small = reader(std::io::Cursor::new(b"ok".to_vec()));
        assert_eq!(receive(small).expect("small output"), b"ok");
        let large = reader(std::io::repeat(b'x'));
        assert!(receive(large)
            .expect_err("unbounded output rejected")
            .contains("limit"));
    }

    #[test]
    fn command_cannot_inherit_a_different_repository() {
        let command = command(Path::new("/tmp"), &["repo", "view"]);
        let vars: std::collections::HashMap<_, _> = command.get_envs().collect();
        for name in [
            "GH_REPO",
            "GIT_DIR",
            "GIT_INDEX_FILE",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_NAMESPACE",
        ] {
            assert_eq!(vars.get(std::ffi::OsStr::new(name)), Some(&None));
        }
        assert_eq!(command.get_current_dir(), Some(Path::new("/tmp")));
    }

    #[test]
    fn cleanup_reaps_a_real_child() {
        let mut child = Command::new("sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sleep available");
        cleanup(&mut child).expect("child and group cleaned up");
        assert!(child.try_wait().expect("status").is_some());
    }
}
