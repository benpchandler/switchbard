//! The `goal` subcommand family — weekly numeric goals tracked relative to
//! target (trajectory: *Weekly goals*).
//!
//! Output contract, same discipline as the other families: stdout is
//! payload only. `create` prints the name alone; `check-in` prints
//! `Checked in <NAME>: actual/target`; `roll` prints
//! `Rolled <N> goals into the week of <MONDAY>`; `list` prints one TSV row
//! per goal: name, week, actual/target, percent, pace. `--week` accepts any
//! date and normalizes to that week's Monday; omitted, it means this week.

use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::Path;
use switchbard_core::{compute_goal_statuses, week_monday_of, GoalMeasure, GoalStatus, NewGoal};

#[derive(Subcommand)]
pub enum GoalCmd {
    /// Define a goal with its first week's target; prints the name alone
    Create(Box<GoalCreateArgs>),
    /// Record a dated observation for a manual goal; prints
    /// `Checked in <NAME>: actual/target`
    #[command(name = "check-in")]
    CheckIn(Box<CheckInArgs>),
    /// List one week's goals, one tab-separated row: name, week,
    /// actual/target, percent, pace (on-track / behind / met / missed)
    List {
        /// Any date in the wanted week (normalized to its Monday);
        /// default: this week
        #[arg(long, value_name = "DATE")]
        week: Option<String>,
    },
    /// Print one goal: fields, then every week's target, actual, and pace
    View { name: String },
    /// Attach tasks and/or projects to a tasks-measured goal as counted
    /// inputs; prints `Attached <N> inputs to <NAME>` (0 = all were
    /// already attached)
    Attach(Box<InputArgs>),
    /// Detach previously attached inputs; prints
    /// `Detached <N> inputs from <NAME>`
    Detach(Box<InputArgs>),
    /// Give every goal lacking the week a new entry carrying its latest
    /// earlier target; prints `Rolled <N> goals into the week of <MONDAY>`
    Roll {
        /// Any date in the target week (normalized to its Monday);
        /// default: this week
        #[arg(long, value_name = "DATE")]
        week: Option<String>,
    },
}

#[derive(Args)]
pub struct GoalCreateArgs {
    /// Goal name — the id every other goal command takes
    pub name: String,
    /// This week's numeric target
    #[arg(long, value_name = "N")]
    pub target: i64,
    /// What the number counts (users, tasks, releases, …)
    #[arg(long, value_name = "UNIT")]
    pub unit: String,
    /// Any date in the first week (normalized to its Monday);
    /// default: this week
    #[arg(long, value_name = "DATE")]
    pub week: Option<String>,
    /// How the actual is measured: manual check-ins, or tasks done in the
    /// week matching --scope
    #[arg(long, value_parser = ["manual", "tasks"], default_value = "manual")]
    pub measure: String,
    /// For --measure tasks: the project name or label counted tasks match
    #[arg(long, value_name = "NAME")]
    pub scope: Option<String>,
}

#[derive(Args)]
pub struct InputArgs {
    /// Goal name
    pub name: String,
    /// A task id to count (repeatable); accepts `TASK-7`, `task-7`, or `7`
    #[arg(long = "task", value_name = "ID")]
    pub tasks: Vec<String>,
    /// A project name whose member tasks count (repeatable; named
    /// `--in-project` to match the membership flag elsewhere and to avoid
    /// the global `--project <DIR>` repo alias)
    #[arg(long = "in-project", value_name = "NAME")]
    pub projects: Vec<String>,
}

#[derive(Args)]
pub struct CheckInArgs {
    /// Goal name
    pub name: String,
    /// The observed value (cumulative for the week, not an increment)
    pub value: i64,
    /// Observation date, YYYY-MM-DD; default: today
    #[arg(long, value_name = "DATE")]
    pub date: Option<String>,
    /// Any date in the wanted week (normalized to its Monday);
    /// default: this week
    #[arg(long, value_name = "DATE")]
    pub week: Option<String>,
}

pub fn run_goal(root: &Path, cmd: &GoalCmd) -> Result<()> {
    match cmd {
        GoalCmd::Create(args) => {
            let measure = match args.measure.as_str() {
                "tasks" => GoalMeasure::Tasks,
                _ => GoalMeasure::Manual,
            };
            let goal = NewGoal {
                name: args.name.clone(),
                unit: args.unit.clone(),
                measure,
                scope: args.scope.clone(),
                week: monday_arg(args.week.as_deref())?,
                target: args.target,
            };
            switchbard_core::create_goal(root, &goal)?;
            if measure == GoalMeasure::Tasks && args.scope.is_none() {
                eprintln!(
                    "switchbard-task: note: '{}' has no --scope; attach what it counts with `goal attach {} --task <ID> --in-project <NAME>`",
                    args.name, args.name
                );
            }
            println!("{}", args.name);
            Ok(())
        }
        GoalCmd::Attach(args) => {
            let repo = switchbard_core::load_backlog_repo(root)?;
            let tasks = canonical_task_ids(&repo, &args.tasks)?;
            let added =
                switchbard_core::attach_goal_inputs(root, &args.name, &tasks, &args.projects)?;
            println!("Attached {added} inputs to {}", args.name);
            Ok(())
        }
        GoalCmd::Detach(args) => {
            let removed =
                switchbard_core::detach_goal_inputs(root, &args.name, &args.tasks, &args.projects)?;
            println!("Detached {removed} inputs from {}", args.name);
            Ok(())
        }
        GoalCmd::CheckIn(args) => {
            let week = monday_arg(args.week.as_deref())?;
            let date = match &args.date {
                Some(date) => date.clone(),
                None => today().format("%Y-%m-%d").to_string(),
            };
            let repo = switchbard_core::load_backlog_repo(root)?;
            if let Some(goal) = repo.goals.iter().find(|g| g.name == args.name) {
                if goal.measure == GoalMeasure::Tasks {
                    anyhow::bail!(
                        "goal '{}' is measured from tasks — its actual is computed, not checked in",
                        args.name
                    );
                }
            }
            switchbard_core::check_in_goal(root, &args.name, &week, &date, args.value)?;
            let repo = switchbard_core::load_backlog_repo(root)?;
            let statuses = compute_goal_statuses(&[&repo], &week, today());
            match statuses.iter().find(|s| s.name == args.name) {
                Some(status) => println!(
                    "Checked in {}: {}/{}",
                    args.name, status.actual, status.target
                ),
                None => println!("Checked in {}: {}", args.name, args.value),
            }
            Ok(())
        }
        GoalCmd::List { week } => {
            let week = monday_arg(week.as_deref())?;
            let repo = switchbard_core::load_backlog_repo(root)?;
            warn(&repo.warnings);
            for status in compute_goal_statuses(&[&repo], &week, today()) {
                println!("{}", goal_row(&status));
            }
            Ok(())
        }
        GoalCmd::View { name } => {
            let repo = switchbard_core::load_backlog_repo(root)?;
            warn(&repo.warnings);
            let Some(goal) = repo.goals.iter().find(|g| &g.name == name) else {
                anyhow::bail!(
                    "no goal '{name}' — try `sb goal list`, or define it with `sb goal create`"
                );
            };
            print!("{}", goal_view(&repo, goal));
            Ok(())
        }
        GoalCmd::Roll { week } => {
            let week = monday_arg(week.as_deref())?;
            let rolled = switchbard_core::roll_goals(root, &week)?;
            println!("Rolled {rolled} goals into the week of {week}");
            Ok(())
        }
    }
}

fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

/// Resolve each `--task` argument against the backlog and return the
/// canonical stored ids, so `7` / `task-7` attach as `TASK-7` and a typo
/// fails here instead of silently counting nothing.
fn canonical_task_ids(repo: &switchbard_core::BacklogRepo, ids: &[String]) -> Result<Vec<String>> {
    ids.iter()
        .map(|id| {
            crate::find_task(&repo.tasks, id)
                .map(|task| task.id.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!("no task '{id}' — check `switchbard-task list` for the id")
                })
        })
        .collect()
}

/// Normalize an optional `--week` value (any date in the week) to that
/// week's Monday; `None` means this week.
fn monday_arg(week: Option<&str>) -> Result<String> {
    let date = match week {
        Some(week) => chrono::NaiveDate::parse_from_str(week.trim(), "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("--week must be a YYYY-MM-DD date, got `{week}`"))?,
        None => today(),
    };
    Ok(week_monday_of(date).format("%Y-%m-%d").to_string())
}

fn warn(warnings: &[String]) {
    for warning in warnings {
        eprintln!("sb: warning: {warning}");
    }
}

fn goal_row(status: &GoalStatus) -> String {
    format!(
        "{}\t{}\t{}/{}\t{:.0}%\t{}",
        status.name,
        status.week,
        status.actual,
        status.target,
        f64::from(status.progress_fraction()) * 100.0,
        status.pace.label(),
    )
}

fn goal_view(repo: &switchbard_core::BacklogRepo, goal: &switchbard_core::GoalDef) -> String {
    let mut out = format!("{}\n", goal.name);
    out.push_str(&format!("Unit: {}\n", goal.unit));
    out.push_str(&format!("Measure: {}\n", goal.measure.label()));
    if let Some(scope) = &goal.scope {
        out.push_str(&format!("Scope: {scope}\n"));
    }
    if !goal.inputs.tasks.is_empty() {
        out.push_str(&format!("Input tasks: {}\n", goal.inputs.tasks.join(", ")));
    }
    if !goal.inputs.projects.is_empty() {
        out.push_str(&format!(
            "Input projects: {}\n",
            goal.inputs.projects.join(", ")
        ));
    }
    if !goal.weeks.is_empty() {
        out.push('\n');
    }
    for week in goal.weeks.keys() {
        let statuses = compute_goal_statuses(&[repo], week, today());
        if let Some(status) = statuses.iter().find(|s| s.name == goal.name) {
            out.push_str(&format!(
                "{week}: {}/{} ({:.0}%) {}\n",
                status.actual,
                status.target,
                f64::from(status.progress_fraction()) * 100.0,
                status.pace.label(),
            ));
        }
    }
    out
}
