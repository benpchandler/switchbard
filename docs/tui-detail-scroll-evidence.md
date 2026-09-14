# Task detail focus and scrolling

## Objective and acceptance

TASK-221: details taller than the terminal must remain readable. Enter opens and focuses task details. Shift+Tab switches keyboard focus between details and list without closing the preview; Esc closes it. Mouse wheel targets the pane under the pointer. Scrolling details must not select another task. Existing PR navigation and modal input keep their contracts.

## State and stress matrix

| State | Required evidence |
| --- | --- |
| Closed, open with list focus, open with detail focus | Real Enter, Shift+Tab, Esc events and rendered focus cue |
| Empty, short and long detail content | No phantom focus; bounded offsets; final content reachable |
| Wrapped, multiline, Unicode and unbroken text | Narrow terminal render and end-of-content reachability |
| Top, middle and bottom; repeated navigation | No overscroll or hidden task selection changes |
| Keyboard and pointer over each pane | Correct pane receives scrolling; list navigation resumes |
| Narrow, ordinary, wide and zero-sized terminal; resize | No panic, clipping bounded, focus usable after resize |
| Help, picker and text input over detail | Modal input takes precedence; no hidden detail movement |
| Page switch and PR list/detail | Existing PR controls and page state remain valid |
| New write/save/conflict/permission states | N/A: this change is transient read-only navigation |
| New network latency/retry/offline states | N/A: no new I/O; existing PR suites cover remote-state behavior |
| Touch and browser zoom | N/A: terminal input; cell-size variation covered by dimensions |

## Execution ledger

- Initial primary checkout: clean, main at f8098a30.
- Isolated implementation: feat/tui-detail-scroll in /Users/bpc/Dev/.worktrees/switchbard-detail-scroll.
- Source and E2E tests delegated to detail_scroll; docs, audit, gates and guarded installation owned by root.
- Reproduction: `cargo test -p switchbard-tui --test detail_pane` failed on the unmodified implementation after Enter and 20 Down events; the screen still showed only description lines 00-12. Task rendering passed a constant zero scroll and the main loop consumed only key events.
- Actual binary terminal audit: isolated tmux socket `codex-detail-scroll`, real Enter/Down/Shift+Tab/End keys, SGR wheel-down escape at detail coordinates, resize from 100x20 to 40x8. Detail scroll reached line 79; Shift+Tab returned list navigation and selecting another task reset the detail position. Local captures: `/tmp/sbt-detail-keyboard.txt`, `/tmp/sbt-detail-mouse.txt`, `/tmp/sbt-detail-end.txt`, `/tmp/sbt-detail-narrow.txt`. A transient rebuild-triggered restart interrupted the first mouse probe; repeating with a private binary copy passed.
- Independent read-only review: no remaining findings after confirming Shift+Tab normalization and wrapped Unicode/unbroken-content tests.
- First full TUI gate exposed an existing row-layout assertion that expected Ctrl-d after Enter to move list selection. The new requested focus contract requires Shift+Tab back to the list first; the regression test is being updated to exercise that transition.
- Full pre-merge TUI gate passed after the explicit row-layout focus transition. Guarded installation refused to drop installed commit `8b6f259e` (line wrapping under `v l`); merged that exact commit without conflicts and independently reviewed integration.
- The merged gate exposed crowded shortcut text in Help. Help now measures terminal display width, separates key/action labels and gives oversized entries their own wrapping row. Browse, detail, row-layout and shortcut targeted suites pass, including the existing initial report-help discovery assertion. Final guarded installation passed.
- Human terminal-emulator visual approval remains separate from automated terminal-buffer rendering.

## Implementation evidence

`crates/switchbard-tui/tests/detail_pane.rs` covers Enter-focused scrolling, reversible Shift+Tab, list selection and detail reset, hovered wheel/click routing, top/bottom clamping, wrapping with Japanese and accented text plus an unbroken string, terminal sizes 0x0/1x1/40x8/100x20/180x50, picker/filter/help precedence, remapped focus binding and help, empty and short details, and page-switch cleanup. The existing PR frame test compares inactive task/PR framing. The authenticated PR journey includes focus assertions but remains an optional ignored test; authenticated live PR focus is not claimed here.

The current detail-edit implementation distinguishes reading focus from its editable field-cursor mode, with `detail_pane::Hit` recording pointer regions. Opening Tasks details focuses reading; a second Enter or `l` activates editing. Shift+Tab returns to the list without closing the pane or losing the scroll position. Reading scroll does not trigger the editor's cursor-follow adjustment. The actual terminal loop enables and disables mouse capture and dispatches both key and mouse events. PR PageUp/PageDown preserves its existing detail-scrolling behavior even with list row focus; Tasks paging follows pane focus.

Rebase recovery retained these contracts alongside the newer detail editor and Agents page. The unchanged 30 detail-edit and eight detail-pane E2Es pass, with one additional mixed reading/editing/Esc/Shift+Tab/mouse/page-switch journey. The focused recovery run passed 63 tests; four authenticated opt-in tests were excluded from that deterministic run.

## Final verification and delivery

`mise run tui-install` passed on the combined branch: formatting, clippy with warnings denied, 253 passed tests and 21 existing optional ignored tests, then the guarded release installation. Installed `sbt build-id` reports clean commit `32cce22c1d4ff709b412641b16fbd9218f433c72`, branch `feat/tui-detail-scroll`. The installer preserved previously installed `8b6f259e` by ancestry; no force flag was used. Final terminal verification also checked readable Help (`/tmp/sbt-detail-help.txt`) and repeated End/Shift+Tab/list navigation on the merged build.

TASK-221 is Done and its live claim released. Code and evidence are committed in the isolated worktree; main remains untouched. No push, PR or remote CI was requested or performed. Automated terminal rendering and real terminal input passed; owner visual acceptance in their own terminal and optional authenticated PR journeys remain unclaimed.
