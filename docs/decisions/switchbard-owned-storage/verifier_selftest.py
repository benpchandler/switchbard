#!/usr/bin/env python3
"""Acceptance harness tests only. Controlled results are NOT product evidence."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

# Avoid any generated file in the decision package.
import sys
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location('storage_verifier', Path(__file__).with_name('verify.py'))
V = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(V)


class VerifierTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='storage-verifier-selftest-')
        self.root = Path(self.temp.name)
        self.patches = [patch.object(V, 'ROOT', self.root), patch.object(V, 'HERE', self.root)]
        for p in self.patches:
            p.start()
        self.ci_sha = 'new'
        self.timing = 2.0
        self.gate_exit = 0
        self.calls = []
        self.prepare_quality()

    def tearDown(self):
        for p in reversed(self.patches):
            p.stop()
        self.temp.cleanup()

    def artifact(self, name, data):
        target = self.root / name
        target.write_bytes(data)
        return {'path': str(target), 'sha256': hashlib.sha256(data).hexdigest()}

    def prepare_quality(self):
        doc = {'commit': 'new', 'ci_run_id': 123, 'perf': {}}
        for phase, commit in [('before', 'old'), ('after', 'new')]:
            csv = self.artifact(phase + '.csv', b'controlled selftest capture, not product data\n')
            binary = self.artifact(phase + '.bin', b'controlled selftest binary placeholder')
            receipt = self.artifact(phase + '.receipt.json', json.dumps({
                'commit': commit, 'binary_sha256': binary['sha256'], 'csv_sha256': csv['sha256'],
                'build_exit': 0, 'real_app': True, 'scenario': 'servers-scroll-smoke',
                'launch_command': 'selftest placeholder', 'capture_started_at': 'start',
                'capture_finished_at': 'end'}).encode())
            doc['perf'][phase] = {'commit': commit, 'scenario': 'servers-scroll-smoke',
                'reviewer': 'selftest', 'env': {'SWITCHBARD_PERF': '1', 'SWITCHBARD_PERF_LOG': csv['path']},
                'csv': csv, 'binary': binary, 'source_receipt': receipt}
        approval = {'reviewer': 'selftest controlled reviewer', 'reviewed_at': 'selftest time',
                    'approved': True, 'scenario': 'servers-scroll-smoke', 'captures': {
                        phase: {'commit': capture['commit'], 'csv_sha256': capture['csv']['sha256'],
                                'binary_sha256': capture['binary']['sha256']}
                        for phase, capture in doc['perf'].items()}}
        doc['perf']['human_approval'] = self.artifact('human-approval.json', json.dumps(approval).encode())
        (self.root / 'quality-inputs.json').write_text(json.dumps(doc))

    def runner(self, command, env, timeout=180):
        self.calls.append(command)
        output = ''
        rc = 0
        if command[0] in ('mise', 'uv'):
            rc = self.gate_exit
        elif command[0] == 'gh':
            output = json.dumps({'headSha': self.ci_sha, 'status': 'completed', 'conclusion': 'success',
                'url': 'https://example.invalid/selftest', 'jobs': [
                    {'name': name, 'conclusion': 'success'} for name in (
                    'ubuntu-latest - mise run fmt', 'ubuntu-latest - mise run clippy',
                    'ubuntu-latest - mise run test', 'macos-latest - mise run clippy',
                    'macos-latest - mise run test')]})
        elif command[0] == 'python3':
            target = Path(command[command.index('--out') + 1])
            target.write_text(json.dumps({'selection': {'summarized_frames': 3},
                'metrics_ms': {'total': {'p95': self.timing}, 'workspace': {'p95': self.timing}}}))
        return {'command': command, 'returncode': rc, 'output': output}

    def quality(self):
        with patch.object(V, 'run', self.runner):
            return V.quality_check('new', {}, {})

    def test_valid_timing_branch(self):
        ok, _, metrics = self.quality()
        self.assertTrue(ok)
        self.assertEqual(metrics['delta_ms'], {'total': 0.0, 'workspace': 0.0})
        self.assertEqual(sum(c[0] == 'mise' for c in self.calls), 1)
        self.assertEqual(sum(c[0] == 'uv' for c in self.calls), 1)

    def test_malformed_and_nonfinite_timing_fail(self):
        for value in ('bad', None, float('nan'), float('inf'), -1.0):
            with self.subTest(value=value):
                self.timing = value
                self.assertFalse(self.quality()[0])

    def test_missing_human_approval_fails(self):
        path = self.root / 'quality-inputs.json'
        doc = json.loads(path.read_text())
        del doc['perf']['human_approval']
        path.write_text(json.dumps(doc))
        self.assertFalse(self.quality()[0])

    def test_outside_workspace_artifact_fails(self):
        with tempfile.TemporaryDirectory(prefix='outside-verifier-selftest-') as outside:
            external = Path(outside) / 'capture.csv'
            external.write_bytes(b'controlled outside file')
            path = self.root / 'quality-inputs.json'
            doc = json.loads(path.read_text())
            # Preserve approved digest metadata to ensure path confinement is the observed rejection.
            doc['perf']['before']['csv']['path'] = str(external)
            path.write_text(json.dumps(doc))
            ok, reason, _ = self.quality()
            self.assertFalse(ok)
            self.assertIn('escapes workspace', reason)

    def test_symlink_escape_fails(self):
        with tempfile.TemporaryDirectory(prefix='outside-verifier-selftest-') as outside:
            external = Path(outside) / 'capture.csv'
            external.write_bytes(b'controlled outside file')
            link = self.root / 'escape.csv'
            link.symlink_to(external)
            path = self.root / 'quality-inputs.json'
            doc = json.loads(path.read_text())
            doc['perf']['before']['csv']['path'] = str(link)
            path.write_text(json.dumps(doc))
            ok, reason, _ = self.quality()
            self.assertFalse(ok)
            self.assertIn('escapes workspace', reason)

    def test_missing_quality_inputs_fail(self):
        (self.root / 'quality-inputs.json').unlink()
        self.assertFalse(self.quality()[0])

    def test_failed_local_gates_fail(self):
        self.gate_exit = 2
        self.assertFalse(self.quality()[0])
        self.assertFalse(any(c[0] == 'gh' for c in self.calls))

    def test_wrong_ci_sha_fails(self):
        self.ci_sha = 'other'
        self.assertFalse(self.quality()[0])

    def test_named_test_fail_closed(self):
        cases = [(0, 'test needed ... ok\n', True),
                 (0, 'test other ... ok\n', False),
                 (1, 'test needed ... ok\n', False),
                 (0, 'test needed ... ignored\n', False),
                 (0, 'test needed ... ok\ntest needed ... ok\n', False)]
        for rc, output, expected in cases:
            self.assertEqual(V.named_test_check({'returncode': rc, 'output': output}, 'needed')[0], expected)

    def test_traceability_missing_duplicate_and_wrong_kind_fail(self):
        contract = '- [ ] MUST-001 [behavior] A\n- [ ] MUST-002 [contract] B\n'
        checks = [('001', 'behavior'), ('002', 'contract')]
        self.assertTrue(V.traceability_check(contract, checks))
        self.assertFalse(V.traceability_check(contract, checks[:1]))
        self.assertFalse(V.traceability_check(contract, checks + checks[:1]))
        self.assertFalse(V.traceability_check(contract + contract, checks))
        self.assertFalse(V.traceability_check(contract, [('001', 'visual'), checks[1]]))
        self.assertFalse(V.traceability_check('', []))

    def test_missing_and_stale_human_evidence_fail(self):
        self.assertFalse(V.evidence_check('reporter_journey', 'new')[0])
        (self.root / 'product-evidence.json').write_text(json.dumps({'commit': 'old', 'checks': {
            'reporter_journey': {'status': 'pass', 'reviewer': 'selftest', 'method': 'selftest'}}}))
        self.assertFalse(V.evidence_check('reporter_journey', 'new')[0])


if __name__ == '__main__':
    unittest.main(verbosity=2)
