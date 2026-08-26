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
    assess_branch_delete, create_worktree, delete_branch, is_primary_worktree, probe_facts,
    remove_worktree, AttachedProcesses, Fact, RemovalIntent, RemovalSafety, RemovalVerdict, Repo,
    WorktreeRef,
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
    /// "removable" (every safety check passed) vs "needs review", through the
    /// one shared `RemovalSafety` evaluation — never a parallel "is this
    /// safe" check. Primary worktrees are dropped
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
            let candidate =
                classify_bulk_candidate(repo, w, &display_name, self.attached_processes(&wt_path));
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
        let progress = self.worktree_bulk_progress.clone();
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
            // Each candidate is its own `git worktree remove` (plus maybe a
            // `git branch -d`), so a nine-worktree sweep is many seconds during
            // which the dialog has already closed and the list has not changed
            // yet. Without this the run is indistinguishable from a hang.
            progress.begin("removing", state.removable.len());
            let summary = run_bulk_removal(&state.removable, state.delete_branches, || {
                progress.advance();
                // Repaint per item: the bar lives on the render path, and the
                // worker is the only thing that knows it moved.
                ctx.request_repaint();
            });
            progress.finish();

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

/// Outcome of one `run_bulk_removal` call. `removed` (not just a count) is
/// what the caller needs to prune `worktrees`/queue `RemovedWorktree`
/// outcomes for the exact set that actually succeeded — a partial failure
/// partway through must not be reported as if every candidate landed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BulkRemovalSummary {
    pub removed: Vec<PathBuf>,
    pub branch_deleted: usize,
    pub first_error: Option<String>,
    /// First `git branch -d` failure, if any — kept separate from
    /// `first_error` because it's a strictly less severe class: the
    /// worktree behind it is already gone (that's `first_error`'s job to
    /// report), only the branch itself lingers. Silently swallowing this
    /// (as the very first cut of this function did) left the status line's
    /// "N branches deleted" undercount unexplained.
    pub first_branch_error: Option<String>,
    /// Branches deliberately left in place because their work landed under
    /// *different commits* (a rebase merge). Nothing is at risk — the content
    /// is already in the base — but git's own `branch -d` guard is
    /// ancestry-based and would refuse, and this sweep does not force-delete.
    /// Counted so the status line explains the shortfall rather than leaving
    /// "N branches deleted" quietly short of the worktrees removed.
    pub branch_left_rebase_merged: usize,
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
        if self.branch_left_rebase_merged > 0 {
            msg.push_str(&format!(
                "; {} branch{} kept (rebase-merged, so `branch -d` would refuse)",
                self.branch_left_rebase_merged,
                if self.branch_left_rebase_merged == 1 {
                    ""
                } else {
                    "es"
                }
            ));
        }
        if let Some(err) = &self.first_branch_error {
            msg.push_str(&format!("; branch delete failed: {err}"));
        }
        msg
    }
}

/// Removes every candidate in `removable` — never `--force`, matching the
/// invariant that nothing in the bulk sweep is ever force-removed — and,
/// when `delete_branches` is set, deletes each one's branch with a plain,
/// non-force `git branch -d`.
///
/// The branch step is gated on `assess_branch_delete`'s **ancestry** verdict,
/// not on the worktree being removable. Those came apart when `WorkLanded`
/// started accepting patch equivalence: a rebase-merged branch is now
/// correctly safe to remove (its content is in the base) while git's own
/// `branch -d` guard, which only looks at reachability, would still refuse.
/// Rather than reach for `-D` on our own authority, the sweep removes the
/// worktree and leaves the branch, counting it in
/// `branch_left_rebase_merged` so the status line says so.
///
/// `needs_review` candidates are never passed in here at all.
///
/// Deliberately synchronous and free of any `Arc<Mutex<..>>`/`egui::Context`
/// — the real git I/O this needs to prove ("5 selected worktrees really
/// gone, `git worktree list` agrees") is exactly what a test can drive
/// directly, the same way `worktree_remove.rs` and
/// `worktree_removal_orchestration.rs` test `remove_worktree`/
/// `delete_branch` themselves rather than the threaded wrapper around them.
///
/// `on_item_done` fires once per candidate, on **every** exit path including a
/// failed removal — it measures how far through the batch we are, not how much
/// of it worked, and a bar that stalls on the first failure is worse than no
/// bar. It is a plain callback rather than a `Progress` handle so this stays
/// free of shared state: the caller owns the wiring, and a test can count
/// invocations without standing up a channel.
pub fn run_bulk_removal(
    removable: &[BulkRemoveCandidate],
    delete_branches: bool,
    mut on_item_done: impl FnMut(),
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
                        if assessment.is_blocked() {
                            // Checked out elsewhere; git would refuse either way.
                        } else if assessment.needs_force() {
                            // Safe to remove, but only patch-equivalent to the
                            // base. Leave the branch rather than force past
                            // git's guard on our own say-so.
                            summary.branch_left_rebase_merged += 1;
                        } else {
                            match delete_branch(&candidate.repo_path, branch, false) {
                                Ok(()) => summary.branch_deleted += 1,
                                Err(e) => {
                                    if summary.first_branch_error.is_none() {
                                        summary.first_branch_error = Some(format!("{branch}: {e}"));
                                    }
                                }
                            }
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
        on_item_done();
    }
    summary
}

/// Classify one selected worktree for the bulk-remove dialog.
///
/// The rules are not here: they are
/// `switchbard_core::removal_safety::RemovalSafety`, the same evaluation the
/// Workspace row's badge and the single-row dialog run. This function only
/// gathers facts and translates a verdict into the dialog's two lists. That
/// is the point - this used to hold its own five-gate ladder while the row
/// held a three-check one, so the same worktree could read "remove ok" on the
/// row and land in "needs review" here in the same frame.
///
/// The intent is [`RemovalIntent::WorktreeAndBranch`] because
/// `ConfirmBulkRemoveWorktrees::delete_branches` defaults on, so an unmerged
/// branch really would lose commits. `run_bulk_removal` then only ever runs a
/// plain `git branch -d`, which is safe precisely because nothing reaches it
/// without `WorkLanded` having passed.
///
/// Facts come from `probe_facts` - fresh, synchronous git at the moment the
/// dialog opens - never from the cached `WorktreeMeta` the row reads. A
/// confirmation has to describe the worktree as it is now.
fn classify_bulk_candidate(
    repo: &Repo,
    w: &WorktreeRef,
    display_name: &str,
    attached: AttachedProcesses,
) -> BulkRemoveCandidate {
    let facts = probe_facts(
        &repo.path,
        &w.path,
        w.branch.as_deref(),
        Fact::Known(attached),
    );
    let safety = RemovalSafety::evaluate(&facts, RemovalIntent::WorktreeAndBranch);
    // Still needed after the verdict, and for a different question: whether
    // git would even accept `branch -d`. A branch checked out in another
    // worktree is not *unsafe* to leave behind, it is simply undeletable, so
    // it is not one of the safety checks - `run_bulk_removal` skips the
    // branch step for it and removes the worktree anyway.
    let branch_assessment = w
        .branch
        .as_ref()
        .map(|b| assess_branch_delete(&repo.path, b, &w.path));

    let review_reason = match safety.verdict() {
        RemovalVerdict::Safe => None,
        // `probe_facts` answers every check synchronously, so `Checking` is
        // unreachable here. Refuse rather than assume: a verdict this code
        // cannot explain must not become a removal.
        _ => Some(
            safety
                .blocking_reason()
                .unwrap_or_else(|| "couldn't establish that this is safe to remove".to_string()),
        ),
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
