#!/usr/bin/env python3
"""Decision-stage RED and future product acceptance. Never treats the model as product."""
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
# id, kind, label, product suite, exact Rust test name, exercised CLI prerequisite
CHECKS = [
    ('001', 'behavior', 'All native domain records and workspace ordering persist across two repositories without backlog files', 'core', 'all_domain_records_reload', 'storage'),  # MUST-001
    ('002', 'behavior', 'Linked worktrees share one scope while independent repos and forks remain separate', 'core', 'repository_identity_and_explicit_binding', 'storage'),  # MUST-002
    ('003', 'behavior', 'Public locators remain compatible and RecordId survives hierarchy changes', 'core', 'stable_identity_and_locators', 'storage'),  # MUST-003
    ('004', 'behavior', 'Raw legacy bytes and unresolved optional memberships survive migration and edits', 'core', 'lossless_raw_records_and_optional_memberships', 'migration'),  # MUST-004
    ('005', 'behavior', 'Whole multi-record command commits or rolls back across processes', 'core', 'multiprocess_atomic_commands', 'storage'),  # MUST-005
    ('006', 'behavior', 'Stale revisions reject and exact replay is idempotent while mismatched replay rejects', 'core', 'revision_and_command_receipts', 'storage'),  # MUST-006
    ('007', 'contract', 'Illegal scoped record graphs reject without side effects', 'core', 'graph_constraints', 'storage'),  # MUST-007
    ('008', 'quality', 'Busy handling terminates within five seconds with retry feedback', 'core', 'busy_timeout_five_seconds', 'storage'),  # MUST-008
    ('009', 'behavior', 'Migration inventories lifecycle, sibling and branch sources with explicit reconciliation', 'core', 'migration_inventory_and_divergence', 'migration'),  # MUST-009
    ('010', 'behavior', 'Backup precedes atomic migration activation and source changes abort safely', 'core', 'migration_cutover_and_source_recheck', 'migration'),  # MUST-010
    ('011', 'behavior', 'Updated clients never write legacy files and report later legacy divergence', 'journey', 'legacy_cutover_no_fallback', 'migration'),  # MUST-011
    ('012', 'behavior', 'Normal domain edits change no repo files in clean or already dirty worktrees', 'journey', 'normal_edits_no_git_churn', 'storage'),  # MUST-012
    ('013', 'behavior', 'All consumers and headless queue agree after worktree removal', 'journey', 'consumer_and_queue_conservation', 'storage'),  # MUST-013
    ('014', 'behavior', 'Refine applies versioned raw content centrally and rejects stale results', 'core', 'refine_versioned_raw_apply', 'storage'),  # MUST-014
    ('015', 'quality', 'Active GUI and TUI see commits within two seconds with at most one poll per second', 'gui', 'storage_acceptance::active_client_visibility', 'storage'),  # MUST-015
    ('016', 'behavior', 'Refocus refreshes before writes and concurrent-edit conflicts retain drafts', 'gui', 'storage_acceptance::refocus_and_drafts', 'storage'),  # MUST-016
    ('017', 'contract', 'One deterministic v1 file round-trips every native record and excludes machine authorities', 'exchange', 'single_file_lossless_envelope', 'export'),  # MUST-017
    ('018', 'behavior', 'Export uses a consistent snapshot and atomic replacement with recoverable checkpoint ordering', 'exchange', 'atomic_export_and_checkpoint_recovery', 'export'),  # MUST-018
    ('019', 'behavior', 'Independent target edits are protected and unchanged repeat export is byte-identical', 'exchange', 'export_protection_and_idempotence', 'export'),  # MUST-019
    ('020', 'behavior', 'Explicit empty bound scope bootstraps while populated scope requires trusted ancestry', 'exchange', 'bootstrap_and_known_base', 'import'),  # MUST-020
    ('021', 'behavior', 'Three-way merge preserves omissions and detects same-record, locator and tombstone conflicts', 'exchange', 'three_way_merge_and_tombstones', 'import'),  # MUST-021
    ('022', 'behavior', 'Preview apply rechecks file digest and repository revision before atomic import', 'exchange', 'stale_preview_and_atomic_apply', 'import'),  # MUST-022
    ('023', 'behavior', 'Explicit resolution preserves originals and validates the complete resulting graph', 'exchange', 'resolution_graph_validation', 'import'),  # MUST-023
    ('024', 'contract', 'Input boundary limits and malformed or untrusted documents reject without live effects', 'exchange', 'untrusted_input_limits', 'import'),  # MUST-024
    ('025', 'behavior', 'Canonicalization repairs format without importing or trusting unknown ancestry', 'exchange', 'canonicalize_not_import', 'import'),  # MUST-025
    ('026', 'behavior', 'Unavailable or corrupt database reports failure without empty success or Markdown fallback', 'journey', 'database_failure_is_explicit', 'storage'),  # MUST-026
    ('027', 'behavior', 'Database-consistent restore retains post-cutover writes and reverse migration reconciles newer data', 'core', 'backup_restore_and_reverse_migration', 'storage'),  # MUST-027
    ('028', 'behavior', 'TUI real-key state journeys preserve drafts and existing legacy fixture behavior', 'tui', 'storage_state_and_keyboard_journey', 'storage'),  # MUST-028
    ('029', 'visual', 'GUI storage states render in narrow current and wide containers against an approved canonical', 'gui', 'storage_acceptance::visual_state_matrix', 'storage'),  # MUST-029
    ('030', 'visual', 'TUI storage states render at 80x24 and current size against an approved canonical', 'tui', 'storage_visual_state_matrix', 'storage'),  # MUST-030
    ('031', 'quality', 'Integrated implementation passes platform gates and render performance comparison', 'evidence', 'integration_gates', 'storage'),  # MUST-031
    ('032', 'behavior', 'Reporter confirms two-client no-PR workflow and independent-clone single-file exchange', 'evidence', 'reporter_journey', 'import'),  # MUST-032
    ('033', 'contract', 'Authority and compatibility documentation matches tested behavior and preserves independent authorities', 'evidence', 'documentation_review', 'storage'),  # MUST-033
    ('034', 'contract', 'Storage files and backups preserve private ownership and permissions under permissive umask', 'core', 'storage_privacy_and_path_substitution', 'storage'),  # MUST-034
]
SUITES = {
    'core': ['cargo', 'test', '-p', 'switchbard-core', '--test', 'central_storage', '--', '--color', 'never'],
    'exchange': ['cargo', 'test', '-p', 'switchbard-core', '--test', 'storage_exchange', '--', '--color', 'never'],
    'journey': ['cargo', 'test', '-p', 'switchbard-task', '--test', 'storage_journey', '--', '--color', 'never'],
    'tui': ['cargo', 'test', '-p', 'switchbard-tui', '--test', 'central_storage', '--', '--color', 'never'],
    'gui': ['cargo', 'test', '-p', 'switchbard-gui', 'storage_acceptance::', '--', '--color', 'never'],
}


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
    probes = {}
    runners = {}
    with tempfile.TemporaryDirectory(prefix='switchbard-storage-acceptance-') as temp:
        sandbox = Path(temp)
        for name in ('home', 'config', 'data', 'cache', 'repo'):
            (sandbox / name).mkdir()
        env.update(HOME=str(sandbox / 'home'), XDG_CONFIG_HOME=str(sandbox / 'config'),
                   XDG_DATA_HOME=str(sandbox / 'data'), XDG_CACHE_HOME=str(sandbox / 'cache'))
        for seam, args in {
            'storage': ['storage', '--help'],
            'migration': ['storage', 'migrate', '--help'],
            'export': ['storage', 'export', '--help'],
            'import': ['storage', 'import', '--help'],
        }.items():
            command = ['cargo', 'run', '-q', '-p', 'switchbard-task', '--', '--repo',
                       str(sandbox / 'repo'), *args]
            probes[seam] = run(command, env)
        # Prerequisite RED avoids expensive future suites when the real CLI rejects the seam.
        if all(p['returncode'] == 0 for p in probes.values()):
            for suite, command in SUITES.items():
                runners[suite] = run(command, env, timeout=600)
        criteria = []
        for number, kind, label, suite, test, seam in CHECKS:
            probe = probes[seam]
            ok = False
            metric = None
            if probe['returncode'] != 0:
                evidence = ('Observed workspace CLI prerequisite failure; full outcome remains unproved. '
                            + probe['output'].strip())
                check_type = 'api-roundtrip'
            elif test == 'integration_gates':
                ok, evidence, metric = quality_check(commit, env, runners)
                check_type = 'measurement'
            elif suite == 'evidence':
                ok, evidence = evidence_check(test, commit)
                check_type = 'measurement' if kind == 'quality' else 'named-test'
            else:
                runner = runners.get(suite, {'returncode': -1, 'output': 'prerequisite failure'})
                ok, matches = named_test_check(runner, test)
                evidence = f'{suite}::{test}: exit={runner["returncode"]}, named_results={matches}'
                check_type = 'named-test'
                if kind == 'visual':
                    visual_ok, visual_evidence = evidence_check(test, commit)
                    ok = ok and visual_ok
                    evidence += '; ' + visual_evidence
            criteria.append({'id': 'MUST-' + number, 'kind': kind, 'label': label,
                             'status': 'pass' if ok else 'fail', 'check_type': check_type,
                             'evidence': evidence, 'metric': metric, 'named_test': test})
    failures = [c['id'] for c in criteria if c['status'] != 'pass']
    # No fake all_flipped claim: first behavior GREEN needs production mutation validation.
    deferred = [c['id'] for c in criteria if c['kind'] in ('behavior', 'contract')]
    mutation_gate = {'all_flipped': False, 'mutations': [], 'deferred': deferred,
                     'reason': 'Production mutations deferred to first GREEN; each behavior and distinct schema target must flip.'}
    if not failures:
        try:
            proof = json.loads((HERE / 'mutation-evidence.json').read_text())
            assert proof['commit'] == commit
            entries = {entry['criterion_id']: entry for entry in proof['mutations']}
            assert len(entries) == len(proof['mutations'])
            for criterion in deferred:
                entry = entries[criterion]
                assert entry['flipped'] is True and entry['restored'] is True
                assert entry['mutation_file'].startswith('crates/') and entry['mutation_diff']
                for phase, expected in (('before', 'pass'), ('mutated', 'fail'), ('restored', 'pass')):
                    artifact = entry['runs'][phase]
                    target = (ROOT / artifact['path']).resolve()
                    assert target.is_relative_to(ROOT)
                    assert hashlib.sha256(target.read_bytes()).hexdigest() == artifact['sha256']
                    run_result = json.loads(target.read_text())
                    matched = [c for c in run_result['criteria'] if c['id'] == criterion]
                    assert len(matched) == 1 and matched[0]['status'] == expected
            mutation_gate = {'all_flipped': True, 'mutations': proof['mutations'], 'deferred': []}
        except (OSError, ValueError, KeyError, TypeError, AssertionError) as error:
            mutation_gate['reason'] = 'Missing/invalid production mutation evidence: ' + str(error)
    result = {'plan': 'switchbard-owned-storage', 'result': 'FAIL' if failures else ('PASS' if mutation_gate['all_flipped'] else 'PARTIAL'),
              'commit': commit, 'timestamp': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'criteria': criteria, 'metrics': {}, 'artifacts': ['acceptance.md', 'verify.py'],
              'skipped': [], 'failures': failures, 'probes': probes, 'runners': runners,
              'mutation_gate': mutation_gate}
    (HERE / 'verifier-results.json').write_text(json.dumps(result, indent=2) + '\n')
    for c in criteria:
        print(f'{c["id"]} {c["status"].upper()}: {c["label"]}')
    print(f'{result["result"]}: {len(failures)}/{len(criteria)} failed; mutation proof deferred')
    return 0 if result['result'] == 'PASS' else 1


if __name__ == '__main__':
    sys.exit(main())
