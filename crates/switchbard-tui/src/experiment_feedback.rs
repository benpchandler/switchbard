//! Repository-scoped experiment instructions and human feedback, retained locally.
//!
//! Drafts and submissions have one writer boundary. Unrelated pane changes merge;
//! competing edits to the same draft refuse. Submission and draft clearing are atomic.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_FILE_BYTES: usize = 1024 * 1024;
pub const MAX_DRAFT_CHARS: usize = 4096;
const MAX_EXPERIMENTS: usize = 256;
const MAX_SUBMISSIONS: usize = 2048;

#[derive(Clone, Default, Deserialize, Serialize)]
struct Draft {
    text: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Submission {
    id: String,
    experiment: String,
    number: u16,
    build: String,
    timestamp: String,
    body: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Feedback {
    version: u32,
    active: Option<String>,
    drafts: BTreeMap<String, Draft>,
    submissions: Vec<Submission>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl Default for Feedback {
    fn default() -> Self {
        Self {
            version: 1,
            active: None,
            drafts: BTreeMap::new(),
            submissions: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

pub struct ExperimentFeedbackStore {
    path: Option<PathBuf>,
    feedback: Feedback,
    blocked: Option<String>,
}

enum Mutation {
    Pin(Option<String>),
    Draft { id: String, text: String },
    Submit(Submission),
}

impl ExperimentFeedbackStore {
    pub fn load(repo: &Path) -> (Self, Option<String>) {
        match configured_path(repo) {
            Ok(path) => Self::load_from(Some(path)),
            Err(error) => {
                let (mut store, _) = Self::load_from(None);
                let warning = format!("Experiment feedback unavailable: {error:#}");
                store.blocked = Some(warning.clone());
                (store, Some(warning))
            }
        }
    }

    /// `None` is an embedded session: it performs no user I/O and refuses writes.
    pub fn load_from(path: Option<PathBuf>) -> (Self, Option<String>) {
        let mut store = Self {
            path,
            feedback: Feedback::default(),
            blocked: None,
        };
        let warning = store.reload();
        (store, warning)
    }

    /// Explicitly refresh a stale snapshot. The caller must retain unsaved UI text.
    /// A failed reload preserves the last readable state and disables writes.
    pub fn reload(&mut self) -> Option<String> {
        let Some(path) = self.path.as_deref() else {
            return self.blocked.clone();
        };
        match read_feedback(path) {
            Ok((feedback, _source)) => {
                self.feedback = feedback;
                self.blocked = None;
            }
            Err(error) => {
                self.blocked = Some(format!(
                    "Experiment feedback unavailable: {error:#}. Preserve the file and repair it before retrying."
                ));
            }
        }
        self.blocked.clone()
    }

    pub fn file_path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn active(&self) -> Option<&str> {
        self.feedback.active.as_deref()
    }

    pub fn draft(&self, id: &str) -> &str {
        self.feedback.drafts.get(id).map_or("", |draft| &draft.text)
    }

    pub fn submission_count(&self, id: &str) -> usize {
        self.feedback
            .submissions
            .iter()
            .filter(|entry| entry.experiment == id)
            .count()
    }

    pub fn pin(&mut self, id: Option<&str>) -> Result<()> {
        if let Some(id) = id {
            validate_id(id)?;
        }
        self.commit(Mutation::Pin(id.map(str::to_owned)))
    }

    pub fn set_draft(&mut self, id: &str, text: &str) -> Result<()> {
        validate_id(id)?;
        validate_text(text)?;
        self.commit(Mutation::Draft {
            id: id.to_owned(),
            text: text.to_owned(),
        })
    }

    /// Records only explicitly submitted text. Empty feedback cannot manufacture intent.
    pub fn submit(&mut self, id: &str, number: u16, build: &str) -> Result<()> {
        validate_id(id)?;
        ensure!(number > 0, "Experiment number must be positive");
        ensure!(
            !build.is_empty() && build.len() <= 1024,
            "Invalid build identity"
        );
        let body = self.draft(id);
        ensure!(!body.trim().is_empty(), "Write feedback before saving it");
        self.commit(Mutation::Submit(Submission {
            id: unique_token()?,
            experiment: id.to_owned(),
            number,
            build: build.to_owned(),
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            body: body.to_owned(),
            extra: BTreeMap::new(),
        }))
    }

    fn commit(&mut self, mutation: Mutation) -> Result<()> {
        if let Some(reason) = &self.blocked {
            bail!("{reason}");
        }
        let path = self
            .path
            .as_deref()
            .context("Experiment feedback unavailable in this embedded session")?;
        let _lock = lock_feedback(path)?;
        let (mut next, source) = read_feedback(path)?;
        merge_mutation(&mut next, &self.feedback, mutation)?;
        validate_feedback(&next)?;
        let bytes = serde_json::to_vec_pretty(&next)?;
        ensure!(
            bytes.len() <= MAX_FILE_BYTES,
            "Feedback file is full (1 MiB). Preserve it before making room."
        );
        write_atomic(path, &bytes, &source)?;
        self.feedback = next;
        Ok(())
    }
}

fn merge_mutation(latest: &mut Feedback, known: &Feedback, mutation: Mutation) -> Result<()> {
    match mutation {
        Mutation::Pin(id) => latest.active = id,
        Mutation::Draft { id, text } => {
            check_draft_unchanged(latest, known, &id)?;
            latest.drafts.entry(id).or_default().text = text;
        }
        Mutation::Submit(submission) => {
            check_draft_unchanged(latest, known, &submission.experiment)?;
            latest
                .drafts
                .get_mut(&submission.experiment)
                .expect("a matching nonempty draft exists before submission")
                .text
                .clear();
            latest.submissions.push(submission);
        }
    }
    Ok(())
}

fn check_draft_unchanged(latest: &Feedback, known: &Feedback, id: &str) -> Result<()> {
    let latest_text = latest
        .drafts
        .get(id)
        .map_or("", |draft| draft.text.as_str());
    let known_text = known.drafts.get(id).map_or("", |draft| draft.text.as_str());
    ensure!(latest_text == known_text, "This experiment's draft changed in another pane. Keep this draft open and reconcile the two drafts before retrying; nothing was overwritten.");
    Ok(())
}

fn configured_path(repo: &Path) -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SWITCHBARD_EXPERIMENT_FEEDBACK_FILE") {
        ensure!(
            !path.is_empty(),
            "SWITCHBARD_EXPERIMENT_FEEDBACK_FILE must not be empty"
        );
        return Ok(PathBuf::from(path));
    }
    let repo = fs::canonicalize(repo).context("Resolve repository for experiment feedback")?;
    let readable: String = crate::views::repo_file_key(&repo)
        .chars()
        .take(48)
        .collect();
    // Stable FNV-1a over the exact canonical path distinguishes lossy display keys.
    let hash = repo
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    dirs::home_dir()
        .map(|home| {
            home.join(".switchbard/experiment-feedback")
                .join(format!("{readable}-{hash:016x}.json"))
        })
        .context("Home directory unavailable")
}

fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty() && id.len() <= 128,
        "Invalid experiment identifier"
    );
    ensure!(
        id.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "Invalid experiment identifier"
    );
    Ok(())
}

fn validate_text(text: &str) -> Result<()> {
    ensure!(
        text.chars().count() <= MAX_DRAFT_CHARS,
        "Feedback is limited to {MAX_DRAFT_CHARS} characters; your saved draft is unchanged"
    );
    Ok(())
}

fn validate_feedback(feedback: &Feedback) -> Result<()> {
    ensure!(
        feedback.version == 1,
        "Unsupported feedback version {}",
        feedback.version
    );
    ensure!(
        feedback.drafts.len() <= MAX_EXPERIMENTS,
        "Feedback has reached {MAX_EXPERIMENTS} experiments; preserve it before making room"
    );
    ensure!(
        feedback.submissions.len() <= MAX_SUBMISSIONS,
        "Feedback has reached {MAX_SUBMISSIONS} submissions; preserve it before making room"
    );
    if let Some(id) = &feedback.active {
        validate_id(id)?;
    }
    for (id, draft) in &feedback.drafts {
        validate_id(id)?;
        validate_text(&draft.text)?;
    }
    for submission in &feedback.submissions {
        validate_id(&submission.experiment)?;
        validate_text(&submission.body)?;
        ensure!(
            !submission.id.is_empty() && submission.id.len() <= 128 && submission.number > 0,
            "Invalid feedback submission identity"
        );
    }
    Ok(())
}

fn read_feedback(path: &Path) -> Result<(Feedback, Option<Vec<u8>>)> {
    let source = read_bounded(path)?;
    let feedback = match &source {
        Some(bytes) => serde_json::from_slice(bytes).context("Invalid experiment feedback JSON")?,
        None => Feedback::default(),
    };
    validate_feedback(&feedback)?;
    Ok((feedback, source))
}

fn read_bounded(path: &Path) -> Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("Read feedback metadata"),
    };
    ensure!(
        metadata.is_file(),
        "Feedback must be a regular file, not a symlink"
    );
    ensure!(
        metadata.len() <= MAX_FILE_BYTES as u64,
        "Feedback exceeds 1 MiB"
    );
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .context("Open feedback")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_FILE_BYTES as u64,
        "Feedback changed type or exceeds 1 MiB"
    );
    let mut bytes = Vec::new();
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= MAX_FILE_BYTES, "Feedback exceeds 1 MiB");
    Ok(Some(bytes))
}

fn lock_feedback(path: &Path) -> Result<File> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).context("Create feedback directory")?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path.with_extension("lock"))
        .context("Open feedback lock")?;
    ensure!(
        lock.metadata()?.is_file(),
        "Feedback lock must be a regular file"
    );
    lock.try_lock()
        .context("Another pane is saving feedback. Try again.")?;
    Ok(lock)
}

fn check_source(path: &Path, source: &Option<Vec<u8>>) -> Result<()> {
    ensure!(read_bounded(path)?.as_ref() == source.as_ref(), "Feedback changed in another pane. Keep this draft open and reload feedback before retrying; nothing was overwritten.");
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8], source: &Option<Vec<u8>>) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", unique_token()?));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .context("Create temporary feedback")?;
    let result = (|| {
        file.write_all(bytes).context("Write feedback")?;
        file.sync_all().context("Flush feedback")?;
        check_source(path, source)?;
        fs::rename(&temporary, path).context("Replace feedback")
    })();
    if let Err(error) = result {
        fs::remove_file(&temporary).with_context(|| {
            format!(
                "Remove temporary feedback {}; original failure: {error:#}",
                temporary.display()
            )
        })?;
        return Err(error);
    }
    Ok(())
}

fn unique_token() -> Result<String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("System clock precedes the Unix epoch")?
        .as_nanos();
    Ok(format!("{stamp}-{}-{sequence}", std::process::id()))
}
