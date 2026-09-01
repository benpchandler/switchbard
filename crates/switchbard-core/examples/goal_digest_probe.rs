//! Probe: replicate the Digest place's goal-card data path for one repo.
fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: goal_digest_probe <repo-root>"));
    let today = chrono::Local::now().date_naive();
    let week = switchbard_core::week_monday_of(today).format("%Y-%m-%d").to_string();
    println!("today={today} week_key={week}");
    match switchbard_core::load_backlog_repo(&root) {
        Ok(repo) => {
            println!("loaded: {} goals, {} tasks", repo.goals.len(), repo.tasks.len());
            let statuses = switchbard_core::compute_goal_statuses(&[&repo], &week, today);
            println!("statuses: {}", statuses.len());
            for s in &statuses {
                println!("  {} {}/{} pace={:?} days_elapsed={}", s.name, s.actual, s.target, s.pace, s.days_elapsed);
            }
        }
        Err(e) => println!("load failed: {e}"),
    }
}
