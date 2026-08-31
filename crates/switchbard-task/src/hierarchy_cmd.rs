//! The `project` and `initiative` subcommand families — the CLI surface of
//! the Linear hierarchy (trajectory: *Linear-vocabulary hierarchy*).
//!
//! Output contract, same discipline as the task verbs: stdout is payload
//! only. `create` prints the name alone (names are the tier's ids);
//! edit-shaped verbs print `Edited <NAME>` or `no changes`; `complete` /
//! `archive` print `Completed <NAME>` / `Canceled <NAME>`; `list` prints
//! one TSV row per entry; `view` prints fields, description, progress, then
//! member-task rows in the task list's own row shape.

use crate::render;
use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::Path;
use switchbard_core::{
    compute_hierarchy_rollup, InitiativeDefPatch, InitiativeRollup, NewInitiativeDef,
    NewProjectDef, ProjectDefPatch, ProjectRollup, WriteOutcome,
};

#[derive(Subcommand)]
pub enum ProjectCmd {
    /// List projects, one tab-separated row: name, status, done/total,
    /// percent, target date, initiative (empty columns when unset)
    List,
    /// Print one project: fields, progress, description, then its member
    /// tasks as task-list rows
    View { name: String },
    /// Define a project (backlog/projects/<slug>.md); prints the name alone
    Create(Box<ProjectCreateArgs>),
    /// Edit a project definition; prints `Edited <NAME>` or `no changes`
    Edit(Box<ProjectEditArgs>),
    /// Set the definition's status to Completed; prints `Completed <NAME>`
    Complete { name: String },
    /// Set the definition's status to Canceled (no file moves — tasks may
    /// still reference the name); prints `Canceled <NAME>`
    Archive { name: String },
}

#[derive(Args)]
pub struct ProjectCreateArgs {
    /// Project name — the exact string tasks reference via --in-project
    pub name: String,
    /// Description (Markdown body of the definition file)
    #[arg(short = 'd', long)]
    pub description: Option<String>,
    /// Lifecycle status (default: Planned; one of Planned, In Progress,
    /// Completed, Canceled)
    #[arg(short = 's', long)]
    pub status: Option<String>,
    /// Target date, YYYY-MM-DD
    #[arg(long, value_name = "DATE")]
    pub target_date: Option<String>,
    /// Initiative this project belongs to (name-keyed, same as membership)
    #[arg(long, value_name = "NAME")]
    pub initiative: Option<String>,
    /// Project lead
    #[arg(long, value_name = "NAME")]
    pub lead: Option<String>,
}

#[derive(Args)]
pub struct ProjectEditArgs {
    pub name: String,
    /// Replace the definition's description body
    #[arg(short = 'd', long)]
    pub description: Option<String>,
    /// Set the lifecycle status
    #[arg(short = 's', long)]
    pub status: Option<String>,
    /// Set the target date, YYYY-MM-DD
    #[arg(long, value_name = "DATE", conflicts_with = "clear_target_date")]
    pub target_date: Option<String>,
    /// Remove the target date
    #[arg(long)]
    pub clear_target_date: bool,
    /// Assign the initiative
    #[arg(long, value_name = "NAME", conflicts_with = "clear_initiative")]
    pub initiative: Option<String>,
    /// Remove the initiative assignment
    #[arg(long)]
    pub clear_initiative: bool,
    /// Set the project lead
    #[arg(long, value_name = "NAME", conflicts_with = "clear_lead")]
    pub lead: Option<String>,
    /// Remove the project lead
    #[arg(long)]
    pub clear_lead: bool,
}

#[derive(Subcommand)]
pub enum InitiativeCmd {
    /// List initiatives, one tab-separated row: name, status, done/total,
    /// percent, target date, project count
    List,
    /// Print one initiative: fields, progress, description, then its member
    /// projects as project-list rows
    View { name: String },
    /// Define an initiative (backlog/initiatives/<slug>.md); prints the name
    Create(Box<InitiativeCreateArgs>),
    /// Edit an initiative definition; prints `Edited <NAME>` or `no changes`
    Edit(Box<InitiativeEditArgs>),
    /// Set the definition's status to Completed; prints `Completed <NAME>`
    Complete { name: String },
    /// Set the definition's status to Canceled; prints `Canceled <NAME>`
    Archive { name: String },
}

#[derive(Args)]
pub struct InitiativeCreateArgs {
    /// Initiative name — the exact string project defs reference
    pub name: String,
    /// Description (Markdown body of the definition file)
    #[arg(short = 'd', long)]
    pub description: Option<String>,
    /// Lifecycle status (default: Planned)
    #[arg(short = 's', long)]
    pub status: Option<String>,
    /// Target date, YYYY-MM-DD
    #[arg(long, value_name = "DATE")]
    pub target_date: Option<String>,
}

#[derive(Args)]
pub struct InitiativeEditArgs {
    pub name: String,
    /// Replace the definition's description body
    #[arg(short = 'd', long)]
    pub description: Option<String>,
    /// Set the lifecycle status
    #[arg(short = 's', long)]
    pub status: Option<String>,
    /// Set the target date, YYYY-MM-DD
    #[arg(long, value_name = "DATE", conflicts_with = "clear_target_date")]
    pub target_date: Option<String>,
    /// Remove the target date
    #[arg(long)]
    pub clear_target_date: bool,
}

pub fn run_project(root: &Path, cmd: &ProjectCmd) -> Result<()> {
    match cmd {
        ProjectCmd::List => {
            let repo = switchbard_core::load_backlog_repo(root)?;
            warn(&repo.warnings);
            for project in flat_projects(&compute_hierarchy_rollup(&[&repo])) {
                println!("{}", project_row(&project));
            }
            Ok(())
        }
        ProjectCmd::View { name } => {
            let repo = switchbard_core::load_backlog_repo(root)?;
            warn(&repo.warnings);
            let rollup = compute_hierarchy_rollup(&[&repo]);
            let Some(project) = flat_projects(&rollup).into_iter().find(|p| &p.name == name) else {
                anyhow::bail!(
                    "no project '{name}' — try `switchbard-task project list`, or define it with `switchbard-task project create`"
                );
            };
            print!("{}", project_view(&project, &repo));
            Ok(())
        }
        ProjectCmd::Create(args) => {
            let def = NewProjectDef {
                name: args.name.clone(),
                status: args.status.clone().unwrap_or_default(),
                target_date: args.target_date.clone(),
                initiative: args.initiative.clone(),
                lead: args.lead.clone(),
                description: args.description.clone().unwrap_or_default(),
            };
            switchbard_core::create_project_def(root, &def)?;
            println!("{}", args.name);
            Ok(())
        }
        ProjectCmd::Edit(args) => {
            let patch = ProjectDefPatch {
                status: args.status.clone(),
                target_date: args.target_date.clone(),
                clear_target_date: args.clear_target_date,
                initiative: args.initiative.clone(),
                clear_initiative: args.clear_initiative,
                lead: args.lead.clone(),
                clear_lead: args.clear_lead,
                description: args.description.clone(),
            };
            print_outcome(
                switchbard_core::edit_project_def(root, &args.name, &patch)?,
                &args.name,
            );
            Ok(())
        }
        ProjectCmd::Complete { name } => {
            let patch = ProjectDefPatch {
                status: Some("Completed".to_string()),
                ..ProjectDefPatch::default()
            };
            let _ = switchbard_core::edit_project_def(root, name, &patch)?;
            println!("Completed {name}");
            Ok(())
        }
        ProjectCmd::Archive { name } => {
            let patch = ProjectDefPatch {
                status: Some("Canceled".to_string()),
                ..ProjectDefPatch::default()
            };
            let _ = switchbard_core::edit_project_def(root, name, &patch)?;
            println!("Canceled {name}");
            Ok(())
        }
    }
}

pub fn run_initiative(root: &Path, cmd: &InitiativeCmd) -> Result<()> {
    match cmd {
        InitiativeCmd::List => {
            let repo = switchbard_core::load_backlog_repo(root)?;
            warn(&repo.warnings);
            let rollup = compute_hierarchy_rollup(&[&repo]);
            for initiative in rollup.initiatives.iter().filter(|i| i.name.is_some()) {
                println!("{}", initiative_row(initiative));
            }
            Ok(())
        }
        InitiativeCmd::View { name } => {
            let repo = switchbard_core::load_backlog_repo(root)?;
            warn(&repo.warnings);
            let rollup = compute_hierarchy_rollup(&[&repo]);
            let Some(initiative) = rollup
                .initiatives
                .iter()
                .find(|i| i.name.as_deref() == Some(name.as_str()))
            else {
                anyhow::bail!(
                    "no initiative '{name}' — try `switchbard-task initiative list`, or define it with `switchbard-task initiative create`"
                );
            };
            print!("{}", initiative_view(initiative, &repo));
            Ok(())
        }
        InitiativeCmd::Create(args) => {
            let def = NewInitiativeDef {
                name: args.name.clone(),
                status: args.status.clone().unwrap_or_default(),
                target_date: args.target_date.clone(),
                description: args.description.clone().unwrap_or_default(),
            };
            switchbard_core::create_initiative_def(root, &def)?;
            println!("{}", args.name);
            Ok(())
        }
        InitiativeCmd::Edit(args) => {
            let patch = InitiativeDefPatch {
                status: args.status.clone(),
                target_date: args.target_date.clone(),
                clear_target_date: args.clear_target_date,
                description: args.description.clone(),
            };
            print_outcome(
                switchbard_core::edit_initiative_def(root, &args.name, &patch)?,
                &args.name,
            );
            Ok(())
        }
        InitiativeCmd::Complete { name } => {
            let patch = InitiativeDefPatch {
                status: Some("Completed".to_string()),
                ..InitiativeDefPatch::default()
            };
            let _ = switchbard_core::edit_initiative_def(root, name, &patch)?;
            println!("Completed {name}");
            Ok(())
        }
        InitiativeCmd::Archive { name } => {
            let patch = InitiativeDefPatch {
                status: Some("Canceled".to_string()),
                ..InitiativeDefPatch::default()
            };
            let _ = switchbard_core::edit_initiative_def(root, name, &patch)?;
            println!("Canceled {name}");
            Ok(())
        }
    }
}

fn warn(warnings: &[String]) {
    for warning in warnings {
        eprintln!("switchbard-task: warning: {warning}");
    }
}

fn print_outcome(outcome: WriteOutcome, name: &str) {
    if outcome.changed() {
        println!("Edited {name}");
    } else {
        println!("no changes");
    }
}

/// Every project across the rollup's initiative buckets, name-sorted — the
/// flat view `project list`/`view` present (the nesting belongs to
/// `initiative view` and the GUI lens).
fn flat_projects(rollup: &switchbard_core::HierarchyRollup) -> Vec<ProjectRollup> {
    let mut projects: Vec<ProjectRollup> = rollup
        .initiatives
        .iter()
        .flat_map(|initiative| initiative.projects.iter().cloned())
        .collect();
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    projects
}

fn project_row(project: &ProjectRollup) -> String {
    format!(
        "{}\t{}\t{}/{}\t{:.0}%\t{}\t{}",
        project.name,
        project.status.as_deref().unwrap_or(""),
        project.done,
        project.total,
        project.completion_pct(),
        project.target_date.as_deref().unwrap_or(""),
        project.initiative.as_deref().unwrap_or(""),
    )
}

fn initiative_row(initiative: &InitiativeRollup) -> String {
    format!(
        "{}\t{}\t{}/{}\t{:.0}%\t{}\t{}",
        initiative.name.as_deref().unwrap_or(""),
        initiative.status.as_deref().unwrap_or(""),
        initiative.done,
        initiative.total,
        initiative.completion_pct(),
        initiative.target_date.as_deref().unwrap_or(""),
        initiative.projects.len(),
    )
}

fn project_view(project: &ProjectRollup, repo: &switchbard_core::BacklogRepo) -> String {
    let mut out = format!("{}\n", project.name);
    push_field(&mut out, "Status", project.status.as_deref().unwrap_or(""));
    push_field(
        &mut out,
        "Target date",
        project.target_date.as_deref().unwrap_or(""),
    );
    push_field(
        &mut out,
        "Initiative",
        project.initiative.as_deref().unwrap_or(""),
    );
    push_field(&mut out, "Lead", project.lead.as_deref().unwrap_or(""));
    out.push_str(&format!(
        "Progress: {}/{} ({:.0}%)\n",
        project.done,
        project.total,
        project.completion_pct()
    ));
    if !project.has_def {
        out.push_str(
            "Definition: none (reference-only — `switchbard-task project create` to add one)\n",
        );
    }
    if let Some(def) = repo.project_defs.iter().find(|d| d.name == project.name) {
        if !def.description.is_empty() {
            out.push_str(&format!("\n{}\n", def.description));
        }
    }
    let mut members: Vec<_> = repo
        .tasks
        .iter()
        .filter(|task| task.project.as_deref() == Some(project.name.as_str()))
        .collect();
    members.sort_by(|a, b| a.id.cmp(&b.id));
    if !members.is_empty() {
        out.push('\n');
        for task in members {
            out.push_str(&format!("{}\n", render::list_row(task)));
        }
    }
    out
}

fn initiative_view(initiative: &InitiativeRollup, repo: &switchbard_core::BacklogRepo) -> String {
    let name = initiative.name.as_deref().unwrap_or("");
    let mut out = format!("{name}\n");
    push_field(
        &mut out,
        "Status",
        initiative.status.as_deref().unwrap_or(""),
    );
    push_field(
        &mut out,
        "Target date",
        initiative.target_date.as_deref().unwrap_or(""),
    );
    out.push_str(&format!(
        "Progress: {}/{} ({:.0}%)\n",
        initiative.done,
        initiative.total,
        initiative.completion_pct()
    ));
    if let Some(def) = repo.initiative_defs.iter().find(|d| d.name == name) {
        if !def.description.is_empty() {
            out.push_str(&format!("\n{}\n", def.description));
        }
    }
    if !initiative.projects.is_empty() {
        out.push('\n');
        for project in &initiative.projects {
            out.push_str(&format!("{}\n", project_row(project)));
        }
    }
    out
}

fn push_field(out: &mut String, name: &str, value: &str) {
    if !value.is_empty() {
        out.push_str(&format!("{name}: {value}\n"));
    }
}
