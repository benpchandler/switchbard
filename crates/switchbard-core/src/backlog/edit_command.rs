//! One edit request, one draft, one durable commit. Frontends only translate flags.
use super::{
    central_commands, mutations, write, BacklogTaskPatch, Ball, ChecklistTextEdit, TaskChecklist,
    TaskSection,
};
use anyhow::{ensure, Result};
use std::path::Path;

#[derive(Debug, Default)]
pub struct TaskEditRequest {
    pub patch: BacklogTaskPatch,
    pub acceptance_edits: Vec<ChecklistTextEdit>,
    pub acceptance_removals: Vec<usize>,
    pub ball: Option<Option<Ball>>,
    pub labels: Vec<(String, bool)>,
    pub checklists: Vec<(TaskChecklist, usize, bool)>,
    pub append_notes: Option<String>,
    pub final_summary: Option<String>,
    /// None leaves parent untouched; Some(None) promotes to top level.
    pub parent: Option<Option<String>>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskEditResult {
    pub changed: bool,
    pub moved: Option<String>,
}

impl TaskEditRequest {
    fn has_content_edits(&self) -> bool {
        !self.patch.is_empty()
            || !self.acceptance_edits.is_empty()
            || !self.acceptance_removals.is_empty()
            || self.ball.is_some()
            || !self.labels.is_empty()
            || !self.checklists.is_empty()
            || self.append_notes.is_some()
            || self.final_summary.is_some()
    }
}

pub fn edit_backlog_task_command(
    root: &Path,
    id: &str,
    request: &TaskEditRequest,
) -> Result<TaskEditResult> {
    ensure!(request.acceptance_removals.is_empty() || !request.checklists.iter().any(|(list, _, _)| matches!(list, TaskChecklist::AcceptanceCriteria)), "acceptance removal cannot be combined with acceptance toggles; run them as separate commands");
    if let Some(status) = &request.patch.status {
        mutations::validate_status(root, status)?;
    }
    if let Some(parent) = &request.parent {
        if let Some((moved, changed)) =
            central_commands::move_task_with_edit(root, id, parent.as_deref(), |text| {
                write::edit_text(text, |draft| apply_request(draft, request))
            })?
        {
            return Ok(TaskEditResult { moved, changed });
        }
        ensure!(!request.has_content_edits(), "combining --parent with other edits requires central task, goals, and ranking storage; migrate those kinds or run the parent move as a separate command");
        return Ok(TaskEditResult {
            changed: false,
            moved: mutations::move_backlog_task(root, id, parent.as_deref())?,
        });
    }
    if !request.has_content_edits() {
        return Ok(TaskEditResult {
            changed: false,
            moved: None,
        });
    }
    let path = mutations::resolve_task_file(root, id)?;
    let changed = write::edit_document(&path, |draft| apply_request(draft, request))?;
    Ok(TaskEditResult {
        changed,
        moved: None,
    })
}

fn apply_request(draft: &mut write::TaskDraft, request: &TaskEditRequest) -> Result<bool> {
    let mut changed = write::revise_task_checklist_draft(
        draft,
        TaskChecklist::AcceptanceCriteria,
        &request.acceptance_edits,
        &request.acceptance_removals,
    )?
    .changed();
    changed |= mutations::apply_patch_draft(draft, &request.patch)?;
    if let Some(holder) = &request.ball {
        let label = holder.as_ref().map(Ball::label);
        changed |= write::reconcile_task_ball_draft(draft, label.as_deref())?.changed();
    }
    for (label, enabled) in &request.labels {
        changed |= write::set_task_label_draft(draft, label, *enabled)?.changed();
    }
    for (list, index, checked) in &request.checklists {
        changed |= write::set_task_checklist_item_draft(draft, *list, *index, *checked)?.changed();
    }
    if let Some(note) = &request.append_notes {
        changed |= write::append_task_notes_draft(draft, note)?.changed();
    }
    if let Some(summary) = &request.final_summary {
        changed |=
            write::replace_task_section_draft(draft, TaskSection::FinalSummary, summary)?.changed();
    }
    Ok(changed)
}
