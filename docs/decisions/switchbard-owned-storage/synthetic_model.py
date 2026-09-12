#!/usr/bin/env python3
"""HISTORICAL SUPERSEDED PROPOSAL. Not normative for phased flexible storage.
Decision-only synthetic algorithm, NOT product storage or implementation proof.

Run from any cwd. Only a temporary SQLite database and synthetic-results.json
are written. Fixture action counts, record counts, and envelope bytes are bounded.
Record equality is deliberately conservative: unrelated fields of the same record
still conflict. The input fixture protocol is internal, not a proposed public API.
"""
import copy
import hashlib
import json
from pathlib import Path
import re
import sqlite3
import tempfile
import uuid

HERE = Path(__file__).resolve().parent
MAX_RECORDS = 100
MAX_BYTES = 262144
REPO = '10000000-0000-4000-8000-000000000001'
OTHER = '10000000-0000-4000-8000-000000000002'


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False)


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def valid_uuid(value):
    try:
        require(str(uuid.UUID(value)) == value, 'identity')
    except (AttributeError, TypeError, ValueError) as error:
        raise ValueError('identity') from error


def validate_record(record):
    require(isinstance(record, dict), 'record-shape')
    require(set(record) == {'id', 'display_id', 'title', 'deleted', 'raw', 'source'}, 'record-shape')
    valid_uuid(record['id'])
    require(isinstance(record['display_id'], str) and
            re.fullmatch(r'TASK-[1-9][0-9]*(?:\.[1-9][0-9]*)?', record['display_id']), 'display-id')
    require(type(record['deleted']) is bool, 'tombstone-shape')
    for field in ('title', 'raw', 'source'):
        require(isinstance(record[field], str) and len(record[field]) <= 8192, 'record-text')


def snapshot_digest(repo, records):
    return hashlib.sha256(canonical({'version': 1, 'repo_id': repo, 'records': records}).encode()).hexdigest()


def validate_envelope(envelope, repo):
    require(len(canonical(envelope).encode()) <= MAX_BYTES, 'size')
    require(set(envelope) == {'version', 'repo_id', 'base', 'base_records', 'records', 'digest'}, 'envelope-shape')
    require(type(envelope['version']) is int and envelope['version'] == 1, 'version')
    valid_uuid(envelope['repo_id'])
    require(envelope['repo_id'] == repo, 'foreign-repo')
    require(envelope['base'] is None or (isinstance(envelope['base'], str) and len(envelope['base']) == 64), 'base-shape')
    require(envelope['digest'] == snapshot_digest(repo, envelope['records']), 'snapshot-digest')
    if envelope['base'] is None:
        require(envelope['base_records'] is None, 'base-shape')
    else:
        require(isinstance(envelope['base_records'], list), 'base-shape')
        require(len(envelope['base_records']) <= MAX_RECORDS, 'record-limit')
        for record in envelope['base_records']:
            validate_record(record)
        require(envelope['base'] == snapshot_digest(repo, envelope['base_records']), 'base-digest')
    rows = envelope['records']
    require(isinstance(rows, list) and len(rows) <= MAX_RECORDS, 'record-limit')
    for row in rows:
        validate_record(row)
    require(len({row['id'] for row in rows}) == len(rows), 'duplicate-identity')
    require(len({row['display_id'] for row in rows}) == len(rows), 'display-collision')


class Model:
    def __init__(self, path):
        self.db = sqlite3.connect(path)
        self.db.executescript('''
            CREATE TABLE repos(id TEXT PRIMARY KEY);
            CREATE TABLE aliases(path TEXT PRIMARY KEY, repo TEXT NOT NULL REFERENCES repos(id));
            CREATE TABLE records(repo TEXT, id TEXT, display TEXT, body TEXT,
                PRIMARY KEY(repo,id), UNIQUE(repo,display));
            CREATE TABLE snapshots(repo TEXT, digest TEXT, body TEXT, PRIMARY KEY(repo,digest));
            CREATE TABLE imports(repo TEXT, digest TEXT, PRIMARY KEY(repo,digest));
            CREATE TABLE exports(repo TEXT PRIMARY KEY, body TEXT);
            CREATE TABLE revisions(repo TEXT PRIMARY KEY, seq INTEGER);
            CREATE TRIGGER record_insert AFTER INSERT ON records BEGIN
                INSERT INTO revisions VALUES (NEW.repo,1)
                ON CONFLICT(repo) DO UPDATE SET seq=seq+1;
            END;
            CREATE TRIGGER record_update AFTER UPDATE ON records BEGIN
                UPDATE revisions SET seq=seq+1 WHERE repo=NEW.repo;
            END;
        ''')
        self.db.execute('PRAGMA foreign_keys=ON')
        with self.db:
            self.db.executemany('INSERT INTO repos VALUES (?)', [(REPO,), (OTHER,)])

    def state(self):
        return {table: self.db.execute(f'SELECT * FROM {table} ORDER BY 1,2').fetchall()
                for table in ('aliases', 'records', 'snapshots', 'imports', 'exports', 'revisions')}

    def records(self, repo):
        return [json.loads(row[0]) for row in self.db.execute(
            'SELECT body FROM records WHERE repo=? ORDER BY id', (repo,))]

    def put(self, repo, record):
        validate_record(record)
        self.db.execute('INSERT INTO records VALUES (?,?,?,?) ON CONFLICT(repo,id) '
                        'DO UPDATE SET display=excluded.display,body=excluded.body',
                        (repo, record['id'], record['display_id'], canonical(record)))

    def snapshot(self, repo):
        records = self.records(repo)
        previous = self.db.execute('SELECT body FROM exports WHERE repo=?', (repo,)).fetchone()
        prior = json.loads(previous[0]) if previous else None
        if prior and prior['records'] == records:
            return prior
        digest = snapshot_digest(repo, records)
        envelope = {'version': 1, 'repo_id': repo, 'base': prior['digest'] if prior else None,
                    'base_records': prior['records'] if prior else None,
                    'records': records, 'digest': digest}
        with self.db:
            self.checkpoint(repo, records)
            self.db.execute('INSERT OR REPLACE INTO exports VALUES (?,?)', (repo, canonical(envelope)))
        return envelope

    def checkpoint(self, repo, records):
        self.db.execute('INSERT OR IGNORE INTO snapshots VALUES (?,?,?)',
                        (repo, snapshot_digest(repo, records), canonical(records)))

    def bootstrap(self, repo, envelope):
        validate_envelope(envelope, repo)
        with self.db:
            self.db.execute('UPDATE repos SET id=id WHERE id=?', (repo,))
            require(not self.records(repo), 'bootstrap-nonempty')
            for record in envelope['records']:
                self.put(repo, record)
            self.checkpoint(repo, envelope['records'])

    def revision(self, repo):
        row = self.db.execute('SELECT seq FROM revisions WHERE repo=?', (repo,)).fetchone()
        return row[0] if row else 0

    def preview(self, repo, envelope):
        validate_envelope(envelope, repo)
        return {'revision': self.revision(repo),
                'source': hashlib.sha256(canonical(envelope).encode()).hexdigest()}

    def apply_preview(self, repo, envelope, plan):
        with self.db:
            self.db.execute('UPDATE repos SET id=id WHERE id=?', (repo,))
            require(plan['revision'] == self.revision(repo), 'stale-preview')
            require(plan['source'] == hashlib.sha256(canonical(envelope).encode()).hexdigest(), 'source-changed')
            return self.merge(repo, envelope)

    def merge(self, repo, envelope, fail_after=None):
        validate_envelope(envelope, repo)
        with self.db:
            self.db.execute('UPDATE repos SET id=id WHERE id=?', (repo,))  # Acquire write lock before reads.
            row = self.db.execute('SELECT body FROM snapshots WHERE repo=? AND digest=?',
                                  (repo, envelope['base'])).fetchone()
            require(row is not None, 'unknown-base')
            base = {item['id']: item for item in json.loads(row[0])}
            current = {item['id']: item for item in self.records(repo)}
            changes = []
            for incoming in envelope['records']:
                key = incoming['id']
                old, local = base.get(key), current.get(key)
                require(not (incoming['deleted'] and old is None), 'unknown-tombstone')
                if local == incoming or incoming == old:
                    continue
                require(local == old, 'record-conflict')
                changes.append(incoming)
            for index, incoming in enumerate(changes, 1):
                self.put(repo, incoming)
                require(index != fail_after, 'forced-failure')
            self.checkpoint(repo, envelope['records'])
            digest = hashlib.sha256(canonical(envelope).encode()).hexdigest()
            self.db.execute('INSERT OR IGNORE INTO imports VALUES (?,?)', (repo, digest))
        return len(changes)


def seed(model):
    with model.db:
        for number in (1, 2):
            model.put(REPO, {'id': f'20000000-0000-4000-8000-{number:012d}',
                            'display_id': f'TASK-{number}', 'title': f'Synthetic {number}',
                            'deleted': False, 'raw': f'---\nid: TASK-{number}\n---\nOriginal bytes\n',
                            'source': f'synthetic/worktree-a/backlog/task-{number}.md'})
    return model.snapshot(REPO)


def edit_envelope(envelope, step):
    result = copy.deepcopy(envelope)
    if not step.get('raw_snapshot'):
        result['base'] = envelope['digest']
        result['base_records'] = copy.deepcopy(envelope['records'])
    result.update(step.get('envelope', {}))
    for edit in step.get('edits', []):
        result['records'][edit['index']].update(edit['set'])
    if 'omit' in step:
        result['records'] = [row for index, row in enumerate(result['records']) if index not in step['omit']]
    if step.get('duplicate'):
        result['records'].append(copy.deepcopy(result['records'][0]))
    if step.get('over_limit'):
        result['records'] *= MAX_RECORDS
    if not step.get('bad_digest'):
        result['digest'] = snapshot_digest(result['repo_id'], result['records'])
    return result


def execute(model, saved, step):
    operation = step['op']
    if operation == 'local':
        row = model.records(REPO)[step['index']]
        row.update(step['set'])
        with model.db:
            model.put(REPO, row)
    elif operation == 'import':
        envelope = edit_envelope(saved[step.get('snapshot', 'base')], step)
        count = model.merge(REPO, envelope, step.get('fail_after'))
        require(count == step['changes'], 'unexpected-change-count')
    elif operation == 'bootstrap':
        envelope = copy.deepcopy(saved['base'])
        repo = REPO if step.get('nonempty') else OTHER
        envelope['repo_id'] = repo
        envelope['digest'] = snapshot_digest(repo, envelope['records'])
        model.bootstrap(repo, envelope)
        require(model.records(repo) == envelope['records'], 'bootstrap-records')
        checkpoint = model.db.execute('SELECT body FROM snapshots WHERE repo=? AND digest=?',
                                      (repo, envelope['digest'])).fetchone()
        require(checkpoint and json.loads(checkpoint[0]) == envelope['records'], 'bootstrap-checkpoint')
    elif operation == 'preview':
        saved['preview_source'] = edit_envelope(saved['base'], step)
        saved['preview'] = model.preview(REPO, saved['preview_source'])
    elif operation == 'apply_preview':
        envelope = copy.deepcopy(saved['preview_source'])
        if step.get('source_change'):
            envelope['records'][0]['title'] = 'Changed after preview'
            envelope['digest'] = snapshot_digest(REPO, envelope['records'])
        count = model.apply_preview(REPO, envelope, saved['preview'])
        require(count == step.get('changes', 1), 'preview-change-count')
    elif operation == 'export':
        saved[step['name']] = model.snapshot(REPO)
    elif operation == 'assert':
        rows = model.records(REPO)
        for item in step.get('records', []):
            require(all(rows[item['index']][key] == value for key, value in item['fields'].items()), 'record-assertion')
        if 'snapshot_stable' in step:
            require(canonical(saved[step['snapshot_stable']]) == saved['_base_bytes'], 'snapshot-mutated')
        if 'equal_snapshots' in step:
            left, right = step['equal_snapshots']
            require(canonical(saved[left]) == canonical(saved[right]), 'nondeterministic-export')
        require(len(rows) == step.get('count', len(rows)), 'record-count')
    elif operation == 'reuse':
        with model.db:
            for path in ('synthetic/main', 'synthetic/worktree-b'):
                model.db.execute('INSERT INTO aliases VALUES (?,?)', (path, REPO))
            model.put(OTHER, model.records(REPO)[0])
        require(model.records(OTHER)[0]['display_id'] == 'TASK-1', 'repo-display-scope')
        aliases = model.db.execute('SELECT DISTINCT repo FROM aliases').fetchall()
        require(aliases == [(REPO,)], 'alias-reuse')
    elif operation == 'assert_aliases':
        rows = []
        for path in ('synthetic/main', 'synthetic/worktree-b'):
            repo = model.db.execute('SELECT repo FROM aliases WHERE path=?', (path,)).fetchone()[0]
            rows.append(model.records(repo))
        require(rows[0] == rows[1] and rows[0][0]['title'] == step['title'], 'alias-stale')
    else:
        raise ValueError('fixture-operation')


def run_case(case, directory):
    model = Model(Path(directory) / (case['id'] + '.sqlite'))
    base = seed(model)
    saved = {'base': base, '_base_bytes': canonical(base)}
    require(len(case['steps']) <= 30, 'fixture-step-limit')
    rejected = 0
    try:
        for step in case['steps']:
            before = model.state()
            try:
                execute(model, saved, step)
            except (ValueError, sqlite3.IntegrityError) as error:
                expected = step.get('reject')
                require(expected is not None and expected in str(error), f'unexpected-rejection: {error}')
                require(model.state() == before, 'rejection-had-durable-effects')
                rejected += 1
            else:
                require('reject' not in step, 'invalid-case-accepted')
                if step.get('unchanged'):
                    require(model.state() == before, 'replay-had-durable-effects')
        return {'id': case['id'], 'status': 'pass', 'rejections_without_effects': rejected}
    finally:
        model.db.close()


def main():
    results = []
    with tempfile.TemporaryDirectory(prefix='switchbard-decision-synthetic-') as directory:
        for filename in ('synthetic-model.json', 'synthetic-invalid-cases.json'):
            fixture = json.loads((HERE / filename).read_text())
            require(len(fixture['cases']) <= 100, 'fixture-case-limit')
            for case in fixture['cases']:
                try:
                    results.append(run_case(case, directory))
                except (ValueError, sqlite3.Error, KeyError, IndexError, TypeError) as error:
                    results.append({'id': case['id'], 'status': 'fail', 'error': str(error)})
    report = {'evidence_scope': 'Proposed synthetic merge algorithm only; no product implementation proof.',
              'model_limits': ['Tasks only; no full hierarchy graph or migration model.',
                               'Small synthetic size limits; not production v1 limit tests.',
                               'Raw text/source fixtures, not the production base64/privacy envelope.',
                               'SQLite-only export checkpoints; no filesystem replacement/crash protocol.',
                               'Source change is modeled by envelope bytes; no real file preview.',
                               'Repository binding is synthetic; no Git common-directory detection.'],
              'passed': sum(row['status'] == 'pass' for row in results),
              'failed': sum(row['status'] == 'fail' for row in results), 'cases': results}
    (HERE / 'synthetic-results.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    return int(report['failed'] != 0)


if __name__ == '__main__':
    raise SystemExit(main())
