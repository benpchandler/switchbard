#!/usr/bin/env python3
"""PreToolUse hook: block writes in the primary worktree (the main checkout).

The main checkout at the repo root is sync-only - it tracks origin/main and is
where fresh worktrees are cut from. Editing files there leaves uncommitted
sediment (the git-ship era's failure mode) and races other agent sessions that
share that one working tree. All real work belongs in a linked worktree.

This is a hard block (a "deny" decision), not a warning. An earlier version used
"ask" - but a non-interactive context (background/workflow subagents, headless
runs) has no human to answer the prompt, so the edit proceeded anyway. "deny"
prevents the edit in every context unless the override env var is set, which is
the deliberate user confirmation: a main-checkout edit can never happen by
accident or be auto-confirmed by an unattended agent.

The guard covers Bash as well as Write/Edit. Under `permissions.defaultMode:
auto` most writes are a `sed -i`, a redirect, or a heredoc; a Write/Edit-only
matcher left the hard block unenforced on the path carrying most of them.
`hook_utils.extract_write_targets` names the paths a Bash command writes, so a
command that only reads - a `cat`, a `grep`, a test run - is never gated.

Detection is path-independent: in the primary worktree `git --git-dir` equals
`git --git-common-dir` (both resolve to `<repo>/.git`); in a linked worktree
they differ (`<repo>/.git/worktrees/<name>` vs `<repo>/.git`). So this fires in
the main checkout and stays silent in every worktree, with no hardcoded paths.

Scope: only the session's own repo. The write target's common-dir must equal
the common-dir of CLAUDE_PROJECT_DIR (which may itself be a linked worktree of
the same repo). Other repos' primary worktrees are not ours to guard - `~/.claude`
is a git repo, and an unscoped guard denied every plan and memory write there
(2026-09-10).

Override / confirmation: set SWITCHBARD_ALLOW_MAIN_EDIT=1 to allow main-checkout
edits for a deliberate session (e.g. resolving the guard itself, syncing, or an
emergency hand-edit). This env var IS the user-confirmation gate - without it,
the edit is denied.

Exit behavior:
- Exit 0, no output  = allow silently (linked worktree, a read, override set,
  or not a git repo)
- Exit 0, JSON deny  = block the write (main checkout, no override)
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

from hook_utils import (
    emit_and_exit,
    extract_write_targets,
    pretooluse_decision,
    project_dir,
    read_hook_input,
)

OVERRIDE_ENV = "SWITCHBARD_ALLOW_MAIN_EDIT"


def _base_dir_for(target_file: str) -> str:
    """Directory to run git from: the edited file's nearest existing ancestor
    if absolute (so new files in new subdirs still resolve), else the session's
    project dir. Both resolve to the worktree the edit lands in."""
    if target_file:
        p = Path(target_file)
        if p.is_absolute():
            d = p.parent
            while not d.exists() and d != d.parent:
                d = d.parent
            return str(d)
    return project_dir()


def _git_dirs(base_dir: str) -> tuple[str, str] | None:
    """(git-dir, common-dir) for base_dir, or None outside any git repo."""
    try:
        git_dir = subprocess.run(
            ["git", "rev-parse", "--absolute-git-dir"],
            cwd=base_dir,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        common_dir = subprocess.run(
            ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
            cwd=base_dir,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None  # not a git repo / git unavailable → don't warn
    if not git_dir or not common_dir:
        return None
    return git_dir, common_dir


def _is_session_repo_primary_worktree(base_dir: str, session_common_dir: str) -> bool:
    """True when base_dir is the primary worktree of the session's own repo.

    Primary means git-dir == common-dir. Scoping to the session's repo keeps the
    guard off other repos' main checkouts - `~/.claude` (plan and memory files)
    is itself a git repo, and an unscoped guard denied every write there.
    """
    dirs = _git_dirs(base_dir)
    if dirs is None:
        return False
    git_dir, common_dir = dirs
    return git_dir == common_dir and common_dir == session_common_dir


def _lands_in_primary_worktree(tool_input: dict, cwd: str | None) -> bool:
    """True when any path this tool call writes sits in the session repo's main checkout.

    A Write/Edit with no readable path still lands in the session's worktree, so
    it falls back to the project dir. A Bash command with no write target writes
    nothing to fall back to - it is a read, and reads are never gated. A session
    outside any git repo has no main checkout to protect.
    """
    targets = extract_write_targets(tool_input, cwd)
    if not targets:
        if "command" in tool_input:
            return False
        targets = ("",)
    session_dirs = _git_dirs(project_dir())
    if session_dirs is None:
        return False
    session_common_dir = session_dirs[1]
    return any(
        _is_session_repo_primary_worktree(_base_dir_for(target), session_common_dir)
        for target in targets
    )


def main() -> None:
    if os.environ.get(OVERRIDE_ENV):
        emit_and_exit(None)

    data = read_hook_input()
    if not _lands_in_primary_worktree(data.get("tool_input", {}), data.get("cwd")):
        emit_and_exit(None)

    emit_and_exit(
        pretooluse_decision(
            "deny",
            "Blocked - you're editing in the main checkout (the primary worktree), "
            "which is sync-only. Real work belongs in a linked worktree to avoid "
            "uncommitted sediment and races with other sessions:\n"
            "  git -C <repo> fetch origin main\n"
            "  git -C <repo> worktree add ~/Dev/.worktrees/switchbard/<concern> "
            "-b <concern>/<slug> origin/main\n"
            "Then commit/push/PR with standard git from the worktree.\n"
            f"If editing the main checkout is genuinely intended, set {OVERRIDE_ENV}=1 "
            "to confirm and allow it for the session - this edit will not proceed "
            "otherwise.",
        )
    )


if __name__ == "__main__":
    main()
