//! TASK-97: the Tasks place's own view state — view mode, group-by, and the
//! filter builder's active/recent predicate sets. Session-only bits
//! (expansion, the add-predicate popup's draft) live alongside the
//! persisted ones for the same reason `BacklogViewState` mixes both: one
//! struct is where an agent looks for "everything Tasks-place-view-shaped."
//!
//! Persisted under the "tasks.all" `FilterMemory` — the same entry
//! `BacklogViewState::persist_filters` already writes to (binding
//! directive: persistence key names are reserved, not TASK-97's to
//! rename). Different facet keys, so the two `persist`/`restore` passes
//! never collide; see [`FACET_*`] for the exact names.

use std::collections::BTreeSet;

use switchbard_core::config::UiConfig;

use super::fields::TaskField;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TasksViewMode {
    #[default]
    List,
    Board,
}

impl TasksViewMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::List => "List",
            Self::Board => "Board",
        }
    }

    /// `pub(crate)`, not private: `ui::backlog::saved_views` (a sibling
    /// top-level module) needs this to encode/decode `SavedView::
    /// tasks_view_mode` — the same stable-persistence-id discipline
    /// `TaskField::as_id` documents.
    pub(crate) fn as_id(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Board => "board",
        }
    }

    pub(crate) fn from_id(id: &str) -> Self {
        match id {
            "board" => Self::Board,
            _ => Self::List,
        }
    }
}

/// One filter-builder predicate: `task` matches when `fields::field_values
/// (task, row, field)` contains `value` — AND-combined with every other
/// active predicate ([`TasksPlaceState::filters`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterPredicate {
    pub field: TaskField,
    pub value: String,
}

const FACET_GROUP_BY: &str = "group_by";
const FACET_VIEW_MODE: &str = "view_mode";
const FACET_FILTERS: &str = "filters";
const FACET_RECENT: &str = "recent_filters";

/// How many past filter sets `recent_filter_sets` remembers — the mock's
/// "recent:" row shows a handful, not a growing history.
const RECENT_CAP: usize = 5;

/// Field/value separators for the compact predicate-set encoding
/// (`encode_set`/`decode_set`) — control characters outside any realistic
/// task field value, so no escaping is needed.
const FIELD_VALUE_SEP: char = '\u{1}';
const PREDICATE_SEP: char = '\u{2}';
const SET_SEP: char = '\u{3}';

#[derive(Debug, Clone)]
pub struct TasksPlaceState {
    pub view_mode: TasksViewMode,
    /// `None` = ungrouped flat list. Defaults to `Project` — the mock's own
    /// default ("Tasks — the primary work list, grouped by project").
    pub group_by: Option<TaskField>,
    pub filters: Vec<FilterPredicate>,
    /// Most-recently-changed-from first, capped at [`RECENT_CAP`].
    pub recent_filter_sets: Vec<Vec<FilterPredicate>>,
    /// Which group keys currently show their expanded in-place summary band
    /// — session-only, same posture as `BacklogViewState::expanded_parents`.
    pub expanded_groups: BTreeSet<String>,
    /// Whether the "+ Filter" add-predicate popup is open — session-only.
    pub filter_builder_open: bool,
    /// Draft field/value for the add-predicate popup — session-only.
    pub draft_field: TaskField,
    pub draft_value: String,
}

impl Default for TasksPlaceState {
    fn default() -> Self {
        Self {
            view_mode: TasksViewMode::default(),
            group_by: Some(TaskField::Project),
            filters: Vec::new(),
            recent_filter_sets: Vec::new(),
            expanded_groups: BTreeSet::new(),
            filter_builder_open: false,
            draft_field: TaskField::default(),
            draft_value: String::new(),
        }
    }
}

impl TasksPlaceState {
    pub fn restore(ui: &UiConfig) -> Self {
        let mut state = Self::default();
        let Some(memory) = ui.filters.get("tasks.all") else {
            return state;
        };
        if let Some(value) = memory.facets.get(FACET_GROUP_BY) {
            state.group_by = if value.is_empty() {
                None
            } else {
                TaskField::from_id(value)
            };
        }
        if let Some(value) = memory.facets.get(FACET_VIEW_MODE) {
            state.view_mode = TasksViewMode::from_id(value);
        }
        if let Some(value) = memory.facets.get(FACET_FILTERS) {
            state.filters = decode_set(value);
        }
        if let Some(value) = memory.facets.get(FACET_RECENT) {
            state.recent_filter_sets = value.split(SET_SEP).map(decode_set).collect();
        }
        state
    }

    pub fn persist(&self, ui: &mut UiConfig) {
        let memory = ui.filters.entry("tasks.all".to_string()).or_default();
        memory.facets.insert(
            FACET_GROUP_BY.to_string(),
            self.group_by
                .map(TaskField::as_id)
                .unwrap_or("")
                .to_string(),
        );
        memory.facets.insert(
            FACET_VIEW_MODE.to_string(),
            self.view_mode.as_id().to_string(),
        );
        if self.filters.is_empty() {
            memory.facets.remove(FACET_FILTERS);
        } else {
            memory
                .facets
                .insert(FACET_FILTERS.to_string(), encode_set(&self.filters));
        }
        if self.recent_filter_sets.is_empty() {
            memory.facets.remove(FACET_RECENT);
        } else {
            let joined = self
                .recent_filter_sets
                .iter()
                .map(|set| encode_set(set))
                .collect::<Vec<_>>()
                .join(&SET_SEP.to_string());
            memory.facets.insert(FACET_RECENT.to_string(), joined);
        }
    }

    /// Record the just-cleared/replaced filter set into `recent_filter_sets`
    /// (most-recent-first, de-duplicated, capped) — called wherever the
    /// active filter set is about to change to something else.
    pub fn remember_recent(&mut self, set: Vec<FilterPredicate>) {
        if set.is_empty() {
            return;
        }
        self.recent_filter_sets.retain(|existing| existing != &set);
        self.recent_filter_sets.insert(0, set);
        self.recent_filter_sets.truncate(RECENT_CAP);
    }
}

fn encode_set(predicates: &[FilterPredicate]) -> String {
    predicates
        .iter()
        .map(|predicate| {
            format!(
                "{}{FIELD_VALUE_SEP}{}",
                predicate.field.as_id(),
                predicate.value
            )
        })
        .collect::<Vec<_>>()
        .join(&PREDICATE_SEP.to_string())
}

fn decode_set(text: &str) -> Vec<FilterPredicate> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split(PREDICATE_SEP)
        .filter_map(|entry| {
            let (field_id, value) = entry.split_once(FIELD_VALUE_SEP)?;
            let field = TaskField::from_id(field_id)?;
            Some(FilterPredicate {
                field,
                value: value.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn predicate(field: TaskField, value: &str) -> FilterPredicate {
        FilterPredicate {
            field,
            value: value.to_string(),
        }
    }

    #[test]
    fn a_fresh_state_defaults_to_grouped_by_project_list_view() {
        let state = TasksPlaceState::default();
        assert_eq!(state.group_by, Some(TaskField::Project));
        assert_eq!(state.view_mode, TasksViewMode::List);
        assert!(state.filters.is_empty());
    }

    #[test]
    fn filter_set_encoding_round_trips_through_persist_and_restore() {
        let state = TasksPlaceState {
            group_by: None,
            view_mode: TasksViewMode::Board,
            filters: vec![
                predicate(TaskField::Status, "In Progress"),
                predicate(TaskField::Label, "bug"),
            ],
            recent_filter_sets: vec![
                vec![predicate(TaskField::Priority, "high")],
                vec![
                    predicate(TaskField::Repo, "switchbard"),
                    predicate(TaskField::Assignee, "ben"),
                ],
            ],
            ..TasksPlaceState::default()
        };

        let mut ui = UiConfig::default();
        state.persist(&mut ui);
        let restored = TasksPlaceState::restore(&ui);

        assert_eq!(restored.group_by, None, "ungrouped round-trips as None");
        assert_eq!(restored.view_mode, TasksViewMode::Board);
        assert_eq!(restored.filters, state.filters);
        assert_eq!(restored.recent_filter_sets, state.recent_filter_sets);
    }

    #[test]
    fn remember_recent_deduplicates_and_caps() {
        let mut state = TasksPlaceState::default();
        let a = vec![predicate(TaskField::Status, "To Do")];
        let b = vec![predicate(TaskField::Status, "Done")];
        for _ in 0..(RECENT_CAP + 3) {
            state.remember_recent(a.clone());
            state.remember_recent(b.clone());
        }
        assert_eq!(state.recent_filter_sets.len(), RECENT_CAP.min(2));
        assert_eq!(state.recent_filter_sets[0], b);
    }

    #[test]
    fn remembering_an_empty_set_is_a_no_op() {
        let mut state = TasksPlaceState::default();
        state.remember_recent(Vec::new());
        assert!(state.recent_filter_sets.is_empty());
    }

    #[test]
    fn a_value_containing_the_separator_characters_never_appears_in_real_task_data() {
        // Documents the encoding's actual safety margin rather than asserting
        // it: the separators are control characters (`\u{1}`..`\u{3}`), well
        // outside anything a task title/label/status can realistically hold.
        let value = "ordinary label";
        assert!(!value.contains(FIELD_VALUE_SEP));
        assert!(!value.contains(PREDICATE_SEP));
        assert!(!value.contains(SET_SEP));
    }
}
