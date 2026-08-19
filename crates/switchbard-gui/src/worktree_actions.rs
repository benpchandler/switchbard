use crate::app::HiveApp;
use crate::runtime::worktree_create::{
    CreateWorktreeDialog, CreateWorktreeOutcome, CreatedWorktree,
};
use crate::runtime::worktree_names::{
    remove_worktree_alias, upsert_worktree_alias, worktree_display_name,
};
use crate::runtime::worktree_rename::RenameWorktreeDialog;
use crate::runtime::{BulkRemoveCandidate, ConfirmBulkRemoveWorktrees};
use eframe::egui;
use std::path::PathBuf;
use std::thread;
use switchbard_core::{
    assess_branch_delete, collect_dirty_files, create_worktree, delete_branch, is_primary_worktree,
    remove_worktree, Repo, WorktreeRef,
};

/// Payload pushed onto `remove_worktree_outcomes` by the worker thread on a
/// successful `git worktree remove`.  The UI thread drains this queue and
/// prunes the matching alias from `config.worktrees` + persists the config,
/// because `config` is owned directly by `HiveApp` and is not `Send`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedWorktree {
    pub repo_path: PathBuf,
    pub worktree_path: PathBuf,
}

/// Build the error string shown in the removal dialog when `git worktree
/// remove` fails after services have already been killed.  Extracted so it can
/// be unit-tested independently of the worker thread.
///
/// When `killed == 0` the caller had nothing to report beyond the git error
/// itself, so we return it verbatim.
pub fn removal_error_message(killed: usize, git_error: &str) -> String {
    if killed == 0 {
        git_error.to_string()
    } else {
        format!(
            "stopped {killed} service{} but removal failed: {git_error}",
            if killed == 1 { "," } else { "s," }
        )
    }
}

impl HiveApp {
    pub fn open_create_worktree(&self, repo: Repo) {
        let worktrees = self.worktrees_snapshot();
        let dialog = CreateWorktreeDialog::new_with_config(repo, &self.config, &worktrees);
        *self.create_worktree_dialog.lock().unwrap() = Some(dialog);
    }

    pub fn cancel_create_worktree(&self) {
        *self.create_worktree_dialog.lock().unwrap() = None;
    }

    pub fn execute_create_worktree(&self, ctx: &egui::Context) {
        let worktrees = self.worktrees_snapshot();
        let (options, created) = {
            let mut guard = self.create_worktree_dialog.lock().unwrap();
            let Some(state) = guard.as_mut() else {
                return;
            };
            if state.busy {
                return;
            }
            let options = match state.validate(&self.config, &worktrees) {
                Ok(options) => options,
                Err(err) => {
                    state.error = Some(err.message().to_string());
                    return;
                }
            };
            state.busy = true;
            state.error = None;
            let created = CreatedWorktree {
                repo: state.repo.clone(),
                worktree_path: options.worktree_path.clone(),
                name: state.name.trim().to_string(),
            };
            (options, created)
        };

        let outcomes = self.create_worktree_outcomes.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let outcome = match create_worktree(options) {
                Ok(()) => CreateWorktreeOutcome::Created(created),
                Err(e) => CreateWorktreeOutcome::Failed(e.to_string()),
            };
            outcomes.lock().unwrap().push(outcome);
            ctx.request_repaint();
        });
    }

    pub fn drain_create_worktree_outcomes(&mut self) {
        let outcomes = {
            let mut guard = self.create_worktree_outcomes.lock().unwrap();
            std::mem::take(&mut *guard)
        };
        for outcome in outcomes {
            match outcome {
                CreateWorktreeOutcome::Created(created) => self.apply_created_worktree(created),
                CreateWorktreeOutcome::Failed(error) => self.apply_create_worktree_error(error),
            }
        }
    }

    /// Drain the worker-to-UI queue for completed removals and prune stale
    /// aliases from the persisted config.  Must run on the UI thread because
    /// `self.config` is owned directly (not behind `Arc<Mutex>`).
    pub fn drain_remove_worktree_outcomes(&mut self) {
        let outcomes = {
            let mut guard = self.remove_worktree_outcomes.lock().unwrap();
            std::mem::take(&mut *guard)
        };
        for removed in outcomes {
            remove_worktree_alias(&mut self.config, &removed.repo_path, &removed.worktree_path);
            self.save_config();
        }
    }

    pub fn open_rename_worktree(&mut self, repo: Repo, worktree: WorktreeRef) {
        let name = worktree_display_name(&self.config, &repo, &worktree);
        self.rename_worktree_dialog =
            Some(RenameWorktreeDialog::new(repo, worktree.path.clone(), name));
    }

    pub fn execute_rename_worktree(&mut self) {
        let Some(mut state) = self.rename_worktree_dialog.take() else {
            return;
        };
        let worktrees = self.worktrees_snapshot();
        if let Err(err) = state.validate_with_worktrees(&self.config, &worktrees) {
            state.error = Some(err.message().to_string());
            self.rename_worktree_dialog = Some(state);
            return;
        }
        let name = state.name.trim().to_string();
        upsert_worktree_alias(
            &mut self.config,
            &state.repo,
            state.worktree_path.clone(),
            name.clone(),
        );
        self.save_config();
        self.config_status
            .set(format!("renamed worktree label to '{name}'"));
    }

    /// TASK-41: classify every currently bulk-selected worktree into
    /// "removable" (clean + merged + nothing running here) vs "needs
    /// review", reusing the exact same primitives the single-row Remove
    /// dialog uses (`collect_dirty_files` + `assess_branch_delete`) — never
    /// a parallel "is this safe" check. Primary worktrees are dropped
    /// silently: `is_primary_worktree` (the real, canonicalizing check, not
    /// the cheap `w.path == repo.path` the checkbox/chip-count use) is the
    /// defense in depth against a primary ever reaching the dialog at all.
    pub fn open_bulk_remove_worktree_confirm(&mut self) {
        let repos = self.repos_snapshot();
        let worktrees = self.worktrees_snapshot();
        let selected = std::mem::take(&mut self.bulk_selected_worktrees);

        let mut removable = Vec::new();
        let mut needs_review = Vec::new();
        for wt_path in selected {
            let Some(w) = worktrees.iter().find(|w| w.path == wt_path) else {
                continue;
            };
            let Some(repo) = repos.iter().find(|r| r.name == w.repo_name) else {
                continue;
            };
            if is_primary_worktree(&repo.path, &wt_path) {
                continue;
            }
            let display_name = worktree_display_name(&self.config, repo, w);
            let active_run_count = self.snapshot_runs_for_worktree(&wt_path).len();
            let listener_count = self
                .state
                .lock()
                .unwrap()
                .listeners
                .iter()
                .filter(|l| l.worktree_path.as_deref() == Some(wt_path.as_path()))
                .count();
            let candidate =
                classify_bulk_candidate(repo, w, &display_name, active_run_count, listener_count);
            if candidate.is_removable() {
                removable.push(candidate);
            } else {
                needs_review.push(candidate);
            }
        }

        *self.confirm_bulk_remove_worktrees.lock().unwrap() = Some(ConfirmBulkRemoveWorktrees {
            removable,
            needs_review,
            delete_branches: true,
        });
    }

    pub fn cancel_bulk_remove_worktree_confirm(&self) {
        *self.confirm_bulk_remove_worktrees.lock().unwrap() = None;
    }

    /// Removes every `removable` candidate (never `--force`, matching the
    /// invariant that nothing in the bulk sweep is ever force-removed) and,
    /// when opted in, deletes each one's branch with a plain, non-force
    /// `git branch -d` — safe because `removable` only ever contains
    /// candidates `assess_branch_delete` already confirmed are fully merged.
    /// `needs_review` candidates are never touched. Unlike
    /// `execute_remove_worktree`'s single-row dialog, this closes
    /// immediately and reports the outcome via `config_status` — the same
    /// shape as `spawn_backlog_bulk_save`/`spawn_backlog_cleanup` use for
    /// their own batch actions.
    pub fn execute_bulk_remove_worktrees(&self, ctx: &egui::Context) {
        let Some(state) = self.confirm_bulk_remove_worktrees.lock().unwrap().take() else {
            return;
        };
        if state.removable.is_empty() {
            return;
        }

        let status = self.config_status.clone();
        let remove_outcomes = self.remove_worktree_outcomes.clone();
        let worktrees = self.worktrees.clone();
        let scanner_kick = self.scanner_kick.clone();
        let probe_kick = self.probe_kick.clone();
        let detection_kick = self.detection_kick.clone();
        let agent_context_kick = self.agent_context_kick.clone();
        let size_kick = self.size_kick.clone();
        let ctx = ctx.clone();

        thread::spawn(move || {
            let needs_review_count = state.needs_review.len();
            let summary = run_bulk_removal(&state.removable, state.delete_branches);

            for path in &summary.removed {
                worktrees.lock().unwrap().retain(|w| &w.path != path);
            }
            for candidate in &state.removable {
                if summary.removed.contains(&candidate.worktree_path) {
                    remove_outcomes.lock().unwrap().push(RemovedWorktree {
                        repo_path: candidate.repo_path.clone(),
                        worktree_path: candidate.worktree_path.clone(),
                    });
                }
            }

            status.set(summary.status_message(state.removable.len(), needs_review_count));

            scanner_kick.notify();
            probe_kick.notify();
            detection_kick.notify();
            agent_context_kick.notify();
            size_kick.notify();
            ctx.request_repaint();
        });
    }

    fn apply_created_worktree(&mut self, created: CreatedWorktree) {
        upsert_worktree_alias(
            &mut self.config,
            &created.repo,
            created.worktree_path.clone(),
            created.name.clone(),
        );
        self.save_config();
        *self.create_worktree_dialog.lock().unwrap() = None;
        let delta = self.refresh_worktrees_from_disk();
        self.config_status.set(format!(
            "created worktree '{}'; {}",
            created.name,
            delta.summary()
        ));
        self.kick_all();
    }

    fn apply_create_worktree_error(&self, error: String) {
        if let Some(state) = self.create_worktree_dialog.lock().unwrap().as_mut() {
            state.busy = false;
            state.error = Some(error);
        } else {
            self.config_status
                .set(format!("create worktree failed: {error}"));
        }
    }
}

/// TASK-41: classify one selected worktree for the bulk-remove dialog.
/// `review_reason` is `None` iff every safety check passed — a worktree with
/// zero attributed listeners/Switchbard runs, no uncommitted changes, and a
/// branch `assess_branch_delete` already confirmed is fully merged into the
/// repo's default branch. The active-run/listener check has no equivalent in
/// the single-row dialog (which can stop services as part of removal); bulk
/// removal deliberately never does that — a batch action shouldn't silently
/// kill running agent processes across N worktrees, so any of them route to
/// "needs review" instead.
/// Outcome of one `run_bulk_removal` call. `removed` (not just a count) is
/// what the caller needs to prune `worktrees`/queue `RemovedWorktree`
/// outcomes for the exact set that actually succeeded — a partial failure
/// partway through must not be reported as if every candidate landed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BulkRemovalSummary {
    pub removed: Vec<PathBuf>,
    pub branch_deleted: usize,
    pub first_error: Option<String>,
}

impl BulkRemovalSummary {
    /// The `config_status` line `execute_bulk_remove_worktrees` reports.
    /// `total_removable`/`needs_review_count` come from the caller because
    /// this summary only knows what it actually touched, not what the
    /// dialog originally offered.
    pub fn status_message(&self, total_removable: usize, needs_review_count: usize) -> String {
        let mut msg = format!(
            "bulk remove: removed {}/{total_removable} worktree(s)",
            self.removed.len()
        );
        if self.branch_deleted > 0 {
            msg.push_str(&format!(
                " ({} branch{} deleted)",
                self.branch_deleted,
                if self.branch_deleted == 1 { "" } else { "es" }
            ));
        }
        if needs_review_count > 0 {
            msg.push_str(&format!(
                "; {needs_review_count} left for review (dirty/unmerged/active)"
            ));
        }
        if let Some(err) = &self.first_error {
            msg.push_str(&format!("; first failure: {err}"));
        }
        msg
    }
}

/// Removes every candidate in `removable` — never `--force`, matching the
/// invariant that nothing in the bulk sweep is ever force-removed — and,
/// when `delete_branches` is set, deletes each one's branch with a plain,
/// non-force `git branch -d`. Safe because `removable` only ever contains
/// candidates `assess_branch_delete` already confirmed are fully merged
/// (`classify_bulk_candidate`'s doc). `needs_review` candidates are never
/// passed in here at all.
///
/// Deliberately synchronous and free of any `Arc<Mutex<..>>`/`egui::Context`
/// — the real git I/O this needs to prove ("5 selected worktrees really
/// gone, `git worktree list` agrees") is exactly what a test can drive
/// directly, the same way `worktree_remove.rs` and
/// `worktree_removal_orchestration.rs` test `remove_worktree`/
/// `delete_branch` themselves rather than the threaded wrapper around them.
pub fn run_bulk_removal(
    removable: &[BulkRemoveCandidate],
    delete_branches: bool,
) -> BulkRemovalSummary {
    let mut summary = BulkRemovalSummary::default();
    for candidate in removable {
        match remove_worktree(&candidate.repo_path, &candidate.worktree_path, false) {
            Ok(()) => {
                summary.removed.push(candidate.worktree_path.clone());
                if delete_branches {
                    if let (Some(branch), Some(assessment)) =
                        (&candidate.branch, &candidate.branch_assessment)
                    {
                        if !assessment.is_blocked()
                            && delete_branch(&candidate.repo_path, branch, false).is_ok()
                        {
                            summary.branch_deleted += 1;
                        }
                    }
                }
            }
            Err(e) => {
                if summary.first_error.is_none() {
                    summary.first_error = Some(format!("{}: {e}", candidate.display_name));
                }
            }
        }
    }
    summary
}

fn classify_bulk_candidate(
    repo: &Repo,
    w: &WorktreeRef,
    display_name: &str,
    active_run_count: usize,
    listener_count: usize,
) -> BulkRemoveCandidate {
    let dirty = collect_dirty_files(&w.path).ok();
    let is_dirty = dirty.as_ref().is_some_and(|files| !files.is_empty());
    let branch_assessment = w
        .branch
        .as_ref()
        .map(|b| assess_branch_delete(&repo.path, b, &w.path));
    let is_merged = branch_assessment.as_ref().is_some_and(|a| !a.needs_force());

    let review_reason = if dirty.is_none() {
        Some("could not verify git status".to_string())
    } else if is_dirty {
        Some("has uncommitted changes".to_string())
    } else if w.branch.is_none() {
        Some("detached HEAD".to_string())
    } else if !is_merged {
        Some("branch not fully merged".to_string())
    } else if active_run_count > 0 || listener_count > 0 {
        Some(format!(
            "{active_run_count} switchbard run(s), {listener_count} attributed listener(s) still here"
        ))
    } else {
        None
    };

    BulkRemoveCandidate {
        repo_path: repo.path.clone(),
        worktree_path: w.path.clone(),
        display_name: display_name.to_string(),
        branch: w.branch.clone(),
        branch_assessment,
        review_reason,
    }
}
