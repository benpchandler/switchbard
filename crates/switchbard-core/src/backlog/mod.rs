//! Backlog.md task data: reading a project's tasks off disk (`parse`),
//! shelling out to the `backlog` CLI to mutate them (`mutations`), and the
//! struct/enum shapes shared by both (`types`).
//!
//! Split from a single 1000+ line `backlog.rs` into these three modules by
//! concern; every name below was already part of this module's public API
//! (see the doc on each submodule) — this is lift-and-shift, not a redesign.

mod mutations;
mod parse;
mod types;

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
    ordered_status_vocabulary, BacklogChecklistItem, BacklogProject, BacklogTask, BacklogTaskPatch,
    BacklogTaskSource, NewBacklogTask, BACKLOG_PRIORITIES, BACKLOG_STATUSES,
    CANONICAL_STATUS_ORDER, STANDARD_STATUSES,
};
