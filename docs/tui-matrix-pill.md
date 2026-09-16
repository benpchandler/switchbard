# Matrix pill progress

The Progress column keeps its `%` header and its place beside Status. Its default appearance is a compact red capsule that fills from left to right using the existing retained-criterion rollup. The same column identity updates existing layouts that already show Progress; no view or task-data migration is needed.

The capsule occupies six terminal cells: rounded ends and four body cells. Partial blocks provide finer progress than whole-cell steps. Zero criteria remain a dash, zero checked criteria show an unfilled capsule, and only an exactly complete checklist shows a fully filled capsule. A small positive ratio remains visibly positive. The detail pane and Checklist retain exact counts and percentages. Done remains manual, including the Review signal for an open task at 100%.

The terminal appearance uses rounded Powerline glyphs supported by the owner's configured Nerd Font. A plain-text fallback is available for other fonts. Fill, empty body and shell colors belong to the existing Lua theme system; selected and working-row backgrounds remain visible around the capsule.

## State and stress evidence

| State or invariant | Verification |
| --- | --- |
| No criteria, zero, tiny positive, 12.5%, half, high, near-full, full | Real task fixture, production rendering and endpoint assertions |
| Open at 100%; Done with unchecked criteria | Status unchanged; exact detail/Checklist remains authoritative |
| Task and descendant criteria; canceled branches | Existing cached core rollup, no replacement calculation |
| Selected, unselected, working rows | Styled buffer evidence; row background preserved |
| Dark, light and monochrome themes | Rendered theme checks; non-color fill distinction |
| Existing saved layouts and custom progress glyphs | Existing column identity and configuration compatibility checks |
| Normal, wide, narrow, short, zero-sized and detail layouts | Production table rendering and clipping checks |
| Sorting and history previews | Same numeric sort and shared table renderer |
| Loading, saving, retry, conflict and permission writes | N/A: this is a read-only rendering change over the existing snapshot |
| Rendering cost | Fixed six-cell rendering and cached rollup lookup; no I/O |

## Configuration and evidence

Set `progress_style = "pill"` (default), `"ascii"`, or `"icons"` in `~/.switchbard/tui.lua`. Existing explicit `glyphs.progress` overrides retain icons unless `progress_style` is set explicitly. Theme surfaces `progress_fill`, `progress_empty`, and `progress_shell` control the capsule colors. At less than six available cells, the renderer uses a compact icon, or an ASCII marker in ASCII mode, instead of clipping a capsule.

[Normal capsule](evidence/matrix-pill/progress-100x20.png), [light theme](evidence/matrix-pill/pill-light-selected.png), [monochrome](evidence/matrix-pill/pill-plain-selected.png), and [ASCII working row](evidence/matrix-pill/pill-ascii-working.png) show production Ratatui buffers rasterized with VictorMono Nerd Font, matching the configured terminal font. These are fixture renders, not native terminal screenshots or owner approval. Font substitution and other terminal emulators remain outside this evidence.

Behavior is exercised through real task fixtures, configuration reloads, keys, and production rendering in `crates/switchbard-tui/tests/progress_icons.rs`. Reproduce the buffers with `SBT_PROGRESS_EVIDENCE_DIR=/tmp/switchbard-progress-evidence mise exec -- cargo test -p switchbard-tui --test progress_icons -- --nocapture`.
