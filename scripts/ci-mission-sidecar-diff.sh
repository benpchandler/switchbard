#!/usr/bin/env bash
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

emit() {
  if [[ -n "$output_file" ]]; then
    printf 'mission_sidecar=%s\n' "$1" >> "$output_file"
  else
    printf '%s\n' "$1"
  fi
}

if [[ -z "$base_sha" || "$base_sha" =~ ^0+$ ]] || \
   ! git -C "$repo_root" cat-file -e "$base_sha^{commit}"; then
  emit true
  exit 0
fi

changed_paths="$(mktemp)"
trap 'rm -f "$changed_paths"' EXIT
if ! git -C "$repo_root" diff --name-only -z "$base_sha" "$head_sha" > "$changed_paths"; then
  emit true
  exit 0
fi

if [[ -n "$output_file" ]]; then
  "$repo_root/scripts/ci-mission-sidecar-scope.sh" \
    --github-output "$output_file" < "$changed_paths"
else
  "$repo_root/scripts/ci-mission-sidecar-scope.sh" < "$changed_paths"
fi
