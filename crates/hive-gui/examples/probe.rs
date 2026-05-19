use hive_core::{attribute, enumerate_worktrees, scan_listeners, Repo, WorktreeRef};
use std::path::PathBuf;

fn main() {
    let base_repos = vec![
        Repo { name: "alpha".into(), path: PathBuf::from("/Users/me/code/alpha") },
        Repo { name: "delta".into(), path: PathBuf::from("/Users/me/code/delta") },
        Repo { name: "gamma".into(), path: PathBuf::from("/Users/me/code/gamma") },
        Repo { name: "beta".into(), path: PathBuf::from("/Users/me/code/beta") },
        Repo { name: "hive".into(), path: PathBuf::from("/Users/me/code/hive") },
    ];

    let mut worktrees: Vec<WorktreeRef> = vec![];
    for repo in &base_repos {
        let mut added_primary = false;
        if let Ok(entries) = enumerate_worktrees(&repo.path) {
            for e in entries {
                if !e.path.exists() { continue; }
                if e.path == repo.path { added_primary = true; }
                worktrees.push(WorktreeRef { repo_name: repo.name.clone(), path: e.path, branch: e.branch });
            }
        }
        if !added_primary {
            worktrees.push(WorktreeRef { repo_name: repo.name.clone(), path: repo.path.clone(), branch: None });
        }
    }
    println!("Tracking {} repos with {} total worktrees", base_repos.len(), worktrees.len());

    let listeners = scan_listeners().unwrap();
    let attributed = attribute(&listeners, &worktrees);
    let total = attributed.len();
    let matched: Vec<_> = attributed.iter().filter(|l| l.repo_name.is_some()).collect();
    println!("Total listeners: {total}");
    println!("Attributed: {}", matched.len());
    for a in &matched {
        let cwd = a.listener.cwd.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        let branch = a.worktree_branch.as_deref().unwrap_or("?");
        println!(
            "  port={:<6} pid={:<6} cmd={:<20} repo={:<20} branch={:<24} cwd={}",
            a.listener.port,
            a.listener.pid,
            a.listener.command_name,
            a.repo_name.as_deref().unwrap_or(""),
            branch,
            cwd,
        );
    }
    println!("---unattributed sample (first 8)---");
    for a in attributed.iter().filter(|l| l.repo_name.is_none()).take(8) {
        let cwd = a.listener.cwd.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
        println!("  port={:<6} pid={:<6} cmd={:<20} cwd={}", a.listener.port, a.listener.pid, a.listener.command_name, cwd);
    }
}
