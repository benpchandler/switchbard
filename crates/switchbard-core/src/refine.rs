//! Refine: an AI-assisted grooming pass over **one** Backlog task, upstream
//! of `crate::dispatch`. A half-baked card handed to the dispatch pipeline
//! produces a weak agent run; this module is the step that fills the card in
//! first — an enriched description, acceptance criteria, and an
//! implementation plan, grounded in the repo the task actually belongs to.
//!
//! Same shape as its sibling `crate::dispatch`: build a prompt from the
//! task's own content, run a headless `claude -p`, and write every result
//! back through the `backlog` CLI. The repo stays the system of record.
//!
//! ## Three deliberate differences from `dispatch`
//!
//! 1. **No worktree.** The refine run explores `repo_root` itself. It writes
//!    no code, so there is nothing to isolate and nothing to branch.
//! 2. **A read-only permission posture.** `dispatch` runs under
//!    `--permission-mode acceptEdits` because it *must* write files. Refine
//!    must not, so it runs under `--permission-mode plan`, which is Claude
//!    Code's supported "explore and propose, don't mutate" mode, plus
//!    `--allowedTools Read,Grep,Glob` so the exploration this prompt asks for
//!    never stalls on a permission prompt nobody is at the keyboard to
//!    answer. Two properties, two flags: plan mode is the *denial* of edits,
//!    the allowlist is the *pre-approval* of reads. `--dangerously-skip-
//!    permissions` is deliberately not used — the whole point of this run is
//!    that it cannot touch the repo.
//! 3. **No label state machine.** `dispatch`'s `dispatch`/`dispatching`/…
//!    labels exist to stop a long, expensive, PR-opening pipeline from
//!    running twice. Refine is a single bounded call whose only effect is one
//!    additive `backlog task edit`; a second one would at worst append
//!    duplicate-ish text, and [`build_refine_patch`]'s dedupe already
//!    swallows the exact-duplicate case. The "don't stack runs per task"
//!    guard is therefore GUI-side and in-memory (see
//!    `HiveApp::spawn_backlog_refine`), not a label written to the repo.
//!
//! ## The additive-apply contract (the load-bearing part)
//!
//! Human-authored content is never destroyed. Concretely:
//!
//! - **Description.** The original text survives *verbatim, as a contiguous
//!   prefix* of whatever is written back. The model's text is appended after
//!   it under a [`REFINED_MARKER`] line. [`build_refine_patch`] re-checks that
//!   prefix property on the string it just built and returns `Err` — applying
//!   nothing — rather than emitting a patch that fails it.
//! - **Acceptance criteria.** Existing criteria keep their text *and* their
//!   checked state, because the patch only ever *appends* (`backlog task edit
//!   --ac`, via [`BacklogTaskPatch::append_acceptance_criteria`]). Nothing
//!   here replaces, reorders, checks, or unchecks a criterion. Model criteria
//!   that duplicate an existing one under [`normalize_criterion`] are dropped.
//! - **Implementation plan.** Empty plan → filled. Non-empty plan → the
//!   original survives verbatim as a prefix, same rule as the description.
//! - **Malformed or partial model output applies nothing.** Parsing is strict
//!   (see [`parse_refine_response`]) and happens entirely before the single
//!   `edit_backlog_task` call, so there is no partially-applied state to
//!   unwind.
//!
//! ## Why the appended block is not a `##` heading
//!
//! `backlog::parse::extract_section` ends a section at the next `## ` line.
//! A `## Refined` heading inside the description would therefore make the
//! appended text vanish from `BacklogTask::description` on the next load —
//! and a second refine would then append to a *truncated* original, quietly
//! breaking the verbatim-prefix guarantee. So the separator is a bold line,
//! not a heading, and [`demote_section_headings`] pushes any `## ` the model
//! itself wrote down to `### ` for the same reason.

use crate::backlog::{edit_backlog_task, BacklogTask, BacklogTaskPatch};
use crate::dispatch::{dispatch_log_dir, shell_quote, unix_now};
use crate::kill::kill_pgid;
use crate::spawn::{spawn_in_session, wait_for_exit, WaitOutcome};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Separator between a human's original prose and an appended refine pass.
/// A bold line rather than a `## ` heading — see the module doc.
pub const REFINED_MARKER: &str = "**Refined by Switchbard**";

/// Default ceiling on one refine run. Generous enough for a real repo
/// exploration (the run reads files and greps before answering), short
/// enough that a wedged run frees the task's button within a coffee break.
pub const DEFAULT_REFINE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

const KILL_GRACE: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct RefineOptions {
    /// The `claude` binary to invoke — a bare name resolves via `$PATH`.
    pub claude_binary: String,
    /// How long to let one refine run go before killing it as stuck.
    pub timeout: Duration,
}

impl Default for RefineOptions {
    fn default() -> Self {
        Self {
            claude_binary: "claude".to_string(),
            timeout: DEFAULT_REFINE_TIMEOUT,
        }
    }
}

/// The contracted JSON the refine run must emit. All three fields are
/// **required** — see [`parse_refine_response`] for why a missing one is
/// treated as a failed run rather than an empty suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RefineSuggestion {
    pub description: String,
    pub acceptance_criteria: Vec<String>,
    pub implementation_plan: String,
}

/// A ready-to-apply patch plus what it will visibly change — the counts are
/// what the GUI's status line reports, and what the unit tests assert on
/// instead of re-deriving them from the patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefinePlan {
    pub patch: BacklogTaskPatch,
    pub description_extended: bool,
    pub criteria_added: usize,
    pub plan_extended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefineResult {
    Applied {
        description_extended: bool,
        criteria_added: usize,
        plan_extended: bool,
    },
    /// The run's output did not satisfy the JSON contract. Nothing was
    /// written — parsing completes before any CLI call.
    Unparseable { message: String },
    /// Parsed fine, but every suggestion was blank or already on the task.
    NothingToApply,
    /// `None` means the run was killed for exceeding `opts.timeout`.
    ClaudeFailed { exit_code: Option<i32> },
    /// The `backlog task edit` call itself failed.
    EditFailed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefineOutcome {
    pub task_id: String,
    pub log_path: PathBuf,
    pub result: RefineResult,
}

/// Filename stem shared by a refine run's log (`<stem>.log`) and prompt
/// (`<stem>-prompt.md`). Lives in the same `switchbard-logs` directory as
/// dispatch runs ([`dispatch_log_dir`]) — one place a user has to look for
/// "what did Switchbard spawn" — distinguished by the `refine-` prefix.
pub fn refine_log_stem(task_id: &str, started_at_unix: u64) -> String {
    format!(
        "refine-{}-{}",
        task_id.to_ascii_lowercase(),
        started_at_unix
    )
}

/// The prompt handed to the headless refine run: the task's current content
/// verbatim, the grounding instruction, and the output contract. Pure, so
/// wording changes are unit-testable without spawning a process.
pub fn build_refine_prompt(task: &BacklogTask) -> String {
    let mut prompt = format!(
        "You are refining Backlog task {} — \"{}\" — so that it is ready to hand \
         to an implementation agent. You are running at the root of the repository \
         this task belongs to.\n\n\
         Explore the repository READ-ONLY (Read/Grep/Glob) before answering. Do not \
         edit, create, or delete any file, and do not run any command that changes \
         state. Ground every suggestion in what is actually in this repo — name real \
         files, real modules, real commands from its own docs and config — rather \
         than generic advice.\n\n",
        task.id, task.title
    );

    prompt.push_str("## Current description\n\n");
    prompt.push_str(blank_as_none(&task.description));
    prompt.push_str("\n\n## Current acceptance criteria\n\n");
    if task.acceptance_criteria.is_empty() {
        prompt.push_str("(none)");
    } else {
        for item in &task.acceptance_criteria {
            let mark = if item.checked { "x" } else { " " };
            prompt.push_str(&format!("- [{mark}] #{} {}\n", item.index, item.text));
        }
        prompt.pop();
    }
    prompt.push_str("\n\n## Current implementation plan\n\n");
    prompt.push_str(blank_as_none(&task.implementation_plan));

    prompt.push_str(
        "\n\n## What to produce\n\n\
         - `description`: what the existing description is MISSING — context, the \
           concrete surfaces involved, constraints, and what is explicitly out of \
           scope. It is appended after the existing description, which is kept \
           verbatim, so do not restate what is already there.\n\
         - `acceptance_criteria`: criteria that are NOT already listed above, each \
           one independently checkable by a reviewer. Do not repeat or reword an \
           existing criterion.\n\
         - `implementation_plan`: ordered, concrete steps naming the real files and \
           commands involved, ending with how the change is verified.\n\n\
         ## Output contract\n\n\
         Your final message must be EXACTLY one JSON object and nothing else — no \
         prose before or after it:\n\n\
         {\n\
         \x20 \"description\": \"markdown string\",\n\
         \x20 \"acceptance_criteria\": [\"string\", \"string\"],\n\
         \x20 \"implementation_plan\": \"markdown string\"\n\
         }\n\n\
         All three keys are REQUIRED. Use an empty string or an empty array for a \
         section you have nothing to add to; omitting a key makes the whole run \
         fail and nothing is applied to the task. Do not use `##` headings inside \
         these strings.\n",
    );
    prompt
}

fn blank_as_none(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        "(empty)"
    } else {
        trimmed
    }
}

/// Pull the contracted JSON object out of a refine run's captured output.
///
/// Defensive by construction, because the input is a log file holding the
/// run's stdout *and* stderr: a bare object, a ```json fence, and an object
/// preceded or followed by stray lines all parse. Strict about the contract
/// itself, because a half-understood suggestion must not be half-applied:
/// a truncated object, a wrong-typed field, a missing key, or an
/// entirely-blank suggestion each fail here — before any CLI call — which is
/// what makes "malformed output applies nothing" true by construction rather
/// than by cleanup.
pub fn parse_refine_response(raw: &str) -> Result<RefineSuggestion> {
    let candidate = extract_json_object(raw)
        .ok_or_else(|| anyhow::anyhow!("no JSON object found in the refine run's output"))?;
    let suggestion: RefineSuggestion = serde_json::from_str(&candidate)
        .context("refine output did not match the JSON contract")?;
    if suggestion.description.trim().is_empty()
        && suggestion.implementation_plan.trim().is_empty()
        && suggestion
            .acceptance_criteria
            .iter()
            .all(|c| c.trim().is_empty())
    {
        bail!("refine output carried no description, criteria, or plan");
    }
    Ok(suggestion)
}

/// The outermost `{ … }` span of `raw`, with a surrounding markdown fence
/// stripped first. Widest-span rather than brace-matching on purpose: the
/// contract is one object, so the first `{` and the last `}` bound it even
/// when it contains nested objects, and anything looser would need a real
/// parser to beat `serde_json`'s own error message.
fn extract_json_object(raw: &str) -> Option<String> {
    let unfenced = strip_code_fence(raw.trim());
    let start = unfenced.find('{')?;
    let end = unfenced.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(unfenced[start..=end].to_string())
}

fn strip_code_fence(text: &str) -> &str {
    let Some(open) = text.find("```") else {
        return text;
    };
    let after_open = &text[open + 3..];
    // Skip the optional language tag on the opening fence line.
    let body_start = after_open.find('\n').map(|i| i + 1).unwrap_or(0);
    let body = &after_open[body_start..];
    match body.find("```") {
        Some(close) => &body[..close],
        None => body,
    }
}

/// Turn a parsed suggestion into the single additive `BacklogTaskPatch` that
/// applies it. Pure — no CLI, no disk — so the whole merge contract in the
/// module doc is unit-testable.
///
/// Returns `Err` only if the merged text would fail the verbatim-prefix
/// guarantee, which by construction it cannot; the check is kept as an
/// always-on internal-invariant assertion because that guarantee is the
/// entire reason a user can safely press this button on a card they wrote by
/// hand.
pub fn build_refine_patch(task: &BacklogTask, suggestion: &RefineSuggestion) -> Result<RefinePlan> {
    let description = merge_prose(&task.description, &suggestion.description)?;
    let implementation_plan =
        merge_prose(&task.implementation_plan, &suggestion.implementation_plan)?;
    let criteria = new_criteria(task, &suggestion.acceptance_criteria);

    let plan = RefinePlan {
        description_extended: description.is_some(),
        criteria_added: criteria.len(),
        plan_extended: implementation_plan.is_some(),
        patch: BacklogTaskPatch {
            description,
            implementation_plan,
            append_acceptance_criteria: criteria,
            ..Default::default()
        },
    };
    debug_assert_eq!(
        plan.patch.is_empty(),
        !plan.description_extended && !plan.plan_extended && plan.criteria_added == 0,
        "a plan that reports a change must produce a non-empty patch"
    );
    Ok(plan)
}

/// `None` when there is nothing to add (blank suggestion, or already
/// present); otherwise the merged text, which always starts with `original`
/// verbatim when `original` is non-empty.
fn merge_prose(original: &str, addition: &str) -> Result<Option<String>> {
    let addition = demote_section_headings(addition.trim());
    if addition.is_empty() {
        return Ok(None);
    }
    let original = original.trim();
    if original.is_empty() {
        return Ok(Some(addition));
    }
    if original.contains(&addition) {
        return Ok(None);
    }
    let merged = format!("{original}\n\n{REFINED_MARKER}\n\n{addition}");
    if !merged.starts_with(original) {
        bail!("refine merge would not preserve the original text verbatim");
    }
    Ok(Some(merged))
}

/// Push any `## ` heading the model wrote down one level. See the module
/// doc: a `## ` line inside a section is what `backlog::parse::
/// extract_section` treats as the *end* of that section, so leaving one in
/// place would silently truncate the task on its next load.
fn demote_section_headings(text: &str) -> String {
    text.lines()
        .map(|line| {
            let indent_len = line.len() - line.trim_start().len();
            if line.trim_start().starts_with("## ") {
                format!("{}#{}", &line[..indent_len], line.trim_start())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Model criteria that are neither blank, nor a normalized duplicate of one
/// the task already carries, nor a duplicate of an earlier entry in the
/// model's own list. Existing criteria are never touched — this is the only
/// thing that reaches the patch.
fn new_criteria(task: &BacklogTask, suggested: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = task
        .acceptance_criteria
        .iter()
        .map(|item| normalize_criterion(&item.text))
        .collect();
    let mut out = Vec::new();
    for criterion in suggested {
        let text = criterion.trim();
        if text.is_empty() {
            continue;
        }
        let normalized = normalize_criterion(text);
        if normalized.is_empty() || seen.contains(&normalized) {
            continue;
        }
        seen.push(normalized);
        out.push(text.to_string());
    }
    out
}

/// Dedupe key for an acceptance criterion: case-, whitespace-, marker-, and
/// trailing-punctuation-insensitive. A model asked not to restate existing
/// criteria still tends to return one back with a capital letter, a period,
/// or the `#3 ` index prefix it saw in the prompt; those are the same
/// criterion, not a new one.
pub fn normalize_criterion(text: &str) -> String {
    let stripped = strip_index_marker(text.trim());
    stripped
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', ';', ':', '!'])
        .to_lowercase()
}

fn strip_index_marker(text: &str) -> &str {
    let Some(rest) = text.strip_prefix('#') else {
        return text;
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return text;
    }
    rest[digits..].trim_start()
}

/// Run one refine pass for `task` against `repo_root` and apply the result.
/// Blocking, like `dispatch::dispatch_one` — safe to call from a background
/// thread the same way the GUI's other core calls are.
///
/// Returns `Err` only for setup failures (log dir, prompt file, spawn); every
/// other outcome, including a model that answered nonsense, reports through
/// [`RefineOutcome::result`] so a caller can put it straight on a status line.
pub fn refine_task(
    repo_root: &Path,
    task: &BacklogTask,
    opts: &RefineOptions,
) -> Result<RefineOutcome> {
    let log_dir = dispatch_log_dir();
    std::fs::create_dir_all(&log_dir).context("failed to create switchbard-logs dir")?;
    let stem = refine_log_stem(&task.id, unix_now());
    let log_path = log_dir.join(format!("{stem}.log"));
    let prompt_path = log_dir.join(format!("{stem}-prompt.md"));
    std::fs::write(&prompt_path, build_refine_prompt(task))
        .context("failed writing refine prompt")?;

    let exit = run_claude_read_only(repo_root, &prompt_path, &log_path, opts)?;
    let result = match exit {
        Some(0) => {
            let raw = std::fs::read_to_string(&log_path).unwrap_or_default();
            apply_refine(repo_root, task, &raw)
        }
        other => RefineResult::ClaudeFailed { exit_code: other },
    };

    Ok(RefineOutcome {
        task_id: task.id.clone(),
        log_path,
        result,
    })
}

/// Spawn the headless read-only run and block until it exits or the timeout
/// elapses. `Ok(None)` means it was killed for running too long. See the
/// module doc for why these are the flags.
fn run_claude_read_only(
    repo_root: &Path,
    prompt_path: &Path,
    log_path: &Path,
    opts: &RefineOptions,
) -> Result<Option<i32>> {
    let command = format!(
        "cat {} | {} -p --permission-mode plan --allowedTools Read,Grep,Glob --output-format text",
        shell_quote(prompt_path),
        opts.claude_binary,
    );
    let run = spawn_in_session(&command, repo_root, log_path).context("failed to spawn claude")?;
    match wait_for_exit(run.pid, opts.timeout).context("failed waiting on claude")? {
        WaitOutcome::Exited(code) => Ok(Some(code)),
        WaitOutcome::TimedOut => {
            let _ = kill_pgid(run.pgid, KILL_GRACE);
            let _ = wait_for_exit(run.pid, KILL_GRACE);
            Ok(None)
        }
    }
}

/// Parse → merge → one `edit_backlog_task`. Every rejection path returns
/// before the CLI call, which is what makes the "malformed output applies
/// nothing" guarantee structural rather than a matter of cleanup.
fn apply_refine(repo_root: &Path, task: &BacklogTask, raw: &str) -> RefineResult {
    let suggestion = match parse_refine_response(raw) {
        Ok(suggestion) => suggestion,
        Err(e) => {
            return RefineResult::Unparseable {
                message: e.to_string(),
            }
        }
    };
    let plan = match build_refine_patch(task, &suggestion) {
        Ok(plan) => plan,
        Err(e) => {
            return RefineResult::Unparseable {
                message: e.to_string(),
            }
        }
    };
    if plan.patch.is_empty() {
        return RefineResult::NothingToApply;
    }
    match edit_backlog_task(repo_root, &task.id, &plan.patch) {
        Ok(_) => RefineResult::Applied {
            description_extended: plan.description_extended,
            criteria_added: plan.criteria_added,
            plan_extended: plan.plan_extended,
        },
        Err(e) => RefineResult::EditFailed {
            message: e.to_string(),
        },
    }
}

/// One-line, human-readable summary — the GUI's status line renders this
/// verbatim, so every variant must say what happened *and* whether anything
/// changed.
pub fn describe_refine_result(task_id: &str, result: &RefineResult) -> String {
    match result {
        RefineResult::Applied {
            description_extended,
            criteria_added,
            plan_extended,
        } => {
            let mut parts = Vec::new();
            if *description_extended {
                parts.push("description".to_string());
            }
            if *criteria_added > 0 {
                parts.push(format!("{criteria_added} acceptance criteria"));
            }
            if *plan_extended {
                parts.push("implementation plan".to_string());
            }
            format!("refined {task_id}: updated {}", parts.join(", "))
        }
        RefineResult::NothingToApply => {
            format!("refine {task_id}: nothing new to add")
        }
        RefineResult::Unparseable { message } => {
            format!("refine {task_id} failed, nothing applied: {message}")
        }
        RefineResult::ClaudeFailed { exit_code: None } => {
            format!("refine {task_id} timed out, nothing applied")
        }
        RefineResult::ClaudeFailed {
            exit_code: Some(code),
        } => format!("refine {task_id} failed, nothing applied: claude exited with {code}"),
        RefineResult::EditFailed { message } => {
            format!("refine {task_id}: backlog edit failed: {message}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backlog::{BacklogChecklistItem, BacklogTaskSource};

    fn task() -> BacklogTask {
        BacklogTask {
            id: "TASK-44".to_string(),
            title: "Refine task".to_string(),
            status: "To Do".to_string(),
            priority: "high".to_string(),
            assignees: vec![],
            labels: vec![],
            dependencies: vec![],
            references: vec![],
            milestone: None,
            parent: None,
            created_date: None,
            updated_date: None,
            description: "The card is half-baked.".to_string(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: vec![],
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: PathBuf::from("/repo/backlog/tasks/task-44.md"),
        }
    }

    fn criterion(index: usize, checked: bool, text: &str) -> BacklogChecklistItem {
        BacklogChecklistItem {
            index,
            checked,
            text: text.to_string(),
        }
    }

    fn suggestion(description: &str, criteria: &[&str], plan: &str) -> RefineSuggestion {
        RefineSuggestion {
            description: description.to_string(),
            acceptance_criteria: criteria.iter().map(|c| c.to_string()).collect(),
            implementation_plan: plan.to_string(),
        }
    }

    // ---- prompt builder ----

    #[test]
    fn build_refine_prompt_includes_the_tasks_current_content_and_the_json_contract() {
        let mut t = task();
        t.acceptance_criteria = vec![criterion(1, true, "Existing criterion")];
        t.implementation_plan = "Step one.".to_string();

        let prompt = build_refine_prompt(&t);

        assert!(prompt.contains("TASK-44"));
        assert!(prompt.contains("The card is half-baked."));
        assert!(prompt.contains("- [x] #1 Existing criterion"));
        assert!(prompt.contains("Step one."));
        assert!(prompt.contains("\"acceptance_criteria\""));
        assert!(prompt.contains("All three keys are REQUIRED"));
    }

    #[test]
    fn build_refine_prompt_states_the_read_only_contract() {
        let prompt = build_refine_prompt(&task());

        assert!(prompt.contains("READ-ONLY"));
        assert!(prompt.contains("Do not edit, create, or delete any file"));
    }

    #[test]
    fn build_refine_prompt_marks_empty_sections_rather_than_leaving_them_blank() {
        let mut t = task();
        t.description = String::new();

        let prompt = build_refine_prompt(&t);

        assert!(prompt.contains("(empty)"), "empty description is labeled");
        assert!(prompt.contains("(none)"), "empty criteria list is labeled");
    }

    // ---- parsing ----

    #[test]
    fn parse_refine_response_accepts_a_bare_json_object() {
        let raw = r#"{"description":"More context.","acceptance_criteria":["It works"],"implementation_plan":"Do it."}"#;

        let parsed = parse_refine_response(raw).unwrap();

        assert_eq!(parsed.description, "More context.");
        assert_eq!(parsed.acceptance_criteria, vec!["It works".to_string()]);
        assert_eq!(parsed.implementation_plan, "Do it.");
    }

    /// Pinned from a *real* run of exactly the command
    /// `run_claude_read_only` builds — `claude -p --permission-mode plan
    /// --allowedTools Read,Grep,Glob --output-format text` fed the output
    /// contract this module's prompt ends with (captured 2026-08-19). Guards
    /// two things at once that no synthetic fixture can: that those flags are
    /// accepted rather than rejected, and that the shape they produce is what
    /// the parser expects — a bare object on one line, no fence, no preamble.
    #[test]
    fn parse_refine_response_handles_the_real_shape_a_headless_run_emits() {
        let raw = r#"{"description": "This is a one-sentence placeholder for the task description.", "acceptance_criteria": ["This is a placeholder acceptance criterion.", "This is another placeholder acceptance criterion."], "implementation_plan": "This is a one-sentence placeholder for the implementation plan."}"#;

        let parsed = parse_refine_response(raw).unwrap();

        assert_eq!(parsed.acceptance_criteria.len(), 2);
        assert!(parsed.description.starts_with("This is a one-sentence"));
    }

    #[test]
    fn parse_refine_response_accepts_a_fenced_block_surrounded_by_log_noise() {
        let raw = "some stderr line\n```json\n{\"description\":\"d\",\"acceptance_criteria\":[],\"implementation_plan\":\"\"}\n```\ntrailing chatter\n";

        let parsed = parse_refine_response(raw).unwrap();

        assert_eq!(parsed.description, "d");
    }

    #[test]
    fn parse_refine_response_rejects_truncated_json() {
        let raw = r#"{"description":"d","acceptance_criteria":["a"#;

        let err = parse_refine_response(raw).unwrap_err();

        assert!(
            err.to_string().contains("no JSON object") || err.to_string().contains("JSON contract"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn parse_refine_response_rejects_an_object_missing_a_contract_field() {
        let raw = r#"{"description":"d","acceptance_criteria":[]}"#;

        assert!(parse_refine_response(raw).is_err());
    }

    #[test]
    fn parse_refine_response_rejects_a_wrong_typed_field() {
        let raw =
            r#"{"description":"d","acceptance_criteria":"not a list","implementation_plan":""}"#;

        assert!(parse_refine_response(raw).is_err());
    }

    #[test]
    fn parse_refine_response_rejects_an_entirely_blank_suggestion() {
        let raw = r#"{"description":"  ","acceptance_criteria":["  "],"implementation_plan":""}"#;

        assert!(parse_refine_response(raw).is_err());
    }

    #[test]
    fn parse_refine_response_rejects_output_with_no_object_at_all() {
        assert!(parse_refine_response("I could not complete this task.").is_err());
    }

    // ---- additive merge ----

    #[test]
    fn build_refine_patch_keeps_the_original_description_as_a_verbatim_prefix() {
        let t = task();

        let plan = build_refine_patch(&t, &suggestion("Extra context.", &[], "")).unwrap();

        let merged = plan.patch.description.clone().unwrap();
        assert!(
            merged.starts_with("The card is half-baked."),
            "original must survive verbatim, got {merged:?}"
        );
        assert!(merged.contains(REFINED_MARKER));
        assert!(merged.contains("Extra context."));
        assert!(plan.description_extended);
    }

    #[test]
    fn build_refine_patch_fills_an_empty_description_without_a_marker() {
        let mut t = task();
        t.description = String::new();

        let plan = build_refine_patch(&t, &suggestion("Fresh prose.", &[], "")).unwrap();

        assert_eq!(plan.patch.description.as_deref(), Some("Fresh prose."));
    }

    #[test]
    fn build_refine_patch_demotes_model_headings_so_the_section_cannot_be_truncated() {
        let t = task();

        let plan = build_refine_patch(&t, &suggestion("## Context\n\nDetails.", &[], "")).unwrap();

        let merged = plan.patch.description.unwrap();
        assert!(merged.contains("### Context"), "got {merged:?}");
        assert!(!merged.contains("\n## Context"));
    }

    #[test]
    fn build_refine_patch_appends_criteria_and_never_replaces_existing_ones() {
        let mut t = task();
        t.acceptance_criteria = vec![
            criterion(1, true, "Existing and checked"),
            criterion(2, false, "Existing and open"),
        ];

        let plan = build_refine_patch(
            &t,
            &suggestion("", &["Brand new criterion", "Another new one"], ""),
        )
        .unwrap();

        assert_eq!(
            plan.patch.append_acceptance_criteria,
            vec![
                "Brand new criterion".to_string(),
                "Another new one".to_string()
            ]
        );
        assert_eq!(plan.criteria_added, 2);
        // Nothing in the patch can touch the two existing criteria: the only
        // criteria field an edit carries is the append list.
        assert!(plan.patch.title.is_none());
        assert!(plan.patch.status.is_none());
    }

    #[test]
    fn build_refine_patch_drops_criteria_that_restate_an_existing_one() {
        let mut t = task();
        t.acceptance_criteria = vec![criterion(1, false, "Refine button applies additively")];

        let plan = build_refine_patch(
            &t,
            &suggestion(
                "",
                &[
                    "#1 Refine button applies additively.",
                    "  refine   BUTTON applies additively  ",
                    "A genuinely new one",
                ],
                "",
            ),
        )
        .unwrap();

        assert_eq!(
            plan.patch.append_acceptance_criteria,
            vec!["A genuinely new one".to_string()],
            "normalized duplicates of an existing criterion must be dropped"
        );
    }

    #[test]
    fn build_refine_patch_dedupes_within_the_models_own_list() {
        let t = task();

        let plan =
            build_refine_patch(&t, &suggestion("", &["Same thing", "same thing."], "")).unwrap();

        assert_eq!(plan.patch.append_acceptance_criteria.len(), 1);
    }

    #[test]
    fn build_refine_patch_fills_an_empty_plan_and_appends_to_a_non_empty_one() {
        let empty_plan = build_refine_patch(&task(), &suggestion("", &[], "1. Do it.")).unwrap();
        assert_eq!(
            empty_plan.patch.implementation_plan.as_deref(),
            Some("1. Do it."),
            "an empty plan is filled outright, with no marker"
        );

        let mut t = task();
        t.implementation_plan = "1. Original step.".to_string();
        let appended = build_refine_patch(&t, &suggestion("", &[], "2. New step.")).unwrap();
        let merged = appended.patch.implementation_plan.unwrap();
        assert!(merged.starts_with("1. Original step."));
        assert!(merged.contains("2. New step."));
        assert!(appended.plan_extended);
    }

    #[test]
    fn build_refine_patch_is_idempotent_when_the_text_is_already_present() {
        let mut t = task();
        t.description = format!("Original.\n\n{REFINED_MARKER}\n\nExtra context.");

        let plan = build_refine_patch(&t, &suggestion("Extra context.", &[], "")).unwrap();

        assert!(
            plan.patch.description.is_none(),
            "re-refining must not append the same prose twice"
        );
    }

    #[test]
    fn build_refine_patch_produces_an_empty_patch_when_there_is_nothing_new() {
        let mut t = task();
        t.acceptance_criteria = vec![criterion(1, false, "Already here")];

        let plan = build_refine_patch(
            &t,
            &suggestion("The card is half-baked.", &["Already here"], ""),
        )
        .unwrap();

        assert!(plan.patch.is_empty());
        assert_eq!(plan.criteria_added, 0);
        assert!(!plan.description_extended);
        assert!(!plan.plan_extended);
    }

    #[test]
    fn a_patch_carrying_only_appended_criteria_is_not_considered_empty() {
        let plan = build_refine_patch(&task(), &suggestion("", &["Only this"], "")).unwrap();

        assert!(
            !plan.patch.is_empty(),
            "BacklogTaskPatch::is_empty must account for appended acceptance criteria, \
             or edit_backlog_task short-circuits and silently drops them"
        );
    }

    // ---- result reporting ----

    #[test]
    fn describe_refine_result_names_what_changed_or_that_nothing_did() {
        let applied = describe_refine_result(
            "TASK-44",
            &RefineResult::Applied {
                description_extended: true,
                criteria_added: 3,
                plan_extended: false,
            },
        );
        assert!(applied.contains("description"));
        assert!(applied.contains("3 acceptance criteria"));

        for result in [
            RefineResult::NothingToApply,
            RefineResult::Unparseable {
                message: "boom".to_string(),
            },
            RefineResult::ClaudeFailed { exit_code: None },
            RefineResult::ClaudeFailed { exit_code: Some(2) },
            RefineResult::EditFailed {
                message: "boom".to_string(),
            },
        ] {
            let text = describe_refine_result("TASK-44", &result);
            assert!(text.contains("TASK-44"), "{text}");
        }
        assert!(
            describe_refine_result("TASK-44", &RefineResult::ClaudeFailed { exit_code: None })
                .contains("timed out")
        );
    }

    #[test]
    fn refine_log_stem_is_prefixed_and_lowercased_so_runs_are_greppable_by_kind() {
        assert_eq!(refine_log_stem("TASK-44", 1234), "refine-task-44-1234");
    }
}
