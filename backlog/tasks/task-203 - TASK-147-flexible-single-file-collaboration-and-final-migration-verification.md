---
id: TASK-203
title: 'TASK-147: flexible single-file collaboration and final migration verification'
status: In Progress
assignee: []
created_date: '2026-09-08 19:06'
updated_date: '2026-09-08 21:07'
labels:
  - task-147
  - storage
  - sync
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: collaborators need a readable optional exchange file that preserves custom data and supports offline catch-up without losing edits. Evidence: second opinion reproduced unknown-base failures; owner requires flexible schema and verification of gradual migration.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Bootstrap/edit/export-back, missed exports, divergent edits, tombstones and unknown content round-trip pass against actual product commands.
- [ ] #2 Database backup/restore and representative real-data migration parity are verified; no routine file writes remain for migrated kinds.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Actual two-peer CLI exchange, explicit custom/collision resolutions, backup/restore, workspace ordering and source-drift tests passed. Independent review identified newer-content-version write protection and source deletion cases; repairs underway before live migration. Optional exchange remains explicit only.
<!-- SECTION:NOTES:END -->
