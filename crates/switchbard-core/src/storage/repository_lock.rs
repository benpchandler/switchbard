use anyhow::{ensure, Context, Result};
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

pub(crate) struct RepositoryLockSet {
    _locks: Vec<RepositoryLock>,
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
        Ok(crate::git_common_dir::resolve(root).unwrap_or_else(|| root.to_path_buf()))
    }

    /// The fence a native write in `root` needs when it touches `kinds`.
    ///
    /// Returns `None` once every kind is centrally authoritative. Central writes
    /// are serialized by SQLite and recheck authority, revisions and sequences
    /// inside their own transaction, and authority is only ever granted (by
    /// migration and import), never revoked, so a kind observed central stays
    /// central. A write that sees any file-authoritative kind takes the lock that
    /// migration and import hold, exactly as before. List every kind the
    /// operation may read or write: an omitted legacy kind would be written
    /// without the fence.
    pub fn fence(root: &Path, kinds: &[&str]) -> Result<Option<Self>> {
        assert!(
            !kinds.is_empty(),
            "invariant: a fence names its record kinds"
        );
        if all_central(root, kinds)? {
            return Ok(None);
        }
        Self::acquire(root).map(Some)
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
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let file = options
            .open(&path)
            .with_context(|| format!("open {}", path.display()))?;
        ensure!(
            file.metadata()?.is_file(),
            "repository lock is not a regular file"
        );
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

    pub(crate) fn acquire_identities(mut identities: Vec<PathBuf>) -> Result<RepositoryLockSet> {
        identities.sort();
        identities.dedup();
        let locks = identities
            .iter()
            .map(|identity| Self::acquire(identity))
            .collect::<Result<Vec<_>>>()?;
        Ok(RepositoryLockSet { _locks: locks })
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

fn all_central(root: &Path, kinds: &[&str]) -> Result<bool> {
    let Some(store) = super::Store::open_existing_default()? else {
        return Ok(false);
    };
    let Some(repo) = store.repository(root)? else {
        return Ok(false);
    };
    for kind in kinds {
        if !store.authority(&repo, kind)? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::RepositoryLock;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn nested_guards_keep_the_stable_lock_until_last_drop() {
        if let Ok(root) = std::env::var("SWITCHBARD_LOCK_CHILD") {
            let _guard = RepositoryLock::acquire(std::path::Path::new(&root)).unwrap();
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let outer = RepositoryLock::acquire(root.path()).unwrap();
        let inner = RepositoryLock::acquire(root.path()).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "storage::repository_lock::tests::nested_guards_keep_the_stable_lock_until_last_drop",
                "--exact",
                "--nocapture",
            ])
            .env("SWITCHBARD_LOCK_CHILD", root.path())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert!(child.try_wait().unwrap().is_none());
        drop(outer);
        std::thread::sleep(Duration::from_millis(100));
        assert!(child.try_wait().unwrap().is_none());
        drop(inner);
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn fence_is_skipped_only_when_every_kind_is_central() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let database = data.path().join("switchbard.sqlite3");
        super::super::with_test_database(&database, || {
            assert!(RepositoryLock::fence(root.path(), &["task"])
                .unwrap()
                .is_some());
            let mut store = super::super::Store::open(&database).unwrap();
            assert!(RepositoryLock::fence(root.path(), &["task"])
                .unwrap()
                .is_some());
            let repo = store.bind_repository(root.path()).unwrap();
            store
                .connection
                .execute("INSERT INTO authority VALUES (?1,'task')", [&repo.0])
                .unwrap();
            assert!(RepositoryLock::fence(root.path(), &["task"])
                .unwrap()
                .is_none());
            assert!(RepositoryLock::fence(root.path(), &["task", "ranking"])
                .unwrap()
                .is_some());
        });
    }
}
