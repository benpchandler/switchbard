#!/usr/bin/env bash
# Install a Switchbard binary from this worktree, refusing a silent downgrade.
#
# WHY THIS EXISTS (TASK-172, and its recurrence on 2026-09-09)
#
# Every Switchbard binary is installed with `cargo install --path` from
# whichever worktree the agent or the owner happens to be standing in, and a
# running `sbt` re-execs itself the moment the file on disk changes. An install
# from a worktree that predates a feature therefore deletes that feature from
# every running session at once, with no prompt and no version change. The
# reported failure both times was "the Pull Requests page is gone".
#
# THE RULE
#
# An install may only move forward. The commit the *installed* binary was built
# from must be an ancestor of the commit being installed. If it is not, this
# worktree does not contain everything the user already had, and the install is
# refused. `--force` overrides, and prints exactly what is being given up.
#
# Ancestry is the right test rather than "is this branch behind main", because
# a feature branch legitimately installs a build that main does not have; what
# is never legitimate is installing a build that drops commits the user is
# already running.

set -euo pipefail

usage() {
    cat <<'USAGE'
usage: install-switchbard.sh [--force] [--dry-run] [sbt|sb]...

Installs the named binaries (default: both) from this worktree.

  --force     install even when it would drop commits from the running build
  --dry-run   report the decision for each binary and install nothing

Exit codes: 0 installed (or would install), 1 refused or failed.
USAGE
}

FORCE=0
DRY_RUN=0
TARGETS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --force) FORCE=1 ;;
        --dry-run) DRY_RUN=1 ;;
        -h|--help) usage; exit 0 ;;
        sbt|sb) TARGETS+=("$1") ;;
        *) echo "install-switchbard.sh: unknown argument: $1" >&2; usage >&2; exit 1 ;;
    esac
    shift
done
[[ ${#TARGETS[@]} -eq 0 ]] && TARGETS=(sbt sb)

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

crate_for() {
    case "$1" in
        sbt) echo "crates/switchbard-tui" ;;
        sb)  echo "crates/switchbard-task" ;;
    esac
}

# The commit an already-installed binary reports, or empty when there is no
# such binary or it predates `build-id`.
installed_commit() {
    local binary="$1" path commit
    path="$(command -v "$binary" 2>/dev/null || true)"
    [[ -z "$path" ]] && return 0
    commit="$("$path" build-id 2>/dev/null | sed -n 's/^commit=//p' || true)"
    [[ "$commit" == "unknown" ]] && return 0
    echo "$commit"
}

HEAD_COMMIT="$(git rev-parse HEAD)"
REFUSED=0

for binary in "${TARGETS[@]}"; do
    prior="$(installed_commit "$binary")"

    if [[ -z "$prior" ]]; then
        echo "$binary: no build stamp on the installed binary; installing $(git rev-parse --short HEAD)"
    elif [[ "$prior" == "$HEAD_COMMIT" ]]; then
        echo "$binary: already built from $(git rev-parse --short HEAD); reinstalling"
    elif git merge-base --is-ancestor "$prior" HEAD 2>/dev/null; then
        echo "$binary: $(git rev-parse --short "$prior") -> $(git rev-parse --short HEAD) (+$(git rev-list --count "$prior"..HEAD) commits)"
    else
        # `git cat-file -e` first: a commit from a deleted worktree's branch may
        # no longer be reachable here, and "cannot verify" must refuse too.
        if ! git cat-file -e "${prior}^{commit}" 2>/dev/null; then
            echo "$binary: REFUSED - the installed build came from commit $prior, which does not exist in this repository." >&2
            echo "  Nothing here can prove this install would not lose work. Fetch that commit, or re-run with --force." >&2
        else
            lost="$(git rev-list --count HEAD.."$prior")"
            echo "$binary: REFUSED - this would drop $lost commit(s) you are already running." >&2
            echo "  installed: $(git rev-parse --short "$prior") ($(git log -1 --format=%s "$prior"))" >&2
            echo "  this tree: $(git rev-parse --short HEAD) ($(git log -1 --format=%s HEAD))" >&2
            echo "  Merge or rebase onto the installed build first, or re-run with --force." >&2
        fi
        if [[ $FORCE -eq 0 ]]; then
            REFUSED=1
            continue
        fi
        echo "  --force given; installing anyway." >&2
    fi

    if [[ $DRY_RUN -eq 1 ]]; then
        echo "$binary: dry run; not installing"
        continue
    fi
    cargo install --path "$(crate_for "$binary")" --locked
done

exit "$REFUSED"
