use super::{clocks, snapshot, ExchangeContent, ExchangeRecord, ExchangeSnapshot};
use crate::storage::{RepositoryId, Store};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub enum ConflictResolution {
    Local,
    Incoming,
    Custom { content: Vec<u8>, deleted: bool },
    RelocateIncoming { locator: String, content: Vec<u8> },
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportPreview {
    pub expected_sequence: u64,
    pub incoming_digest: String,
    pub conflicts: Vec<String>,
    pub changed_records: usize,
    pub validation_errors: Vec<String>,
    pub candidate_record_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub changed_records: usize,
    pub sequence: u64,
}

impl Store {
    pub fn preview_import(
        &self,
        repo: &RepositoryId,
        incoming: &ExchangeSnapshot,
    ) -> Result<ImportPreview> {
        self.preview_import_checked(repo, incoming, |_| Ok(()))
    }

    pub fn preview_import_checked(
        &self,
        repo: &RepositoryId,
        incoming: &ExchangeSnapshot,
        validate: impl FnOnce(&ExchangeSnapshot) -> Result<()>,
    ) -> Result<ImportPreview> {
        incoming.validate()?;
        let tx = self.connection.unchecked_transaction()?;
        let (mut preview, candidates) = plan(&tx, repo, incoming, &BTreeMap::new())?;
        if preview.conflicts.is_empty() {
            if let Err(error) = validate(&merged_snapshot(&tx, repo, incoming, &candidates)?) {
                preview.validation_errors.push(error.to_string());
            }
        }
        tx.commit()?;
        Ok(preview)
    }

    /// Imports opaque envelopes; native application boundaries must use
    /// `apply_import_checked` to validate the complete proposed domain state.
    pub fn apply_import(
        &mut self,
        repo: &RepositoryId,
        incoming: &ExchangeSnapshot,
        expected_sequence: u64,
    ) -> Result<ImportResult> {
        self.apply_import_with_resolutions(repo, incoming, expected_sequence, &BTreeMap::new())
    }

    /// Resolves generic envelopes without native payload validation. Application
    /// callers must use `apply_import_checked` with their domain validator.
    pub fn apply_import_with_resolutions(
        &mut self,
        repo: &RepositoryId,
        incoming: &ExchangeSnapshot,
        expected_sequence: u64,
        resolutions: &BTreeMap<String, ConflictResolution>,
    ) -> Result<ImportResult> {
        self.apply_import_checked(repo, incoming, expected_sequence, resolutions, |_| Ok(()))
    }

    /// The validator sees the complete proposed repository under the writer lock.
    pub fn apply_import_checked(
        &mut self,
        repo: &RepositoryId,
        incoming: &ExchangeSnapshot,
        expected_sequence: u64,
        resolutions: &BTreeMap<String, ConflictResolution>,
        validate: impl FnOnce(&ExchangeSnapshot) -> Result<()>,
    ) -> Result<ImportResult> {
        incoming.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sequence: u64 = tx.query_row("SELECT sequence FROM metadata", [], |row| row.get(0))?;
        ensure!(
            sequence == expected_sequence,
            "stale import preview; preview again"
        );
        let result = apply_checked(&tx, repo, incoming, resolutions, validate)?;
        tx.commit()?;
        Ok(result)
    }
}

pub(super) fn apply_checked(
    connection: &Connection,
    repo: &RepositoryId,
    incoming: &ExchangeSnapshot,
    resolutions: &BTreeMap<String, ConflictResolution>,
    validate: impl FnOnce(&ExchangeSnapshot) -> Result<()>,
) -> Result<ImportResult> {
    let (preview, candidates) = plan(connection, repo, incoming, resolutions)?;
    ensure!(
        preview.conflicts.is_empty(),
        "concurrent record conflicts require resolution: {}",
        preview.conflicts.join(", ")
    );
    validate(&merged_snapshot(connection, repo, incoming, &candidates)?)?;
    for candidate in &candidates {
        connection.execute(
            "UPDATE documents SET locator=?1 WHERE id=?2 AND repo_id=?3",
            params![
                format!("__exchange_{}", uuid::Uuid::new_v4()),
                candidate.id,
                repo.0
            ],
        )?;
    }
    for candidate in &candidates {
        persist_import(connection, repo, candidate)?;
    }
    let mut authority_changes = 0;
    for kind in &incoming.kinds {
        authority_changes += connection.execute(
            "INSERT OR IGNORE INTO authority(repo_id,kind) VALUES (?1,?2)",
            params![repo.0, kind],
        )?;
    }
    if !candidates.is_empty() || authority_changes > 0 {
        connection.execute("UPDATE metadata SET sequence=sequence+1", [])?;
    }
    Ok(ImportResult {
        changed_records: candidates.len(),
        sequence: connection.query_row("SELECT sequence FROM metadata", [], |row| row.get(0))?,
    })
}

fn merged_snapshot(
    connection: &Connection,
    repo: &RepositoryId,
    incoming: &ExchangeSnapshot,
    candidates: &[ExchangeRecord],
) -> Result<ExchangeSnapshot> {
    let mut merged = snapshot(connection, repo)?;
    let mut records: BTreeMap<_, _> = merged
        .records
        .into_iter()
        .map(|record| (record.id.clone(), record))
        .collect();
    for candidate in candidates {
        records.insert(candidate.id.clone(), candidate.clone());
    }
    merged.records = records.into_values().collect();
    merged.kinds.extend(incoming.kinds.clone());
    merged.kinds.sort();
    merged.kinds.dedup();
    merged.refresh_digest()?;
    merged.validate()?;
    Ok(merged)
}

fn plan(
    connection: &Connection,
    repo: &RepositoryId,
    incoming: &ExchangeSnapshot,
    resolutions: &BTreeMap<String, ConflictResolution>,
) -> Result<(ImportPreview, Vec<ExchangeRecord>)> {
    ensure!(
        repo == &incoming.repo_id,
        "exchange belongs to another repository"
    );
    let local = snapshot(connection, repo)?;
    ensure!(
        local.epoch_id == incoming.epoch_id,
        "exchange epoch differs; explicit reconciliation required"
    );
    let local_by_id: BTreeMap<_, _> = local
        .records
        .iter()
        .map(|record| (&record.id, record))
        .collect();
    let mut conflicts = Vec::new();
    let mut candidates = Vec::new();
    let mut used_resolutions = std::collections::BTreeSet::new();
    for incoming_record in &incoming.records {
        let Some(local_record) = local_by_id.get(&incoming_record.id) else {
            let mut record = incoming_record.clone();
            if let Some(ConflictResolution::RelocateIncoming { locator, content }) =
                resolutions.get(&record.id)
            {
                ensure!(
                    super::super::content_is_understood(&record.kind, record.content_version),
                    "cannot relocate unsupported opaque content"
                );
                super::super::validate_locator(&record.kind, locator)?;
                record.locator = locator.clone();
                record.content = ExchangeContent::from_bytes(content);
                record.content.bytes()?;
                clocks::increment(connection, &mut record.clock)?;
                used_resolutions.insert(record.id.clone());
            }
            candidates.push(record);
            continue;
        };
        match merge_record(
            connection,
            local_record,
            incoming_record,
            resolutions.get(&incoming_record.id),
        )? {
            Merge::Keep => {}
            Merge::Update(record, resolved) => {
                if resolved {
                    used_resolutions.insert(record.id.clone());
                }
                candidates.push(record);
            }
            Merge::Conflict => conflicts.push(incoming_record.id.clone()),
        }
    }
    resolve_locator_collisions(
        connection,
        &local.records,
        &mut candidates,
        resolutions,
        &mut used_resolutions,
        &mut conflicts,
    )?;
    ensure!(
        resolutions.keys().all(|key| used_resolutions.contains(key)),
        "resolution targets a record without a concurrent conflict"
    );
    validate_locators(&local.records, &candidates)?;
    let preview = ImportPreview {
        expected_sequence: connection
            .query_row("SELECT sequence FROM metadata", [], |row| row.get(0))?,
        incoming_digest: incoming.digest.clone(),
        conflicts,
        changed_records: candidates.len(),
        validation_errors: Vec::new(),
        candidate_record_ids: candidates.iter().map(|record| record.id.clone()).collect(),
    };
    Ok((preview, candidates))
}

enum Merge {
    Keep,
    Update(ExchangeRecord, bool),
    Conflict,
}

fn merge_record(
    connection: &Connection,
    local: &ExchangeRecord,
    incoming: &ExchangeRecord,
    resolution: Option<&ConflictResolution>,
) -> Result<Merge> {
    ensure!(
        local.kind == incoming.kind,
        "record kind cannot change for {}",
        local.id
    );
    let local_ahead = clocks::dominates(&local.clock, &incoming.clock);
    let incoming_ahead = clocks::dominates(&incoming.clock, &local.clock);
    let equal = same_content(local, incoming)?;
    if local_ahead && incoming_ahead {
        ensure!(
            equal,
            "equal causal clocks carry divergent content for {}",
            local.id
        );
        return Ok(Merge::Keep);
    }
    if local_ahead {
        return Ok(Merge::Keep);
    }
    if incoming_ahead {
        return Ok(Merge::Update(incoming.clone(), false));
    }
    if equal {
        let mut record = local.clone();
        record.clock = clocks::join(&local.clock, &incoming.clock);
        clocks::validate(&record.clock)?;
        return Ok(Merge::Update(record, false));
    }
    let Some(resolution) = resolution else {
        return Ok(Merge::Conflict);
    };
    ensure!(
        super::super::content_is_understood(&local.kind, local.content_version)
            && super::super::content_is_understood(&incoming.kind, incoming.content_version),
        "cannot edit or resolve unsupported opaque content; use a compatible client"
    );
    let mut record = match resolution {
        ConflictResolution::Local => local.clone(),
        ConflictResolution::Incoming => incoming.clone(),
        ConflictResolution::RelocateIncoming { locator, content } => {
            let mut record = incoming.clone();
            record.locator = locator.clone();
            record.content = ExchangeContent::from_bytes(content);
            record
        }
        ConflictResolution::Custom { content, deleted } => {
            let mut record = local.clone();
            record.content = ExchangeContent::from_bytes(content);
            record.deleted = *deleted;
            record
        }
    };
    record.clock = clocks::join(&local.clock, &incoming.clock);
    clocks::increment(connection, &mut record.clock)?;
    record.content.bytes()?;
    Ok(Merge::Update(record, true))
}

fn same_content(left: &ExchangeRecord, right: &ExchangeRecord) -> Result<bool> {
    Ok(left.kind == right.kind
        && left.locator == right.locator
        && left.deleted == right.deleted
        && left.content_version == right.content_version
        && left.content.bytes()? == right.content.bytes()?)
}

fn resolve_locator_collisions(
    connection: &Connection,
    local: &[ExchangeRecord],
    candidates: &mut Vec<ExchangeRecord>,
    resolutions: &BTreeMap<String, ConflictResolution>,
    used: &mut std::collections::BTreeSet<String>,
    conflicts: &mut Vec<String>,
) -> Result<()> {
    let mut merged: BTreeMap<_, _> = local
        .iter()
        .map(|record| (record.id.clone(), record.clone()))
        .collect();
    for record in candidates.iter() {
        merged.insert(record.id.clone(), record.clone());
    }
    let mut counts = BTreeMap::new();
    for record in merged.values() {
        *counts
            .entry((record.kind.clone(), record.locator.clone()))
            .or_insert(0usize) += 1;
    }
    let mut accepted = Vec::new();
    for mut record in std::mem::take(candidates) {
        if counts[&(record.kind.clone(), record.locator.clone())] > 1 {
            if let Some(ConflictResolution::RelocateIncoming { locator, content }) =
                resolutions.get(&record.id)
            {
                ensure!(
                    super::super::content_is_understood(&record.kind, record.content_version),
                    "cannot relocate unsupported opaque content"
                );
                record.locator = locator.clone();
                super::super::validate_locator(&record.kind, &record.locator)?;
                record.content = ExchangeContent::from_bytes(content);
                record.content.bytes()?;
                clocks::increment(connection, &mut record.clock)?;
                used.insert(record.id.clone());
            } else {
                conflicts.push(record.id.clone());
                continue;
            }
        }
        accepted.push(record);
    }
    *candidates = accepted;
    Ok(())
}

fn validate_locators(local: &[ExchangeRecord], candidates: &[ExchangeRecord]) -> Result<()> {
    let mut merged: BTreeMap<_, _> = local.iter().map(|record| (&record.id, record)).collect();
    for record in candidates {
        merged.insert(&record.id, record);
    }
    let mut locators = std::collections::BTreeSet::new();
    ensure!(
        merged.len() <= crate::storage::MAX_DOCUMENTS,
        "merged snapshot exceeds record limit"
    );
    for record in merged.values() {
        ensure!(
            locators.insert((&record.kind, &record.locator)),
            "record locator collision: {}:{}",
            record.kind,
            record.locator
        );
    }
    Ok(())
}

fn persist_import(
    connection: &Connection,
    repo: &RepositoryId,
    record: &ExchangeRecord,
) -> Result<()> {
    let old: Option<(String, u64)> = connection
        .query_row(
            "SELECT repo_id,revision FROM documents WHERE id=?1",
            [&record.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    ensure!(
        old.as_ref().is_none_or(|old| old.0 == repo.0),
        "record ID belongs to another repository"
    );
    let revision = old.map_or(Ok(1), |old| {
        old.1.checked_add(1).context("document revision exhausted")
    })?;
    let bytes = record.content.bytes()?;
    connection.execute("INSERT INTO documents(id,repo_id,kind,locator,revision,content,deleted,content_version) VALUES (?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET locator=excluded.locator,revision=excluded.revision,content=excluded.content,deleted=excluded.deleted,content_version=excluded.content_version",
        params![record.id,repo.0,record.kind,record.locator,revision,bytes,record.deleted,record.content_version])?;
    connection.execute(
        "INSERT INTO revisions(record_id,revision,content,deleted,content_version) VALUES (?1,?2,?3,?4,?5)",
        params![record.id, revision, bytes, record.deleted, record.content_version],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO document_locators(repo_id,kind,locator) VALUES (?1,?2,?3)",
        params![repo.0, record.kind, record.locator],
    )?;
    clocks::save(connection, &record.id, &record.clock)
}
