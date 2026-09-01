---
id: TASK-88
title: 'Queue protocol: first-class queue verbs on switchbard-task'
status: Done
assignee: []
created_date: '2026-09-01 01:56'
updated_date: '2026-09-01 02:01'
labels:
  - task-queue
  - cli
  - protocol
dependencies: []
priority: high
project: Task Queue
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The agent-facing boundary the orchestrator drives (standard: ~/.claude/standards/agent-facing-design.md - never lose intent, ack = claim-before-work, stdout payload only, status+next_step pairing). Verb family: 'queue list' (dispatch-labeled tasks in stack-rank computed order, TSV: id, claim state, priority, project, title), 'queue send <id>' / 'queue withdraw <id>' (add/remove the dispatch label), 'queue claim <id>' (the dispatch->dispatching label swap via claim_task_for_dispatch - work stays queued until acknowledged; a killed reader loses nothing), 'queue release <id> --outcome dispatched|failed --note <text>' (outcome + note through the native write layer), 'queue prompt <id>' (prints build_dispatch_prompt output so the orchestrator never re-derives the prompt). Help text is the output contract.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 queue list prints the dispatch-labeled queue in computed rank order with claim states
- [x] #2 queue send/withdraw/claim/release round-trip through the existing label ladder and write layer; double-claim is refused with a next step
- [x] #3 queue prompt emits exactly build_dispatch_prompt's output on stdout
- [x] #4 Process-boundary CLI tests cover the family incl. error shapes
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
queue verb family shipped on switchbard-task: list (dispatch-labeled tasks in stack-rank computed order with claim states, TSV), send/withdraw (withdraw refuses in-flight tasks), claim (the dispatch->dispatching acknowledgment before any work; prints the prior status for release to restore; moves to In Progress like dispatch_one), release --outcome dispatched|failed (walks the exact release_as_dispatched/release_as_failed ladder the Rust pipeline uses - made pub for one claim vocabulary), and prompt (build_dispatch_prompt verbatim). Errors are one stderr line with next steps; help text documents the protocol. Two process-boundary tests cover the full ladder walk, rank ordering, double-claim/withdraw refusals, evidence-required releases, and prompt parity. mise run ci green.
<!-- SECTION:FINAL_SUMMARY:END -->
