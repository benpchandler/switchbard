"""Exercise real sbt signals using isolated home, repo, and terminal sessions."""
import json
import fcntl
import os
from pathlib import Path
import pty
import select
import shutil
import signal
import subprocess
import struct
import sys
import tempfile
import termios
import time


def wait_screen(fd, text):
    screen = b""
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        ready, _, _ = select.select([fd], [], [], 0.1)
        if ready:
            screen += os.read(fd, 65536)
            if text in screen:
                return
    raise AssertionError(f"missing {text!r}: {screen[-4000:]!r}")


def wait_exit(child, master):
    """Keep acting as a terminal while the app flushes and restores its screen."""
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        status = child.poll()
        if status is not None:
            return status
        ready, _, _ = select.select([master], [], [], 0.05)
        if ready:
            try:
                os.read(master, 65536)
            except OSError as error:
                if error.errno != 5:  # A closed PTY reports EIO on some platforms.
                    raise
    raise AssertionError("signal quit did not finish within 5 seconds")


def run_session(binary, repo, env, args, expected, ending, edit=None):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 0, 0))
    child = subprocess.Popen(
        [binary, *args], cwd=repo, env=env, stdin=slave, stdout=slave,
        stderr=slave, start_new_session=True,
    )
    os.close(slave)
    try:
        wait_screen(master, expected)
        if edit:
            os.write(master, b"/" + edit.encode() + b"\r")
            wait_screen(master, edit.encode())
        child.send_signal(ending)
        assert wait_exit(child, master) == 0, "signal quit failed"
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
        os.close(master)


def timer_checkpoint_then_forced_exit(binary, repo, env, views_dir):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 0, 0))
    child = subprocess.Popen(
        [binary, "--fresh"], cwd=repo, env=env, stdin=slave, stdout=slave,
        stderr=slave, start_new_session=True,
    )
    os.close(slave)
    started = time.monotonic()
    try:
        wait_screen(master, b"Tasks")
        os.write(master, b"/timercheckpoint\r")
        wait_screen(master, b"timercheckpoint")
        deadline = started + 40
        durable = False
        while time.monotonic() < deadline:
            assert child.poll() is None, "process exited before live checkpoint"
            ready, _, _ = select.select([master], [], [], 0.1)
            if ready:
                os.read(master, 65536)
            records = list(views_dir.glob("*.resume"))
            histories = list(views_dir.glob("*.history.json"))
            if len(records) != 1 or len(histories) != 1:
                continue
            resume = json.loads(records[0].read_text().split("=", 1)[1])
            history = json.loads(histories[0].read_text())
            assert history["version"] == 1, history
            durable = "timercheckpoint" in resume["task_view"] and any(
                entry["page"] == "tasks" and "timercheckpoint" in entry["lua"]
                for entry in history["entries"]
            )
            if durable:
                break
        assert durable, "timer did not persist both resume and history within 40s"
        assert time.monotonic() - started >= 29, "checkpoint bypassed the timer"
        child.kill()
        assert child.wait(timeout=5) == -signal.SIGKILL
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
        os.close(master)
    run_session(binary, repo, env, [], b"timercheckpoint", signal.SIGTERM)
    print("30-second timer: live resume+history persisted; SIGKILL cold restore passed")


with tempfile.TemporaryDirectory(prefix="sbt-signal-resume-") as directory:
    root = Path(directory)
    binary = str(root / "sbt")
    shutil.copy2(sys.argv[1], binary)
    repo = root / "repo"
    (repo / "backlog" / "tasks").mkdir(parents=True)
    (repo / "backlog" / "config.yml").write_text(
        "project_name: fixture\nstatuses: ['To Do', 'Done']\ntask_prefix: TASK\n"
    )
    env = dict(os.environ, HOME=str(root / "home"), TERM="xterm-256color")
    env.pop("SBT_RESUME", None)
    for index, ending in enumerate((signal.SIGHUP, signal.SIGTERM, signal.SIGINT)):
        value = f"signal{index}"
        run_session(binary, repo, env, ["--repo", ".", "--fresh"],
                    b"Tasks", ending, value)
        records = list((root / "home" / ".switchbard" / "views").glob("*.resume"))
        assert len(records) == 1, records
        record = json.loads(records[0].read_text().split("=", 1)[1])
        assert value in record["task_view"], record
        run_session(binary, repo, env, [], value.encode(), signal.SIGTERM)
        run_session(binary, repo, env, ["--fresh"], b"Tasks", signal.SIGTERM)
        fresh_record = json.loads(records[0].read_text().split("=", 1)[1])
        assert value not in fresh_record["task_view"], "--fresh retained last filter"
    assert not list((root / "home" / ".switchbard" / "views").glob("*.lua"))
    print("SIGHUP, SIGTERM, SIGINT: durable quit and cold restore; relative/absolute repo identity")
    timer_checkpoint_then_forced_exit(
        binary, repo, env, root / "home" / ".switchbard" / "views"
    )
