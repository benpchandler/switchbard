---
id: TASK-80.4
title: Render mixed local and GitHub-backed work in the Task Queue
status: To Do
assignee: []
created_date: '2026-08-31 21:45'
updated_date: '2026-09-01 12:37'
labels:
  - gui
  - github
  - task-queue
  - design
dependencies:
  - TASK-80.2
  - TASK-80.3
parent_task_id: TASK-80
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the existing IA V2 Tasks / Dispatches Task Queue surface against the approved authority contract. Present the local dispatch band and explicitly configured GitHub Project bands without shadow tasks, show GitHub delivery state as sourced observation, and keep local human outcome acceptance distinct from merged, released, or deployed evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Users can distinguish a native task, a linked local task with nested delivery evidence, and an unlinked remote projection, including source and Project-membership provenance.
- [ ] #2 Queue order is the local dispatch stack-rank band followed by configured GitHub Project bands in binding order and GitHub item order; remote-only items gain Switchbard rank only through explicit local adoption, which suppresses the standalone projection.
- [ ] #3 Issue, PR, checks, merge, release, deployment, freshness, Unknown, Stale, Unavailable, partial, and MissingOrInaccessible states use honest progressive disclosure and never imply local Done.
- [ ] #4 Empty, loading, partial, stale, unavailable, rate-limited, mixed-source, duplicate-membership, linked, narrow-window, long-title, and high-volume states are designed and verified under the design-state matrix.
- [ ] #5 Refresh stays off the render path, at most 100 remote rows disclose per source page, and measured rendering remains within the existing Switchbard performance contract.
- [ ] #6 Before mixed-source body implementation, create and owner-review docs/decisions/task-queue-authority-model/task-queue-visual-canonical.html; before completion, compare the clean exact implementation revision against both named canonicals in Visual Review, resolve findings against current evidence, and record explicit human approval.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Authority correction (2026-09-01): the prior acceptance criterion that mixed Switchbard priority with GitHub attention signals is superseded by the owner-approved deterministic source-band model in docs/decisions/task-queue-authority-model/. This task must satisfy that package and must not invent remote-only priority or an attention-based reorder.
<!-- SECTION:NOTES:END -->
