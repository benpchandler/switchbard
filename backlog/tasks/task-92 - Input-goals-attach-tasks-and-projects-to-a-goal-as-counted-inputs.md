---
id: TASK-92
title: 'Input goals: attach tasks and projects to a goal as counted inputs'
status: Done
assignee:
  - '@claude'
created_date: '2026-09-01 02:09'
updated_date: '2026-09-01 02:19'
labels:
  - goals
  - backlog
  - cli
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner requirement (2026-09-01, IA V2 review session): goals must accept ATTACHED tasks and projects as 'input goals'. Impact: without this, tasks-measured goals can only count by a single project/label scope; a cross-cutting goal ('land these three specific tasks plus everything in project X') cannot be expressed, so weekly goals under-represent real intent. Evidence: owner instruction in session + the IA V2 mock's goal-page Inputs card (frozen artifact for TASK-77).

Design: GoalDef gains inputs { tasks: [ids], projects: [names] } in goals.yml (records-not-documents; line-surgical writes like check-ins). A tasks-measured goal's actual counts a task once if it matches scope OR is an attached task OR belongs to an attached project (done + updated in the goal week). Attach validates task ids against the backlog and canonicalizes them; manual goals refuse inputs (they take check-ins). CLI: goal attach/detach with repeatable --task/--project; goal view lists inputs; goal create --measure tasks no longer hard-requires --scope (stderr note points at goal attach). GUI affordance ships with IA V2 implementation, not here.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 GoalDef carries inputs (task ids + project names); goals.yml round-trips them and absent inputs parse as empty
- [x] #2 goal attach / goal detach are line-surgical (only the inputs lines change), dedupe, canonicalize task ids against the backlog, refuse manual goals, and error usefully on unknown tasks or detaching something not attached
- [x] #3 compute_goal_statuses counts a task once when it matches scope OR attached task OR attached project, done-in-week; covered by a core test with all three paths plus overlap
- [x] #4 goal view prints inputs; goal create --measure tasks without --scope succeeds with a stderr note naming goal attach; CLI round-trip test covers attach -> actual -> detach
- [x] #5 mise run ci green on both-platform-equivalent local run
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Input goals shipped in core + CLI. GoalDef gains inputs { tasks, projects } parsed from goals.yml (absent = empty; flow lists always single-quoted so commas cannot split items). attach_goal_inputs / detach_goal_inputs are line-surgical (only the inputs block changes; verified by a byte-diff test), dedupe case-insensitively on task ids, refuse manual goals, drop the block when it empties, and fail closed on hand-restyled files like every other goals write. compute_goal_statuses counts a task once when it matches scope OR is an attached task OR belongs to an attached project (single-pass union, overlap test included). CLI: goal attach/detach (--task repeatable, canonicalized via find_task so '7' stores 'TASK-7' and typos fail loudly; --in-project repeatable - named to dodge the global --project repo alias, which clap caught), goal view prints Input tasks/projects, scopeless --measure tasks create now succeeds with a stderr note pointing at goal attach. Evidence: mise run ci exit 0, 42 suites green incl. 3 new core tests + 1 new CLI round-trip test; GUI untouched except test literals (IA V2 will build the Inputs card per the frozen mock).
<!-- SECTION:FINAL_SUMMARY:END -->
