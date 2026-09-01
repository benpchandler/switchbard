"""Graph-transition tests over a fake proto - no git, gh, or agent.

The fake honors the protocol's shapes (claim returns prior status, releases
record outcome) so these tests pin the graph's *decisions*: happy path
releases dispatched with the PR, an agent failure routes straight to an
honest failed release, an unproven outcome interrupts with the exact
remainder instead of releasing, a resume re-evaluates fresh evidence, and
a killed run resumes from its checkpoint in a fresh graph instance.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

import pytest
from langgraph.checkpoint.sqlite import SqliteSaver
from langgraph.types import Command

from switchbard_orchestrator.events import Emitter
from switchbard_orchestrator.graph import RunOptions, build_run_graph
from switchbard_orchestrator.proto import AcceptanceItem, ProtoError


@dataclass
class FakeProto:
    repo_root: Path
    agent_exit: int = 0
    agent_crash: bool = False
    gate_exit: int = 0
    commits: int = 1
    acceptance_checked: bool = True
    calls: list[tuple] = field(default_factory=list)
    released: tuple | None = None

    def claim(self, task_id):
        self.calls.append(("claim", task_id))
        return "To Do"

    def prepare_worktree(self, task_id, base_branch):
        worktree = self.repo_root / "wt" / task_id
        worktree.mkdir(parents=True, exist_ok=True)
        return worktree

    def prompt(self, task_id):
        return f"work {task_id}"

    def run_agent(self, prompt_path, worktree, log_path):
        self.calls.append(("run_agent", str(prompt_path)))
        if self.agent_crash:
            raise ProtoError("orchestrator killed mid-run")
        Path(log_path).write_text("agent output\n")
        return self.agent_exit

    def run_gate(self, gate_cmd, worktree, log_path):
        self.calls.append(("gate", gate_cmd))
        return self.gate_exit

    def commit_all(self, worktree, message):
        self.calls.append(("commit_all", message))

    def commits_ahead(self, worktree, base_branch):
        return self.commits

    def acceptance(self, task_id, repo=None):
        return [
            AcceptanceItem(index=1, checked=True, text="always done"),
            AcceptanceItem(index=2, checked=self.acceptance_checked, text="the real proof"),
        ]

    def push(self, worktree, branch):
        self.calls.append(("push", branch))

    def pr_create(self, worktree, title, body):
        return "https://example.test/pull/9"

    def release_dispatched(self, task_id, pr_url):
        self.released = ("dispatched", task_id, pr_url)

    def release_failed(self, task_id, reason, prior_status):
        self.released = ("failed", task_id, reason, prior_status)


def run(tmp_path, proto, opts=None, saver=None, payload="new"):
    emitter = Emitter(tmp_path / "run.events.jsonl", "TASK-1")
    graph = build_run_graph(proto, emitter, opts or RunOptions(), saver)
    config = {"configurable": {"thread_id": "t"}} if saver else None
    state = (
        {"task_id": "TASK-1", "title": "Fixture", "base_branch": "main"}
        if payload == "new"
        else payload
    )
    return graph.invoke(state, config)


def events(tmp_path):
    lines = (tmp_path / "run.events.jsonl").read_text().splitlines()
    return [json.loads(line) for line in lines]


def test_happy_path_releases_dispatched_with_the_pr(tmp_path):
    proto = FakeProto(tmp_path)
    result = run(tmp_path, proto)
    assert result["outcome"] == "dispatched"
    assert proto.released == ("dispatched", "TASK-1", "https://example.test/pull/9")
    names = [e["node"] for e in events(tmp_path) if e["event"] == "node_enter"]
    assert names == [
        "claim", "prepare", "run_agent", "collect", "gate", "reconcile", "open_pr", "release",
    ]


def test_agent_failure_routes_to_an_honest_failed_release(tmp_path):
    proto = FakeProto(tmp_path, agent_exit=2)
    result = run(tmp_path, proto)
    assert result["outcome"] == "failed"
    assert proto.released[0] == "failed"
    assert "claude exited with 2" in proto.released[2]
    assert proto.released[3] == "To Do", "prior status restored"
    names = [e["node"] for e in events(tmp_path) if e["event"] == "node_enter"]
    assert "open_pr" not in names, "no PR for a failed run"


def test_unproven_outcome_interrupts_with_the_exact_remainder(tmp_path):
    proto = FakeProto(tmp_path, acceptance_checked=False, commits=0)
    saver_path = tmp_path / "ck.sqlite"
    with SqliteSaver.from_conn_string(str(saver_path)) as saver:
        result = run(tmp_path, proto, saver=saver)
        assert "__interrupt__" in result, "task-green is not outcome-proven"
        remainder = result["__interrupt__"][0].value["remainder"]
        assert remainder == [
            "no commits on the dispatch branch",
            "AC #2 unchecked: the real proof",
        ]
        assert proto.released is None, "the graph itself never releases a false claim"

        # Human fixes the evidence; resume re-enters reconcile and finishes.
        proto.acceptance_checked = True
        proto.commits = 3
        result = run(tmp_path, proto, saver=saver, payload=Command(resume="go"))
        assert result["outcome"] == "dispatched"
        assert proto.released[0] == "dispatched"


def test_gate_failure_is_part_of_the_remainder(tmp_path):
    proto = FakeProto(tmp_path, gate_exit=1)
    saver_path = tmp_path / "ck.sqlite"
    with SqliteSaver.from_conn_string(str(saver_path)) as saver:
        result = run(tmp_path, proto, opts=RunOptions(gate_cmd="mise run ci"), saver=saver)
        remainder = result["__interrupt__"][0].value["remainder"]
        assert remainder == ["gate `mise run ci` failed (exit 1)"]


def test_a_killed_run_resumes_from_its_checkpoint_in_a_fresh_graph(tmp_path):
    saver_path = tmp_path / "ck.sqlite"
    proto = FakeProto(tmp_path, agent_crash=True)
    with SqliteSaver.from_conn_string(str(saver_path)) as saver:
        with pytest.raises(ProtoError):
            run(tmp_path, proto, saver=saver)
        assert ("claim", "TASK-1") in proto.calls, "died mid-run, after the claim"
        assert proto.released is None

    # "Restart": a fresh proto, a fresh saver connection over the same
    # sqlite file, a fresh compiled graph - continue with invoke(None).
    revived = FakeProto(tmp_path)
    with SqliteSaver.from_conn_string(str(saver_path)) as saver:
        result = run(tmp_path, revived, saver=saver, payload=None)
        assert result["outcome"] == "dispatched"
        assert ("claim", "TASK-1") not in revived.calls, (
            "resume continues from the checkpoint instead of re-claiming"
        )
        assert revived.released[0] == "dispatched"
