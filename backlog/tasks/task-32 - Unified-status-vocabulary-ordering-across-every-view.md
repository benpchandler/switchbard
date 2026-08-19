---
id: TASK-32
title: Unified status vocabulary + ordering across every view
status: Done
assignee: []
created_date: '2026-08-05 17:18'
updated_date: '2026-08-05 17:18'
labels:
  - backlog
  - ux
dependencies: []
priority: high
ordinal: 32000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner: 'all projects should share a common set of statuses across every view.' Board columns (TASK-25's local union), List's status filter dropdown (was hardcoded to the 3-entry BACKLOG_STATUSES const, missing a project's declared-but-currently-empty statuses like Icebox), the detail-pane status editor, the Create modal's status picker, and Statistics' status distribution all independently derived or hardcoded their own status lists, so they could silently disagree on what statuses existed for the same project.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
New single source of truth: switchbard_core::ordered_status_vocabulary (backlog/types.rs) unions BACKLOG_STATUSES + every given project's configured_statuses + any status actually carried by a task in scope, ordered per a new 6-entry CANONICAL_STATUS_ORDER (Backlog, Icebox, To Do, In Progress, In Review, Done, then extras alphabetically) — Backlog is a reserved sort-order slot, not an always-shown status, matching how Icebox already behaved. Every consumer now calls this one function instead of deriving its own list: board.rs's column_order (replaces TASK-25's local copy), toolbar.rs's status filter dropdown (previously used sort::status_options, which unlike Board's union never consulted configured_statuses — the exact List/Board mismatch the owner caught), detail.rs's status editor (scoped to the task's own project), create.rs's Create modal status picker (scoped to the target project), and stats.rs's Status distribution (reordered canonically instead of BTreeMap's alphabetical order). sort.rs's status_rank (List's Status column sort) now uses the same CANONICAL_STATUS_ORDER instead of the old 3-entry BACKLOG_STATUSES-based rank. format::render_value_combo's options parameter changed from &[&str] to &[String] to accept a freshly computed, project-scoped vocabulary. Removed sort::status_options (dead code, superseded). Proven via 5 unit tests on ordered_status_vocabulary itself; the ComboBox-fed call sites are the established UNDRIVABLE-BY-KITTEST pattern already documented for every other status/priority combo in this codebase, confirmed (not assumed) by an actual click probe before falling back to code review — see backlog_controls.rs's note above board_shows_the_icebox_column_even_with_zero_icebox_tasks.
<!-- SECTION:NOTES:END -->
