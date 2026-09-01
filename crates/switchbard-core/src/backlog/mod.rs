//! Backlog-format task data, natively owned since the format fork (the
//! trajectory doc's *Backlog format fork* entry, owner-approved 2026-08-28):
//! reading a project's tasks off disk (`parse`), the surgical write layer
//! (`write`), id allocation (`allocate`), the caller-facing by-task-id
//! facade over both (`mutations`), and the struct/enum shapes shared by all
//! of them (`types`). No mutation shells out to the `backlog` CLI any more;
//! the files stay Backlog.md-compatible on disk.

mod allocate;
mod goals;
mod hierarchy;
mod mutations;
mod parse;
mod ranking;
pub mod status_config;
mod types;
mod write;

pub use allocate::{create_task_allocating_id, next_task_id, ACTIVE_BRANCH_DAYS};

pub use goals::{
    check_in_goal, create_goal, roll_goals, GoalCheckIn, GoalDef, GoalMeasure, GoalWeek, NewGoal,
};

pub use hierarchy::{
    create_initiative_def, create_project_def, edit_initiative_def, edit_project_def,
    InitiativeDef, InitiativeDefPatch, NewInitiativeDef, NewProjectDef, ProjectDef,
    ProjectDefPatch, DEFAULT_PROJECT_STATUS, PROJECT_STATUSES,
};

pub use mutations::{
    append_backlog_notes, archive_backlog_task, complete_backlog_task, create_backlog_task,
    edit_backlog_task, set_backlog_acceptance_checked, set_backlog_dod_checked,
    set_backlog_final_summary, set_backlog_label, swap_backlog_label,
};
pub use parse::{
    body_round_trips, is_backlog_repo, load_backlog_repo, parse_backlog_day, task_file_round_trips,
};
pub use ranking::{
    expedite_task, rank_project, rank_task, unexpedite_task, unrank_project, unrank_task,
    RankPlacement, RepoRanking,
};
pub use types::{
    assignable_statuses, missing_standard_statuses, ordered_status_vocabulary,
    BacklogChecklistItem, BacklogRepo, BacklogTask, BacklogTaskPatch, BacklogTaskSource,
    NewBacklogTask, BACKLOG_PRIORITIES, BACKLOG_STATUSES, CANONICAL_STATUS_ORDER,
    STANDARD_STATUSES,
};
pub use write::{
    append_task_acceptance_criteria, append_task_notes, replace_task_section,
    set_task_checklist_item, set_task_label, set_task_list_field, set_task_priority,
    set_task_project, set_task_status, set_task_title, swap_task_label, write_new_task_file,
    TaskChecklist, TaskListField, TaskSection, WriteOutcome,
};
