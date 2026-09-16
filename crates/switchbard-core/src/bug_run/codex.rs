//! Codex CLI protocol: durable explicit thread identity and a typed final handoff.
use super::{process, AgentOutcome, Lease, MessageRole, Run, Store, MAX_TEXT};
use anyhow::{ensure, Context, Result};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

pub(super) fn execute(
    store: &mut Store,
    lease: &Lease,
    run: &Run,
    artifacts: &Path,
) -> Result<AgentOutcome> {
    let schema = artifacts.join(format!("outcome-schema-{}.json", lease.token));
    write_private(&schema, outcome_schema().to_string().as_bytes())?;
    let output = artifacts.join(format!("outcome-{}.json", lease.token));
    let prompt = prompt(run)?;
    let input = artifacts.join(format!("input-{}.txt", lease.token));
    write_private(&input, prompt.as_bytes())?;
    write_private(&output, b"")?;
    let command = command(run, &schema, &output, &input)?;
    let permit = artifacts.join(format!("start-{}.permit", lease.token));
    store.prepare_child(
        lease,
        schema.to_str().context("non-UTF8 run marker")?,
        &permit,
    )?;
    let mut recorded = None;
    process::capture_guarded(command, Duration::MAX, &permit, Some(&input), |event| {
        match event {
            process::ProcessEvent::Spawned(pgid) => {
                store.record_child(lease, pgid)?;
            }
            process::ProcessEvent::Reaped => {
                store.record_child_reaped(lease)?;
            }
            process::ProcessEvent::Output(events) if recorded.is_none() => {
                if let Some(thread) = started_thread(events)? {
                    store.record_thread(lease, &thread)?;
                    recorded = Some(thread);
                }
            }
            process::ProcessEvent::Output(_) => {}
        }
        Ok(recorded.is_none())
    })?;
    ensure!(
        recorded.is_some(),
        "Codex did not emit thread.started; no continuation identity was established"
    );
    ensure!(
        !std::fs::symlink_metadata(&output)?.file_type().is_symlink(),
        "Codex outcome path became a symlink"
    );
    let text = process::read_bounded(&output)?;
    ensure!(
        text.len() <= MAX_TEXT * 3,
        "Codex final response exceeds supported limit"
    );
    serde_json::from_str(&text)
        .context("Codex final response does not match the Inbox handoff schema")
}

fn command(run: &Run, schema: &Path, output: &Path, input: &Path) -> Result<Command> {
    let mut command = Command::new(&run.options.codex_binary);
    command.current_dir(run.worktree.as_ref().context("run has no worktree")?);
    command.args([
        "exec",
        "-c",
        "approval_policy=\"never\"",
        "-c",
        "sandbox_mode=\"workspace-write\"",
    ]);
    if let Some(thread) = &run.thread_id {
        command.args(["resume", thread]);
    }
    command
        .arg("--json")
        .arg("--output-schema")
        .arg(schema)
        .arg("--output-last-message")
        .arg(output)
        .arg("-");
    command.stdin(Stdio::from(std::fs::File::open(input)?));
    Ok(command)
}

fn prompt(run: &Run) -> Result<String> {
    let instructions = "You are the bug-fixing agent paired with the human reporter through Switchbard Inbox. Reproduce the bug before fixing it, follow the repository instructions, and work only in this isolated worktree. Do not push, create or merge PRs, edit task records, mark human acceptance criteria, or claim human approval. The supervisor owns commits and publication. Never fabricate an owner answer. If clarification or a blocked prerequisite is needed, return kind awaiting_answer with the exact question. Otherwise return kind awaiting_review with a concise change summary, actual commands/results in tests, and remaining risks. The final response must match the supplied JSON schema. Publication occurs only after the owner explicitly requests it.\n";
    if run.thread_id.is_some() {
        let reply = run.messages.iter().rev().find(|m| m.role == MessageRole::OwnerReply)
            .map_or("The previous attempt failed. Continue the same task and inspect the existing work before taking further action.", |m| m.text.as_str());
        return Ok(format!(
            "{instructions}\nRun {} task {}\nOwner reply (verbatim):\n{reply}",
            run.id, run.task_id
        ));
    }
    ensure!(
        run.report_snapshot.len() <= MAX_TEXT,
        "bug task evidence exceeds prompt limit; reduce evidence explicitly before starting"
    );
    Ok(format!(
        "{instructions}\nRun {} task {}\nTitle: {}\nReporter evidence:\n{}",
        run.id, run.task_id, run.title, run.report_snapshot
    ))
}

fn write_private(path: &Path, content: &[u8]) -> Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?
        .write_all(content)?;
    Ok(())
}

pub(super) fn started_thread(events: &str) -> Result<Option<String>> {
    let mut found = None;
    for line in events
        .split_inclusive('\n')
        .take(32_768)
        .filter(|line| line.ends_with('\n'))
    {
        let event: serde_json::Value =
            serde_json::from_str(line).context("invalid Codex JSON event")?;
        if event.get("type").and_then(|v| v.as_str()) == Some("thread.started") {
            let id = event
                .get("thread_id")
                .and_then(|v| v.as_str())
                .context("Codex thread.started omitted thread_id")?;
            ensure!(
                found.as_deref().is_none_or(|previous| previous == id),
                "Codex emitted conflicting threads"
            );
            found = Some(id.to_owned());
        }
    }
    Ok(found)
}

fn outcome_schema() -> serde_json::Value {
    serde_json::json!({
        "type":"object", "additionalProperties":false,
        "required":["kind","question","summary","tests","risks"],
        "properties": {
            "kind":{"type":"string","enum":["awaiting_answer","awaiting_review"]},
            "question":{"type":"string"}, "summary":{"type":"string"},
            "tests":{"type":"array","items":{"type":"string"}},
            "risks":{"type":"array","items":{"type":"string"}}
        }
    })
}
