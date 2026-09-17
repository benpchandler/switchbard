#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
"$repo_root/scripts/test-ci-rust-scope.sh"
"$repo_root/scripts/test-ci-mission-sidecar-scope.sh"
"$repo_root/scripts/test-ci-change-scope.sh"
"$repo_root/scripts/test-git-hooks.sh"
"$repo_root/scripts/test-install-guard.sh"
"$repo_root/scripts/test-install-failure.sh"
"$repo_root/scripts/test-auto-install-main.sh"
bash "$repo_root/scripts/test-install-release.sh"
python3 "$repo_root/scripts/test-rust-flag-standard.py"
ruby "$repo_root/scripts/test-ci-workflow.rb"
