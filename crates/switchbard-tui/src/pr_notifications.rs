//! Bounded session alerts derived only from successive repository observations.
use std::collections::VecDeque;
use switchbard_core::{PrLifecycle, PrListRow, PrSnapshot};

const MAX_NOTIFICATIONS: usize = 32;
const MAX_MESSAGE_CHARS: usize = 240;

#[derive(Default)]
pub struct PrNotifications {
    messages: VecDeque<String>,
}

impl PrNotifications {
    pub fn latest(&self) -> Option<&str> {
        self.messages.back().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Dismiss the current alert, revealing the next retained observation.
    pub fn dismiss(&mut self) {
        self.messages.pop_back();
    }

    pub fn push(&mut self, message: String) {
        if self.messages.len() == MAX_NOTIFICATIONS {
            self.messages.pop_front();
        }
        self.messages
            .push_back(message.chars().take(MAX_MESSAGE_CHARS).collect());
    }

    pub fn observe(&mut self, previous: Option<&PrSnapshot>, next: &PrSnapshot) {
        let Some(previous) = previous.filter(|old| old.repository_url == next.repository_url)
        else {
            return;
        };
        for row in next.rows.iter().take(switchbard_core::MAX_PULL_REQUESTS) {
            // Newly loaded history and rows absent from a bounded window have no baseline.
            if let Some(old) = previous.rows.iter().find(|old| old.id == row.id) {
                self.observe_row(
                    old,
                    row,
                    previous.enrichment_warning.is_none() && next.enrichment_warning.is_none(),
                );
            }
        }
    }

    fn observe_row(&mut self, old: &PrListRow, row: &PrListRow, delivery_available: bool) {
        let mut changes = Vec::with_capacity(5);
        if old.lifecycle != row.lifecycle {
            changes.push(format!("state: {}", row.lifecycle.label()));
        }
        if row.lifecycle == PrLifecycle::Open && delivery_available {
            if old.checks != row.checks {
                changes.push(format!("checks: {}", row.checks.label()));
            }
            if old.review != row.review {
                changes.push(format!("review: {}", row.review.label()));
            }
            if old.merge != row.merge {
                changes.push(format!("merge: {}", row.merge.label()));
            }
        }
        if old.draft != row.draft {
            changes.push(
                if row.draft {
                    "Draft"
                } else {
                    "Ready for review"
                }
                .to_string(),
            );
        }
        if !changes.is_empty() {
            self.push(format!("#{}: {}", row.number, changes.join(" · ")));
        }
    }
}
