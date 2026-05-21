//! Probe example — pass one or more repo paths as args and see how the
//! scanner attributes the current OS listeners back to them.
//!
//! Usage:
//!   cargo run --example probe -- /path/to/repo-a /path/to/repo-b ...

use std::path::PathBuf;
use switchbard_core::{attribute, enumerate_worktrees, scan_listeners, Repo, WorktreeRef};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: cargo run --example probe -- /path/to/repo [/path/to/repo ...]");
        std::process::exit(1);
    }
    let base_repos: Vec<Repo> = args
        .into_iter()
        .map(|s| {
            let path = PathBuf::from(&s);
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo")
                .to_string();
            Repo { name, path }
        })
        .collect();

    let mut worktrees: Vec<WorktreeRef> = vec![];
    for repo in &base_repos {
        let mut added_primary = false;
        if let Ok(entries) = enumerate_worktrees(&repo.path) {
            for e in entries {
                if !e.path.exists() {
                    continue;
                }
                if e.path == repo.path {
                    added_primary = true;
                }
                worktrees.push(WorktreeRef {
                    repo_name: repo.name.clone(),
                    path: e.path,
                    branch: e.branch,
                    head: e.head,
                });
            }
        }
        if !added_primary {
            worktrees.push(WorktreeRef {
                repo_name: repo.name.clone(),
                path: repo.path.clone(),
                branch: None,
                head: String::new(),
            });
        }
    }
    println!(
        "Tracking {} repos with {} total worktrees",
        base_repos.len(),
        worktrees.len()
    );

    let listeners = scan_listeners().unwrap();
    let attributed = attribute(&listeners, &worktrees);
    let total = attributed.len();
    let matched: Vec<_> = attributed
        .iter()
        .filter(|l| l.repo_name.is_some())
        .collect();
    println!("Total listeners: {total}");
    println!("Attributed: {}", matched.len());
    for a in &matched {
        let cwd = a
            .listener
            .cwd
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
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
        let cwd = a
            .listener
            .cwd
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        println!(
            "  port={:<6} pid={:<6} cmd={:<20} cwd={}",
            a.listener.port, a.listener.pid, a.listener.command_name, cwd
        );
    }
}
