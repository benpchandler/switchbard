//! sbt's read-only view of the auto-install pipeline (TASK-227).
//!
//! `scripts/install-switchbard.sh` and `scripts/auto-install-main.sh` are the
//! only writers of two small JSON files describing what happened the last
//! time sb/sbt were installed: a hold marker while a manual branch install is
//! protected from being overwritten, and a receipt of the last install
//! attempt. This module is the only place sbt reads them, so the startup
//! banner and any future surface agree with each other and with the scripts'
//! own log line.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, Utc};
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
struct HoldMarker {
    branch: String,
    until: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
struct InstallReceipt {
    outcome: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    from_branch: Option<String>,
}

fn read_hold(dir: &Path, now: DateTime<Utc>) -> Option<HoldMarker> {
    let bytes = std::fs::read(dir.join("hold.json")).ok()?;
    let hold: HoldMarker = serde_json::from_slice(&bytes).ok()?;
    (hold.until > now).then_some(hold)
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

/// A one-line notice for an ordinary (non-restart) launch when auto-install
/// has something the owner should know: an active hold, or a refused
/// attempt. `None` when there is nothing to say - the common case.
#[must_use]
pub fn pending_notice(dir: Option<&Path>, now: DateTime<Utc>) -> Option<String> {
    let dir = dir?;
    if let Some(hold) = read_hold(dir, now) {
        let until = hold.until.with_timezone(&Local).format("%H:%M");
        return Some(format!(
            "auto-install is holding {} until {until} - `mise run install --force` updates now",
            hold.branch
        ));
    }
    let receipt = read_receipt(dir)?;
    (receipt.outcome == "refused").then(|| {
        let branch = receipt
            .from_branch
            .as_deref()
            .unwrap_or("an unrecognized branch");
        let reason = receipt.reason.as_deref().unwrap_or("reason not recorded");
        format!(
            "auto-install refused (installed build: {branch}): {reason} - \
             run `mise run install --force` once you've confirmed that's safe"
        )
    })
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(8)]
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
    fn an_unexpired_hold_names_the_branch_and_a_resolving_gesture() {
        let dir = tempfile::tempdir().expect("tempdir");
        let now = Utc::now();
        let until = now + chrono::Duration::hours(1);
        write(
            dir.path(),
            "hold.json",
            &format!(
                r#"{{"branch": "feat/x", "until": "{}"}}"#,
                until.to_rfc3339()
            ),
        );
        let notice = pending_notice(Some(dir.path()), now).expect("hold notice");
        assert!(notice.contains("feat/x"), "{notice}");
        assert!(notice.contains("--force"), "{notice}");
    }

    #[test]
    fn an_expired_hold_produces_no_notice_on_its_own() {
        let dir = tempfile::tempdir().expect("tempdir");
        let now = Utc::now();
        let until = now - chrono::Duration::hours(1);
        write(
            dir.path(),
            "hold.json",
            &format!(
                r#"{{"branch": "feat/x", "until": "{}"}}"#,
                until.to_rfc3339()
            ),
        );
        assert_eq!(pending_notice(Some(dir.path()), now), None);
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
    fn a_held_receipt_alone_produces_no_notice_the_hold_file_already_did() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "last-install.json", r#"{"outcome": "held"}"#);
        assert_eq!(pending_notice(Some(dir.path()), Utc::now()), None);
    }

    #[test]
    fn a_hold_takes_priority_over_a_stale_refused_receipt() {
        let dir = tempfile::tempdir().expect("tempdir");
        let now = Utc::now();
        let until = now + chrono::Duration::minutes(30);
        write(
            dir.path(),
            "last-install.json",
            r#"{"outcome": "refused", "reason": "stale reason from before the hold"}"#,
        );
        write(
            dir.path(),
            "hold.json",
            &format!(
                r#"{{"branch": "feat/y", "until": "{}"}}"#,
                until.to_rfc3339()
            ),
        );
        let notice = pending_notice(Some(dir.path()), now).expect("hold notice");
        assert!(notice.contains("feat/y"), "{notice}");
    }

    #[test]
    fn malformed_json_is_treated_as_no_signal_not_a_crash() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "last-install.json", "not json");
        assert_eq!(pending_notice(Some(dir.path()), Utc::now()), None);
    }
}
