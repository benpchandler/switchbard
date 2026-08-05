//! Single-instance guard — a pid-stamped lock file at `~/.switchbard/
//! switchbard.lock`.
//!
//! TASK-22's incident writeup named "two GUI instances racing config saves"
//! as a contributing risk, separate from the confirmed root cause. This
//! gives a second launch a way to detect a live instance and back off
//! instead of both processes writing `~/.switchbard/config.toml`
//! concurrently.
//!
//! Stale-detecting: a lock file left behind by a crash (no clean `Drop`) is
//! not honored forever. `acquire` checks whether the pid it names is still
//! alive via `kill(pid, 0)` (same probe `kill.rs` uses for process-group
//! liveness) and silently reclaims the file if not.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;

const RELATIVE_PATH: &str = ".switchbard/switchbard.lock";

/// Held for the lifetime of the acquiring process. Dropping it removes the
/// lock file, but only if the file still names *this* process's pid — a
/// lock we no longer hold (e.g. reclaimed elsewhere) must never be deleted
/// out from under whoever holds it now.
#[derive(Debug)]
pub struct InstanceLock {
    path: PathBuf,
}

#[derive(Debug)]
pub enum AcquireError {
    /// Another live process already holds the lock.
    AlreadyRunning(u32),
    Io(io::Error),
}

impl fmt::Display for AcquireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRunning(pid) => write!(f, "already running as pid {pid}"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AcquireError {}

impl From<io::Error> for AcquireError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// The single canonical lock path, beside `config::default_path()`. `None`
/// only if `dirs::home_dir` can't find a home directory.
pub fn default_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(RELATIVE_PATH))
}

/// Acquire the single-instance lock at `path`, stamping it with this
/// process's pid. Returns `AlreadyRunning` if an existing lock file names a
/// pid that is still alive. A missing, unparsable, or dead-pid lock file is
/// reclaimed silently — that's the "stale-detecting" half of the contract.
pub fn acquire(path: &Path) -> Result<InstanceLock, AcquireError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(pid) = live_holder(path) {
        return Err(AcquireError::AlreadyRunning(pid));
    }
    fs::write(path, process::id().to_string())?;
    Ok(InstanceLock {
        path: path.to_path_buf(),
    })
}

/// `Some(pid)` if `path` names a pid that is still alive; `None` if the
/// file is missing, unparsable, or names a process that's gone — any of
/// which mean the lock is free to take.
fn live_holder(path: &Path) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    let pid: u32 = text.trim().parse().ok()?;
    process_is_alive(pid).then_some(pid)
}

/// `kill(pid, 0)` — signal 0 doesn't deliver; it just validates the target.
/// Mirrors `kill::group_alive`'s EPERM handling: a pid that exists but that
/// we can't signal (owned by another user) is still alive for this
/// purpose — only ESRCH means "gone".
fn process_is_alive(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        if let Ok(text) = fs::read_to_string(&self.path) {
            if text.trim() == process::id().to_string() {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn acquires_a_fresh_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("switchbard.lock");

        let lock = acquire(&path).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents.trim(), process::id().to_string());
        drop(lock);
    }

    #[test]
    fn refuses_when_another_live_process_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("switchbard.lock");

        let mut child = Command::new("sleep")
            .arg("9999")
            .spawn()
            .expect("spawn sleep");
        let child_pid = child.id();
        std::thread::sleep(Duration::from_millis(50));
        fs::write(&path, child_pid.to_string()).unwrap();

        let err = acquire(&path).unwrap_err();
        assert!(matches!(err, AcquireError::AlreadyRunning(pid) if pid == child_pid));

        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn reclaims_a_lock_left_by_a_dead_pid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("switchbard.lock");

        // Far above the macOS PID ceiling (99999); cannot be a live process.
        fs::write(&path, "987654321").unwrap();

        let lock = acquire(&path).unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents.trim(), process::id().to_string());
        drop(lock);
    }

    #[test]
    fn reclaims_a_corrupted_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("switchbard.lock");
        fs::write(&path, "not-a-pid").unwrap();

        let lock = acquire(&path).unwrap();
        drop(lock);
    }

    #[test]
    fn drop_removes_the_lock_file_it_still_owns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("switchbard.lock");

        let lock = acquire(&path).unwrap();
        assert!(path.exists());
        drop(lock);
        assert!(!path.exists());
    }

    #[test]
    fn drop_does_not_remove_a_lock_file_it_no_longer_owns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("switchbard.lock");

        let lock = acquire(&path).unwrap();
        // Someone else reclaimed the file after we lost it (simulated).
        fs::write(&path, "1").unwrap();

        drop(lock);
        assert!(
            path.exists(),
            "drop must not delete a lock it no longer owns"
        );
    }

    #[test]
    fn missing_parent_dir_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/switchbard.lock");

        let lock = acquire(&path).unwrap();
        assert!(path.exists());
        drop(lock);
    }
}
