//! Owner-requested publication pins reviewed content and reconciles remote writes.
use super::{Lease, Run, Store};
use anyhow::{ensure, Context, Result};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);

pub(super) fn publish(store: &mut Store, lease: &Lease) -> Result<()> {
    let run = store.get(&lease.run.id)?;
    if let Err(error) = validate_and_gate(store, lease, &run) {
        store.fail(
            lease,
            &format!("Publication stopped before push: {error}"),
            error
                .downcast_ref::<super::process::UnreapedProcess>()
                .is_some(),
        )?;
        return Err(error);
    }
    let mut wrote_remote = false;
    let result = publish_remote(store, lease, &run, &mut wrote_remote);
    match result {
        Ok(url) => {
            record_task_pr(&run, &url).with_context(|| {
                format!("PR exists at {url}, but task link needs reconciliation")
            })?;
            store.complete_publish(lease, &url)?;
            Ok(())
        }
        Err(error) => {
            store.fail(
                lease,
                &format!("Publication outcome needs reconciliation: {error}"),
                wrote_remote,
            )?;
            Err(error)
        }
    }
}

fn validate_and_gate(store: &mut Store, lease: &Lease, run: &Run) -> Result<()> {
    validate_review(run)?;
    validate_target(run)?;
    let worktree = run.worktree.as_ref().context("No execution worktree")?;
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(&run.options.gate_command)
        .current_dir(worktree);
    super::process::capture_leased(
        command,
        Duration::from_secs(3600),
        store,
        lease,
        "publication gate",
    )?;
    validate_review(run)?;
    let count: u64 = git(worktree, &["rev-list", "--count", "origin/main..HEAD"])?
        .trim()
        .parse()?;
    ensure!(
        count > 0,
        "reviewed branch has no changes against origin/main"
    );
    Ok(())
}

pub(super) fn validate_review(run: &Run) -> Result<()> {
    let worktree = run.worktree.as_ref().context("No execution worktree")?;
    ensure!(
        super::repository_key(worktree)? == run.repo_key,
        "execution repository changed"
    );
    let branch = run.branch.as_deref().context("No execution branch")?;
    ensure!(
        git(worktree, &["symbolic-ref", "--short", "HEAD"])?.trim() == branch,
        "execution branch changed"
    );
    let head = run.review_head.as_deref().context("No reviewed commit")?;
    ensure!(
        git(worktree, &["rev-parse", "HEAD"])?.trim() == head,
        "commit changed after review; send feedback for a fresh review"
    );
    ensure!(
        git(worktree, &["status", "--porcelain"])?.trim().is_empty(),
        "uncommitted work appeared after review; request a fresh review"
    );
    Ok(())
}

/// Freeze the destination before showing the owner a publication confirmation.
pub(super) fn capture_review_target(store: &mut Store, lease: &Lease) -> Result<()> {
    let run = store.get(&lease.run.id)?;
    let worktree = run.worktree.as_ref().context("No execution worktree")?;
    let remote = git(worktree, &["remote", "get-url", "--push", "origin"])?;
    let remote = remote.trim();
    let repository = github_origin(remote)?;
    store.record_review_target(lease, remote, &repository)?;
    Ok(())
}

fn validate_target(run: &Run) -> Result<(&str, &str)> {
    let worktree = run.worktree.as_ref().context("No execution worktree")?;
    let remote = run
        .review_remote
        .as_deref()
        .context("Review has no pinned remote; request a fresh review")?;
    let repository = run
        .review_repository
        .as_deref()
        .context("Review has no pinned repository")?;
    validate_remote_target(worktree, remote, repository)?;
    Ok((remote, repository))
}

fn validate_remote_target(worktree: &Path, remote: &str, repository: &str) -> Result<()> {
    ensure!(
        github_origin(remote)? == repository,
        "pinned repository and remote disagree"
    );
    ensure!(
        git(worktree, &["remote", "get-url", "--push", "origin"])?.trim() == remote,
        "origin changed after review; request a fresh review before publishing"
    );
    Ok(())
}

fn publish_remote(
    store: &mut Store,
    lease: &Lease,
    run: &Run,
    wrote_remote: &mut bool,
) -> Result<String> {
    let worktree = run.worktree.as_ref().context("No execution worktree")?;
    let branch = run.branch.as_deref().context("No execution branch")?;
    let (remote, repo) = validate_target(run)?;
    if let Some(url) = existing_pr(
        worktree,
        repo,
        branch,
        run.review_head.as_deref().context("No reviewed head")?,
    )? {
        return Ok(url);
    }
    let head = run.review_head.as_deref().context("No reviewed head")?;
    *wrote_remote = true;
    let mut push = crate::git_cmd();
    push.arg("-C")
        .arg(worktree)
        .args(["push", remote, &format!("{head}:refs/heads/{branch}")])
        .env("GIT_TERMINAL_PROMPT", "0");
    super::process::capture_leased(push, NETWORK_TIMEOUT, store, lease, "push reviewed branch")?;
    if let Some(url) = existing_pr(
        worktree,
        repo,
        branch,
        run.review_head.as_deref().context("No reviewed head")?,
    )? {
        return Ok(url);
    }
    let body = format!(
        "{}\n\nTask: {}\n\nValidation: `{}` passed on reviewed commit `{}`.\n\n{}",
        run.summary,
        run.task_id,
        run.options.gate_command,
        run.review_head.as_deref().unwrap_or_default(),
        run.evidence
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let parent = std::env::temp_dir().join(format!("switchbard-pr-{}.md", uuid::Uuid::new_v4()));
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&parent)?
        .write_all(body.as_bytes())?;
    let command = gh_command(
        worktree,
        &[
            "pr",
            "create",
            "--repo",
            repo,
            "--base",
            "main",
            "--head",
            branch,
            "--title",
            &run.title,
            "--body-file",
            parent.to_str().context("PR body path is not UTF-8")?,
        ],
    );
    let result = super::process::capture_leased(
        command,
        NETWORK_TIMEOUT,
        store,
        lease,
        "create reviewed PR",
    );
    std::fs::remove_file(parent)?;
    result?;
    existing_pr(
        worktree,
        repo,
        branch,
        run.review_head.as_deref().context("No reviewed head")?,
    )?
    .context("PR creation returned but readback is missing")
}

fn existing_pr(worktree: &Path, repo: &str, branch: &str, head: &str) -> Result<Option<String>> {
    let output = gh(
        worktree,
        &[
            "pr",
            "list",
            "--repo",
            repo,
            "--head",
            branch,
            "--state",
            "all",
            "--limit",
            "2",
            "--json",
            "url,state,headRefName,headRefOid,baseRefName,headRepository,headRepositoryOwner",
        ],
    )?;
    #[derive(serde::Deserialize)]
    struct Pr {
        url: String,
        state: String,
        #[serde(rename = "headRefName")]
        head: String,
        #[serde(rename = "headRefOid")]
        oid: String,
        #[serde(rename = "baseRefName")]
        base: String,
        #[serde(rename = "headRepository")]
        repository: HeadRepository,
        #[serde(rename = "headRepositoryOwner")]
        owner: HeadOwner,
    }
    #[derive(serde::Deserialize)]
    struct HeadRepository {
        name: String,
    }
    #[derive(serde::Deserialize)]
    struct HeadOwner {
        login: String,
    }
    let rows: Vec<Pr> = serde_json::from_str(&output)?;
    ensure!(
        rows.len() <= 1,
        "multiple PRs match the execution branch; reconcile in GitHub"
    );
    let Some(pr) = rows.into_iter().next() else {
        return Ok(None);
    };
    ensure!(
        pr.head == branch && pr.state == "OPEN",
        "existing branch PR is closed or merged; inspect it before continuing"
    );
    ensure!(
        pr.oid == head && pr.base == "main",
        "existing PR no longer matches reviewed head/base"
    );
    ensure!(
        format!("{}/{}", pr.owner.login, pr.repository.name).eq_ignore_ascii_case(repo),
        "PR head belongs to another repository"
    );
    Ok(Some(pr.url))
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let mut command = crate::git_cmd();
    command
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    super::process::capture(command, NETWORK_TIMEOUT)
}

fn gh(root: &Path, args: &[&str]) -> Result<String> {
    super::process::capture(gh_command(root, args), NETWORK_TIMEOUT)
}

fn gh_command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("gh");
    command
        .current_dir(root)
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env_remove("GH_REPO");
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_NAMESPACE",
    ] {
        command.env_remove(name);
    }
    command
}

pub(super) fn record_task_pr(run: &Run, url: &str) -> Result<()> {
    let _lock = crate::storage::RepositoryLock::acquire(&run.repo_root)?;
    let backlog = crate::load_backlog_repo(&run.repo_root)?;
    let task = super::task_flow::find_task(run, &backlog)?;
    if let Some(expected) = &run.task_storage {
        ensure!(
            task.storage_identity
                .as_ref()
                .is_some_and(|actual| actual.record_id == expected.record_id
                    && actual.repository_id == expected.repository_id),
            "task identity changed; PR is preserved for reconciliation"
        );
    }
    let mut references = task.references.clone();
    if !references.iter().any(|existing| existing == url) {
        references.push(url.to_owned());
    }
    let note = format!("Bug PR: {url}");
    let status = backlog
        .configured_statuses
        .iter()
        .find(|status| status.eq_ignore_ascii_case("In Review"))
        .cloned();
    crate::edit_backlog_task_command(
        &run.repo_root,
        &task.id,
        &crate::TaskEditRequest {
            patch: crate::BacklogTaskPatch {
                references: Some(references),
                status,
                ..Default::default()
            },
            append_notes: (!task
                .implementation_notes
                .lines()
                .any(|line| line.trim() == note))
            .then_some(note),
            ball: Some(Some(crate::Ball::Me)),
            ..Default::default()
        },
    )?;
    Ok(())
}

fn github_origin(url: &str) -> Result<String> {
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("git@github.com:"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        .context("origin is not a supported github.com remote")?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let parts: Vec<_> = path.split('/').collect();
    ensure!(
        parts.len() == 2
            && parts.iter().all(|p| !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))),
        "invalid origin repository"
    );
    Ok(path.to_owned())
}

pub fn review_diff(run: &Run) -> Result<String> {
    validate_review(run)?;
    let root = run.worktree.as_ref().context("No execution worktree")?;
    let head = run.review_head.as_deref().context("No reviewed commit")?;
    git(
        root,
        &[
            "--no-pager",
            "diff",
            "--no-ext-diff",
            "--no-color",
            &format!("origin/main...{head}"),
            "--",
        ],
    )
}

/// Observe a stopped publication, then adopt its PR or safely requeue owner-requested publication.
pub fn reconcile_publication(database: &Path, id: &str, revision: u64) -> Result<Run> {
    let mut store = Store::open(database)?;
    let run = store.prepare_publication_reconciliation(id, revision)?;
    let worktree = run
        .worktree
        .as_ref()
        .context("No retained execution worktree")?;
    validate_review(&run)?;
    let (remote, repo) = validate_target(&run)?;
    let branch = run.branch.as_deref().context("No retained branch")?;
    let head = run.review_head.as_deref().context("No reviewed head")?;
    if let Some(url) = existing_pr(worktree, repo, branch, head)? {
        record_task_pr(&run, &url)?;
        return store.complete_reconciled_publication(id, run.revision, &url);
    }
    let reference = format!("refs/heads/{branch}");
    let refs = git(worktree, &["ls-remote", "--refs", remote, &reference])?;
    validate_remote_branch(&refs, head, &reference)?;
    store.requeue_reconciled_publication(id, run.revision)
}

fn validate_remote_branch(output: &str, head: &str, reference: &str) -> Result<()> {
    let rows: Vec<_> = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(2)
        .collect();
    ensure!(rows.len() <= 1, "ambiguous remote branch identity");
    if let Some(row) = rows.first() {
        let fields: Vec<_> = row.split_whitespace().take(3).collect();
        ensure!(
            fields == [head, reference],
            "remote branch changed since owner review; publication remains unresolved"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_publication_destination_requires_fresh_owner_review() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("bug-publish-target-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root)?;
        git(&root, &["init"])?;
        git(
            &root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/fixture/source.git",
            ],
        )?;
        let target = "git@github.com:fixture/destination.git";
        git(&root, &["remote", "set-url", "--push", "origin", target])?;
        validate_remote_target(&root, target, "fixture/destination")?;
        assert!(validate_remote_target(&root, target, "fixture/wrong").is_err());
        git(
            &root,
            &[
                "remote",
                "set-url",
                "--push",
                "origin",
                "https://github.com/fixture/changed.git",
            ],
        )?;
        assert!(validate_remote_target(&root, target, "fixture/destination").is_err());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn recovery_accepts_only_absent_or_exact_reviewed_remote_branch() -> Result<()> {
        let head = "a".repeat(40);
        let reference = "refs/heads/bug/task-143";
        validate_remote_branch("", &head, reference)?;
        validate_remote_branch(&format!("{head}\t{reference}\n"), &head, reference)?;
        assert!(validate_remote_branch(
            &format!("{}\t{reference}\n", "b".repeat(40)),
            &head,
            reference
        )
        .is_err());
        assert!(
            validate_remote_branch(&format!("{head}\trefs/heads/other"), &head, reference).is_err()
        );
        assert!(validate_remote_branch(
            &format!("{head}\t{reference}\n{head}\t{reference}"),
            &head,
            reference
        )
        .is_err());
        Ok(())
    }
}
