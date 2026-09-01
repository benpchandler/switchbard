#!/usr/bin/env bash
# Materialize the exact xplan source revision from Switchbard's vendored Git bundle.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PIN="$REPO_ROOT/xplan-sidecar-pin.json"
DESTINATION=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pin)
      PIN="$2"
      shift 2
      ;;
    --destination)
      DESTINATION="$2"
      shift 2
      ;;
    *)
      echo "unsupported argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$DESTINATION" || ! -f "$PIN" || -e "$DESTINATION" ]]; then
  echo "checkout requires an existing pin and an absent destination" >&2
  exit 2
fi

eval "$(python3 - "$PIN" <<'PY'
import json
import pathlib
import shlex
import sys

pin = json.loads(pathlib.Path(sys.argv[1]).read_text())
for key in ("xplan_repository", "xplan_source_revision"):
    value = pin.get(key)
    if not isinstance(value, str) or not value:
        raise SystemExit(f"pin field is missing: {key}")
    print(f"{key.upper()}={shlex.quote(value)}")
PY
)"

BUNDLE="$REPO_ROOT/vendor/xplan/xplan-source-${XPLAN_SOURCE_REVISION}.bundle"
if [[ ! -f "$BUNDLE" || -L "$BUNDLE" ]]; then
  echo "vendored xplan source bundle is missing" >&2
  exit 1
fi

git bundle verify "$BUNDLE" >/dev/null
if ! git bundle list-heads "$BUNDLE" | awk '{print $1}' | grep -Fxq "$XPLAN_SOURCE_REVISION"; then
  echo "vendored xplan source bundle does not contain the pinned revision" >&2
  exit 1
fi

mkdir -p "$DESTINATION"
git -C "$DESTINATION" init --quiet
git -C "$DESTINATION" fetch --quiet "$BUNDLE" "$XPLAN_SOURCE_REVISION"
git -C "$DESTINATION" checkout --quiet --detach FETCH_HEAD
git -C "$DESTINATION" remote add origin "$XPLAN_REPOSITORY"

if [[ "$(git -C "$DESTINATION" rev-parse HEAD)" != "$XPLAN_SOURCE_REVISION" ]]; then
  echo "vendored xplan checkout revision mismatch" >&2
  exit 1
fi
if [[ -n "$(git -C "$DESTINATION" status --porcelain=v1 --untracked-files=all)" ]]; then
  echo "vendored xplan checkout is not clean" >&2
  exit 1
fi

printf '%s\n' "$DESTINATION"
