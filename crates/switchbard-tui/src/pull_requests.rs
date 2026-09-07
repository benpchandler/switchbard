//! In-memory PR observations. One bounded request may run off the event thread.
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};
use switchbard_core::{fetch_pull_requests_with_limit, PrListRow, PrSnapshot};

#[derive(Default)]
pub struct PullRequests {
    pub snapshot: Option<PrSnapshot>,
    pub error: Option<String>,
    pub notifications: crate::pr_notifications::PrNotifications,
    pub selected: usize,
    pub scroll: usize,
    pub detail_scroll: u16,
    pub filter: String,
    pub sort: Option<crate::sort::Sort>,
    pending_selection: Option<String>,
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
            Err(error) => self.fail(format!("Could not start refresh: {error}")),
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
                    self.fail("Refresh worker stopped; retry refresh".into());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if (visible || self.attempted.is_some())
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
                self.observe_notifications(&snapshot);
                snapshot
                    .rows
                    .sort_by_key(|row| (row.attention_rank(), std::cmp::Reverse(row.number)));
                if self.pending_selection.is_none() {
                    self.pending_selection = self.row().map(|row| row.id.clone());
                }
                self.snapshot = Some(snapshot);
                self.error = None;
                // App refreshes task links before projecting the accepted snapshot.
            }
            Err(error) => self.fail(error),
        }
    }

    fn fail(&mut self, error: String) {
        if self.error.as_ref() != Some(&error) {
            self.notifications.push(format!("Refresh failed: {error}"));
        }
        self.error = Some(error);
    }

    fn observe_notifications(&mut self, snapshot: &PrSnapshot) {
        self.notifications.observe(self.snapshot.as_ref(), snapshot);
        if self.error.is_some() {
            self.notifications.push("Refresh recovered".into());
        }
        let previous_warning = self
            .snapshot
            .as_ref()
            .and_then(|s| s.enrichment_warning.as_ref());
        if previous_warning != snapshot.enrichment_warning.as_ref() {
            if let Some(warning) = &snapshot.enrichment_warning {
                self.notifications
                    .push(format!("Delivery details unavailable: {warning}"));
            } else if previous_warning.is_some() {
                self.notifications.push("Delivery details recovered".into());
            }
        }
    }

    pub fn dismiss_notifications(&mut self) {
        self.notifications.dismiss();
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

    pub fn selection_identity(&self) -> Option<String> {
        self.pending_selection
            .clone()
            .or_else(|| self.row().map(|row| row.id.clone()))
    }

    pub fn restore_selection(&mut self, id: Option<String>) {
        self.pending_selection = id;
    }

    pub fn values(&self, column: crate::columns::Column, row: &PrListRow) -> Vec<String> {
        let links = self.links.get(&row.url).map(Vec::as_slice).unwrap_or(&[]);
        column.pr_values(row, links)
    }

    pub fn matches(&self, filter: &crate::tasks::Filter, row: &PrListRow) -> bool {
        filter.matches_values(&[&row.number.to_string(), &row.title], |field| {
            self.values(field.column(), row)
        })
    }

    pub fn column_values(&self, column: crate::columns::Column) -> Vec<(String, usize)> {
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        if let Some(snapshot) = &self.snapshot {
            for row in &snapshot.rows {
                for value in self.values(column, row) {
                    *counts.entry(value).or_default() += 1;
                }
            }
        }
        for value in column.spec().vocabulary {
            counts.entry((*value).to_string()).or_default();
        }
        let mut values: Vec<_> = counts.into_iter().collect();
        values.sort_by(|a, b| {
            if column == crate::columns::Column::Id {
                a.0.parse::<u64>()
                    .unwrap_or(u64::MAX)
                    .cmp(&b.0.parse::<u64>().unwrap_or(u64::MAX))
            } else {
                column
                    .vocabulary_rank(&a.0)
                    .cmp(&column.vocabulary_rank(&b.0))
                    .then_with(|| b.1.cmp(&a.1))
                    .then_with(|| a.0.cmp(&b.0))
            }
        });
        values
    }

    pub fn refilter(&mut self) {
        let Some(_) = self.snapshot else {
            return;
        };
        let restored = self.pending_selection.take();
        let selected = restored
            .clone()
            .or_else(|| self.row().map(|row| row.id.clone()));
        let filter = crate::tasks::Filter::parse(&self.filter);
        let mut visible: Vec<usize> = self
            .snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| self.matches(&filter, row))
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default();
        if let Some(snapshot) = &self.snapshot {
            if let Some(sort) = self.sort {
                visible.sort_by(|&a, &b| self.compare(&snapshot.rows[a], &snapshot.rows[b], sort));
            }
        }
        self.visible = visible;
        self.selected = selected
            .and_then(|id| {
                self.visible
                    .iter()
                    .position(|&i| self.snapshot.as_ref().is_some_and(|s| s.rows[i].id == id))
            })
            .unwrap_or_else(|| {
                if restored.is_some() {
                    0
                } else {
                    self.selected.min(self.visible.len().saturating_sub(1))
                }
            });
        self.scroll = self.scroll.min(self.selected);
    }

    fn compare(&self, a: &PrListRow, b: &PrListRow, sort: crate::sort::Sort) -> std::cmp::Ordering {
        use crate::{columns::Column, sort::Order};
        let values = |row| self.values(sort.column, row).join(",");
        let order = match sort.order {
            Order::Semantic => sort
                .column
                .vocabulary_rank(&values(a))
                .cmp(&sort.column.vocabulary_rank(&values(b))),
            _ if sort.column == Column::Id => a.number.cmp(&b.number),
            _ => values(a).to_lowercase().cmp(&values(b).to_lowercase()),
        };
        let order = if sort.order == Order::Descending {
            order.reverse()
        } else {
            order
        };
        order.then_with(|| a.number.cmp(&b.number))
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
