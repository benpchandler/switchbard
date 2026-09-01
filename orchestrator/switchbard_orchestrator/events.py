"""Append-only JSONL run events - the live-progress feed (TASK-90 schema,
emitted from TASK-89 forward so the GUI half has real data to render).

One file per run, `<log stem>.events.jsonl` in the dispatch log dir, next
to the run's `.log`. Schema per line:

    {"ts_ms": int, "task_id": str, "event": str, ...detail}

Events: run_start, node_enter, node_exit (with "ok"), heartbeat (with
"log_bytes"), interrupt (with "remainder": [str]), release (with "outcome"
and optionally "pr_url"), run_end. Consumers must tolerate unknown events
and unknown fields; a missing or malformed file degrades to no live
progress, never to an error.
"""

from __future__ import annotations

import json
import threading
import time
from pathlib import Path


class Emitter:
    def __init__(self, path: Path, task_id: str) -> None:
        self.path = path
        self.task_id = task_id
        path.parent.mkdir(parents=True, exist_ok=True)

    def emit(self, event: str, **detail: object) -> None:
        line = {
            "ts_ms": int(time.time() * 1000),
            "task_id": self.task_id,
            "event": event,
            **detail,
        }
        # Append-only, one json object per line; a torn final line is the
        # reader's problem to skip (and every reader must).
        with open(self.path, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(line, ensure_ascii=False) + "\n")


class Heartbeat:
    """Emit a heartbeat with the agent log's size while a blocking step
    runs, so 'working' and 'hung' are distinguishable from outside."""

    def __init__(self, emitter: Emitter, log_path: Path, period_s: float = 15.0) -> None:
        self._emitter = emitter
        self._log_path = log_path
        self._period = period_s
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def __enter__(self) -> "Heartbeat":
        self._thread = threading.Thread(target=self._loop, daemon=True)
        self._thread.start()
        return self

    def __exit__(self, *_exc: object) -> None:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=2.0)

    def _loop(self) -> None:
        while not self._stop.wait(self._period):
            try:
                size = self._log_path.stat().st_size
            except OSError:
                size = 0
            self._emitter.emit("heartbeat", log_bytes=size)
