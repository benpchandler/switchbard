---
id: TASK-182
title: Resolve and validate parent links through the shared task layer
status: In Progress
assignee: []
created_date: '2026-09-08 13:29'
updated_date: '2026-09-08 13:30'
labels:
  - tui
  - hierarchy
  - ball:agent
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: task users lose parent-child grouping when shorthand parent references do not match canonical task IDs. Evidence: TASK-141.1 through TASK-141.4 store parent_task_id 141, parse.rs preserves it verbatim, and group.rs compares against TASK-141 literally; discovered during the owner status inquiry and parent-picker request. Scope: shared existing-task parent resolution and validation needed by the picker, retaining the supported one-level hierarchy.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Parent choices and parent mutations use one shared existing-task eligibility rule and reject self-parenting and unsupported nesting.
- [ ] #2 Shorthand and canonical parent references resolve consistently for existing tasks, including alternate repository prefixes.
- [ ] #3 Regression tests cover valid, missing, self, nested and legacy shorthand parent references.
<!-- AC:END -->
