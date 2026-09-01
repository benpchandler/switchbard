---
id: TASK-114
title: Perf-doc staleness sweep from IA V2
status: To Do
assignee: []
created_date: '2026-09-01 08:20'
labels:
  - docs
  - perf
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
tasks_place_perf_smoke.rs's doc comment cites projects_rank_perf_smoke's baseline as ~26-28ms p95 (per TASK-97's own evidence, the fixture now measures roughly ~14ms). Separately, the IA V2 decision record cites TASK-78 as the fix for the dark warn-contrast issue, but TASK-78's acceptance criteria are about elevation tokens, not contrast.

Impact: future perf work anchors comparisons on a stale baseline (false regressions flagged, or true regressions missed), and a reader following the decision record's citation to TASK-78 will not find the contrast fix they're looking for.

Evidence: tasks_place_perf_smoke.rs doc comment (cites ~26-28ms); TASK-97's Final Summary/notes (measured projects_rank_perf_smoke-class fixture at p95 ~14.4ms); IA V2 decision record's TASK-78 citation vs TASK-78's actual ACs (elevation tokens).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 tasks_place_perf_smoke.rs's baseline citation corrected to the current projects_rank_perf_smoke measurement
- [ ] #2 Decision record's citation corrected to name the task that actually fixed the dark warn-contrast issue (or the claim corrected if no such task exists)
<!-- AC:END -->
