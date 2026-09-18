//! The one list every menu uses: numbered rows, lettered rows, type-ahead.
//! A row carries a typed payload so the app dispatches on what was picked,
//! never on the label text.

use crate::columns::Column;
use crate::sort::Order;
use crate::tasks::{Filter, FilterField};
use switchbard_core::Ball;

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
    Title,
    Header,
    Heading(String),
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
    /// One named column moves live; Enter accepts, Esc restores its starting order.
    MoveColumn(Column),
    /// After `p`: what to paint.
    PaintTarget,
    PaintRowValues,
    PaintHeadings,
    /// A column's values, one color each.
    PaintValues(Column),
    /// Step one of the style picker: the fill the cells wear. What is being
    /// composed lives in `App::paint_draft`, not here, so toggling ink in step
    /// two never rewrites this purpose and `←` keeps meaning "back a step".
    PaintHighlight,
    /// Step two: the ink written over that fill. Space adds a token, Enter
    /// applies what the title is previewing.
    PaintText,
    /// After `p c`: which column to paint whole.
    PaintColumn,
    /// `p o`: the rule hierarchy, top is the base.
    PaintRules,
    ChoosePaintRule(PaintRuleAction),
    /// After a digit in browse: everything that can be done with that column.
    ColumnActions(Column),
    /// `,`: standing preferences under every view.
    Settings,
    Experiments,
    Experiment(String),
    /// `tg`: the repo's goals, marked where the named task is attached; picking toggles.
    Goals(String),
    /// `o`: what to organize the list by; the current choice is marked.
    Organize,
    /// `t b`: who should act next on the selected task.
    Ball,
    Merge,
    Task,
    TaskCancel,
    AgentKill,
    TaskStatus(String),
    TaskPlanning(String),
    TaskProject(String),
    TaskParent(String),
    TopList,
    /// The detail pane's own status/priority/project/labels pickers
    /// (TASK-222): distinct from `Task*` above because they return focus to
    /// `Mode::DetailFocus` on close rather than to `Mode::Browse` — see
    /// `PickerPurpose::is_detail`.
    DetailStatus(String),
    DetailPlanning(String),
    DetailPriority(String),
    DetailProject(String),
    DetailLabels(String),
    Views,
    History,
    SaveView,
    GlobalView,
    /// `v n`: which slot to name.
    RenameView,
    /// `v x`: which slot to delete.
    DeleteView,
    ChooseColumnAction(ColumnAction),
}

impl PickerPurpose {
    /// Whether closing this picker (Esc, `h`/Left back, or a completed pick)
    /// returns to `Mode::DetailFocus` rather than `Mode::Browse` — the
    /// detail pane's own field pickers, opened while the pane has focus.
    pub fn is_detail(&self) -> bool {
        matches!(
            self,
            Self::DetailPlanning(_)
                | Self::DetailStatus(_)
                | Self::DetailPriority(_)
                | Self::DetailProject(_)
                | Self::DetailLabels(_)
        )
    }
}

/// What a column's menu offers; each row is one of these on a letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnAction {
    Filter,
    Sort,
    Group,
    Paint,
    Glyphs,
    Abbreviate,
    Hide,
    Move,
    Earlier,
    Later,
}

impl ColumnAction {
    pub fn key(self) -> char {
        match self {
            ColumnAction::Filter => 'f',
            ColumnAction::Sort => 's',
            ColumnAction::Group => 'o',
            ColumnAction::Paint => 'p',
            ColumnAction::Glyphs => 'g',
            ColumnAction::Abbreviate => 'a',
            ColumnAction::Hide => 'x',
            ColumnAction::Move => 'm',
            ColumnAction::Earlier => 'K',
            ColumnAction::Later => 'J',
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ColumnAction::Filter => "filter by its values",
            ColumnAction::Sort => "sort by it",
            ColumnAction::Group => "outline by it",
            ColumnAction::Paint => "paint by it",
            ColumnAction::Glyphs => "glyphs on/off",
            ColumnAction::Abbreviate => "abbreviate on/off",
            ColumnAction::Hide => "hide it",
            ColumnAction::Move => "move columns",
            ColumnAction::Earlier => "move column earlier",
            ColumnAction::Later => "move column later",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskAction {
    New,
    Edit,
    Cancel,
    Ball,
    Append,
    Status,
    Planning,
    Project,
    Parent,
    TopList,
    Drop,
    Pin,
    Goals,
    /// The `d` fast path: mark the selected task Done without opening the
    /// full status picker.
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintRuleAction {
    Earlier,
    Later,
    Delete,
}

/// What a row means when picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    ExperimentTry,
    ExperimentNext,
    ExperimentHide,
    /// A value of a field, a color name, or a sort order label.
    Text(String),
    Column(Column),
    Order(Order),
    /// Sort: clear the sort.
    NoSort,
    /// Organize: what to organize by (flat when empty).
    Grouping(crate::group::Grouping),
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
    SelectedRowValues,
    GroupHeadings,
    PaintScope(PaintPick),
    /// Paint: open the rule hierarchy.
    OrderRules,
    DeleteAllPaint,
    /// Paint rules list: the rule at this position.
    Rule(usize),
    PaintRuleAction(PaintRuleAction),
    ColumnAction(ColumnAction),
    Ball(Option<Ball>),
    NewBallHolder,
    /// The detail pane's labels picker: open the "type a new label" input.
    NewLabel,
    TaskAction(TaskAction),
    Rank(usize),
    ViewSlot(usize),
    ViewHistory,
    HistoryView(String),
    SaveView,
    GlobalView,
    RenameView,
    DeleteView,
    GlobalSettings,
    Experiments,
    Experiment(String),
    ExperimentDecision(crate::experiments::ExperimentDecision),
    Update,
    TitleWrapping,
    RowSpacing,
    Project(Option<String>),
    Parent(Option<String>),
    KeepTask,
    ConfirmTaskCancel,
    CancelMerge,
    CancelAgentKill,
    ConfirmAgentKill,
    Merge(switchbard_core::PrMergeMethod),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickOption {
    pub label: String,
    pub count: usize,
    /// A letter that picks this row directly; rows without one are numbered.
    pub key: Option<char>,
    pub payload: Payload,
}

#[derive(Clone)]
pub struct ExperimentButtonHit {
    pub area: ratatui::layout::Rect,
    pub id: String,
    /// None opens the existing decision menu for an already reviewed feature.
    pub decision: Option<crate::experiments::ExperimentDecision>,
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

    pub fn paint_column(
        registry: &crate::columns::ColumnRegistry,
        column: Column,
        hidden: bool,
    ) -> PickOption {
        let mut option = Self::column(registry, column, hidden);
        option.label = if hidden {
            format!("{}{}", column.label(registry), Column::HIDDEN_TAG)
        } else {
            column.label(registry).to_string()
        };
        option
    }

    pub fn column(
        registry: &crate::columns::ColumnRegistry,
        column: Column,
        hidden: bool,
    ) -> PickOption {
        let label = if hidden {
            format!("{}{}", column.name(registry), Column::HIDDEN_TAG)
        } else {
            column.name(registry).to_string()
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
    /// Wrapped selected history entry preview, independent of list selection.
    pub preview_scroll: u16,
}

impl ValuePicker {
    pub fn new(purpose: PickerPurpose, options: Vec<PickOption>) -> ValuePicker {
        ValuePicker {
            purpose,
            options,
            typed: String::new(),
            number: String::new(),
            selected: 0,
            preview_scroll: 0,
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
        PickerPurpose::Columns => "↑↓/jk select · →/l open · ←/h back · Esc closes",
        PickerPurpose::MoveColumns(_) => "type column numbers in the order you want · enter done",
        PickerPurpose::MoveColumn(_) => "←→/hl move · Enter accepts · Esc restores",
        PickerPurpose::PaintValues(_) => "value then color · repeats · h back · esc done",
        PickerPurpose::PaintColumn | PickerPurpose::PaintHeadings => {
            "number or name · h back · esc"
        }
        PickerPurpose::PaintRowValues => "paints this value wherever it appears · ← back · esc",
        PickerPurpose::PaintTarget => "number or letter picks · esc",
        PickerPurpose::PaintHighlight => {
            "the fill · none leaves the cell bare · name or number picks · ← back · esc"
        }
        PickerPurpose::PaintText => "space adds a token · enter applies · ← back to the fill · esc",
        PickerPurpose::PaintRules | PickerPurpose::ChoosePaintRule(_) => {
            "↑/↓ select · key or Enter picks · h back · Esc closes"
        }
        PickerPurpose::ColumnActions(_) => "letter picks · esc",
        PickerPurpose::Settings | PickerPurpose::Experiments | PickerPurpose::Experiment(_) => {
            "↑↓/jk select · →/l open · ←/h back · Esc closes"
        }
        PickerPurpose::Goals(_) => "number or name attaches or detaches · esc",
        PickerPurpose::Organize => {
            "number or name organizes · the current one again flattens · x off · esc"
        }
        PickerPurpose::TaskCancel => "c confirms cancellation · Enter/Esc keeps task",
        PickerPurpose::AgentKill => "j/k select · Enter confirms selection · Esc cancels",
        PickerPurpose::Merge => "number confirms · j/k select · Enter confirms · Esc cancels",
        PickerPurpose::Views if picker.position_of_key('l').is_some() => {
            "l line wrap · ↑↓/jk select · →/Enter open · ←/h back · Esc closes"
        }
        PickerPurpose::Task
        | PickerPurpose::TaskPlanning(_)
        | PickerPurpose::TaskStatus(_)
        | PickerPurpose::TaskProject(_)
        | PickerPurpose::TopList
        | PickerPurpose::Views
        | PickerPurpose::SaveView
        | PickerPurpose::GlobalView
        | PickerPurpose::RenameView
        | PickerPurpose::DeleteView
        | PickerPurpose::ChooseColumnAction(_) => "↑↓/jk select · →/l open · ←/h back · Esc closes",
        PickerPurpose::History => {
            "↑↓ select · type to find · Enter restores · v s number saves · Esc"
        }
        PickerPurpose::TaskParent(_) => "type ID/title · ↑↓ select · Enter saves · ← back · Esc",
        PickerPurpose::Ball => "number or name picks · new person opens entry · esc",
        PickerPurpose::DetailPlanning(_)
        | PickerPurpose::DetailStatus(_)
        | PickerPurpose::DetailPriority(_)
        | PickerPurpose::DetailProject(_) => "number or name picks · esc returns to the pane",
        PickerPurpose::DetailLabels(_) => {
            "enter toggles · n adds a new label · esc returns to the pane"
        }
    }
}
