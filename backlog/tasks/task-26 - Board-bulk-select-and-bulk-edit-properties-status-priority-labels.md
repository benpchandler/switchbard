---
id: TASK-26
title: 'Board: bulk select and bulk-edit properties (status/priority/labels)'
status: To Do
assignee: []
created_date: '2026-08-05 14:02'
labels:
  - board
  - ux
dependencies: []
priority: medium
ordinal: 26000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner-requested UX (2026-08-05): the List lens already has multi-select (plain/shift-range click on bulk checkboxes) and a bulk-edit menu (Pending::bulk_save, one backlog CLI call per project) that reuses BacklogTaskPatch. Board lens has neither. Add the same bulk-select pattern to Board cards and reuse the existing bulk-edit machinery (not a parallel implementation) so status/priority/labels can be changed across a multi-card selection.
<!-- SECTION:DESCRIPTION:END -->
