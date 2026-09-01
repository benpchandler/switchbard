//! TASK-97: the Tasks place's one enumeration of "fields you can group or
//! filter by" — `TaskField` and [`field_values`] are the single generic
//! engine both the "Group by: <field>" combo (`groups.rs`) and the filter
//! builder (`filters.rs`) share. Adding a groupable/filterable field means
//! adding one arm here, never a parallel hardcoded implementation in either
//! caller — the binding directive's "generic over every enumerable task
//! field... never a hardcoded option list" requirement.

use std::collections::HashMap;

use crate::ui::backlog::RepoRow;
use switchbard_core::{BacklogRepo, BacklogTask};

/// Every field `Group by:` and the filter builder can key on, derived
/// straight from `BacklogTask`'s own schema (`backlog/types.rs`) plus the
/// project-def-derived `Initiative`. Order here is display order in both
/// combos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum TaskField {
    #[default]
    Project,
    Status,
    Initiative,
    Priority,
    Label,
    Repo,
    Assignee,
    Parent,
    Source,
}

impl TaskField {
    pub const ALL: [TaskField; 9] = [
        Self::Project,
        Self::Status,
        Self::Initiative,
        Self::Priority,
        Self::Label,
        Self::Repo,
        Self::Assignee,
        Self::Parent,
        Self::Source,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Status => "Status",
            Self::Initiative => "Initiative",
            Self::Priority => "Priority",
            Self::Label => "Label",
            Self::Repo => "Repo",
            Self::Assignee => "Assignee",
            Self::Parent => "Parent",
            Self::Source => "Source",
        }
    }

    /// Stable persistence id (`TasksPlaceState`'s facet encoding) — never
    /// renamed once shipped, same discipline as `BacklogTaskSortKey::
    /// as_saved_id`.
    pub fn as_id(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Status => "status",
            Self::Initiative => "initiative",
            Self::Priority => "priority",
            Self::Label => "label",
            Self::Repo => "repo",
            Self::Assignee => "assignee",
            Self::Parent => "parent",
            Self::Source => "source",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.as_id() == id)
    }
}

pub(super) const NO_PROJECT: &str = "No project";
const NO_INITIATIVE: &str = "No initiative";
const NO_LABEL: &str = "No label";
const UNASSIGNED: &str = "Unassigned";
const TOP_LEVEL: &str = "Top-level (no parent)";

/// Project name → initiative name, first-def-wins (the same conflict rule
/// `switchbard_core::compute_hierarchy_rollup` applies one tier up) — the
/// join `TaskField::Initiative` needs that no core function exposes
/// per-task, only per-aggregate.
pub(super) fn initiative_by_project(repo: &BacklogRepo) -> HashMap<&str, &str> {
    let mut map = HashMap::new();
    for def in &repo.project_defs {
        if let Some(initiative) = &def.initiative {
            map.entry(def.name.as_str()).or_insert(initiative.as_str());
        }
    }
    map
}

/// Every bucket `task` belongs to for `field`. Most fields are single-
/// valued (one-element vec, the common case every caller must still handle
/// as a `Vec`); `Label`/`Assignee` are genuinely multi-valued and fan a task
/// out into each of its values (or one "none" bucket when empty) — a task
/// with two labels appears in both label groups and matches a filter
/// predicate naming either.
pub(super) fn field_values(task: &BacklogTask, row: &RepoRow, field: TaskField) -> Vec<String> {
    match field {
        TaskField::Project => vec![switchbard_core::effective_project(task, &row.repo)
            .map(str::to_string)
            .unwrap_or_else(|| NO_PROJECT.to_string())],
        TaskField::Status => vec![task.status.clone()],
        TaskField::Initiative => {
            let by_project = initiative_by_project(&row.repo);
            let name = switchbard_core::effective_project(task, &row.repo)
                .and_then(|project| by_project.get(project).copied());
            vec![name.unwrap_or(NO_INITIATIVE).to_string()]
        }
        TaskField::Priority => vec![task.priority.clone()],
        TaskField::Label => {
            if task.labels.is_empty() {
                vec![NO_LABEL.to_string()]
            } else {
                task.labels.clone()
            }
        }
        TaskField::Repo => vec![row.repo_name.clone()],
        TaskField::Assignee => {
            if task.assignees.is_empty() {
                vec![UNASSIGNED.to_string()]
            } else {
                task.assignees.clone()
            }
        }
        TaskField::Parent => vec![task.parent.clone().unwrap_or_else(|| TOP_LEVEL.to_string())],
        TaskField::Source => vec![task.source.label().to_string()],
    }
}

/// Distinct values `field` takes across `tasks`, alphabetical — the filter
/// builder's value picker for a chosen field, and available to a future
/// group-by-field value inspector. Not per-facet-excluded like `sort::
/// project_options`/`label_options` (those exist for a single always-on
/// filter row where offering a dead-end value is a real trap); the filter
/// builder's own AND semantics mean a predicate can always be removed, so a
/// simpler "every value visible right now" list is the honest, much
/// cheaper choice here.
pub(super) fn distinct_values<'a>(
    tasks: impl Iterator<Item = (&'a BacklogTask, &'a RepoRow)>,
    field: TaskField,
) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for (task, row) in tasks {
        for value in field_values(task, row, field) {
            seen.insert(value);
        }
    }
    seen.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_id_round_trips() {
        for field in TaskField::ALL {
            assert_eq!(TaskField::from_id(field.as_id()), Some(field));
        }
    }

    #[test]
    fn an_unrecognized_id_is_none() {
        assert_eq!(TaskField::from_id("not-a-real-field"), None);
    }
}
