#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
scope="$repo_root/scripts/ci-rust-scope.sh"

assert_scope() {
  expected="$1"
  shift
  actual="$(printf '%s\0' "$@" | "$scope")"
  if [[ "$actual" != "$expected" ]]; then
    printf 'expected rust scope %s, got %s for:\n' "$expected" "$actual" >&2
    printf '  %s\n' "$@" >&2
    exit 1
  fi
}

# Prose and backlog records cannot change a Rust build's result.
assert_scope false "backlog/tasks/task-1 - a task.md"
assert_scope false backlog/ranking.yml backlog/projects/Bugs.md
assert_scope false "docs/a file with spaces.md" README.md CLAUDE.md
assert_scope false docs/perf/README.md crates/switchbard-tui/README.md
# PR #139's actual diff, the change that motivated this (TASK-180).
assert_scope false \
  "backlog/tasks/task-119 - Add-guarded-CI,-branch-update,-and-merge-operations.md" \
  "backlog/tasks/task-141 - sbt-idea-PR-page.md" \
  "backlog/tasks/task-141.5 - Merge-a-selected-PR-from-sbt.md"

# Anything that can reach the build runs the matrix.
assert_scope true crates/switchbard-core/src/lib.rs
assert_scope true Cargo.lock
assert_scope true mise.toml
assert_scope true .github/workflows/ci.yml
assert_scope true scripts/ci-rust-scope.sh
assert_scope true crates/switchbard-gui/assets/icon.png
assert_scope true .claude/settings.json
# Not docs/** wholesale: the canonical screenshots under it are baselines a
# gated test binary reads, so a non-Markdown file there runs the matrix.
assert_scope true docs/qa/screenshots/mission_sidecar_ready_light.png
assert_scope true docs/perf/ledger.csv
assert_scope true "docs/a file with spaces.md" docs/qa/screenshots/x.png
# One live path among inert ones still runs it.
assert_scope true "backlog/tasks/task-1 - a task.md" crates/switchbard-tui/src/view.rs
# An empty diff tells us nothing, so it fails open.
assert_scope true

output_file="$(mktemp)"
trap 'rm -f "$output_file"' EXIT
printf '%s\0' README.md | "$scope" --github-output "$output_file"
if [[ "$(<"$output_file")" != "rust=false" ]]; then
  echo "GitHub output contract failed" >&2
  exit 1
fi

echo "rust change scope: PASS"
