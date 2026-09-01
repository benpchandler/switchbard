---
id: TASK-106
title: 'GUI: Tasks-place list title column and detail rail collide at narrow window widths, hiding task titles entirely'
status: To Do
assignee: []
created_date: '2026-09-01 06:07'
labels:
  - gui
  - ia
dependencies: []
priority: medium
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: at narrow window widths (~700px, a realistic laptop-half-screen or split-view size), the Tasks place's List row title column and the persistent detail rail (min width 320px, rail.rs) compete for space, and task titles can render fully invisible (zero/near-zero remaining width) while status/priority pills still show. A user in a narrow window cannot tell which task is which.

Evidence: crates/switchbard-gui/tests/qa_screenshots_tasks_place.rs's tasks_place_narrow_{light,dark}.png (docs/qa/screenshots/), set_size(700, 800), shows list rows with status/priority pills but no visible title text. Found during TASK-97's visual QA pass (2026-09-01). The sidebar itself correctly collapses to the icon rail at this width (nav.rs's existing NARROW_WIDTH_THRESHOLD/rail behavior, unaffected) — this is specifically the central list column vs. the detail rail.

Root cause: crates/switchbard-gui/src/ui/backlog/list.rs's task_col_width computes `ui.available_width() - TRAILING_COLS_WIDTH (- REPO_COL_WIDTH)`, which goes to near-zero once the detail rail's fixed minimum width (rail.rs MIN_WIDTH = 320.0) and the sidebar rail eat most of a narrow window's space — same underlying column-width function the legacy List lens already had, so this likely predates TASK-97 and simply wasn't screenshotted at this width before.

Options needing a decision: collapse/hide the detail rail below some width threshold (mirrors the sidebar's own narrow-width behavior), let trailing columns (Repo/Status/Priority/AC) drop first before the title column shrinks further (mock §7d's own stated rule: "the facet bar wraps and the Delivery column drops first" — an analogous column-dropping strategy may already be the intended answer), or set a title-column minimum width that the rail negotiates around.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Task titles remain visibly readable (non-zero, non-truncated-to-nothing width) in the Tasks place List body down to a defined minimum supported window width
- [ ] #2 The chosen fix (rail collapse, column dropping, or minimum-width negotiation) is documented as the one rule call sites follow
- [ ] #3 qa_screenshots_tasks_place.rs's narrow-width fixture (700px) shows visible task titles in both themes
<!-- AC:END -->
