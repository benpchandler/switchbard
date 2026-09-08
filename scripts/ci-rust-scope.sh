#!/usr/bin/env bash
# Does this change need the Rust matrix (fmt, clippy, test on both OSes)?
#
# Reads NUL-delimited changed paths on stdin, emits `rust=true|false`.
#
# The answer is `true` unless *every* changed path is one that provably
# cannot alter a Rust build or its result. Prose and backlog records are the
# only such paths: the repo's task markdown, the docs tree, and Markdown at
# the root. Everything else - crates, scripts, workflows, toolchain pins,
# assets, lockfiles - runs the matrix.
#
# The default is deliberately "run it". A path that ought to be inert but is
# not costs a false green on a merge; a path that is inert but runs the
# matrix costs a few minutes. Add to this list only where the first cost is
# provably zero (TASK-180).
set -euo pipefail

output_file=""
if [[ $# -gt 0 ]]; then
  if [[ $# -ne 2 || "$1" != "--github-output" ]]; then
    echo "usage: $0 [--github-output PATH] < NUL_DELIMITED_PATHS" >&2
    exit 2
  fi
  output_file="$2"
fi

run_rust=false
saw_path=false
while IFS= read -r -d '' path; do
  saw_path=true
  case "$path" in
    backlog/*) ;;
    docs/*) ;;
    *.md)
      # Root-level Markdown only; a nested .md was matched by an earlier
      # arm if it is inert, and must otherwise run the matrix.
      if [[ "$path" == */* ]]; then
        run_rust=true
        break
      fi
      ;;
    *)
      run_rust=true
      break
      ;;
  esac
done

# An empty diff means we could not learn anything about the change; run.
if [[ "$saw_path" == false ]]; then
  run_rust=true
fi

if [[ -n "$output_file" ]]; then
  printf 'rust=%s\n' "$run_rust" >> "$output_file"
else
  printf '%s\n' "$run_rust"
fi
