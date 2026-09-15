# Emphasis Roles implementation and evidence ledger

## Objective

Make SBT easier to scan: distinguish navigation, view context, column headings, group headings and task content; emphasize important work without making every row loud; make formatting controls discoverable from the list. Existing task descriptions guide implementation but do not override this outcome.

## Scope and decisions

1. Theme-defined quiet, strong, alert, band and struck roles compose with palette tokens and legacy colors. Preserve saved views and most-specific-rule precedence, add stop-if-true, validate one rule-owned band.
2. Improve default header weight, active-tab identity, filter context and active sort affordance. Keep factual labels and glyphs so color is supplementary.
3. Extend painting to title, header and group headings. Offer selected-row context and its grouping values through the existing paint menu, plus keyboard access to any displayed column value. Do not invent a second navigation model that makes ordinary task browsing slower.
4. Declare preset backgrounds, check rendered legibility, support a light preset, and keep working indicators visible through their pulse.
5. Verify real keyboard journeys and rendered states, then dogfood the actual binary against this repository.

Continuous gradients in TASK-219 remain outside this slice: the canonical trajectory marks them speculative and no concrete scanning decision needs them yet. The light preset is independently useful and included. Task requests for unit tests are implemented as real-app E2E tests per the TUI contract. Reporter-confirmation criteria remain a human review gate and will not be self-certified.

## State and stress matrix

| Dimension | Required evidence |
| --- | --- |
| Default, active selection, grouped and flat lists | Real app buffer tests and terminal dogfood |
| Empty, one, many, long/wrapped and Unicode titles | Existing list stress suite plus emphasis render coverage |
| Tasks and PRs; read-only PR facts; navigation badges | Rendered keyboard journeys; legacy page/navigation tests |
| Role combinations, specific overrides, stop-if-true, duplicate band | E2E commands, rendered cell styles, refused mutation status |
| Existing literal colors and palette-token views | Existing paint/persistence suite plus role save/reopen tests |
| Invalid role/config/saved rule, external view edits | Warning and persistence tests, no silent overwrite |
| Narrow 40x8, normal 100x24, wide 160x40, zero-sized | Real terminal buffers; clipping/no-panic checks |
| Keyboard-only, repeated paint, cancel/back, page changes | Real key events; current focus and selection preserved |
| Working pulse trough/peak and motion disabled | Rendered working rows at controlled phases |
| Colored presets and light; plain terminal colors | APCA rendered-pair gate, plain explicitly exempt |
| Remote loading/stale/failure | Existing PR state suite; emphasis does not infer remote facts |
| Saving/reload/conflict | Saved view/persistence tests |
| Permission/access removal | N/A to local formatting; no new authority or remote mutations |
| Touch/browser zoom | N/A to terminal UI; terminal grid resizing covers container changes |
| Performance | Existing bounded rendering; full TUI suite and real navigation responsiveness |

## Authorization and boundaries

Implementation authorized. No merge requested. Worktree: `feat/tui-emphasis-roles`, branched from origin/main at d50c2281. Primary checkout baseline: modified `crates/switchbard-core/src/agent_sessions.rs` and `session_title.rs`; neither is in this work's lease. Running owner app is shared; dogfood uses a separate tmux server and no owner session restart. Existing installed feature build has a hold; installation must preserve lineage and must not force a downgrade.

## Evidence status

Baseline: isolated `tmux -L sbt-emphasis` session using installed sbt, real repository at 140x36. Confirmed dense undifferentiated second-row metadata, low header distinction, and identical badge/active-tab fill. Candidate: isolated copied binary `/tmp/sbt-emphasis-dogfood`, real repository data, 140x36 and 40x8. Keyboard journeys exercised grouped tasks, two-line titles, Tasks/PR navigation, paint menu, `strong+p2` composition, second-band refusal with existing paint retained, clear paint, light theme and popup rendering. The PR page displayed cached observation age and PARTIAL coverage honestly; no PR writes were performed. Concurrent isolated sessions produced a view-history conflict; stale sessions were closed and the view reopened without forcing an overwrite.

Live use exposed a light-theme popup canvas bug: terminal Clear reset the menu to the dark terminal background. Picker and history popups now restore the declared canvas. Independent review also caught custom-role background bypass, incomplete title-scope coverage and mistaken removal of literal group names containing `field:`. All were fixed with real-app regression coverage. Legacy empty categorical rules and blank semicolon segments remain loadable and saveable.

### Evidence map

- `tests/emphasis_hierarchy.rs`: priority/completion hierarchy without color alone, active tab versus badge, sort underline, raw group values, title scopes on both pages, light popup/history canvas; terminal grids 0x0, 1x1, 40x8, 100x24 and 160x40, long Unicode titles and empty filters.
- `tests/emphasis_rules.rs`: composition, precedence, stop, singleton band, persistence, invalid rules, custom-field cleanup and legacy no-op compatibility.
- `tests/emphasis_controls.rs`: actual paint key journeys, context choices, cancellation and atomic refusal.
- `tests/emphasis_theme.rs` and `tests/legibility.rs`: configuration diagnostics, fallback roles, preset rendering, published APCA reference pairs, detail text, selection, working trough/peak and disabled motion.
- Existing full-suite coverage exercises navigation, PR data states, reload, history, custom fields, filter/column controls and persistence. The PTY signal/resume driver now drains rendered output while waiting for exit so terminal backpressure cannot create a false shutdown timeout.
- `screens/baseline.png`, `darkroom.png`, `roles.png`, `light-menu.png`, `narrow.png`, `pr-light.png`: visual evidence. These are Menlo rasterizations of captured real terminal ANSI, with sibling `.ansi` originals, not native OS-window screenshots. Baseline and candidate reflect different task status snapshots because implementation claims changed task states.

### Checks and limits

`mise run ci` passed across the workspace on macOS (full log `/tmp/sbt-emphasis-ci.log`). Final TUI formatting, clippy and complete test results are recorded in the delivery closeout below. A human visual review is available through Visual Review target `darkroom-102a644d9dc6`, revision `revision-38021a1c51c10c12ad8e`; no unresolved annotations were present at the final check. That is not reporter approval.

Remaining limits: Linux CI has not run for this unpushed branch; terminal emulator/font differences have not been visually audited; plain and user-supplied colors intentionally have no numeric contrast guarantee. Performance evidence is responsive live navigation plus bounded rendering and existing regression coverage, not a benchmark. Continuous scales are deferred. Human reporter confirmation remains open.

The owner installation at `94df3d04` belongs to a different feature lineage and has an install hold. It was not replaced. Only owned isolated tmux sessions were used. No merge or deployment is claimed.

### Delivery closeout

Final source passed `cargo fmt --all --check`, `cargo clippy -p switchbard-tui --all-targets -- -D warnings`, and `cargo test -p switchbard-tui`: 393 passed, 0 failed, 22 pre-existing ignored tests. Final test log: `/tmp/sbt-emphasis-final-tests.log`. Independent review rechecked the three original findings and found no remaining blocker in those scopes. Tasks 215-218 and 175-177/200 are In Review, work claims released; task 219 retains the deferred scale criterion and its light-preset criterion is checked. Primary checkout was clean at closeout; its initial core edits were not touched by this work.
