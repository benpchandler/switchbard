"""End-to-end drain over a real temp repo: real `sb` binary,
real git worktree + push to a bare origin, a stub agent that does what the
prompt asks (edit, commit, check the AC), and a stub `gh` that prints a PR
url. Proves the whole protocol walk without network or a live model.

Requires the built binary: set SWITCHBARD_TASK_BIN, or have
`target/debug/switchbard-task` present (cargo build -p switchbard-task).
"""

from __future__ import annotations

import os
import stat
import subprocess
from pathlib import Path

import pytest

from switchbard_orchestrator.cli import main

REPO_ROOT = Path(__file__).resolve().parents[2]


def task_bin() -> str | None:
    env = os.environ.get("SWITCHBARD_TASK_BIN")
    if env and Path(env).is_file():
        return env
    # The crate's binary is being renamed switchbard-task -> sb; accept both.
    for name in ("sb", "switchbard-task"):
        candidate = REPO_ROOT / "target" / "debug" / name
        if candidate.is_file():
            return str(candidate)
    return None


def sh(cwd: Path, *argv: str) -> str:
    result = subprocess.run(argv, cwd=cwd, capture_output=True, text=True)
    assert result.returncode == 0, f"{argv}: {result.stderr}"
    return result.stdout


def write_script(path: Path, body: str) -> str:
    path.write_text(body)
    path.chmod(path.stat().st_mode | stat.S_IEXEC)
    return str(path)


@pytest.mark.skipif(task_bin() is None, reason="build switchbard-task first")
def test_drain_once_walks_a_real_repo_to_a_released_pr(tmp_path, capsys, monkeypatch):
    binary = task_bin()
    repo = tmp_path / "repo"
    repo.mkdir()
    sh(repo, "git", "init", "-q", "-b", "main")
    sh(repo, "git", "config", "user.email", "test@example.test")
    sh(repo, "git", "config", "user.name", "Test")
    (repo / "backlog" / "tasks").mkdir(parents=True)
    (repo / "backlog" / "config.yml").write_text(
        'statuses: ["Icebox", "To Do", "In Progress", "In Review", "Done"]\n'
    )
    sh(repo, "git", "add", "-A")
    sh(repo, "git", "commit", "-qm", "init")

    origin = tmp_path / "origin.git"
    sh(tmp_path, "git", "init", "-q", "--bare", str(origin))
    sh(repo, "git", "remote", "add", "origin", str(origin))
    sh(repo, "git", "push", "-qu", "origin", "main")

    # A queued task, committed to main so the worktree checkout carries it.
    task_id = sh(repo, binary, "create", "Prove the loop", "--ac", "It works").strip()
    sh(repo, "git", "add", "-A")
    sh(repo, "git", "commit", "-qm", "task")
    sh(repo, binary, "queue", "send", task_id)

    # Stub agent: does what a real run would - writes a file, commits, and
    # checks its AC in the WORKTREE's copy of the task.
    fake_claude = write_script(
        tmp_path / "fake-claude",
        "#!/bin/sh\n"
        "cat > /dev/null\n"  # consume the prompt like the real -p does
        'echo "did the work" > proof.txt\n'
        "git add -A && git commit -qm agent-work\n"
        f'"{binary}" --repo . edit {task_id} --check-ac 1\n'
        "git add -A && git commit -qm agent-ac\n",
    )
    fake_gh = write_script(
        tmp_path / "fake-gh",
        "#!/bin/sh\necho https://example.test/pull/42\n",
    )

    monkeypatch.setenv("SWITCHBARD_ORCHESTRATOR_HOME", str(tmp_path / "orch-home"))
    monkeypatch.setenv("TMPDIR", str(tmp_path / "tmp"))
    (tmp_path / "tmp").mkdir()

    code = main(
        [
            "drain", "--once",
            "--repo", str(repo),
            "--task-bin", binary,
            "--claude-bin", fake_claude,
            "--gh-bin", fake_gh,
        ]
    )
    captured = capsys.readouterr()
    assert code == 0
    assert captured.out.strip() == f"{task_id}\tdispatched\thttps://example.test/pull/42"

    # The claim released honestly through the real ladder.
    view = sh(repo, binary, "view", task_id)
    assert "Status: In Review" in view
    assert "Dispatch PR: https://example.test/pull/42" in view
    assert "dispatched" in view

    # The branch actually landed on origin with the agent's commits.
    branch_head = sh(tmp_path, "git", "-C", str(origin), "rev-parse", "dispatch/" + task_id.lower())
    assert branch_head.strip()

    # Live-progress events were emitted alongside the run log.
    events = list((tmp_path / "tmp" / "switchbard-logs").glob("*.events.jsonl"))
    assert events, "events sidecar exists"
    text = events[0].read_text()
    for marker in ("run_start", '"node": "claim"', '"node": "reconcile"', '"outcome": "dispatched"'):
        assert marker in text, text


@pytest.mark.skipif(task_bin() is None, reason="build switchbard-task first")
def test_unproven_run_releases_failed_with_the_remainder(tmp_path, capsys, monkeypatch):
    binary = task_bin()
    repo = tmp_path / "repo"
    repo.mkdir()
    sh(repo, "git", "init", "-q", "-b", "main")
    sh(repo, "git", "config", "user.email", "t@example.test")
    sh(repo, "git", "config", "user.name", "T")
    (repo / "backlog" / "tasks").mkdir(parents=True)
    (repo / "backlog" / "config.yml").write_text('statuses: ["To Do", "In Progress", "Done"]\n')
    sh(repo, "git", "add", "-A")
    sh(repo, "git", "commit", "-qm", "init")
    task_id = sh(repo, binary, "create", "Half done", "--ac", "Never checked").strip()
    sh(repo, "git", "add", "-A")
    sh(repo, "git", "commit", "-qm", "task")
    sh(repo, binary, "queue", "send", task_id)

    # Agent commits work but never checks its AC - task-green, outcome-unproven.
    fake_claude = write_script(
        tmp_path / "fake-claude",
        "#!/bin/sh\ncat > /dev/null\necho x > proof.txt\ngit add -A && git commit -qm w\n",
    )
    fake_gh = write_script(tmp_path / "fake-gh", "#!/bin/sh\necho should-not-run >&2\nexit 1\n")

    monkeypatch.setenv("SWITCHBARD_ORCHESTRATOR_HOME", str(tmp_path / "orch-home"))
    monkeypatch.setenv("TMPDIR", str(tmp_path / "tmp"))
    (tmp_path / "tmp").mkdir()

    code = main(
        [
            "drain", "--once",
            "--repo", str(repo),
            "--task-bin", binary,
            "--claude-bin", fake_claude,
            "--gh-bin", fake_gh,
        ]
    )
    captured = capsys.readouterr()
    assert code == 0
    out = captured.out.strip()
    assert out.startswith(f"{task_id}\tfailed\tunproven outcome:")
    assert "AC #1 unchecked: Never checked" in out

    view = sh(repo, binary, "view", task_id)
    assert "Status: To Do" in view, "prior status restored on the honest failure"
    assert "dispatch-failed" in view
    assert "AC #1 unchecked" in view, "the remainder landed in the task's notes"
