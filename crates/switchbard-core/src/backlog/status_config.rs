//! The shared status-configuration write boundary. Legacy repositories retain
//! their config files; migrated repositories use the central config document.
//! Only the statuses line is patched, preserving custom fields and comments.

use super::aggregate_storage;

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
    aggregate_storage::with_edit(repo_root, "config", "backlog/config.yml", |edit| {
        let path = if edit.is_central() {
            repo_root.join("backlog/config.yml")
        } else {
            config_path(repo_root)
                .ok_or_else(|| anyhow!("no backlog config found under {}", repo_root.display()))?
        };
        let original = if edit.is_central() && !edit.exists(&path) {
            "statuses: []\n".to_string()
        } else {
            edit.text(&path)?
        };
        let (out, ordered) = with_standard_statuses(&original, &path)?;
        if !edit.stage(&out) {
            super::write::atomic_write(&path, &out)?;
        }
        Ok(ordered)
    })
}

/// Effective config content, including the centrally owned singleton after cutover.
/// An empty migrated kind has the same defaults as an absent legacy config.
pub(super) fn read_config(repo_root: &Path) -> Result<Option<String>> {
    if let Some(content) = aggregate_storage::read(repo_root, "config", "backlog/config.yml")? {
        return Ok(content);
    }
    config_path(repo_root)
        .map(|path| {
            std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))
        })
        .transpose()
}

fn with_standard_statuses(original: &str, path: &Path) -> Result<(String, Vec<String>)> {
    let (line_idx, declared) = parse_statuses_line(original)
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
    Ok((out, ordered))
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
