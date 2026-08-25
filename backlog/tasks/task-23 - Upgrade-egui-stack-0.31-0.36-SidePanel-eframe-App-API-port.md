---
id: TASK-23
title: 'Upgrade egui stack 0.31 -> 0.36 (SidePanel/eframe::App API port)'
status: Done
assignee: []
created_date: '2026-08-05 14:01'
updated_date: '2026-08-25 17:26'
labels:
  - tech-debt
  - egui
dependencies: []
priority: medium
ordinal: 23000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Fix-wave follow-up (2026-08-05): the crash-fix upgrade (commit 34d6bf4) landed on egui/eframe/egui_kittest 0.31, not the owner's preferred latest 0.36, because 0.30->0.36 hit 27 errors including SidePanel/TopBottomPanel removed from the egui namespace and eframe::App's core trait method signature changed from update(&mut self, ctx, frame) to ui(&mut self, ui, frame) -- an architectural rewrite of the panel/app-loop system, not mechanical renames. This needs a deliberate, scoped port: every CentralPanel/SidePanel/TopBottomPanel call site across app.rs/sidebar.rs/top_bar.rs/agent_context.rs/workspace/mod.rs/backlog/mod.rs and HiveApp's eframe::App impl need to move to the new API shape. Verify egui_commonmark and egui_kittest both have 0.36-compatible releases before starting (they did as of 2026-08-05: egui_commonmark 0.24, egui_kittest 0.36). Re-run the full kittest suite afterward to confirm no semantic drift in what the QA suite's queries actually mean under the new API.
<!-- SECTION:DESCRIPTION:END -->
