//! Worktree enumeration helper — used by `main`, the probe thread, and the
//! HiveApp methods that respond to "Refresh" / "Add repo".
//!
//! Lives in its own module so the call sites can stay one-liners and the
//! "ensure the primary checkout is in the list even when git says nothing"
//! quirk only lives in one place.

use switchbard_core::{enumerate_worktrees, Repo, WorktreeRef};

pub fn expand_worktrees(repos: &[Repo]) -> Vec<WorktreeRef> {
    let mut out = Vec::new();
    for repo in repos {
        let mut added_primary = false;
        if let Ok(entries) = enumerate_worktrees(&repo.path) {
            for e in entries {
                if !e.path.exists() {
                    continue;
                }
                if e.path == repo.path {
                    added_primary = true;
                }
                out.push(WorktreeRef {
                    repo_name: repo.name.clone(),
                    path: e.path,
                    branch: e.branch,
                    head: e.head,
                });
            }
        }
        if !added_primary {
            out.push(WorktreeRef {
                repo_name: repo.name.clone(),
                path: repo.path.clone(),
                branch: None,
                head: String::new(),
            });
        }
    }
    out
}
