---
id: TASK-14
title: 'Design system refresh: apply chosen direction to theme.rs + components'
status: Done
assignee: []
created_date: '2026-08-05 03:03'
updated_date: '2026-08-05 04:19'
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

Direction B (Flight Strips) implemented as the light theme, direction A's lamp language as the dark theme (Operator's Console), per the owner decision recorded here. theme.rs restructured: former pub const colors are now theme::xxx() accessor functions resolving against a thread-local ThemeChoice (persisted on Config::ui.theme), mechanically renamed at every call site in crates/switchbard-gui. Every chromatic constant retuned per-ground for BOTH themes (not just carried over) — the board panel (#DFE3E6, L~0.76) is darker than the stock light panel the old palette targeted, and dark-theme text needs bright-on-near-black rather than dark-on-light tuning. Split danger() (text-color role, theme-aware) from a new theme-independent danger_fill() used only by danger_button(), since the light theme's single danger red can't serve both a legible-on-dark-chassis text role and a white-text-safe button-fill role simultaneously. Embedded Barlow Semi Condensed (labels) + JetBrains Mono (ids/numerics) OFL fonts fetched from google/fonts with their OFL.txt licenses, installed once via theme::install_fonts (font-atlas rebuild, expensive) separate from the per-frame theme::apply (Visuals swap, cheap — needed every frame so the live toggle takes effect immediately). Component pass: Board lens flight-strip cards (repo rail hues, selection as a border-color change rather than a translucent fill after the stock selection.bg_fill overlay was found to fail WCAG AA on the dark theme), Statistics lens instrument-gauge header. legibility_audit.rs extended to render every Backlog lens under both themes (it previously only covered Servers/Agent Context) and to exempt genuinely-disabled controls per WCAG 1.4.3 (detected via the real accessibility tree, not guessed) rather than chasing egui's architecturally-fixed 50%-fade-toward-panel disable styling; separately fixed TextEdit hint-text contrast, which is NOT WCAG-exempt, by retuning the fade target itself.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Both palettes pass legibility_audit (WCAG AA, both themes, all four Backlog lenses plus the pre-existing Servers/Agent Context views). Theme toggle lives in the top bar and persists like the existing zoom control. mise run ci green. Sequenced together with TASK-15 on one branch per this task's own note (build the new surfaces once, in the final visual language).
<!-- SECTION:FINAL_SUMMARY:END -->
