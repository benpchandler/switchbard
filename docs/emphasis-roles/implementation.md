# Emphasis Roles implementation and evidence ledger

## Objective

Make SBT easier to scan: distinguish navigation, view context, column headings, group headings and task content; emphasize important work without making every row loud; make formatting controls discoverable from the list. Existing task descriptions guide implementation but do not override this outcome.

## Scope and decisions

1. Theme-defined quiet, strong, alert, band and struck roles compose with palette tokens and legacy colors. Preserve saved views and most-specific-rule precedence, add stop-if-true. Fills are per cell: a rule composes one fill and the ink over it, any number of rules may carry a fill, and the one-band-per-view refusal is gone (TASK-237).
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
| Role combinations, specific overrides, stop-if-true, two fills on one cell | E2E commands, rendered cell styles, refused mutation status |
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
- `tests/emphasis_rules.rs`: composition, precedence, stop, coexisting fills and per-cell fill precedence, persistence, invalid rules, custom-field cleanup and legacy no-op compatibility.
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

## Integration follow-through (2026-09-15)

Owner feedback: the roles were not visible in the everyday app. Objective now includes integrating the installed feature lineage and delivering the combined build, with default scanning cues visible without paint setup. Merge 0423eb25 preserves installed sbt 94df3d04 and installed sb 37846fd0; guarded installer dry-run allows both with no dropped commits. The baseline worktree and primary checkout were clean. The installed detail sections/cancellation features are retained, and their tests join the emphasis matrix. No unrelated branches are included merely because they exist. Default cue review, combined gates, live dogfood and installed build readback are required before closeout. A feature install remains temporary under the auto-install hold; upstream merge is not claimed by a local install.

The default priority follow-through maps open High priority cells to alert, Low priority cells to quiet, and completed priority cells to quiet before explicit paint. Titles retain strong for open High and quiet+struck for completed tasks. The added real-app regression first reproduced the missing alert foreground, then all six hierarchy tests passed. Live binary 979a7499 was exercised against real task data in the isolated terminal: the combined high/low cues and expandable task details are visible together in `screens/integrated.png` and `screens/integrated-detail.png`, with raw ANSI companions. Visual Review target `integrated-0d08f7725e95`, revision `revision-1cf5a6b012859600202a`, owns this follow-through review. These images remain terminal-output rasterizations rather than OS-window captures.

Combined validation passed: `mise run ci` (`/tmp/sbt-emphasis-integrated-ci.log`), final TUI fmt/clippy and all 407 tests with 22 pre-existing ignored (`/tmp/sbt-emphasis-integrated-tui.log`). Guarded installation of both sbt and sb succeeded at 2026-09-15T22:22:41Z, revision b7657dbc, clean build identity and no dropped commits. Both installed binaries report that revision; three running sessions logged reload into it. Hold expires 2026-09-16T00:22:41Z. Visual Review has no unresolved annotations; reporter approval remains open. The branch is being prepared for upstream review so the temporary install is not confused with durable main delivery.

## Highlight slots and fill-and-ink rules (TASK-237)

Owner review of the installed build found the fill system too narrow to say
"pink highlight, red text for alert" or "these rows yellow, those blue": `band`
was the only fill, its color was fixed per preset, one rule per view could use
it, and no other role could set a background.

Decisions taken: highlight slots are explicit hex per preset (`theme.highlights`,
at least `h1`, `h2` and `h3` everywhere, warm paper fills on light, reverse video
and terminal colors on plain), with a computed fallback for a slot a theme omits
that moves the matching palette color to a fixed OKLCH lightness step from the
canvas (`highlight::derive_fill`, one named function). Any role may own a fill;
role validation refuses only a fill no ink in the theme can be read on, measured
with the shipped APCA implementation (`legibility.rs`) rather than a blanket ban
on backgrounds. `paint_eval::compose` is the single authority for which token in
a rule is the fill and which is ink, so `h2`, `h2+alert`, `band+red` and
`alert+p3` all resolve one way, and a fill is the fill wherever it is written in
the list. Fill conflicts between rules resolve exactly like ink conflicts, to the
lower and more specific rule.

Two consequences worth naming. The style picker lists the slots as swatches
between the roles and the colors, so the color numbers shift by the number of
slots; typing a color name or its unique prefix is unaffected. Palette tokens as
ink stay outside the preset contrast guarantee, as the guide already states: the
palette is chosen independently of the theme, and `darkroom`'s first hue is
deliberately close to its body text, so gating it on a fill would assert a claim
the project does not make.

Evidence: `tests/emphasis_rules.rs` covers the four grammar shapes on real cells,
coexistence on different scopes, most-specific-wins on one cell, a legacy `band`
view rendering unchanged, and save/reopen. `tests/emphasis_theme.rs` covers a
declared role fill reaching cells, an unreadable fill refused with a warning,
declared and derived slots per preset, plain's terminal-owned slot, and an
unknown slot name. `tests/emphasis_controls.rs` covers the picker journey: two
fills accepted where one was refused, swatches drawn in their own fill and ink,
and a typed `h2+alert` previewed and applied. `tests/legibility.rs` gates every
role ink over every preset fill, unselected and selected, and the owner's pink
fill with alert ink at rest, under the cursor, through a working pulse and at its
steady peak.

## Two-step style picker (TASK-245)

The owner tried the highlight-slot check on installed main 5a9e9ae8 and found
the combination unreachable: one row of the style picker applied one token, so
`h3+alert` existed only as typed text. The picker now asks for the highlight
first and the text second.

Decisions: the draft being composed (`App::paint_draft`) is the one authority
while either step is open, so the purposes carry no state and `←` keeps meaning
"back a step" rather than "undo a toggle". `paint_eval::compose_text` writes the
rule and is the inverse of `compose`, which reads it, so the picker can only
produce grammar `:paint` already parses and a view already saves. Reopening a
scope seeds the draft from the rule it wears, split by the theme that composed
it, which is also what marks the rows. Space is additive on the text step only;
`keep default ink` is the absence of ink and clears what was gathered, so
`none` then `keep default ink` is how a rule is removed. A rule typed in full
lands whole at either step.

One deviation from the brief, deliberately: step one lists `none`, `band` and
the highlight slots, not the named colors. A bare color is ink in the saved
grammar, and `compose` classifies a token by the style behind it, so offering
"red as a fill" would need either a new token, which is reserved, or a
positional reinterpretation that would change how existing saved rules such as
`struck+red` render. The nine slots cover colored fills instead: a theme with a
canvas derives the six it does not declare from the palette, so the step offers
ten fills without touching the grammar.

The picker's color numbering returns to what it was before TASK-237, because
the colors now sit on their own step with no swatches above them: `12` is
lightblue again, `5` is magenta.

Review caught three things after the first pass. The stop marker was being
dropped on a round trip, because `compose` strips it before the split and
`set_rule` replaces a rule wholesale: the draft now carries it and
`compose_text` writes it back, so restyling `column:title=green!` keeps the
`!` and the rules above it keep being stopped. Picking a text style now
replaces ink that merely came from the existing rule while still joining ink
gathered with Space, so restyling green to red writes `red` rather than
`green+red`. The ink cap reports itself on the status line instead of
swallowing the token, and the row preview borrows the drafted ink rather than
cloning it per row per frame.

Evidence: `tests/emphasis_controls.rs` covers the two-step journey end to end
(highlight, then text, producing `h3+alert` and rendering fill and ink), the
additive text step (`strong` and `p2` gathered with Space and previewed before
Enter), Esc at either step, `←` back to a still-marked fill, a rule typed in
full at either step, and both steps at 40 columns, plus a stopped rule restyled without losing its
marker and the ink cap reporting itself. `tests/legibility.rs` gates all ten
fills the step now offers, the six derived ones included. Every existing paint
journey was updated to walk through the highlight step and still passes.
