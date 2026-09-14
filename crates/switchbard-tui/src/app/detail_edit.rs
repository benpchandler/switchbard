//! The editable detail pane (TASK-222): a focusable surface over structured
//! task fields, reusing the app's existing picker/input vocabulary. Multiline
//! prose (description body, implementation plan, acceptance-criterion text)
//! is out of scope — those stay read-only here, edited with `sb edit`.
//!
//! Every write goes through `switchbard_core`'s native write layer
//! (`edit_backlog_task_expected` / `set_backlog_acceptance_checked`), gated
//! by [`DetailDraft`]'s own stale-check: `edit_backlog_task_expected`'s
//! revision guard only fires for a centrally-stored task (see its own doc
//! comment), so a raw byte-for-byte compare against the file read when the
//! edit opened is the "or equivalent stale-check" that also catches a plain
//! filesystem edit landing between focus and save — the shape every sbt
//! fixture actually exercises (`tests/harness` never stands up a central
//! store).

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use switchbard_core::{BacklogStorageIdentity, BacklogTaskPatch};

use super::{App, DetailInputKind, Mode, Pane};
use crate::detail_pane::{self, FieldRow};
use crate::page::Page;
use crate::picker::{Payload, PickOption, PickerPurpose};
use crate::tasks;

/// The pre-write snapshot a detail-pane save checks itself against; see the
/// module doc for why this exists alongside `edit_backlog_task_expected`'s
/// own central-storage revision guard.
#[derive(Debug, Clone)]
pub(super) struct DetailDraft {
    task_id: String,
    path: PathBuf,
    snapshot: String,
    identity: Option<BacklogStorageIdentity>,
}

fn detail_input_cap(kind: DetailInputKind) -> usize {
    match kind {
        DetailInputKind::Title => 1024,
        DetailInputKind::DueDate | DetailInputKind::NewLabel => 256,
    }
}

impl App {
    /// Whether the detail pane currently holds input focus: cursor
    /// navigation, a single-line field capture, or one of its own pickers.
    pub fn detail_focused(&self) -> bool {
        matches!(self.mode, Mode::DetailFocus | Mode::DetailInput(_))
            || self
                .picker
                .as_ref()
                .is_some_and(|picker| picker.purpose.is_detail())
    }

    /// The selected task's field rows in the pane's fixed order; `view` walks
    /// the same list to render, so cursor and rendering can never disagree
    /// about what row is what (Rule 2: one owning definition).
    pub fn detail_rows(&self) -> Vec<FieldRow> {
        match self.selected_task() {
            Some(task) => detail_pane::field_rows(
                task,
                self.relations.blocked_by.get(&task.id).map_or(0, Vec::len),
                self.relations.blocks.get(&task.id).map_or(0, Vec::len),
            ),
            None => Vec::new(),
        }
    }

    /// The second `open` gesture (or `l`/`Right`): give the pane focus.
    /// A no-op with nothing selected — there is nowhere to put a cursor.
    pub(super) fn enter_detail_focus(&mut self) {
        if self.selected_task().is_none() {
            self.status = "nothing selected".to_string();
            return;
        }
        let rows = self.detail_rows();
        self.detail_cursor = self.detail_cursor.min(rows.len().saturating_sub(1));
        self.mode = Mode::DetailFocus;
        self.begin_detail_edit();
        self.status.clear();
    }

    /// First `back`/`Esc`/`h`/`Left` while focused: give focus back to the
    /// list, leaving the pane open. A second `back` (now in `Mode::Browse`,
    /// handled by the ordinary `Action::Back`) closes it.
    fn leave_detail_focus(&mut self) {
        self.mode = Mode::Browse;
        self.detail_draft = None;
        self.status.clear();
    }

    /// Close the pane outright and drop every bit of in-progress edit state.
    pub(super) fn close_detail_pane(&mut self) {
        self.pane = Pane::None;
        self.detail_cursor = 0;
        self.detail_scroll = 0;
        self.detail_draft = None;
    }

    /// `Tab` while the pane holds focus (nav, a field input, or one of its
    /// own pickers): switching pages cancels whatever was in flight, the
    /// same way it already does from plain `Mode::Browse`.
    pub(super) fn cancel_detail_and_switch_page(&mut self) {
        self.picker = None;
        self.picker_parents.clear();
        self.input.clear();
        self.mode = Mode::Browse;
        self.close_detail_pane();
        let target = self.page.toggle();
        self.switch_page(target);
        if self.page == Page::PullRequests {
            self.refresh_pr_state();
        }
        self.status.clear();
    }

    pub(super) fn handle_detail_focus_key(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Tab => self.cancel_detail_and_switch_page(),
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => self.leave_detail_focus(),
            KeyCode::Down | KeyCode::Char('j') => self.move_detail_cursor(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_detail_cursor(-1),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.activate_detail_row(),
            KeyCode::Char(' ') => self.toggle_detail_acceptance_at_cursor(),
            _ => {}
        }
    }

    fn move_detail_cursor(&mut self, delta: isize) {
        let rows = self.detail_rows();
        if rows.is_empty() {
            self.detail_cursor = 0;
            return;
        }
        let max = rows.len() - 1;
        let next = self.detail_cursor as isize + delta;
        self.detail_cursor = next.clamp(0, max as isize) as usize;
    }

    /// Enter/`l`/`Right` on the cursor row: open that field's editor, or
    /// refuse with a status message on a read-only task. Every value this
    /// needs is cloned out of the selected task up front so the match below
    /// is free to call `&mut self` methods (open a picker, start a capture).
    fn activate_detail_row(&mut self) {
        let rows = self.detail_rows();
        let Some(row) = rows.get(self.detail_cursor).copied() else {
            return;
        };
        let Some(task) = self.selected_task() else {
            return;
        };
        let task_id = task.id.clone();
        let task_editable = task.editable();
        let title = task.title.clone();
        let due_date = task.due_date.clone().unwrap_or_default();
        if row.editable() && !task_editable {
            self.status = format!("{task_id} is read-only");
            return;
        }
        match row {
            FieldRow::Title => {
                if self.begin_detail_edit() {
                    self.input = title;
                    self.mode = Mode::DetailInput(DetailInputKind::Title);
                    self.status.clear();
                }
            }
            FieldRow::Status => self.open_detail_status_picker(),
            FieldRow::Priority => self.open_detail_priority_picker(),
            FieldRow::Project => self.open_detail_project_picker(),
            FieldRow::DueDate => {
                if self.begin_detail_edit() {
                    self.input = due_date;
                    self.mode = Mode::DetailInput(DetailInputKind::DueDate);
                    self.status.clear();
                }
            }
            FieldRow::Labels => {
                self.status.clear();
                self.open_detail_labels_picker();
            }
            FieldRow::Acceptance(index) => self.toggle_detail_acceptance(index),
            FieldRow::Description | FieldRow::BlockedBy(_) | FieldRow::Blocks(_) => {}
        }
    }

    fn toggle_detail_acceptance_at_cursor(&mut self) {
        let rows = self.detail_rows();
        if let Some(FieldRow::Acceptance(index)) = rows.get(self.detail_cursor).copied() {
            self.toggle_detail_acceptance(index);
        }
    }

    /// `Space` (or Enter) on an acceptance row: flip its checked state
    /// through `set_backlog_acceptance_checked`, gated by the same
    /// stale-draft compare as every other detail-pane save.
    ///
    /// Deliberately does **not** call `begin_detail_edit` here: unlike every
    /// other row, a toggle opens and commits in the same keystroke, so
    /// re-snapshotting immediately beforehand would compare the file against
    /// itself and the stale-draft guard would never fire. The snapshot this
    /// checks against is whichever one is already current — taken when focus
    /// entered the pane, or refreshed after this task's last successful
    /// detail-pane save — so a real edit landing anywhere in between is
    /// still caught.
    fn toggle_detail_acceptance(&mut self, position: usize) {
        let Some(task) = self.selected_task() else {
            return;
        };
        if !task.editable() {
            self.status = format!("{} is read-only", task.id);
            return;
        }
        let Some(item) = task.acceptance_criteria.get(position) else {
            return;
        };
        let id = task.id.clone();
        let checklist_index = item.index;
        let checked = !item.checked;
        match self.save_detail_checklist(&id, checklist_index, checked) {
            Ok(_) => {
                self.reload_tasks();
                self.select_task(&id);
                self.status = format!(
                    "{id} acceptance #{checklist_index} {}",
                    if checked { "checked" } else { "unchecked" }
                );
                self.telemetry
                    .record("action", format!("detail_ac_toggle {id}"));
                self.begin_detail_edit();
            }
            Err(error) => self.fail(error),
        }
    }

    pub(super) fn handle_detail_input_key(&mut self, event: KeyEvent) {
        let Mode::DetailInput(kind) = self.mode else {
            return;
        };
        match event.code {
            KeyCode::Tab => self.cancel_detail_and_switch_page(),
            // A new-label capture came from the labels picker (`n`), so
            // canceling it returns there rather than dropping all the way
            // to plain cursor focus — the same place a *successful* add
            // (`commit_detail_new_label`) already reopens into.
            KeyCode::Esc if kind == DetailInputKind::NewLabel => {
                self.input.clear();
                self.open_detail_labels_picker();
            }
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::DetailFocus;
                self.status = "edit cancelled".to_string();
            }
            KeyCode::Enter if event.kind == KeyEventKind::Press => match kind {
                DetailInputKind::Title => self.commit_detail_title(),
                DetailInputKind::DueDate => self.commit_detail_due_date(),
                DetailInputKind::NewLabel => self.commit_detail_new_label(),
            },
            KeyCode::Backspace => {
                self.input.pop();
                self.status.clear();
            }
            KeyCode::Char(c)
                if !c.is_control()
                    && !event
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let cap = detail_input_cap(kind);
                if self.input.len() + c.len_utf8() <= cap {
                    self.input.push(c);
                    self.status.clear();
                } else {
                    self.fail(format!("limit: {cap} bytes"));
                }
            }
            _ => {}
        }
    }

    /// Snapshot the selected task's file for the stale-check, refusing on a
    /// read-only task or an unreadable file. Called when focus is gained and
    /// again every time a row that opens a separate editor (a picker or a
    /// single-line capture) does so, so the snapshot is as fresh as the
    /// moment editing actually began. The one exception is acceptance
    /// toggling (`toggle_detail_acceptance`), which opens and commits in one
    /// keystroke and so never refreshes the snapshot immediately before its
    /// own save — see that function's doc.
    ///
    /// Also called from `reload_tasks` (`pub(super)` for that) after a
    /// background file-change reload lands while the pane is in plain
    /// `Mode::DetailFocus` — cursor navigation only, no picker or capture
    /// open — so a `Space` toggle or a freshly opened picker's pick is
    /// checked against what the pane now actually shows, not a snapshot from
    /// before the very reload that just repainted it. `Mode::DetailInput`
    /// and an open `Detail*` picker are deliberately excluded: the user is
    /// mid-draft against the old content there, and the stale refusal on
    /// save is the correct outcome, not a reload target to silently move.
    pub(super) fn begin_detail_edit(&mut self) -> bool {
        let Some(task) = self.selected_task() else {
            self.status = "nothing selected".to_string();
            return false;
        };
        if !task.editable() {
            self.status = format!("{} is read-only", task.id);
            return false;
        }
        let id = task.id.clone();
        let path = task.path.clone();
        let identity = task.storage_identity.clone();
        let snapshot = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.fail(format!("{id}: could not read task file: {error}"));
                return false;
            }
        };
        self.detail_draft = Some(DetailDraft {
            task_id: id,
            path,
            snapshot,
            identity,
        });
        true
    }

    fn save_detail_patch(&self, id: &str, patch: &BacklogTaskPatch) -> Result<String, String> {
        let draft = self.checked_draft(id)?;
        switchbard_core::edit_backlog_task_expected(
            &self.repo_root,
            id,
            patch,
            draft.identity.as_ref(),
        )
        .map_err(|error| error.to_string())
    }

    fn save_detail_checklist(
        &self,
        id: &str,
        index: usize,
        checked: bool,
    ) -> Result<String, String> {
        self.checked_draft(id)?;
        switchbard_core::set_backlog_acceptance_checked(&self.repo_root, id, index, checked)
            .map_err(|error| error.to_string())
    }

    /// The stale-draft guard every detail-pane save runs first: the draft
    /// must belong to the task being saved, and the file it snapshotted must
    /// still read back byte-for-byte the same.
    fn checked_draft(&self, id: &str) -> Result<&DetailDraft, String> {
        let Some(draft) = self
            .detail_draft
            .as_ref()
            .filter(|draft| draft.task_id == id)
        else {
            return Err("no active edit; reopen the row and retry".to_string());
        };
        match std::fs::read_to_string(&draft.path) {
            Ok(current) if current == draft.snapshot => Ok(draft),
            // Names the actual gesture rather than a generic "reload and
            // retry": one `Esc` always returns to plain `Mode::DetailFocus`
            // (from a picker, from a capture, or already there), and `Enter`
            // on the same row re-activates it with a fresh snapshot —
            // `reload_tasks` also refreshes the snapshot on its own the
            // moment a background file change lands while already in
            // `Mode::DetailFocus`, so this message is a fallback for the
            // window right after a stale save is refused, not the only way
            // to recover.
            Ok(_) => Err(format!(
                "{id} changed on disk; press Esc then Enter to reload"
            )),
            Err(error) => Err(format!("{id}: could not verify draft: {error}")),
        }
    }

    /// Save a patch, reload, keep the pane on the same task, and refresh the
    /// draft snapshot for the next edit. Returns whether the save succeeded,
    /// so a caller holding a typed draft (title, due date) can decide
    /// whether to clear it.
    fn apply_detail_patch(
        &mut self,
        patch: BacklogTaskPatch,
        ok_message: impl FnOnce(&str) -> String,
    ) -> bool {
        let Some(id) = self
            .detail_draft
            .as_ref()
            .map(|draft| draft.task_id.clone())
        else {
            self.fail("no active edit; reopen the row and retry".to_string());
            return false;
        };
        match self.save_detail_patch(&id, &patch) {
            Ok(_) => {
                self.reload_tasks();
                self.select_task(&id);
                self.status = ok_message(&id);
                self.telemetry.record("action", format!("detail_edit {id}"));
                self.begin_detail_edit();
                true
            }
            Err(error) => {
                self.fail(error);
                false
            }
        }
    }

    fn open_detail_status_picker(&mut self) {
        let Some(id) = self.selected_task().map(|task| task.id.clone()) else {
            return;
        };
        if !self.begin_detail_edit() {
            return;
        }
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
        self.open_picker(PickerPurpose::DetailStatus(id), options);
        self.status.clear();
    }

    fn open_detail_priority_picker(&mut self) {
        let Some(id) = self.selected_task().map(|task| task.id.clone()) else {
            return;
        };
        if !self.begin_detail_edit() {
            return;
        }
        let options = switchbard_core::BACKLOG_PRIORITIES
            .iter()
            .map(|priority| PickOption::text((*priority).to_string(), 0))
            .collect();
        self.open_picker(PickerPurpose::DetailPriority(id), options);
        self.status.clear();
    }

    fn open_detail_project_picker(&mut self) {
        let Some(id) = self.selected_task().map(|task| task.id.clone()) else {
            return;
        };
        if !self.begin_detail_edit() {
            return;
        }
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
        self.open_picker(PickerPurpose::DetailProject(id), options);
        self.status.clear();
    }

    /// Repo-wide known labels (most-used first, the same order `f` uses for
    /// the Labels column), each with a checkmark for the selected task, plus
    /// a keyed row that opens a single-line "new label" capture.
    fn open_detail_labels_picker(&mut self) {
        let Some(id) = self.selected_task().map(|task| task.id.clone()) else {
            return;
        };
        if !self.begin_detail_edit() {
            return;
        }
        let names: Vec<String> = tasks::field_values(
            &self.registry,
            &self.tasks,
            tasks::FilterField::Label,
            &self.goals,
            &self.relations.blocked,
        )
        .into_iter()
        .map(|(name, _)| name)
        .collect();
        let mut options: Vec<PickOption> = names
            .into_iter()
            .map(|name| PickOption::text(name, 0))
            .collect();
        options.push(PickOption::keyed('n', "+ add new label", Payload::NewLabel));
        self.open_picker(PickerPurpose::DetailLabels(id), options);
    }

    pub(super) fn commit_detail_status(&mut self, status: &str) {
        let patch = BacklogTaskPatch {
            status: Some(status.to_string()),
            ..Default::default()
        };
        let status_owned = status.to_string();
        self.apply_detail_patch(patch, move |id| format!("{id} is {status_owned}"));
        self.mode = Mode::DetailFocus;
    }

    pub(super) fn commit_detail_priority(&mut self, priority: &str) {
        let patch = BacklogTaskPatch {
            priority: Some(priority.to_string()),
            ..Default::default()
        };
        let priority_owned = priority.to_string();
        self.apply_detail_patch(patch, move |id| format!("{id} priority: {priority_owned}"));
        self.mode = Mode::DetailFocus;
    }

    pub(super) fn commit_detail_project(&mut self, project: Option<&str>) {
        let patch = BacklogTaskPatch {
            project: project.map(str::to_string),
            clear_project: project.is_none(),
            ..Default::default()
        };
        let label = project.unwrap_or("Unassigned").to_string();
        self.apply_detail_patch(patch, move |id| format!("{id} project: {label}"));
        self.mode = Mode::DetailFocus;
    }

    /// A label row picked in the multi-select panel: toggle it and reopen,
    /// the same "stays open, immediate write" shape `t g` (goals) already
    /// uses for its own multi-select.
    pub(super) fn commit_detail_label_toggle(&mut self, label: &str) {
        let Some(task) = self.selected_task() else {
            self.fail("no task selected".to_string());
            return;
        };
        let mut labels = task.labels.clone();
        let removing = labels
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(label));
        if removing {
            labels.retain(|existing| !existing.eq_ignore_ascii_case(label));
        } else {
            labels.push(label.to_string());
        }
        let patch = BacklogTaskPatch {
            labels: Some(labels),
            ..Default::default()
        };
        let verb = if removing { '-' } else { '+' };
        let label_owned = label.to_string();
        self.apply_detail_patch(patch, move |id| format!("{id} labels: {verb}{label_owned}"));
        self.open_detail_labels_picker();
    }

    pub(super) fn begin_detail_new_label(&mut self) {
        self.picker = None;
        self.input.clear();
        self.mode = Mode::DetailInput(DetailInputKind::NewLabel);
        self.status.clear();
    }

    fn commit_detail_title(&mut self) {
        let title = self.input.trim().to_string();
        if title.is_empty() {
            self.fail("title is required".to_string());
            return;
        }
        let patch = BacklogTaskPatch {
            title: Some(title),
            ..Default::default()
        };
        if self.apply_detail_patch(patch, |id| format!("{id} title saved")) {
            self.input.clear();
            self.mode = Mode::DetailFocus;
        }
    }

    fn commit_detail_due_date(&mut self) {
        let text = self.input.trim().to_string();
        let (due_date, clear_due_date) = if text.is_empty() {
            (None, true)
        } else {
            match switchbard_core::parse_due_date(&text) {
                Ok(valid) => (Some(valid), false),
                Err(error) => {
                    self.fail(error.to_string());
                    return;
                }
            }
        };
        let patch = BacklogTaskPatch {
            due_date,
            clear_due_date,
            ..Default::default()
        };
        if self.apply_detail_patch(patch, |id| format!("{id} due date saved")) {
            self.input.clear();
            self.mode = Mode::DetailFocus;
        }
    }

    fn commit_detail_new_label(&mut self) {
        let label = self.input.trim().to_string();
        if label.is_empty() {
            self.fail("label cannot be empty".to_string());
            return;
        }
        let Some(task) = self.selected_task() else {
            self.fail("no task selected".to_string());
            return;
        };
        let mut labels = task.labels.clone();
        if !labels
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&label))
        {
            labels.push(label.clone());
        }
        let patch = BacklogTaskPatch {
            labels: Some(labels),
            ..Default::default()
        };
        let label_owned = label.clone();
        if self.apply_detail_patch(patch, move |id| format!("{id} labels: +{label_owned}")) {
            self.input.clear();
            self.open_detail_labels_picker();
        }
    }
}
