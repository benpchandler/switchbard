---
id: TASK-138
title: 'sbt: group the task list by project (and other categorical columns) with o'
status: Done
assignee: []
created_date: '2026-09-02 22:46'
updated_date: '2026-09-03 01:14'
labels:
  - tui
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
**Impact:** anyone opening sbt on a multi-project repo (CambridgeKitchens: nine lender projects under Getting Financing) sees a flat list and cannot tell tasks belong to projects. Owner reaction: "I don't understand how the tasks are structured; are there a bunch of tasks for each bank that I can't see?" Ignored, the hierarchy on disk stays invisible in the terminal.

**Evidence:** owner report on the CambridgeKitchens backlog after the per-lender restructure, 2026-09-02; `sb project list --repo ~/Dev/CambridgeKitchens` shows nine projects, `sbt` showed one flat list with only the project column as a trace.

**Decision (owner latitude granted):** key is `o` (outline), keeping `g`/`G` as top/bottom; `o` toggles flat vs the last group column (default project) rather than cycling; other columns via the header-digit menu (`2o`) and `:group <column>` / `:group off`. Collapse is not built.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 In a repo with more than one project, the flat footer says N projects · o groups by project; a single-project repo shows no hint
- [x] #2 o sections by project in stack-rank order; each heading shows name, def status, done/total; the initiative name is in the title bar; tasks without a project land in a final no project section
- [x] #3 Grouping is a projection over the filtered, sorted rows: sort holds inside sections, empty sections are omitted, an empty result shows no headings
- [x] #4 A sub-issue sits under its parent when both are in the section; a filtered-out parent is not resurrected
- [x] #5 Cursor movement (j k g G page) skips headings
- [x] #6 Saved views record group and reopen grouped; :group <column> and :group off work; the header-digit menu offers o group by it for groupable columns only
- [x] #7 groupable is one ColumnSpec field; tests cover section order, heading content, no-project bucket, sub-issue adjacency, empty result, filter composition; preflight passes; trajectory records the decision
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Shipped in PR feat/tui-group-by. o sections by project (rank order, def status, done/total, initiative in title, no-project bucket last); :group <column>/off and the header-digit menu cover status, priority, ball. Grouping is a projection over filtered+sorted rows in group.rs; headings are rows the cursor skips; sub-issues sit under their parent; saved views carry group. Keys: o (outline) so g/G stay top/bottom; o toggles flat vs last group column. Collapse not built. Evidence: CambridgeKitchens shows nine ranked sections; six E2E tests; preflight green.
<!-- SECTION:FINAL_SUMMARY:END -->
