# Actual test coverage and remaining work

This map binds current product tests, not historical model checks. Root runs verify.py after integration stabilizes; this document records no fresh passes. Supporting tests cover only their assertions. A nonempty residual gap prevents the whole MUST from passing even if all supporting tests pass.

## MUST-001

Residual: Need one combined two-repo full-domain reload/config/workspace-order journey and actual phased migration evidence; current tests establish slices.

- `core::storage::tests::per_kind_authority_and_flexible_bytes_survive_reopen`
- `core::backlog::task_storage::tests::task_cutover_all_lifecycles_read_and_edit_after_source_files_disappear`
- `core::backlog::hierarchy::storage_tests::hierarchy_empty_cutover_creates_without_legacy_directories_and_renames_stably`
- `core::backlog::aggregate_storage::tests::aggregate_empty_authority_creates_without_backlog_files`
- `core::storage::workspace_order::tests::workspace_ordering_stays_local_and_follows_stable_targets_after_rehome_and_rebind`

## MUST-002

Residual: Need explicit fork-not-autobound journey and independent-clone scope decision coverage.

- `core::storage::tests::linked_worktrees_share_identity`
- `core::storage::tests::moved_repository_explicit_rebind_retains_old_alias_and_epoch`
- `core::storage::tests::reused_git_path_never_silently_joins_old_identity`
- `core::storage::tests::rebind_cannot_take_another_repository_binding`
- `core::storage::exchange::tests::separate_local_bindings_can_explicitly_share_repository_identity`
- `journey::rebind_cli_preserves_repository_content_at_new_checkout`

## MUST-003

Residual: GUI selection/draft/lock/cache rebind and reparent identity journey not yet mapped.

- `core::backlog::task_storage::tests::task_cutover_lifecycle_moves_keep_identity_and_sources`
- `core::backlog::task_storage::tests::task_cutover_reparent_commits_dependencies_and_aggregates_together`
- `core::backlog::hierarchy::storage_tests::hierarchy_empty_cutover_creates_without_legacy_directories_and_renames_stably`
- `core::storage::workspace_order::tests::workspace_ordering_stays_local_and_follows_stable_targets_after_rehome_and_rebind`

## MUST-004

Residual: Projection rebuild, newer payload-version handling and built-in/custom name promotion are unproved; schema-unchanged test name does not assert actual schema version.

- `core::storage::tests::per_kind_authority_and_flexible_bytes_survive_reopen`
- `core::backlog::task_storage::tests::task_cutover_preserves_custom_content_and_projection_and_multi_field_atomicity`
- `core::backlog::hierarchy::storage_tests::hierarchy_cutover_preserves_projection_and_custom_content_without_file_writes`
- `core::backlog::aggregate_storage::tests::aggregate_cutover_preserves_raw_extensions_and_reads_changed_values`
- `core::storage::exchange::tests::readable_unknown_content_and_binary_round_trip`
- `journey::custom_document_edit_is_lossless_revision_checked_and_requires_no_schema_change`

## MUST-005

Residual: Current independent connections use threads; independent-process full-command rollback and receipt effects not proved.

- `core::storage::tests::independent_connections_do_not_lose_read_modify_writes`
- `core::storage::tests::multi_kind_edit_is_atomic_and_rehome_retains_locator_history`
- `core::backlog::hierarchy::storage_tests::hierarchy_central_project_rename_late_failure_rolls_back_every_kind`
- `core::backlog::task_storage::tests::task_cutover_concurrent_creates_are_distinct_without_markdown_writes`

## MUST-006

Residual: No command_id receipts or mismatched-ID replay test; causal import replay is a different contract.

- `core::storage::tests::stale_revision_and_closure_failure_have_no_effect`
- `core::storage::exchange::tests::concurrent_equal_content_coalesces_and_replay_is_idempotent`

## MUST-007

Residual: Known native graphs validate; explicitly required dangling-link/deletion-plan semantics and immutable kind/lifecycle transitions need complete tests.

- `core::backlog::storage_validation::tests::duplicate_task_ids_across_lifecycles_and_definition_names_reject`
- `core::backlog::storage_validation::tests::resolved_cycles_reject_but_optional_missing_targets_remain_raw`
- `core::backlog::storage_validation::tests::checked_bind_rolls_back_malformed_native_documents`
- `core::storage::exchange::tests::domain_validator_failure_rolls_back_import_and_binding`

## MUST-008

Residual: No real competing-lock wall-clock measurement <=5s and retry-feedback test.

No current product test bound.

## MUST-009

Residual: No declared residual gap; every named test below must pass.

- `core::backlog::migration::tests::inventory_preserves_custom_bytes_and_rejects_divergence`
- `core::backlog::migration::tests::unmerged_branch_only_definition_refuses_cutover`
- `core::backlog::migration::tests::identical_branch_source_passes_and_ref_change_changes_digest`
- `core::backlog::migration::tests::obsolete_ancestor_and_unchanged_branch_content_do_not_block`
- `core::storage::tests::duplicate_sources_require_equal_bytes`
- `core::backlog::task_storage::tests::task_cutover_all_lifecycles_read_and_edit_after_source_files_disappear`

## MUST-010

Residual: Need protected backup-before-activation failure injection and migration CLI branch-ref preview invalidation assertion.

- `core::storage::tests::migration_rechecks_sources_and_preserves_original_bytes`
- `core::storage::tests::migration_failure_rolls_back_documents_provenance_and_authority`
- `journey::stale_migration_preview_rejects_without_database_or_source_effects`

## MUST-011

Residual: Later legacy-file divergence reporting without auto-import not proved.

- `journey::initiative_cutover_preserves_reads_custom_content_and_legacy_files`
- `journey::corrupt_database_cannot_silently_write_legacy_definition`
- `core::backlog::task_storage::tests::task_cutover_all_lifecycles_read_and_edit_after_source_files_disappear`

## MUST-012

Residual: Need clean and already-dirty Git status/bytes comparisons for every native command family; current source preservation is narrower.

- `journey::initiative_cutover_preserves_reads_custom_content_and_legacy_files`
- `core::backlog::task_storage::tests::task_cutover_concurrent_creates_are_distinct_without_markdown_writes`
- `core::backlog::hierarchy::storage_tests::hierarchy_cutover_preserves_projection_and_custom_content_without_file_writes`
- `core::backlog::aggregate_storage::tests::aggregate_cutover_preserves_raw_extensions_and_reads_changed_values`

## MUST-013

Residual: Need native GUI/TUI/headless queue/refine/dispatch same-record journey after execution-worktree deletion and cross-repo RunId artifact isolation.

- `core::backlog::task_storage::tests::task_cutover_all_lifecycles_read_and_edit_after_source_files_disappear`

## MUST-014

Residual: No central versioned refine result/apply/stale-result journey bound.

No current product test bound.

## MUST-015

Residual: No measured two-running-native-client <=2000ms visibility and <=1Hz poll evidence.

No current product test bound.

## MUST-016

Residual: No actual GUI/TUI refocus-before-write and retained dirty-draft conflict journey bound.

No current product test bound.

## MUST-017

Residual: Need independent fixed v2 wire golden comparison, all-native-domain roundtrip and reviewed independent-edit PR diffs; newer content-version behavior unproved.

- `core::storage::exchange::tests::readable_unknown_content_and_binary_round_trip`
- `core::storage::exchange::tests::utf8_lines_preserve_final_newline_crlf_and_reject_ambiguous_fragments`
- `journey::two_cli_peers_bootstrap_edit_and_skip_exports_without_replaying_history`
- `core::backlog::storage_validation::tests::custom_nested_content_and_custom_status_survive_validation_exactly`

## MUST-018

Residual: Need concurrent consistent-snapshot export and write/rename/ambiguous-outcome fault-injection recovery proof.

- `file::storage_exchange_file::tests::competing_export_and_independent_file_change_are_preserved`

## MUST-019

Residual: Need exact unchanged repeated CLI export bytes and edited-target reconcile/replace journeys.

- `file::storage_exchange_file::tests::competing_export_and_independent_file_change_are_preserved`
- `journey::two_cli_peers_bootstrap_edit_and_skip_exports_without_replaying_history`

## MUST-020

Residual: Epoch mismatch rejects, but explicit epoch compaction/reconciliation and full native-domain peer journeys remain unproved.

- `core::storage::exchange::tests::bootstrap_edit_export_back_and_empty_kind`
- `core::storage::exchange::tests::skipped_exports_and_alternating_imports`
- `core::storage::exchange::tests::stale_snapshot_and_omission_never_roll_back_or_delete`
- `core::storage::exchange::tests::restored_database_rotates_replica_and_divergent_changes_conflict`
- `core::storage::exchange::tests::equal_clock_different_content_and_wrong_epoch_reject`
- `journey::two_cli_peers_bootstrap_edit_and_skip_exports_without_replaying_history`

## MUST-021

Residual: No declared residual gap; every named test below must pass.

- `core::storage::exchange::tests::offline_disjoint_edits_converge`
- `core::storage::exchange::tests::same_record_conflict_is_atomic_and_explicit_resolution_converges`
- `core::storage::exchange::tests::stale_snapshot_and_omission_never_roll_back_or_delete`
- `core::storage::exchange::tests::edit_tombstone_conflicts_and_tombstone_dominates_stale_replay`
- `core::storage::exchange::tests::equal_clock_different_content_and_wrong_epoch_reject`
- `core::storage::exchange::tests::concurrent_equal_content_coalesces_and_replay_is_idempotent`
- `core::storage::exchange::tests::offline_creation_collision_requires_explicit_relocation_and_preserves_both`
- `core::backlog::storage_validation::tests::duplicate_task_ids_across_lifecycles_and_definition_names_reject`

## MUST-022

Residual: Need changed exact source bytes between import preview/apply rejection (semantic snapshot digest is not exact file-byte digest).

- `core::storage::exchange::tests::stale_preview_and_record_locator_collision_have_no_effect`
- `core::storage::exchange::tests::bind_preview_sequence_is_rechecked_under_writer_lock`
- `journey::stale_migration_preview_rejects_without_database_or_source_effects`

## MUST-023

Residual: Need explicit custom-resolution complete native graph rejection and retained conflicting originals recovery evidence.

- `core::storage::exchange::tests::same_record_conflict_is_atomic_and_explicit_resolution_converges`
- `core::storage::exchange::tests::domain_validator_failure_rolls_back_import_and_binding`
- `core::backlog::storage_validation::tests::checked_bind_rolls_back_malformed_native_documents`

## MUST-024

Residual: Explicit depth32 and counter exhaustion have tests; inclusive/one-over 64MiB,100k,4MiB,128-clock and full malformed-form corpus with zero live effects remain unproved.

- `core::storage::exchange::tests::duplicate_outer_and_clock_keys_and_counter_exhaustion_reject`
- `core::backlog::storage_validation::tests::native_paths_are_confined_and_unknown_kind_stays_opaque`
- `core::backlog::storage_validation::tests::malformed_known_yaml_rejects_instead_of_falling_back_to_empty`
- `core::backlog::storage_validation::tests::config_and_aggregate_singleton_semantics_are_checked`
- `core::storage::exchange::tests::json_nesting_has_an_explicit_32_level_boundary`

## MUST-025

Residual: No public canonicalize workflow implemented or tested.

No current product test bound.

## MUST-026

Residual: No declared residual gap; every named test below must pass.

- `core::storage::tests::malformed_database_never_falls_back_to_legacy`
- `core::storage::tests::lost_established_database_never_restores_legacy_authority`
- `core::storage::tests::unknown_schema_and_foreign_database_are_rejected`
- `core::backlog::hierarchy::storage_tests::hierarchy_database_failure_never_reads_or_writes_legacy_fallback`
- `journey::corrupt_database_cannot_silently_write_legacy_definition`

## MUST-027

Residual: Backup/reopen and supported CLI restore now have product tests; lossless reverse migration of post-cutover data remains unproved.

- `core::storage::tests::backup_restores_authority_documents_and_provenance`
- `core::storage::exchange::tests::restored_database_rotates_replica_and_divergent_changes_conflict`
- `journey::backup_restore_preserves_all_table_fingerprint_and_original_backup_bytes`
- `journey::recovery_refuses_existing_files_and_missing_database_with_marker`
- `journey::empty_or_non_sqlite_restore_source_is_not_treated_as_a_fresh_database`

## MUST-028

Residual: No real-key TUI storage state/retained draft/legacy fixture journey bound.

No current product test bound.

## MUST-029

Residual: No approved canonical plus complete real GUI state/container renders.

No current product test bound.

## MUST-030

Residual: No approved canonical plus complete real TUI state/container renders.

No current product test bound.

## MUST-031

Residual: Actual integration/evidence gate evaluates this criterion.

No current product test bound.

## MUST-032

Residual: Actual integration/evidence gate evaluates this criterion.

No current product test bound.

## MUST-033

Residual: Actual integration/evidence gate evaluates this criterion.

No current product test bound.

## MUST-034

Residual: Need permissive-umask, sidecar/backup/new-directory modes, foreign-owner substitution and existing-directory non-chmod evidence.

- `core::storage::tests::private_database_and_symlink_rejection`
- `core::backlog::migration::tests::symlinked_parent_directory_is_rejected`

## Non-test completion gates

Real-data phased cutover, same-or-intentional behavioral comparison, running native client continuity and reporter confirmation are not proven by temporary fixtures. Record actual source evidence in the root migration ledger; a self-authored pass assertion cannot substitute for reporter confirmation. Exact build identity, CI and native perf are separate. Production mutation execution and its independent review remain unproved; artifact hashes and asserted pass/fail statuses alone cannot satisfy that gate.
