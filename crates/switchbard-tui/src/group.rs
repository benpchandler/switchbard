//! Grouping: a projection over the already-filtered, already-sorted task order
//! into rows, where a heading is an ordinary row the cursor skips.

use switchbard_core::BacklogTask;

use crate::columns::Column;
use crate::tasks::ProjectSummary;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Heading(String),
    Task(usize),
}

/// `ordered` is the sorted task order; the result keeps it inside each section.
/// `pinned` (the top list's ids, in order, or empty) becomes the first section,
/// like a project that outranks every project; its members leave their own.
pub fn rows(
    tasks: &[BacklogTask],
    ordered: &[usize],
    group: Option<Column>,
    projects: &[ProjectSummary],
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
        rows.push(Row::Heading(format!("top · {}", pinned.len())));
        rows.extend(top.iter().copied().map(Row::Task));
    }
    let ordered: Vec<usize> = ordered
        .iter()
        .copied()
        .filter(|index| !top.contains(index))
        .collect();
    top.clear();
    let Some(column) = group else {
        rows.extend(ordered.iter().map(|&index| Row::Task(index)));
        return rows;
    };
    let ordered = ordered.as_slice();
    for key in section_keys(tasks, ordered, column, projects) {
        let members: Vec<usize> = ordered
            .iter()
            .copied()
            .filter(|&index| section_key(&tasks[index], column) == key)
            .collect();
        if members.is_empty() {
            continue;
        }
        rows.push(Row::Heading(heading(column, &key, projects)));
        rows.extend(
            with_subissues_under_parents(tasks, &members)
                .into_iter()
                .map(Row::Task),
        );
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

fn section_key(task: &BacklogTask, column: Column) -> String {
    column.values(task).into_iter().next().unwrap_or_default()
}

/// Section order: projects by stack rank; vocabulary columns by their rank;
/// otherwise by name. Tasks without a value come last.
fn section_keys(
    tasks: &[BacklogTask],
    ordered: &[usize],
    column: Column,
    projects: &[ProjectSummary],
) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for &index in ordered {
        let key = section_key(&tasks[index], column);
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
                projects
                    .iter()
                    .position(|project| project.name == *key)
                    .unwrap_or(usize::MAX),
                key.to_lowercase(),
            ),
            other => (0, other.vocabulary_rank(key), key.to_lowercase()),
        }
    };
    keys.sort_by_key(rank);
    keys
}

fn heading(column: Column, key: &str, projects: &[ProjectSummary]) -> String {
    if key.is_empty() {
        return format!("no {}", column.name());
    }
    match column {
        Column::Project => match projects.iter().find(|project| project.name == key) {
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
