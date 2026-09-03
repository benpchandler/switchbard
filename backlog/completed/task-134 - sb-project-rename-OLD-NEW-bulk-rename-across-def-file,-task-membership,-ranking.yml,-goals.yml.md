---
id: TASK-134
title: 'sb project rename <OLD> <NEW>: bulk rename across def file, task membership, ranking.yml, goals.yml'
status: Done
assignee: []
created_date: '2026-09-02 21:51'
updated_date: '2026-09-02 21:59'
labels:
  - cli
  - tasks
  - hierarchy
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: the trajectory doc lists project rename as 'deliberately deferred, ask before building'; the owner asked for it 2026-09-02 while restructuring the CambridgeKitchens financing backlog (a project needs to become an initiative's member under a new name). Without it the only path is recreate-and-reassign, which loses the def file and any rank/goal references. Evidence: docs/product-trajectory.md 'Linear-vocabulary hierarchy' entry; ranking.rs RepoRanking.projects/tasks keyed by name; goals.rs scope + inputs.projects keyed by name.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 switchbard_core::rename_project(root, old, new) renames the def file (name: + slug) when one exists, rewrites project:/milestone: on every member task in tasks/completed/drafts/archive, and rewrites the name in ranking.yml (projects list, tasks map key) and goals.yml (scope, inputs.projects)
- [x] #2 refuses when NEW is already defined or referenced, when OLD is neither defined nor referenced, and on a def-slug collision; validation runs before any write
- [x] #3 sb project rename OLD NEW prints the new name and a count of files touched
- [x] #4 trajectory doc records the decision; the switchbard skill doc drops rename from Known gaps
- [x] #5 unit tests cover def+tasks+ranking+goals, and each refusal
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
rename_project() in hierarchy.rs (def name+slug, member tasks in all dirs incl. legacy milestone:, ranking.yml via rename_project_in_ranking, goals.yml via rename_project_in_goals); refusals validated before any write; sb project rename OLD NEW prints Renamed OLD -> NEW (n tasks, def, ranking, goals). 3 tests. Trajectory doc + switchbard skill updated. Gates clean, 508 core tests.
<!-- SECTION:FINAL_SUMMARY:END -->
