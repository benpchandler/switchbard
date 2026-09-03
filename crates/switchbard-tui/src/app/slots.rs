//! The `v` chords: open a slot, save into one, promote one to global.

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, Mode};
use crate::views;

impl App {
    pub(super) fn handle_view_chord_key(&mut self, event: KeyEvent) {
        self.mode = Mode::Browse;
        match event.code {
            KeyCode::Char('s') => {
                self.mode = Mode::ViewSaveSlot;
                self.status = format!(
                    "save view into: d (default, slot 1) or 1-{}",
                    (self.views.len() + 1).min(views::MAX_SLOTS)
                );
            }
            KeyCode::Char('g') => {
                self.mode = Mode::ViewGlobalSlot;
                self.status = format!(
                    "make global: d (slot 1) or 1-{} copies that slot to every repo",
                    self.views.len()
                );
            }
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                let slot = digit.to_digit(10).unwrap_or(0) as usize;
                if (1..=self.views.len()).contains(&slot) {
                    self.switch_view(slot - 1);
                    self.telemetry.record("action", format!("view_open {slot}"));
                } else {
                    self.fail(format!(
                        "no view in slot {digit}; saved: 1-{}",
                        self.views.len()
                    ));
                }
            }
            KeyCode::Esc => self.status.clear(),
            other => {
                self.status =
                    format!("v then a slot number, s to save, or g for global, not {other:?}")
            }
        }
    }

    pub(super) fn handle_view_save_slot_key(&mut self, event: KeyEvent) {
        self.mode = Mode::Browse;
        let slot = match event.code {
            KeyCode::Char('d') => 1,
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                digit.to_digit(10).unwrap_or(0) as usize
            }
            KeyCode::Esc => {
                self.status.clear();
                return;
            }
            other => {
                self.status = format!("save needs d or a slot number, not {other:?}");
                return;
            }
        };
        let next_free = self.views.len() + 1;
        if slot == 0 || slot > next_free.min(views::MAX_SLOTS) {
            self.fail(format!(
                "slot {slot} is out of reach; use 1-{}",
                next_free.min(views::MAX_SLOTS)
            ));
            return;
        }
        self.save_view(slot - 1);
    }

    pub(super) fn handle_view_global_slot_key(&mut self, event: KeyEvent) {
        self.mode = Mode::Browse;
        let slot = match event.code {
            KeyCode::Char('d') => 1,
            KeyCode::Char(digit) if digit.is_ascii_digit() => {
                digit.to_digit(10).unwrap_or(0) as usize
            }
            KeyCode::Esc => {
                self.status.clear();
                return;
            }
            other => {
                self.status = format!("global needs d or a slot number, not {other:?}");
                return;
            }
        };
        if slot == 0 || slot > self.views.len() {
            self.fail(format!(
                "no view in slot {slot}; saved: 1-{}",
                self.views.len()
            ));
            return;
        }
        match self.views.promote(slot - 1) {
            Ok(()) => {
                self.status =
                    format!("slot {slot} is now global: every repo opens it with v{slot}");
                self.telemetry
                    .record("action", format!("view_global {slot}"));
            }
            Err(error) => self.fail(error),
        }
    }

    pub(super) fn save_view(&mut self, slot: usize) {
        self.state.filter = self.state.filter.trim().to_string();
        let saved = self.state.clone();
        match self.views.save_repo(slot, saved) {
            Ok(()) => {
                self.view = slot;
                self.status = format!(
                    "saved v{} for this repo · vg{} makes it global",
                    slot + 1,
                    slot + 1
                );
                self.telemetry
                    .record("action", format!("view_save {}", slot + 1));
            }
            Err(error) => self.fail(error),
        }
    }

    pub(super) fn switch_view(&mut self, slot: usize) {
        let Some(saved) = self.views.get(slot) else {
            return;
        };
        self.view = slot;
        self.state = saved;
        self.refilter();
    }
}
