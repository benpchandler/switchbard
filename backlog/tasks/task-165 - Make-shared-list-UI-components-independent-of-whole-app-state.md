---
id: TASK-165
title: Make shared list UI components independent of whole-app state
status: To Do
assignee: []
created_date: '2026-09-07 10:05'
labels:
  - abstraction
  - refactor
  - ui
  - tui
  - gui
dependencies:
  - TASK-162
priority: medium
project: Abstraction
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: developers adding lists must wire rendering to the whole App or duplicate controls, increasing the chance that users encounter inconsistent keyboard, selection and layout behavior. Priority is medium because this is a maintainability and consistency gap.

Evidence: crates/switchbard-tui/src/view.rs:21 draw takes &mut App and draw_table also reads App; crates/switchbard-gui/src/ui/components/table_shell.rs:11 already provides a reusable GUI shell. Verified using sed -n "1,55p" on view.rs and rg -n "pub fn" on table_shell.rs. Build on the typed picker work in TASK-129 and module split in TASK-131; retain frontend-specific rendering.

Discovery: owner-requested reuse assessment in session 01a07c1d-7259-73d1-82b7-97d68ba8ac87, recovered 2026-09-07; source verified at 7d09874 with rg/sed. Reconcile the implementation behind TASK-141.1 through TASK-141.4 before changing code; their PR parity behavior is not new scope here. Preserve existing commands and saved data. Choose between a small shared core contract and frontend-local adapters based on two concrete consumers; document the boundary and rationale before extraction, without a speculative registration framework.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Extract bounded list/picker presentation inputs and explicit interaction outputs so reusable components do not require the entire App.
- [ ] #2 Tasks and one existing non-task list consume the shared primitives; feature actions and detail content remain owned by their features.
- [ ] #3 Inventory and reuse existing GUI table/filter/badge primitives where applicable; document the GUI/TUI boundary without forcing a common widget framework.
- [ ] #4 Verify keyboard navigation, selection, focus, empty/loading/error states where applicable, long labels, narrow layouts and large lists against the canonical design-state matrix, recording evidence and explicit gaps; run required render-path performance checks if GUI paths change.
<!-- AC:END -->
