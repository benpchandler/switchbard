//! Existing parent choices and native reparenting, bound to the selected task
//! ID, or - with a non-empty Tasks selection - every marked task.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};
use switchbard_core::{eligible_backlog_parents, load_backlog_repo, move_backlog_task};

use super::App;
use crate::picker::{Payload, PickOption, PickerPurpose};

impl App {
    pub(super) fn open_task_parent_picker(&mut self) {
        if self.defer_task_storage() {
            return;
        }
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

    /// The one write that reparents a task, shared by the single-row and
    /// bulk paths so they can never disagree about what a reparent is:
    /// `Ok(Some(new_id))` when core renamed the task under its new parent,
    /// `Ok(None)` when it already had that parent and nothing changed.
    fn write_parent(&self, id: &str, parent: Option<&str>) -> Result<Option<String>, String> {
        move_backlog_task(&self.repo_root, id, parent).map_err(|error| error.to_string())
    }

    pub(super) fn change_task_parent(&mut self, id: &str, parent: Option<&str>) {
        if self.defer_task_storage() {
            return;
        }
        match self.write_parent(id, parent) {
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

    /// `t a` with a non-empty selection: every marked task instead of just
    /// `primary` (the task the picker opened against). Reparenting mints a
    /// new id for the task that moves (`write_parent`), so an id captured
    /// before the batch started can go stale mid-batch - `remap` tracks
    /// old -> new for every id this very batch has already renamed, and
    /// each remaining target is re-resolved through it before its own move
    /// is attempted. A target that still doesn't resolve - already folded
    /// into a move earlier in this batch, or removed on disk under us -
    /// reads as a named failure rather than a raw core lookup error.
    ///
    /// Core refuses to move a task that still has a sub-issue at all
    /// (`move_backlog_task`), so a marked parent and its marked child would
    /// otherwise succeed or fail by view-order accident; `reorder_by_current_parent`
    /// moves a marked child ahead of its marked parent using nothing but the
    /// `parent` field already loaded for this view, so the parent's own
    /// sub-issue check always sees the child already gone. Core also never
    /// lets a sub-issue become a parent (one level of nesting), so a task
    /// can never become its own descendant - this loop never re-implements
    /// either guard, it only orders and re-resolves ids around them.
    pub(super) fn apply_task_parent(&mut self, primary: &str, parent: Option<&str>) {
        if self.defer_task_storage() {
            return;
        }
        let ids = super::task_bulk::targets(self, primary);
        if ids.len() == 1 {
            self.change_task_parent(&ids[0], parent);
            return;
        }
        let ids = self.reorder_by_current_parent(ids);
        let mut remap: HashMap<String, String> = HashMap::new();
        let mut failures = Vec::new();
        for id in &ids {
            let mut current = id.as_str();
            while let Some(mapped) = remap.get(current) {
                current = mapped.as_str();
            }
            let current = current.to_string();
            match self.write_parent(&current, parent) {
                Ok(Some(new_id)) => {
                    remap.insert(id.clone(), new_id);
                }
                Ok(None) => {}
                Err(error) => {
                    let error = if self.task_exists(&current) {
                        error
                    } else {
                        "already moved or no longer exists".to_string()
                    };
                    failures.push(super::task_bulk::BulkFailure {
                        id: id.clone(),
                        error,
                    });
                }
            }
        }
        // Marks the batch itself renamed follow their task to its new id;
        // everything else (unchanged, failed, or outside this batch
        // entirely) keeps whatever mark it already had. `reload_tasks`
        // below drops any id - moved or not - that no longer exists.
        for id in &ids {
            if let Some(new_id) = remap.get(id) {
                self.task_selection.marked.remove(id);
                self.task_selection.marked.insert(new_id.clone());
            }
        }
        self.reload_tasks();
        self.telemetry.record(
            "action",
            format!(
                "bulk_parent {} {}/{}",
                parent.unwrap_or("none"),
                ids.len() - failures.len(),
                ids.len()
            ),
        );
        self.status = super::task_bulk::summarize(
            &format!("parent → {}", parent.unwrap_or("none")),
            ids.len(),
            &failures,
        );
    }

    /// Stable-partitions `ids` so a marked task whose *current* parent is
    /// also marked comes before that parent, preserving view order within
    /// each half. Reads only `self.tasks` (already loaded for this view) -
    /// it never asks core anything, so it can never disagree with what core
    /// decides once each move is actually attempted.
    fn reorder_by_current_parent(&self, ids: Vec<String>) -> Vec<String> {
        let marked: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        let has_marked_parent = |id: &str| -> bool {
            self.tasks
                .iter()
                .find(|task| task.id == id)
                .and_then(|task| task.parent.as_deref())
                .is_some_and(|parent| marked.contains(parent))
        };
        let (children_of_marked, rest): (Vec<String>, Vec<String>) =
            ids.iter().cloned().partition(|id| has_marked_parent(id));
        children_of_marked.into_iter().chain(rest).collect()
    }

    fn task_exists(&self, id: &str) -> bool {
        load_backlog_repo(&self.repo_root)
            .map(|repo| repo.tasks.iter().any(|task| task.id == id))
            .unwrap_or(false)
    }
}
