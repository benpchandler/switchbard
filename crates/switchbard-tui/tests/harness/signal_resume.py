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


class TerminalScreen:
    """Small stateful decoder for the crossterm sequences this harness sees."""

    def __init__(self, rows=24, columns=100):
        self.rows = rows
        self.columns = columns
        self.cells = [[" "] * columns for _ in range(rows)]
        self.row = 0
        self.column = 0
        self.pending = b""

    def feed(self, data):
        data = self.pending + data
        self.pending = b""
        index = 0
        while index < len(data):
            byte = data[index]
            if byte == 0x1b:
                end = index + 1
                if end == len(data):
                    self.pending = data[index:]
                    break
                if data[end] != ord("["):
                    index += 2
                    continue
                end += 1
                while end < len(data) and not (0x40 <= data[end] <= 0x7e):
                    end += 1
                if end == len(data):
                    self.pending = data[index:]
                    break
                self._csi(data[index + 2:end], data[end])
                index = end + 1
                continue
            if byte == 0x0d:
                self.column = 0
            elif byte == 0x0a:
                self.row = min(self.rows - 1, self.row + 1)
            elif byte == 0x08:
                self.column = max(0, self.column - 1)
            elif 0x20 <= byte <= 0x7e:
                self.cells[self.row][self.column] = chr(byte)
                self.column = min(self.columns - 1, self.column + 1)
            index += 1

    def _csi(self, body, final):
        if final in (ord("H"), ord("f")):
            parts = body.lstrip(b"?").split(b";")
            row = int(parts[0] or b"1") - 1
            column = int(parts[1] or b"1") - 1 if len(parts) > 1 else 0
            self.row = max(0, min(self.rows - 1, row))
            self.column = max(0, min(self.columns - 1, column))
        elif final in (ord("J"), ord("K")) and body in (b"", b"0", b"1", b"2", b"3"):
            mode = int(body or b"0")
            if final == ord("K"):
                self._erase_line(mode)
            else:
                self._erase_display(mode)

    def _erase_line(self, mode):
        if mode not in (0, 1, 2):
            return
        start = 0 if mode in (1, 2) else self.column
        end = self.column + 1 if mode == 1 else self.columns
        self.cells[self.row][start:end] = [" "] * (end - start)

    def _erase_display(self, mode):
        # 3J clears saved scrollback, which this visible-screen observer does not retain.
        if mode == 3:
            return
        cursor = self.row * self.columns + self.column
        start = 0 if mode in (1, 2) else cursor
        end = cursor + 1 if mode == 1 else self.rows * self.columns
        for position in range(start, end):
            row, column = divmod(position, self.columns)
            self.cells[row][column] = " "

    def contains(self, text):
        return any(text in "".join(row) for row in self.cells)


def screen_decoder_self_test():
    screen = TerminalScreen(rows=2, columns=12)
    screen.feed(b"\x1b[1;1Hsig")
    screen.feed(b"\x1b[2;1Hnal0")
    assert not screen.contains("signal0")
    screen.feed(b"\x1b[1;4Hnal0")
    assert screen.contains("signal0")
    screen.feed(b"\x1b[1;12H\x1b[2K")
    assert not screen.contains("signal0")
    assert screen.cells[0] == [" "] * 12
    screen_erase_self_test()


def screen_erase_self_test():
    expected = {
        ("K", ""): ["ABCDEF", "GH    ", "MNOPQR"],
        ("K", "0"): ["ABCDEF", "GH    ", "MNOPQR"],
        ("K", "1"): ["ABCDEF", "   JKL", "MNOPQR"],
        ("K", "2"): ["ABCDEF", "      ", "MNOPQR"],
        ("K", "3"): ["ABCDEF", "GHIJKL", "MNOPQR"],
        ("J", ""): ["ABCDEF", "GH    ", "      "],
        ("J", "0"): ["ABCDEF", "GH    ", "      "],
        ("J", "1"): ["      ", "   JKL", "MNOPQR"],
        ("J", "2"): ["      ", "      ", "      "],
        ("J", "3"): ["ABCDEF", "GHIJKL", "MNOPQR"],
    }
    for (command, mode), rows in expected.items():
        screen = TerminalScreen(rows=3, columns=6)
        screen.feed(b"\x1b[1;1HABCDEF\x1b[2;1HGHIJKL\x1b[3;1HMNOPQR")
        screen.feed(b"\x1b[2;3")
        assert screen.pending == b"\x1b[2;3"
        screen.feed(b"H\x1b[")
        assert screen.pending == b"\x1b["
        screen.feed((mode + command).encode())
        assert screen.pending == b""
        assert screen.cells == [list(row) for row in rows], (command, mode, screen.cells)
        assert (screen.row, screen.column) == (1, 2)


def wait_screen(fd, text, resize_width=None):
    if resize_width is not None:
        fcntl.ioctl(
            fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, resize_width, 0, 0)
        )
    screen = TerminalScreen(columns=resize_width or 100)
    started = time.monotonic()
    deadline = time.monotonic() + 8
    repainted = False
    while time.monotonic() < deadline:
        ready, _, _ = select.select([fd], [], [], 0.1)
        if ready:
            screen.feed(os.read(fd, 65536))
            if screen.contains(text.decode() if isinstance(text, bytes) else text):
                return
        if resize_width is None and not repainted and time.monotonic() - started >= 0.5:
            # The TUI paints cells incrementally.  A cursor move can split a
            # label across the raw PTY stream, so request the terminal's
            # normal resize repaint before treating the screen as absent.
            fcntl.ioctl(fd, termios.TIOCSWINSZ,
                        struct.pack("HHHH", 24, 99, 0, 0))
            repainted = True
    raise AssertionError(f"missing {text!r}: {screen.cells[-4:]!r}")


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
        wait_screen(master, b"timercheckpoint", resize_width=99)
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
    screen_decoder_self_test()
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
