//! Recovery equality includes every durable table, except the rotated replica identity.
use super::Store;
use anyhow::{ensure, Result};
use rusqlite::types::ValueRef;
use sha2::{Digest, Sha256};

const QUERIES: [(&str, &str); 10] = [
    ("authority", "SELECT * FROM authority ORDER BY repo_id,kind"),
    ("bindings", "SELECT * FROM bindings ORDER BY binding"),
    ("clocks", "SELECT * FROM clocks ORDER BY record_id"),
    (
        "document_locators",
        "SELECT * FROM document_locators ORDER BY repo_id,kind,locator",
    ),
    ("documents", "SELECT * FROM documents ORDER BY id"),
    ("metadata", "SELECT sequence FROM metadata"),
    (
        "migration_sources",
        "SELECT * FROM migration_sources ORDER BY repo_id,kind,locator,source_path",
    ),
    (
        "repositories",
        "SELECT * FROM repositories ORDER BY repo_id",
    ),
    (
        "revisions",
        "SELECT * FROM revisions ORDER BY record_id,revision",
    ),
    (
        "workspace_ordering",
        "SELECT * FROM workspace_ordering ORDER BY singleton",
    ),
];

impl Store {
    /// Hashes typed, length-delimited cells from a single SQLite read snapshot.
    pub fn recovery_fingerprint(&self) -> Result<String> {
        let tx = self.connection.unchecked_transaction()?;
        let tables = tx.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?
            .query_map([], |row| row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(
            tables
                .iter()
                .map(String::as_str)
                .eq(QUERIES.iter().map(|(name, _)| *name)),
            "recovery fingerprint requires an updated table inventory"
        );
        let mut hash = Sha256::new();
        hash.update(b"switchbard-recovery-v1");
        for (name, query) in QUERIES {
            hash_bytes(&mut hash, b'T', name.as_bytes());
            let mut statement = tx.prepare(query)?;
            let columns = statement.column_count();
            let mut rows = statement.query([])?;
            let mut exhausted = false;
            for _ in 0..10_000_000 {
                let Some(row) = rows.next()? else {
                    exhausted = true;
                    break;
                };
                hash.update(b"R");
                hash.update((columns as u64).to_le_bytes());
                for column in 0..columns {
                    hash_value(&mut hash, row.get_ref(column)?);
                }
            }
            ensure!(exhausted, "recovery table exceeds verification row limit");
        }
        tx.commit()?;
        Ok(format!("{:x}", hash.finalize()))
    }
}

fn hash_value(hash: &mut Sha256, value: ValueRef<'_>) {
    match value {
        ValueRef::Null => hash_bytes(hash, b'N', &[]),
        ValueRef::Integer(value) => hash_bytes(hash, b'I', &value.to_le_bytes()),
        ValueRef::Real(value) => hash_bytes(hash, b'F', &value.to_bits().to_le_bytes()),
        ValueRef::Text(value) => hash_bytes(hash, b'T', value),
        ValueRef::Blob(value) => hash_bytes(hash, b'B', value),
    }
}

fn hash_bytes(hash: &mut Sha256, kind: u8, bytes: &[u8]) {
    hash.update([kind]);
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
