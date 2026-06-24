//! Hardened constructor for `git` subprocesses.
//!
//! Switchbard always builds git commands via [`git_cmd`] instead of
//! `Command::new("git")`. The constructor strips the per-invocation `GIT_*`
//! environment variables git exports into hook processes (notably an absolute
//! `GIT_DIR` from a linked worktree). Left set, those override the `-C <path>`
//! we pass and silently redirect commands — including the temp-repo git calls
//! in the test suite — at the real repository, corrupting its `.git/config`.
//! Clearing them makes every invocation discover its repo from the directory we
//! actually gave it. In normal runs none of these vars are set, so this is a
//! no-op; the guarantee matters under an inherited git environment.

use std::process::Command;

const LEAKABLE_GIT_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_INDEX_FILE",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_NAMESPACE",
];

/// A `git` [`Command`] with inherited `GIT_*` discovery overrides scrubbed.
/// Use everywhere instead of `Command::new("git")`.
pub fn git_cmd() -> Command {
    let mut cmd = Command::new("git");
    for var in LEAKABLE_GIT_VARS {
        cmd.env_remove(var);
    }
    cmd
}
