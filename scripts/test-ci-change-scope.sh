#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
test_repo="$scratch/repo"
mkdir -p "$test_repo/scripts"

git -C "$test_repo" init -q
git -C "$test_repo" config user.name "Switchbard CI Test"
git -C "$test_repo" config user.email "ci@example.invalid"
cp "$repo_root/scripts/ci-change-scope.sh" "$test_repo/scripts/"
cp "$repo_root/scripts/ci-mission-sidecar-scope.sh" "$test_repo/scripts/"
cp "$repo_root/scripts/ci-rust-scope.sh" "$test_repo/scripts/"
printf 'baseline\n' > "$test_repo/README.md"
git -C "$test_repo" add .
git -C "$test_repo" commit -q -m baseline
base_sha="$(git -C "$test_repo" rev-parse HEAD)"

mkdir -p "$test_repo/crates/switchbard-gui/src/ui/backlog"
printf 'ui only\n' > "$test_repo/crates/switchbard-gui/src/ui/backlog/board.rs"
git -C "$test_repo" add .
git -C "$test_repo" commit -q -m "ui only"
ui_sha="$(git -C "$test_repo" rev-parse HEAD)"
actual="$($repo_root/scripts/ci-change-scope.sh \
  --repo "$test_repo" "$base_sha" "$ui_sha")"
if [[ "$actual" != "rust=true
mission_sidecar=false" ]]; then
  echo "UI-only Git diff routed wrongly: $actual" >&2
  exit 1
fi

mkdir -p "$test_repo/crates/switchbard-core/src"
printf 'mission change\n' > "$test_repo/crates/switchbard-core/src/mission_supervisor.rs"
git -C "$test_repo" add .
git -C "$test_repo" commit -q -m mission
mission_sha="$(git -C "$test_repo" rev-parse HEAD)"
actual="$($repo_root/scripts/ci-change-scope.sh \
  --repo "$test_repo" "$ui_sha" "$mission_sha")"
if [[ "$actual" != "rust=true
mission_sidecar=true" ]]; then
  echo "mission Git diff routed wrongly: $actual" >&2
  exit 1
fi

actual="$($repo_root/scripts/ci-change-scope.sh \
  --repo "$test_repo" 0000000000000000000000000000000000000000 "$mission_sha")"
if [[ "$actual" != "rust=true
mission_sidecar=true" ]]; then
  echo "missing base did not fail open for every scope: $actual" >&2
  exit 1
fi

output_file="$scratch/github-output"
$repo_root/scripts/ci-change-scope.sh \
  --repo "$test_repo" --github-output "$output_file" "$base_sha" "$ui_sha"
if [[ "$(<"$output_file")" != "rust=true
mission_sidecar=false" ]]; then
  echo "Git diff GitHub output contract failed" >&2
  exit 1
fi

# TASK-180: a backlog-only commit must route away from the Rust matrix
# through the real Git plumbing, not just the pure scope script.
mkdir -p "$test_repo/backlog/tasks"
printf 'task record\n' > "$test_repo/backlog/tasks/task-1 - a task.md"
git -C "$test_repo" add .
git -C "$test_repo" commit -q -m "backlog only"
backlog_sha="$(git -C "$test_repo" rev-parse HEAD)"
actual="$($repo_root/scripts/ci-change-scope.sh \
  --repo "$test_repo" "$mission_sha" "$backlog_sha")"
if [[ "$actual" != "rust=false
mission_sidecar=false" ]]; then
  echo "backlog-only Git diff still routed to the Rust matrix: $actual" >&2
  exit 1
fi

# The bug PR #146 exposed: a branch whose base has moved on. A two-dot
# base..head diff also reports every path in the commits landed since, so a
# one-file PR looked like a 60-file one and every matrix ran. The comparison
# must be against the merge base.
git -C "$test_repo" checkout -q -b feature "$mission_sha"
mkdir -p "$test_repo/backlog/tasks"
printf 'one task edit\n' > "$test_repo/backlog/tasks/task-2 - only change.md"
git -C "$test_repo" add .
git -C "$test_repo" commit -q -m "backlog only, on a branch"
feature_sha="$(git -C "$test_repo" rev-parse HEAD)"

git -C "$test_repo" checkout -q master 2>/dev/null || git -C "$test_repo" checkout -q main
mkdir -p "$test_repo/crates/switchbard-core/src"
printf 'someone else landed Rust\n' > "$test_repo/crates/switchbard-core/src/lib.rs"
git -C "$test_repo" add .
git -C "$test_repo" commit -q -m "unrelated Rust on the base branch"
moved_base_sha="$(git -C "$test_repo" rev-parse HEAD)"

actual="$($repo_root/scripts/ci-change-scope.sh \
  --repo "$test_repo" "$moved_base_sha" "$feature_sha")"
if [[ "$actual" != "rust=false
mission_sidecar=false" ]]; then
  echo "a moved base branch leaked its own commits into the diff: $actual" >&2
  exit 1
fi

echo "CI Git diff routing: PASS"
