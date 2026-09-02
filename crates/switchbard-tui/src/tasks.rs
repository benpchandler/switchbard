//! Task loading and the filter language shared by `/`, `f`, and saved views.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    Status,
    Priority,
    Label,
    Project,
}

impl FilterField {
    fn parse(keyword: &str) -> Option<FilterField> {
        Some(match keyword {
            "status" => FilterField::Status,
            "pri" | "priority" => FilterField::Priority,
            "label" => FilterField::Label,
            "project" => FilterField::Project,
            _ => return None,
        })
    }

    pub fn keyword(self) -> &'static str {
        match self {
            FilterField::Status => "status",
            FilterField::Priority => "pri",
            FilterField::Label => "label",
            FilterField::Project => "project",
        }
    }

    fn values_of(self, task: &BacklogTask) -> Vec<String> {
        match self {
            FilterField::Status => vec![task.status.clone()],
            FilterField::Priority => vec![task.priority.clone()],
            FilterField::Label => task.labels.clone(),
            FilterField::Project => task.project.clone().into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    Text(String),
    Field(FilterField, String),
    Excluded(FilterField, String),
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
                let Some((keyword, value)) = lower.split_once(':') else {
                    return Term::Text(lower);
                };
                let Some(field) = FilterField::parse(keyword) else {
                    return Term::Text(lower);
                };
                match value.strip_prefix('!') {
                    Some(excluded) => Term::Excluded(field, loose(excluded)),
                    None => Term::Field(field, loose(value)),
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
            Term::Field(field, needle) => field
                .values_of(task)
                .iter()
                .any(|value| loose(value).contains(needle)),
            Term::Excluded(field, needle) => !field
                .values_of(task)
                .iter()
                .any(|value| loose(value) == *needle),
        })
    }

    /// Adds `field:!value` if absent, removes it if present. Exclusions stack.
    pub fn toggle_exclusion(text: &str, field: FilterField, value: &str) -> String {
        let word = format!("{}:!{}", field.keyword(), loose(value));
        let mut words: Vec<String> = text.split_whitespace().map(str::to_string).collect();
        match words
            .iter()
            .position(|existing| existing.to_lowercase() == word)
        {
            Some(index) => {
                words.remove(index);
            }
            None => words.push(word),
        }
        words.join(" ")
    }

    pub fn is_excluded(text: &str, field: FilterField, value: &str) -> bool {
        let word = format!("{}:!{}", field.keyword(), loose(value));
        text.split_whitespace()
            .any(|existing| existing.to_lowercase() == word)
    }

    /// Replaces any positive `field:` term with `field:value`; exclusions and other terms stay.
    pub fn with_field(text: &str, field: FilterField, value: &str) -> String {
        let prefix = format!("{}:", field.keyword());
        let exclusion_prefix = format!("{prefix}!");
        let mut words: Vec<String> = text
            .split_whitespace()
            .filter(|word| {
                let lower = word.to_lowercase();
                !lower.starts_with(&prefix) || lower.starts_with(&exclusion_prefix)
            })
            .map(str::to_string)
            .collect();
        words.push(format!("{prefix}{}", loose(value)));
        words.join(" ")
    }
}

/// "To Do", "todo", and "TO-DO" all compare equal.
fn loose(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Distinct values a field takes across `tasks`, most common first, with counts.
pub fn field_values(tasks: &[BacklogTask], field: FilterField) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for value in tasks.iter().flat_map(|task| field.values_of(task)) {
        match counts.iter_mut().find(|(seen, _)| *seen == value) {
            Some(entry) => entry.1 += 1,
            None => counts.push((value, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts
}
