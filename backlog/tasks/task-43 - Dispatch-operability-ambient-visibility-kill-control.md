---
id: TASK-43
title: 'Dispatch operability: ambient visibility + kill control'
status: Done
assignee: []
created_date: '2026-08-20 02:28'
updated_date: '2026-08-20 13:11'
labels: []
dependencies: []
priority: high
ordinal: 43000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Dispatch state is pull-only today (Dispatches tab) and the only control over a runaway agent is the silent 30-min timeout. Add: (1) top-bar chip '⚙ N running · <oldest elapsed>' in dispatch_accent, flipping to danger when anything is Failed/Orphaned/stalled, hidden when idle, click jumps to Dispatches tab; (2) Dispatches tab badge with attention count; (3) pgid sidecar file written by dispatch_one at spawn and removed on release, enabling a confirm-armed Kill button on in-flight rows (kill_pgid; the blocked pipeline does the bookkeeping and releases as dispatch-failed); (4) in-flight rows show elapsed + 'hard kill in Xm'; (5) max_turns in DispatchOptions passed as --max-turns (default 50).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Top-bar chip visible from every tab whenever a run is queued/in flight/needs attention; hidden otherwise; click navigates to Dispatches
- [ ] #2 Kill button on in-flight rows terminates the run's process group; task lands on dispatch-failed with a note, via the existing pipeline release path
- [ ] #3 Sidecar pid file created at spawn, removed on release; dispatch_inspect surfaces it
- [ ] #4 In-flight rows show elapsed and time remaining until hard kill
- [ ] #5 claude -p invoked with --max-turns from DispatchOptions
- [ ] #6 Per-frame work in chip/badge stays arithmetic on cached state; mise run ci green
<!-- AC:END -->
