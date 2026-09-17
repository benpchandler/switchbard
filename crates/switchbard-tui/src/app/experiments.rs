//! Owner-paced feature decisions and the request to use an installed update.

use super::{App, Mode, Pane};
use crate::experiments::{catalog, ExperimentDecision, ExperimentReview};
use crate::page::Page;
use crate::picker::{Payload, PickOption, PickerPurpose};

impl App {
    pub(super) fn open_experiments(&mut self) {
        let warning = self.experiments.reload();
        let mut options = catalog()
            .iter()
            .map(|spec| {
                let state = self.experiments.state(spec.id);
                let enabled = if state.enabled { "on" } else { "off" };
                let review = match state.review {
                    ExperimentReview::Unreviewed => "to try",
                    ExperimentReview::Kept => "kept",
                    ExperimentReview::RemovalRequested => "removal requested",
                };
                PickOption::numbered(
                    format!("{} · {enabled} · {review}", spec.title),
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
        self.status = format!("{} · {}", spec.task, spec.description);
    }

    pub(super) fn decide_experiment(&mut self, id: &str, decision: ExperimentDecision) {
        match self.experiments.set_decision(id, decision) {
            Ok(()) => {
                self.open_experiments();
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

    pub(super) fn request_update(&mut self) {
        self.picker = None;
        self.picker_parents.clear();
        self.mode = Mode::Browse;
        self.update_requested = true;
        self.status = "Checking the installed update".into();
    }

    pub fn experiment_notice(&self) -> Option<String> {
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
