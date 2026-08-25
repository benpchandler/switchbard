---
id: TASK-22
title: 'BUG: ~/.switchbard/config.toml repos list wiped during real-binary perf run'
status: To Do
assignee: []
created_date: '2026-08-05 05:21'
updated_date: '2026-08-05 05:54'
labels:
  - bug
  - incident
dependencies: []
priority: high
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Incident 2026-08-05 ~01:15: during automated perf smokes launching the packaged binary, config.toml lost its repos/worktrees lists (ui block survived; no config.broken-* backup created, so not the parse-failure path). Root cause unknown — suspects: concurrent instances racing save (two agents ran real binaries tonight), a save-on-exit writing an empty runtime state, or SWITCHBARD_PERF path. Repro + fix needed; consider: never persist an empty repos list over a non-empty one without a tombstone/backup, and single-instance lock. Owner restore recommendation (evidence-based): May-21 backup set with hive->switchbard rename — budget, CambridgeKitchens, MusicProduction, switchbard, DealFinder (worktree counts sum to the logged 83 exactly); suggest adding hub. Wiped file archived at ~/.switchbard/config.wiped-20260805-incident.toml.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Root cause confirmed and fixed on feature/integration (commit fix(gui):
stop test code from writing to the real ~/.switchbard/config.toml).

The perf-run binary launch was A contributing incident, but not the
recurring cause: HiveApp::save_config() always wrote to the real
~/.switchbard/config.toml with no path override, so any kittest that
clicks a Save/Delete-style control (e.g. the Wave 2 saved_views test)
silently overwrote the developer's real config on every `cargo test`
run, headless or not. Fixed by adding HiveApp::config_save_path:
Option<PathBuf>, defaulting to None (unchanged production behavior);
GUI test fixtures now opt in to an isolated temp path explicitly.
Verified: full workspace test suite runs with the real config file's
mtime/sha256 unchanged before and after.

Follow-up still open and NOT addressed by this fix: single-instance
lock and never-persist-empty-over-non-empty tombstone/backup, both
named in the original description.
<!-- SECTION:NOTES:END -->
