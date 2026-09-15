//! Checklist coverage, never an automatic lifecycle transition.
use super::{BacklogRepo, BacklogTask, BacklogTaskSource};
use std::collections::{HashMap, HashSet};
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChecklistProgress {
    pub checked: usize,
    pub total: usize,
}
impl ChecklistProgress {
    pub fn percentage(self) -> Option<f64> {
        (self.total > 0).then(|| 100.0 * self.checked as f64 / self.total as f64)
    }
    pub fn needs_review(self, task: &BacklogTask) -> bool {
        self.total > 0
            && self.checked == self.total
            && task.source == BacklogTaskSource::Active
            && !task.is_done()
            && included(task)
    }
}
fn included(task: &BacklogTask) -> bool {
    task.source != BacklogTaskSource::Archived
        && !["canceled", "cancelled", "archived"]
            .iter()
            .any(|s| task.status.eq_ignore_ascii_case(s))
}
pub fn checklist_progress(repo: &BacklogRepo) -> HashMap<String, ChecklistProgress> {
    let mut by_id = HashMap::new();
    for task in &repo.tasks {
        by_id.entry(task.id.to_ascii_lowercase()).or_insert(task);
    }
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for task in by_id.values() {
        let parent = task
            .parent
            .as_deref()
            .or_else(|| task.id.rsplit_once('.').map(|(parent, _)| parent));
        if let Some(parent) = parent {
            children
                .entry(parent.to_ascii_lowercase())
                .or_default()
                .push(task.id.to_ascii_lowercase());
        }
    }
    by_id
        .keys()
        .map(|id| {
            let mut progress = ChecklistProgress::default();
            let mut seen = HashSet::new();
            let mut pending = vec![id.as_str()];
            while let Some(next) = pending.pop() {
                if !seen.insert(next) {
                    continue;
                }
                let Some(task) = by_id.get(next).filter(|task| included(task)) else {
                    continue;
                };
                progress.total += task.acceptance_criteria.len();
                progress.checked += task.acceptance_done_count();
                if let Some(descendants) = children.get(next) {
                    pending.extend(descendants.iter().map(String::as_str));
                }
            }
            (by_id[id].id.clone(), progress)
        })
        .collect()
}
