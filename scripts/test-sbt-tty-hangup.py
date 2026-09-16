#!/usr/bin/env python3
"""Prove sbt exits when its terminal goes away (TASK-232).

Two cases against a real pty, an isolated HOME and a fixture backlog:

1. pty EOF, no signal: sbt runs in its own session (no controlling tty, so
   the kernel sends no SIGHUP), the detail pane is focused, then the pty
   master is closed. sbt must exit 0 within 5s and leave a resume record.
2. SIGTERM while the reader spins, hangup watch disabled
   (SBT_NO_HANGUP_WATCH=1): sbt reads keys from one pty and draws to
   another. Closing the input pty's master makes crossterm spin forever on
   a zero-byte read while every draw still succeeds, so nothing but the
   event-reader thread keeps the main loop observing signals. sbt must
   stay up for 1.5s, then SIGTERM must end it within 2s with exit 0 and a
   resume record.

Usage: test-sbt-tty-hangup.py [path-to-sbt] [eof|sigterm]. Without a path
the script builds the workspace's debug `sbt` and uses that; without a case
name it runs both.
"""
import fcntl
import json
import os
import pty
import select
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCREEN_TIMEOUT = 15.0


def build_sbt() -> str:
    proc = subprocess.run(
        ["cargo", "build", "-p", "switchbard-tui", "--bin", "sbt", "--message-format=json"],
        cwd=REPO, check=True, capture_output=True, text=True,
    )
    for line in proc.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") == "compiler-artifact" and message.get("executable"):
            if message["target"]["name"] == "sbt":
                return message["executable"]
    raise SystemExit("cargo produced no sbt executable")


def wait_screen(master: int, needle: bytes) -> None:
    screen = b""
    deadline = time.monotonic() + SCREEN_TIMEOUT
    while time.monotonic() < deadline:
        ready, _, _ = select.select([master], [], [], 0.1)
        if ready:
            screen += os.read(master, 65536)
            if needle in screen:
                return
    raise AssertionError(f"never saw {needle!r}; last screen: {screen[-3000:]!r}")


def wait_exit(child: subprocess.Popen, seconds: float):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        status = child.poll()
        if status is not None:
            return status
        time.sleep(0.05)
    return None


def open_sized_pty():
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 120, 0, 0))
    return master, slave


def start(binary: str, repo: Path, env: dict, extra_env: dict, separate_input: bool):
    """Start sbt with the detail pane focused. Returns the child, the master
    of the pty it draws to, and the master of the pty it reads keys from
    (the same fd unless `separate_input`)."""
    screen_master, screen_slave = open_sized_pty()
    if separate_input:
        input_master, input_slave = open_sized_pty()
    else:
        input_master, input_slave = screen_master, screen_slave
    child = subprocess.Popen(
        [binary, "--repo", str(repo), "--fresh"],
        stdin=input_slave, stdout=screen_slave, stderr=screen_slave,
        env={**env, **extra_env}, start_new_session=True,
    )
    os.close(screen_slave)
    if separate_input:
        os.close(input_slave)
    try:
        wait_screen(screen_master, b"Tasks")
        os.write(input_master, b"\r")
        os.write(input_master, b"\r")
        wait_screen(screen_master, b"pane focused")
    except BaseException:
        child.kill()
        child.wait(timeout=5)
        os.close(screen_master)
        if separate_input:
            os.close(input_master)
        raise
    return child, screen_master, input_master


def resume_records(home: Path):
    return list((home / ".switchbard" / "views").glob("*.resume"))


def cpu_seconds(pid: int) -> float:
    out = subprocess.run(["ps", "-o", "cputime=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    if not out:
        return 0.0
    parts = [float(p) for p in out.split(":")]
    return parts[-1] + 60 * parts[-2] + (3600 * parts[-3] if len(parts) > 2 else 0)


def case_eof_without_signal(binary: str, repo: Path, env: dict, home: Path) -> None:
    child, master, _ = start(binary, repo, env, {}, separate_input=False)
    try:
        os.close(master)
        status = wait_exit(child, 5.0)
        assert status is not None, "sbt survived pty EOF for 5s (headless spin, TASK-232)"
        assert status == 0, f"sbt exited {status} after pty EOF, expected 0"
        assert resume_records(home), "no resume record written on hangup exit"
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
    print("pty EOF without a signal: exited 0 within 5s with a resume record")


def drain(master: int) -> None:
    """Keep acting as the screen while sbt shuts down and restores it."""
    try:
        while select.select([master], [], [], 0.05)[0]:
            if not os.read(master, 65536):
                break
    except OSError:
        pass


def case_sigterm_while_reader_spins(binary: str, repo: Path, env: dict, home: Path) -> None:
    for record in resume_records(home):
        record.unlink()
    child, screen, keys = start(binary, repo, env, {"SBT_NO_HANGUP_WATCH": "1"}, separate_input=True)
    try:
        os.close(keys)
        assert wait_exit(child, 1.5) is None, "sbt should stay up: its screen is alive, only input hit EOF"
        child.send_signal(signal.SIGTERM)
        deadline = time.monotonic() + 2.0
        status = None
        while time.monotonic() < deadline:
            status = child.poll()
            if status is not None:
                break
            drain(screen)
        assert status is not None, "SIGTERM ignored for 2s while the reader spins on EOF (main loop stuck)"
        assert status == 0, f"sbt exited {status} on SIGTERM, expected 0"
        assert resume_records(home), "no resume record written on SIGTERM exit"
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
        os.close(screen)
    print("SIGTERM while the reader spins on EOF (watch disabled): exited 0 within 2s with a resume record")


def main() -> None:
    binary = sys.argv[1] if len(sys.argv) > 1 else build_sbt()
    only = sys.argv[2] if len(sys.argv) > 2 else None
    if only not in (None, "eof", "sigterm"):
        raise SystemExit(f"unknown case {only!r}: expected eof or sigterm")
    with tempfile.TemporaryDirectory(prefix="sbt-tty-hangup-") as directory:
        root = Path(directory)
        copied = root / "sbt"
        shutil.copy2(binary, copied)
        repo = root / "repo"
        (repo / "backlog" / "tasks").mkdir(parents=True)
        (repo / "backlog" / "config.yml").write_text(
            "project_name: fixture\nstatuses: ['To Do', 'Done']\ntask_prefix: TASK\n"
        )
        (repo / "backlog" / "tasks" / "task-1 - Fixture task.md").write_text(
            "---\nid: task-1\ntitle: Fixture task\nstatus: To Do\npriority: medium\n"
            "labels: []\ncreated_date: '2026-09-15 12:00'\n---\n\n## Description\n\nFixture.\n"
        )
        home = root / "home"
        env = dict(os.environ, HOME=str(home), TERM="xterm-256color")
        env.pop("SBT_RESUME", None)
        env.pop("SBT_NO_HANGUP_WATCH", None)
        if only in (None, "eof"):
            case_eof_without_signal(str(copied), repo, env, home)
        if only in (None, "sigterm"):
            case_sigterm_while_reader_spins(str(copied), repo, env, home)
    print("sbt tty hangup: PASS")


if __name__ == "__main__":
    main()
