//! The one list every menu uses: numbered rows, lettered rows, type-ahead.
//! A row carries a typed payload so the app dispatches on what was picked,
//! never on the label text.

use crate::config::Column;
use crate::sort::Order;
use crate::tasks::{Filter, FilterField};

/// What a column was picked for: `f` filters by its values, `s` sorts by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnPurpose {
    Filter,
    Sort,
}

/// What a picked color lands on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaintPick {
    Value(Column, String),
    Rows(String),
    Column(Column),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerPurpose {
    Filter(FilterField),
    Sort(Column),
    /// After `f`/`s`: which column; hidden ones listed last.
    ChooseColumn(ColumnPurpose),
    /// The `c` picker: which columns show, in what order.
    Columns,
    /// After `c m`: typed column numbers become the new order, live.
    MoveColumns(Vec<usize>),
    /// After `p`: what to paint.
    PaintTarget,
    /// A column's values, one color each.
    PaintValues(Column),
    /// After a target: which color.
    PaintColor(PaintPick),
    /// After `p c`: which column to paint whole.
    PaintColumn,
    /// `p o`: the rule hierarchy, top is the base.
    PaintRules,
}

/// What a row means when picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// A value of a field, a color name, or a sort order label.
    Text(String),
    Column(Column),
    Order(Order),
    /// Sort: clear the sort.
    NoSort,
    /// Color: clear the rule on this target.
    NoColor,
    /// Paint: one color per value of the column being painted.
    Auto,
    /// Paint: the task with this id.
    ThisRow(String),
    /// Paint: every row the filter matches.
    FilteredRows(String),
    /// Paint: pick a column to color whole.
    WholeColumn,
    /// Paint: open the rule hierarchy.
    OrderRules,
    DeleteAllPaint,
    /// Paint rules list: the rule at this position.
    Rule(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickOption {
    pub label: String,
    pub count: usize,
    /// A letter that picks this row directly; rows without one are numbered.
    pub key: Option<char>,
    pub payload: Payload,
}

impl PickOption {
    pub fn text(label: impl Into<String>, count: usize) -> PickOption {
        let label = label.into();
        PickOption {
            payload: Payload::Text(label.clone()),
            label,
            count,
            key: None,
        }
    }

    pub fn column(column: Column, hidden: bool) -> PickOption {
        let label = if hidden {
            format!("{}{}", column.name(), Column::HIDDEN_TAG)
        } else {
            column.name().to_string()
        };
        PickOption {
            label,
            count: 0,
            key: None,
            payload: Payload::Column(column),
        }
    }

    pub fn keyed(key: char, label: impl Into<String>, payload: Payload) -> PickOption {
        PickOption {
            label: label.into(),
            count: 0,
            key: Some(key),
            payload,
        }
    }

    pub fn numbered(label: impl Into<String>, payload: Payload) -> PickOption {
        PickOption {
            label: label.into(),
            count: 0,
            key: None,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuePicker {
    pub purpose: PickerPurpose,
    pub options: Vec<PickOption>,
    pub typed: String,
    /// A first digit waiting for a second when the numbered rows run past nine.
    pub number: String,
    /// Index into `matching()`.
    pub selected: usize,
}

impl ValuePicker {
    pub fn new(purpose: PickerPurpose, options: Vec<PickOption>) -> ValuePicker {
        ValuePicker {
            purpose,
            options,
            typed: String::new(),
            number: String::new(),
            selected: 0,
        }
    }

    /// Rows whose label starts with what has been typed; failing any, rows
    /// containing it. Case and spaces are ignored either way.
    pub fn matching(&self) -> Vec<PickOption> {
        let prefixed: Vec<PickOption> = self
            .options
            .iter()
            .filter(|option| Filter::loose_starts_with(&option.label, &self.typed))
            .cloned()
            .collect();
        if !prefixed.is_empty() {
            return prefixed;
        }
        self.options
            .iter()
            .filter(|option| Filter::loose_contains(&option.label, &self.typed))
            .cloned()
            .collect()
    }

    pub fn highlighted(&self) -> Option<PickOption> {
        self.matching().get(self.selected).cloned()
    }

    /// The key shown beside each matching row: its letter, or its number among
    /// the unlettered rows.
    pub fn row_keys(&self) -> Vec<String> {
        let mut number = 0;
        self.matching()
            .iter()
            .map(|option| match option.key {
                Some(key) => key.to_string(),
                None => {
                    number += 1;
                    number.to_string()
                }
            })
            .collect()
    }

    /// How many rows are numbered (for two-digit entry).
    pub fn numbered_count(&self) -> usize {
        self.matching()
            .iter()
            .filter(|option| option.key.is_none())
            .count()
    }

    /// Position in `matching()` of the row numbered `number`.
    pub fn position_of_number(&self, number: usize) -> Option<usize> {
        self.row_keys()
            .iter()
            .position(|key| *key == number.to_string())
    }

    /// Position in `matching()` of the row lettered `key`.
    pub fn position_of_key(&self, key: char) -> Option<usize> {
        self.matching()
            .iter()
            .position(|option| option.key == Some(key))
    }
}

/// The keys that work in this picker, shown inside the box and in the footer.
pub fn hint(picker: &ValuePicker) -> &'static str {
    match &picker.purpose {
        PickerPurpose::Filter(_) => "number or name picks one · space toggles · esc",
        PickerPurpose::Sort(_) => "number or name picks · esc",
        PickerPurpose::ChooseColumn(_) => "number or name · hidden columns listed last · esc",
        PickerPurpose::Columns => "number or name toggles · m reorder · g glyphs · K/J nudge · esc",
        PickerPurpose::MoveColumns(_) => "type column numbers in the order you want · enter done",
        PickerPurpose::PaintValues(_) => "value then color · repeats · h back · esc done",
        PickerPurpose::PaintColumn => "number or name · h back · esc",
        PickerPurpose::PaintTarget => "number or letter picks · esc",
        PickerPurpose::PaintColor(_) => "name or #hex · space clears · h back · esc",
        PickerPurpose::PaintRules => "K/J reorder · del removes · h back · esc",
    }
}
