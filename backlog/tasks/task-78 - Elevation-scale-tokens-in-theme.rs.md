---
id: TASK-78
title: Elevation scale tokens in theme.rs
status: To Do
assignee: []
created_date: '2026-08-31 21:20'
labels:
  - design
  - gui
dependencies: []
priority: high
project: EGUI Polish
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Codify levels 0-3 (well / panel / card / overlay) as named tokens - fill, stroke weight+color, shadow - for both palettes, with a doc comment stating the one rule for choosing a level. No surface sweeps yet; tokens land with the existing theme tests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both palettes carry the four-level scale; WCAG legibility contract still green
<!-- AC:END -->
