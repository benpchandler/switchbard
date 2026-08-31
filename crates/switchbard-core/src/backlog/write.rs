//! Native, surgical mutations of Backlog task files — the format fork's write
//! layer (trajectory: *Backlog format fork*, owner-approved 2026-08-28).
//!
//! # Surgical, and what that buys
//!
//! Every edit here rewrites **only the bytes of the field or section it
//! targets**. Frontmatter keys this crate doesn't model (`ordinal`, anything
//! a future format adds), key order, quoting style, and author formatting all
//! survive byte-for-byte — pinned by `tests/write_layer_real_files.rs`, which
//! runs the gate over every real task file in this repository. Body sections
//! this crate doesn't model (`## Resolution` and friends, common in real
//! repos) survive the same way: a section edit rewrites only its own span,
//! so an unknown heading's block is opaque to every operation here
//! (TASK-45). The precedent is `super::status_config`, which already edits
//! `config.yml` at line level; this module extends that philosophy to the
//! task files themselves.
//!
//! # Contract
//!
//! - **A no-op writes nothing.** If an edit leaves the file's bytes
//!   unchanged, the file is not touched and `updated_date` is not bumped.
//! - **Any real change bumps `updated_date`** (parity with the `backlog` CLI
//!   this layer replaces).
//! - **Writes are atomic**: write-tmp-then-rename, the same pattern as
//!   `crate::config::save_to`.
//! - **Body-structure edits fail closed** on
//!   [`super::parse::task_file_round_trips`]` == false`, the same contract
//!   `crate::refine` already holds: "I could not verify the structure" must
//!   mean "do not rewrite it", never "assume it's fine".
//!
//! Not yet wired into `super::mutations` — that swap is its own step in the
//! fork sequence, so this layer lands with its gate and no callers to break.

use super::parse::{
    heading_title, parse_checklist_index, scan_fences, task_file_round_trips,
    KNOWN_SECTION_HEADINGS,
};
use super::types::NewBacklogTask;
use anyhow::{bail, Context, Result};
use serde_yaml::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Whether an edit actually changed the file. `Unchanged` means the bytes on
/// disk are exactly what they were — nothing was written and `updated_date`
/// was not bumped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "an Unchanged outcome often means the caller's edit was stale"]
pub enum WriteOutcome {
    Changed,
    Unchanged,
}

impl WriteOutcome {
    pub fn changed(self) -> bool {
        self == Self::Changed
    }
}

/// The frontmatter list fields the write layer edits. `assignee` is singular
/// on disk (the CLI's own key); the enum name mirrors the field, not the
/// domain plural.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskListField {
    Labels,
    Assignee,
    Dependencies,
    References,
}

impl TaskListField {
    fn key(self) -> &'static str {
        match self {
            Self::Labels => "labels",
            Self::Assignee => "assignee",
            Self::Dependencies => "dependencies",
            Self::References => "references",
        }
    }
}

/// The marker-comment-wrapped prose sections a task body carries. Checklist
/// sections (Acceptance Criteria, Definition of Done) are deliberately not
/// here — they are structured lists with their own operations, not
/// replace-whole-section prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskSection {
    Description,
    ImplementationPlan,
    ImplementationNotes,
    FinalSummary,
}

impl TaskSection {
    fn heading(self) -> &'static str {
        match self {
            Self::Description => "Description",
            Self::ImplementationPlan => "Implementation Plan",
            Self::ImplementationNotes => "Implementation Notes",
            Self::FinalSummary => "Final Summary",
        }
    }

    /// The CLI's own marker name for this section — `SECTION:{marker}:BEGIN`
    /// / `:END`. Observed across every tracked repo's task files, not
    /// guessed.
    fn marker(self) -> &'static str {
        match self {
            Self::Description => "DESCRIPTION",
            Self::ImplementationPlan => "PLAN",
            Self::ImplementationNotes => "NOTES",
            Self::FinalSummary => "FINAL_SUMMARY",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskChecklist {
    AcceptanceCriteria,
    DefinitionOfDone,
}

impl TaskChecklist {
    fn heading(self) -> &'static str {
        match self {
            Self::AcceptanceCriteria => "Acceptance Criteria",
            Self::DefinitionOfDone => "Definition of Done",
        }
    }

    fn marker(self) -> &'static str {
        match self {
            Self::AcceptanceCriteria => "AC",
            Self::DefinitionOfDone => "DOD",
        }
    }
}

// ---- public operations: frontmatter ----

pub fn set_task_status(path: &Path, status: &str) -> Result<WriteOutcome> {
    let status = validated_single_line("status", status)?.to_string();
    apply_edit(path, move |fm, _| {
        set_scalar(fm, "status", &yaml_scalar(&status), None);
        Ok(())
    })
}

pub fn set_task_priority(path: &Path, priority: &str) -> Result<WriteOutcome> {
    let priority = validated_single_line("priority", priority)?.to_string();
    apply_edit(path, move |fm, _| {
        set_scalar(fm, "priority", &yaml_scalar(&priority), None);
        Ok(())
    })
}

pub fn set_task_title(path: &Path, title: &str) -> Result<WriteOutcome> {
    let title = validated_single_line("title", title)?.to_string();
    apply_edit(path, move |fm, _| {
        set_scalar(fm, "title", &yaml_scalar(&title), None);
        Ok(())
    })
}

/// `Some(name)` assigns the milestone (inserting the key after `priority` if
/// the task never had one); `None` removes the key entirely.
pub fn set_task_milestone(path: &Path, milestone: Option<&str>) -> Result<WriteOutcome> {
    let Some(name) = milestone else {
        return apply_edit(path, |fm, _| {
            remove_key(fm, "milestone");
            Ok(())
        });
    };
    let name = validated_single_line("milestone", name)?.to_string();
    apply_edit(path, move |fm, _| {
        set_scalar(fm, "milestone", &yaml_scalar(&name), Some("priority"));
        Ok(())
    })
}

/// Replace a list field wholesale. Values are trimmed and empties dropped
/// (the same normalization `super::mutations` applied before comma-joining
/// for the CLI). Note this re-renders the whole list block, so a
/// *semantically* identical list in a different on-disk style (e.g. an
/// inline `[a, b]`) is rewritten to block style — the byte-no-op guarantee
/// holds only when the rendered bytes match.
pub fn set_task_list_field(
    path: &Path,
    field: TaskListField,
    values: &[String],
) -> Result<WriteOutcome> {
    let cleaned = cleaned_list_values(field.key(), values)?;
    apply_edit(path, move |fm, _| {
        set_list(fm, field.key(), &cleaned);
        Ok(())
    })
}

/// Add or remove one label without disturbing the rest — the freshness
/// semantics of the CLI's `--add-label`/`--remove-label`: the current list is
/// read from the file at write time, not from the caller's possibly stale
/// snapshot. A semantic no-op (adding a label already present, removing one
/// already absent) leaves the file bytes completely untouched, whatever
/// style the list is currently written in.
pub fn set_task_label(path: &Path, label: &str, enabled: bool) -> Result<WriteOutcome> {
    let label = validated_single_line("label", label)?.to_string();
    apply_edit(path, move |fm, _| {
        let labels = current_list(fm, "labels");
        let has = labels.iter().any(|l| l == &label);
        if has == enabled {
            return Ok(());
        }
        let mut next = labels;
        if enabled {
            next.push(label.clone());
        } else {
            next.retain(|l| l != &label);
        }
        set_list(fm, "labels", &next);
        Ok(())
    })
}

/// Atomically replace one label with another in a single write — the
/// dispatch pipeline's claim primitive.
///
/// **Strict, deliberately stronger than the CLI swap this replaces**: it
/// *fails* when the task doesn't carry `from` at write time, instead of
/// adding `to` anyway. A dispatch claim (`dispatch` → `dispatching`) is a
/// race for a token; a swap that succeeds without holding the token would
/// let two claimants both "win". The read happens inside the same
/// read-modify-rename cycle as the write, so the lost-race window is the
/// microseconds between them, not a caller's stale snapshot.
pub fn swap_task_label(path: &Path, from: &str, to: &str) -> Result<WriteOutcome> {
    let from = validated_single_line("label", from)?.to_string();
    let to = validated_single_line("label", to)?.to_string();
    apply_edit(path, move |fm, _| {
        let labels = current_list(fm, "labels");
        if !labels.iter().any(|l| l == &from) {
            bail!("task does not carry label `{from}`");
        }
        let mut next: Vec<String> = labels.into_iter().filter(|l| l != &from).collect();
        if !next.iter().any(|l| l == &to) {
            next.push(to.clone());
        }
        set_list(fm, "labels", &next);
        Ok(())
    })
}

// ---- public operations: body ----

/// Replace one prose section's content wholesale, regenerating its
/// `SECTION:*:BEGIN/END` markers. Creates the section (at its canonical
/// position among the known headings) if the task doesn't have it yet.
pub fn replace_task_section(
    path: &Path,
    section: TaskSection,
    content: &str,
) -> Result<WriteOutcome> {
    ensure_body_rewrite_safe(path)?;
    apply_edit(path, move |_, rest| {
        let block = marked_section_block(section.heading(), section.marker(), content);
        upsert_section(rest, section.heading(), block);
        Ok(())
    })
}

/// Append a note to Implementation Notes, creating the section if missing.
/// Existing notes are never rewritten — the note is inserted just before the
/// section's END marker, separated by a blank line when the section already
/// has content.
pub fn append_task_notes(path: &Path, note: &str) -> Result<WriteOutcome> {
    let note = note.trim().to_string();
    if note.is_empty() {
        bail!("note is empty");
    }
    ensure_body_rewrite_safe(path)?;
    apply_edit(path, move |_, rest| {
        append_to_marked_section(rest, TaskSection::ImplementationNotes, &note);
        Ok(())
    })
}

/// Append acceptance criteria, never disturbing existing ones — the
/// `--ac`-not-`--acceptance-criteria` contract `crate::refine` depends on.
/// Numbering continues from the highest existing `#N`.
pub fn append_task_acceptance_criteria(path: &Path, items: &[String]) -> Result<WriteOutcome> {
    let cleaned: Vec<String> = items
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if cleaned.is_empty() {
        return Ok(WriteOutcome::Unchanged);
    }
    for item in &cleaned {
        if item.contains('\n') {
            bail!("acceptance criteria must be single-line");
        }
    }
    ensure_body_rewrite_safe(path)?;
    apply_edit(path, move |_, rest| {
        append_criteria(rest, &cleaned);
        Ok(())
    })
}

/// Check or uncheck one checklist item by its `#N` index (1-based, matching
/// what the task file itself displays). Flipping a mark is a single-line
/// byte-surgery, so this deliberately does *not* require the whole body to
/// round-trip: the fence-aware section scan is enough to guarantee the flip
/// lands on the same line the parser counts, and no other byte moves.
/// Setting an item to the state it's already in is a no-op.
pub fn set_task_checklist_item(
    path: &Path,
    list: TaskChecklist,
    index: usize,
    checked: bool,
) -> Result<WriteOutcome> {
    if index == 0 {
        bail!("checklist indices are 1-based");
    }
    apply_edit(path, move |_, rest| {
        let mut lines = split_lines(rest);
        let inside = fence_flags(rest);
        let span = section_span(&lines, &inside, list.heading())
            .with_context(|| format!("task has no {} section", list.heading()))?;
        if set_checked_in_span(&mut lines, span, index, checked)? {
            *rest = lines.join("\n");
        }
        Ok(())
    })
}

// ---- public operations: create ----

/// Write a brand-new task file in the CLI's on-disk shape. The caller owns
/// ID allocation (`id` is the bare id — `"42"`, or a decimal subtask id like
/// `"42.1"`, without the `{PREFIX}-` prefix) and prefix resolution (`prefix`
/// is the project's configured `task_prefix`, e.g. `"LED"`, already
/// uppercased — see `super::parse::configured_task_prefix`); this function
/// owns collision *safety*: the file is opened with `create_new`, so racing
/// two creates onto the same id fails cleanly rather than overwriting.
///
/// The frontmatter id is written uppercased (`id: LED-42`) and the filename
/// lowercased (`led-42 - ....md`) — the exact casing split observed in
/// budget's own `LED`-prefixed tasks, which the `backlog` CLI produces and
/// reads regardless of how a human typed `task_prefix` in `config.yml`.
///
/// Deliberately omits `ordinal` (the CLI web board's manual ordering hint) —
/// nothing in this app reads it, and a fresh task has no meaningful position
/// to claim. Recorded as a decision on the write-layer task.
pub fn write_new_task_file(
    tasks_dir: &Path,
    prefix: &str,
    id: &str,
    task: &NewBacklogTask,
) -> Result<PathBuf> {
    validate_task_id(id)?;
    let prefix = prefix.trim();
    if prefix.is_empty() {
        bail!("task prefix is empty");
    }
    let title = validated_single_line("title", &task.title)?;
    let text = new_task_text(prefix, id, title, task, &local_stamp())?;
    let path = tasks_dir.join(format!(
        "{}-{id} - {}.md",
        prefix.to_ascii_lowercase(),
        filename_slug(title)
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("creating {} (id already taken?)", path.display()))?;
    file.write_all(text.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// A bare task id is digit groups joined by single dots — `42`, `42.1`,
/// `42.1.3`. Anything else would poison both the filename convention and the
/// id-matching every reader does.
fn validate_task_id(id: &str) -> Result<()> {
    let well_formed = !id.is_empty()
        && id
            .split('.')
            .all(|group| !group.is_empty() && group.chars().all(|c| c.is_ascii_digit()));
    if !well_formed {
        bail!("malformed task id `{id}` (expected digits, optionally dot-separated)");
    }
    Ok(())
}

// ---- the edit engine ----

/// A task file split at its frontmatter fences. `join_raw(split_raw(text))`
/// is byte-identical by construction: `fm` is exactly the lines between the
/// two `---` fences, `rest` is exactly everything after the closing fence
/// (leading newline included).
struct RawTask {
    fm: Vec<String>,
    rest: String,
}

fn split_raw(text: &str) -> Result<RawTask> {
    if text.contains('\r') {
        bail!("task file has CR line endings; refusing to edit");
    }
    let Some(after) = text.strip_prefix("---\n") else {
        bail!("task file does not begin with a `---` frontmatter fence");
    };
    let Some(end) = after.find("\n---") else {
        bail!("task file's frontmatter fence is unterminated");
    };
    let fm = after[..end].split('\n').map(str::to_string).collect();
    let rest = after[end + "\n---".len()..].to_string();
    Ok(RawTask { fm, rest })
}

fn join_raw(fm: &[String], rest: &str) -> String {
    format!("---\n{}\n---{}", fm.join("\n"), rest)
}

/// Read → transform → compare → (maybe) bump `updated_date` and atomically
/// write. The byte comparison happens *before* the date bump, which is what
/// makes "no-op writes nothing" hold: an edit that reproduces the file
/// exactly returns `Unchanged` without touching disk.
fn apply_edit(
    path: &Path,
    edit: impl FnOnce(&mut Vec<String>, &mut String) -> Result<()>,
) -> Result<WriteOutcome> {
    let original =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let RawTask { mut fm, mut rest } = split_raw(&original)?;
    debug_assert_eq!(
        join_raw(&fm, &rest),
        original,
        "split/join must be byte-lossless"
    );
    edit(&mut fm, &mut rest)?;
    if join_raw(&fm, &rest) == original {
        return Ok(WriteOutcome::Unchanged);
    }
    bump_updated_date(&mut fm, &local_stamp());
    atomic_write(path, &join_raw(&fm, &rest))?;
    Ok(WriteOutcome::Changed)
}

/// Same write-tmp-then-rename shape as `crate::config::save_to`, with two
/// courtesies rename alone wouldn't give: a read-only task file is
/// **refused** (rename needs only directory permission, so it would happily
/// replace a file its owner locked — the CLI honored the bit, and so do
/// we), and the original file's permissions survive onto the replacement.
fn atomic_write(path: &Path, text: &str) -> Result<()> {
    let permissions = fs::metadata(path)
        .with_context(|| format!("inspecting {}", path.display()))?
        .permissions();
    if permissions.readonly() {
        bail!("{} is read-only", path.display());
    }
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    fs::set_permissions(&tmp, permissions)
        .with_context(|| format!("preserving permissions on {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}

fn ensure_body_rewrite_safe(path: &Path) -> Result<()> {
    if task_file_round_trips(path) {
        return Ok(());
    }
    bail!(
        "refusing to rewrite {}: the task body does not round-trip losslessly \
         (unbalanced code fences, a duplicated section heading, or content \
         before the first heading) — fix the file by hand first",
        path.display()
    )
}

// ---- frontmatter primitives ----

/// The key a frontmatter line declares, if it declares one. Indented lines
/// (block-list items, continuations) belong to the key above them.
fn line_key(line: &str) -> Option<&str> {
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }
    let (key, _) = line.split_once(':')?;
    let valid = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some(key)
}

/// `[start, end)` of the lines belonging to `key`: its own line plus any
/// indented continuation lines (a block list's items).
fn key_span(fm: &[String], key: &str) -> Option<(usize, usize)> {
    let start = fm.iter().position(|l| line_key(l) == Some(key))?;
    let mut end = start + 1;
    while end < fm.len() && line_key(&fm[end]).is_none() {
        end += 1;
    }
    debug_assert!(start < end && end <= fm.len(), "span is a valid line range");
    Some((start, end))
}

/// Replace `key`'s lines with `key: rendered`, or insert the key if absent —
/// after `insert_after`'s span when given (and present), else at the end.
fn set_scalar(fm: &mut Vec<String>, key: &str, rendered: &str, insert_after: Option<&str>) {
    let line = format!("{key}: {rendered}");
    if let Some((start, end)) = key_span(fm, key) {
        fm.splice(start..end, [line]);
        return;
    }
    let at = insert_after
        .and_then(|k| key_span(fm, k))
        .map_or(fm.len(), |(_, end)| end);
    fm.insert(at, line);
}

fn set_list(fm: &mut Vec<String>, key: &str, values: &[String]) {
    let lines = render_list(key, values);
    if let Some((start, end)) = key_span(fm, key) {
        fm.splice(start..end, lines);
    } else {
        fm.extend(lines);
    }
}

fn remove_key(fm: &mut Vec<String>, key: &str) {
    if let Some((start, end)) = key_span(fm, key) {
        fm.drain(start..end);
    }
}

/// Empty list → the CLI's inline `key: []`; otherwise its block style with
/// two-space-indented `- ` items.
fn render_list(key: &str, values: &[String]) -> Vec<String> {
    if values.is_empty() {
        return vec![format!("{key}: []")];
    }
    let mut lines = Vec::with_capacity(values.len() + 1);
    lines.push(format!("{key}:"));
    lines.extend(values.iter().map(|v| format!("  - {}", yaml_scalar(v))));
    lines
}

/// Read a list field's current items from the frontmatter, via the same
/// serde-yaml reader `super::parse` trusts — not a hand parse that could
/// drift from it.
fn current_list(fm: &[String], key: &str) -> Vec<String> {
    let mapping = serde_yaml::from_str::<Value>(&fm.join("\n"))
        .ok()
        .and_then(|value| value.as_mapping().cloned())
        .unwrap_or_default();
    super::parse::yaml_string_list(&mapping, key)
}

fn bump_updated_date(fm: &mut Vec<String>, stamp: &str) {
    set_scalar(
        fm,
        "updated_date",
        &format!("'{stamp}'"),
        Some("created_date"),
    );
}

/// `"YYYY-MM-DD HH:MM"` in local time — the CLI's own timestamp shape (see
/// `super::parse::parse_backlog_day`, which parses exactly this back).
fn local_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

/// Render a frontmatter scalar the way the CLI does: plain where plain is
/// unambiguous, single-quoted (with `''` escaping) where YAML could misread
/// it — colons, comment starts, bool/null lookalikes, and anything not
/// starting with a plain ASCII letter (dates, numbers, punctuation-led
/// titles).
fn yaml_scalar(value: &str) -> String {
    debug_assert!(
        !value.contains('\n'),
        "frontmatter scalars are single-line; public entry points validate"
    );
    if needs_quoting(value) {
        format!("'{}'", value.replace('\'', "''"))
    } else {
        value.to_string()
    }
}

fn needs_quoting(value: &str) -> bool {
    let Some(first) = value.chars().next() else {
        return true;
    };
    if !first.is_ascii_alphabetic() || value.ends_with(' ') || value.ends_with(':') {
        return true;
    }
    if value.contains(": ") || value.contains(" #") {
        return true;
    }
    matches!(
        value.to_ascii_lowercase().as_str(),
        "true" | "false" | "null" | "yes" | "no" | "on" | "off"
    )
}

// ---- body primitives ----

/// `split('\n')`, which unlike `str::lines` keeps a trailing empty element
/// for the file's final newline — so `join("\n")` is byte-lossless.
fn split_lines(rest: &str) -> Vec<String> {
    rest.split('\n').map(str::to_string).collect()
}

/// Fence flags for `rest`, index-aligned with [`split_lines`]. (`scan_fences`
/// iterates `str::lines`, which yields the same lines minus a possible final
/// empty — never a fence — so lookups use `.get(i)` with a `false` default.)
fn fence_flags(rest: &str) -> Vec<bool> {
    scan_fences(rest).inside
}

fn top_heading<'l>(lines: &'l [String], inside: &[bool], i: usize) -> Option<&'l str> {
    if inside.get(i).copied().unwrap_or(false) {
        return None;
    }
    heading_title(&lines[i])
}

/// `[start, end)` of one section: its heading line through the last content
/// line before the next unfenced heading (or EOF), *excluding* trailing blank
/// lines — those separate sections and belong to no one, so edits never
/// disturb them.
fn section_span(lines: &[String], inside: &[bool], heading: &str) -> Option<(usize, usize)> {
    let start = (0..lines.len()).find(|&i| {
        top_heading(lines, inside, i).is_some_and(|t| t.eq_ignore_ascii_case(heading))
    })?;
    let mut end = start + 1;
    while end < lines.len() && top_heading(lines, inside, end).is_none() {
        end += 1;
    }
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    debug_assert!(start < end && end <= lines.len(), "span is a valid range");
    Some((start, end))
}

/// The CLI's prose-section shape: heading, blank, BEGIN marker, content, END
/// marker.
fn marked_section_block(heading: &str, marker: &str, content: &str) -> Vec<String> {
    let mut block = vec![
        format!("## {heading}"),
        String::new(),
        format!("<!-- SECTION:{marker}:BEGIN -->"),
    ];
    block.extend(content.trim().lines().map(str::to_string));
    block.push(format!("<!-- SECTION:{marker}:END -->"));
    block
}

/// The CLI's checklist-section shape: heading, then markers directly (no
/// blank line), items numbered from `first_index`.
fn checklist_block(
    heading: &str,
    marker: &str,
    items: &[String],
    first_index: usize,
) -> Vec<String> {
    let mut block = vec![format!("## {heading}"), format!("<!-- {marker}:BEGIN -->")];
    block.extend(
        items
            .iter()
            .enumerate()
            .map(|(k, text)| format!("- [ ] #{} {}", first_index + k, text)),
    );
    block.push(format!("<!-- {marker}:END -->"));
    block
}

fn canonical_rank(heading: &str) -> usize {
    KNOWN_SECTION_HEADINGS
        .iter()
        .position(|h| h.eq_ignore_ascii_case(heading))
        .unwrap_or(usize::MAX)
}

/// Where a not-yet-present section belongs: before the first existing
/// section that follows it in canonical order, else at the body's logical
/// end (past which only blank lines / the trailing newline sit).
fn insert_pos(lines: &[String], inside: &[bool], heading: &str) -> usize {
    let rank = canonical_rank(heading);
    for i in 0..lines.len() {
        if let Some(title) = top_heading(lines, inside, i) {
            if canonical_rank(title) > rank {
                return i;
            }
        }
    }
    let mut end = lines.len();
    while end > 0 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    end
}

/// Insert `block` at `pos`, adding the single blank line that separates it
/// from a non-blank neighbor on either side.
fn insert_block_at(lines: &mut Vec<String>, pos: usize, block: Vec<String>) {
    debug_assert!(pos <= lines.len(), "insert position is in range");
    let mut chunk = Vec::with_capacity(block.len() + 2);
    if pos > 0 && !lines[pos - 1].trim().is_empty() {
        chunk.push(String::new());
    }
    chunk.extend(block);
    if pos < lines.len() && !lines[pos].trim().is_empty() {
        chunk.push(String::new());
    }
    lines.splice(pos..pos, chunk);
}

/// Replace `heading`'s section with `block`, or insert it at its canonical
/// position if the task doesn't have one.
fn upsert_section(rest: &mut String, heading: &str, block: Vec<String>) {
    let mut lines = split_lines(rest);
    let inside = fence_flags(rest);
    if let Some((start, end)) = section_span(&lines, &inside, heading) {
        lines.splice(start..end, block);
    } else {
        let pos = insert_pos(&lines, &inside, heading);
        insert_block_at(&mut lines, pos, block);
    }
    *rest = lines.join("\n");
}

/// Append `content` inside an existing marked section (just before its END
/// marker), or create the whole section if the task doesn't have it.
fn append_to_marked_section(rest: &mut String, section: TaskSection, content: &str) {
    let mut lines = split_lines(rest);
    let inside = fence_flags(rest);
    let Some((start, end)) = section_span(&lines, &inside, section.heading()) else {
        let block = marked_section_block(section.heading(), section.marker(), content);
        upsert_section(rest, section.heading(), block);
        return;
    };
    let end_marker = format!("<!-- SECTION:{}:END -->", section.marker());
    let at = (start..end)
        .find(|&i| lines[i].trim() == end_marker)
        .unwrap_or(end);
    let begin_marker = format!("<!-- SECTION:{}:BEGIN -->", section.marker());
    let mut chunk: Vec<String> = Vec::new();
    let prev = lines[at - 1].trim().to_string();
    if !prev.is_empty() && prev != begin_marker && prev != format!("## {}", section.heading()) {
        chunk.push(String::new());
    }
    chunk.extend(content.lines().map(str::to_string));
    lines.splice(at..at, chunk);
    *rest = lines.join("\n");
}

// ---- checklist primitives ----

struct ChecklistLine {
    index: usize,
    checked: bool,
}

/// Mirror of `super::parse::parse_checklist_section`'s per-line logic:
/// same `- [` shape, same `#N`-or-ordinal index resolution, same skipping of
/// empty-text items — so the line this module flips is exactly the line the
/// parser counts.
fn parse_checklist_line(line: &str, fallback: usize) -> Option<ChecklistLine> {
    let rest = line.trim().strip_prefix("- [")?;
    let (mark, rest) = rest.split_once(']')?;
    let (index, text) = parse_checklist_index(rest.trim(), fallback);
    if text.is_empty() {
        return None;
    }
    Some(ChecklistLine {
        index,
        checked: mark.trim().eq_ignore_ascii_case("x"),
    })
}

/// Flip the item with `index` inside `span`. Returns whether a line changed
/// (`false`: it was already in the requested state). Errors when no item
/// carries that index.
fn set_checked_in_span(
    lines: &mut [String],
    span: (usize, usize),
    index: usize,
    checked: bool,
) -> Result<bool> {
    let mut count = 0usize;
    for line in lines.iter_mut().take(span.1).skip(span.0) {
        let Some(item) = parse_checklist_line(line, count + 1) else {
            continue;
        };
        count += 1;
        if item.index != index {
            continue;
        }
        if item.checked == checked {
            return Ok(false);
        }
        let flipped = flip_checklist_mark(line, checked)?;
        *line = flipped;
        return Ok(true);
    }
    bail!("no checklist item #{index} in this section")
}

/// Single-line byte surgery: only the one character between `[` and `]`
/// changes.
fn flip_checklist_mark(line: &str, checked: bool) -> Result<String> {
    let open = line
        .find("- [")
        .map(|p| p + "- [".len())
        .context("checklist line lost its `- [`")?;
    let close = line[open..]
        .find(']')
        .map(|off| open + off)
        .context("checklist line lost its `]`")?;
    let mark = if checked { "x" } else { " " };
    Ok(format!("{}{}{}", &line[..open], mark, &line[close..]))
}

fn append_criteria(rest: &mut String, items: &[String]) {
    let mut lines = split_lines(rest);
    let inside = fence_flags(rest);
    let list = TaskChecklist::AcceptanceCriteria;
    let Some(span) = section_span(&lines, &inside, list.heading()) else {
        let block = checklist_block(list.heading(), list.marker(), items, 1);
        let pos = insert_pos(&lines, &inside, list.heading());
        insert_block_at(&mut lines, pos, block);
        *rest = lines.join("\n");
        return;
    };
    let first_index = 1 + max_checklist_index(&lines, span);
    let rendered: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(k, text)| format!("- [ ] #{} {}", first_index + k, text))
        .collect();
    let end_marker = format!("<!-- {}:END -->", list.marker());
    let at = (span.0..span.1)
        .find(|&i| lines[i].trim() == end_marker)
        .unwrap_or(span.1);
    lines.splice(at..at, rendered);
    *rest = lines.join("\n");
}

fn max_checklist_index(lines: &[String], span: (usize, usize)) -> usize {
    let mut count = 0usize;
    let mut max = 0usize;
    for line in lines.iter().take(span.1).skip(span.0) {
        if let Some(item) = parse_checklist_line(line, count + 1) {
            count += 1;
            max = max.max(item.index);
        }
    }
    max
}

// ---- create primitives ----

fn new_task_text(
    prefix: &str,
    id: &str,
    title: &str,
    task: &NewBacklogTask,
    stamp: &str,
) -> Result<String> {
    let status = default_if_blank(&task.status, "To Do");
    let priority = default_if_blank(&task.priority, "medium");
    let mut fm = vec![
        format!("id: {prefix}-{id}"),
        format!("title: {}", yaml_scalar(title)),
        format!(
            "status: {}",
            yaml_scalar(validated_single_line("status", status)?)
        ),
    ];
    fm.extend(render_list(
        "assignee",
        &cleaned_list_values("assignee", &task.assignees)?,
    ));
    fm.push(format!("created_date: '{stamp}'"));
    fm.extend(render_list(
        "labels",
        &cleaned_list_values("labels", &task.labels)?,
    ));
    fm.extend(render_list(
        "dependencies",
        &cleaned_list_values("dependencies", &task.dependencies)?,
    ));
    fm.push(format!(
        "priority: {}",
        yaml_scalar(validated_single_line("priority", priority)?)
    ));
    if let Some(milestone) = &task.milestone {
        fm.push(format!(
            "milestone: {}",
            yaml_scalar(validated_single_line("milestone", milestone)?)
        ));
    }
    if let Some(parent) = &task.parent {
        fm.push(format!(
            "parent_task_id: {}",
            yaml_scalar(validated_single_line("parent", parent)?)
        ));
    }
    Ok(format!(
        "---\n{}\n---{}",
        fm.join("\n"),
        new_task_body(task)?
    ))
}

fn new_task_body(task: &NewBacklogTask) -> Result<String> {
    let mut chunks: Vec<String> = Vec::new();
    if !task.description.trim().is_empty() {
        chunks
            .push(marked_section_block("Description", "DESCRIPTION", &task.description).join("\n"));
    }
    let criteria = cleaned_list_values("acceptance criteria", &task.acceptance_criteria)?;
    if !criteria.is_empty() {
        chunks.push(checklist_block("Acceptance Criteria", "AC", &criteria, 1).join("\n"));
    }
    if chunks.is_empty() {
        return Ok("\n".to_string());
    }
    Ok(format!("\n\n{}\n", chunks.join("\n\n")))
}

/// The observed CLI filename convention: whitespace becomes `-`, characters
/// that are shell- or filesystem-hostile are dropped, runs of `-` collapse.
/// Capped at 180 chars so a long title can't overflow a 255-byte filename.
/// Only a convention, not an identity: the id lives in the frontmatter.
fn filename_slug(title: &str) -> String {
    const DROPPED: &[char] = &[
        '/', '\\', ':', '*', '?', '"', '\'', '<', '>', '|', '#', '%', '&', '{', '}', '$', '!', '@',
        '`', '+', '=',
    ];
    let mapped: String = title
        .chars()
        .filter(|c| !DROPPED.contains(c) && !c.is_control())
        .map(|c| if c.is_whitespace() { '-' } else { c })
        .collect();
    let joined = mapped
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let capped: String = joined.chars().take(180).collect();
    if capped.is_empty() {
        "task".to_string()
    } else {
        capped
    }
}

// ---- shared validation ----

fn validated_single_line<'v>(field: &str, value: &'v str) -> Result<&'v str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{field} must not be empty");
    }
    if trimmed.contains('\n') {
        bail!("{field} must be a single line");
    }
    Ok(trimmed)
}

fn cleaned_list_values(field: &str, values: &[String]) -> Result<Vec<String>> {
    let mut cleaned = Vec::with_capacity(values.len());
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.contains('\n') {
            bail!("{field} entries must be single-line");
        }
        cleaned.push(trimmed.to_string());
    }
    Ok(cleaned)
}

fn default_if_blank<'v>(value: &'v str, default: &'v str) -> &'v str {
    if value.trim().is_empty() {
        default
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse_task_file;
    use super::super::types::BacklogTaskSource;
    use super::*;
    use std::fs;

    /// A faithful CLI-shaped fixture: quoted title, block and inline lists,
    /// an `ordinal` key this crate doesn't model, marked sections, `#N`
    /// checklists.
    const FIXTURE: &str = "---\n\
        id: TASK-9\n\
        title: 'Fixture: a title the CLI would quote'\n\
        status: To Do\n\
        assignee: []\n\
        created_date: '2026-08-05 02:30'\n\
        labels:\n\
        \x20 - hub\n\
        \x20 - slice-2\n\
        dependencies: []\n\
        priority: medium\n\
        ordinal: 9000\n\
        ---\n\
        \n\
        ## Description\n\
        \n\
        <!-- SECTION:DESCRIPTION:BEGIN -->\n\
        Do the thing.\n\
        <!-- SECTION:DESCRIPTION:END -->\n\
        \n\
        ## Acceptance Criteria\n\
        <!-- AC:BEGIN -->\n\
        - [ ] #1 First criterion\n\
        - [x] #2 Second criterion\n\
        <!-- AC:END -->\n";

    fn fixture_file(dir: &tempfile::TempDir) -> std::path::PathBuf {
        let path = dir.path().join("task-9 - Fixture.md");
        fs::write(&path, FIXTURE).expect("fixture writes");
        path
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).expect("task file reads back")
    }

    /// Every line except the ones in `except` must survive an edit verbatim,
    /// in order — the module's core "surgical" promise.
    fn assert_only_lines_touched(before: &str, after: &str, except: &[&str]) {
        let filtered = |text: &str| -> Vec<String> {
            text.lines()
                .filter(|l| !except.iter().any(|prefix| l.starts_with(prefix)))
                .map(str::to_string)
                .collect()
        };
        assert_eq!(
            filtered(before),
            filtered(after),
            "an edit touched lines outside {except:?}"
        );
    }

    #[test]
    fn status_edit_touches_only_status_and_updated_date() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        let outcome = set_task_status(&path, "In Progress").expect("edit succeeds");

        assert_eq!(outcome, WriteOutcome::Changed);
        let after = read(&path);
        assert!(after.contains("status: In Progress"));
        assert!(
            after.contains("updated_date: '"),
            "any real change bumps updated_date"
        );
        assert!(after.contains("ordinal: 9000"), "unmodeled keys survive");
        assert_only_lines_touched(FIXTURE, &after, &["status:", "updated_date:"]);
    }

    #[test]
    fn noop_status_edit_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        let outcome = set_task_status(&path, "To Do").expect("edit succeeds");

        assert_eq!(outcome, WriteOutcome::Unchanged);
        assert_eq!(
            read(&path),
            FIXTURE,
            "a no-op must leave the file byte-identical"
        );
    }

    #[test]
    fn updated_date_is_inserted_after_created_date_when_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        assert_eq!(
            set_task_priority(&path, "high").expect("edit succeeds"),
            WriteOutcome::Changed
        );

        let after = read(&path);
        let lines: Vec<&str> = after.lines().collect();
        let created = lines
            .iter()
            .position(|l| l.starts_with("created_date:"))
            .expect("created_date survives");
        assert!(
            lines[created + 1].starts_with("updated_date:"),
            "updated_date sits directly after created_date, got {:?}",
            lines[created + 1]
        );
    }

    #[test]
    fn title_with_colon_is_single_quoted_and_reparses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        assert_eq!(
            set_task_title(&path, "New: with a colon, and 'quotes'").expect("edit succeeds"),
            WriteOutcome::Changed
        );

        assert!(read(&path).contains("title: 'New: with a colon, and ''quotes'''"));
        let task = parse_task_file(&path, BacklogTaskSource::Active).expect("reparses");
        assert_eq!(task.title, "New: with a colon, and 'quotes'");
    }

    #[test]
    fn list_edit_moves_between_inline_empty_and_block_styles() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        assert_eq!(
            set_task_list_field(&path, TaskListField::Dependencies, &["task-3".to_string()])
                .expect("edit succeeds"),
            WriteOutcome::Changed
        );
        let after = read(&path);
        assert!(after.contains("dependencies:\n  - task-3"));

        assert_eq!(
            set_task_list_field(&path, TaskListField::Labels, &[]).expect("edit succeeds"),
            WriteOutcome::Changed
        );
        assert!(
            read(&path).contains("labels: []"),
            "emptied list renders inline"
        );
    }

    #[test]
    fn label_add_and_remove_are_surgical_and_semantic_noops_write_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        assert_eq!(
            set_task_label(&path, "hub", true).expect("edit succeeds"),
            WriteOutcome::Unchanged,
            "adding a label already present must not rewrite the file"
        );
        assert_eq!(read(&path), FIXTURE);

        assert_eq!(
            set_task_label(&path, "dispatch", true).expect("edit succeeds"),
            WriteOutcome::Changed
        );
        let task = parse_task_file(&path, BacklogTaskSource::Active).expect("reparses");
        assert_eq!(task.labels, vec!["hub", "slice-2", "dispatch"]);

        assert_eq!(
            set_task_label(&path, "hub", false).expect("edit succeeds"),
            WriteOutcome::Changed
        );
        let task = parse_task_file(&path, BacklogTaskSource::Active).expect("reparses");
        assert_eq!(task.labels, vec!["slice-2", "dispatch"]);
    }

    #[test]
    fn swap_label_replaces_in_one_write_and_fails_without_the_source_label() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        assert_eq!(
            swap_task_label(&path, "hub", "hub-claimed").expect("edit succeeds"),
            WriteOutcome::Changed
        );
        let task = parse_task_file(&path, BacklogTaskSource::Active).expect("reparses");
        assert_eq!(task.labels, vec!["slice-2", "hub-claimed"]);

        let before = read(&path);
        let err = swap_task_label(&path, "hub", "again").expect_err("source label is gone");
        assert!(err.to_string().contains("hub"), "unexpected error: {err}");
        assert_eq!(read(&path), before, "a failed swap must not touch the file");
    }

    #[test]
    fn swap_label_never_duplicates_an_already_present_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        assert_eq!(
            swap_task_label(&path, "hub", "slice-2").expect("edit succeeds"),
            WriteOutcome::Changed
        );
        let task = parse_task_file(&path, BacklogTaskSource::Active).expect("reparses");
        assert_eq!(task.labels, vec!["slice-2"], "no duplicate slice-2");
    }

    #[test]
    fn milestone_assign_inserts_after_priority_and_clear_removes_the_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        assert_eq!(
            set_task_milestone(&path, Some("m-1")).expect("edit succeeds"),
            WriteOutcome::Changed
        );
        let after = read(&path);
        let lines: Vec<&str> = after.lines().collect();
        let priority = lines
            .iter()
            .position(|l| l.starts_with("priority:"))
            .expect("priority survives");
        assert_eq!(lines[priority + 1], "milestone: m-1");

        assert_eq!(
            set_task_milestone(&path, None).expect("edit succeeds"),
            WriteOutcome::Changed
        );
        assert!(!read(&path).contains("milestone:"));
    }

    #[test]
    fn replace_section_regenerates_markers_and_survives_fenced_headings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);
        let content = "Intro.\n\n```markdown\n## Not a real section\n```\n\nOutro.";

        assert_eq!(
            replace_task_section(&path, TaskSection::Description, content).expect("edit succeeds"),
            WriteOutcome::Changed
        );

        let task = parse_task_file(&path, BacklogTaskSource::Active).expect("reparses");
        assert_eq!(task.description, content);
        assert_eq!(
            task.acceptance_criteria.len(),
            2,
            "later sections untouched"
        );
        let after = read(&path);
        assert!(after.contains("<!-- SECTION:DESCRIPTION:BEGIN -->"));
        assert!(after.contains("<!-- SECTION:DESCRIPTION:END -->"));
    }

    #[test]
    fn replace_section_creates_a_missing_section_in_canonical_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        assert_eq!(
            replace_task_section(&path, TaskSection::ImplementationPlan, "1. Step").expect("edits"),
            WriteOutcome::Changed
        );

        let after = read(&path);
        let description = after.find("## Description").expect("description present");
        let criteria = after
            .find("## Acceptance Criteria")
            .expect("criteria present");
        let plan = after.find("## Implementation Plan").expect("plan inserted");
        assert!(
            description < criteria && criteria < plan,
            "plan lands after criteria per canonical section order"
        );
        let task = parse_task_file(&path, BacklogTaskSource::Active).expect("reparses");
        assert_eq!(task.implementation_plan, "1. Step");
        assert_eq!(task.acceptance_criteria.len(), 2);
    }

    /// TASK-45: a human section the format has no field for must neither
    /// block the save (the old guard refused it) nor be deleted by it (the
    /// pre-guard behavior). Surgery replaces only the target section's span,
    /// so the custom block survives byte-for-byte.
    #[test]
    fn replacing_a_section_preserves_an_unknown_section_byte_for_byte() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("task-1 - Custom.md");
        let custom_block = "## Resolution\n\nRoot cause: the cache.\n\n> verbatim   spacing kept\n";
        fs::write(
            &path,
            format!(
                "---\nid: TASK-1\ntitle: Custom\n---\n\n## Description\n\nOld body.\n\n{custom_block}"
            ),
        )
        .expect("fixture writes");

        let outcome = replace_task_section(&path, TaskSection::Description, "New body.")
            .expect("a unique custom section must not block a section replace");

        assert_eq!(outcome, WriteOutcome::Changed);
        let after = read(&path);
        assert!(
            after.contains(custom_block.trim_end()),
            "the custom section must survive verbatim: {after}"
        );
        assert!(after.contains("New body."), "{after}");
        assert!(!after.contains("Old body."), "{after}");
    }

    #[test]
    fn body_edits_fail_closed_on_a_lossy_body() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("task-1 - Lossy.md");
        fs::write(
            &path,
            "---\nid: TASK-1\ntitle: Lossy\n---\n\nOrphan preamble.\n\n## Description\n\nBody.\n",
        )
        .expect("fixture writes");
        let before = read(&path);

        let err =
            replace_task_section(&path, TaskSection::Description, "New.").expect_err("must refuse");

        assert!(
            err.to_string().contains("round-trip"),
            "unexpected error: {err}"
        );
        assert_eq!(
            read(&path),
            before,
            "a refused edit must not touch the file"
        );
    }

    #[test]
    fn append_notes_creates_the_section_then_appends_into_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        assert_eq!(
            append_task_notes(&path, "First note.").expect("edit succeeds"),
            WriteOutcome::Changed
        );
        assert_eq!(
            append_task_notes(&path, "Second note.").expect("edit succeeds"),
            WriteOutcome::Changed
        );

        let task = parse_task_file(&path, BacklogTaskSource::Active).expect("reparses");
        assert_eq!(task.implementation_notes, "First note.\n\nSecond note.");
        assert_eq!(
            task.description, "Do the thing.",
            "other sections untouched"
        );
    }

    #[test]
    fn append_acceptance_criteria_continues_numbering_and_disturbs_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        assert_eq!(
            append_task_acceptance_criteria(&path, &["Third".to_string(), " ".to_string()])
                .expect("edit succeeds"),
            WriteOutcome::Changed
        );

        let after = read(&path);
        assert!(after.contains("- [ ] #3 Third"));
        let task = parse_task_file(&path, BacklogTaskSource::Active).expect("reparses");
        assert_eq!(task.acceptance_criteria.len(), 3);
        assert!(
            task.acceptance_criteria[1].checked,
            "existing checked state untouched"
        );
        assert_only_lines_touched(FIXTURE, &after, &["- [ ] #3", "updated_date:"]);
    }

    #[test]
    fn checklist_check_flips_one_character_and_rechecking_is_a_noop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        let outcome = set_task_checklist_item(&path, TaskChecklist::AcceptanceCriteria, 1, true)
            .expect("edit succeeds");
        assert_eq!(outcome, WriteOutcome::Changed);
        let after = read(&path);
        assert!(after.contains("- [x] #1 First criterion"));
        assert_only_lines_touched(FIXTURE, &after, &["- [x] #1", "- [ ] #1", "updated_date:"]);

        let again = set_task_checklist_item(&path, TaskChecklist::AcceptanceCriteria, 1, true)
            .expect("edit succeeds");
        assert_eq!(again, WriteOutcome::Unchanged, "already-checked is a no-op");
    }

    #[test]
    fn checklist_errors_name_the_missing_index_and_missing_section() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);

        let err = set_task_checklist_item(&path, TaskChecklist::AcceptanceCriteria, 7, true)
            .expect_err("no #7 exists");
        assert!(err.to_string().contains("#7"), "unexpected error: {err}");

        let err = set_task_checklist_item(&path, TaskChecklist::DefinitionOfDone, 1, true)
            .expect_err("fixture has no DoD section");
        assert!(
            err.to_string().contains("Definition of Done"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn create_writes_the_cli_shape_exactly_and_reparses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let task = NewBacklogTask {
            title: "Ship it: a 'quoted' title".to_string(),
            description: "Why and what.".to_string(),
            status: String::new(),
            priority: String::new(),
            acceptance_criteria: vec!["One".to_string(), "Two".to_string()],
            parent: Some("TASK-3".to_string()),
            labels: vec!["format-fork".to_string()],
            assignees: vec![],
            milestone: Some("m-1".to_string()),
            dependencies: vec!["task-5".to_string()],
        };

        let path = write_new_task_file(dir.path(), "TASK", "42", &task).expect("create succeeds");

        let text = read(&path);
        let stamp_line = text
            .lines()
            .find(|l| l.starts_with("created_date:"))
            .expect("created_date present")
            .to_string();
        let expected = format!(
            "---\n\
             id: TASK-42\n\
             title: 'Ship it: a ''quoted'' title'\n\
             status: To Do\n\
             assignee: []\n\
             {stamp_line}\n\
             labels:\n\
             \x20 - format-fork\n\
             dependencies:\n\
             \x20 - task-5\n\
             priority: medium\n\
             milestone: m-1\n\
             parent_task_id: TASK-3\n\
             ---\n\
             \n\
             ## Description\n\
             \n\
             <!-- SECTION:DESCRIPTION:BEGIN -->\n\
             Why and what.\n\
             <!-- SECTION:DESCRIPTION:END -->\n\
             \n\
             ## Acceptance Criteria\n\
             <!-- AC:BEGIN -->\n\
             - [ ] #1 One\n\
             - [ ] #2 Two\n\
             <!-- AC:END -->\n"
        );
        assert_eq!(text, expected, "created file must match the CLI's shape");

        let parsed = parse_task_file(&path, BacklogTaskSource::Active).expect("reparses");
        assert_eq!(parsed.id, "TASK-42");
        assert_eq!(parsed.title, "Ship it: a 'quoted' title");
        assert_eq!(parsed.parent.as_deref(), Some("TASK-3"));
        assert_eq!(parsed.acceptance_criteria.len(), 2);
    }

    /// The reproduction: a project configured with `task_prefix: "LED"`
    /// (budget's own config) must mint `id: LED-11` and a `led-11 - ....md`
    /// filename — the exact shape a real budget task file carries — not the
    /// hardcoded `TASK-`/`task-` this crate used to emit regardless of
    /// config.
    #[test]
    fn create_honors_a_configured_prefix_in_both_id_and_filename() {
        let dir = tempfile::tempdir().expect("tempdir");
        let task = NewBacklogTask {
            title: "Fix the prefix bug".to_string(),
            description: String::new(),
            status: String::new(),
            priority: String::new(),
            acceptance_criteria: vec![],
            parent: None,
            labels: vec![],
            assignees: vec![],
            milestone: None,
            dependencies: vec![],
        };

        let path = write_new_task_file(dir.path(), "LED", "11", &task).expect("create succeeds");

        assert!(
            path.ends_with("led-11 - Fix-the-prefix-bug.md"),
            "filename must use the configured prefix, lowercased: {path:?}"
        );
        let text = read(&path);
        assert!(
            text.contains("id: LED-11"),
            "frontmatter id must use the configured prefix, uppercased: {text:?}"
        );

        let parsed = parse_task_file(&path, BacklogTaskSource::Active).expect("reparses");
        assert_eq!(parsed.id, "LED-11");
    }

    #[test]
    fn create_refuses_to_overwrite_an_existing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let task = NewBacklogTask {
            title: "Same title".to_string(),
            description: String::new(),
            status: String::new(),
            priority: String::new(),
            acceptance_criteria: vec![],
            parent: None,
            labels: vec![],
            assignees: vec![],
            milestone: None,
            dependencies: vec![],
        };

        let first =
            write_new_task_file(dir.path(), "TASK", "7", &task).expect("first create succeeds");
        let before = read(&first);
        let err =
            write_new_task_file(dir.path(), "TASK", "7", &task).expect_err("collision must fail");

        assert!(
            err.to_string().contains("creating"),
            "unexpected error: {err}"
        );
        assert_eq!(read(&first), before, "the existing file must be untouched");
    }

    #[test]
    fn filename_slug_matches_the_observed_convention() {
        assert_eq!(
            filename_slug("Dispatch worker: flag task → worktree"),
            "Dispatch-worker-flag-task-→-worktree"
        );
        assert_eq!(filename_slug("a/b\\c*d"), "abcd");
        assert_eq!(
            filename_slug("  "),
            "task",
            "a blank title still gets a filename"
        );
        assert!(filename_slug(&"x".repeat(400)).chars().count() <= 180);
    }

    /// Rename-over-file needs only *directory* permission, so without an
    /// explicit check the write layer would happily replace a file its
    /// owner marked read-only — a lock signal the CLI honored. Pinned here
    /// because the GUI's board-drag failure-path test injects its failure
    /// exactly this way.
    #[test]
    fn editing_a_read_only_file_is_refused_and_changes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = fixture_file(&dir);
        let mut perms = fs::metadata(&path).expect("metadata").permissions();
        perms.set_readonly(true);
        fs::set_permissions(&path, perms).expect("make read-only");

        let err = set_task_status(&path, "Done").expect_err("must refuse");

        assert!(
            err.to_string().contains("read-only"),
            "unexpected error: {err}"
        );
        assert_eq!(
            read(&path),
            FIXTURE,
            "a refused edit must not touch the file"
        );
        // No permission restore needed: unlinking a read-only file only
        // requires directory write permission, which the tempdir has.
    }

    #[test]
    fn editing_a_file_without_frontmatter_fails_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("task-1 - Bare.md");
        fs::write(&path, "just prose, no fences\n").expect("fixture writes");

        let err = set_task_status(&path, "Done").expect_err("must refuse");
        assert!(
            err.to_string().contains("frontmatter"),
            "unexpected error: {err}"
        );
        assert_eq!(read(&path), "just prose, no fences\n");
    }
}
