//! In-memory PR observations. One bounded request may run off the event thread.
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};
use switchbard_core::{fetch_pull_requests_with_limit, PrLifecycle, PrListRow, PrSnapshot};

/// Poll cadence while a just-merged PR hasn't shown as merged (or gone) yet
/// (TASK-204). GitHub's list read is eventually consistent, so the routine
/// `pr_refresh_seconds` cadence (default 60s) would otherwise leave the row
/// looking untouched for up to a minute after the user watched the merge land.
const EXPECTED_MERGE_POLL_SECONDS: u64 = 3;
/// Bound on how long the tightened cadence runs before giving up on this PR
/// and falling back to the routine cadence with a notification.
const EXPECTED_MERGE_WINDOW_SECONDS: u64 = 90;

/// A word-processor-style shift-up/shift-down range select, anchored at the
/// row the first extend started from. `added` is the sweep's own bookkeeping
/// of which ids it has marked so far, kept separate from `marked` itself: a
/// row `space` marked outside the sweep is never in `added`, so contracting
/// the range back over it leaves it alone (only what the sweep put there is
/// the sweep's to take away).
#[derive(Debug, Clone, Default)]
struct RangeSweep {
    anchor_id: String,
    added: std::collections::BTreeSet<String>,
}

/// One outstanding "this PR should show merged soon" expectation (TASK-204).
/// Single-slot by design: a second confirmed merge while one is outstanding
/// replaces it. The traded-off case (two merges landing within the same
/// window) drops fast-poll tracking for the first PR in favor of the second;
/// both still converge on the routine cadence at worst.
struct ExpectedMerge {
    id: String,
    head_oid: String,
    number: u64,
    since: Instant,
}

#[derive(Default)]
pub struct PullRequests {
    /// What columns exist. The PR table shows only built-ins, but the filter
    /// grammar is shared with the task page, so the same registry answers both.
    registry: Arc<crate::columns::ColumnRegistry>,
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
    completed_at: Option<Instant>,
    remaining_seconds: u64,
    last_open_count: Option<(u64, std::time::SystemTime)>,
    expected_merge: Option<ExpectedMerge>,
    /// PR ids marked for a bulk merge (`mark`); consumed in visible order by `m`.
    pub marked: std::collections::BTreeSet<String>,
    /// The in-progress shift-up/shift-down range select, if one is live.
    range_sweep: Option<RangeSweep>,
}

impl PullRequests {
    /// Take up the registry `App` rebuilt for this repo load.
    pub fn set_registry(&mut self, registry: Arc<crate::columns::ColumnRegistry>) {
        self.registry = registry;
    }

    pub fn refresh(&mut self, root: &Path) {
        if self.pending.is_some() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let root = root.to_path_buf();
        let limit = self.requested_limit.max(100);
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
            Err(error) => {
                self.fail(format!("Could not start refresh: {error}"));
                self.completed_at = Some(Instant::now());
            }
        }
    }

    pub fn tick(&mut self, root: &Path, refresh_seconds: u64, now: Instant) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    changed = result.is_ok();
                    self.accept(result);
                    self.completed_at = Some(now);
                    if changed {
                        self.reconcile_expected_merge();
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.fail("Refresh worker stopped; retry refresh".into());
                    self.completed_at = Some(now);
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        self.expire_expected_merge(now);
        let cadence_seconds = if self.expected_merge.is_some() {
            EXPECTED_MERGE_POLL_SECONDS
        } else {
            refresh_seconds
        };
        let elapsed = self
            .completed_at
            .map(|completed| now.saturating_duration_since(completed));
        self.remaining_seconds = elapsed.map_or(0, |elapsed| {
            Duration::from_secs(cadence_seconds)
                .saturating_sub(elapsed)
                .as_secs()
        });
        if elapsed.is_none_or(|elapsed| elapsed >= Duration::from_secs(cadence_seconds)) {
            self.refresh(root);
        }
        changed
    }

    /// Called once GitHub confirms a merge landed (`MergeFlow`'s only write
    /// into this expectation, never on a rejected or unknown outcome).
    /// `PullRequests` owns the expectation and the cadence it drives; the
    /// merge flow only tells it what to expect (single writer, TASK-204).
    pub fn expect_merge(&mut self, id: String, head_oid: String, since: Instant) {
        let number = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.rows.iter().find(|row| row.id == id))
            .map_or(0, |row| row.number);
        self.expected_merge = Some(ExpectedMerge {
            id,
            head_oid,
            number,
            since,
        });
    }

    /// Whether `row` is the one row an outstanding merge expectation covers,
    /// so rendering can mark it pending without a second copy of the state.
    pub fn expecting_merge(&self, row: &PrListRow) -> bool {
        self.expected_merge
            .as_ref()
            .is_some_and(|expected| expected.id == row.id && expected.head_oid == row.head_oid)
    }

    /// Whether the list is on the tightened post-merge cadence, for the
    /// observation header's refresh label.
    pub fn syncing_after_merge(&self) -> bool {
        self.expected_merge.is_some()
    }

    /// Converged = a refreshed snapshot no longer lists the expected PR as
    /// open (row absent, or present and merged). Only called after accepting
    /// a successful refresh.
    fn reconcile_expected_merge(&mut self) {
        let Some(expected) = &self.expected_merge else {
            return;
        };
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let converged = match snapshot.rows.iter().find(|row| row.id == expected.id) {
            None => true,
            Some(row) => row.lifecycle == PrLifecycle::Merged,
        };
        if converged {
            self.expected_merge = None;
        }
    }

    /// Gives up on an expectation that outlived its window, returning to the
    /// routine cadence and telling the user GitHub still disagrees.
    fn expire_expected_merge(&mut self, now: Instant) {
        let Some(expected) = &self.expected_merge else {
            return;
        };
        if now.saturating_duration_since(expected.since)
            < Duration::from_secs(EXPECTED_MERGE_WINDOW_SECONDS)
        {
            return;
        }
        let number = expected.number;
        self.expected_merge = None;
        self.notifications.push(format!(
            "PR #{number} still shows open after merge; check GitHub"
        ));
    }

    fn accept(&mut self, result: Result<PrSnapshot, String>) {
        match result {
            Ok(mut snapshot) => {
                let same_repository = self
                    .snapshot
                    .as_ref()
                    .is_some_and(|previous| previous.repository == snapshot.repository);
                match &snapshot.open_count {
                    Ok(count) => self.last_open_count = Some((*count, snapshot.observed_at)),
                    Err(_) if !same_repository => self.last_open_count = None,
                    Err(_) => {}
                }
                self.observe_notifications(&snapshot);
                snapshot
                    .rows
                    .sort_by_key(|row| (row.attention_rank(), std::cmp::Reverse(row.number)));
                if self.pending_selection.is_none() {
                    self.pending_selection = self.row().map(|row| row.id.clone());
                }
                self.marked.retain(|id| {
                    snapshot
                        .rows
                        .iter()
                        .any(|row| &row.id == id && row.lifecycle == PrLifecycle::Open)
                });
                self.snapshot = Some(snapshot);
                self.error = None;
                // App refreshes task links before projecting the accepted snapshot.
            }
            Err(error) => self.fail(error),
        }
    }

    fn fail(&mut self, error: String) {
        if self.error.as_ref() != Some(&error) && !Self::missing_github_cli(&error) {
            self.notifications.push(format!("Refresh failed: {error}"));
        }
        self.error = Some(error);
    }

    fn missing_github_cli(error: &str) -> bool {
        error.starts_with("Cannot run gh:")
    }

    fn observe_notifications(&mut self, snapshot: &PrSnapshot) {
        let previous = self
            .snapshot
            .as_ref()
            .and_then(|s| s.open_count.as_ref().err());
        let current = snapshot.open_count.as_ref().err();
        if previous != current {
            if let Some(error) = current {
                self.notifications
                    .push(format!("Open PR count unavailable: {error}"));
            } else if previous.is_some() {
                self.notifications.push("Open PR count recovered".into());
            }
        }
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

    /// Move the cursor to `id` if it is visible; false when it is not.
    pub fn select_id(&mut self, id: &str) -> bool {
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        let position = self
            .visible
            .iter()
            .position(|index| snapshot.rows.get(*index).is_some_and(|row| row.id == id));
        match position {
            Some(position) => {
                if position != self.selected {
                    self.detail_scroll = 0;
                }
                self.selected = position;
                true
            }
            None => false,
        }
    }

    /// Flip the cursor row's bulk-merge mark. Only open PRs can be marked.
    pub fn toggle_mark(&mut self) -> Result<bool, &'static str> {
        let Some(row) = self.row() else {
            return Err("No PR selected");
        };
        if row.lifecycle != PrLifecycle::Open {
            return Err("Only open PRs can be marked for merge");
        }
        let id = row.id.clone();
        if self.marked.remove(&id) {
            Ok(false)
        } else {
            self.marked.insert(id);
            Ok(true)
        }
    }

    /// Word-processor-style range select: the first call anchors at the
    /// cursor row; each further call moves the cursor by `delta` (`1` or
    /// `-1`) and marks every open PR between the anchor and the new cursor,
    /// inclusive, unmarking any the range has left behind. Only rows this
    /// sweep itself marked are ever unmarked (`RangeSweep::added`); a row
    /// `space` marked outside the sweep survives a contraction that passes
    /// back over it. Returns the number of rows currently marked, for the
    /// caller's status line.
    pub fn extend_mark(&mut self, delta: isize) -> usize {
        if self.snapshot.is_none() {
            return self.marked.len();
        }
        let anchor_id = match &self.range_sweep {
            Some(sweep) => sweep.anchor_id.clone(),
            None => match self.row() {
                Some(row) => row.id.clone(),
                None => return self.marked.len(),
            },
        };
        self.step(delta);
        let Some(anchor_position) = self.position_of(&anchor_id) else {
            // The anchor row is no longer visible (filtered out from under
            // the sweep); nothing sane left to sweep against.
            self.range_sweep = None;
            return self.marked.len();
        };
        let lo = anchor_position.min(self.selected);
        let hi = anchor_position.max(self.selected);
        let in_range: std::collections::BTreeSet<String> = self.visible[lo..=hi]
            .iter()
            .filter_map(|&index| self.snapshot.as_ref().and_then(|s| s.rows.get(index)))
            .filter(|row| row.lifecycle == PrLifecycle::Open)
            .map(|row| row.id.clone())
            .collect();
        let sweep = self.range_sweep.get_or_insert_with(|| RangeSweep {
            anchor_id: anchor_id.clone(),
            added: std::collections::BTreeSet::new(),
        });
        // Rows the range no longer covers: only the ones this sweep put there
        // itself come back off; a row already marked before the sweep ever
        // touched it (`space`, or a leftover from before) was never recorded
        // in `added`, so it is left exactly as it was.
        for id in &sweep.added {
            if !in_range.contains(id) {
                self.marked.remove(id);
            }
        }
        sweep.added.retain(|id| in_range.contains(id));
        // Rows newly covered: mark them, and record only the ones that were
        // not already marked, so a pre-existing mark is never claimed as the
        // sweep's own to later take back.
        for id in &in_range {
            if !sweep.added.contains(id) && !self.marked.contains(id) {
                sweep.added.insert(id.clone());
            }
            self.marked.insert(id.clone());
        }
        self.marked.len()
    }

    /// The visible position of PR `id`, for the range sweep's anchor.
    fn position_of(&self, id: &str) -> Option<usize> {
        self.visible.iter().position(|&index| {
            self.snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.rows.get(index))
                .is_some_and(|row| row.id == id)
        })
    }

    /// Drop the in-progress range-select anchor: any plain cursor move, page
    /// switch, filter change, or mark clear starts the next extend fresh.
    pub fn reset_range_sweep(&mut self) {
        self.range_sweep = None;
    }

    /// Marked PR ids in the order the list shows them: the bulk-merge order.
    pub fn marked_in_view_order(&self) -> Vec<String> {
        let Some(snapshot) = &self.snapshot else {
            return Vec::new();
        };
        self.visible
            .iter()
            .filter_map(|index| snapshot.rows.get(*index))
            .filter(|row| self.marked.contains(&row.id))
            .map(|row| row.id.clone())
            .collect()
    }

    pub fn is_marked(&self, row: &PrListRow) -> bool {
        self.marked.contains(&row.id)
    }

    pub fn clear_marks(&mut self) {
        self.marked.clear();
        self.reset_range_sweep();
    }

    pub fn values(&self, column: crate::columns::Column, row: &PrListRow) -> Vec<String> {
        let links = self.links.get(&row.url).map(Vec::as_slice).unwrap_or(&[]);
        column.pr_values(row, links)
    }

    pub fn matches(&self, filter: &crate::tasks::Filter, row: &PrListRow) -> bool {
        filter.matches_row(&self.column_adapter(row))
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
        for value in column.spec(&self.registry).vocabulary().iter() {
            counts.entry(value.to_string()).or_default();
        }
        let mut values: Vec<_> = counts.into_iter().collect();
        values.sort_by(|a, b| {
            if column == crate::columns::Column::Id {
                a.0.parse::<u64>()
                    .unwrap_or(u64::MAX)
                    .cmp(&b.0.parse::<u64>().unwrap_or(u64::MAX))
            } else {
                column
                    .vocabulary_rank(&self.registry, &a.0)
                    .cmp(&column.vocabulary_rank(&self.registry, &b.0))
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
        let visible = self.visible_for(&self.filter, self.sort);
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

    pub(crate) fn visible_for(&self, query: &str, sort: Option<crate::sort::Sort>) -> Vec<usize> {
        let filter = crate::tasks::Filter::parse(query, &self.registry);
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
            if let Some(sort) = sort {
                visible.sort_by(|&a, &b| self.compare(&snapshot.rows[a], &snapshot.rows[b], sort));
            }
        }
        visible
    }

    fn compare(&self, a: &PrListRow, b: &PrListRow, sort: crate::sort::Sort) -> std::cmp::Ordering {
        crate::sort::compare_values(
            &self.column_adapter(a),
            &self.column_adapter(b),
            sort,
            &self.registry,
        )
    }

    pub fn column_adapter<'a>(&'a self, row: &'a PrListRow) -> crate::column_values::PrValues<'a> {
        let links = self.links.get(&row.url).map(Vec::as_slice).unwrap_or(&[]);
        crate::column_values::PrValues { row, links }
    }

    pub fn observation_stale(&self, refresh_seconds: u64) -> bool {
        self.error.is_some()
            || self.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot
                    .observed_at
                    .elapsed()
                    .is_ok_and(|age| age.as_secs() >= refresh_seconds.saturating_mul(2))
            })
    }

    pub fn last_open_count(&self) -> Option<(u64, std::time::SystemTime)> {
        self.last_open_count
    }

    pub fn refresh_label(&self) -> String {
        if self.loading() {
            "refreshing".into()
        } else if self.syncing_after_merge() {
            format!("syncing {}s", self.remaining_seconds)
        } else {
            format!("{}s", self.remaining_seconds)
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use switchbard_core::PrSnapshot;

    fn snapshot(repository: &str, open_count: Result<u64, String>) -> PrSnapshot {
        PrSnapshot {
            repository: repository.into(),
            repository_url: format!("https://github.com/{repository}"),
            observed_at: SystemTime::now(),
            rows: Vec::new(),
            truncated: false,
            limit: 100,
            open_count,
            enrichment_warning: None,
        }
    }

    #[test]
    fn count_failure_retains_last_known_positive_and_zero_without_hiding_error() {
        for count in [0, 5] {
            let mut prs = PullRequests::default();
            prs.accept(Ok(snapshot("owner/repo", Ok(count))));
            prs.accept(Ok(snapshot("owner/repo", Err("offline".into()))));

            assert_eq!(prs.last_open_count().map(|(value, _)| value), Some(count));
            assert!(prs.snapshot.unwrap().open_count.is_err());
        }
    }

    #[test]
    fn count_failure_does_not_cross_repository_boundary() {
        let mut prs = PullRequests::default();
        prs.accept(Ok(snapshot("owner/old", Ok(5))));
        prs.accept(Ok(snapshot("owner/new", Err("offline".into()))));

        assert_eq!(prs.last_open_count(), None);
    }

    #[test]
    fn missing_github_cli_does_not_create_repeated_notifications() {
        let mut prs = PullRequests::default();
        prs.fail("Cannot run gh: No such file or directory".into());
        prs.fail("Cannot run gh: No such file or directory".into());

        assert!(prs.notifications.is_empty());
        assert!(prs.error.is_some());
    }

    fn pr_row(id: &str, number: u64, head_oid: &str, lifecycle: PrLifecycle) -> PrListRow {
        PrListRow {
            id: id.into(),
            number,
            title: "A title".into(),
            url: format!("https://github.com/owner/repo/pull/{number}"),
            head_oid: head_oid.into(),
            draft: false,
            lifecycle,
            merged_at: None,
            checks: switchbard_core::PrChecks::Unknown,
            review: switchbard_core::PrReview::Unknown,
            merge: switchbard_core::PrMerge::Unknown,
        }
    }

    fn snapshot_with_rows(repository: &str, rows: Vec<PrListRow>) -> PrSnapshot {
        PrSnapshot {
            rows,
            ..snapshot(repository, Ok(0))
        }
    }

    // (a) an outstanding expectation tightens the refresh cadence and clears
    // once a refreshed snapshot no longer lists the PR as open.
    #[test]
    fn expected_merge_tightens_cadence_and_clears_on_convergence() {
        let mut prs = PullRequests::default();
        let row = pr_row("pr-1", 42, "headoid", PrLifecycle::Open);
        prs.accept(Ok(snapshot_with_rows("owner/repo", vec![row.clone()])));
        prs.refilter();
        let root = Path::new("does-not-exist-for-a-unit-test");
        let base = Instant::now();
        prs.completed_at = Some(base);
        prs.expect_merge(row.id.clone(), row.head_oid.clone(), base);

        assert!(prs.syncing_after_merge());
        assert!(prs.expecting_merge(&row));
        assert!(prs.refresh_label().contains("syncing"));

        // Well inside the tightened 3s cadence: no refresh triggered yet, and
        // the countdown reflects the tightened window, not the routine 60s.
        assert!(!prs.tick(root, 60, base + Duration::from_secs(1)));
        assert!(!prs.loading());
        assert_eq!(prs.remaining_seconds, EXPECTED_MERGE_POLL_SECONDS - 1);

        // Still reported open: the expectation survives a refresh.
        prs.accept(Ok(snapshot_with_rows(
            "owner/repo",
            vec![pr_row("pr-1", 42, "headoid", PrLifecycle::Open)],
        )));
        prs.reconcile_expected_merge();
        assert!(prs.syncing_after_merge(), "still open: expectation stays");

        // GitHub now reports it merged: the expectation clears.
        prs.accept(Ok(snapshot_with_rows(
            "owner/repo",
            vec![pr_row("pr-1", 42, "headoid", PrLifecycle::Merged)],
        )));
        prs.reconcile_expected_merge();
        assert!(!prs.syncing_after_merge());
        assert!(!prs.refresh_label().contains("syncing"));
    }

    // (a, continued) an absent row also counts as converged.
    #[test]
    fn expected_merge_converges_when_the_row_disappears() {
        let mut prs = PullRequests::default();
        let row = pr_row("pr-1", 42, "headoid", PrLifecycle::Open);
        prs.accept(Ok(snapshot_with_rows("owner/repo", vec![row.clone()])));
        prs.expect_merge(row.id.clone(), row.head_oid.clone(), Instant::now());

        prs.accept(Ok(snapshot_with_rows("owner/repo", Vec::new())));
        prs.reconcile_expected_merge();

        assert!(!prs.syncing_after_merge());
    }

    // (b) window expiry clears the expectation and notifies, once, naming
    // the PR by number.
    #[test]
    fn expected_merge_expires_after_its_window_with_a_notification() {
        let mut prs = PullRequests::default();
        let row = pr_row("pr-1", 42, "headoid", PrLifecycle::Open);
        prs.accept(Ok(snapshot_with_rows("owner/repo", vec![row.clone()])));
        let since = Instant::now();
        prs.expect_merge(row.id.clone(), row.head_oid.clone(), since);

        prs.expire_expected_merge(since + Duration::from_secs(EXPECTED_MERGE_WINDOW_SECONDS - 1));
        assert!(
            prs.syncing_after_merge(),
            "window has not elapsed; expectation must remain"
        );

        prs.expire_expected_merge(since + Duration::from_secs(EXPECTED_MERGE_WINDOW_SECONDS));
        assert!(!prs.syncing_after_merge());
        assert!(prs
            .notifications
            .latest()
            .is_some_and(|message| message.contains("PR #42")
                && message.contains("still shows open after merge")));
    }
}
