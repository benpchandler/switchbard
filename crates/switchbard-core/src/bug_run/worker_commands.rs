//! Lease-fenced worker transitions never manufacture owner intent.
use super::transitions::{
    apply_outcome, queued_action, validate_identifier, validate_pr_url, validate_text,
};
use super::{repository_key, AgentOutcome, Lease, Run, RunAction, RunState, Store, Worker};
use anyhow::{ensure, Context, Result};
use std::path::Path;

impl Store {
    pub fn claim(&mut self, id: &str) -> Result<Lease> {
        let token = uuid::Uuid::new_v4().to_string();
        let run = self.mutate(id, |run| {
            let action =
                queued_action(run.state).context("run is not queued or was already claimed")?;
            ensure!(run.lease.is_none(), "run already has a worker lease");
            run.state = if action == RunAction::Publish {
                RunState::Publishing
            } else {
                RunState::Running
            };
            if action != RunAction::Publish {
                run.review_head = None;
                run.review_remote = None;
                run.review_repository = None;
            }
            run.retry_action = Some(action);
            run.lease = Some(Worker {
                pid: std::process::id(),
                boot_time: crate::boot_time::boot_epoch_unix(),
                token: token.clone(),
                process_group: None,
                command_marker: None,
                child_reaped: false,
                start_permit: None,
            });
            Ok(())
        })?;
        let action = run.retry_action.context("claimed run has no action")?;
        Ok(Lease { run, token, action })
    }

    pub fn record_thread(&mut self, lease: &Lease, thread_id: &str) -> Result<Run> {
        validate_identifier(thread_id)?;
        self.worker_command(lease, |run| {
            ensure!(
                run.state == RunState::Running,
                "thread requires a running agent"
            );
            ensure!(
                run.thread_id.as_deref().is_none_or(|id| id == thread_id),
                "resume returned a different Codex thread"
            );
            run.thread_id = Some(thread_id.into());
            Ok(())
        })
    }

    pub(super) fn prepare_child(
        &mut self,
        lease: &Lease,
        marker: &str,
        permit: &Path,
    ) -> Result<Run> {
        validate_text(marker, false)?;
        ensure!(
            permit.is_absolute() && !permit.try_exists()?,
            "startup permit must be an unused absolute path"
        );
        self.worker_command(lease, |run| {
            let worker = run.lease.as_mut().context("worker lease missing")?;
            worker.command_marker = Some(marker.into());
            worker.child_reaped = false;
            worker.process_group = None;
            worker.start_permit = Some(permit.into());
            Ok(())
        })
    }

    pub(super) fn record_child(&mut self, lease: &Lease, pgid: u32) -> Result<Run> {
        ensure!(pgid > 0 && pgid <= i32::MAX as u32, "invalid process group");
        self.worker_command(lease, |run| {
            let worker = run.lease.as_mut().context("worker lease missing")?;
            ensure!(worker.command_marker.is_some(), "child was not prepared");
            worker.process_group = Some(pgid);
            Ok(())
        })
    }

    pub(super) fn record_child_reaped(&mut self, lease: &Lease) -> Result<Run> {
        self.worker_command(lease, |run| {
            run.lease
                .as_mut()
                .context("worker lease missing")?
                .child_reaped = true;
            Ok(())
        })
    }

    pub fn record_workspace(
        &mut self,
        lease: &Lease,
        worktree: &Path,
        branch: &str,
    ) -> Result<Run> {
        ensure!(
            !branch.is_empty()
                && branch.len() <= 256
                && !branch.starts_with('-')
                && !branch.chars().any(char::is_whitespace),
            "invalid run branch"
        );
        ensure!(
            repository_key(worktree)? == lease.run.repo_key,
            "execution worktree belongs to another repository"
        );
        let worktree = worktree.canonicalize()?;
        self.worker_command(lease, |run| {
            ensure!(
                run.worktree.as_ref().is_none_or(|p| p == &worktree),
                "run already owns another worktree"
            );
            ensure!(
                run.branch.as_deref().is_none_or(|b| b == branch),
                "run already owns another branch"
            );
            run.worktree = Some(worktree);
            run.branch = Some(branch.into());
            Ok(())
        })
    }

    pub fn complete_agent(&mut self, lease: &Lease, outcome: AgentOutcome) -> Result<Run> {
        self.worker_command(lease, |run| {
            ensure!(
                run.state == RunState::Running,
                "agent outcome requires a running agent"
            );
            ensure!(
                run.thread_id.is_some(),
                "agent outcome has no resumable Codex thread"
            );
            apply_outcome(run, outcome)?;
            run.lease = None;
            run.error = None;
            Ok(())
        })
    }

    pub fn record_review_head(&mut self, lease: &Lease, head: &str) -> Result<Run> {
        ensure!(
            matches!(head.len(), 40 | 64) && head.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid review Git head"
        );
        self.worker_command(lease, |run| {
            ensure!(
                run.state == RunState::Running,
                "review head requires a running agent"
            );
            run.review_head = Some(head.to_ascii_lowercase());
            Ok(())
        })
    }

    pub fn record_review_target(
        &mut self,
        lease: &Lease,
        remote: &str,
        repository: &str,
    ) -> Result<Run> {
        validate_text(remote, false)?;
        ensure!(
            repository.split('/').count() == 2
                && repository.split('/').all(|part| !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))),
            "invalid reviewed repository"
        );
        self.worker_command(lease, |run| {
            ensure!(
                run.state == RunState::Running,
                "review target requires a running agent"
            );
            run.review_remote = Some(remote.into());
            run.review_repository = Some(repository.into());
            Ok(())
        })
    }

    pub fn complete_publish(&mut self, lease: &Lease, url: &str) -> Result<Run> {
        validate_pr_url(url)?;
        self.worker_command(lease, |run| {
            ensure!(
                run.state == RunState::Publishing,
                "PR result requires a publishing run"
            );
            run.pr_url = Some(url.into());
            run.state = RunState::PrOpen;
            run.error = None;
            run.lease = None;
            run.retry_action = None;
            Ok(())
        })
    }

    pub fn fail(&mut self, lease: &Lease, error: &str, unknown: bool) -> Result<Run> {
        validate_text(error, false)?;
        self.worker_command(lease, |run| {
            run.error = Some(error.into());
            run.state = if unknown {
                RunState::Unknown
            } else {
                RunState::Failed
            };
            if run.thread_id.is_some() && run.retry_action == Some(RunAction::StartAgent) {
                run.retry_action = Some(RunAction::ResumeAgent);
            }
            if !unknown {
                run.lease = None;
            }
            Ok(())
        })
    }
}
