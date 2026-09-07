//! In-memory PR observations. One bounded request may run off the event thread.
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};
use switchbard_core::{fetch_pull_requests, PrListRow, PrSnapshot};

#[derive(Default)]
pub struct PullRequests {
    pub snapshot: Option<PrSnapshot>,
    pub error: Option<String>,
    pub selected: usize,
    pub scroll: usize,
    pub detail_scroll: u16,
    pub links: std::collections::BTreeMap<String, Vec<(String, String)>>,
    pending: Option<Receiver<Result<PrSnapshot, String>>>,
    attempted: Option<Instant>,
}

impl PullRequests {
    pub fn refresh(&mut self, root: &Path) {
        if self.pending.is_some() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let root = root.to_path_buf();
        self.attempted = Some(Instant::now());
        match std::thread::Builder::new()
            .name("sbt-pr-read".into())
            .spawn(move || {
                // A closed receiver means the app quit; no result remains to publish.
                if tx.send(fetch_pull_requests(&root)).is_err() { /* app already closed */ }
            }) {
            Ok(_) => self.pending = Some(rx),
            Err(error) => self.error = Some(format!("Could not start refresh: {error}")),
        }
    }

    pub fn tick(&mut self, root: &Path, visible: bool, refresh_seconds: u64) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    changed = result.is_ok();
                    self.accept(result);
                }
                Err(TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.error = Some("Refresh worker stopped; retry refresh".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if visible
            && self.error.is_none()
            && self
                .attempted
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(refresh_seconds))
        {
            self.refresh(root);
        }
        changed
    }

    fn accept(&mut self, result: Result<PrSnapshot, String>) {
        match result {
            Ok(mut snapshot) => {
                snapshot
                    .rows
                    .sort_by_key(|row| (row.attention_rank(), std::cmp::Reverse(row.number)));
                let id = self.row().map(|row| row.id.clone());
                self.selected = id
                    .and_then(|id| snapshot.rows.iter().position(|row| row.id == id))
                    .unwrap_or(0);
                self.snapshot = Some(snapshot);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn refresh_links(&mut self, tasks: &[switchbard_core::BacklogTask]) {
        self.links.clear();
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let urls: std::collections::HashSet<&str> =
            snapshot.rows.iter().map(|r| r.url.as_str()).collect();
        for task in tasks {
            for reference in &task.references {
                if urls.contains(reference.as_str()) {
                    let links = self.links.entry(reference.clone()).or_default();
                    if !links.iter().any(|(id, _)| id == &task.id) {
                        links.push((task.id.clone(), task.title.clone()));
                    }
                }
            }
        }
    }

    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }
    pub fn row(&self) -> Option<&PrListRow> {
        self.snapshot.as_ref()?.rows.get(self.selected)
    }
    pub fn step(&mut self, delta: isize) {
        let count = self.snapshot.as_ref().map_or(0, |s| s.rows.len());
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
    }
}
