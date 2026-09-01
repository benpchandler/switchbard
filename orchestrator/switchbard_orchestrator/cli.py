"""Driver: drain the queue, resume interrupted runs, report status.

    python -m switchbard_orchestrator drain  --repo PATH [--once] [--gate CMD] ...
    python -m switchbard_orchestrator resume --repo PATH --task ID [--gate CMD] ...
    python -m switchbard_orchestrator status --repo PATH

Agent-facing contract (the standard's channel discipline): stdout carries
one line per completed run - `<TASK-ID>\t<outcome>[\t<pr-url-or-reason>]` -
and nothing else; narration goes to stderr. Drain is serial on purpose
(same GitHub rate-limit reasoning as `drain_dispatch_queue`); `--once`
processes at most one task and exits, which is also the test/dogfood mode.

Durability: one checkpoint thread per (repo, task) under
`~/.switchbard/orchestrator/checkpoints.sqlite`. On start, drain first
resumes any *claimed* task that has a checkpoint (a killed orchestrator's
in-flight run) before claiming anything new.
"""

from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

from langgraph.checkpoint.sqlite import SqliteSaver
from langgraph.types import Command

from .events import Emitter
from .graph import RunOptions, build_run_graph
from .proto import Proto, dispatch_log_dir, dispatch_log_stem, now_unix


def checkpoint_dir() -> Path:
    import os

    override = os.environ.get("SWITCHBARD_ORCHESTRATOR_HOME")
    if override:
        return Path(override)
    return Path.home() / ".switchbard" / "orchestrator"


def thread_id(repo_root: Path, task_id: str) -> str:
    return f"{repo_root}|{task_id}"


def narrate(message: str) -> None:
    print(f"orchestrator: {message}", file=sys.stderr, flush=True)


class Driver:
    def __init__(self, proto: Proto, opts: RunOptions, saver: SqliteSaver) -> None:
        self.proto = proto
        self.opts = opts
        self.saver = saver

    def _config(self, task_id: str) -> dict:
        return {"configurable": {"thread_id": thread_id(self.proto.repo_root, task_id)}}

    def _emitter(self, task_id: str) -> Emitter:
        stem = dispatch_log_stem(task_id, now_unix())
        return Emitter(dispatch_log_dir() / f"{stem}.events.jsonl", task_id)

    def _finish(self, task_id: str, emitter: Emitter, result: dict) -> str:
        """Turn a graph result into the stdout contract line; convert an
        interrupt into an honest failed release carrying the remainder."""
        if "__interrupt__" in result:
            remainder = list(result["__interrupt__"][0].value.get("remainder", []))
            state = self.graph_state(task_id)
            prior = state.get("prior_status", "To Do")
            reason = "unproven outcome: " + "; ".join(remainder)
            self.proto.release_failed(task_id, reason, prior)
            emitter.emit("release", outcome="failed", reason=reason)
            emitter.emit("run_end", outcome="failed")
            return f"{task_id}\tfailed\t{reason}"
        outcome = result.get("outcome", "failed")
        emitter.emit("run_end", outcome=outcome)
        if outcome == "dispatched":
            return f"{task_id}\tdispatched\t{result.get('pr_url', '')}"
        return f"{task_id}\tfailed\t{result.get('failure', 'unknown failure')}"

    def graph_state(self, task_id: str) -> dict:
        graph = build_run_graph(self.proto, self._emitter(task_id), self.opts, self.saver)
        snapshot = graph.get_state(self._config(task_id))
        return dict(snapshot.values or {})

    def run_new(self, task_id: str, title: str) -> str:
        emitter = self._emitter(task_id)
        emitter.emit("run_start", mode="new")
        graph = build_run_graph(self.proto, emitter, self.opts, self.saver)
        result = graph.invoke(
            {"task_id": task_id, "title": title, "base_branch": self.opts.base_branch},
            self._config(task_id),
        )
        return self._finish(task_id, emitter, result)

    def run_resume(self, task_id: str, resume_interrupt: bool) -> str:
        emitter = self._emitter(task_id)
        emitter.emit("run_start", mode="resume")
        graph = build_run_graph(self.proto, emitter, self.opts, self.saver)
        payload = Command(resume="resumed") if resume_interrupt else None
        result = graph.invoke(payload, self._config(task_id))
        return self._finish(task_id, emitter, result)

    def has_checkpoint(self, task_id: str) -> bool:
        graph = build_run_graph(self.proto, self._emitter(task_id), self.opts, self.saver)
        snapshot = graph.get_state(self._config(task_id))
        return bool(snapshot.values)


def cmd_drain(args: argparse.Namespace) -> int:
    proto, opts = _build(args)
    with SqliteSaver.from_conn_string(str(_checkpoint_db())) as saver:
        driver = Driver(proto, opts, saver)
        while True:
            worked = _drain_pass(driver, args.once)
            if args.once:
                return 0
            if not worked:
                time.sleep(args.poll_seconds)


def _drain_pass(driver: Driver, once: bool) -> bool:
    """One queue sweep. Resumes orphaned claims first, then claims new
    work, top of the stack-rank order first. Returns whether anything ran."""
    worked = False
    rows = driver.proto.queue_list()
    for row in rows:
        if row.state == "claimed" and driver.has_checkpoint(row.task_id):
            narrate(f"resuming in-flight run for {row.task_id}")
            print(driver.run_resume(row.task_id, resume_interrupt=False), flush=True)
            worked = True
            if once:
                return worked
    for row in driver.proto.queue_list():
        if row.state != "queued":
            continue
        narrate(f"claiming {row.task_id} ({row.title})")
        try:
            print(driver.run_new(row.task_id, row.title), flush=True)
        except Exception as err:  # release honestly, keep the checkpoint
            _fail_open(driver, row.task_id, err)
        worked = True
        if once:
            return worked
    return worked


def _fail_open(driver: Driver, task_id: str, err: Exception) -> None:
    reason = f"orchestrator error: {err}"
    narrate(reason)
    prior = driver.graph_state(task_id).get("prior_status", "To Do")
    try:
        driver.proto.release_failed(task_id, reason, prior)
    except Exception as release_err:
        narrate(f"release after failure also failed: {release_err}")
    print(f"{task_id}\tfailed\t{reason}", flush=True)


def cmd_resume(args: argparse.Namespace) -> int:
    proto, opts = _build(args)
    with SqliteSaver.from_conn_string(str(_checkpoint_db())) as saver:
        driver = Driver(proto, opts, saver)
        if not driver.has_checkpoint(args.task):
            print(
                f"orchestrator: error: no checkpoint for {args.task} - "
                f"nothing to resume; `drain` starts fresh runs",
                file=sys.stderr,
            )
            return 1
        # A human resume follows a remainder interrupt: the task was
        # released failed, so re-queue and re-claim before re-entering.
        states = {row.task_id: row.state for row in proto.queue_list()}
        if states.get(args.task) != "claimed":
            proto._task("queue", "send", args.task)
            proto.claim(args.task)
        print(driver.run_resume(args.task, resume_interrupt=True), flush=True)
        return 0


def cmd_status(args: argparse.Namespace) -> int:
    proto, _ = _build(args)
    for row in proto.queue_list():
        print(f"{row.task_id}\t{row.state}\t{row.title}")
    return 0


def _checkpoint_db() -> Path:
    directory = checkpoint_dir()
    directory.mkdir(parents=True, exist_ok=True)
    return directory / "checkpoints.sqlite"


def _build(args: argparse.Namespace) -> tuple[Proto, RunOptions]:
    proto = Proto(
        repo_root=Path(args.repo).resolve(),
        task_bin=args.task_bin,
        claude_bin=args.claude_bin,
        gh_bin=args.gh_bin,
        max_turns=args.max_turns,
    )
    return proto, RunOptions(base_branch=args.base_branch, gate_cmd=args.gate)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="switchbard-orchestrator")
    sub = parser.add_subparsers(dest="command", required=True)
    for name, fn in (("drain", cmd_drain), ("resume", cmd_resume), ("status", cmd_status)):
        p = sub.add_parser(name)
        p.add_argument("--repo", required=True, help="Backlog repo root")
        p.add_argument("--task-bin", default="switchbard-task")
        p.add_argument("--claude-bin", default="claude")
        p.add_argument("--gh-bin", default="gh")
        p.add_argument("--base-branch", default="main")
        p.add_argument("--gate", default=None, help="gate command run in the worktree")
        p.add_argument("--max-turns", type=int, default=50)
        p.set_defaults(fn=fn)
    sub.choices["drain"].add_argument("--once", action="store_true")
    sub.choices["drain"].add_argument("--poll-seconds", type=float, default=30.0)
    sub.choices["resume"].add_argument("--task", required=True)
    args = parser.parse_args(argv)
    return args.fn(args)
