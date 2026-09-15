//! Read-only task fields; all values come from the loaded task snapshot.
use crate::{app::App, detail_pane::Section};
use switchbard_core::BacklogTask;

pub(super) fn lines(app: &App, task: &BacklogTask, section: Section) -> Vec<String> {
    match section {
        Section::Plan => body(&task.implementation_plan),
        Section::Notes => body(&task.implementation_notes),
        Section::Summary => body(&task.final_summary),
        Section::Acceptance => vec!["Not set".to_string()],
        Section::DefinitionOfDone => {
            let items = task
                .definition_of_done
                .iter()
                .map(|item| format!("[{}] {}", if item.checked { "x" } else { " " }, item.text))
                .collect::<Vec<_>>();
            if items.is_empty() {
                body("")
            } else {
                items
            }
        }
        Section::References => {
            if task.references.is_empty() {
                body("")
            } else {
                task.references.clone()
            }
        }
        Section::Relations => relations(app, task),
        Section::Metadata => metadata(task),
        Section::Properties | Section::Description => Vec::new(),
    }
}

fn body(text: &str) -> Vec<String> {
    if text.trim().is_empty() {
        vec!["Not set".to_string()]
    } else {
        text.lines().map(str::to_string).collect()
    }
}

fn value(text: Option<&str>) -> &str {
    text.filter(|text| !text.trim().is_empty())
        .unwrap_or("Not set")
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        "Not set".to_string()
    } else {
        items.join(", ")
    }
}

fn relations(app: &App, task: &BacklogTask) -> Vec<String> {
    let children = app
        .relations
        .children
        .get(&task.id)
        .into_iter()
        .flatten()
        .map(|child| {
            format!(
                "{} {} ({}; {})",
                child.id,
                child.title,
                child.status,
                child.source.label()
            )
        })
        .collect::<Vec<_>>();
    let mut lines = vec![
        format!("parent: {}", value(task.parent.as_deref())),
        format!("dependencies: {}", list(&task.dependencies)),
        format!("subtasks: {}", list(&children)),
    ];
    if app
        .relations
        .blocked_by
        .get(&task.id)
        .is_none_or(Vec::is_empty)
    {
        lines.push("blocked by: Not set".to_string());
    }
    if app.relations.blocks.get(&task.id).is_none_or(Vec::is_empty) {
        lines.push("blocks: Not set".to_string());
    }
    lines
}

fn metadata(task: &BacklogTask) -> Vec<String> {
    let mut lines = vec![
        format!("task ID: {}", task.id),
        format!("assignees: {}", list(&task.assignees)),
        format!("created: {}", value(task.created_date.as_deref())),
        format!("updated: {}", value(task.updated_date.as_deref())),
        format!("lifecycle: {}", task.source.label()),
        format!("source path: {}", task.path.display()),
    ];
    if let Some(identity) = &task.storage_identity {
        lines.extend([
            format!("repository ID: {}", identity.repository_id),
            format!("record ID: {}", identity.record_id),
            format!("revision: {}", identity.revision),
        ]);
    } else {
        lines.extend(
            [
                "repository ID: Not set",
                "record ID: Not set",
                "revision: Not set",
            ]
            .map(str::to_string),
        );
    }
    if task.custom.is_empty() {
        lines.push("custom fields: Not set".to_string());
    } else {
        lines.extend(
            task.custom
                .iter()
                .map(|(key, value)| format!("{key}: {value}")),
        );
    }
    lines
}
