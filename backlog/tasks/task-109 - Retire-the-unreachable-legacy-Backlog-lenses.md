---
id: TASK-109
title: Retire the unreachable legacy Backlog lenses
status: To Do
assignee: []
created_date: '2026-09-01 08:19'
labels:
  - gui
  - cleanup
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Legacy lens bodies (ui/backlog/stats.rs Statistics+burndown, portfolio.rs, projects.rs, milestones grouping, lens toolbar, BacklogLens Statistics/Portfolio/Projects variants) compile but are unreachable from the Place routing (app.rs ~2560-2580's own comment confirms this); qa_screenshots.rs still regenerates screenshots of dead UI.

Impact: maintainers keep paying test and compile cost for surfaces users cannot reach; ongoing drift risk as the dead code silently diverges from the live Tasks place.

Evidence: app.rs ~2560-2580 (routing comment confirming unreachability); ui/backlog/{stats.rs,portfolio.rs,projects.rs}; BacklogLens enum's Statistics/Portfolio/Projects variants; qa_screenshots.rs still generating screenshots of these lenses; TASK-97's Final Summary (removed tests for the genuinely-cut Portfolio/Statistics lenses with no replacement).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dead lens bodies deleted (stats.rs Statistics+burndown, portfolio.rs, projects.rs, milestones grouping, lens toolbar)
- [ ] #2 BacklogLens Statistics/Portfolio/Projects variants pruned
- [ ] #3 qa_screenshots.rs legacy-lens screenshot coverage removed
- [ ] #4 mise run ci green
<!-- AC:END -->
