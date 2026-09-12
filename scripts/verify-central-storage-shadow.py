#!/usr/bin/env python3
"""Rehearse eligible native migrations in one disposable DB without touching sources."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

KINDS = ("initiative", "project", "config", "ranking", "goals", "task")
VIEWS = (("list",), ("project", "list"), ("initiative", "list"), ("goal", "list"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--repo", action="append", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    binary_digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    temporary = Path(tempfile.mkdtemp(prefix="switchbard-storage-rehearsal-"))
    database = temporary / "shadow.sqlite3"
    environment = dict(os.environ, SWITCHBARD_DATABASE=str(database))

    def run(root, *flags):
        try:
            result = subprocess.run([str(binary), "--repo", str(root), *flags],
                                    env=environment, capture_output=True, timeout=180)
            return result.returncode, result.stdout, result.stderr.decode(errors="replace").strip()
        except subprocess.TimeoutExpired:
            return 124, b"", "command exceeded the 180-second rehearsal limit"

    report = {"mode": "disposable shared database; no default database opened",
              "database": str(database), "binary": str(binary),
              "binary_sha256": binary_digest, "repositories": []}
    failures = []
    for root in args.repo:
        root = root.resolve(strict=True)
        views_before = {" ".join(view): run(root, *view) for view in VIEWS}
        sources = {}
        phases = []
        for kind in KINDS:
            code, output, error = run(root, "storage", "migrate", "--kind", kind)
            if code:
                phases.append({"kind": kind, "status": "held", "reason": error})
                continue
            preview = json.loads(output)
            for source in preview["sources"]:
                path = Path(source if isinstance(source, str) else source["path"])
                sources[str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
            code, output, error = run(root, "storage", "migrate", "--kind", kind,
                                      "--apply", "--preview-digest", preview["digest"])
            phase = {"kind": kind, "preview_digest": preview["digest"],
                     "records": preview["records"], "status": "shadow-central" if code == 0 else "failed"}
            phase["result" if code == 0 else "error"] = json.loads(output) if code == 0 else error
            phases.append(phase)
            if code:
                failures.append(f"{root}: {kind} apply failed: {error}")
        views = []
        for view in VIEWS:
            before = views_before[" ".join(view)]
            after = run(root, *view)
            equal = before[:2] == after[:2]
            views.append({"command": " ".join(view), "before_exit": before[0],
                          "after_exit": after[0], "output_identical": equal,
                          "before_sha256": hashlib.sha256(before[1]).hexdigest(),
                          "after_sha256": hashlib.sha256(after[1]).hexdigest(),
                          "before_error": before[2], "after_error": after[2]})
            if before[0] or after[0]:
                failures.append(f"{root}: {view} read failed; equal errors are not parity proof")
            if not equal:
                views[-1]["before_output"] = before[1].decode(errors="replace")
                views[-1]["after_output"] = after[1].decode(errors="replace")
                failures.append(f"{root}: {view} changed; inspect expected union/reconciliation before live activation")
        changed = [path for path, digest in sources.items()
                   if not Path(path).is_file() or hashlib.sha256(Path(path).read_bytes()).hexdigest() != digest]
        failures.extend(f"source changed during rehearsal: {path}" for path in changed)
        report["repositories"].append({"root": str(root), "phases": phases, "views": views,
                                       "source_count": len(sources), "changed_sources": changed})
        print(f"{root.name}: {sum(p['status'] == 'shadow-central' for p in phases)} phases rehearsed; "
              f"{sum(p['status'] == 'held' for p in phases)} held; sources unchanged={not changed}", flush=True)
    if hashlib.sha256(binary.read_bytes()).hexdigest() != binary_digest:
        failures.append("binary changed during rehearsal; rerun with an isolated immutable build")
    report["verification_failures"] = failures
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Report: {args.report}; held phases are not migrated or declared complete.")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
