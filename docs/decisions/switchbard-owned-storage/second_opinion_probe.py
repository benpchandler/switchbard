#!/usr/bin/env python3
"""Reproduce second-opinion concerns in disposable model stores, not the app."""
import importlib.util
import json
from pathlib import Path
import tempfile

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('storage_model', HERE / 'synthetic_model.py')
model = importlib.util.module_from_spec(spec)
spec.loader.exec_module(model)


def change_title(peer, title):
    row = peer.records(model.REPO)[0]
    row['title'] = title
    with peer.db:
        peer.put(model.REPO, row)


def import_result(peer, envelope):
    try:
        peer.merge(model.REPO, envelope)
        return 'accepted'
    except ValueError as error:
        return str(error)


def probe_bootstrap():
    with tempfile.TemporaryDirectory() as root:
        a = model.Model(Path(root) / 'a.sqlite')
        b = model.Model(Path(root) / 'b.sqlite')
        baseline = model.seed(a)
        b.bootstrap(model.REPO, baseline)
        change_title(b, 'B edit')
        exported = b.snapshot(model.REPO)
        return {'case': 'bootstrap-edit-export-back', 'export_base': exported['base'],
                'receiver_result': import_result(a, exported),
                'scope': 'synthetic implementation; contract says accepted checkpoint should be usable'}


def probe_skipped_export():
    with tempfile.TemporaryDirectory() as root:
        a = model.Model(Path(root) / 'a.sqlite')
        b = model.Model(Path(root) / 'b.sqlite')
        baseline = model.seed(a)
        b.bootstrap(model.REPO, baseline)
        for title in ('first private export', 'second export to share'):
            change_title(a, title)
            exported = a.snapshot(model.REPO)
        return {'case': 'skip-unshared-intermediate-export',
                'receiver_result': import_result(b, exported),
                'scope': 'contract known-base rule; receiver unchanged since common baseline'}


if __name__ == '__main__':
    print(json.dumps([probe_bootstrap(), probe_skipped_export()], indent=2))
