//! Tab completion for the `/` filter editor.
//!
//! There is one key registry and one value registry, and this module reads
//! both rather than inventing its own: keys come from the same
//! [`ColumnRegistry`](crate::columns::ColumnRegistry) catalog `page_columns()`
//! already exposes (built-ins plus every field the repo declares), and values
//! come from [`App::column_values`], the same bounded list the `p` menu and
//! the column filter pickers read. Custom fields fall out of the column
//! catalog for free: a declared field is just another entry in it.
//!
//! Completion is computed fresh from `filter_text()` on every call rather
//! than cached, so there is nothing to keep in sync and Esc/backspace can
//! never leave a stale suggestion behind.

use crate::filter::FilterField;
use crate::tasks::Filter;

use super::App;

/// How many candidates the inline list shows before folding the rest into a
/// "+N more" tail.
const CANDIDATE_CAP: usize = 8;

/// The suggestions to show beneath the editor for its current word; `None`
/// when there is nothing worth offering (free text, an already-exact match,
/// or a key that resolves to nothing).
pub struct FilterCompletionHint {
    pub items: Vec<String>,
    pub more: usize,
}

/// What the word being typed resolves to.
enum WordContext {
    /// No `:` yet: a bare key prefix (or a plain search word, which simply
    /// never matches any key).
    Key { typed: String },
    /// After `key:`, optionally negated and optionally following earlier
    /// comma-joined values; `typed` is the segment currently being completed.
    Value {
        key: String,
        field: FilterField,
        negate: bool,
        prior: String,
        typed: String,
    },
    /// A `:` whose key half is not a field this page answers to: nothing to
    /// complete.
    Unrecognized,
}

fn word_context(word: &str, registry: &crate::columns::ColumnRegistry) -> WordContext {
    let Some((key, rest)) = word.split_once(':') else {
        return WordContext::Key {
            typed: word.to_string(),
        };
    };
    let Some(field) = FilterField::parse(&key.to_lowercase(), registry) else {
        return WordContext::Unrecognized;
    };
    let (negate, rest) = match rest.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, rest),
    };
    let (prior, typed) = match rest.rsplit_once(',') {
        Some((prior, typed)) => (format!("{prior},"), typed.to_string()),
        None => (String::new(), rest.to_string()),
    };
    WordContext::Value {
        key: key.to_string(),
        field,
        negate,
        prior,
        typed,
    }
}

/// The editor text split at its last word: `head` is everything before it
/// (including any trailing whitespace), `word` is what a completion replaces.
///
/// Finds the whitespace char's byte index via `char_indices` (not `rfind` on
/// a byte offset) and slices past its full UTF-8 width: a multi-byte
/// whitespace character (NBSP, U+00A0) would otherwise split mid-codepoint
/// and panic, and this runs on every render frame while the editor is open.
fn split_last_word(text: &str) -> (&str, &str) {
    match text.char_indices().rev().find(|(_, c)| c.is_whitespace()) {
        Some((index, c)) => {
            let end = index + c.len_utf8();
            (&text[..end], &text[end..])
        }
        None => ("", text),
    }
}

/// One resolved completion action for the current word.
enum Resolution {
    /// Exactly one candidate (or the typed text already exactly names one
    /// among several): replace the word with it in full.
    Unique(String),
    /// Still ambiguous, but every match shares a longer prefix than what is
    /// typed: extend to that shared prefix, same as shell tab completion.
    Extend(String),
    /// No candidate matches, or typing already sits at the shared prefix:
    /// nothing to do.
    None,
}

fn resolve(typed: &str, candidates: &[String]) -> Resolution {
    if candidates.is_empty() {
        return Resolution::None;
    }
    if candidates.len() == 1 {
        return Resolution::Unique(candidates[0].clone());
    }
    if let Some(exact) = candidates.iter().find(|c| c.eq_ignore_ascii_case(typed)) {
        return Resolution::Unique(exact.clone());
    }
    let common = longest_common_prefix(candidates);
    if common.len() > typed.len() {
        Resolution::Extend(common)
    } else {
        Resolution::None
    }
}

fn longest_common_prefix(values: &[String]) -> String {
    let Some(first) = values.first() else {
        return String::new();
    };
    let mut end = first.len();
    for value in &values[1..] {
        let shared = first
            .char_indices()
            .zip(value.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map(|((index, ch), _)| index + ch.len_utf8())
            .unwrap_or(0);
        end = end.min(shared);
        if end == 0 {
            break;
        }
    }
    first[..end].to_string()
}

impl App {
    /// Every filter keyword this page's columns accept, plus the free-word
    /// keys (`title`) that carry no category column of their own. `id` comes
    /// through the `Id` column's own spec, not the category-picker's
    /// `filter_field()` (which deliberately excludes it there, since a
    /// per-task id is not a group of values, but it is still a filter key).
    fn filter_key_catalog(&self) -> Vec<FilterField> {
        let mut fields = vec![FilterField::Title];
        for column in self.page_columns() {
            if let Some(field) = column.spec(&self.registry).field {
                fields.push(field);
            }
        }
        let mut keys: Vec<(String, FilterField)> = fields
            .into_iter()
            .map(|field| (field.keyword(&self.registry).to_string(), field))
            .collect();
        keys.sort_by(|a, b| a.0.cmp(&b.0));
        keys.dedup_by(|a, b| a.0 == b.0);
        keys.into_iter().map(|(_, field)| field).collect()
    }

    fn key_candidates(&self, typed: &str) -> Vec<String> {
        let typed = typed.to_lowercase();
        let mut keys: Vec<String> = self
            .filter_key_catalog()
            .into_iter()
            .map(|field| field.keyword(&self.registry).to_string())
            .filter(|key| key.starts_with(&typed))
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// Live values for `field`'s column, loosely normalized (lowercase, no
    /// spaces or dashes) - the spelling a filter word actually needs, since
    /// a value with a space cannot survive as one whitespace-delimited term.
    fn value_candidates(&self, field: FilterField, typed: &str) -> Vec<String> {
        let mut values: Vec<String> = self
            .column_values(field.column())
            .into_iter()
            .map(|(value, _)| Filter::loose_key(&value))
            .filter(|value| Filter::loose_starts_with(value, typed))
            .collect();
        values.sort();
        values.dedup();
        values
    }

    /// Tab: complete the editor's current word against its key or value
    /// candidates. A no-op when nothing matches or the word is not in a
    /// completable position (free text, an unrecognized key).
    pub(super) fn complete_filter_word(&mut self) {
        let text = self.filter_text().to_string();
        let (head, word) = split_last_word(&text);
        let (head, word) = (head.to_string(), word.to_string());
        match word_context(&word, &self.registry) {
            WordContext::Key { typed } => {
                let candidates = self.key_candidates(&typed);
                match resolve(&typed, &candidates) {
                    Resolution::Unique(key) => self.set_filter(format!("{head}{key}:")),
                    Resolution::Extend(extended) => self.set_filter(format!("{head}{extended}")),
                    Resolution::None => {}
                }
            }
            WordContext::Value {
                key,
                field,
                negate,
                prior,
                typed,
            } => {
                let candidates = self.value_candidates(field, &typed);
                let bang = if negate { "!" } else { "" };
                match resolve(&typed, &candidates) {
                    Resolution::Unique(value) => {
                        self.set_filter(format!("{head}{key}:{bang}{prior}{value}"));
                    }
                    Resolution::Extend(extended) => {
                        self.set_filter(format!("{head}{key}:{bang}{prior}{extended}"));
                    }
                    Resolution::None => {}
                }
            }
            WordContext::Unrecognized => {}
        }
    }

    /// The candidate list to show beneath the editor for its current word,
    /// live as the user types - not gated behind Tab, so ambiguity is visible
    /// before it is resolved.
    pub fn filter_completion_hint(&self) -> Option<FilterCompletionHint> {
        let text = self.filter_text().to_string();
        let (_, word) = split_last_word(&text);
        let (typed, candidates) = match word_context(word, &self.registry) {
            WordContext::Key { typed } => {
                let candidates = self.key_candidates(&typed);
                (typed, candidates)
            }
            WordContext::Value { field, typed, .. } => {
                let candidates = self.value_candidates(field, &typed);
                (typed, candidates)
            }
            WordContext::Unrecognized => return None,
        };
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() == 1 && candidates[0].eq_ignore_ascii_case(&typed) {
            return None;
        }
        let more = candidates.len().saturating_sub(CANDIDATE_CAP);
        let items = candidates.into_iter().take(CANDIDATE_CAP).collect();
        Some(FilterCompletionHint { items, more })
    }
}
