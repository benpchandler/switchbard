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
            KeyCode::Left | KeyCode::Up | KeyCode::Char('h' | 'k') => {
                self.preview_column_move(column, -1)
            }
            KeyCode::Right | KeyCode::Down | KeyCode::Char('l' | 'j') => {
                self.preview_column_move(column, 1)
            }
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                self.type_column_destination(column, digit)
            }
            KeyCode::Backspace => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.number.pop();
                }
                self.preview_column_destination(column);
            }
            KeyCode::Enter if self.preview_column_destination(column) => {
                self.finish_column_move(false)
            }
            KeyCode::Esc => self.finish_column_move(true),
            _ => {}
        }
        true
    }

    fn preview_column_move(&mut self, column: Column, delta: isize) {
        if let Some(picker) = self.picker.as_mut() {
            picker.number.clear();
        }
        self.status.clear();
        self.move_column(column, delta);
        self.refresh_column_move_options(column);
    }

    fn type_column_destination(&mut self, column: Column, digit: char) {
        let max_digits = self.state.columns.len().to_string().len();
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        if picker.number.len() >= max_digits {
            self.status = "Position too long; Backspace edits, arrows clear it".into();
            return;
        }
        picker.number.push(digit);
        self.preview_column_destination(column);
    }

    /// Insert at a one-based destination, shifting intervening columns in order.
    /// Retain only ambiguous prefixes and invalid input for correction.
    fn preview_column_destination(&mut self, column: Column) -> bool {
        let Some(picker) = self.picker.as_ref() else {
            return false;
        };
        if picker.number.is_empty() {
            self.status.clear();
            return true;
        }
        let count = self.state.columns.len();
        let position = picker.number.parse::<usize>().unwrap_or(0);
        if position == 0 || position > count {
            self.status = format!("Choose position 1-{count}; Backspace edits, arrows clear it");
            return false;
        }
        let Some(index) = self.state.columns.iter().position(|shown| *shown == column) else {
            self.status = "This column is no longer shown; Esc restores the order".into();
            return false;
        };
        if index != position - 1 {
            self.state.columns.remove(index);
            self.state.columns.insert(position - 1, column);
            self.telemetry.record(
                "action",
                format!("column_move_to {} {position}", column.name(&self.registry)),
            );
        }
        let can_extend = position.checked_mul(10).is_some_and(|next| next <= count);
        if let Some(picker) = self.picker.as_mut() {
            if !can_extend {
                picker.number.clear();
            }
        }
        self.status = if can_extend {
            format!("Position {position}; another digit continues, Enter accepts")
        } else {
            String::new()
        };
        self.refresh_column_move_options(column);
        true
    }

    fn refresh_column_move_options(&mut self, column: Column) {
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
