//! Debugging tool: why does a worktree read one way on the Workspace row and
//! another way in a removal dialog?
//!
//! The two surfaces source the same `landed` fact differently — the row reuses
//! the Merged/Orphan/Live badge's probe, the dialogs call `probe_facts` — and
//! this prints both side by side so a disagreement is visible rather than
//! inferred. `a_detached_worktree_parked_on_main_is_landed_not_unprovable` in
//! `removal_safety.rs` pins the case that motivated it.
//!
//! ```sh
//! cargo run -p switchbard-core --example removal_check -- <repo> <worktree> [branch]
//! ```
use switchbard_core::git_probe::probe_worktree_staleness;
use switchbard_core::removal_safety::probe_facts;
use switchbard_core::{AttachedProcesses, Fact, RemovalIntent, RemovalSafety};

fn main() {
    let mut args = std::env::args().skip(1);
    let repo = std::path::PathBuf::from(args.next().expect("usage: <repo> <worktree> [branch]"));
    let worktree =
        std::path::PathBuf::from(args.next().expect("usage: <repo> <worktree> [branch]"));
    let branch = args.next();

    println!("repo:     {}", repo.display());
    println!("worktree: {}", worktree.display());
    println!("branch:   {branch:?}\n");

    // What the Workspace row's badge sees.
    println!(
        "row badge (probe_worktree_staleness): {:?}\n",
        probe_worktree_staleness(&repo, &worktree)
    );

    // What the removal dialogs see.
    let facts = probe_facts(
        &repo,
        &worktree,
        branch.as_deref(),
        Fact::Known(AttachedProcesses::default()),
    );
    println!("probe_facts: {facts:#?}\n");

    for intent in [
        RemovalIntent::WorktreeOnly,
        RemovalIntent::WorktreeAndBranch,
    ] {
        let safety = RemovalSafety::evaluate(&facts, intent);
        println!("=== {intent:?} => {:?}", safety.verdict());
        println!("{}", safety.tooltip());
        println!("blocking_reason: {:?}\n", safety.blocking_reason());
    }
}
