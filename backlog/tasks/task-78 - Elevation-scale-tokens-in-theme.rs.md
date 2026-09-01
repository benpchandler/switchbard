---
id: TASK-78
title: Elevation scale tokens in theme.rs
status: To Do
assignee: []
created_date: '2026-08-31 21:20'
updated_date: '2026-09-01 11:29'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
From TASK-76's WCAG audit (2026-09-01): dark warn_orange #E87A5A reads 4.52:1 on card_bg (below the 4.5 small-text threshold once any tint is applied; the ~5.2:1 comment in theme.rs holds only against panel_fill). Impact: dark-mode warn/danger text on cards is marginal for low-vision users today. Validated replacement: #F0907A (5.51 on card, 4.88 on tinted chips). Fold into the token sweep. [Recovered from the mockup session's unpushed branch; the #F0907A replacement itself has since landed in theme.rs via the parity polish PR #87 - the token sweep codifies it rather than introduces it.]
<!-- SECTION:NOTES:END -->
