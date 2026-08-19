//! Print what `dispatch_inspect` derives for every dispatch-labeled task in a
//! repo, plus the classification the Dispatch view will give it.
//!
//! A debugging tool, not a product (see CLAUDE.md's examples convention):
//! answers "why is the Dispatch view showing this run that way?" against a real
//! repo instead of a fixture.
//!
//! ```sh
//! cargo run -p switchbard-core --example dispatch_probe -- ~/Dev/MusicProduction
//! ```

use switchbard_core::dispatch_inspect::{inspect_dispatch_run, now_unix};
use switchbard_core::{
    load_backlog_project, DispatchOptions, DISPATCHED_LABEL, DISPATCHING_LABEL,
    DISPATCH_FAILED_LABEL, DISPATCH_LABEL,
};

fn main() {
    let Some(root) = std::env::args().nth(1) else {
        eprintln!("usage: dispatch_probe <repo-root>");
        std::process::exit(2);
    };
    let root = std::path::PathBuf::from(root);

    let project = match load_backlog_project(&root) {
        Ok(project) => project,
        Err(e) => {
            eprintln!("failed to load backlog project at {}: {e}", root.display());
            std::process::exit(1);
        }
    };

    let now = now_unix();
    let timeout = DispatchOptions::default().timeout;
    let mut found = 0;

    for task in &project.tasks {
        let labels: Vec<&str> = task
            .labels
            .iter()
            .map(String::as_str)
            .filter(|l| {
                matches!(
                    *l,
                    DISPATCH_LABEL | DISPATCHING_LABEL | DISPATCHED_LABEL | DISPATCH_FAILED_LABEL
                )
            })
            .collect();
        if labels.is_empty() {
            continue;
        }
        found += 1;

        let run = inspect_dispatch_run(&root, &task.id);
        let elapsed = run
            .elapsed(now)
            .map(|d| format!("{}s", d.as_secs()))
            .unwrap_or_else(|| "-".to_string());

        println!("{} — {}", task.id, task.title);
        println!("  status        {}", task.status);
        println!("  dispatch tag  {}", labels.join(", "));
        println!("  branch        {}", run.branch);
        println!(
            "  worktree      {} ({})",
            run.worktree_path.display(),
            if run.worktree_exists {
                "exists"
            } else {
                "gone"
            }
        );
        match &run.log_path {
            Some(path) => println!(
                "  log           {} ({} bytes)",
                path.display(),
                run.log_bytes
            ),
            None => println!("  log           (none — never started)"),
        }
        println!("  elapsed       {elapsed}");
        println!("  past timeout  {}", run.looks_stalled(now, timeout));
        println!("  agent output  {}", run.log_has_output());

        let still_claimed = task.labels.iter().any(|l| l == DISPATCHING_LABEL);
        let orphaned = run.looks_orphaned(now, still_claimed);
        println!("  ORPHANED      {orphaned}");
        println!(
            "  view section  {}",
            if orphaned {
                "Finished, never released"
            } else if still_claimed {
                "In flight"
            } else {
                "(see dispatch label)"
            }
        );
        println!();
    }

    if found == 0 {
        println!("no dispatch-labeled tasks in {}", root.display());
    }
}
