//! Which tree this binary was compiled from.
//!
//! One authority for the question "what am I running", shared by every
//! installed Switchbard binary. `build.rs` stamps the values; this module is
//! the only place that reads them, so `--version`, the event log, and the
//! install guard can never disagree about the answer.

/// Full 40-character commit the binary was compiled from, or `"unknown"` when
/// the build tree was not a git checkout.
pub const BUILD_COMMIT: &str = env!("SWITCHBARD_BUILD_COMMIT");

/// Branch checked out at compile time, `"detached"`, or `"unknown"`.
pub const BUILD_BRANCH: &str = env!("SWITCHBARD_BUILD_BRANCH");

/// Workspace version — the number alone, which is deliberately *not* enough to
/// identify a build (every branch reports the same one).
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

const BUILD_DIRTY: &str = env!("SWITCHBARD_BUILD_DIRTY");

/// Whether the tree had uncommitted changes at compile time.
pub fn build_is_dirty() -> bool {
    BUILD_DIRTY == "true"
}

/// Whether the commit stamp identifies a real commit. A build from a tarball
/// or a git-less environment cannot be compared against a repository, and
/// callers that gate on ancestry must treat that as "cannot answer" rather
/// than as a pass.
pub fn build_commit_is_known() -> bool {
    BUILD_COMMIT != "unknown"
}

/// The `--version` line: version plus the identity that actually distinguishes
/// one build from another. Example: `0.4.0 (9e3770b6 main)`.
pub const VERSION_LINE: &str = env!("SWITCHBARD_BUILD_VERSION_LINE");

/// [`VERSION_LINE`], for call sites that read better as a call.
pub fn version_line() -> &'static str {
    VERSION_LINE
}

/// The machine-readable `build-id` report every installed binary prints.
///
/// Consumed by `scripts/install-switchbard.sh` to decide whether an install
/// would move the user backwards, so the shape is a contract: one
/// `key=value` line per fact, stable keys, no decoration. Adding a key is
/// safe; renaming or reordering one is not.
pub fn build_id_report() -> String {
    format!(
        "commit={BUILD_COMMIT}\nbranch={BUILD_BRANCH}\ndirty={}\nversion={CRATE_VERSION}\n",
        build_is_dirty()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_line_carries_more_than_the_workspace_version() {
        // The whole point: two builds of different branches must not produce
        // the same string, which is exactly what reporting CRATE_VERSION
        // alone did.
        let line = version_line();
        assert!(line.starts_with(CRATE_VERSION), "{line}");
        assert!(line.len() > CRATE_VERSION.len(), "{line}");
        assert!(line.contains(BUILD_BRANCH), "{line}");
    }

    #[test]
    fn a_known_commit_stamp_is_a_full_hex_sha() {
        if !build_commit_is_known() {
            return;
        }
        assert_eq!(BUILD_COMMIT.len(), 40, "{BUILD_COMMIT}");
        assert!(
            BUILD_COMMIT.chars().all(|c| c.is_ascii_hexdigit()),
            "{BUILD_COMMIT}"
        );
    }

    #[test]
    fn the_build_id_report_is_parseable_key_value_lines() {
        let report = build_id_report();
        let keys: Vec<&str> = report
            .lines()
            .map(|line| line.split_once('=').expect("every line is key=value").0)
            .collect();
        assert_eq!(keys, ["commit", "branch", "dirty", "version"]);
    }

    #[test]
    fn the_reported_commit_is_the_one_the_install_guard_would_compare() {
        let report = build_id_report();
        let commit = report
            .lines()
            .find_map(|line| line.strip_prefix("commit="))
            .expect("commit key present");
        assert_eq!(commit, BUILD_COMMIT);
    }
}
