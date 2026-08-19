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

use crate::app::HiveApp;
use crate::runtime::BacklogTaskKey;
use crate::ui::backlog::dispatch_ui::{self, DispatchState};
use crate::ui::theme;
use eframe::egui;
use std::time::Duration;
use switchbard_core::dispatch_inspect::{now_unix, DispatchRun};
use switchbard_core::{BacklogTask, DispatchOptions};

/// One dispatched task, joined to whatever is knowable about its run.
struct DispatchRow {
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
    /// The `dispatching` label alone does NOT mean in flight. An orphaned run
    /// carries that label forever (its releaser died), so the label is checked
    /// against the run's own evidence before trusting it — see
    /// `DispatchRun::looks_orphaned`.
    fn section(&self, now: u64) -> Section {
        match self.state {
            DispatchState::InFlight => {
                let orphaned = self
                    .run
                    .as_ref()
                    .is_some_and(|run| run.looks_orphaned(now, true));
                if orphaned {
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
            Section::Orphaned => "Finished, never released",
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
                "The agent finished and committed, but Switchbard exited before pushing. \
                 Review the branch, then push and open the PR by hand."
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
                render_section(ui, section, &in_section, now, timeout);
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
        render_row(ui, row, now, timeout);
    }
    ui.add_space(10.0);
}

fn render_row(ui: &mut egui::Ui, row: &DispatchRow, now: u64, timeout: Duration) {
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
                render_run_details(ui, row, run, now, timeout);
            }
            render_outcome(ui, &row.state);
        });
}

fn render_run_details(
    ui: &mut egui::Ui,
    row: &DispatchRow,
    run: &DispatchRun,
    now: u64,
    timeout: Duration,
) {
    let orphaned = row.section(now) == Section::Orphaned;
    // An orphan carries the in-flight label but is not running, so the
    // live-run affordances (ticking "running", the empty-log reassurance)
    // must not apply to it.
    let in_flight = matches!(row.state, DispatchState::InFlight) && !orphaned;
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
            if stalled {
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
