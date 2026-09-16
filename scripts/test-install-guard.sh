#!/usr/bin/env bash
# Proves scripts/install-switchbard.sh refuses the install that caused
# TASK-172, that a manual non-main install needs --branch (TASK-227), that
# --main-authority is a one-way street - main replaces a running branch build
# only once that branch is merged or deleted on origin (TASK-272) - and that
# every attempt leaves a receipt.
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

# --- TASK-272: --main-authority is a one-way street ----------------------
# A running build main does not contain is left alone until its branch is
# delivered: merged as a PR (this repo squash-merges, so the branch commit is
# never an ancestor of main) or deleted on origin. `stale` is still pushed and
# open on origin here, and no `gh` on PATH knows of a merged PR.
ma_check() {
    local name="$1" expected="$2"
    shift 2
    local output status
    set +e
    output="$(cd "$CHECKOUT" && PATH="$BIN:$PATH" bash "$GUARD" --dry-run --main-authority sbt "$@" 2>&1)"
    status=$?
    set -e
    if [[ "$status" -ne "$expected" ]]; then
        fail "$name (expected exit $expected, got $status)" "$output"
        return 1
    fi
    echo "ok: $name"
    printf '%s' "$output" > "$WORK/last-ma-output"
}
last_ma_output() { cat "$WORK/last-ma-output" 2>/dev/null; }

stub_reports "$STALE_UNRELATED" stale
ma_check "main-authority refuses over a branch still open on origin" 1
if [[ "$(last_ma_output)" != *"main does not yet contain"* || "$(last_ma_output)" != *"unrelated work"* ]]; then
    fail "refusal must name the wait and print what would be dropped" "$(last_ma_output)"
fi
[[ "$(receipt_field outcome)" == refused ]] || fail "receipt not marked refused for the open-branch case"
[[ "$(receipt_field from_branch)" == stale ]] || fail "refused receipt does not name the installed branch"
grep -q "unrelated work" "$RECEIPT" || fail "refused receipt's dropped list is missing the commit"

# --force is the one override, and it says what it drops.
ma_check "main-authority --force installs over the open branch" 0 --force
[[ "$(last_ma_output)" == *"unrelated work"* ]] || fail "--force did not print the dropped commit" "$(last_ma_output)"
[[ "$(receipt_field outcome)" == installed ]] || fail "receipt not marked installed after --force"

# A merged PR for that branch (squash-merged: the commit is still not an
# ancestor) delivers it. `gh` is stubbed on PATH to answer as GitHub would.
cat > "$BIN/gh" <<'STUB'
#!/usr/bin/env bash
[[ "$1" == "pr" && "$2" == "list" ]] || exit 1
echo "${TEST_GH_MERGED_PR:-}"
STUB
chmod +x "$BIN/gh"
export TEST_GH_MERGED_PR=77
ma_check "main-authority installs once the branch's PR is merged (squash)" 0
[[ "$(last_ma_output)" == *"merged as PR #77"* ]] || fail "install did not name the merged PR" "$(last_ma_output)"
[[ "$(receipt_field outcome)" == installed ]] || fail "receipt not marked installed after the merged-PR case"
grep -q "unrelated work" "$RECEIPT" || fail "installed receipt's dropped list is missing the superseded commit"
export TEST_GH_MERGED_PR=""
ma_check "an unmerged PR (gh answers nothing) still refuses" 1
rm -f "$BIN/gh"

# The branch merged as a real merge commit: plain ancestry, no gh needed.
git -C "$REPO" checkout -q main
git -C "$REPO" merge -q --no-edit stale
git -C "$REPO" push -q "$ORIGIN" main
git -C "$CHECKOUT" fetch -q origin
git -C "$CHECKOUT" checkout -q --detach origin/main
ma_check "main-authority installs once main contains the installed commit" 0
[[ "$(receipt_field outcome)" == installed ]] || fail "receipt not marked installed after the merge"

# A branch deleted on origin has nothing left to protect: install, print the
# drop. The commit is unknown to this checkout too, exactly like a branch
# that was deleted before the agent ever saw it.
stub_reports "0000000000000000000000000000000000000000" ghost
ma_check "main-authority installs over a branch deleted on origin" 0
[[ "$(last_ma_output)" == *"no longer exists on origin"* ]] || fail "deleted-branch install did not say so" "$(last_ma_output)"
[[ "$(receipt_field outcome)" == installed ]] || fail "receipt not marked installed for the deleted-branch case"

# No branch name at all cannot be verified: refuse closed.
stub_reports "0000000000000000000000000000000000000000" HEAD
ma_check "main-authority refuses an unknown commit with no branch to ask about" 1
[[ "$(last_ma_output)" == *"cannot verify"* ]] || fail "unverifiable install did not say 'cannot verify'" "$(last_ma_output)"

# origin/main behind the running build (a force-push) is a rewind: refuse.
git -C "$REPO" push -q --force "$ORIGIN" "$BASE:refs/heads/main"
git -C "$CHECKOUT" fetch -q origin
git -C "$CHECKOUT" checkout -q --detach origin/main
stub_reports "$FEATURE" main
ma_check "main-authority refuses a rewind of main" 1
[[ "$(last_ma_output)" == *"rewind"* ]] || fail "rewind refusal did not say 'rewind'" "$(last_ma_output)"
git -C "$REPO" push -q --force "$ORIGIN" main
git -C "$CHECKOUT" fetch -q origin
git -C "$CHECKOUT" checkout -q --detach origin/main

# --hold is gone; asking for it is an error, not a silent no-op.
set +e
output="$(cd "$REPO" && PATH="$BIN:$PATH" bash "$GUARD" --dry-run --hold sbt 2>&1)"
status=$?
set -e
if [[ "$status" -ne 1 || "$output" != *"--hold was removed"* ]]; then
    fail "--hold should be refused as removed (got exit $status)" "$output"
else
    echo "ok: --hold is refused as removed"
fi

# A leftover TASK-227 hold file is swept, never honored.
mkdir -p "$SWITCHBARD_AUTO_INSTALL_DIR"
echo '{"branch": "stale", "until": "2999-01-01T00:00:00Z"}' > "$HOLD_FILE"
stub_reports "$BASE" main
ma_check "a leftover hold file neither blocks nor survives main-authority" 0
[[ -f "$HOLD_FILE" ]] && fail "legacy hold file was not swept"

# --- a fetch that fails must fail closed, not `|| true` -------------------

# A fetch that fails must refuse closed, not silently proceed as if main were
# unreachable-but-fine (the `|| true` this replaced would have let it through).
git -C "$CHECKOUT" remote set-url origin "$WORK/no-such-remote.git"
set +e
output="$(cd "$CHECKOUT" && PATH="$BIN:$PATH" bash "$GUARD" --dry-run --main-authority sbt 2>&1)"
status=$?
set -e
if [[ "$status" -ne 1 ]]; then
    fail "a failed fetch under --main-authority should refuse (got exit $status)" "$output"
elif [[ "$output" != *"could not verify origin/main"* ]]; then
    fail "fetch failure did not name 'could not verify origin/main'" "$output"
else
    echo "ok: a failed fetch under --main-authority fails closed"
fi
[[ "$(receipt_field outcome)" == refused ]] || fail "receipt not marked refused for the failed-fetch case"
git -C "$CHECKOUT" remote set-url origin "$ORIGIN"

if [[ "$FAILURES" -gt 0 ]]; then
    echo "$FAILURES install-guard case(s) failed" >&2
    exit 1
fi
echo "install guard: all cases passed"
