---
id: TASK-89
title: 'LangGraph orchestrator: durable claim-run-verify-release graph'
status: To Do
assignee: []
created_date: '2026-09-01 01:56'
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
- [ ] #1 orchestrator drain picks up the top-ranked queued task, claims before any work, and drives it to PR or an honest failure release
- [ ] #2 A killed orchestrator resumes its in-flight graph from the checkpoint after restart (proven by a kill/restart test)
- [ ] #3 Reconcile interrupts with the exact unproven-AC remainder instead of releasing a false success
- [ ] #4 All state writes to tasks go through switchbard-task; the orchestrator never edits task files
<!-- AC:END -->
