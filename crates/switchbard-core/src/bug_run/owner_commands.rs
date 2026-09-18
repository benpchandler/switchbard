//! Explicit owner commands preserve drafts and fence stale/replayed actions.
use super::transitions::{
    action_state, append_message, payload_never_permitted, validate_pr_url,
    validate_publication_reconciliation, validate_text, worker_definitely_dead,
    worker_execution_gone,
};
use super::{MessageRole, Run, RunAction, RunState, Store};
use anyhow::{ensure, Context, Result};

impl Store {
    pub fn fail_launch(&mut self, id: &str, revision: u64, error: &str) -> Result<Run> {
        validate_text(error, false)?;
        self.owner_command(id, revision, |run| {
            ensure!(
                run.state.is_queued() && run.lease.is_none(),
                "run was already claimed; launch failure cannot change it"
            );
            run.state = RunState::Failed;
            run.error = Some(error.into());
            Ok(())
        })
    }

    pub fn save_draft(&mut self, id: &str, revision: u64, text: &str) -> Result<Run> {
        validate_text(text, true)?;
        self.owner_command(id, revision, |run| {
            ensure!(
                matches!(
                    run.state,
                    RunState::AwaitingAnswer | RunState::AwaitingReview
                ),
                "this run is not accepting a reply"
            );
            run.draft = text.into();
            Ok(())
        })
    }

    pub fn submit_reply(&mut self, id: &str, revision: u64) -> Result<Run> {
        self.owner_command(id, revision, |run| {
            ensure!(
                matches!(
                    run.state,
                    RunState::AwaitingAnswer | RunState::AwaitingReview
                ),
                "this run is not awaiting an owner reply"
            );
            ensure!(
                run.thread_id.is_some(),
                "cannot resume without the original Codex thread"
            );
            validate_text(&run.draft, false)?;
            append_message(run, MessageRole::OwnerReply, run.draft.clone())?;
            run.draft.clear();
            run.state = RunState::ResumeQueued;
            run.retry_action = Some(RunAction::ResumeAgent);
            Ok(())
        })
    }

    pub fn request_publish(&mut self, id: &str, revision: u64) -> Result<Run> {
        self.owner_command(id, revision, |run| {
            ensure!(
                run.state == RunState::AwaitingReview,
                "publication requires an owner review request"
            );
            ensure!(
                run.draft.is_empty(),
                "send the saved review reply before requesting publication"
            );
            ensure!(run.review_head.is_some(), "review has no pinned Git head");
            ensure!(
                run.review_remote.is_some() && run.review_repository.is_some(),
                "review has no pinned publication target"
            );
            append_message(
                run,
                MessageRole::OwnerPublish,
                "Owner requested PR publication".into(),
            )?;
            run.state = RunState::PublishQueued;
            run.retry_action = Some(RunAction::Publish);
            Ok(())
        })
    }

    pub fn retry(&mut self, id: &str, revision: u64) -> Result<Run> {
        self.owner_command(id, revision, |run| {
            ensure!(
                run.state == RunState::Failed || (run.state == RunState::Unknown
                    && run.retry_action != Some(RunAction::Publish)
                    && (run.thread_id.is_some() || run.lease.as_ref().is_some_and(payload_never_permitted))
                    && run.lease.as_ref().is_some_and(worker_execution_gone)),
                "unknown outcome requires a retained thread and proven stopped process group; publication requires read-only reconciliation"
            );
            ensure!(
                run.lease.as_ref().is_none_or(worker_definitely_dead),
                "previous worker may still be live; stop it and reconcile before retry"
            );
            let action = match run.retry_action.context("run has no safe retry action")? {
                RunAction::StartAgent if run.thread_id.is_some() => RunAction::ResumeAgent,
                action => action,
            };
            ensure!(
                action != RunAction::ResumeAgent || run.thread_id.is_some(),
                "cannot resume without the original thread"
            );
            run.state = action_state(action);
            run.retry_action = Some(action);
            run.error = None;
            run.lease = None;
            Ok(())
        })
    }

    pub(super) fn prepare_publication_reconciliation(
        &self,
        id: &str,
        revision: u64,
    ) -> Result<Run> {
        let run = self.get(id)?;
        validate_publication_reconciliation(&run, revision)?;
        Ok(run)
    }

    pub(super) fn requeue_reconciled_publication(
        &mut self,
        id: &str,
        revision: u64,
    ) -> Result<Run> {
        self.owner_command(id, revision, |run| {
            validate_publication_reconciliation(run, revision)?;
            run.state = RunState::PublishQueued;
            run.lease = None;
            run.error = None;
            Ok(())
        })
    }

    pub(super) fn complete_reconciled_publication(
        &mut self,
        id: &str,
        revision: u64,
        url: &str,
    ) -> Result<Run> {
        validate_pr_url(url)?;
        self.owner_command(id, revision, |run| {
            validate_publication_reconciliation(run, revision)?;
            run.state = RunState::PrOpen;
            run.pr_url = Some(url.into());
            run.error = None;
            run.lease = None;
            run.retry_action = None;
            Ok(())
        })
    }

    /// A reader can expose an interrupted run, but cannot requeue a possibly-live worker.
    pub fn mark_unknown(&mut self, id: &str, revision: u64, reason: &str) -> Result<Run> {
        validate_text(reason, false)?;
        self.owner_command(id, revision, |run| {
            ensure!(
                matches!(run.state, RunState::Running | RunState::Publishing),
                "only active runs can become unknown"
            );
            ensure!(
                run.lease.as_ref().is_some_and(worker_definitely_dead),
                "worker may still be live"
            );
            run.state = RunState::Unknown;
            run.error = Some(reason.into());
            Ok(())
        })
    }
}
