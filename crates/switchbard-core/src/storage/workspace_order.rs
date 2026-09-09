//! Machine-local workspace ordering, deliberately outside repository exchange.
//! Original YAML and unresolved locators remain intact; known targets use stable IDs.
use super::{list_documents, RepositoryId, Store, MAX_DOCUMENTS, MAX_DOCUMENT_BYTES};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceOrderTarget {
    pub repository_id: RepositoryId,
    pub record_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceOrderEntry {
    pub locator: String,
    pub target: Option<WorkspaceOrderTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOrdering {
    pub source_path: PathBuf,
    pub content: Vec<u8>,
    pub entries: Vec<WorkspaceOrderEntry>,
}

#[derive(serde::Deserialize)]
struct OrderingYaml {
    #[serde(default)]
    ranked: Vec<String>,
}

/// Digest of the exact bounded source bytes, for a preview-to-cutover check.
pub fn workspace_ordering_source_digest(source: &Path) -> Result<String> {
    Ok(content_digest(&read_source(source)?))
}

/// One coherent source snapshot for preview output and its exact digest.
pub fn preview_workspace_ordering(source: &Path) -> Result<(Vec<u8>, String)> {
    let content = read_source(source)?;
    parse(&content)?;
    let digest = content_digest(&content);
    Ok((content, digest))
}

impl Store {
    pub fn workspace_ordering(&self) -> Result<Option<WorkspaceOrdering>> {
        load(&self.connection)
    }

    pub fn migrate_workspace_ordering_checked(
        &mut self,
        source: &Path,
        repositories: &[(String, PathBuf)],
        expected_content_digest: &str,
    ) -> Result<WorkspaceOrdering> {
        let source = source
            .canonicalize()
            .context("resolve ordering.yml source")?;
        let content = read_source(&source)?;
        ensure!(
            content_digest(&content) == expected_content_digest,
            "ordering.yml changed since preview; inspect again before migrating"
        );
        let ranked = parse(&content)?;
        let bindings = bindings(self, repositories)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous = load(&tx)?;
        let entries = resolve(
            &tx,
            ranked,
            &bindings,
            previous
                .as_ref()
                .map(|old| old.entries.as_slice())
                .unwrap_or_default(),
        )?;
        ensure!(
            content_digest(&read_source(&source)?) == expected_content_digest,
            "ordering.yml changed during migration; retry"
        );
        if let Some(previous) = previous {
            ensure!(previous.content == content && previous.entries == entries, "workspace ordering already migrated; refusing to replace central state with a source file");
        } else {
            tx.execute("INSERT INTO workspace_ordering(singleton,source_path,content,entries_json) VALUES(1,?1,?2,?3)", params![source.to_str().context("non-UTF-8 ordering source path")?, content, serde_json::to_string(&entries)?])?;
            tx.execute("UPDATE metadata SET sequence=sequence+1", [])?;
        }
        tx.commit()?;
        self.workspace_ordering()?
            .context("workspace ordering migration did not persist")
    }

    /// Replace the central raw document with revision protection. Unchanged
    /// textual locators retain their resolved stable target across task renames.
    pub fn replace_workspace_ordering(
        &mut self,
        content: &[u8],
        repositories: &[(String, PathBuf)],
        expected_sequence: u64,
    ) -> Result<WorkspaceOrdering> {
        let ranked = parse(content)?;
        let bindings = bindings(self, repositories)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sequence: u64 = tx.query_row("SELECT sequence FROM metadata", [], |row| row.get(0))?;
        ensure!(
            sequence == expected_sequence,
            "workspace state changed; reload ordering and retry"
        );
        let old = load(&tx)?.context("workspace ordering has not been migrated")?;
        let entries = resolve(&tx, ranked, &bindings, &old.entries)?;
        if old.content != content || old.entries != entries {
            tx.execute(
                "UPDATE workspace_ordering SET content=?1,entries_json=?2 WHERE singleton=1",
                params![content, serde_json::to_string(&entries)?],
            )?;
            tx.execute("UPDATE metadata SET sequence=sequence+1", [])?;
        }
        tx.commit()?;
        self.workspace_ordering()?
            .context("workspace ordering disappeared")
    }

    #[cfg(test)]
    fn migrate_workspace_ordering(
        &mut self,
        source: &Path,
        repositories: &[(String, PathBuf)],
    ) -> Result<WorkspaceOrdering> {
        self.migrate_workspace_ordering_checked(
            source,
            repositories,
            &workspace_ordering_source_digest(source)?,
        )
    }
}

fn load(connection: &rusqlite::Connection) -> Result<Option<WorkspaceOrdering>> {
    let stored: Option<(String, Vec<u8>, String)> = connection
        .query_row(
            "SELECT source_path,content,entries_json FROM workspace_ordering WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    stored
        .map(|(source_path, content, entries)| {
            let ranked = parse(&content)?;
            let entries: Vec<WorkspaceOrderEntry> = serde_json::from_str(&entries)
                .context("invalid stored workspace ordering references")?;
            ensure!(
                ranked.len() == entries.len()
                    && ranked
                        .iter()
                        .zip(&entries)
                        .all(|(locator, entry)| locator == &entry.locator),
                "stored workspace ordering references disagree with content"
            );
            Ok(WorkspaceOrdering {
                source_path: source_path.into(),
                content,
                entries,
            })
        })
        .transpose()
}

fn parse(content: &[u8]) -> Result<Vec<String>> {
    ensure!(
        content.len() <= MAX_DOCUMENT_BYTES,
        "ordering.yml exceeds size limit"
    );
    let yaml: OrderingYaml = serde_yaml::from_slice(content)
        .context("ordering.yml is malformed; resolve before editing")?;
    ensure!(
        yaml.ranked.len() <= MAX_DOCUMENTS,
        "workspace ordering count exceeds limit"
    );
    Ok(yaml.ranked)
}

fn read_source(source: &Path) -> Result<Vec<u8>> {
    let metadata = std::fs::metadata(source)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_DOCUMENT_BYTES as u64,
        "ordering.yml is not a bounded regular file"
    );
    let content = std::fs::read(source)?;
    ensure!(
        content.len() <= MAX_DOCUMENT_BYTES,
        "ordering.yml exceeds size limit"
    );
    Ok(content)
}

fn content_digest(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(content))
}

fn bindings(
    store: &Store,
    repositories: &[(String, PathBuf)],
) -> Result<Vec<(String, RepositoryId)>> {
    ensure!(
        repositories.len() <= MAX_DOCUMENTS,
        "tracked repository count exceeds limit"
    );
    repositories
        .iter()
        .map(|(name, root)| Ok(store.repository(root)?.map(|repo| (name.clone(), repo))))
        .collect::<Result<Vec<_>>>()
        .map(|items| items.into_iter().flatten().collect())
}

fn resolve(
    connection: &rusqlite::Connection,
    ranked: Vec<String>,
    bindings: &[(String, RepositoryId)],
    previous: &[WorkspaceOrderEntry],
) -> Result<Vec<WorkspaceOrderEntry>> {
    let mut tasks = std::collections::BTreeMap::<String, Option<WorkspaceOrderTarget>>::new();
    let mut count = 0usize;
    for (name, repo) in bindings {
        for doc in list_documents(connection, repo, "task")?
            .into_iter()
            .filter(|doc| {
                !doc.deleted && super::content_is_understood(&doc.kind, doc.content_version)
            })
        {
            let task = crate::backlog::parse_task_text(
                Path::new(&doc.locator),
                crate::BacklogTaskSource::Active,
                std::str::from_utf8(&doc.content)?,
            )?
            .0;
            let target = WorkspaceOrderTarget {
                repository_id: repo.clone(),
                record_id: doc.id,
            };
            tasks
                .entry(format!("{name}:{}", task.id))
                .and_modify(|known| {
                    if known.as_ref() != Some(&target) {
                        *known = None;
                    }
                })
                .or_insert(Some(target));
            count += 1;
            ensure!(count <= MAX_DOCUMENTS, "workspace task count exceeds limit");
        }
    }
    let retained = previous
        .iter()
        .filter_map(|entry| {
            entry
                .target
                .as_ref()
                .map(|target| (entry.locator.as_str(), target))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    Ok(ranked
        .into_iter()
        .map(|locator| {
            let target = retained
                .get(locator.as_str())
                .map(|target| (*target).clone())
                .or_else(|| tasks.get(&locator).cloned().flatten());
            WorkspaceOrderEntry { locator, target }
        })
        .collect())
}

#[cfg(test)]
mod tests;
