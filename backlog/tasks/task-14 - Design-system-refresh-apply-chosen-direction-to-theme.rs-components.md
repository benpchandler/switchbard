---
id: TASK-14
title: 'Design system refresh: apply chosen direction to theme.rs + components'
status: In Progress
assignee: []
created_date: '2026-08-05 03:03'
updated_date: '2026-08-05 03:17'
labels:
  - hub
  - design
dependencies:
  - TASK-10
priority: high
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Visual redesign of the app per the design-directions artifact (2026-08-04): three candidate directions — A Operator's Console (dark, lamp/jack language), B Flight Strips (light, ATC strip bays, drag-reorder = ordering.yml), C Instrument Panel (Rams gray, signal orange). Direction pending owner pick; recommendation on record is B with A's lamp language for agent state. Scope: embedded fonts, theme.rs palette, component pass (badges, rows, buttons), keep WCAG-AA legibility contract + legibility_audit green. Sequence AFTER feature/unified-backlog-view lands (TASK-10) — same ui/ surface.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
DECISION (owner, 2026-08-04): Direction B — Flight Strips — chosen, borrowing A's lamp-status language for agent/dispatch state. Reference: design-directions artifact v1 (three-directions-v1). Board #DFE3E6, strip #FBFBF9, 7 distinguishable repo rail hues, overdue #B3391F, bays = triage tiers, Barlow Semi Condensed + JetBrains Mono embedded (OFL).
<!-- SECTION:NOTES:END -->
