//! Writing a project's declared status list into its `backlog/config.yml`.
//!
//! The one place this crate mutates a *project's configuration* rather than
//! its tasks, and it exists because the shared status vocabulary has to be
//! made true rather than assumed. See
//! [`crate::backlog::ordered_status_vocabulary`] for the bug that taught us
//! the difference.
//!
//! # Why this edits the file instead of shelling out
//!
//! Every other mutation in this crate goes through the `backlog` CLI, which
//! owns the format. Statuses are the documented exception — the CLI refuses
//! the write and names the file as the way in:
//!
//! ```text
//! $ backlog config set statuses '[...]'
//! statuses cannot be set directly. View current values with
//! 'backlog config get statuses'. Edit the list in the project config file
//! (`backlog/config.yml`, `.backlog/config.yml`, or `backlog.config.yml`)
//! directly.
//! ```
//!
//! So a line-level edit is the owning tool's own instruction, not a bypass of
//! it. Only the `statuses:` line is rewritten; every other key, comment and
//! byte in the file is left exactly as found.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use super::types::{order_statuses_public, STANDARD_STATUSES};

/// The three names the `backlog` CLI accepts for a project's config, in the
/// order its own error message lists them.
const CONFIG_CANDIDATES: [&str; 3] = [
    "backlog/config.yml",
    ".backlog/config.yml",
    "backlog.config.yml",
];

/// Where this repo keeps its backlog config, if it has one.
pub fn config_path(repo_root: &Path) -> Option<PathBuf> {
    CONFIG_CANDIDATES
        .iter()
        .map(|c| repo_root.join(c))
        .find(|p| p.is_file())
}

/// Add every [`STANDARD_STATUSES`] value this project doesn't already declare,
/// keeping any it declares that aren't standard.
///
/// Additive by construction: nothing is removed, so no existing task can be
/// left carrying a status the config no longer allows. Returns the new list.
pub fn add_standard_statuses(repo_root: &Path) -> Result<Vec<String>> {
    let path = config_path(repo_root)
        .ok_or_else(|| anyhow!("no backlog config found under {}", repo_root.display()))?;
    let original =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;

    let (line_idx, declared) = parse_statuses_line(&original)
        .ok_or_else(|| anyhow!("{} has no `statuses:` line", path.display()))?;

    let mut merged: std::collections::BTreeSet<String> = declared.into_iter().collect();
    for standard in STANDARD_STATUSES {
        if !merged.iter().any(|d| d.eq_ignore_ascii_case(standard)) {
            merged.insert((*standard).to_string());
        }
    }
    let ordered = order_statuses_public(merged);

    let rendered = format!(
        "statuses: [{}]",
        ordered
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut lines: Vec<&str> = original.lines().collect();
    lines[line_idx] = &rendered;
    let mut out = lines.join("\n");
    if original.ends_with('\n') {
        out.push('\n');
    }
    // Written whole rather than in place: a partial write here would leave the
    // project with a config the CLI can't parse, which breaks every task
    // operation in that repo, not just statuses.
    std::fs::write(&path, out).with_context(|| format!("writing {}", path.display()))?;
    Ok(ordered)
}

/// The `statuses:` line's index and the values it declares.
///
/// Deliberately not a YAML parse: rewriting the file through a serializer
/// would reformat every other key and strip comments, turning a one-line
/// change into an unreviewable diff in someone else's repo.
fn parse_statuses_line(config: &str) -> Option<(usize, Vec<String>)> {
    let (idx, line) = config
        .lines()
        .enumerate()
        .find(|(_, l)| l.trim_start().starts_with("statuses:"))?;
    let body = line.split_once(':')?.1.trim();
    let inner = body.strip_prefix('[')?.strip_suffix(']')?;
    let values = inner
        .split(',')
        .map(|v| v.trim().trim_matches(['"', '\'']).to_string())
        .filter(|v| !v.is_empty())
        .collect();
    Some((idx, values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn repo_with_config(body: &str) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("backlog")).unwrap();
        let path = tmp.path().join("backlog/config.yml");
        std::fs::write(&path, body).unwrap();
        let root = tmp.path().to_path_buf();
        (tmp, root)
    }

    const TRIO: &str = "project_name: \"Demo\"\n\
                        default_status: \"To Do\"\n\
                        statuses: [\"To Do\", \"In Progress\", \"Done\"]\n\
                        default_editor: \"hx\"\n";

    #[test]
    fn adds_only_what_is_missing_and_orders_canonically() {
        let (_tmp, root) = repo_with_config(TRIO);
        let out = add_standard_statuses(&root).unwrap();
        assert_eq!(
            out,
            vec!["Icebox", "To Do", "In Progress", "In Review", "Done"]
        );
    }

    /// The whole point is to stop losing writes, so this must never drop a
    /// status a repo already relies on — a task carrying it would become
    /// unmovable and, worse, unrestorable.
    #[test]
    fn a_repos_own_nonstandard_status_survives() {
        let (_tmp, root) = repo_with_config(
            "statuses: [\"To Do\", \"Blocked\", \"Done\"]\nproject_name: \"Demo\"\n",
        );
        let out = add_standard_statuses(&root).unwrap();
        assert!(out.contains(&"Blocked".to_string()), "got {out:?}");
        assert!(out.contains(&"In Review".to_string()));
    }

    /// Someone else's repo: the diff has to be one line, or nobody will
    /// review it.
    #[test]
    fn every_other_line_is_left_byte_identical() {
        let (_tmp, root) = repo_with_config(TRIO);
        add_standard_statuses(&root).unwrap();
        let after = std::fs::read_to_string(root.join("backlog/config.yml")).unwrap();

        let before_lines: Vec<&str> = TRIO.lines().collect();
        let after_lines: Vec<&str> = after.lines().collect();
        assert_eq!(before_lines.len(), after_lines.len());
        for (b, a) in before_lines.iter().zip(&after_lines) {
            if b.starts_with("statuses:") {
                assert_ne!(b, a, "the statuses line is the one that changes");
            } else {
                assert_eq!(b, a, "nothing else may move");
            }
        }
        assert!(after.ends_with('\n'), "trailing newline preserved");
    }

    #[test]
    fn already_standardized_is_a_no_op_in_content() {
        let (_tmp, root) = repo_with_config(
            "statuses: [\"Icebox\", \"To Do\", \"In Progress\", \"In Review\", \"Done\"]\n",
        );
        let before = std::fs::read_to_string(root.join("backlog/config.yml")).unwrap();
        add_standard_statuses(&root).unwrap();
        let after = std::fs::read_to_string(root.join("backlog/config.yml")).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn a_repo_without_a_backlog_config_reports_rather_than_creating_one() {
        let tmp = TempDir::new().unwrap();
        assert!(add_standard_statuses(tmp.path()).is_err());
        assert!(config_path(tmp.path()).is_none());
    }
}
