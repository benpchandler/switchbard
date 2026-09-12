"""Run the PreToolUse hooks that ``.claude/settings.json`` actually registers.

Loading a hook with :mod:`hook_loader` and calling its pure functions proves the
hook's *logic*. It does not prove the hook ever receives the event: a hook whose
matcher omits a tool is silent no matter how correct its logic is. This module
closes that gap by resolving the matcher the same way Claude Code does and then
running each matching hook as a subprocess against a real payload.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SETTINGS = ROOT / ".claude" / "settings.json"

MATCHER_TIMEOUT_SECONDS = 30


def registered_hook_commands(tool_name: str) -> list[str]:
    """Hook commands registered for ``tool_name`` on PreToolUse, in run order."""
    settings = json.loads(SETTINGS.read_text())
    commands: list[str] = []
    for group in settings["hooks"]["PreToolUse"]:
        if re.fullmatch(group["matcher"], tool_name):
            commands.extend(hook["command"] for hook in group["hooks"])
    return commands


def is_registered(hook_name: str, tool_name: str) -> bool:
    """True when ``<hook_name>.py`` runs for ``tool_name`` on PreToolUse."""
    suffix = f"/.claude/hooks/{hook_name}.py"
    return any(cmd.endswith(suffix) for cmd in registered_hook_commands(tool_name))


def run_hook(
    hook_name: str,
    tool_name: str,
    tool_input: dict,
    *,
    env: dict[str, str] | None = None,
    project_dir: Path | str = ROOT,
) -> dict | None:
    """Run one hook as Claude Code would; return its decision, or None if silent.

    Returns None both when the hook is not registered for ``tool_name`` and when
    it is registered but stays quiet - the two ways a write can slip past it.
    """
    if not is_registered(hook_name, tool_name):
        return None

    hook_env = {**os.environ, "CLAUDE_PROJECT_DIR": str(project_dir)}
    hook_env.pop("SWITCHBARD_ALLOW_MAIN_EDIT", None)
    if env:
        hook_env.update(env)

    completed = subprocess.run(
        [sys.executable, str(ROOT / ".claude" / "hooks" / f"{hook_name}.py")],
        input=json.dumps(
            {
                "cwd": str(project_dir),
                "hook_event_name": "PreToolUse",
                "tool_name": tool_name,
                "tool_input": tool_input,
            }
        ),
        env=hook_env,
        cwd=str(project_dir),
        capture_output=True,
        text=True,
        check=True,
        timeout=MATCHER_TIMEOUT_SECONDS,
    )
    stdout = completed.stdout.strip()
    return json.loads(stdout) if stdout else None


def decision_of(result: dict | None) -> str | None:
    """The permissionDecision in a hook result, or None when it stayed silent."""
    if result is None:
        return None
    return result["hookSpecificOutput"]["permissionDecision"]
