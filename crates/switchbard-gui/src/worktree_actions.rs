use crate::app::HiveApp;
use crate::runtime::worktree_create::{
    CreateWorktreeDialog, CreateWorktreeOutcome, CreatedWorktree,
};
use crate::runtime::worktree_names::{upsert_worktree_alias, worktree_display_name};
use crate::runtime::worktree_rename::RenameWorktreeDialog;
use eframe::egui;
use std::thread;
use switchbard_core::{create_worktree, Repo, WorktreeRef};

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
