//! Independent planning and reviewable migrations over the native core boundary.
use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};
use switchbard_core::{PlanningMigrationPreview, PlanningState};

#[derive(Subcommand)]
pub enum PlanningCmd {
    /// JSON task rows with planning, status, ordered position and checklist coverage.
    List {
        /// Include archived/completed records; default is active tasks.
        #[arg(long)]
        all: bool,
    },
    /// Change planning only; does not check criteria or change execution status.
    Set { id: String, state: PlanningState },
    /// Reorder one Planned task within the repository-wide prioritized list.
    Rank {
        id: String,
        #[command(flatten)]
        place: crate::rank_cmd::PlacementArgs,
    },
    /// Save an exact migration preview locally; stdout contains counts, not task content.
    Preview {
        /// New private JSON file. Never overwrites an existing preview.
        #[arg(long)]
        out: PathBuf,
    },
    /// Apply a reviewed preview atomically within this repository, with backup and receipt.
    Apply {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        backup_dir: PathBuf,
    },
}

pub fn run(root: &Path, cmd: &PlanningCmd) -> Result<()> {
    match cmd {
        PlanningCmd::List { all } => list(root, *all),
        PlanningCmd::Set { id, state } => {
            let id = resolve(root, id)?;
            let changed = switchbard_core::set_task_planning(root, &id, *state)?.changed();
            emit(json!({"id": id, "planning": state.as_str(), "changed": changed}))
        }
        PlanningCmd::Rank { id, place } => {
            let id = resolve(root, id)?;
            let place = place.to_placement(|anchor| resolve(root, anchor))?;
            let changed = switchbard_core::rank_planned_task(root, &id, &place)?.changed();
            emit(json!({"id": id, "changed": changed}))
        }
        PlanningCmd::Preview { out } => preview(root, out),
        PlanningCmd::Apply { plan, backup_dir } => {
            let preview: PlanningMigrationPreview =
                serde_json::from_slice(&std::fs::read(plan)?)
                    .context("invalid planning preview; run planning preview again")?;
            let receipt = switchbard_core::apply_planning_migration(root, &preview, backup_dir)?;
            emit(json!({"tasks_changed": receipt.tasks_changed,
                "already_applied": receipt.already_applied, "state": receipt.state,
                "backup_path": receipt.backup_path, "receipt_path": receipt.receipt_path}))
        }
    }
}

fn resolve(root: &Path, id: &str) -> Result<String> {
    let repo = switchbard_core::load_backlog_repo(root)?;
    repo.tasks
        .iter()
        .find(|task| {
            task.id.eq_ignore_ascii_case(id)
                || task
                    .id
                    .rsplit_once('-')
                    .is_some_and(|(_, bare)| bare.eq_ignore_ascii_case(id))
        })
        .map(|task| task.id.clone())
        .with_context(|| format!("no task {id}; use sb list --all"))
}

fn list(root: &Path, all: bool) -> Result<()> {
    let repo = switchbard_core::load_backlog_repo(root)?;
    let progress = switchbard_core::checklist_progress(&repo);
    let order = switchbard_core::planning_order(&repo);
    let rows: Vec<Value> = repo
        .tasks
        .iter()
        .filter(|task| all || task.source == switchbard_core::BacklogTaskSource::Active)
        .map(|task| {
            let coverage = progress.get(&task.id);
            json!({"id": task.id, "title": task.title, "planning": task.planning.as_str(),
                "status": task.status, "source": task.source.label(),
                "position": order.iter().position(|id| id == &task.id).map(|i| i + 1),
                "checked": coverage.map(|p| p.checked), "total": coverage.map(|p| p.total),
                "percentage": coverage.and_then(|p| p.percentage()),
                "needs_review": coverage.is_some_and(|p| p.needs_review(task))})
        })
        .collect();
    emit(json!({"tasks": rows, "planned_order": order}))
}

fn preview(root: &Path, out: &Path) -> Result<()> {
    let preview = switchbard_core::prepare_planning_migration(root)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(out)
        .context("cannot create preview; choose a new path")?;
    file.write_all(&serde_json::to_vec_pretty(&preview)?)?;
    file.sync_all()?;
    emit(
        json!({"preview": out, "repository_id": preview.repository_id,
        "tasks_changed": preview.task_changes.len(), "planned_tasks": preview.planned_order.len(),
        "unknown_statuses": preview.unknown_statuses, "already_migrated": preview.already_migrated}),
    )
}

fn emit(value: Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
