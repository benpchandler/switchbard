use crate::types::{AttributedListener, LocalListener, WorktreeRef};

/// Attribute each listener to a (repo, worktree) pair via cwd-prefix match.
/// Worktrees are tried in order of decreasing path length so the most-specific
/// path wins (e.g. a worktree at `~/Dev/repo/.worktrees/foo` is matched before
/// the primary at `~/Dev/repo`).
pub fn attribute(
    listeners: &[LocalListener],
    worktrees: &[WorktreeRef],
) -> Vec<AttributedListener> {
    let mut sorted: Vec<&WorktreeRef> = worktrees.iter().collect();
    sorted.sort_by_key(|w| std::cmp::Reverse(w.path.as_os_str().len()));

    listeners
        .iter()
        .map(|l| {
            let matched = l
                .cwd
                .as_ref()
                .and_then(|cwd| sorted.iter().find(|w| cwd.starts_with(&w.path)));
            AttributedListener {
                repo_name: matched.map(|w| w.repo_name.clone()),
                worktree_path: matched.map(|w| w.path.clone()),
                worktree_branch: matched.and_then(|w| w.branch.clone()),
                listener: l.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_listener(pid: u32, port: u16, cwd: Option<&str>) -> LocalListener {
        LocalListener {
            pid,
            pgid: pid as i32,
            port,
            command_name: "x".into(),
            cwd: cwd.map(PathBuf::from),
        }
    }

    fn wt(repo: &str, path: &str, branch: Option<&str>) -> WorktreeRef {
        WorktreeRef {
            repo_name: repo.into(),
            path: PathBuf::from(path),
            branch: branch.map(|b| b.into()),
            head: String::new(),
        }
    }

    #[test]
    fn matches_by_cwd_prefix_to_worktree() {
        let worktrees = vec![
            wt(
                "alpha",
                "/Users/me/code/alpha",
                Some("main"),
            ),
            wt(
                "alpha",
                "/Users/me/code/.worktrees/alpha/feat/tracks-tab",
                Some("feat/tracks-tab"),
            ),
            wt("delta", "/Users/me/code/delta", Some("main")),
        ];
        let listeners = vec![
            make_listener(1, 8000, Some("/Users/me/code/delta/scripts")),
            make_listener(2, 8420, Some("/Users/me/code/alpha/lyon")),
            make_listener(
                3,
                8421,
                Some("/Users/me/code/.worktrees/alpha/feat/tracks-tab/services/bundle"),
            ),
            make_listener(4, 7000, Some("/usr/bin")),
            make_listener(5, 9000, None),
        ];
        let out = attribute(&listeners, &worktrees);
        assert_eq!(out[0].repo_name.as_deref(), Some("delta"));
        assert_eq!(out[0].worktree_branch.as_deref(), Some("main"));
        assert_eq!(out[1].repo_name.as_deref(), Some("alpha"));
        assert_eq!(out[1].worktree_branch.as_deref(), Some("main"));
        assert_eq!(out[2].repo_name.as_deref(), Some("alpha"));
        // The more-specific worktree path wins over the primary path.
        assert_eq!(out[2].worktree_branch.as_deref(), Some("feat/tracks-tab"));
        assert_eq!(out[3].repo_name, None);
        assert_eq!(out[4].repo_name, None);
    }
}
