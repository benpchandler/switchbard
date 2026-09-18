//! One off-thread merge preparation/submission; the shared picker owns explicit confirmation.
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use switchbard_core::{
    PrMergeMethod, PrMergeOutcome, PrMergePreparation, PrMergeResult, PreparedPrMerge,
};

use super::{App, Mode};
use crate::{
    config::{Action, KeyChord},
    page::Page,
    picker::{Payload, PickOption, PickerPurpose},
};

enum Reply {
    Prepared(Result<PrMergePreparation, String>),
    Submitted(PrMergeResult),
}

/// A bulk merge in flight (TASK-250): the marked PRs still to merge, in list
/// order, and the one method the human confirmed for all of them. Each next
/// PR is re-prepared after the previous merge lands and refreshes the list,
/// and only a plainly CLEAN one is submitted without asking again.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MergeQueue {
    /// (id, number) of every PR not yet started, front first.
    pub remaining: VecDeque<(String, u64)>,
    /// Confirmed once, applied to every PR in the queue.
    pub method: Option<PrMergeMethod>,
    pub total: usize,
    pub merged: usize,
}

impl MergeQueue {
    fn summary(&self) -> String {
        let numbers: Vec<String> = self
            .remaining
            .iter()
            .map(|(_, number)| format!("#{number}"))
            .collect();
        format!(
            "Bulk merge: {} PRs in list order; after this one: {}",
            self.total,
            if numbers.is_empty() {
                "none".to_string()
            } else {
                numbers.join(", ")
            }
        )
    }
}

#[derive(Default)]
pub struct MergeFlow {
    pending: Option<Receiver<Reply>>,
    prepared: Option<PreparedPrMerge>,
    target: Option<(String, String)>,
    target_number: Option<u64>,
    dismissed: bool,
    submitting: bool,
    refresh_after_result: bool,
    /// Set when a confirmed bulk merge should continue once the list refresh lands.
    advance_after_refresh: bool,
    pub queue: Option<MergeQueue>,
    pub confirmation_visible: bool,
    pub last_result: Option<PrMergeResult>,
}

impl MergeFlow {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn is_submitting(&self) -> bool {
        self.submitting
    }
    pub fn ongoing(&self) -> Option<&'static str> {
        if self.submitting {
            Some("Merge submitting; waiting for verified result")
        } else if self.pending.is_some() && !self.dismissed {
            Some("Preparing merge; Esc cancels")
        } else if self.advance_after_refresh {
            Some("Bulk merge: waiting for the list to refresh before the next PR; Esc stops")
        } else {
            None
        }
    }
    /// The queue is live while there are PRs left after the current one.
    pub fn queue_active(&self) -> bool {
        self.queue.as_ref().is_some_and(|q| !q.remaining.is_empty())
    }
    pub fn confirmation_lines(&self) -> Vec<String> {
        let Some(prepared) = &self.prepared else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        if let Some(queue) = &self.queue {
            lines.push(queue.summary());
        }
        lines.extend([
            format!("{} #{}", prepared.repository(), prepared.number()),
            prepared.title().to_string(),
            format!("Head: {}", prepared.head_oid()),
            format!("Base: {} ({})", prepared.base_ref(), prepared.base_oid()),
            format!("Signed in as: {}", prepared.viewer()),
            prepared.url().to_string(),
        ]);
        // A merge GitHub allows but does not call green is confirmed with the
        // reason in front of the choice, never behind it (TASK-171).
        lines.extend(prepared.readiness_caveat());
        if self.queue.is_some() {
            lines.push(
                "Each following PR is re-checked after the previous merge lands; one that is not CLEAN stops the queue."
                    .into(),
            );
            lines.push("Choose a method to confirm it for every PR in the queue. Task status stays unchanged.".into());
        } else {
            lines
                .push("Choose a method to confirm this merge. Task status stays unchanged.".into());
        }
        lines
    }

    /// What to tell `PullRequests` to expect, if anything (TASK-204). Only a
    /// confirmed merge on the PR this flow targeted should start fast-poll
    /// tracking; a rejection or an unknown outcome leaves GitHub's own state
    /// alone rather than guessing at it.
    fn merge_expectation(&self, result: &PrMergeResult) -> Option<(String, String)> {
        if result.outcome != PrMergeOutcome::Confirmed {
            return None;
        }
        self.target.clone()
    }
}

impl App {
    pub(super) fn open_pr_merge(&mut self) {
        // Say so rather than swallowing the key. Preparation can take a
        // while when GitHub is slow, and Esc dismisses the confirmation
        // without ending the request behind it - so a silent no-op here
        // reads as `m` being broken for as long as that request runs.
        if self.pr_merge.is_submitting() {
            self.status = "Merge submitting; wait for the result".into();
            return;
        }
        if self.pr_merge.is_pending() {
            self.status = "Still checking the last merge; try again in a moment".into();
            return;
        }
        if self.pr_merge.prepared.is_some() {
            return;
        }
        if !self.pull_requests.selection.marked.is_empty() {
            let ids = self.pull_requests.marked_in_view_order();
            let Some(first) = ids.first().cloned() else {
                self.status =
                    "Marked PRs are filtered out of view; clear the filter or the marks".into();
                return;
            };
            let numbered: Vec<(String, u64)> = ids
                .iter()
                .filter_map(|id| {
                    let snapshot = self.pull_requests.snapshot.as_ref()?;
                    let row = snapshot.rows.iter().find(|row| &row.id == id)?;
                    Some((row.id.clone(), row.number))
                })
                .collect();
            self.pr_merge.queue = Some(MergeQueue {
                total: numbered.len(),
                remaining: numbered.into_iter().skip(1).collect(),
                method: None,
                merged: 0,
            });
            self.pull_requests.select_id(&first);
        } else {
            self.pr_merge.queue = None;
        }
        let Some(row) = self.pull_requests.row().cloned() else {
            self.status = "No PR selected".into();
            return;
        };
        self.prepare_pr_merge_row(row);
    }

    pub(super) fn toggle_pr_mark(&mut self) {
        match self.pull_requests.toggle_mark() {
            Ok(true) => {
                let count = self.pull_requests.selection.marked.len();
                self.status =
                    format!("Marked for merge ({count}); m merges all in list order, Esc clears");
                self.pull_requests.step(1);
            }
            Ok(false) => {
                let count = self.pull_requests.selection.marked.len();
                self.status = format!("Unmarked ({count} marked)");
            }
            Err(reason) => self.status = reason.into(),
        }
    }

    /// Word-processor-style shift-up/shift-down range select: extend (or
    /// contract) the bulk-merge mark from the sweep's anchor to the cursor.
    pub(super) fn extend_pr_mark(&mut self, delta: isize) {
        let count = self.pull_requests.extend_mark(delta);
        self.status = format!("Marked for merge ({count}); m merges all in list order, Esc clears");
    }

    /// Stop a bulk merge before its next PR starts. Never interrupts a merge
    /// that is already submitting; that one finishes and reports on its own.
    pub(super) fn cancel_pr_merge_queue(&mut self, reason: &str) {
        let Some(queue) = self.pr_merge.queue.take() else {
            self.pr_merge.advance_after_refresh = false;
            return;
        };
        self.pr_merge.advance_after_refresh = false;
        if queue.method.is_some() || queue.merged > 0 {
            let left: Vec<String> = queue
                .remaining
                .iter()
                .map(|(_, number)| format!("#{number}"))
                .collect();
            let line = format!(
                "Bulk merge stopped ({reason}): {} of {} merged; not merged: {}",
                queue.merged,
                queue.total,
                if left.is_empty() {
                    "none".to_string()
                } else {
                    left.join(", ")
                }
            );
            self.pull_requests.notifications.push(line.clone());
            self.status = line;
        }
    }

    fn prepare_pr_merge_row(&mut self, row: switchbard_core::PrListRow) {
        let Some(snapshot) = self.pull_requests.snapshot.clone() else {
            self.status = "Merge unavailable: refresh the PR list first".into();
            self.pr_merge.queue = None;
            return;
        };
        let root = self.repo_root.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        self.pr_merge.target = Some((row.id.clone(), row.head_oid.clone()));
        self.pr_merge.target_number = Some(row.number);
        self.pr_merge.dismissed = false;
        match std::thread::Builder::new()
            .name("sbt-merge-prepare".into())
            .spawn(move || {
                if tx
                    .send(Reply::Prepared(switchbard_core::prepare_pr_merge(
                        &root, &snapshot, &row,
                    )))
                    .is_err()
                { /* app closed */ }
            }) {
            Ok(_) => self.pr_merge.pending = Some(rx),
            Err(error) => self.status = format!("Could not prepare merge: {error}"),
        }
    }

    pub(super) fn cancel_pr_merge(&mut self) {
        if self.pr_merge.submitting {
            return;
        }
        self.pr_merge.dismissed = true;
        self.pr_merge.prepared = None;
        self.pr_merge.confirmation_visible = false;
        // A queue nobody confirmed a method for is just the marks; drop it.
        if self
            .pr_merge
            .queue
            .as_ref()
            .is_some_and(|q| q.method.is_none())
        {
            self.pr_merge.queue = None;
        }
        if self
            .picker
            .as_ref()
            .is_some_and(|p| p.purpose == PickerPurpose::Merge)
        {
            self.picker = None;
            self.mode = Mode::Browse;
        }
    }

    pub(super) fn merge_target_current(&self) -> bool {
        self.page == Page::PullRequests
            && self.pull_requests.row().is_some_and(|row| {
                self.pr_merge
                    .target
                    .as_ref()
                    .is_some_and(|(id, head)| id == &row.id && head == &row.head_oid)
            })
    }

    pub(super) fn tick_pr_merge(&mut self) {
        if self.pr_merge.refresh_after_result && !self.pull_requests.loading() {
            self.pr_merge.refresh_after_result = false;
            self.pull_requests.refresh(&self.repo_root);
        }
        if self.pr_merge.advance_after_refresh
            && !self.pull_requests.loading()
            && !self.pr_merge.refresh_after_result
            && self.pr_merge.pending.is_none()
        {
            self.pr_merge.advance_after_refresh = false;
            self.advance_merge_queue();
            return;
        }
        if !self.merge_target_current() {
            self.cancel_pr_merge();
        }
        let Some(rx) = &self.pr_merge.pending else {
            return;
        };
        match rx.try_recv() {
            Ok(reply) => {
                self.pr_merge.pending = None;
                match reply {
                    Reply::Prepared(result) if !self.pr_merge.dismissed => {
                        self.accept_merge_preparation(result)
                    }
                    Reply::Prepared(_) => {}
                    Reply::Submitted(result) => self.accept_merge_result(result),
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.pr_merge.pending = None;
                if self.pr_merge.submitting {
                    self.accept_merge_result(PrMergeResult { outcome: PrMergeOutcome::OutcomeUnknown,
                        message: "Merge worker stopped; inspect GitHub and merge receipts before retrying".into(), receipt_path: None });
                } else {
                    self.status = "Merge preparation stopped; try again".into();
                }
            }
        }
    }

    /// Start the next queued PR after the previous merge landed and the list
    /// refreshed. The cursor follows the queue so every guard that reads the
    /// selected row keeps meaning what it means for a single merge.
    fn advance_merge_queue(&mut self) {
        let Some(next) = self
            .pr_merge
            .queue
            .as_mut()
            .and_then(|queue| queue.remaining.pop_front())
        else {
            let merged = self.pr_merge.queue.take().map(|q| (q.merged, q.total));
            if let Some((merged, total)) = merged {
                self.status = format!("Bulk merge complete: {merged} of {total} merged");
                self.pull_requests.notifications.push(self.status.clone());
            }
            return;
        };
        if self.mode != Mode::Browse || self.page != Page::PullRequests {
            if let Some(queue) = self.pr_merge.queue.as_mut() {
                queue.remaining.push_front(next);
            }
            self.cancel_pr_merge_queue("left the PR list");
            return;
        }
        if !self.pull_requests.select_id(&next.0) {
            if let Some(queue) = self.pr_merge.queue.as_mut() {
                queue.remaining.push_front(next);
            }
            self.cancel_pr_merge_queue("next PR is no longer listed as open");
            return;
        }
        let Some(row) = self.pull_requests.row().cloned() else {
            self.cancel_pr_merge_queue("next PR is no longer listed");
            return;
        };
        self.prepare_pr_merge_row(row);
    }

    fn accept_merge_preparation(&mut self, result: Result<PrMergePreparation, String>) {
        let queued_method = self.pr_merge.queue.as_ref().and_then(|queue| queue.method);
        if let Some(method) = queued_method {
            match result {
                Ok(PrMergePreparation::Ready(prepared)) => {
                    if let Some(caveat) = prepared.readiness_caveat() {
                        self.cancel_pr_merge_queue(&format!("#{} {caveat}", prepared.number()));
                        return;
                    }
                    if self.mode != Mode::Browse || !self.merge_target_current() {
                        self.cancel_pr_merge_queue("selection changed");
                        return;
                    }
                    self.spawn_pr_merge_submit(prepared, method);
                }
                Ok(PrMergePreparation::Disabled(reason)) | Err(reason) => {
                    self.cancel_pr_merge_queue(&reason);
                }
            }
            return;
        }
        match result {
            Ok(PrMergePreparation::Ready(prepared)) => {
                // Never interrupt a filter, command, or other picker opened during preparation.
                if self.mode != Mode::Browse || !self.merge_target_current() {
                    return;
                }
                let mut options = vec![PickOption::numbered("Cancel", Payload::CancelMerge)];
                options.extend(prepared.methods().into_iter().map(|method| {
                    PickOption::numbered(
                        format!("Confirm {}", method.label()),
                        Payload::Merge(method),
                    )
                }));
                self.pr_merge.prepared = Some(prepared);
                self.open_picker(PickerPurpose::Merge, options);
            }
            Ok(PrMergePreparation::Disabled(reason)) | Err(reason) => {
                self.status = format!("Merge unavailable: {reason}");
                self.pull_requests.notifications.push(self.status.clone());
                self.pr_merge.queue = None;
            }
        }
    }

    pub(super) fn handle_merge_picker_key(&mut self, event: KeyEvent) {
        if event.kind != KeyEventKind::Press {
            return;
        }
        if self.config.keys.get(&KeyChord::from_event(&event)) == Some(&Action::Page) {
            self.cancel_pr_merge();
            self.apply(&Action::Page);
            return;
        }
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        match event.code {
            KeyCode::Esc => self.cancel_pr_merge(),
            KeyCode::Down | KeyCode::Char('j') => {
                picker.selected = (picker.selected + 1).min(picker.options.len().saturating_sub(1))
            }
            KeyCode::Up | KeyCode::Char('k') => picker.selected = picker.selected.saturating_sub(1),
            KeyCode::Enter => self.apply_picked_value(),
            KeyCode::Char(digit @ '1'..='4') => {
                let index = digit as usize - '1' as usize;
                if index < picker.options.len() {
                    picker.selected = index;
                    self.apply_picked_value();
                }
            }
            _ => {} // No type-ahead, keyed letters or unique-match auto-confirmation.
        }
    }

    pub(super) fn submit_pr_merge(&mut self, method: PrMergeMethod) {
        if self.pr_merge.is_pending()
            || !self.merge_target_current()
            || !self.pr_merge.confirmation_visible
        {
            self.cancel_pr_merge();
            self.status = "Merge not submitted: show the full confirmation and try again".into();
            return;
        }
        let Some(prepared) = self.pr_merge.prepared.take() else {
            return;
        };
        if let Some(queue) = self.pr_merge.queue.as_mut() {
            queue.method = Some(method);
        }
        self.spawn_pr_merge_submit(prepared, method);
    }

    fn spawn_pr_merge_submit(&mut self, prepared: PreparedPrMerge, method: PrMergeMethod) {
        let root = self.repo_root.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("sbt-merge-submit".into())
            .spawn(move || {
                if tx
                    .send(Reply::Submitted(switchbard_core::execute_pr_merge(
                        &root, prepared, method,
                    )))
                    .is_err()
                { /* receipt retained by core */ }
            }) {
            Ok(_) => {
                self.pr_merge.pending = Some(rx);
                self.pr_merge.submitting = true;
            }
            Err(error) => self.status = format!("Could not start merge: {error}"),
        }
    }

    /// Apply a completed core merge result without performing a GitHub write.
    pub fn accept_merge_result(&mut self, result: PrMergeResult) {
        self.pr_merge.submitting = false;
        self.status = match result.outcome {
            PrMergeOutcome::Confirmed => self.pr_merge.target_number.map_or_else(
                || "Merged PR".into(),
                |number| format!("Merged PR #{number}"),
            ),
            PrMergeOutcome::Rejected => format!("Merge rejected: {}", result.message),
            PrMergeOutcome::OutcomeUnknown => format!("Merge outcome unknown: {}", result.message),
        };
        self.telemetry.record("merge_result", &result.message);
        if let Some(path) = &result.receipt_path {
            self.telemetry
                .record("merge_receipt", path.display().to_string());
        }
        self.pull_requests.notifications.push(self.status.clone());
        if let Some((id, head_oid)) = self.pr_merge.merge_expectation(&result) {
            self.pull_requests
                .expect_merge(id, head_oid, Instant::now());
        }
        if let Some(queue) = self.pr_merge.queue.as_mut() {
            if result.outcome == PrMergeOutcome::Confirmed {
                queue.merged += 1;
                if let Some((id, _)) = &self.pr_merge.target {
                    self.pull_requests.selection.marked.remove(id);
                }
                self.pr_merge.advance_after_refresh = true;
            } else {
                self.pr_merge.last_result = Some(result);
                self.cancel_pr_merge_queue("merge did not confirm");
                if self.pull_requests.loading() {
                    self.pr_merge.refresh_after_result = true;
                } else {
                    self.pull_requests.refresh(&self.repo_root);
                }
                return;
            }
        }
        self.pr_merge.last_result = Some(result);
        if self.pull_requests.loading() {
            self.pr_merge.refresh_after_result = true;
        } else {
            self.pull_requests.refresh(&self.repo_root);
        }
    }

    pub(super) fn request_quit(&mut self) {
        if self.report.is_pending() {
            self.status = "Saving report; wait for the result before quitting".into();
        } else if self.agent_kill.is_submitting() {
            self.status = "Agent signal pending; wait for the result before quitting".into();
        } else if self.pr_merge.is_submitting() {
            self.status = "Merge submitting; wait for the result before quitting".into();
        } else {
            self.should_quit = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(outcome: PrMergeOutcome) -> PrMergeResult {
        PrMergeResult {
            outcome,
            message: "test".into(),
            receipt_path: None,
        }
    }

    fn flow_targeting(id: &str, head_oid: &str) -> MergeFlow {
        MergeFlow {
            target: Some((id.into(), head_oid.into())),
            ..Default::default()
        }
    }

    #[test]
    fn confirmed_outcome_expects_the_prepared_target() {
        let flow = flow_targeting("pr-1", "abc123");
        assert_eq!(
            flow.merge_expectation(&result(PrMergeOutcome::Confirmed)),
            Some(("pr-1".to_string(), "abc123".to_string()))
        );
    }

    #[test]
    fn rejected_and_unknown_outcomes_expect_nothing() {
        let flow = flow_targeting("pr-1", "abc123");
        assert_eq!(
            flow.merge_expectation(&result(PrMergeOutcome::Rejected)),
            None
        );
        assert_eq!(
            flow.merge_expectation(&result(PrMergeOutcome::OutcomeUnknown)),
            None
        );
    }

    #[test]
    fn confirmed_outcome_without_a_captured_target_expects_nothing() {
        let flow = MergeFlow::default();
        assert_eq!(
            flow.merge_expectation(&result(PrMergeOutcome::Confirmed)),
            None
        );
    }
}
