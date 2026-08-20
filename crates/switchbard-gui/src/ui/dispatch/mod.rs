//! The Dispatch view: one place to see what the dispatch pipeline is doing.
//!
//! Answers the question the Backlog view could not — "I flagged a task, is it
//! actually working?" The detail rail shows a task's dispatch *pill*, but a
//! pill is a state name, not progress: it can't say how long a run has been
//! going, which worktree it is in, or where its log is. This view is that
//! missing surface, across every tracked repo at once.
//!
//! ## Where the data comes from
//!
//! State is derived, never stored. A task's dispatch label *is* its pipeline
//! state (`ui::backlog::dispatch_ui::dispatch_state`, reused here rather than
//! re-derived so the two views can never disagree), and the run's paths and
//! start time come from `switchbard_core::dispatch_inspect`, which rebuilds
//! them from the repo root and task id. See that module's doc for why there is
//! no run store.
//!
//! ## Render-path discipline
//!
//! The only per-frame work here is arithmetic. Every filesystem read behind a
//! [`DispatchRun`] happens on the backlog worker
//! (`workers::refresh_dispatch_runs`); this module reads the resulting cache
//! and recomputes just `elapsed` from the cached start stamp, so the elapsed
//! time still ticks live at frame rate without a `read_dir` per row.
//!
//! [`summarize_dispatch`] holds that line for the *top bar*, which renders on
//! every frame of every tab. It walks the same two caches without cloning a
//! task or a run — the row-building path below clones because it renders the
//! rows; the summary only counts them.
//!
//! ## The Kill button
//!
//! An in-flight row whose run has a pgid sidecar
//! (`switchbard_core::dispatch::dispatch_pid_path`) gets a confirm-armed Kill
//! control. It sends exactly one signal and records nothing: the pipeline is
//! blocked in `wait_for_exit` on that process group, so the kill makes the
//! wait return and `dispatch_one` releases the task as `dispatch-failed` with
//! a note through its ordinary failure path. Writing any state from here
//! would be a second writer racing the first.

use crate::app::HiveApp;
use crate::runtime::BacklogTaskKey;
use crate::ui::backlog::dispatch_ui::{self, DispatchCategory, DispatchState};
use crate::ui::theme;
use eframe::egui;
use std::path::PathBuf;
use std::time::Duration;
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun};
use switchbard_core::{BacklogTask, DispatchOptions};

/// Ambient dispatch counts for the top bar — the chip and the Dispatches tab
/// badge, i.e. the answer to "is anything running?" from a tab that is not
/// this one.
///
/// The buckets are **disjoint**, so `queued + in_flight + needs_attention` is
/// a real total rather than a double count. A stalled run lands in
/// `needs_attention` rather than `in_flight`: a run past its own hard-kill
/// deadline is not in the happy path any more, and reporting it as healthily
/// running is exactly the false reassurance this whole task exists to remove.
/// Runs that already opened a PR are in none of them — awaiting review is not
/// an operational state, and a permanent chip nobody can clear is chrome, not
/// information.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DispatchSummary {
    /// Flagged for dispatch, not yet claimed by the worker.
    pub queued: usize,
    /// Claimed, running, and still inside its timeout.
    pub in_flight: usize,
    /// Failed, orphaned, or past its timeout — a human has to look.
    pub needs_attention: usize,
    /// Elapsed seconds of the oldest *claimed* run (healthy or stalled).
    /// `None` when nothing is claimed.
    pub oldest_running_secs: Option<u64>,
}

impl DispatchSummary {
    /// Fold one dispatch-labeled task into the counts. Pure — `now` and
    /// `timeout` are parameters, not clock reads — so every visibility rule
    /// below is unit-testable without a frame or a filesystem.
    fn observe(
        &mut self,
        category: DispatchCategory,
        run: Option<&DispatchRun>,
        now: u64,
        timeout: Duration,
    ) {
        match category {
            DispatchCategory::Queued => self.queued += 1,
            DispatchCategory::Failed => self.needs_attention += 1,
            DispatchCategory::InFlight => {
                // The `dispatching` label alone is not proof a run is live —
                // an abandoned one wears it forever. Same predicate the view's
                // sectioning uses, so the chip and the list can never disagree
                // about which runs are stuck.
                let abandoned = run.is_some_and(|run| run.is_abandoned(now, true));
                let stalled = run.is_some_and(|run| run.looks_stalled(now, timeout));
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
/// Deliberately not `collect_rows` + fold: that path clones every task and
/// every run so it can render them, and the top bar renders on every frame of
/// every tab — including tabs that never look at dispatch at all.
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
        let projects = app.backlog_projects.lock().unwrap();
        projects
            .iter()
            .flat_map(|(root, project)| {
                project.tasks.iter().filter_map(move |task| {
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
    let timeout = DispatchOptions::default().timeout;
    let runs = app.dispatch_runs.lock().unwrap();
    let mut summary = DispatchSummary::default();
    for (key, category) in &flagged {
        summary.observe(*category, runs.get(key), now, timeout);
    }
    summary
}

/// One dispatched task, joined to whatever is knowable about its run.
struct DispatchRow {
    /// Repo root, not just the display name: the Kill control keys its
    /// confirm state by [`BacklogTaskKey`], and a task id is only unique
    /// within one project.
    repo_root: PathBuf,
    repo_name: String,
    task: BacklogTask,
    state: DispatchState,
    run: Option<DispatchRun>,
}

impl DispatchRow {
    /// Section ordering: things needing attention first. Orphans lead because
    /// they are work already finished that nothing will ever pick up again;
    /// in-flight next because those rows change while you watch; failures
    /// outrank finished work because they are the ones asking for a decision.
    ///
    /// The `dispatching` label alone does NOT mean in flight. A run whose
    /// releaser died carries that label forever, so the label is checked
    /// against the run's own evidence before trusting it — see
    /// `DispatchRun::is_abandoned`, which covers both the agent-finished and
    /// the process-group-is-gone flavours of that.
    fn section(&self, now: u64) -> Section {
        match self.state {
            DispatchState::InFlight => {
                let abandoned = self
                    .run
                    .as_ref()
                    .is_some_and(|run| run.is_abandoned(now, true));
                if abandoned {
                    Section::Orphaned
                } else {
                    Section::InFlight
                }
            }
            DispatchState::Queued => Section::Queued,
            DispatchState::Failed { .. } => Section::Failed,
            DispatchState::Dispatched { .. } => Section::AwaitingReview,
            DispatchState::NotFlagged => Section::Queued,
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Section {
    Orphaned,
    InFlight,
    Queued,
    Failed,
    AwaitingReview,
}

impl Section {
    fn title(self) -> &'static str {
        match self {
            Section::Orphaned => "Claimed, but nothing is running",
            Section::InFlight => "In flight",
            Section::Queued => "Queued",
            Section::Failed => "Failed",
            Section::AwaitingReview => "Awaiting review",
        }
    }

    /// What the section means, for someone who has not memorised the label
    /// state machine. Kept next to the title rather than in a tooltip: this
    /// view exists precisely because the pipeline was opaque.
    fn blurb(self) -> &'static str {
        match self {
            Section::Orphaned => {
                "The task is still claimed but its agent is gone — either it finished and \
                 Switchbard exited before pushing, or the run died with it. Review the \
                 branch, then push and open the PR by hand, or re-flag the task."
            }
            Section::InFlight => "A headless agent is running now in its own worktree.",
            Section::Queued => "Flagged, waiting for the dispatch worker's next poll.",
            Section::Failed => "The run ended without a PR. Re-flag to retry.",
            Section::AwaitingReview => "The agent finished and opened a PR.",
        }
    }

    const ALL: [Section; 5] = [
        Section::Orphaned,
        Section::InFlight,
        Section::Queued,
        Section::Failed,
        Section::AwaitingReview,
    ];
}

pub fn render(app: &mut HiveApp, ctx: &egui::Context) {
    let rows = collect_rows(app);
    let now = now_unix();
    let timeout = DispatchOptions::default().timeout;

    egui::CentralPanel::default().show(ctx, |ui| {
        if rows.is_empty() {
            render_empty(ui);
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for section in Section::ALL {
                let in_section: Vec<&DispatchRow> = rows
                    .iter()
                    .filter(|row| row.section(now) == section)
                    .collect();
                if in_section.is_empty() {
                    continue;
                }
                render_section(app, ui, section, &in_section, now, timeout);
            }
        });
    });
}

/// Join every dispatch-labeled task to its cached run, filtered by the shared
/// top-bar filter. Sorted newest-run-first inside a section so a long queue
/// keeps the thing that just happened at the top.
fn collect_rows(app: &HiveApp) -> Vec<DispatchRow> {
    let projects = app.backlog_projects_snapshot();
    let runs = app.dispatch_runs_snapshot();
    let repos = app.repos_snapshot();
    let filter = app.filter.to_lowercase();

    let mut rows: Vec<DispatchRow> = Vec::new();
    for (root, project) in &projects {
        let repo_name = repos
            .iter()
            .find(|repo| &repo.path == root)
            .map(|repo| repo.name.clone())
            .unwrap_or_else(|| root.display().to_string());

        for task in &project.tasks {
            let state = dispatch_ui::dispatch_state(task);
            if matches!(state, DispatchState::NotFlagged) {
                continue;
            }
            let run = runs
                .get(&(root.clone(), task.id.clone()) as &BacklogTaskKey)
                .cloned();
            let row = DispatchRow {
                repo_root: root.clone(),
                repo_name: repo_name.clone(),
                task: task.clone(),
                state,
                run,
            };
            if row_matches(&row, &filter) {
                rows.push(row);
            }
        }
    }

    rows.sort_by(|a, b| {
        let a_started = a.run.as_ref().and_then(|r| r.started_at_unix).unwrap_or(0);
        let b_started = b.run.as_ref().and_then(|r| r.started_at_unix).unwrap_or(0);
        b_started.cmp(&a_started).then(a.task.id.cmp(&b.task.id))
    });
    rows
}

fn row_matches(row: &DispatchRow, filter_lc: &str) -> bool {
    if filter_lc.is_empty() {
        return true;
    }
    let branch = row
        .run
        .as_ref()
        .map(|run| run.branch.as_str())
        .unwrap_or_default();
    [
        row.task.id.as_str(),
        row.task.title.as_str(),
        row.repo_name.as_str(),
        branch,
    ]
    .iter()
    .any(|field| field.to_lowercase().contains(filter_lc))
}

fn render_section(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    section: Section,
    rows: &[&DispatchRow],
    now: u64,
    timeout: Duration,
) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} ({})", section.title(), rows.len()))
                .strong()
                .size(15.0),
        );
        ui.label(egui::RichText::new(section.blurb()).color(theme::muted_text()));
    });
    ui.separator();
    for row in rows {
        render_row(app, ui, row, now, timeout);
    }
    ui.add_space(10.0);
}

fn render_row(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &DispatchRow,
    now: u64,
    timeout: Duration,
) {
    egui::Frame::default()
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                dispatch_ui::render_dispatch_pill(ui, &row.state);
                ui.label(egui::RichText::new(&row.task.id).strong());
                ui.label(&row.task.title);
                ui.label(
                    egui::RichText::new(format!("[{}]", row.repo_name)).color(theme::muted_text()),
                );
            });

            if let Some(run) = &row.run {
                render_run_details(app, ui, row, run, now, timeout);
            }
            render_outcome(ui, &row.state);
        });
}

fn render_run_details(
    app: &mut HiveApp,
    ui: &mut egui::Ui,
    row: &DispatchRow,
    run: &DispatchRun,
    now: u64,
    timeout: Duration,
) {
    let orphaned = row.section(now) == Section::Orphaned;
    // An abandoned run carries the in-flight label but is not running, so the
    // live-run affordances (ticking "running", the empty-log reassurance)
    // must not apply to it.
    let in_flight = matches!(row.state, DispatchState::InFlight) && !orphaned;
    // The deadline exists only because a supervisor is enforcing it. When the
    // Switchbard that spawned this agent is gone, `wait_for_exit` died with
    // it: nothing will time the run out, and nothing will release the task
    // when it ends. Everything below keys off this rather than off the
    // in-flight label, because a countdown nobody is counting is a lie.
    let supervised = run.liveness.is_supervised();
    ui.horizontal(|ui| {
        if let Some(elapsed) = run.elapsed(now) {
            let stalled = in_flight && run.looks_stalled(now, timeout);
            let label = if in_flight {
                format!("running {}", format_elapsed(elapsed))
            } else if orphaned {
                format!("abandoned after {}", format_elapsed(elapsed))
            } else {
                format!("ran {}", format_elapsed(elapsed))
            };
            let color = if stalled {
                theme::danger()
            } else {
                theme::muted_text()
            };
            ui.label(egui::RichText::new(label).color(color));
            // The deadline, not just the clock: "running 12m" only means
            // something to someone who has memorised `DispatchOptions::
            // default().timeout`. Spelling out how long is left is what turns
            // the elapsed time into a decision ("kill it now, or let the
            // hard kill have it in three minutes").
            if in_flight && supervised {
                if let Some(remaining) = time_until_hard_kill(elapsed, timeout) {
                    ui.label(
                        egui::RichText::new(format!(
                            "· hard kill in {}",
                            format_elapsed(remaining)
                        ))
                        .color(theme::muted_text()),
                    );
                }
            }
            if stalled && supervised {
                ui.label(
                    egui::RichText::new(format!(
                        "past the {} timeout — check the log",
                        format_elapsed(timeout)
                    ))
                    .color(theme::danger()),
                );
            }
        }
        ui.label(egui::RichText::new(&run.branch).color(theme::muted_text()));
    });

    if in_flight && !supervised {
        render_unsupervised_notice(ui, run);
    }

    // The single most misleading signal in this pipeline, called out inline
    // rather than left for the user to misread as a dead run: `claude -p
    // --output-format text` writes nothing until it exits.
    if in_flight && !run.log_has_output() {
        ui.label(
            egui::RichText::new("log is empty until the run finishes — this is normal")
                .color(theme::muted_text())
                .italics(),
        );
    }

    if let Some(log_path) = &run.log_path {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("log").color(theme::muted_text()));
            ui.label(
                egui::RichText::new(log_path.display().to_string()).color(theme::muted_text()),
            );
        });
    }
    if run.worktree_exists {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("worktree").color(theme::muted_text()));
            ui.label(
                egui::RichText::new(run.worktree_path.display().to_string())
                    .color(theme::muted_text()),
            );
        });
    }
    if in_flight {
        render_kill_control(app, ui, row, run);
    }
}

/// The honest replacement for the deadline, on a run whose supervisor is gone.
///
/// Two different situations wear the same `dispatching` label here, and they
/// need different words:
///
/// - the agent is verifiably still running, but unsupervised — no timeout will
///   fire and nothing will release the task, so it can run indefinitely;
/// - nothing about the run can be verified at all (a sidecar from before the
///   last reboot, a legacy-format one, a failed probe) — in which case the app
///   genuinely does not know whether an agent is out there.
///
/// The old copy said "hard kill in Xm" in both cases, which was the F3
/// finding: a countdown promising an enforcement mechanism that no longer
/// exists.
fn render_unsupervised_notice(ui: &mut egui::Ui, run: &DispatchRun) {
    let text = match run.liveness.doubt() {
        Some(doubt) => format!("unverified — {}", doubt.explain()),
        None if run.liveness.killable_pgid().is_some() => {
            "unsupervised — the app that started this run is gone, so no deadline applies and \
             nothing will release the task when it ends"
                .to_string()
        }
        None => "unsupervised — no record of a live process for this run".to_string(),
    };
    ui.label(egui::RichText::new(text).color(theme::amber()).italics());
}

/// Confirm-armed Kill for one in-flight run, mirroring the two-step shape of
/// the Dispatch button it undoes (`backlog::detail_lists::
/// render_dispatch_toggle`) — this is the more destructive of the pair, so it
/// does not get the *lighter* affordance.
///
/// ## What has to be true before this renders a button
///
/// Exactly one thing, and it is not "a sidecar exists": the run's liveness
/// verdict must be `Alive`, which `dispatch_inspect` only issues after
/// authenticating the sidecar against this boot **and** confirming the process
/// group still carries this run's own prompt path. Reading a pgid from
/// anywhere else would reintroduce the audit's F1: a Switchbard force-quit
/// mid-run leaves its sidecar behind, macOS recycles pids at 99999, and a
/// button built on the raw number is a SIGKILL pointed at a stranger — under a
/// confirmation dialog that cheerfully reassures the user it is safe.
///
/// So there is no `pgid` parameter here and no fallback path. `killable_pgid`
/// is the gate; when it says `None`, `render_unsupervised_notice` has already
/// explained why in the row above.
///
/// ## Why the copy changes with supervision
///
/// A supervised kill is a complete operation: the pipeline is blocked on this
/// group, so the signal unblocks it and the task releases as `dispatch-failed`
/// with a note. An **unsupervised** kill stops the agent and nothing else —
/// there is no pipeline left to do the bookkeeping, so the task stays on
/// `dispatching` and a human has to finish the job. Promising the first while
/// doing the second is precisely the F3 finding.
fn render_kill_control(app: &mut HiveApp, ui: &mut egui::Ui, row: &DispatchRow, run: &DispatchRun) {
    let Some(pgid) = run.liveness.killable_pgid() else {
        return;
    };
    let supervised = run.liveness.is_supervised();
    let key: BacklogTaskKey = (row.repo_root.clone(), row.task.id.clone());
    if app.dispatch_kill_confirm.as_ref() == Some(&key) {
        ui.horizontal(|ui| {
            let aftermath = if supervised {
                "The task is released as dispatch-failed with a note."
            } else {
                "Nothing is supervising this run, so the task stays on `dispatching` — \
                 re-flag or resolve it by hand afterwards."
            };
            ui.colored_label(
                theme::amber(),
                format!(
                    "Kill {}'s agent (pgid {pgid})? The worktree and anything it already \
                     committed are left alone. {aftermath}",
                    row.task.id
                ),
            );
            let hover = if supervised {
                "Signals the run's process group; the pipeline then releases the task as \
                 dispatch-failed with a note"
            } else {
                "Signals the run's process group. Its supervisor is gone, so nothing will \
                 release the task afterwards"
            };
            if ui.button("Confirm kill").on_hover_text(hover).clicked() {
                app.spawn_kill_dispatch(row.task.id.clone(), pgid, supervised, ui.ctx());
                app.dispatch_kill_confirm = None;
            }
            if ui.button("Cancel kill").clicked() {
                app.dispatch_kill_confirm = None;
            }
        });
    } else {
        let hover = if supervised {
            "Stop this headless agent now, without waiting for the hard kill"
        } else {
            "Stop this headless agent now. Nothing is supervising it, so the task will need \
             resolving by hand"
        };
        if ui.button("Kill run").on_hover_text(hover).clicked() {
            app.dispatch_kill_confirm = Some(key);
        }
    }
}

/// How long an in-flight run has before `dispatch_one` kills it for exceeding
/// `timeout`. `None` once the deadline has passed — at that point the run is
/// stalled and the view says so instead of counting down past zero.
fn time_until_hard_kill(elapsed: Duration, timeout: Duration) -> Option<Duration> {
    timeout.checked_sub(elapsed).filter(|left| !left.is_zero())
}

fn render_outcome(ui: &mut egui::Ui, state: &DispatchState) {
    match state {
        DispatchState::Dispatched { pr_url } => match pr_url {
            Some(url) => {
                ui.hyperlink_to(url, url);
            }
            None => {
                ui.label(
                    egui::RichText::new("(PR link not found in notes)").color(theme::muted_text()),
                );
            }
        },
        DispatchState::Failed { reason } => {
            let text = reason.as_deref().unwrap_or("(no reason recorded)");
            ui.label(egui::RichText::new(text).color(theme::danger()));
        }
        _ => {}
    }
}

fn render_empty(ui: &mut egui::Ui) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new("Nothing dispatched yet").strong());
        ui.label(
            egui::RichText::new(
                "Flag a task for dispatch from the Backlog view and it shows up here.",
            )
            .color(theme::muted_text()),
        );
    });
}

/// Compact `2h 14m` / `7m 30s` / `45s`. Minutes matter for a run measured in
/// tens of minutes against a 30-minute timeout; seconds only matter early on.
fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    match (secs / 3600, (secs % 3600) / 60, secs % 60) {
        (0, 0, s) => format!("{s}s"),
        (0, m, s) => format!("{m}m {s}s"),
        (h, m, _) => format!("{h}h {m}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchbard_core::dispatch_inspect::DispatchRunLiveness;

    const TIMEOUT: Duration = Duration::from_secs(30 * 60);

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
        }
    }

    const NOW: u64 = 1_000_000;

    fn summarize(entries: &[(DispatchCategory, Option<DispatchRun>)]) -> DispatchSummary {
        let mut summary = DispatchSummary::default();
        for (category, run) in entries {
            summary.observe(*category, run.as_ref(), NOW, TIMEOUT);
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
        let stalled = run(TIMEOUT.as_secs() + 60, 0, None);
        assert!(stalled.looks_stalled(NOW, TIMEOUT));

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

    /// A run inside its timeout counts down; one past it has no countdown to
    /// show and the view says "stalled" instead of a negative number.
    #[test]
    fn the_hard_kill_countdown_stops_at_the_deadline() {
        assert_eq!(
            time_until_hard_kill(Duration::from_secs(600), TIMEOUT),
            Some(Duration::from_secs(1_200))
        );
        assert_eq!(time_until_hard_kill(TIMEOUT, TIMEOUT), None);
        assert_eq!(
            time_until_hard_kill(TIMEOUT + Duration::from_secs(60), TIMEOUT),
            None
        );
    }

    #[test]
    fn elapsed_formats_by_the_largest_useful_unit() {
        assert_eq!(format_elapsed(Duration::from_secs(45)), "45s");
        assert_eq!(format_elapsed(Duration::from_secs(450)), "7m 30s");
        assert_eq!(format_elapsed(Duration::from_secs(8_040)), "2h 14m");
    }

    /// In-flight sorts ahead of finished work, and failures ahead of reviews —
    /// the ordering the view relies on to put the actionable rows on top.
    #[test]
    fn sections_are_ordered_attention_first() {
        let mut sections = vec![
            Section::AwaitingReview,
            Section::Failed,
            Section::Queued,
            Section::InFlight,
            Section::Orphaned,
        ];
        sections.sort();
        assert_eq!(
            sections,
            vec![
                Section::Orphaned,
                Section::InFlight,
                Section::Queued,
                Section::Failed,
                Section::AwaitingReview
            ]
        );
    }
}
