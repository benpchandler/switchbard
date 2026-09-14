#!/usr/bin/env bash
# Proves scripts/auto-install-main.sh's own behavior, independent of the real
# guard: the stale-lock reclaim, cloning from whatever
# SWITCHBARD_AUTO_INSTALL_REMOTE names (a local bare repo here, never the
# network), that it always passes --main-authority, and that each receipt
# outcome dispatches to the right log line. Every case uses a temp STATE_DIR
# and a fake PATH; nothing here ever touches ~/.cargo/bin or a real remote.

set -euo pipefail

SCRIPT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/auto-install-main.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
FAILURES=0

fail() {
    FAILURES=$((FAILURES + 1))
    echo "FAIL: $1"
    shift
    [[ $# -gt 0 ]] && printf '%s\n' "$1" | sed 's/^/    /'
}

# A fake `mise` (the checkout has no mise.toml of its own to resolve a
# toolchain from) that just runs what follows `exec --`, and a fake `cargo`
# so nothing here ever actually compiles anything.
FAKE_BIN="$WORK/fakebin"
mkdir -p "$FAKE_BIN"
cat > "$FAKE_BIN/mise" <<'STUB'
#!/usr/bin/env bash
if [[ "$1" == "exec" && "$2" == "--" ]]; then
    shift 2
    exec "$@"
fi
exit 1
STUB
chmod +x "$FAKE_BIN/mise"

# The synthetic "GitHub": a bare repo whose scripts/install-switchbard.sh is a
# stub that records its own arguments and writes whatever receipt the test
# asks for via env vars - so these cases prove auto-install-main.sh's own
# clone/checkout/dispatch logic without running the real guard at all.
REMOTE="$WORK/remote.git"
SEED="$WORK/seed"
git init -q -b main "$SEED"
git -C "$SEED" config user.email test@example.com
git -C "$SEED" config user.name Test
mkdir -p "$SEED/scripts"
cat > "$SEED/scripts/install-switchbard.sh" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$@" > "${TEST_INSTALL_ARGS_LOG:-/dev/null}"
STATE_DIR="${SWITCHBARD_AUTO_INSTALL_DIR:-$HOME/.switchbard/auto-install}"
mkdir -p "$STATE_DIR"
outcome="${TEST_STUB_OUTCOME:-installed}"
printf '{"outcome": "%s"}' "$outcome" > "$STATE_DIR/last-install.json"
case "$outcome" in
    installed|held) exit 0 ;;
    *) exit 1 ;;
esac
STUB
chmod +x "$SEED/scripts/install-switchbard.sh"
git -C "$SEED" add -A
git -C "$SEED" commit -qm "stub guard for the auto-install-main.sh harness"
git init -q --bare -b main "$REMOTE"
git -C "$SEED" push -q "$REMOTE" main

run() {
    local state_dir="$1"
    shift
    PATH="$FAKE_BIN:$PATH" \
        SWITCHBARD_AUTO_INSTALL_DIR="$state_dir" \
        SWITCHBARD_AUTO_INSTALL_REMOTE="$REMOTE" \
        "$@" bash "$SCRIPT"
}

# --- clones from SWITCHBARD_AUTO_INSTALL_REMOTE, checks out origin/main ----
STATE1="$WORK/state1"
run "$STATE1"
if [[ ! -d "$STATE1/checkout/.git" ]]; then
    fail "did not clone a checkout from SWITCHBARD_AUTO_INSTALL_REMOTE"
elif [[ "$(git -C "$STATE1/checkout" rev-parse HEAD)" != "$(git -C "$SEED" rev-parse main)" ]]; then
    fail "checkout HEAD does not match the remote's main tip"
elif ! grep -q "cloning" "$STATE1/auto-install.log"; then
    fail "log does not record the clone" "$(cat "$STATE1/auto-install.log")"
else
    echo "ok: clones from the local bare remote and checks out its main tip"
fi

# --- always invokes install-switchbard.sh with --main-authority -----------
STATE2="$WORK/state2"
ARGS_LOG="$WORK/args.log"
run "$STATE2" env TEST_INSTALL_ARGS_LOG="$ARGS_LOG"
if [[ ! -f "$ARGS_LOG" ]] || ! grep -q -- "--main-authority" "$ARGS_LOG"; then
    fail "install-switchbard.sh was not invoked with --main-authority" "$(cat "$ARGS_LOG" 2>/dev/null)"
else
    echo "ok: always invokes install-switchbard.sh with --main-authority"
fi

# --- outcome-to-log dispatch -------------------------------------------
for outcome in installed held refused failed; do
    state="$WORK/state-$outcome"
    run "$state" env TEST_STUB_OUTCOME="$outcome"
    log_contents="$(cat "$state/auto-install.log" 2>/dev/null)"
    # Anchored at end-of-line: the always-present informational line
    # ("origin/main is X; installed sbt=Y sb=Z") also contains the substring
    # "installed ", so a loose match can't tell it apart from the dispatch's
    # own "installed <sha>" line.
    case "$outcome" in
        installed)
            if ! grep -qE ' installed [0-9a-f]+$' "$state/auto-install.log"; then
                fail "outcome=installed did not log an 'installed <sha>' line" "$log_contents"
            else
                echo "ok: outcome=installed dispatches to an 'installed' log line"
            fi
            ;;
        held)
            if grep -qE ' installed [0-9a-f]+$' "$state/auto-install.log" \
                || [[ "$log_contents" == *"refused"* || "$log_contents" == *"failed"* ]]; then
                fail "outcome=held should log nothing beyond the guard's own 'holding' line" "$log_contents"
            else
                echo "ok: outcome=held adds no extra dispatch line (the guard already logged it)"
            fi
            ;;
        refused)
            if [[ "$log_contents" != *"install refused"* ]]; then
                fail "outcome=refused did not log 'install refused'" "$log_contents"
            else
                echo "ok: outcome=refused dispatches to an 'install refused' log line"
            fi
            ;;
        failed)
            if [[ "$log_contents" != *"build failed"* ]]; then
                fail "outcome=failed did not log 'build failed'" "$log_contents"
            else
                echo "ok: outcome=failed dispatches to a 'build failed' log line"
            fi
            ;;
    esac
done

# --- a fresh lock blocks a concurrent run; a stale one is reclaimed --------
STATE3="$WORK/state3"
mkdir -p "$STATE3"
mkdir "$STATE3/lock"
run "$STATE3"
if [[ ! -d "$STATE3/lock" ]]; then
    fail "a fresh lock must not be removed by a run that couldn't acquire it"
elif [[ -d "$STATE3/checkout" ]]; then
    fail "a run that couldn't acquire the lock must not have done any work"
else
    echo "ok: a fresh lock blocks a concurrent run and is left alone"
fi

backdate() {
    # `touch -t` always reads its stamp as *local* time on both BSD and GNU -
    # computing it with `date -u` silently sets a future mtime instead
    # whenever local time trails UTC (any zone west of Greenwich).
    local path="$1" mins="$2" stamp
    stamp="$(date -v-"${mins}"M +%Y%m%d%H%M.%S 2>/dev/null || date -d "-${mins} minutes" +%Y%m%d%H%M.%S)"
    touch -t "$stamp" "$path"
}
backdate "$STATE3/lock" 90
run "$STATE3"
if [[ -d "$STATE3/lock" ]]; then
    fail "a stale lock should be reclaimed and released again after the run"
elif [[ ! -d "$STATE3/checkout" ]]; then
    fail "a stale lock's reclaim should have let the run proceed"
else
    echo "ok: a stale (90m old) lock is reclaimed and the run proceeds"
fi

if [[ "$FAILURES" -gt 0 ]]; then
    echo "$FAILURES auto-install-main case(s) failed" >&2
    exit 1
fi
echo "auto-install-main: all cases passed"
