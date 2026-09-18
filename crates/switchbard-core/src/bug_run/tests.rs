use super::*;
use anyhow::Result;
use std::path::Path;

fn git(root: &Path, args: &[&str]) {
    let output = crate::git_cmd()
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("fixture git starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture(root: &Path) -> Result<String> {
    std::fs::create_dir_all(root.join("backlog/tasks"))?;
    git(root, &["init", "-b", "main"]);
    let task = crate::NewBacklogTask {
        title: "Unicode bug 漢字 🐛".into(), description: "Evidence from screen\n\n## Screen\n\nEXACT SCREEN 漢字\n\n## Action trail\n\nEXACT ACTION TRAIL".into(),
        status: "To Do".into(), priority: "medium".into(), acceptance_criteria: vec!["Owner confirms".into()],
        parent: None, labels: vec!["bug".into()], assignees: vec![], project: None,
        dependencies: vec![], due_date: None, custom: vec![],
    };
    let (id, _) = crate::create_task_allocating_id(root, &task)?;
    git(root, &["add", "backlog"]);
    git(root, &["commit", "-m", "fixture"]);
    Ok(id)
}

#[test]
fn real_supervisor_process_contract_preserves_evidence_and_resumes_same_thread() -> Result<()> {
    scenario(|repo, path, id| {
        let executable = runtime_fixture(repo)?;
        let mut store = Store::open(path)?;
        let run = store.enqueue(
            repo,
            id,
            RunOptions {
                codex_binary: executable.to_string_lossy().into(),
                gate_command: "test -f fixed.txt".into(),
            },
        )?;
        run_supervisor(path, &run.id)?;
        let question = store.get(&run.id)?;
        assert_eq!(question.state, RunState::AwaitingAnswer);
        assert_eq!(question.thread_id.as_deref(), Some("fixture-thread"));
        let prompt = std::fs::read_to_string(executable.with_extension("prompt"))?;
        assert!(prompt.contains("EXACT SCREEN 漢字"));
        assert!(prompt.contains("EXACT ACTION TRAIL"));
        let draft = store.save_draft(
            &run.id,
            question.revision,
            "Owner says fix the narrow layout 🐛",
        )?;
        store.submit_reply(&run.id, draft.revision)?;
        run_supervisor(path, &run.id)?;
        let review = store.get(&run.id)?;
        assert_eq!(review.state, RunState::AwaitingReview);
        assert_eq!(review.thread_id, question.thread_id);
        assert!(review.review_head.is_some());
        assert!(review
            .evidence
            .iter()
            .any(|e| e.contains("Supervisor verified")));
        assert_eq!(review.messages.len(), 3);
        assert!(
            std::fs::read_to_string(executable.with_extension("prompt"))?
                .contains("Owner says fix the narrow layout 🐛")
        );
        run_supervisor(path, &run.id)?;
        assert_eq!(store.get(&run.id)?, review);
        Ok(())
    })
}

fn runtime_fixture(repo: &Path) -> Result<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let parent = repo.parent().expect("fixture parent");
    let origin = parent.join("origin.git");
    git(
        repo,
        &[
            "clone",
            "--bare",
            repo.to_str().expect("UTF8 path"),
            origin.to_str().expect("UTF8 path"),
        ],
    );
    git(
        repo,
        &[
            "remote",
            "add",
            "origin",
            origin.to_str().expect("UTF8 path"),
        ],
    );
    git(repo, &["config", "user.name", "Fixture"]);
    git(
        repo,
        &[
            "config",
            "remote.origin.pushurl",
            "https://github.com/fixture/repo.git",
        ],
    );
    git(repo, &["config", "user.email", "fixture@example.invalid"]);
    git(repo, &["config", "commit.gpgsign", "false"]);
    let executable = parent.join("fake-codex.py");
    std::fs::write(
        &executable,
        r#"#!/usr/bin/env python3
import json, pathlib, sys
args = sys.argv[1:]
if args == ['--version']:
    print('codex fixture')
    sys.exit(0)
assert 'dangerously-bypass-approvals-and-sandbox' not in ' '.join(args)
assert 'sandbox_mode="workspace-write"' in args
prompt = sys.stdin.read()
pathlib.Path(__file__).with_suffix('.prompt').write_text(prompt)
print(json.dumps({'type':'thread.started','thread_id':'fixture-thread'}), flush=True)
if 'resume' in args:
    assert args[args.index('resume')+1] == 'fixture-thread'
    pathlib.Path('fixed.txt').write_text('fixed narrow layout')
    result = {'kind':'awaiting_review','question':'','summary':'Fixed narrow layout','tests':['fixture test passed'],'risks':[]}
else:
    result = {'kind':'awaiting_answer','question':'Which layout?','summary':'','tests':[],'risks':[]}
output = pathlib.Path(args[args.index('--output-last-message')+1])
output.write_text(json.dumps(result))
"#,
    )?;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))?;
    Ok(executable)
}

#[test]
fn missing_codex_is_known_failure_and_keeps_bug() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = store.enqueue(
            repo,
            id,
            RunOptions {
                codex_binary: "/unavailable/switchbard-codex-fixture".into(),
                gate_command: "true".into(),
            },
        )?;
        assert!(run_supervisor(path, &run.id).is_err());
        let failed = store.get(&run.id)?;
        assert_eq!(failed.state, RunState::Failed);
        assert!(crate::read_backlog_task_snapshot(repo, &run.task_id)?
            .content
            .contains("EXACT SCREEN"));
        assert_eq!(
            store.retry(&run.id, failed.revision)?.state,
            RunState::AgentQueued
        );
        Ok(())
    })
}

#[test]
fn process_timeout_and_output_bound_return_actionable_errors() {
    let mut command = std::process::Command::new("/bin/sh");
    command.args(["-c", "sleep 30"]);
    let result = super::process::capture(command, std::time::Duration::from_millis(50));
    assert!(format!("{:#}", result.expect_err("timeout")).contains("timed out"));
    let mut command = std::process::Command::new("python3");
    command.args(["-c", "print('x' * 5000000)"]);
    assert!(super::process::capture(command, std::time::Duration::from_secs(10)).is_err());
}

#[test]
fn thread_events_preserve_partial_unicode_and_reject_identity_changes() -> Result<()> {
    assert_eq!(
        super::codex::started_thread(
            "{\"type\":\"thread.started\",\"thread_id\":\"abc\"}\n{\"partial\":"
        )?,
        Some("abc".into())
    );
    assert!(super::codex::started_thread("{\"type\":\"thread.started\",\"thread_id\":\"abc\"}\n{\"type\":\"thread.started\",\"thread_id\":\"xyz\"}\n").is_err());
    Ok(())
}

fn scenario(test: impl FnOnce(&Path, &Path, &str) -> Result<()>) -> Result<()> {
    let dir = tempfile::tempdir()?;
    crate::storage::with_test_database(&dir.path().join("tasks.sqlite"), || {
        let repo = dir.path().join("repo");
        let id = fixture(&repo)?;
        test(&repo, &dir.path().join("runs.sqlite"), &id)
    })
}

fn awaiting_answer(store: &mut Store, repo: &Path, id: &str) -> Result<Run> {
    let run = store.enqueue(repo, id, RunOptions::default())?;
    let lease = store.claim(&run.id)?;
    store.record_thread(&lease, "original-thread")?;
    store.complete_agent(
        &lease,
        AgentOutcome::AwaitingAnswer {
            question: "Which screen? 漢字".into(),
        },
    )
}

#[test]
fn enqueue_requires_task_and_coalesces_worktrees_and_bare_ids() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        assert!(store
            .enqueue(repo, "999999", RunOptions::default())
            .is_err());
        let run = store.enqueue(repo, id, RunOptions::default())?;
        let sibling = repo.parent().expect("fixture parent").join("sibling");
        git(
            repo,
            &[
                "worktree",
                "add",
                "-b",
                "sibling",
                sibling.to_str().expect("UTF8 fixture"),
            ],
        );
        assert_eq!(repository_key(repo)?, repository_key(&sibling)?);
        assert_eq!(
            run.id,
            store
                .enqueue(&sibling, &run.task_id, RunOptions::default())?
                .id
        );
        assert_eq!(store.list(&sibling)?.len(), 1);
        Ok(())
    })
}

#[test]
fn same_task_number_in_other_repository_does_not_alias() -> Result<()> {
    scenario(|repo, path, id| {
        let other = repo.parent().expect("fixture parent").join("other");
        let other_id = fixture(&other)?;
        let mut store = Store::open(path)?;
        let run = store.enqueue(repo, id, RunOptions::default())?;
        let second = store.enqueue(&other, &other_id, RunOptions::default())?;
        assert_ne!(run.id, second.id);
        assert_eq!(store.list(repo)?, vec![run]);
        assert_eq!(store.list(&other)?, vec![second]);
        Ok(())
    })
}

#[test]
fn owner_draft_survives_stale_write_restart_and_atomic_submit() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = awaiting_answer(&mut store, repo, id)?;
        let text = "Inbox\n日本語 and 🐛\n  preserve spacing  ";
        let saved = store.save_draft(&run.id, run.revision, text)?;
        assert!(store
            .save_draft(&run.id, run.revision, "stale overwrite")
            .is_err());
        drop(store);
        let mut store = Store::open(path)?;
        assert_eq!(store.get(&run.id)?.draft, text);
        let sent = store.submit_reply(&run.id, saved.revision)?;
        assert_eq!(sent.state, RunState::ResumeQueued);
        assert!(sent.draft.is_empty());
        assert_eq!(sent.messages[1].text, text);
        assert!(store.submit_reply(&run.id, saved.revision).is_err());
        let lease = store.claim(&run.id)?;
        assert_eq!(lease.action, RunAction::ResumeAgent);
        assert!(store.record_thread(&lease, "wrong-thread").is_err());
        assert_eq!(
            store.get(&run.id)?.thread_id.as_deref(),
            Some("original-thread")
        );
        Ok(())
    })
}

#[test]
fn concurrent_readers_cannot_claim_or_complete_same_work_twice() -> Result<()> {
    scenario(|repo, path, id| {
        let mut first = Store::open(path)?;
        let mut second = Store::open(path)?;
        let run = first.enqueue(repo, id, RunOptions::default())?;
        let lease = first.claim(&run.id)?;
        assert!(second.claim(&run.id).is_err());
        first.record_thread(&lease, "thread")?;
        let outcome = AgentOutcome::AwaitingAnswer {
            question: "Question".into(),
        };
        first.complete_agent(&lease, outcome.clone())?;
        assert!(second.complete_agent(&lease, outcome).is_err());
        assert_eq!(second.get(&run.id)?.messages.len(), 1);
        Ok(())
    })
}

#[test]
fn review_reply_is_not_publish_and_publish_requires_explicit_pinned_review() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = store.enqueue(repo, id, RunOptions::default())?;
        let lease = store.claim(&run.id)?;
        store.record_thread(&lease, "thread")?;
        store.record_review_head(&lease, &"a".repeat(40))?;
        store.record_review_target(&lease, "https://github.com/test/repo.git", "test/repo")?;
        let review = store.complete_agent(
            &lease,
            AgentOutcome::AwaitingReview {
                summary: "Fixed".into(),
                tests: vec!["cargo test passed".into()],
                risks: vec![],
            },
        )?;
        let draft = store.save_draft(&run.id, review.revision, "Please change this")?;
        assert!(store.request_publish(&run.id, draft.revision).is_err());
        let cleared = store.save_draft(&run.id, draft.revision, "")?;
        let queued = store.request_publish(&run.id, cleared.revision)?;
        assert_eq!(queued.state, RunState::PublishQueued);
        assert!(store.request_publish(&run.id, cleared.revision).is_err());
        let publish = store.claim(&run.id)?;
        assert_eq!(publish.action, RunAction::Publish);
        assert!(store
            .complete_publish(&publish, "https://example.com/a/b/pull/1")
            .is_err());
        let done = store.complete_publish(&publish, "https://github.com/test/repo/pull/1")?;
        assert_eq!(done.state, RunState::PrOpen);
        assert_eq!(
            done.messages.last().expect("publish message").role,
            MessageRole::OwnerPublish
        );
        Ok(())
    })
}

#[test]
fn failure_retry_preserves_thread_and_unknown_never_blindly_relaunches() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = store.enqueue(repo, id, RunOptions::default())?;
        let first = store.claim(&run.id)?;
        let failed = store.fail(&first, "executable unavailable", false)?;
        let queued = store.retry(&run.id, failed.revision)?;
        assert_eq!(queued.state, RunState::AgentQueued);
        let second = store.claim(&run.id)?;
        store.record_thread(&second, "retained-thread")?;
        let failed = store.fail(&second, "agent exited", false)?;
        assert_eq!(
            store.retry(&run.id, failed.revision)?.state,
            RunState::ResumeQueued
        );
        let third = store.claim(&run.id)?;
        let unknown = store.fail(&third, "outcome not observed", true)?;
        assert!(store.retry(&run.id, unknown.revision).is_err());
        assert!(store
            .complete_agent(
                &second,
                AgentOutcome::AwaitingAnswer {
                    question: "stale".into()
                }
            )
            .is_err());
        Ok(())
    })
}

#[test]
fn oversized_draft_fails_without_changing_saved_intent() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = awaiting_answer(&mut store, repo, id)?;
        let saved = store.save_draft(&run.id, run.revision, "retain me")?;
        assert!(store
            .save_draft(&run.id, saved.revision, &"界".repeat(MAX_TEXT))
            .is_err());
        assert_eq!(store.get(&run.id)?, saved);
        Ok(())
    })
}

#[test]
fn publication_link_is_idempotent_and_preserves_owner_content() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let mut run = store.enqueue(repo, id, RunOptions::default())?;
        crate::edit_backlog_task_command(
            repo,
            id,
            &crate::TaskEditRequest {
                patch: crate::BacklogTaskPatch {
                    references: Some(vec!["https://example.com/evidence".into()]),
                    ..Default::default()
                },
                append_notes: Some("Keep owner notes 漢字".into()),
                ..Default::default()
            },
        )?;
        let url = "https://github.com/fixture/repo/pull/1";
        super::publish::record_task_pr(&run, url)?;
        super::publish::record_task_pr(&run, url)?;
        let backlog = crate::load_backlog_repo(repo)?;
        let task = backlog
            .tasks
            .iter()
            .find(|t| t.id == run.task_id)
            .expect("retained task");
        assert_eq!(task.references, vec!["https://example.com/evidence", url]);
        assert!(task.implementation_notes.contains("Keep owner notes 漢字"));
        assert_eq!(task.implementation_notes.matches(url).count(), 1);
        assert!(!task.acceptance_criteria[0].checked);
        run.task_storage = Some(TaskStorageIdentity {
            repository_id: "wrong-repo".into(),
            record_id: "wrong-record".into(),
            revision: 1,
        });
        assert!(
            super::publish::record_task_pr(&run, "https://github.com/fixture/repo/pull/2").is_err()
        );
        Ok(())
    })
}

#[test]
fn publication_rejects_changed_head_branch_or_dirty_review() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let mut run = store.enqueue(repo, id, RunOptions::default())?;
        run.worktree = Some(repo.into());
        run.branch = Some("main".into());
        let output = crate::git_cmd()
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "HEAD"])
            .output()?;
        run.review_head = Some(String::from_utf8(output.stdout)?.trim().into());
        // Task ID reservation metadata lives outside Git and is ignored by the runtime worktree.
        git(repo, &["clean", "-fd"]);
        super::publish::validate_review(&run)?;
        std::fs::write(repo.join("dirty.txt"), "new work")?;
        assert!(super::publish::validate_review(&run).is_err());
        git(repo, &["add", "dirty.txt"]);
        git(repo, &["commit", "-m", "changed head"]);
        assert!(super::publish::validate_review(&run).is_err());
        run.branch = Some("other-branch".into());
        assert!(super::publish::validate_review(&run).is_err());
        Ok(())
    })
}

#[test]
fn future_schema_is_rejected_without_overwriting_existing_data() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("future.sqlite");
    let connection = rusqlite::Connection::open(&path)?;
    connection.execute_batch("PRAGMA application_id=1396851271; PRAGMA user_version=999; CREATE TABLE future(data TEXT); INSERT INTO future VALUES('preserved');")?;
    assert!(Store::open(&path).is_err());
    assert_eq!(
        connection.query_row("SELECT data FROM future", [], |r| r.get::<_, String>(0))?,
        "preserved"
    );
    assert_eq!(
        connection.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))?,
        999
    );
    Ok(())
}

#[test]
fn report_snapshot_is_frozen_before_later_task_edits() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = store.enqueue(repo, id, RunOptions::default())?;
        crate::edit_backlog_task_command(
            repo,
            id,
            &crate::TaskEditRequest {
                patch: crate::BacklogTaskPatch {
                    description: Some("Changed later".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        let retained = store.get(&run.id)?;
        assert_eq!(retained.report_snapshot, run.report_snapshot);
        assert!(retained.report_snapshot.contains("EXACT SCREEN 漢字"));
        Ok(())
    })
}

#[test]
fn launch_failure_is_durable_but_cannot_override_claim() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = store.enqueue(repo, id, RunOptions::default())?;
        let failed = store.fail_launch(&run.id, run.revision, "spawn failed")?;
        assert_eq!(
            Store::open(path)?.get(&run.id)?.error.as_deref(),
            Some("spawn failed")
        );
        let queued = store.retry(&run.id, failed.revision)?;
        store.claim(&run.id)?;
        assert!(store
            .fail_launch(&run.id, queued.revision, "late spawn failure")
            .is_err());
        assert_eq!(store.get(&run.id)?.state, RunState::Running);
        Ok(())
    })
}

#[test]
fn unknown_agent_cannot_resume_until_recorded_child_group_is_gone() -> Result<()> {
    use std::os::unix::process::CommandExt;
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = store.enqueue(repo, id, RunOptions::default())?;
        let lease = store.claim(&run.id)?;
        store.record_thread(&lease, "same-thread")?;
        store.prepare_child(&lease, "exact-run-marker", &path.with_extension("permit"))?;
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .process_group(0)
            .spawn()?;
        store.record_child(&lease, child.id())?;
        let mut dead = std::process::Command::new("/usr/bin/true").spawn()?;
        let dead_pid = dead.id();
        dead.wait()?;
        let mut crashed = store.get(&run.id)?;
        crashed.lease.as_mut().expect("worker").pid = dead_pid;
        rusqlite::Connection::open(path)?.execute(
            "UPDATE bug_runs SET payload=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&crashed)?, run.id],
        )?;
        store.reconcile_interrupted(repo)?;
        let unknown = store.get(&run.id)?;
        assert_eq!(unknown.state, RunState::Unknown);
        let blocked = store.retry(&run.id, unknown.revision).is_err();
        child.kill()?;
        child.wait()?;
        assert!(
            blocked,
            "surviving Codex group must prevent duplicate resume"
        );
        let resumed = store.retry(&run.id, unknown.revision)?;
        assert_eq!(resumed.state, RunState::ResumeQueued);
        assert_eq!(resumed.thread_id.as_deref(), Some("same-thread"));
        Ok(())
    })
}

#[test]
fn agent_task_authority_edits_are_retained_but_never_committed() -> Result<()> {
    scenario(|repo, path, id| {
        let executable = runtime_fixture(repo)?;
        let source = std::fs::read_to_string(&executable)?.replace("pathlib.Path('fixed.txt').write_text('fixed narrow layout')", "pathlib.Path('fixed.txt').write_text('fixed narrow layout')\n    next(pathlib.Path('backlog/tasks').glob('*.md')).write_text('unauthorized task edit')");
        std::fs::write(&executable, source)?;
        let mut store = Store::open(path)?;
        let run = store.enqueue(
            repo,
            id,
            RunOptions {
                codex_binary: executable.to_string_lossy().into(),
                gate_command: "true".into(),
            },
        )?;
        run_supervisor(path, &run.id)?;
        let question = store.get(&run.id)?;
        let draft = store.save_draft(&run.id, question.revision, "proceed")?;
        store.submit_reply(&run.id, draft.revision)?;
        assert!(run_supervisor(path, &run.id).is_err());
        let failed = store.get(&run.id)?;
        assert_eq!(failed.state, RunState::Failed);
        assert!(failed
            .error
            .as_deref()
            .expect("failure")
            .contains("task authority"));
        assert!(crate::read_backlog_task_snapshot(repo, &run.task_id)?
            .content
            .contains("EXACT SCREEN"));
        assert!(failed.review_head.is_none());
        Ok(())
    })
}

#[test]
fn supervisor_rejects_symlinked_artifact_directory() -> Result<()> {
    scenario(|repo, path, id| {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let executable = runtime_fixture(repo)?;
        let mut store = Store::open(path)?;
        let run = store.enqueue(
            repo,
            id,
            RunOptions {
                codex_binary: executable.to_string_lossy().into(),
                gate_command: "true".into(),
            },
        )?;
        let base = path
            .parent()
            .expect("database parent")
            .join("bug-run-artifacts");
        std::fs::create_dir(&base)?;
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700))?;
        symlink(repo, base.join(&run.id))?;
        assert!(run_supervisor(path, &run.id).is_err());
        assert!(store
            .get(&run.id)?
            .error
            .as_deref()
            .expect("failure")
            .contains("real directory"));
        assert!(!repo.join("worktree").exists());
        Ok(())
    })
}

#[test]
fn simultaneous_connections_have_exactly_one_claim_winner() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = store.enqueue(repo, id, RunOptions::default())?;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let barrier = std::sync::Arc::clone(&barrier);
                let path = path.to_path_buf();
                let id = run.id.clone();
                std::thread::spawn(move || {
                    let mut store = Store::open(path).expect("open peer store");
                    barrier.wait();
                    store.claim(&id).is_ok()
                })
            })
            .collect();
        let wins = handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .filter(|won| *won)
            .count();
        assert_eq!(wins, 1);
        assert_eq!(store.get(&run.id)?.state, RunState::Running);
        Ok(())
    })
}

#[test]
fn absent_start_permit_closes_the_pre_pgid_crash_window() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = store.enqueue(repo, id, RunOptions::default())?;
        let lease = store.claim(&run.id)?;
        store.record_thread(&lease, "retained-thread")?;
        let permit = path.with_extension("start-permit");
        let marker = path.with_extension("payload-ran");
        store.prepare_child(&lease, "exact-command-marker", &permit)?;
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "touch \"$1\"", "fixture"]).arg(&marker);
        let result = super::process::capture_guarded(
            command,
            std::time::Duration::from_secs(5),
            &permit,
            None,
            |event| {
                if matches!(event, super::process::ProcessEvent::Spawned(_)) {
                    anyhow::bail!("simulate supervisor failure before PGID persistence");
                }
                Ok(false)
            },
        );
        assert!(result.is_err());
        assert!(!marker.exists());
        assert!(!permit.exists());
        let mut dead = std::process::Command::new("/usr/bin/true").spawn()?;
        let pid = dead.id();
        dead.wait()?;
        let mut crashed = store.get(&run.id)?;
        let worker = crashed.lease.as_mut().expect("worker");
        worker.pid = pid;
        assert!(worker.process_group.is_none());
        assert!(!worker.child_reaped);
        rusqlite::Connection::open(path)?.execute(
            "UPDATE bug_runs SET payload=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&crashed)?, run.id],
        )?;
        store.reconcile_interrupted(repo)?;
        let unknown = store.get(&run.id)?;
        assert_eq!(unknown.state, RunState::Unknown);
        let resumed = store.retry(&run.id, unknown.revision)?;
        assert_eq!(resumed.state, RunState::ResumeQueued);
        assert_eq!(resumed.thread_id.as_deref(), Some("retained-thread"));
        Ok(())
    })
}

#[test]
fn guarded_writer_cannot_execute_before_durable_group_record() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = store.enqueue(repo, id, RunOptions::default())?;
        let lease = store.claim(&run.id)?;
        let marker = path.with_extension("writer-ran");
        let mut command = std::process::Command::new("/bin/sh");
        command
            .args(["-c", "touch \"$1\"; printf complete", "fixture"])
            .arg(&marker);
        let output = super::process::capture_leased(
            command,
            std::time::Duration::from_secs(5),
            &mut store,
            &lease,
            "fixture-write",
        )?;
        assert_eq!(output, "complete");
        assert!(marker.exists());
        let worker = store.get(&run.id)?.lease.expect("retained lease");
        assert!(worker.process_group.is_some());
        assert!(worker.child_reaped);
        assert!(worker.start_permit.expect("permit receipt").exists());
        Ok(())
    })
}

#[test]
fn first_launch_retry_requires_proof_that_payload_never_started() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = store.enqueue(repo, id, RunOptions::default())?;
        let lease = store.claim(&run.id)?;
        let permit = path.with_extension("first-start-permit");
        store.prepare_child(&lease, "first-agent", &permit)?;
        let mut dead = std::process::Command::new("/usr/bin/true").spawn()?;
        let pid = dead.id();
        dead.wait()?;
        let mut crashed = store.get(&run.id)?;
        crashed.lease.as_mut().expect("worker").pid = pid;
        rusqlite::Connection::open(path)?.execute(
            "UPDATE bug_runs SET payload=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&crashed)?, run.id],
        )?;
        store.reconcile_interrupted(repo)?;
        let unknown = store.get(&run.id)?;
        std::fs::write(&permit, "permit exists but group identity missing")?;
        assert!(store.retry(&run.id, unknown.revision).is_err());
        std::fs::remove_file(&permit)?;
        assert_eq!(
            store.retry(&run.id, unknown.revision)?.state,
            RunState::AgentQueued
        );
        Ok(())
    })
}

#[test]
fn publication_requeue_preserves_explicit_owner_intent_and_pins() -> Result<()> {
    scenario(|repo, path, id| {
        let mut store = Store::open(path)?;
        let run = store.enqueue(repo, id, RunOptions::default())?;
        let lease = store.claim(&run.id)?;
        store.record_thread(&lease, "review-thread")?;
        store.record_review_head(&lease, &"a".repeat(40))?;
        store.record_review_target(
            &lease,
            "https://github.com/fixture/repo.git",
            "fixture/repo",
        )?;
        let review = store.complete_agent(
            &lease,
            AgentOutcome::AwaitingReview {
                summary: "fixed".into(),
                tests: vec!["test passed".into()],
                risks: vec![],
            },
        )?;
        store.request_publish(&run.id, review.revision)?;
        let publish = store.claim(&run.id)?;
        let unknown = store.fail(&publish, "remote outcome unknown", true)?;
        assert!(store
            .requeue_reconciled_publication(&run.id, unknown.revision)
            .is_err());
        let mut dead = std::process::Command::new("/usr/bin/true").spawn()?;
        let pid = dead.id();
        dead.wait()?;
        let mut crashed = unknown.clone();
        crashed.lease.as_mut().expect("worker").pid = pid;
        rusqlite::Connection::open(path)?.execute(
            "UPDATE bug_runs SET payload=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&crashed)?, run.id],
        )?;
        let queued = store.requeue_reconciled_publication(&run.id, unknown.revision)?;
        assert_eq!(queued.state, RunState::PublishQueued);
        assert_eq!(queued.messages, unknown.messages);
        assert_eq!(queued.review_head, unknown.review_head);
        assert_eq!(queued.review_remote, unknown.review_remote);
        assert!(store
            .requeue_reconciled_publication(&run.id, unknown.revision)
            .is_err());
        Ok(())
    })
}
