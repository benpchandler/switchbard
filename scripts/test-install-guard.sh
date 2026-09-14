#!/usr/bin/env bash
# Proves scripts/install-switchbard.sh refuses the install that caused
# TASK-172, and the TASK-227 additions on top of it: --main-authority drops
# instead of refusing, a manual non-main install needs --branch, a --hold
# blocks auto-install until it expires, and every attempt leaves a receipt.
#
# Each case builds a throwaway repository with a real divergent history and a
# stub binary on PATH that answers `build-id` the way an installed sbt does, so
# the guard is exercised through exactly the interface it uses in production.

set -euo pipefail

GUARD="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/install-switchbard.sh"
FAILURES=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Every receipt/hold read or written by these cases lives here - never the
# real $HOME, even when a developer runs this gate locally.
export SWITCHBARD_AUTO_INSTALL_DIR="$WORK/state"
RECEIPT="$SWITCHBARD_AUTO_INSTALL_DIR/last-install.json"
HOLD_FILE="$SWITCHBARD_AUTO_INSTALL_DIR/hold.json"

# A repo with `main` carrying a feature commit and `stale` branching before it.
REPO="$WORK/repo"
# `-b main` explicitly: the default branch name is a per-machine git setting,
# and CI's default (master) silently broke the merge case below.
git init -q -b main "$REPO"
git -C "$REPO" config user.email test@example.com
git -C "$REPO" config user.name Test
echo base > "$REPO/f"
git -C "$REPO" add f
git -C "$REPO" commit -qm base
BASE="$(git -C "$REPO" rev-parse HEAD)"
echo feature > "$REPO/f"
git -C "$REPO" commit -qam "add the pull requests page"
FEATURE="$(git -C "$REPO" rev-parse HEAD)"
git -C "$REPO" checkout -q -b stale "$BASE"
echo other > "$REPO/g"
git -C "$REPO" add g
git -C "$REPO" commit -qm "unrelated work"
# Captured before the merge below moves the `stale` branch pointer forward -
# the commit object itself stays reachable (it becomes the merge's parent)
# and is exactly the "diverged, never on main" fixture --main-authority needs.
STALE_UNRELATED="$(git -C "$REPO" rev-parse stale)"

# A second remote this repo's `main` is pushed to, and a detached clone of it -
# the shape scripts/auto-install-main.sh actually runs --main-authority from.
# `stale` is pushed too (as a real GitHub feature branch would be) so the
# clone's object database has STALE_UNRELATED to reason about, even though
# only `main` is ever checked out there.
ORIGIN="$WORK/origin.git"
git init -q --bare -b main "$ORIGIN"
git -C "$REPO" push -q "$ORIGIN" main stale
CHECKOUT="$WORK/checkout"
git clone -q "$ORIGIN" "$CHECKOUT"
git -C "$CHECKOUT" checkout -q --detach main

# A stub `sbt` reporting whatever commit/branch the case pins.
BIN="$WORK/bin"
mkdir -p "$BIN"
stub_reports() {
    local commit="$1" branch="${2:-x}"
    cat > "$BIN/sbt" <<STUB
#!/usr/bin/env bash
[[ "\${1:-}" == "build-id" ]] || exit 1
printf 'commit=%s\nbranch=%s\ndirty=false\nversion=0.4.0\n' "$commit" "$branch"
STUB
    chmod +x "$BIN/sbt"
}

receipt_field() {
    sed -n "s/.*\"$1\": *\"\\([^\"]*\\)\".*/\\1/p" "$RECEIPT" 2>/dev/null | head -1
}

fail() {
    FAILURES=$((FAILURES + 1))
    echo "FAIL: $1"
    shift
    [[ $# -gt 0 ]] && printf '%s\n' "$1" | sed 's/^/    /'
}

# extra guard args go after `--dry-run sbt`, e.g. `check ... -- --branch`
check() {
    local name="$1" expected="$2" head_branch="$3"
    shift 3
    [[ "${1:-}" == "--" ]] && shift
    local output status
    git -C "$REPO" checkout -q "$head_branch"
    set +e
    output="$(cd "$REPO" && PATH="$BIN:$PATH" bash "$GUARD" --dry-run sbt "$@" 2>&1)"
    status=$?
    set -e
    if [[ "$status" -ne "$expected" ]]; then
        fail "$name (expected exit $expected, got $status)" "$output"
    else
        echo "ok: $name"
    fi
}

# The exact TASK-172 shape: running a build that has the page, installing from
# a branch that never had it. `--branch` only acknowledges "not main"; the
# ancestry refusal underneath is what this case is actually proving.
stub_reports "$FEATURE"
check "refuses installing a tree missing the running build's commits" 1 stale -- --branch
[[ "$(receipt_field outcome)" == refused ]] || fail "receipt not marked refused after the TASK-172 case"

# The same tree is fine once it contains that commit.
git -C "$REPO" checkout -q stale
git -C "$REPO" merge -q --no-edit main
check "allows installing a tree that contains the running build" 0 stale -- --branch
[[ "$(receipt_field outcome)" == installed ]] || fail "receipt not marked installed after a clean forward install"

# Forward from an older build on main needs no --branch at all.
stub_reports "$BASE"
check "allows a plain forward install" 0 main

# A commit this repository has never heard of cannot be proven safe.
stub_reports "0000000000000000000000000000000000000000"
check "refuses a build stamped with an unknown commit" 1 main

# No stamp at all (a binary predating build-id) installs with a notice.
cat > "$BIN/sbt" <<'STUB'
#!/usr/bin/env bash
exit 1
STUB
chmod +x "$BIN/sbt"
check "allows install when the running binary has no stamp" 0 main

# --- TASK-227: a manual non-main install is an explicit choice -------------
stub_reports "$BASE"
git -C "$REPO" checkout -q stale
set +e
output="$(cd "$REPO" && PATH="$BIN:$PATH" bash "$GUARD" --dry-run sbt 2>&1)"
status=$?
set -e
if [[ "$status" -ne 1 ]]; then
    fail "manual install off main without --branch should refuse (got exit $status)" "$output"
elif [[ "$output" != *"--branch"* ]]; then
    fail "refusal does not name the --branch flag" "$output"
else
    echo "ok: manual install off main without --branch refuses and names the flag"
fi
[[ "$(receipt_field outcome)" == refused ]] || fail "receipt not marked refused for the missing --branch case"

# --- TASK-227 AC #3: sb and sbt move together, never one without the other -
stub_sb_reports() {
    local commit="$1" branch="${2:-x}"
    cat > "$BIN/sb" <<STUB
#!/usr/bin/env bash
[[ "\${1:-}" == "build-id" ]] || exit 1
printf 'commit=%s\nbranch=%s\ndirty=false\nversion=0.4.0\n' "$commit" "$branch"
STUB
    chmod +x "$BIN/sb"
}
stub_reports "0000000000000000000000000000000000000000" # sbt: unknown commit, refuses
stub_sb_reports "$BASE"                                   # sb: plain forward, would pass alone
git -C "$REPO" checkout -q main
set +e
output="$(cd "$REPO" && PATH="$BIN:$PATH" bash "$GUARD" --dry-run sb sbt 2>&1)"
status=$?
set -e
if [[ "$status" -ne 1 ]]; then
    fail "sb+sbt should refuse together when sbt's own guard refuses" "$output"
elif [[ "$output" == *"sb: dry run"* ]]; then
    fail "sb must not be queued for install while sbt is refused (not atomic)" "$output"
else
    echo "ok: sb and sbt install atomically - sbt's refusal blocks sb too"
fi
[[ "$(receipt_field outcome)" == refused ]] || fail "receipt not marked refused for the atomic-refusal case"
rm -f "$BIN/sb"

# --- TASK-227: --main-authority drops instead of refusing, and receipts it -
stub_reports "$STALE_UNRELATED" stale
set +e
output="$(cd "$CHECKOUT" && PATH="$BIN:$PATH" bash "$GUARD" --dry-run --main-authority sbt 2>&1)"
status=$?
set -e
if [[ "$status" -ne 0 ]]; then
    fail "main-authority should install, not refuse, a diverged branch" "$output"
elif [[ "$output" != *"unrelated work"* ]]; then
    fail "main-authority did not print the dropped commit" "$output"
else
    echo "ok: main-authority installs over a diverged branch and prints what it drops"
fi
[[ "$(receipt_field outcome)" == installed ]] || fail "receipt not marked installed for the main-authority drop case"
grep -q "unrelated work" "$RECEIPT" || fail "receipt's dropped list is missing the dropped commit"

# An installed commit unreachable from this repository is dropped the same way.
stub_reports "0000000000000000000000000000000000000000" ghost
set +e
output="$(cd "$CHECKOUT" && PATH="$BIN:$PATH" bash "$GUARD" --dry-run --main-authority sbt 2>&1)"
status=$?
set -e
[[ "$status" -eq 0 ]] || fail "main-authority should install over an unknown commit, not refuse" "$output"

# --- TASK-227: an unexpired hold blocks auto-install; an expired one sweeps -
portable_iso() {
    local mins="$1" sign iso
    if [[ "$mins" == -* ]]; then sign="-"; mins="${mins#-}"; else sign="+"; fi
    iso="$(date -u -d "${sign}${mins} minutes" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null)" && { echo "$iso"; return; }
    date -u -j -v"${sign}${mins}M" +%Y-%m-%dT%H:%M:%SZ
}
FUTURE="$(portable_iso 30)"
mkdir -p "$SWITCHBARD_AUTO_INSTALL_DIR"
printf '{"branch": "feat/x", "until": "%s"}' "$FUTURE" > "$HOLD_FILE"
stub_reports "$BASE" x
set +e
output="$(cd "$CHECKOUT" && PATH="$BIN:$PATH" bash "$GUARD" --dry-run --main-authority sbt 2>&1)"
status=$?
set -e
if [[ "$status" -ne 0 ]]; then
    fail "an unexpired hold should not fail the run" "$output"
elif [[ "$output" != *"holding feat/x until"* ]]; then
    fail "unexpired hold did not log 'holding feat/x until ...'" "$output"
elif [[ "$(receipt_field outcome)" != held ]]; then
    fail "receipt not marked held while a hold is active"
elif [[ ! -f "$HOLD_FILE" ]]; then
    fail "an unexpired hold must not be swept"
else
    echo "ok: an unexpired hold blocks main-authority and receipts 'held'"
fi

PAST="$(portable_iso -30)"
printf '{"branch": "feat/x", "until": "%s"}' "$PAST" > "$HOLD_FILE"
set +e
output="$(cd "$CHECKOUT" && PATH="$BIN:$PATH" bash "$GUARD" --dry-run --main-authority sbt 2>&1)"
status=$?
set -e
if [[ "$status" -ne 0 ]]; then
    fail "an expired hold must not block the install" "$output"
elif [[ -f "$HOLD_FILE" ]]; then
    fail "an expired hold must be swept"
elif [[ "$(receipt_field outcome)" != installed ]]; then
    fail "receipt not marked installed once the hold expired"
else
    echo "ok: an expired hold is swept and main-authority proceeds"
fi

if [[ "$FAILURES" -gt 0 ]]; then
    echo "$FAILURES install-guard case(s) failed" >&2
    exit 1
fi
echo "install guard: all cases passed"
