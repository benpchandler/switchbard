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
cp "$repo_root/scripts/ci-mission-sidecar-diff.sh" "$test_repo/scripts/"
cp "$repo_root/scripts/ci-mission-sidecar-scope.sh" "$test_repo/scripts/"
printf 'baseline\n' > "$test_repo/README.md"
git -C "$test_repo" add .
git -C "$test_repo" commit -q -m baseline
base_sha="$(git -C "$test_repo" rev-parse HEAD)"

mkdir -p "$test_repo/crates/switchbard-gui/src/ui/backlog"
printf 'ui only\n' > "$test_repo/crates/switchbard-gui/src/ui/backlog/board.rs"
git -C "$test_repo" add .
git -C "$test_repo" commit -q -m "ui only"
ui_sha="$(git -C "$test_repo" rev-parse HEAD)"
actual="$($repo_root/scripts/ci-mission-sidecar-diff.sh \
  --repo "$test_repo" "$base_sha" "$ui_sha")"
if [[ "$actual" != "false" ]]; then
  echo "UI-only Git diff unexpectedly selected mission-sidecar CI" >&2
  exit 1
fi

mkdir -p "$test_repo/crates/switchbard-core/src"
printf 'mission change\n' > "$test_repo/crates/switchbard-core/src/mission_supervisor.rs"
git -C "$test_repo" add .
git -C "$test_repo" commit -q -m mission
mission_sha="$(git -C "$test_repo" rev-parse HEAD)"
actual="$($repo_root/scripts/ci-mission-sidecar-diff.sh \
  --repo "$test_repo" "$ui_sha" "$mission_sha")"
if [[ "$actual" != "true" ]]; then
  echo "mission Git diff did not select mission-sidecar CI" >&2
  exit 1
fi

actual="$($repo_root/scripts/ci-mission-sidecar-diff.sh \
  --repo "$test_repo" 0000000000000000000000000000000000000000 "$mission_sha")"
if [[ "$actual" != "true" ]]; then
  echo "missing base did not fail open" >&2
  exit 1
fi

output_file="$scratch/github-output"
$repo_root/scripts/ci-mission-sidecar-diff.sh \
  --repo "$test_repo" --github-output "$output_file" "$base_sha" "$ui_sha"
if [[ "$(<"$output_file")" != "mission_sidecar=false" ]]; then
  echo "Git diff GitHub output contract failed" >&2
  exit 1
fi

echo "mission-sidecar Git diff routing: PASS"
