#!/usr/bin/env bash
# Proves scripts/install-switchbard.sh refuses the install that caused TASK-172.
#
# Each case builds a throwaway repository with a real divergent history and a
# stub binary on PATH that answers `build-id` the way an installed sbt does, so
# the guard is exercised through exactly the interface it uses in production.

set -euo pipefail

GUARD="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/install-switchbard.sh"
FAILURES=0
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A repo with `main` carrying a feature commit and `stale` branching before it.
REPO="$WORK/repo"
git init -q "$REPO"
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

# A stub `sbt` reporting whatever commit the case pins.
BIN="$WORK/bin"
mkdir -p "$BIN"
stub_reports() {
    cat > "$BIN/sbt" <<STUB
#!/usr/bin/env bash
[[ "\${1:-}" == "build-id" ]] || exit 1
printf 'commit=%s\nbranch=x\ndirty=false\nversion=0.4.0\n' "$1"
STUB
    chmod +x "$BIN/sbt"
}

check() {
    local name="$1" expected="$2" head_branch="$3" output status
    git -C "$REPO" checkout -q "$head_branch"
    set +e
    output="$(cd "$REPO" && PATH="$BIN:$PATH" bash "$GUARD" --dry-run sbt 2>&1)"
    status=$?
    set -e
    if [[ "$status" -ne "$expected" ]]; then
        echo "FAIL: $name (expected exit $expected, got $status)"
        echo "$output" | sed 's/^/    /'
        FAILURES=$((FAILURES + 1))
    else
        echo "ok: $name"
    fi
}

# The exact TASK-172 shape: running a build that has the page, installing from
# a branch that never had it.
stub_reports "$FEATURE"
check "refuses installing a tree missing the running build's commits" 1 stale

# The same tree is fine once it contains that commit.
git -C "$REPO" checkout -q stale
git -C "$REPO" merge -q --no-edit main
check "allows installing a tree that contains the running build" 0 stale

# Forward from an older build is the ordinary case and must not be blocked.
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

if [[ "$FAILURES" -gt 0 ]]; then
    echo "$FAILURES install-guard case(s) failed" >&2
    exit 1
fi
echo "install guard: all cases passed"
