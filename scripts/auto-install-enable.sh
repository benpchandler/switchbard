#!/usr/bin/env bash
# Install (or remove, with --disable) the launchd agent that keeps sb and sbt in
# step with origin/main. The agent runs a stable copy of
# scripts/auto-install-main.sh under ~/.switchbard/auto-install so a moved or
# deleted worktree cannot break it.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STATE_DIR="$HOME/.switchbard/auto-install"
LABEL="com.switchbard.auto-install"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
SCRIPT="$STATE_DIR/auto-install-main.sh"
DOMAIN="gui/$(id -u)"

if [[ "${1:-}" == "--disable" ]]; then
    launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
    rm -f "$PLIST"
    echo "auto-install disabled; checkout and log left in $STATE_DIR"
    exit 0
fi

mkdir -p "$STATE_DIR" "$HOME/Library/LaunchAgents"
cp "$REPO_ROOT/scripts/auto-install-main.sh" "$SCRIPT"
chmod +x "$SCRIPT"
sed -e "s|__SCRIPT__|$SCRIPT|g" -e "s|__HOME__|$HOME|g" \
    "$REPO_ROOT/launchd/$LABEL.plist" > "$PLIST"
launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
launchctl bootstrap "$DOMAIN" "$PLIST"
launchctl kickstart -k "$DOMAIN/$LABEL"
echo "auto-install enabled: every 5 minutes, origin/main -> sb + sbt (log: $STATE_DIR/auto-install.log)"
