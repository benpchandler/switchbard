//! The column catalog: one row per column with everything the rest of the crate
//! needs to know about it.
//!
//! Two kinds of column live here and behave identically downstream:
//!
//! * **built-ins** — the fixed rows of [`BUILTIN_COLUMNS`], one `Column` variant each;
//! * **custom** — one per field the repo declares in `backlog/config.yml`
//!   (`switchbard_core::FieldDecl`), reached through [`Column::Custom`].
//!
//! [`ColumnRegistry`] is the one authority for *what columns exist*. It is built
//! once per repo load, never per frame, and every lookup that used to read the
//! const catalog now goes through it. Adding a built-in column is one row in
//! [`BUILTIN_COLUMNS`] plus its value accessor in `crate::column_values`; adding a
//! custom one is a line in the repo's own config.
//!
//! # Why the registry is append-only
//!
//! A [`FieldId`] is an index, and `Column` is `Copy` — once a `Column::Custom(id)`
//! is sitting in a saved view, the live `ViewState`, a paint rule, or a config
//! glyph map, nothing can go back and renumber it. So [`ColumnRegistry::refresh`]
//! only ever *appends*: a field that leaves `backlog/config.yml` keeps its id and
//! is marked undeclared (dropped from the catalog and from name lookup, so it
//! disappears from the table and the pickers) rather than being removed and
//! letting the next field inherit its id. Re-declaring it later reuses that same
//! id. The cost is one dead spec per removed field for the life of the process;
//! what it buys is that a `Column` value can never silently start meaning a
//! different field mid-session.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;

use switchbard_core::{
    declared_value_rank, BacklogTask, FieldDecl, FieldKind, GoalDef, BACKLOG_PRIORITIES,
    CANONICAL_STATUS_ORDER,
};

use crate::filter::FilterField;

/// A custom field's place in [`ColumnRegistry`]; see the module doc for why it is
/// stable for the life of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FieldId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Id,
    Status,
    Priority,
    Title,
    Labels,
    Project,
    Ball,
    /// `yes` when an open (not-done) dependency blocks the task; a done task
    /// is never blocked. Backed by `switchbard_core::is_blocked` — see
    /// `tasks::TaskRelations`.
    Blocked,
    /// The optional target date (`due_date:`), `YYYY-MM-DD`.
    Due,
    /// Position in the repo's top list (the expedite lane); empty when not in it.
    Rank,
    /// The weekly goal(s) the task feeds: by scope, attachment, or attached project.
    Goal,
    /// One glyph per live agent session working the task; empty when none is.
    Work,
    Lifecycle,
    Tasks,
    Checks,
    Review,
    Merge,
    Draft,
    Filed,
    Merged,
    /// A field the repo declares for itself. Resolved through the registry that
    /// handed the id out; see the module doc.
    Custom(FieldId),
}

/// The order a column's values sort and section in, when it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vocabulary<'a> {
    /// A built-in column's fixed order.
    Fixed(&'a [&'static str]),
    /// An enum field's declared `values`, which are its declared order.
    Declared(&'a [String]),
}

impl Vocabulary<'_> {
    pub fn is_empty(&self) -> bool {
        match self {
            Vocabulary::Fixed(values) => values.is_empty(),
            Vocabulary::Declared(values) => values.is_empty(),
        }
    }

    pub fn iter(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        match self {
            Vocabulary::Fixed(values) => Box::new(values.iter().copied()),
            Vocabulary::Declared(values) => Box::new(values.iter().map(String::as_str)),
        }
    }
}

#[derive(Clone)]
pub struct ColumnSpec {
    pub column: Column,
    /// The config and save-file spelling.
    pub name: Cow<'static, str>,
    /// Accepted as an alias when parsing.
    pub alias: Option<&'static str>,
    /// The table header.
    pub header: Cow<'static, str>,
    /// Text width; `None` means "take the remaining room".
    pub width: Option<u16>,
    /// The filter field backing this column, when its values are categories.
    pub field: Option<FilterField>,
    /// A built-in column's semantic order; a custom column's comes from `decl`.
    fixed_vocabulary: &'static [&'static str],
    /// Whether `g` can section the list by this column: one value per task, few values.
    pub groupable: bool,
    pub multi_valued: bool,
    pub numeric: bool,
    /// The declaration behind a custom column; `None` for every built-in.
    decl: Option<FieldDecl>,
    /// Whether the repo still declares this column. Always true for built-ins;
    /// see the module doc for why a custom column can outlive its declaration.
    declared: bool,
}

impl ColumnSpec {
    /// The order this column's values sort and section in; empty when it has none.
    pub fn vocabulary(&self) -> Vocabulary<'_> {
        match &self.decl {
            Some(decl) if decl.kind == FieldKind::Enum => Vocabulary::Declared(&decl.values),
            Some(_) => Vocabulary::Declared(&[]),
            None => Vocabulary::Fixed(self.fixed_vocabulary),
        }
    }

    /// Where `value` sits in this column's order (high before low, To Do before
    /// Done); unknown values and columns without an order rank last. A custom
    /// column defers to `switchbard_core::declared_value_rank`, the same
    /// definition `sb list --sort` uses.
    pub fn vocabulary_rank(&self, value: &str) -> usize {
        if let Some(decl) = &self.decl {
            return declared_value_rank(decl, value);
        }
        if self.fixed_vocabulary.is_empty() {
            return usize::MAX;
        }
        self.fixed_vocabulary
            .iter()
            .position(|known| known.eq_ignore_ascii_case(value))
            .unwrap_or(self.fixed_vocabulary.len())
    }

    /// The declaration behind a custom column.
    pub fn decl(&self) -> Option<&FieldDecl> {
        self.decl.as_ref()
    }
}

/// A `static` rather than a `const`: `ColumnSpec` now owns heap types for the
/// custom case, and a `const` would materialize (and drop) the whole table at
/// every use site. Every row is a built-in: `decl: None`, always `declared`.
pub static BUILTIN_COLUMNS: [ColumnSpec; 20] = [
    ColumnSpec {
        column: Column::Id,
        name: Cow::Borrowed("id"),
        alias: None,
        header: Cow::Borrowed("id"),
        width: Some(12),
        field: Some(FilterField::Id),
        fixed_vocabulary: &[],
        groupable: false,
        multi_valued: false,
        numeric: true,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Status,
        name: Cow::Borrowed("status"),
        alias: None,
        header: Cow::Borrowed("status"),
        width: Some(12),
        field: Some(FilterField::Status),
        fixed_vocabulary: CANONICAL_STATUS_ORDER,
        groupable: true,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Priority,
        name: Cow::Borrowed("priority"),
        alias: Some("pri"),
        header: Cow::Borrowed("pri"),
        width: Some(7),
        field: Some(FilterField::Priority),
        fixed_vocabulary: BACKLOG_PRIORITIES,
        groupable: true,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Title,
        name: Cow::Borrowed("title"),
        alias: None,
        header: Cow::Borrowed("title"),
        width: None,
        field: None,
        fixed_vocabulary: &[],
        groupable: false,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Labels,
        name: Cow::Borrowed("labels"),
        alias: None,
        header: Cow::Borrowed("labels"),
        width: Some(20),
        field: Some(FilterField::Label),
        fixed_vocabulary: &[],
        groupable: false,
        multi_valued: true,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Project,
        name: Cow::Borrowed("project"),
        alias: None,
        header: Cow::Borrowed("project"),
        width: Some(18),
        field: Some(FilterField::Project),
        fixed_vocabulary: &[],
        groupable: true,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Ball,
        name: Cow::Borrowed("ball"),
        alias: None,
        header: Cow::Borrowed("ball"),
        width: Some(14),
        field: Some(FilterField::Ball),
        fixed_vocabulary: &["me", "agent"],
        groupable: true,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Blocked,
        name: Cow::Borrowed("blocked"),
        alias: None,
        header: Cow::Borrowed("blocked"),
        width: Some(7),
        field: Some(FilterField::Blocked),
        fixed_vocabulary: &["yes", "no"],
        groupable: true,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Due,
        name: Cow::Borrowed("due"),
        alias: None,
        header: Cow::Borrowed("due"),
        width: Some(10),
        field: Some(FilterField::Due),
        fixed_vocabulary: &[],
        groupable: false,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Rank,
        name: Cow::Borrowed("rank"),
        alias: Some("top"),
        header: Cow::Borrowed("#"),
        width: Some(4),
        field: None,
        fixed_vocabulary: &[],
        groupable: false,
        multi_valued: false,
        numeric: true,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Goal,
        name: Cow::Borrowed("goal"),
        alias: None,
        header: Cow::Borrowed("goal"),
        width: Some(18),
        field: Some(FilterField::Goal),
        fixed_vocabulary: &[],
        groupable: true,
        multi_valued: true,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Work,
        name: Cow::Borrowed("work"),
        alias: None,
        header: Cow::Borrowed("work"),
        width: Some(4),
        field: None,
        fixed_vocabulary: &[],
        groupable: false,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Lifecycle,
        name: Cow::Borrowed("lifecycle"),
        alias: None,
        header: Cow::Borrowed("State"),
        width: Some(7),
        field: Some(FilterField::Status),
        fixed_vocabulary: &["Open", "Closed", "Merged"],
        groupable: false,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Tasks,
        name: Cow::Borrowed("tasks"),
        alias: None,
        header: Cow::Borrowed("Tasks"),
        width: Some(12),
        field: Some(FilterField::Tasks),
        fixed_vocabulary: &[],
        groupable: false,
        multi_valued: true,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Checks,
        name: Cow::Borrowed("checks"),
        alias: None,
        header: Cow::Borrowed("Checks"),
        width: Some(14),
        field: Some(FilterField::Checks),
        fixed_vocabulary: &[
            "Failed",
            "Unknown",
            "Pending",
            "Passed",
            "None observed",
            "Not fetched",
        ],
        groupable: false,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Review,
        name: Cow::Borrowed("review"),
        alias: None,
        header: Cow::Borrowed("Review"),
        width: Some(18),
        field: Some(FilterField::Review),
        fixed_vocabulary: &[
            "Changes requested",
            "Review unknown",
            "Review required",
            "Approved",
        ],
        groupable: false,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Merge,
        name: Cow::Borrowed("merge"),
        alias: None,
        header: Cow::Borrowed("Merge"),
        width: Some(18),
        field: Some(FilterField::Merge),
        fixed_vocabulary: &[
            "Merge conflict",
            "Mergeability unknown",
            "No merge conflict",
        ],
        groupable: false,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Draft,
        name: Cow::Borrowed("draft"),
        alias: None,
        header: Cow::Borrowed("Draft"),
        width: Some(7),
        field: Some(FilterField::Draft),
        fixed_vocabulary: &["Draft", "Ready"],
        groupable: false,
        multi_valued: false,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Filed,
        name: Cow::Borrowed("filed"),
        alias: Some("created"),
        header: Cow::Borrowed("filed (UTC)"),
        width: Some(16),
        field: Some(FilterField::Filed),
        fixed_vocabulary: crate::date_fields::BUCKETS,
        groupable: false,
        multi_valued: true,
        numeric: false,
        decl: None,
        declared: true,
    },
    ColumnSpec {
        column: Column::Merged,
        name: Cow::Borrowed("merged"),
        alias: Some("merged_at"),
        header: Cow::Borrowed("merged (UTC)"),
        width: Some(16),
        field: Some(FilterField::Merged),
        fixed_vocabulary: crate::date_fields::BUCKETS,
        groupable: false,
        multi_valued: true,
        numeric: false,
        decl: None,
        declared: true,
    },
];

/// How wide a custom column may grow before the renderer truncates it.
const CUSTOM_COLUMN_WIDTH: u16 = 18;

/// What columns exist, built once per repo load. See the module doc.
#[derive(Default)]
pub struct ColumnRegistry {
    /// One entry per custom field seen this session, in first-seen order;
    /// `FieldId` indexes it. Append-only — see the module doc.
    custom: Vec<ColumnSpec>,
}

impl ColumnRegistry {
    /// Built-ins only: the Pull Requests page, and any caller with no repo.
    pub fn builtin_only() -> ColumnRegistry {
        ColumnRegistry::default()
    }

    /// The registry for a repo's declared fields. A repo that declares none, or
    /// whose `backlog/config.yml` cannot be read, yields the built-ins alone —
    /// a custom column is an addition to the table, never a precondition for it.
    pub fn for_repo(root: &Path) -> ColumnRegistry {
        let mut registry = ColumnRegistry::builtin_only();
        registry.refresh(&switchbard_core::declared_fields(root).unwrap_or_default());
        registry
    }

    /// Take up the repo's current declarations: new fields are appended, changed
    /// ones are updated in place, and a field no longer declared keeps its id but
    /// leaves the catalog. See the module doc for why this never renumbers.
    pub fn refresh(&mut self, decls: &[FieldDecl]) {
        for spec in &mut self.custom {
            spec.declared = false;
        }
        for decl in decls {
            match self
                .custom
                .iter()
                .position(|spec| spec.name == decl.name.as_str())
            {
                Some(index) => {
                    let column = self.custom[index].column;
                    self.custom[index] = custom_spec(column, decl);
                }
                None => {
                    let column = Column::Custom(FieldId(self.custom.len()));
                    self.custom.push(custom_spec(column, decl));
                }
            }
        }
    }

    /// Every column's spec, built-in or custom.
    pub fn spec(&self, column: Column) -> &ColumnSpec {
        match column {
            Column::Custom(FieldId(index)) => self.custom.get(index).expect(
                "invariant: a FieldId is only handed out by this registry, which never shrinks",
            ),
            other => BUILTIN_COLUMNS
                .iter()
                .find(|spec| spec.column == other)
                .expect("invariant: every built-in column has a row in BUILTIN_COLUMNS"),
        }
    }

    /// The column a saved view, config key, or typed name refers to. Accepts a
    /// declared field either bare (what a user types, and what `tui.lua` keys
    /// spell) or under [`FIELD_PREFIX`] (what a saved view writes). A name the
    /// repo no longer declares resolves to nothing, which is what drops it from
    /// a loaded view.
    pub fn parse(&self, text: &str) -> Option<Column> {
        let text = text.trim().trim_end_matches(HIDDEN_TAG);
        BUILTIN_COLUMNS
            .iter()
            .find(|spec| spec.name == text || spec.alias == Some(text))
            .map(|spec| spec.column)
            .or_else(|| self.parse_custom(text.strip_prefix(FIELD_PREFIX).unwrap_or(text)))
    }

    /// A declared custom field by its bare name.
    pub fn parse_custom(&self, text: &str) -> Option<Column> {
        self.custom
            .iter()
            .find(|spec| spec.declared && spec.name == text)
            .map(|spec| spec.column)
    }

    /// Whether `name` is a saved reference to a repo-declared field this repo
    /// does not declare any more: it carries [`FIELD_PREFIX`], so sbt itself
    /// wrote it and knows exactly what it meant.
    ///
    /// The prefix exists for this one question. A saved view naming something
    /// sbt cannot resolve is otherwise preserved untouched and blocks saving —
    /// the record may come from a newer build, and overwriting it would destroy
    /// work. A dropped field is the one case where sbt *does* know the name was
    /// its own and can safely drop the column with a note instead. Nothing
    /// syntactic separates `field:counterparty` from a future build's
    /// `counterparty` column; the prefix is what makes the answer exact.
    pub fn unknown_field_name(&self, name: &str) -> bool {
        let Some(bare) = name.trim().strip_prefix(FIELD_PREFIX) else {
            return false;
        };
        // `self.parse`, not `parse_custom`: a declaration can never take a
        // built-in key (`switchbard_core::BUILTIN_FIELD_KEYS`), so `field:status`
        // is a malformed record to preserve, not a field to drop.
        switchbard_core::valid_field_name(bare) && self.parse(bare).is_none()
    }

    /// The task page's catalog: the built-in task columns, then this repo's
    /// declared fields in declaration order.
    pub fn task_columns(&self) -> Vec<Column> {
        BUILTIN_TASK_COLUMNS
            .iter()
            .copied()
            .chain(self.declared_custom())
            .collect()
    }

    /// The columns `g` can section by, in catalog order.
    pub fn groupable_task_columns(&self) -> Vec<Column> {
        self.task_columns()
            .into_iter()
            .filter(|column| self.spec(*column).groupable)
            .collect()
    }

    /// Whether this registry already reflects exactly `decls`, in order. The
    /// reload path asks before rebuilding, so an unchanged `backlog/config.yml`
    /// costs one comparison instead of a new registry and a re-sanitized view.
    pub fn is_current(&self, decls: &[FieldDecl]) -> bool {
        let declared: Vec<&FieldDecl> = self
            .custom
            .iter()
            .filter(|spec| spec.declared)
            .filter_map(|spec| spec.decl.as_ref())
            .collect();
        declared.len() == decls.len()
            && declared
                .iter()
                .zip(decls)
                .all(|(mine, theirs)| *mine == theirs)
    }

    /// This registry with `decls` taken up, keeping every id it has handed out
    /// (see the module doc). The caller publishes the result; nothing mutates
    /// the registry other callers are still holding.
    pub fn reloaded(&self, decls: &[FieldDecl]) -> ColumnRegistry {
        let mut next = ColumnRegistry {
            custom: self.custom.clone(),
        };
        next.refresh(decls);
        next
    }

    /// This repo's declared custom columns, in declaration order.
    pub fn declared_custom(&self) -> impl Iterator<Item = Column> + '_ {
        self.custom
            .iter()
            .filter(|spec| spec.declared)
            .map(|spec| spec.column)
    }
}

fn custom_spec(column: Column, decl: &FieldDecl) -> ColumnSpec {
    let Column::Custom(id) = column else {
        unreachable!("invariant: a custom spec is only ever built for Column::Custom");
    };
    ColumnSpec {
        column,
        name: Cow::Owned(decl.name.clone()),
        alias: None,
        header: Cow::Owned(decl.name.clone()),
        width: Some(CUSTOM_COLUMN_WIDTH),
        // A declared field is categorical by construction: `f`, glyphs, and
        // paint-by-value all read the same values the column shows.
        field: Some(FilterField::Custom(id)),
        fixed_vocabulary: &[],
        groupable: decl.groupable,
        multi_valued: false,
        numeric: false,
        decl: Some(decl.clone()),
        declared: true,
    }
}

/// The `· hidden` tag picker lists add to a column that is not showing.
pub const HIDDEN_TAG: &str = " · hidden";

/// How a saved view spells a repo-declared field, so that sbt can tell its own
/// dropped field from a column it has never heard of. See
/// [`ColumnRegistry::unknown_field_name`].
pub const FIELD_PREFIX: &str = "field:";

/// The built-in task columns, in catalog order. The registry appends the repo's
/// own fields after them; a shown set is a user-ordered subset of both.
const BUILTIN_TASK_COLUMNS: [Column; 13] = [
    Column::Id,
    Column::Status,
    Column::Priority,
    Column::Title,
    Column::Labels,
    Column::Project,
    Column::Ball,
    Column::Blocked,
    Column::Due,
    Column::Rank,
    Column::Goal,
    Column::Work,
    Column::Filed,
];

impl Column {
    pub const PR_ALL: [Column; 9] = [
        Column::Id,
        Column::Lifecycle,
        Column::Tasks,
        Column::Checks,
        Column::Title,
        Column::Review,
        Column::Merge,
        Column::Draft,
        Column::Merged,
    ];

    pub const PR_DEFAULT: [Column; 5] = [
        Column::Id,
        Column::Lifecycle,
        Column::Tasks,
        Column::Checks,
        Column::Title,
    ];

    pub const DEFAULT_SHOWN: [Column; 4] =
        [Column::Id, Column::Status, Column::Priority, Column::Title];

    /// The `· hidden` tag picker lists add to a column that is not showing.
    pub const HIDDEN_TAG: &str = HIDDEN_TAG;

    pub fn spec(self, registry: &ColumnRegistry) -> &ColumnSpec {
        registry.spec(self)
    }

    pub fn name(self, registry: &ColumnRegistry) -> &str {
        &registry.spec(self).name
    }

    /// How a saved view, sort, grouping, or paint rule writes this column:
    /// a built-in by name, a declared field under [`FIELD_PREFIX`]. Reading
    /// accepts either spelling; only writing adds the prefix.
    pub fn save_name(self, registry: &ColumnRegistry) -> String {
        match self {
            Column::Custom(_) => format!("{FIELD_PREFIX}{}", self.name(registry)),
            _ => self.name(registry).to_string(),
        }
    }

    pub fn label(self, registry: &ColumnRegistry) -> &str {
        match self {
            Self::Filed => "When task filed",
            Self::Merged => "When merged",
            _ => self.name(registry),
        }
    }

    /// The date-bucket columns (`filed`, `merged`), whose values are periods
    /// rather than days. `due`, and a declared date field, are not: they carry
    /// one exact day.
    pub fn is_date(self) -> bool {
        matches!(self, Self::Filed | Self::Merged)
    }

    pub fn header(self, registry: &ColumnRegistry) -> &str {
        &registry.spec(self).header
    }

    pub fn groupable(self, registry: &ColumnRegistry) -> bool {
        registry.spec(self).groupable
    }

    /// The field whose values are categories, for `f`, paint by value, and glyphs.
    /// Id is a field for filtering but not a category.
    pub fn filter_field(self, registry: &ColumnRegistry) -> Option<FilterField> {
        match self {
            Column::Id => None,
            _ => registry.spec(self).field,
        }
    }

    /// Where `value` sits in the column's vocabulary (high before low, To Do before
    /// Done); unknown values and columns without a vocabulary rank last.
    pub fn vocabulary_rank(self, registry: &ColumnRegistry, value: &str) -> usize {
        registry.spec(self).vocabulary_rank(value)
    }

    /// The values a task carries in this column (labels and goals can be several).
    /// `goals` is the repo's goal set and `blocked` the blocked-task ids
    /// (`tasks::TaskRelations::blocked`): both are repo-level facts derived
    /// once per load, not stored on the task itself.
    pub fn values(
        self,
        registry: &ColumnRegistry,
        task: &BacklogTask,
        goals: &[GoalDef],
        blocked: &HashSet<String>,
    ) -> Vec<String> {
        crate::column_values::ColumnValues::values(
            &crate::column_values::TaskValues {
                registry,
                task,
                goals,
                top: &[],
                blocked,
            },
            self,
        )
    }

    pub fn pr_values(
        self,
        row: &switchbard_core::PrListRow,
        links: &[(String, String)],
    ) -> Vec<String> {
        crate::column_values::ColumnValues::values(
            &crate::column_values::PrValues { row, links },
            self,
        )
    }

    /// The cell as text: the values joined. Filter and sort read this.
    pub fn cell_text(
        self,
        registry: &ColumnRegistry,
        task: &BacklogTask,
        goals: &[GoalDef],
        blocked: &HashSet<String>,
    ) -> String {
        if self.is_date() {
            self.values(registry, task, goals, blocked)
                .into_iter()
                .next()
                .unwrap_or_default()
        } else {
            self.values(registry, task, goals, blocked).join(",")
        }
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
    pub fn display_text(
        self,
        registry: &ColumnRegistry,
        task: &BacklogTask,
        abbreviated: bool,
        goals: &[GoalDef],
        blocked: &HashSet<String>,
    ) -> String {
        match (self, abbreviated) {
            (Column::Id, true) => bare_id(&task.id).to_string(),
            (Column::Priority, true) => task
                .priority
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase().to_string())
                .unwrap_or_default(),
            _ => self.cell_text(registry, task, goals, blocked),
        }
    }

    /// The widest the column may grow; the renderer fits it to content below that.
    pub fn max_width(self, registry: &ColumnRegistry) -> Option<u16> {
        registry.spec(self).width
    }
}

/// `TASK-80.3` -> `80.3`, `LED-648.11` -> `648.11`; an id with no prefix is itself.
pub fn bare_id(id: &str) -> &str {
    match id.rsplit_once('-') {
        Some((_, rest)) if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) => rest,
        _ => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(name: &str, values: &[&str], groupable: bool) -> FieldDecl {
        FieldDecl {
            name: name.to_string(),
            kind: if values.is_empty() {
                FieldKind::Text
            } else {
                FieldKind::Enum
            },
            values: values.iter().map(|v| (*v).to_string()).collect(),
            groupable,
        }
    }

    #[test]
    fn a_declared_field_joins_the_task_catalog_after_the_built_ins() {
        let mut registry = ColumnRegistry::builtin_only();
        registry.refresh(&[decl("counterparty", &["GoSBA Loans", "Nick"], true)]);
        let columns = registry.task_columns();
        assert_eq!(columns.len(), BUILTIN_TASK_COLUMNS.len() + 1);
        let column = *columns.last().unwrap();
        assert_eq!(column.name(&registry), "counterparty");
        assert_eq!(registry.parse("counterparty"), Some(column));
        assert!(registry.groupable_task_columns().contains(&column));
    }

    #[test]
    fn an_enum_field_ranks_its_values_in_declared_order_with_the_unknown_last() {
        let mut registry = ColumnRegistry::builtin_only();
        registry.refresh(&[decl("counterparty", &["GoSBA Loans", "Nick"], true)]);
        let column = registry.parse("counterparty").unwrap();
        assert_eq!(column.vocabulary_rank(&registry, "GoSBA Loans"), 0);
        assert_eq!(column.vocabulary_rank(&registry, "Nick"), 1);
        assert_eq!(column.vocabulary_rank(&registry, "Someone else"), 2);
    }

    /// The invariant the module doc names: a removed field's id is never reused.
    #[test]
    fn refresh_never_renumbers_a_field_that_already_has_an_id() {
        let mut registry = ColumnRegistry::builtin_only();
        registry.refresh(&[
            decl("counterparty", &["Nick"], true),
            decl("stage", &[], false),
        ]);
        let counterparty = registry.parse("counterparty").unwrap();
        let stage = registry.parse("stage").unwrap();

        registry.refresh(&[decl("stage", &[], false)]);
        assert_eq!(registry.parse("counterparty"), None, "no longer declared");
        assert_eq!(registry.parse("stage"), Some(stage), "id unchanged");
        assert_eq!(
            registry.spec(counterparty).name,
            "counterparty",
            "the dropped column still resolves rather than aliasing another field"
        );
        assert!(!registry.task_columns().contains(&counterparty));

        registry.refresh(&[
            decl("stage", &[], false),
            decl("counterparty", &["Nick"], true),
        ]);
        assert_eq!(
            registry.parse("counterparty"),
            Some(counterparty),
            "re-declaring reuses the original id"
        );
    }

    #[test]
    fn refresh_picks_up_an_edited_declaration_in_place() {
        let mut registry = ColumnRegistry::builtin_only();
        registry.refresh(&[decl("counterparty", &["Nick"], false)]);
        let column = registry.parse("counterparty").unwrap();
        assert!(!column.groupable(&registry));
        registry.refresh(&[decl("counterparty", &["Nick", "GoSBA Loans"], true)]);
        assert_eq!(registry.parse("counterparty"), Some(column));
        assert!(column.groupable(&registry));
        assert_eq!(column.vocabulary_rank(&registry, "GoSBA Loans"), 1);
    }

    /// The prefix is the whole point: a bare name sbt cannot resolve may be a
    /// newer build's column and must be preserved, never dropped.
    #[test]
    fn only_a_prefixed_name_counts_as_a_field_this_repo_dropped() {
        let mut registry = ColumnRegistry::builtin_only();
        registry.refresh(&[decl("counterparty", &["Nick"], true)]);
        assert!(registry.unknown_field_name("field:stage"));
        assert!(
            !registry.unknown_field_name("stage"),
            "could be a new column"
        );
        assert!(
            !registry.unknown_field_name("field:counterparty"),
            "declared"
        );
        assert!(!registry.unknown_field_name("field:status"), "a built-in");
        assert!(!registry.unknown_field_name("field:Stage"), "not a name");
    }

    #[test]
    fn a_declared_field_reads_back_from_either_spelling_and_writes_the_prefixed_one() {
        let mut registry = ColumnRegistry::builtin_only();
        registry.refresh(&[decl("counterparty", &["Nick"], true)]);
        let column = registry.parse("counterparty").expect("bare");
        assert_eq!(registry.parse("field:counterparty"), Some(column));
        assert_eq!(column.save_name(&registry), "field:counterparty");
        assert_eq!(
            Column::Status.save_name(&registry),
            "status",
            "a built-in is never prefixed"
        );
    }

    #[test]
    fn a_text_field_has_no_vocabulary_so_nothing_offers_it_a_semantic_order() {
        let mut registry = ColumnRegistry::builtin_only();
        registry.refresh(&[decl("note", &[], false)]);
        let column = registry.parse("note").unwrap();
        assert!(registry.spec(column).vocabulary().is_empty());
    }
}
