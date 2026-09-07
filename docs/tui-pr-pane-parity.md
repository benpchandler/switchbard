# PR detail pane parity

The PR side pane uses the task pane presentation: a full-height 50/50 split, plain border, bold title, muted identity/status line, blank-line spacing, and accent section labels. `detail_pane.rs` owns these shared choices. PR check/review/merge observations, reference-derived task links, and Ctrl-d/u scrolling remain available.

## State and stress coverage

| State or stress | Evidence |
| --- | --- |
| Default, loading, no selected row | `pr_detail_matches_task_pane_frame_and_empty_state`: actual key events compare task and PR top-border cells at 40x12, 80x24, 120x40, 180x50; empty text stays visible. |
| Unavailable, retry, navigation during read | `non_repository_is_unavailable_not_a_successful_empty_list`: actual failing Git process, refresh, page switch. |
| Selected, historical, many rows, no linked tasks | `live_repository_renders_actual_pull_requests`: authenticated GitHub read, side-by-side list, title/metadata/section styles, j/k selection. |
| One linked task, long title/URL/head hash, narrow/short/current/wide | `live_task_links_and_failed_refresh_preserve_real_task_bytes`: real task reference, four terminal sizes, repeated Ctrl-d reaches the final line. |
| Stale snapshot, dependency failure, task read-only invariant | Same live journey removes the temporary repository remote, retries, preserves observations and every task byte. |
| Empty filter, historical filter, pagination, resumed page/filter | `live_all_states_can_be_filtered_without_refetching` and `pr_filters_use_the_shared_picker_and_preserve_task_filter_and_restart`. |
| Dirty, saving, conflict, outcome unknown, write permission transitions | N/A: the PR pane is a cached read-only view; no save or mutation is introduced. Access/read failures use the unavailable/stale paths. |
| Pointer, touch, browser zoom | N/A: keyboard TUI; terminal cell dimensions are the layout input. |
| Open/draft/check variants, many linked tasks, Unicode/multiline extreme content, fetch cap 1000, prolonged latency | Coverage gap: no new live cases established for these states. Existing observation semantics and 20-link bound are retained. |
| Human appearance approval | Pending user use of installed TUI. Automated cell assertions are not approval. |

## Verification

The new parity test failed before the change: the PR top border was nested at row 7 in a 40x12 terminal, while the task border started at row 1. It passes with the shared full-height split.

`SBT_PR_REPO=/Users/bpc/Dev/switchbard cargo test -p switchbard-tui --test pull_requests -- --include-ignored --nocapture` passed all six journeys against real GitHub observations, including 100 loaded PRs, historical filtering, pagination to 135 PRs, and an actual failed refresh. No GitHub writes were made.

Per-slice gate: `mise run tui-install` passed. It runs formatting, clippy, the TUI test suite, and installs the release binary. The running TUI resumes through its existing binary-change reload mechanism.
