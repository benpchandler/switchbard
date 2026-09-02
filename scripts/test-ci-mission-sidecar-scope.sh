#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
scope="$repo_root/scripts/ci-mission-sidecar-scope.sh"

assert_scope() {
  expected="$1"
  shift
  actual="$(printf '%s\0' "$@" | "$scope")"
  if [[ "$actual" != "$expected" ]]; then
    printf 'expected mission-sidecar scope %s, got %s for:\n' \
      "$expected" "$actual" >&2
    printf '  %s\n' "$@" >&2
    exit 1
  fi
}

assert_scope false crates/switchbard-gui/src/ui/backlog/board.rs
assert_scope false "docs/a file with spaces.md" README.md
assert_scope true crates/switchbard-core/src/mission_supervisor.rs
assert_scope true crates/switchbard-gui/src/main.rs
assert_scope true xplan-sidecar-pin.json
assert_scope true vendor/xplan/xplan.bundle
assert_scope true scripts/bundle-mac.sh
assert_scope true Cargo.lock
assert_scope true .github/workflows/ci.yml
assert_scope true README.md crates/switchbard-gui/tests/mission_command_view.rs

output_file="$(mktemp)"
trap 'rm -f "$output_file"' EXIT
printf '%s\0' README.md | "$scope" --github-output "$output_file"
if [[ "$(<"$output_file")" != "mission_sidecar=false" ]]; then
  echo "GitHub output contract failed" >&2
  exit 1
fi

echo "mission-sidecar change scope: PASS"
