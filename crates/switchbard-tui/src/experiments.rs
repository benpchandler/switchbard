//! Per-user experiment decisions, independent of task completion and repository state.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_FILE_BYTES: usize = 128 * 1024;

pub struct ExperimentSpec {
    /// Permanent display number; never renumber or reuse after retirement.
    pub number: u16,
    pub id: &'static str,
    pub task: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub page: crate::page::Page,
}

const CATALOG: &[ExperimentSpec] = &[
    ExperimentSpec {
        number: 1,
        page: crate::page::Page::Tasks,
        id: "task-edit-shortcut",
        task: "TASK-251",
        title: "Open the selected task directly in the editor with t e",
        description: "Select a task. Press t, then e; use arrows to highlight its title. Enter should open a text box; Esc cancels.",
    },
    ExperimentSpec {
        number: 2,
        page: crate::page::Page::Tasks,
        id: "detail-metadata-first",
        task: "TASK-254",
        title: "Move assignees and created/updated dates near the top of task details",
        description: "Select a task and press Enter to open details. Assignees and dates should be below Properties; Off moves them to the bottom.",
    },
    ExperimentSpec {
        number: 3,
        page: crate::page::Page::Tasks,
        id: "detail-compact-done",
        task: "TASK-253",
        title: "Hide empty Definition of Done and move existing requirements beside acceptance criteria",
        description: "In switchbard, filter to id:TASK-36 and open its details. On hides the empty Definition of Done heading; Off restores it.",
    },
    ExperimentSpec {
        number: 4,
        page: crate::page::Page::Tasks,
        id: "bug-dispatch",
        task: "TASK-143",
        title: "Send a filed bug straight to Codex and answer it in Inbox",
        description: "With it on, :bug files the report and starts a Codex run in its own worktree; watch Inbox for its question. Off, :bug only files, like :idea.",
    },
    ExperimentSpec {
        number: 5,
        page: crate::page::Page::Tasks,
        id: "glyph-column-name",
        task: "TASK-286",
        title: "Name a glyph column in its header instead of stacking every glyph there",
        description: "Look at a glyph column's header: it reads the column's name. Off, it reads as every glyph in the column run together, which is unreadable past about three values.",
    },
];

// Reject accidental duplicate numbers at compile time, before they can be displayed.
const _: () = {
    let mut index = 0;
    while index < CATALOG.len() {
        assert!(CATALOG[index].number > 0, "experiment numbers start at 1");
        let mut other = index + 1;
        while other < CATALOG.len() {
            assert!(
                CATALOG[index].number != CATALOG[other].number,
                "duplicate experiment number"
            );
            other += 1;
        }
        index += 1;
    }
};

pub fn catalog() -> &'static [ExperimentSpec] {
    CATALOG
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentReview {
    #[default]
    Unreviewed,
    Kept,
    RemovalRequested,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExperimentState {
    pub enabled: bool,
    pub review: ExperimentReview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExperimentDecision {
    Enable,
    Disable,
    Keep,
    Remove,
}

impl ExperimentState {
    fn apply(self, decision: ExperimentDecision) -> Self {
        match decision {
            ExperimentDecision::Enable => Self {
                enabled: true,
                review: match self.review {
                    ExperimentReview::RemovalRequested => ExperimentReview::Unreviewed,
                    review => review,
                },
            },
            ExperimentDecision::Disable => Self {
                enabled: false,
                ..self
            },
            ExperimentDecision::Keep => Self {
                enabled: true,
                review: ExperimentReview::Kept,
            },
            ExperimentDecision::Remove => Self {
                enabled: false,
                review: ExperimentReview::RemovalRequested,
            },
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct Decisions {
    version: u32,
    experiments: BTreeMap<String, Value>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl Default for Decisions {
    fn default() -> Self {
        Self {
            version: 1,
            experiments: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }
}

pub struct ExperimentStore {
    path: Option<PathBuf>,
    source: Option<Vec<u8>>,
    decisions: Decisions,
    blocked: Option<String>,
}

impl ExperimentStore {
    /// Load `SWITCHBARD_EXPERIMENTS_FILE`, or `~/.switchbard/experiments.json`.
    /// A missing file starts disabled. An unreadable file remains untouched and read-only.
    pub fn load() -> (Self, Option<String>) {
        match configured_path() {
            Ok(path) => Self::load_from(Some(path)),
            Err(error) => {
                let (mut store, _) = Self::load_from(None);
                let warning = format!("Experiments disabled: {error:#}.");
                store.blocked = Some(warning.clone());
                (store, Some(warning))
            }
        }
    }

    /// `None` creates disabled embedded state with no user I/O; mutations are refused.
    pub fn load_from(path: Option<PathBuf>) -> (Self, Option<String>) {
        let mut store = Self {
            path,
            source: None,
            decisions: Decisions::default(),
            blocked: None,
        };
        if store.path.is_none() {
            return (store, None);
        }
        if let Err(error) = store.read_source() {
            store.blocked = Some(format!(
                "Experiments disabled: {error:#}. Preserve the file, repair it, then reopen Experiments."
            ));
        }
        let warning = store.blocked.clone();
        (store, warning)
    }

    /// Refresh this store's selected file without consulting the environment again.
    pub fn reload(&mut self) -> Option<String> {
        if self.path.is_none() {
            return self.blocked.clone();
        }
        let (store, warning) = Self::load_from(self.path.clone());
        *self = store;
        warning
    }

    pub fn state(&self, id: &str) -> ExperimentState {
        if !catalog().iter().any(|spec| spec.id == id) {
            return ExperimentState::default();
        }
        self.decisions
            .experiments
            .get(id)
            .map_or_else(ExperimentState::default, |value| {
                serde_json::from_value(value.clone())
                    .expect("known experiment states were validated")
            })
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.state(id).enabled
    }

    pub fn unreviewed_count(&self) -> usize {
        catalog()
            .iter()
            .filter(|spec| self.state(spec.id).review == ExperimentReview::Unreviewed)
            .count()
    }

    /// Commit to disk before changing effective behavior. Disabling preserves acceptance.
    pub fn set_decision(&mut self, id: &str, decision: ExperimentDecision) -> Result<()> {
        if let Some(reason) = &self.blocked {
            bail!("{reason}");
        }
        ensure!(
            catalog().iter().any(|spec| spec.id == id),
            "Unknown experiment: {id}"
        );
        let path = self
            .path
            .as_deref()
            .context("Experiment decisions unavailable in this embedded session")?;
        let _lock = lock_decisions(path)?;
        self.check_source(path)?;
        let mut next = self.decisions.clone();
        let state = self.state(id).apply(decision);
        let value = next
            .experiments
            .entry(id.to_owned())
            .or_insert_with(|| Value::Object(Default::default()));
        let fields = value
            .as_object_mut()
            .expect("known experiment state is an object");
        fields.insert("enabled".into(), Value::Bool(state.enabled));
        fields.insert("review".into(), serde_json::to_value(state.review)?);
        let bytes = serde_json::to_vec_pretty(&next)?;
        ensure!(
            bytes.len() <= MAX_FILE_BYTES,
            "Experiment decisions exceed 128 KiB"
        );
        self.write_atomic(path, &bytes)?;
        self.decisions = next;
        self.source = Some(bytes);
        Ok(())
    }

    fn read_source(&mut self) -> Result<()> {
        let path = self.path.as_deref().context("Home directory unavailable")?;
        let source = read_bounded(path)?;
        let decisions = match &source {
            Some(bytes) => serde_json::from_slice::<Decisions>(bytes)
                .context("Invalid experiment decisions")?,
            None => Decisions::default(),
        };
        ensure!(
            decisions.version == 1,
            "Unsupported experiment decisions version {}",
            decisions.version
        );
        for spec in catalog() {
            if let Some(value) = decisions.experiments.get(spec.id) {
                ensure!(
                    value.is_object(),
                    "Decision for {} must be an object",
                    spec.id
                );
                let state: ExperimentState = serde_json::from_value(value.clone())
                    .with_context(|| format!("Invalid decision for {}", spec.id))?;
                ensure!(
                    !state.enabled || state.review != ExperimentReview::RemovalRequested,
                    "Removed experiment {} cannot be enabled",
                    spec.id
                );
            }
        }
        self.decisions = decisions;
        self.source = source;
        Ok(())
    }

    fn check_source(&self, path: &Path) -> Result<()> {
        ensure!(
            read_bounded(path)? == self.source,
            "Experiment decisions changed in another window. Reopen Experiments and try again."
        );
        Ok(())
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let temporary = temporary_path(path);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .context("Create temporary experiment decisions")?;
        let result = (|| {
            file.write_all(bytes)
                .context("Write experiment decisions")?;
            file.sync_all().context("Flush experiment decisions")?;
            self.check_source(path)?;
            fs::rename(&temporary, path).context("Replace experiment decisions")
        })();
        if result.is_err() {
            fs::remove_file(&temporary).with_context(|| {
                format!(
                    "Remove temporary file {}; original failure: {result:?}",
                    temporary.display()
                )
            })?;
        }
        result
    }
}

fn configured_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SWITCHBARD_EXPERIMENTS_FILE") {
        ensure!(
            !path.is_empty(),
            "SWITCHBARD_EXPERIMENTS_FILE must not be empty"
        );
        return Ok(PathBuf::from(path));
    }
    dirs::home_dir()
        .map(|home| home.join(".switchbard/experiments.json"))
        .context("Home directory unavailable")
}

fn read_bounded(path: &Path) -> Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("Read experiment decisions metadata"),
    };
    ensure!(
        metadata.is_file(),
        "Experiment decisions must be a regular file, not a symlink"
    );
    ensure!(
        metadata.len() <= MAX_FILE_BYTES as u64,
        "Experiment decisions exceed 128 KiB"
    );
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .context("Open experiment decisions")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_FILE_BYTES as u64,
        "Experiment decisions changed type or exceed 128 KiB"
    );
    let mut bytes = Vec::new();
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_FILE_BYTES,
        "Experiment decisions exceed 128 KiB"
    );
    Ok(Some(bytes))
}

fn lock_decisions(path: &Path) -> Result<File> {
    fs::create_dir_all(
        path.parent()
            .context("Experiment decisions have no parent directory")?,
    )?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path.with_extension("lock"))
        .context("Open experiment decisions lock")?;
    ensure!(
        file.metadata()?.is_file(),
        "Experiment decisions lock must be a regular file"
    );
    file.try_lock()
        .context("Experiment decisions are busy. Try again.")?;
    Ok(file)
}

fn temporary_path(path: &Path) -> PathBuf {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.with_extension(format!("{}.{}.{}.tmp", std::process::id(), stamp, sequence))
}
