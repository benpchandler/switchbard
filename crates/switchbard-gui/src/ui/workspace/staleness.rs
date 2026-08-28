//! TASK-41: the Merged/NoUpstream/Live staleness badge, the on-disk size label,
//! and the All/Merged/NoUpstream/Live/Dirty filter chip row.
//!
//! Split out of `mod.rs` on purpose — see `power-of-10-overrides.md`'s "Known
//! debt" note on that file's size; new Workspace work should carve toward
//! smaller modules, not pile onto it further. `Snapshot` (private in `mod.rs`)
//! is still reachable here: Rust visibility lets a descendant module see its
//! ancestor's private items, the same way `tooltips.rs` already does.

use super::Snapshot;
use crate::app::HiveApp;
use crate::runtime::{is_retired_worktree, worktree_is_primary, WorktreeMeta, WorktreeSizeEntry};
use crate::ui::components::{mono_label, status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use std::path::PathBuf;
use switchbard_core::{humanize_size, LandedEvidence, WorktreeStaleness};

/// Which staleness class the Workspace filter chips currently narrow to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StalenessFilter {
    #[default]
    All,
    Merged,
    NoUpstream,
    Live,
    Dirty,
}

impl StalenessFilter {
    pub const ALL: [StalenessFilter; 5] = [
        Self::All,
        Self::Merged,
        Self::NoUpstream,
        Self::Live,
        Self::Dirty,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Merged => "Merged",
            Self::NoUpstream => "No upstream",
            Self::Live => "Live",
            Self::Dirty => "Dirty",
        }
    }
}

/// Does `meta` belong to the current filter? `None` (probe hasn't returned
/// yet) never matches a non-`All` filter — an unclassified worktree should
/// not silently appear under "Merged" once the probe happens to catch up a
/// frame later than the render.
pub(super) fn passes_staleness_filter(
    filter: StalenessFilter,
    meta: Option<&WorktreeMeta>,
) -> bool {
    match filter {
        StalenessFilter::All => true,
        StalenessFilter::Dirty => meta.is_some_and(|m| m.is_dirty() == Some(true)),
        StalenessFilter::Merged => {
            meta.is_some_and(|m| matches!(m.staleness, Some(WorktreeStaleness::Merged { .. })))
        }
        StalenessFilter::NoUpstream => {
            meta.is_some_and(|m| matches!(m.staleness, Some(WorktreeStaleness::NoUpstream)))
        }
        StalenessFilter::Live => {
            meta.is_some_and(|m| matches!(m.staleness, Some(WorktreeStaleness::Live)))
        }
    }
}

/// The staleness badge rendered inline on each worktree row, alongside the
/// existing dirty/drift health pills (`render_health_inline` in `mod.rs`).
pub(super) fn render_staleness_badge(ui: &mut egui::Ui, m: &WorktreeMeta) {
    match &m.staleness {
        None => {
            ui.label(egui::RichText::new("staleness ...").color(theme::weak_text()))
                .on_hover_text("Merged/NoUpstream/Live probe hasn't returned yet");
        }
        // Distinct from `None`: the probe ran and git could not answer. It
        // used to fall through to the no-upstream class, quietly nominating a worktree for
        // retirement on no evidence at all.
        Some(WorktreeStaleness::Unknown) => {
            status_pill(
                ui,
                StatusKind::Warn,
                "staleness ?",
                Some("git couldn't say whether this branch is merged or tracked"),
            );
        }
        Some(WorktreeStaleness::Merged { base, evidence }) => {
            // Same badge either way — the work is in the base and the worktree
            // is a sweep candidate — but the hover distinguishes them, because
            // a rebase-merged branch outlives its worktree: `git branch -d` is
            // ancestry-based and refuses it.
            let tip = match evidence {
                LandedEvidence::Ancestry => format!(
                    "Fully merged into {base} — a candidate for the bulk-remove sweep once clean"
                ),
                LandedEvidence::PatchEquivalent => format!(
                    "Already in {base} under different commits (rebase-merged) — a sweep \
                     candidate once clean, though the branch itself is kept, since \
                     `git branch -d` only looks at reachability"
                ),
            };
            status_pill(ui, StatusKind::Good, "merged", Some(&tip));
        }
        // Deliberately renders nothing. The condition is real, but the
        // remote-drift chip a few pixels to the left already says exactly
        // "no upstream" for it. This badge used to say "orphan" instead,
        // which is how one fact came to have two names on one row; renaming
        // it to match the chip would have printed the same phrase twice.
        Some(WorktreeStaleness::NoUpstream) => {}
        Some(WorktreeStaleness::Live) => {
            status_pill(
                ui,
                StatusKind::Info,
                "live",
                Some("Still ahead of/behind a configured upstream — probably active work"),
            );
        }
    }
}

/// The on-disk size label, sourced from the independently-cadenced `sizes`
/// map (`workers::spawn_size`) rather than `WorktreeMeta` — see that map's
/// doc for why size can't share the git-probe tick.
pub(super) fn render_size_label(ui: &mut egui::Ui, entry: Option<&WorktreeSizeEntry>) {
    match entry {
        None => {
            ui.label(egui::RichText::new("size ...").color(theme::weak_text()))
                .on_hover_text("On-disk size is refreshed lazily in the background (du is slow — see workers::spawn_size)");
        }
        Some(WorktreeSizeEntry { bytes: None, .. }) => {
            ui.label(egui::RichText::new("size ?").color(theme::weak_text()))
                .on_hover_text("`du` failed for this worktree (missing dir, permission error)");
        }
        Some(WorktreeSizeEntry {
            bytes: Some(bytes), ..
        }) => {
            mono_label(ui, &humanize_size(*bytes), None)
                .on_hover_text(format!("{bytes} bytes on disk (du -sk)"));
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct StalenessCounts {
    pub all: usize,
    pub merged: usize,
    pub no_upstream: usize,
    pub live: usize,
    pub dirty: usize,
}

impl StalenessCounts {
    pub(super) fn for_filter(self, filter: StalenessFilter) -> usize {
        match filter {
            StalenessFilter::All => self.all,
            StalenessFilter::Merged => self.merged,
            StalenessFilter::NoUpstream => self.no_upstream,
            StalenessFilter::Live => self.live,
            StalenessFilter::Dirty => self.dirty,
        }
    }
}

/// Counts behind the filter chips + the "Select all merged+clean" button.
/// Primary checkouts are excluded: a repo's own primary is trivially
/// "Merged" (its `HEAD` *is* the default branch) but is never a legal
/// bulk-remove candidate, so counting it would inflate "N retired" with
/// worktrees nobody can actually retire.
pub(super) fn compute_counts(snap: &Snapshot) -> StalenessCounts {
    let mut counts = StalenessCounts::default();
    for w in &snap.worktrees {
        if worktree_is_primary(w, &snap.repos) {
            continue;
        }
        counts.all += 1;
        let Some(m) = snap.meta.get(&w.path) else {
            continue;
        };
        match &m.staleness {
            Some(WorktreeStaleness::Merged { .. }) => counts.merged += 1,
            Some(WorktreeStaleness::NoUpstream) => counts.no_upstream += 1,
            Some(WorktreeStaleness::Live) => counts.live += 1,
            // An unclassifiable worktree is claimed by no chip: putting it
            // under one would assert the very thing the probe couldn't.
            Some(WorktreeStaleness::Unknown) | None => {}
        }
        if m.is_dirty() == Some(true) {
            counts.dirty += 1;
        }
    }
    counts
}

/// Every worktree `is_retired_worktree` would count — what "Select all
/// merged+clean" selects. The shared predicate (`crate::runtime`) is also
/// what the git-probe worker uses to compute the cached top-bar nudge count
/// (`retired_worktree_count`, below), so this list and that count can never
/// disagree about which worktrees qualify.
fn merged_and_clean_paths(snap: &Snapshot) -> Vec<PathBuf> {
    snap.worktrees
        .iter()
        .filter(|w| {
            is_retired_worktree(
                w,
                &snap.repos,
                snap.meta.get(&w.path),
                super::attached_processes(
                    snap,
                    &w.path,
                    snap.listeners_by_wt.get(&w.path).map_or(0, Vec::len),
                ),
            )
        })
        .map(|w| w.path.clone())
        .collect()
}

/// Top-bar nudge count ("N retired worktrees"). Reads a cache written once
/// per git-probe tick by `workers::spawn_probe` (using the same
/// `is_retired_worktree` predicate `merged_and_clean_paths` above uses)
/// rather than recomputing it here — this used to clone `repos`/`worktrees`
/// and lock `meta` on every top-bar frame across every tab, which is wasted
/// work for a number that only actually changes once per probe tick.
pub fn retired_worktree_count(app: &HiveApp) -> usize {
    *app.retired_worktree_count.lock().unwrap()
}

/// The filter-chip row + bulk-selection convenience controls, rendered once
/// below the workspace summary line.
pub(super) fn render_filter_bar(ui: &mut egui::Ui, app: &mut HiveApp, snap: &Snapshot) {
    let counts = compute_counts(snap);
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("staleness:").color(theme::weak_text()));
        for filter in StalenessFilter::ALL {
            let label = format!("{} ({})", filter.label(), counts.for_filter(filter));
            if ui
                .selectable_label(app.staleness_filter == filter, label)
                .clicked()
            {
                app.staleness_filter = filter;
            }
        }
        ui.separator();
        let selectable = merged_and_clean_paths(snap);
        if ui
            .add_enabled(
                !selectable.is_empty(),
                egui::Button::new(format!("Select all merged+clean ({})", selectable.len())),
            )
            .on_hover_text("Select every clean, fully-merged worktree for bulk removal")
            .clicked()
        {
            app.bulk_selected_worktrees = selectable.into_iter().collect();
        }
        if ui
            .add_enabled(
                !app.bulk_selected_worktrees.is_empty(),
                egui::Button::new("Clear selection"),
            )
            .clicked()
        {
            app.bulk_selected_worktrees.clear();
        }
        ui.separator();
        // While a sweep is live the bar takes the button's place rather than
        // sitting beside it — the same rule the Backlog toolbar uses, and for
        // the same reason: offering to start a second removal mid-run is
        // offering a race over the same worktree list.
        //
        // Each removal is its own `git worktree remove`, so a nine-worktree
        // sweep is many seconds during which the dialog has already closed and
        // the rows have not gone yet. Without this the run is
        // indistinguishable from a hang.
        match app.worktree_bulk_progress.snapshot() {
            Some(progress) => {
                ui.add(
                    egui::ProgressBar::new(progress.fraction())
                        .desired_width(220.0)
                        .text(progress.label()),
                )
                .on_hover_text(
                    "Removing worktrees; it is safe to keep working elsewhere in the app",
                );
            }
            None => {
                let n = app.bulk_selected_worktrees.len();
                let remove_label = format!("Remove {n} selected…");
                if ui
                    .add_enabled(n > 0, theme::danger_button(&remove_label))
                    .clicked()
                {
                    app.open_bulk_remove_worktree_confirm();
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::WorktreeMeta;

    fn meta_with_staleness(staleness: Option<WorktreeStaleness>) -> WorktreeMeta {
        WorktreeMeta {
            staleness,
            ..Default::default()
        }
    }

    fn meta_dirty(dirty: bool) -> WorktreeMeta {
        WorktreeMeta {
            dirty_files: Some(if dirty {
                vec![" M src/lib.rs".to_string()]
            } else {
                vec![]
            }),
            ..Default::default()
        }
    }

    #[test]
    fn all_filter_matches_everything_including_unprobed() {
        assert!(passes_staleness_filter(StalenessFilter::All, None));
        assert!(passes_staleness_filter(
            StalenessFilter::All,
            Some(&meta_with_staleness(Some(WorktreeStaleness::Live)))
        ));
    }

    #[test]
    fn merged_filter_only_matches_merged_and_never_unprobed() {
        let merged = meta_with_staleness(Some(WorktreeStaleness::Merged {
            base: "main".into(),
            evidence: LandedEvidence::Ancestry,
        }));
        assert!(passes_staleness_filter(
            StalenessFilter::Merged,
            Some(&merged)
        ));
        assert!(!passes_staleness_filter(StalenessFilter::Merged, None));
        assert!(!passes_staleness_filter(
            StalenessFilter::Merged,
            Some(&meta_with_staleness(Some(WorktreeStaleness::Live)))
        ));
    }

    #[test]
    fn no_upstream_and_live_filters_are_mutually_exclusive() {
        let no_upstream = meta_with_staleness(Some(WorktreeStaleness::NoUpstream));
        let live = meta_with_staleness(Some(WorktreeStaleness::Live));
        assert!(passes_staleness_filter(
            StalenessFilter::NoUpstream,
            Some(&no_upstream)
        ));
        assert!(!passes_staleness_filter(
            StalenessFilter::NoUpstream,
            Some(&live)
        ));
        assert!(passes_staleness_filter(StalenessFilter::Live, Some(&live)));
        assert!(!passes_staleness_filter(
            StalenessFilter::Live,
            Some(&no_upstream)
        ));
    }

    #[test]
    fn dirty_filter_is_orthogonal_to_staleness() {
        // Merged + dirty at once — the filter must key off dirty state only.
        let mut m = meta_with_staleness(Some(WorktreeStaleness::Merged {
            base: "main".into(),
            evidence: LandedEvidence::Ancestry,
        }));
        m.dirty_files = Some(vec!["?? scratch.txt".to_string()]);
        assert!(passes_staleness_filter(StalenessFilter::Dirty, Some(&m)));
        assert!(!passes_staleness_filter(
            StalenessFilter::Dirty,
            Some(&meta_dirty(false))
        ));
        assert!(!passes_staleness_filter(StalenessFilter::Dirty, None));
    }
}

#[cfg(test)]
mod select_all_agrees_with_the_badge {
    use super::*;
    use crate::runtime::{is_retired_worktree, WorktreeMeta};
    use switchbard_core::{AttachedProcesses, Fact, LandedEvidence};

    fn merged_and_clean() -> WorktreeMeta {
        WorktreeMeta {
            dirty_files: Some(vec![]),
            staleness: Some(WorktreeStaleness::Merged {
                base: "main".into(),
                evidence: LandedEvidence::Ancestry,
            }),
            lock: Fact::Known(None),
            ..Default::default()
        }
    }

    fn wt() -> switchbard_core::WorktreeRef {
        switchbard_core::WorktreeRef {
            repo_name: "demo".into(),
            path: PathBuf::from("/repo/wt"),
            branch: Some("feat/x".into()),
            head: "abc1234".into(),
        }
    }

    fn repos() -> Vec<switchbard_core::Repo> {
        vec![switchbard_core::Repo {
            name: "demo".into(),
            path: PathBuf::from("/repo"),
        }]
    }

    #[test]
    fn a_clean_merged_idle_worktree_is_retired() {
        assert!(is_retired_worktree(
            &wt(),
            &repos(),
            Some(&merged_and_clean()),
            AttachedProcesses::default(),
        ));
    }

    /// The reported bug: "Select all merged+clean" picked a worktree whose own
    /// badge read `remove blocked`, because the selector only knew about
    /// merged-ness and dirt while the badge also knows what is running there.
    #[test]
    fn a_worktree_with_something_running_is_not_retired() {
        for attached in [
            AttachedProcesses {
                listeners: 1,
                ..Default::default()
            },
            AttachedProcesses {
                switchbard_runs: 1,
                ..Default::default()
            },
            AttachedProcesses {
                dispatch_runs: 1,
                ..Default::default()
            },
        ] {
            assert!(
                !is_retired_worktree(&wt(), &repos(), Some(&merged_and_clean()), attached),
                "still busy ({attached:?}) — the badge would say blocked, so this must not \
                 be offered for bulk selection"
            );
        }
    }

    /// The other check the old predicate could not see.
    #[test]
    fn a_locked_worktree_is_not_retired() {
        let mut meta = merged_and_clean();
        meta.lock = Fact::Known(Some("rebasing".into()));
        assert!(!is_retired_worktree(
            &wt(),
            &repos(),
            Some(&meta),
            AttachedProcesses::default()
        ));
    }

    #[test]
    fn an_unprobed_worktree_is_never_retired() {
        assert!(!is_retired_worktree(
            &wt(),
            &repos(),
            Some(&WorktreeMeta::default()),
            AttachedProcesses::default()
        ));
        assert!(!is_retired_worktree(
            &wt(),
            &repos(),
            None,
            AttachedProcesses::default()
        ));
    }
}
