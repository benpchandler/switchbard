---
id: TASK-161
title: 'sbt: treat PR lifecycle as a filter over the repository list'
status: In Review
assignee: []
created_date: '2026-09-07 00:21'
updated_date: '2026-09-07 00:30'
labels:
  - tui
  - github
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: owner cannot organize delivery consistently when the page excludes closed and merged PRs at the data source. Evidence: owner correction after installed TASK-160: active PRs must be a filter over all PRs. Fetch all lifecycle states with explicit bounded history loading, use existing search/filter grammar and picker behavior, preserve separate task/PR filters.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 All-state source includes open closed and merged PRs; status is a cached-list filter, not a GitHub source restriction.
- [x] #2 Shared search and filter picker work without changing task filters; filters survive page toggle and self-restart; partial coverage remains explicit.
- [x] #3 Live history and load-more journeys plus crate gates pass; checked build installed for owner review.
- [x] #4 Enter opens a right-hand PR detail pane while the PR list remains visible, matching task-page layout; long detail scrolls.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner steering while in progress: Enter must use a right-hand pane like Tasks, rather than a full-width detail page.

All lifecycle metadata, shared filters and right detail pane implemented. Three real GitHub E2Es passed including expansion to 200, stale retention and narrow detail scrolling. Final gate and install pending.

Installed code revision d1326d2 via mise run tui-install; formatting, Clippy and ordinary TUI E2Es passed. Three serialized live history/link/scroll journeys passed, plus real j/k navigation with the detail pane retained. Native owner review pending. Local feat/tui-pages branch; no push or merge.
<!-- SECTION:NOTES:END -->
