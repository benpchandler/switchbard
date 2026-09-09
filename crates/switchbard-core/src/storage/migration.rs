//! Explicit per-kind cutover with source-byte preservation in the same commit.
use super::{
    persist_change, validate_locator, RepositoryId, Store, MAX_DOCUMENTS, MAX_DOCUMENT_BYTES,
};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SourceDocument {
    pub kind: String,
    pub locator: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
struct CapturedSource {
    source: SourceDocument,
    bytes: Vec<u8>,
    digest: String,
}

#[derive(Debug, Clone)]
pub struct MigrationPlan {
    repo: RepositoryId,
    kinds: Vec<String>,
    sources: Vec<CapturedSource>,
    selected: Vec<SelectedDocument>,
    lock_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct SelectedDocument {
    pub kind: String,
    pub locator: String,
    pub bytes: Vec<u8>,
}

impl MigrationPlan {
    /// Caller inventories all relevant worktrees and passes even absent kinds explicitly.
    pub fn capture(
        repo: RepositoryId,
        kinds: Vec<String>,
        sources: Vec<SourceDocument>,
    ) -> Result<Self> {
        ensure!(
            !kinds.is_empty() && kinds.len() <= 128,
            "invalid migration kinds"
        );
        ensure!(
            sources.len() <= MAX_DOCUMENTS,
            "migration source count exceeds limit"
        );
        for kind in &kinds {
            validate_locator(kind, "migration")?;
        }
        let sources = sources
            .into_iter()
            .map(|source| {
                validate_locator(&source.kind, &source.locator)?;
                ensure!(
                    kinds.contains(&source.kind),
                    "source kind is outside migration scope"
                );
                let bytes = read_source(&source.path)?;
                let digest = digest(&bytes);
                Ok(CapturedSource {
                    source,
                    bytes,
                    digest,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        validate_duplicates(&sources)?;
        let mut selected = BTreeMap::new();
        for source in &sources {
            selected
                .entry((source.source.kind.clone(), source.source.locator.clone()))
                .or_insert_with(|| SelectedDocument {
                    kind: source.source.kind.clone(),
                    locator: source.source.locator.clone(),
                    bytes: source.bytes.clone(),
                });
        }
        Ok(Self {
            repo,
            kinds,
            sources,
            selected: selected.into_values().collect(),
            lock_root: None,
        })
    }

    /// Only the repository inventory's evidence-based reconciliation calls this.
    /// Each selection must match a preserved source or its narrowly verified fused-fence repair.
    pub(crate) fn capture_selected(
        repo: RepositoryId,
        kinds: Vec<String>,
        sources: Vec<(SourceDocument, Vec<u8>)>,
        selected: Vec<SelectedDocument>,
    ) -> Result<Self> {
        ensure!(
            !kinds.is_empty() && kinds.len() <= 128,
            "invalid migration kinds"
        );
        ensure!(
            sources.len() <= MAX_DOCUMENTS && selected.len() <= MAX_DOCUMENTS,
            "migration count exceeds limit"
        );
        for kind in &kinds {
            validate_locator(kind, "migration")?;
        }
        let mut captured = Vec::new();
        for (source, expected) in sources {
            validate_locator(&source.kind, &source.locator)?;
            ensure!(
                kinds.contains(&source.kind),
                "source outside migration kinds"
            );
            let bytes = read_source(&source.path)?;
            ensure!(
                bytes == expected,
                "migration source changed since inventory: {}",
                source.path.display()
            );
            captured.push(CapturedSource {
                source,
                digest: digest(&bytes),
                bytes,
            });
        }
        let mut available = BTreeMap::<(&str, &str), std::collections::BTreeSet<String>>::new();
        for source in &captured {
            available
                .entry((&source.source.kind, &source.source.locator))
                .or_default()
                .insert(source.digest.clone());
            if let Some((repaired, _)) = crate::backlog::migration_repairs::repair_selected(
                &source.source.kind,
                &source.source.locator,
                &source.bytes,
            )? {
                available
                    .entry((&source.source.kind, &source.source.locator))
                    .or_default()
                    .insert(digest(&repaired));
            }
        }
        let mut keys = std::collections::BTreeSet::new();
        for document in &selected {
            validate_locator(&document.kind, &document.locator)?;
            ensure!(
                keys.insert((&document.kind, &document.locator)),
                "duplicate selected migration document"
            );
            ensure!(
                available
                    .get(&(document.kind.as_str(), document.locator.as_str()))
                    .is_some_and(|digests| digests.contains(digest(&document.bytes).as_str())),
                "selected migration bytes do not match a preserved source"
            );
        }
        ensure!(
            captured
                .iter()
                .all(|source| keys.contains(&(&source.source.kind, &source.source.locator))),
            "preserved source has no selected document"
        );
        Ok(Self {
            repo,
            kinds,
            sources: captured,
            selected,
            lock_root: None,
        })
    }

    pub fn repository_id(&self) -> &RepositoryId {
        &self.repo
    }

    pub fn kinds(&self) -> &[String] {
        &self.kinds
    }

    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    pub fn with_lock_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.lock_root = Some(root.into());
        self
    }

    pub fn verify_sources(&self) -> Result<()> {
        for source in &self.sources {
            ensure!(
                digest(&read_source(&source.source.path)?) == source.digest,
                "migration source changed since preview: {}",
                source.source.path.display()
            );
        }
        Ok(())
    }
}

impl Store {
    /// Source files are deliberately not removed or rewritten. Original bytes and digests
    /// remain recoverable in migration_sources even after ordinary document mutations.
    /// This generic document API does not validate native task semantics. Native
    /// application boundaries must use `apply_migration_checked` with their validator.
    pub fn apply_migration(&mut self, plan: &MigrationPlan) -> Result<usize> {
        self.apply_migration_checked(plan, |_| Ok(()))
    }

    pub fn apply_migration_checked(
        &mut self,
        plan: &MigrationPlan,
        validate: impl FnOnce(&super::ExchangeSnapshot) -> Result<()>,
    ) -> Result<usize> {
        let lock_root = plan.lock_root.as_deref().or_else(|| {
            plan.sources.first().map(|source| source.source.path.as_path())
        });
        let lock_root = lock_root.context("migration requires repository lock identity")?;
        let _repository_lock = super::RepositoryLock::acquire(lock_root)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for kind in &plan.kinds {
            let active: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM authority WHERE repo_id=?1 AND kind=?2)",
                params![plan.repo.0, kind],
                |row| row.get(0),
            )?;
            ensure!(!active, "kind {kind} is already centrally authoritative");
        }
        plan.verify_sources()?;
        for source in &plan.sources {
            tx.execute("INSERT INTO migration_sources(repo_id,kind,locator,source_path,digest,content) VALUES (?1,?2,?3,?4,?5,?6)",
                params![plan.repo.0, source.source.kind, source.source.locator, source.source.path.to_str().context("non-UTF8 migration source")?, source.digest, source.bytes])?;
        }
        for document in &plan.selected {
            persist_change(
                &tx,
                &plan.repo,
                &document.kind,
                &document.locator,
                None,
                Some(document.bytes.clone()),
            )?;
        }
        for kind in &plan.kinds {
            tx.execute(
                "INSERT INTO authority(repo_id,kind) VALUES (?1,?2)",
                params![plan.repo.0, kind],
            )?;
        }
        tx.execute("UPDATE metadata SET sequence=sequence+1", [])?;
        validate(&super::exchange::snapshot(&tx, &plan.repo)?)?;
        tx.commit()?;
        Ok(plan.selected.len())
    }
}

fn validate_duplicates(sources: &[CapturedSource]) -> Result<()> {
    let mut unique = BTreeMap::new();
    for source in sources {
        let key = (&source.source.kind, &source.source.locator);
        if let Some(previous) = unique.insert(key, &source.digest) {
            ensure!(
                previous == &source.digest,
                "divergent copies of {}:{} require explicit reconciliation",
                key.0,
                key.1
            );
        }
    }
    Ok(())
}

fn read_source(path: &std::path::Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_DOCUMENT_BYTES,
        "migration document too large"
    );
    Ok(bytes)
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
