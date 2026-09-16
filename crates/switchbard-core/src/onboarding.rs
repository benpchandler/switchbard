//! Backend-neutral repository onboarding contract shared by frontends.
use anyhow::{ensure, Result};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryOnboardingStatus {
    Central,
    Legacy,
    Registered,
    Unconfigured,
}

/// Established-store errors propagate; absence never conceals corruption.
pub fn repository_onboarding_status(root: &Path) -> Result<RepositoryOnboardingStatus> {
    ensure!(root.is_dir(), "repository root must be a directory");
    if let Some(store) = crate::storage::Store::open_existing_default()? {
        if let Some(repo) = store.repository(root)? {
            return Ok(if store.workspace_is_central(&repo)? {
                RepositoryOnboardingStatus::Central
            } else {
                RepositoryOnboardingStatus::Registered
            });
        }
    }
    Ok(if crate::backlog_repo_available(root)? {
        RepositoryOnboardingStatus::Legacy
    } else {
        RepositoryOnboardingStatus::Unconfigured
    })
}

/// Register a fresh workspace through the current centralized storage adapter.
/// Existing legacy data requires the reviewed migration flow.
pub fn setup_repository(root: &Path) -> Result<crate::storage::RepositoryId> {
    let root = root.canonicalize()?;
    ensure!(root.is_dir(), "repository root must be a directory");
    crate::storage::Store::open_default()?.setup_repository(&root, None)
}

/// Explicit choices cannot overwrite an existing workspace configuration.
pub fn setup_repository_with_options(
    root: &Path,
    options: &crate::RepositorySetupOptions,
) -> Result<crate::storage::RepositoryId> {
    let options = options.validated()?;
    let root = root.canonicalize()?;
    ensure!(root.is_dir(), "repository root must be a directory");
    crate::storage::Store::open_default()?.setup_repository(&root, Some(&options))
}
