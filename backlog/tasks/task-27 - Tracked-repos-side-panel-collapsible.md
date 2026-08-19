---
id: TASK-27
title: 'Tracked repos side panel: collapsible'
status: Done
assignee: []
created_date: '2026-08-05 14:02'
updated_date: '2026-08-05 15:56'
labels:
  - ux
  - sidebar
dependencies: []
priority: medium
ordinal: 27000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner-requested UX (2026-08-05): the sidebar listing tracked repos/worktrees should be collapsible to reclaim horizontal space. Persist the collapsed state on Config::ui (additive field, default false/expanded so existing configs are unaffected).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented (commit f12e5d2): UiConfig::sidebar_collapsed persisted additively; sidebar.rs branches to a collapsed rail with a single expand toggle. Unaffected by TASK-29's click/drag defect (sidebar buttons are plain egui::Button, never wrapped in dnd_drag_source).
<!-- SECTION:NOTES:END -->
