---
id: TASK-71
title: 'Core goals module: def files + check-in log parse/write'
status: To Do
assignee: []
created_date: '2026-08-31 17:02'
labels:
  - goals
  - core
dependencies:
  - TASK-70
priority: high
project: Weekly goals
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
backlog/goals/<slug>.md over the shared frontmatter engine (pattern: backlog/hierarchy.rs): GoalDef {name, week, target, unit, measure (manual|tasks), scope}, check-in entries parsed from a '## Check-ins' body list ('- YYYY-MM-DD: N'), append-only check_in write op, loader riding load_backlog_repo so goals reach every snapshot.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Def round-trip + append-only check-in tests
- [ ] #2 Byte-surgical writes; write_layer_real_files stays green
- [ ] #3 Goals load into BacklogRepo without extra IO
<!-- AC:END -->
