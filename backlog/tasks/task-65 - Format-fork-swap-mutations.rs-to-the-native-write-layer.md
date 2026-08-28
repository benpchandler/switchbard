---
id: TASK-65
title: 'Format fork: swap mutations.rs to the native write layer'
status: To Do
assignee: []
created_date: '2026-08-28 18:40'
labels:
  - format-fork
dependencies:
  - TASK-63
  - TASK-64
priority: high
ordinal: 64000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Re-implement the nine mutation functions in backlog/mutations.rs on top of backlog::write plus the ID allocator, with unchanged signatures, so GUI, dispatch, and refine callers do not change. Includes the file-move dispositions (archive -> backlog/archive/tasks/, complete -> backlog/completed/ with the Done-only rule mutations.rs already documents) and create filename convention. Behavior parity notes: label add/remove reads the file at write time (freshness semantics of --add-label); --ac semantics stay append-only. Update the module header docs that cite the CLI as authority (mutations, dispatch, refine). Delete parse_created_task_id stdout scraping - create returns the id directly. Replace tests/backlog_cli_mutations.rs real-CLI round trips with native-layer equivalents.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All nine mutation functions write natively with unchanged signatures; no caller outside backlog/ changes
- [ ] #2 Archive and complete file moves implemented with the Done-only rule preserved
- [ ] #3 parse_created_task_id and its pinned-output tests deleted; create returns the new task id directly
- [ ] #4 Module header docs in mutations, dispatch, and refine no longer name the CLI as the write authority
- [ ] #5 tests/backlog_cli_mutations.rs replaced by native-write-layer round trips; mise run ci green
<!-- AC:END -->
