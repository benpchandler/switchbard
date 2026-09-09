//! Shared, flexible document authority. Payload bytes are never projected back into storage.
mod established;
mod source_drift;
pub use source_drift::{LegacySourceDrift, LegacySourceState};
mod exchange;
mod identity;
mod kind_transaction;
mod migration;
mod permissions;
mod recovery;
mod repository_lock;
mod schema_v2;
mod workspace_order;
pub use workspace_order::{
    preview_workspace_ordering, workspace_ordering_source_digest, WorkspaceOrderEntry,
    WorkspaceOrderTarget, WorkspaceOrdering,
};
#[cfg(test)]
mod tests;

use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub use exchange::{
    ConflictResolution, ExchangeContent, ExchangeRecord, ExchangeSnapshot, ImportPreview,
    ImportResult, MAX_EXCHANGE_BYTES,
};
use identity::repository_binding;
pub(crate) use migration::SelectedDocument;
pub use migration::{MigrationPlan, SourceDocument};
pub use repository_lock::RepositoryLock;
pub(crate) use repository_lock::RepositoryLockSet;

pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_DOCUMENTS: usize = 100_000;
const SCHEMA_VERSION: i64 = 5;
const APPLICATION_ID: i64 = 0x53425244;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepositoryId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Document {
    pub id: String,
    pub repo_id: RepositoryId,
    pub kind: String,
    pub locator: String,
    pub revision: u64,
    pub content: Vec<u8>,
    #[serde(default = "default_content_version")]
    pub content_version: u32,
    pub deleted: bool,
}

pub fn default_content_version() -> u32 {
    1
}

/// Domain payload capability, separate from the database and exchange schemas.
pub fn content_is_understood(kind: &str, version: u32) -> bool {
    version == 1
        && matches!(
            kind,
            "task" | "initiative" | "project" | "config" | "goals" | "ranking"
        )
}

impl Document {
    pub fn ensure_understood(&self) -> Result<()> {
        ensure!(content_is_understood(&self.kind, self.content_version),
            "unsupported {} content version {} for record {}; preserve opaque content and use a compatible client", self.kind, self.content_version, self.id);
        Ok(())
    }
}

pub struct Store {
    connection: Connection,
    path: PathBuf,
}

#[cfg(test)]
thread_local! { static TEST_DATABASE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) }; }

#[cfg(test)]
pub(crate) fn with_test_database<T>(path: &Path, run: impl FnOnce() -> T) -> T {
    struct Reset(Option<PathBuf>);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_DATABASE.with(|value| *value.borrow_mut() = self.0.take());
        }
    }
    let _reset = Reset(TEST_DATABASE.with(|value| value.replace(Some(path.to_path_buf()))));
    run()
}

pub fn default_database_path() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(path) = TEST_DATABASE.with(|value| value.borrow().clone()) {
        return Ok(path);
    }
    if let Some(path) = std::env::var_os("SWITCHBARD_DATABASE") {
        ensure!(!path.is_empty(), "SWITCHBARD_DATABASE must not be empty");
        return Ok(PathBuf::from(path));
    }
    Ok(dirs::home_dir()
        .context("cannot determine home directory")?
        .join(".switchbard/switchbard.sqlite3"))
}

/// Cheap revision poll shared by all frontends; absence is the only legacy result.
pub fn current_change_sequence() -> Result<Option<u64>> {
    Store::open_existing_default()?
        .as_ref()
        .map(Store::change_sequence)
        .transpose()
}

impl Store {
    pub fn open_default() -> Result<Self> {
        Self::open(default_database_path()?)
    }

    /// Absence alone means legacy mode. Corruption and permission errors propagate.
    pub fn open_existing_default() -> Result<Option<Self>> {
        Self::open_existing(default_database_path()?)
    }

    pub fn open_existing(path: impl AsRef<Path>) -> Result<Option<Self>> {
        established::check(path.as_ref())?;
        match std::fs::symlink_metadata(path.as_ref()) {
            Ok(_) => Self::open(path).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        established::check(&path)?;
        permissions::prepare(&path)?;
        let connection = Connection::open(&path).context("open Switchbard database")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;",
        )?;
        initialize(&connection)?;
        permissions::validate(&path)?;
        established::record(&path)?;
        Ok(Self { connection, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn authority(&self, repo: &RepositoryId, kind: &str) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM authority WHERE repo_id=?1 AND kind=?2)",
            params![repo.0, kind],
            |row| row.get(0),
        )?)
    }

    pub fn authority_for_root(&self, root: &Path, kind: &str) -> Result<Option<RepositoryId>> {
        let Some(repo) = self.repository(root)? else {
            return Ok(None);
        };
        Ok(self.authority(&repo, kind)?.then_some(repo))
    }

    pub fn read(&self, repo: &RepositoryId, kind: &str, locator: &str) -> Result<Option<Document>> {
        read_document(&self.connection, repo, kind, locator)
    }

    /// Includes tombstones so exchange and reconciliation cannot mistake deletion for absence.
    pub fn list(&self, repo: &RepositoryId, kind: &str) -> Result<Vec<Document>> {
        list_documents(&self.connection, repo, kind)
    }

    /// The closure sees the latest committed bytes under the database writer lock.
    /// Some(expected) rejects stale drafts; None is safe for a surgical transformation.
    pub fn mutate<F>(
        &mut self,
        repo: &RepositoryId,
        kind: &str,
        locator: &str,
        expected: Option<u64>,
        change: F,
    ) -> Result<Option<Document>>
    where
        F: FnOnce(Option<&Document>) -> Result<Option<Vec<u8>>>,
    {
        self.mutate_transaction(repo, kind, locator, expected, |_| Ok(()), change)
    }

    /// Runs kind-wide uniqueness checks under the same writer lock as the edit.
    pub fn mutate_checked<F, V>(
        &mut self,
        repo: &RepositoryId,
        kind: &str,
        locator: &str,
        expected: Option<u64>,
        validate: V,
        change: F,
    ) -> Result<Option<Document>>
    where
        F: FnOnce(Option<&Document>) -> Result<Option<Vec<u8>>>,
        V: FnOnce(&[Document]) -> Result<()>,
    {
        self.mutate_transaction(
            repo,
            kind,
            locator,
            expected,
            |connection| validate(&list_documents(connection, repo, kind)?),
            change,
        )
    }

    fn mutate_transaction<F, V>(
        &mut self,
        repo: &RepositoryId,
        kind: &str,
        locator: &str,
        expected: Option<u64>,
        validate: V,
        change: F,
    ) -> Result<Option<Document>>
    where
        F: FnOnce(Option<&Document>) -> Result<Option<Vec<u8>>>,
        V: FnOnce(&Connection) -> Result<()>,
    {
        validate_locator(kind, locator)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let authoritative: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM authority WHERE repo_id=?1 AND kind=?2)",
            params![repo.0, kind],
            |row| row.get(0),
        )?;
        ensure!(
            authoritative,
            "record kind has not been migrated to central storage"
        );
        validate(&tx)?;
        let old = read_document(&tx, repo, kind, locator)?;
        if let Some(ref document) = old {
            document.ensure_understood()?;
        } else {
            ensure!(
                content_is_understood(kind, 1),
                "unsupported document kind for local mutation: {kind}"
            );
        }
        if let Some(revision) = expected {
            ensure!(
                old.as_ref().map(|doc| doc.revision) == Some(revision),
                "stale document revision; reload and retry"
            );
        }
        let content = change(old.as_ref())?;
        let result = persist_change(&tx, repo, kind, locator, old, content)?;
        tx.commit()?;
        Ok(result)
    }

    /// SQLite creates a transactionally consistent copy. Never overwrite an existing backup.
    pub fn backup_to(&self, destination: &Path) -> Result<()> {
        ensure!(
            !destination.try_exists()?,
            "backup destination already exists"
        );
        permissions::prepare(destination)?;
        self.connection.execute(
            "VACUUM INTO ?1",
            [destination.to_str().context("non-UTF8 backup path")?],
        )?;
        permissions::validate(destination)?;
        std::fs::File::open(destination)?.sync_all()?;
        Ok(())
    }

    pub fn change_sequence(&self) -> Result<u64> {
        Ok(self
            .connection
            .query_row("SELECT sequence FROM metadata", [], |row| row.get(0))?)
    }
}

fn initialize(connection: &Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    ensure!(
        (0..=SCHEMA_VERSION).contains(&version),
        "unsupported Switchbard database schema {version}"
    );
    if version > 0 {
        let application: i64 =
            connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
        ensure!(
            application == APPLICATION_ID,
            "not a Switchbard document database"
        );
        if version == 1 {
            schema_v2::upgrade(connection)?;
        }
        if version < 3 {
            schema_v2::add_locator_history(connection)?;
        }
        if version < 4 {
            schema_v2::add_workspace_ordering(connection)?;
        }
        if version < 5 {
            schema_v2::add_content_version(connection)?;
        }
        return Ok(());
    }
    let tables: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        tables == 0,
        "refusing to initialize a nonempty unrecognized database"
    );
    connection.execute_batch(include_str!("schema.sql"))?;
    schema_v2::upgrade(connection)?;
    schema_v2::add_locator_history(connection)?;
    schema_v2::add_workspace_ordering(connection)?;
    schema_v2::add_content_version(connection)?;
    Ok(())
}

fn validate_locator(kind: &str, locator: &str) -> Result<()> {
    ensure!(
        !kind.is_empty() && kind.len() <= 128,
        "invalid document kind"
    );
    ensure!(
        !locator.is_empty() && locator.len() <= 4096 && !locator.contains('\0'),
        "invalid document locator"
    );
    Ok(())
}

fn document_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Document> {
    Ok(Document {
        id: row.get(0)?,
        repo_id: RepositoryId(row.get(1)?),
        kind: row.get(2)?,
        locator: row.get(3)?,
        revision: row.get(4)?,
        content: row.get(5)?,
        deleted: row.get(6)?,
        content_version: row.get(7)?,
    })
}

fn read_document(
    connection: &Connection,
    repo: &RepositoryId,
    kind: &str,
    locator: &str,
) -> Result<Option<Document>> {
    Ok(connection.query_row("SELECT id,repo_id,kind,locator,revision,content,deleted,content_version FROM documents WHERE repo_id=?1 AND kind=?2 AND locator=?3",
        params![repo.0, kind, locator], document_row).optional()?)
}

fn persist_change(
    connection: &Connection,
    repo: &RepositoryId,
    kind: &str,
    locator: &str,
    old: Option<Document>,
    content: Option<Vec<u8>>,
) -> Result<Option<Document>> {
    if old.is_none() && content.is_none() {
        return Ok(None);
    }
    let deleted = content.is_none();
    let content =
        content.unwrap_or_else(|| old.as_ref().expect("existing deletion").content.clone());
    ensure!(
        content.len() <= MAX_DOCUMENT_BYTES,
        "document exceeds size limit"
    );
    if let Some(ref document) = old {
        if document.content == content && document.deleted == deleted {
            return Ok(old);
        }
    }
    let document = Document {
        id: old
            .as_ref()
            .map_or_else(|| Uuid::new_v4().to_string(), |d| d.id.clone()),
        repo_id: repo.clone(),
        kind: kind.into(),
        locator: locator.into(),
        revision: old.as_ref().map_or(Ok(1), |d| {
            d.revision
                .checked_add(1)
                .context("document revision exhausted")
        })?,
        content,
        content_version: old.as_ref().map_or(1, |document| document.content_version),
        deleted,
    };
    connection.execute("INSERT INTO documents(id,repo_id,kind,locator,revision,content,deleted) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(repo_id,kind,locator) DO UPDATE SET revision=excluded.revision,content=excluded.content,deleted=excluded.deleted",
        params![document.id, repo.0, kind, locator, document.revision, document.content, deleted])?;
    connection.execute(
        "INSERT INTO revisions(record_id,revision,content,deleted) VALUES (?1,?2,?3,?4)",
        params![document.id, document.revision, document.content, deleted],
    )?;
    kind_transaction::remember_locator(connection, &document)?;
    exchange::increment_clock(connection, &document.id)?;
    connection.execute("UPDATE metadata SET sequence=sequence+1", [])?;
    Ok(Some(document))
}

fn list_documents(
    connection: &Connection,
    repo: &RepositoryId,
    kind: &str,
) -> Result<Vec<Document>> {
    let mut statement = connection.prepare("SELECT id,repo_id,kind,locator,revision,content,deleted,content_version FROM documents WHERE repo_id=?1 AND kind=?2 ORDER BY locator LIMIT ?3")?;
    let documents = statement
        .query_map(params![repo.0, kind, MAX_DOCUMENTS + 1], document_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        documents.len() <= MAX_DOCUMENTS,
        "document count exceeds limit"
    );
    Ok(documents)
}
