#!/usr/bin/env bash
# Install a Switchbard binary, refusing a silent downgrade.
#
# WHY THIS EXISTS (TASK-172, and its recurrence on 2026-09-09 and 2026-09-13)
#
# Every Switchbard binary is installed with `cargo install --path`, and a
# running `sbt` re-execs itself the moment the file on disk changes. An
# install from a tree that predates a feature therefore deletes that feature
# from every running session at once, with no prompt and no version change.
#
# TWO CALLERS, TWO RULES (TASK-227)
#
# A human standing in a worktree and the unattended launchd agent
# (`scripts/auto-install-main.sh`) need different answers to "is this safe":
#
# - A human's worktree may legitimately be a feature branch that does not
#   (yet) contain everything main has, or vice versa. The rule for a manual
#   install is ANCESTRY: refuse unless the target contains every commit the
#   running binary already has. `--force` overrides.
# - The unattended agent only ever installs origin/main's own tip, passed
#   with `--main-authority`. There main IS the authority, so a running build
#   that main does not contain (a feature branch someone installed on
#   purpose) is not a downgrade to refuse - it is exactly the case this repo
#   needs auto-install to resolve on its own. `--main-authority` never
#   refuses on ancestry; it installs and prints precisely what is dropped.
#   It still refuses if the checkout is not actually origin/main's tip -
#   that would mean the flag is being used somewhere it should not be.
#
# A manual install from any branch but main must now pass `--branch`,
# so "I meant to install a feature branch" is always an explicit choice,
# never an accident of `cd`. `--hold <duration>` (default 2h) additionally
# tells the auto-install agent to leave that choice alone for a while.
#
# Every attempt (installed, refused, held, failed) leaves a durable receipt at
# `$STATE_DIR/last-install.json` so a person - or sbt's startup banner - can
# see what happened without reading the log.

set -euo pipefail

usage() {
    cat <<'USAGE'
usage: install-switchbard.sh [options] [sbt|sb]...

Installs the named binaries (default: both) from this worktree.

  --force            install even when a guard below would refuse
  --dry-run          report the decision for each binary and install nothing
  --branch           acknowledge an intentional install from a non-main branch
  --hold [DURATION]  protect that branch install from auto-install for DURATION
                      (30m, 2h, ...; default 2h, capped at 24h). Implies
                      --branch. Refused on main - there is nothing to hold
                      main back from.
  --main-authority   for the auto-install agent only: the target tree must be
                      origin/main's own tip; never refuses on ancestry, only
                      on that precondition. Prints what it drops.

Every attempt (installed, refused, held, failed) leaves a receipt at
$SWITCHBARD_AUTO_INSTALL_DIR/last-install.json (default
~/.switchbard/auto-install/last-install.json).

Exit codes: 0 an install happened, would happen (--dry-run), or an unexpired
hold left it alone. 1 refused, or a build failed (nothing is ever replaced
when a build fails - see perform_installs below).
USAGE
}

STATE_DIR="${SWITCHBARD_AUTO_INSTALL_DIR:-$HOME/.switchbard/auto-install}"
HOLD_FILE="$STATE_DIR/hold.json"
RECEIPT_FILE="$STATE_DIR/last-install.json"
MAX_DROPPED=20

FORCE=0
DRY_RUN=0
BRANCH_ACK=0
MAIN_AUTHORITY=0
HOLD_DURATION=""
TARGETS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --force) FORCE=1 ;;
        --dry-run) DRY_RUN=1 ;;
        --branch) BRANCH_ACK=1 ;;
        --hold)
            BRANCH_ACK=1
            if [[ $# -ge 2 && "$2" =~ ^[0-9]+[mh]$ ]]; then
                HOLD_DURATION="$2"
                shift
            else
                HOLD_DURATION="2h"
            fi
            ;;
        --main-authority) MAIN_AUTHORITY=1 ;;
        -h|--help) usage; exit 0 ;;
        sbt|sb) TARGETS+=("$1") ;;
        *) echo "install-switchbard.sh: unknown argument: $1" >&2; usage >&2; exit 1 ;;
    esac
    shift
done
[[ ${#TARGETS[@]} -eq 0 ]] && TARGETS=(sbt sb)
BINARY_LABEL="$(IFS=,; echo "${TARGETS[*]}")"

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

crate_for() {
    case "$1" in
        sbt) echo "crates/switchbard-tui" ;;
        sb)  echo "crates/switchbard-task" ;;
    esac
}

# Every wait on the network or an external binary is bounded (Bash
# Power-of-10 rule 2): `timeout`/`gtimeout` where available, else a `perl
# alarm` shim (perl ships on every machine this runs on already - no new
# dependency), else run unbounded as a last resort rather than refuse to run
# at all.
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

# The commit (or branch) an already-installed binary reports, or empty when
# there is no such binary or it predates `build-id`.
installed_field() {
    local binary="$1" key="$2" path
    path="$(command -v "$binary" 2>/dev/null || true)"
    [[ -z "$path" ]] && return 0
    run_with_timeout 5 "$path" build-id 2>/dev/null | sed -n "s/^${key}=//p" || true
}
installed_commit() { installed_field "$1" commit; }
installed_branch() { installed_field "$1" branch; }

# --- portable UTC timestamp helpers (macOS `date` vs. GNU `date`) ----------
now_iso() { date -u +%Y-%m-%dT%H:%M:%SZ; }
iso_to_epoch() {
    date -u -d "$1" +%s 2>/dev/null || date -u -j -f '%Y-%m-%dT%H:%M:%SZ' "$1" +%s 2>/dev/null || echo 0
}
iso_after_now() {
    local secs="$1"
    date -u -d "+${secs} seconds" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null \
        || date -u -j -v "+${secs}S" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null
}
duration_to_seconds() {
    local d="$1"
    if [[ "$d" =~ ^([0-9]+)m$ ]]; then
        echo $(( 10#${BASH_REMATCH[1]} * 60 ))
    elif [[ "$d" =~ ^([0-9]+)h$ ]]; then
        echo $(( 10#${BASH_REMATCH[1]} * 3600 ))
    else
        echo 7200
    fi
}

# --- tiny hand-rolled JSON, so a comma or a quote in a commit subject can
# never produce a receipt sbt's parser trips on ------------------------------
json_escape() {
    local s="$1"
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    s="${s//$'\n'/\\n}"
    s="${s//$'\t'/\\t}"
    printf '%s' "$s"
}
json_str_or_null() {
    if [[ -z "${1:-}" ]]; then printf 'null'; else printf '"%s"' "$(json_escape "$1")"; fi
}
json_array() {
    local first=1
    printf '['
    for item in "$@"; do
        [[ $first -eq 1 ]] && first=0 || printf ','
        printf '"%s"' "$(json_escape "$item")"
    done
    printf ']'
}

# Writes $1 to path $2 as a temp file in the same directory, then `mv`s it
# into place - a reader (sbt's startup banner, polling on its own schedule)
# can never observe a half-written file.
atomic_write() {
    local contents="$1" path="$2" tmp
    tmp="$(mktemp "${path}.XXXXXX")"
    printf '%s' "$contents" > "$tmp"
    mv -f "$tmp" "$path"
}

write_receipt() {
    local outcome="$1" from_commit="$2" from_branch="$3" to_commit="$4" to_branch="$5" reason="$6" hold_until="$7"
    shift 7
    mkdir -p "$STATE_DIR"
    local body
    body="$(cat <<JSON
{
  "ts": $(json_str_or_null "$(now_iso)"),
  "binary": $(json_str_or_null "$BINARY_LABEL"),
  "outcome": $(json_str_or_null "$outcome"),
  "from_commit": $(json_str_or_null "$from_commit"),
  "from_branch": $(json_str_or_null "$from_branch"),
  "to_commit": $(json_str_or_null "$to_commit"),
  "to_branch": $(json_str_or_null "$to_branch"),
  "dropped": $(json_array "$@"),
  "reason": $(json_str_or_null "$reason"),
  "hold_until": $(json_str_or_null "$hold_until")
}
JSON
)"
    atomic_write "$body" "$RECEIPT_FILE"
}

# Up to MAX_DROPPED "shortsha subject" lines reachable from $1 but not $2.
dropped_commits() {
    local prior="$1" head="$2" sha
    git rev-list --max-count="$MAX_DROPPED" "$head..$prior" 2>/dev/null | while read -r sha; do
        printf '%s %s\n' "$(git rev-parse --short "$sha")" "$(git log -1 --format=%s "$sha")"
    done
}

# Where `cargo install --path` would put the binaries by default - the same
# resolution cargo itself uses, so a build failure's scratch root lands on
# the same filesystem as the real destination and the final move is a plain
# rename, not a copy.
CARGO_ROOT="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}"
BIN_DIR="$CARGO_ROOT/bin"
TMP_INSTALL_ROOT=""
# An EXIT trap's own exit status becomes the script's if it's the last thing
# that ran - `return 0` is load-bearing here, not decoration: without it, a
# no-op cleanup (`[[ -n "" ]]` is false) would silently turn every `exit 0`
# into a 1.
cleanup_tmp_install_root() {
    if [[ -n "$TMP_INSTALL_ROOT" ]]; then
        rm -rf "$TMP_INSTALL_ROOT"
    fi
    return 0
}
trap cleanup_tmp_install_root EXIT

# Builds every named target into a scratch root first; only once ALL of them
# build cleanly does it move the resulting binaries into place with a
# same-filesystem `mv` (a rename, so it can't leave a half-written binary
# behind). A build failure never replaces anything - not even the target that
# built fine - and writes a "failed" receipt naming which binary and why
# before exiting 1 (TASK-227 audit #2).
perform_installs() {
    mkdir -p "$CARGO_ROOT"
    TMP_INSTALL_ROOT="$(mktemp -d "$CARGO_ROOT/.switchbard-install.XXXXXX")"
    local binary crate_path status
    for binary in "$@"; do
        crate_path="$(crate_for "$binary")"
        status=0
        # `tee` keeps normal live build output on stderr while also capturing
        # it, so a failure's receipt can quote the actual last line.
        cargo install --path "$crate_path" --locked --root "$TMP_INSTALL_ROOT" \
            2> >(tee "$TMP_INSTALL_ROOT/$binary.stderr" >&2) || status=$?
        wait
        if [[ $status -ne 0 ]]; then
            local last_line
            last_line="$(tail -1 "$TMP_INSTALL_ROOT/$binary.stderr" 2>/dev/null | cut -c1-200)"
            REASON="cargo install failed for $binary (exit $status): ${last_line:-no output captured}"
            echo "$binary: REFUSED - $REASON" >&2
            write_receipt failed "$FROM_COMMIT" "$FROM_BRANCH" "$HEAD_COMMIT" "$TO_BRANCH_LABEL" "$REASON" ""
            exit 1
        fi
    done
    mkdir -p "$BIN_DIR"
    for binary in "$@"; do
        mv -f "$TMP_INSTALL_ROOT/bin/$binary" "$BIN_DIR/$binary"
    done
}

HEAD_COMMIT="$(git rev-parse HEAD)"
CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"

if [[ -n "$HOLD_DURATION" ]]; then
    HOLD_SECONDS="$(duration_to_seconds "$HOLD_DURATION")"
    if [[ "$HOLD_SECONDS" -gt 86400 ]]; then
        REASON="--hold $HOLD_DURATION exceeds the 24h cap"
        echo "REFUSED - $REASON." >&2
        write_receipt refused "" "" "$HEAD_COMMIT" "$CURRENT_BRANCH" "$REASON" ""
        exit 1
    fi
    if [[ "$CURRENT_BRANCH" == "main" ]]; then
        REASON="--hold on main is meaningless - main is never held back from itself"
        echo "REFUSED - $REASON." >&2
        write_receipt refused "" "" "$HEAD_COMMIT" "$CURRENT_BRANCH" "$REASON" ""
        exit 1
    fi
fi

if [[ $MAIN_AUTHORITY -eq 1 ]]; then
    mkdir -p "$STATE_DIR"
    if [[ -f "$HOLD_FILE" ]]; then
        hold_branch="$(sed -n 's/.*"branch": *"\([^"]*\)".*/\1/p' "$HOLD_FILE" | head -1)"
        hold_until="$(sed -n 's/.*"until": *"\([^"]*\)".*/\1/p' "$HOLD_FILE" | head -1)"
        if [[ "$hold_branch" == "main" ]]; then
            # Only a manual install writes this file, and it now refuses to
            # write one for main - a hold naming main here is stale or hand-
            # edited, and holding "main" back from main is meaningless.
            echo "dropping an invalid hold on main (a hold protects a non-main branch, never main)" >&2
            rm -f "$HOLD_FILE"
        else
            hold_epoch="$(iso_to_epoch "$hold_until")"
            now_epoch="$(date -u +%s)"
            if [[ -n "$hold_until" && "$hold_epoch" -gt "$now_epoch" && $FORCE -eq 0 ]]; then
                echo "holding $hold_branch until $hold_until"
                write_receipt held "" "$hold_branch" "$HEAD_COMMIT" origin/main "manual hold active" "$hold_until"
                exit 0
            fi
            rm -f "$HOLD_FILE"
        fi
    fi

    if [[ $FORCE -eq 0 ]]; then
        if ! run_with_timeout 60 git fetch -q origin main 2>/dev/null; then
            REASON="could not verify origin/main (git fetch failed or timed out)"
            echo "REFUSED - $REASON; --main-authority requires it." >&2
            write_receipt refused "" "" "$HEAD_COMMIT" origin/main "$REASON" ""
            exit 1
        fi
        ORIGIN_MAIN="$(git rev-parse origin/main 2>/dev/null || true)"
        if [[ -z "$ORIGIN_MAIN" ]]; then
            REASON="cannot resolve origin/main in this checkout"
            echo "REFUSED - $REASON; --main-authority requires it." >&2
            write_receipt refused "" "" "$HEAD_COMMIT" origin/main "$REASON" ""
            exit 1
        fi
        if [[ "$HEAD_COMMIT" != "$ORIGIN_MAIN" ]]; then
            REASON="this checkout ($(git rev-parse --short "$HEAD_COMMIT")) is not origin/main's tip ($(git rev-parse --short "$ORIGIN_MAIN"))"
            echo "REFUSED - $REASON; --main-authority only installs origin/main's own tip." >&2
            write_receipt refused "" "" "$HEAD_COMMIT" origin/main "$REASON" ""
            exit 1
        fi
    fi

    FROM_COMMIT=""
    FROM_BRANCH=""
    DROPPED=()
    REASON=""
    for binary in "${TARGETS[@]}"; do
        prior="$(installed_commit "$binary")"
        prior_branch="$(installed_branch "$binary")"
        if [[ -z "$prior" ]]; then
            echo "$binary: no build stamp on the installed binary; installing $(git rev-parse --short "$HEAD_COMMIT")"
        elif [[ "$prior" == "$HEAD_COMMIT" ]]; then
            echo "$binary: already built from $(git rev-parse --short "$HEAD_COMMIT"); reinstalling"
        elif git cat-file -e "${prior}^{commit}" 2>/dev/null && git merge-base --is-ancestor "$prior" "$HEAD_COMMIT" 2>/dev/null; then
            echo "$binary: $(git rev-parse --short "$prior") -> $(git rev-parse --short "$HEAD_COMMIT") (+$(git rev-list --count "$prior".."$HEAD_COMMIT") commits)"
        else
            THIS_DROPPED=()
            if git cat-file -e "${prior}^{commit}" 2>/dev/null; then
                # origin/main itself moved backward relative to what is
                # already running (a force-push) is a distinct, more alarming
                # case than "a stray feature branch was installed" - label it.
                if git merge-base --is-ancestor "$HEAD_COMMIT" "$prior" 2>/dev/null; then
                    THIS_REASON="rewind: origin/main is behind the installed build's branch '$prior_branch'"
                else
                    THIS_REASON="dropping branch '$prior_branch', not on origin/main"
                fi
                while IFS= read -r line; do THIS_DROPPED+=("$line"); done < <(dropped_commits "$prior" "$HEAD_COMMIT")
                echo "$binary: main authority - $THIS_REASON ($(git rev-list --count "$HEAD_COMMIT".."$prior") commit(s)):" >&2
                printf '  %s\n' "${THIS_DROPPED[@]+"${THIS_DROPPED[@]}"}" >&2
            else
                THIS_REASON="dropping unknown commit $prior from branch '$prior_branch'"
                THIS_DROPPED=("$prior (unknown commit; not reachable in this repository)")
                echo "$binary: main authority - $THIS_REASON; origin/main is authoritative" >&2
            fi
            # sbt is what reads the receipt; prefer its story, else keep the
            # first divergence seen so the receipt is never empty.
            if [[ "$binary" == "sbt" || -z "$FROM_COMMIT" ]]; then
                FROM_COMMIT="$prior"
                FROM_BRANCH="$prior_branch"
                DROPPED=("${THIS_DROPPED[@]+"${THIS_DROPPED[@]}"}")
                REASON="$THIS_REASON"
            fi
        fi
    done
    TO_BRANCH_LABEL=origin/main
    if [[ $DRY_RUN -eq 1 ]]; then
        for binary in "${TARGETS[@]}"; do
            echo "$binary: dry run; not installing"
        done
    else
        perform_installs "${TARGETS[@]}"
    fi
    # bash 3.2 (macOS's /bin/bash) treats a *declared-but-empty* array as
    # unset under `set -u`; the `+` guard is the portable way to expand
    # "zero or more elements" without tripping that quirk.
    write_receipt installed "$FROM_COMMIT" "$FROM_BRANCH" "$HEAD_COMMIT" origin/main "$REASON" "" "${DROPPED[@]+"${DROPPED[@]}"}"
    exit 0
fi

# --- manual install: ancestry guard, now gated by --branch off main --------
if [[ "$CURRENT_BRANCH" != "main" && $BRANCH_ACK -eq 0 && $FORCE -eq 0 ]]; then
    REASON="installing branch '$CURRENT_BRANCH', not main; re-run with --branch to confirm"
    echo "REFUSED - $REASON." >&2
    write_receipt refused "" "" "$HEAD_COMMIT" "$CURRENT_BRANCH" "$REASON" ""
    exit 1
fi

# Two passes, so neither binary is ever replaced when the other refuses
# (TASK-227 AC #3): first decide every target's verdict, then only install if
# nothing refused.
REFUSED=0
REASON=""
FROM_COMMIT=""
FROM_BRANCH=""
for binary in "${TARGETS[@]}"; do
    prior="$(installed_commit "$binary")"

    if [[ -z "$prior" ]]; then
        echo "$binary: no build stamp on the installed binary; installing $(git rev-parse --short "$HEAD_COMMIT")"
    elif [[ "$prior" == "$HEAD_COMMIT" ]]; then
        echo "$binary: already built from $(git rev-parse --short "$HEAD_COMMIT"); reinstalling"
    elif git merge-base --is-ancestor "$prior" "$HEAD_COMMIT" 2>/dev/null; then
        echo "$binary: $(git rev-parse --short "$prior") -> $(git rev-parse --short "$HEAD_COMMIT") (+$(git rev-list --count "$prior".."$HEAD_COMMIT") commits)"
    else
        # `git cat-file -e` first: a commit from a deleted worktree's branch may
        # no longer be reachable here, and "cannot verify" must refuse too.
        if ! git cat-file -e "${prior}^{commit}" 2>/dev/null; then
            echo "$binary: REFUSED - the installed build came from commit $prior, which does not exist in this repository." >&2
            echo "  Nothing here can prove this install would not lose work. Fetch that commit, or re-run with --force." >&2
            REASON="installed build's commit $prior is unknown to this repository"
        else
            lost="$(git rev-list --count "$HEAD_COMMIT".."$prior")"
            echo "$binary: REFUSED - this would drop $lost commit(s) you are already running." >&2
            echo "  installed: $(git rev-parse --short "$prior") ($(git log -1 --format=%s "$prior"))" >&2
            echo "  this tree: $(git rev-parse --short "$HEAD_COMMIT") ($(git log -1 --format=%s "$HEAD_COMMIT"))" >&2
            echo "  Merge or rebase onto the installed build first, or re-run with --force." >&2
            REASON="would drop $lost commit(s) already running"
        fi
        if [[ "$binary" == "sbt" || -z "$FROM_COMMIT" ]]; then
            FROM_COMMIT="$prior"
            FROM_BRANCH="$(installed_branch "$binary")"
        fi
        [[ $FORCE -eq 0 ]] && REFUSED=1
    fi
done

if [[ $REFUSED -eq 1 && $FORCE -eq 0 ]]; then
    echo "REFUSED - not installing any of: ${TARGETS[*]} (neither binary moves while either refuses)." >&2
    write_receipt refused "$FROM_COMMIT" "$FROM_BRANCH" "$HEAD_COMMIT" "$CURRENT_BRANCH" "$REASON" ""
    exit 1
fi
if [[ $REFUSED -eq 1 ]]; then
    echo "--force given; installing ${TARGETS[*]} anyway." >&2
fi

TO_BRANCH_LABEL="$CURRENT_BRANCH"
if [[ $DRY_RUN -eq 1 ]]; then
    for binary in "${TARGETS[@]}"; do
        echo "$binary: dry run; not installing"
    done
else
    perform_installs "${TARGETS[@]}"
fi

HOLD_UNTIL=""
if [[ -n "$HOLD_DURATION" && $DRY_RUN -eq 0 ]]; then
    mkdir -p "$STATE_DIR"
    HOLD_UNTIL="$(iso_after_now "$(duration_to_seconds "$HOLD_DURATION")")"
    atomic_write "{\"branch\": $(json_str_or_null "$CURRENT_BRANCH"), \"until\": $(json_str_or_null "$HOLD_UNTIL")}" "$HOLD_FILE"
    echo "auto-install replaces this with main after $HOLD_UNTIL"
elif [[ "$CURRENT_BRANCH" != "main" && $DRY_RUN -eq 0 ]]; then
    echo "auto-install replaces this with main within the next check (~60s); pass --hold to delay that"
fi

write_receipt installed "$FROM_COMMIT" "$FROM_BRANCH" "$HEAD_COMMIT" "$CURRENT_BRANCH" "" "$HOLD_UNTIL"
exit 0
