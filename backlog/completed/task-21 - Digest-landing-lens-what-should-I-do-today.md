---
id: TASK-21
title: 'Digest landing lens: what should I do today'
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
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Landing view: overdue, newly unblocked, in-progress, recently done, per repo; entry point to triage. Becomes the app's default view.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Digest lens (crates/switchbard-gui/src/ui/backlog/digest.rs): Overdue/Newly-unblocked/In-progress/Recently-done, each capped at 6 cross-repo flight-strip cards. Overdue wires to TriageDue::Overdue (always empty today, no due-date field yet, kept forward-compatible). Newly unblocked uses switchbard_core::is_newly_unblocked (task-18's dependency graph + a recency window on the cleared dependency's updated_date, since there's no persisted blocked-state history to diff against). BacklogLens::default() changed from List to Digest — this only changes the Backlog TAB's own default lens, not the app's default ViewTab (still Servers). Each section's 'View all' jumps to the List lens; clicking a task jumps to List with it selected.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Landing screen shipped as the Backlog tab's default lens. Covered by a kittest test (default lens assertion, in-progress task rendering, View all entry point) and by legibility_audit in both themes.
<!-- SECTION:FINAL_SUMMARY:END -->
