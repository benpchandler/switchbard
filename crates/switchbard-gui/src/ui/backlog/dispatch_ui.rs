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
/// [`dispatch_state`] is this function plus a note lookup, so the two can
/// never rank a task differently.
///
/// **A live claim outranks a finished verdict.** `dispatching` is checked
/// first because it is the only label that describes something happening
/// *now*; `dispatched` and `dispatch-failed` describe a run that is over. A
/// task carrying both is a task whose previous attempt's label outlived its
/// re-claim, and in that state the truth is the live run.
///
/// This is the belt to `claim_task_for_dispatch`'s braces, and — unlike the
/// original ordering, which claimed to be exactly that while doing the
/// opposite (audit N4) — it actually holds. The claim's label strip is
/// best-effort by design (it must never abort a run that has already passed
/// the double-dispatch guard), so it *can* fail, leaving `dispatch-failed`
/// beside `dispatching`. Ranked the old way that reinstated the whole F4b
/// symptom: a healthy agent rendered as a red DISPATCH FAILED pill, lighting
/// the attention chip with a warning nothing could clear.
///
/// Among the terminal labels `dispatched` still outranks `dispatch-failed`:
/// those two only coexist across separate attempts, and a PR is the more
/// useful thing to surface.
pub(crate) fn dispatch_category(task: &BacklogTask) -> DispatchCategory {
    let has = |label: &str| task.labels.iter().any(|l| l == label);
    if has(DISPATCHING_LABEL) {
        DispatchCategory::InFlight
    } else if has(DISPATCHED_LABEL) {
        DispatchCategory::Dispatched
    } else if has(DISPATCH_FAILED_LABEL) {
        DispatchCategory::Failed
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
            pr_url: find_note_token(&task.implementation_notes, "Dispatch PR: "),
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
/// The whole remainder of the last line carrying `prefix`.
///
/// For prose values — `Dispatch failed: <reason>` is a sentence, and cutting
/// it at the first space would render "worktree already exists" as
/// "worktree".
fn find_note_suffix(notes: &str, prefix: &str) -> Option<String> {
    notes
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(prefix))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The **first whitespace-delimited token** after `prefix` on the last line
/// carrying it.
///
/// For values that are a single word — a URL. `release_as_dispatched` writes
/// `Dispatch PR: <url>`, but a human editing the task afterwards may append
/// prose to that same line: one real task reads `Dispatch PR: https://…/847
/// — follow-up fixing a review-blocked…`, and taking the rest of the line
/// made the whole paragraph the "URL". `render_outcome` passes that value as
/// both a hyperlink's label *and* its target, so the result was a wall of
/// blue text wrapped across the row and a link that opened nothing.
///
/// A URL cannot contain unescaped whitespace, so the first token is all of it
/// and anything after is commentary.
fn find_note_token(notes: &str, prefix: &str) -> Option<String> {
    notes
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(prefix))
        .and_then(|rest| rest.split_whitespace().next())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

/// Compact tinted chip for List rows / Board strips / Digest / Dispatches /
/// Command — the mock's `.chip.amber`/`.chip.warn` dispatch pill
/// (TASK-76 parity pass). `None` for `NotFlagged` — a row with nothing to
/// say about dispatch shouldn't render an empty chip. Text stays the
/// existing uppercase wording (`query_by_label("DISPATCHING")` etc. is load-
/// bearing across several test suites) — only the paint routine changes,
/// from a bare `ui.label` to `theme::painted_chip`'s tinted pill.
///
/// Queued/InFlight use [`theme::amber`], not [`theme::dispatch_accent`]:
/// the mock's every literal "dispatching"/"queued" chip is `.chip.amber`
/// (`--dispatch` is a same-valued but otherwise-unused token in the mock's
/// own CSS, not a chip class) — `dispatch_accent` equals `sky` in the light
/// palette, which read as a wrong blue chip here even though Digest's own
/// in-flight row (plain `StatusKind::Warn`) already rendered amber
/// correctly. `dispatch_accent` stays the right color for the sidebar/top-
/// bar ambient lamp, which is a dot, not a chip. Failed uses
/// [`theme::warn_orange`] to match the mock's `.chip.warn` "failed" chip —
/// `danger` is a button-fill role with no chip class in the mock at all.
pub(crate) fn render_dispatch_pill(ui: &mut egui::Ui, state: &DispatchState) {
    let (text, color) = match state {
        DispatchState::NotFlagged => return,
        DispatchState::Queued => ("QUEUED", theme::amber()),
        DispatchState::InFlight => ("DISPATCHING", theme::amber()),
        DispatchState::Dispatched { .. } => ("DISPATCHED", theme::green()),
        DispatchState::Failed { .. } => ("DISPATCH FAILED", theme::warn_orange()),
    };
    theme::painted_chip(ui, Some(theme::chip_tint(color)), color, text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchbard_core::{BacklogTaskSource, DISPATCHING_LABEL};

    fn task_labelled(labels: &[&str]) -> BacklogTask {
        BacklogTask {
            id: "TASK-1".to_string(),
            title: "Example".to_string(),
            status: "In Progress".to_string(),
            priority: "medium".to_string(),
            assignees: vec![],
            labels: labels.iter().map(|l| l.to_string()).collect(),
            dependencies: vec![],
            references: vec![],
            project: None,
            parent: None,
            created_date: None,
            updated_date: None,
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: "Dispatch failed: boom\nDispatch PR: https://example/pr/1"
                .to_string(),
            final_summary: String::new(),
            acceptance_criteria: vec![],
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: std::path::PathBuf::from("/repo/backlog/tasks/task-1.md"),
        }
    }

    /// A PR note with prose appended after the URL yields just the URL.
    ///
    /// Real data from the budget repo: `release_as_dispatched` writes
    /// `Dispatch PR: <url>`, and a human later appended a paragraph of review
    /// context to the same line. `render_outcome` uses this value as both a
    /// hyperlink's label and its target, so taking the rest of the line
    /// produced a wall of blue text *and* a link that opened nothing.
    #[test]
    fn a_pr_note_with_trailing_prose_yields_only_the_url() {
        let mut task = task_labelled(&["dispatched"]);
        task.implementation_notes =
            "Dispatch PR: https://github.com/o/r/pull/847 — follow-up fixing a review-blocked \
             HIGH-confirmed finding on PR #843"
                .to_string();
        match dispatch_state(&task) {
            DispatchState::Dispatched { pr_url } => assert_eq!(
                pr_url.as_deref(),
                Some("https://github.com/o/r/pull/847"),
                "the URL ends at the first whitespace; the rest is commentary"
            ),
            other => panic!("expected Dispatched, got {other:?}"),
        }
    }

    /// A failure *reason* is prose and keeps its whole line — the opposite
    /// rule from the URL above, which is why the two extractors are separate.
    #[test]
    fn a_failure_reason_keeps_its_whole_sentence() {
        let mut task = task_labelled(&["dispatch-failed"]);
        task.implementation_notes = "Dispatch failed: worktree already exists".to_string();
        match dispatch_state(&task) {
            DispatchState::Failed { reason } => assert_eq!(
                reason.as_deref(),
                Some("worktree already exists"),
                "a reason cut at the first space would report only 'worktree'"
            ),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// Audit N4. `claim_task_for_dispatch` strips the previous attempt's
    /// terminal labels, but that strip is best-effort — it must never abort a
    /// run that already passed the double-dispatch guard — so this ordering
    /// is the fallback when it fails. Ranked the other way, a live agent
    /// reports as Failed for its entire run and lights the attention chip
    /// with a warning nothing can clear.
    #[test]
    fn a_live_claim_outranks_a_stale_failure_label() {
        let task = task_labelled(&[DISPATCH_FAILED_LABEL, DISPATCHING_LABEL]);

        assert_eq!(dispatch_category(&task), DispatchCategory::InFlight);
        assert_eq!(dispatch_state(&task), DispatchState::InFlight);
    }

    /// The same for a task re-flagged after a *successful* run, which is the
    /// likelier sequence (open a PR, decide it needs another pass).
    #[test]
    fn a_live_claim_outranks_a_stale_dispatched_label() {
        let task = task_labelled(&[DISPATCHED_LABEL, DISPATCHING_LABEL]);

        assert_eq!(dispatch_category(&task), DispatchCategory::InFlight);
        assert_eq!(dispatch_state(&task), DispatchState::InFlight);
    }

    /// Once the claim is released the terminal verdict is the truth again —
    /// the rule is "a live claim wins", not "dispatching wins forever".
    #[test]
    fn a_released_task_reports_its_terminal_verdict() {
        assert_eq!(
            dispatch_category(&task_labelled(&[DISPATCH_FAILED_LABEL])),
            DispatchCategory::Failed
        );
        assert_eq!(
            dispatch_category(&task_labelled(&[DISPATCHED_LABEL])),
            DispatchCategory::Dispatched
        );
    }

    /// Across separate attempts a PR is the more useful thing to surface.
    #[test]
    fn a_pr_outranks_an_older_failure() {
        let task = task_labelled(&[DISPATCH_FAILED_LABEL, DISPATCHED_LABEL]);

        assert_eq!(dispatch_category(&task), DispatchCategory::Dispatched);
    }

    #[test]
    fn the_ladders_bottom_rungs_are_unchanged() {
        assert_eq!(
            dispatch_category(&task_labelled(&[DISPATCH_LABEL])),
            DispatchCategory::Queued
        );
        assert_eq!(
            dispatch_category(&task_labelled(&["hub"])),
            DispatchCategory::NotFlagged
        );
    }

    /// `dispatch_state` is `dispatch_category` plus a note lookup, so the two
    /// cannot rank a task differently — pinned because they are consumed by
    /// different surfaces (top bar vs. detail rail) and a future edit to one
    /// would otherwise silently diverge.
    #[test]
    fn state_and_category_agree_on_every_label_combination() {
        for labels in [
            vec![],
            vec![DISPATCH_LABEL],
            vec![DISPATCHING_LABEL],
            vec![DISPATCHED_LABEL],
            vec![DISPATCH_FAILED_LABEL],
            vec![DISPATCHING_LABEL, DISPATCH_FAILED_LABEL],
            vec![DISPATCHING_LABEL, DISPATCHED_LABEL],
            vec![DISPATCHED_LABEL, DISPATCH_FAILED_LABEL],
            vec![DISPATCH_LABEL, DISPATCHING_LABEL],
        ] {
            let task = task_labelled(&labels);
            let expected = match dispatch_category(&task) {
                DispatchCategory::NotFlagged => DispatchState::NotFlagged,
                DispatchCategory::Queued => DispatchState::Queued,
                DispatchCategory::InFlight => DispatchState::InFlight,
                DispatchCategory::Dispatched => DispatchState::Dispatched {
                    pr_url: Some("https://example/pr/1".to_string()),
                },
                DispatchCategory::Failed => DispatchState::Failed {
                    reason: Some("boom".to_string()),
                },
            };
            assert_eq!(dispatch_state(&task), expected, "labels {labels:?}");
        }
    }
}
