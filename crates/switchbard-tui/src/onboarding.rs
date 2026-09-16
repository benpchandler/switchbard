//! Repository setup consent and readable, optional configuration questions.
mod questions;
use anyhow::{bail, Result};
use std::io::IsTerminal;
use std::path::Path;
use switchbard_core::{
    repository_onboarding_status, RepositoryOnboardingStatus, RepositorySetupOptions,
};

pub fn ensure_workspace(
    root: &Path,
    explicit: bool,
    yes: bool,
    overrides: Option<RepositorySetupOptions>,
) -> Result<bool> {
    let overrides = overrides.map(|options| options.validated()).transpose()?;
    if existing_workspace(root, explicit, overrides.as_ref())? {
        return Ok(true);
    }
    if !yes && !std::io::stdin().is_terminal() {
        bail!("No SBT workspace is configured for {} in the centralized database. Run sbt init --yes from this repository, or use sbt --repo <DIR> init --yes.", root.display());
    }
    let options = if yes {
        Some(overrides.clone().unwrap_or_default())
    } else {
        questions::choose(root, overrides.clone())?
    };
    let Some(options) = options else {
        eprintln!("Setup canceled. Resume with sbt --repo <DIR> init.");
        return Ok(false);
    };
    save_workspace(root, &options)?;
    Ok(true)
}

fn save_workspace(root: &Path, options: &RepositorySetupOptions) -> Result<()> {
    switchbard_core::setup_repository_with_options(root, options)?;
    eprintln!(
        "SBT workspace ready for {} in the centralized database.",
        root.display()
    );
    Ok(())
}

fn existing_workspace(
    root: &Path,
    explicit: bool,
    customized: Option<&RepositorySetupOptions>,
) -> Result<bool> {
    match repository_onboarding_status(root)? {
        RepositoryOnboardingStatus::Central => {
            if let Some(options) = customized {
                switchbard_core::setup_repository_with_options(root, options)?;
            }
            Ok(true)
        }
        RepositoryOnboardingStatus::Legacy | RepositoryOnboardingStatus::Registered
            if !explicit =>
        {
            Ok(true)
        }
        RepositoryOnboardingStatus::Legacy | RepositoryOnboardingStatus::Registered => {
            bail!("existing legacy workspace requires reviewed migration; run sb storage status")
        }
        RepositoryOnboardingStatus::Unconfigured => Ok(false),
    }
}
