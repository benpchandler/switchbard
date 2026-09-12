#!/usr/bin/env python3
"""HISTORICAL SUPERSEDED PROPOSAL. Not normative for phased flexible storage.
Proposed wire-format vectors only; not product import or database proof."""
import base64
import binascii
import hashlib
import json
from pathlib import Path
import re
import sys

HERE = Path(__file__).resolve().parent
MAX_BYTES = 64 * 1024 * 1024
MAX_RECORDS = 100_000
MAX_PAYLOAD = 4 * 1024 * 1024
UUID = r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}'
HEX = r'[0-9a-f]{64}'


def require(test, code):
    if not test:
        raise ValueError(code)


def reject_number(value):
    raise ValueError('non-integer-number')


def object_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate-key')
        result[key] = value
    return result


def string(value):
    require(isinstance(value, str), 'string')
    require(not any(0xD800 <= ord(character) <= 0xDFFF for character in value), 'surrogate')


def inspect_tree(value, depth=0):
    if isinstance(value, dict):
        require(depth + 1 <= 32, 'depth')
        for key, item in value.items():
            string(key)
            require(key.isascii(), 'non-ascii-key')
            inspect_tree(item, depth + 1)
    elif isinstance(value, list):
        require(depth + 1 <= 32, 'depth')
        for item in value:
            inspect_tree(item, depth + 1)
    elif isinstance(value, str):
        string(value)
    else:
        require(value is None or type(value) in (int, bool), 'non-integer-number')


def parse(data):
    require(len(data) <= MAX_BYTES, 'file-size')
    require(not data.startswith(b'\xef\xbb\xbf'), 'bom')
    try:
        value = json.loads(data.decode('utf-8'), object_pairs_hook=object_pairs,
                           parse_float=reject_number, parse_constant=reject_number)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise ValueError('json') from error
    inspect_tree(value)
    return value


def compact(value):
    inspect_tree(value)
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':'),
                      allow_nan=False).encode('utf-8')


def digest(repo, records):
    return hashlib.sha256(compact({'format': 'switchbard.tasks', 'version': 1,
                                  'repo_id': repo, 'records': records})).hexdigest()


def pretty(envelope):
    inspect_tree(envelope)
    return (json.dumps(envelope, sort_keys=True, ensure_ascii=False, indent=2,
                       allow_nan=False) + '\n').encode('utf-8')


def keys(value, expected):
    require(isinstance(value, dict) and set(value) == set(expected.split()), 'closed-shape')


def matching(value, pattern, code):
    require(isinstance(value, str) and re.fullmatch(pattern, value) is not None, code)


def validate_link(link, repo):
    require(isinstance(link, dict), 'link')
    if link.get('state') == 'resolved':
        keys(link, 'relation state repo_id record_id')
        matching(link['record_id'], UUID, 'record-id')
    elif link.get('state') == 'unresolved_optional':
        keys(link, 'relation state repo_id original_name')
        string(link['original_name'])
        require(0 < len(link['original_name']) <= 1024, 'original-name')
    else:
        raise ValueError('link-state')
    matching(link['relation'], r'[a-z][a-z0-9_]{0,63}', 'relation')
    require(link['repo_id'] == repo, 'cross-scope-link')


def validate_record(record, repo):
    keys(record, 'record_id kind public_id lifecycle tombstone raw_encoding raw_payload links')
    matching(record['record_id'], UUID, 'record-id')
    require(record['kind'] in ('task', 'project', 'initiative', 'goal', 'ranking', 'config'), 'kind')
    if record['public_id'] is not None:
        string(record['public_id'])
        require(0 < len(record['public_id']) <= 256, 'public-id')
    require(record['lifecycle'] in ('active', 'completed', 'draft', 'archived', 'tombstoned'), 'lifecycle')
    require(type(record['tombstone']) is bool and
            record['tombstone'] == (record['lifecycle'] == 'tombstoned'), 'tombstone')
    if record['kind'] == 'config':
        require(record['public_id'] == 'task-config', 'config-role')
        require(record['lifecycle'] == 'active' and not record['tombstone'], 'config-lifecycle')
    require(record['raw_encoding'] == 'base64', 'raw-encoding')
    string(record['raw_payload'])
    try:
        raw = base64.b64decode(record['raw_payload'], validate=True)
    except (ValueError, binascii.Error) as error:
        raise ValueError('base64') from error
    require(base64.b64encode(raw).decode('ascii') == record['raw_payload'], 'noncanonical-base64')
    require(len(raw) <= MAX_PAYLOAD, 'payload-size')
    require(isinstance(record['links'], list), 'links')
    for link in record['links']:
        validate_link(link, repo)
    encoded_links = [compact(link) for link in record['links']]
    require(encoded_links == sorted(set(encoded_links)), 'link-order-or-duplicate')
    return raw


def validate_snapshot(records, repo):
    require(isinstance(records, list), 'records')
    require(sum(record.get('kind') == 'config' for record in records if isinstance(record, dict)) <= 1,
            'config-singleton')
    for record in records:
        validate_record(record, repo)
    ids = [record['record_id'] for record in records]
    require(ids == sorted(set(ids)), 'record-order-or-duplicate')
    public = [(record['kind'], record['public_id']) for record in records if record['public_id'] is not None]
    require(len(public) == len(set(public)), 'public-id-collision')


def validate(envelope):
    keys(envelope, 'version repo_id digest base base_records records')
    require(type(envelope['version']) is int and envelope['version'] == 1, 'version')
    matching(envelope['repo_id'], UUID, 'repo-id')
    matching(envelope['digest'], HEX, 'digest-shape')
    require(isinstance(envelope['records'], list), 'records')
    base_rows = envelope['base_records']
    require(base_rows is None or isinstance(base_rows, list), 'base-records')
    require(len(envelope['records']) + len(base_rows or []) <= MAX_RECORDS, 'record-limit')
    validate_snapshot(envelope['records'], envelope['repo_id'])
    require(envelope['digest'] == digest(envelope['repo_id'], envelope['records']), 'snapshot-digest')
    if envelope['base'] is None:
        require(base_rows is None, 'base-pair')
    else:
        matching(envelope['base'], HEX, 'base-digest-shape')
        require(base_rows is not None, 'base-pair')
        validate_snapshot(base_rows, envelope['repo_id'])
        require(envelope['base'] == digest(envelope['repo_id'], base_rows), 'base-digest')
        base_config = next((record for record in base_rows if record['kind'] == 'config'), None)
        if base_config is not None:
            current_config = next((record for record in envelope['records'] if record['kind'] == 'config'), None)
            require(current_config is not None and current_config['record_id'] == base_config['record_id'],
                    'config-continuity')


def run_vectors(vectors):
    results = []
    for case in vectors['cases']:
        data = case['file_utf8'].encode('utf-8')
        try:
            envelope = parse(data)
            validate(envelope)
            require('reject' not in case, 'expected-rejection')
            require(pretty(envelope) == data, 'golden-export-bytes')
            require(hashlib.sha256(data).hexdigest() == case['file_sha256'], 'file-digest')
            require(envelope['digest'] == case['snapshot_sha256'], 'golden-snapshot-digest')
            require(envelope['base'] == case['base_sha256'], 'golden-base-digest')
            if 'raw_hex' in case:
                raw = validate_record(envelope['records'][0], envelope['repo_id'])
                require(raw.hex() == case['raw_hex'], 'raw-byte-preservation')
        except ValueError as error:
            if case.get('reject') == str(error):
                results.append({'id': case['id'], 'status': 'pass', 'rejection': str(error)})
            else:
                results.append({'id': case['id'], 'status': 'fail', 'error': str(error)})
        else:
            results.append({'id': case['id'], 'status': 'pass'})
    return results


def main():
    vectors = json.loads((HERE / 'exchange-v1-vectors.json').read_text())
    results = run_vectors(vectors)
    report = {'scope': 'Wire-format specification vectors only; not product parser, import, or database proof.',
              'passed': sum(result['status'] == 'pass' for result in results),
              'failed': sum(result['status'] == 'fail' for result in results), 'cases': results}
    (HERE / 'exchange-v1-vector-results.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    return int(report['failed'] != 0)


if __name__ == '__main__':
    sys.exit(main())
