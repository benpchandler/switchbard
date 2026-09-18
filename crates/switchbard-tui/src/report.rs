//! Bug and idea creation for an explicit repository or legacy tool-report scope.
//! Both carry the screen and recent action trail captured at submission.

use std::path::Path;

use anyhow::{bail, Result};
use switchbard_core::{create_backlog_task, create_task_allocating_id, NewBacklogTask};

/// Standing bug bucket. Legacy tool ideas retain their unassigned project;
/// repository captures use the owner-selected Bugs and Ideas buckets.
const BUG_PROJECT: &str = "Bugs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportScope {
    Tool,
    Repository,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    Bug,
    Idea,
}

impl ReportKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ReportKind::Bug => "bug",
            ReportKind::Idea => "idea",
        }
    }

    fn project(self) -> Option<String> {
        match self {
            ReportKind::Bug => Some(BUG_PROJECT.to_string()),
            ReportKind::Idea => None,
        }
    }
}

pub struct ReportContext<'a> {
    pub intent: &'a str,
    pub location: &'a str,
    pub screen: &'a str,
    pub trail: &'a [String],
}

pub fn file_report(repo_root: &Path, kind: ReportKind, context: ReportContext) -> Result<String> {
    file_scoped_report(repo_root, ReportScope::Tool, kind, context)
}

pub fn file_scoped_report(
    repo_root: &Path,
    scope: ReportScope,
    kind: ReportKind,
    context: ReportContext,
) -> Result<String> {
    let intent = context.intent.trim();
    if intent.is_empty() {
        bail!("say what you were trying to do: :{} <text>", kind.label());
    }
    let mut task = report_task(kind, intent, context.description(intent));
    if scope == ReportScope::Repository {
        apply_repository_defaults(kind, intent, &mut task);
        return create_backlog_task(repo_root, &task);
    }
    let (id, _path) = create_task_allocating_id(repo_root, &task)?;
    Ok(id)
}

impl ReportContext<'_> {
    fn description(&self, intent: &str) -> String {
        format!(
            "Filed from sbt {version} while at {location}.\n\n\
         Impact: {intent}\n\
         Evidence: screen and action trail below, captured at filing time.\n\n\
         ## Screen\n\n```text\n{screen}\n```\n\n## Action trail\n\n```text\n{trail}\n```",
            version = env!("CARGO_PKG_VERSION"),
            location = self.location,
            screen = self.screen.trim_end(),
            trail = self.trail.join("\n"),
        )
    }
}

fn report_task(kind: ReportKind, intent: &str, description: String) -> NewBacklogTask {
    NewBacklogTask {
        title: format!("sbt {}: {intent}", kind.label()),
        description,
        status: "To Do".to_string(),
        priority: "medium".to_string(),
        acceptance_criteria: vec![
            "Reporter confirms the behaviour in sbt matches what they were trying to do"
                .to_string(),
        ],
        parent: None,
        labels: vec!["tui".to_string(), kind.label().to_string()],
        assignees: Vec::new(),
        project: kind.project(),
        dependencies: Vec::new(),
        due_date: None,
        custom: Vec::new(),
    }
}

fn apply_repository_defaults(kind: ReportKind, intent: &str, task: &mut NewBacklogTask) {
    task.title = format!("{}: {intent}", kind.label());
    task.status = "Not started".to_string();
    task.priority = "low".to_string();
    task.labels = vec![kind.label().to_string()];
    task.acceptance_criteria = vec![format!(
        "Reporter confirms this {} is addressed: {intent}",
        kind.label()
    )];
    // Membership creates the bucket even without a separate project definition.
    task.project = Some(match kind {
        ReportKind::Bug => BUG_PROJECT.to_string(),
        ReportKind::Idea => "Ideas".to_string(),
    });
}
