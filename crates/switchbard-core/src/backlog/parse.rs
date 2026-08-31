//! Turning a Backlog-format project directory (and the raw task markdown
//! inside it) into `super::types` structs. Read-only — writes live in
//! `super::write` behind the `super::mutations` facade.

use super::types::{BacklogChecklistItem, BacklogRepo, BacklogTask, BacklogTaskSource};
use anyhow::{bail, Context, Result};
use serde_yaml::{Mapping, Value};
use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn is_backlog_repo(root: &Path) -> bool {
    root.join("backlog/config.yml").is_file()
        || root.join("backlog/tasks").is_dir()
        || root.join("backlog/drafts").is_dir()
}

pub fn load_backlog_repo(root: &Path) -> Result<BacklogRepo> {
    if !is_backlog_repo(root) {
        bail!("{} is not a Backlog project", root.display());
    }

    let mut warnings = Vec::new();
    let mut tasks = Vec::new();
    for (rel, source) in [
        ("tasks", BacklogTaskSource::Active),
        ("completed", BacklogTaskSource::Completed),
        ("drafts", BacklogTaskSource::Draft),
        ("archive/tasks", BacklogTaskSource::Archived),
    ] {
        let dir = root.join("backlog").join(rel);
        if !dir.is_dir() {
            continue;
        }
        let mut entries = fs::read_dir(&dir)
            .with_context(|| format!("cannot read {}", dir.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(OsStr::to_str) == Some("md"))
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            match parse_task_file(&path, source) {
                Ok((task, task_warnings)) => {
                    warnings.extend(
                        task_warnings
                            .into_iter()
                            .map(|warning| format!("{}: {warning}", path.display())),
                    );
                    tasks.push(task);
                }
                Err(err) => warnings.push(format!("{}: {err}", path.display())),
            }
        }
    }

    tasks.sort_by(compare_tasks);
    let project_defs = super::hierarchy::load_project_defs(root, &mut warnings);
    let initiative_defs = super::hierarchy::load_initiative_defs(root, &mut warnings);
    let goals = super::goals::load_goals(root, &mut warnings);
    Ok(BacklogRepo {
        root: root.to_path_buf(),
        tasks,
        warnings,
        project_defs,
        initiative_defs,
        goals,
        loaded_at_unix: unix_now(),
        configured_statuses: parse_config_statuses(root),
    })
}

/// Read `backlog/config.yml`'s `statuses:` array — see `BacklogRepo::
/// configured_statuses`'s doc for why this is worth a second read alongside
/// the task files themselves. Never fails the whole project load: a
/// missing/unreadable/malformed config just yields an empty list, same as
/// if this function didn't exist.
pub(super) fn parse_config_statuses(root: &Path) -> Vec<String> {
    let path = root.join("backlog/config.yml");
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(value) = serde_yaml::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let Some(mapping) = value.as_mapping() else {
        return Vec::new();
    };
    yaml_string_list(mapping, "statuses")
}

/// The `backlog` CLI's own default task-id prefix, used whenever a project's
/// `backlog/config.yml` doesn't declare `task_prefix` — the exact value this
/// crate's id allocator hardcoded before it read the config at all (see
/// `super::allocate`'s module doc for the LED-prefixed-project bug that
/// hardcoding caused).
pub(super) const DEFAULT_TASK_PREFIX: &str = "TASK";

/// Read `backlog/config.yml`'s `task_prefix:` scalar, uppercased.
///
/// Uppercased because that's what the CLI itself writes into the id
/// (`id: LED-272`) regardless of how a human typed the key's value; the
/// lowercase form used in filenames (`led-272 - ....md`) is derived from this
/// at the call site, not stored separately — one value, two renderings.
///
/// Never fails the whole project load: a missing/unreadable/malformed
/// config, or a project that simply doesn't declare the key, yields
/// [`DEFAULT_TASK_PREFIX`] — same fallback as `parse_config_statuses`.
pub(super) fn configured_task_prefix(root: &Path) -> String {
    let path = root.join("backlog/config.yml");
    let Ok(text) = fs::read_to_string(&path) else {
        return DEFAULT_TASK_PREFIX.to_string();
    };
    let Ok(value) = serde_yaml::from_str::<Value>(&text) else {
        return DEFAULT_TASK_PREFIX.to_string();
    };
    let Some(mapping) = value.as_mapping() else {
        return DEFAULT_TASK_PREFIX.to_string();
    };
    mapping
        .get(Value::String("task_prefix".to_string()))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_uppercase)
        .unwrap_or_else(|| DEFAULT_TASK_PREFIX.to_string())
}

/// [`body_round_trips`] for one task file on disk — what `crate::refine`
/// asks before it overwrites a whole section.
///
/// Fails **closed**: an unreadable file returns `false`. The only reason to
/// call this is to decide whether a destructive replace-write is safe, and
/// "I could not check" must mean "do not write", never "assume it's fine".
pub fn task_file_round_trips(path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let (_, body) = split_frontmatter(&text);
    body_round_trips(body)
}

/// Parse one task file. The second tuple element is per-file *warnings* —
/// conditions worth surfacing that don't make the file unusable (today:
/// carrying both the `project:` key and its legacy `milestone:` spelling).
/// Hard failures stay in the `Err` channel.
pub(super) fn parse_task_file(
    path: &Path,
    source: BacklogTaskSource,
) -> Result<(BacklogTask, Vec<String>)> {
    let text = fs::read_to_string(path).with_context(|| "cannot read task markdown")?;
    let (frontmatter, body) = split_frontmatter(&text);
    let mut warnings = Vec::new();
    let id = yaml_string(&frontmatter, "id").unwrap_or_else(|| id_from_filename(path));
    let title = yaml_string(&frontmatter, "title").unwrap_or_else(|| id.clone());
    let status = yaml_string(&frontmatter, "status").unwrap_or_else(|| match source {
        BacklogTaskSource::Completed => "Done".to_string(),
        BacklogTaskSource::Draft => "Draft".to_string(),
        BacklogTaskSource::Archived => "Archived".to_string(),
        BacklogTaskSource::Active => "To Do".to_string(),
    });
    let priority = yaml_string(&frontmatter, "priority").unwrap_or_else(|| "medium".to_string());
    let description = extract_section(body, "Description");
    let implementation_plan = extract_section(body, "Implementation Plan");
    let implementation_notes = extract_section(body, "Implementation Notes");
    let final_summary = extract_section(body, "Final Summary");
    let acceptance_criteria =
        parse_checklist_section(&extract_section(body, "Acceptance Criteria"));
    let definition_of_done = parse_checklist_section(&extract_section(body, "Definition of Done"));

    // `project:` is the fork's membership key (trajectory: *Linear-vocabulary
    // hierarchy*); `milestone:` is the pre-divergence spelling, read as a
    // fallback so no mass migration is needed. A file carrying both is
    // mechanically safe — `project:` wins — but worth a warning, since the
    // next membership write will drop the legacy key.
    let project = yaml_string(&frontmatter, "project");
    let legacy_milestone = yaml_string(&frontmatter, "milestone");
    if project.is_some() && legacy_milestone.is_some() {
        warnings.push(
            "carries both `project:` and legacy `milestone:`; `project:` wins \
             (the next project assignment removes the legacy key)"
                .to_string(),
        );
    }

    let task = BacklogTask {
        id,
        title,
        status,
        priority,
        assignees: yaml_string_list(&frontmatter, "assignee"),
        labels: yaml_string_list(&frontmatter, "labels"),
        dependencies: yaml_string_list(&frontmatter, "dependencies"),
        references: yaml_string_list(&frontmatter, "references"),
        project: project.or(legacy_milestone),
        // The real `backlog` CLI (v1.47.1) writes `parent_task_id:`, not
        // `parent:` — confirmed empirically in the 2026-08-05 QA audit
        // (docs/qa/2026-08-05-parity-qa.md, Defect 1). Fall back to the old
        // key so fixtures/tasks written before this fix still parse.
        parent: yaml_string(&frontmatter, "parent_task_id")
            .or_else(|| yaml_string(&frontmatter, "parent")),
        created_date: yaml_string(&frontmatter, "created_date"),
        updated_date: yaml_string(&frontmatter, "updated_date"),
        description,
        implementation_plan,
        implementation_notes,
        final_summary,
        acceptance_criteria,
        definition_of_done,
        source,
        path: path.to_path_buf(),
    };
    Ok((task, warnings))
}

pub(super) fn split_frontmatter(text: &str) -> (Mapping, &str) {
    let Some(rest) = text.strip_prefix("---") else {
        return (Mapping::new(), text);
    };
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    let Some(end) = rest.find("\n---") else {
        return (Mapping::new(), text);
    };
    let yaml_text = &rest[..end];
    let body_start = end + "\n---".len();
    let body = rest[body_start..]
        .strip_prefix('\n')
        .unwrap_or(&rest[body_start..]);
    let mapping = serde_yaml::from_str::<Value>(yaml_text)
        .ok()
        .and_then(|value| value.as_mapping().cloned())
        .unwrap_or_default();
    (mapping, body)
}

pub(super) fn yaml_string(map: &Mapping, key: &str) -> Option<String> {
    let value = map.get(Value::String(key.to_string()))?;
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}

pub(super) fn yaml_string_list(map: &Mapping, key: &str) -> Vec<String> {
    let Some(value) = map.get(Value::String(key.to_string())) else {
        return Vec::new();
    };
    match value {
        Value::Sequence(items) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(s) => Some(s.trim().to_string()),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
            .filter(|s| !s.is_empty())
            .collect(),
        Value::String(s) => s
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// The body of one `## <heading>` section, with the CLI's own structural
/// marker comments removed.
///
/// Two rules here are load-bearing rather than cosmetic, because **every
/// replace-write path in this app writes back what this function returned**
/// (`edit_backlog_task`'s `-d`/`--plan`: the detail rail's Save, and
/// `crate::refine`). Anything this reader drops, the next save deletes from
/// disk. TASK-44's audit found both of the original rules lossy:
///
/// 1. **Code fences suspend section detection.** A `## ` line inside a
///    backtick- or tilde-fenced block is content — a markdown sample, a doc
///    excerpt, a diff — not the start of the next section. Ending the section
///    there dropped the rest of the fence *and every line after it*. Only a
///    *closed* fence counts (see [`scan_fences`]): an unterminated opener is
///    treated as literal text, so a malformed task degrades to the old
///    behavior instead of hiding all its later sections. That degradation is
///    safe for *reading* and unsafe for *writing*, which is why
///    [`body_round_trips`] refuses it outright.
/// 2. **Only the CLI's own `NAME:BEGIN`/`NAME:END` comments are dropped**
///    (see [`is_section_marker_comment`]), not every HTML comment. An
///    author's `<!-- note to self -->` is content and survives.
fn extract_section(body: &str, heading: &str) -> String {
    let fenced = scan_fences(body).inside;
    let mut in_section = false;
    let mut lines = Vec::new();
    for (i, line) in body.lines().enumerate() {
        let trimmed = line.trim_start();
        if !fenced[i] && trimmed.starts_with("## ") {
            if in_section {
                break;
            }
            let title = trimmed.trim_start_matches('#').trim();
            if title.eq_ignore_ascii_case(heading) {
                in_section = true;
            }
            continue;
        }
        if !in_section {
            continue;
        }
        if !fenced[i] && is_section_marker_comment(line.trim()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

/// Result of one pass over a body's code fences.
pub(super) struct FenceScan {
    /// Per line: is it inside (or a delimiter of) a *properly closed* fence?
    pub(super) inside: Vec<bool>,
    /// Did every opener find a closer? `false` means the body's fences are
    /// malformed — readable, but not safe to write back.
    balanced: bool,
}

/// Locate every properly closed code fence in `body`.
///
/// An opener with no closer is deliberately *not* treated as a fence, so
/// `extract_section` keeps finding the sections after it. A task's acceptance
/// criteria silently vanishing from every view is a far worse read-side
/// failure than the one fence-awareness exists to prevent. The write side
/// takes the opposite stance: [`body_round_trips`] rejects any body whose
/// fences don't balance, because that same degradation is what lets a
/// truncated read look self-consistent (audit finding R1).
///
/// Pairing follows CommonMark: a closer uses the *same character*, is *at
/// least as long* as its opener, and carries no info string. The length rule
/// matters — without it a four-backtick opener would be "closed" by an
/// ordinary three-backtick line inside it (audit finding R2), silently
/// truncating the fence and, with it, the section.
pub(super) fn scan_fences(body: &str) -> FenceScan {
    let lines: Vec<&str> = body.lines().collect();
    let mut inside = vec![false; lines.len()];
    let mut open: Option<(usize, char, usize)> = None;
    for (i, line) in lines.iter().enumerate() {
        let Some((marker, run)) = fence_run(line) else {
            continue;
        };
        match open {
            None => open = Some((i, marker, run)),
            Some((start, kind, opener_len))
                if kind == marker && run >= opener_len && closes_fence(line, marker, run) =>
            {
                for flag in inside.iter_mut().take(i + 1).skip(start) {
                    *flag = true;
                }
                open = None;
            }
            Some(_) => {}
        }
    }
    FenceScan {
        inside,
        balanced: open.is_none(),
    }
}

/// The fence character and run length a line opens with, if it is a fence
/// delimiter at all (3+ backticks or tildes).
fn fence_run(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = trimmed.chars().take_while(|c| *c == marker).count();
    (run >= 3).then_some((marker, run))
}

/// A closing fence carries nothing after its run — CommonMark forbids an info
/// string on the closer, which is exactly what distinguishes a nested
/// opener from the real close.
fn closes_fence(line: &str, marker: char, run: usize) -> bool {
    let trimmed = line.trim_start();
    let rest: String = trimmed.chars().skip(run).collect();
    debug_assert!(
        trimmed.starts_with(marker),
        "closes_fence is only called on a line fence_run already matched"
    );
    rest.trim().is_empty()
}

/// The `## ` headings the Backlog format defines: exactly the six sections
/// `parse_task_file` extracts. Used by [`body_round_trips`] rule 3 (a
/// *known* heading swallowed by a fence means the structure was misread)
/// and by the write layer's canonical section ordering. Deliberately *not*
/// an allowlist: a heading outside this set is an opaque human section the
/// surgical writer preserves untouched (TASK-45).
pub(super) const KNOWN_SECTION_HEADINGS: &[&str] = &[
    "Description",
    "Acceptance Criteria",
    "Implementation Plan",
    "Implementation Notes",
    "Definition of Done",
    "Final Summary",
];

fn is_known_section_heading(title: &str) -> bool {
    KNOWN_SECTION_HEADINGS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(title))
}

/// The `## ` heading a line declares, if it declares one.
pub(super) fn heading_title(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    trimmed
        .starts_with("## ")
        .then(|| trimmed.trim_start_matches('#').trim())
}

/// One of the `backlog` CLI's own structural markers — `<!-- AC:BEGIN -->`,
/// `<!-- DOD:END -->`, `<!-- SECTION:DESCRIPTION:BEGIN -->` and friends (the
/// full set observed across the tracked repos' task files). Matching the
/// shape rather than an enumerated list keeps a future
/// `SECTION:WHATEVER:BEGIN` working; matching *only* this shape keeps an
/// author's ordinary HTML comment out of the discard pile.
fn is_section_marker_comment(trimmed: &str) -> bool {
    let Some(inner) = trimmed
        .strip_prefix("<!--")
        .and_then(|rest| rest.strip_suffix("-->"))
    else {
        return false;
    };
    let inner = inner.trim();
    let Some((name, terminator)) = inner.rsplit_once(':') else {
        return false;
    };
    if !matches!(terminator, "BEGIN" | "END") {
        return false;
    }
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == ':' || c == '_')
}

/// Is this body's structure one a section-replacing write can safely be based
/// on?
///
/// The write-path safety net for TASK-44's audit findings F1/R1/R3.
/// `extract_section` is fence- and comment-aware now, but a *replace*-write
/// (`-d`, `--plan`) stakes real user data on the reader being lossless, and
/// the next lossy case is by definition one nobody has thought of yet. So
/// callers about to overwrite a whole section ask this first and skip the
/// write when it is `false`.
///
/// ## Four rules, and why conservation alone was not enough
///
/// The first version of this function checked conservation only: every
/// non-blank line must reappear, in order, across the extracted sections.
/// That check was **circular** — it derived "which lines are headings" with
/// the same predicate the reader uses, so a lossy read that manifested *as a
/// spurious heading* was self-consistent and passed. The auditor's repro:
/// `Intro.` / an unterminated ```` ```sh ```` / `## build` / `make all`. The
/// unmatched opener is not a fence, so `## build` looks like a section to
/// both the reader and the check; conservation balanced, the guard said
/// "safe", and the write deleted `## build` and `make all`. Three structural
/// rules now bound that class before conservation runs at all:
///
/// 1. **Fences must balance.** An unmatched opener is exactly the state that
///    makes a truncated read look self-consistent (R1), and the state a
///    mismatched closer length produces (R2).
/// 2. **No unfenced `## ` heading may repeat** (case-insensitive, known or
///    unknown). `extract_section` returns only a heading's *first* span, so a
///    repeat is exactly the state where "the section" is ambiguous — a file
///    already carrying one fails closed rather than trapping the caller in a
///    loop (R3). *Unknown* headings are deliberately allowed (TASK-45): the
///    write layer is surgical, so a section this format has no field for —
///    `## Resolution`, `## Root Cause Hypothesis` on 51 of 345 real task
///    files measured during TASK-44 — is an opaque block no edit ever
///    enters, and conservation (rule 4) covers its content like any other
///    section's. Refusing them protected nothing while freezing ~15% of
///    real tasks; prose misread as a heading now costs at worst a *split*
///    (the text survives byte-for-byte under its own opaque heading), never
///    a deletion.
/// 3. **No known section heading may sit inside a fence.** That means fence
///    pairing swallowed a real section boundary, which is how a plan heading
///    ends up embedded in a description (R3).
/// 4. **Conservation**, as before: every non-blank line reappears exactly
///    once, in order, across the extracted sections — ignoring headings and
///    the CLI's own markers, which the writer regenerates. Lines are compared
///    trimmed; interior indentation is provably preserved by
///    `extract_section` (it pushes each line verbatim), so the trim only
///    absorbs whitespace its final `.trim()` touches and cannot mask a
///    dropped or reordered line.
///
/// This bounds a class; it is not a proof of losslessness. Rules 1–3 make the
/// structure recognizable before rule 4 compares content, which is what stops
/// the reader from being its own witness — but a future reader bug that
/// preserves balanced fences, unique headings, and line conservation would
/// still slip through. It is a strong check, not a theorem.
pub fn body_round_trips(body: &str) -> bool {
    let scan = scan_fences(body);
    // Rule 1.
    if !scan.balanced {
        return false;
    }

    let mut expected: Vec<&str> = Vec::new();
    let mut headings: Vec<&str> = Vec::new();
    for (i, line) in body.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match heading_title(line) {
            // Rule 3.
            Some(title) if scan.inside[i] && is_known_section_heading(title) => return false,
            Some(_) if scan.inside[i] => {}
            // Rule 2.
            Some(title) => {
                if headings.iter().any(|seen| seen.eq_ignore_ascii_case(title)) {
                    return false;
                }
                headings.push(title);
                continue;
            }
            None => {}
        }
        if !scan.inside[i] && is_section_marker_comment(trimmed) {
            continue;
        }
        expected.push(trimmed);
    }

    // Rule 4.
    let mut actual: Vec<String> = Vec::new();
    for heading in &headings {
        for line in extract_section(body, heading).lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                actual.push(trimmed.to_string());
            }
        }
    }

    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual.iter())
            .all(|(want, got)| *want == got.as_str())
}

fn parse_checklist_section(section: &str) -> Vec<BacklogChecklistItem> {
    let mut out = Vec::new();
    for line in section.lines() {
        let Some(rest) = line.trim().strip_prefix("- [") else {
            continue;
        };
        let Some((mark, rest)) = rest.split_once(']') else {
            continue;
        };
        let checked = mark.trim().eq_ignore_ascii_case("x");
        let rest = rest.trim();
        let (index, text) = parse_checklist_index(rest, out.len() + 1);
        if text.is_empty() {
            continue;
        }
        out.push(BacklogChecklistItem {
            index,
            checked,
            text,
        });
    }
    out
}

pub(super) fn parse_checklist_index(text: &str, fallback: usize) -> (usize, String) {
    let Some(rest) = text.strip_prefix('#') else {
        return (fallback, text.trim().to_string());
    };
    let digits_len = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .map(char::len_utf8)
        .sum::<usize>();
    if digits_len == 0 {
        return (fallback, text.trim().to_string());
    }
    let index = rest[..digits_len].parse::<usize>().unwrap_or(fallback);
    let label = rest[digits_len..].trim().to_string();
    (index, label)
}

fn id_from_filename(path: &Path) -> String {
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("task");
    let id = stem
        .split_whitespace()
        .next()
        .unwrap_or(stem)
        .trim_start_matches("task-")
        .trim_start_matches("TASK-");
    format!("TASK-{}", id.to_ascii_uppercase())
}

fn compare_tasks(a: &BacklogTask, b: &BacklogTask) -> Ordering {
    source_rank(a.source)
        .cmp(&source_rank(b.source))
        .then_with(|| status_rank(&a.status).cmp(&status_rank(&b.status)))
        .then_with(|| priority_rank(&a.priority).cmp(&priority_rank(&b.priority)))
        .then_with(|| task_id_key(&a.id).cmp(&task_id_key(&b.id)))
        .then_with(|| a.title.cmp(&b.title))
}

fn source_rank(source: BacklogTaskSource) -> usize {
    match source {
        BacklogTaskSource::Active => 0,
        BacklogTaskSource::Draft => 1,
        BacklogTaskSource::Completed => 2,
        BacklogTaskSource::Archived => 3,
    }
}

fn status_rank(status: &str) -> usize {
    match status.to_ascii_lowercase().as_str() {
        "in progress" => 0,
        "to do" => 1,
        "done" => 2,
        "draft" => 3,
        "archived" => 4,
        _ => 5,
    }
}

fn priority_rank(priority: &str) -> usize {
    match priority.to_ascii_lowercase().as_str() {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 3,
    }
}

fn task_id_key(id: &str) -> Vec<u32> {
    id.trim_start_matches("TASK-")
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok())
        .collect()
}

/// Milliseconds, not seconds — see `BacklogRepo::loaded_at_unix`'s doc
/// for why the finer precision matters.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Parse a Backlog `"YYYY-MM-DD HH:MM"` timestamp (`created_date`/
/// `updated_date`) into a day count since the Unix epoch. Shared by
/// `backlog_stats` (burndown, portfolio) and `backlog_relations` ("newly
/// unblocked") — both only need day granularity, unlike `backlog_triage`'s
/// age-based tiebreak, which keeps its own seconds-precision parser.
pub fn parse_backlog_day(value: &str) -> Option<i64> {
    chrono::NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%d %H:%M")
        .ok()
        .map(|dt| dt.and_utc().timestamp().div_euclid(86_400))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TASK-25 (owner-requested UX): `configured_statuses` reads
    /// `backlog/config.yml`'s `statuses:` array — budget's own config
    /// declares exactly this set.
    #[test]
    fn parses_config_statuses_from_config_yml() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("backlog")).unwrap();
        fs::write(
            dir.path().join("backlog/config.yml"),
            "project_name: \"Ledger\"\ndefault_status: \"To Do\"\nstatuses: [\"Icebox\", \"To Do\", \"In Progress\", \"In Review\", \"Done\"]\n",
        )
        .unwrap();

        let statuses = parse_config_statuses(dir.path());
        assert_eq!(
            statuses,
            vec!["Icebox", "To Do", "In Progress", "In Review", "Done"]
        );
    }

    /// Missing/malformed config is never fatal — `configured_statuses` just
    /// comes back empty, same as if the function didn't exist. Confirms
    /// both a fully-missing file and a config.yml with no `statuses` key.
    #[test]
    fn missing_or_statusless_config_yields_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(parse_config_statuses(dir.path()), Vec::<String>::new());

        fs::create_dir_all(dir.path().join("backlog")).unwrap();
        fs::write(
            dir.path().join("backlog/config.yml"),
            "project_name: \"No statuses key\"\n",
        )
        .unwrap();
        assert_eq!(parse_config_statuses(dir.path()), Vec::<String>::new());
    }

    /// The bug this exists to fix: budget's `backlog/config.yml` declares
    /// `task_prefix: "LED"`, and the value must come back uppercased
    /// regardless of how it's spelled in the file.
    #[test]
    fn reads_a_declared_task_prefix_uppercased() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("backlog")).unwrap();
        fs::write(
            dir.path().join("backlog/config.yml"),
            "project_name: \"Ledger\"\ntask_prefix: \"led\"\n",
        )
        .unwrap();

        assert_eq!(configured_task_prefix(dir.path()), "LED");
    }

    /// Missing config, a config with no `task_prefix` key, and a config with
    /// no `backlog/` directory at all must all fall back to the CLI's own
    /// default — preserving exactly the behavior this crate hardcoded before
    /// it read the config.
    #[test]
    fn missing_task_prefix_falls_back_to_the_cli_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(configured_task_prefix(dir.path()), DEFAULT_TASK_PREFIX);

        fs::create_dir_all(dir.path().join("backlog")).unwrap();
        fs::write(
            dir.path().join("backlog/config.yml"),
            "project_name: \"No prefix key\"\n",
        )
        .unwrap();
        assert_eq!(configured_task_prefix(dir.path()), DEFAULT_TASK_PREFIX);
    }

    /// TASK-44 audit finding F1 (HIGH). `extract_section` used to end a
    /// section at *any* line starting with `## ` and to drop *any* line
    /// starting with `<!--`. Both are lossy in the read direction, and every
    /// replace-write path in this app (`edit_backlog_task`'s `-d`/`--plan`,
    /// i.e. the detail rail's Save and `crate::refine`) writes back what the
    /// parser returned — so a fenced code block containing a `## ` heading,
    /// a non-marker HTML comment, or anything after such a fence was
    /// permanently deleted on the next save. These four tests pin the exact
    /// shapes the auditor reproduced end to end.
    #[test]
    fn a_markdown_heading_inside_a_code_fence_does_not_end_the_section() {
        let body = "## Description\n\n\
                    <!-- SECTION:DESCRIPTION:BEGIN -->\n\
                    Intro paragraph.\n\n\
                    ```markdown\n\
                    ## Not a real section\n\
                    fenced body\n\
                    ```\n\n\
                    Trailing paragraph after the fence.\n\
                    <!-- SECTION:DESCRIPTION:END -->\n\n\
                    ## Acceptance Criteria\n\
                    <!-- AC:BEGIN -->\n\
                    - [ ] #1 Real criterion\n\
                    <!-- AC:END -->\n";

        let description = extract_section(body, "Description");

        assert!(
            description.contains("## Not a real section"),
            "a heading inside a fence is content, not a section break: {description:?}"
        );
        assert!(
            description.contains("Trailing paragraph after the fence."),
            "content after the fence must survive: {description:?}"
        );
        assert_eq!(
            parse_checklist_section(&extract_section(body, "Acceptance Criteria")).len(),
            1,
            "the real section break after the fence must still be honored"
        );
    }

    #[test]
    fn a_non_marker_html_comment_is_preserved_as_content() {
        let body = "## Description\n\n\
                    <!-- SECTION:DESCRIPTION:BEGIN -->\n\
                    Before.\n\
                    <!-- a note the author wrote on purpose -->\n\
                    After.\n\
                    <!-- SECTION:DESCRIPTION:END -->\n";

        let description = extract_section(body, "Description");

        assert!(
            description.contains("<!-- a note the author wrote on purpose -->"),
            "only the CLI's own SECTION/AC/DOD markers may be dropped: {description:?}"
        );
    }

    #[test]
    fn the_clis_own_section_markers_are_still_dropped() {
        let body = "## Description\n\n\
                    <!-- SECTION:DESCRIPTION:BEGIN -->\n\
                    Body.\n\
                    <!-- SECTION:DESCRIPTION:END -->\n";

        assert_eq!(extract_section(body, "Description"), "Body.");
    }

    /// Degrade-to-old-behavior guard: an unterminated fence must not swallow
    /// every later section (which would make a task's acceptance criteria
    /// silently vanish from every view). An opener with no closer is treated
    /// as literal text, exactly as before this fix.
    #[test]
    fn an_unterminated_code_fence_does_not_swallow_the_following_sections() {
        let body = "## Description\n\n\
                    ```rust\n\
                    fn never_closed() {}\n\n\
                    ## Acceptance Criteria\n\
                    - [ ] #1 Still visible\n";

        assert_eq!(
            parse_checklist_section(&extract_section(body, "Acceptance Criteria")).len(),
            1,
            "an unbalanced fence must not hide later sections"
        );
    }

    /// The write-path safety net (audit finding F1, layer 2). Any future
    /// lossy read this fix did not anticipate must degrade to a no-op write,
    /// never to a silent deletion — so `crate::refine` asks this before it
    /// emits a `-d`/`--plan` replace-write.
    #[test]
    fn body_round_trips_accepts_a_body_the_parser_reproduces_completely() {
        let body = "## Description\n\n\
                    <!-- SECTION:DESCRIPTION:BEGIN -->\n\
                    Intro.\n\n\
                    ```markdown\n\
                    ## Fenced heading\n\
                    ```\n\
                    Outro.\n\
                    <!-- SECTION:DESCRIPTION:END -->\n\n\
                    ## Acceptance Criteria\n\
                    <!-- AC:BEGIN -->\n\
                    - [ ] #1 Criterion\n\
                    <!-- AC:END -->\n";

        assert!(body_round_trips(body));
    }

    #[test]
    fn body_round_trips_rejects_a_body_whose_content_the_parser_drops() {
        // Content before the first `## ` heading belongs to no section, so
        // the parser cannot reproduce it — exactly the class of silent loss
        // the guard exists to catch.
        let body = "A stray preamble the parser has nowhere to put.\n\n\
                    ## Description\n\n\
                    Body.\n";

        assert!(!body_round_trips(body));
    }

    // ---- audit round 2: the guard must not be its own witness ----

    /// R1, the auditor's exact repro. An unmatched fence opener is not a
    /// fence, so `## build` looked like a section heading to the reader *and*
    /// to the conservation check — self-consistent, "safe", and the write
    /// then deleted `## build` and `make all`. Rejected by rule 1: the
    /// unbalanced fence is exactly the state that makes a truncated read
    /// look self-consistent. (The unknown heading itself is no longer a
    /// ground for refusal — see TASK-45 — but this body never reaches rule 2.)
    #[test]
    fn body_round_trips_rejects_a_spurious_heading_produced_by_an_unterminated_fence() {
        let body = "## Description\n\n\
                    Intro.\n\
                    ```sh\n\
                    ## build\n\
                    make all\n";

        assert!(
            !body_round_trips(body),
            "conservation alone called this safe; it is not"
        );
    }

    /// TASK-45 inverted the old "reject any unknown heading" rule: the write
    /// layer is surgical, so a human section the format has no field for
    /// (`## Resolution` on 51 of 345 real task files) is an opaque block a
    /// section-replace never enters. Refusing it froze ~15% of real tasks
    /// while protecting nothing.
    #[test]
    fn body_round_trips_accepts_a_unique_unknown_heading_as_an_opaque_section() {
        let known = "## Description\n\nIntro.\n\n## Implementation Notes\n\nNotes.\n";
        assert!(body_round_trips(known));

        let custom = "## Description\n\nIntro.\n\n## Resolution\n\nRoot cause: the cache.\n";
        assert!(
            body_round_trips(custom),
            "a unique custom section is preserved by surgical writes, not a reason to refuse"
        );
    }

    /// The half of the old rule 2 that survives: a *repeated* heading — known
    /// or unknown — makes "the section" ambiguous (`extract_section` returns
    /// only the first span), so it still fails closed.
    #[test]
    fn body_round_trips_rejects_a_repeated_heading() {
        let known = "## Description\n\nA.\n\n## Description\n\nB.\n";
        assert!(
            !body_round_trips(known),
            "a duplicated known heading is ambiguous"
        );

        let unknown = "## Resolution\n\nA.\n\n## Resolution\n\nB.\n";
        assert!(
            !body_round_trips(unknown),
            "a duplicated custom heading is just as ambiguous"
        );
    }

    /// R2: CommonMark requires a closing fence at least as long as its
    /// opener. Closing a four-backtick fence on an ordinary three-backtick
    /// line ends the fence early, which drops `## Not a section` back out
    /// into the open where it truncates the section.
    #[test]
    fn a_short_fence_line_does_not_close_a_longer_opener() {
        let body = "## Description\n\n\
                    ````markdown\n\
                    ```\n\
                    ## Not a section\n\
                    ````\n\
                    Outro.\n\n\
                    ## Acceptance Criteria\n\
                    - [ ] #1 Criterion\n";

        let description = extract_section(body, "Description");

        assert!(
            description.contains("## Not a section"),
            "a 3-backtick line cannot close a 4-backtick fence: {description:?}"
        );
        assert!(description.contains("Outro."), "{description:?}");
        assert!(body_round_trips(body));
        assert_eq!(
            parse_checklist_section(&extract_section(body, "Acceptance Criteria")).len(),
            1,
            "the real section break after the fence must still be honored"
        );
    }

    /// …and a closer may not carry an info string, which is what keeps a
    /// *nested opener* from being mistaken for the close.
    #[test]
    fn a_fence_line_with_an_info_string_is_an_opener_not_a_closer() {
        let body = "## Description\n\n\
                    ```sh\n\
                    ```python\n\
                    ```\n\
                    Outro.\n";

        let description = extract_section(body, "Description");

        assert!(description.contains("```python"), "{description:?}");
        assert!(description.contains("Outro."), "{description:?}");
    }

    /// R3, the one-way trap. An unterminated opener in Description pairs with
    /// the opener in Implementation Plan, swallowing the plan's own heading.
    /// Left unguarded, refine would then see an "empty" plan to fill and
    /// write a description containing a literal `## Implementation Plan`
    /// line — corrupting the file in a way that blocks every later refine.
    /// Rule 3 refuses at the write, so the trap is never built.
    #[test]
    fn body_round_trips_rejects_a_fence_that_swallowed_a_real_section_heading() {
        let body = "## Description\n\n\
                    <!-- SECTION:DESCRIPTION:BEGIN -->\n\
                    Intro.\n\
                    ```sh\n\
                    make all\n\
                    <!-- SECTION:DESCRIPTION:END -->\n\n\
                    ## Implementation Plan\n\n\
                    <!-- SECTION:PLAN:BEGIN -->\n\
                    ```python\n\
                    code\n\
                    ```\n\
                    <!-- SECTION:PLAN:END -->\n";

        assert!(
            !body_round_trips(body),
            "a known section heading buried inside a fence means the pairing ate a boundary"
        );
    }

    /// …and a file already carrying the damage fails closed rather than
    /// looping: the caller reports "cannot round-trip" forever instead of
    /// re-corrupting it on every run.
    #[test]
    fn body_round_trips_rejects_an_already_duplicated_section_heading() {
        let body = "## Description\n\nIntro.\n\n\
                    ## Implementation Plan\n\n1. Step\n\n\
                    ## Implementation Plan\n\n2. Step\n";

        assert!(!body_round_trips(body));
    }

    /// The file-level entry point `crate::refine` actually calls. Fails
    /// closed on a missing file, because "I could not check" must never be
    /// read as "safe to overwrite".
    #[test]
    fn task_file_round_trips_distinguishes_a_normal_task_from_a_lossy_one() {
        let dir = tempfile::tempdir().unwrap();

        let good = dir.path().join("good.md");
        fs::write(
            &good,
            "---\nid: TASK-1\ntitle: Fine\n---\n\n\
             ## Description\n\n\
             <!-- SECTION:DESCRIPTION:BEGIN -->\n\
             Intro.\n\n\
             ```markdown\n\
             ## Fenced heading\n\
             ```\n\
             Outro.\n\
             <!-- SECTION:DESCRIPTION:END -->\n",
        )
        .unwrap();
        assert!(task_file_round_trips(&good));

        // Body text sitting before the first `## ` heading belongs to no
        // section, so no replace-write could ever put it back.
        let lossy = dir.path().join("lossy.md");
        fs::write(
            &lossy,
            "---\nid: TASK-2\ntitle: Lossy\n---\n\n\
             An orphan preamble.\n\n\
             ## Description\n\n\
             Body.\n",
        )
        .unwrap();
        assert!(!task_file_round_trips(&lossy));

        assert!(
            !task_file_round_trips(&dir.path().join("does-not-exist.md")),
            "an unreadable file must fail closed"
        );
    }

    #[test]
    fn parses_backlog_task_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("task-18 - Example.md");
        fs::write(
            &path,
            r#"---
id: TASK-18
title: Example task
status: To Do
assignee:
  - ben
labels:
  - research
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Do the thing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 First criterion
- [x] #2 Second criterion
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Existing note.
<!-- SECTION:NOTES:END -->
"#,
        )
        .unwrap();

        let task = parse_task_file(&path, BacklogTaskSource::Active).unwrap().0;

        assert_eq!(task.id, "TASK-18");
        assert_eq!(task.title, "Example task");
        assert_eq!(task.priority, "low");
        assert_eq!(task.assignees, vec!["ben"]);
        assert_eq!(task.labels, vec!["research"]);
        assert_eq!(task.description, "Do the thing.");
        assert_eq!(task.implementation_notes, "Existing note.");
        assert_eq!(task.acceptance_criteria.len(), 2);
        assert_eq!(task.acceptance_criteria[0].index, 1);
        assert!(!task.acceptance_criteria[0].checked);
        assert!(task.acceptance_criteria[1].checked);
    }

    /// The Linear-hierarchy divergence's membership-key rule (trajectory:
    /// *Linear-vocabulary hierarchy*, divergence 1): `project:` is the key,
    /// `milestone:` is the legacy fallback, and a file carrying both parses
    /// with `project:` winning plus a warning naming the condition.
    #[test]
    fn project_key_is_preferred_with_legacy_milestone_as_fallback() {
        let dir = tempfile::tempdir().unwrap();

        let with_project = dir.path().join("task-1 - New.md");
        fs::write(
            &with_project,
            "---\nid: TASK-1\ntitle: New\nstatus: To Do\nproject: Lucella cutover\n---\n",
        )
        .unwrap();
        let (task, warnings) = parse_task_file(&with_project, BacklogTaskSource::Active).unwrap();
        assert_eq!(task.project.as_deref(), Some("Lucella cutover"));
        assert!(warnings.is_empty(), "{warnings:?}");

        let legacy = dir.path().join("task-2 - Legacy.md");
        fs::write(
            &legacy,
            "---\nid: TASK-2\ntitle: Legacy\nstatus: To Do\nmilestone: v1\n---\n",
        )
        .unwrap();
        let (task, warnings) = parse_task_file(&legacy, BacklogTaskSource::Active).unwrap();
        assert_eq!(
            task.project.as_deref(),
            Some("v1"),
            "legacy milestone: still resolves membership"
        );
        assert!(warnings.is_empty(), "the fallback alone is not a warning");

        let both = dir.path().join("task-3 - Both.md");
        fs::write(
            &both,
            "---\nid: TASK-3\ntitle: Both\nstatus: To Do\nproject: current\nmilestone: stale\n---\n",
        )
        .unwrap();
        let (task, warnings) = parse_task_file(&both, BacklogTaskSource::Active).unwrap();
        assert_eq!(task.project.as_deref(), Some("current"), "project: wins");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("legacy `milestone:`"), "{warnings:?}");
    }

    /// The both-keys warning must survive the project-load aggregation with
    /// the file path prefixed, since `BacklogRepo::warnings` is where every
    /// surface reads it from.
    #[test]
    fn both_keys_warning_reaches_repo_warnings_with_the_path() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("backlog/tasks")).unwrap();
        fs::write(dir.path().join("backlog/config.yml"), "statuses: []\n").unwrap();
        let path = dir.path().join("backlog/tasks/task-1 - Both.md");
        fs::write(
            &path,
            "---\nid: TASK-1\ntitle: Both\nstatus: To Do\nproject: a\nmilestone: b\n---\n",
        )
        .unwrap();

        let repo = load_backlog_repo(dir.path()).unwrap();
        assert_eq!(repo.warnings.len(), 1, "{:?}", repo.warnings);
        assert!(
            repo.warnings[0].starts_with(&path.display().to_string()),
            "{:?}",
            repo.warnings
        );
    }

    /// Regression for the 2026-08-05 QA audit's HIGH defect: the format
    /// (defined then by the `backlog` CLI, now by `super::write`) uses
    /// `parent_task_id:`, not `parent:`. This is the fast, in-process
    /// complement to `backlog_mutations.rs`'s on-disk round trip — it pins
    /// the parser's key preference directly, plus the fallback for
    /// `parent:`-only fixtures written before this fix.
    #[test]
    fn parses_parent_task_id_and_falls_back_to_the_old_parent_key() {
        let dir = tempfile::tempdir().unwrap();

        let real_cli_path = dir.path().join("task-2 - Subtask.md");
        fs::write(
            &real_cli_path,
            "---\nid: TASK-2\ntitle: Subtask\nparent_task_id: TASK-1\n---\n",
        )
        .unwrap();
        let real_cli_task = parse_task_file(&real_cli_path, BacklogTaskSource::Active)
            .unwrap()
            .0;
        assert_eq!(real_cli_task.parent.as_deref(), Some("TASK-1"));

        let old_fixture_path = dir.path().join("task-3 - Old fixture.md");
        fs::write(
            &old_fixture_path,
            "---\nid: TASK-3\ntitle: Old fixture\nparent: TASK-1\n---\n",
        )
        .unwrap();
        let old_fixture_task = parse_task_file(&old_fixture_path, BacklogTaskSource::Active)
            .unwrap()
            .0;
        assert_eq!(old_fixture_task.parent.as_deref(), Some("TASK-1"));
    }

    #[test]
    fn detects_backlog_project() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("backlog/tasks")).unwrap();

        assert!(is_backlog_repo(dir.path()));
    }

    #[test]
    fn sorts_task_id_decimals_numerically() {
        let mut ids = ["TASK-150.10", "TASK-2", "TASK-150.2"];
        ids.sort_by_key(|id| task_id_key(id));

        assert_eq!(ids, ["TASK-2", "TASK-150.2", "TASK-150.10"]);
    }
}
