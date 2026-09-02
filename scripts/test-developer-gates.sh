#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
"$repo_root/scripts/test-ci-mission-sidecar-scope.sh"
"$repo_root/scripts/test-ci-mission-sidecar-diff.sh"
"$repo_root/scripts/test-git-hooks.sh"
ruby "$repo_root/scripts/test-ci-workflow.rb"
