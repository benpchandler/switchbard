//! First-launch confirmation runs before alternate-screen or raw terminal mode.
use anyhow::{bail, Result};
use std::io::{BufRead, IsTerminal, Read, Write};
use std::path::Path;
use switchbard_core::{repository_onboarding_status, RepositoryOnboardingStatus};

pub fn ensure_workspace(root: &Path, explicit: bool, yes: bool) -> Result<bool> {
    match repository_onboarding_status(root)? {
        RepositoryOnboardingStatus::Central => return Ok(true),
        RepositoryOnboardingStatus::Legacy | RepositoryOnboardingStatus::Registered
            if !explicit =>
        {
            return Ok(true)
        }
        RepositoryOnboardingStatus::Legacy | RepositoryOnboardingStatus::Registered => {
            bail!("existing legacy workspace requires reviewed migration; run sb storage status")
        }
        RepositoryOnboardingStatus::Unconfigured => {}
    }
    if !yes && !std::io::stdin().is_terminal() {
        bail!("No SBT workspace is configured for {} in the centralized database. Run sbt init --yes from this repository, or use sbt --repo <DIR> init --yes.", root.display());
    }
    if !yes && !ask_setup(root)? {
        return Ok(false);
    }
    switchbard_core::setup_repository(root)?;
    eprintln!(
        "SBT workspace ready for {} in the centralized database.",
        root.display()
    );
    Ok(true)
}

fn ask_setup(root: &Path) -> Result<bool> {
    let mut output = std::io::stderr().lock();
    writeln!(
        output,
        "No SBT workspace is configured for {}.",
        root.display()
    )?;
    writeln!(
        output,
        "Setup registers this repository in Switchbard's centralized database."
    )?;
    writeln!(
        output,
        "Defaults: TASK task IDs; To Do, In Progress, Done statuses."
    )?;
    write!(output, "Set it up now? [y/N] ")?;
    output.flush()?;
    let accepted = confirmed(&mut std::io::stdin().lock())?;
    if !accepted {
        writeln!(output, "Setup canceled. Resume from this repository with sbt init, or use sbt --repo <DIR> init.")?;
    }
    Ok(accepted)
}

fn confirmed(input: &mut impl BufRead) -> Result<bool> {
    let mut response = String::new();
    input.take(256).read_line(&mut response)?;
    Ok(
        response.len() < 256
            && matches!(response.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
    )
}
