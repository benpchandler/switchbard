//! Transactional execution state; every command validates its predecessor and custody.
use super::transitions::{validate_options, validate_text, worker_definitely_dead};
use super::{
    repository_key, Lease, Run, RunAction, RunOptions, RunState, TaskStorageIdentity, MAX_EVIDENCE,
    MAX_MESSAGES, MAX_RUNS, MAX_TEXT,
};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Store {
    connection: Connection,
}

pub fn default_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SWITCHBARD_BUG_RUN_DB") {
        return Ok(path.into());
    }
    Ok(dirs::home_dir()
        .context("cannot locate home directory")?
        .join(".switchbard/bug-runs.sqlite3"))
}

impl Store {
    pub fn open_default() -> Result<Self> {
        Self::open(default_path()?)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        secure_file(path)?;
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_millis(50))?;
        initialize(&mut connection)?;
        Ok(Self { connection })
    }

    /// Enqueue only after the authoritative task exists. Replay returns the original run.
    pub fn enqueue(&mut self, repo: &Path, task_id: &str, options: RunOptions) -> Result<Run> {
        let key = repository_key(repo)?;
        let _task_lock = crate::storage::RepositoryLock::acquire(repo)?;
        let backlog = crate::load_backlog_repo(repo)?;
        let task = backlog
            .tasks
            .iter()
            .find(|t| {
                t.id.eq_ignore_ascii_case(task_id)
                    || t.id
                        .split_once('-')
                        .is_some_and(|(_, bare)| bare.eq_ignore_ascii_case(task_id))
            })
            .context("bug task does not exist in the authoritative backlog")?;
        ensure!(
            task.editable() && !task.is_done(),
            "only active unfinished bug tasks can be dispatched"
        );
        ensure!(
            task.labels.iter().any(|l| l.eq_ignore_ascii_case("bug")),
            "only bug tasks may enter bug dispatch"
        );
        ensure!(
            !task
                .labels
                .iter()
                .any(|l| matches!(l.as_str(), "dispatch" | "dispatching")),
            "task already belongs to the legacy dispatch queue"
        );
        validate_options(&options)?;
        let run = new_run(repo, key, task, options)?;
        let task_key = run.task_storage.as_ref().map_or_else(
            || format!("task:{}", run.task_id),
            |s| format!("record:{}", s.record_id),
        );
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(payload) = tx
            .query_row(
                "SELECT payload FROM bug_runs WHERE repo_key=?1 AND task_key=?2",
                params![run.repo_key, task_key],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            return decode(&payload);
        }
        let count: usize = tx.query_row("SELECT COUNT(*) FROM bug_runs", [], |r| r.get(0))?;
        ensure!(
            count < MAX_RUNS,
            "bug run storage limit reached; existing runs were preserved"
        );
        tx.execute(
            "INSERT INTO bug_runs VALUES(?1,?2,?3,?4,?5)",
            params![run.id, run.repo_key, task_key, run.revision, encode(&run)?],
        )?;
        tx.commit()?;
        Ok(run)
    }

    pub fn get(&self, id: &str) -> Result<Run> {
        read_run(&self.connection, id)
    }

    pub fn reconcile_interrupted(&mut self, repo: &Path) -> Result<()> {
        for run in self.list(repo)?.into_iter().take(MAX_RUNS) {
            if matches!(run.state, RunState::Running | RunState::Publishing)
                && run.lease.as_ref().is_some_and(worker_definitely_dead)
            {
                let unknown = self.mark_unknown(&run.id, run.revision, "Supervisor exited before recording an outcome. Inspect the retained worktree and process logs before recovery.")?;
                super::task_flow::project(&unknown)?;
            }
        }
        Ok(())
    }

    pub fn list(&self, repo: &Path) -> Result<Vec<Run>> {
        let key = repository_key(repo)?;
        let mut stmt = self.connection.prepare(
            "SELECT payload FROM bug_runs WHERE repo_key=?1 ORDER BY rowid DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![key, MAX_RUNS + 1], |r| r.get::<_, String>(0))?;
        let mut runs = Vec::new();
        for row in rows.take(MAX_RUNS + 1) {
            runs.push(decode(&row?)?);
        }
        ensure!(
            runs.len() <= MAX_RUNS,
            "bug run list exceeds supported limit"
        );
        Ok(runs)
    }

    pub(super) fn owner_command(
        &mut self,
        id: &str,
        revision: u64,
        command: impl FnOnce(&mut Run) -> Result<()>,
    ) -> Result<Run> {
        self.mutate(id, |run| {
            ensure!(
                run.revision == revision,
                "run changed; refresh and preserve your draft before retrying"
            );
            command(run)
        })
    }

    pub(super) fn worker_command(
        &mut self,
        lease: &Lease,
        command: impl FnOnce(&mut Run) -> Result<()>,
    ) -> Result<Run> {
        self.mutate(&lease.run.id, |run| {
            ensure!(
                matches!(run.state, RunState::Running | RunState::Publishing),
                "worker no longer owns an active run"
            );
            ensure!(
                run.lease.as_ref().is_some_and(|w| w.token == lease.token),
                "worker lease no longer owns this run"
            );
            command(run)
        })
    }

    pub(super) fn mutate(
        &mut self,
        id: &str,
        command: impl FnOnce(&mut Run) -> Result<()>,
    ) -> Result<Run> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut run = read_run(&tx, id)?;
        let revision = run.revision;
        command(&mut run)?;
        run.revision = revision.checked_add(1).context("run revision overflow")?;
        run.updated_at_unix = now();
        let changed = tx.execute(
            "UPDATE bug_runs SET revision=?1,payload=?2 WHERE id=?3 AND revision=?4",
            params![run.revision, encode(&run)?, id, revision],
        )?;
        ensure!(changed == 1, "run changed concurrently");
        tx.commit()?;
        Ok(run)
    }
}

fn new_run(
    repo: &Path,
    repo_key: String,
    task: &crate::BacklogTask,
    options: RunOptions,
) -> Result<Run> {
    validate_text(&task.title, false)?;
    let report_snapshot = crate::read_backlog_task_snapshot(repo, &task.id)?.content;
    validate_text(&report_snapshot, false)?;
    let task_storage = task.storage_identity.as_ref().map(|s| TaskStorageIdentity {
        repository_id: s.repository_id.clone(),
        record_id: s.record_id.clone(),
        revision: s.revision,
    });
    Ok(Run {
        id: uuid::Uuid::new_v4().to_string(),
        task_id: task.id.clone(),
        title: task.title.clone(),
        repo_root: repo.canonicalize()?,
        repo_key,
        task_storage,
        report_snapshot,
        options,
        state: RunState::AgentQueued,
        revision: 1,
        thread_id: None,
        worktree: None,
        branch: None,
        review_head: None,
        review_remote: None,
        review_repository: None,
        draft: String::new(),
        prompt: String::new(),
        summary: String::new(),
        evidence: Vec::new(),
        risks: Vec::new(),
        error: None,
        pr_url: None,
        lease: None,
        retry_action: Some(RunAction::StartAgent),
        messages: Vec::new(),
        created_at_unix: now(),
        updated_at_unix: now(),
    })
}

fn initialize(connection: &mut Connection) -> Result<()> {
    const APPLICATION_ID: u32 = 0x53424247;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: u32 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let app: u32 = tx.query_row("PRAGMA application_id", [], |r| r.get(0))?;
    let tables: usize = tx.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table'",
        [],
        |r| r.get(0),
    )?;
    if version == 0 && app == 0 && tables == 0 {
        tx.execute_batch(
            "CREATE TABLE bug_runs(id TEXT PRIMARY KEY, repo_key TEXT NOT NULL,
            task_key TEXT NOT NULL, revision INTEGER NOT NULL, payload TEXT NOT NULL,
            UNIQUE(repo_key,task_key)); PRAGMA user_version=1; PRAGMA application_id=1396851271;",
        )?;
    } else {
        ensure!(
            version == 1 && app == APPLICATION_ID,
            "unsupported bug run database version or application; stored data was not changed"
        );
    }
    tx.commit()?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;",
    )?;
    Ok(())
}

fn read_run(connection: &Connection, id: &str) -> Result<Run> {
    let payload: String = connection
        .query_row("SELECT payload FROM bug_runs WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?
        .context("bug run not found")?;
    decode(&payload)
}

fn encode(run: &Run) -> Result<String> {
    Ok(serde_json::to_string(run)?)
}
fn decode(payload: &str) -> Result<Run> {
    ensure!(
        payload.len() <= MAX_TEXT * (MAX_MESSAGES + MAX_EVIDENCE * 2 + 8),
        "bug run payload exceeds supported limit"
    );
    serde_json::from_str(payload).context("invalid persisted bug run")
}
pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn secure_file(path: &Path) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "bug run database is not a regular file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
