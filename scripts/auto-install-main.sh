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
# - It delegates the downgrade decision to scripts/install-switchbard.sh and
#   never passes --force: if origin/main does not contain the running build
#   (someone installed a feature branch on purpose), it logs and leaves it.
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
    git clone -q "$REMOTE_URL" "$CHECKOUT" >> "$LOG" 2>&1
fi
cd "$CHECKOUT"
git fetch -q origin main >> "$LOG" 2>&1
TARGET="$(git rev-parse origin/main)"

# The commit an installed binary reports, or empty when there is none.
installed_commit() {
    local path
    path="$(command -v "$1" 2>/dev/null || true)"
    [[ -z "$path" ]] && return 0
    "$path" build-id 2>/dev/null | sed -n 's/^commit=//p' || true
}

if [[ "$(installed_commit sbt)" == "$TARGET" && "$(installed_commit sb)" == "$TARGET" ]]; then
    exit 0
fi

log "origin/main is $(git rev-parse --short "$TARGET"); installed sbt=$(installed_commit sbt | cut -c1-8) sb=$(installed_commit sb | cut -c1-8)"
git checkout -q --detach "$TARGET" >> "$LOG" 2>&1
if mise exec -- bash scripts/install-switchbard.sh >> "$LOG" 2>&1; then
    log "installed $(git rev-parse --short "$TARGET")"
else
    log "install refused or failed; see above. Nothing changed."
fi
