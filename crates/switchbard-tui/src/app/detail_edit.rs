//! The editable detail pane (TASK-222): a focusable surface over structured
//! task fields, reusing the app's existing picker/input vocabulary. Multiline
//! prose (implementation plan, acceptance-criterion text) stays read-only
//! here, edited with `sb edit`; the description body renders in full but is
//! likewise not inline-editable — `sb edit <id> --description` is its path.
//!
//! Drafts read from the authoritative task store. Revision/content checks protect
//! both field saves and checkbox toggles from concurrent edits.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Position;

use switchbard_core::BacklogTaskPatch;

use super::{App, DetailInputKind, Mode, Pane};
use crate::config::{Action, KeyChord};
use crate::detail_pane::{self, FieldRow};
use crate::page::Page;
use crate::picker::{Payload, PickOption, PickerPurpose};
use crate::tasks;

pub(super) type DetailDraft = switchbard_core::BacklogTaskSnapshot;

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
        (self.pane == Pane::Detail && self.detail_read_focus && self.mode == Mode::Browse)
            || self.detail_edit_focused()
    }

    pub fn detail_edit_focused(&self) -> bool {
        matches!(self.mode, Mode::DetailFocus | Mode::DetailInput(_))
            || self
                .picker
                .as_ref()
                .is_some_and(|picker| picker.purpose.is_detail())
    }

    pub(super) fn toggle_detail_focus(&mut self) {
        if self.pane != Pane::Detail {
            return;
        }
        let focused = self.detail_focused();
        self.mode = Mode::Browse;
        self.detail_draft = None;
        self.detail_read_focus = !focused;
        self.status.clear();
    }

    /// Reading uses rendered-line offsets, while editing uses field-row navigation.
    pub(super) fn scroll_detail_reading(&mut self, action: &Action) -> bool {
        let pr_paging =
            self.page == Page::PullRequests && matches!(action, Action::PageDown | Action::PageUp);
        if !self.page.has_list_view()
            || self.pane != Pane::Detail
            || (!self.detail_read_focus && !pr_paging)
        {
            return false;
        }
        let viewport = self.detail_hit.detail_area.height.saturating_sub(2).max(1);
        let delta = match action {
            Action::Down => 1,
            Action::Up => -1,
            Action::PageDown => i32::from(viewport),
            Action::PageUp => -i32::from(viewport),
            Action::Top => -65535,
            Action::Bottom => 65535,
            _ => return false,
        };
        let scroll = if self.page == Page::PullRequests {
            &mut self.pull_requests.detail_scroll
        } else {
            &mut self.detail_scroll
        };
        *scroll = (i32::from(*scroll) + delta).clamp(0, 65535) as u16;
        true
    }

    pub(crate) fn detail_presentation(&self) -> detail_pane::DetailPresentation {
        detail_pane::DetailPresentation {
            metadata_first: self.experiments.is_enabled("detail-metadata-first"),
            compact_done: self.experiments.is_enabled("detail-compact-done"),
        }
    }

    /// The selected task's field rows in the pane's presentation order; `view` walks
    /// the same list to render, so cursor and rendering can never disagree
    /// about what row is what (Rule 2: one owning definition).
    pub fn detail_rows(&self) -> Vec<FieldRow> {
        match self.selected_task() {
            Some(task) => detail_pane::visible_rows(
                detail_pane::presented_field_rows(
                    task,
                    self.relations.blocked_by.get(&task.id).map_or(0, Vec::len),
                    self.relations.blocks.get(&task.id).map_or(0, Vec::len),
                    self.detail_presentation(),
                ),
                &self.detail_collapsed,
            ),
            None => Vec::new(),
        }
    }

    /// The second `open` gesture (or `l`/`Right`): give the pane focus.
    /// A no-op with nothing selected — there is nowhere to put a cursor.
    pub(super) fn enter_detail_focus(&mut self) {
        if self.defer_task_storage() {
            return;
        }
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
        self.detail_read_focus = false;
        self.detail_draft = None;
        self.status.clear();
    }

    /// Close the pane outright and drop every bit of in-progress edit state.
    pub(super) fn close_detail_pane(&mut self) {
        self.pane = Pane::None;
        self.detail_read_focus = false;
        self.detail_cursor = 0;
        self.detail_scroll = 0;
        self.detail_scroll_anchor = None;
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
        let chord = KeyChord::from_event(&event);
        if let Some(action @ (Action::RepoIdea | Action::RepoBug | Action::Command)) =
            self.config.keys.get(&chord).cloned()
        {
            self.apply(&action);
            return;
        }
        if self.config.keys.get(&chord) == Some(&Action::FocusPane) {
            self.toggle_detail_focus();
            return;
        }
        match event.code {
            KeyCode::Tab => self.cancel_detail_and_switch_page(),
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => self.leave_detail_focus(),
            KeyCode::Down | KeyCode::Char('j') => self.move_detail_cursor(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_detail_cursor(-1),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.activate_detail_row(),
            KeyCode::Char('z') => {
                if let Some(row) = self.detail_rows().get(self.detail_cursor) {
                    self.toggle_detail_section(row.section());
                }
            }
            KeyCode::Char('Z') => self.set_detail_sections_collapsed(true),
            KeyCode::Char('A') => self.set_detail_sections_collapsed(false),
            KeyCode::Char(' ') => self.toggle_detail_acceptance_at_cursor(),
            _ => self.handle_detail_scroll_key(&event),
        }
    }

    fn toggle_detail_section(&mut self, section: detail_pane::Section) {
        if !self.detail_collapsed.remove(&section) {
            self.detail_collapsed.insert(section);
        }
        self.focus_detail_section(section);
    }

    fn set_detail_sections_collapsed(&mut self, collapsed: bool) {
        let section = self
            .detail_rows()
            .get(self.detail_cursor)
            .map_or(detail_pane::Section::Properties, |row| row.section());
        self.detail_collapsed = if collapsed {
            detail_pane::Section::ALL.into_iter().collect()
        } else {
            Default::default()
        };
        self.focus_detail_section(section);
    }

    fn focus_detail_section(&mut self, section: detail_pane::Section) {
        self.detail_cursor = self
            .detail_rows()
            .iter()
            .position(|row| row.section() == section)
            .unwrap_or(0);
        self.detail_scroll_anchor = None;
    }

    /// The physical `PageDown`/`PageUp` keys, or whatever the user's own
    /// `page_down`/`page_up` Lua bindings are (`ctrl-d`/`ctrl-u` by
    /// default): scroll the pane by its own last-rendered height without
    /// moving the cursor — the one way to page through a long description
    /// body, since `j`/`k` only walk field rows. Restores TASK-221's
    /// "bounded offsets, unchanged task selection" for long content; see
    /// `view::adjust_detail_scroll` for how the auto-follow-cursor scroll
    /// gets out of the way of a deliberate page here.
    fn handle_detail_scroll_key(&mut self, event: &KeyEvent) {
        let chord = KeyChord::from_event(event);
        match self.config.keys.get(&chord) {
            Some(Action::PageDown) => self.page_detail_scroll(1),
            Some(Action::PageUp) => self.page_detail_scroll(-1),
            _ => {}
        }
    }

    /// Move `detail_scroll` by `direction` (`1` or `-1`) times the pane's
    /// own last-rendered viewport height, clamped to a non-negative offset;
    /// the render-time clamp in `detail_pane::draw` bounds the top end
    /// against the content actually on screen.
    pub(super) fn page_detail_scroll(&mut self, direction: i32) {
        let delta = direction * i32::from(self.detail_viewport.max(1));
        self.detail_scroll = (i32::from(self.detail_scroll) + delta).clamp(0, 65535) as u16;
    }

    /// Real terminal mouse input, restoring TASK-221's AC #2 ("keyboard and
    /// mouse scroll long details") against the row-cursor model: wheel over
    /// the detail pane scrolls it the same way `page_detail_scroll` does on
    /// Tasks, or nudges `pull_requests.detail_scroll` on the PR pane (which
    /// has no row cursor of its own); wheel over the list moves the list
    /// selection, the same as an `Action::Down`/`Up` key press. A left
    /// click on a Tasks pane row moves the cursor there and enters
    /// `Mode::DetailFocus`; a left click anywhere in the list selects that
    /// row and returns to `Mode::Browse`. Ignored while a picker, a
    /// single-line capture, the help screen, or Inbox is showing — none of
    /// those have the stable list/detail split `detail_hit` describes.
    pub fn handle_mouse(&mut self, event: MouseEvent) {
        self.interaction_generation = self.interaction_generation.wrapping_add(1);
        if self.handle_experiment_mouse(event) {
            return;
        }
        if !matches!(self.mode, Mode::Browse | Mode::DetailFocus) {
            return;
        }
        if self.page == Page::Inbox || self.pane == Pane::Help {
            return;
        }
        let position = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::ScrollDown => self.scroll_at(position, 1),
            MouseEventKind::ScrollUp => self.scroll_at(position, -1),
            MouseEventKind::Down(MouseButton::Left) => self.click_at(position),
            _ => {}
        }
    }

    fn scroll_at(&mut self, position: Position, direction: i32) {
        if self.pane == Pane::Detail && self.detail_hit.detail_area.contains(position) {
            if self.page == Page::PullRequests {
                self.detail_read_focus = true;
                let delta = direction;
                self.pull_requests.detail_scroll =
                    (i32::from(self.pull_requests.detail_scroll) + delta).clamp(0, 65535) as u16;
            } else {
                self.detail_scroll =
                    (i32::from(self.detail_scroll) + direction).clamp(0, 65535) as u16;
            }
            return;
        }
        if self.detail_hit.list_area.contains(position) {
            self.mode = Mode::Browse;
            self.detail_read_focus = false;
            let action = if direction > 0 {
                Action::Down
            } else {
                Action::Up
            };
            self.apply(&action);
        }
    }

    fn click_at(&mut self, position: Position) {
        if self.page == Page::PullRequests
            && self.pane == Pane::Detail
            && self.detail_hit.detail_area.contains(position)
        {
            self.detail_read_focus = true;
            return;
        }
        if self.page == Page::Tasks
            && self.pane == Pane::Detail
            && self.detail_hit.detail_area.contains(position)
        {
            let inner = self.detail_hit.detail_inner();
            let display_line = position
                .y
                .saturating_sub(inner.y)
                .saturating_add(self.detail_scroll);
            if let Some((_, section)) = self
                .detail_hit
                .section_starts
                .iter()
                .find(|(line, _)| *line == display_line)
                .copied()
            {
                self.enter_detail_focus();
                self.toggle_detail_section(section);
                return;
            }
            if let Some(row) = self.detail_hit.row_at(display_line) {
                self.detail_cursor = row;
                self.enter_detail_focus();
                // Clicking a description body focuses its row without jumping to its heading.
                self.detail_scroll_anchor = self.selected_task().map(|task| (task.id.clone(), row));
            }
            return;
        }
        if self.detail_hit.list_area.contains(position) {
            if self.page == Page::Tasks {
                if let Some(index) = self.detail_hit.list_row_at(position.y) {
                    self.select(index);
                }
            }
            self.mode = Mode::Browse;
            self.detail_read_focus = false;
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
            FieldRow::Section(section) => self.toggle_detail_section(section),
            FieldRow::Content(_) => {
                self.status = "read-only detail; use sb edit for supported fields".to_string();
            }
            FieldRow::Title => {
                if self.begin_detail_edit() {
                    self.input = title;
                    self.mode = Mode::DetailInput(DetailInputKind::Title);
                    self.status.clear();
                }
            }
            FieldRow::Status => self.open_detail_status_picker(),
            FieldRow::Planning => self.open_detail_planning_picker(),
            FieldRow::Checklist => {
                self.status =
                    "Checklist counts criteria on this task and descendants; Done is explicit"
                        .to_string();
            }
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
            FieldRow::Description => {
                self.status = format!("edit the description with sb edit {task_id} --description");
            }
            FieldRow::BlockedBy(_) | FieldRow::Blocks(_) => {}
        }
    }

    fn toggle_detail_acceptance_at_cursor(&mut self) {
        let rows = self.detail_rows();
        match rows.get(self.detail_cursor).copied() {
            Some(FieldRow::Acceptance(index)) => self.toggle_detail_acceptance(index),
            Some(FieldRow::Section(section)) => self.toggle_detail_section(section),
            _ => {}
        }
    }

    /// `Space` (or Enter) on an acceptance row: flip its checked state
    /// through `set_backlog_acceptance_checked_expected`, gated by the same
    /// stale-draft compare as every other detail-pane save.
    ///
    /// Deliberately does **not** call `begin_detail_edit` here: unlike every
    /// other row, a toggle opens and commits in the same keystroke, so
    /// re-snapshotting immediately beforehand would compare the record against
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
        if self.defer_task_storage() {
            return false;
        }
        let Some(task) = self.selected_task() else {
            self.status = "nothing selected".to_string();
            return false;
        };
        if !task.editable() {
            self.status = format!("{} is read-only", task.id);
            return false;
        }
        let id = task.id.clone();
        let snapshot = match switchbard_core::read_backlog_task_snapshot(&self.repo_root, &id) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.fail(format!("{id}: could not read task: {error}"));
                return false;
            }
        };
        self.detail_draft = Some(snapshot);
        true
    }

    fn save_detail_patch(&self, id: &str, patch: &BacklogTaskPatch) -> Result<String, String> {
        let draft = self.checked_draft(id)?;
        switchbard_core::edit_backlog_task_snapshot(&self.repo_root, id, patch, draft)
            .map_err(|error| error.to_string())
    }

    fn save_detail_checklist(
        &self,
        id: &str,
        index: usize,
        checked: bool,
    ) -> Result<String, String> {
        let draft = self.checked_draft(id)?;
        switchbard_core::set_backlog_acceptance_checked_expected(
            &self.repo_root,
            id,
            index,
            checked,
            draft,
        )
        .map_err(|error| error.to_string())
    }

    /// Every save verifies the authoritative identity and content captured at edit start.
    fn checked_draft(&self, id: &str) -> Result<&DetailDraft, String> {
        if self.report.is_pending() || self.task_refresh_pending() {
            return Err(
                "Task refresh or report save in progress; retry editing when finished".into(),
            );
        }
        let draft = self
            .detail_draft
            .as_ref()
            .filter(|draft| draft.task_id == id)
            .ok_or_else(|| "no active edit; reopen the row and retry".to_string())?;
        switchbard_core::validate_backlog_task_snapshot(&self.repo_root, draft).map_err(
            |error| {
                format!(
                    "{id} changed on disk or in storage; press Esc then Enter to reload: {error}"
                )
            },
        )?;
        Ok(draft)
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

impl App {
    fn open_detail_planning_picker(&mut self) {
        let Some(id) = self.selected_task().map(|task| task.id.clone()) else {
            return;
        };
        if !self.begin_detail_edit() {
            return;
        }
        self.open_picker(
            PickerPurpose::DetailPlanning(id),
            super::task_status::planning_options(),
        );
        self.status.clear();
    }

    pub(super) fn commit_detail_planning(&mut self, value: &str) {
        let planning = match value.parse::<switchbard_core::PlanningState>() {
            Ok(state) => state,
            Err(error) => {
                self.fail(error.to_string());
                return;
            }
        };
        let Some(id) = self
            .detail_draft
            .as_ref()
            .map(|draft| draft.task_id.clone())
        else {
            self.fail("no active edit; reopen the row and retry".to_string());
            return;
        };
        let result = self.checked_draft(&id).and_then(|draft| {
            switchbard_core::set_task_planning_snapshot(&self.repo_root, &id, planning, draft)
                .map_err(|error| error.to_string())
        });
        match result {
            Ok(_) => {
                self.reload_tasks();
                self.select_task(&id);
                self.status = format!("{id} is {planning}; execution status unchanged");
                self.telemetry
                    .record("action", format!("detail_planning {id} {planning}"));
                self.begin_detail_edit();
            }
            Err(error) => self.fail(error),
        }
        self.mode = Mode::DetailFocus;
    }
}
