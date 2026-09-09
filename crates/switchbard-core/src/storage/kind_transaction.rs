//! Kind-wide transformations are one commit, including allocation and reference fanout.
use super::{
    exchange, validate_locator, Document, RepositoryId, Store, MAX_DOCUMENTS, MAX_DOCUMENT_BYTES,
};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, TransactionBehavior};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

impl Store {
    pub fn mutate_kind<T>(
        &mut self,
        repo: &RepositoryId,
        kind: &str,
        expected_sequence: Option<u64>,
        change: impl FnOnce(&mut Vec<Document>) -> Result<T>,
    ) -> Result<T> {
        self.mutate_kind_with_history(repo, kind, expected_sequence, |documents, _history| {
            change(documents)
        })
    }

    pub fn used_locators(&self, repo: &RepositoryId, kind: &str) -> Result<Vec<String>> {
        used_locators(&self.connection, repo, kind)
    }

    pub fn mutate_kind_with_history<T>(
        &mut self,
        repo: &RepositoryId,
        kind: &str,
        expected_sequence: Option<u64>,
        change: impl FnOnce(&mut Vec<Document>, &[String]) -> Result<T>,
    ) -> Result<T> {
        self.mutate_kinds(repo, &[kind], expected_sequence, |documents, history| {
            change(
                documents,
                history.get(kind).expect("requested history present"),
            )
        })
    }

    pub fn mutate_kinds<T>(
        &mut self,
        repo: &RepositoryId,
        kinds: &[&str],
        expected_sequence: Option<u64>,
        change: impl FnOnce(&mut Vec<Document>, &BTreeMap<String, Vec<String>>) -> Result<T>,
    ) -> Result<T> {
        ensure!(
            !kinds.is_empty() && kinds.len() <= 128,
            "invalid transaction kind count"
        );
        ensure!(
            kinds.iter().collect::<BTreeSet<_>>().len() == kinds.len(),
            "duplicate transaction kind"
        );
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(expected) = expected_sequence {
            let actual: u64 =
                tx.query_row("SELECT sequence FROM metadata", [], |row| row.get(0))?;
            ensure!(actual == expected, "stale kind snapshot; reload and retry");
        }
        let mut original = Vec::new();
        let mut history = BTreeMap::new();
        for kind in kinds {
            let authoritative: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM authority WHERE repo_id=?1 AND kind=?2)",
                params![repo.0, kind],
                |row| row.get(0),
            )?;
            ensure!(authoritative, "record kind {kind} has not been migrated");
            original.extend(super::list_documents(&tx, repo, kind)?);
            ensure!(
                original.len() <= MAX_DOCUMENTS,
                "transaction document count exceeds limit"
            );
            history.insert((*kind).to_string(), used_locators(&tx, repo, kind)?);
        }
        let mut documents = original.clone();
        let result = change(&mut documents, &history)?;
        validate_documents(repo, kinds, &original, &mut documents)?;
        persist(&tx, &original, &documents)?;
        tx.commit()?;
        Ok(result)
    }
}

fn validate_documents(
    repo: &RepositoryId,
    kinds: &[&str],
    original: &[Document],
    documents: &mut [Document],
) -> Result<()> {
    ensure!(documents.len() <= MAX_DOCUMENTS, "too many documents");
    let mut ids = BTreeSet::new();
    let mut locators = BTreeSet::new();
    for document in documents {
        if document.id.is_empty() {
            document.id = Uuid::new_v4().to_string();
        }
        Uuid::parse_str(&document.id)?;
        ensure!(
            &document.repo_id == repo && kinds.contains(&document.kind.as_str()),
            "document scope cannot change"
        );
        validate_locator(&document.kind, &document.locator)?;
        ensure!(
            document.content_version > 0,
            "content version must be positive"
        );
        ensure!(
            document.content.len() <= MAX_DOCUMENT_BYTES,
            "document too large"
        );
        ensure!(ids.insert(document.id.clone()), "duplicate record ID");
        ensure!(
            locators.insert((document.kind.clone(), document.locator.clone())),
            "duplicate document locator"
        );
    }
    ensure!(
        original.iter().all(|document| ids.contains(&document.id)),
        "omitting an existing document is not deletion; retain a tombstone"
    );
    Ok(())
}

fn persist(
    connection: &rusqlite::Connection,
    original: &[Document],
    documents: &[Document],
) -> Result<()> {
    let originals: BTreeMap<_, _> = original.iter().map(|doc| (&doc.id, doc)).collect();
    for document in documents {
        if let Some(old) = originals.get(&document.id) {
            ensure!(
                old.kind == document.kind,
                "existing document kind cannot change"
            );
        }
        if let Some(old) = originals.get(&document.id) {
            ensure!(
                old.content_version == document.content_version,
                "local edits cannot change payload version"
            );
            if old.locator != document.locator
                || old.content != document.content
                || old.deleted != document.deleted
            {
                old.ensure_understood()?;
            }
        } else {
            document.ensure_understood()?;
        }
        if !originals.contains_key(&document.id) {
            let exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
                [&document.id],
                |row| row.get(0),
            )?;
            ensure!(!exists, "new record ID already belongs to another scope");
        }
    }
    let changed: Vec<_> = documents
        .iter()
        .filter(|doc| {
            originals.get(&doc.id).is_none_or(|old| {
                old.locator != doc.locator
                    || old.content != doc.content
                    || old.deleted != doc.deleted
            })
        })
        .collect();
    // Free only changing locators inside this transaction, allowing an atomic locator swap.
    for document in &changed {
        connection.execute(
            "UPDATE documents SET locator=?1 WHERE id=?2",
            params![format!("__transaction_{}", Uuid::new_v4()), document.id],
        )?;
    }
    for document in &changed {
        let revision = originals.get(&document.id).map_or(Ok(1), |old| {
            old.revision
                .checked_add(1)
                .context("document revision exhausted")
        })?;
        connection.execute("INSERT INTO documents(id,repo_id,kind,locator,revision,content,deleted,content_version) VALUES (?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET locator=excluded.locator,revision=excluded.revision,content=excluded.content,deleted=excluded.deleted",
            params![document.id,document.repo_id.0,document.kind,document.locator,revision,document.content,document.deleted,document.content_version])?;
        connection.execute(
            "INSERT INTO revisions(record_id,revision,content,deleted,content_version) VALUES (?1,?2,?3,?4,?5)",
            params![document.id, revision, document.content, document.deleted, document.content_version],
        )?;
        remember_locator(connection, document)?;
        exchange::increment_clock(connection, &document.id)?;
    }
    if !changed.is_empty() {
        connection.execute("UPDATE metadata SET sequence=sequence+1", [])?;
    }
    Ok(())
}

pub(super) fn remember_locator(
    connection: &rusqlite::Connection,
    document: &Document,
) -> Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO document_locators(repo_id,kind,locator) VALUES (?1,?2,?3)",
        params![document.repo_id.0, document.kind, document.locator],
    )?;
    Ok(())
}

fn used_locators(
    connection: &rusqlite::Connection,
    repo: &RepositoryId,
    kind: &str,
) -> Result<Vec<String>> {
    let values = connection.prepare("SELECT locator FROM document_locators WHERE repo_id=?1 AND kind=?2 ORDER BY locator LIMIT ?3")?
        .query_map(params![repo.0,kind,MAX_DOCUMENTS + 1], |row| row.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        values.len() <= MAX_DOCUMENTS,
        "locator history exceeds allocation limit"
    );
    Ok(values)
}
