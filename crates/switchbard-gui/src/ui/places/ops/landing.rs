//! feat/landing-stage: the landing-stage chip rendered in the worktree row's
//! trailing cluster, right where the head SHA used to be — see commit
//! 8487798 ("refactor(workspace): drop the head SHA...") for why that slot
//! was cleared for exactly this.
//!
//! Split out of `mod.rs` on purpose, following `staleness.rs`'s own
//! precedent (see that module's header doc and `power-of-10-overrides.md`'s
//! "known debt" note on `mod.rs`'s size) — new Workspace work should carve
//! toward smaller modules, not pile onto it further.
//!
//! [`landing_chip`] is the pure "what does this row's chip say" mapper, kept
//! free of `egui` so it is unit-testable without a `Ui` or a kittest harness
//! — the same shape `staleness::passes_staleness_filter` already uses.
//! [`render_landing_chip`] is the thin paint step on top of it and does
//! nothing else.

use crate::runtime::LandingEntry;
use crate::ui::components::{status_pill, StatusKind};
use crate::ui::theme;
use eframe::egui;
use switchbard_core::LandingStage;

/// Semantic color family for a rendered chip — a private, `PartialEq`-able
/// mirror of [`StatusKind`] (which isn't `PartialEq`, since none of its
/// existing call sites needed to compare one) so [`landing_chip`]'s output
/// stays assertable in tests without touching `status_pill.rs` for a
/// two-file feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LandingChipTone {
    Good,
    Warn,
    Info,
    Danger,
}

impl LandingChipTone {
    fn status_kind(self) -> StatusKind {
        match self {
            Self::Good => StatusKind::Good,
            Self::Warn => StatusKind::Warn,
            Self::Info => StatusKind::Info,
            Self::Danger => StatusKind::Danger,
        }
    }
}

/// What the landing-stage chip shows for one worktree — the full state
/// matrix (see the design-state review this module was written against, and
/// the post-review audit finding that added the detached-HEAD case below):
///
/// - **`None`** (render nothing): no unlanded work at all —
///   `LandingStage::Landed`'s own doc: "the stage question does not arise."
///   This is the *only* silent case. A detached HEAD with unlanded work is
///   not silent — see `Chip` below — because every other signal on this same
///   row (the lavender unlanded dot, the trunk chip, `is_noteworthy`'s
///   auto-expand, and `removal_safety`'s removal-blocking check) already
///   treats it as the row's most attention-worthy state, not a non-question.
/// - **`Pending`**: unlanded work exists, there is a branch to probe, but
///   the background worker hasn't reached this worktree yet (bounded batch,
///   own cadence — see `workers::spawn_landing`).
/// - **`Chip`**: a terminal answer, one of:
///   - one of `LandingStage`'s five "real" variants, or its explicit
///     unknown, once the worker has probed a branched worktree; or
///   - **detached HEAD with unlanded work** — no branch means no push and no
///     PR are possible, ever, so this needs no worker probe at all: it is
///     answerable immediately and permanently from `w.branch` alone. A real
///     stall (nothing can land this way until the worktree gets a branch),
///     rendered distinctly from both `Pending` (a probe that just hasn't run
///     yet) and `PrStateUnknown` (a probe that ran and failed) — this is
///     neither: it is a question with a known, permanent answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LandingChipView {
    Pending,
    Chip {
        text: String,
        tone: LandingChipTone,
        tooltip: String,
    },
}

/// Map this row's inputs to what its chip should show. Pure — no `egui`, so
/// this is exercised directly by this module's tests instead of through a
/// kittest harness.
pub(super) fn landing_chip(
    has_unlanded: bool,
    has_branch: bool,
    entry: Option<&LandingEntry>,
) -> Option<LandingChipView> {
    if !has_unlanded {
        return None;
    }
    if !has_branch {
        // No worker probe needed or possible: a detached HEAD can never be
        // pushed or opened as a PR, so this is a permanent, immediately
        // knowable answer, not a pending question — see `LandingChipView`'s
        // doc for why this must not render as blank, `Pending`, or
        // `PrStateUnknown`. `removal_safety.rs` independently treats this
        // exact condition as the one that must block a removal (dropping
        // the only ref reaching those commits); the chip agrees rather than
        // contradicting that badge by staying silent.
        return Some(LandingChipView::Chip {
            text: "detached".to_string(),
            tone: LandingChipTone::Warn,
            tooltip: "Unlanded commits on a detached HEAD — there is no branch to push or open \
                      a pull request from, so this work cannot land by any push/PR path until \
                      it is given one."
                .to_string(),
        });
    }
    let Some(entry) = entry else {
        return Some(LandingChipView::Pending);
    };
    match &entry.stage {
        // Only reachable if the cache is stale relative to a fresher `0`
        // unlanded count the caller hasn't yet observed — `has_unlanded`
        // already gates this out in the normal case. Render nothing rather
        // than a chip contradicting the row's own trunk count.
        LandingStage::Landed => None,
        LandingStage::Unpushed => Some(LandingChipView::Chip {
            text: "unpushed".to_string(),
            tone: LandingChipTone::Warn,
            tooltip: "Committed locally, never pushed — nothing to review yet.".to_string(),
        }),
        LandingStage::PushedNoPr => Some(LandingChipView::Chip {
            text: "no PR".to_string(),
            tone: LandingChipTone::Warn,
            tooltip: "Pushed to origin, but no pull request has ever been opened for this branch."
                .to_string(),
        }),
        LandingStage::InReview { number, url } => Some(LandingChipView::Chip {
            text: format!("PR #{number}"),
            tone: LandingChipTone::Info,
            tooltip: format!("In review: {url}"),
        }),
        LandingStage::Rejected { number, url } => Some(LandingChipView::Chip {
            text: format!("PR #{number} closed"),
            tone: LandingChipTone::Danger,
            tooltip: format!("Closed without merging — reopening would undo that decision: {url}"),
        }),
        LandingStage::MergedButUnlanded { number, url } => Some(LandingChipView::Chip {
            text: format!("PR #{number} merged"),
            tone: LandingChipTone::Good,
            tooltip: format!(
                "Merged on GitHub — the unlanded count here is a known squash-merge false \
                 negative (patch-id based); this branch is actually done: {url}"
            ),
        }),
        LandingStage::PrStateUnknown { pushed, why } => Some(LandingChipView::Chip {
            text: if *pushed {
                "PR state ?".to_string()
            } else {
                "landing ?".to_string()
            },
            tone: LandingChipTone::Warn,
            tooltip: format!("Couldn't ask GitHub: {why}"),
        }),
    }
}

/// Paint whatever [`landing_chip`] decided this row should show. Does not
/// render anything for `None` — the caller passes that straight through, and
/// the only state that reaches it is "no unlanded work" (a detached HEAD
/// with unlanded work is `Chip { text: "detached", .. }`, not `None` — see
/// `LandingChipView`'s doc), same as the head SHA's old spot leaving nothing
/// behind for a primary worktree.
pub(super) fn render_landing_chip(ui: &mut egui::Ui, view: Option<LandingChipView>) {
    match view {
        None => {}
        Some(LandingChipView::Pending) => {
            ui.label(egui::RichText::new("landing ...").color(theme::weak_text()))
                .on_hover_text(
                    "Push/PR state hasn't been checked yet — refreshed in the background, \
                     a bounded batch every few minutes (workers::spawn_landing)",
                );
        }
        Some(LandingChipView::Chip {
            text,
            tone,
            tooltip,
        }) => {
            status_pill(ui, tone.status_kind(), text, Some(&tooltip));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn entry(stage: LandingStage) -> LandingEntry {
        LandingEntry {
            stage,
            computed_at: Instant::now(),
        }
    }

    /// No unlanded work: the stage question does not arise, regardless of
    /// what (if anything) is cached — mirrors `LandingStage::derive`'s own
    /// `unlanded == 0 => Landed` guard.
    #[test]
    fn no_unlanded_work_renders_nothing() {
        assert_eq!(landing_chip(false, true, None), None);
        assert_eq!(
            landing_chip(false, true, Some(&entry(LandingStage::Unpushed))),
            None,
            "even a cached stage from before the branch landed must not linger"
        );
    }

    /// A detached HEAD with unlanded work is the row's most attention-worthy
    /// state, not a non-question — `headline_dot`, `render_trunk_inline`,
    /// `is_noteworthy`, and `removal_safety`'s removal-blocking check all
    /// already treat it that way. The chip must say so with a terminal
    /// "detached" answer (no worker probe possible or needed — no branch
    /// means no push and no PR, ever), and that answer must be visibly
    /// distinct from both `Pending` (a probe that hasn't run yet) and
    /// `PrStateUnknown` (a probe that ran and failed).
    #[test]
    fn detached_head_with_unlanded_work_renders_a_terminal_stall_chip() {
        let view = landing_chip(true, false, None).unwrap();
        assert_eq!(
            view,
            LandingChipView::Chip {
                text: "detached".to_string(),
                tone: LandingChipTone::Warn,
                tooltip: "Unlanded commits on a detached HEAD — there is no branch to push or \
                          open a pull request from, so this work cannot land by any push/PR \
                          path until it is given one."
                    .to_string(),
            }
        );
        assert_ne!(
            view,
            LandingChipView::Pending,
            "a permanently unanswerable question is not the same as one not yet asked"
        );
        let unknown_pr_state = landing_chip(
            true,
            true,
            Some(&entry(LandingStage::PrStateUnknown {
                pushed: false,
                why: "couldn't read the branch's remote-tracking ref".to_string(),
            })),
        )
        .unwrap();
        assert_ne!(
            view, unknown_pr_state,
            "a permanent no-branch answer must not read like a failed probe"
        );
    }

    /// Even cached data from before the worktree went detached must not
    /// override the terminal "detached" answer — the branch fact wins.
    #[test]
    fn detached_head_overrides_any_stale_cached_stage() {
        let view = landing_chip(
            true,
            false,
            Some(&entry(LandingStage::InReview {
                number: 1,
                url: "u".into(),
            })),
        )
        .unwrap();
        let LandingChipView::Chip { text, .. } = view else {
            panic!("expected a chip");
        };
        assert_eq!(text, "detached");
    }

    /// The worker hasn't reached this (unlanded, branched) worktree yet.
    #[test]
    fn unlanded_with_no_cache_entry_is_pending() {
        assert_eq!(
            landing_chip(true, true, None),
            Some(LandingChipView::Pending)
        );
    }

    #[test]
    fn unpushed_is_a_warn_chip() {
        let view = landing_chip(true, true, Some(&entry(LandingStage::Unpushed))).unwrap();
        assert_eq!(
            view,
            LandingChipView::Chip {
                text: "unpushed".to_string(),
                tone: LandingChipTone::Warn,
                tooltip: "Committed locally, never pushed — nothing to review yet.".to_string(),
            }
        );
    }

    #[test]
    fn pushed_no_pr_is_a_warn_chip_distinct_from_unpushed() {
        let view = landing_chip(true, true, Some(&entry(LandingStage::PushedNoPr))).unwrap();
        let LandingChipView::Chip { text, tone, .. } = view else {
            panic!("expected a chip");
        };
        assert_eq!(tone, LandingChipTone::Warn);
        assert_eq!(text, "no PR");
        assert_ne!(
            text, "unpushed",
            "two different facts must not read the same"
        );
    }

    #[test]
    fn in_review_is_an_info_chip_naming_the_pr() {
        let view = landing_chip(
            true,
            true,
            Some(&entry(LandingStage::InReview {
                number: 452,
                url: "https://example.test/452".into(),
            })),
        )
        .unwrap();
        let LandingChipView::Chip {
            text,
            tone,
            tooltip,
        } = view
        else {
            panic!("expected a chip");
        };
        assert_eq!(tone, LandingChipTone::Info);
        assert_eq!(text, "PR #452");
        assert!(tooltip.contains("https://example.test/452"));
    }

    #[test]
    fn rejected_is_a_danger_chip_not_a_stall() {
        let view = landing_chip(
            true,
            true,
            Some(&entry(LandingStage::Rejected {
                number: 18,
                url: "u".into(),
            })),
        )
        .unwrap();
        let LandingChipView::Chip { text, tone, .. } = view else {
            panic!("expected a chip");
        };
        assert_eq!(tone, LandingChipTone::Danger);
        assert_eq!(text, "PR #18 closed");
    }

    #[test]
    fn merged_but_unlanded_is_a_good_chip_not_a_warning() {
        let view = landing_chip(
            true,
            true,
            Some(&entry(LandingStage::MergedButUnlanded {
                number: 857,
                url: "u".into(),
            })),
        )
        .unwrap();
        let LandingChipView::Chip { text, tone, .. } = view else {
            panic!("expected a chip");
        };
        assert_eq!(tone, LandingChipTone::Good);
        assert_eq!(text, "PR #857 merged");
    }

    /// The hard constraint this whole feature exists to satisfy: `Ok(None)`
    /// (`PushedNoPr`) and `Err` (`PrStateUnknown`) must never render the
    /// same, or an in-review branch that failed a `gh` probe would read as
    /// an un-offered stall.
    #[test]
    fn pr_state_unknown_never_reads_like_confirmed_no_pr() {
        let confirmed_no_pr =
            landing_chip(true, true, Some(&entry(LandingStage::PushedNoPr))).unwrap();
        let couldnt_ask = landing_chip(
            true,
            true,
            Some(&entry(LandingStage::PrStateUnknown {
                pushed: true,
                why: "gh: not logged in".to_string(),
            })),
        )
        .unwrap();
        assert_ne!(
            confirmed_no_pr, couldnt_ask,
            "Ok(None) and Err must render distinctly end to end"
        );
        let LandingChipView::Chip { text, tooltip, .. } = couldnt_ask else {
            panic!("expected a chip");
        };
        assert_eq!(text, "PR state ?");
        assert!(tooltip.contains("gh: not logged in"));
    }

    #[test]
    fn pr_state_unknown_wording_reflects_whether_push_state_is_known() {
        let unknown_and_not_known_pushed = landing_chip(
            true,
            true,
            Some(&entry(LandingStage::PrStateUnknown {
                pushed: false,
                why: "couldn't read the branch's remote-tracking ref".to_string(),
            })),
        )
        .unwrap();
        let LandingChipView::Chip { text, .. } = unknown_and_not_known_pushed else {
            panic!("expected a chip");
        };
        assert_eq!(text, "landing ?");
    }
}
