# PR controls E2E evidence

Actual GitHub reads and live work-store projection. Tests run through real app key events and terminal cells.

```text
   Compiling switchbard-tui v0.4.0 (/Users/bpc/Dev/.worktrees/switchbard-tui-pr-pane/crates/switchbard-tui)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.72s
     Running tests/pages.rs (/Users/bpc/Library/Caches/switchbard/cargo-target/debug/deps/pages-1b9b3fc9ec08c0ab)

running 4 tests
test page_binding_is_configurable_and_help_is_available_on_both_pages ... ok
test toggles_pages_without_losing_task_context ... ok
test page_survives_self_restart_and_hidden_task_commands_do_nothing ... ok
test pages_render_at_small_and_large_terminal_sizes_with_no_tasks ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s

     Running tests/pr_controls.rs (/Users/bpc/Library/Caches/switchbard/cargo-target/debug/deps/pr_controls-4809424900ec371c)

running 10 tests
test live_claimed_task_is_visible_on_tasks_page ... ok
test task_reload_while_pr_page_is_active_keeps_task_filter_sort_and_selection ... ok
test empty_pr_controls_use_shared_pickers_and_preserve_task_settings ... ok
test pr_column_menu_and_saved_layout_are_isolated_from_tasks ... ok
test pr_columns_reorder_and_global_save_do_not_change_tasks ... ok
test live_pr_title_check_sorts_and_review_merge_facets_match_observations ... ok
test live_pr_historical_facets_and_paint_rule_order_use_shared_controls ... ok
test live_pr_sort_filter_and_refresh_preserve_identity_and_task_state ... ok
test live_pr_paint_links_and_saved_view_survive_page_switch_and_restart ... ok
test live_pr_selected_identity_survives_self_restart ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.14s

     Running tests/pull_requests.rs (/Users/bpc/Library/Caches/switchbard/cargo-target/debug/deps/pull_requests-a9065ea8f49870d0)

running 6 tests
test pr_filters_use_the_shared_picker_and_preserve_task_filter_and_restart ... ok
test pr_detail_matches_task_pane_frame_and_empty_state ... ok
test non_repository_is_unavailable_not_a_successful_empty_list ... ok
test live_repository_renders_actual_pull_requests ... ok
test live_task_links_and_failed_refresh_preserve_real_task_bytes ... ok
test live_all_states_can_be_filtered_without_refetching ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.85s

```
