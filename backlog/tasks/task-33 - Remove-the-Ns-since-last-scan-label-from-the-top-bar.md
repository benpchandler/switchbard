---
id: TASK-33
title: Remove the 'Ns since last scan' label from the top bar
status: Done
assignee: []
created_date: '2026-08-05 17:23'
updated_date: '2026-08-05 17:24'
labels:
  - gui
  - ux
dependencies: []
priority: medium
ordinal: 33000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner UX pass: the ticking 'Ns since last scan' label next to the Refresh button was visual noise — staleness isn't itself actionable, the Refresh button next to it already is. Command decision: remove it; the Refresh affordance stays; staleness can surface later as a subtle indicator if ever needed.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Removed from top_bar.rs's render(): the label plus the now-unused last_scan destructure from scan_summary (its tuple shrank from 4 to 3 fields). ScanState::last_scan (app.rs) is left in place, still populated by the scanner worker (workers.rs) — just no longer read by the UI — since a future subtle staleness indicator may want it. No test referenced the removed text; mise run ci green, both themes pass legibility_audit.
<!-- SECTION:NOTES:END -->
