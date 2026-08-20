---
id: TASK-45
title: >-
  Save path can silently delete custom task sections (guard or preserve unknown
  headings)
status: To Do
assignee: []
created_date: '2026-08-20 03:32'
labels: []
dependencies: []
priority: high
ordinal: 45000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Measured during TASK-44: 51 of 345 real task files (all in the budget repo, zero in switchbard) carry human-written sections the Backlog format has no field for (## Resolution, ## Root Cause Hypothesis, ## Reproduction Steps). parse_task_file extracts six known sections; content under any other heading lands in no BacklogTask field, so the detail rail's Save (-d replace-write from the parsed description) genuinely deletes those sections today. Refine is guarded by task_file_round_trips since PR #25; Save is not. Two candidate fixes, product call required: (a) wire Save through the same round-trip guard — safe but starts refusing saves on ~15% of real tasks; (b) the real fix: make the parse/write cycle round-trip-complete by preserving unknown sections as opaque blocks and re-emitting them on write, then Save never needs to refuse. Prefer (b) unless its CLI write path can't express it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A Save on a task file carrying an unknown ## section cannot delete that section
- [ ] #2 Decision between refuse-vs-preserve recorded in docs/product-trajectory.md with rationale
- [ ] #3 Regression test with a custom-heading fixture on the Save path
<!-- AC:END -->
