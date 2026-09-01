---
id: TASK-97
title: 'IA V2: Tasks place - generic grouping, filter builder, rank sort, expanding headers'
status: To Do
assignee: []
created_date: '2026-09-01 02:24'
labels:
  - ia
  - gui
dependencies: []
priority: high
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The primary work list (trajectory: IA V2). Group-by generic over every field (project, status, initiative, priority, label, repo, ...), filter-builder + recent filters (no hardcoded chips), Sort: rank via the stack-ranking order, group headers with computed roll-ups that EXPAND IN PLACE for summary (roll-up, goal pace, description) - the project page is cut per the decision record. Stroke-ring selection (unify with TASK-38). Sub-issues indent in place. List/Board as view modes sharing facet state. Inherits TASK-13 virtualization obligations.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Group-by works over any field with computed roll-up headers; no project page anywhere
- [ ] #2 Expanded header shows summary inline; stack rank appears only as a sort option
- [ ] #3 Selection uses the Board's stroke ring in list rows; List/Board share facets
<!-- AC:END -->
