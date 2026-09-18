//! Project execution custody into the canonical task without completing human acceptance.
use super::{Run, RunState};
use anyhow::{Context, Result};

pub(super) fn find_task<'a>(
    run: &Run,
    backlog: &'a crate::BacklogRepo,
) -> Result<&'a crate::BacklogTask> {
    backlog
        .tasks
        .iter()
        .find(|task| match &run.task_storage {
            Some(expected) => task.storage_identity.as_ref().is_some_and(|actual| {
                actual.record_id == expected.record_id
                    && actual.repository_id == expected.repository_id
            }),
            None => task.id == run.task_id,
        })
        .context("reported task identity no longer exists")
}

pub(super) fn project(run: &Run) -> Result<()> {
    let _lock = crate::storage::RepositoryLock::acquire(&run.repo_root)?;
    let backlog = crate::load_backlog_repo(&run.repo_root)?;
    let task = find_task(run, &backlog)?;
    let agent_turn = matches!(run.state, RunState::Running | RunState::Publishing);
    let desired = if agent_turn {
        Some("In Progress")
    } else if matches!(run.state, RunState::AwaitingReview | RunState::PrOpen) {
        Some("In Review")
    } else {
        None
    };
    let status = desired
        .and_then(|name| {
            backlog
                .configured_statuses
                .iter()
                .find(|status| status.eq_ignore_ascii_case(name))
        })
        .cloned();
    crate::edit_backlog_task_command(
        &run.repo_root,
        &task.id,
        &crate::TaskEditRequest {
            patch: crate::BacklogTaskPatch {
                status,
                ..Default::default()
            },
            ball: Some(Some(if agent_turn {
                crate::Ball::Agent
            } else {
                crate::Ball::Me
            })),
            ..Default::default()
        },
    )?;
    Ok(())
}
