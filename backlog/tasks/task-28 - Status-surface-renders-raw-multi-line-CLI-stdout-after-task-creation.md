---
id: TASK-28
title: Status surface renders raw multi-line CLI stdout after task creation
status: Done
assignee: []
created_date: '2026-08-05 14:58'
updated_date: '2026-08-05 15:56'
labels:
  - bug
  - ux
dependencies: []
priority: high
ordinal: 28000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner-found live bug (2026-08-05): after creating a task, the action-status surface renders the RAW multi-line stdout of 'backlog task create' (full task render with ===== underlines, sections, etc.), stretching the top panel into a huge blank region across the top of the window. Screenshot evidence: creating MusicProduction task-431 produced a ~500px dark void top-right showing the whole CLI dump. Fix in two layers: (1) the create path should parse just what it needs from stdout and report a compact one-liner (e.g. 'Created MusicProduction:TASK-431'), same pattern as other mutation statuses; (2) defense in depth on the status surface itself: it should never render unbounded multi-line text -- clamp to a single line (truncate with ellipsis, full text on hover tooltip), so no future CLI's verbose stdout can blow up the layout again.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented (commit 3809f1a): parse_created_task_id extracts just the task id from backlog task create's verbose stdout; spawn_backlog_create reports a compact 'Created {repo}:{id}' message; new action_status_label component clamps every status surface to one line as defense in depth. Unaffected by TASK-29's click/drag defect (status labels are plain, non-interactive).
<!-- SECTION:NOTES:END -->
