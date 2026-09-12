"""Shared utilities for Claude Code hooks.

Eliminates duplicated boilerplate across PreToolUse, PostToolUse,
and PermissionRequest hook scripts.
"""

from __future__ import annotations

import json
import os
import re
import shlex
import sys
from pathlib import Path

# ── Project path constants ───────────────────────────────────────

PROJECT_SUBDIRS: tuple[str, ...] = (
    "src/",
    "frontend/",
    "backend/",
    "tests/",
    "test/",
    "lib/",
    "docs/",
    "scripts/",
    "db/",
    ".claude/",
    "e2e/",
    "public/",
    "backlog/",
)
"""Unified list of safe project subdirectories - single source of truth.

Used by dangerous-command-guard (checkout detection). Add new directories here,
not in individual hooks.
"""


def project_dir() -> str:
    """Return CLAUDE_PROJECT_DIR or cwd as fallback."""
    return os.environ.get("CLAUDE_PROJECT_DIR", os.getcwd())


def is_safe_project_path(file_path: str) -> bool:
    """Check if a file path resolves to within the project directory.

    Uses pathlib resolve() to canonicalize symlinks and .. traversal,
    preventing path traversal attacks like src/../../../etc/passwd.
    """
    proj = Path(project_dir()).resolve()
    target = Path(file_path) if Path(file_path).is_absolute() else proj / file_path
    target = target.resolve()
    return target == proj or proj in target.parents


# ── Hook input/output ───────────────────────────────────────────


def read_hook_input() -> dict:
    """Read and parse JSON from stdin. Exits on parse error."""
    try:
        return json.load(sys.stdin)
    except json.JSONDecodeError as e:
        hook_log("unknown", f"Invalid JSON input: {e}")
        sys.exit(1)


_PRETOOLUSE_DECISIONS = {"deny", "ask"}
_PERMREQUEST_BEHAVIORS = {"allow", "deny", "prompt"}


def pretooluse_decision(decision: str, reason: str) -> dict:
    """Build a PreToolUse hookSpecificOutput dict.

    Args:
        decision: "deny" or "ask"
        reason: Human-readable explanation shown to the user

    Raises:
        ValueError: If decision is not "deny" or "ask"
    """
    if decision not in _PRETOOLUSE_DECISIONS:
        raise ValueError(
            f"Invalid pretooluse decision {decision!r}, "
            f"expected one of {_PRETOOLUSE_DECISIONS}"
        )
    return {
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": decision,
            "permissionDecisionReason": reason,
        }
    }


def permrequest_decision(behavior: str, reason: str | None = None) -> dict:
    """Build a PermissionRequest hookSpecificOutput dict.

    Args:
        behavior: "allow", "deny", or "prompt"
        reason: Optional explanation (not included for "prompt")

    Raises:
        ValueError: If behavior is not "allow", "deny", or "prompt"
    """
    if behavior not in _PERMREQUEST_BEHAVIORS:
        raise ValueError(
            f"Invalid permrequest behavior {behavior!r}, "
            f"expected one of {_PERMREQUEST_BEHAVIORS}"
        )
    result = {
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": {"behavior": behavior},
        }
    }
    if reason and behavior != "prompt":
        result["hookSpecificOutput"]["metadata"] = {"reason": reason}
    return result


def emit_and_exit(result: dict | None) -> None:
    """Print JSON result (if any) to stdout and exit 0."""
    if result is not None:
        print(json.dumps(result))
    sys.exit(0)


def hook_log(hook_name: str, msg: str) -> None:
    """Log to stderr with consistent prefix."""
    print(f"[{hook_name}] {msg}", file=sys.stderr)


# ── Tool input extraction ──────────────────────────────────────

FILE_WRITING_TOOLS = frozenset({"Write", "Edit", "Bash"})
"""Tools that can modify files - shared across guard hooks."""


def extract_target_file(tool_input: dict) -> str:
    """Extract target file path from Write, Edit, or Bash _simulatedSedEdit.

    Returns the file path string, or "" if none found.
    """
    file_path = tool_input.get("file_path", "")
    if file_path:
        return file_path

    sed_edit = tool_input.get("_simulatedSedEdit")
    if isinstance(sed_edit, dict):
        return sed_edit.get("filePath", "")

    return ""


# ── Bash write-target parsing ──────────────────────────────────
#
# Under `permissions.defaultMode: auto` most file writes are Bash commands
# (`sed -i`, a redirect, a heredoc), not Write/Edit calls. Guard hooks that
# only read `file_path` are blind to them, so this parser names the paths a
# Bash command writes. It answers writes only: a `cat`, `grep`, or `sed -n`
# yields nothing, so read gating can never leak out of it.
#
# Known gaps, by design - a write hidden inside an interpreter body
# (`python3 - <<PY` calling `Path(...).write_text(...)`), a path built from an
# unexpanded shell variable, and a build tool that writes files as a side
# effect are not visible in the command text and are not guessed at.

MAX_COMMAND_LINES = 400
MAX_COMMAND_TOKENS = 2000

_HEREDOC_START = re.compile(
    r"<<-?\s*(?P<quote>['\"]?)(?P<tag>[A-Za-z_][A-Za-z0-9_]*)(?P=quote)"
)
_COMMAND_SEPARATORS = frozenset({"&&", "||", "|", "|&", ";", "&", ";;"})
_UNFINISHED_LINE_ENDINGS = ("\\", "&&", "||", "|", "&", ";")
_WRITE_REDIRECTS = frozenset({">", ">>", "&>", "&>>", ">|"})
_OTHER_REDIRECTS = frozenset({"<", "<<", "<<<", "<&", "<>", ">&", ">>&"})
_DISCARDED_TARGETS = frozenset(
    {"/dev/null", "/dev/stdout", "/dev/stderr", "/dev/tty", "/dev/fd/1", "/dev/fd/2"}
)
_DESTINATION_COMMANDS = frozenset({"cp", "mv", "install", "rsync", "ln"})
_UNRESOLVABLE = ("$", "`")


def _strip_heredoc_bodies(command: str) -> str:
    """Drop heredoc bodies, keeping the command lines that introduce them.

    A heredoc body is arbitrary text - often the source being written. Left in,
    it tokenizes as commands and its content is mistaken for redirect targets.
    """
    lines = command.split("\n")[:MAX_COMMAND_LINES]
    kept: list[str] = []
    index = 0
    while index < len(lines):
        line = lines[index]
        kept.append(line)
        index += 1
        match = _HEREDOC_START.search(line)
        if match is None or "<<<" in line:
            continue
        while index < len(lines) and lines[index].strip() != match.group("tag"):
            index += 1
        index += 1  # skip the terminator line itself
    return "\n".join(kept)


def _join_command_lines(command: str) -> str:
    """Join lines into one string, marking each line break that ends a command.

    The tokenizer treats a newline as plain whitespace, which would run
    `sed -i ... file` on line two into whatever preceded it and hide the
    command name. An explicit `;` keeps each line its own command.
    """
    joined: list[str] = []
    for line in command.split("\n")[:MAX_COMMAND_LINES]:
        stripped = line.rstrip()
        joined.append(line)
        if stripped and not stripped.endswith(_UNFINISHED_LINE_ENDINGS):
            joined.append(";")
    return " ".join(joined)


def _tokenize_command(command: str) -> list[str] | None:
    """Split a command into words and redirect operators, or None if unparseable."""
    lexer = shlex.shlex(command, posix=True, punctuation_chars=True)
    lexer.whitespace_split = True
    try:
        return list(lexer)[:MAX_COMMAND_TOKENS]
    except ValueError:
        return None


def _split_segments(tokens: list[str]) -> list[list[str]]:
    """Group tokens into individual commands, split on shell separators."""
    segments: list[list[str]] = []
    current: list[str] = []
    for token in tokens:
        if token in _COMMAND_SEPARATORS:
            if current:
                segments.append(current)
            current = []
        else:
            current.append(token)
    if current:
        segments.append(current)
    return segments


def _command_name(argv: list[str]) -> str:
    return argv[0].rsplit("/", 1)[-1] if argv else ""


def _operands(argv: list[str]) -> list[str]:
    """Non-flag, non-empty arguments - the file operands of a simple command."""
    return [token for token in argv[1:] if token and not token.startswith("-")]


def _partition_redirections(argv: list[str]) -> tuple[list[str], list[str]]:
    """Split a command into its words and the paths its redirections write.

    Redirections can appear anywhere, so they have to come out before the words
    are read as file operands - otherwise `tee out.py < in.py` reports `in.py`
    as a second destination. `2>&1` duplicates a descriptor and writes nothing.
    """
    words: list[str] = []
    targets: list[str] = []
    token_is_a_target = False
    token_is_a_source = False
    for token in argv:
        if token_is_a_target:
            targets.append(token)
            token_is_a_target = False
        elif token_is_a_source:
            token_is_a_source = False
        elif token in _WRITE_REDIRECTS:
            token_is_a_target = True
        elif token in _OTHER_REDIRECTS:
            token_is_a_source = True
        else:
            words.append(token)
    return words, targets


def _sed_targets(argv: list[str]) -> list[str]:
    """Files rewritten by `sed -i`. Without `-i`, sed only reads."""
    if _command_name(argv) != "sed":
        return []

    in_place = False
    script_is_a_flag = False
    operands: list[str] = []
    skip_next = False
    for token in argv[1:]:
        if skip_next:
            skip_next = False
        elif token.startswith("--"):
            in_place = in_place or token.startswith("--in-place")
            skip_next = token in ("--expression", "--file")
            script_is_a_flag = script_is_a_flag or skip_next
        elif token.startswith("-") and len(token) > 1:
            in_place = in_place or "i" in token[1:]
            skip_next = token[-1] in ("e", "f")
            script_is_a_flag = script_is_a_flag or skip_next
        elif token:
            operands.append(token)

    if not in_place:
        return []
    # `sed -i 's/a/b/' file` - the script is the first operand unless -e/-f gave
    # it. (BSD `sed -i '' ...` passes an empty suffix, already dropped above.)
    return operands if script_is_a_flag else operands[1:]


def _destination_targets(argv: list[str]) -> list[str]:
    """The destination of `cp`, `mv`, `install`, `rsync`, `ln`."""
    if _command_name(argv) not in _DESTINATION_COMMANDS:
        return []
    operands = _operands(argv)
    return operands[-1:] if len(operands) >= 2 else []


def _named_file_targets(argv: list[str]) -> list[str]:
    """Files named directly by a writing command (`tee`, `touch`)."""
    if _command_name(argv) not in ("tee", "touch"):
        return []
    return _operands(argv)


def _segment_targets(argv: list[str]) -> tuple[list[str], list[str]]:
    """The paths one command writes, and its words (for `cd` tracking)."""
    words, redirected = _partition_redirections(argv)
    return words, [
        *redirected,
        *_sed_targets(words),
        *_destination_targets(words),
        *_named_file_targets(words),
    ]


def _cd_destination(argv: list[str]) -> str | None:
    """The directory a `cd` moves to, or None when it is not a plain `cd`."""
    if _command_name(argv) != "cd":
        return None
    operands = _operands(argv)
    return operands[0] if operands else None


def _resolve(path: str, cwd: str | None) -> str | None:
    """Absolutize `path` against `cwd`, or None when it cannot be resolved."""
    if not path or any(marker in path for marker in _UNRESOLVABLE):
        return None
    expanded = os.path.expanduser(path)
    if os.path.isabs(expanded):
        return os.path.normpath(expanded)
    if cwd is None:
        return None
    return os.path.normpath(os.path.join(cwd, expanded))


def bash_write_targets(command: str, cwd: str | None = None) -> tuple[str, ...]:
    """Absolute paths a Bash command writes to, in the order it writes them.

    Returns an empty tuple for a command that only reads, for one whose targets
    cannot be resolved (an unexpanded `$VAR`, a relative path after a `cd` to an
    unknown directory), and for one that will not tokenize.
    """
    tokens = _tokenize_command(_join_command_lines(_strip_heredoc_bodies(command)))
    if tokens is None:
        return ()

    working_dir = cwd if cwd is not None else project_dir()
    targets: list[str] = []
    for argv in _split_segments(tokens):
        words, segment_targets = _segment_targets(argv)
        destination = _cd_destination(words)
        if destination is not None:
            working_dir = _resolve(destination, working_dir)
            continue
        for target in segment_targets:
            if target in _DISCARDED_TARGETS:
                continue
            resolved = _resolve(target, working_dir)
            if resolved is not None and resolved not in targets:
                targets.append(resolved)
    return tuple(targets)


def extract_write_targets(tool_input: dict, cwd: str | None = None) -> tuple[str, ...]:
    """Every file path a tool call will write, for Write, Edit, or Bash.

    The single authority on "what does this tool call write" - guard hooks read
    it instead of `file_path`, so registering one for Bash covers the write
    forms auto mode actually uses.

    `cwd` is the shell's working directory, which the hook input carries; a
    relative path in a Bash command resolves against it. It falls back to the
    project dir when the caller has none.
    """
    named = extract_target_file(tool_input)
    if named:
        return (named,)
    return bash_write_targets(tool_input.get("command", ""), cwd)


def extract_written_content(tool_input: dict) -> str:
    """Extract content being written from Write, Edit, or Bash _simulatedSedEdit.

    - Write: returns full file content
    - Edit: returns the new_string replacement (partial)
    - Bash _simulatedSedEdit: returns full file content after edit
    """
    content = tool_input.get("content", "")
    if content:
        return content

    new_string = tool_input.get("new_string", "")
    if new_string:
        return new_string

    sed_edit = tool_input.get("_simulatedSedEdit")
    if isinstance(sed_edit, dict):
        return sed_edit.get("newContent", "")

    return ""


def is_under_project_path(file_path: str, relative_dir: str) -> bool:
    """Check if file_path is under a project-relative directory.

    Uses canonical path resolution to prevent traversal.
    """
    if not file_path:
        return False
    proj = Path(project_dir()).resolve()
    target = Path(file_path) if Path(file_path).is_absolute() else proj / file_path
    target = target.resolve()
    expected_dir = (proj / relative_dir).resolve()
    return target == expected_dir or expected_dir in target.parents


# ── Backend health check ───────────────────────────────────────


def is_backend_running(port: int = 8000, timeout: int = 2) -> bool:
    """Check if backend is responding on localhost.

    Queries the OpenAPI endpoint to verify the backend is up and serving
    valid JSON. Returns True only on HTTP 200 with a parseable JSON body.

    Args:
        port: The port to check (default 8000)
        timeout: Request timeout in seconds (default 2)

    Returns:
        True if backend responds with 200 and valid JSON, False otherwise
    """
    import http.client
    import urllib.error
    import urllib.request

    url = f"http://localhost:{port}/openapi.json"
    try:
        req = urllib.request.Request(url)
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            if resp.status != 200:
                return False
            body = resp.read()
            json.loads(body)
            return True
    except (
        urllib.error.URLError,
        http.client.HTTPException,
        json.JSONDecodeError,
        TimeoutError,
        OSError,
    ) as e:
        hook_log("hook_utils", f"Backend health check failed: {e}")
        return False
