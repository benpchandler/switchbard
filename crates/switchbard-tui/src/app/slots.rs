//! The `v` chords: open a slot, save into one, promote one to global, name one,
//! delete one.

use crate::app::App;
use crate::picker::{Payload, PickOption, PickerPurpose, TaskAction};
use crate::views;

impl App {
    pub(super) fn open_view_picker(&mut self, purpose: PickerPurpose) {
        let mut options: Vec<PickOption> = self
            .views
            .slots()
            .into_iter()
            .map(|(slot, view, _)| {
                PickOption::keyed(
                    char::from_digit((slot + 1) as u32, 10).expect("slots are bounded to nine"),
                    view.display_name(),
                    Payload::ViewSlot(slot),
                )
            })
            .collect();
        match purpose {
            PickerPurpose::Views => {
                if self.page == crate::page::Page::Tasks {
                    options.push(PickOption::keyed(
                        'p',
                        if self.state.pin_top {
                            "Hide top list section"
                        } else {
                            "Show top list section"
                        },
                        Payload::TaskAction(TaskAction::Pin),
                    ));
                }
                options.push(PickOption::keyed(
                    's',
                    "Save current view",
                    Payload::SaveView,
                ));
                options.push(PickOption::keyed(
                    'g',
                    "Make a view global",
                    Payload::GlobalView,
                ));
                options.push(PickOption::keyed('n', "Name a view", Payload::RenameView));
                options.push(PickOption::keyed('x', "Delete a view", Payload::DeleteView));
            }
            PickerPurpose::SaveView => {
                if let Some(slot) =
                    (0..views::MAX_SLOTS).find(|slot| self.views.get(*slot).is_none())
                {
                    options.push(PickOption::keyed(
                        char::from_digit((slot + 1) as u32, 10).expect("slots are bounded to nine"),
                        "New slot",
                        Payload::ViewSlot(slot),
                    ));
                }
                options.push(PickOption::keyed(
                    'd',
                    "Default (slot 1)",
                    Payload::ViewSlot(0),
                ));
            }
            PickerPurpose::GlobalView => options.push(PickOption::keyed(
                'd',
                "Default (slot 1)",
                Payload::ViewSlot(0),
            )),
            // Naming and deletion only ever act on a slot that already holds a view.
            PickerPurpose::RenameView | PickerPurpose::DeleteView => {}
            _ => return,
        }
        self.open_picker(purpose, options);
        self.status.clear();
    }

    pub(super) fn promote_view(&mut self, slot: usize) {
        match self.views.promote(slot) {
            Ok(()) => {
                self.status = format!("slot {} is now global", slot + 1);
                self.telemetry
                    .record("action", format!("view_global {}", slot + 1));
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
                self.status = format!("saved v{} for this repo", slot + 1);
                self.telemetry
                    .record("action", format!("view_save {}", slot + 1));
            }
            Err(error) => self.fail(error),
        }
    }

    /// Opens the one-line name input for a slot, prefilled with its current name.
    pub(super) fn open_rename_view(&mut self, slot: usize) {
        let Some(view) = self.views.get(slot) else {
            self.fail(format!("no view in slot {}", slot + 1));
            return;
        };
        self.rename_slot = Some(slot);
        self.input = view.name;
        self.mode = crate::app::Mode::RenameView;
        self.status.clear();
    }

    /// Commits the typed name to whichever file the slot's effective
    /// definition lives in.
    pub(super) fn save_view_name(&mut self, slot: usize, name: String) {
        match self.views.set_name(slot, name.clone()) {
            Ok(()) => {
                // Naming the slot currently open must not itself count as a
                // divergence from it (the header would misreport `custom`).
                if self.view == slot {
                    self.state.name = name.clone();
                }
                self.status = if name.is_empty() {
                    format!("slot {} unnamed", slot + 1)
                } else {
                    format!("slot {} named \"{name}\"", slot + 1)
                };
                self.telemetry
                    .record("action", format!("view_name {}", slot + 1));
            }
            Err(error) => self.fail(error),
        }
    }

    pub(super) fn delete_view(&mut self, slot: usize) {
        match self.views.delete(slot) {
            Ok(()) => {
                self.status = format!("deleted slot {}", slot + 1);
                self.telemetry
                    .record("action", format!("view_delete {}", slot + 1));
                if self.view == slot {
                    self.switch_view(0);
                }
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
        self.state.sanitize(self.page);
        self.refilter();
    }
}
