//! The `v` chords: open a slot, save into one, promote one to global.

use crate::app::App;
use crate::picker::{Payload, PickOption, PickerPurpose, TaskAction};
use crate::views;

impl App {
    pub(super) fn open_view_picker(&mut self, purpose: PickerPurpose) {
        let mut options: Vec<PickOption> = self
            .views
            .slots()
            .into_iter()
            .enumerate()
            .map(|(slot, (view, _))| PickOption::numbered(view.name(), Payload::ViewSlot(slot)))
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
            }
            PickerPurpose::SaveView => {
                if options.len() < views::MAX_SLOTS {
                    options.push(PickOption::numbered(
                        "New slot",
                        Payload::ViewSlot(options.len()),
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
