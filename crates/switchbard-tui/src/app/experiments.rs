//! Owner-paced feature decisions and the request to use an installed update.

use super::{App, Mode, Pane};
use crate::experiments::{catalog, ExperimentDecision};
use crate::page::Page;
use crate::picker::{Payload, PickOption, PickerPurpose};

impl App {
    pub(super) fn open_experiments(&mut self) {
        let previous_rows = self.detail_rows();
        let warning = self.experiments.reload();
        self.reconcile_experiment_detail_rows(previous_rows);
        let mut options = catalog()
            .iter()
            .map(|spec| {
                PickOption::numbered(
                    format!("E{:03} · {}", spec.number, spec.title),
                    Payload::Experiment(spec.id.to_string()),
                )
            })
            .collect::<Vec<_>>();
        if self.update_available {
            options.push(PickOption::keyed('u', "Update now", Payload::Update));
        }
        self.open_picker(PickerPurpose::Experiments, options);
        self.status = warning.unwrap_or_else(|| {
            if catalog().is_empty() {
                "No experiments in this build".into()
            } else {
                "Choose a feature · decisions apply in every repo".into()
            }
        });
    }

    pub(super) fn open_experiment(&mut self, id: &str) {
        let Some(spec) = catalog().iter().find(|spec| spec.id == id) else {
            self.fail("That experiment is unavailable in this build".into());
            return;
        };
        let state = self.experiments.state(id);
        let toggle = if state.enabled {
            ('d', "Disable", ExperimentDecision::Disable)
        } else {
            ('e', "Enable", ExperimentDecision::Enable)
        };
        let options = [
            toggle,
            ('a', "Keep · enable and accept", ExperimentDecision::Keep),
            (
                'r',
                "Remove · disable and request removal",
                ExperimentDecision::Remove,
            ),
        ]
        .into_iter()
        .map(|(key, label, decision)| {
            PickOption::keyed(key, label, Payload::ExperimentDecision(decision))
        })
        .collect();
        self.open_picker(PickerPurpose::Experiment(id.to_string()), options);
        self.status = format!("E{:03} · {} · {}", spec.number, spec.task, spec.description);
    }

    pub(super) fn decide_experiment(&mut self, id: &str, decision: ExperimentDecision) {
        let previous_rows = self.detail_rows();
        match self.experiments.set_decision(id, decision) {
            Ok(()) => {
                self.open_experiments();
                if let Some(picker) = self.picker.as_mut() {
                    picker.selected = picker.options.iter().position(|option| {
                        matches!(&option.payload, Payload::Experiment(candidate) if candidate == id)
                    }).unwrap_or(0);
                }
                self.reconcile_experiment_detail_rows(previous_rows);
                self.status = match decision {
                    ExperimentDecision::Enable => "Enabled · try it now",
                    ExperimentDecision::Disable => "Disabled",
                    ExperimentDecision::Keep => "Kept and enabled · code cleanup can follow later",
                    ExperimentDecision::Remove => {
                        "Disabled · removal requested; code is still present"
                    }
                }
                .into();
                self.telemetry
                    .record("experiment", format!("{id} {decision:?}"));
            }
            Err(error) => {
                self.open_experiment(id);
                self.fail(format!("Decision not saved: {error:#}"));
            }
        }
    }

    fn reconcile_experiment_detail_rows(&mut self, previous: Vec<crate::detail_pane::FieldRow>) {
        let rows = self.detail_rows();
        if previous == rows {
            return;
        }
        self.detail_cursor = previous
            .get(self.detail_cursor)
            .and_then(|selected| rows.iter().position(|row| row == selected))
            .unwrap_or(0);
        self.detail_scroll = 0;
        self.detail_scroll_anchor = None;
    }

    pub(super) fn handle_experiment_key(&mut self, event: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;
        let Some(picker) = self.picker.as_ref().filter(|picker| {
            picker.purpose == PickerPurpose::Experiments
                && picker.typed.is_empty()
                && picker.number.is_empty()
                && event.modifiers.is_empty()
        }) else {
            return false;
        };
        let Some(Payload::Experiment(id)) = picker.highlighted().map(|option| option.payload)
        else {
            return false;
        };
        let decision = match event.code {
            KeyCode::Char(' ') => {
                if self.experiments.is_enabled(&id) {
                    ExperimentDecision::Disable
                } else {
                    ExperimentDecision::Enable
                }
            }
            KeyCode::Char('a') => ExperimentDecision::Keep,
            KeyCode::Char('r') => ExperimentDecision::Remove,
            _ => return false,
        };
        self.decide_experiment(&id, decision);
        true
    }

    pub(super) fn handle_experiment_mouse(&mut self, event: crossterm::event::MouseEvent) -> bool {
        use crossterm::event::{MouseButton, MouseEventKind};
        if self.mode != Mode::PickValue
            || !self
                .picker
                .as_ref()
                .is_some_and(|picker| picker.purpose == PickerPurpose::Experiments)
        {
            return false;
        }
        if event.kind == MouseEventKind::Down(MouseButton::Left) {
            let position = ratatui::layout::Position::new(event.column, event.row);
            if let Some(hit) = self
                .experiment_hits
                .iter()
                .find(|hit| hit.area.contains(position))
                .cloned()
            {
                match hit.decision {
                    Some(decision) => self.decide_experiment(&hit.id, decision),
                    None => self.open_experiment(&hit.id),
                }
            }
        }
        true
    }

    pub(super) fn request_update(&mut self) {
        self.picker = None;
        self.picker_parents.clear();
        self.mode = Mode::Browse;
        self.update_requested = true;
        self.status = "Checking the installed update".into();
    }

    /// Whether a modal confirmation owns the screen. Its full identity must
    /// fit before it can fire (`submit_agent_kill`), and the notification row
    /// is taken out of the body to make it, so an advisory banner must yield
    /// that row rather than shrink a confirmation into refusing itself.
    fn confirmation_open(&self) -> bool {
        self.picker.as_ref().is_some_and(|picker| {
            matches!(
                picker.purpose,
                crate::picker::PickerPurpose::AgentKill
                    | crate::picker::PickerPurpose::TaskCancel
                    | crate::picker::PickerPurpose::Merge
            )
        })
    }

    pub fn experiment_notice(&self) -> Option<String> {
        if self.confirmation_open() {
            return None;
        }
        if self.update_available {
            return Some(":update · New build ready".into());
        }
        match self.experiments.unreviewed_count() {
            0 => None,
            1 => Some(":experiments · 1 feature to try".into()),
            count => Some(format!(":experiments · {count} features to try")),
        }
    }

    pub(super) fn open_experimental_task_editor(&mut self) {
        if self.page != Page::Tasks || !self.experiments.is_enabled("task-edit-shortcut") {
            self.fail("Enable Quick task editing in Experiments first".into());
            return;
        }
        if self.defer_task_storage() || self.selected_task().is_none() {
            return;
        }
        self.pane = Pane::Detail;
        self.detail_read_focus = false;
        self.enter_detail_focus();
    }
}
