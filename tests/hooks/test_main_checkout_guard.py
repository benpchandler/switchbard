"""Regression tests for main-checkout-guard.

The guard blocks (a "deny" decision, a hard block) Write/Edit in the primary
worktree (the main checkout) and stays silent in linked worktrees, distinguished
by `git --git-dir` vs `--git-common-dir`. A SWITCHBARD_ALLOW_MAIN_EDIT override is
the user-confirmation gate that allows the edit.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest
from hook_runner import is_registered

HOOK = (
    Path(__file__).resolve().parents[2] / ".claude" / "hooks" / "main-checkout-guard.py"
)


def _git(args: list[str], cwd: Path) -> None:
    subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True)


def _run_payload(
    tool_name: str,
    tool_input: dict,
    extra_env: dict[str, str] | None = None,
    cwd: Path | None = None,
) -> dict | None:
    """Invoke the guard with a raw payload; return the deny JSON or None (silent)."""
    env = {**os.environ}
    env.pop("SWITCHBARD_ALLOW_MAIN_EDIT", None)
    if extra_env:
        env.update(extra_env)
    payload: dict = {"tool_name": tool_name, "tool_input": tool_input}
    if cwd is not None:
        payload["cwd"] = str(cwd)
    result = subprocess.run(
        [sys.executable, str(HOOK)],
        input=json.dumps(payload),
        env=env,
        capture_output=True,
        text=True,
        check=True,
    )
    out = result.stdout.strip()
    return json.loads(out) if out else None


def _run_guard(
    file_path: str, project: Path, extra_env: dict[str, str] | None = None
) -> dict | None:
    """Invoke the guard with a Write payload from a session rooted at ``project``."""
    env = {"CLAUDE_PROJECT_DIR": str(project), **(extra_env or {})}
    return _run_payload("Write", {"file_path": file_path}, env)


def _run_bash(
    command: str, cwd: Path, extra_env: dict[str, str] | None = None
) -> dict | None:
    """Invoke the guard with a Bash payload, as auto mode routes most writes."""
    env = {"CLAUDE_PROJECT_DIR": str(cwd), **(extra_env or {})}
    return _run_payload("Bash", {"command": command}, env, cwd=cwd)


def _make_primary_repo(root: Path) -> Path:
    repo = root / "repo"
    repo.mkdir()
    _git(["init", "--initial-branch=main"], repo)
    _git(["config", "user.email", "test@test.com"], repo)
    _git(["config", "user.name", "Test"], repo)
    (repo / "seed.txt").write_text("seed\n")
    _git(["add", "seed.txt"], repo)
    _git(["commit", "-m", "init"], repo)
    return repo


def test_edit_in_primary_worktree_is_denied(tmp_path):
    repo = _make_primary_repo(tmp_path)

    decision = _run_guard(str(repo / "src" / "x.py"), repo)

    assert decision is not None, "expected a deny decision in the primary worktree"
    out = decision["hookSpecificOutput"]
    assert out["permissionDecision"] == "deny"
    assert "main checkout" in out["permissionDecisionReason"]
    assert "SWITCHBARD_ALLOW_MAIN_EDIT" in out["permissionDecisionReason"]


def test_edit_in_linked_worktree_is_allowed(tmp_path):
    repo = _make_primary_repo(tmp_path)
    worktree = tmp_path / "wt"
    _git(["worktree", "add", "-b", "feature", str(worktree), "main"], repo)

    decision = _run_guard(str(worktree / "src" / "x.py"), repo)

    assert decision is None, "linked worktree edits must be allowed"


def test_override_env_allows_primary_edit(tmp_path):
    repo = _make_primary_repo(tmp_path)

    decision = _run_guard(
        str(repo / "src" / "x.py"), repo, extra_env={"SWITCHBARD_ALLOW_MAIN_EDIT": "1"}
    )

    assert decision is None, "override env must bypass the guard"


def test_non_git_path_is_allowed(tmp_path):
    repo = _make_primary_repo(tmp_path)

    decision = _run_guard(str(tmp_path / "loose" / "x.py"), repo)

    assert decision is None, "paths outside any git repo must not warn"


def test_edit_in_another_repos_primary_worktree_is_allowed(tmp_path):
    """The guard protects the session repo's main checkout, not every repo's.

    `~/.claude` is a git repo of its own; plan files and memory files live in
    its primary worktree. An unscoped guard denied every write there.
    """
    repo = _make_primary_repo(tmp_path)
    (tmp_path / "other").mkdir()
    other = _make_primary_repo(tmp_path / "other")

    decision = _run_guard(str(other / "plans" / "x.md"), repo)

    assert decision is None, "another repo's main checkout is not ours to guard"


def test_session_in_linked_worktree_still_guards_its_main_checkout(tmp_path):
    repo = _make_primary_repo(tmp_path)
    worktree = tmp_path / "wt"
    _git(["worktree", "add", "-b", "feature", str(worktree), "main"], repo)

    decision = _run_guard(str(repo / "src" / "x.py"), worktree)

    assert decision is not None, "the shared main checkout is guarded from any worktree"
    assert decision["hookSpecificOutput"]["permissionDecision"] == "deny"


def test_session_outside_any_repo_guards_nothing(tmp_path):
    repo = _make_primary_repo(tmp_path)
    loose = tmp_path / "loose"
    loose.mkdir()

    decision = _run_guard(str(repo / "src" / "x.py"), loose)

    assert decision is None, "a session with no repo has no main checkout to protect"


# ── The Bash write path ─────────────────────────────────────────
#
# `permissions.defaultMode: auto` routes most file writes through Bash - a
# `sed -i`, a redirect, a heredoc. A guard registered only for Write/Edit is
# blind to all of them, so the hard block that protects the main checkout was
# never enforced on the path that carries most writes.


@pytest.mark.parametrize("tool_name", ["Write", "Edit", "Bash"])
def test_guard_is_registered_for_every_write_path(tool_name: str) -> None:
    assert is_registered("main-checkout-guard", tool_name), (
        f"main-checkout-guard is not registered for {tool_name}; a write made "
        "through that tool never reaches the guard"
    )


@pytest.mark.parametrize(
    "command",
    [
        "sed -i 's/a/b/' src/x.py",
        "echo broken >> src/x.py",
        "cat > src/x.py <<'PY'\nx = 1\nPY",
        "cp /tmp/other.py src/x.py",
        "tee src/x.py < /tmp/other.py",
    ],
)
def test_bash_write_in_primary_worktree_is_denied(tmp_path, command: str) -> None:
    repo = _make_primary_repo(tmp_path)

    decision = _run_bash(command, repo)

    assert decision is not None, f"expected a deny for a Bash write: {command!r}"
    out = decision["hookSpecificOutput"]
    assert out["permissionDecision"] == "deny"
    assert "main checkout" in out["permissionDecisionReason"]


def test_bash_write_via_absolute_path_is_denied(tmp_path) -> None:
    """The path in the command decides, not the shell's cwd."""
    repo = _make_primary_repo(tmp_path)
    elsewhere = tmp_path / "elsewhere"
    elsewhere.mkdir()

    decision = _run_bash(
        f"sed -i 's/a/b/' {repo}/src/x.py",
        elsewhere,
        {"CLAUDE_PROJECT_DIR": str(repo)},
    )

    assert decision is not None, "an absolute write into the main checkout must deny"
    assert decision["hookSpecificOutput"]["permissionDecision"] == "deny"


def test_bash_write_after_cd_into_primary_worktree_is_denied(tmp_path) -> None:
    repo = _make_primary_repo(tmp_path)
    elsewhere = tmp_path / "elsewhere"
    elsewhere.mkdir()

    decision = _run_bash(
        f"cd {repo} && sed -i 's/a/b/' src/x.py",
        elsewhere,
        {"CLAUDE_PROJECT_DIR": str(repo)},
    )

    assert decision is not None, "a cd into the main checkout must still deny"
    assert decision["hookSpecificOutput"]["permissionDecision"] == "deny"


def test_bash_write_in_linked_worktree_is_allowed(tmp_path) -> None:
    repo = _make_primary_repo(tmp_path)
    worktree = tmp_path / "wt"
    _git(["worktree", "add", "-b", "feature", str(worktree), "main"], repo)

    assert _run_bash("sed -i 's/a/b/' src/x.py", worktree) is None


@pytest.mark.parametrize(
    "command",
    [
        "cat src/x.py",
        "grep -rn 'needle' src/",
        "git -C . status --short",
        "uv run pytest -q 2>&1 | tail -20",
        "python3 -c 'print(1)' > /dev/null",
        "sed -n '1,20p' src/x.py",
        "echo scratch > /tmp/notes.txt",
    ],
)
def test_bash_reads_in_primary_worktree_stay_silent(tmp_path, command: str) -> None:
    """The guard blocks writes. A read in the main checkout is the normal case."""
    repo = _make_primary_repo(tmp_path)

    assert _run_bash(command, repo) is None, f"read must not be gated: {command!r}"


def test_override_env_allows_bash_write_in_primary_worktree(tmp_path) -> None:
    repo = _make_primary_repo(tmp_path)

    decision = _run_bash(
        "sed -i 's/a/b/' src/x.py", repo, {"SWITCHBARD_ALLOW_MAIN_EDIT": "1"}
    )

    assert decision is None, "override env must bypass the guard on Bash too"


def test_bash_write_resolves_against_the_shell_cwd_not_the_project_dir(
    tmp_path,
) -> None:
    """A relative path follows the shell, which can be a subdirectory or another repo."""
    repo = _make_primary_repo(tmp_path)
    worktree = tmp_path / "wt"
    _git(["worktree", "add", "-b", "feature", str(worktree), "main"], repo)

    from_worktree = _run_payload(
        "Bash",
        {"command": "sed -i 's/a/b/' seed.txt"},
        {"CLAUDE_PROJECT_DIR": str(repo)},
        cwd=worktree,
    )
    from_main = _run_payload(
        "Bash",
        {"command": "sed -i 's/a/b/' seed.txt"},
        {"CLAUDE_PROJECT_DIR": str(worktree)},
        cwd=repo,
    )

    assert from_worktree is None, "the shell was in the worktree; the write is fine"
    assert from_main is not None, "the shell was in the main checkout; deny"
    assert from_main["hookSpecificOutput"]["permissionDecision"] == "deny"
