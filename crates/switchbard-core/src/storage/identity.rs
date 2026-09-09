//! Repository identity and retained path aliases, independent of checkout availability.
use super::{RepositoryId, Store};
use anyhow::{bail, ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

impl Store {
    pub(crate) fn repository_lock_for(&self, repo: &RepositoryId) -> Result<super::RepositoryLock> {
        let binding: String = self.connection.query_row(
            "SELECT binding FROM bindings WHERE repo_id=?1 ORDER BY binding LIMIT 1",
            [&repo.0],
            |row| row.get(0),
        )?;
        let root = binding
            .strip_prefix("git:")
            .or_else(|| binding.strip_prefix("path:"))
            .context("repository has no lockable path binding")?;
        super::RepositoryLock::acquire(Path::new(root))
    }

    pub fn repository(&self, root: &Path) -> Result<Option<RepositoryId>> {
        let alias = path_alias(root)?;
        let retained = lookup(&self.connection, &alias)?;
        match std::fs::metadata(root) {
            Ok(_) => {
                let binding = repository_binding(root)?;
                let current = lookup(&self.connection, &binding)?;
                ensure!(
                    retained.is_none() || retained == current,
                    "repository path now has a different identity; explicit rebind is required: {}",
                    root.display()
                );
                reject_legacy_binding(&self.connection, root, current.as_ref())?;
                Ok(current)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(retained),
            Err(error) => Err(error.into()),
        }
    }

    pub fn bind_repository(&mut self, root: &Path) -> Result<RepositoryId> {
        let _repository_lock = super::RepositoryLock::acquire(root)?;
        let binding = repository_binding(root)?;
        self.repository(root)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let repo = if let Some(repo) = lookup(&tx, &binding)? {
            repo
        } else {
            let repo = RepositoryId(Uuid::new_v4().to_string());
            tx.execute(
                "INSERT INTO repositories(repo_id,epoch_id) VALUES (?1,?2)",
                params![repo.0, Uuid::new_v4().to_string()],
            )?;
            repo
        };
        register(&tx, &repo, &binding, root)?;
        tx.commit()?;
        Ok(repo)
    }

    /// Explicit continuity decision after a move. Previous aliases remain readable.
    pub fn rebind_repository(&mut self, repo: &RepositoryId, new_root: &Path) -> Result<()> {
        let _repository_lock = super::RepositoryLock::acquire(new_root)?;
        let binding = repository_binding(new_root)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let known: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM repositories WHERE repo_id=?1)",
            [&repo.0],
            |row| row.get(0),
        )?;
        ensure!(known, "unknown repository ID");
        register(&tx, repo, &binding, new_root)?;
        tx.commit()?;
        Ok(())
    }
}

pub(super) fn register(
    connection: &Connection,
    repo: &RepositoryId,
    binding: &str,
    root: &Path,
) -> Result<()> {
    ensure!(
        repository_binding(root)? == binding,
        "repository directory changed during binding; retry"
    );
    let canonical = root.canonicalize()?;
    let legacy = binding_parts(root)?.0;
    ensure!(
        lookup(connection, &legacy)?
            .as_ref()
            .is_none_or(|existing| existing == repo),
        "older repository binding belongs to another repository"
    );
    let mut changes = 0usize;
    for key in [
        binding.to_string(),
        path_alias(root)?,
        path_alias(&canonical)?,
    ] {
        let existing = lookup(connection, &key)?;
        ensure!(
            existing.as_ref().is_none_or(|existing| existing == repo),
            "repository binding belongs to another repository: {key}"
        );
        changes += connection.execute(
            "INSERT OR IGNORE INTO bindings(binding,repo_id) VALUES (?1,?2)",
            params![key, repo.0],
        )?;
    }
    if changes > 0 {
        connection.execute("UPDATE metadata SET sequence=sequence+1", [])?;
    }
    Ok(())
}

fn lookup(connection: &Connection, binding: &str) -> Result<Option<RepositoryId>> {
    Ok(connection
        .query_row(
            "SELECT repo_id FROM bindings WHERE binding=?1",
            [binding],
            |row| Ok(RepositoryId(row.get(0)?)),
        )
        .optional()?)
}

fn reject_legacy_binding(
    connection: &Connection,
    root: &Path,
    current: Option<&RepositoryId>,
) -> Result<()> {
    if current.is_some() {
        return Ok(());
    }
    let (legacy, _) = binding_parts(root)?;
    ensure!(
        lookup(connection, &legacy)?.is_none(),
        "older repository binding needs an explicit rebind to establish directory identity"
    );
    Ok(())
}

pub(super) fn repository_binding(root: &Path) -> Result<String> {
    let (key, identity_path) = binding_parts(root)?;
    let metadata = std::fs::metadata(identity_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(format!(
            "{key}#instance:{}:{}",
            metadata.dev(),
            metadata.ino()
        ))
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Ok(key)
    }
}

fn binding_parts(root: &Path) -> Result<(String, PathBuf)> {
    let root = root.canonicalize().context("resolve repository path")?;
    ensure!(root.is_dir(), "repository path is not a directory");
    let output = crate::git_cmd()
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()?;
    if output.status.success() {
        let common = PathBuf::from(String::from_utf8(output.stdout)?.trim()).canonicalize()?;
        return Ok((
            format!(
                "git:{}",
                common.to_str().context("non-UTF8 Git common directory")?
            ),
            common,
        ));
    }
    if root.join(".git").try_exists()? {
        bail!("cannot resolve Git common directory")
    }
    Ok((
        format!(
            "path:{}",
            root.to_str().context("non-UTF8 repository path")?
        ),
        root,
    ))
}

fn path_alias(root: &Path) -> Result<String> {
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    Ok(format!(
        "alias:{}",
        normalized.to_str().context("non-UTF8 repository path")?
    ))
}
