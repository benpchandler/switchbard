use crate::types::{AttributedListener, LocalListener, WorktreeRef};
use std::path::Path;

/// Sort worktrees by decreasing path length so the most-specific path is
/// tried first — shared by every cwd-prefix attribution caller (this
/// module's own [`attribute`] and `agent_sessions::attribute_agent_sessions`)
/// so the ordering rule lives in exactly one place.
pub(crate) fn sort_by_specificity(worktrees: &[WorktreeRef]) -> Vec<&WorktreeRef> {
    let mut sorted: Vec<&WorktreeRef> = worktrees.iter().collect();
    sorted.sort_by_key(|w| std::cmp::Reverse(w.path.as_os_str().len()));
    sorted
}

/// The one cwd-prefix match algorithm: the most-specific worktree whose path
/// is a prefix of `cwd`, or `None` if nothing covers it (no cwd at all, or a
/// cwd outside every known worktree). `worktrees_by_specificity` must already
/// be sorted by [`sort_by_specificity`] — most callers hold one such sort
/// across many `cwd`s in a loop, so re-sorting per call would be wasted work.
pub(crate) fn most_specific_worktree<'a>(
    cwd: Option<&Path>,
    worktrees_by_specificity: &[&'a WorktreeRef],
) -> Option<&'a WorktreeRef> {
    let cwd = cwd?;
    worktrees_by_specificity
        .iter()
        .find(|w| cwd.starts_with(&w.path))
        .copied()
}

/// Attribute each listener to a (repo, worktree) pair via cwd-prefix match.
/// Worktrees are tried in order of decreasing path length so the most-specific
/// path wins (e.g. a worktree at `~/Dev/repo/.worktrees/foo` is matched before
/// the primary at `~/Dev/repo`). Same algorithm `agent_sessions::
/// attribute_agent_sessions` uses for interactive/dispatch agent processes —
/// [`most_specific_worktree`] is the one place it lives.
pub fn attribute(
    listeners: &[LocalListener],
    worktrees: &[WorktreeRef],
) -> Vec<AttributedListener> {
    let sorted = sort_by_specificity(worktrees);

    listeners
        .iter()
        .map(|l| {
            let matched = most_specific_worktree(l.cwd.as_deref(), &sorted);
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
            wt("alpha", "/Users/dev/code/alpha", Some("main")),
            wt(
                "alpha",
                "/Users/dev/code/.worktrees/alpha/feat/tracks-tab",
                Some("feat/tracks-tab"),
            ),
            wt("beta", "/Users/dev/code/beta", Some("main")),
        ];
        let listeners = vec![
            make_listener(1, 8000, Some("/Users/dev/code/beta/scripts")),
            make_listener(2, 8420, Some("/Users/dev/code/alpha/lyon")),
            make_listener(
                3,
                8421,
                Some("/Users/dev/code/.worktrees/alpha/feat/tracks-tab/services/bundle"),
            ),
            make_listener(4, 7000, Some("/usr/bin")),
            make_listener(5, 9000, None),
        ];
        let out = attribute(&listeners, &worktrees);
        assert_eq!(out[0].repo_name.as_deref(), Some("beta"));
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
