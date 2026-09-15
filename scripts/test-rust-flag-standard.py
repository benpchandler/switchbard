#!/usr/bin/env python3
"""Guard the one Rust flag standard shared by every cargo entry point.

mise tasks, Git hooks, CI, `cargo install --path` and plain cargo must compile
with the same flags. rustflags are part of every artifact's identity: a task
that sets RUSTFLAGS rebuilds every dependency into a second copy the first time
anyone alternates with a command that does not, which is how worktree targets
reached 14-21 GB. Warnings are denied by [workspace.lints.rust] in Cargo.toml
instead; lint levels reach only workspace crates, so dependency artifacts stay
shared.

The guard keeps both halves - no tracked file sets rustflags, and every
workspace member inherits the one deny - and proves its detectors against a
fixture repository before it trusts a clean result here.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path
from typing import Any

GUARD_PATH = "scripts/test-rust-flag-standard.py"
MAX_MEMBERS = 256

# git grep matches this case-insensitively, which also covers
# CARGO_BUILD_RUSTFLAGS and CARGO_ENCODED_RUSTFLAGS. The scan is textual on
# purpose: any file type that launches cargo can set rustflags, so no single
# parser owns the question.
RUSTFLAGS_PATTERN = (
    # Assignments: RUSTFLAGS=, RUSTFLAGS+=, RUSTFLAGS:, rustflags = [...],
    # build.rustflags=, env["RUSTFLAGS"] =
    r"""rustflags[]"'+]*[[:space:]]*[:=]"""
    # Calls: .env("RUSTFLAGS", ...), setdefault('RUSTFLAGS', ...)
    r"""|["'][a-z_]*rustflags["'][[:space:]]*,"""
)

REMEDY = (
    'Deny warnings only with [workspace.lints.rust] warnings = "deny" in Cargo.toml, '
    "inherited by every member through [lints] workspace = true, and never set "
    "rustflags (CLAUDE.md, Gates)."
)

# Each line sets rustflags and must be reported, in order, and nothing else.
FIXTURE_SETS = (
    'env = { RUSTFLAGS = "-D warnings" }',
    'RUSTFLAGS="-D warnings" cargo test',
    'export RUSTFLAGS+=" -C debuginfo=0"',
    "    RUSTFLAGS: -D warnings",
    'rustflags = ["-D", "warnings"]',
    "cargo --config 'build.rustflags=[\"-Dwarnings\"]' build",
    "CARGO_ENCODED_RUSTFLAGS=-Dwarnings cargo build",
    'env["RUSTFLAGS"] = "-D warnings"',
    'Command::new("cargo").env("CARGO_BUILD_RUSTFLAGS", "-D warnings")',
)
FIXTURE_FILES = (
    ("sets.txt", "\n".join(FIXTURE_SETS) + "\n"),
    ("mentions.txt", "unset RUSTFLAGS\n# RUSTFLAGS changes artifact identity.\n"),
    ("notes.md", 'RUSTFLAGS="-D warnings" cargo test\n'),
    (
        "Cargo.toml",
        (
            '[workspace]\nmembers = ["crates/*"]\n\n'
            '[workspace.lints.rust]\nwarnings = "warn"\n'
        ),
    ),
    (
        "crates/opted-in/Cargo.toml",
        '[package]\nname = "opted-in"\n\n[lints]\nworkspace = true\n',
    ),
    (
        "crates/opted-out/Cargo.toml",
        '[package]\nname = "opted-out"\nworkspace = true\n',
    ),
)
MISSING_DENY = 'Cargo.toml does not set [workspace.lints.rust] warnings = "deny"'
NOT_INHERITED = "does not inherit the warning policy ([lints] workspace = true)"
FIXTURE_POLICY_VIOLATIONS = (
    MISSING_DENY,
    f"crates/opted-out/Cargo.toml {NOT_INHERITED}",
)


def git(cwd: Path, *args: str) -> str:
    """Run git in cwd and return its stripped stdout; any failure raises."""
    if not cwd.is_dir():
        raise RuntimeError(f"git working directory does not exist: {cwd}")
    result = subprocess.run(
        ["git", "-C", str(cwd), *args], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"git {' '.join(args)} failed in {cwd}: {result.stderr.strip()}"
        )
    return result.stdout.strip()


def tracked_rustflags(repo: Path) -> list[str]:
    """Return path:line:text for each tracked line that sets rustflags.

    Markdown is prose and history (backlog records, evidence logs), never build
    input, so it is not scanned.
    """
    if not (repo / ".git").exists():
        raise RuntimeError(f"not a git repository: {repo}")
    command = ["git", "-C", str(repo), "grep", "-n", "-I", "-i", "-E"]
    command += [RUSTFLAGS_PATTERN, "--", ".", ":!*.md", f":!{GUARD_PATH}"]
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    # Exit 1 means no match; anything higher is a failed scan, never a clean one.
    if result.returncode > 1:
        raise RuntimeError(f"git grep failed in {repo}: {result.stderr.strip()}")
    hits = result.stdout.splitlines()
    if (result.returncode == 0) != bool(hits):
        raise RuntimeError(f"git grep exit {result.returncode} disagrees with output")
    return hits


def member_manifests(repo: Path, workspace: dict[str, Any]) -> list[Path]:
    """Expand [workspace] members, literal paths or globs, to member manifests."""
    patterns = workspace.get("members", [])
    if not isinstance(patterns, list) or not all(isinstance(p, str) for p in patterns):
        raise RuntimeError("Cargo.toml [workspace] members must be a list of paths")
    manifests: list[Path] = []
    for pattern in patterns[:MAX_MEMBERS]:
        matches = sorted(repo.glob(pattern))
        if not matches:
            raise RuntimeError(f"workspace member {pattern!r} matches nothing")
        manifests += [match / "Cargo.toml" for match in matches]
    if len(patterns) > MAX_MEMBERS or len(manifests) > MAX_MEMBERS:
        raise RuntimeError(f"over {MAX_MEMBERS} workspace members; raise MAX_MEMBERS")
    return manifests


def warning_policy_violations(repo: Path) -> list[str]:
    """Return each way the workspace escapes its one warnings = "deny" policy."""
    root = tomllib.loads((repo / "Cargo.toml").read_text(encoding="utf-8"))
    workspace = root.get("workspace")
    if not isinstance(workspace, dict):
        raise RuntimeError(f"{repo / 'Cargo.toml'} has no [workspace] table")
    violations: list[str] = []
    if workspace.get("lints", {}).get("rust", {}).get("warnings") != "deny":
        violations.append(MISSING_DENY)
    for manifest in member_manifests(repo, workspace):
        member = tomllib.loads(manifest.read_text(encoding="utf-8"))
        if member.get("lints", {}).get("workspace") is not True:
            violations.append(f"{manifest.relative_to(repo)} {NOT_INHERITED}")
    return violations


def prove_detectors() -> None:
    """Fail unless both scans report every seeded violation and nothing compliant."""
    with tempfile.TemporaryDirectory(prefix="rust-flag-standard-") as scratch:
        fixture = Path(scratch)
        git(fixture, "init", "-q")
        for relative, text in FIXTURE_FILES:
            path = fixture / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        git(fixture, "add", ".")
        expected = [f"sets.txt:{n}:{line}" for n, line in enumerate(FIXTURE_SETS, 1)]
        found = tracked_rustflags(fixture)
        if found != expected:
            raise RuntimeError(f"rustflags detector drifted:\n{expected=}\n{found=}")
        policy = warning_policy_violations(fixture)
        if policy != list(FIXTURE_POLICY_VIOLATIONS):
            raise RuntimeError(f"warning policy detector drifted:\n{policy=}")


def main() -> int:
    """Prove the detectors, then report every violation in this repository."""
    prove_detectors()
    repo = Path(git(Path(__file__).resolve().parent, "rev-parse", "--show-toplevel"))
    problems = [f"sets rustflags: {hit}" for hit in tracked_rustflags(repo)]
    problems += warning_policy_violations(repo)
    if problems:
        print("Rust flag standard: FAIL", *problems, REMEDY, sep="\n", file=sys.stderr)
        return 1
    print("Rust flag standard: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
