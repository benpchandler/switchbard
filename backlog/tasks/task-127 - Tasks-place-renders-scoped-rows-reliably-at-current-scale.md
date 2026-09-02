---
id: TASK-127
title: Tasks place renders scoped rows reliably at current scale
status: In Progress
assignee: []
created_date: '2026-09-01 18:35'
updated_date: '2026-09-01 21:13'
labels:
  - bug
  - gui
  - usability
  - performance
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Incident outcome: the Tasks place must render the same scoped task set its sidebar count reports, without a blank window, panic, stale hidden repo scope, or post-refresh disappearance.

Observed in the installed app on 2026-09-01: Switchbard-only scope reported 40 tasks while the table reported 0; All repos reported 496 and opening Tasks blanked the window. An exact origin/main probe at bb848568 reproduced the material incident: persisted Board view, All repos, 496 open tasks, and a window that remained blank for far beyond five seconds before eventually painting after on the order of minutes. Sampling the live process localized the delay to unbounded Board card construction; List is already virtualized. The positive-count/zero-row report remains a separate scope/filter-honesty check because current-main deterministic filter-empty state renders an explicit message rather than blanking.

State and stress matrix: cold/no snapshot; loading; loaded empty; one; Switchbard-only many; All repos realistic maximum (496+); long content; current and narrow window; no filters; explicit repo filter; migrated legacy scope; refresh success; refresh failure retaining last-known rows; restart; repeated scope changes; keyboard navigation; and bounded render responsiveness. Each applicable state needs behavioral, visual, accessibility, or performance evidence; unsupported states must be named as gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A deterministic native GUI journey on the exact merged base proves the pre-fix failure, or proves the installed-bundle revision is the only cause before implementation begins.
- [x] #2 Sidebar counts and Tasks rows derive from the same explicit scope and source state; no loaded state can report a positive scoped count while rendering zero matching rows without an honest loading, empty, stale, or error explanation.
- [ ] #3 All repos, one repo, zero/one/many/496+ tasks, persisted and migrated filters, refresh success/failure, restart, repeated scope changes, long content, and narrow-window states are covered by the design-state matrix and bound to evidence.
- [ ] #4 Opening or changing Tasks scope never blanks the native window, panics, loses the last-known usable rows on refresh failure, or enables mutation against stale cached-only data.
- [x] #5 Focused tests, render performance evidence, full mise run ci, and a packaged exact-revision GUI proof pass before the incident is closed.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-09-01 pre-fix exact-revision evidence: built release package from origin/main bb848568 in an isolated worktree, launched it as unit-local SwitchbardExact, and preserved the user config with tasks view_mode=board. Digest loaded 496 open tasks. Clicking Tasks produced a fully blank content window for far beyond 5 seconds while the process remained alive; a 2-second sample repeatedly showed render_tasks_place -> render_board -> render_column -> render_strip -> paint_card across the full Board set. The view eventually painted after on the order of minutes, confirming a synchronous unbounded Board render rather than a crash. Existing focused List perf smoke was green at 400 tasks (p95 13.994ms) and did not exercise Board.

2026-09-01 implementation evidence: Board columns now use bounded show_rows construction for cards and keep + Add task as the final reachable virtual row; empty columns retain an honest empty/add state. Card geometry is enforced at 148 px with a live debug assertion. Headless and GPU evidence deliberately rejected 124 px, 132 px, and a 142 px GPU-rendered card before the final 148 px contract passed. A focused scope/filter test proves a positive 1-open scope with zero filter matches renders No tasks match the current filters plus 0 of 1 · 1 open, never a silent blank. The performance fixture now contains 500 visible open tasks. Pre-fix Board p95 was 65.445 ms; final Board p95 is 13.158 ms, p50 12.471 ms, max 105.159 ms across 200 frames, while List p95 is 15.227 ms. Standard and narrow Board screenshots passed on GPU in light and dark themes; inspected states show aligned cards, clamped long content, usable facets, and no overlap in the captured viewport. Final mise run ci passed in 32.79 seconds. EXTERNAL_BLOCK: Computer Use still returns cgWindowNotFound even for Finder and times out against the signed exact-revision probe, so the packaged post-fix Tasks navigation and end-of-column scroll journey is not independently observed; the probe was stopped and the installed /Applications/Switchbard.app process was restored. AC3-5 remain open pending that exact packaged journey and the remaining refresh/restart/state-matrix proof. Time log: triage and exact pre-fix reproduction about 9 min including 24.75 s release build and a blank lasting far beyond 5 s; cheap-agent baseline tests about 1 min; cheap-agent implementation about 5 min; adversarial geometry review and repair about 7 min; focused and GPU verification about 4 min including 6.72 s standard and 5.00 s narrow screenshot runs; final perf 4.96 s; final full CI 32.79 s; desktop automation retries about 5 min; orchestration total about 45 min.

2026-09-01 recovered native verification: Computer Use recovered at 20:45 EDT; Finder became observable in 416 ms. Rebuilt the release executable from exact implementation commit 5bcae8b in 5.92 s, installed it into the existing unique probe bundle with the pinned sidecar/resources, re-signed nested executables and the bundle, and passed deep strict codesign verification. The exact probe loaded 496 open tasks. Warm All-repos Tasks navigation rendered a populated Board in 952 ms instead of the pre-fix multi-minute blank. Switchbard-only scope rendered 40 open rows in 929 ms; repeated scope toggles remained responsive and All repos was restored in 1.20 s. Refresh completed in 802 ms with the same 492 visible / 496 open rows retained. A process restart eventually loaded 496 open tasks and Board reopened in 1.23 s; standard and narrow GPU proof, 500-task perf, focused tests, and full CI were already green, so packaged exact-revision AC5 is now satisfied. Remaining AC3/AC4 gaps: cold restart displayed the false empty message No tracked worktrees have a backlog/config.yml or backlog/tasks directory until the first background scan completed at about 33.5 s, rather than an honest loading state; and audit found no focused test proving a refresh failure retains last-known rows while disabling mutation against stale cached-only data. The installed /Applications/Switchbard.app was restored and the probe stopped.

2026-09-01 explicit task-read-state slice: added a shared TasksReadState lifecycle (InitialLoading, Ready, Refreshing, Stale with failed-source count) beside the cached rows. Cold Tasks and launch Digest now show an honest loading state instead of the prior false empty message; a successful zero-row scan alone may show the true empty state. Manual refresh and tracked-repo changes enter Refreshing without clearing rows. The backlog worker publishes Ready on a clean scan or Stale on any source failure, and a focused worker test proves a failed source retains its last-known task row. Tasks and Digest keep retained rows visible with explicit refreshing/stale copy; an empty stale model offers Retry. Focused verification only, per owner direction: retention unit test passed; three Tasks lifecycle UI tests passed; refresh-button transition test passed; cargo clippy -p switchbard-gui --lib -- -D warnings passed in 9.31 s; 500-row render smoke passed (Board p50 12.689 ms / p95 13.770 ms, List p50 13.408 ms / p95 15.533 ms). No full CI or packaged native journey was rerun in this light pass. AC4 remains open because mutation gating while the model is stale was intentionally left for the next discussion.
<!-- SECTION:NOTES:END -->
