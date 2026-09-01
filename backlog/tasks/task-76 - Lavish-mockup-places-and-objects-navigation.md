---
id: TASK-76
title: 'Lavish mockup: places-and-objects navigation'
status: Done
assignee:
  - '@claude'
created_date: '2026-08-31 21:20'
updated_date: '2026-09-01 10:51'
labels:
  - ia
  - design
dependencies: []
priority: high
project: Information Architecture V2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Mock the sidebar-of-places IA in switchbard's real theme palettes (pattern: the weekly-goals mockup): sidebar (digest, projects, goals, repos), a project drill-in page (overview, member tasks with list/board toggle, goals, burndown), a goal page, and where Dispatches lands as a facet. Owner reviews and annotates before any decision record is written.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Mockup reviewed by the owner in Lavish; direction decisions captured
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner review rounds 1-2 in Lavish (2026-08-31/09-01), decisions captured for TASK-77:
1. RESOLVED scope model: Option B amended - sidebar places are scoped by a MULTI-SELECT repo switcher (pick any set of repos, views aggregate). Rejected: Option A global-tree-with-badges as drawn, and one-repo-at-a-time switching.
2. RESOLVED naming/structure: 'Projects' is not a place. The place is TASKS (primary work list); projects are an organizational grouping over it (Group by: Project headers with computed roll-ups). Matches Linear's issues-primacy. The Repos place renames to OPS so 'repo' only ever names the scope.
3. Mock hygiene rulings: commentary must never sit inside a frame (reading key added); goal surfaces need explicit new/edit/check-in affordances (added, incl. Goals index with inline cumulative check-in).
4. Still open for TASK-77: Q12 does the project page survive or is the group header enough; Q1 dispatch facet vs place; Q3 initiatives as second grouping level; Q8 Ops merge shape; Q9 sub-issue indent vs collapse; Q11 cumulative check-in affordance; Q13 sidebar children under Tasks.
Artifact: ~/.lavish/switchbard-ia-places.html (Lavish session 3941254e8bd08b0e).

Round 3 decisions (2026-09-01): (a) Q1 RESOLVED - Dispatch gets a home as a built-in 'Dispatches' view under the Tasks place (queue rows with kill/retry/log; footer lamp deep-links); not a top-level place. (b) Q13 RESOLVED - sidebar children under Tasks are built-in views (All tasks, Dispatches) plus explicitly pinned projects only; nothing auto-populates. Remaining open for TASK-77: Q12 project-page survival, Q3 initiatives grouping, Q8 Ops merge shape, Q9 sub-issue rendering, Q11 check-in affordance, Q2 digest composition, Q5-Q7 detail placement.

Round 4 (2026-09-01, terminal): pin icon rejected. Adopt Linear's pattern instead: (a) FAVORITES group at the top of the sidebar holding explicitly favorited objects rendered with their type glyph (project, saved filter) - favoriting is an action, never sidebar decoration; (b) saved filters are first-class named views (Linear 'Views') that can be favorited; (c) under Tasks only the built-in views All tasks + Dispatches remain. Q13 closed in this form.

Round 5 (2026-09-01): (a) Owner flags the attention feed ('Needs a human') as a super important area - built out with inline actions per row: PR Review/Merge, Open mock (visual review), dispatch Retry/Log, server Restart (Ops notifications added per owner note), worktree Remove, port Kill. Architecture answer recorded: feed rows are computed from their owning objects (PR probe, run reaper, server watch, port scan, removal_safety), deep-link to them, and reuse the same command verbs - not stored on tasks. Placement is open as Q2 (A Digest section / B Inbox place / C plus Tasks view). (b) Mock format rulings: frames get real window height; resolved questions removed from the artifact into a 'Decided so far' section; all open questions converted to lettered multiple choice with side-by-side renderings where possible (7c vs 7c-B for sub-issues). Open set: Q2, Q3, Q5-Q9, Q11, Q12.

Round 6 (2026-09-01): (a) Q2 RESOLVED = A: attention feed lives on Digest; feed given full width with per-row actions; server/Ops notification rows included. (b) Q3 RESOLVED by broader ruling: Tasks grouping must be generic over every available field (project/status/initiative/priority/label/repo/...), never hardcoded options; filtering is a Filter-builder plus recent filters, no hardcoded action chips. (c) Dispatches must move toward the Mission Command console the owner has in mind - new Q14 (A run list + activity line / B + selected-run console w/ log tail, AC progress, SITREPs / C full fleet place); sidebar highlight bug fixed (Tasks no longer marked active on the Dispatches view). (d) Project page flagged confusing a third time - Q12 recommendation recorded as B (cut; expand group header in place), B alternative rendered. (e) Ops must retain ALL existing functionality - worktrees, services start/stop, listeners, open-in-browser, kill squatter, agent sessions, removal safety - now rendered. (f) Standing mock rule reaffirmed twice: no commentary inside frames. Open set: Q5-Q9, Q11, Q12, Q14.

Round 7 (2026-09-01): (a) Group headers in Tasks must WIN the color-hierarchy against task rows (owner: grouping took a backseat, hard to distinguish groups; dark mode fared better) - headers now use the recessed faint surface, bold 12.5px, 2px top rule, taller padding, in both themes. Implementation note for EGUI Polish overlap: group headers need their own elevation token. (b) Burndown-on-project-page questioned again - clarified it plots remaining member tasks over time (per TASK-76 spec) and recorded that Q12=B cuts it with the page; more evidence toward B.

Round 8 (2026-09-01): process correction from owner - agent must run its own visual review (screenshots, optical pass) using the design-review skill kit (~/Downloads/design-review-skill-kit) before every handback, instead of blind HTML edits. Root cause of 'you didn't fix it' found: the owner's Chrome tab was showing a DIFFERENT lavish artifact (xplan Portfolio Mission Command session), so round-7 fixes were invisible; correct session reopened. Self-review (headless Chrome captures, light + dark) found and fixed: stale 'Repos' place on the Goal-page sidebar (missed rename); FAVORITES glyphs indistinguishable (saved filter now uses a distinct glyph); dev-speak inside frames (ungrouped tail, collapsed, measure/scope config vocabulary, computed-from-done-in-week) replaced with user copy; project-page table had task IDs inside the Status column (moved into Task cell). Dark mode verified: group headers dominate correctly.

Round 9 (2026-09-01, first round after the poll-truncation fix): (a) Q12 RESOLVED = B and stronger: the project page is CUT entirely; the group header expands in place for summary, and the stack rank surfaces only as a sort/filter option in Tasks (Sort: rank next to Group by) - never a page or dedicated column. Q5 dissolves with it (delivery = status-cell badge + facet). (b) Q14 RESOLVED = C: Command becomes its own place - the fleet console (agents, missions, leases, SITREPs, support requests with respond affordance); Dispatches under Tasks remains the task-delivery facet; same runs, two axes. (c) Buttons: icon-only with tooltips for universal actions everywhere (open/merge/retry/log/kill/remove/edit/check-in); text labels only where no icon is unambiguous (empty-state CTAs, Roll). (d) Selected rows get a dedicated stronger tint + 3px stroke so selection never reads as a group header (owner: too close in color). Visual self-review run post-change (light render, tiles): icons, sort control, selection contrast, and Command console all verified. Open set: Q6, Q7, Q8, Q9, Q11.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Mockup delivered and reviewed across nine owner rounds in Lavish (~/.lavish/switchbard-ia-places.html, session 3941254e8bd08b0e); every direction question either resolved in the notes above or explicitly carried (Q6-Q9, Q11 remain open for follow-up work). The decided IA shipped end to end: TASK-96..101 implemented the places (PRs #80-#85), closure landed in PR #86, and the adversarial parity audit against the frozen mock closed in the polish pass PR #87 (tinted chip language, Dispatches/Command tables, Digest goal-card grid, narrow-width fix, one stroke authority, QA fixtures). A CI break the program surfaced (Linux dead-code in agent_sessions.rs, red main since PR #83) was fixed in PR #89.
<!-- SECTION:FINAL_SUMMARY:END -->
