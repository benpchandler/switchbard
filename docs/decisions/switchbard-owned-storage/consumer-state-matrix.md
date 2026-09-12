# Central storage consumer state evidence

Validated in the isolated task-147 worktree on 2026-09-08. No installed application was restarted and no user database was changed for these checks.

| State or stress | Expected behavior | Evidence |
| --- | --- | --- |
| Task reparent or repository rebind | GUI selection, draft identity, sets, and write locks retain repository ID + record ID; current public ID/root update for execution | `runtime::task_identity::tests::identity_survives_reparent_revision_and_repository_rebind` |
| Dirty editor receives changed revision | Retain typed title and auxiliary draft fields, show conflict, prevent stale Save | `ui::backlog::detail::identity_tests::dirty_editor_survives_reparent_and_rebind_with_explicit_conflict` |
| External edit while detail is open | Real rendered warning and disabled Save; explicit discard control reloads current record after reparent | `backlog_controls::central_external_edit_retains_draft_and_reload_control_uses_latest_record` (egui input and AccessKit control click) |
| Clean editor or acknowledged own save | Refresh clean fields; acknowledge visible own save without manufacturing a conflict | `ui::backlog::detail::identity_tests::clean_editor_refreshes_and_acknowledged_own_save_clears_draft_state` |
| TUI central task change | Poll sequence at most once per second, retain selection by stable record ID, paint changed title | `central_storage::central_refresh_preserves_selection_after_reparent` (isolated SQLite process, real core mutations, TestBackend render, reparent visible below two seconds) |
| TUI central store becomes unreadable then recovers | Keep last good rows, report failure, retry even with unchanged sequence | Same isolated test injects invalid database bytes and restores fixture; no user files involved |
| GUI read failure | Retain last good rows and mark stale | Existing `workers::tests::failed_task_read_keeps_last_known_rows_and_marks_model_stale` and vanished-source test |
| GUI idle / unfocused | Central sequence checked each second; legacy complete scan retains prior cadence; errors trigger retry | `spawn_backlog` implementation; worker wake/timeout unit tests. Native wall-clock central refresh remains an explicit live validation gap |
| Large dataset | No storage IO in render; rebuild identity address index only for changed cached snapshots | Explicit Tasks perf smoke: 500 tasks, 200 frames: Board p95 15.496ms; grouped List p95 18.656ms; both configured thresholds passed |

`cargo test -p switchbard-gui --lib` passed 175 tests. `cargo check -p switchbard-gui --tests` passed after fixture migration. Full `backlog_controls` passed 89 tests; the first broad run had one existing scroll-settle timeout, then both the isolated rerun and final broad run passed. Full `cargo test -p switchbard-tui --tests` passed, and the added central-store failure/recovery case passed separately. `cargo clippy -p switchbard-gui -p switchbard-tui --all-targets -- -D warnings` passed on the final consumer source. The perf smoke was explicitly run with `--ignored`, not merely enumerated.

Native installed GUI observation, cross-process timing on the owner's full repository set, and permission-loss rendering remain outside this isolated evidence. No claim of a deployed/native application revision is made here. Repository aliases keep an existing valid execution address when available; stable IDs never fall back to a reused display ID after a record disappears.
