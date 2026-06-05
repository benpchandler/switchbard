#!/usr/bin/env bash
#
# Rebuild the Switchbard app bundle + DMG and reload the running app — but only
# when Rust sources changed since the last build.
#
# Invoked by the Claude Code Stop hook (.claude/settings.json) so that whatever
# is running after a code change is always the freshly-built app. The guard
# keeps pure-conversation turns free: if no source file is newer than the last
# bundle, this exits in a few stat calls.
#
# The heavy work (release build + DMG + relaunch) is detached with nohup so the
# session regains control immediately; progress lands in the log below. A
# directory lock prevents overlapping Stop events from stacking builds.
#
#   log:  $TMPDIR/switchbard-reload.log
#   lock: $TMPDIR/switchbard-reload.lock
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

APP_BUNDLE="target/release/Switchbard.app"
LOG="${TMPDIR:-/tmp}/switchbard-reload.log"
LOCK="${TMPDIR:-/tmp}/switchbard-reload.lock"

# --- guard: skip unless a source file is newer than the last-built bundle ----
# First build (no bundle yet) always proceeds. We scan only source trees, never
# target/, so this stays cheap.
if [[ -e "$APP_BUNDLE" ]]; then
  changed="$(find crates scripts Cargo.toml Cargo.lock -type f \
    \( -name '*.rs' -o -name '*.toml' -o -name '*.sh' -o -name '*.icns' -o -name '*.png' \) \
    -newer "$APP_BUNDLE" -print -quit 2>/dev/null || true)"
  [[ -z "$changed" ]] && exit 0
fi

# --- clear a stale lock from a crashed build (older than 30 min) --------------
if [[ -d "$LOCK" ]] && find "$LOCK" -prune -mmin +30 2>/dev/null | grep -q .; then
  rmdir "$LOCK" 2>/dev/null || true
fi

# Self-contained worker so it survives the detached shell. Takes the repo root
# as its only argument; re-derives everything else from there.
rebuild() {
  cd "$1" || exit 1
  local app="target/release/Switchbard.app"
  local lock="${TMPDIR:-/tmp}/switchbard-reload.lock"

  # Atomic lock: mkdir succeeds for exactly one racer.
  mkdir "$lock" 2>/dev/null || exit 0
  trap 'rmdir "${TMPDIR:-/tmp}/switchbard-reload.lock" 2>/dev/null || true' EXIT

  echo "=== $(date '+%F %T') rebuild starting (sources changed) ==="
  if bash scripts/package-dmg.sh; then
    echo "--- reloading app ---"
    # Quit gently first (lets it persist ~/.switchbard/config.toml). This
    # targets the bundled executable name (capital S) — covers both this
    # build and an installed /Applications copy (same bundle id), but not a
    # `cargo run` dev binary (lowercase switchbard).
    osascript -e 'quit app "Switchbard"' 2>/dev/null || true

    # WAIT for it to actually exit. open(1) on a still-quitting same-bundle-id
    # app reactivates the dying instance instead of launching the fresh binary,
    # so we must see the process gone before relaunching.
    for _ in $(seq 1 20); do
      pgrep -x Switchbard >/dev/null 2>&1 || break
      sleep 0.25
    done
    # Stubborn? Escalate TERM → KILL.
    if pgrep -x Switchbard >/dev/null 2>&1; then
      echo "    gentle quit ignored — sending SIGTERM/SIGKILL"
      pkill -x Switchbard 2>/dev/null || true
      sleep 0.5
      pkill -9 -x Switchbard 2>/dev/null || true
      sleep 0.5
    fi

    # -n forces a brand-new instance of THIS freshly-built bundle, so we never
    # re-focus a stale process by accident.
    open -n "$app"
    echo "=== $(date '+%F %T') reload complete (pid: $(pgrep -x Switchbard | tr '\n' ' '))==="
  else
    echo "=== $(date '+%F %T') build FAILED — running app left untouched ==="
  fi
}

nohup bash -c "$(declare -f rebuild); rebuild '$REPO_ROOT'" >>"$LOG" 2>&1 &
disown 2>/dev/null || true

echo "switchbard: source change detected — rebuilding DMG + reloading app (log: $LOG)"
exit 0
