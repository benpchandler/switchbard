---
id: TASK-44
title: 'Refine task: AI-assisted fill of description/ACs/plan before dispatch'
status: Done
assignee: []
created_date: '2026-08-20 02:28'
updated_date: '2026-08-20 13:11'
labels: []
dependencies: []
priority: high
ordinal: 44000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A half-baked card dispatched as-is produces a weak agent run. Add a 'Refine' button to the task detail rail (near Dispatch): runs a headless claude -p against the repo root (read-only exploration) fed the task's current content, gets back structured JSON (enriched description, acceptance criteria list, implementation plan), and applies it additively through the backlog CLI (edit_backlog_task / BacklogTaskPatch) — never editing task markdown directly, never destroying human-authored content. No new label state machine: spawn_* thread + backlog_status pattern, button disabled while a refine is in flight for that task. Document the feature in docs/product-trajectory.md (unified task hub).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Refine button on editable tasks produces enriched description, ACs, and implementation plan applied via the backlog CLI
- [ ] #2 Existing description text and existing ACs are preserved (additive merge)
- [ ] #3 Malformed/partial agent output applies nothing and reports a clear status message
- [ ] #4 Refine runs are bounded by a timeout and cannot stack per task
- [ ] #5 Prompt builder and JSON parsing unit-tested in switchbard-core; product-trajectory.md updated; mise run ci green
<!-- AC:END -->
