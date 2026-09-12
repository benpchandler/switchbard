---
id: TASK-201
title: 'TASK-147: central document store and initiative/project strangler cutover'
status: In Progress
assignee: []
created_date: '2026-09-08 19:06'
updated_date: '2026-09-09 02:39'
labels:
  - task-147
  - storage
  - migration
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: Switchbard users need routine definition updates shared across worktrees without PRs. Implement flexible central SQLite authority per repository/kind and migrate lower-impact initiative/project definitions first. Evidence: TASK-147 owner authorization and second-opinion/schema-flexibility contract in codex/task-147-storage-contract; hierarchy.rs currently reads/writes definition Markdown directly.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One database stores full custom content with transactional updates and per-kind authority; migrated kinds never fall back to files on error.
- [ ] #2 Temporary-store parity, unchanged original files, concurrent writes and linked-worktree visibility pass before real cutover.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Flexible central store, per-kind native adapters, concurrency, recovery and source-preservation proofs are implemented in codex/task-147-storage-contract. Six real repositories rehearsed in a temporary shared database with identical task lists and unchanged originals. Final independent findings are being closed before default-database activation; not yet live.

LIVE hierarchy cutover verified on source build 6c484a49: seven repositories, 14 initiative/project authority switches, 68 central records. CambridgeKitchens12, switchbard11, budget13, MusicProduction29, matterline0, visual-review1, hub2. All ordinary task/project/initiative/goal outputs identical before and after each phase, all source SHA256 values unchanged. Default database ~/.switchbard/switchbard.sqlite3 mode0600, SQLite quick_check ok. Native migration created verified pre-phase database/source recovery snapshots. Evidence ~/.switchbard/migration-reviews/task147-20260909-live/*-live.json. Installed sb/sbt/app upgraded with old versions backed up; no task authority cutover yet. Full preflight still has 32 known legacy write test failures; concrete repair underway separately under owner-directed rollout policy.
<!-- SECTION:NOTES:END -->
