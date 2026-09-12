//! Explicit read-only inspection of source files retained after a cutover.
use super::{RepositoryId, Store, MAX_DOCUMENTS, MAX_DOCUMENT_BYTES};
use anyhow::{ensure, Result};
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacySourceState {
    Matched,
    Changed,
    Missing,
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LegacySourceDrift {
    pub kind: String,
    pub locator: String,
    pub path: PathBuf,
    pub state: LegacySourceState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Store {
    /// Compare retained source files with their original migration digests.
    /// This never imports, rewrites, or falls back to the source. Missing files
    /// can be intentional retirement and are reported rather than rejected.
    pub fn legacy_source_drift(&self, repo: &RepositoryId) -> Result<Vec<LegacySourceDrift>> {
        let mut statement = self.connection.prepare(
            "SELECT s.kind,s.locator,s.source_path,s.digest FROM migration_sources s
             JOIN authority a ON a.repo_id=s.repo_id AND a.kind=s.kind
             WHERE s.repo_id=?1 ORDER BY s.kind,s.source_path,s.locator LIMIT ?2",
        )?;
        let mut rows = statement.query(params![repo.0, MAX_DOCUMENTS as u64 + 1])?;
        let mut report = Vec::new();
        while let Some(row) = rows.next()? {
            ensure!(
                report.len() < MAX_DOCUMENTS,
                "retained source count exceeds inspection limit"
            );
            let kind = row.get(0)?;
            let locator = row.get(1)?;
            let path = PathBuf::from(row.get::<_, String>(2)?);
            let expected: String = row.get(3)?;
            ensure!(
                expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid retained source digest"
            );
            let (state, detail) = compare_source(&path, &expected);
            report.push(LegacySourceDrift {
                kind,
                locator,
                path,
                state,
                detail,
            });
        }
        Ok(report)
    }
}

fn compare_source(path: &Path, expected: &str) -> (LegacySourceState, Option<String>) {
    let read = || -> std::io::Result<Vec<u8>> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // A retained path may have been replaced with a FIFO by an old
            // client. Inspection must not wait for a writer to connect.
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(std::io::Error::other(
                "retained source is not a regular file",
            ));
        }
        let mut bytes = Vec::new();
        file.take(MAX_DOCUMENT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    match read() {
        Ok(bytes) if bytes.len() > MAX_DOCUMENT_BYTES => (
            LegacySourceState::Changed,
            Some("source exceeds document byte limit".into()),
        ),
        Ok(bytes) if format!("{:x}", Sha256::digest(&bytes)) == expected => {
            (LegacySourceState::Matched, None)
        }
        Ok(_) => (LegacySourceState::Changed, None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (LegacySourceState::Missing, None)
        }
        Err(error) => (LegacySourceState::Unreadable, Some(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{MigrationPlan, SourceDocument};

    #[test]
    fn retained_source_report_is_bounded_read_only_and_ignores_current_content() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        let path = root.join("source.md");
        std::fs::write(&path, b"original\n").unwrap();
        let mut store = Store::open(dir.path().join("db")).unwrap();
        let repo = store.bind_repository(&root).unwrap();
        let plan = MigrationPlan::capture(
            repo.clone(),
            vec!["initiative".into()],
            vec![SourceDocument {
                kind: "initiative".into(),
                locator: "source.md".into(),
                path: path.clone(),
            }],
        )
        .unwrap();
        store.apply_migration(&plan).unwrap();
        let _ = store
            .mutate(&repo, "initiative", "source.md", None, |_| {
                Ok(Some(b"central update".to_vec()))
            })
            .unwrap();
        let sequence = store.change_sequence().unwrap();
        let state = |store: &Store| store.legacy_source_drift(&repo).unwrap()[0].state.clone();
        assert_eq!(state(&store), LegacySourceState::Matched);
        std::fs::write(&path, b"external update\n").unwrap();
        assert_eq!(state(&store), LegacySourceState::Changed);
        std::fs::write(&path, vec![b'x'; MAX_DOCUMENT_BYTES + 1]).unwrap();
        assert_eq!(state(&store), LegacySourceState::Changed);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(state(&store), LegacySourceState::Missing);
        std::fs::create_dir(&path).unwrap();
        assert_eq!(state(&store), LegacySourceState::Unreadable);
        assert_eq!(store.change_sequence().unwrap(), sequence);
        assert_eq!(
            store
                .read(&repo, "initiative", "source.md")
                .unwrap()
                .unwrap()
                .content,
            b"central update"
        );
        store
            .connection
            .execute("DELETE FROM authority WHERE repo_id=?1", [&repo.0])
            .unwrap();
        assert!(store.legacy_source_drift(&repo).unwrap().is_empty());
    }
}
