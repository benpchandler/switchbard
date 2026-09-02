//! One command from anywhere files a bug or idea as a task in the repo being viewed,
//! carrying the screen and the recent action trail so nothing has to be re-described.

use std::path::Path;

use anyhow::{bail, Result};
use switchbard_core::{create_task_allocating_id, NewBacklogTask};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    Bug,
    Idea,
}

impl ReportKind {
    fn label(self) -> &'static str {
        match self {
            ReportKind::Bug => "bug",
            ReportKind::Idea => "idea",
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
    let intent = context.intent.trim();
    if intent.is_empty() {
        bail!("say what you were trying to do: :{} <text>", kind.label());
    }
    let description = format!(
        "Filed from sbt {version} while at {location}.\n\n\
         Impact: {intent}\n\
         Evidence: screen and action trail below, captured at filing time.\n\n\
         ## Screen\n\n```text\n{screen}\n```\n\n## Action trail\n\n```text\n{trail}\n```",
        version = env!("CARGO_PKG_VERSION"),
        location = context.location,
        screen = context.screen.trim_end(),
        trail = context.trail.join("\n"),
    );
    let task = NewBacklogTask {
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
        project: None,
        dependencies: Vec::new(),
    };
    let (id, _path) = create_task_allocating_id(repo_root, &task)?;
    Ok(id)
}
