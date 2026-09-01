---
id: TASK-90
title: 'Live progress: run events sidecar + GUI rendering'
status: To Do
assignee: []
created_date: '2026-09-01 01:56'
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
- [ ] #1 Events sidecar schema documented; orchestrator emits it; malformed/missing sidecar degrades to today's view
- [ ] #2 Dispatches view shows current phase and last-heartbeat age per run, live while the orchestrator works
- [ ] #3 An interrupted run surfaces its remainder (unproven ACs) in the run detail
<!-- AC:END -->
