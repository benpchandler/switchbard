#!/usr/bin/env bash
set -euo pipefail

unset GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE \
  GIT_OBJECT_DIRECTORY GIT_NAMESPACE

usage() {
  echo "usage: $0 [--repo PATH]" >&2
}

repo="."
if [[ $# -gt 0 ]]; then
  if [[ $# -ne 2 || "$1" != "--repo" ]]; then
    usage
    exit 2
  fi
  repo="$2"
fi

repo_root="$(git -C "$repo" rev-parse --show-toplevel)"
for hook in pre-commit pre-push; do
  hook_path="$repo_root/.githooks/$hook"
  if [[ ! -x "$hook_path" ]]; then
    echo "hook is missing or not executable: $hook_path" >&2
    exit 1
  fi
done

# A relative path resolves against each worktree root. An absolute primary-
# checkout path would run the wrong branch's hooks from linked worktrees.
git -C "$repo_root" config --local core.hooksPath .githooks

configured="$(git -C "$repo_root" config --local --get core.hooksPath)"
if [[ "$configured" != ".githooks" ]]; then
  echo "failed to configure relative core.hooksPath: $configured" >&2
  exit 1
fi

echo "Installed Switchbard hooks for $repo_root"
