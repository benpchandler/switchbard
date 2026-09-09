use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(crate) struct RepositoryLock {
    path: PathBuf,
}

impl RepositoryLock {
    pub(crate) fn acquire(root: &Path) -> Result<Self> {
        let root = root.canonicalize().context("resolve repository lock root")?;
        let common = git_common_dir(&root)?;
        let path = common.join(".switchbard-storage.lock");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if Instant::now() >= deadline {
                        anyhow::bail!("timed out waiting for repository storage lock: {}", path.display());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error).with_context(|| format!("create {}", path.display())),
            }
        }
    }
}

impl Drop for RepositoryLock {
    fn drop(&mut self) {
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
