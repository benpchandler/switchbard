//! sbt's read-only view of the auto-install pipeline (TASK-227).
//!
//! `scripts/install-switchbard.sh` and `scripts/auto-install-main.sh` are the
//! only writers of one small JSON file: a receipt of the last install
//! attempt. This module is the only place sbt reads it, so the startup
//! banner and any future surface agree with each other and with the scripts'
//! own log line. Since TASK-272 an install is a one-way street - main
//! replaces a running branch build only once that branch is merged or
//! deleted on origin - so a `refused` receipt is usually "waiting for a
//! merge", and the banner says so rather than sounding an alarm.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// The same variable the install scripts honor
/// (`SWITCHBARD_AUTO_INSTALL_DIR` in `scripts/install-switchbard.sh` and
/// `scripts/auto-install-main.sh`).
pub const AUTO_INSTALL_DIR_ENV: &str = "SWITCHBARD_AUTO_INSTALL_DIR";

/// Carried across a self-restart's `exec` so the startup banner can name the
/// build being replaced. Set by `main.rs`, read by `main.rs` on the next launch.
pub const PREV_BUILD_ENV: &str = "SBT_RESUME_BUILD";

#[must_use]
pub fn state_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(AUTO_INSTALL_DIR_ENV) {
        return Some(PathBuf::from(dir));
    }
    dirs::home_dir().map(|home| home.join(".switchbard").join("auto-install"))
}

#[derive(Debug, Clone, Deserialize)]
struct InstallReceipt {
    outcome: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    from_branch: Option<String>,
}

fn read_receipt(dir: &Path) -> Option<InstallReceipt> {
    let bytes = std::fs::read(dir.join("last-install.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The one line sbt shows right after a self-restart into a new binary.
/// `prev_build` is the previous build's commit, carried through
/// [`PREV_BUILD_ENV`]; `None` when this is the process's first launch ever
/// with that variable set (predates it, or the env was stripped).
#[must_use]
pub fn updated_status_line(prev_build: Option<&str>) -> String {
    let mut line = format!(
        "updated to {} {}",
        short(switchbard_core::BUILD_COMMIT),
        switchbard_core::BUILD_BRANCH
    );
    if let Some(prev) =
        prev_build.filter(|prev| !prev.is_empty() && *prev != switchbard_core::BUILD_COMMIT)
    {
        line.push_str(&format!(" (from {})", short(prev)));
    }
    line
}

/// The guard's own wording for "main does not contain the running build's
/// branch yet" - the ordinary, expected wait, not a fault.
const WAITING_FOR_MERGE: &str = "main does not yet contain";

/// A one-line notice for an ordinary (non-restart) launch when auto-install
/// has something the owner should know: a wait for a merge, a refused
/// attempt, or a failed build. `None` when there is nothing to say - the
/// common case. `_now` is kept so a future time-based notice has its clock
/// injected rather than read.
#[must_use]
pub fn pending_notice(dir: Option<&Path>, _now: DateTime<Utc>) -> Option<String> {
    let dir = dir?;
    let receipt = read_receipt(dir)?;
    let reason = receipt.reason.as_deref().unwrap_or("reason not recorded");
    match receipt.outcome.as_str() {
        "refused" if reason.starts_with(WAITING_FOR_MERGE) => {
            let branch = receipt
                .from_branch
                .as_deref()
                .unwrap_or("an unrecognized branch");
            Some(format!(
                "auto-install is waiting for {branch} to merge before installing main - \
                 `mise run install --force` installs main now"
            ))
        }
        "refused" => {
            let branch = receipt
                .from_branch
                .as_deref()
                .unwrap_or("an unrecognized branch");
            Some(format!(
                "auto-install refused (installed build: {branch}): {reason} - \
                 run `mise run install --force` once you've confirmed that's safe"
            ))
        }
        "failed" => Some(format!(
            "auto-install's last build failed: {reason} - \
             see {}, nothing was replaced",
            dir.join("auto-install.log").display()
        )),
        _ => None,
    }
}

fn short(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, contents: &str) {
        let mut file = std::fs::File::create(dir.join(name)).expect("create fixture");
        file.write_all(contents.as_bytes()).expect("write fixture");
    }

    #[test]
    fn updated_status_line_names_the_current_build() {
        let line = updated_status_line(None);
        assert!(line.starts_with("updated to "), "{line}");
        assert!(!line.contains("(from"), "{line}");
    }

    #[test]
    fn updated_status_line_names_the_previous_build_when_it_differs() {
        let line = updated_status_line(Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"));
        assert!(line.contains("(from deadbeef)"), "{line}");
    }

    #[test]
    fn updated_status_line_omits_from_when_previous_equals_current() {
        let line = updated_status_line(Some(switchbard_core::BUILD_COMMIT));
        assert!(!line.contains("(from"), "{line}");
    }

    #[test]
    fn no_dir_means_no_notice() {
        assert_eq!(pending_notice(None, Utc::now()), None);
    }

    #[test]
    fn empty_dir_means_no_notice() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(pending_notice(Some(dir.path()), Utc::now()), None);
    }

    #[test]
    fn a_wait_for_merge_reads_as_waiting_not_as_a_refusal() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            dir.path(),
            "last-install.json",
            r#"{"outcome": "refused", "reason": "main does not yet contain a321320f (branch 'feat/tui-filter-cleanup', still open on origin); merge or delete it", "from_branch": "feat/tui-filter-cleanup"}"#,
        );
        let notice = pending_notice(Some(dir.path()), Utc::now()).expect("waiting notice");
        assert!(
            notice.starts_with("auto-install is waiting for feat/tui-filter-cleanup to merge"),
            "{notice}"
        );
        assert!(!notice.contains("refused"), "{notice}");
        assert!(notice.contains("mise run install --force"), "{notice}");
    }

    #[test]
    fn a_leftover_hold_file_is_ignored() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            dir.path(),
            "hold.json",
            r#"{"branch": "feat/x", "until": "2999-01-01T00:00:00Z"}"#,
        );
        assert_eq!(pending_notice(Some(dir.path()), Utc::now()), None);
    }

    #[test]
    fn a_refused_receipt_surfaces_its_branch_reason_and_a_resolving_command() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            dir.path(),
            "last-install.json",
            r#"{"outcome": "refused", "reason": "would drop 3 commits", "from_branch": "feat/z"}"#,
        );
        let notice = pending_notice(Some(dir.path()), Utc::now()).expect("refusal notice");
        assert!(notice.contains("feat/z"), "{notice}");
        assert!(notice.contains("would drop 3 commits"), "{notice}");
        assert!(notice.contains("mise run install --force"), "{notice}");
    }

    #[test]
    fn a_failed_build_receipt_surfaces_its_reason_and_names_nothing_replaced() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            dir.path(),
            "last-install.json",
            r#"{"outcome": "failed", "reason": "cargo install failed for sbt (exit 101): error[E0308]"}"#,
        );
        let notice = pending_notice(Some(dir.path()), Utc::now()).expect("failure notice");
        assert!(notice.contains("error[E0308]"), "{notice}");
        assert!(notice.contains("nothing was replaced"), "{notice}");
    }

    #[test]
    fn an_installed_receipt_produces_no_notice() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            dir.path(),
            "last-install.json",
            r#"{"outcome": "installed"}"#,
        );
        assert_eq!(pending_notice(Some(dir.path()), Utc::now()), None);
    }

    #[test]
    fn malformed_json_is_treated_as_no_signal_not_a_crash() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "last-install.json", "not json");
        assert_eq!(pending_notice(Some(dir.path()), Utc::now()), None);
    }
}
