//! Project membership uses the repo's known projects and native task editor.

use super::App;
use crate::picker::{Payload, PickOption, PickerPurpose};

impl App {
    pub(super) fn open_task_project_picker(&mut self) {
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

    /// The one write that sets a task's project, shared by the single-row and
    /// bulk paths so they can never disagree about what assigning one means.
    fn write_project(&self, id: &str, project: Option<&str>) -> Result<(), String> {
        let patch = switchbard_core::BacklogTaskPatch {
            project: project.map(str::to_string),
            clear_project: project.is_none(),
            ..Default::default()
        };
        switchbard_core::edit_backlog_task(&self.repo_root, id, &patch)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub(super) fn change_task_project(&mut self, id: &str, project: Option<&str>) {
        if self.defer_task_storage() {
            return;
        }
        match self.write_project(id, project) {
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

    /// `t p` with a non-empty selection: every marked task instead of just
    /// `primary` (the task the picker opened against).
    pub(super) fn apply_task_project(&mut self, primary: &str, project: Option<&str>) {
        if self.defer_task_storage() {
            return;
        }
        let ids = super::task_bulk::targets(self, primary);
        if ids.len() == 1 {
            self.change_task_project(&ids[0], project);
            return;
        }
        let mut failures = Vec::new();
        for id in &ids {
            if let Err(error) = self.write_project(id, project) {
                failures.push(super::task_bulk::BulkFailure {
                    id: id.clone(),
                    error,
                });
            }
        }
        self.reload_tasks();
        let label = project.unwrap_or("Unassigned");
        self.telemetry.record(
            "action",
            format!(
                "bulk_project {label} {}/{}",
                ids.len() - failures.len(),
                ids.len()
            ),
        );
        self.status =
            super::task_bulk::summarize(&format!("project → {label}"), ids.len(), &failures);
    }
}
