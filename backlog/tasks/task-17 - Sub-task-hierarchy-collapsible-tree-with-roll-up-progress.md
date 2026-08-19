---
id: TASK-17
title: 'Sub-task hierarchy: collapsible tree with roll-up progress'
status: Done
assignee: []
created_date: '2026-08-05 03:55'
updated_date: '2026-08-05 05:15'
labels:
  - hub
  - beyond-parity
dependencies:
  - TASK-15
priority: high
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parent/child (decimal IDs) rendered as tree, cross-repo; parent rows show children done/total roll-up; create-subtask from parent detail. Mutations via backlog CLI (-p).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
List lens rows nest into a collapsible sub-task tree via BacklogTask::parent (already parsed by core, previously unused). tree.rs (split out of list.rs to avoid re-crossing the LOC ceiling) decides which rows nest and walks the expand/collapse recursion; list.rs kept ownership of one row's actual column rendering. Roll-up badge and expanded children always resolve against the full project (switchbard_core::children), not the filtered/sorted view. Create-subtask: NewBacklogTask gained parent: Option<String> (-p/--parent), wired from a new '+ Subtask' button in the detail pane's new Sub-tasks section.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Tree, roll-up badge, and create-subtask all shipped and covered by a kittest test (collapse/expand/pre-fill) plus a real parent/child pair in legibility_audit's fixture (both themes).
<!-- SECTION:FINAL_SUMMARY:END -->
