---
id: TASK-152
title: 'sbt: paint_auto stores a palette token, not a resolved hex'
status: To Do
assignee: []
created_date: '2026-09-04 12:30'
updated_date: '2026-09-04 12:32'
labels:
  - tui
  - paint
  - ux
dependencies: []
priority: high
project: Views That Keep Themselves
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: anyone who changes palette or theme after painting a column. paint_auto resolves the palette to hex at write time, so a saved view keeps colors from whatever palette was live when it ran, and no later palette change reaches it. The owner installed the darkroom theme and could not tell why the screen still looked wrong: every row was still painted #f0883e from bloomberg, and the theme's own text color #C3B7A6 was never drawn at all. The stale value is also the loudest thing on a lights-out screen (#ffcc00 measures 13.2:1 against the darkroom background, vs 10.1:1 for body text).

Evidence: ~/.switchbard/views/Users_bpc_Dev_switchbard.lua slot 1 held 'by:status=done:#ffcc00,todo:#f0883e,inprogress:#2ea043,icebox:#f85149'. A live capture of sbt with that view emitted #F0883E and never emitted #C3B7A6. Telemetry (~/.switchbard/tui-events.jsonl, 2026-09-02..04): of 28 paint color decisions, 16 came from paint_auto and 11 were named colors; the owner typed hex zero times. So every hex in that file was machine-authored on the path where the user expressed the LEAST specific intent. Discovered while installing the darkroom theme, 2026-09-04.

Corroborating smell: paint::recolor_from_palettes exists only to reverse-engineer which palette index a hex came from by searching every known palette for a match. That reverse lookup is needed solely because the token was discarded at write time, and it fails silently when a palette definition is edited.

Decision left open: token spelling. Either a palette-slot reference (e.g. done:p1) which keeps a value's color stable as the palette changes, or a whole-rule 'auto' marker (by:status=auto) resolved at render, which is simpler but reshuffles every color when a new value appears. Recommend the slot reference. Hex typed by a user must keep working verbatim - this is about what paint_auto writes, not about banning hex.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 paint_auto writes a palette-slot token per value, not a resolved hex
- [ ] #2 A hex the user types or picks is stored verbatim and is never rewritten by a palette change
- [ ] #3 Changing palette (:palette or palette= in tui.lua) recolors token-backed rules with no reverse lookup over known palettes
- [ ] #4 Loading a views.lua that still holds resolved hex keeps working; existing files need no migration step from the user
- [ ] #5 recolor_from_palettes is deleted or reduced to the legacy-hex path only
<!-- AC:END -->
