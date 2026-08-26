---
id: TASK-49
title: 'Backlog toolbar tidy: one container, one count, Enter-to-save'
status: Done
assignee: []
created_date: '2026-08-26 00:39'
labels: []
dependencies: []
priority: medium
ordinal: 49000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Four rows of chrome in three container treatments sat above the board (bordered tabs, unframed saved-views row, second bordered filter panel). Consolidated to one container. Header said 'All projects · 374 open · 1509 total' while the filter panel said '370 visible' - two numbers, four values, an unexplained gap. Now one count that explains itself: '370 of 1509 · 374 open' when filtered. visible_count is an Option so a lens without the filter row cannot claim N of M; lens_filters is the single definition. Save commits on Enter, retiring a get_all_by_label(Save).nth(2) disambiguation in ui_views. Reviewed as wireframes in visual-review before building (Option A). Landed in PR #33.
<!-- SECTION:DESCRIPTION:END -->
