//! The terminal as the two things the main loop must never block on: a
//! source of key and mouse events, and a line that can be hung up.
//!
//! crossterm 0.28's unix event source loops on a zero-byte read forever
//! (`event/source/unix/mio.rs`: `Ok(0)` neither advances the parser nor
//! breaks out), so once the pty behind the tty is gone, `event::poll` and
//! `event::read` never return. When the main loop was their caller it could
//! no longer observe the shutdown flag SIGHUP and SIGTERM set, and sbt
//! became a headless process at 100% CPU that only SIGKILL ended
//! (TASK-232).
//!
//! One design rule fixes both symptoms: **the main loop waits only on things
//! it owns** - a channel and two flags - never on crossterm directly.
//!
//! - [`spawn_event_reader`] moves `event::read` onto its own thread and
//!   hands events over a channel; the loop waits on the channel with a
//!   timeout, so a signal is seen within one tick no matter what the reader
//!   is doing. A reader spinning on EOF costs one core for the few
//!   milliseconds between hangup and exit; it can no longer hold the loop.
//! - [`spawn_hangup_watch`] polls the tty and raises a flag on `POLLHUP`,
//!   for a hangup that arrives without SIGHUP: sbt started in its own
//!   session (no controlling tty, as an E2E harness does) gets no signal
//!   when the pty master closes. macOS reports `POLLHUP` on a pty only when
//!   `POLLIN` is also requested (with `events = 0` it reports nothing), so
//!   the watch asks for `POLLIN` and backs off while ordinary input is
//!   pending for the reader to drain.
//!
//! `SBT_NO_HANGUP_WATCH=1` disables the watch. It exists for the scripted
//! E2E check (`scripts/test-sbt-tty-hangup.py`) to prove the reader thread
//! alone keeps SIGTERM effective after EOF; it is not a user setting.

use crossterm::event::{self, Event};
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Environment variable that disables [`spawn_hangup_watch`]; test-only.
pub const NO_HANGUP_WATCH_ENV: &str = "SBT_NO_HANGUP_WATCH";

/// How long one `poll(2)` in the hangup watch blocks before re-checking
/// whether the process is already shutting down for another reason.
const HANGUP_POLL_MS: libc::c_int = 500;

/// Back-off while input is pending but not yet drained by the reader
/// thread: `POLLIN` stays set until crossterm reads, and the watch must not
/// spin in the meantime.
const INPUT_PENDING_BACKOFF: Duration = Duration::from_millis(50);

/// Start the thread that owns `crossterm::event::read` for the life of the
/// process. Events arrive on the returned channel; the sender is dropped
/// (and the channel disconnects) if crossterm ever returns an error.
pub fn spawn_event_reader() -> io::Result<Receiver<Event>> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("sbt-events".into())
        .spawn(move || {
            // Bounded by the process, not by a count: this thread lives
            // exactly as long as the terminal does, and the main loop's
            // exit ends it with the process.
            while let Ok(event) = event::read() {
                if tx.send(event).is_err() {
                    break;
                }
            }
        })?;
    Ok(rx)
}

/// Start the thread that raises `hung_up` when the terminal reports a
/// hangup. Returns `Ok(false)` without starting one when the watch is
/// disabled by [`NO_HANGUP_WATCH_ENV`] or when no tty can be found to
/// watch; the signal path still covers those cases.
pub fn spawn_hangup_watch(hung_up: Arc<AtomicBool>, stop: Arc<AtomicBool>) -> io::Result<bool> {
    if std::env::var_os(NO_HANGUP_WATCH_ENV).is_some_and(|v| !v.is_empty()) {
        return Ok(false);
    }
    let Some(tty) = watched_tty() else {
        return Ok(false);
    };
    thread::Builder::new()
        .name("sbt-hangup-watch".into())
        .spawn(move || watch(tty, &hung_up, &stop))?;
    Ok(true)
}

/// The same tty crossterm reads: stdin when it is a terminal, else
/// `/dev/tty`. Keeps the `/dev/tty` handle open for the watch's lifetime.
enum WatchedTty {
    Stdin,
    DevTty(std::fs::File),
}

impl WatchedTty {
    fn raw_fd(&self) -> RawFd {
        match self {
            Self::Stdin => io::stdin().as_raw_fd(),
            Self::DevTty(file) => file.as_raw_fd(),
        }
    }
}

fn watched_tty() -> Option<WatchedTty> {
    // SAFETY: `isatty` only inspects the descriptor; stdin's fd is valid for
    // the life of the process.
    if unsafe { libc::isatty(io::stdin().as_raw_fd()) } == 1 {
        return Some(WatchedTty::Stdin);
    }
    std::fs::File::open("/dev/tty").ok().map(WatchedTty::DevTty)
}

fn watch(tty: WatchedTty, hung_up: &AtomicBool, stop: &AtomicBool) {
    let fd = tty.raw_fd();
    // Bounded by the process: ends on hangup, on an unrecoverable poll
    // error, or as soon as the main loop is stopping for any other reason.
    while !stop.load(Ordering::Relaxed) && !hung_up.load(Ordering::Relaxed) {
        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `pollfd` is a valid, initialised array of one entry that
        // outlives the call; `poll` writes only its `revents` field.
        let ready = unsafe { libc::poll(&mut pollfd, 1, HANGUP_POLL_MS) };
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        if ready == 0 {
            continue;
        }
        if pollfd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
            hung_up.store(true, Ordering::SeqCst);
            return;
        }
        // Only `POLLIN`: a keystroke is waiting for the reader thread.
        thread::sleep(INPUT_PENDING_BACKOFF);
    }
}
