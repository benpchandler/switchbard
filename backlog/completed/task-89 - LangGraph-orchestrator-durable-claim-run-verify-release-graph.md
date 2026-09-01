---
id: TASK-89
title: 'LangGraph orchestrator: durable claim-run-verify-release graph'
status: Done
assignee: []
created_date: '2026-09-01 01:56'
updated_date: '2026-09-01 02:12'
labels:
  - task-queue
  - orchestrator
  - langgraph
dependencies:
  - TASK-88
priority: high
project: Task Queue
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Python orchestrator at orchestrator/ (uv-managed, pinned langgraph + sqlite checkpointer - the xplan langgraph-mission-shadow probe's substrate, now adopted for switchbard's dispatch pickup per owner goal 2026-09-01). Per-task StateGraph: claim (via switchbard-task queue claim) -> worktree (git worktree add, dispatch/<id> branch) -> run agent (headless claude -p with the queue prompt, acceptEdits, max-turns bound) -> gate (repo CI task in the worktree) -> reconcile (every AC mapped to evidence; unproven ACs interrupt with the exact remainder - task-green is not outcome-proven, per the shadow's completion-integrity model) -> push + gh pr create -> release with outcome. SqliteSaver checkpoints under ~/.switchbard/orchestrator/ so a restart resumes mid-graph instead of orphaning the run; drain mode polls the queue on a cadence and respects max_concurrent=1 + gh spacing like drain_dispatch_queue.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 orchestrator drain picks up the top-ranked queued task, claims before any work, and drives it to PR or an honest failure release
- [x] #2 A killed orchestrator resumes its in-flight graph from the checkpoint after restart (proven by a kill/restart test)
- [x] #3 Reconcile interrupts with the exact unproven-AC remainder instead of releasing a false success
- [x] #4 All state writes to tasks go through switchbard-task; the orchestrator never edits task files
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
orchestrator/ package shipped: LangGraph StateGraph per task (claim->prepare->run_agent->collect->gate->reconcile->open_pr->release) with SQLite checkpoints under ~/.switchbard/orchestrator (SWITCHBARD_ORCHESTRATOR_HOME override); drain resumes claimed-with-checkpoint runs before claiming new work, serially, in stack-rank order. Reconcile = completion-integrity: commits + optional gate + every AC checked in the RUN WORKTREE's task copy (a real design bug caught while writing the E2E: reading the primary checkout would have interrupted every run); unproven interrupts with the exact remainder, driver releases an honest failure carrying it, resume re-claims and re-evaluates. All task mutations via the sb queue protocol - the orchestrator never edits task files. Events JSONL emitted per run (TASK-90's feed). Evidence: 5 graph tests incl. fresh-graph checkpoint resume (the kill/restart proxy: fresh proto, fresh sqlite connection, fresh compiled graph, invoke(None) continues past the claim without re-claiming) + 2 real-repo E2E tests driving the real sb binary, git worktrees, a bare origin, stub agent/gh: full walk to dispatched/In Review/PR-note, and the unproven walk to dispatch-failed with remainder in notes + prior status restored. uv run pytest: 7 passed; mise run ci green.
<!-- SECTION:FINAL_SUMMARY:END -->
