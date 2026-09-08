---
id: TASK-182
title: Resolve and validate parent links through the shared task layer
status: In Review
assignee: []
created_date: '2026-09-08 13:29'
updated_date: '2026-09-08 13:57'
labels:
  - tui
  - hierarchy
  - ball:me
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: task users lose parent-child grouping when shorthand parent references do not match canonical task IDs. Evidence: TASK-141.1 through TASK-141.4 store parent_task_id 141, parse.rs preserves it verbatim, and group.rs compares against TASK-141 literally; discovered during the owner status inquiry and parent-picker request. Scope: shared existing-task parent resolution and validation needed by the picker, retaining the supported one-level hierarchy.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Parent choices and parent mutations use one shared existing-task eligibility rule and reject self-parenting and unsupported nesting.
- [x] #2 Shorthand and canonical parent references resolve consistently for existing tasks, including alternate repository prefixes.
- [x] #3 Regression tests cover valid, missing, self, nested and legacy shorthand parent references.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Shared parent resolution and eligibility implemented in feat/tui-parent-picker. 147 core backlog tests pass, including legacy shorthand and alternate-prefix cases. Integration and remaining validation in progress.

Final review found two mixed-prefix inconsistencies: candidate selection inferred the source prefix, and native move could rewrite a legacy parent with the configured prefix instead of its actual ID. Both are being covered by regression tests before installing. Existing user task relationships remain untouched.

Implemented in b73cb28b on feat/tui-parent-picker, based on merged main f5c5f61c. Shared resolution, candidate eligibility, canonical create/move persistence, stale-write validation and mixed TASK/LED prefix regressions verified. 149 final backlog tests and core all-target clippy passed; prior full core suite passed 584 with 1 existing ignored test. Integrated into installed sbt via tui-install. No PR/merge for this feature yet. Existing runtime-claim rename gap tracked separately as TASK-185.
<!-- SECTION:NOTES:END -->
