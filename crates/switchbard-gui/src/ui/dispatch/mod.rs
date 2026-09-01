//! Ambient dispatch status shared across the whole app.
//!
//! TASK-98 moved the Dispatches view itself to `ui::places::dispatches`
//! (task-scoped delivery state: per-run rows, facets, kill/retry/log). What
//! stays here is [`DispatchSummary`] and [`summarize_dispatch`] — the
//! *counts*, not the rows — because three call sites that render on every
//! frame regardless of which place is active (the top bar's chip, the nav's
//! Tasks "Dispatches (N)" badge, and the nav's footer lamp) all need the same
//! answer to "is anything running?" without paying to build a single row.
//!
//! ## Where the data comes from
//!
//! State is derived, never stored. A task's dispatch label *is* its pipeline
//! state (`ui::backlog::dispatch_ui::dispatch_state`, reused here rather than
//! re-derived so this and the Dispatches view can never disagree), and the
//! run's paths and start time come from `switchbard_core::dispatch_inspect`,
//! which rebuilds them from the repo root and task id. See that module's doc
//! for why there is no run store.
//!
//! ## Render-path discipline
//!
//! The only per-frame work here is arithmetic. Every filesystem read behind a
//! `DispatchRun` happens on the backlog worker (`workers::refresh_dispatch_
//! runs`); [`summarize_dispatch`] reads the resulting cache and does one pass
//! over cached tasks plus one over the matching runs — no clone of a task or
//! a run, since it only counts them (see its own doc for the exact cost).

use crate::app::HiveApp;
use crate::runtime::BacklogTaskKey;
use crate::ui::backlog::dispatch_ui::{self, DispatchCategory};
use crate::ui::theme::{self, ActionIcon};
use eframe::egui;
use std::path::Path;
use std::time::Duration;
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun};
use switchbard_core::DispatchOptions;

/// Ambient dispatch counts shared by the top bar's chip, the nav's Tasks
/// place "Dispatches (N)" badge, and the nav's footer lamp — the answer to
/// "is anything running?" from wherever you are. Scoped by the sidebar's
/// repo scope (TASK-96 post-review), the same as the Dispatches view itself
/// (`ui::places::dispatches::collect_rows`) — one scoping rule everywhere, so
/// narrowing scope can never leave an ambient indicator claiming a run the
/// list doesn't show.
/// Computed once per frame in `HiveApp::render_ui` and passed down to every
/// reader — see that call site's own comment for why it must not be
/// recomputed per-caller.
///
/// The buckets are **disjoint**, so `queued + in_flight + needs_attention` is
/// a real total rather than a double count. A stalled run lands in
/// `needs_attention` rather than `in_flight`: a run past its own advisory
/// staleness threshold is not in the happy path any more, and reporting it as
/// healthily running is exactly the false reassurance this whole view exists
/// to remove — even though (TASK-46) the run itself is still going and
/// nothing here will kill it. Runs that already opened a PR are in none of
/// them — awaiting review is not an operational state, and a permanent chip
/// nobody can clear is chrome, not information.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DispatchSummary {
    /// Flagged for dispatch, not yet claimed by the worker.
    pub queued: usize,
    /// Claimed, running, and not yet past the advisory staleness threshold.
    pub in_flight: usize,
    /// Failed, orphaned, or past the advisory staleness threshold — a human
    /// has to look, though a staleness hit alone is still a *running* run
    /// (TASK-46 removed the automatic kill; see `crate::dispatch`'s module
    /// doc).
    pub needs_attention: usize,
    /// Elapsed seconds of the oldest *claimed* run (healthy or stalled).
    /// `None` when nothing is claimed.
    pub oldest_running_secs: Option<u64>,
}

impl DispatchSummary {
    /// Fold one dispatch-labeled task into the counts. Pure — `now` and
    /// `stale_after` are parameters, not clock reads — so every visibility
    /// rule below is unit-testable without a frame or a filesystem.
    pub(crate) fn observe(
        &mut self,
        category: DispatchCategory,
        run: Option<&DispatchRun>,
        now: u64,
        stale_after: Duration,
    ) {
        match category {
            DispatchCategory::Queued => self.queued += 1,
            DispatchCategory::Failed => self.needs_attention += 1,
            DispatchCategory::InFlight => {
                // The `dispatching` label alone is not proof a run is live —
                // an abandoned one wears it forever. Same predicate the
                // Dispatches view's sectioning uses, so the chip and the
                // list can never disagree about which runs are stuck.
                //
                // Known gap, accepted (audit N5): a claimed run whose
                // sidecar is `Unverifiable` — from a previous boot, or in
                // the legacy format — counts here as *running* until
                // `looks_stalled` catches it at the 30-minute mark.
                // `is_abandoned` needs positive proof of death, and "we
                // can't tell" is not that. Counting the other way would
                // mean an unverifiable sidecar could raise a permanent red
                // alarm over a run that is in fact healthy, which is the
                // worse error for a chip whose whole job is to be believed;
                // the row itself already says "unverified" for anyone who
                // looks.
                let abandoned = run.is_some_and(|run| run.is_abandoned(now, true));
                let stalled = run.is_some_and(|run| run.looks_stalled(now, stale_after));
                if abandoned || stalled {
                    self.needs_attention += 1;
                } else {
                    self.in_flight += 1;
                }
                if !abandoned {
                    if let Some(elapsed) = run.and_then(|run| run.elapsed(now)) {
                        let secs = elapsed.as_secs();
                        self.oldest_running_secs =
                            Some(self.oldest_running_secs.map_or(secs, |old| old.max(secs)));
                    }
                }
            }
            DispatchCategory::Dispatched | DispatchCategory::NotFlagged => {}
        }
    }

    /// Nothing queued, nothing running, nothing to fix. The top bar renders
    /// no chip and no badge in this state — the same "no ticking counters
    /// with nothing to say" rule that removed the last-scan label and keeps
    /// the retired-worktrees nudge silent at zero.
    pub fn is_idle(&self) -> bool {
        self.queued == 0 && self.in_flight == 0 && self.needs_attention == 0
    }

    /// Whether the chip should read as an alarm rather than as status.
    pub fn needs_attention(&self) -> bool {
        self.needs_attention > 0
    }

    /// The Dispatches tab badge count: work in flight plus work asking for a
    /// decision. Queued tasks are deliberately excluded — the badge is about
    /// runs, and a queued task has not started one.
    pub fn badge_count(&self) -> usize {
        self.in_flight + self.needs_attention
    }

    /// One line for the top-bar chip. Leads with whatever is most urgent:
    /// attention first, then live runs with the oldest one's elapsed time
    /// (the number that says "should I go look?"), then a bare queue depth.
    pub fn chip_text(&self) -> String {
        if self.needs_attention > 0 {
            let mut text = format!(
                "⚠ {} dispatch run{} need{} attention",
                self.needs_attention,
                if self.needs_attention == 1 { "" } else { "s" },
                if self.needs_attention == 1 { "s" } else { "" },
            );
            if self.in_flight > 0 {
                text.push_str(&format!(" · {} running", self.in_flight));
            }
            return text;
        }
        if self.in_flight > 0 {
            let mut text = format!("⚙ {} running", self.in_flight);
            if let Some(secs) = self.oldest_running_secs {
                text.push_str(&format!(" · {}", format_elapsed(Duration::from_secs(secs))));
            }
            if self.queued > 0 {
                text.push_str(&format!(" · {} queued", self.queued));
            }
            return text;
        }
        format!("⚙ {} queued", self.queued)
    }
}

/// Count what the top bar needs without building a single row.
///
/// Deliberately not `ui::places::dispatches::collect_rows` + fold: that path
/// clones every task and every run so it can render them, and the top bar
/// renders on every frame of every tab — including tabs that never look at
/// dispatch at all.
///
/// What this actually costs per frame, precisely: one pass over the cached
/// tasks comparing label strings ([`dispatch_ui::dispatch_category`], chosen
/// over `dispatch_state` because the latter allocates a `String` per finished
/// task to extract a PR link the top bar never renders), plus one `Vec` of
/// `(repo root, task id)` keys for the dispatch-labeled tasks only. That `Vec`
/// is empty — and therefore allocation-free — in the overwhelmingly common
/// case of nothing being dispatched. It exists so the two mutexes are taken
/// one at a time rather than nested, matching `workers::refresh_dispatch_runs`'s
/// ordering. No task, run, or note text is cloned.
pub(crate) fn summarize_dispatch(app: &HiveApp) -> DispatchSummary {
    let flagged: Vec<(BacklogTaskKey, DispatchCategory)> = {
        let repos = app.backlog_repos.lock().unwrap();
        repos
            .iter()
            // Post-review (TASK-96): scoped, matching `collect_rows` below —
            // one scoping rule everywhere. Previously this fed the nav
            // badge/footer lamp and the top-bar chip unscoped while the
            // Dispatches list itself was already scoped, so narrowing scope
            // could leave the badge/chip claiming runs the list didn't show
            // at all. `root` is a repo root path (see `collect_rows`'s own
            // doc), so this is the same `path_in_scope` check.
            .filter(|(root, _)| crate::runtime::path_in_scope(root, &app.repo_scope))
            .flat_map(|(root, repo)| {
                repo.tasks.iter().filter_map(move |task| {
                    match dispatch_ui::dispatch_category(task) {
                        DispatchCategory::NotFlagged => None,
                        category => Some(((root.clone(), task.id.clone()), category)),
                    }
                })
            })
            .collect()
    };
    if flagged.is_empty() {
        return DispatchSummary::default();
    }

    let now = now_unix();
    let stale_after = DispatchOptions::default().stale_after;
    let runs = app.dispatch_runs.lock().unwrap();
    let mut summary = DispatchSummary::default();
    for (key, category) in &flagged {
        summary.observe(*category, runs.get(key), now, stale_after);
    }
    summary
}

/// Compact `2h 14m` / `7m 30s` / `45s`. Minutes matter for a run measured in
/// tens of minutes against a 30-minute default staleness threshold; seconds
/// only matter early on. Shared with `ui::places::dispatches` (its rows'
/// elapsed/SITREP-age text) and `ui::places::command` (its rows' SITREP-age
/// text) — one formatter, so "45s" vs "0m 45s" can never disagree across the
/// three surfaces that show a dispatch run's age.
pub(crate) fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    match (secs / 3600, (secs % 3600) / 60, secs % 60) {
        (0, 0, s) => format!("{s}s"),
        (0, m, s) => format!("{m}m {s}s"),
        (h, m, _) => format!("{h}h {m}m"),
    }
}

/// The Kill icon button for one in-flight run — shared by `ui::places::
/// dispatches` and `ui::places::command`, the two surfaces that render a
/// dispatch run's Kill control, so there is exactly one kill-confirm
/// implementation for `dispatch_kill_confirm`'s state machine to match.
/// Arms the confirm banner (see [`render_kill_confirm_banner`]) on click;
/// renders nothing once armed, so a caller placing this inside a
/// `right_to_left` action cluster never has to reason about the confirm
/// banner's own internal layout (see that function's doc for why the two
/// are deliberately two separate calls, not one).
///
/// ## What has to be true before this renders a button
///
/// Exactly one thing, and it is not "a sidecar exists": the run's liveness
/// verdict must be `Alive`, which `dispatch_inspect` only issues after
/// authenticating the sidecar against this boot **and** confirming the
/// process group still carries this run's own prompt path. Reading a pgid
/// from anywhere else would reintroduce a Switchbard-force-quit-mid-run /
/// pid-recycling false kill — see `DispatchRunLiveness`'s own doc.
pub(crate) fn render_kill_icon(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    repo_root: &Path,
    task_id: &str,
    run: &DispatchRun,
) {
    let Some(_pgid) = run.liveness.killable_pgid() else {
        return;
    };
    if run.started_at_unix.is_none() {
        return;
    }
    let key: BacklogTaskKey = (repo_root.to_path_buf(), task_id.to_string());
    if app.dispatch_kill_confirm.as_ref() == Some(&key) {
        return;
    }
    if theme::action_icon_button(ui, ActionIcon::Kill, "Kill", true).clicked() {
        app.dispatch_kill_confirm = Some(key);
    }
}

/// The "Kill pgid...? Confirm / Cancel" banner, once [`render_kill_icon`]
/// has armed it. A deliberately separate call from the icon itself — not a
/// nested `with_layout` inside the same `right_to_left` action cluster —
/// for a concrete, observed reason: this app renders every action cluster
/// inside `right_to_left`, and a bare `Layout::left_to_right` sub-`with_
/// layout` nested inside it did not reliably re-bound to one text line the
/// way a top-level `ui.horizontal` does (an early version of this control
/// intermittently claimed the *entire remaining rect*, width and height,
/// stretching a row's selection highlight across a whole `ScrollArea` with
/// the label and buttons stranded mid-canvas). A banner on its own plain
/// line, rendered by the caller only while armed, sidesteps the whole
/// question and reads better besides — a kill confirmation deserves a full
/// line, not a squeeze into a 3-icon strip.
///
/// Returns `true` while the confirm banner is armed (whether or not it drew
/// anything — `render_kill_icon`'s own gate can still refuse for a run with
/// no verified pgid), so callers can decide whether to render it at all.
pub(crate) fn render_kill_confirm_banner(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    repo_root: &Path,
    task_id: &str,
    run: &DispatchRun,
) -> bool {
    let Some(pgid) = run.liveness.killable_pgid() else {
        return false;
    };
    let Some(started_at_unix) = run.started_at_unix else {
        return false;
    };
    let key: BacklogTaskKey = (repo_root.to_path_buf(), task_id.to_string());
    if app.dispatch_kill_confirm.as_ref() != Some(&key) {
        return false;
    }
    let supervised = run.liveness.is_supervised();
    ui.horizontal(|ui| {
        let aftermath = if supervised {
            "the task is released as dispatch-failed with a note"
        } else {
            "the task stays on `dispatching` — resolve it by hand afterwards"
        };
        ui.label(
            egui::RichText::new(format!("Kill pgid {pgid}? {aftermath}.")).color(theme::amber()),
        );
        if ui.small_button("Confirm").clicked() {
            app.spawn_kill_dispatch(task_id.to_string(), started_at_unix, ui.ctx());
            app.dispatch_kill_confirm = None;
        }
        if ui.small_button("Cancel").clicked() {
            app.dispatch_kill_confirm = None;
        }
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchbard_core::dispatch_inspect::DispatchRunLiveness;

    const STALE_AFTER: Duration = Duration::from_secs(30 * 60);

    /// A run started `age` seconds before "now" (= `NOW`), with `log_bytes`
    /// of output already flushed. Output + an old mtime is what
    /// `looks_orphaned` keys on, so `finished_ago` drives that separately.
    fn run(age: u64, log_bytes: u64, finished_ago: Option<u64>) -> DispatchRun {
        run_with_liveness(age, log_bytes, finished_ago, DispatchRunLiveness::NoSidecar)
    }

    fn run_with_liveness(
        age: u64,
        log_bytes: u64,
        finished_ago: Option<u64>,
        liveness: DispatchRunLiveness,
    ) -> DispatchRun {
        DispatchRun {
            task_id: "TASK-1".to_string(),
            branch: "dispatch/task-1".to_string(),
            worktree_path: std::path::PathBuf::from("/repo/.worktrees/dispatch-task-1"),
            worktree_exists: true,
            log_path: None,
            prompt_path: None,
            started_at_unix: Some(NOW - age),
            log_bytes,
            log_modified_unix: finished_ago.map(|ago| NOW - ago),
            liveness,
            progress: switchbard_core::dispatch_inspect::RunProgress::default(),
        }
    }

    const NOW: u64 = 1_000_000;

    fn summarize(entries: &[(DispatchCategory, Option<DispatchRun>)]) -> DispatchSummary {
        let mut summary = DispatchSummary::default();
        for (category, run) in entries {
            summary.observe(*category, run.as_ref(), NOW, STALE_AFTER);
        }
        summary
    }

    /// The chip's whole reason to exist is that it is *absent* almost all the
    /// time. Nothing flagged at all, and nothing flagged that has finished,
    /// must both read as idle — a permanently-lit chip is chrome, not signal.
    #[test]
    fn a_workspace_with_no_live_dispatch_work_is_idle() {
        assert!(summarize(&[]).is_idle());

        let finished = summarize(&[(DispatchCategory::Dispatched, Some(run(600, 900, Some(600))))]);

        assert!(finished.is_idle(), "awaiting review is not an alarm");
        assert_eq!(finished.badge_count(), 0);
    }

    #[test]
    fn queued_and_running_tasks_are_counted_separately() {
        let summary = summarize(&[
            (DispatchCategory::Queued, None),
            (DispatchCategory::Queued, None),
            (DispatchCategory::InFlight, Some(run(300, 0, None))),
        ]);

        assert_eq!(summary.queued, 2);
        assert_eq!(summary.in_flight, 1);
        assert_eq!(summary.needs_attention, 0);
        assert!(!summary.is_idle());
        // The badge is about runs, so the two queued tasks don't inflate it.
        assert_eq!(summary.badge_count(), 1);
        assert!(!summary.needs_attention());
    }

    /// A queue with nothing claimed yet still shows a chip — the user flagged
    /// something and it has not started, which is worth an ambient word.
    #[test]
    fn a_queue_with_nothing_claimed_still_shows_a_chip() {
        let summary = summarize(&[(DispatchCategory::Queued, None)]);

        assert!(!summary.is_idle());
        assert_eq!(summary.badge_count(), 0);
        assert_eq!(summary.chip_text(), "⚙ 1 queued");
    }

    #[test]
    fn the_chip_reports_the_oldest_running_run() {
        let summary = summarize(&[
            (DispatchCategory::InFlight, Some(run(90, 0, None))),
            (DispatchCategory::InFlight, Some(run(450, 0, None))),
        ]);

        assert_eq!(summary.oldest_running_secs, Some(450));
        assert_eq!(summary.chip_text(), "⚙ 2 running · 7m 30s");
    }

    /// The attention flip, one bucket at a time. Each of the three is a
    /// different upstream condition (label / mtime evidence / clock) and each
    /// has to move the chip into its danger register on its own.
    #[test]
    fn a_failed_run_flips_the_chip_to_attention() {
        let summary = summarize(&[(DispatchCategory::Failed, None)]);

        assert!(summary.needs_attention());
        assert_eq!(summary.needs_attention, 1);
        assert_eq!(summary.in_flight, 0);
        assert_eq!(summary.chip_text(), "⚠ 1 dispatch run needs attention");
    }

    #[test]
    fn an_orphaned_run_flips_the_chip_to_attention_despite_its_in_flight_label() {
        let orphan = run(3_000, 900, Some(600));
        assert!(
            orphan.looks_orphaned(NOW, true),
            "fixture must be an orphan"
        );

        let summary = summarize(&[(DispatchCategory::InFlight, Some(orphan))]);

        assert!(summary.needs_attention());
        assert_eq!(summary.in_flight, 0, "an orphan is not running");
        // An orphan's elapsed time is not a live clock — it stopped when the
        // agent did — so it must not be what the chip counts up.
        assert_eq!(summary.oldest_running_secs, None);
    }

    #[test]
    fn a_stalled_run_counts_as_attention_not_as_healthy_running() {
        let stalled = run(STALE_AFTER.as_secs() + 60, 0, None);
        assert!(stalled.looks_stalled(NOW, STALE_AFTER));

        let summary = summarize(&[
            (DispatchCategory::InFlight, Some(stalled)),
            (DispatchCategory::InFlight, Some(run(120, 0, None))),
        ]);

        assert_eq!(summary.in_flight, 1);
        assert_eq!(summary.needs_attention, 1);
        assert_eq!(summary.badge_count(), 2);
        assert_eq!(
            summary.chip_text(),
            "⚠ 1 dispatch run needs attention · 1 running"
        );
    }

    #[test]
    fn attention_wording_pluralizes_both_ways() {
        let two = summarize(&[
            (DispatchCategory::Failed, None),
            (DispatchCategory::Failed, None),
        ]);

        assert_eq!(two.chip_text(), "⚠ 2 dispatch runs need attention");
    }

    /// F1/F4a through the summary: a claimed run whose process group has been
    /// verified gone must feed the chip as *attention*, not as a healthy
    /// running agent. Its log is empty — indistinguishable from a live run by
    /// file evidence alone — so this is the only signal that catches it.
    #[test]
    fn a_run_whose_group_is_verified_gone_counts_as_attention() {
        let dead = run_with_liveness(300, 0, None, DispatchRunLiveness::Gone);
        assert!(dead.is_abandoned(NOW, true));

        let summary = summarize(&[(DispatchCategory::InFlight, Some(dead))]);

        assert_eq!(summary.needs_attention, 1);
        assert_eq!(summary.in_flight, 0, "a dead group is not running");
        assert!(summary.needs_attention());
        assert_eq!(
            summary.oldest_running_secs, None,
            "a dead run's clock stopped; it must not drive the chip's elapsed time"
        );
    }

    /// An unsupervised but verifiably *live* agent is still running, so it
    /// belongs in the running count — the audit's F4a scope is deliberately
    /// limited to verified-dead groups. What changes for it is the row's copy
    /// (no deadline), not the chip's arithmetic.
    #[test]
    fn an_unsupervised_but_live_run_still_counts_as_running() {
        let live = run_with_liveness(
            300,
            0,
            None,
            DispatchRunLiveness::Alive {
                pgid: 4242,
                supervised: false,
            },
        );
        assert!(!live.is_abandoned(NOW, true));

        let summary = summarize(&[(DispatchCategory::InFlight, Some(live))]);

        assert_eq!(summary.in_flight, 1);
        assert_eq!(summary.needs_attention, 0);
    }

    #[test]
    fn elapsed_formats_by_the_largest_useful_unit() {
        assert_eq!(format_elapsed(Duration::from_secs(45)), "45s");
        assert_eq!(format_elapsed(Duration::from_secs(450)), "7m 30s");
        assert_eq!(format_elapsed(Duration::from_secs(8_040)), "2h 14m");
    }
}
