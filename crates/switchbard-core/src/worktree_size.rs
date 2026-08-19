//! On-disk size probe for a worktree.
//!
//! Shells out to `du -sk`, which macOS's BSD `du` and Linux's GNU `du` both
//! support identically (`-s` summarize, `-k` 1024-byte blocks) — no
//! `#[cfg(target_os = …)]` branch needed, keeping this in step with the
//! repo's cross-platform-first invariant (CLAUDE.md).
//!
//! Deliberately **not** wired into any periodic cadence here — `du` over a
//! real worktree (node_modules, target/, build artifacts) measured ~0.8-1.5s
//! per call on a real 138-worktree machine (see `workers.rs`'s cadence-policy
//! doc and `examples/scan_cadence_audit.rs`), an order of magnitude more
//! expensive than any other per-worktree probe in this crate. Like every
//! other probe here, this function itself is a pure, uncached `(path) ->
//! Option<u64>` call; the GUI layer owns caching + a lazy-refresh cadence
//! (`switchbard_gui::workers::spawn_size`), the same split the git probes use
//! (core probes, GUI schedules).

use std::path::Path;
use std::process::Command;

/// Total on-disk size of everything under `path`, in bytes. `None` on any
/// failure (missing dir, permission error, `du` not on `PATH`) — callers
/// treat this the same as every other probe in this crate: render "size
/// unknown", never panic and never block on a retry.
pub fn probe_worktree_size(path: &Path) -> Option<u64> {
    let output = Command::new("du").arg("-sk").arg(path).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let kb: u64 = stdout.split_whitespace().next()?.parse().ok()?;
    Some(kb.saturating_mul(1024))
}

/// Human-readable size for the Workspace size column/tooltip — "482 KB",
/// "1.3 GB". Picks the largest unit that keeps the number >= 1, one decimal
/// place above KB (bytes/KB are exact enough as whole numbers).
pub fn humanize_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.0} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn probes_a_real_directory_and_returns_a_positive_size() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("payload.bin"), vec![0u8; 64 * 1024]).unwrap();

        let bytes = probe_worktree_size(tmp.path()).expect("du should succeed on a real dir");
        // `du` reports whole filesystem blocks, so this is >= the raw
        // payload size, never less.
        assert!(bytes >= 64 * 1024, "expected at least 64KB, got {bytes}");
    }

    #[test]
    fn missing_path_returns_none() {
        let missing = std::env::temp_dir().join("switchbard-size-probe-does-not-exist");
        assert_eq!(probe_worktree_size(&missing), None);
    }

    #[test]
    fn humanize_size_picks_the_largest_sensible_unit() {
        assert_eq!(humanize_size(512), "512 B");
        assert_eq!(humanize_size(2048), "2 KB");
        assert_eq!(humanize_size(5 * 1024 * 1024), "5 MB");
        assert_eq!(
            humanize_size(3 * 1024 * 1024 * 1024 + 512 * 1024 * 1024),
            "3.5 GB"
        );
    }
}
