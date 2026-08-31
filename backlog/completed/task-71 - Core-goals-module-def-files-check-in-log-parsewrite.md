---
id: TASK-71
title: 'Core goals module: backlog/goals.yml parse + surgical check-in writes'
status: Done
assignee: []
created_date: '2026-08-31 17:02'
updated_date: '2026-08-31 17:49'
labels:
  - goals
  - core
dependencies:
  - TASK-70
priority: high
project: Weekly goals
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
backlog/goals/<slug>.md over the shared frontmatter engine (pattern: backlog/hierarchy.rs): GoalDef {name, week, target, unit, measure (manual|tasks), scope}, check-in entries parsed from a '## Check-ins' body list ('- YYYY-MM-DD: N'), append-only check_in write op, loader riding load_backlog_repo so goals reach every snapshot.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Def round-trip + append-only check-in tests
- [x] #2 Byte-surgical writes; write_layer_real_files stays green
- [x] #3 Goals load into BacklogRepo without extra IO
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Superseding the description's markdown-def design (owner decision): one backlog/goals.yml per repo (goals -> weeks -> {target, checkins:[{date,value}]}). Parse via serde_yaml; writes are line-surgical YAML edits through the shared write layer, precedent status_config.rs; check-in append inserts a line, byte-no-op guarantee holds. No backlog/goals/ directory.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
backlog/goals.rs: GoalDef/GoalWeek/GoalCheckIn (integer values keep the snapshot Eq), tolerant serde_yaml read riding load_backlog_repo (missing file silent, malformed warns + loads empty, unknown measure skips with warning), line-surgical create/check-in/roll writes over the module's own emitted shape with fail-closed location matching, byte-no-op roll, atomic replace incl. first-write path. Six unit tests incl. a strict only-one-line-changed surgical assertion. CI green.
<!-- SECTION:FINAL_SUMMARY:END -->
