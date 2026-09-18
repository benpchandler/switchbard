//! Word-processor-style bulk selection shared by Tasks and Pull Requests:
//! `space` toggles the cursor row's mark, `shift-down`/`shift-up` extend a
//! sweep from an anchor to the cursor. The sweep's own bookkeeping
//! (`Sweep::added`) is kept separate from `marked` itself: a row `space`
//! marked outside the sweep is never in `added`, so contracting the range
//! back over it leaves it alone — only what the sweep put there is the
//! sweep's to take away. Callers own cursor movement and the definition of
//! "what is in range"; this type only owns the marks and the sweep's anchor.
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
struct Sweep {
    anchor_id: String,
    added: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub marked: BTreeSet<String>,
    sweep: Option<Sweep>,
}

impl Selection {
    pub fn is_marked(&self, id: &str) -> bool {
        self.marked.contains(id)
    }

    /// Flip `id`'s mark; returns the mark state after the toggle.
    pub fn toggle(&mut self, id: &str) -> bool {
        if self.marked.remove(id) {
            false
        } else {
            self.marked.insert(id.to_string());
            true
        }
    }

    pub fn clear(&mut self) {
        self.marked.clear();
        self.reset_sweep();
    }

    /// Any plain cursor move, page switch, filter change, or outline change
    /// starts the next `shift-down`/`shift-up` fresh, from wherever the
    /// cursor lands.
    pub fn reset_sweep(&mut self) {
        self.sweep = None;
    }

    /// The live sweep's anchor id, fixed since its first `extend` call until
    /// the next `reset_sweep`.
    pub fn sweep_anchor(&self) -> Option<&str> {
        self.sweep.as_ref().map(|sweep| sweep.anchor_id.as_str())
    }

    /// Apply one sweep step. `anchor` is only consulted the first time (when
    /// no sweep is live yet, it becomes the sweep's fixed anchor); `in_range`
    /// is every id the sweep covers as of this call, inclusive of both ends.
    /// Returns the number of ids marked afterward.
    pub fn extend(&mut self, anchor: String, in_range: BTreeSet<String>) -> usize {
        let sweep = self.sweep.get_or_insert_with(|| Sweep {
            anchor_id: anchor,
            added: BTreeSet::new(),
        });
        // Rows the range no longer covers: only the ones this sweep put
        // there itself come back off.
        for id in &sweep.added {
            if !in_range.contains(id) {
                self.marked.remove(id);
            }
        }
        sweep.added.retain(|id| in_range.contains(id));
        // Rows newly covered: mark them, recording only the ones that were
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

    /// Drop marks for ids that no longer exist after a reload or a bulk
    /// action that removed some of them.
    pub fn retain(&mut self, mut exists: impl FnMut(&str) -> bool) {
        self.marked.retain(|id| exists(id));
    }
}
