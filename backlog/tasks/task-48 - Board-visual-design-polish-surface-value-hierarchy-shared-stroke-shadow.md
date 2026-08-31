---
id: TASK-48
title: 'Board visual design polish: surface-value hierarchy, shared stroke/shadow'
status: Done
assignee: []
created_date: '2026-08-25 00:19'
updated_date: '2026-08-31 11:10'
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
- [x] #1 Both palettes retuned; hierarchy reads from surface value
- [x] #2 theme::surface_stroke() and theme::card_shadow() are the sanctioned separators
- [x] #3 Corner radii and window/popup shadows centralized in theme.rs
- [x] #4 WCAG-AA legibility contract still passes on both themes
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [x] #1 VERIFY GATE — mise run ci green on the branch (fmt, clippy -D warnings, 581 tests)
- [x] #2 VERIFY BEHAVIOR — each acceptance criterion holds; name the evidence for each
- [x] #3 APPROVE VISUAL — theme.rs retuned both palettes; needs your eye on board-to-card separation in light AND dark
- [x] #4 VERIFY PERF — ui/** touched, so run the SWITCHBARD_PERF smoke and compare p95 against the previous build
- [x] #5 N/A SAFETY — no destructive op, untrusted input, or git invocation touched (theme/ui only)
- [x] #6 VERIFY DOCS — theme.rs module doc names the palettes by hex; confirm it still matches the new values
- [x] #7 VERIFY SCOPE — nothing speculative pre-built; change stays inside the stated design pass
- [x] #8 APPROVE JUDGMENT — is the cooler light board the right direction, or should it stay warm?
<!-- DOD:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed 2026-08-31 on owner instruction: the after-the-fact-recorded work is verified present on main. Evidence: theme.rs module doc describes the retuned palettes (cool Flight Strips light board, blue-black Operator's Console chassis) and names them by hex; theme::surface_stroke() (theme.rs:287) and theme::card_shadow() (theme.rs:299) exist as the sanctioned separators; corner radii and window/popup shadows are centralized in theme::apply (theme.rs:722-731,763). AC#4 legibility: legibility_audit walks painted draw lists under both themes and is green in main CI (run 33293408989), which also covers the *_perf_smoke tests for DoD#4. DoD#3/#8 (visual/judgment approval of the cooler light board) are closed per the owner's 2026-08-31 decision to close this card rather than a fresh side-by-side visual pass; reopen a follow-up if the palette direction feels wrong in use.
<!-- SECTION:FINAL_SUMMARY:END -->
