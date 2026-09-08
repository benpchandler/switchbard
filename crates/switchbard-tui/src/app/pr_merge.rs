//! One off-thread merge preparation/submission; the shared picker owns explicit confirmation.
use std::sync::mpsc::{self, Receiver, TryRecvError};

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

#[derive(Default)]
pub struct MergeFlow {
    pending: Option<Receiver<Reply>>,
    prepared: Option<PreparedPrMerge>,
    target: Option<(String, String)>,
    dismissed: bool,
    submitting: bool,
    refresh_after_result: bool,
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
        } else {
            None
        }
    }
    pub fn confirmation_lines(&self) -> Vec<String> {
        let Some(prepared) = &self.prepared else {
            return Vec::new();
        };
        let mut lines = vec![
            format!("{} #{}", prepared.repository(), prepared.number()),
            prepared.title().to_string(),
            format!("Head: {}", prepared.head_oid()),
            format!("Base: {} ({})", prepared.base_ref(), prepared.base_oid()),
            format!("Signed in as: {}", prepared.viewer()),
            prepared.url().to_string(),
        ];
        // A merge GitHub allows but does not call green is confirmed with the
        // reason in front of the choice, never behind it (TASK-171).
        lines.extend(prepared.readiness_caveat());
        lines.push("Choose a method to confirm this merge. Task status stays unchanged.".into());
        lines
    }
}

impl App {
    pub(super) fn open_pr_merge(&mut self) {
        if self.pr_merge.is_pending() || self.pr_merge.prepared.is_some() {
            return;
        }
        let Some(row) = self.pull_requests.row().cloned() else {
            self.status = "No PR selected".into();
            return;
        };
        let Some(snapshot) = self.pull_requests.snapshot.clone() else {
            self.status = "Merge unavailable: refresh the PR list first".into();
            return;
        };
        let root = self.repo_root.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        self.pr_merge.target = Some((row.id.clone(), row.head_oid.clone()));
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

    fn accept_merge_preparation(&mut self, result: Result<PrMergePreparation, String>) {
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

    fn accept_merge_result(&mut self, result: PrMergeResult) {
        self.pr_merge.submitting = false;
        let outcome = match result.outcome {
            PrMergeOutcome::Confirmed => "Merge confirmed",
            PrMergeOutcome::Rejected => "Merge rejected",
            PrMergeOutcome::OutcomeUnknown => "Merge outcome unknown",
        };
        self.status = format!("{outcome}: {}", result.message);
        if let Some(path) = &result.receipt_path {
            self.pull_requests
                .notifications
                .push(format!("Merge receipt: {}", path.display()));
        }
        self.pull_requests.notifications.push(self.status.clone());
        self.pr_merge.last_result = Some(result);
        if self.pull_requests.loading() {
            self.pr_merge.refresh_after_result = true;
        } else {
            self.pull_requests.refresh(&self.repo_root);
        }
    }

    pub(super) fn request_quit(&mut self) {
        if self.pr_merge.is_submitting() {
            self.status = "Merge submitting; wait for the result before quitting".into();
        } else {
            self.should_quit = true;
        }
    }
}
