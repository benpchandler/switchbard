# Shared terminal list presentation

TASK-165 chooses frontend-local adapters after comparing the existing Tasks table and Pull Requests list. Both need terminal cell geometry, header styling and bounded scroll windows. They do not share feature row values, grouping, paint predicates, refresh authority or details. `list_presentation.rs` therefore accepts explicit rectangle, widths, labels, styles, row counts and scroll/selection values. It returns a `ListViewport` the feature applies to its own state. The picker renderer accepts lines and a block and explicitly returns whether all confirmation text fitted. It cannot access `App` or execute an action. Existing typed `PickOption`/`Payload` interactions remain owned by `picker.rs` and feature dispatch.

The feature entry points remain adapters around `App`; the reusable rendering primitives do not require it. Task group headings and live-work pulse, PR partial/stale/unknown observations, paint, feature actions, and detail panes remain feature-owned. TASK-141.1 through TASK-141.4 parity behavior is retained: independent PR columns/filter/sort/paint/views, numbered headers, lifecycle/check metadata and feature-specific details. No registration framework or core widget model is added.

## GUI inventory and boundary

- `switchbard-gui/src/ui/components/table_shell.rs::table_shell` already centralizes egui table defaults (striped, resizable, cell layout, outer scroll ownership).
- `switchbard-gui/src/ui/filter_bar.rs` owns bar/search/facet labels/clear controls, reused by GUI surfaces; predicates remain feature-owned.
- `switchbard-gui/src/ui/components/badge.rs` owns missing/loading/count badges; `status_pill.rs`, `action_status.rs`, `mono_cell.rs`, `path_cell.rs`, `branch_label.rs` and `section.rs` own other existing presentation conventions.
- These GUI primitives already use explicit local inputs. They remain the GUI authorities; importing them into ratatui would force framework dependencies and incompatible event/layout semantics. Domain catalogs and filter facts may be shared independently of widgets. No GUI render path changed, so GUI frame p95 comparison is N/A.

## State and stress matrix

Evidence paths below are relative to `crates/switchbard-tui/tests/`. All tests use the real App, real key events, real backlog files and terminal rendering; no mock clients or unit tests were added.

| State or stress | Evidence / gate |
| --- | --- |
| Default, active selection, keyboard focus, repeated navigation | `browse.rs`, `menu_pickers.rs`, `list_presentation.rs` |
| Empty/filter-to-zero, unavailable PR source, retry | `list_presentation.rs`, `pr_controls.rs`, `pull_requests.rs::non_repository_is_unavailable_not_a_successful_empty_list` |
| Loading | Existing real PR worker journeys; latency-dependent loading render remains a gap for deterministic capture |
| Group headings, selection through sections | `group.rs` |
| Many rows (253), viewport-bounded drawing, resize 48x8 / 100x20 / 160x30 | `list_presentation.rs::large_list_keeps_last_selection_visible_after_resize_and_filter` |
| Long unbroken labels and Japanese script, short/narrow picker, focus return | `list_presentation.rs::long_unicode_picker_labels_are_clipped_and_navigation_retains_focus` |
| Picker scrolling, parent navigation, task disappearance, selected task identity | `menu_pickers.rs` |
| Confirmation too narrow/short, pending duplicate guard, retry/outcome unknown | `pr_merge.rs`; authority text must fit before explicit confirmation |
| Independent pages, persisted layout and resumption | `pr_controls.rs`, `pages.rs`, `views.rs` |
| Stale/partial/historical/read-only PR content | Existing `pr_refresh.rs`, `pr_actions.rs`, `pull_requests.rs` live gates; full authenticated live history is not proved by the temporary non-repository tests |
| Dirty/saving/success/failure/conflict/access-limited | Feature-owned mutation/PR tests remain authorities; renderer never initiates writes. Live role changes and removed GitHub access remain explicit gaps |
| Zero-size terminal and pathological one-line body | `browse.rs::zero_size_terminal_does_not_crash` verifies zero-size; `list_presentation.rs::one_body_slot_keeps_grouped_selected_task_visible_instead_of_heading` verifies selected task visibility at 100x6 and heading restoration at 100x7 |
| Multi-line/duplicate content and overlapping picker/detail combinations | Existing detail and picker tests; exhaustive combinations remain a gap |
| Pointer/touch, browser zoom | N/A: terminal keyboard UI; terminal text scaling is represented by cell-dimension resize |
| Numeric performance budget at maximum backlog / 1000 real PRs | Gap: bounded viewport structure verified, no production-scale latency claim or GUI benchmark claim |

The list component clamps scroll calculations and visits at most the visible row count when rendering. Feature-specific fitted column width calculation still scans the feature's loaded rows, as before. This change does not claim to remove that cost.

## Validation

Targeted command: `CARGO_TARGET_DIR=/tmp/switchbard-abstractions-target cargo test -p switchbard-tui --test list_presentation --test browse --test menu_pickers --test group --test pr_controls --test pr_merge --test pull_requests`.

Targeted results: 43 passed, 13 opt-in evidence/live tests ignored. `cargo clippy -p switchbard-tui --all-targets -- -D warnings` passed with the same dedicated target directory. The ignored authenticated live tests are future gates, not observed state evidence. Automated terminal-buffer evidence is not human visual approval. Full installed-TUI and authenticated live PR use remain integration gates owned by the parent mission.

## Short grouped viewport regression

A real-key/backlog/render reproduction at 100x6 showed a project heading consuming the only body row and hiding the selected task. `one_body_slot_keeps_grouped_selected_task_visible_instead_of_heading` failed before the fix with the heading alone on screen. Heading retention now requires two body slots; the test covers one-slot selection, j/k navigation, and restoring both heading and selection after resize to 100x7.
