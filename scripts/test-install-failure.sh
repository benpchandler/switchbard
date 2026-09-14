#!/usr/bin/env bash
# Proves a cargo build failure during install never half-installs: a failed
# build for one binary must not leave the other one replaced either, and it
# must leave a "failed" receipt naming what happened (TASK-227 audit #2).
#
# Builds two real, minimal, standalone crates at the exact relative paths
# install-switchbard.sh's crate_for() maps sb/sbt to: sb compiles cleanly,
# sbt does not. Nothing here is stubbed except which binaries the machine
# already reports installed (there are none, so neither guard has anything
# to refuse on ancestry - the build failure is the only thing under test).

set -euo pipefail

GUARD="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/install-switchbard.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
FAILURES=0

REPO="$WORK/repo"
git init -q -b main "$REPO"
git -C "$REPO" config user.email test@example.com
git -C "$REPO" config user.name Test

mkdir -p "$REPO/crates/switchbard-task/src" "$REPO/crates/switchbard-tui/src"
cat > "$REPO/crates/switchbard-task/Cargo.toml" <<'TOML'
[package]
name = "sb"
version = "0.1.0"
edition = "2021"
TOML
echo 'fn main() {}' > "$REPO/crates/switchbard-task/src/main.rs"

cat > "$REPO/crates/switchbard-tui/Cargo.toml" <<'TOML'
[package]
name = "sbt"
version = "0.1.0"
edition = "2021"
TOML
echo 'fn main() { this is not valid rust syntax' > "$REPO/crates/switchbard-tui/src/main.rs"

(cd "$REPO/crates/switchbard-task" && cargo generate-lockfile) >/dev/null 2>&1
(cd "$REPO/crates/switchbard-tui" && cargo generate-lockfile) >/dev/null 2>&1

git -C "$REPO" add -A
git -C "$REPO" commit -qm "fixture: sb builds cleanly, sbt does not"

# No stamp on either binary (neither is "already installed" here), so the
# ancestry guard has nothing to refuse - only the build itself is under test.
BIN="$WORK/bin"
mkdir -p "$BIN"
for name in sb sbt; do
    cat > "$BIN/$name" <<'STUB'
#!/usr/bin/env bash
exit 1
STUB
    chmod +x "$BIN/$name"
done

INSTALL_ROOT="$WORK/cargo-install-root"
mkdir -p "$INSTALL_ROOT/bin"
export CARGO_INSTALL_ROOT="$INSTALL_ROOT"
export SWITCHBARD_AUTO_INSTALL_DIR="$WORK/state"

set +e
output="$(cd "$REPO" && PATH="$BIN:$PATH" bash "$GUARD" --branch sb sbt 2>&1)"
status=$?
set -e

if [[ "$status" -ne 1 ]]; then
    echo "FAIL: a broken sbt build should exit 1 (got $status)"
    echo "$output" | sed 's/^/    /'
    FAILURES=$((FAILURES + 1))
else
    echo "ok: a broken sbt build exits 1"
fi

if [[ -e "$INSTALL_ROOT/bin/sb" ]]; then
    echo "FAIL: sb was installed even though sbt's build failed (not atomic)"
    FAILURES=$((FAILURES + 1))
else
    echo "ok: sb is not installed while sbt's build fails (both-or-neither)"
fi
if [[ -e "$INSTALL_ROOT/bin/sbt" ]]; then
    echo "FAIL: sbt was installed despite its own build failing"
    FAILURES=$((FAILURES + 1))
fi

outcome="$(sed -n 's/.*"outcome": *"\([^"]*\)".*/\1/p' "$WORK/state/last-install.json" 2>/dev/null | head -1)"
if [[ "$outcome" != "failed" ]]; then
    echo "FAIL: receipt outcome is '$outcome', expected 'failed'"
    echo "$output" | sed 's/^/    /'
    FAILURES=$((FAILURES + 1))
else
    echo "ok: receipt outcome is 'failed'"
fi

if [[ "$output" != *"sbt"* ]]; then
    echo "FAIL: failure output does not name sbt as the binary that failed"
    FAILURES=$((FAILURES + 1))
else
    echo "ok: failure output names which binary failed"
fi

if [[ "$FAILURES" -gt 0 ]]; then
    echo "$FAILURES install-failure case(s) failed" >&2
    exit 1
fi
echo "install failure handling: all cases passed"
