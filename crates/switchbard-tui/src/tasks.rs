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

/// One word of a filter. `pri:high,medium` is one term matching either value;
/// `status:!done` hides a value; a bare word searches id and title.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    Text(String),
    AnyOf(FilterField, Vec<String>),
    Not(FilterField, String),
}

impl Term {
    fn parse(word: &str) -> Term {
        let lower = word.to_lowercase();
        let Some((keyword, value)) = lower.split_once(':') else {
            return Term::Text(lower);
        };
        let Some(field) = FilterField::parse(keyword) else {
            return Term::Text(lower);
        };
        match value.strip_prefix('!') {
            Some(hidden) => Term::Not(field, loose(hidden)),
            None => Term::AnyOf(field, value.split(',').map(loose).collect()),
        }
    }

    fn field(&self) -> Option<FilterField> {
        match self {
            Term::Text(_) => None,
            Term::AnyOf(field, _) | Term::Not(field, _) => Some(*field),
        }
    }

    fn allows(&self, values: &[String]) -> bool {
        match self {
            Term::Text(_) => true,
            Term::AnyOf(_, wanted) => values
                .iter()
                .any(|value| wanted.iter().any(|want| loose(value).contains(want))),
            Term::Not(_, hidden) => !values.iter().any(|value| loose(value) == *hidden),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    terms: Vec<Term>,
}

impl Filter {
    pub fn parse(text: &str) -> Filter {
        Filter {
            terms: text.split_whitespace().map(Term::parse).collect(),
        }
    }

    pub fn matches(&self, task: &BacklogTask) -> bool {
        self.terms.iter().all(|term| match term {
            Term::Text(needle) => {
                task.id.to_lowercase().contains(needle)
                    || task.title.to_lowercase().contains(needle)
            }
            Term::AnyOf(field, _) | Term::Not(field, _) => term.allows(&field.values_of(task)),
        })
    }

    /// Would a task carrying exactly `value` for `field` pass this filter's terms for that field?
    pub fn field_allows(text: &str, field: FilterField, value: &str) -> bool {
        let values = [value.to_string()];
        Filter::parse(text)
            .terms
            .iter()
            .filter(|term| term.field() == Some(field))
            .all(|term| term.allows(&values))
    }

    pub fn loose_contains(haystack: &str, needle: &str) -> bool {
        loose(haystack).contains(&loose(needle))
    }

    /// Replaces every term for `field` with `field:value`; other fields' terms stay.
    pub fn with_only(text: &str, field: FilterField, value: &str) -> String {
        let mut words = words_without_field(text, field);
        words.push(format!("{}:{}", field.keyword(), loose(value)));
        words.join(" ")
    }

    /// Rewrites `field`'s terms so exactly `shown` (out of `all`) pass, in the shortest form:
    /// no term when everything is shown, `field:!x` hides when at most half are hidden,
    /// `field:a,b` otherwise.
    pub fn with_shown(text: &str, field: FilterField, all: &[String], shown: &[String]) -> String {
        let mut words = words_without_field(text, field);
        let hidden: Vec<&String> = all.iter().filter(|value| !shown.contains(value)).collect();
        if hidden.is_empty() {
        } else if shown.is_empty() || hidden.len() <= shown.len() {
            for value in hidden {
                words.push(format!("{}:!{}", field.keyword(), loose(value)));
            }
        } else {
            let values: Vec<String> = shown.iter().map(|value| loose(value)).collect();
            words.push(format!("{}:{}", field.keyword(), values.join(",")));
        }
        words.join(" ")
    }
}

fn words_without_field(text: &str, field: FilterField) -> Vec<String> {
    text.split_whitespace()
        .filter(|word| Term::parse(word).field() != Some(field))
        .map(str::to_string)
        .collect()
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
