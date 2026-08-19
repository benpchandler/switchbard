---
id: TASK-19
title: 'Portfolio lens: per-repo health'
status: Done
assignee: []
created_date: '2026-08-05 03:55'
updated_date: '2026-08-05 05:15'
labels:
  - hub
  - beyond-parity
dependencies:
  - TASK-15
priority: medium
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Group-by-repo view: open/in-progress/done counts, oldest task age, last activity, blocked count per repo. Read-only aggregation.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
RepoStats (switchbard_core::backlog_stats, already built for TASK-16) extended with blocked/oldest_open_created_date/last_activity_updated_date, computed in the same per-repo pass as the existing totals — no parallel aggregation. New Portfolio lens (portfolio.rs) is presentation only: a read-only per-repo table (open/in-progress/done/blocked/oldest-open/last-activity).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Per-repo health table shipped as a fifth Backlog lens. Covered by a kittest test and by legibility_audit in both themes; core additions covered by a new backlog_stats unit test.
<!-- SECTION:FINAL_SUMMARY:END -->
