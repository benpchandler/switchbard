//! `switchbard-dispatch` — headless drain of the dispatch queue, no GUI.
//!
//! Reads the same `~/.switchbard/config.toml` the GUI writes and, for every
//! tracked repo that's a Backlog project, drains up to
//! `SWITCHBARD_DISPATCH_MAX_CONCURRENT` queued tasks via
//! `switchbard_core::dispatch`. One run does one drain pass per repo, then
//! exits — this is meant to be invoked periodically by launchd (see
//! `assets/launchd/com.switchbard.dispatch.plist`), not kept resident. See
//! `docs/product-trajectory.md`'s "Owner-scoped exception" note for why a
//! scheduled batch process doesn't violate the local-first "no daemon"
//! stance, and `docs/INSTALL-DISPATCH.md` for how to wire the plist up
//! (deliberately not done by this crate itself).
//!
//! Requires `git`, `gh`, and `claude` on `$PATH`, and `gh auth status` to
//! already be logged in — this binary does no auth setup of its own.

use std::time::Duration;
use switchbard_core::config;
use switchbard_core::dispatch::{drain_dispatch_queue, DispatchOptions, DispatchResult};
use switchbard_core::{is_backlog_project, load_backlog_project};

fn main() {
    let cfg = config::load();
    let opts = dispatch_options_from_env();

    let mut total = 0usize;
    let mut opened = 0usize;
    let mut failed = 0usize;

    for repo in &cfg.repos {
        if !is_backlog_project(&repo.path) {
            continue;
        }
        let project = match load_backlog_project(&repo.path) {
            Ok(project) => project,
            Err(e) => {
                eprintln!(
                    "switchbard-dispatch: {}: failed to load backlog project: {e}",
                    repo.name
                );
                continue;
            }
        };
        for outcome in drain_dispatch_queue(&repo.path, &project, &opts) {
            total += 1;
            match &outcome.result {
                DispatchResult::PrOpened { url } => {
                    opened += 1;
                    println!(
                        "switchbard-dispatch: {} [{}] -> {url}",
                        repo.name, outcome.task_id
                    );
                }
                other => {
                    failed += 1;
                    eprintln!(
                        "switchbard-dispatch: {} [{}] failed: {other:?} (log: {})",
                        repo.name,
                        outcome.task_id,
                        outcome.log_path.display()
                    );
                }
            }
        }
    }

    println!(
        "switchbard-dispatch: drained {total} task(s), {opened} PR(s) opened, {failed} failed"
    );
}

/// `DispatchOptions::default()` overridden by any `SWITCHBARD_DISPATCH_*`
/// environment variables that are set — the only configuration surface this
/// binary has (see the module doc: no CLI flags, no second config file).
/// Unparseable numeric overrides are ignored rather than treated as a fatal
/// error, so a typo'd launchd `EnvironmentVariables` entry degrades to the
/// default instead of silently skipping every repo.
fn dispatch_options_from_env() -> DispatchOptions {
    let mut opts = DispatchOptions::default();
    if let Ok(base) = std::env::var("SWITCHBARD_DISPATCH_BASE_BRANCH") {
        opts.base_branch = base;
    }
    if let Ok(bin) = std::env::var("SWITCHBARD_DISPATCH_CLAUDE_BIN") {
        opts.claude_binary = bin;
    }
    if let Ok(remote) = std::env::var("SWITCHBARD_DISPATCH_REMOTE") {
        opts.remote = remote;
    }
    if let Ok(max) = std::env::var("SWITCHBARD_DISPATCH_MAX_CONCURRENT") {
        if let Ok(n) = max.parse::<usize>() {
            opts.max_concurrent = n;
        }
    }
    if let Ok(secs) = std::env::var("SWITCHBARD_DISPATCH_TIMEOUT_SECS") {
        if let Ok(n) = secs.parse::<u64>() {
            opts.timeout = Duration::from_secs(n);
        }
    }
    opts
}
