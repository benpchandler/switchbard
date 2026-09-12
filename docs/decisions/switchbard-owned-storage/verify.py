#!/usr/bin/env python3
"""Current product acceptance: actual named tests plus explicit unmet outcome gates."""
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
# Stable criterion identities; actual current test bindings are in BINDINGS below.
CHECKS = [
    ('001', 'behavior', 'All native domain records and workspace ordering persist across two repositories without backlog files'),  # MUST-001
    ('002', 'behavior', 'Linked worktrees share one scope while independent repos and forks remain separate'),  # MUST-002
    ('003', 'behavior', 'Public locators remain compatible and RecordId survives hierarchy changes'),  # MUST-003
    ('004', 'behavior', 'Raw legacy bytes and custom content survive migration, live edits and projection rebuild'),  # MUST-004
    ('005', 'behavior', 'Whole multi-record command commits or rolls back across processes'),  # MUST-005
    ('006', 'behavior', 'Stale revisions reject and exact replay is idempotent while mismatched replay rejects'),  # MUST-006
    ('007', 'contract', 'Illegal scoped record graphs reject without side effects'),  # MUST-007
    ('008', 'quality', 'Busy handling terminates within five seconds with retry feedback'),  # MUST-008
    ('009', 'behavior', 'Migration inventories lifecycle, sibling and branch sources with explicit reconciliation'),  # MUST-009
    ('010', 'behavior', 'Backup precedes atomic migration activation and source changes abort safely'),  # MUST-010
    ('011', 'behavior', 'Updated clients never write legacy files and report later legacy divergence'),  # MUST-011
    ('012', 'behavior', 'Normal domain edits change no repo files in clean or already dirty worktrees'),  # MUST-012
    ('013', 'behavior', 'All consumers and headless queue agree after worktree removal'),  # MUST-013
    ('014', 'behavior', 'Refine applies versioned raw content centrally and rejects stale results'),  # MUST-014
    ('015', 'quality', 'Active GUI and TUI see commits within two seconds with at most one poll per second'),  # MUST-015
    ('016', 'behavior', 'Refocus refreshes before writes and concurrent-edit conflicts retain drafts'),  # MUST-016
    ('017', 'contract', 'One deterministic current-state file preserves native/custom/opaque records and excludes machine authorities'),  # MUST-017
    ('018', 'behavior', 'Export uses a consistent snapshot and atomic replacement with recoverable checkpoint ordering'),  # MUST-018
    ('019', 'behavior', 'Independent target edits are protected and unchanged repeat export is byte-identical'),  # MUST-019
    ('020', 'behavior', 'Explicit bound bootstrap and causal peer exchange preserve newer work'),  # MUST-020
    ('021', 'behavior', 'Causal merge preserves omissions and detects document, locator and tombstone conflicts'),  # MUST-021
    ('022', 'behavior', 'Preview apply rechecks file digest and repository revision before atomic import'),  # MUST-022
    ('023', 'behavior', 'Explicit resolution preserves originals and validates the complete resulting graph'),  # MUST-023
    ('024', 'contract', 'Input boundary limits and malformed or untrusted documents reject without live effects'),  # MUST-024
    ('025', 'behavior', 'Canonicalization repairs format without importing or trusting unknown ancestry'),  # MUST-025
    ('026', 'behavior', 'Unavailable or corrupt database reports failure without empty success or Markdown fallback'),  # MUST-026
    ('027', 'behavior', 'Database-consistent restore retains post-cutover writes and reverse migration reconciles newer data'),  # MUST-027
    ('028', 'behavior', 'TUI real-key state journeys preserve drafts and existing legacy fixture behavior'),  # MUST-028
    ('029', 'visual', 'GUI storage states render in narrow current and wide containers against an approved canonical'),  # MUST-029
    ('030', 'visual', 'TUI storage states render at 80x24 and current size against an approved canonical'),  # MUST-030
    ('031', 'quality', 'Integrated implementation passes platform gates and render performance comparison'),  # MUST-031
    ('032', 'behavior', 'Reporter confirms two-client no-PR workflow and independent-clone single-file exchange'),  # MUST-032
    ('033', 'contract', 'Authority and compatibility documentation matches tested behavior and preserves independent authorities'),  # MUST-033
    ('034', 'contract', 'Storage files and backups preserve private ownership and permissions under permissive umask'),  # MUST-034
]

SUITES = {
    'core': ['cargo', 'test', '-p', 'switchbard-core', '--lib', '--', 'storage::',
             'backlog::task_storage::', 'backlog::hierarchy::storage_tests::',
             'backlog::aggregate_storage::', 'backlog::migration::', 'backlog::storage_validation::', '--color', 'never'],
    'journey': ['cargo', 'test', '-p', 'switchbard-task', '--test', 'storage', '--test', 'storage_recovery', '--', '--color', 'never'],
    'file': ['cargo', 'test', '-p', 'switchbard-task', '--bin', 'sb', '--', 'storage_exchange_file::tests::', '--color', 'never'],
}

# Every actual supporting test is parsed independently; residual gaps prevent aggregate PASS.
BINDINGS = {'001': [('core', 'storage::tests::per_kind_authority_and_flexible_bytes_survive_reopen'),
         ('core',
          'backlog::task_storage::tests::task_cutover_all_lifecycles_read_and_edit_after_source_files_disappear'),
         ('core',
          'backlog::hierarchy::storage_tests::hierarchy_empty_cutover_creates_without_legacy_directories_and_renames_stably'),
         ('core',
          'backlog::aggregate_storage::tests::aggregate_empty_authority_creates_without_backlog_files'),
         ('core',
          'storage::workspace_order::tests::workspace_ordering_stays_local_and_follows_stable_targets_after_rehome_and_rebind')],
 '002': [('core', 'storage::tests::linked_worktrees_share_identity'),
         ('core', 'storage::tests::moved_repository_explicit_rebind_retains_old_alias_and_epoch'),
         ('core', 'storage::tests::reused_git_path_never_silently_joins_old_identity'),
         ('core', 'storage::tests::rebind_cannot_take_another_repository_binding'),
         ('core',
          'storage::exchange::tests::separate_local_bindings_can_explicitly_share_repository_identity'),
         ('journey', 'rebind_cli_preserves_repository_content_at_new_checkout')],
 '003': [('core', 'backlog::task_storage::tests::task_cutover_lifecycle_moves_keep_identity_and_sources'),
         ('core',
          'backlog::task_storage::tests::task_cutover_reparent_commits_dependencies_and_aggregates_together'),
         ('core',
          'backlog::hierarchy::storage_tests::hierarchy_empty_cutover_creates_without_legacy_directories_and_renames_stably'),
         ('core',
          'storage::workspace_order::tests::workspace_ordering_stays_local_and_follows_stable_targets_after_rehome_and_rebind')],
 '004': [('core', 'storage::tests::per_kind_authority_and_flexible_bytes_survive_reopen'),
         ('core',
          'backlog::task_storage::tests::task_cutover_preserves_custom_content_and_projection_and_multi_field_atomicity'),
         ('core',
          'backlog::hierarchy::storage_tests::hierarchy_cutover_preserves_projection_and_custom_content_without_file_writes'),
         ('core',
          'backlog::aggregate_storage::tests::aggregate_cutover_preserves_raw_extensions_and_reads_changed_values'),
         ('core', 'storage::exchange::tests::readable_unknown_content_and_binary_round_trip'),
         ('journey', 'custom_document_edit_is_lossless_revision_checked_and_requires_no_schema_change')],
 '005': [('core', 'storage::tests::independent_connections_do_not_lose_read_modify_writes'),
         ('core', 'storage::tests::multi_kind_edit_is_atomic_and_rehome_retains_locator_history'),
         ('core',
          'backlog::hierarchy::storage_tests::hierarchy_central_project_rename_late_failure_rolls_back_every_kind'),
         ('core',
          'backlog::task_storage::tests::task_cutover_concurrent_creates_are_distinct_without_markdown_writes')],
 '006': [('core', 'storage::tests::stale_revision_and_closure_failure_have_no_effect'),
         ('core', 'storage::exchange::tests::concurrent_equal_content_coalesces_and_replay_is_idempotent')],
 '007': [('core',
          'backlog::storage_validation::tests::duplicate_task_ids_across_lifecycles_and_definition_names_reject'),
         ('core',
          'backlog::storage_validation::tests::resolved_cycles_reject_but_optional_missing_targets_remain_raw'),
         ('core', 'backlog::storage_validation::tests::checked_bind_rolls_back_malformed_native_documents'),
         ('core', 'storage::exchange::tests::domain_validator_failure_rolls_back_import_and_binding')],
 '008': [],
 '009': [('core', 'backlog::migration::tests::inventory_preserves_custom_bytes_and_rejects_divergence'),
         ('core', 'backlog::migration::tests::unmerged_branch_only_definition_refuses_cutover'),
         ('core', 'backlog::migration::tests::identical_branch_source_passes_and_ref_change_changes_digest'),
         ('core', 'backlog::migration::tests::obsolete_ancestor_and_unchanged_branch_content_do_not_block'),
         ('core', 'storage::tests::duplicate_sources_require_equal_bytes'),
         ('core',
          'backlog::task_storage::tests::task_cutover_all_lifecycles_read_and_edit_after_source_files_disappear')],
 '010': [('core', 'storage::tests::migration_rechecks_sources_and_preserves_original_bytes'),
         ('core', 'storage::tests::migration_failure_rolls_back_documents_provenance_and_authority'),
         ('journey', 'stale_migration_preview_rejects_without_database_or_source_effects')],
 '011': [('journey', 'initiative_cutover_preserves_reads_custom_content_and_legacy_files'),
         ('journey', 'corrupt_database_cannot_silently_write_legacy_definition'),
         ('core',
          'backlog::task_storage::tests::task_cutover_all_lifecycles_read_and_edit_after_source_files_disappear')],
 '012': [('journey', 'initiative_cutover_preserves_reads_custom_content_and_legacy_files'),
         ('core',
          'backlog::task_storage::tests::task_cutover_concurrent_creates_are_distinct_without_markdown_writes'),
         ('core',
          'backlog::hierarchy::storage_tests::hierarchy_cutover_preserves_projection_and_custom_content_without_file_writes'),
         ('core',
          'backlog::aggregate_storage::tests::aggregate_cutover_preserves_raw_extensions_and_reads_changed_values')],
 '013': [('core',
          'backlog::task_storage::tests::task_cutover_all_lifecycles_read_and_edit_after_source_files_disappear')],
 '014': [],
 '015': [],
 '016': [],
 '017': [('core', 'storage::exchange::tests::readable_unknown_content_and_binary_round_trip'),
         ('core',
          'storage::exchange::tests::utf8_lines_preserve_final_newline_crlf_and_reject_ambiguous_fragments'),
         ('journey', 'two_cli_peers_bootstrap_edit_and_skip_exports_without_replaying_history'),
         ('core',
          'backlog::storage_validation::tests::custom_nested_content_and_custom_status_survive_validation_exactly')],
 '018': [('file',
          'storage_exchange_file::tests::competing_export_and_independent_file_change_are_preserved')],
 '019': [('file', 'storage_exchange_file::tests::competing_export_and_independent_file_change_are_preserved'),
         ('journey', 'two_cli_peers_bootstrap_edit_and_skip_exports_without_replaying_history')],
 '020': [('core', 'storage::exchange::tests::bootstrap_edit_export_back_and_empty_kind'),
         ('core', 'storage::exchange::tests::skipped_exports_and_alternating_imports'),
         ('core', 'storage::exchange::tests::stale_snapshot_and_omission_never_roll_back_or_delete'),
         ('core',
          'storage::exchange::tests::restored_database_rotates_replica_and_divergent_changes_conflict'),
         ('core', 'storage::exchange::tests::equal_clock_different_content_and_wrong_epoch_reject'),
         ('journey', 'two_cli_peers_bootstrap_edit_and_skip_exports_without_replaying_history')],
 '021': [('core', 'storage::exchange::tests::offline_disjoint_edits_converge'),
         ('core',
          'storage::exchange::tests::same_record_conflict_is_atomic_and_explicit_resolution_converges'),
         ('core', 'storage::exchange::tests::stale_snapshot_and_omission_never_roll_back_or_delete'),
         ('core', 'storage::exchange::tests::edit_tombstone_conflicts_and_tombstone_dominates_stale_replay'),
         ('core', 'storage::exchange::tests::equal_clock_different_content_and_wrong_epoch_reject'),
         ('core', 'storage::exchange::tests::concurrent_equal_content_coalesces_and_replay_is_idempotent'),
         ('core',
          'storage::exchange::tests::offline_creation_collision_requires_explicit_relocation_and_preserves_both'),
         ('core',
          'backlog::storage_validation::tests::duplicate_task_ids_across_lifecycles_and_definition_names_reject')],
 '022': [('core', 'storage::exchange::tests::stale_preview_and_record_locator_collision_have_no_effect'),
         ('core', 'storage::exchange::tests::bind_preview_sequence_is_rechecked_under_writer_lock'),
         ('journey', 'stale_migration_preview_rejects_without_database_or_source_effects')],
 '023': [('core',
          'storage::exchange::tests::same_record_conflict_is_atomic_and_explicit_resolution_converges'),
         ('core', 'storage::exchange::tests::domain_validator_failure_rolls_back_import_and_binding'),
         ('core', 'backlog::storage_validation::tests::checked_bind_rolls_back_malformed_native_documents')],
 '024': [('core', 'storage::exchange::tests::duplicate_outer_and_clock_keys_and_counter_exhaustion_reject'),
         ('core',
          'backlog::storage_validation::tests::native_paths_are_confined_and_unknown_kind_stays_opaque'),
         ('core',
          'backlog::storage_validation::tests::malformed_known_yaml_rejects_instead_of_falling_back_to_empty'),
         ('core', 'backlog::storage_validation::tests::config_and_aggregate_singleton_semantics_are_checked'),
         ('core', 'storage::exchange::tests::json_nesting_has_an_explicit_32_level_boundary')],
 '025': [],
 '026': [('core', 'storage::tests::malformed_database_never_falls_back_to_legacy'),
         ('core', 'storage::tests::lost_established_database_never_restores_legacy_authority'),
         ('core', 'storage::tests::unknown_schema_and_foreign_database_are_rejected'),
         ('core',
          'backlog::hierarchy::storage_tests::hierarchy_database_failure_never_reads_or_writes_legacy_fallback'),
         ('journey', 'corrupt_database_cannot_silently_write_legacy_definition')],
 '027': [('core', 'storage::tests::backup_restores_authority_documents_and_provenance'),
         ('core',
          'storage::exchange::tests::restored_database_rotates_replica_and_divergent_changes_conflict'),
         ('journey', 'backup_restore_preserves_all_table_fingerprint_and_original_backup_bytes'),
         ('journey', 'recovery_refuses_existing_files_and_missing_database_with_marker'),
         ('journey', 'empty_or_non_sqlite_restore_source_is_not_treated_as_a_fresh_database')],
 '028': [],
 '029': [],
 '030': [],
 '031': [],
 '032': [],
 '033': [],
 '034': [('core', 'storage::tests::private_database_and_symlink_rejection'),
         ('core', 'backlog::migration::tests::symlinked_parent_directory_is_rejected')]}
GAPS = {'001': 'Need one combined two-repo full-domain reload/config/workspace-order journey and actual phased '
        'migration evidence; current tests establish slices.',
 '002': 'Need explicit fork-not-autobound journey and independent-clone scope decision coverage.',
 '003': 'GUI selection/draft/lock/cache rebind and reparent identity journey not yet mapped.',
 '004': 'Projection rebuild, newer payload-version handling and built-in/custom name promotion are unproved; '
        'schema-unchanged test name does not assert actual schema version.',
 '005': 'Current independent connections use threads; independent-process full-command rollback and receipt '
        'effects not proved.',
 '006': 'No command_id receipts or mismatched-ID replay test; causal import replay is a different contract.',
 '007': 'Known native graphs validate; explicitly required dangling-link/deletion-plan semantics and '
        'immutable kind/lifecycle transitions need complete tests.',
 '008': 'No real competing-lock wall-clock measurement <=5s and retry-feedback test.',
 '009': '',
 '010': 'Need protected backup-before-activation failure injection and migration CLI branch-ref preview '
        'invalidation assertion.',
 '011': 'Later legacy-file divergence reporting without auto-import not proved.',
 '012': 'Need clean and already-dirty Git status/bytes comparisons for every native command family; current '
        'source preservation is narrower.',
 '013': 'Need native GUI/TUI/headless queue/refine/dispatch same-record journey after execution-worktree '
        'deletion and cross-repo RunId artifact isolation.',
 '014': 'No central versioned refine result/apply/stale-result journey bound.',
 '015': 'No measured two-running-native-client <=2000ms visibility and <=1Hz poll evidence.',
 '016': 'No actual GUI/TUI refocus-before-write and retained dirty-draft conflict journey bound.',
 '017': 'Need independent fixed v2 wire golden comparison, all-native-domain roundtrip and reviewed '
        'independent-edit PR diffs; newer content-version behavior unproved.',
 '018': 'Need concurrent consistent-snapshot export and write/rename/ambiguous-outcome fault-injection '
        'recovery proof.',
 '019': 'Need exact unchanged repeated CLI export bytes and edited-target reconcile/replace journeys.',
 '020': 'Epoch mismatch rejects, but explicit epoch compaction/reconciliation and full native-domain peer '
        'journeys remain unproved.',
 '021': '',
 '022': 'Need changed exact source bytes between import preview/apply rejection (semantic snapshot digest is '
        'not exact file-byte digest).',
 '023': 'Need explicit custom-resolution complete native graph rejection and retained conflicting originals '
        'recovery evidence.',
 '024': 'Explicit depth32 and counter exhaustion have tests; inclusive/one-over 64MiB,100k,4MiB,128-clock '
        'and full malformed-form corpus with zero live effects remain unproved.',
 '025': 'No public canonicalize workflow implemented or tested.',
 '026': '',
 '027': 'Backup/reopen and supported CLI restore now have product tests; lossless reverse migration of '
        'post-cutover data remains unproved.',
 '028': 'No real-key TUI storage state/retained draft/legacy fixture journey bound.',
 '029': 'No approved canonical plus complete real GUI state/container renders.',
 '030': 'No approved canonical plus complete real TUI state/container renders.',
 '031': '',
 '032': '',
 '033': '',
 '034': 'Need permissive-umask, sidecar/backup/new-directory modes, foreign-owner substitution and '
        'existing-directory non-chmod evidence.'}


def run(command, env, timeout=180):
    try:
        p = subprocess.run(command, cwd=ROOT, env=env, text=True, stdout=subprocess.PIPE,
                           stderr=subprocess.STDOUT, timeout=timeout, check=False)
        return {'command': command, 'returncode': p.returncode, 'output': p.stdout}
    except (subprocess.TimeoutExpired, OSError) as error:
        return {'command': command, 'returncode': -1, 'output': str(error)}


def evidence_check(test, commit):
    """Human/live artifacts are a separate explicit gate, not inferred from unit success."""
    path = HERE / 'product-evidence.json'
    try:
        doc = json.loads(path.read_text())
        item = doc['checks'][test]
        assert doc['commit'] == commit, 'stale source revision'
        assert item['status'] == 'pass' and item['reviewer'] and item['method'], 'missing review'
        assert item['artifacts'], 'no actual evidence artifacts'
        for artifact in item['artifacts']:
            target = (ROOT / artifact['path']).resolve()
            assert target.is_relative_to(ROOT), 'artifact escapes workspace'
            assert hashlib.sha256(target.read_bytes()).hexdigest() == artifact['sha256'], 'artifact mismatch'
        if test == 'reporter_journey':
            assert item['reporter_confirmed'] is True and item['measured_visibility_ms'] <= 2000
            assert item['source_receipt'] in [a['path'] for a in item['artifacts']]
            assert item['journeys'] == ['two_client_no_pr', 'independent_clone_exchange']
        if 'visual' in test:
            assert item['canonical'] in [a['path'] for a in item['artifacts']]
            expected = {'80x24', 'current'} if test.startswith('storage_visual') else {'narrow', 'current', 'wide'}
            assert set(item['containers']) == expected
            states = {'empty', 'legacy', 'migration_preview', 'conflict', 'migrated', 'read', 'edit', 'saving', 'success', 'failure', 'retry', 'unknown_outcome', 'stale_preview', 'read_only_recovery'}
            assert set(item['states']) == states
            covered = {(r['container'], r['state']) for r in item['renders']}
            assert covered == {(c, s) for c in expected for s in states}
            assert all(r['path'] in [a['path'] for a in item['artifacts']] and r['path'].endswith('.png') for r in item['renders'])
        return True, str(path.relative_to(ROOT)) + ':' + test
    except (OSError, ValueError, KeyError, TypeError, AssertionError) as error:
        return False, 'missing/invalid product evidence: ' + str(error)



def quality_check(commit, env, runners):
    """Execute local gates; re-query exact-source CI and recompute captured performance."""
    for key, command in (
        ('preflight', ['mise', 'run', 'preflight']),
        ('orchestrator', ['uv', 'run', '--directory', 'orchestrator', 'pytest']),
    ):
        runners[key] = run(command, env, timeout=1800)
    failed = [key for key in ('preflight', 'orchestrator') if runners[key]['returncode'] != 0]
    if failed:
        return False, 'Local quality command failed: ' + ', '.join(failed), None
    try:
        doc = json.loads((HERE / 'quality-inputs.json').read_text())
        assert doc['commit'] == commit, 'quality inputs are stale'
        # gh-axi run view has no structured JSON option; use gh for this machine oracle.
        run_id = str(doc['ci_run_id'])
        assert run_id.isdigit(), 'invalid CI run ID'
        runners['platform_ci'] = run(
            ['gh', 'run', 'view', run_id, '--repo', 'benpchandler/switchbard',
             '--json', 'headSha,status,conclusion,jobs,url'], env)
        assert runners['platform_ci']['returncode'] == 0, 'remote CI query failed or unavailable; requires existing GH_CONFIG_DIR authentication or GH_TOKEN'
        ci = json.loads(runners['platform_ci']['output'])
        assert ci['headSha'] == commit, 'CI covers a different revision'
        assert ci['status'] == 'completed' and ci['conclusion'] == 'success', 'CI not successful'
        required = {'ubuntu-latest - mise run fmt', 'ubuntu-latest - mise run clippy',
                    'ubuntu-latest - mise run test', 'macos-latest - mise run clippy',
                    'macos-latest - mise run test'}
        jobs = {j['name']: j for j in ci['jobs']}
        assert all(jobs[k]['conclusion'] == 'success' for k in required), 'missing/failed platform jobs'
        approval_artifact = doc['perf']['human_approval']
        approval_path = (ROOT / approval_artifact['path']).resolve()
        assert approval_path.is_relative_to(ROOT.resolve()), 'human approval escapes workspace'
        assert hashlib.sha256(approval_path.read_bytes()).hexdigest() == approval_artifact['sha256']
        approval = json.loads(approval_path.read_text())
        assert approval['reviewer'] and approval['reviewed_at'] and approval['approved'] is True
        assert approval['scenario'] == 'servers-scroll-smoke'
        for phase in ('before', 'after'):
            capture = doc['perf'][phase]
            assert approval['captures'][phase] == {
                'commit': capture['commit'], 'csv_sha256': capture['csv']['sha256'],
                'binary_sha256': capture['binary']['sha256']}
        metrics = {'ci_url': ci['url'], 'native_capture_provenance': 'HUMAN ATTESTATION',
                   'capture_approval': str(approval_path.relative_to(ROOT.resolve()))}
        previous = doc['perf']['before']['commit']
        assert previous != commit, 'performance baseline must precede current revision'
        ancestor = run(['git', '-C', str(ROOT), 'merge-base', '--is-ancestor', previous, commit], env)
        assert ancestor['returncode'] == 0, 'performance baseline is not an ancestor'
        with tempfile.TemporaryDirectory(prefix='switchbard-perf-verify-') as temp:
            for phase in ('before', 'after'):
                capture = doc['perf'][phase]
                assert capture['commit'] == (previous if phase == 'before' else commit)
                assert capture['scenario'] == 'servers-scroll-smoke' and capture['reviewer']
                assert capture['env']['SWITCHBARD_PERF'] == '1'
                paths = {}
                for kind in ('csv', 'binary', 'source_receipt'):
                    artifact = capture[kind]
                    target = Path(artifact['path'])
                    if not target.is_absolute():
                        target = ROOT / target
                    target = target.resolve()
                    assert target.is_relative_to(ROOT.resolve()), kind + ' artifact escapes workspace'
                    assert hashlib.sha256(target.read_bytes()).hexdigest() == artifact['sha256'], kind + ' hash mismatch'
                    paths[kind] = target
                assert Path(capture['env']['SWITCHBARD_PERF_LOG']).resolve() == paths['csv'], 'capture log mismatch'
                # Receipt records the coordinated real app build/run, not a synthetic timing.
                receipt = json.loads(paths['source_receipt'].read_text())
                assert receipt['commit'] == capture['commit'] and receipt['binary_sha256'] == capture['binary']['sha256']
                assert receipt['csv_sha256'] == capture['csv']['sha256'] and receipt['build_exit'] == 0
                assert receipt['real_app'] is True and receipt['scenario'] == 'servers-scroll-smoke'
                assert receipt['launch_command'] and receipt['capture_started_at'] and receipt['capture_finished_at']
                output = Path(temp) / (phase + '.json')
                runners['perf_' + phase] = run(
                    ['python3', 'scripts/perf-summary.py', '--csv', str(paths['csv']),
                     '--out', str(output), '--label', 'Storage acceptance ' + phase,
                     '--scenario', 'servers-scroll-smoke', '--filter', 'servers'], env)
                assert runners['perf_' + phase]['returncode'] == 0
                summary = json.loads(output.read_text())
                assert summary['selection']['summarized_frames'] > 0, 'empty performance capture'
                metrics[phase] = {key: summary['metrics_ms'][key]['p95'] for key in ('total', 'workspace')}
                assert all(math.isfinite(v) and v >= 0 for v in metrics[phase].values())
        metrics['delta_ms'] = {k: metrics['after'][k] - metrics['before'][k] for k in ('total', 'workspace')}
        # Source plan requires a measured comparison, not an invented regression percentage.
        return True, 'Real local gates, exact-source platform CI, measured p95 comparison; HUMAN ATTESTATION for actual native capture', metrics
    except (OSError, ValueError, KeyError, TypeError, AssertionError) as error:
        return False, 'Missing/failed exact-source CI or real performance evidence: ' + str(error), None



def named_test_check(runner, test):
    named = re.findall(r'^test ([^\s]+) \.\.\. (ok|FAILED|ignored)(?: .*)?$',
                       runner['output'], flags=re.MULTILINE)
    matches = [status for name, status in named if name == test]
    return runner['returncode'] == 0 and matches == ['ok'], matches


def traceability_check(contract, checks):
    expected = re.findall(r'^- \[ \] (MUST-\d+) \[([^]]+)\]', contract, flags=re.MULTILINE)
    actual = [('MUST-' + c[0], c[1]) for c in checks]
    return bool(expected) and len(set(expected)) == len(expected) and len({c[0] for c in actual}) == len(actual) and expected == actual



def main():
    if not traceability_check((HERE / 'acceptance.md').read_text(), CHECKS):
        print('FAIL: missing, duplicate or mismatched acceptance traceability', file=sys.stderr)
        return 1
    commit = subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True).strip()
    env = os.environ.copy()
    for key in list(env):
        if key.startswith('GIT_') and key != 'GIT_EXEC_PATH':
            env.pop(key)
    # Preserve build tool roots; only runtime state is isolated. No installed sb is used.
    original_home = Path.home()
    env.setdefault('CARGO_HOME', str(original_home / '.cargo'))
    env.setdefault('RUSTUP_HOME', str(original_home / '.rustup'))
    # Only the read-only CI query needs the caller's existing GitHub auth location.
    # Retain a config path (or inherited GH_TOKEN); never read or print credentials.
    env.setdefault('GH_CONFIG_DIR', str(Path(env.get('XDG_CONFIG_HOME', str(original_home / '.config'))) / 'gh'))
    env['CARGO_TARGET_DIR'] = str(Path(tempfile.gettempdir()) / ('switchbard-storage-contract-' + hashlib.sha256(str(ROOT).encode()).hexdigest()[:16]))
    runners = {}
    with tempfile.TemporaryDirectory(prefix='switchbard-storage-acceptance-') as temp:
        sandbox = Path(temp)
        for name in ('home', 'config', 'data', 'cache'):
            (sandbox / name).mkdir()
        env.update(HOME=str(sandbox / 'home'), XDG_CONFIG_HOME=str(sandbox / 'config'),
                   XDG_DATA_HOME=str(sandbox / 'data'), XDG_CACHE_HOME=str(sandbox / 'cache'),
                   SWITCHBARD_DATABASE=str(sandbox / 'isolated.sqlite3'))
        for suite, command in SUITES.items():
            runners[suite] = run(command, env, timeout=900)
        criteria = []
        for number, kind, label in CHECKS:
            support = []
            for suite, name in BINDINGS[number]:
                passed, matches = named_test_check(runners[suite], name)
                support.append({'suite': suite, 'test': name, 'status': 'pass' if passed else 'fail',
                                'runner_exit': runners[suite]['returncode'], 'named_results': matches})
            metric = None
            gap = GAPS[number]
            ok = bool(support) and all(item['status'] == 'pass' for item in support) and not gap
            evidence = gap or 'All bound actual named tests passed with no declared residual gap.'
            if number == '031':
                ok, evidence, metric = quality_check(commit, env, runners)
            elif number in ('032', '033'):
                ok, evidence = evidence_check('reporter_journey' if number == '032' else 'documentation_review', commit)
            elif kind == 'visual':
                _, evidence = evidence_check('storage_acceptance::visual_state_matrix' if number == '029' else 'storage_visual_state_matrix', commit)
                evidence = gap + '; ' + evidence
                ok = False  # No real native visual test currently bound; artifacts alone cannot pass.
            if any(item['status'] == 'fail' for item in support):
                evidence += '; missing/failed actual named tests listed in supporting_tests'
            criteria.append({'id': 'MUST-' + number, 'kind': kind, 'label': label,
                'status': 'pass' if ok else 'fail',
                'check_type': 'measurement' if kind == 'quality' else 'named-test',
                'evidence': evidence, 'metric': metric, 'supporting_tests': support,
                'residual_gap': gap})
    failures = [c['id'] for c in criteria if c['status'] != 'pass']
    # No fake all_flipped claim: first behavior GREEN needs production mutation validation.
    deferred = [c['id'] for c in criteria if c['kind'] in ('behavior', 'contract')]
    mutation_gate = {'all_flipped': False, 'mutations': [], 'deferred': deferred,
                     'reason': 'Production mutations deferred to first GREEN; each behavior and distinct schema target must flip.'}
    # Earlier artifact-only proof accepted asserted statuses without actual mutation execution.
    # Fail closed: no supplied JSON can self-certify this independent production-proof gate.
    mutation_gate['reason'] = 'Actual production mutation execution and independent revision-bound review remain unproved; artifact-only status assertions cannot pass.'
    result = {'plan': 'switchbard-owned-storage', 'result': 'FAIL' if failures else ('PASS' if mutation_gate['all_flipped'] else 'PARTIAL'),
              'commit': commit, 'timestamp': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'criteria': criteria, 'metrics': {}, 'scope_note': 'Whole-mission catalog; partial kind slices cannot satisfy aggregate criteria. Historical model/wire artifacts are not current coverage. Actual named tests are bound; residual coverage gaps and independent mutation proof remain before final GREEN.', 'artifacts': ['acceptance.md', 'verify.py', 'phased-contract.md', 'historical-evidence.md'],
              'skipped': [], 'failures': failures, 'runners': runners,
              'mutation_gate': mutation_gate}
    (HERE / 'verifier-results.json').write_text(json.dumps(result, indent=2) + '\n')
    for c in criteria:
        print(f'{c["id"]} {c["status"].upper()}: {c["label"]}')
    print(f'{result["result"]}: {len(failures)}/{len(criteria)} failed; mutation proof deferred')
    return 0 if result['result'] == 'PASS' else 1


if __name__ == '__main__':
    sys.exit(main())
