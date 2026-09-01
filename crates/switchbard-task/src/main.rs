//! `switchbard-task` — the terminal and agent write path for Backlog-format
//! tasks, over `switchbard_core::backlog`'s native write layer.
//!
//! One of the format fork's frontends (trajectory: *Backlog format fork*):
//! the GUI, `switchbard-dispatch`, and this binary all write through the
//! same one implementation in `switchbard-core`, which is the fork's core
//! invariant. This is what keeps dispatch's "flaggable from a plain
//! terminal with no Switchbard running" property alive after the external
//! `backlog` CLI is retired, and what agents working a task call to check
//! criteria, append notes, and move statuses.
//!
//! Designed against `~/.claude/standards/agent-facing-design.md`:
//! - **stdout carries only the payload** (ids, task renders, list rows,
//!   result lines); anything conversational goes to stderr.
//! - **Every error is one readable line on stderr with the next step in
//!   it**, and a nonzero exit — no raw error chains, no silent failures.
//! - **The help text is the output contract**, per subcommand, because it
//!   is the only documentation an agent reliably reads.
//! - Nothing here blocks or waits, so the banner/heartbeat rules don't
//!   apply; every command does its work and exits.

mod goals_cmd;
mod hierarchy_cmd;
mod rank_cmd;
mod render;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};
use switchbard_core::{BacklogTask, BacklogTaskPatch, NewBacklogTask};

/// How many directory levels [`find_repo_root`] will climb before giving
/// up — bounded so a weird mount layout can't turn root discovery into an
/// unbounded walk.
const MAX_ROOT_WALK: usize = 64;

#[derive(Parser)]
#[command(
    name = "switchbard-task",
    version,
    about = "Read and write Backlog-format tasks (switchbard's native task layer)",
    long_about = "Read and write Backlog-format tasks through switchbard's native write \
                  layer — the same implementation the Switchbard GUI and switchbard-dispatch \
                  use, and the replacement for the external `backlog` CLI's write path.\n\n\
                  REPO RESOLUTION: commands act on the Backlog repo containing the \
                  current directory (the nearest ancestor with a backlog/ directory), or the \
                  one named by --repo (--project <DIR> is a deprecated alias).\n\n\
                  TASK IDS: every <ID> accepts `TASK-7`, `task-7`, or bare `7`, plus decimal \
                  subtask ids like `7.2`.\n\n\
                  OUTPUT CONTRACT: stdout carries only the payload — `create` prints the new \
                  task id alone; edit-shaped commands print `Edited <ID>` or `no changes`; \
                  `view` prints the task; `list` prints one tab-separated row per task \
                  (id, status, priority, labels, project, title). Errors are one line on \
                  stderr and exit code 1.\n\n\
                  HIERARCHY: tasks belong to a named project (`--in-project`, stored as \
                  `project:` frontmatter; legacy `milestone:` is read as a fallback and \
                  rewritten on assignment), and projects belong to a named initiative. The \
                  `project` and `initiative` subcommand families manage the optional \
                  definition files (backlog/projects/, backlog/initiatives/) that give a \
                  name lifecycle, and their `list`/`view` roll up member done/total counts.\n\n\
                  GOALS: weekly numeric goals live in backlog/goals.yml (records, not \
                  markdown) - `goal create/list/view/check-in/roll`. Pace compares \
                  actual/target against the elapsed week: on-track, behind, met, missed.\n\n\
                  RANKING: manual stack rank lives in backlog/ranking.yml (records, not \
                  markdown). `rank project/task <X> --top|--before|--after <sibling>` ranks \
                  within the sibling scope; `unrank` removes; `expedite <ID>` jumps a task \
                  over the whole computed order (true interrupts only - a new task that \
                  merely belongs high in its project takes `create --rank-top` instead). \
                  `list` and `project list` print the computed order: expedited first, then \
                  ranked, then everything else by status/priority/id.\n\n\
                  DISPATCH: flag a task for an autonomous run with \
                  `switchbard-task edit <ID> --add-label dispatch`."
)]
struct Cli {
    /// Repo root to act on (default: nearest ancestor of the current
    /// directory containing a backlog/ directory)
    #[arg(long, global = true, value_name = "DIR")]
    repo: Option<PathBuf>,

    /// Deprecated alias for --repo
    #[arg(
        long,
        global = true,
        value_name = "DIR",
        hide = true,
        conflicts_with = "repo"
    )]
    project: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List tasks, one tab-separated row per task: id, status, priority,
    /// labels (comma-joined), project, title
    List {
        /// Only rows whose status matches (case-insensitive)
        #[arg(long, value_name = "STATUS")]
        status: Option<String>,
        /// Only rows assigned to this project (exact name match)
        #[arg(long = "in-project", value_name = "NAME")]
        in_project: Option<String>,
        /// Include completed, draft, and archived tasks (default: active only)
        #[arg(long)]
        all: bool,
    },
    /// Print one task in full: fields, then every section verbatim
    View {
        /// Task id (TASK-7, task-7, 7, or 7.2)
        id: String,
    },
    /// Create a task; prints the new task's id (e.g. `TASK-42`) on stdout
    Create(Box<CreateArgs>),
    /// Edit one task; prints `Edited <ID>` or `no changes`
    Edit(Box<EditArgs>),
    /// Move a non-Done task to backlog/archive/tasks (abandonment).
    /// Done tasks must be completed instead
    Archive { id: String },
    /// Move a Done task to backlog/completed. Refuses non-Done tasks
    Complete { id: String },
    /// Manage project definitions (the completable tier above tasks)
    #[command(subcommand)]
    Project(hierarchy_cmd::ProjectCmd),
    /// Manage initiative definitions (the grouping tier above projects)
    #[command(subcommand)]
    Initiative(hierarchy_cmd::InitiativeCmd),
    /// Weekly numeric goals tracked relative to target (backlog/goals.yml)
    #[command(subcommand)]
    Goal(goals_cmd::GoalCmd),
    /// Stack-rank a project or task within its sibling scope
    /// (backlog/ranking.yml); prints `Edited <X>` or `no changes`
    #[command(subcommand)]
    Rank(rank_cmd::RankCmd),
    /// Remove a project or task from its ranked list
    #[command(subcommand)]
    Unrank(rank_cmd::UnrankCmd),
    /// Add a task to the expedite lane - it jumps the entire computed
    /// order (cross-project interrupts only)
    Expedite { id: String },
    /// Remove a task from the expedite lane
    Unexpedite { id: String },
}

#[derive(Args)]
struct CreateArgs {
    /// Task title
    title: String,
    /// Description (Markdown)
    #[arg(short = 'd', long)]
    description: Option<String>,
    /// Initial status (default: To Do; validated against backlog/config.yml)
    #[arg(short = 's', long)]
    status: Option<String>,
    /// Priority: high, medium, or low (default: medium)
    #[arg(long)]
    priority: Option<String>,
    /// Acceptance criterion (repeatable, appended in order)
    #[arg(long = "ac", value_name = "TEXT")]
    acceptance_criteria: Vec<String>,
    /// Parent task id — the new task gets the next decimal child id
    /// (parent TASK-7 → TASK-7.1)
    #[arg(short = 'p', long)]
    parent: Option<String>,
    /// Labels, comma-separated
    #[arg(short = 'l', long, value_delimiter = ',')]
    labels: Vec<String>,
    /// Assignees, comma-separated
    #[arg(short = 'a', long, value_delimiter = ',')]
    assignees: Vec<String>,
    /// Assign the task to a project (stored as `project:` frontmatter;
    /// `--milestone` is a deprecated alias)
    #[arg(
        short = 'm',
        long = "in-project",
        alias = "milestone",
        value_name = "NAME"
    )]
    in_project: Option<String>,
    /// Dependency task ids, comma-separated
    #[arg(long, value_delimiter = ',')]
    depends_on: Vec<String>,
    #[command(flatten)]
    rank: rank_cmd::CreatePlacementArgs,
}

#[derive(Args)]
struct EditArgs {
    /// Task id (TASK-7, task-7, 7, or 7.2)
    id: String,
    /// Replace the title
    #[arg(short = 't', long)]
    title: Option<String>,
    /// Replace the Description section
    #[arg(short = 'd', long)]
    description: Option<String>,
    /// Set the status (validated against backlog/config.yml)
    #[arg(short = 's', long)]
    status: Option<String>,
    /// Set the priority
    #[arg(long)]
    priority: Option<String>,
    /// Replace the whole label list, comma-separated (see --add-label /
    /// --remove-label for single-label changes that can't race)
    #[arg(short = 'l', long, value_delimiter = ',')]
    labels: Option<Vec<String>>,
    /// Replace the assignee list, comma-separated
    #[arg(short = 'a', long, value_delimiter = ',')]
    assignees: Option<Vec<String>>,
    /// Replace the dependency list, comma-separated
    #[arg(long, value_delimiter = ',')]
    depends_on: Option<Vec<String>>,
    /// Replace the whole references list (repeatable; the set of --ref
    /// flags becomes the new list)
    #[arg(long = "ref", value_name = "URL")]
    references: Option<Vec<String>>,
    /// Replace the Implementation Plan section
    #[arg(long)]
    plan: Option<String>,
    /// Append an acceptance criterion (repeatable; never disturbs existing
    /// criteria or their checked state)
    #[arg(long = "ac", value_name = "TEXT")]
    acceptance_criteria: Vec<String>,
    /// Assign the task to a project (rewrites a legacy `milestone:` key as
    /// `project:`; `--milestone` is a deprecated alias)
    #[arg(
        short = 'm',
        long = "in-project",
        alias = "milestone",
        value_name = "NAME",
        conflicts_with = "clear_project"
    )]
    in_project: Option<String>,
    /// Remove the project assignment (removes legacy `milestone:` too;
    /// `--clear-milestone` is a deprecated alias)
    #[arg(long, alias = "clear-milestone")]
    clear_project: bool,
    /// Add one label, leaving the rest untouched
    #[arg(long, value_name = "LABEL")]
    add_label: Vec<String>,
    /// Remove one label, leaving the rest untouched
    #[arg(long, value_name = "LABEL")]
    remove_label: Vec<String>,
    /// Check acceptance criterion #N (repeatable)
    #[arg(long, value_name = "N")]
    check_ac: Vec<usize>,
    /// Uncheck acceptance criterion #N (repeatable)
    #[arg(long, value_name = "N")]
    uncheck_ac: Vec<usize>,
    /// Check Definition of Done item #N (repeatable)
    #[arg(long, value_name = "N")]
    check_dod: Vec<usize>,
    /// Uncheck Definition of Done item #N (repeatable)
    #[arg(long, value_name = "N")]
    uncheck_dod: Vec<usize>,
    /// Append a note to Implementation Notes (existing notes are never
    /// rewritten)
    #[arg(long, value_name = "TEXT")]
    append_notes: Option<String>,
    /// Replace the Final Summary section — the wrap-up written once, when
    /// the task is finished
    #[arg(long, value_name = "TEXT")]
    final_summary: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(&cli) {
        eprintln!("switchbard-task: error: {err}");
        std::process::exit(1);
    }
}

fn run(cli: &Cli) -> Result<()> {
    if cli.project.is_some() {
        eprintln!("switchbard-task: warning: --project is deprecated; use --repo");
    }
    let root = resolve_repo(cli.repo.as_deref().or(cli.project.as_deref()))?;
    match &cli.command {
        Command::List {
            status,
            in_project,
            all,
        } => list(&root, status.as_deref(), in_project.as_deref(), *all),
        Command::View { id } => view(&root, id),
        Command::Create(args) => create(&root, args),
        Command::Edit(args) => edit(&root, args),
        Command::Archive { id } => {
            println!("{}", switchbard_core::archive_backlog_task(&root, id)?);
            Ok(())
        }
        Command::Complete { id } => {
            println!("{}", switchbard_core::complete_backlog_task(&root, id)?);
            Ok(())
        }
        Command::Project(cmd) => hierarchy_cmd::run_project(&root, cmd),
        Command::Initiative(cmd) => hierarchy_cmd::run_initiative(&root, cmd),
        Command::Goal(cmd) => goals_cmd::run_goal(&root, cmd),
        Command::Rank(cmd) => rank_cmd::run_rank(&root, cmd),
        Command::Unrank(cmd) => rank_cmd::run_unrank(&root, cmd),
        Command::Expedite { id } => rank_cmd::run_expedite(&root, id),
        Command::Unexpedite { id } => rank_cmd::run_unexpedite(&root, id),
    }
}

/// The repo root: `--repo` (or its deprecated `--project` alias) verbatim
/// (validated), else the nearest ancestor of the current directory that is
/// a Backlog repo.
fn resolve_repo(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = explicit {
        if switchbard_core::is_backlog_repo(root) {
            return Ok(root.to_path_buf());
        }
        bail!(
            "{} is not a Backlog repo (no backlog/ directory there)",
            root.display()
        );
    }
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    find_repo_root(&cwd).ok_or_else(|| {
        anyhow!(
            "no Backlog repo found at or above {} — run inside one, or pass --repo <repo-root>",
            cwd.display()
        )
    })
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .take(MAX_ROOT_WALK)
        .find(|dir| switchbard_core::is_backlog_repo(dir))
        .map(Path::to_path_buf)
}

fn list(root: &Path, status: Option<&str>, in_project: Option<&str>, all: bool) -> Result<()> {
    let repo = switchbard_core::load_backlog_repo(root)?;
    for warning in &repo.warnings {
        eprintln!("switchbard-task: warning: {warning}");
    }
    for task in &repo.tasks {
        if !all && task.source != switchbard_core::BacklogTaskSource::Active {
            continue;
        }
        if let Some(wanted) = status {
            if !task.status.eq_ignore_ascii_case(wanted) {
                continue;
            }
        }
        if let Some(wanted) = in_project {
            if task.project.as_deref() != Some(wanted) {
                continue;
            }
        }
        println!("{}", render::list_row(task));
    }
    Ok(())
}

fn view(root: &Path, id: &str) -> Result<()> {
    let project = switchbard_core::load_backlog_repo(root)?;
    let task = find_task(&project.tasks, id).ok_or_else(|| {
        anyhow!(
            "no task {id} in {} — try `switchbard-task list --all`",
            root.display()
        )
    })?;
    print!("{}", render::task_view(task));
    Ok(())
}

/// Case-insensitive id match, `TASK-` prefix optional on the query.
pub(crate) fn find_task<'t>(tasks: &'t [BacklogTask], id: &str) -> Option<&'t BacklogTask> {
    let bare = bare_id(id);
    tasks
        .iter()
        .find(|task| bare_id(&task.id).eq_ignore_ascii_case(bare))
}

fn bare_id(id: &str) -> &str {
    let trimmed = id.trim();
    trimmed
        .get(..5)
        .filter(|p| p.eq_ignore_ascii_case("task-"))
        .map_or(trimmed, |_| &trimmed[5..])
}

fn create(root: &Path, args: &CreateArgs) -> Result<()> {
    let task = NewBacklogTask {
        title: args.title.clone(),
        description: args.description.clone().unwrap_or_default(),
        status: args.status.clone().unwrap_or_default(),
        priority: args.priority.clone().unwrap_or_default(),
        acceptance_criteria: args.acceptance_criteria.clone(),
        parent: args.parent.clone(),
        labels: args.labels.clone(),
        assignees: args.assignees.clone(),
        project: args.in_project.clone(),
        dependencies: args.depends_on.clone(),
    };
    let id = switchbard_core::create_backlog_task(root, &task)?;
    println!("{id}");
    if let Some(place) = args.rank.to_placement_args() {
        rank_cmd::rank_new_task(root, &id, &place)?;
    }
    Ok(())
}

/// Apply the edit in a fixed order: the patch-shaped fields in one
/// `edit_backlog_task`, then label add/removes, checklist toggles, the note
/// append, and the final summary — each through the same facade the GUI and
/// dispatch use. Prints `Edited <ID>` if anything changed, else
/// `no changes`.
fn edit(root: &Path, args: &EditArgs) -> Result<()> {
    let patch = patch_from(args);
    let mut changed = false;
    if !patch.is_empty() {
        changed |= switchbard_core::edit_backlog_task(root, &args.id, &patch)? != "no changes";
    }
    for label in &args.add_label {
        changed |= switchbard_core::set_backlog_label(root, &args.id, label, true)? != "no changes";
    }
    for label in &args.remove_label {
        changed |=
            switchbard_core::set_backlog_label(root, &args.id, label, false)? != "no changes";
    }
    changed |= toggle_checklists(root, args)?;
    if let Some(note) = &args.append_notes {
        changed |= switchbard_core::append_backlog_notes(root, &args.id, note)? != "no changes";
    }
    if let Some(summary) = &args.final_summary {
        changed |=
            switchbard_core::set_backlog_final_summary(root, &args.id, summary)? != "no changes";
    }
    if changed {
        println!("Edited {}", args.id);
    } else {
        println!("no changes");
    }
    Ok(())
}

fn toggle_checklists(root: &Path, args: &EditArgs) -> Result<bool> {
    let mut changed = false;
    for (indices, checked) in [(&args.check_ac, true), (&args.uncheck_ac, false)] {
        for &index in indices {
            changed |=
                switchbard_core::set_backlog_acceptance_checked(root, &args.id, index, checked)?
                    != "no changes";
        }
    }
    for (indices, checked) in [(&args.check_dod, true), (&args.uncheck_dod, false)] {
        for &index in indices {
            changed |= switchbard_core::set_backlog_dod_checked(root, &args.id, index, checked)?
                != "no changes";
        }
    }
    Ok(changed)
}

fn patch_from(args: &EditArgs) -> BacklogTaskPatch {
    BacklogTaskPatch {
        title: args.title.clone(),
        description: args.description.clone(),
        status: args.status.clone(),
        priority: args.priority.clone(),
        labels: args.labels.clone(),
        assignees: args.assignees.clone(),
        dependencies: args.depends_on.clone(),
        references: args.references.clone(),
        implementation_plan: args.plan.clone(),
        append_acceptance_criteria: args.acceptance_criteria.clone(),
        project: args.in_project.clone(),
        clear_project: args.clear_project,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// clap's own debug assertions catch conflicting arg ids/aliases (e.g.
    /// the global deprecated `--project <DIR>` vs the task `--in-project`
    /// family) at test time instead of at first parse in production.
    #[test]
    fn clap_definition_is_internally_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn bare_id_strips_the_prefix_case_insensitively() {
        assert_eq!(bare_id("TASK-7"), "7");
        assert_eq!(bare_id("task-7.2"), "7.2");
        assert_eq!(bare_id(" 7 "), "7");
        assert_eq!(bare_id("tas"), "tas");
    }

    #[test]
    fn find_repo_root_climbs_to_the_nearest_backlog_dir_and_is_bounded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("repo");
        let deep = root.join("crates/something/src");
        std::fs::create_dir_all(root.join("backlog/tasks")).expect("fixture");
        std::fs::create_dir_all(&deep).expect("fixture");

        assert_eq!(find_repo_root(&deep), Some(root.clone()));
        assert_eq!(
            find_repo_root(dir.path()),
            None,
            "a dir with no backlog/ above it resolves to nothing"
        );
    }

    #[test]
    fn cli_definition_is_internally_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
