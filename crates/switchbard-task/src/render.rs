//! Stdout payload rendering for `sb` — the stable shapes the
//! help text promises (one TSV row per task for `list`, a fields-then-
//! sections block for `view`). Presentation only; every fact comes from the
//! parsed [`BacklogTask`].

use switchbard_core::{BacklogChecklistItem, BacklogTask};

/// `id \t status \t priority \t labels(comma) \t project \t title` — stable
/// columns, greppable, no alignment padding (agents parse this, humans pipe
/// it to `column -t`). The project column sits before the title so the one
/// free-text column stays last for naive tab-splitters.
pub fn list_row(task: &BacklogTask) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        task.id,
        task.status,
        task.priority,
        task.labels.join(","),
        task.project.as_deref().unwrap_or(""),
        task.title
    )
}

/// The full task: one `Field: value` line per non-empty field, a blank
/// line, then every non-empty section under its own `## ` heading with
/// checklists in their on-disk `- [x] #N` shape.
pub fn task_view(task: &BacklogTask) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} - {}\n", task.id, task.title));
    push_field(&mut out, "Status", &task.status);
    push_field(&mut out, "Priority", &task.priority);
    push_field(&mut out, "Labels", &task.labels.join(", "));
    push_field(&mut out, "Assignee", &task.assignees.join(", "));
    push_field(&mut out, "Project", task.project.as_deref().unwrap_or(""));
    push_field(&mut out, "Parent", task.parent.as_deref().unwrap_or(""));
    push_field(&mut out, "Dependencies", &task.dependencies.join(", "));
    push_field(&mut out, "References", &task.references.join(", "));
    push_field(
        &mut out,
        "Created",
        task.created_date.as_deref().unwrap_or(""),
    );
    push_field(
        &mut out,
        "Updated",
        task.updated_date.as_deref().unwrap_or(""),
    );
    push_field(&mut out, "Source", task.source.label());
    push_field(&mut out, "File", &task.path.display().to_string());
    push_section(&mut out, "Description", &task.description);
    push_checklist(&mut out, "Acceptance Criteria", &task.acceptance_criteria);
    push_section(&mut out, "Implementation Plan", &task.implementation_plan);
    push_section(&mut out, "Implementation Notes", &task.implementation_notes);
    push_checklist(&mut out, "Definition of Done", &task.definition_of_done);
    push_section(&mut out, "Final Summary", &task.final_summary);
    out
}

fn push_field(out: &mut String, name: &str, value: &str) {
    if !value.is_empty() {
        out.push_str(&format!("{name}: {value}\n"));
    }
}

fn push_section(out: &mut String, heading: &str, body: &str) {
    if body.trim().is_empty() {
        return;
    }
    out.push_str(&format!("\n## {heading}\n\n{}\n", body.trim_end()));
}

fn push_checklist(out: &mut String, heading: &str, items: &[BacklogChecklistItem]) {
    if items.is_empty() {
        return;
    }
    out.push_str(&format!("\n## {heading}\n\n"));
    for item in items {
        let mark = if item.checked { "x" } else { " " };
        out.push_str(&format!("- [{mark}] #{} {}\n", item.index, item.text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use switchbard_core::BacklogTaskSource;

    fn task() -> BacklogTask {
        BacklogTask {
            id: "TASK-7".to_string(),
            title: "Render me".to_string(),
            status: "In Progress".to_string(),
            priority: "high".to_string(),
            assignees: vec!["ben".to_string()],
            labels: vec!["fork".to_string(), "cli".to_string()],
            dependencies: vec![],
            references: vec![],
            project: None,
            parent: None,
            created_date: Some("2026-08-28 10:00".to_string()),
            updated_date: None,
            description: "Why.".to_string(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: vec![BacklogChecklistItem {
                index: 1,
                checked: true,
                text: "Proven".to_string(),
            }],
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: PathBuf::from("/p/backlog/tasks/task-7 - Render me.md"),
        }
    }

    #[test]
    fn list_row_is_six_stable_tab_separated_columns_with_title_last() {
        assert_eq!(
            list_row(&task()),
            "TASK-7\tIn Progress\thigh\tfork,cli\t\tRender me"
        );
        let mut assigned = task();
        assigned.project = Some("Lucella cutover".to_string());
        assert_eq!(
            list_row(&assigned),
            "TASK-7\tIn Progress\thigh\tfork,cli\tLucella cutover\tRender me"
        );
    }

    #[test]
    fn view_renders_fields_then_sections_and_omits_empty_ones() {
        let text = task_view(&task());
        assert!(text.starts_with("TASK-7 - Render me\n"));
        assert!(text.contains("Status: In Progress\n"));
        assert!(text.contains("Labels: fork, cli\n"));
        assert!(!text.contains("Project:"), "empty fields are omitted");
        assert!(text.contains("\n## Description\n\nWhy.\n"));
        assert!(text.contains("- [x] #1 Proven\n"));
        assert!(
            !text.contains("## Implementation Plan"),
            "empty sections are omitted"
        );
    }
}
