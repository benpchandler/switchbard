# Task progress icons

Progress is a compact, read-only column beside Status in new default task views. It uses the same cached acceptance-criteria rollup as Checklist: each retained criterion on the task or its descendants has equal weight, and canceled branches are excluded. It does not estimate time or effort.

| Icon | Criteria checked |
| --- | --- |
| `-` | No criteria to measure |
| `○` | 0% |
| `◔` | More than 0%, less than 34% |
| `◑` | 34% to less than 67% |
| `◕` | 67% to less than 100% |
| `●` | Exactly 100% |

The filled circle says every retained criterion is checked. Done remains an explicit execution status. An open task at 100% still shows Review in Checklist and the task details; a Done task with unchecked criteria retains its actual progress. Small positive ratios do not become zero, and incomplete ratios do not become full through rounding.

Existing saved views keep their column selections and order. Add Progress through `c` and the Progress entry, then move it next to Status using the existing column-order controls. Exact counts and percentages remain available in Checklist and the detail pane. Icon mappings live in the normal `glyphs.progress` Lua configuration, keyed by `unmeasured`, `empty`, `low`, `medium`, `high` and `complete`. The compact table header is `%`; the picker calls it Progress.

## State and stress contract

| State or invariant | Evidence |
| --- | --- |
| Unmeasured, zero, partial boundaries, tiny positive ratios, near-full and full | Real task fixture and rendered Progress cells |
| Manual Done independent from checked criteria | Open full and Done incomplete fixture rows; Review remains in details |
| Parent and descendant criteria, canceled branches | Existing core rollup consumed by rendered cells |
| Default position immediately after Status | Fresh-view E2E render and column order |
| Existing saved views unchanged | Old saved arrangement reload |
| Add, move, hide, save and reopen; two-digit column position | Existing column controls with real key events; `10 %` header remains visible |
| Lua overrides | Hot-reloaded custom glyph rendering |
| Narrow, normal, wide and short terminal; detail pane | Real Ratatui container rendering |
| Empty filter, long titles, selection | Rendered fixtures and navigation |
| Loading, saving, retry, write conflict, permission transitions | N/A: this column adds no storage or network operation; it displays the existing loaded snapshot |
| Render cost | Cached lookup and bounded icon selection per cell; no per-frame filesystem or database access |

Behavioral evidence: `crates/switchbard-tui/tests/progress_icons.rs` executes real task writes in isolated fixtures, key events and production rendering. It covers percentage boundaries, tiny positive and near-full ratios, numeric sorting, descendant/canceled aggregation, manual status independence, saved-view controls and Lua glyph overrides. The existing `columns` and `planning_progress` suites remain part of validation.

[Normal render](evidence/progress-icons/progress-100x20.png), [wide render](evidence/progress-icons/progress-140x24.png), [narrow render](evidence/progress-icons/progress-60x12.png), [short render](evidence/progress-icons/progress-40x8.png), and [detail render](evidence/progress-icons/progress-detail.png) are Menlo rasterizations of the real Ratatui test buffers, with corresponding text files alongside them. The 40-column view necessarily abbreviates headers and content; the Progress icon remains visible. These are fixture renders, not native OS screenshots or owner visual approval.

Reproduce buffers with `SBT_PROGRESS_EVIDENCE_DIR=/tmp/switchbard-progress-evidence mise exec -- cargo test -p switchbard-tui --test progress_icons -- --nocapture`. Automated render checks do not claim coverage of every terminal font.

Independent review also identified an existing numbered-header width assumption: fixed columns allowed only one digit for their position. Width calculation now includes the actual position, so moving Progress to column 10 retains the `%` label.
