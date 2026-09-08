//! Existing parent choices and native reparenting, bound to the selected task ID.

use crossterm::event::{KeyCode, KeyEvent};
use switchbard_core::{eligible_backlog_parents, load_backlog_repo, move_backlog_task};

use super::App;
use crate::picker::{Payload, PickOption, PickerPurpose};

impl App {
    pub(super) fn open_task_parent_picker(&mut self) {
        let Some(id) = self.selected_task().map(|task| task.id.clone()) else {
            self.status = "no task selected".to_string();
            return;
        };
        match self.parent_options(&id) {
            Ok((options, selected)) => {
                let empty = options.len() == 1;
                self.open_picker(PickerPurpose::TaskParent(id), options);
                if let Some(picker) = self.picker.as_mut() {
                    picker.selected = selected;
                }
                self.status = if empty {
                    "No eligible parent tasks. Changing parent assigns a new task ID."
                } else {
                    "Changing parent assigns a new task ID. Type ID or title; Enter saves."
                }
                .to_string();
            }
            Err(error) => self.fail(format!("parent: {error}")),
        }
    }

    fn parent_options(&self, id: &str) -> anyhow::Result<(Vec<PickOption>, usize)> {
        let repo = load_backlog_repo(&self.repo_root)?;
        let candidates = eligible_backlog_parents(&repo, id)?;
        let current = repo
            .tasks
            .iter()
            .find(|task| task.id == id)
            .and_then(|task| task.parent.as_deref());
        let mut options = vec![PickOption::numbered(
            "No parent (top-level task)",
            Payload::Parent(None),
        )];
        options.extend(candidates.into_iter().map(|task| {
            let title = task.title.split_whitespace().collect::<Vec<_>>().join(" ");
            PickOption::numbered(
                format!("{} · {title}", task.id),
                Payload::Parent(Some(task.id.clone())),
            )
        }));
        let selected = options.iter().position(|option| {
            matches!(&option.payload, Payload::Parent(parent) if parent.as_deref() == current)
        }).unwrap_or(0);
        Ok((options, selected))
    }

    /// Parent IDs and titles are text, including digits and navigation initials.
    /// Only explicit Enter/right-arrow applies a relationship.
    pub(super) fn handle_parent_search_key(&mut self, event: KeyEvent) -> bool {
        let Some(picker) = self
            .picker
            .as_mut()
            .filter(|picker| matches!(picker.purpose, PickerPurpose::TaskParent(_)))
        else {
            return false;
        };
        let KeyCode::Char(character) = event.code else {
            return false;
        };
        if picker.typed.chars().count() < 256 {
            picker.typed.push(character);
            picker.selected = 0;
        }
        true
    }

    pub(super) fn change_task_parent(&mut self, id: &str, parent: Option<&str>) {
        match move_backlog_task(&self.repo_root, id, parent) {
            Ok(new_id) => {
                let selected = new_id.as_deref().unwrap_or(id);
                self.reload_tasks();
                self.select_task(selected);
                self.picker_parents.clear();
                self.status = match new_id {
                    Some(ref new_id) => {
                        format!("{id} → {new_id}; parent: {}", parent.unwrap_or("none"))
                    }
                    None => format!("{id}: parent unchanged"),
                };
                self.telemetry.record(
                    "action",
                    format!("parent {id} {}", parent.unwrap_or("none")),
                );
            }
            Err(error) => {
                self.reload_tasks();
                self.fail(format!("{id}: parent change failed: {error}"));
            }
        }
    }
}
