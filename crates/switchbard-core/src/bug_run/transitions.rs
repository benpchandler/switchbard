//! Single definitions of run bounds, transitions, and recovery proofs.
use super::store::now;
use super::{
    AgentOutcome, Message, MessageRole, Run, RunAction, RunOptions, RunState, Worker, MAX_EVIDENCE,
    MAX_MESSAGES, MAX_TEXT,
};
use anyhow::{ensure, Context, Result};
use std::time::Duration;

pub(super) fn validate_text(text: &str, empty_allowed: bool) -> Result<()> {
    ensure!(
        text.len() <= MAX_TEXT,
        "text exceeds {MAX_TEXT} bytes; nothing was saved"
    );
    ensure!(
        empty_allowed || !text.trim().is_empty(),
        "text must not be empty"
    );
    Ok(())
}
pub(super) fn validate_identifier(text: &str) -> Result<()> {
    ensure!(
        !text.is_empty() && text.len() <= 256 && !text.chars().any(char::is_whitespace),
        "invalid Codex thread identifier"
    );
    Ok(())
}
pub(super) fn validate_options(options: &RunOptions) -> Result<()> {
    validate_text(&options.codex_binary, false)?;
    validate_text(&options.gate_command, false)
}
pub(super) fn append_message(run: &mut Run, role: MessageRole, text: String) -> Result<()> {
    validate_text(&text, false)?;
    ensure!(
        run.messages.len() < MAX_MESSAGES,
        "conversation limit reached; existing messages and draft were preserved"
    );
    run.messages.push(Message {
        role,
        text,
        created_at_unix: now(),
    });
    Ok(())
}
pub(super) fn apply_outcome(run: &mut Run, outcome: AgentOutcome) -> Result<()> {
    match outcome {
        AgentOutcome::AwaitingAnswer { question } => {
            append_message(run, MessageRole::AgentQuestion, question.clone())?;
            run.prompt = question;
            run.state = RunState::AwaitingAnswer;
        }
        AgentOutcome::AwaitingReview {
            summary,
            tests,
            risks,
        } => {
            ensure!(
                tests.len() <= MAX_EVIDENCE && risks.len() <= MAX_EVIDENCE,
                "too many review evidence items"
            );
            for text in tests.iter().chain(&risks).take(MAX_EVIDENCE * 2) {
                validate_text(text, false)?;
            }
            append_message(run, MessageRole::AgentReview, summary.clone())?;
            run.summary = summary;
            run.evidence = tests;
            run.risks = risks;
            run.state = RunState::AwaitingReview;
        }
    }
    run.retry_action = Some(RunAction::ResumeAgent);
    Ok(())
}
pub(super) fn queued_action(state: RunState) -> Option<RunAction> {
    match state {
        RunState::AgentQueued => Some(RunAction::StartAgent),
        RunState::ResumeQueued => Some(RunAction::ResumeAgent),
        RunState::PublishQueued => Some(RunAction::Publish),
        _ => None,
    }
}
pub(super) fn action_state(action: RunAction) -> RunState {
    match action {
        RunAction::StartAgent => RunState::AgentQueued,
        RunAction::ResumeAgent => RunState::ResumeQueued,
        RunAction::Publish => RunState::PublishQueued,
    }
}
pub(super) fn worker_definitely_dead(worker: &Worker) -> bool {
    let current_boot = crate::boot_time::boot_epoch_unix();
    if current_boot.is_some() && worker.boot_time.is_some() && worker.boot_time != current_boot {
        return true;
    }
    worker.pid > 0 && !crate::work_sessions::pid_alive(worker.pid)
}

pub(super) fn worker_execution_gone(worker: &Worker) -> bool {
    if !worker_definitely_dead(worker) {
        return false;
    }
    let boot = crate::boot_time::boot_epoch_unix();
    if boot.is_some() && worker.boot_time.is_some() && boot != worker.boot_time {
        return true;
    }
    if worker.child_reaped {
        return true;
    }
    let Some(group) = worker.process_group else {
        if worker
            .start_permit
            .as_ref()
            .is_some_and(|path| path.try_exists().is_ok_and(|exists| !exists))
        {
            // The dead supervisor never permitted the wrapper to execute its payload.
            return true;
        }
        return worker.command_marker.is_none();
    };
    let mut command = std::process::Command::new("/bin/ps");
    command.args(["-axo", "pgid="]);
    let Ok(output) = super::process::capture(command, Duration::from_secs(5)) else {
        return false;
    };
    for (index, line) in output.lines().enumerate().take(32_769) {
        if index == 32_768 {
            return false;
        }
        let Ok(actual) = line.trim().parse::<u32>() else {
            return false;
        };
        if actual == group {
            return false;
        }
    }
    true
}

pub(super) fn payload_never_permitted(worker: &Worker) -> bool {
    worker
        .start_permit
        .as_ref()
        .is_some_and(|path| path.try_exists().is_ok_and(|exists| !exists))
}

pub(super) fn validate_publication_reconciliation(run: &Run, revision: u64) -> Result<()> {
    ensure!(
        run.revision == revision,
        "run changed; refresh before reconciliation"
    );
    ensure!(
        run.state == RunState::Unknown && run.retry_action == Some(RunAction::Publish),
        "only unknown publication can be reconciled"
    );
    ensure!(
        run.lease.as_ref().is_some_and(worker_execution_gone),
        "publication worker may still be live"
    );
    Ok(())
}
pub(super) fn validate_pr_url(url: &str) -> Result<()> {
    validate_text(url, false)?;
    let path = url
        .strip_prefix("https://github.com/")
        .context("PR must have an exact HTTPS GitHub URL")?;
    let parts: Vec<_> = path.split('/').take(5).collect();
    ensure!(
        parts.len() == 4
            && !parts[0].is_empty()
            && !parts[1].is_empty()
            && parts[2] == "pull"
            && parts[3].parse::<u64>().is_ok_and(|n| n > 0),
        "invalid pull request URL"
    );
    Ok(())
}
