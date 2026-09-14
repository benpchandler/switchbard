# Editable task detail pane

## Objective and acceptance

TASK-222: the sbt task detail pane becomes a focusable, editable surface for
structured fields (title, status, priority, project, due date, labels,
acceptance-criterion checkmarks) without leaving the terminal, using the same
picker/single-line-input vocabulary the list already has. Multiline prose
(description body, implementation plan, acceptance-criterion text) is
explicitly out of scope — a textarea or `$EDITOR` handoff is a separate,
undecided surface. The owner authorized implementation on 2026-09-13.

## Design

- A second `open` gesture (Enter by default, or `l`/`Right`) on an
  already-open pane moves the cursor into it (`Mode::DetailFocus`); a first
  `back`/`Esc`/`h`/`Left` returns focus to the list, a second closes the
  pane. `j`/`k` move the cursor; `Enter`/`l`/`Right` opens the row's editor;
  `Space` toggles an acceptance row.
- Row order is fixed: title, status, priority, project, due date, labels,
  then a read-only description hint ("edit with `sb edit`"), then one row
  per acceptance item, then the read-only blocked-by/blocks lists — one
  function, `detail_pane::field_rows`, builds this list and both the cursor
  logic (`app::detail_edit`) and the renderer (`view::draw_detail`) walk it,
  so they cannot disagree about what row N is.
- Status/priority/project/labels open the existing picker vocabulary under
  new purposes (`PickerPurpose::Detail{Status,Priority,Project,Labels}`)
  that return focus to `Mode::DetailFocus` on close instead of `Mode::Browse`;
  title and due date open a single-line capture (`Mode::DetailInput`), the
  same shape `Mode::NewTask` already uses. Labels is a multi-select panel
  (toggle-and-reopen, the same shape `t g` already uses for goals) plus a
  keyed row that opens a "new label" capture.
- Every save is gated by a byte-for-byte compare against the file read when
  the row's editor opened (`app::detail_edit::DetailDraft`), then goes
  through `switchbard_core::edit_backlog_task_expected` (fields) or
  `set_backlog_acceptance_checked` (checkmarks). This is the "or equivalent
  stale-check" the task named: `edit_backlog_task_expected`'s own revision
  guard only fires for a centrally-stored task (see its doc comment) and
  cannot see a plain-file edit — the shape every `tests/harness` fixture is,
  since no test stands up a central store outside `tests/central_storage.rs`'s
  own subprocess isolation. The content compare is defense in depth on top
  of, not instead of, that guard.
- A read-only task (`BacklogTask::editable()` false — any source but
  `Active`, e.g. a task migrated into `backlog/drafts`) renders every field
  but refuses every edit with a `"<id> is read-only"` status message.

## State and stress matrix

| Dimension | Required evidence | Test(s) |
| --- | --- | --- |
| Default (pane closed) | Opening the app shows no detail pane. | Implicit in every test below (`Harness::new()` starts with `pane: Pane::None`); also `crates/switchbard-tui/tests/browse.rs::lists_every_task_with_repo_name_and_count`. |
| Open, unfocused | A first `open` shows fields without a cursor highlight; list keys still move the list. | `second_open_or_right_focuses_the_pane`, `filter_that_hides_the_selected_task_updates_the_unfocused_pane_live` |
| Focused | A second `open`/`l`/`Right` moves the cursor in; `j`/`k` move it; the cursor row is highlighted. | `second_open_or_right_focuses_the_pane`, `j_k_move_the_cursor_without_moving_list_selection`, `cursor_highlight_follows_j_and_k` |
| Editing input | Title/due-date/new-label capture prefills, types, and shows a two-line footer (draft + status). | `title_edit_prefills_saves_and_round_trips_non_ascii`, `due_date_validates_clears_and_rejects_garbage`, `labels_multi_select_toggles_and_a_new_label_can_be_added` |
| Saving success | Write lands, `reload_tasks` + reselect, status names the field, pane stays focused on the same row. | `title_edit_prefills_saves_and_round_trips_non_ascii`, `status_priority_and_project_pickers_write_through_the_native_layer`, `due_date_validates_clears_and_rejects_garbage`, `space_and_enter_toggle_an_acceptance_row` |
| Save failure (stale identity) | A file changed on disk between focus and save fails visibly with no write, for a typed field, a picker-driven field, and an acceptance toggle. | `stale_draft_on_disk_fails_the_save_and_keeps_the_typed_input`, `stale_draft_blocks_a_picker_driven_field_pick`, `stale_draft_blocks_an_acceptance_toggle_and_leaves_the_file_untouched` |
| Read-only task | Fields render, cursor moves, every edit attempt is refused with a status message and no write. | `read_only_task_shows_fields_but_refuses_every_edit` |
| Nothing selected | The pane says "nothing selected"; the focus gesture is a no-op. | `nothing_selected_makes_the_focus_gesture_a_no_op` |
| Zero acceptance items | No acceptance section renders (`field_rows` only extends past a task's own `acceptance_criteria.len()`). | Covered by every test against the 3-task fixture, whose seeded tasks carry exactly one acceptance item each, plus the zero-label fixtures in `zero_and_many_labels_render_as_none_or_a_joined_list`, which carry the same single-item shape; the many-item test below is the same code path at the other extreme, so both ends of `0..len()` are exercised. |
| Many (30+) acceptance items, cursor scrolls | A 35-item task's last row is reachable and visible after scrolling. | `many_acceptance_items_scroll_the_cursor_into_view` |
| Long unbroken title (500 chars) wraps, cursor still highlighted | A 500-character title with no whitespace renders and keeps its cursor highlight once wrapped. | `long_unbroken_title_wraps_and_stays_on_screen` |
| Zero labels | Renders `labels: (none)`. | `zero_and_many_labels_render_as_none_or_a_joined_list` |
| Many labels | Renders the full joined list. | `zero_and_many_labels_render_as_none_or_a_joined_list` |
| Empty project | Renders `project: (unassigned)` (and due date `(none)`). | `empty_project_and_due_date_show_placeholders` |
| Title with non-ASCII | Accented Latin and CJK round-trip through prefill, edit, and save. | `title_edit_prefills_saves_and_round_trips_non_ascii` |
| Container: 40x12 | Renders and accepts focus/navigation without panicking. | `three_terminal_sizes_render_the_focused_pane_without_panicking` |
| Container: 80x24 | Same. | `three_terminal_sizes_render_the_focused_pane_without_panicking` |
| Container: 180x50 | Same. | `three_terminal_sizes_render_the_focused_pane_without_panicking` |
| Cursor stays visible after resize | Shrinking the terminal after scrolling deep into a long list keeps the cursor row on screen. | `cursor_stays_visible_after_resize` |
| Edit then Esc keeps original value | Canceling a field capture leaves the stored value untouched. | `esc_during_an_edit_keeps_the_original_value` |
| List keys go to the pane while focused | `j`/`k` move the pane's cursor, not the table's selected row. | `j_k_move_the_cursor_without_moving_list_selection` |
| Switching page (Tab) closes pane focus and cancels input | From cursor-only focus, a mid-field capture, and an open Detail* picker. | `tab_while_focused_or_mid_edit_switches_page_and_cancels` |
| Filter that hides the selected task while pane focused | N/A as stated — see note below; the adjacent, reachable scenario is covered instead. | `filter_that_hides_the_selected_task_updates_the_unfocused_pane_live` |
| Failure: writer error surfaces in status and preserves input | A rejected due date and a stale save both keep the typed draft and show the error. | `due_date_validates_clears_and_rejects_garbage`, `stale_draft_on_disk_fails_the_save_and_keeps_the_typed_input` |
| Cancel a label picker's own capture returns to the picker | Esc on a "new label" draft goes back to the labels panel, the same place a successful add already reopens into — not out to plain cursor focus. | `esc_from_a_new_label_capture_returns_to_the_labels_picker` |

### N/A note: "filter that hides the selected task while pane focused"

Filtering (`/`) is only reachable from `Mode::Browse`; `Mode::DetailFocus`,
`Mode::DetailInput`, and the detail pane's own pickers each have their own
key handler that does not forward `/` to filter mode (the same way none of
them forward arbitrary letters to command mode or column actions — this
mirrors the existing `Mode::PickValue` precedent, which does not fall through
to filtering either). Reaching Filter mode therefore first requires leaving
focus via `Esc`/`h`/`Left`, at which point the pane is open but unfocused —
the literally-focused variant of this scenario cannot occur. The unfocused
variant is real and covered: `filter_that_hides_the_selected_task_updates_the_unfocused_pane_live`
shows the pane always renders `app.selected_task()` live, so a filter change
that moves selection elsewhere (or to nothing) is reflected on the very next
frame with no extra bookkeeping. Separately, a *reload* (not a filter change)
racing an open Detail* picker or `Mode::DetailFocus`/`DetailInput` while the
selected task disappears entirely is handled by extending the same
cancel-on-disappearance branch `reload_tasks` already runs for `TaskStatus`/
`TaskProject`/etc.

## Evidence sources and limits

- `crates/switchbard-tui/tests/detail_edit.rs`: the 25 tests named above, run
  through the real key-handling path (`Harness::press`/`type_text`) against a
  real temporary Backlog-format repo, the same harness every other `sbt` test
  file uses.
- `crates/switchbard-tui/tests/harness/mod.rs::select_task_titled`: a new
  helper, filtering to a single task by title and landing the cursor on it.
  It deliberately leaves the resulting one-task filter in place rather than
  clearing it back to the full list — clearing would re-select by row
  position, which lands on a different task the instant the unfiltered
  order differs from the filtered one. A test that renames the selected
  task's title switches the filter to `id:<id>` first, since an
  in-place title filter would otherwise hide the very row it just renamed.
- Two pre-existing tests changed to match the new pane content (multiline
  description prose is no longer shown inline; the combined
  `id · status · priority · labels` metadata line is now one row per field):
  `crates/switchbard-tui/tests/browse.rs::j_and_k_move_selection_and_enter_opens_detail`,
  `crates/switchbard-tui/tests/columns.rs::ids_drop_the_repo_prefix_priority_is_a_letter_and_columns_fit_their_content`.
- `crates/switchbard-tui/tests/blocked.rs::detail_pane_lists_blocked_by_and_blocks`
  (pre-existing, unchanged, still green): the blocked-by/blocks read-only
  rows kept their exact rendering; this task only moved them into the same
  per-row loop the new editable fields use.
- Two real bugs the harness caught and this change fixes, not test-only
  workarounds:
  - Typing a value that starts with `h` or `l` (`"high"`, `"low"`) into a
    picker with nothing typed yet is swallowed by the picker-wide
    back/open convention (`h`/`l` with an empty typed buffer). `Filter`
    pickers already carried an escape hatch for this
    (`legacy_value_initial`); the new `Detail*` pickers now share it —
    without it, typing a priority into the detail pane's priority picker
    would silently back out instead
    (`status_priority_and_project_pickers_write_through_the_native_layer`).
  - `Mode::DetailInput`'s footer originally showed only the typed draft,
    never `app.status` — so a rejected due date or a stale-save error was
    set correctly but never rendered. Fixed with a dedicated two-line
    footer (`view::draw_detail_input`), the same shape `Mode::NewTask`
    already used for its own draft-plus-status footer
    (`due_date_validates_clears_and_rejects_garbage`,
    `stale_draft_on_disk_fails_the_save_and_keeps_the_typed_input`).
- `App::tick()` throttles its own storage-reload check to once per second
  (`storage_checked`); a test that needs a *second* reload within the same
  instant (moving a file into `backlog/drafts` after an earlier seed+tick)
  presses `r` (`Action::Reload`) instead, the same un-throttled path a user
  gets from the keyboard (`read_only_task_shows_fields_but_refuses_every_edit`).
- **Independent-review fix (MAJOR):** `toggle_detail_acceptance` was calling
  `begin_detail_edit` (re-snapshotting the draft) immediately before its own
  `save_detail_checklist` call, so the stale-draft compare always ran against
  a snapshot of itself — a real edit landing between focus and a `Space`
  toggle was silently overwritten instead of refused. `Space` is the one row
  that opens and commits in a single keystroke, so unlike every other field
  there is no separate "editor open" moment to snapshot at; the fix snapshots
  only at focus entry (and after this task's own prior successful save) and
  never refreshes immediately before the toggle's save. Verified by
  temporarily reintroducing the bad `begin_detail_edit` call and confirming
  `stale_draft_blocks_an_acceptance_toggle_and_leaves_the_file_untouched`
  fails against it (it turns "changed on disk; reload and retry" into a raw
  parse error against the tampered file, which is the actual behavior the
  guard exists to prevent). `stale_draft_blocks_a_picker_driven_field_pick`
  is the matching regression test for a picker-driven field, which never had
  this bug (its snapshot is already taken when the picker opens, not
  immediately before the commit) but had no dedicated test before this pass.
- **Independent-review fix:** a "new label" capture's Esc handler dropped
  straight to `Mode::DetailFocus`, while a *successful* add reopened the
  labels picker — an inconsistent cancel/commit symmetry. Esc on
  `DetailInputKind::NewLabel` now reopens the labels picker too
  (`esc_from_a_new_label_capture_returns_to_the_labels_picker`).
- **Independent-review cleanup:** removed `App::detail_task_id`, a
  write-only field whose doc comment claimed a cancellation mechanism that
  actually lives in `reload_tasks`'s existing per-`PickerPurpose`
  cancellation branch (extended to cover `Mode::DetailFocus`/`DetailInput`
  directly, with no dependency on the removed field); split `draw_detail`
  into `build_detail_lines` (row construction) and `adjust_detail_scroll`
  (the cursor-visibility math) so neither exceeds the repo's function-length
  norm; and replaced raw `[index]`/`[&id]` indexing in `detail_row_text` with
  `.get()` chains and an `.expect("invariant: ...")` naming exactly which
  invariant (`detail_pane::field_rows` and the row list agree by
  construction) would have to break for the index to be out of range.
- `mise run fmt`, `mise run clippy` (workspace, `RUSTFLAGS=-D warnings`), and
  `mise run test` (full workspace suite) all pass unpiped with exit code 0
  on the final tree.
