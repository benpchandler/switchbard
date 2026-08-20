//! Dispatch state derivation shared by the detail pane and the List/Board
//! row markers. `switchbard_core::dispatch` is the system of record for this
//! state — a task's own label (`dispatch` / `dispatching` / `dispatched` /
//! `dispatch-failed`) *is* the state machine, and its notes carry the PR
//! link or failure reason (`"Dispatch PR: <url>"` / `"Dispatch failed:
//! <reason>"`, appended verbatim by `dispatch::release_as_dispatched`/
//! `release_as_failed`). This module only reads and presents that; it
//! never writes it — the actual pipeline runs on `workers::spawn_dispatch`.

use crate::ui::theme;
use eframe::egui;
use switchbard_core::{
    BacklogTask, DISPATCHED_LABEL, DISPATCHING_LABEL, DISPATCH_FAILED_LABEL, DISPATCH_LABEL,
};

/// Which rung of the label ladder a task is on, with no notes parsed.
///
/// Split out of [`DispatchState`] for the top bar: the chip and tab badge
/// only ever need to *count* by category, and running
/// [`dispatch_state`] to get one would allocate a `String` per finished task
/// (the PR link / failure reason) on every frame of every tab, for text
/// nothing in the top bar renders. This is the same ladder, decided by
/// labels alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DispatchCategory {
    NotFlagged,
    Queued,
    InFlight,
    Dispatched,
    Failed,
}

/// The label ladder, and the single place its precedence is written down.
///
/// Order matters and is not arbitrary: a task carries several of these labels
/// across its life, and the *most recent outcome* has to win. `dispatch_one`
/// clears the previous attempt's terminal labels as it claims (see
/// `switchbard_core::dispatch::claim_task_for_dispatch`), so a re-flagged task
/// no longer reports its old verdict over a live run — but the ordering here
/// stays the belt to that fix's braces.
pub(crate) fn dispatch_category(task: &BacklogTask) -> DispatchCategory {
    let has = |label: &str| task.labels.iter().any(|l| l == label);
    if has(DISPATCHED_LABEL) {
        DispatchCategory::Dispatched
    } else if has(DISPATCH_FAILED_LABEL) {
        DispatchCategory::Failed
    } else if has(DISPATCHING_LABEL) {
        DispatchCategory::InFlight
    } else if has(DISPATCH_LABEL) {
        DispatchCategory::Queued
    } else {
        DispatchCategory::NotFlagged
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DispatchState {
    /// No dispatch label at all — the normal state for a task nobody has
    /// opted in.
    NotFlagged,
    /// Labeled `dispatch`: queued, not yet claimed by the worker.
    Queued,
    /// Labeled `dispatching`: the worker has claimed it and a headless
    /// `claude -p` run is (or was, if the app restarted mid-run) in flight.
    InFlight,
    Dispatched {
        pr_url: Option<String>,
    },
    Failed {
        reason: Option<String>,
    },
}

/// Derive the current state from `task.labels` + `task.implementation_notes`
/// — see the module doc for why those are authoritative rather than any
/// state this app tracks itself.
pub(crate) fn dispatch_state(task: &BacklogTask) -> DispatchState {
    match dispatch_category(task) {
        DispatchCategory::Dispatched => DispatchState::Dispatched {
            pr_url: find_note_suffix(&task.implementation_notes, "Dispatch PR: "),
        },
        DispatchCategory::Failed => DispatchState::Failed {
            reason: find_note_suffix(&task.implementation_notes, "Dispatch failed: "),
        },
        DispatchCategory::InFlight => DispatchState::InFlight,
        DispatchCategory::Queued => DispatchState::Queued,
        DispatchCategory::NotFlagged => DispatchState::NotFlagged,
    }
}

/// The text after the last line starting with `prefix` in `notes` — notes
/// are append-only, so the *last* match is the most recent dispatch attempt
/// if a task was ever re-flagged after a prior failure.
fn find_note_suffix(notes: &str, prefix: &str) -> Option<String> {
    notes
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(prefix))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Compact pill for List rows / Board strips. `None` for `NotFlagged` — a
/// row with nothing to say about dispatch shouldn't render an empty pill.
pub(crate) fn render_dispatch_pill(ui: &mut egui::Ui, state: &DispatchState) {
    let (text, color) = match state {
        DispatchState::NotFlagged => return,
        DispatchState::Queued => ("QUEUED", theme::dispatch_accent()),
        DispatchState::InFlight => ("DISPATCHING", theme::dispatch_accent()),
        DispatchState::Dispatched { .. } => ("DISPATCHED", theme::green()),
        DispatchState::Failed { .. } => ("DISPATCH FAILED", theme::danger()),
    };
    ui.label(egui::RichText::new(text).small().strong().color(color));
}
