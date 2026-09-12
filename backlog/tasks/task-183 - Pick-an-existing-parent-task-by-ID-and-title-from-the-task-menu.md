---
id: TASK-183
title: Pick an existing parent task by ID and title from the task menu
status: In Review
assignee: []
created_date: '2026-09-08 13:30'
updated_date: '2026-09-08 16:51'
labels:
  - tui
  - hierarchy
  - ball:me
dependencies:
  - TASK-182
priority: medium
references:
  - https://github.com/benpchandler/switchbard/pull/143
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Impact: TUI users cannot assign a task parent from the task picker and must identify tasks by number outside the interaction flow. Evidence: owner request in this session; current app task chord offers rank, ball, complete, pin and goals without a parent picker. Scope: searchable existing eligible parent choices with ID and title, cancel and remove-parent paths, shared mutation validation and honest outcome feedback.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The task interaction menu opens a parent picker that shows eligible existing task IDs and titles and searches both.
- [x] #2 Selecting a parent and removing a parent persist through the core mutation layer; cancellation changes nothing and failures remain visible.
- [x] #3 Real-key TUI tests cover selection, title search, empty results, stale targets, narrow terminal and reopening after save.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Parent linking added to the current task picker under t a, with existing task IDs and titles, explicit Enter to save, and remove-parent path. Disk-backed TUI state and stress verification in progress. No installation or completion claim yet.

Eight new disk-backed parent-picker E2E journeys pass, and the full TUI suite is green. Covers ID/title search, explicit commit, canonical persistence, unlink, cancel/back, empty search, stale parent/source, unsupported nesting and narrow/Unicode/many-choice layouts. Final mixed-prefix integration check and installation remain.

Installed from b73cb28b with mise run tui-install: formatting, TUI clippy, 125 TUI tests passed (18 existing opt-in tests ignored), then release install succeeded. Installed sbt SHA256 7e358fee8d039743403e21bb17e0ccc852cb45be790a3491f3b754c7a186cfc5 matches isolated release artifact. Use t a, search ID/title, arrows and Enter; No parent promotes. Eight new parent journeys passed. State/stress evidence: docs/task-parent-picker/ledger.md. Human native appearance approval and PR/merge not claimed. TASK-185 records the existing active-claim renaming gap.

Delivered and merged PR143 on 2026-09-08 at20:50:56Z, merge b5661281af1803aca12f02242419726433f54d68. All8 PR checks passed;8 real-key picker journeys passed. This supersedes prior no-PR/no-merge notes. No shared app restart or new install during merge. Native appearance approval is not inferred from merge; task remains In Review with ball:me for that confirmation.
<!-- SECTION:NOTES:END -->
