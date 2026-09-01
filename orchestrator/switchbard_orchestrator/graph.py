"""The per-task run graph (trajectory: *Task Queue orchestration*, TASK-89).

    claim -> prepare -> run_agent -> collect -> gate -> reconcile -> open_pr -> release
                              \\------------------(agent failed)------------------/

Durable by construction: every node boundary checkpoints (SQLite), so a
killed orchestrator resumes its in-flight run from the last completed node
instead of orphaning the claim. *Reconcile* carries the completion-integrity
model the xplan langgraph-mission-shadow probe validated: task-green is not
outcome-proven - every acceptance criterion must be checked, the branch must
actually carry commits, and a configured gate must pass; anything unproven
`interrupt()`s with the exact remainder rather than releasing a false
`dispatched`. The driver converts that interrupt into an honest failed
release; `resume` re-claims and re-enters reconcile, which re-evaluates
against fresh evidence.

Nothing in this graph edits task files: claims, releases, and reads all go
through the `switchbard-task queue` protocol (`proto.Proto`).
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import TypedDict

from langgraph.graph import END, START, StateGraph
from langgraph.types import interrupt

from .events import Emitter, Heartbeat
from .proto import dispatch_branch, dispatch_log_dir, dispatch_log_stem, now_unix


class RunState(TypedDict, total=False):
    task_id: str
    title: str
    prior_status: str
    base_branch: str
    worktree: str
    branch: str
    prompt_path: str
    log_path: str
    agent_exit: int
    gate_exit: int
    commits: int
    unproven: list[str]
    pr_url: str
    outcome: str
    failure: str


@dataclass
class RunOptions:
    base_branch: str = "main"
    gate_cmd: str | None = None


def build_run_graph(proto, emitter: Emitter, opts: RunOptions, checkpointer=None):
    """Compile the run graph. `proto` is duck-typed (`proto.Proto` or a test
    fake); every node emits enter/exit events through `emitter`."""

    def node(name):
        def wrap(fn):
            def inner(state: RunState) -> RunState:
                emitter.emit("node_enter", node=name)
                try:
                    update = fn(state)
                except BaseException:
                    emitter.emit("node_exit", node=name, ok=False)
                    raise
                emitter.emit("node_exit", node=name, ok=True)
                return update

            inner.__name__ = name
            return inner

        return wrap

    @node("claim")
    def claim(state: RunState) -> RunState:
        prior = proto.claim(state["task_id"])
        return {"prior_status": prior}

    @node("prepare")
    def prepare(state: RunState) -> RunState:
        task_id = state["task_id"]
        worktree = proto.prepare_worktree(task_id, state.get("base_branch", opts.base_branch))
        stem = dispatch_log_stem(task_id, now_unix())
        log_dir = dispatch_log_dir()
        log_dir.mkdir(parents=True, exist_ok=True)
        # Keep one log/prompt pair per run attempt; the events file for the
        # whole run is owned by the driver and outlives attempts.
        prompt_path = log_dir / f"{stem}-prompt.md"
        prompt_path.write_text(proto.prompt(task_id), encoding="utf-8")
        return {
            "worktree": str(worktree),
            "branch": dispatch_branch(task_id),
            "prompt_path": str(prompt_path),
            "log_path": str(log_dir / f"{stem}.log"),
        }

    @node("run_agent")
    def run_agent(state: RunState) -> RunState:
        log_path = Path(state["log_path"])
        with Heartbeat(emitter, log_path):
            exit_code = proto.run_agent(
                Path(state["prompt_path"]), Path(state["worktree"]), log_path
            )
        update: RunState = {"agent_exit": exit_code}
        if exit_code != 0:
            update["failure"] = f"claude exited with {exit_code}"
        return update

    @node("collect")
    def collect(state: RunState) -> RunState:
        worktree = Path(state["worktree"])
        proto.commit_all(worktree, f"{state['task_id']}: dispatch run")
        commits = proto.commits_ahead(worktree, state.get("base_branch", opts.base_branch))
        return {"commits": commits}

    @node("gate")
    def gate(state: RunState) -> RunState:
        if not opts.gate_cmd:
            return {}
        exit_code = proto.run_gate(
            opts.gate_cmd, Path(state["worktree"]), Path(state["log_path"])
        )
        return {"gate_exit": exit_code}

    @node("reconcile")
    def reconcile(state: RunState) -> RunState:
        unproven: list[str] = []
        if state.get("commits", 0) == 0:
            unproven.append("no commits on the dispatch branch")
        gate_exit = state.get("gate_exit")
        if opts.gate_cmd and gate_exit != 0:
            unproven.append(f"gate `{opts.gate_cmd}` failed (exit {gate_exit})")
        for item in proto.acceptance(state["task_id"], repo=Path(state["worktree"])):
            if not item.checked:
                unproven.append(f"AC #{item.index} unchecked: {item.text}")
        if unproven:
            emitter.emit("interrupt", remainder=unproven)
            # Halts here with the exact remainder; a resume re-runs this
            # node from the top against fresh evidence.
            interrupt({"remainder": unproven})
        return {"unproven": []}

    @node("open_pr")
    def open_pr(state: RunState) -> RunState:
        worktree = Path(state["worktree"])
        proto.push(worktree, state["branch"])
        title = f"{state['task_id']}: {state.get('title', 'dispatch run')}"
        body = (
            f"Automated dispatch run for {state['task_id']} via the Switchbard "
            f"orchestrator.\n\n🤖 Generated with switchbard-orchestrator"
        )
        return {"pr_url": proto.pr_create(worktree, title, body)}

    @node("release")
    def release(state: RunState) -> RunState:
        if state.get("failure"):
            proto.release_failed(
                state["task_id"], state["failure"], state.get("prior_status", "To Do")
            )
            emitter.emit("release", outcome="failed", reason=state["failure"])
            return {"outcome": "failed"}
        proto.release_dispatched(state["task_id"], state["pr_url"])
        emitter.emit("release", outcome="dispatched", pr_url=state["pr_url"])
        return {"outcome": "dispatched"}

    graph = StateGraph(RunState)
    for fn in (claim, prepare, run_agent, collect, gate, reconcile, open_pr, release):
        graph.add_node(fn.__name__, fn)
    graph.add_edge(START, "claim")
    graph.add_edge("claim", "prepare")
    graph.add_edge("prepare", "run_agent")
    graph.add_conditional_edges(
        "run_agent",
        lambda state: "release" if state.get("failure") else "collect",
        {"release": "release", "collect": "collect"},
    )
    graph.add_edge("collect", "gate")
    graph.add_edge("gate", "reconcile")
    graph.add_edge("reconcile", "open_pr")
    graph.add_edge("open_pr", "release")
    graph.add_edge("release", END)
    return graph.compile(checkpointer=checkpointer)
