//! TASK-41: the Merged/Orphan/Live staleness badge, the on-disk size label,
//! and the All/Merged/Orphan/Live/Dirty filter chip row.
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
    Orphan,
    Live,
    Dirty,
}

impl StalenessFilter {
    pub const ALL: [StalenessFilter; 5] = [
        Self::All,
        Self::Merged,
        Self::Orphan,
        Self::Live,
        Self::Dirty,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Merged => "Merged",
            Self::Orphan => "Orphan",
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
        StalenessFilter::Orphan => {
            meta.is_some_and(|m| matches!(m.staleness, Some(WorktreeStaleness::Orphan)))
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
                .on_hover_text("Merged/Orphan/Live probe hasn't returned yet");
        }
        // Distinct from `None`: the probe ran and git could not answer. It
        // used to fall through to `orphan`, quietly nominating a worktree for
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
        Some(WorktreeStaleness::Orphan) => {
            status_pill(
                ui,
                StatusKind::Neutral,
                "orphan",
                Some("No upstream configured — nothing is tracking this branch anymore"),
            );
        }
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
    pub orphan: usize,
    pub live: usize,
    pub dirty: usize,
}

impl StalenessCounts {
    pub(super) fn for_filter(self, filter: StalenessFilter) -> usize {
        match filter {
            StalenessFilter::All => self.all,
            StalenessFilter::Merged => self.merged,
            StalenessFilter::Orphan => self.orphan,
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
            Some(WorktreeStaleness::Orphan) => counts.orphan += 1,
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
        .filter(|w| is_retired_worktree(w, &snap.repos, snap.meta.get(&w.path)))
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
        let n = app.bulk_selected_worktrees.len();
        let remove_label = format!("Remove {n} selected…");
        if ui
            .add_enabled(n > 0, theme::danger_button(&remove_label))
            .clicked()
        {
            app.open_bulk_remove_worktree_confirm();
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
    fn orphan_and_live_filters_are_mutually_exclusive() {
        let orphan = meta_with_staleness(Some(WorktreeStaleness::Orphan));
        let live = meta_with_staleness(Some(WorktreeStaleness::Live));
        assert!(passes_staleness_filter(
            StalenessFilter::Orphan,
            Some(&orphan)
        ));
        assert!(!passes_staleness_filter(
            StalenessFilter::Orphan,
            Some(&live)
        ));
        assert!(passes_staleness_filter(StalenessFilter::Live, Some(&live)));
        assert!(!passes_staleness_filter(
            StalenessFilter::Live,
            Some(&orphan)
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
