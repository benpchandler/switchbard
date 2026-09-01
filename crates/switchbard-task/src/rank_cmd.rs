//! The `rank`, `unrank`, `expedite`, and `unexpedite` verbs - the CLI
//! surface of stack ranking (trajectory: *Stack ranking*), over
//! `switchbard_core::backlog::ranking`'s write layer.
//!
//! Output contract, same discipline as every other edit-shaped verb:
//! stdout prints `Edited <ID>` (or `Edited <NAME>`) when the ranking file
//! changed, `no changes` when it didn't; errors are one line on stderr
//! naming the next step. Placement is exactly one of `--top`,
//! `--before <sibling>`, `--after <sibling>` - anchors must already be
//! ranked in the same sibling scope.

use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::Path;
use switchbard_core::{BacklogRepo, RankPlacement, WriteOutcome};

#[derive(Subcommand)]
pub enum RankCmd {
    /// Rank a project against the repo's other projects
    Project {
        /// Project name (exact, as `project list` prints it)
        name: String,
        #[command(flatten)]
        place: PlacementArgs,
    },
    /// Rank a task among its siblings - its project's tasks, the repo-root
    /// group, or its parent's sub-issues
    Task {
        /// Task id (TASK-7, task-7, 7, or 7.2)
        id: String,
        #[command(flatten)]
        place: PlacementArgs,
    },
}

#[derive(Subcommand)]
pub enum UnrankCmd {
    /// Remove a project from the ranked project list
    Project { name: String },
    /// Remove a task from its scope's ranked list (works even when the
    /// task itself is gone - that is how a stray entry is cleared)
    Task { id: String },
}

/// Exactly one placement per invocation.
#[derive(Args)]
#[group(required = true, multiple = false)]
pub struct PlacementArgs {
    /// Place at the top of the ranked list
    #[arg(long)]
    top: bool,
    /// Place immediately above this already-ranked sibling
    #[arg(long, value_name = "SIBLING")]
    before: Option<String>,
    /// Place immediately below this already-ranked sibling
    #[arg(long, value_name = "SIBLING")]
    after: Option<String>,
}

impl PlacementArgs {
    /// Build the core placement, canonicalizing an anchor through
    /// `resolve` (identity for project names, id resolution for tasks).
    fn to_placement(&self, resolve: impl Fn(&str) -> Result<String>) -> Result<RankPlacement> {
        if let Some(anchor) = &self.before {
            return Ok(RankPlacement::Before(resolve(anchor)?));
        }
        if let Some(anchor) = &self.after {
            return Ok(RankPlacement::After(resolve(anchor)?));
        }
        Ok(RankPlacement::Top)
    }
}

pub fn run_rank(root: &Path, cmd: &RankCmd) -> Result<()> {
    match cmd {
        RankCmd::Project { name, place } => {
            let placement = place.to_placement(|anchor| Ok(anchor.to_string()))?;
            print_outcome(switchbard_core::rank_project(root, name, &placement)?, name);
        }
        RankCmd::Task { id, place } => {
            let repo = switchbard_core::load_backlog_repo(root)?;
            let full = resolve_task_id(&repo, id)?;
            let placement = place.to_placement(|anchor| resolve_task_id(&repo, anchor))?;
            print_outcome(switchbard_core::rank_task(root, &full, &placement)?, &full);
        }
    }
    Ok(())
}

pub fn run_unrank(root: &Path, cmd: &UnrankCmd) -> Result<()> {
    match cmd {
        UnrankCmd::Project { name } => {
            print_outcome(switchbard_core::unrank_project(root, name)?, name);
        }
        UnrankCmd::Task { id } => {
            let full = resolve_task_id_loose(root, id);
            print_outcome(switchbard_core::unrank_task(root, &full)?, &full);
        }
    }
    Ok(())
}

pub fn run_expedite(root: &Path, id: &str) -> Result<()> {
    let repo = switchbard_core::load_backlog_repo(root)?;
    let full = resolve_task_id(&repo, id)?;
    print_outcome(switchbard_core::expedite_task(root, &full)?, &full);
    Ok(())
}

pub fn run_unexpedite(root: &Path, id: &str) -> Result<()> {
    let full = resolve_task_id_loose(root, id);
    print_outcome(switchbard_core::unexpedite_task(root, &full)?, &full);
    Ok(())
}

/// Rank the just-created task per the `create --rank-*` flags. The task
/// already exists when this runs, so a failure names it rather than
/// pretending the create didn't happen.
pub fn rank_new_task(root: &Path, id: &str, place: &PlacementArgs) -> Result<()> {
    let repo = switchbard_core::load_backlog_repo(root)?;
    let placement = place.to_placement(|anchor| resolve_task_id(&repo, anchor))?;
    switchbard_core::rank_task(root, id, &placement)
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!("{id} was created, but ranking it failed: {err}"))
}

/// Optional create-time placement - same three flags, none required.
#[derive(Args)]
#[group(multiple = false)]
pub struct CreatePlacementArgs {
    /// Rank the new task at the top of its sibling scope
    #[arg(long)]
    rank_top: bool,
    /// Rank the new task immediately above this already-ranked sibling
    #[arg(long, value_name = "SIBLING")]
    rank_before: Option<String>,
    /// Rank the new task immediately below this already-ranked sibling
    #[arg(long, value_name = "SIBLING")]
    rank_after: Option<String>,
}

impl CreatePlacementArgs {
    pub fn to_placement_args(&self) -> Option<PlacementArgs> {
        if !self.rank_top && self.rank_before.is_none() && self.rank_after.is_none() {
            return None;
        }
        Some(PlacementArgs {
            top: self.rank_top,
            before: self.rank_before.clone(),
            after: self.rank_after.clone(),
        })
    }
}

/// A query id (`TASK-7`, `task-7`, `7`, `7.2`) canonicalized to the stored
/// id, through the same matcher `view`/`edit` use.
fn resolve_task_id(repo: &BacklogRepo, id: &str) -> Result<String> {
    crate::find_task(&repo.tasks, id)
        .map(|task| task.id.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no task {id} in {} — try `sb list --all`",
                repo.root.display()
            )
        })
}

/// Like [`resolve_task_id`], but a query naming no live task passes
/// through verbatim - `unrank`/`unexpedite` legitimately target ids whose
/// tasks are gone.
fn resolve_task_id_loose(root: &Path, id: &str) -> String {
    switchbard_core::load_backlog_repo(root)
        .ok()
        .and_then(|repo| crate::find_task(&repo.tasks, id).map(|task| task.id.clone()))
        .unwrap_or_else(|| id.trim().to_string())
}

fn print_outcome(outcome: WriteOutcome, subject: &str) {
    if outcome.changed() {
        println!("Edited {subject}");
    } else {
        println!("no changes");
    }
}
