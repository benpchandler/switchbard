//! Optional full snapshots with per-record causal clocks; no export advances ancestry.
mod clocks;
mod merge;
#[cfg(test)]
mod tests;

use super::{
    document_row, repository_binding, RepositoryId, Store, MAX_DOCUMENTS, MAX_DOCUMENT_BYTES,
};
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use uuid::Uuid;

pub(super) use clocks::increment_clock;
pub use merge::{ConflictResolution, ImportPreview, ImportResult};
pub const MAX_EXCHANGE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "encoding", content = "data", deny_unknown_fields)]
pub enum ExchangeContent {
    #[serde(rename = "utf8-lines")]
    Utf8Lines(Vec<String>),
    #[serde(rename = "base64")]
    Base64(String),
}

impl ExchangeContent {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(text) => Self::Utf8Lines(text.split_inclusive('\n').map(str::to_owned).collect()),
            Err(_) => Self::Base64(STANDARD.encode(bytes)),
        }
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        let bytes = match self {
            Self::Utf8Lines(lines) => {
                for (index, line) in lines.iter().enumerate() {
                    ensure!(!line.is_empty(), "empty UTF8 line fragment");
                    ensure!(
                        !line.strip_suffix('\n').unwrap_or(line).contains('\n'),
                        "embedded newline in UTF8 line fragment"
                    );
                    ensure!(
                        index + 1 == lines.len() || line.ends_with('\n'),
                        "unterminated interior UTF8 line fragment"
                    );
                }
                lines.concat().into_bytes()
            }
            Self::Base64(text) => STANDARD.decode(text)?,
        };
        ensure!(
            bytes.len() <= MAX_DOCUMENT_BYTES,
            "exchange document exceeds size limit"
        );
        Ok(bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExchangeRecord {
    pub id: String,
    pub kind: String,
    pub locator: String,
    pub content: ExchangeContent,
    #[serde(
        default = "super::default_content_version",
        skip_serializing_if = "is_version_one"
    )]
    pub content_version: u32,
    pub deleted: bool,
    #[serde(deserialize_with = "clocks::deserialize")]
    pub clock: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExchangeSnapshot {
    pub version: u32,
    pub repo_id: RepositoryId,
    pub epoch_id: String,
    pub kinds: Vec<String>,
    pub records: Vec<ExchangeRecord>,
    pub digest: String,
}

impl ExchangeSnapshot {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_EXCHANGE_BYTES, "exchange exceeds 64 MiB");
        validate_json_depth(bytes)?;
        let snapshot: Self = serde_json::from_slice(bytes)?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut canonical = self.clone();
        canonical
            .records
            .sort_by(|left, right| left.id.cmp(&right.id));
        canonical.kinds.sort();
        let mut bytes = serde_json::to_vec_pretty(&canonical)?;
        bytes.push(b'\n');
        ensure!(bytes.len() <= MAX_EXCHANGE_BYTES, "exchange exceeds 64 MiB");
        Ok(bytes)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 2,
            "unsupported exchange version {}",
            self.version
        );
        validate_uuid(&self.repo_id.0)?;
        validate_uuid(&self.epoch_id)?;
        ensure!(
            self.records.len() <= MAX_DOCUMENTS && self.kinds.len() <= 128,
            "exchange count exceeds limit"
        );
        let kinds: BTreeSet<_> = self.kinds.iter().collect();
        ensure!(kinds.len() == self.kinds.len(), "duplicate exchange kinds");
        let mut ids = BTreeSet::new();
        let mut locators = BTreeSet::new();
        let mut total = 0usize;
        for kind in &self.kinds {
            super::validate_locator(kind, "exchange")?;
        }
        for record in &self.records {
            validate_uuid(&record.id)?;
            super::validate_locator(&record.kind, &record.locator)?;
            ensure!(kinds.contains(&record.kind), "record kind is not declared");
            ensure!(ids.insert(&record.id), "duplicate record ID {}", record.id);
            ensure!(
                locators.insert((&record.kind, &record.locator)),
                "duplicate record locator {}",
                record.locator
            );
            ensure!(
                record.content_version > 0,
                "content version must be positive"
            );
            clocks::validate(&record.clock)?;
            total = total
                .checked_add(record.content.bytes()?.len())
                .context("exchange size overflow")?;
            ensure!(
                total <= MAX_EXCHANGE_BYTES,
                "exchange payloads exceed size limit"
            );
        }
        ensure!(
            self.digest == self.computed_digest()?,
            "exchange digest mismatch"
        );
        Ok(())
    }

    /// Recompute after an explicit edit or conflict resolution; this grants no import authority.
    pub fn refresh_digest(&mut self) -> Result<()> {
        self.digest = self.computed_digest()?;
        Ok(())
    }

    fn computed_digest(&self) -> Result<String> {
        let mut canonical = self.clone();
        canonical.digest.clear();
        canonical
            .records
            .sort_by(|left, right| left.id.cmp(&right.id));
        canonical.kinds.sort();
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical)?)
        ))
    }
}

impl Store {
    pub fn export_snapshot(&self, repo: &RepositoryId) -> Result<ExchangeSnapshot> {
        let tx = self.connection.unchecked_transaction()?;
        let snapshot = snapshot(&tx, repo)?;
        tx.commit()?;
        Ok(snapshot)
    }

    /// Explicitly joins an unbound, empty local scope to this snapshot's identity.
    /// It never guesses identity from a remote URL or repository name.
    /// This generic envelope API does not validate native payload semantics.
    /// Native application boundaries must use `bind_exchange_checked`.
    pub fn bind_exchange(
        &mut self,
        root: &Path,
        incoming: &ExchangeSnapshot,
    ) -> Result<RepositoryId> {
        self.bind_exchange_checked(root, incoming, |_| Ok(()))
    }

    pub fn bind_exchange_checked(
        &mut self,
        root: &Path,
        incoming: &ExchangeSnapshot,
        validate: impl FnOnce(&ExchangeSnapshot) -> Result<()>,
    ) -> Result<RepositoryId> {
        let sequence = self.change_sequence()?;
        self.bind_exchange_checked_at(root, incoming, sequence, validate)
    }

    pub fn bind_exchange_checked_at(
        &mut self,
        root: &Path,
        incoming: &ExchangeSnapshot,
        expected_sequence: u64,
        validate: impl FnOnce(&ExchangeSnapshot) -> Result<()>,
    ) -> Result<RepositoryId> {
        let _repository_lock = super::RepositoryLock::acquire(root)?;
        incoming.validate()?;
        let binding = repository_binding(root)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let actual_sequence: u64 =
            tx.query_row("SELECT sequence FROM metadata", [], |row| row.get(0))?;
        ensure!(
            actual_sequence == expected_sequence,
            "stale bind preview; preview again"
        );
        let existing: Option<String> = tx
            .query_row(
                "SELECT repo_id FROM bindings WHERE binding=?1",
                [&binding],
                |row| row.get(0),
            )
            .optional()?;
        ensure!(
            existing.is_none(),
            "scope is already bound; use import for its existing identity"
        );
        let present: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM repositories WHERE repo_id=?1)",
            [&incoming.repo_id.0],
            |row| row.get(0),
        )?;
        if !present {
            tx.execute(
                "INSERT INTO repositories(repo_id,epoch_id) VALUES (?1,?2)",
                params![incoming.repo_id.0, incoming.epoch_id],
            )?;
        }
        super::identity::register(&tx, &incoming.repo_id, &binding, root)?;
        merge::apply_checked(&tx, &incoming.repo_id, incoming, &BTreeMap::new(), validate)?;
        tx.commit()?;
        Ok(incoming.repo_id.clone())
    }

    /// Required after restoring a backup before making new local edits.
    pub fn rotate_replica(&mut self) -> Result<String> {
        let replica = Uuid::new_v4().to_string();
        self.connection
            .execute("UPDATE metadata SET replica_id=?1", [&replica])?;
        Ok(replica)
    }
}

pub(super) fn snapshot(connection: &Connection, repo: &RepositoryId) -> Result<ExchangeSnapshot> {
    let epoch_id = connection.query_row(
        "SELECT epoch_id FROM repositories WHERE repo_id=?1",
        [&repo.0],
        |row| row.get(0),
    )?;
    let kinds = connection
        .prepare("SELECT kind FROM authority WHERE repo_id=?1 ORDER BY kind")?
        .query_map([&repo.0], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    let documents = connection.prepare("SELECT id,repo_id,kind,locator,revision,content,deleted,content_version FROM documents WHERE repo_id=?1 ORDER BY id LIMIT ?2")?
        .query_map(params![repo.0,MAX_DOCUMENTS + 1], document_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        documents.len() <= MAX_DOCUMENTS,
        "repository exceeds exchange record limit"
    );
    let records = documents
        .into_iter()
        .map(|document| {
            Ok(ExchangeRecord {
                clock: clocks::load(connection, &document.id)?,
                id: document.id,
                kind: document.kind,
                locator: document.locator,
                content: ExchangeContent::from_bytes(&document.content),
                content_version: document.content_version,
                deleted: document.deleted,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut snapshot = ExchangeSnapshot {
        version: 2,
        repo_id: repo.clone(),
        epoch_id,
        kinds,
        records,
        digest: String::new(),
    };
    snapshot.refresh_digest()?;
    snapshot.validate()?;
    Ok(snapshot)
}

fn validate_uuid(value: &str) -> Result<()> {
    ensure!(
        Uuid::parse_str(value)?.to_string() == value,
        "UUID must use canonical lowercase hyphenated form"
    );
    Ok(())
}

fn validate_json_depth(bytes: &[u8]) -> Result<()> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'[' | b'{' => {
                    depth += 1;
                    ensure!(depth <= 32, "exchange JSON depth exceeds 32");
                }
                b']' | b'}' => {
                    depth = depth
                        .checked_sub(1)
                        .context("unbalanced JSON closing delimiter")?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn is_version_one(version: &u32) -> bool {
    *version == 1
}
