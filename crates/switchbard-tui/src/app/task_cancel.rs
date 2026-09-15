//! Explicit cancellation of one selected task; never terminates an agent.
use super::{App, Mode};
use crate::picker::{Payload, PickOption, PickerPurpose};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use switchbard_core::PreparedTaskCancellation;

#[derive(Default)]
pub struct CancelFlow {
    prepared: Option<PreparedTaskCancellation>,
    pub confirmation_visible: bool,
}

impl CancelFlow {
    pub fn confirmation_lines(&self) -> Vec<String> {
        self.prepared
            .as_ref()
            .map(|task| {
                vec![
                    format!("Cancel {}?", task.id()),
                    task.title().to_string(),
                    "Only this task is canceled. Subtasks remain unchanged.".into(),
                    "The record is archived. Running agents and claims remain unchanged.".into(),
                ]
            })
            .unwrap_or_default()
    }
}

impl App {
    pub(super) fn open_task_cancellation(&mut self) {
        let Some(id) = self.selected_task().map(|task| task.id.clone()) else {
            return;
        };
        match switchbard_core::prepare_task_cancellation(&self.repo_root, &id) {
            Ok(prepared) => {
                self.task_cancel.prepared = Some(prepared);
                self.task_cancel.confirmation_visible = false;
                self.open_picker(
                    PickerPurpose::TaskCancel,
                    vec![
                        PickOption::keyed('k', "Keep task", Payload::KeepTask),
                        PickOption::keyed('c', "Confirm cancellation", Payload::ConfirmTaskCancel),
                    ],
                );
            }
            Err(error) => self.fail(format!("Cancellation unavailable: {error}")),
        }
    }

    pub(super) fn handle_task_cancel_key(&mut self, event: KeyEvent) {
        if event.kind != KeyEventKind::Press {
            return;
        }
        match event.code {
            KeyCode::Char('c') => self.confirm_task_cancellation(),
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('k') => {
                self.close_task_cancellation();
                self.status = "Task kept".into();
            }
            _ => {}
        }
    }

    fn close_task_cancellation(&mut self) {
        self.task_cancel = CancelFlow::default();
        self.picker = None;
        self.picker_parents.clear();
        self.mode = Mode::Browse;
    }

    fn confirm_task_cancellation(&mut self) {
        if !self.task_cancel.confirmation_visible {
            self.status = "Show the full cancellation confirmation before confirming".into();
            return;
        }
        let Some(prepared) = self.task_cancel.prepared.take() else {
            return;
        };
        let current = self
            .selected_task()
            .is_some_and(|task| task.id == prepared.id());
        self.close_task_cancellation();
        if !current || self.page != crate::page::Page::Tasks {
            self.fail("Selected task changed; reopen cancellation".into());
            return;
        }
        match switchbard_core::cancel_task_expected(&self.repo_root, prepared) {
            Ok(message) => {
                self.reload_tasks();
                self.telemetry.record("action", &message);
                self.status = message;
            }
            Err(error) => self.fail(format!("Task not canceled: {error}")),
        }
    }
}
