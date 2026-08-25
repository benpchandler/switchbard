---
id: TASK-48
title: 'Board visual design polish: surface-value hierarchy, shared stroke/shadow'
status: In Progress
assignee: []
created_date: '2026-08-25 00:19'
labels: []
dependencies: []
priority: medium
ordinal: 48000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Surface hierarchy was coming from egui's stock gray strokes rather than from the surfaces themselves, which read as generic and fought the Flight Strips / Operator's Console language theme.rs already describes. Retunes both palettes so value carries the hierarchy: the light board moves to a cooler workspace neutral against a paper-white strip, and the dark chassis moves from warm brown-black to blue-black with a raised instrument-panel card. Adds theme::surface_stroke() (hairline at 24% weak-text alpha) and theme::card_shadow() (restrained, 2px offset / 6 blur) as the two sanctioned ways to separate adjacent surfaces so component code stops reaching for stock visuals; corner radii and window/popup shadows move to the same central definition. Recorded after the fact: the work landed as a local commit that was never PR'd.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both palettes retuned; hierarchy reads from surface value
- [ ] #2 theme::surface_stroke() and theme::card_shadow() are the sanctioned separators
- [ ] #3 Corner radii and window/popup shadows centralized in theme.rs
- [ ] #4 WCAG-AA legibility contract still passes on both themes
<!-- AC:END -->
