//! Tooltip-text builders for the Worktrees view. Pure functions over the
//! probe data so they're easy to unit-test and stay out of the table render
//! path.

use crate::runtime::Activity;
use switchbard_core::{
    humanize_age, CommitSummary, DriftDetail, DriftProbe, TrunkDetail, TrunkDivergence,
};

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

/// The row's answer to "what would I lose if this branch went away".
///
/// Every commit the user might expect to see is accounted for: the ones at
/// risk are listed, and the ones already upstream under a different SHA are
/// counted rather than silently dropped. Without that line a rebase-merged
/// branch reads as "0 unlanded" while `git log` shows 11 commits ahead, and
/// the row looks like it is hiding something.
pub fn trunk_tooltip(divergence: &TrunkDivergence, detail: Option<&TrunkDetail>) -> String {
    let mut s = format!(
        "Measured against `{}` — the same comparison the removal checks use.\n{} unlanded, {} behind\n",
        divergence.base, divergence.unlanded, divergence.behind
    );
    let Some(d) = detail else {
        return s;
    };
    if !d.unlanded.is_empty() {
        s.push_str(&format!(
            "\nAt risk if the branch goes{}:\n",
            truncation_suffix(
                d.unlanded.len(),
                divergence.unlanded as usize,
                d.unlanded_truncated
            )
        ));
        for c in &d.unlanded {
            s.push_str(&format!("  {}  {}\n", c.short_sha, c.subject));
        }
    }
    if d.already_upstream > 0 {
        s.push_str(&format!(
            "\n{} further commit{} already upstream under a different SHA (rebase-merged) — not at risk.\n",
            d.already_upstream,
            if d.already_upstream == 1 { " is" } else { "s are" }
        ));
    }
    if !d.behind.is_empty() {
        s.push_str(&format!(
            "\nBehind{}:\n",
            truncation_suffix(
                d.behind.len(),
                divergence.behind as usize,
                d.behind_truncated
            )
        ));
        for c in &d.behind {
            s.push_str(&format!("  {}  {}\n", c.short_sha, c.subject));
        }
    }
    s
}

pub fn ref_drift_tooltip(
    label: &str,
    probe: &DriftProbe,
    detail: Option<&DriftDetail>,
    fetch_unix: Option<u64>,
) -> String {
    match probe {
        DriftProbe::Ready {
            base,
            ahead,
            behind,
        } => {
            let mut s = format!(
                "{label} comparison against `{base}`\n{ahead} commit{} ahead, {behind} behind\n",
                if *ahead == 1 { "" } else { "s" }
            );
            if label == "remote" {
                s.push_str(&fetch_line(fetch_unix));
                s.push_str(
                    "\nNote: Switchbard doesn't run `git fetch`; remote state uses local remote-tracking refs.",
                );
            }
            append_drift_detail(&mut s, *ahead, *behind, detail);
            s
        }
        DriftProbe::MissingBase { base } => {
            format!("Cannot compare {label}: `{base}` does not exist locally.")
        }
        DriftProbe::NoUpstream => "No upstream remote is configured for this branch.".to_string(),
    }
}

pub fn in_sync_tooltip(fetch_unix: Option<u64>) -> String {
    let mut s = String::from("in sync with upstream\n");
    s.push_str(&fetch_line(fetch_unix));
    s.push_str(
        "\nNote: Switchbard doesn't run `git fetch` — this reflects your local view \
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

fn append_drift_detail(s: &mut String, ahead: u32, behind: u32, detail: Option<&DriftDetail>) {
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
}

fn truncation_suffix(shown: usize, total: usize, truncated: bool) -> String {
    if truncated && total > shown {
        format!(" (showing {shown} of {total})")
    } else {
        String::new()
    }
}
