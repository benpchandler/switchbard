# Recognizable history preview

## Objective and boundary

The owner rejected the installed history screen because it repeats serialized settings rather than showing a recognizable arrangement. Approved direction: a plain-language title and a miniature table. This slice produces real application examples for review before visual acceptance or a broader rollout. Task 154 acceptance criterion 3 is reopened. Capture, deduplication, persistence, restore and deliberate slot saving retain their existing contracts.

The table applies the saved arrangement to current cached data; it is explicitly labeled as current data, not a historical snapshot. The existing terminal theme, column names, grouping and paint are the design source. Preview navigation must not mutate the active view or write a checkpoint.

## State and stress matrix

| State or transition | Evidence for this slice |
| --- | --- |
| One saved PR view, like the rejected screenshot | Real-key PR render with plain-language title, real headings and representative current rows; visual review pending. |
| Task view with grouping and paint | Real-key render preserving saved presentation and current palette; visual review pending. |
| Several saved views and selection changes | Keyboard journey proves preview changes while active state and persisted files remain unchanged. |
| Empty history, no matching history, empty filtered data | Explicit empty messages; distinguish no history from no rows matching an existing arrangement. |
| Long titles, filters, many columns and Unicode | Bounded title/layout; focused E2E render checks. |
| Narrow and short terminal | Focused layout checks; broader visual review remains pending after first example. |
| Invalid or unsupported history | Existing persistence protection retained; no fabricated preview. |
| PR loading, stale or unavailable cache | Show cache state or empty data honestly; never fetch or mutate from rendering. |
| Restore, cancel, repeated navigation and capture while picker open | Existing stable payload and restore journeys retained; add nonmutation check for preview. |
| Network writes, permissions changes, historical row snapshots | N/A: this is a local preview of saved view settings using already available data. |

## Delivery state

Initial primary checkout changes in app/pickers.rs, view.rs and tests/filter.rs are preserved. New isolated worktree starts clean from installed commit 851e0b56 on feat/tui-history-preview. This review slice is not visual approval and does not close task 154.

## First rendered examples

The real application TestBackend emits cell symbols, colors and modifiers under `crates/switchbard-tui/tmp/history-preview/`. A PNG rendering preserves these terminal cells for Visual Review. Tasks use a real temporary backlog with representative tasks; the opt-in Pull Requests example reads the live repository through the existing read-only fetch path. Neither represents historical row data. Grouped Tasks, multiple choices, empty current data and a PR view have been rendered. Final visual acceptance remains pending.

The first review targets are `pull-requests-6fbc942b88a0` and `tasks-grouped-c397f958f6d3`, owned by the history-preview worktree. Initial revisions: `revision-28c60c268f848a086474` and `revision-f827e2b380c8f9050363`. No annotations had been submitted at initial handoff; this is not approval.

Focused validation passed: real-key history preview, recognition, history restore, row-layout and detail suites, plus the separately invoked authenticated read-only PR preview. Clippy passed with warnings denied. Existing raw-configuration expectations were replaced by rendered table-row and color assertions.

Full `mise run tui` passed: formatting, all-target clippy with warnings denied, and the complete TUI test suite including real signal/timer recovery. The authenticated PR preview was also run separately and passed. This slice is committed locally for review; installation and visual acceptance remain pending.

## Full-context walkthrough

At the owner’s request, the real-key capture journeys now show both Tasks and Pull Requests at 120 columns by 30 rows: normal list, Views menu, history preview, and the restored list. Captures are under `crates/switchbard-tui/tmp/history-context/`, with corresponding Visual Review targets prefixed `tasks-1` through `tasks-4` and `prs-1` through `prs-4`. All three capture journeys passed, including the separately invoked live read-only PR journey. This followup changes capture coverage only; the review build is still not installed and visual acceptance remains open.
