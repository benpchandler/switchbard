//! `sb` — the terminal and agent write path for Backlog-format
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

mod field_cmd;
mod goals_cmd;
mod hierarchy_cmd;
mod queue_cmd;
mod rank_cmd;
mod render;
mod storage_cmd;
mod storage_document;
mod storage_exchange;
mod storage_exchange_file;
mod storage_ordering;
mod storage_recovery;
mod work_cmd;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};
use switchbard_core::{BacklogTask, BacklogTaskPatch, ChecklistTextEdit, NewBacklogTask};

/// How many directory levels [`find_repo_root`] will climb before giving
/// up — bounded so a weird mount layout can't turn root discovery into an
/// unbounded walk.
const MAX_ROOT_WALK: usize = 64;

#[derive(Parser)]
#[command(
    name = "sb",
    version = switchbard_core::VERSION_LINE,
    about = "Read and write Backlog-format tasks (switchbard's native task layer)",
    long_about = "Read and write Backlog-format tasks through switchbard's native write \
                  layer — the same implementation the Switchbard GUI and switchbard-dispatch \
                  use, and the replacement for the external `backlog` CLI's write path.\n\n\
                  REPO RESOLUTION: commands act on the Backlog repo containing the \
                  current directory (a registered repository or nearest backlog/ directory), or the \
                  one named by --repo (--project <DIR> is a deprecated alias).\n\n\
                  STORAGE: after explicit per-kind migration, the central Switchbard database \
                  owns reads and writes across worktrees. Legacy source files remain preserved. \
                  Use `storage status` and `storage migrate --help`; ordinary edits never export. \
                  `storage export` writes the optional .switchbard/tasks.json collaboration file.\n\n\
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
                  GOALS: weekly numeric goals use central storage after migration, otherwise backlog/goals.yml (records, not \
                  markdown) - `goal create/list/view/check-in/roll`. Pace compares \
                  actual/target against the elapsed week: on-track, behind, met, missed.\n\n\
                  RANKING: manual stack rank uses central storage after migration, otherwise backlog/ranking.yml (records, not \
                  markdown). `rank project/task <X> --top|--before|--after <sibling>` ranks \
                  within the sibling scope; `unrank` removes; `expedite <ID>` jumps a task \
                  over the whole computed order (true interrupts only - a new task that \
                  merely belongs high in its project takes `create --rank-top` instead). \
                  `list` and `project list` print the computed order: expedited first, then \
                  ranked, then everything else by status/priority/id.\n\n\
                  DISPATCH: flag a task for an autonomous run with \
                  `sb edit <ID> --add-label dispatch`.\n\n\
                  QUEUE: the `queue` family is the orchestrator's protocol surface - \
                  `queue list` (dispatch queue in stack-rank order), `queue send/withdraw <ID>`, \
                  `queue claim <ID>` (acknowledge: dispatch -> dispatching, prints the prior \
                  status - keep it for `release`), `queue release <ID> --outcome \
                  dispatched|failed`, `queue prompt <ID>` (the exact headless-agent prompt). \
                  Claim before work; releases walk the same label ladder the built-in \
                  dispatch pipeline uses.\n\n\
                  WORK: the `work` family is an interactive session's live-work protocol - \
                  `work claim <ID>` before editing (the row lights up in sbt, status In \
                  Progress, ball to the agent), `work release <ID>` when every acceptance \
                  criterion is checked (or `--note <why>` to hand it back unfinished), \
                  `work pass <ID>` is the owner's word, `work list` shows live sessions, \
                  and `work hook` is the Claude Code hook entry point that enforces \
                  claim-before-edit and no-stop-while-claimed. Identity comes from \
                  CLAUDE_CODE_SESSION_ID / CLAUDE_PID; the claim dies with the process."
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
    /// Inspect or explicitly migrate central planning storage.
    Storage(storage_cmd::StorageArgs),
    /// Print the git identity of this build as `key=value` lines. The
    /// install guard reads this to refuse a downgrade; see
    /// `switchbard_core::build_identity`.
    BuildId,
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
        /// Only tasks with an open (not-done) dependency and not themselves
        /// done (switchbard_core::is_blocked); mutually exclusive with --ready
        #[arg(long, conflicts_with = "ready")]
        blocked: bool,
        /// Only tasks with no open dependency and not themselves done —
        /// actionable now; mutually exclusive with --blocked
        #[arg(long, conflicts_with = "blocked")]
        ready: bool,
        /// Only rows where a declared custom field equals a value
        /// (repeatable; every `--where` must match — AND, not OR)
        #[arg(long = "where", value_name = "NAME=VALUE")]
        where_: Vec<String>,
        /// Sort by a declared custom field: enum fields sort in declared
        /// value order, other kinds sort lexically; tasks not setting the
        /// field sort last
        #[arg(long, value_name = "NAME")]
        sort: Option<String>,
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
    /// The dispatch queue protocol (list/send/withdraw/claim/release/prompt)
    #[command(subcommand)]
    Queue(queue_cmd::QueueCmd),
    /// Live work: which tasks this session is working (claim/release/pass/list/hook)
    #[command(subcommand)]
    Work(work_cmd::WorkCmd),
    /// Manage this repo's declared custom task fields (backlog/config.yml's
    /// `fields:` list) — add/edit/remove/list
    #[command(subcommand)]
    Field(field_cmd::FieldCmd),
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
    /// Target date, YYYY-MM-DD
    #[arg(long, value_name = "YYYY-MM-DD")]
    due: Option<String>,
    /// Set a declared custom field's value at creation time (repeatable;
    /// validated against backlog/config.yml's `fields:` list)
    #[arg(long = "set", value_name = "NAME=VALUE")]
    set: Vec<String>,
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
    /// criteria or their checked state; applied after --edit-ac and
    /// --remove-ac)
    #[arg(long = "ac", value_name = "TEXT")]
    acceptance_criteria: Vec<String>,
    /// Replace the text of acceptance criterion #N, keeping its [x]/[ ]
    /// state and number (repeatable; N is the number `sb view` shows before
    /// this command runs)
    #[arg(long, num_args = 2, value_names = ["N", "TEXT"])]
    edit_ac: Vec<String>,
    /// Remove acceptance criterion #N and renumber the rest #1..#n with no
    /// gap (repeatable; every N is the pre-command number, so
    /// `--remove-ac 2 --remove-ac 4` drops the original #2 and #4; applied
    /// after --edit-ac, before --ac; cannot be combined with --check-ac /
    /// --uncheck-ac - run those as a separate command)
    #[arg(long, value_name = "N", conflicts_with_all = ["check_ac", "uncheck_ac"])]
    remove_ac: Vec<usize>,
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
    /// Set the due date, YYYY-MM-DD
    #[arg(long, value_name = "YYYY-MM-DD", conflicts_with = "clear_due")]
    due: Option<String>,
    /// Remove the due date
    #[arg(long)]
    clear_due: bool,
    /// Move the task under another parent (its id is re-minted as that
    /// parent's next sub-issue, e.g. TASK-8.3) or `none` to promote it to a
    /// top-level task; dependencies, rank, and goal inputs follow the new
    /// id. Applied after every other edit; prints `Moved <OLD> -> <NEW>`
    #[arg(long, value_name = "ID|none")]
    parent: Option<String>,
    /// Pass the ball - who acts next: `me`, `agent`, a named person, or `none`
    /// to drop it (stored as a ball:<holder> label; sbt's `b` key cycles the
    /// me / agent / none shortcuts)
    #[arg(long, value_name = "me|agent|PERSON|none")]
    ball: Option<String>,
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
    /// Set a declared custom field's value (repeatable; validated against
    /// backlog/config.yml's `fields:` list — see `sb field list`)
    #[arg(long = "set", value_name = "NAME=VALUE")]
    set: Vec<String>,
    /// Remove a declared custom field's value (repeatable)
    #[arg(long = "unset", value_name = "NAME")]
    unset: Vec<String>,
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(&cli) {
        eprintln!("sb: error: {err}");
        std::process::exit(1);
    }
}

fn run(cli: &Cli) -> Result<()> {
    if cli.project.is_some() {
        eprintln!("sb: warning: --project is deprecated; use --repo");
    }
    // Answerable anywhere: the install guard asks an installed binary what
    // it is, from whatever directory it happens to be run in, and a missing
    // backlog/ directory must not turn that into an error.
    if matches!(cli.command, Command::BuildId) {
        print!("{}", switchbard_core::build_id_report());
        return Ok(());
    }
    if let Command::Storage(args) = &cli.command {
        let root = cli
            .repo
            .clone()
            .or_else(|| cli.project.clone())
            .unwrap_or(std::env::current_dir()?);
        return storage_cmd::run(&root, args);
    }
    let root = resolve_repo(cli.repo.as_deref().or(cli.project.as_deref()))?;
    match &cli.command {
        Command::BuildId => unreachable!("handled before repo resolution"),
        Command::Storage(_) => {
            unreachable!("storage commands handled before legacy scope resolution")
        }
        Command::List {
            status,
            in_project,
            all,
            blocked,
            ready,
            where_,
            sort,
        } => list(
            &root,
            &ListFilters {
                status: status.as_deref(),
                in_project: in_project.as_deref(),
                all: *all,
                blocked: *blocked,
                ready: *ready,
                where_,
                sort: sort.as_deref(),
            },
        ),
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
        Command::Queue(cmd) => queue_cmd::run_queue(&root, cmd),
        Command::Work(cmd) => work_cmd::run_work(&root, cmd),
        Command::Field(cmd) => field_cmd::run_field(&root, cmd),
    }
}

/// The repo root: `--repo` (or its deprecated `--project` alias) verbatim
/// (validated), else the nearest ancestor of the current directory that is
/// a Backlog repo.
fn resolve_repo(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = explicit {
        if switchbard_core::backlog_repo_available(root)? {
            return Ok(root.to_path_buf());
        }
        bail!(
            "{} is not a Backlog repo (no backlog/ directory there)",
            root.display()
        );
    }
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    find_repo_root(&cwd)?.ok_or_else(|| {
        anyhow!(
            "no Backlog repo found at or above {} — run inside one, or pass --repo <repo-root>",
            cwd.display()
        )
    })
}

pub(crate) fn find_repo_root(start: &Path) -> Result<Option<PathBuf>> {
    for dir in start.ancestors().take(MAX_ROOT_WALK) {
        if switchbard_core::backlog_repo_available(dir)? {
            return Ok(Some(dir.to_path_buf()));
        }
    }
    Ok(None)
}

/// Filters and sort applied by `sb list` (grouped so `list` doesn't carry
/// clippy's too-many-arguments smell across the format fork's two additive
/// filter sets: blocked/ready and declared custom fields).
struct ListFilters<'a> {
    status: Option<&'a str>,
    in_project: Option<&'a str>,
    all: bool,
    /// Only tasks with an open (not-done) dependency and not themselves
    /// done; mutually exclusive with `ready` at the CLI boundary.
    blocked: bool,
    /// Only tasks with no open dependency and not themselves done;
    /// mutually exclusive with `blocked` at the CLI boundary.
    ready: bool,
    where_: &'a [String],
    sort: Option<&'a str>,
}

fn list(root: &Path, filters: &ListFilters) -> Result<()> {
    let repo = switchbard_core::load_backlog_repo(root)?;
    for warning in &repo.warnings {
        eprintln!("sb: warning: {warning}");
    }
    let field_filters = field_cmd::parse_pairs(filters.where_)?;
    for (name, _) in &field_filters {
        find_declared(&repo.fields, name)?;
    }
    let mut rows: Vec<&BacklogTask> = repo
        .tasks
        .iter()
        .filter(|task| filters.all || task.source == switchbard_core::BacklogTaskSource::Active)
        .filter(|task| {
            filters
                .status
                .is_none_or(|wanted| task.status.eq_ignore_ascii_case(wanted))
        })
        .filter(|task| {
            filters
                .in_project
                .is_none_or(|wanted| task.project.as_deref() == Some(wanted))
        })
        .filter(|task| {
            field_filters
                .iter()
                .all(|(name, value)| task.custom.get(name).is_some_and(|v| v == value))
        })
        .filter(|task| {
            // A done task is never blocked or ready — it isn't actionable work
            // either way — matching the GUI's and sbt's own
            // `!task.is_done() && is_blocked(...)` guard.
            let is_blocked = !task.is_done() && switchbard_core::is_blocked(task, &repo);
            if filters.blocked && !is_blocked {
                return false;
            }
            if filters.ready && (task.is_done() || is_blocked) {
                return false;
            }
            true
        })
        .collect();
    if let Some(name) = filters.sort {
        let decl = find_declared(&repo.fields, name)?;
        rows.sort_by(|a, b| custom_sort_key(a, decl).cmp(&custom_sort_key(b, decl)));
    }
    for task in rows {
        println!("{}", render::list_row(task));
    }
    Ok(())
}

/// `(absent, declared-value rank, raw value)` — absent sorts last; an enum
/// field's rank is its position in the declaration's `values` (its
/// sort/section order); every other kind ranks 0, so the tuple's third
/// element (lexical) is what actually orders them.
fn custom_sort_key<'t>(
    task: &'t BacklogTask,
    decl: &switchbard_core::FieldDecl,
) -> (bool, usize, &'t str) {
    match task.custom.get(&decl.name) {
        None => (true, usize::MAX, ""),
        Some(value) => {
            let rank = if decl.kind == switchbard_core::FieldKind::Enum {
                decl.values
                    .iter()
                    .position(|v| v == value)
                    .unwrap_or(usize::MAX)
            } else {
                0
            };
            (false, rank, value.as_str())
        }
    }
}

fn find_declared<'f>(
    fields: &'f [switchbard_core::FieldDecl],
    name: &str,
) -> Result<&'f switchbard_core::FieldDecl> {
    fields.iter().find(|f| f.name == name).ok_or_else(|| {
        anyhow!("unknown field `{name}` — see `sb field list` for what this repo declares")
    })
}

fn view(root: &Path, id: &str) -> Result<()> {
    let project = switchbard_core::load_backlog_repo(root)?;
    let task = find_task(&project.tasks, id)
        .ok_or_else(|| anyhow!("no task {id} in {} — try `sb list --all`", root.display()))?;
    print!("{}", render::task_view(task, &project.fields));
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
    let due_date = args
        .due
        .as_deref()
        .map(switchbard_core::parse_due_date)
        .transpose()?;
    let custom = field_cmd::parse_pairs(&args.set)?;
    field_cmd::validate_set_pairs(root, &custom)?;
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
        due_date,
        custom,
    };
    let id = switchbard_core::create_backlog_task(root, &task)?;
    println!("{id}");
    if let Some(place) = args.rank.to_placement_args() {
        rank_cmd::rank_new_task(root, &id, &place)?;
    }
    Ok(())
}

/// Translate flags into the shared atomic edit command, preserving their order.
fn edit(root: &Path, args: &EditArgs) -> Result<()> {
    use switchbard_core::{TaskChecklist, TaskEditRequest};
    let set_custom = field_cmd::parse_pairs(&args.set)?;
    field_cmd::validate_set_pairs(root, &set_custom)?;
    field_cmd::validate_unset_names(root, &args.unset)?;
    let mut checklists = Vec::new();
    for (indices, checked, list) in [
        (&args.check_ac, true, TaskChecklist::AcceptanceCriteria),
        (&args.uncheck_ac, false, TaskChecklist::AcceptanceCriteria),
        (&args.check_dod, true, TaskChecklist::DefinitionOfDone),
        (&args.uncheck_dod, false, TaskChecklist::DefinitionOfDone),
    ] {
        checklists.extend(indices.iter().map(|index| (list, *index, checked)));
    }
    let request = TaskEditRequest {
        patch: patch_from(args, set_custom)?,
        acceptance_edits: acceptance_edits(&args.edit_ac)?,
        acceptance_removals: args.remove_ac.clone(),
        ball: args
            .ball
            .as_deref()
            .map(switchbard_core::Ball::parse)
            .transpose()?,
        labels: args
            .add_label
            .iter()
            .map(|label| (label.clone(), true))
            .chain(args.remove_label.iter().map(|label| (label.clone(), false)))
            .collect(),
        checklists,
        append_notes: args.append_notes.clone(),
        final_summary: args.final_summary.clone(),
        parent: args
            .parent
            .as_ref()
            .map(|parent| (!parent.eq_ignore_ascii_case("none")).then(|| parent.clone())),
    };
    let result = switchbard_core::edit_backlog_task_command(root, &args.id, &request)?;
    if result.changed {
        println!("Edited {}", args.id);
    }
    match result.moved {
        Some(new_id) => println!("Moved {} -> {new_id}", args.id),
        None if !result.changed => println!("no changes"),
        None => {}
    }
    Ok(())
}

/// clap hands `--edit-ac N TEXT` over as flat `[N, TEXT, N, TEXT, …]` pairs;
/// this is where N becomes a number.
fn acceptance_edits(raw: &[String]) -> Result<Vec<ChecklistTextEdit>> {
    debug_assert!(raw.len().is_multiple_of(2), "clap enforces num_args = 2");
    raw.chunks_exact(2)
        .map(|pair| {
            let index = pair[0].parse::<usize>().map_err(|_| {
                anyhow!(
                    "--edit-ac needs a criterion number, got `{}` (usage: --edit-ac <N> <TEXT>)",
                    pair[0]
                )
            })?;
            Ok(ChecklistTextEdit {
                index,
                text: pair[1].clone(),
            })
        })
        .collect()
}

fn patch_from(args: &EditArgs, set_custom: Vec<(String, String)>) -> Result<BacklogTaskPatch> {
    let due_date = args
        .due
        .as_deref()
        .map(switchbard_core::parse_due_date)
        .transpose()?;
    Ok(BacklogTaskPatch {
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
        due_date,
        clear_due_date: args.clear_due,
        set_custom,
        unset_custom: args.unset.clone(),
    })
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

        assert_eq!(
            find_repo_root(&deep).expect("scope lookup"),
            Some(root.clone())
        );
        assert_eq!(
            find_repo_root(dir.path()).expect("scope lookup"),
            None,
            "a dir with no backlog/ above it resolves to nothing"
        );
    }

    #[test]
    fn cli_definition_is_internally_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    /// A throwaway Backlog project: just enough directory shape for
    /// `create`/`edit`/`view` to run against.
    fn fixture_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("backlog/tasks")).expect("project layout");
        dir
    }

    /// AC #1: `sb create --due` writes `due_date` and `sb view` shows it.
    #[test]
    fn create_with_due_writes_due_date_and_view_shows_it() {
        let fixture = fixture_repo();
        let root = fixture.path();
        let cli = Cli::try_parse_from(["sb", "create", "Fixture task", "--due", "2026-09-14"])
            .expect("parses");
        let Command::Create(args) = cli.command else {
            panic!("expected Create");
        };
        create(root, &args).expect("create succeeds");

        let project = switchbard_core::load_backlog_repo(root).expect("reparse");
        let task = &project.tasks[0];
        assert_eq!(task.due_date.as_deref(), Some("2026-09-14"));
        assert!(render::task_view(task, &[]).contains("Due: 2026-09-14"));
    }

    /// AC #2: an invalid `--due` is rejected at the CLI boundary with a
    /// message naming the expected shape, for both `create` and `edit`.
    #[test]
    fn create_rejects_a_malformed_due_date() {
        let fixture = fixture_repo();
        let root = fixture.path();
        let cli = Cli::try_parse_from(["sb", "create", "Fixture task", "--due", "09/14/2026"])
            .expect("parses");
        let Command::Create(args) = cli.command else {
            panic!("expected Create");
        };
        let error = create(root, &args).unwrap_err();
        assert!(error.to_string().contains("YYYY-MM-DD"), "got: {error}");
    }

    #[test]
    fn edit_rejects_a_malformed_due_date_without_touching_the_task() {
        let fixture = fixture_repo();
        let root = fixture.path();
        let create_cli = Cli::try_parse_from(["sb", "create", "Fixture task"]).expect("parses");
        let Command::Create(create_args) = create_cli.command else {
            panic!("expected Create");
        };
        create(root, &create_args).expect("create succeeds");

        let edit_cli =
            Cli::try_parse_from(["sb", "edit", "TASK-1", "--due", "not-a-date"]).expect("parses");
        let Command::Edit(edit_args) = edit_cli.command else {
            panic!("expected Edit");
        };
        let error = patch_from(&edit_args, Vec::new()).unwrap_err();
        assert!(error.to_string().contains("YYYY-MM-DD"), "got: {error}");

        let project = switchbard_core::load_backlog_repo(root).expect("reparse");
        assert_eq!(
            project.tasks[0].due_date, None,
            "the rejected edit never wrote"
        );
    }

    #[test]
    fn edit_can_set_and_clear_the_due_date() {
        let fixture = fixture_repo();
        let root = fixture.path();
        let create_cli = Cli::try_parse_from(["sb", "create", "Fixture task"]).expect("parses");
        let Command::Create(create_args) = create_cli.command else {
            panic!("expected Create");
        };
        create(root, &create_args).expect("create succeeds");

        let edit_cli =
            Cli::try_parse_from(["sb", "edit", "TASK-1", "--due", "2026-09-14"]).expect("parses");
        let Command::Edit(edit_args) = edit_cli.command else {
            panic!("expected Edit");
        };
        edit(root, &edit_args).expect("edit succeeds");
        let project = switchbard_core::load_backlog_repo(root).expect("reparse");
        assert_eq!(project.tasks[0].due_date.as_deref(), Some("2026-09-14"));

        let clear_cli =
            Cli::try_parse_from(["sb", "edit", "TASK-1", "--clear-due"]).expect("parses");
        let Command::Edit(clear_args) = clear_cli.command else {
            panic!("expected Edit");
        };
        edit(root, &clear_args).expect("edit succeeds");
        let project = switchbard_core::load_backlog_repo(root).expect("reparse");
        assert_eq!(project.tasks[0].due_date, None);
    }
}
