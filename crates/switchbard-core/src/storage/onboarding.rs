//! Atomic fresh repository activation; retained legacy records require migration.
use super::{identity, persist_change, RepositoryId, RepositoryLock, Store};
use anyhow::{ensure, Result};
use rusqlite::{params, TransactionBehavior};
use std::path::Path;
use uuid::Uuid;

const NATIVE_KINDS: [&str; 6] = [
    "initiative",
    "project",
    "config",
    "ranking",
    "goals",
    "task",
];

impl Store {
    pub(crate) fn setup_repository(
        &mut self,
        root: &Path,
        options: Option<&crate::RepositorySetupOptions>,
    ) -> Result<RepositoryId> {
        let root = root.canonicalize()?;
        ensure!(root.is_dir(), "repository root must be a directory");
        let binding = identity::repository_binding(&root)?;
        let _lock = RepositoryLock::acquire_identities(vec![RepositoryLock::identity(&root)?])?;
        if let Some(repo) = self.repository(&root)? {
            self.check_existing_setup(&repo, options)?;
            return Ok(repo);
        }
        let digests = empty_inventory_digests(&root)?;
        self.activate_fresh_repository(&root, &binding, &digests, options)
    }

    fn check_existing_setup(
        &self,
        repo: &RepositoryId,
        options: Option<&crate::RepositorySetupOptions>,
    ) -> Result<()> {
        ensure!(self.workspace_is_central(repo)?, "registered workspace has legacy record kinds; use sb storage migrate for reviewed migration");
        if let Some(options) = options {
            let config = self.read(repo, "config", "backlog/config.yml")?;
            let matches = match config {
                Some(config) if !config.deleted => options.matches_config(&config.content)?,
                _ => false,
            };
            ensure!(matches, "workspace already exists with different settings; setup cannot overwrite them; launch sbt without setup overrides to use its existing settings");
        }
        Ok(())
    }

    pub(crate) fn workspace_is_central(&self, repo: &RepositoryId) -> Result<bool> {
        for kind in NATIVE_KINDS {
            if !self.authority(repo, kind)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn activate_fresh_repository(
        &mut self,
        root: &Path,
        binding: &str,
        digests: &[String],
        options: Option<&crate::RepositorySetupOptions>,
    ) -> Result<RepositoryId> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure!(
            empty_inventory_digests(root)? == digests,
            "repository sources changed during setup; retry"
        );
        let repo = RepositoryId(Uuid::new_v4().to_string());
        tx.execute(
            "INSERT INTO repositories(repo_id,epoch_id) VALUES (?1,?2)",
            params![repo.0, Uuid::new_v4().to_string()],
        )?;
        identity::register(&tx, &repo, binding, root)?;
        activate_defaults(&tx, &repo, options)?;
        tx.commit()?;
        Ok(repo)
    }
}

fn empty_inventory_digests(root: &Path) -> Result<Vec<String>> {
    NATIVE_KINDS.into_iter().map(|kind| {
        let inventory = crate::backlog::migration::inventory_kind(root, kind)?;
        ensure!(inventory.documents.is_empty(),
            "existing legacy {kind} data requires reviewed migration; use sb storage migrate --kind {kind}");
        Ok(inventory.digest())
    }).collect()
}

fn activate_defaults(
    tx: &rusqlite::Transaction<'_>,
    repo: &RepositoryId,
    options: Option<&crate::RepositorySetupOptions>,
) -> Result<()> {
    for kind in NATIVE_KINDS {
        tx.execute(
            "INSERT INTO authority(repo_id,kind) VALUES (?1,?2)",
            params![repo.0, kind],
        )?;
    }
    let options = options.cloned().unwrap_or_default().validated()?;
    persist_change(
        tx,
        repo,
        "config",
        "backlog/config.yml",
        None,
        Some(options.config_bytes()?),
    )?;
    Ok(())
}
