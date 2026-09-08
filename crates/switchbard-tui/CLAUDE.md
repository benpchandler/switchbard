# sbt - switchbard terminal UI
Binary `sbt` (this crate). Run in a backlog repo: `sbt`, `sbt stats`, `sbt paths`. Install: `cargo install --path crates/switchbard-tui`.
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
- `app/pr_merge.rs` - m prepares merge off-thread; shared confirmation picker defaults to Cancel, digits explicitly confirm a method. Full repo/PR/head/base/viewer must fit before confirming. Pending submits block duplicates, quit and reexec; page switches retain result alerts and core receipt links.
- `pr_notifications.rs` - bounded session PR change/availability alerts across pages; n dismisses latest. After first PR visit, refresh continues on Tasks. O opens selected PR in the browser.
- `page.rs` - Tasks / Pull Requests identity and allowed actions; Tab (`page` in Lua) toggles, the header marks the active page. `pull_requests.rs` caches bounded repo reads off-thread; `pr_view.rs` renders the list/detail. PR refresh uses `pr_refresh_seconds` (default 60): the first observation row counts down, shows refreshing in flight, and restarts on completion (including failure); r retries immediately. The source includes Open/Closed/Merged, initially 100 rows; `:more` expands by 100 up to 1000 with explicit partial coverage. `/`, `f`, `s`, `p`, `c`, numbered headers and `v` reuse shared controls with independent PR state; `status:`/`lifecycle:`, `tasks:`, `checks:`, `review:`, `merge:`, `draft:` and `title:` read cached PR fields. Sorting retains selected identity; painting never writes PRs; Enter toggles the right detail pane, j/k select rows, Ctrl-d/u scroll details. Metadata survives optional active-check enrichment failures. State/Tasks/Checks use compact widths; Tasks shows an ID or link count, absent links a dash. Checks shows only check observations (closed/merged: NF, expanded to Not fetched in details); review and merge observations remain separately labeled in details.
- `app/` - `mod.rs` state, loop, browse keys, commands; `pickers.rs` column/filter/sort
  pickers + the shared picker key handler; `new_task.rs` the Tasks `t n` title draft and native create; `task_parent.rs` the `t a` existing-parent picker (ID/title search, native reparent); `task_status.rs` / `task_project.rs` the `t s` status and `t p` project native edits with stable task IDs; `paint_flow.rs` the `p` flow; `slots.rs` the shared `v` open/save/global pickers.
- `picker.rs` - the one list every menu uses: typed `PickOption` payloads, numbered/lettered rows,
  type-ahead; `app/` dispatches on payloads. Task/view menus and column/settings/paint-rule actions are selectable rows; mutation menus require Enter after type-ahead. A bounded parent stack supports ←/h back and →/l open, with Esc closing all; filter-value initials h/l keep typeahead precedence when matching a label (arrows always navigate); merge confirmation retains explicit authorization. Short pickers scroll to selection; results stay in the footer. A digit in browse opens that column's `ColumnActions`.
- `detail_pane.rs` - shared task/PR split, border, bold title, muted metadata, accent sections, wrapping and bounded scroll.
- `view.rs` - rendering only (the table is hand-drawn so headings span the row); snapshots the screen text for reports.
- `columns.rs` - the column catalog: one `ColumnSpec` row per column (name, header, width,
  field, vocabulary) plus `values`/`cell_text`; every other module asks it.
- `config.rs` - `default.lua` baked in, user file layered over; keys, theme, glyphs, palettes.
  Theme = named `Surface`s (title chip, header band, heading, selected, label, link, chip,
  keys, hint...) each a Style (fg/bg/modifiers) and `theme.columns` maps columns to surfaces.
- `tasks.rs` - task load + filter language (`status: pri: label: project:` + words,
  loose match: `status:todo` == "To Do"); `field_values` feeds the `f <n>` picker.
- `views.rs` - `ViewState` (filter, sort, columns, glyphs, paint, group) is what a slot saves and
  a restart resumes, one Lua record for both; global `~/.switchbard/views.lua`, per-repo overrides
  in `views/<repo path>.lua`; PRs use `.prs.lua` beside these files. `vs<n>` saves to repo, `vg<n>` promotes to global; slot 1 opens. Both page states survive Tab/restart.
- `paint.rs` - `p` rules in a hierarchy: `by:<col>=v:c,...`, `rows:<filter>=c`, `column:<col>=c`.
  Top rule is the base (whole rows); lower rules paint only their scope. `po` reorders.
- `group.rs` - `Grouping` (0-2 nested levels, `project›goal`): `o` picks it; headings over the filtered, sorted order carry project def status, done/total, or goal week actual/target, pace.
- `ball.rs` - who holds the ball: `ball:me`/`ball:agent` labels (`dispatching` = agent); `b` cycles. `tg` / `:goal <name>`: attach or detach the task to a goal.
- Live work (core `work_sessions`, TASK-150): `work` column = one ● per live session (`sb work claim`), the `working` band and the row's text pulse on a soft-clipped sine (`work.period_ms`, `work.frames`, `work.flatten`), title counts `working:N`, detail names sessions; `w` passes the task (ends every claim).
- Top list = core's expedite lane: `tr` picks position, adds to end, or removes; `t<n>`/`tt`/Delete remain aliases. `vp` shows or hides the section; `#` column.
- `settings.rs` - `,` panel: hide statuses everywhere; per-repo file, `g` promotes to global.
- `report.rs` - `:bug`/`:idea` => task via core write layer. `telemetry.rs` - JSONL log, trail, `stats`.
## Loop
Slice => commit on a `feat/tui-*` branch => `mise run tui-install` => the running sbt re-execs
itself (`main.rs::InstalledBinary`, resumes view/filter/row) => user drives it => drain `label:tui`.
## Gates
Per slice: `mise run tui-install` (fmt, clippy, tests for this crate only, then install).
Never run `mise run ci` mid-loop: its RUSTFLAGS differ from cargo install, so the
whole workspace including the GUI rebuilds. Run it once before merge.
