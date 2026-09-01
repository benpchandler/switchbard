---
id: TASK-100
title: 'IA V2: Ops place - merged Servers/Workspace, one row per worktree'
status: In Progress
assignee: []
created_date: '2026-09-01 02:24'
updated_date: '2026-09-01 05:49'
labels:
  - ia
  - gui
dependencies: []
priority: medium
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rename Repos->Ops and merge (trajectory: IA V2). One row per worktree: services start/stop, listeners with open-in-browser and logs, external squatters with kill, git state, agent sessions attributed per worktree, removal behind the removal_safety verdict. Retains every existing Servers/Workspace capability.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 All current Servers + Workspace actions reachable from the merged rows
- [x] #2 Removal still gated on RemovalVerdict (and RemovalAuthorization once TASK-81 lands)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented as crates/switchbard-gui/src/ui/places/ops.rs (+ ops/{row,agent,bulk_remove,create_worktree,landing,rename_worktree,staleness,tooltips}.rs, the last 6 moved verbatim from the retired ui/workspace/). Ops routing in app.rs now points at ui::places::ops::render; ui/workspace/ deleted. Table: egui_extras::TableBuilder, genuinely virtualized (body.rows only invokes visible rows). Columns Worktree/Git/Services/Listening/Agent/actions per mock section 6. Agent cell: dispatch-run attribution only (claude - active Nh); TASK-98's agent_sessions core capability was not on main when this task started (confirmed via git log --all) - ui::places::ops::agent module doc names the seam for wiring interactive sessions in later. AC #2: RemovalAuthorization (TASK-81) has NOT actually landed - only its backlog task definition was committed (d3c3e52), no RemovalAuthorization type exists in switchbard-core. Removal is gated on RemovalVerdict alone via the unchanged RemovalSafety/removal_facts path, noted honestly per the brief's own fallback instruction. Tracked-repos side panel retired from Ops per binding directive (repo add/remove now Settings-only, already true, verified by existing settings tests); ui/sidebar.rs trimmed to just the remove-repo confirmation modal.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Ops place merged: one row per worktree (primary bold repo+branch, linked worktrees indented with a hook-arrow), matching mock section 6. Every legacy Servers/Workspace verb is reachable from the merged rows: start/stop (Services cell), open-in-browser (Services + Listening cells), logs (Actions cell, new - wires the previously-unused ActiveRun::log_path field via a new HiveApp::open_log_file), kill (Listening cell + squatter rows), rename/remove (Actions cell), bulk-select/bulk-remove and the staleness facet bar (unchanged, reused). Removal stays gated on RemovalVerdict via the untouched RemovalSafety/removal_facts path; TASK-81's RemovalAuthorization has not actually landed (only its backlog definition has), noted honestly. Agent cell shows dispatch-run attribution with a named seam + doc for TASK-98's not-yet-landed agent_sessions capability. Perf: p50/p95 frame and workspace render time both improved roughly 4-5x vs the TASK-96 baseline (25.5ms/27.0ms to 5.8ms/6.5ms workspace p50/p95), mostly because the new table is genuinely virtualized. mise run ci green; new/updated kittest and unit test coverage for row rendering, verb reachability, removal gating, and the tiered Open-button resolution logic (restored from the retired module). Screenshots captured both themes: populated multi-repo, squatter row, selected-row stroke ring, narrow width.
<!-- SECTION:FINAL_SUMMARY:END -->
