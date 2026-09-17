//! `s`: building a cascading sort one layer at a time, shown as a breadcrumb.
//!
//! The whole entry is one gesture. `s` opens the column chooser, a column opens
//! its order list, and an order *locks* that layer - applied to the list at once,
//! and written into the breadcrumb. From there `Tab` starts the next layer, `Enter`
//! finishes, and `←`/`Backspace`/`Shift-Tab` walk back out of the breadcrumb a step
//! at a time, exactly reversing how it was built. `Esc` abandons the entry and puts
//! back the stack the view had.
//!
//! The stage is never stored: it is read from what is open. A column picker means
//! "choosing a column", an order picker means "choosing that column's order", and no
//! picker at all means every layer typed so far is locked. That is why undo is one
//! function rather than a state machine with its own idea of where the user is.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Mode};
use crate::columns::Column;
use crate::picker::{ColumnPurpose, PickerPurpose};
use crate::sort::{self, Order, Sort};

/// The sort stack being built, live from `s` until Enter commits or Esc restores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortEntry {
    /// Layers locked so far. Applied to the list as each one locks, so the
    /// breadcrumb and the rows on screen never disagree.
    pub layers: Vec<Sort>,
    /// What the view sorted by when entry began; Esc puts this back.
    pub original: Vec<Sort>,
}

/// What the breadcrumb is waiting for right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// A column picker is open for the next layer.
    Column,
    /// This column is chosen and its order picker is open.
    Order(Column),
    /// Every layer typed so far is locked and nothing is open.
    Locked,
}

impl App {
    /// `s`: start an entry over whatever the view already sorts by.
    pub(super) fn begin_sort_entry(&mut self) {
        self.sort_entry = Some(SortEntry {
            layers: self.state.sort.clone(),
            original: self.state.sort.clone(),
        });
        self.open_column_chooser(ColumnPurpose::Sort);
    }

    fn sort_stage(&self) -> Stage {
        match self.picker.as_ref().map(|picker| &picker.purpose) {
            Some(PickerPurpose::Sort(column)) => Stage::Order(*column),
            Some(PickerPurpose::ChooseColumn(ColumnPurpose::Sort)) => Stage::Column,
            _ => Stage::Locked,
        }
    }

    /// The breadcrumb as it stands: locked layers, then the layer being typed.
    /// `↑id › pri` reads "sorted by id ascending, and picking priority's order".
    /// Empty when no entry is in flight.
    pub fn sort_breadcrumb(&self) -> String {
        let Some(entry) = self.sort_entry.as_ref() else {
            return String::new();
        };
        let mut text = sort::stack_label(&entry.layers, &self.registry);
        let pending = match self.sort_stage() {
            Stage::Order(column) => Some(column.header(&self.registry).to_string()),
            Stage::Column => Some(String::new()),
            Stage::Locked => None,
        };
        if let Some(pending) = pending {
            if !text.is_empty() {
                text.push_str(sort::LAYER_SEPARATOR);
            }
            text.push_str(&pending);
        }
        text
    }

    /// An order was picked: that layer is settled, applied, and the breadcrumb grows.
    pub(super) fn lock_sort_layer(&mut self, column: Column, order: Order) {
        let Some(entry) = self.sort_entry.as_mut() else {
            // No entry in flight: this pick is the whole stack.
            self.state.sort = vec![Sort { column, order }];
            self.picker = None;
            self.picker_parents.clear();
            self.mode = Mode::Browse;
            self.refilter();
            return;
        };
        // A column orders the list at one depth or another, never both: picking
        // one that already has a layer moves it here rather than doubling it.
        entry.layers.retain(|layer| layer.column != column);
        entry.layers.push(Sort { column, order });
        entry.layers.truncate(sort::MAX_LAYERS);
        self.state.sort = entry.layers.clone();
        self.picker = None;
        self.picker_parents.clear();
        self.mode = Mode::SortEntry;
        self.refilter();
        self.status.clear();
        self.telemetry.record(
            "action",
            format!("sort_pick {}:{order:?}", column.header(&self.registry)),
        );
    }

    /// `Tab`: another layer, to break the ties the layers above it leave.
    fn sort_entry_next_layer(&mut self) {
        let Some(entry) = self.sort_entry.as_ref() else {
            return;
        };
        let depth = entry.layers.len();
        if depth >= sort::MAX_LAYERS {
            self.status = format!("{} sort layers is the limit", sort::MAX_LAYERS);
            return;
        }
        self.open_column_chooser(ColumnPurpose::Sort);
        self.telemetry
            .record("action", format!("sort_layer {}", depth.saturating_add(1)));
    }

    /// `←`/`Backspace`/`Shift-Tab`: one step back up the breadcrumb. Undoing the
    /// first step of the first layer leaves the entry entirely.
    pub(super) fn sort_entry_undo(&mut self) {
        let Some(entry) = self.sort_entry.as_ref() else {
            return;
        };
        let last = entry.layers.last().copied();
        match (self.sort_stage(), last) {
            // A column is chosen but not its order: drop back to the column list.
            (Stage::Order(_), _) => {
                self.picker = None;
                self.picker_parents.clear();
                self.open_column_chooser(ColumnPurpose::Sort);
            }
            // Nothing typed for this layer yet: undo the layer above it.
            (Stage::Column | Stage::Locked, Some(layer)) => self.reopen_sort_layer(layer),
            (Stage::Column | Stage::Locked, None) => self.cancel_sort_entry(),
        }
        self.telemetry.record("action", "sort_undo");
    }

    /// Pops a locked layer back open at its order picker, so the key that locked
    /// it is the key being asked for again.
    fn reopen_sort_layer(&mut self, layer: Sort) {
        let Some(entry) = self.sort_entry.as_mut() else {
            return;
        };
        entry.layers.pop();
        self.state.sort = entry.layers.clone();
        self.refilter();
        self.picker = None;
        self.picker_parents.clear();
        self.open_sort_picker(layer.column);
    }

    /// `Enter`: the stack stands as the breadcrumb shows it.
    pub(super) fn commit_sort_entry(&mut self) {
        let Some(entry) = self.sort_entry.take() else {
            return;
        };
        self.state.sort = entry.layers;
        self.picker = None;
        self.picker_parents.clear();
        self.mode = Mode::Browse;
        self.refilter();
        self.status.clear();
        let stack = sort::stack_to_text(&self.state.sort, &self.registry);
        self.telemetry
            .record("action", format!("sort_done {stack}"));
    }

    /// `Esc`: the view goes back to the stack it had before `s`.
    pub(super) fn cancel_sort_entry(&mut self) {
        let Some(entry) = self.sort_entry.take() else {
            return;
        };
        self.state.sort = entry.original;
        self.picker = None;
        self.picker_parents.clear();
        self.mode = Mode::Browse;
        self.refilter();
        self.status.clear();
        self.telemetry.record("action", "sort_cancel");
    }

    /// `none` in the order list: the whole stack goes, and so does the entry.
    pub(super) fn clear_sort_entry(&mut self) {
        self.sort_entry = None;
        self.state.sort.clear();
        self.picker = None;
        self.picker_parents.clear();
        self.mode = Mode::Browse;
        self.refilter();
        self.telemetry.record("action", "sort_pick none");
    }

    /// The locked stage, where the breadcrumb is the whole interface. Its own
    /// keys are Tab, Enter, undo and Esc; anything else settles the stack and is
    /// handled as an ordinary browse key, so the breadcrumb never traps anyone.
    pub(super) fn handle_sort_entry_key(&mut self, event: KeyEvent) {
        let stepping_back = event.code == KeyCode::BackTab
            || (event.code == KeyCode::Tab && event.modifiers.contains(KeyModifiers::SHIFT));
        if stepping_back {
            self.sort_entry_undo();
            return;
        }
        match event.code {
            KeyCode::Tab => self.sort_entry_next_layer(),
            KeyCode::Left | KeyCode::Backspace | KeyCode::Delete => self.sort_entry_undo(),
            KeyCode::Enter => self.commit_sort_entry(),
            KeyCode::Esc => self.cancel_sort_entry(),
            _ => {
                self.commit_sort_entry();
                self.handle_browse_key(event);
            }
        }
    }
}
