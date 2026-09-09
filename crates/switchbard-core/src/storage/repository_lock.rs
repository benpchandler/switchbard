use anyhow::{Context, Result};
use std::cell::RefCell;
use std::fs::File;
use std::marker::PhantomData;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct RepositoryLock {
    state: Arc<LockState>,
    _not_send: PhantomData<Rc<()>>,
}

struct LockState {
    path: PathBuf,
    _file: File,
}

thread_local! {
    static HELD: RefCell<Vec<(PathBuf, Arc<LockState>)>> = const { RefCell::new(Vec::new()) };
}

impl RepositoryLock {
    pub(crate) fn identity(root: &Path) -> Result<PathBuf> {
        let root = root
            .canonicalize()
            .context("resolve repository lock root")?;
        let root = if root.is_file() {
            root.parent().context("source has no parent")?
        } else {
            &root
        };
        Ok(git_common_dir(root).unwrap_or_else(|_| root.to_path_buf()))
    }

    pub fn acquire(root: &Path) -> Result<Self> {
        let common = Self::identity(root)?;
        let path = common.join(".switchbard-storage.lock");
        if let Some(state) = HELD.with(|held| {
            held.borrow()
                .iter()
                .find(|(held, _)| held == &path)
                .map(|(_, state)| Arc::clone(state))
        }) {
            HELD.with(|held| held.borrow_mut().push((path, Arc::clone(&state))));
            return Ok(Self {
                state,
                _not_send: PhantomData,
            });
        }
        let file = File::create(&path).with_context(|| format!("create {}", path.display()))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            #[cfg(unix)]
            let acquired =
                unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 };
            #[cfg(not(unix))]
            let acquired = true;
            if acquired {
                break;
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "timed out waiting for repository storage lock: {}",
                    path.display()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let state = Arc::new(LockState {
            path: path.clone(),
            _file: file,
        });
        HELD.with(|held| held.borrow_mut().push((path, Arc::clone(&state))));
        Ok(Self {
            state,
            _not_send: PhantomData,
        })
    }
}

impl Drop for RepositoryLock {
    fn drop(&mut self) {
        HELD.with(|held| {
            let mut held = held.borrow_mut();
            if let Some(index) = held.iter().rposition(|(path, _)| path == &self.state.path) {
                held.remove(index);
            }
        });
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

#[cfg(test)]
mod tests {
    use super::RepositoryLock;

    #[test]
    fn nested_guards_keep_the_stable_lock_until_last_drop() {
        let root = tempfile::tempdir().unwrap();
        let outer = RepositoryLock::acquire(root.path()).unwrap();
        let inner = RepositoryLock::acquire(root.path()).unwrap();
        drop(outer);
        assert!(root.path().join(".switchbard-storage.lock").exists());
        drop(inner);
        assert!(root.path().join(".switchbard-storage.lock").exists());
    }
}
