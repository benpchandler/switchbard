---
id: TASK-15
title: Functional parity with Backlog.md webview (task surfaces)
status: In Progress
assignee: []
created_date: '2026-08-05 03:10'
updated_date: '2026-08-05 03:17'
labels:
  - hub
  - parity
dependencies:
  - TASK-10
priority: high
ordinal: 15000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owner directive 2026-08-04: the unified Backlog view must match the functionality of the Backlog.md web UI (v1.47.1, e.g. localhost:6420) for task work. Parity audit done via live inspection; gaps to close (Tier 1):

1. BOARD LENS: kanban columns by status (Icebox/To Do/In Progress/In Review/Done per repo config) as a second lens beside the triage list; drag between columns = status change via backlog CLI. Cross-repo like the list.
2. GLOBAL SEARCH: free-text across all repos (title, id, description, labels), keyboard-first (Cmd+K style).
3. DETAIL PARITY: markdown-RENDERED description (egui_commonmark; keep raw editor behind Edit toggle), Dependencies (view+edit), References (view+add), DoD checkboxes (--check-dod), Implementation Plan section, labels edit, assignee edit, milestone display, Archive action, created/updated timestamps.
4. DRAFTS: visible as a filterable source (already parsed by core).
5. THEME: light/dark toggle — dark palette to come from design TASK-14 (direction A lamp language is the natural dark twin of B).

Tier 2 (separate tasks): cross-repo Statistics dashboard (TASK-16). DEFERRED by recommendation: Documents & Decisions browsing, Milestones management view, Clean-up-old-tasks — revisit after dispatch (TASK-11) ships.

Execution note: implement TOGETHER with TASK-14 (Flight Strips) on the same branch — building missing surfaces once, in the new visual language, instead of restyling twice.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Board lens with per-status columns, cross-repo, drag-to-change-status writing through backlog CLI
- [ ] #2 Global free-text search across all tracked repos with keyboard activation
- [ ] #3 Detail pane: rendered markdown description with edit toggle; dependencies, references, DoD, plan, labels, assignee, milestone, archive all present and mutating via CLI
- [ ] #4 Drafts surfaced as a source filter
- [ ] #5 Light/dark theme toggle with both palettes passing legibility_audit
- [ ] #6 Milestones: milestone view (tasks grouped by milestone, cross-repo) and milestone assignment in the detail pane
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner 2026-08-04: documents/decisions browsing confirmed punted; milestones promoted into Tier 1; burndown added to TASK-16. Theme pairing confirmed: B=light, A-lamp=dark.
<!-- SECTION:NOTES:END -->
