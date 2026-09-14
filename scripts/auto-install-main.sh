#!/usr/bin/env bash
# Keep the installed sb and sbt in step with origin/main without anyone thinking
# about it.
#
# WHY THIS EXISTS
#
# A running sbt re-execs itself the moment the file on disk changes, so the only
# manual step between "merged to main" and "every live session has it" was
# someone running `mise run install`. That step was routinely forgotten and the
# installed binary drifted behind main. This script is that step, run on a timer
# (see `mise run auto-install-enable`, which installs the launchd agent).
#
# HOW IT STAYS SAFE
#
# - It builds from its own detached checkout under ~/.switchbard/auto-install,
#   never from a worktree a person or agent is editing.
# - It installs only origin/main, and only when the installed build's commit is
#   not already that commit.
# - It delegates to scripts/install-switchbard.sh with --main-authority
#   (TASK-227): since this checkout is always origin/main's own tip, that flag
#   makes main authoritative rather than refusing on ancestry - a running
#   build main doesn't contain (a feature branch installed on purpose) gets
#   replaced, with exactly what was dropped printed and receipted, instead of
#   auto-install refusing silently for hours (2026-09-13, 116 refusals).
#   A manual `--hold` on that branch still defers this cycle; see the guard's
#   own header for the receipt/hold contract sbt's startup banner reads.
# - A lock directory prevents two runs from overlapping.
#
#   checkout: ~/.switchbard/auto-install/checkout
#   log:      ~/.switchbard/auto-install/auto-install.log
set -euo pipefail

STATE_DIR="${SWITCHBARD_AUTO_INSTALL_DIR:-$HOME/.switchbard/auto-install}"
CHECKOUT="$STATE_DIR/checkout"
LOG="$STATE_DIR/auto-install.log"
LOCK="$STATE_DIR/lock"
REMOTE_URL="${SWITCHBARD_AUTO_INSTALL_REMOTE:-https://github.com/benpchandler/switchbard.git}"

mkdir -p "$STATE_DIR"
log() { printf '%s %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$*" >> "$LOG"; }

# Every wait on the network is bounded (Bash Power-of-10 rule 2): `timeout`/
# `gtimeout` where available, else a `perl alarm` shim (no new dependency),
# else run unbounded rather than refuse to run at all. Mirrors the same
# helper in install-switchbard.sh - small enough that sourcing it across two
# standalone scripts would add more indirection than it saves.
run_with_timeout() {
    local secs="$1"
    shift
    if command -v timeout >/dev/null 2>&1; then
        timeout "$secs" "$@"
    elif command -v gtimeout >/dev/null 2>&1; then
        gtimeout "$secs" "$@"
    elif command -v perl >/dev/null 2>&1; then
        perl -e 'alarm shift @ARGV; exec @ARGV or exit 127' "$secs" "$@"
    else
        "$@"
    fi
}

if ! mkdir "$LOCK" 2>/dev/null; then
    # A lock older than an hour belongs to a crashed run.
    if find "$LOCK" -prune -mmin +60 2>/dev/null | grep -q .; then
        rmdir "$LOCK" 2>/dev/null || true
        mkdir "$LOCK" 2>/dev/null || exit 0
    else
        exit 0
    fi
fi
trap 'rmdir "$LOCK" 2>/dev/null || true' EXIT

if [[ ! -d "$CHECKOUT/.git" ]]; then
    log "cloning $REMOTE_URL into $CHECKOUT"
    if ! run_with_timeout 300 git clone -q "$REMOTE_URL" "$CHECKOUT" >> "$LOG" 2>&1; then
        log "clone failed or timed out; will retry next cycle"
        exit 0
    fi
fi
cd "$CHECKOUT"
# SWITCHBARD_AUTO_INSTALL_REMOTE is the one source of truth for where this
# checkout pulls from - re-pointing it every tick means changing the env var
# takes effect immediately, not only on the first-ever clone.
git remote set-url origin "$REMOTE_URL" >> "$LOG" 2>&1
if ! run_with_timeout 60 git fetch -q origin main >> "$LOG" 2>&1; then
    log "fetch failed or timed out; will retry next cycle"
    exit 0
fi
TARGET="$(git rev-parse origin/main)"

# The commit an installed binary reports, or empty when there is none.
installed_commit() {
    local path
    path="$(command -v "$1" 2>/dev/null || true)"
    [[ -z "$path" ]] && return 0
    run_with_timeout 5 "$path" build-id 2>/dev/null | sed -n 's/^commit=//p' || true
}

if [[ "$(installed_commit sbt)" == "$TARGET" && "$(installed_commit sb)" == "$TARGET" ]]; then
    exit 0
fi

log "origin/main is $(git rev-parse --short "$TARGET"); installed sbt=$(installed_commit sbt | cut -c1-8) sb=$(installed_commit sb | cut -c1-8)"
git checkout -q --detach "$TARGET" >> "$LOG" 2>&1
if mise exec -- bash scripts/install-switchbard.sh --main-authority >> "$LOG" 2>&1; then
    GUARD_STATUS=0
else
    GUARD_STATUS=$?
fi
RECEIPT="$STATE_DIR/last-install.json"
OUTCOME="$(sed -n 's/.*"outcome": *"\([^"]*\)".*/\1/p' "$RECEIPT" 2>/dev/null | head -1)"
case "$OUTCOME" in
    installed) log "installed $(git rev-parse --short "$TARGET")" ;;
    held)      ;; # the guard already logged "holding <branch> until <time>"
    refused)   log "install refused; see above. Nothing changed." ;;
    failed)    log "build failed; nothing replaced. See above and the receipt's reason." ;;
    *)         log "install exited $GUARD_STATUS with no readable receipt; see above." ;;
esac
