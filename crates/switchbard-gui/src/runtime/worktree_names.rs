use std::path::{Path, PathBuf};

use switchbard_core::{config::Config, Repo, WorktreeAlias, WorktreeRef};

pub fn worktree_display_name(config: &Config, repo: &Repo, worktree: &WorktreeRef) -> String {
    configured_worktree_name(config, repo, &worktree.path).unwrap_or_else(|| {
        inferred_worktree_name_for(&worktree.path, &repo.name, worktree.branch.as_deref())
    })
}

/// A display name for a worktree with no configured alias.
///
/// Normally the directory's own name, which is what a worktree laid out as
/// `.worktrees/<feature>` is actually called. But some tools nest a checkout
/// under a per-worktree parent and give the leaf the *repo's* name -
/// `.treehouse/budget-404c3c/1/budget`, `.../2/budget`, and so on. The leaf
/// then names the repo, not the worktree, so every such worktree renders
/// identically, and identically to the primary checkout.
///
/// That is not a cosmetic problem. It surfaced as four lines reading `budget`
/// in the bulk-remove confirmation, which reads as an offer to delete the repo
/// itself. A name that cannot distinguish two things is not doing the one job
/// a name has, and a destructive dialog is the worst place to discover it.
///
/// So when the leaf just repeats the repo name, fall back to the branch, which
/// is what actually distinguishes these. Failing that (detached HEAD), qualify
/// the leaf with its parent directory rather than returning a bare repeat.
fn inferred_worktree_name_for(path: &Path, repo_name: &str, branch: Option<&str>) -> String {
    let leaf = inferred_worktree_name(path);
    if !leaf.eq_ignore_ascii_case(repo_name) {
        return leaf;
    }
    if let Some(branch) = branch.map(str::trim).filter(|b| !b.is_empty()) {
        return branch.to_string();
    }
    match path.parent().and_then(Path::file_name) {
        Some(parent) => format!("{}/{leaf}", parent.to_string_lossy()),
        None => leaf,
    }
}

pub fn configured_worktree_name(
    config: &Config,
    repo: &Repo,
    worktree_path: &Path,
) -> Option<String> {
    config
        .worktrees
        .iter()
        .find(|alias| {
            same_path(&alias.repo_path, &repo.path)
                && same_path(&alias.worktree_path, worktree_path)
        })
        .map(|alias| alias.name.clone())
}

pub fn unique_worktree_name_error(
    config: &Config,
    repo: &Repo,
    candidate: &str,
    current_path: Option<&Path>,
) -> Option<String> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return Some("Name cannot be empty".to_string());
    }
    let candidate_lc = trimmed.to_lowercase();
    for alias in &config.worktrees {
        if !same_path(&alias.repo_path, &repo.path) {
            continue;
        }
        if current_path.is_some_and(|path| same_path(&alias.worktree_path, path)) {
            continue;
        }
        if alias.name.trim().to_lowercase() == candidate_lc {
            return Some(format!("'{trimmed}' is already used in {}", repo.name));
        }
    }
    None
}

pub fn worktree_name_conflict_error(
    config: &Config,
    repo: &Repo,
    worktrees: &[WorktreeRef],
    candidate: &str,
    current_path: Option<&Path>,
) -> Option<String> {
    if let Some(err) = unique_worktree_name_error(config, repo, candidate, current_path) {
        return Some(err);
    }
    let trimmed = candidate.trim();
    let candidate_lc = trimmed.to_lowercase();
    for worktree in worktrees {
        if worktree.repo_name != repo.name {
            continue;
        }
        if current_path.is_some_and(|path| same_path(&worktree.path, path)) {
            continue;
        }
        if worktree_display_name(config, repo, worktree)
            .trim()
            .to_lowercase()
            == candidate_lc
        {
            return Some(format!("'{trimmed}' is already used in {}", repo.name));
        }
    }
    None
}

pub fn default_worktree_location(repo: &Repo, name: &str) -> PathBuf {
    let repo_leaf = repo
        .path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| repo.name.clone());
    let base = repo
        .path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repo.path.clone())
        .join(".worktrees")
        .join(repo_leaf);
    base.join(slug_for_worktree_name(name))
}

pub fn upsert_worktree_alias(
    config: &mut Config,
    repo: &Repo,
    worktree_path: PathBuf,
    name: String,
) {
    let trimmed = name.trim().to_string();
    if let Some(alias) = config.worktrees.iter_mut().find(|alias| {
        same_path(&alias.repo_path, &repo.path) && same_path(&alias.worktree_path, &worktree_path)
    }) {
        alias.name = trimmed;
        return;
    }
    config.worktrees.push(WorktreeAlias {
        repo_path: repo.path.clone(),
        worktree_path,
        name: trimmed,
    });
}

pub fn remove_worktree_alias(config: &mut Config, repo_path: &Path, worktree_path: &Path) {
    config.worktrees.retain(|alias| {
        !(same_path(&alias.repo_path, repo_path) && same_path(&alias.worktree_path, worktree_path))
    });
}

fn inferred_worktree_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

pub fn slug_for_worktree_name(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "worktree".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod display_name_tests {
    use super::*;

    fn repo() -> Repo {
        Repo {
            name: "budget".into(),
            path: PathBuf::from("/Users/x/Dev/budget"),
        }
    }

    fn worktree(path: &str, branch: Option<&str>) -> WorktreeRef {
        WorktreeRef {
            repo_name: "budget".into(),
            path: PathBuf::from(path),
            branch: branch.map(str::to_string),
            head: "abc1234".into(),
        }
    }

    #[test]
    fn the_ordinary_layout_still_uses_the_directory_name() {
        assert_eq!(
            worktree_display_name(
                &Config::default(),
                &repo(),
                &worktree("/Users/x/Dev/.worktrees/budget-docs", Some("rebrand/docs")),
            ),
            "budget-docs"
        );
    }

    /// The reported bug: a layout that names the leaf after the *repo* made
    /// every such worktree render identically, and identically to the primary
    /// checkout - four lines reading "budget" in the bulk-remove dialog.
    #[test]
    fn a_leaf_that_only_repeats_the_repo_name_falls_back_to_the_branch() {
        let a = worktree_display_name(
            &Config::default(),
            &repo(),
            &worktree(
                "/Users/x/.treehouse/budget-404c3c/1/budget",
                Some("fm/led-1"),
            ),
        );
        let b = worktree_display_name(
            &Config::default(),
            &repo(),
            &worktree(
                "/Users/x/.treehouse/budget-404c3c/2/budget",
                Some("fm/led-2"),
            ),
        );
        assert_eq!(a, "fm/led-1");
        assert_eq!(b, "fm/led-2");
        assert_ne!(a, b, "two worktrees must never render the same name");
        assert_ne!(a, "budget", "must not read as the repo itself");
    }

    /// Detached HEAD has no branch to fall back to, so qualify with the parent
    /// rather than hand back a bare repeat of the repo name.
    #[test]
    fn a_detached_leaf_is_qualified_by_its_parent_directory() {
        assert_eq!(
            worktree_display_name(
                &Config::default(),
                &repo(),
                &worktree("/Users/x/.treehouse/budget-404c3c/3/budget", None),
            ),
            "3/budget"
        );
    }

    /// A configured alias is the user's own choice and always wins.
    #[test]
    fn a_configured_alias_still_wins() {
        let mut config = Config::default();
        config.worktrees.push(WorktreeAlias {
            repo_path: PathBuf::from("/Users/x/Dev/budget"),
            worktree_path: PathBuf::from("/Users/x/.treehouse/budget-404c3c/1/budget"),
            name: "scratch".into(),
        });
        assert_eq!(
            worktree_display_name(
                &config,
                &repo(),
                &worktree(
                    "/Users/x/.treehouse/budget-404c3c/1/budget",
                    Some("fm/led-1")
                ),
            ),
            "scratch"
        );
    }
}
