//! Backlog.md task data: reading a project's tasks off disk (`parse`),
//! shelling out to the `backlog` CLI to mutate them (`mutations`), the
//! native write layer that will replace those shell-outs (`write` — see the
//! trajectory doc's *Backlog format fork* entry), and the struct/enum shapes
//! shared by all of them (`types`).
//!
//! Split from a single 1000+ line `backlog.rs` into modules by concern; the
//! `parse`/`mutations`/`types` names were already part of this module's
//! public API (see the doc on each submodule) — that split was
//! lift-and-shift, not a redesign.

mod mutations;
mod parse;
pub mod status_config;
mod types;
mod write;

pub use mutations::{
    append_backlog_notes, archive_backlog_task, complete_backlog_task, create_backlog_task,
    edit_backlog_task, set_backlog_acceptance_checked, set_backlog_dod_checked, set_backlog_label,
    swap_backlog_label,
};
pub use parse::{
    backlog_cli_path, body_round_trips, is_backlog_project, load_backlog_project,
    parse_backlog_day, parse_created_task_id, task_file_round_trips,
};
pub use types::{
    assignable_statuses, missing_standard_statuses, ordered_status_vocabulary,
    BacklogChecklistItem, BacklogProject, BacklogTask, BacklogTaskPatch, BacklogTaskSource,
    NewBacklogTask, BACKLOG_PRIORITIES, BACKLOG_STATUSES, CANONICAL_STATUS_ORDER,
    STANDARD_STATUSES,
};
pub use write::{
    append_task_acceptance_criteria, append_task_notes, replace_task_section,
    set_task_checklist_item, set_task_label, set_task_list_field, set_task_milestone,
    set_task_priority, set_task_status, set_task_title, write_new_task_file, TaskChecklist,
    TaskListField, TaskSection, WriteOutcome,
};
