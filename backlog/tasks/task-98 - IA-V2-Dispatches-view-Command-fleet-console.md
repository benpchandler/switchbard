---
id: TASK-98
title: 'IA V2: Dispatches view + Command fleet console'
status: In Progress
assignee: []
created_date: '2026-09-01 02:24'
updated_date: '2026-09-01 05:51'
labels:
  - ia
  - gui
  - dispatch-failed
dependencies: []
priority: high
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Dispatch's two axes (trajectory: IA V2). Under Tasks: the built-in Dispatches view - per-run status, live activity line, elapsed, watch/kill/retry/log. Command as its own place: the agent-scoped fleet console - agents, missions, worktree leases, SITREP age, support requests (NEEDS_DECISION etc.) with respond affordance and blast-radius note. Footer lamp deep-links.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Dispatches view lists runs with kill/retry/log wired to existing dispatch_kill/reaper paths
- [x] #2 Command place renders the fleet with support requests surfaced and respondable
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Dispatch failed: claude exited with 1

Named gap (per binding directive 5): no NEEDS_DECISION/SITREP store exists in switchbard-core. Command's support-request card renders evidence-only (state/elapsed/log path) from DispatchRun; it never fabricates decision text. A structured support-request store is real future work, not built here.

Named gap: switchbard_core::agent_sessions classifies interactive sessions by exact process comm name (claude/codex). Real installs that run as a wrapping interpreter (node, a shim script) will not be detected — a deliberate honest boundary (no argv substring matching, to avoid false positives), documented in the module doc.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Split IA V2's dispatch axis: ui::places::dispatches (Tasks/Dispatches, task-scoped: facets Active/Queued/Finished/Failed, per-row Watch/Kill/Retry/Log via the existing dispatch_kill/spawn_backlog_dispatch_toggle paths, selected-run detail card with bounded log tail + AC chips + SITREP age) and ui::places::command (Command place, agent-scoped: unions dispatch runs + new switchbard_core::agent_sessions interactive claude/codex scan, facets All/Dispatch/Interactive/Needs-you, evidence-only support-request card, Fleet|Context|Hooks section switcher keeping the old Agents view's Context/Hooks reachable). New core capability agent_sessions.rs (cfg-gated OS scan, pure+tested parse layer) polled by a new 5s GUI worker. mise run ci green; new kittest coverage in command_fleet.rs plus updated dispatch_operability.rs/nav_ia_v2.rs/ui_views.rs/legibility_audit.rs for the new default Fleet section and Active-facet default. Two named gaps recorded in notes (no NEEDS_DECISION store; agent_sessions misses node-wrapped CLI installs).
<!-- SECTION:FINAL_SUMMARY:END -->
