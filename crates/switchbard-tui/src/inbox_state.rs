//! Cached Inbox projection and durable owner intentions.
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use switchbard_core::bug_run::{Run, RunState, Store};

type ReadResult = Result<Vec<Run>, String>;

#[derive(Default)]
pub struct Inbox {
    pub rows: Vec<Run>,
    pub selected: usize,
    pub scroll: u16,
    pub editing: bool,
    pub publish_confirmation: Option<Run>,
    pub confirmation_visible: bool,
    pub draft: String,
    pub diff: Option<String>,
    diff_reader: Option<Receiver<Result<(String, String), String>>>,
    action_reader: Option<Receiver<Result<Run, String>>>,
    pub draft_revision: u64,
    pub error: Option<String>,
    pub read_error: Option<String>,
    pub path: Option<PathBuf>,
    checked: Option<Instant>,
    reader: Option<Receiver<ReadResult>>,
    launched: std::collections::HashSet<String>,
}

impl Inbox {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            ..Self::default()
        }
    }
    pub fn row(&self) -> Option<&Run> {
        self.rows.get(self.selected)
    }
    pub fn attention(&self) -> usize {
        self.rows.iter().filter(|r| r.state.needs_owner()).count()
    }
    pub fn store(&self) -> Result<Store> {
        Store::open(self.path.as_ref().context("Inbox storage unavailable")?)
    }
    pub fn update(&mut self, run: Run) {
        if let Some(row) = self.rows.iter_mut().find(|r| r.id == run.id) {
            *row = run;
        } else {
            self.rows.insert(0, run);
            self.selected = 0;
        }
        self.checked = None;
    }
    pub fn retry_launch(&mut self, id: &str) {
        self.launched.remove(id);
        self.error = None;
    }
    pub fn reconcile(&mut self) {
        if self.action_reader.is_some() {
            return;
        }
        let (Some(run), Some(path)) = (self.row().cloned(), self.path.clone()) else {
            return;
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        self.action_reader = Some(receiver);
        std::thread::spawn(move || {
            let result =
                switchbard_core::bug_run::reconcile_publication(&path, &run.id, run.revision)
                    .map_err(|e| e.to_string());
            if sender.send(result).is_err() { /* view closed, durable result retained */ }
        });
    }
    pub fn show_diff(&mut self) {
        if self.diff.is_some() {
            self.diff = None;
            self.scroll = 0;
            return;
        }
        if self.diff_reader.is_some() {
            return;
        }
        let Some(run) = self.row().cloned() else {
            return;
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        self.diff_reader = Some(receiver);
        std::thread::spawn(move || {
            let result = switchbard_core::bug_run::review_diff(&run)
                .map(|diff| (run.id, diff))
                .map_err(|e| e.to_string());
            if sender.send(result).is_err() { /* Inbox closed */ }
        });
    }
    pub fn move_by(&mut self, delta: isize) {
        self.diff = None;
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.rows.len().saturating_sub(1));
        self.scroll = 0;
    }
    pub fn begin_reply(&mut self) {
        let Some(run) = self.row() else {
            return;
        };
        if !matches!(
            run.state,
            RunState::AwaitingAnswer | RunState::AwaitingReview
        ) {
            return;
        }
        let (draft, revision) = (run.draft.clone(), run.revision);
        self.draft = draft;
        self.draft_revision = revision;
        self.editing = true;
        self.diff = None;
    }
    pub fn save_draft(&mut self) -> Result<()> {
        let id = self.row().context("No selected request")?.id.clone();
        let run = self
            .store()?
            .save_draft(&id, self.draft_revision, &self.draft)?;
        self.draft_revision = run.revision;
        self.update(run);
        Ok(())
    }
    pub fn keep_both_drafts(&mut self) -> Result<()> {
        let id = self.row().context("No selected request")?.id.clone();
        let mut store = self.store()?;
        let current = store.get(&id)?;
        let draft = if current.draft == self.draft {
            self.draft.clone()
        } else {
            format!(
                "{}\n\nDraft from this window:\n{}",
                current.draft, self.draft
            )
        };
        let saved = store.save_draft(&id, current.revision, &draft)?;
        self.draft = saved.draft.clone();
        self.draft_revision = saved.revision;
        self.update(saved);
        Ok(())
    }
    pub fn submit(&mut self) -> Result<()> {
        self.save_draft()?;
        let run = self.row().context("No selected request")?;
        let run = self.store()?.submit_reply(&run.id, self.draft_revision)?;
        self.editing = false;
        self.update(run);
        Ok(())
    }
    pub fn tick(&mut self, repo: &Path, reports: Option<&Path>) {
        if let Some(receiver) = &self.action_reader {
            match receiver.try_recv() {
                Ok(Ok(run)) => {
                    self.update(run);
                    self.action_reader = None;
                    self.error = None;
                }
                Ok(Err(error)) => {
                    self.error = Some(error);
                    self.action_reader = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error =
                        Some("Inbox action stopped; reload to inspect durable state".into());
                    self.action_reader = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.diff_reader {
            match receiver.try_recv() {
                Ok(Ok((id, diff))) => {
                    if self.row().is_some_and(|r| r.id == id) {
                        self.diff = Some(diff);
                        self.scroll = 0;
                    }
                    self.diff_reader = None;
                }
                Ok(Err(error)) => {
                    self.error = Some(error);
                    self.diff_reader = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("Diff reader stopped".into());
                    self.diff_reader = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.reader {
            match receiver.try_recv() {
                Ok(result) => {
                    self.reader = None;
                    self.apply_read(result);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.reader = None;
                    self.read_error = Some("Inbox reader stopped; retrying".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        self.launch_ready();
        if self.reader.is_some()
            || self
                .checked
                .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        let roots = vec![repo.to_path_buf(), reports.unwrap_or(repo).to_path_buf()];
        let (sender, receiver) = mpsc::sync_channel(1);
        self.checked = Some(Instant::now());
        self.reader = Some(receiver);
        std::thread::spawn(move || {
            let result = read_rows(&path, &roots).map_err(|error| error.to_string());
            if sender.send(result).is_err() { /* the Inbox was closed */ }
        });
    }
    fn apply_read(&mut self, result: ReadResult) {
        match result {
            Ok(mut rows) => {
                for existing in &self.rows {
                    if let Some(row) = rows.iter_mut().find(|row| row.id == existing.id) {
                        if row.revision < existing.revision {
                            *row = existing.clone();
                        }
                    } else {
                        rows.push(existing.clone());
                    }
                }
                let selected = self.row().map(|r| r.id.clone());
                self.rows = rows;
                self.selected = selected
                    .and_then(|id| self.rows.iter().position(|r| r.id == id))
                    .unwrap_or(0);
                self.read_error = None;
            }
            Err(error) => self.read_error = Some(error),
        }
    }
    fn launch_ready(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        self.launched
            .retain(|id| self.rows.iter().any(|r| &r.id == id && r.state.is_queued()));
        for run in self.rows.iter().filter(|r| r.state.is_queued()).take(4) {
            if !self.launched.insert(run.id.clone()) {
                continue;
            }
            if let Err(error) = crate::bug_supervisor::launch(&path, &run.id, run.revision) {
                let message = format!("Cannot start supervisor: {error}. Use :retry in Inbox.");
                match Store::open(&path)
                    .and_then(|mut store| store.fail_launch(&run.id, run.revision, &message))
                {
                    Ok(_) => self.checked = None,
                    Err(error) => {
                        self.error = Some(format!("{message} Could not save failure: {error}"))
                    }
                }
            }
        }
    }
}

fn read_rows(path: &Path, roots: &[PathBuf]) -> Result<Vec<Run>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut store = Store::open(path)?;
    let mut rows = Vec::new();
    for root in roots.iter().take(2) {
        store.reconcile_interrupted(root)?;
        for run in store.list(root)? {
            if !rows.iter().any(|r: &Run| r.id == run.id) {
                rows.push(run);
            }
        }
    }
    rows.sort_by_key(|r| (!r.state.needs_owner(), std::cmp::Reverse(r.updated_at_unix)));
    Ok(rows)
}

pub fn state_label(state: RunState) -> &'static str {
    match state {
        RunState::AgentQueued => "Queued for Codex",
        RunState::Running => "Codex working",
        RunState::AwaitingAnswer => "Your answer needed",
        RunState::ResumeQueued => "Reply queued",
        RunState::AwaitingReview => "Your review needed",
        RunState::PublishQueued => "PR queued",
        RunState::Publishing => "Opening PR",
        RunState::PrOpen => "PR open",
        RunState::Failed => "Needs retry",
        RunState::Unknown => "Outcome unknown",
    }
}
