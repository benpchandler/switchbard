---
name: EGUI Polish
status: Planned
---

Owner diagnosis (2026-08-31): visual layering to differentiate elements is missing. theme.rs already carries a five-surface ramp (faint_bg wells, panel_fill, nav_bg/rail_bg, card_bg) but nothing communicates elevation: no shadows, uniform hairline strokes, collapsing headers at the same level as contents, whitespace-only separation. Direction: codify an explicit elevation scale in theme.rs (level 0 well / 1 panel / 2 card / 3 overlay, each with fill + stroke + shadow), then sweep surfaces to it consistently, composing with the WCAG legibility contract. Solvable within egui - a design-token problem, not a framework problem. Working name - expect renames.
