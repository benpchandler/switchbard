//! Project membership uses the repo's known projects and native task editor.

use super::App;
use crate::picker::{Payload, PickOption, PickerPurpose};

impl App {
    pub(super) fn open_task_project_picker(&mut self) {
        let Some(id) = self.selected_task().map(|task| task.id.clone()) else {
            self.status = "no task selected".to_string();
            return;
        };
        let repo = match switchbard_core::load_backlog_repo(&self.repo_root) {
            Ok(repo) => repo,
            Err(error) => {
                self.fail(format!("could not load projects: {error}"));
                return;
            }
        };
        let mut options = vec![PickOption::numbered("Unassigned", Payload::Project(None))];
        options.extend(
            repo.project_names()
                .into_iter()
                .map(|name| PickOption::numbered(name.clone(), Payload::Project(Some(name)))),
        );
        self.open_picker(PickerPurpose::TaskProject(id), options);
        self.status.clear();
    }

    pub(super) fn change_task_project(&mut self, id: &str, project: Option<&str>) {
        let patch = switchbard_core::BacklogTaskPatch {
            project: project.map(str::to_string),
            clear_project: project.is_none(),
            ..Default::default()
        };
        match switchbard_core::edit_backlog_task(&self.repo_root, id, &patch) {
            Ok(_) => {
                self.reload_tasks();
                self.select_task(id);
                self.status = format!("{id} project: {}", project.unwrap_or("Unassigned"));
                self.telemetry.record(
                    "action",
                    format!("project {id} {}", project.unwrap_or("none")),
                );
            }
            Err(error) => self.fail(format!("{id}: {error}")),
        }
    }
}
