//! Tooltip-text builders for the Worktrees view. Pure functions over the
//! probe data so they're easy to unit-test and stay out of the table render
//! path.

use crate::runtime::Activity;
use hive_core::{humanize_age, CommitSummary, DriftDetail};

/// Format the dirty-cell tooltip: "N changed files" header + first ~10 raw
/// porcelain lines verbatim.
pub fn dirty_tooltip(files: &[String]) -> String {
    const SHOW: usize = 10;
    let mut s = format!(
        "{} changed file{}:\n",
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
    for line in files.iter().take(SHOW) {
        s.push_str("  ");
        s.push_str(line);
        s.push('\n');
    }
    if files.len() > SHOW {
        s.push_str(&format!("  … and {} more\n", files.len() - SHOW));
    }
    s.push_str("\nLegend: 'M ' modified, '??' untracked, 'A ' added, ' D' deleted.");
    s
}

pub fn drift_tooltip(
    ahead: u32,
    behind: u32,
    detail: Option<&DriftDetail>,
    fetch_unix: Option<u64>,
) -> String {
    let mut s = format!(
        "{ahead} commit{} ahead of upstream, {behind} behind\n",
        if ahead == 1 { "" } else { "s" }
    );
    s.push_str(&fetch_line(fetch_unix));
    if let Some(d) = detail {
        if !d.ahead.is_empty() {
            s.push_str(&format!(
                "\nAhead{}:\n",
                truncation_suffix(d.ahead.len(), ahead as usize, d.ahead_truncated)
            ));
            for c in &d.ahead {
                s.push_str(&format!("  {}  {}\n", c.short_sha, c.subject));
            }
        }
        if !d.behind.is_empty() {
            s.push_str(&format!(
                "\nBehind{}:\n",
                truncation_suffix(d.behind.len(), behind as usize, d.behind_truncated)
            ));
            for c in &d.behind {
                s.push_str(&format!("  {}  {}\n", c.short_sha, c.subject));
            }
        }
    }
    s
}

pub fn in_sync_tooltip(fetch_unix: Option<u64>) -> String {
    let mut s = String::from("in sync with upstream\n");
    s.push_str(&fetch_line(fetch_unix));
    s.push_str(
        "\nNote: Hive doesn't run `git fetch` — this reflects your local view \
         of origin, not what's actually there right now.",
    );
    s
}

pub fn activity_tooltip(act: &Activity, commits: &[CommitSummary]) -> String {
    let mut s = format!(
        "{} commit{} in the last hour, {} in the last 24h",
        act.count_1h,
        if act.count_1h == 1 { "" } else { "s" },
        act.count_24h,
    );
    if let Some(t) = act.newest_unix {
        s.push_str(&format!("\nNewest: {}", humanize_age(t)));
    }
    if !commits.is_empty() {
        s.push_str("\n\nRecent commits:\n");
        for c in commits.iter().take(5) {
            s.push_str(&format!(
                "  {}  ({})  {}\n",
                c.short_sha,
                humanize_age(c.committed_unix),
                c.subject
            ));
        }
        if commits.len() > 5 {
            s.push_str(&format!("  … and {} more\n", commits.len() - 5));
        }
    }
    s
}

pub fn recent_commits_tooltip(commits: &[CommitSummary]) -> String {
    let mut s = String::from("Recent commits:\n");
    for c in commits.iter().take(5) {
        s.push_str(&format!(
            "  {}  ({})  {}\n",
            c.short_sha,
            humanize_age(c.committed_unix),
            c.subject
        ));
    }
    if commits.len() > 5 {
        s.push_str(&format!("  … and {} more\n", commits.len() - 5));
    }
    s
}

fn fetch_line(fetch_unix: Option<u64>) -> String {
    match fetch_unix {
        Some(t) => format!("Last `git fetch`: {}", humanize_age(t)),
        None => "Last `git fetch`: never (or no remote configured)".to_string(),
    }
}

fn truncation_suffix(shown: usize, total: usize, truncated: bool) -> String {
    if truncated && total > shown {
        format!(" (showing {shown} of {total})")
    } else {
        String::new()
    }
}
