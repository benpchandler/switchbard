//! The write layer's byte-preservation gate, run over every real task file
//! in this repository (active, completed, drafts, archived).
//!
//! Unit tests in `backlog::write` pin each operation against curated
//! fixtures; this suite pins the layer against the *population* — every
//! shape the `backlog` CLI has actually written here over the project's
//! life. The two promises under test:
//!
//! 1. **A no-op edit writes nothing**: setting a task's status to the value
//!    it already has leaves the file byte-identical. (This transitively
//!    proves the raw split/rejoin is byte-lossless: the comparison happens
//!    on the rebuilt text.)
//! 2. **A real edit is surgical**: setting a new status changes only the
//!    `status:` line and the `updated_date:` line, and reparsing shows every
//!    other field untouched.

use std::fs;
use std::path::{Path, PathBuf};
use switchbard_core::{load_backlog_repo, set_task_status, BacklogTask, WriteOutcome};

const TASK_DIRS: [&str; 4] = [
    "backlog/tasks",
    "backlog/completed",
    "backlog/drafts",
    "backlog/archive/tasks",
];

/// At least this many real files must participate, so the gate can never
/// silently pass because a path moved and nothing was found.
const MIN_FILES: usize = 30;

fn real_task_files() -> Vec<PathBuf> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for dir in TASK_DIRS {
        let Ok(entries) = fs::read_dir(repo_root.join(dir)) else {
            continue;
        };
        files.extend(
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "md")),
        );
    }
    files.sort();
    assert!(
        files.len() >= MIN_FILES,
        "expected at least {MIN_FILES} real task files, found {} — did the backlog move?",
        files.len()
    );
    files
}

/// Copy one real task file into a minimal Backlog project in `dir` and parse
/// it back through the crate's own reader.
fn staged_copy(dir: &Path, source: &Path) -> (PathBuf, BacklogTask) {
    let tasks_dir = dir.join("backlog/tasks");
    fs::create_dir_all(&tasks_dir).expect("tempdir project layout");
    let staged = tasks_dir.join(source.file_name().expect("task files have names"));
    fs::copy(source, &staged).expect("staging a real task file");
    (staged, parse_single(dir))
}

fn parse_single(project_root: &Path) -> BacklogTask {
    let project = load_backlog_repo(project_root).expect("staged project loads");
    assert_eq!(project.tasks.len(), 1, "one staged file, one task");
    project.tasks.into_iter().next().expect("task exists")
}

/// The raw value of a top-level `status:` line, only when it is written in
/// the plain style this layer itself renders — the shapes where "write the
/// same value back" must be a byte-no-op.
fn plain_status_value(text: &str) -> Option<&str> {
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix("status: "))?
        .trim();
    let plain = !value.is_empty() && !value.starts_with('\'') && !value.starts_with('"');
    plain.then_some(value)
}

#[test]
fn noop_status_edit_is_byte_identical_across_every_real_task_file() {
    let mut covered = 0usize;
    for source in real_task_files() {
        let original = fs::read_to_string(&source).expect("real task file reads");
        let Some(status) = plain_status_value(&original) else {
            continue;
        };
        let dir = tempfile::tempdir().expect("tempdir");
        let (staged, _) = staged_copy(dir.path(), &source);

        let outcome = set_task_status(&staged, status)
            .unwrap_or_else(|e| panic!("{}: {e}", source.display()));

        assert_eq!(outcome, WriteOutcome::Unchanged, "{}", source.display());
        assert_eq!(
            fs::read_to_string(&staged).expect("staged file reads"),
            original,
            "{}: a no-op must leave every byte alone",
            source.display()
        );
        covered += 1;
    }
    assert!(
        covered >= MIN_FILES,
        "only {covered} files exercised the no-op path — the gate has gone vacuous"
    );
}

#[test]
fn status_edit_touches_only_status_and_updated_date_across_every_real_task_file() {
    const PROBE: &str = "Format Fork Probe";
    for source in real_task_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (staged, before) = staged_copy(dir.path(), &source);
        let original = fs::read_to_string(&staged).expect("staged file reads");

        let outcome =
            set_task_status(&staged, PROBE).unwrap_or_else(|e| panic!("{}: {e}", source.display()));

        assert_eq!(outcome, WriteOutcome::Changed, "{}", source.display());
        let edited = fs::read_to_string(&staged).expect("staged file reads");
        assert_untouched_outside(&original, &edited, &source);
        assert!(
            !staged.with_extension("md.tmp").exists(),
            "{}: the tmp sidecar must not linger",
            source.display()
        );

        let after = parse_single(dir.path());
        assert_eq!(after.status, PROBE, "{}", source.display());
        assert_fields_untouched(&before, &after, &source);
    }
}

/// Every line except `status:` and `updated_date:` must survive verbatim, in
/// order.
fn assert_untouched_outside(before: &str, after: &str, source: &Path) {
    let filtered = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| !l.starts_with("status:") && !l.starts_with("updated_date:"))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        filtered(before),
        filtered(after),
        "{}: the edit leaked outside the status/updated_date lines",
        source.display()
    );
}

fn assert_fields_untouched(before: &BacklogTask, after: &BacklogTask, source: &Path) {
    let name = source.display();
    assert_eq!(before.id, after.id, "{name}");
    assert_eq!(before.title, after.title, "{name}");
    assert_eq!(before.priority, after.priority, "{name}");
    assert_eq!(before.assignees, after.assignees, "{name}");
    assert_eq!(before.labels, after.labels, "{name}");
    assert_eq!(before.dependencies, after.dependencies, "{name}");
    assert_eq!(before.references, after.references, "{name}");
    assert_eq!(before.project, after.project, "{name}");
    assert_eq!(before.parent, after.parent, "{name}");
    assert_eq!(before.created_date, after.created_date, "{name}");
    assert_eq!(before.description, after.description, "{name}");
    assert_eq!(
        before.implementation_plan, after.implementation_plan,
        "{name}"
    );
    assert_eq!(
        before.implementation_notes, after.implementation_notes,
        "{name}"
    );
    assert_eq!(before.final_summary, after.final_summary, "{name}");
    assert_eq!(
        before.acceptance_criteria, after.acceptance_criteria,
        "{name}"
    );
    assert_eq!(
        before.definition_of_done, after.definition_of_done,
        "{name}"
    );
}
