use anyhow::{Context, Result};
use std::cell::RefCell;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;

pub struct RepositoryLock {
    path: PathBuf,
    file: Option<File>,
    reentrant: bool,
}

thread_local! {
    static HELD: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

impl RepositoryLock {
    pub fn acquire(root: &Path) -> Result<Self> {
        let root = root.canonicalize().context("resolve repository lock root")?;
        let root = if root.is_file() { root.parent().context("source has no parent")? } else { &root };
        let common = git_common_dir(root).unwrap_or_else(|_| root.to_path_buf());
        let path = common.join(".switchbard-storage.lock");
        if HELD.with(|held| held.borrow().iter().any(|held| held == &path)) {
            HELD.with(|held| held.borrow_mut().push(path.clone()));
            return Ok(Self { path, file: None, reentrant: true });
        }
        let file = File::create(&path).with_context(|| format!("create {}", path.display()))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            #[cfg(unix)]
            let acquired = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 };
            #[cfg(not(unix))]
            let acquired = true;
            if acquired { break; }
            if Instant::now() >= deadline { anyhow::bail!("timed out waiting for repository storage lock: {}", path.display()); }
            std::thread::sleep(Duration::from_millis(10));
        }
        HELD.with(|held| held.borrow_mut().push(path.clone()));
        Ok(Self { path, file: Some(file), reentrant: false })
    }
}

impl Drop for RepositoryLock {
    fn drop(&mut self) {
        HELD.with(|held| {
            let mut held = held.borrow_mut();
            if let Some(index) = held.iter().rposition(|path| path == &self.path) { held.remove(index); }
        });
        if self.reentrant { return; }
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

fn git_common_dir(root: &Path) -> Result<PathBuf> {
    let output = crate::git_cmd()
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()?;
    if !output.status.success() {
        anyhow::bail!("cannot resolve Git common directory for {}", root.display());
    }
    Ok(PathBuf::from(String::from_utf8(output.stdout)?.trim()).canonicalize()?)
}
