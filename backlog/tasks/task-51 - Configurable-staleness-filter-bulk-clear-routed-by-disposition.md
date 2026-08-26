---
id: TASK-51
title: Configurable staleness filter + bulk clear routed by disposition
status: Done
assignee: []
created_date: '2026-08-26 00:39'
labels: []
dependencies: []
priority: high
ordinal: 51000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
346 tasks in To Do, many obsolete, no way to find or clear them - Clean Up Old Tasks only completes Done tasks. Staleness is 'untouched for N days' from the same date the card shows, threshold persisted on Config::ui.stale_after_days (default 90), filter session-only. An unparseable date is NOT stale: this gates an archive. Bulk clear routes Done->complete and the rest->archive in one worker, and the control names the verb it will actually perform (Archive/Complete/Clear). Column headers gain a select-all toggle. Guards: lens gate, narrowed-view gate lifted by explicit selection, column-local selection, mixed-batch count, verb rule - each confirmed to fail under sabotage. Landed in PR #33.
<!-- SECTION:DESCRIPTION:END -->
