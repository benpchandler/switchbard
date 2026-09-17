//! Single-flight background storage observation, with stale-result rejection after local edits.
use super::{App, Mode};
use crate::{config, tasks};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};
use switchbard_core::{BacklogStorageIdentity, BacklogTask, BacklogTaskSnapshot};

struct Refresh {
    generation: u64,
    sequence: Option<u64>,
    modified: Option<SystemTime>,
    backlog: Option<tasks::Backlog>,
    detail: Option<BacklogTaskSnapshot>,
}

struct DetailTarget {
    id: String,
    identity: Option<BacklogStorageIdentity>,
}

impl DetailTarget {
    fn matches(&self, task: &BacklogTask) -> bool {
        match (&self.identity, &task.storage_identity) {
            (Some(old), Some(new)) => {
                old.repository_id == new.repository_id && old.record_id == new.record_id
            }
            (None, _) => self.id == task.id,
            _ => false,
        }
    }
}

struct RefreshRequest {
    root: PathBuf,
    generation: u64,
    seen: (Option<u64>, Option<SystemTime>),
    force: bool,
    detail_target: Option<DetailTarget>,
}

impl RefreshRequest {
    fn read(self) -> anyhow::Result<Refresh> {
        let sequence = switchbard_core::storage::current_change_sequence()?;
        let modified = config::modified_at(&self.root.join("backlog/tasks"));
        let (backlog, detail) = if self.force || self.seen != (sequence, modified) {
            self.read_backlog()?
        } else {
            (None, None)
        };
        Ok(Refresh {
            generation: self.generation,
            sequence,
            modified,
            backlog,
            detail,
        })
    }

    fn read_backlog(
        &self,
    ) -> anyhow::Result<(Option<tasks::Backlog>, Option<BacklogTaskSnapshot>)> {
        let _detail_lock = self
            .detail_target
            .as_ref()
            .map(|_| switchbard_core::storage::RepositoryLock::acquire(&self.root))
            .transpose()?;
        let backlog = tasks::load(&self.root)?;
        let detail = self
            .detail_target
            .as_ref()
            .and_then(|target| backlog.tasks.iter().find(|task| target.matches(task)))
            .filter(|task| task.editable())
            .map(|task| switchbard_core::read_backlog_task_snapshot(&self.root, &task.id))
            .transpose()?;
        Ok((Some(backlog), detail))
    }
}

#[derive(Default)]
pub(super) struct TaskRefresh {
    pending: Option<Receiver<Result<Refresh, String>>>,
    force: bool,
}

impl App {
    pub(super) fn defer_task_storage(&mut self) -> bool {
        if self.report.is_pending() || self.task_refresh_pending() {
            self.status =
                "Task refresh or report save in progress; retry editing when finished".into();
            return true;
        }
        false
    }

    pub(super) fn request_task_refresh(&mut self) {
        self.task_refresh.force = true;
        self.storage_checked = None;
    }

    pub fn task_refresh_pending(&self) -> bool {
        self.task_refresh.force || self.task_refresh.pending.is_some()
    }

    pub(super) fn tick_task_refresh(&mut self) {
        if let Some(rx) = &self.task_refresh.pending {
            match rx.try_recv() {
                Ok(result) => {
                    self.task_refresh.pending = None;
                    self.accept_task_refresh(result);
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.task_refresh.pending = None;
                    self.fail_task_refresh("task refresh worker stopped; retrying".into());
                }
            }
        }
        if self.report.is_pending()
            || self
                .storage_checked
                .is_some_and(|last| last.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.start_task_refresh();
    }

    fn start_task_refresh(&mut self) {
        let detail_target = (self.mode == Mode::DetailFocus)
            .then(|| self.selected_task())
            .flatten()
            .map(|task| DetailTarget {
                id: task.id.clone(),
                identity: task.storage_identity.clone(),
            });
        let request = RefreshRequest {
            root: self.repo_root.clone(),
            generation: self.task_generation,
            seen: (self.storage_seen, self.tasks_seen),
            force: std::mem::take(&mut self.task_refresh.force) || self.storage_retry,
            detail_target,
        };
        let (tx, rx) = mpsc::sync_channel(1);
        self.storage_checked = Some(Instant::now());
        match std::thread::Builder::new()
            .name("sbt-task-refresh".into())
            .spawn(move || {
                if tx
                    .send(request.read().map_err(|error| error.to_string()))
                    .is_err()
                { /* app closed */ }
            }) {
            Ok(_) => self.task_refresh.pending = Some(rx),
            Err(error) => self.fail_task_refresh(format!("task refresh: {error}")),
        }
    }

    fn fail_task_refresh(&mut self, message: String) {
        self.storage_retry = true;
        if self.mode == Mode::DetailFocus {
            self.detail_draft = None;
        }
        self.fail(message);
    }

    fn accept_task_refresh(&mut self, result: Result<Refresh, String>) {
        match result {
            Ok(refresh) if refresh.generation == self.task_generation => {
                self.storage_seen = refresh.sequence;
                self.tasks_seen = refresh.modified;
                if let Some(backlog) = refresh.backlog {
                    self.accept_tasks(Ok(backlog), false);
                    self.accept_refreshed_detail(refresh.detail);
                    self.select_filed_report();
                }
            }
            Ok(_) => self.request_task_refresh(),
            Err(error) => self.fail_task_refresh(format!("task storage: {error}")),
        }
    }

    fn accept_refreshed_detail(&mut self, snapshot: Option<BacklogTaskSnapshot>) {
        if self.mode != Mode::DetailFocus {
            return;
        }
        let current = snapshot.as_ref().is_some_and(|snapshot| {
            self.selected_task().is_some_and(|task| {
                task.id == snapshot.task_id && task.storage_identity == snapshot.identity
            })
        });
        if current {
            self.detail_draft = snapshot;
        } else {
            self.detail_draft = None;
            if self.selected_task().is_some_and(|task| task.editable()) {
                self.request_task_refresh();
            }
        }
    }
}
