"""The orchestrator's only hands: subprocess wrappers around the
`sb queue` protocol (the switchbard-task crate's binary), git, `gh`, and the headless agent.

Every task mutation goes through `switchbard-task` - the orchestrator never
edits task files (the one-writer invariant extends to it; TASK-89 AC #4).
Each wrapper raises ProtoError with the tool's own stderr line so failures
surface verbatim instead of paraphrased.
"""

from __future__ import annotations

import os
import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path


class ProtoError(RuntimeError):
    """A protocol subprocess failed; the message is its stderr."""


@dataclass(frozen=True)
class QueueRow:
    task_id: str
    state: str
    priority: str
    project: str
    title: str


@dataclass(frozen=True)
class AcceptanceItem:
    index: int
    checked: bool
    text: str


def dispatch_branch(task_id: str) -> str:
    """Mirror of `switchbard_core::dispatch_branch_name`."""
    return f"dispatch/{task_id.lower()}"


def dispatch_worktree(repo_root: Path, task_id: str) -> Path:
    """Mirror of `switchbard_core::dispatch_worktree_path`."""
    return repo_root / ".worktrees" / f"dispatch-{task_id.lower()}"


def dispatch_log_dir() -> Path:
    """Mirror of `switchbard_core::dispatch_log_dir`."""
    return Path(os.environ.get("TMPDIR", "/tmp")) / "switchbard-logs"


def dispatch_log_stem(task_id: str, started_at_unix: int) -> str:
    """Mirror of `switchbard_core::dispatch_log_stem`."""
    return f"dispatch-{task_id.lower()}-{started_at_unix}"


@dataclass
class Proto:
    repo_root: Path
    task_bin: str = "sb"
    claude_bin: str = "claude"
    gh_bin: str = "gh"
    git_bin: str = "git"
    max_turns: int = 50
    extra_env: dict[str, str] = field(default_factory=dict)

    # ---- switchbard-task queue protocol ----

    def _task(self, *args: str) -> str:
        return self._run([self.task_bin, "--repo", str(self.repo_root), *args])

    def queue_list(self) -> list[QueueRow]:
        rows = []
        for line in self._task("queue", "list").splitlines():
            parts = line.split("\t")
            if len(parts) == 5:
                rows.append(QueueRow(*parts))
        return rows

    def claim(self, task_id: str) -> str:
        """Acknowledge the handoff; returns the task's prior status."""
        return self._task("queue", "claim", task_id).strip()

    def release_dispatched(self, task_id: str, pr_url: str) -> None:
        self._task("queue", "release", task_id, "--outcome", "dispatched", "--pr", pr_url)

    def release_failed(self, task_id: str, reason: str, prior_status: str) -> None:
        self._task(
            "queue", "release", task_id, "--outcome", "failed",
            "--note", reason, "--prior-status", prior_status,
        )

    def prompt(self, task_id: str) -> str:
        return self._task("queue", "prompt", task_id)

    def acceptance(self, task_id: str, repo: Path | None = None) -> list[AcceptanceItem]:
        """Parse the checkbox lines out of `view` - the CLI's rendered
        `- [ ] #N text` shape is part of its output contract.

        `repo` defaults to the primary checkout; reconcile passes the RUN
        WORKTREE, because that is where the agent's AC checks live (they
        ride the PR; the primary checkout never sees them mid-run)."""
        out = self._run(
            [self.task_bin, "--repo", str(repo or self.repo_root), "view", task_id]
        )
        items: list[AcceptanceItem] = []
        in_section = False
        for line in out.splitlines():
            if line.startswith("## "):
                in_section = line.strip() == "## Acceptance Criteria"
                continue
            stripped = line.strip()
            if in_section and stripped.startswith("- ["):
                checked = stripped.startswith("- [x]") or stripped.startswith("- [X]")
                rest = stripped[5:].strip()
                index = 0
                if rest.startswith("#"):
                    head, _, tail = rest.partition(" ")
                    try:
                        index = int(head[1:])
                    except ValueError:
                        index = 0
                    rest = tail
                items.append(AcceptanceItem(index=index, checked=checked, text=rest))
        return items

    # ---- git / worktree ----

    def _git(self, *args: str, cwd: Path | None = None) -> str:
        return self._run([self.git_bin, "-C", str(cwd or self.repo_root), *args])

    def prepare_worktree(self, task_id: str, base_branch: str) -> Path:
        """Create (or reuse) the run's worktree on its dispatch branch -
        idempotent so a resumed run lands back in the same place."""
        worktree = dispatch_worktree(self.repo_root, task_id)
        branch = dispatch_branch(task_id)
        if worktree.is_dir():
            return worktree
        worktree.parent.mkdir(parents=True, exist_ok=True)
        try:
            self._git("worktree", "add", "-b", branch, str(worktree), base_branch)
        except ProtoError:
            # The branch may survive a removed worktree from a prior run.
            self._git("worktree", "add", str(worktree), branch)
        return worktree

    def commits_ahead(self, worktree: Path, base_branch: str) -> int:
        out = self._git("rev-list", "--count", f"{base_branch}..HEAD", cwd=worktree)
        return int(out.strip() or "0")

    def commit_all(self, worktree: Path, message: str) -> None:
        """Idempotent tail-commit of anything the agent left uncommitted."""
        self._git("add", "-A", cwd=worktree)
        status = self._git("status", "--porcelain", cwd=worktree)
        if status.strip():
            self._git("commit", "-m", message, cwd=worktree)

    def push(self, worktree: Path, branch: str) -> None:
        self._git("push", "-u", "origin", branch, cwd=worktree)

    def pr_create(self, worktree: Path, title: str, body: str) -> str:
        out = self._run(
            [self.gh_bin, "pr", "create", "--title", title, "--body", body],
            cwd=worktree,
        )
        for token in out.split():
            if token.startswith("https://") and "/pull/" in token:
                return token.strip()
        raise ProtoError(f"gh pr create returned no PR url: {out.strip()!r}")

    # ---- the agent ----

    def run_agent(self, prompt_path: Path, worktree: Path, log_path: Path) -> int:
        """Headless `claude -p` - acceptEdits, turn-bound, deliberately no
        wall-clock kill (the LED-580 lesson). Blocking; returns exit code."""
        with open(prompt_path, "rb") as prompt, open(log_path, "ab") as log:
            proc = subprocess.Popen(
                [
                    self.claude_bin, "-p",
                    "--permission-mode", "acceptEdits",
                    "--max-turns", str(self.max_turns),
                ],
                stdin=prompt,
                stdout=log,
                stderr=log,
                cwd=worktree,
                env={**os.environ, **self.extra_env},
            )
            return proc.wait()

    def run_gate(self, gate_cmd: str, worktree: Path, log_path: Path) -> int:
        with open(log_path, "ab") as log:
            log.write(f"\n--- gate: {gate_cmd} ---\n".encode())
            log.flush()
            proc = subprocess.Popen(
                ["/bin/sh", "-c", gate_cmd],
                stdout=log,
                stderr=log,
                cwd=worktree,
                env={**os.environ, **self.extra_env},
            )
            return proc.wait()

    # ---- plumbing ----

    def _run(self, argv: list[str], cwd: Path | None = None) -> str:
        result = subprocess.run(
            argv,
            capture_output=True,
            text=True,
            cwd=cwd,
            env={**os.environ, **self.extra_env},
        )
        if result.returncode != 0:
            detail = result.stderr.strip() or result.stdout.strip() or f"exit {result.returncode}"
            raise ProtoError(f"{' '.join(argv[:2])}: {detail}")
        return result.stdout


def now_unix() -> int:
    return int(time.time())
