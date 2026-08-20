//! Run one real Refine pass against a real repo and print what it did.
//!
//! A debugging tool, not a product (see CLAUDE.md's examples convention).
//! The GUI's Refine button is one `spawn_backlog_refine` call around
//! `refine_task`, so this is the same pipeline without the window — the way
//! to answer "is the prompt getting a usable answer out of the model today?"
//! or "why did that refine apply nothing?" against a real Backlog project.
//!
//! **This really does spawn `claude` and really does edit the task** (the
//! additive way — see `switchbard_core::refine`'s contract). Point it at a
//! repo you're happy to have a task grow a "Refined by Switchbard" section in.
//!
//! ```sh
//! cargo run -p switchbard-core --example refine_probe -- ~/Dev/switchbard TASK-44
//! ```

use switchbard_core::{describe_refine_result, load_backlog_project, refine_task, RefineOptions};

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(root), Some(task_id)) = (args.next(), args.next()) else {
        eprintln!("usage: refine_probe <repo-root> <task-id>");
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
    let Some(task) = project.tasks.iter().find(|t| t.id == task_id) else {
        eprintln!("no task {task_id} in {}", root.display());
        std::process::exit(1);
    };

    let opts = RefineOptions::default();
    println!(
        "refining {} — \"{}\" (timeout {}s, read-only run at {})",
        task.id,
        task.title,
        opts.timeout.as_secs(),
        root.display()
    );

    match refine_task(&root, task, &opts) {
        Ok(outcome) => {
            println!("log: {}", outcome.log_path.display());
            println!("{}", describe_refine_result(&task.id, &outcome.result));
        }
        Err(e) => {
            eprintln!("refine failed to start: {e}");
            std::process::exit(1);
        }
    }
}
