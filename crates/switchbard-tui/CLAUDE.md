# sbt - switchbard terminal UI

Binary `sbt` (this crate). Run in a backlog repo: `sbt`, `sbt stats`, `sbt paths`.
Install for the user's tab: `cargo install --path crates/switchbard-tui`.

## Standing commitments (owner-set, 2026-09-02)
1. Everything the user might tune lives in Lua (`~/.switchbard/tui.lua`, hot reload).
   New feature => new config surface only if a user would plausibly change it.
2. Fast and findable: no action more than two keystrokes away; `?` must list it.
3. `:bug` / `:idea` work from anywhere and carry screen + action trail automatically.
4. Tests are E2E only (`tests/*.rs` by feature over `tests/harness/`): real key events,
   real backlog on disk, assert on rendered screen. No unit tests, no mocks.
5. Self-documenting names over comments. One module per concept.
6. Minimalism: cut what telemetry shows unused. Prove usefulness before adding.
7. This file stays under 50 lines and is updated every slice.
8. Telemetry (`~/.switchbard/tui-events.jsonl`) records key, action, timing, error.
   `sbt stats` is how we learn what is used, slow, or unbound.

## Module map
- `app/` - `mod.rs` state, loop, browse keys, commands; `pickers.rs` column/filter/sort
  pickers + the shared picker key handler; `paint_flow.rs` the `p` flow; `slots.rs` the `v` chords.
- `picker.rs` - the one list every menu uses: typed `PickOption` payloads, numbered and
  lettered rows, type-ahead; `app/` dispatches on payloads, never label text. A digit in
  browse opens `ColumnActions` for that header position: filter/sort/paint/glyphs/hide/move.
- `view.rs` - rendering only (the table is hand-drawn so headings span the row); snapshots the screen text for reports.
- `columns.rs` - the column catalog: one `ColumnSpec` row per column (name, header, width,
  field, vocabulary) plus `values`/`cell_text`; every other module asks it.
- `config.rs` - `default.lua` baked in, user file layered over; keys, theme, glyphs.
  Columns (shown set, order, glyph mode via `g`) are view state; numbers are positions.
- `tasks.rs` - task load + filter language (`status: pri: label: project:` + words,
  loose match: `status:todo` == "To Do"); `field_values` feeds the `f <n>` picker.
- `views.rs` - `ViewState` (filter, sort, columns, glyphs, paint, group) is what a slot saves and
  a restart resumes, one Lua record for both; global `~/.switchbard/views.lua`, per-repo overrides
  in `views/<repo path>.lua`; `vs<n>` saves to repo, `vg<n>` promotes to global; slot 1 opens.
- `paint.rs` - `p` rules in a hierarchy: `by:<col>=v:c,...`, `rows:<filter>=c`, `column:<col>=c`.
  Top rule is the base (whole rows); lower rules paint only their scope. `po` reorders.
- `group.rs` - `o`: rows = headings + tasks, a projection over the filtered, sorted order;
  project headings carry def status and done/total from `tasks::ProjectSummary`.
- `ball.rs` - who holds the ball: `ball:me`/`ball:agent` labels (`dispatching` = agent); `b` cycles.
- `sort.rs` - `s <n>`: ascending/descending/semantic (vocabulary rank), ties by id.
- `report.rs` - `:bug`/`:idea` => task via core write layer. `telemetry.rs` - JSONL log, trail, `stats`.

## Loop
Slice => commit on `feat/tui` => `cargo install` => the running sbt re-execs itself
(`main.rs::InstalledBinary`, resumes view/filter/row) => user drives it => drain
`label:tui` tasks (`v4` in starter views) at the start of the next slice.

## Gates
Per slice: `mise run tui-install` (fmt, clippy, tests for this crate only, then install).
Never run `mise run ci` mid-loop: its RUSTFLAGS differ from cargo install, so the
whole workspace including the GUI rebuilds. Run it once before merge.
