//! A single column's reversible arrangement preview.

use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Mode};
use crate::columns::Column;
use crate::picker::PickerPurpose;
use crate::views::ViewState;

impl App {
    pub(super) fn open_column_move(&mut self, column: Column) {
        let Some(index) = self.state.columns.iter().position(|shown| *shown == column) else {
            self.status = "Show this column before moving it".into();
            return;
        };
        self.move_origin = Some(self.state.columns.clone());
        self.open_picker(
            PickerPurpose::MoveColumn(column),
            self.shown_column_options(),
        );
        if let Some(picker) = self.picker.as_mut() {
            picker.selected = index;
        }
        self.status.clear();
    }

    /// Own the full key stream so arrows cannot navigate the generic picker.
    pub(super) fn handle_column_move_key(&mut self, event: KeyEvent) -> bool {
        let Some(PickerPurpose::MoveColumn(column)) =
            self.picker.as_ref().map(|picker| &picker.purpose)
        else {
            return false;
        };
        let column = *column;
        if !event.modifiers.is_empty() {
            return true;
        }
        match event.code {
            KeyCode::Left | KeyCode::Char('h') => self.preview_column_move(column, -1),
            KeyCode::Right | KeyCode::Char('l') => self.preview_column_move(column, 1),
            KeyCode::Enter => self.finish_column_move(false),
            KeyCode::Esc => self.finish_column_move(true),
            _ => {}
        }
        true
    }

    fn preview_column_move(&mut self, column: Column, delta: isize) {
        self.move_column(column, delta);
        let options = self.shown_column_options();
        let selected = self.state.columns.iter().position(|shown| *shown == column);
        if let (Some(picker), Some(selected)) = (self.picker.as_mut(), selected) {
            picker.options = options;
            picker.selected = selected;
        }
    }

    fn finish_column_move(&mut self, cancel: bool) {
        if let Some(original) = self.move_origin.take() {
            if cancel {
                self.state.columns = original;
            }
        }
        self.picker = None;
        self.picker_parents.clear();
        self.mode = Mode::Browse;
        self.status = if cancel {
            "Column order restored"
        } else {
            "Column order accepted"
        }
        .into();
        self.telemetry.record(
            "action",
            if cancel {
                "column_move_cancel"
            } else {
                "column_move_accept"
            },
        );
    }

    /// Unaccepted previews never enter restart checkpoints or view history.
    pub(super) fn view_state_for_persistence(&self) -> ViewState {
        let mut state = self.state.clone();
        if matches!(
            self.picker.as_ref().map(|picker| &picker.purpose),
            Some(PickerPurpose::MoveColumn(_))
        ) {
            if let Some(original) = &self.move_origin {
                state.columns.clone_from(original);
            }
        }
        state
    }
}
