//! Grouping: a projection over the already-filtered, already-sorted task order
//! into rows, where a heading is an ordinary row the cursor skips.

use switchbard_core::{BacklogTask, GoalDef};

use crate::columns::Column;
use crate::tasks::{GoalSummary, ProjectSummary};

/// Everything a heading can say beyond the section's key: project facts by
/// stack rank, goal facts in `goals.yml` order, and the goal defs membership
/// derives from.
pub struct Headings<'a> {
    pub projects: &'a [ProjectSummary],
    pub goals: &'a [GoalDef],
    pub goal_summaries: &'a [GoalSummary],
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

/// What the list is organized by: nothing, one column, or one column nested
/// inside another (`project›goal`). The one spelling every surface uses: the
/// `o` picker, `:group`, the title bar, and saved views.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Grouping(Vec<Column>);

impl Grouping {
    pub const MAX_DEPTH: usize = 2;

    pub fn flat() -> Grouping {
        Grouping(Vec::new())
    }

    pub fn by(column: Column) -> Grouping {
        Grouping(vec![column])
    }

    pub fn nested(outer: Column, inner: Column) -> Grouping {
        Grouping(vec![outer, inner])
    }

    pub fn levels(&self) -> &[Column] {
        &self.0
    }

    pub fn is_flat(&self) -> bool {
        self.0.is_empty()
    }

    pub fn outer(&self) -> Option<Column> {
        self.0.first().copied()
    }

    /// `project`, `project,goal` (or `project›goal`), `off`, or empty. Every
    /// level must be groupable and distinct, at most `MAX_DEPTH` deep.
    pub fn parse(text: &str) -> Option<Grouping> {
        let text = text.trim();
        if text.is_empty() || text == "off" || text == "none" {
            return Some(Grouping::flat());
        }
        let mut levels: Vec<Column> = Vec::new();
        for name in text.split([',', '›']) {
            let column = Column::parse(name.trim()).filter(|column| column.groupable())?;
            if levels.contains(&column) {
                return None;
            }
            levels.push(column);
        }
        (levels.len() <= Grouping::MAX_DEPTH).then_some(Grouping(levels))
    }

    /// How it reads on screen: `project›goal`, or `off`.
    pub fn name(&self) -> String {
        if self.is_flat() {
            return "off".to_string();
        }
        self.0
            .iter()
            .map(|column| column.name())
            .collect::<Vec<_>>()
            .join("›")
    }

    /// The saved-view and `:group` spelling: `project,goal`.
    pub fn text(&self) -> String {
        self.0
            .iter()
            .map(|column| column.name())
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// `ordered` is the sorted task order; the result keeps it inside each section.
/// `pinned` (the top list's ids, in order, or empty) becomes the first section,
/// like a project that outranks every project; its members leave their own.
pub fn rows(
    tasks: &[BacklogTask],
    ordered: &[usize],
    grouping: &Grouping,
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
    rows.extend(sections(tasks, &ordered, grouping.levels(), headings, 0));
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
            .filter(|&index| section_key(&tasks[index], column, headings.goals) == key)
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
fn section_key(task: &BacklogTask, column: Column, goals: &[GoalDef]) -> String {
    column
        .values(task, goals)
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
        let key = section_key(&tasks[index], column, headings.goals);
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
            other => (0, other.vocabulary_rank(key), key.to_lowercase()),
        }
    };
    keys.sort_by_key(rank);
    keys
}

fn heading(column: Column, key: &str, headings: &Headings<'_>) -> String {
    if key.is_empty() {
        return format!("no {}", column.name());
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
