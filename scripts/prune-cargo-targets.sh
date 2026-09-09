#!/usr/bin/env bash
# Remove Cargo target directories that no live worktree of this repo owns.
#
# Each worktree gets its own target directory (mise.toml, TASK-179), so the
# cache root accumulates one directory per worktree that ever built here plus
# the debris of the old shared layout. This asks mise for each live worktree's
# resolved directory rather than recomputing the naming rule, so mise.toml stays
# the only place that owns it.
#
# Prints what it would remove and exits. Pass --yes to actually remove.
set -euo pipefail

unset GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE \
  GIT_OBJECT_DIRECTORY GIT_NAMESPACE CARGO_TARGET_DIR

apply=0
if [[ "${1-}" == "--yes" ]]; then
  apply=1
elif [[ $# -gt 0 ]]; then
  echo "usage: $0 [--yes]" >&2
  exit 2
fi

repo_root="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
here="$(cd "$repo_root" && mise exec -- printenv CARGO_TARGET_DIR)"
cache_root="$(dirname "$here")"
if [[ "$cache_root" == "/" || ! -d "$cache_root" ]]; then
  echo "refusing to prune an implausible cache root: $cache_root" >&2
  exit 1
fi

keep=()
while IFS= read -r line; do
  [[ "$line" == worktree\ * ]] || continue
  worktree="${line#worktree }"
  [[ -f "$worktree/mise.toml" ]] || continue
  resolved="$(cd "$worktree" && mise exec -- printenv CARGO_TARGET_DIR 2>/dev/null || true)"
  # A worktree on a commit predating the per-worktree default resolves to the
  # cache root itself. It owns no child directory, so it protects nothing.
  if [[ "$(dirname "$resolved")" == "$cache_root" ]]; then
    keep+=("$resolved")
  fi
done < <(git -C "$repo_root" worktree list --porcelain)

stale=()
for entry in "$cache_root"/*; do
  [[ -e "$entry" ]] || continue
  owned=0
  for k in "${keep[@]}"; do
    [[ "$entry" == "$k" ]] && owned=1 && break
  done
  (( owned )) || stale+=("$entry")
done

if (( ${#stale[@]} == 0 )); then
  echo "nothing to prune under $cache_root"
  exit 0
fi

for entry in "${stale[@]}"; do
  printf '%s\t%s\n' "$(du -sh "$entry" | cut -f1)" "$entry"
done

if (( ! apply )); then
  echo
  echo "dry run: ${#stale[@]} entr(y|ies) above are unowned. Re-run with --yes to remove."
  exit 0
fi

for entry in "${stale[@]}"; do
  rm -rf "$entry"
  echo "removed $entry"
done
