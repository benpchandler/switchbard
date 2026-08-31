---
id: TASK-12
title: Headless switchbard-dispatch binary under launchd
status: Done
assignee: []
created_date: '2026-08-05 02:30'
updated_date: '2026-08-31 11:10'
labels:
  - hub
  - slice-3
dependencies: []
priority: medium
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Slice 3 (trajectory doc). Thin bin target reusing switchbard-core dispatch machinery; launchd plist drains the dispatch queue with the GUI closed. Owner-scoped exception to the no-daemon stance recorded in product-trajectory.md (2026-08-04).
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed as already shipped. crates/switchbard-dispatch exists on main as the thin headless binary over switchbard-core's dispatch machinery (CLAUDE.md architecture section documents it). The launchd wrapper remains a user-side install step, not repo work. Card predates the shipped dispatch arc and was never updated.
<!-- SECTION:FINAL_SUMMARY:END -->
