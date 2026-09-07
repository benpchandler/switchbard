---
id: TASK-159
title: 'sbt: add Tasks and Pull Requests page navigation'
status: In Review
assignee: []
created_date: '2026-09-06 23:50'
updated_date: '2026-09-06 23:55'
labels:
  - tui
  - ux
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: the owner needs a distinct PR workspace and clear page location before delivery data is introduced. Evidence: owner-directed first slice in the PR entry-point review; current app Pane has only task detail and help. Scope: page shell, configurable toggle, current-page indicator, task-state preservation and honest unconnected state.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Tab toggles Tasks and Pull Requests with an always-visible active-page indicator and configurable binding.
- [x] #2 Returning to Tasks preserves filter and selection; PR page cannot mutate hidden tasks.
- [x] #3 Empty, narrow, help and self-restart journeys pass rendered TUI tests; install the checked slice for owner review.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented on feat/tui-pages in /Users/bpc/Dev/.worktrees/switchbard-tui-pages. Initial worktree status was clean. Primary checkout baseline has unrelated ranking.yml and TASK-150 edits, untouched. Owner review will use installed sbt; GitHub data remains unconnected.

Validation: reproduced missing Tab navigation in the new E2E journey before implementation; mise run tui and mise run tui-install passed formatting, warning-free Clippy and all TUI E2E tests. Installed /Users/bpc/.cargo/bin/sbt from code commit 77be484. Page navigation is ready for owner review; no GitHub fetch, PR data, push or merge performed. Existing paint-menu test now finds its heading instead of assuming an absolute screen row. Continue on feat/tui-pages for owner feedback; product direction and state matrix are in docs/product-trajectory.md.
<!-- SECTION:NOTES:END -->
