use std::path::{Path, PathBuf};

use switchbard_core::{config::Config, Repo, WorktreeAlias, WorktreeRef};

pub fn worktree_display_name(config: &Config, repo: &Repo, worktree: &WorktreeRef) -> String {
    configured_worktree_name(config, repo, &worktree.path)
        .unwrap_or_else(|| inferred_worktree_name(&worktree.path))
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
