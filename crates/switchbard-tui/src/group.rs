//! Grouping: a projection over the already-filtered, already-sorted task order
//! into rows, where a heading is an ordinary row the cursor skips.

use std::collections::HashSet;

use switchbard_core::{BacklogTask, GoalDef};

use crate::columns::{Column, ColumnRegistry};
use crate::tasks::{GoalSummary, ProjectSummary};

/// Everything a heading can say beyond the section's key: project facts by
/// stack rank, goal facts in `goals.yml` order, the goal defs membership
/// derives from, and blocked-task ids (`tasks::TaskRelations::blocked`) for
/// grouping by the `blocked` column.
pub struct Headings<'a> {
    /// What columns exist: section keys for a repo-declared field read through it.
    pub registry: &'a ColumnRegistry,
    pub projects: &'a [ProjectSummary],
    pub goals: &'a [GoalDef],
    pub goal_summaries: &'a [GoalSummary],
    pub blocked: &'a HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A section heading; `depth` 0 is outermost, 1 is nested inside it.
    Heading {
        text: String,
        depth: usize,
    },
    Task(usize),
}

/// What the list is organized by: nothing, one to `MAX_DEPTH` columns nested
/// in order (`project›goal`), or `auto` — resolved fresh from the filtered
/// set by [`auto_levels`] rather than stored. The one spelling every surface
/// uses: the `o` picker, `:outline` (`:group` still works), the title bar,
/// and saved views.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Levels {
    Fixed(Vec<Column>),
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grouping(Levels);

impl Default for Grouping {
    fn default() -> Grouping {
        Grouping::flat()
    }
}

impl Grouping {
    pub const MAX_DEPTH: usize = 4;

    pub fn flat() -> Grouping {
        Grouping(Levels::Fixed(Vec::new()))
    }

    pub fn by(column: Column) -> Grouping {
        Grouping(Levels::Fixed(vec![column]))
    }

    pub fn nested(outer: Column, inner: Column) -> Grouping {
        Grouping(Levels::Fixed(vec![outer, inner]))
    }

    /// `auto`: resolved fresh from the filtered set by [`auto_levels`], never
    /// stored as concrete columns. See the module doc.
    pub fn auto() -> Grouping {
        Grouping(Levels::Auto)
    }

    pub fn is_auto(&self) -> bool {
        matches!(self.0, Levels::Auto)
    }

    /// The levels this grouping literally names; empty for `auto`, whose
    /// levels only exist once [`auto_levels`] has resolved them for a
    /// particular filtered set.
    pub fn levels(&self) -> &[Column] {
        match &self.0 {
            Levels::Fixed(levels) => levels,
            Levels::Auto => &[],
        }
    }

    pub fn is_flat(&self) -> bool {
        matches!(&self.0, Levels::Fixed(levels) if levels.is_empty())
    }

    pub fn outer(&self) -> Option<Column> {
        self.levels().first().copied()
    }

    /// `project`, `project,goal,ball` (or `project›goal›ball`), `auto`, `off`,
    /// or empty. Every fixed level must be groupable and distinct, at most
    /// `MAX_DEPTH` deep.
    pub fn parse(text: &str, registry: &ColumnRegistry) -> Option<Grouping> {
        let text = text.trim();
        if text.is_empty() || text == "off" || text == "none" {
            return Some(Grouping::flat());
        }
        if text.eq_ignore_ascii_case("auto") {
            return Some(Grouping::auto());
        }
        let mut levels: Vec<Column> = Vec::new();
        for name in text.split([',', '›']) {
            let column = registry
                .parse(name.trim())
                .filter(|column| column.groupable(registry))?;
            if levels.contains(&column) {
                return None;
            }
            levels.push(column);
        }
        (levels.len() <= Grouping::MAX_DEPTH).then_some(Grouping(Levels::Fixed(levels)))
    }

    /// How it reads on screen: `project›goal`, `auto`, or `off`. `auto` never
    /// resolves here — see [`Grouping::label`] for the decorated form the
    /// title bar and status line show once something has resolved it.
    pub fn name(&self, registry: &ColumnRegistry) -> String {
        match &self.0 {
            Levels::Auto => "auto".to_string(),
            Levels::Fixed(levels) if levels.is_empty() => "off".to_string(),
            Levels::Fixed(_) => names_joined(self.levels(), registry, "›"),
        }
    }

    /// How this grouping reads once resolved: unchanged for a fixed grouping;
    /// for `auto`, `auto` alone until `resolved` is non-empty, then
    /// `auto (project›counterparty)`. `resolved` is always this grouping's
    /// actual current levels: the caller's [`auto_levels`] answer when this
    /// is `auto`, its own levels otherwise — the title bar and the `o`
    /// status line both read this one function rather than deriving their
    /// own answer.
    pub fn label(&self, registry: &ColumnRegistry, resolved: &[Column]) -> String {
        if !self.is_auto() {
            return self.name(registry);
        }
        if resolved.is_empty() {
            "auto".to_string()
        } else {
            format!("auto ({})", names_joined(resolved, registry, "›"))
        }
    }

    /// The saved-view spelling: `project,goal`, a declared field prefixed, or
    /// the literal word `auto` — never the levels it resolves to.
    /// `:outline`/`:group` read this back and the bare name alike.
    pub fn text(&self, registry: &ColumnRegistry) -> String {
        match &self.0 {
            Levels::Auto => "auto".to_string(),
            Levels::Fixed(levels) => levels
                .iter()
                .map(|column| column.save_name(registry))
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

/// `project›goal`: how a list of levels reads on screen, joined by `separator`.
fn names_joined(levels: &[Column], registry: &ColumnRegistry, separator: &str) -> String {
    levels
        .iter()
        .map(|column| column.name(registry))
        .collect::<Vec<_>>()
        .join(separator)
}

/// `:group auto`: the groupable columns in `candidates` ranked by
/// distinct-value count over `ordered` (the current filtered set), fewest
/// first, ties broken by `candidates`' own order (the registry's column
/// order). A column carries no sectioning signal and is dropped when every
/// task shares one value, or when every task carries a different one (an
/// id-shaped column). Unset counts as its own value only when some tasks in
/// `ordered` carry a value and others do not — a column nobody has set at
/// all contributes no bucket, so it is dropped by the "one value" rule
/// instead of miscounted as one. Capped at `max_depth`; parent›child nesting
/// stays innermost regardless, via `with_subissues_under_parents` downstream.
pub fn auto_levels(
    tasks: &[BacklogTask],
    ordered: &[usize],
    candidates: &[Column],
    headings: &Headings<'_>,
    max_depth: usize,
) -> Vec<Column> {
    let total = ordered.len();
    let mut ranked: Vec<(usize, usize, Column)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(rank, &column)| {
            let mut values: Vec<String> = Vec::new();
            let mut unset = 0usize;
            for &index in ordered {
                let key = section_key(&tasks[index], column, headings);
                if key.is_empty() {
                    unset += 1;
                } else if !values.contains(&key) {
                    values.push(key);
                }
            }
            let distinct = values.len() + usize::from(unset > 0 && unset < total);
            (distinct > 1 && distinct < total).then_some((distinct, rank, column))
        })
        .collect();
    ranked.sort_by_key(|&(distinct, rank, _)| (distinct, rank));
    ranked
        .into_iter()
        .take(max_depth)
        .map(|(_, _, column)| column)
        .collect()
}

/// `ordered` is the sorted task order; the result keeps it inside each section.
/// `pinned` (the top list's ids, in order, or empty) becomes the first section,
/// like a project that outranks every project; its members leave their own.
/// `levels` is the grouping's *resolved* levels: a fixed grouping's own, or
/// the caller's [`auto_levels`] answer for `auto` — this function never
/// resolves `auto` itself.
pub fn rows(
    tasks: &[BacklogTask],
    ordered: &[usize],
    levels: &[Column],
    headings: &Headings<'_>,
    pinned: &[String],
) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut top: Vec<usize> = pinned
        .iter()
        .filter_map(|id| {
            ordered
                .iter()
                .copied()
                .find(|&index| tasks[index].id == *id)
        })
        .collect();
    if !top.is_empty() {
        rows.push(Row::Heading {
            text: format!("top · {}", pinned.len()),
            depth: 0,
        });
        rows.extend(top.iter().copied().map(Row::Task));
    }
    let ordered: Vec<usize> = ordered
        .iter()
        .copied()
        .filter(|index| !top.contains(index))
        .collect();
    top.clear();
    rows.extend(sections(tasks, &ordered, levels, headings, 0));
    rows
}

/// One level of sections over `ordered`, recursing into `levels[1..]` inside
/// each; the innermost level places sub-issues under their parents.
fn sections(
    tasks: &[BacklogTask],
    ordered: &[usize],
    levels: &[Column],
    headings: &Headings<'_>,
    depth: usize,
) -> Vec<Row> {
    let Some((&column, inner)) = levels.split_first() else {
        return with_subissues_under_parents(tasks, ordered)
            .into_iter()
            .map(Row::Task)
            .collect();
    };
    let mut rows = Vec::new();
    for key in section_keys(tasks, ordered, column, headings) {
        let members: Vec<usize> = ordered
            .iter()
            .copied()
            .filter(|&index| section_key(&tasks[index], column, headings) == key)
            .collect();
        if members.is_empty() {
            continue;
        }
        rows.push(Row::Heading {
            text: heading(column, &key, headings),
            depth,
        });
        rows.extend(sections(tasks, &members, inner, headings, depth + 1));
    }
    rows
}

/// The initiative(s) the grouped projects belong to, for the title bar.
pub fn initiatives(projects: &[ProjectSummary]) -> Vec<String> {
    let mut names: Vec<String> = projects
        .iter()
        .filter_map(|project| project.initiative.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// A task sits in one section: its first value (a task feeding several goals
/// files under the first in `goals.yml` order).
fn section_key(task: &BacklogTask, column: Column, headings: &Headings<'_>) -> String {
    column
        .values(headings.registry, task, headings.goals, headings.blocked)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Section order: projects by stack rank; goals in `goals.yml` order;
/// vocabulary columns by their rank; otherwise by name. Tasks without a value come last.
fn section_keys(
    tasks: &[BacklogTask],
    ordered: &[usize],
    column: Column,
    headings: &Headings<'_>,
) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for &index in ordered {
        let key = section_key(&tasks[index], column, headings);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    let rank = |key: &String| -> (usize, usize, String) {
        if key.is_empty() {
            return (2, 0, String::new());
        }
        match column {
            Column::Project => (
                0,
                headings
                    .projects
                    .iter()
                    .position(|project| project.name == *key)
                    .unwrap_or(usize::MAX),
                key.to_lowercase(),
            ),
            Column::Goal => (
                0,
                headings
                    .goals
                    .iter()
                    .position(|goal| goal.name == *key)
                    .unwrap_or(usize::MAX),
                key.to_lowercase(),
            ),
            // Every other column, a declared field included, sections in its
            // own vocabulary order — a declared enum field's `values` list.
            other => (
                0,
                other.vocabulary_rank(headings.registry, key),
                key.to_lowercase(),
            ),
        }
    };
    keys.sort_by_key(rank);
    keys
}

fn heading(column: Column, key: &str, headings: &Headings<'_>) -> String {
    if key.is_empty() {
        return format!("no {}", column.name(headings.registry));
    }
    match column {
        Column::Goal => match headings
            .goal_summaries
            .iter()
            .find(|goal| goal.name == key)
            .and_then(|goal| goal.progress.as_ref())
        {
            Some(progress) => format!(
                "{key} · {}/{} {} · {}",
                progress.actual, progress.target, progress.unit, progress.pace
            ),
            None => key.to_string(),
        },
        Column::Project => match headings.projects.iter().find(|project| project.name == key) {
            Some(project) => format!(
                "{} · {} · {}/{}",
                project.name,
                project.status.as_deref().unwrap_or("no def"),
                project.done,
                project.total
            ),
            None => key.to_string(),
        },
        _ => key.to_string(),
    }
}

/// Each sub-issue whose parent is in the section sits right after that parent,
/// in the sub-issues' own sorted order; orphans keep their place.
fn with_subissues_under_parents(tasks: &[BacklogTask], members: &[usize]) -> Vec<usize> {
    let parent_in_section = |index: usize| -> Option<usize> {
        let parent = tasks[index].parent.as_deref()?;
        members
            .iter()
            .copied()
            .find(|&candidate| tasks[candidate].id == parent)
    };
    let mut placed = Vec::with_capacity(members.len());
    for &index in members {
        if parent_in_section(index).is_some() {
            continue;
        }
        placed.push(index);
        placed.extend(
            members
                .iter()
                .copied()
                .filter(|&child| parent_in_section(child) == Some(index)),
        );
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use switchbard_core::BacklogTaskSource;

    fn task(id: &str, status: &str, priority: &str, project: Option<&str>) -> BacklogTask {
        BacklogTask {
            storage_identity: None,
            id: id.to_string(),
            title: id.to_string(),
            status: status.to_string(),
            priority: priority.to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: vec![],
            references: vec![],
            project: project.map(str::to_string),
            parent: None,
            created_date: None,
            updated_date: None,
            due_date: None,
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: vec![],
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: std::path::PathBuf::from(format!("/repo/backlog/tasks/{id}.md")),
            custom: std::collections::BTreeMap::new(),
        }
    }

    fn headings<'a>(registry: &'a ColumnRegistry, blocked: &'a HashSet<String>) -> Headings<'a> {
        Headings {
            registry,
            projects: &[],
            goals: &[],
            goal_summaries: &[],
            blocked,
        }
    }

    fn levels(tasks: &[BacklogTask], candidates: &[Column], max_depth: usize) -> Vec<Column> {
        let registry = ColumnRegistry::builtin_only();
        let blocked = HashSet::new();
        let all: Vec<usize> = (0..tasks.len()).collect();
        auto_levels(
            tasks,
            &all,
            candidates,
            &headings(&registry, &blocked),
            max_depth,
        )
    }

    #[test]
    fn a_column_every_task_shares_carries_no_signal_and_is_dropped() {
        let tasks = [
            task("TASK-1", "To Do", "medium", None),
            task("TASK-2", "To Do", "medium", None),
            task("TASK-3", "To Do", "medium", None),
        ];
        assert_eq!(levels(&tasks, &[Column::Status], 4), Vec::<Column>::new());
    }

    #[test]
    fn a_column_where_every_task_differs_is_id_shaped_and_is_dropped() {
        let tasks = [
            task("TASK-1", "To Do", "medium", Some("Ally")),
            task("TASK-2", "To Do", "medium", Some("Chase")),
            task("TASK-3", "To Do", "medium", Some("Wells")),
        ];
        assert_eq!(levels(&tasks, &[Column::Project], 4), Vec::<Column>::new());
    }

    #[test]
    fn unset_counts_as_a_value_only_when_some_tasks_are_set() {
        // Two distinct values plus the unset bucket: three sections.
        let mixed = [
            task("TASK-1", "To Do", "medium", Some("Ally")),
            task("TASK-2", "To Do", "medium", Some("Chase")),
            task("TASK-3", "To Do", "medium", None),
            task("TASK-4", "To Do", "medium", None),
        ];
        assert_eq!(levels(&mixed, &[Column::Project], 4), [Column::Project]);

        // Nobody has set it: no bucket at all, so the "one value" rule drops it.
        let all_unset = [
            task("TASK-1", "To Do", "medium", None),
            task("TASK-2", "To Do", "medium", None),
        ];
        assert_eq!(
            levels(&all_unset, &[Column::Project], 4),
            Vec::<Column>::new()
        );
    }

    #[test]
    fn survivors_order_fewest_distinct_values_first() {
        let tasks = [
            task("TASK-1", "To Do", "high", Some("Ally")),
            task("TASK-2", "To Do", "medium", Some("Ally")),
            task("TASK-3", "Done", "low", Some("Chase")),
            task("TASK-4", "Done", "low", Some("Wells")),
        ];
        // status: 2 distinct, project: 3 distinct, priority: 3 distinct.
        assert_eq!(
            levels(
                &tasks,
                &[Column::Priority, Column::Status, Column::Project],
                4
            ),
            [Column::Status, Column::Priority, Column::Project]
        );
    }

    #[test]
    fn a_tie_in_distinct_count_keeps_the_candidates_own_order() {
        let tasks = [
            task("TASK-1", "To Do", "high", None),
            task("TASK-2", "To Do", "low", None),
            task("TASK-3", "Done", "high", None),
            task("TASK-4", "Done", "low", None),
        ];
        // Both status and priority carry 2 distinct values; input order wins.
        assert_eq!(
            levels(&tasks, &[Column::Priority, Column::Status], 4),
            [Column::Priority, Column::Status]
        );
        assert_eq!(
            levels(&tasks, &[Column::Status, Column::Priority], 4),
            [Column::Status, Column::Priority]
        );
    }

    #[test]
    fn the_result_never_grows_past_max_depth() {
        let tasks = [
            task("TASK-1", "To Do", "high", Some("Ally")),
            task("TASK-2", "To Do", "medium", Some("Chase")),
            task("TASK-3", "Done", "high", Some("Ally")),
            task("TASK-4", "Done", "medium", Some("Wells")),
        ];
        let candidates = [Column::Status, Column::Priority, Column::Project];
        let all = levels(&tasks, &candidates, 4);
        assert_eq!(all, [Column::Status, Column::Priority, Column::Project]);
        let capped = levels(&tasks, &candidates, 2);
        assert_eq!(capped, &all[..2]);
    }
}
