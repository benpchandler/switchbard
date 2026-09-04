//! The column catalog: one row per column with everything the rest of the crate
//! needs to know about it. Adding a column is one row here plus its value accessor.

use switchbard_core::{BacklogTask, BACKLOG_PRIORITIES, CANONICAL_STATUS_ORDER};

use crate::ball::Ball;
use crate::tasks::FilterField;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Id,
    Status,
    Priority,
    Title,
    Labels,
    Project,
    Ball,
}

pub struct ColumnSpec {
    pub column: Column,
    /// The config and save-file spelling.
    pub name: &'static str,
    /// Accepted as an alias when parsing.
    pub alias: Option<&'static str>,
    /// The table header.
    pub header: &'static str,
    /// Text width; `None` means "take the remaining room".
    pub width: Option<u16>,
    /// The filter field backing this column, when its values are categories.
    pub field: Option<FilterField>,
    /// The order its values sort in semantically, if the vocabulary has one.
    pub vocabulary: &'static [&'static str],
    /// Whether `g` can section the list by this column: one value per task, few values.
    pub groupable: bool,
}

pub const COLUMNS: [ColumnSpec; 7] = [
    ColumnSpec {
        column: Column::Id,
        name: "id",
        alias: None,
        header: "id",
        width: Some(12),
        field: Some(FilterField::Id),
        vocabulary: &[],
        groupable: false,
    },
    ColumnSpec {
        column: Column::Status,
        name: "status",
        alias: None,
        header: "status",
        width: Some(12),
        field: Some(FilterField::Status),
        vocabulary: CANONICAL_STATUS_ORDER,
        groupable: true,
    },
    ColumnSpec {
        column: Column::Priority,
        name: "priority",
        alias: Some("pri"),
        header: "pri",
        width: Some(7),
        field: Some(FilterField::Priority),
        vocabulary: BACKLOG_PRIORITIES,
        groupable: true,
    },
    ColumnSpec {
        column: Column::Title,
        name: "title",
        alias: None,
        header: "title",
        width: None,
        field: None,
        vocabulary: &[],
        groupable: false,
    },
    ColumnSpec {
        column: Column::Labels,
        name: "labels",
        alias: None,
        header: "labels",
        width: Some(20),
        field: Some(FilterField::Label),
        vocabulary: &[],
        groupable: false,
    },
    ColumnSpec {
        column: Column::Project,
        name: "project",
        alias: None,
        header: "project",
        width: Some(18),
        field: Some(FilterField::Project),
        vocabulary: &[],
        groupable: true,
    },
    ColumnSpec {
        column: Column::Ball,
        name: "ball",
        alias: None,
        header: "ball",
        width: Some(6),
        field: Some(FilterField::Ball),
        vocabulary: &["me", "agent"],
        groupable: true,
    },
];

impl Column {
    /// Every column sbt knows, in catalog order. Shown columns are a user-ordered subset.
    pub const ALL: [Column; 7] = [
        Column::Id,
        Column::Status,
        Column::Priority,
        Column::Title,
        Column::Labels,
        Column::Project,
        Column::Ball,
    ];

    pub const DEFAULT_SHOWN: [Column; 4] =
        [Column::Id, Column::Status, Column::Priority, Column::Title];

    /// The `· hidden` tag picker lists add to a column that is not showing.
    pub const HIDDEN_TAG: &str = " · hidden";

    pub fn spec(self) -> &'static ColumnSpec {
        COLUMNS
            .iter()
            .find(|spec| spec.column == self)
            .expect("every column has a spec row")
    }

    pub fn parse(text: &str) -> Option<Column> {
        let text = text.trim_end_matches(Column::HIDDEN_TAG);
        COLUMNS
            .iter()
            .find(|spec| spec.name == text || spec.alias == Some(text))
            .map(|spec| spec.column)
    }

    pub fn name(self) -> &'static str {
        self.spec().name
    }

    pub fn header(self) -> &'static str {
        self.spec().header
    }

    pub fn groupable(self) -> bool {
        self.spec().groupable
    }

    /// The columns `g` can section by, in catalog order.
    pub fn groupable_columns() -> Vec<Column> {
        Column::ALL.into_iter().filter(|c| c.groupable()).collect()
    }

    /// The field whose values are categories, for `f`, paint by value, and glyphs.
    /// Id is a field for filtering but not a category.
    pub fn filter_field(self) -> Option<FilterField> {
        match self {
            Column::Id => None,
            _ => self.spec().field,
        }
    }

    /// Where `value` sits in the column's vocabulary (high before low, To Do before
    /// Done); unknown values and columns without a vocabulary rank last.
    pub fn vocabulary_rank(self, value: &str) -> usize {
        let vocabulary = self.spec().vocabulary;
        if vocabulary.is_empty() {
            return usize::MAX;
        }
        vocabulary
            .iter()
            .position(|known| known.eq_ignore_ascii_case(value))
            .unwrap_or(vocabulary.len())
    }

    /// The values a task carries in this column (labels can be several).
    pub fn values(self, task: &BacklogTask) -> Vec<String> {
        match self {
            Column::Id => vec![task.id.clone()],
            Column::Status => vec![task.status.clone()],
            Column::Priority => vec![task.priority.clone()],
            Column::Title => vec![task.title.clone()],
            Column::Labels => task.labels.clone(),
            Column::Project => task.project.clone().into_iter().collect(),
            Column::Ball => {
                let ball = Ball::text(Ball::of(task));
                if ball.is_empty() {
                    Vec::new()
                } else {
                    vec![ball.to_string()]
                }
            }
        }
    }

    /// The cell as text: the values joined. Filter and sort read this.
    pub fn cell_text(self, task: &BacklogTask) -> String {
        self.values(task).join(",")
    }

    /// Columns with a short form: id without its repo prefix, priority as H/M/L.
    pub fn abbreviable(self) -> bool {
        matches!(self, Column::Id | Column::Priority)
    }

    /// Abbreviated by default; a view can turn any of these off (`1a`, or `a` in `c`).
    pub const DEFAULT_ABBREVIATED: [Column; 2] = [Column::Id, Column::Priority];

    /// What the table shows. Abbreviated: the id without its repo prefix (the
    /// title bar names the repo; `80.3` is the part that varies), priority as
    /// H/M/L. The detail pane, reports, and filters always keep full values.
    pub fn display_text(self, task: &BacklogTask, abbreviated: bool) -> String {
        match (self, abbreviated) {
            (Column::Id, true) => bare_id(&task.id).to_string(),
            (Column::Priority, true) => task
                .priority
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase().to_string())
                .unwrap_or_default(),
            _ => self.cell_text(task),
        }
    }

    /// The widest the column may grow; the renderer fits it to content below that.
    pub fn max_width(self) -> Option<u16> {
        self.spec().width
    }
}

/// `TASK-80.3` -> `80.3`, `LED-648.11` -> `648.11`; an id with no prefix is itself.
pub fn bare_id(id: &str) -> &str {
    match id.rsplit_once('-') {
        Some((_, rest)) if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) => rest,
        _ => id,
    }
}
