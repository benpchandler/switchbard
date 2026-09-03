---
id: TASK-131
title: 'sbt: split app.rs by concept (picker, paint flow, view slots) and drive.rs by feature'
status: In Review
assignee: []
created_date: '2026-09-02 19:26'
updated_date: '2026-09-02 21:36'
labels:
  - tui
  - refactor
  - ball:me
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: app.rs is 1389 lines and drive.rs 1177; handle_pick_value_key alone is a 200-line match whose arm order is load-bearing (a digit arm placed after a generic one broke cm reorder during development). Findability and traceability are the cost the standards name. Evidence: wc -l crates/switchbard-tui/src/app.rs tests/drive.rs on feat/tui-2 at 104db3e.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 app.rs under 500 lines; picker key handling, paint flow, and view-slot chords each in their own module named for the concept
- [x] #2 tests/drive.rs split into per-feature files sharing one harness module; every test still end-to-end
- [x] #3 CLAUDE.md module map updated, still under 50 lines
<!-- AC:END -->
