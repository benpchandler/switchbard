//! Filesystem resolution of a repository's Git common directory.
//!
//! Equivalent to `git -C <path> rev-parse --path-format=absolute
//! --git-common-dir` for the layouts Switchbard meets (plain checkouts, linked
//! worktrees, submodule gitfiles, bare repositories, and paths inside a Git
//! directory), without starting a process. Storage identity and locking resolve
//! this on every write, and a `git` launch per resolution is what made macOS
//! `syspolicyd` scan tens of thousands of processes per test run.
//!
//! Deliberate differences from Git: `GIT_*` discovery variables are ignored
//! (matching [`crate::git_env::git_cmd`], which strips them), and
//! `safe.directory`, `discovery.bare`, and `GIT_CEILING_DIRECTORIES` are not
//! consulted. Discovery stops at a filesystem boundary, as Git does by default.

use std::path::{Path, PathBuf};

/// Upper bound on ancestor directories inspected; deeper than any real path.
const MAX_DISCOVERY_DEPTH: usize = 256;
/// Upper bound on the bytes read from a `.git` file or `commondir` file.
const MAX_POINTER_BYTES: u64 = 4096;

/// The canonical common Git directory that `path` belongs to, or `None` when
/// `path` is not inside a Git repository (or cannot be read).
pub fn resolve(path: &Path) -> Option<PathBuf> {
    let start = path.canonicalize().ok()?;
    let start = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start
    };
    let git_dir = discover(&start)?;
    let common = common_of(&git_dir)?;
    debug_assert!(
        common.is_absolute(),
        "invariant: canonical path is absolute"
    );
    Some(common)
}

/// Walks from `start` toward the root, returning the first Git directory found.
fn discover(start: &Path) -> Option<PathBuf> {
    let device = device_of(start)?;
    for dir in start.ancestors().take(MAX_DISCOVERY_DEPTH) {
        if device_of(dir) != Some(device) {
            return None;
        }
        let dot_git = dir.join(".git");
        if dot_git.is_file() {
            // Git refuses an invalid gitfile rather than continuing upward.
            return read_gitfile(&dot_git, dir).filter(|target| is_git_dir(target));
        }
        if dot_git.is_dir() && is_git_dir(&dot_git) {
            return Some(dot_git);
        }
        if is_git_dir(dir) {
            return Some(dir.to_path_buf());
        }
    }
    None
}

/// Follows a `commondir` pointer when present; otherwise the directory is its own common dir.
fn common_of(git_dir: &Path) -> Option<PathBuf> {
    let pointer = git_dir.join("commondir");
    let common = if pointer.is_file() {
        let target = PathBuf::from(read_small(&pointer)?.trim_end_matches(['\n', '\r']));
        git_dir.join(target)
    } else {
        git_dir.to_path_buf()
    };
    common.canonicalize().ok()
}

/// Parses a `gitdir: <path>` gitfile; relative targets resolve against `base`.
fn read_gitfile(file: &Path, base: &Path) -> Option<PathBuf> {
    let text = read_small(file)?;
    let target = text.lines().next()?.strip_prefix("gitdir: ")?.trim_end();
    if target.is_empty() {
        return None;
    }
    Some(base.join(target))
}

/// Git's own test: a HEAD, plus object and ref stores (directly or via `commondir`).
fn is_git_dir(dir: &Path) -> bool {
    if !dir.join("HEAD").exists() {
        return false;
    }
    if dir.join("commondir").is_file() {
        return true;
    }
    dir.join("objects").is_dir() && dir.join("refs").is_dir()
}

fn read_small(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_POINTER_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

#[cfg(unix)]
fn device_of(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|metadata| metadata.dev())
}

#[cfg(not(unix))]
fn device_of(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|_| 0)
}

#[cfg(test)]
#[path = "git_common_dir_tests.rs"]
mod tests;
