# Configurable terminal task rows

## Objective and acceptance

TASK-214: let the owner experiment with task title wrapping and optional blank spacing in the actual terminal list before adding a PR column. Reuse the existing view settings authority; preserve compact defaults and independent PR settings. The user authorized implementation on 2026-09-12. Shared binary installation or live-session restart is a separate coordination boundary; an isolated runnable build is the initial delivery target.

## State and stress matrix

| Dimension | Required evidence |
| --- | --- |
| Default and legacy views | Existing files load with compact single-line rows. |
| Active and changed settings | Keyboard settings change wrapping and spacing without losing selected identity. |
| Persistence and interruption | Saved view load and self-restart retain the layout. |
| Empty, one, many tasks | Correct viewport and no phantom selectable spacing rows. |
| Long titles and unbroken strings | Wrapping stays within title width and bounded row height. |
| Unicode and multiline input | Display-cell widths and line breaks do not overlap adjacent columns. |
| Narrow, normal, wide and short containers | Selection remains visible, including a row taller than the viewport. |
| Grouping, top list and detail pane | Headings and wrapped rows share correct viewport accounting. |
| Keyboard and paging | Navigation refers to tasks, not terminal line indices. |
| Pointer | N/A: the current TUI has no mouse event handling or click hit targets; this change does not introduce them. |
| Filter, sort, resize and repeated changes | Selection and scrolling recover deterministically. |
| Paint and live-work markers | Row styling covers all content lines and remains legible. |
| Scale and responsiveness | Layout work bounded by task input and capped title height. |
| Save failure and invalid external settings | Existing persistence failure policy remains intact; invalid settings rejected or explicitly defaulted at the boundary. |
| Remote lifecycle and permissions | N/A: this change introduces no remote requests or authority transitions. Existing PR behavior must remain unchanged. |
| Touch and browser zoom | N/A: native terminal cells and terminal input. Terminal resize is covered above. |

## Objective ledger

- Baseline: primary checkout clean; isolated branch `feat/sbt-row-layout` starts at `971fec72`.
- Board: TASK-214 created and claimed through `sb`.
- Implementation: delegated TUI code and test ownership; command owns integration, documentation and final evidence.
- Chosen control contract: existing Tasks settings menu cycles title wrapping through off, 2, 3, or 6 lines and row spacing through compact or one blank line. Settings use the existing view record. PR rendering stays compact.
- Independent review: final fresh-code review found no remaining correctness blocker. The requested combined grouping/detail paging regression was added. PR records containing unsupported layout remain protected from overwrite; task-only scope and promotion labels are explicit.
- Initial verification: `mise run preflight` passed on 2026-09-12: formatting, workspace clippy, 1,437 tests passed, 50 existing opt-in tests ignored, and developer hook/install/CI-routing checks passed. Log: `/tmp/sbt-row-layout-preflight-final.log`. Delivery review subsequently added a ninth row-layout integration regression for title-cap overflow. The extra private helper unit test was removed to follow the TUI's E2E-only test convention; the 250-task real-input navigation journey remains.
- Owner experimentation: isolated trial launcher provided; visual approval is not inferred from tests.

## Controls and persistence

On Tasks, press `,` to open settings, `w` to cycle wrapping (off, 2, 3, 6 lines), and `s` to toggle the blank separation line. Press Esc to return to the list. These are current-view changes; `v s` opens the existing save-view menu. Saved views and self-restart use the same `ViewState` record. Old records omit `title_lines` and `row_spacing` and keep compact defaults. Invalid values and unsupported PR layouts preserve their source files under the existing view repair policy.

Titles use the same ratatui paragraph wrapping for measurement and painting, with input capped at 4096 Unicode scalars and at most six rendered lines. Truncated wrapped titles show an ellipsis. The full detail text remains available. Metadata stays on the first line, selection and live-work styling cover content lines, and spacing is not selectable.

The local trial launcher is `tmp/try-sbt-row-layout.command`. It uses this linked worktree as the repo scope: central storage supplies the same Switchbard tasks while ordinary repo-view saves use the worktree's separate view path. This avoids putting new layout fields into the main checkout's view file while older installed sessions remain open. Global view promotion remains an explicit shared action through the existing menu.

## Evidence sources and limits

- `crates/switchbard-tui/tests/row_layout.rs`: real input, saved-view and resume journeys; narrow title wrapping and separation; invalid-file preservation; Unicode and unbroken strings; resize, empty list, 250-task navigation, grouped short viewport, detail paging, and selected continuation-line paint.
- `crates/switchbard-tui/src/list_presentation.rs`: independent code review verified the selected-minus-viewport lower bound limits height measurements to a viewport-sized suffix; scale behavior is exercised through the real terminal harness in `tests/row_layout.rs`.
- `SBT_ROW_LAYOUT_EVIDENCE=/tmp/sbt-row-layout-evidence mise exec -- cargo test -p switchbard-tui --test row_layout`: optional exports of actual TestBackend cell buffers for compact, wrapped/spaced and capped Unicode states. Command inspected PNG reconstructions of compact and wrapped/spaced buffers. These are fixture evidence, not live terminal screenshots or owner visual approval.
- Developer hook, install-guard and CI-routing contracts: passed via `bash scripts/test-developer-gates.sh`.
- Related fix: terminal text snapshots now skip wide-glyph continuation cells, which previously polluted `last_screen` with stale hidden characters. Actual terminal glyph rendering is unchanged.
- Validation correction during verification: mlua's integer conversion silently accepted fractions and numeric strings. The new exact-value boundary rejects these; malformed-value source preservation and valid integral-number compatibility pass on the final tree.
- Remaining limitation: no human terminal appearance approval yet. The local gate does not establish Linux CI or installation into existing live sessions.
