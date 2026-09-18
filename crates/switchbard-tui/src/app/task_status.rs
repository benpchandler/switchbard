//! Status choices come from the repo vocabulary and write through the native editor.

use crate::picker::{PickOption, PickerPurpose};

use super::App;

impl App {
    pub(super) fn open_task_status_picker(&mut self) {
        if self.defer_task_storage() {
            return;
        }
        let Some(id) = self.selected_task().map(|task| task.id.clone()) else {
            self.status = "no task selected".to_string();
            return;
        };
        let repo = match switchbard_core::load_backlog_repo(&self.repo_root) {
            Ok(repo) => repo,
            Err(error) => {
                self.fail(format!("could not load statuses: {error}"));
                return;
            }
        };
        let options = switchbard_core::assignable_statuses(&repo)
            .into_iter()
            .filter(|status| {
                !status.eq_ignore_ascii_case("Canceled")
                    && !status.eq_ignore_ascii_case("Cancelled")
            })
            .map(|status| PickOption::text(status, 0))
            .collect::<Vec<_>>();
        if options.is_empty() {
            self.fail("no statuses configured for this repo".to_string());
            return;
        }
        self.open_picker(PickerPurpose::TaskStatus(id), options);
        self.status.clear();
    }

    /// The one write that sets a task's status, shared by the single-row and
    /// bulk paths so they can never disagree about what a status change is.
    fn write_status(&self, id: &str, status: &str) -> Result<(), String> {
        let patch = switchbard_core::BacklogTaskPatch {
            status: Some(status.to_string()),
            ..Default::default()
        };
        switchbard_core::edit_backlog_task(&self.repo_root, id, &patch)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub(super) fn change_task_status(&mut self, id: &str, status: &str) {
        if self.defer_task_storage() {
            return;
        }
        match self.write_status(id, status) {
            Ok(_) => {
                self.reload_tasks();
                self.select_task(id);
                self.status = format!("{id} is {status}");
                self.telemetry
                    .record("action", format!("status {id} {status}"));
            }
            Err(error) => self.fail(format!("{id}: {error}")),
        }
    }

    /// `t s` with a non-empty selection: every marked task instead of just
    /// `primary` (the task the picker opened against).
    pub(super) fn apply_task_status(&mut self, primary: &str, status: &str) {
        if self.defer_task_storage() {
            return;
        }
        let ids = super::task_bulk::targets(self, primary);
        if ids.len() == 1 {
            self.change_task_status(&ids[0], status);
            return;
        }
        let mut failures = Vec::new();
        for id in &ids {
            if let Err(error) = self.write_status(id, status) {
                failures.push(super::task_bulk::BulkFailure {
                    id: id.clone(),
                    error,
                });
            }
        }
        self.reload_tasks();
        self.telemetry.record(
            "action",
            format!(
                "bulk_status {status} {}/{}",
                ids.len() - failures.len(),
                ids.len()
            ),
        );
        self.status =
            super::task_bulk::summarize(&format!("status → {status}"), ids.len(), &failures);
    }
}

impl App {
    pub(super) fn open_task_planning_picker(&mut self) {
        let Some(id) = self.selected_task().map(|task| task.id.clone()) else {
            return;
        };
        self.open_picker(PickerPurpose::TaskPlanning(id), planning_options());
        self.status.clear();
    }

    pub(super) fn change_task_planning(&mut self, id: &str, value: &str) {
        if self.defer_task_storage() {
            return;
        }
        let planning = match value.parse::<switchbard_core::PlanningState>() {
            Ok(planning) => planning,
            Err(error) => {
                self.fail(error.to_string());
                return;
            }
        };
        match switchbard_core::set_task_planning(&self.repo_root, id, planning) {
            Ok(_) => {
                self.reload_tasks();
                self.select_task(id);
                self.status = format!("{id} is {planning}; execution status unchanged");
                self.telemetry
                    .record("action", format!("planning {id} {planning}"));
            }
            Err(error) => self.fail(format!("{id}: {error}")),
        }
    }

    pub(super) fn plan_or_append(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };
        if self.legacy_order || task.planning == switchbard_core::PlanningState::Planned {
            self.set_rank(self.top.len() + 1);
        } else {
            let id = task.id.clone();
            self.change_task_planning(&id, "Planned");
        }
    }
}

pub(super) fn planning_options() -> Vec<PickOption> {
    [
        switchbard_core::PlanningState::Considering,
        switchbard_core::PlanningState::Planned,
    ]
    .into_iter()
    .map(|state| PickOption::text(state.to_string(), 0))
    .collect()
}
