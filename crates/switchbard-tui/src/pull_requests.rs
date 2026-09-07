//! In-memory PR observations. One bounded request may run off the event thread.
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};
use switchbard_core::{fetch_pull_requests_with_limit, PrListRow, PrSnapshot};

#[derive(Default)]
pub struct PullRequests {
    pub snapshot: Option<PrSnapshot>,
    pub error: Option<String>,
    pub selected: usize,
    pub scroll: usize,
    pub detail_scroll: u16,
    pub filter: String,
    pub visible: Vec<usize>,
    requested_limit: usize,
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
        let limit = self.requested_limit.max(100);
        self.attempted = Some(Instant::now());
        match std::thread::Builder::new()
            .name("sbt-pr-read".into())
            .spawn(move || {
                // A closed receiver means the app quit; no result remains to publish.
                if tx
                    .send(fetch_pull_requests_with_limit(&root, limit))
                    .is_err()
                { /* app already closed */ }
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
                self.snapshot = Some(snapshot);
                self.error = None;
                self.refilter();
                self.selected = id
                    .and_then(|id| {
                        self.visible.iter().position(|&i| {
                            self.snapshot.as_ref().expect("accepted snapshot").rows[i].id == id
                        })
                    })
                    .unwrap_or(0);
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

    pub fn load_more(&mut self, root: &Path) {
        if self.loading()
            || self
                .snapshot
                .as_ref()
                .is_some_and(|s| !s.truncated || s.limit >= switchbard_core::MAX_PULL_REQUESTS)
        {
            return;
        }
        self.requested_limit =
            (self.requested_limit.max(100) + 100).min(switchbard_core::MAX_PULL_REQUESTS);
        self.refresh(root);
    }

    pub fn refilter(&mut self) {
        let filter = crate::tasks::Filter::parse(&self.filter);
        self.visible = self
            .snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| {
                        filter.matches_values(&[&row.number.to_string(), &row.title], |field| {
                            match field {
                                crate::tasks::FilterField::Status => {
                                    vec![row.lifecycle.label().to_string()]
                                }
                                crate::tasks::FilterField::Id => vec![row.number.to_string()],
                                _ => Vec::new(),
                            }
                        })
                    })
                    .map(|(index, _)| index)
                    .collect()
            })
            .unwrap_or_default();
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
        self.scroll = self.scroll.min(self.selected);
    }

    pub fn loading(&self) -> bool {
        self.pending.is_some()
    }
    pub fn row(&self) -> Option<&PrListRow> {
        self.snapshot
            .as_ref()?
            .rows
            .get(*self.visible.get(self.selected)?)
    }
    pub fn step(&mut self, delta: isize) {
        let previous = self.selected;
        let count = self.visible.len();
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(count.saturating_sub(1));
        if self.selected != previous {
            self.detail_scroll = 0;
        }
    }
}
