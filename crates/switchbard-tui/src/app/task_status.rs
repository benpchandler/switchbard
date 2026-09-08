//! Status choices come from the repo vocabulary and write through the native editor.

use crate::picker::{PickOption, PickerPurpose};

use super::App;

impl App {
    pub(super) fn open_task_status_picker(&mut self) {
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
            .map(|status| PickOption::text(status, 0))
            .collect::<Vec<_>>();
        if options.is_empty() {
            self.fail("no statuses configured for this repo".to_string());
            return;
        }
        self.open_picker(PickerPurpose::TaskStatus(id), options);
        self.status.clear();
    }

    pub(super) fn change_task_status(&mut self, id: &str, status: &str) {
        let patch = switchbard_core::BacklogTaskPatch {
            status: Some(status.to_string()),
            ..Default::default()
        };
        match switchbard_core::edit_backlog_task(&self.repo_root, id, &patch) {
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
}
