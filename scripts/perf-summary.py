#!/usr/bin/env python3
"""Summarize a Switchbard perf CSV into a durable JSON ledger record."""

from __future__ import annotations

import argparse
import csv
import json
import math
import subprocess
from datetime import datetime
from pathlib import Path
from typing import Iterable


TIMING_COLUMNS = ("total_ms", "top_bar_ms", "central_ms", "workspace_ms")
COUNT_COLUMNS = ("rows", "expanded_rows", "services", "listeners")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--csv", required=True, type=Path, help="Perf CSV path")
    parser.add_argument("--out", required=True, type=Path, help="JSON output path")
    parser.add_argument("--label", required=True, help="Human-readable benchmark label")
    parser.add_argument("--scenario", required=True, help="Stable scenario slug")
    parser.add_argument(
        "--filter",
        choices=("servers", "agent-context", "all"),
        default="servers",
        help="Rows to summarize. 'servers' means rows > 0.",
    )
    parser.add_argument(
        "--last",
        type=int,
        default=None,
        help="Summarize only the last N matching frames",
    )
    parser.add_argument("--notes", default="", help="Optional note for this run")
    return parser.parse_args()


def percentile(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    values = sorted(values)
    rank = math.ceil((p / 100.0) * len(values))
    index = max(rank - 1, 0)
    return values[min(index, len(values) - 1)]


def duration_summary(rows: list[dict[str, str]], column: str) -> dict[str, float]:
    values = [float(row[column]) for row in rows]
    return {
        "p50": round(percentile(values, 50), 3),
        "p95": round(percentile(values, 95), 3),
        "p99": round(percentile(values, 99), 3),
        "max": round(max(values, default=0.0), 3),
    }


def count_max(rows: list[dict[str, str]], column: str) -> int:
    return max((int(row[column]) for row in rows), default=0)


def filtered_rows(rows: Iterable[dict[str, str]], mode: str) -> list[dict[str, str]]:
    if mode == "all":
        return list(rows)
    if mode == "servers":
        return [row for row in rows if int(row["rows"]) > 0]
    return [row for row in rows if int(row["rows"]) == 0]


def git_value(*args: str) -> str | None:
    try:
        return subprocess.check_output(("git", *args), text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def git_dirty() -> bool | None:
    try:
        subprocess.check_call(("git", "diff", "--quiet"))
        subprocess.check_call(("git", "diff", "--cached", "--quiet"))
        return False
    except subprocess.CalledProcessError:
        return True
    except OSError:
        return None


def iso_from_mtime(path: Path) -> str:
    return datetime.fromtimestamp(path.stat().st_mtime).astimezone().isoformat()


def main() -> int:
    args = parse_args()
    with args.csv.open(newline="") as handle:
        rows = list(csv.DictReader(handle))

    matching = filtered_rows(rows, args.filter)
    selected = matching[-args.last :] if args.last else matching

    record = {
        "schema_version": 1,
        "label": args.label,
        "scenario": args.scenario,
        "notes": args.notes,
        "source_csv": str(args.csv),
        "captured_at": iso_from_mtime(args.csv),
        "recorded_at": datetime.now().astimezone().isoformat(),
        "git": {
            "branch": git_value("branch", "--show-current"),
            "sha": git_value("rev-parse", "HEAD"),
            "dirty": git_dirty(),
        },
        "selection": {
            "filter": args.filter,
            "last": args.last,
            "source_frames": len(rows),
            "matching_frames": len(matching),
            "summarized_frames": len(selected),
        },
        "counts_max": {column: count_max(selected, column) for column in COUNT_COLUMNS},
        "metrics_ms": {
            column.replace("_ms", ""): duration_summary(selected, column)
            for column in TIMING_COLUMNS
        },
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("w") as handle:
        json.dump(record, handle, indent=2)
        handle.write("\n")
    print(args.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
