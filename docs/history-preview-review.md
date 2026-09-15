# Recognizable history cards

## Acceptance contract

Each visible history entry owns a compact card containing its plain-language title, relative time and miniature table. At 120 columns by 30 rows, at least two different entries and their table contents must be visible simultaneously. Selecting each entry to see its table is not an acceptable substitute. Earlier renders of a list plus one selected preview are superseded.

Each miniature applies the saved columns, filter, sorting, grouping and paint to current cached data. It is labeled as current data and does not imply a historical row snapshot. Only visible cards are rendered, with a small bounded table in each. The existing terminal theme and production row renderers are the design source.

Arrow keys select cards; Page Up/Down moves through cards; Enter restores the selected payload; Esc preserves the active view. Merely navigating or rendering cards must not mutate the active view, selection, saved slots or checkpoint files.

## State and stress matrix

| State | Required evidence |
| --- | --- |
| Two or more entries at 120x30 and 100x24 | Independent table headings and contents appear simultaneously; selected and unselected cards both have miniatures. |
| One entry | One compact card; no duplicated selected-preview region. |
| Many entries | Card paging keeps selection visible; only viewport cards render. |
| Narrow, short and tiny terminal | Selection and restore remain usable; one compact card where two cannot fit. |
| Empty history and unmatched search | Explicit history-level empty state. |
| Empty current rows | Empty message belongs to that entry's card. |
| PR loading, unavailable, stale or truncated cache | Honest status and partial-data disclosure; no fetch or writes from rendering. |
| Different grouping, columns and paints | Each card independently applies its own saved settings using the current palette. |
| Navigation, cancel and restore | Active state stays unchanged until Enter; correct selected payload is restored. |
| Historical record snapshots, network writes and role changes | N/A: cards show existing current data using saved presentation settings. |

## Verification and delivery

The real-key regression first reproduced the mismatch: multiple entries were listed, but only one table was visible. Correction evidence uses real App journeys, real temporary backlogs, and a separately invoked authenticated read-only PR journey. Visual captures come from the complete terminal buffer, preserving cell symbols, colors and modifiers.

The initial primary-checkout edits in app/pickers.rs, view.rs and tests/filter.rs remain excluded. Work is isolated on feat/tui-history-preview. Installation identity and final gate results are recorded in TASK-154. Visual approval is distinct from automated checks and installation.

Nine focused E2Es passed, including authenticated PR cards, simultaneous populated miniatures, navigation invariance, short screens, long Unicode titles, relative-time visibility and theme colors. Four nested grouping levels collapse into a breadcrumb so headings cannot crowd all task data out of a miniature. Full-size tables retain their original grouping rows.

Rendered terminal evidence is available in Visual Review targets `tasks-cards-c9473b60bebc`, `pull-requests-cards-779b38f20f9c` and `tasks-nested-card-3927c26f0196`, owned by this worktree. The corrected renders show independent populated tables and readable unselected titles. Captured terminal buffers and PNGs are under `crates/switchbard-tui/tmp/history-cards/`.

Rendering is bounded to viewport cards, but each visible card still computes its full current-data projection. Large-backlog performance has not been benchmarked. The captured terminal renders were reviewed and approved; installation, CI and merge remain separate delivery gates.
