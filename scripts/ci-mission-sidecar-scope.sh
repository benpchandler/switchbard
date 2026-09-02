#!/usr/bin/env bash
set -euo pipefail

output_file=""
if [[ $# -gt 0 ]]; then
  if [[ $# -ne 2 || "$1" != "--github-output" ]]; then
    echo "usage: $0 [--github-output PATH] < NUL_DELIMITED_PATHS" >&2
    exit 2
  fi
  output_file="$2"
fi

run_mission_sidecar=false
while IFS= read -r -d '' path; do
  case "$path" in
    .github/workflows/ci.yml | \
      Cargo.toml | Cargo.lock | mise.toml | \
      crates/*/Cargo.toml | \
      xplan-sidecar-pin.json | vendor/xplan/* | \
      scripts/acquire-xplan-mission-sidecar.sh | \
      scripts/bundle-mac.sh | \
      scripts/checkout-pinned-xplan.sh | \
      scripts/ci-mission-sidecar-diff.sh | \
      scripts/ci-mission-sidecar-scope.sh | \
      scripts/test-ci-mission-sidecar-diff.sh | \
      scripts/test-ci-mission-sidecar-scope.sh | \
      crates/switchbard-core/examples/mission_* | \
      crates/switchbard-core/src/mission_* | \
      crates/switchbard-core/tests/mission_* | \
      crates/switchbard-gui/src/app.rs | \
      crates/switchbard-gui/src/main.rs | \
      crates/switchbard-gui/src/mission_* | \
      crates/switchbard-gui/src/workers.rs | \
      crates/switchbard-gui/src/ui/missions.rs | \
      crates/switchbard-gui/tests/mission_*)
      run_mission_sidecar=true
      break
      ;;
  esac
done

if [[ -n "$output_file" ]]; then
  printf 'mission_sidecar=%s\n' "$run_mission_sidecar" >> "$output_file"
else
  printf '%s\n' "$run_mission_sidecar"
fi
