//! TASK-100 medic pass: the Git column's single compact status chip.
//!
//! The original merged-table Git cell rendered up to five separate
//! fragments inline — a dirty pill, a `+N/-M` trunk chip, a remote-drift
//! chip, a staleness badge (which could itself render the bare words
//! `staleness ...`), and a size label (`size ...`). The 2026-09-01
//! screenshot review of `docs/qa/screenshots/ops_table_populated_light.png`
//! called this out as "cluttered truncated text" against the frozen mock
//! (§6 of `switchbard-ia-places.html`), which shows exactly one clean chip
//! per row — `dirty · ahead 1`, `ahead 2`, `clean` — with everything else
//! reachable on hover.
//!
//! `compute_git_chip` is the pure decision function (unit-tested directly,
//! no `Harness` needed); `render_git_chip` is the thin egui wrapper
//! `row.rs` calls. Splitting the two keeps the priority logic — which of
//! five independently-probed facts wins the one visible label — honest and
//! easy to regression-test without standing up a table row for every case.

use eframe::egui;

use crate::runtime::{WorktreeMeta, WorktreeSizeEntry};
use crate::ui::components::{status_pill, StatusKind};
use switchbard_core::{DriftProbe, TrunkDivergence, WorktreeStaleness};

use super::tooltips;

/// One row's Git-column verdict: a short label for the always-visible chip,
/// plus the longer multi-section hover detail that used to be five
/// always-visible fragments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GitChip {
    pub kind: StatusKind,
    pub label: String,
    pub detail: String,
}

pub(super) fn compute_git_chip(m: &WorktreeMeta, size: Option<&WorktreeSizeEntry>) -> GitChip {
    let (kind, label) = chip_label_and_kind(m);
    let detail = [
        dirty_section(m),
        trunk_section(m),
        drift_section(m),
        tooltips::staleness_tooltip(m.staleness.as_ref()),
        tooltips::size_tooltip(size),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n");
    GitChip {
        kind,
        label,
        detail,
    }
}

pub(super) fn render_git_chip(ui: &mut egui::Ui, chip: &GitChip) {
    let resp = status_pill(ui, chip.kind, chip.label.clone(), None);
    if !chip.detail.is_empty() {
        resp.on_hover_text(&chip.detail);
    }
}

/// `ahead N`, `behind M`, `ahead N / behind M`, or `None` when the worktree
/// holds nothing the trunk lacks and isn't behind it either.
fn trunk_ahead_behind_text(t: &TrunkDivergence) -> Option<String> {
    match (t.unlanded, t.behind) {
        (0, 0) => None,
        (a, 0) => Some(format!("ahead {a}")),
        (0, b) => Some(format!("behind {b}")),
        (a, b) => Some(format!("ahead {a} / behind {b}")),
    }
}

/// The dominance order a single chip has to pick between: dirty beats
/// everything (it's the one fact that blocks a clean removal outright), an
/// unclassifiable staleness probe and a missing upstream are both worth
/// flagging over a plain "clean", trunk divergence is the same fact the
/// removal badge already keys off of, remote drift is the same shape one
/// level up (comparison is the branch's own upstream, not the repo's
/// trunk), and `merged`/`clean` are the two flavors of "nothing to see
/// here".
fn chip_label_and_kind(m: &WorktreeMeta) -> (StatusKind, String) {
    let trunk_text = m.trunk.as_ref().and_then(trunk_ahead_behind_text);
    match m.is_dirty() {
        None => (StatusKind::Neutral, "…".to_string()),
        Some(true) => {
            let label = match &trunk_text {
                Some(t) => format!("dirty · {t}"),
                None => "dirty".to_string(),
            };
            (StatusKind::Warn, label)
        }
        Some(false) => {
            if matches!(m.staleness, Some(WorktreeStaleness::Unknown)) {
                return (StatusKind::Warn, "staleness ?".to_string());
            }
            if matches!(m.remote_drift, Some(DriftProbe::NoUpstream)) {
                return (StatusKind::Warn, "no upstream".to_string());
            }
            if let Some(t) = trunk_text {
                return (StatusKind::Info, t);
            }
            if let Some(DriftProbe::Ready { ahead, behind, .. }) = &m.remote_drift {
                if ahead + behind > 0 {
                    return (StatusKind::Info, format!("remote +{ahead}/-{behind}"));
                }
            }
            if matches!(m.staleness, Some(WorktreeStaleness::Merged { .. })) {
                return (StatusKind::Good, "merged".to_string());
            }
            (StatusKind::Good, "clean".to_string())
        }
    }
}

fn dirty_section(m: &WorktreeMeta) -> String {
    match m.is_dirty() {
        None => "Dirty probe pending or failed".to_string(),
        Some(true) => tooltips::dirty_tooltip(m.dirty_files.as_deref().unwrap_or(&[])),
        Some(false) => "No uncommitted changes".to_string(),
    }
}

fn trunk_section(m: &WorktreeMeta) -> String {
    let Some(t) = &m.trunk else {
        return String::new();
    };
    if t.unlanded + t.behind == 0 {
        return String::new();
    }
    tooltips::trunk_tooltip(t, m.trunk_detail.as_ref())
}

fn drift_section(m: &WorktreeMeta) -> String {
    match &m.remote_drift {
        Some(probe @ DriftProbe::Ready { ahead, behind, .. }) if ahead + behind > 0 => {
            tooltips::ref_drift_tooltip(
                "remote",
                probe,
                m.remote_drift_detail.as_ref(),
                m.fetch_unix,
            )
        }
        Some(probe @ (DriftProbe::NoUpstream | DriftProbe::MissingBase { .. })) => {
            tooltips::ref_drift_tooltip("remote", probe, None, m.fetch_unix)
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchbard_core::LandedEvidence;

    fn clean() -> WorktreeMeta {
        WorktreeMeta {
            dirty_files: Some(vec![]),
            ..Default::default()
        }
    }

    fn trunk(unlanded: u32, behind: u32) -> TrunkDivergence {
        TrunkDivergence {
            base: "origin/main".into(),
            unlanded,
            ancestry_ahead: unlanded,
            behind,
        }
    }

    #[test]
    fn probe_pending_renders_a_bare_ellipsis_not_a_worded_fragment() {
        let (kind, label) = chip_label_and_kind(&WorktreeMeta::default());
        assert_eq!(label, "…");
        assert_eq!(kind, StatusKind::Neutral);
    }

    #[test]
    fn dirty_with_unlanded_work_folds_the_trunk_fragment_into_one_chip() {
        let m = WorktreeMeta {
            dirty_files: Some(vec![" M src/lib.rs".into()]),
            trunk: Some(trunk(1, 0)),
            ..Default::default()
        };
        let (kind, label) = chip_label_and_kind(&m);
        assert_eq!(label, "dirty · ahead 1");
        assert_eq!(kind, StatusKind::Warn);
    }

    #[test]
    fn dirty_alone_has_no_trailing_separator() {
        let m = WorktreeMeta {
            dirty_files: Some(vec!["?? scratch.txt".into()]),
            ..Default::default()
        };
        let (_, label) = chip_label_and_kind(&m);
        assert_eq!(label, "dirty");
    }

    #[test]
    fn unclassifiable_staleness_beats_a_bare_clean() {
        let m = WorktreeMeta {
            staleness: Some(WorktreeStaleness::Unknown),
            ..clean()
        };
        let (kind, label) = chip_label_and_kind(&m);
        assert_eq!(label, "staleness ?");
        assert_eq!(kind, StatusKind::Warn);
    }

    #[test]
    fn no_upstream_is_flagged_even_when_clean() {
        let m = WorktreeMeta {
            remote_drift: Some(DriftProbe::NoUpstream),
            staleness: Some(WorktreeStaleness::NoUpstream),
            ..clean()
        };
        let (kind, label) = chip_label_and_kind(&m);
        assert_eq!(label, "no upstream");
        assert_eq!(kind, StatusKind::Warn);
    }

    #[test]
    fn trunk_divergence_wins_over_a_plain_clean() {
        let m = WorktreeMeta {
            trunk: Some(trunk(2, 1)),
            ..clean()
        };
        let (kind, label) = chip_label_and_kind(&m);
        assert_eq!(label, "ahead 2 / behind 1");
        assert_eq!(kind, StatusKind::Info);
    }

    #[test]
    fn behind_only_reads_behind_not_ahead_zero() {
        let m = WorktreeMeta {
            trunk: Some(trunk(0, 3)),
            ..clean()
        };
        let (_, label) = chip_label_and_kind(&m);
        assert_eq!(label, "behind 3");
    }

    #[test]
    fn remote_drift_is_the_fallback_when_trunk_is_flat() {
        let m = WorktreeMeta {
            remote_drift: Some(DriftProbe::Ready {
                base: "origin/feat".into(),
                ahead: 2,
                behind: 0,
            }),
            ..clean()
        };
        let (kind, label) = chip_label_and_kind(&m);
        assert_eq!(label, "remote +2/-0");
        assert_eq!(kind, StatusKind::Info);
    }

    #[test]
    fn merged_renders_once_nothing_more_urgent_is_true() {
        let m = WorktreeMeta {
            staleness: Some(WorktreeStaleness::Merged {
                base: "main".into(),
                evidence: LandedEvidence::Ancestry,
            }),
            ..clean()
        };
        let (kind, label) = chip_label_and_kind(&m);
        assert_eq!(label, "merged");
        assert_eq!(kind, StatusKind::Good);
    }

    #[test]
    fn nothing_notable_falls_all_the_way_to_clean() {
        let (kind, label) = chip_label_and_kind(&clean());
        assert_eq!(label, "clean");
        assert_eq!(kind, StatusKind::Good);
    }

    /// The exact regression this module exists for: the retired Git cell
    /// rendered `staleness ...` and `size ...` as their own always-visible
    /// fragments. Both must now live only in the hover detail's prose, never
    /// as the bare, wordless-looking ellipsis fragments the screenshot
    /// review flagged.
    #[test]
    fn the_chip_label_never_contains_the_retired_bare_ellipsis_fragments() {
        for m in [
            WorktreeMeta::default(),
            clean(),
            WorktreeMeta {
                staleness: Some(WorktreeStaleness::Unknown),
                ..clean()
            },
        ] {
            let (_, label) = chip_label_and_kind(&m);
            assert!(!label.contains("staleness ..."), "got {label:?}");
            assert!(!label.contains("size ..."), "got {label:?}");
        }
    }

    #[test]
    fn compute_git_chip_carries_dirty_file_detail_into_the_tooltip_not_the_label() {
        let m = WorktreeMeta {
            dirty_files: Some(vec![" M src/lib.rs".into(), "?? new.txt".into()]),
            ..Default::default()
        };
        let chip = compute_git_chip(&m, None);
        assert_eq!(chip.label, "dirty");
        assert!(chip.detail.contains("src/lib.rs"));
        assert!(chip.detail.contains("new.txt"));
    }
}
