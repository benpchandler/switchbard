//! Run the real "safe to remove" rules over your actual tracked repos and
//! print what each worktree's badge will say, and why.
//!
//! This is the ground-truth check on `removal_safety`: the GUI row, the bulk
//! sweep, and the confirm dialog all evaluate exactly what this prints, so a
//! verdict here that looks wrong is a rule that is wrong, not a rendering
//! bug. It reads git and nothing else - no removals, no writes.
//!
//! Process attribution is deliberately reported as `Pending` rather than
//! guessed at: listeners, started services, and dispatch runs live in the
//! GUI's scanner state, not in git. That means every worktree here shows the
//! `NoProcesses` check unresolved, which is the honest answer for a tool that
//! cannot see them - and it doubles as a demonstration that an unresolved
//! check yields `Checking`, never `Safe`.
//!
//! Usage:
//!   cargo run -p switchbard-core --example removal_safety_audit -- /path/to/repo [...]

use std::path::PathBuf;

use switchbard_core::{
    enumerate_worktrees, probe_facts, worktree_remove::assess_branch_delete, CheckOutcome, Fact,
    RemovalIntent, RemovalSafety, RemovalVerdict,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "usage: cargo run -p switchbard-core --example removal_safety_audit -- \
             /path/to/repo [/path/to/repo ...]"
        );
        std::process::exit(1);
    }

    let mut tally = [0usize; 4];
    for arg in args {
        let repo = PathBuf::from(&arg);
        let Ok(entries) = enumerate_worktrees(&repo) else {
            println!("{arg}: not a git repo (or git failed)\n");
            continue;
        };
        println!("── {arg}");
        for entry in entries {
            if !entry.path.exists() {
                continue;
            }
            let facts = probe_facts(
                &repo,
                &entry.path,
                entry.branch.as_deref(),
                // See the module doc: git cannot answer this one.
                Fact::Pending,
            );
            let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
            tally[verdict_index(safety.verdict())] += 1;

            let name = entry
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            println!(
                "  {:<28} {:<14} [{}]",
                name,
                safety.headline(),
                entry.branch.as_deref().unwrap_or("detached")
            );
            for check in safety.checks() {
                // Passing checks are the boring majority; print only what the
                // user would actually act on, plus anything unresolved.
                if check.outcome != CheckOutcome::Pass {
                    println!("      {} {}", check.outcome.marker(), check.detail);
                }
            }
            // The one question the safety rules deliberately do not answer:
            // whether git would even accept `branch -d`. Not a safety check
            // (a branch left behind loses nothing), but worth surfacing here
            // so the audit explains the whole dialog.
            if let Some(branch) = &entry.branch {
                let assessment = assess_branch_delete(&repo, branch, &entry.path);
                if assessment.is_blocked() {
                    println!(
                        "      (branch also checked out elsewhere - `branch -d` would refuse)"
                    );
                }
            }
        }
        println!();
    }

    // `Safe` is unreachable from this tool by construction - `NoProcesses`
    // is always unresolved here - so reporting a "safe 0" would read as a
    // finding rather than as the tool's own blind spot. The `Checking` bucket
    // is the useful number: those worktrees cleared every check git can
    // answer, and are exactly the ones the GUI will paint "remove ok" once it
    // supplies the process count.
    let summary = format!(
        "{} would be removable (cleared every git-answerable check) · {} blocked · {} primary",
        tally[2], tally[1], tally[3]
    );
    println!("{summary}");
    if tally[0] > 0 {
        println!(
            "warning: {} worktree(s) reported Safe, but this tool never supplies              a process count - that should be impossible",
            tally[0]
        );
    }
}

fn verdict_index(verdict: RemovalVerdict) -> usize {
    match verdict {
        RemovalVerdict::Safe => 0,
        RemovalVerdict::Blocked => 1,
        RemovalVerdict::Checking => 2,
        RemovalVerdict::Primary => 3,
    }
}
