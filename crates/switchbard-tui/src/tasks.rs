//! Task loading and the filter language shared by `/` and saved views.

use std::path::Path;

use anyhow::Result;
use switchbard_core::{load_backlog_repo, BacklogTask, BacklogTaskSource};

pub fn load(root: &Path) -> Result<Vec<BacklogTask>> {
    let repo = load_backlog_repo(root)?;
    Ok(repo
        .tasks
        .into_iter()
        .filter(|task| {
            matches!(
                task.source,
                BacklogTaskSource::Active | BacklogTaskSource::Draft
            )
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    Text(String),
    Status(String),
    Priority(String),
    Label(String),
    Project(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    terms: Vec<Term>,
}

impl Filter {
    pub fn parse(text: &str) -> Filter {
        let terms = text
            .split_whitespace()
            .map(|word| {
                let lower = word.to_lowercase();
                match lower.split_once(':') {
                    Some(("status", value)) => Term::Status(value.to_string()),
                    Some(("pri", value)) | Some(("priority", value)) => {
                        Term::Priority(value.to_string())
                    }
                    Some(("label", value)) => Term::Label(value.to_string()),
                    Some(("project", value)) => Term::Project(value.to_string()),
                    _ => Term::Text(lower),
                }
            })
            .collect();
        Filter { terms }
    }

    pub fn matches(&self, task: &BacklogTask) -> bool {
        self.terms.iter().all(|term| match term {
            Term::Text(needle) => {
                task.id.to_lowercase().contains(needle)
                    || task.title.to_lowercase().contains(needle)
            }
            Term::Status(needle) => task.status.to_lowercase().contains(needle),
            Term::Priority(needle) => task.priority.to_lowercase().contains(needle),
            Term::Label(needle) => task
                .labels
                .iter()
                .any(|label| label.to_lowercase().contains(needle)),
            Term::Project(needle) => task
                .project
                .as_deref()
                .is_some_and(|project| project.to_lowercase().contains(needle)),
        })
    }
}
