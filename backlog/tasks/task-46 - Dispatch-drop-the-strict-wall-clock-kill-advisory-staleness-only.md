---
id: TASK-46
title: 'Dispatch: drop the strict wall-clock kill; advisory staleness only'
status: Done
assignee: []
created_date: '2026-08-20 14:27'
updated_date: '2026-08-25 00:21'
labels: []
dependencies: []
priority: medium
ordinal: 46000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
LED-580 post-mortem (2026-08-20): a productive 30-minute run doing legitimate work across 29 files was hard-killed at the wall-clock timeout, stranding uncommitted work. Owner decision: no strict wall clock. Runaway protection is now layered elsewhere: --max-turns bounds loops at the source, the identity-gated Kill button gives manual control, and is_abandoned/stalled detection feeds the attention chip. Change DispatchOptions.timeout to an advisory staleness threshold: no automatic kill_pgid; a run past the threshold classifies as needs-attention (chip + Dispatch view) but keeps running. Keep the --max-turns loop bound. Update the deadline label ('hard kill in Ym') to honest running-time/attention wording, and the dispatch.rs module doc + wait path accordingly.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 No code path kills a dispatch run on wall-clock time
- [x] #2 A run past the advisory threshold surfaces as needs-attention in chip and Dispatch view while continuing to run
- [x] #3 Deadline label and hover copy no longer promise a hard kill
- [x] #4 max_turns loop bound preserved; docs updated; mise run ci green
<!-- AC:END -->
