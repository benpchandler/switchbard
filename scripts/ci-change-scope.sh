#!/usr/bin/env bash
# One place answers "what does this change require?".
#
# Takes the two commits CI is comparing, resolves the changed paths once, and
# hands the same list to every scope script - `rust` (the fmt/clippy/test
# matrix, TASK-180) and `mission_sidecar` (the live helper matrix). Each
# scope script owns its own answer; this owns the diff and nothing else.
#
# Fails open on purpose: a base we cannot resolve, or a diff we cannot take,
# means we do not know what changed, so everything runs.
set -euo pipefail

repo="."
output_file=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      [[ $# -ge 2 ]] || { echo "--repo requires a path" >&2; exit 2; }
      repo="$2"
      shift 2
      ;;
    --github-output)
      [[ $# -ge 2 ]] || { echo "--github-output requires a path" >&2; exit 2; }
      output_file="$2"
      shift 2
      ;;
    *)
      break
      ;;
  esac
done

if [[ $# -ne 2 ]]; then
  echo "usage: $0 [--repo PATH] [--github-output PATH] BASE_SHA HEAD_SHA" >&2
  exit 2
fi
base_sha="$1"
head_sha="$2"
repo_root="$(git -C "$repo" rev-parse --show-toplevel)"

# Every scope, in the order they are emitted. Each is a script under
# scripts/ that reads NUL-delimited paths and prints `<name>=true|false`.
scopes=(rust mission_sidecar)
script_for() {
  case "$1" in
    rust) printf '%s\n' "$repo_root/scripts/ci-rust-scope.sh" ;;
    mission_sidecar) printf '%s\n' "$repo_root/scripts/ci-mission-sidecar-scope.sh" ;;
    *) echo "unknown scope $1" >&2; exit 2 ;;
  esac
}

emit_all() {
  local verdict="$1"
  for scope in "${scopes[@]}"; do
    if [[ -n "$output_file" ]]; then
      printf '%s=%s\n' "$scope" "$verdict" >> "$output_file"
    else
      printf '%s=%s\n' "$scope" "$verdict"
    fi
  done
}

if [[ -z "$base_sha" || "$base_sha" =~ ^0+$ ]] || \
   ! git -C "$repo_root" cat-file -e "$base_sha^{commit}"; then
  emit_all true
  exit 0
fi

changed_paths="$(mktemp)"
trap 'rm -f "$changed_paths"' EXIT
if ! git -C "$repo_root" diff --name-only -z "$base_sha" "$head_sha" > "$changed_paths"; then
  emit_all true
  exit 0
fi

for scope in "${scopes[@]}"; do
  script="$(script_for "$scope")"
  if [[ -n "$output_file" ]]; then
    # Each scope script writes its own `<name>=<verdict>` line.
    "$script" --github-output "$output_file" < "$changed_paths"
  else
    printf '%s=%s\n' "$scope" "$("$script" < "$changed_paths")"
  fi
done
