---
id: TASK-90
title: 'Live progress: run events sidecar + GUI rendering'
status: Done
assignee: []
created_date: '2026-09-01 01:56'
updated_date: '2026-09-01 02:23'
labels:
  - task-queue
  - gui
  - design
dependencies:
  - TASK-89
priority: high
project: Task Queue
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The orchestrator appends structured JSONL events (node entered/exited, heartbeat, agent log bytes, interrupt reason) to <log stem>.events.jsonl in dispatch_log_dir. dispatch_inspect parses the sidecar into DispatchRun (current phase, last heartbeat, interrupt remainder); the Dispatches view and top-bar chip render live phase + progress instead of only elapsed time. Run design-state before the GUI half.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Events sidecar schema documented; orchestrator emits it; malformed/missing sidecar degrades to today's view
- [x] #2 Dispatches view shows current phase and last-heartbeat age per run, live while the orchestrator works
- [x] #3 An interrupted run surfaces its remainder (unproven ACs) in the run detail
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Design-state record (compact, per the standing rule) for the run-details progress block: (1) no sidecar / built-in-pipeline run - block renders nothing, view byte-identical to before (RunProgress::is_empty guard); (2) live phase - accent 'phase: <node>' + heartbeat age; (3) silent >60s with a phase open - heartbeat turns warn (nudge, not alarm - long blocking steps are legitimate, the LED-580 lesson); (4) interrupt - 'unproven remainder' list in warn, suppressed once a later attempt released dispatched; (5) terminal outcome - muted 'orchestrator: <outcome>'; (6) malformed/garbage sidecar - parse skips bad lines, degrades to empty (unit-tested); (7) resume - run_start clears the previous attempt's remainder/outcome (unit-tested). Stem unification found and fixed: driver now mints one stem per run, checkpointed in graph state, so log/prompt/events share it and dispatch_inspect can join them; resume reuses the checkpointed stem.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Events sidecar schema documented in events.py and emitted by the orchestrator (run_start/node_enter/node_exit/heartbeat/interrupt/release/run_end, ts_ms + tolerant-reader contract); dispatch_inspect::RunProgress parses the newest run's <stem>.events.jsonl (phase, heartbeat, remainder, outcome) with unit tests for tolerance, interrupt, and resume-clearing; the Dispatches run detail renders phase + heartbeat age (warn past 60s of silence), terminal outcome, and the exact unproven remainder - nothing renders for sidecar-less runs so legacy views are untouched. mise run ci + orchestrator pytest green.
<!-- SECTION:FINAL_SUMMARY:END -->
