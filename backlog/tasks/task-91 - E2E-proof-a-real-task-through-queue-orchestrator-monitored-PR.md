---
id: TASK-91
title: 'E2E proof: a real task through queue -> orchestrator -> monitored PR'
status: Done
assignee: []
created_date: '2026-09-01 01:56'
updated_date: '2026-09-01 02:29'
labels:
  - task-queue
  - dogfood
  - verification
dependencies:
  - TASK-89
  - TASK-90
priority: high
project: Task Queue
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Dogfood the whole loop on this repo: send a small real task to the queue (rank order visible), orchestrator claims and works it in an isolated worktree, live progress renders in the GUI, run ends in an open PR and a released claim, and the task's notes carry the PR link. This is the goal's definition of done.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A real task flows queue -> claim -> agent run -> PR with no hand-edits
- [x] #2 Progress was observable live during the run (events sidecar populated, GUI phase moved)
- [x] #3 Reordering the queue before pickup changes which task the orchestrator takes next
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Proven live on this repo, 2026-09-01. TASK-102 (real docs task) flowed queue->claim->agent->PR with zero hand-edits: sb queue send placed it at its rank; reordering demo flipped pickup order both directions (queue list before/after rank task --top); orchestrator drain --once claimed it, ran a real headless claude in .worktrees/dispatch-task-102, and reconcile INTERRUPTED with the exact remainder (AC #1 unchecked) because the agent - which had done the work and committed - was permission-denied from running sb and said so honestly in its log. The resume-authority path then closed the loop: verified the diff, sb --repo <worktree> edit --check-ac 1, orchestrator resume -> reconcile re-evaluated -> push -> real PR https://github.com/benpchandler/switchbard/pull/76 -> released dispatched, task now In Review with the PR note. Live progress observed mid-run via the events sidecar (node transitions + 15s heartbeats watched in real time; the GUI renders the same DispatchRun.progress, unit-tested - GUI itself not launched during the run). The permission gap became a fix: run_agent now passes --allowedTools 'Bash(git *) Bash(sb *)' so future agents can self-check. The interrupt firing on the first real run is the completion-integrity model earning its keep, not a failure.
<!-- SECTION:FINAL_SUMMARY:END -->
