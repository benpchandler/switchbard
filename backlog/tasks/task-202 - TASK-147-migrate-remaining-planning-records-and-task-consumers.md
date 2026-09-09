---
id: TASK-202
title: 'TASK-147: migrate remaining planning records and task consumers'
status: In Progress
assignee: []
created_date: '2026-09-08 19:06'
updated_date: '2026-09-09 02:44'
labels:
  - task-147
  - storage
  - migration
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: partial storage migration leaves ordinary task and planning changes requiring PR synchronization. Progress after proven low-impact cutover through config, ranking, goals, all task lifecycles and every GUI/TUI/CLI/dispatch/refine consumer. Evidence: user asks gradual migration through everything with same-or-intentional-change verification; TASK-147 contract identifies existing consumers.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each migrated kind passes before/after content and command parity with unknown fields preserved and originals recoverable.
- [ ] #2 All native task-domain reads/writes use shared central authority across linked worktrees without routine Git changes.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
All native kinds and CLI/GUI/TUI/dispatch/refine routes implemented. GUI 175 unit and 89 control tests, full TUI tests and 500-task performance checks passed. Real source inventory distinguishes350 stale ancestor copies from39 review candidates; actual activation awaits integrated validation and source reconciliation.

2026-09-08: Managed validation 01M21WK0GYEQR173C20RWFM857 passed review/tests/documentation/full isolated-target preflight at d763eb87; no publishing or live activation. Release rehearsal across seven repositories matched task/project/initiative/goal views with all originals unchanged and no verification failures; six conflicting kind migrations remain held. Additional public Refine journey exposed stale snapshot overwrite; revision-checked fix and transition regression are being completed before rollout. Private evidence: ~/.switchbard/migration-reviews/task147-20260908/.

Follow-up committed 3bbda583: stale central Refine and legacy-capture/cutover overwrite fixed;35 Refine tests/core Clippy/public journeys pass. Managed run01M21XJB3MRP2WB3CJ3V8C9AKX now review awaiting user finding storage-migration-no-writer-quiescence, so branch remains pipeline-owned. Verified app bundle prepared; no installation or live activation. Durable private handoff: ~/.switchbard/migration-reviews/task147-20260908/current-handoff.md.

Owner approved shared per-repository cross-process writer enforcement, plus brief external-editor pause. Resumed managed run01M21XJB3MRP2WB3CJ3V8C9AKX. First lock patch56ed8b66 failed independent audit (source-file Git path, nested aggregate locks, task/hierarchy bypass, absent-kind scope, crash recovery); correction requested through pipeline. This is implementation repair under existing authorization, not a new owner decision. No live activation.

LIVE remaining-planning slice: goals/config/ranking migrated where unambiguous; combined hierarchy plus planning is33verified phases76records across7repositories. Matterline config and Switchbard ranking explicitly held. after-planning.sqlite3 is a verified whole-DB backup. Task cutover preparation refreshed Cambridge inventory44records; originals will remain retained. Concrete legacy public standalone writer regression fixed at8b5d4bb with35focused tests passing. Refine/dispatch custom-context repair underway before central task activation. Owner direction explicitly removes speculative concurrency proof as a rollout prerequisite.
<!-- SECTION:NOTES:END -->
