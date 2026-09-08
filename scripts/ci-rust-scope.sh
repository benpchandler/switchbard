#!/usr/bin/env bash
# Does this change need the Rust matrix (fmt, clippy, test on both OSes)?
#
# Reads NUL-delimited changed paths on stdin, emits `rust=true|false`.
#
# The answer is `true` unless *every* changed path is one that provably
# cannot alter a Rust build or its result. Two families qualify:
#
#   backlog/**   The repo's own task records. Verified inert: no test in the
#                gate reads this repo's backlog - every `backlog/...` path in
#                the test suites is built under a tempdir root.
#   *.md         Prose, at the repo root or anywhere under docs/.
#
# Note what is NOT skipped: `docs/**` wholesale. `docs/qa/screenshots/*.png`
# are canonical baselines that gated test binaries read and compare against
# (crates/switchbard-gui/tests/mission_command_sidecar_visual.rs), so a
# non-Markdown file under docs/ can change a Rust test's result and must run
# the matrix.
#
# The default is deliberately "run it". A path that ought to be inert but is
# not costs a false green on a merge; a path that is inert but runs the
# matrix costs a few minutes. Add to this list only where the first cost is
# provably zero, and check for readers before you do (TASK-180).
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
    *.md) ;;
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
