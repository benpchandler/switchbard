---
id: TASK-96
title: 'IA V2: sidebar shell - places nav, multi-select repo scope, favorites'
status: To Do
assignee: []
created_date: '2026-09-01 02:24'
labels:
  - ia
  - gui
dependencies: []
priority: high
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
First slice of the places-and-objects IA (trajectory: Information architecture V2). Replace the lens tab row with the sidebar: places Digest / Tasks / Command / Goals / Ops, a multi-select repo scope switcher that every place aggregates over, a FAVORITES group (explicitly favorited objects with type glyphs, no auto-population), the ambient dispatch lamp footer, and the collapsed icon rail at narrow width. Places route to the existing surfaces during transition. Includes the UiConfig.filters re-key from lens names to place/view names (unmatched old keys dropped).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Sidebar renders places + multi-select scope; every surface aggregates over the checked repo set
- [ ] #2 FAVORITES holds only explicitly favorited objects; favoriting is an action on the object, glyphs by type, no pins
- [ ] #3 Filters persist under place/view keys; stale lens keys dropped; perf smoke on render paths
<!-- AC:END -->
