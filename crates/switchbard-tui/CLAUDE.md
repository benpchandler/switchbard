# sbt - switchbard terminal UI

Binary `sbt` (this crate). Run in a backlog repo: `sbt`, `sbt stats`, `sbt paths`.
Install for the user's tab: `cargo install --path crates/switchbard-tui`.

## Standing commitments (owner-set, 2026-09-02)
1. Everything the user might tune lives in Lua (`~/.switchbard/tui.lua`, hot reload).
   New feature => new config surface only if a user would plausibly change it.
2. Fast and findable: no action more than two keystrokes away; `?` must list it.
3. `:bug` / `:idea` work from anywhere and carry screen + action trail automatically.
4. Tests are E2E only (`tests/drive.rs`): real key events, real backlog on disk,
   assert on rendered screen. No unit tests, no mocks.
5. Self-documenting names over comments. One module per concept.
6. Minimalism: cut what telemetry shows unused. Prove usefulness before adding.
7. This file stays under 50 lines and is updated every slice.
8. Telemetry (`~/.switchbard/tui-events.jsonl`) records key, action, timing, error.
   `sbt stats` is how we learn what is used, slow, or unbound.

## Module map
- `app.rs` - state + the one key-handling path (`handle_key`, `apply`, `run_command`).
- `view.rs` - rendering only; also snapshots the screen text for reports.
- `config.rs` - `default.lua` baked in, user file layered over; keys/theme/columns/views.
- `tasks.rs` - task load + filter language (`status: pri: label: project:` + words,
  loose match: `status:todo` == "To Do"); `field_values` feeds the `f <n>` picker.
- `views.rs` - slots of filter+sort: global `~/.switchbard/views.lua`, per-repo overrides in
  `views/<repo path>.lua`; `vs<n>` saves to repo, `vg<n>` promotes to global; slot 1 opens.
- `sort.rs` - `s <n>` orders: ascending/descending/semantic (vocabulary rank), ties by id.
- `report.rs` - `:bug`/`:idea` => task via `switchbard-core` write layer.
- `telemetry.rs` - JSONL event log, in-memory trail, `stats`.

## Loop
Slice => commit on `feat/tui` => `cargo install` => the running sbt re-execs itself
(`main.rs::InstalledBinary`, resumes view/filter/row) => user drives it => drain
`label:tui` tasks (`v4` in starter views) at the start of the next slice.

## Gates
Per slice: `mise run tui-install` (fmt, clippy, tests for this crate only, then install).
Never run `mise run ci` mid-loop: its RUSTFLAGS differ from cargo install, so the
whole workspace including the GUI rebuilds. Run it once before merge.
