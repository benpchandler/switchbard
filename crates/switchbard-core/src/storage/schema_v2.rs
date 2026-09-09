//! Upgrade the document envelope without projecting or rewriting payload bytes.
use anyhow::Result;
use rusqlite::{params, Connection};
use uuid::Uuid;

pub(super) fn upgrade(connection: &Connection) -> Result<()> {
    connection.execute_batch("PRAGMA foreign_keys=OFF; PRAGMA legacy_alter_table=ON;")?;
    let result = upgrade_transaction(connection);
    connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA legacy_alter_table=OFF;")?;
    result
}

fn upgrade_transaction(connection: &Connection) -> Result<()> {
    let tx = connection.unchecked_transaction()?;
    tx.execute_batch(include_str!("schema_v2.sql"))?;
    let replica = Uuid::new_v4().to_string();
    tx.execute("UPDATE metadata SET replica_id=?1", [&replica])?;
    let repos = tx
        .prepare("SELECT repo_id FROM repositories")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for repo in repos {
        tx.execute(
            "UPDATE repositories SET epoch_id=?1 WHERE repo_id=?2",
            params![Uuid::new_v4().to_string(), repo],
        )?;
    }
    let documents = tx
        .prepare("SELECT id,revision FROM documents")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, revision) in documents {
        let clock = serde_json::to_string(&std::collections::BTreeMap::from([(
            replica.clone(),
            revision,
        )]))?;
        tx.execute(
            "INSERT INTO clocks(record_id,clock) VALUES (?1,?2)",
            params![id, clock],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub(super) fn add_locator_history(connection: &Connection) -> Result<()> {
    connection.execute_batch("BEGIN IMMEDIATE;
        CREATE TABLE document_locators(repo_id TEXT NOT NULL REFERENCES repositories(repo_id),kind TEXT NOT NULL,locator TEXT NOT NULL,PRIMARY KEY(repo_id,kind,locator));
        INSERT INTO document_locators SELECT repo_id,kind,locator FROM documents;
        PRAGMA user_version=3; COMMIT;")?;
    Ok(())
}

pub(super) fn add_workspace_ordering(connection: &Connection) -> Result<()> {
    connection.execute_batch("BEGIN IMMEDIATE;
        CREATE TABLE workspace_ordering(singleton INTEGER PRIMARY KEY CHECK(singleton=1), source_path TEXT NOT NULL, content BLOB NOT NULL, entries_json TEXT NOT NULL);
        PRAGMA user_version=4; COMMIT;")?;
    Ok(())
}

pub(super) fn add_content_version(connection: &Connection) -> Result<()> {
    connection.execute_batch("BEGIN IMMEDIATE;
        ALTER TABLE documents ADD COLUMN content_version INTEGER NOT NULL DEFAULT 1 CHECK(content_version BETWEEN 1 AND 4294967295);
        ALTER TABLE revisions ADD COLUMN content_version INTEGER NOT NULL DEFAULT 1 CHECK(content_version BETWEEN 1 AND 4294967295);
        PRAGMA user_version=5; COMMIT;")?;
    Ok(())
}
