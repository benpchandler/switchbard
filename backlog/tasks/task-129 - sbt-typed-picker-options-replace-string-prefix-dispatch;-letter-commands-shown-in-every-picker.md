---
id: TASK-129
title: 'sbt: typed picker options replace string-prefix dispatch; letter commands shown in every picker'
status: In Review
assignee: []
created_date: '2026-09-02 19:26'
updated_date: '2026-09-02 19:32'
labels:
  - tui
  - refactor
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: sbt users cannot discover letter chords (pr, pf, pc, po, pd, cm, cg) without reading the footer, and the code dispatches on option label strings ("delete all", "order rules", "row ", "column (whole)"), so renaming a label silently breaks a chord (it already did once: 'clear all' collided with 'c'). Evidence: crates/switchbard-tui/src/app.rs apply_picked_value PaintTarget/Columns arms; commit 51fc405 renamed clear-all to dodge the collision; owner request 2026-09-02 'show all commands incl. letters in the picker for all menus'.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Every picker row renders its key (number or letter) and every letter command (r f c o d m g h) is visible inside the picker box, not only the footer
- [x] #2 apply_picked_value dispatches on a typed PickOption payload; no starts_with / string parsing of option labels remains in app.rs
- [x] #3 cargo test -p switchbard-tui green with no test asserting on option label prefixes
<!-- AC:END -->
