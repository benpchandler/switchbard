//! Read-only health inspection; never creates, upgrades, or claims database authority.
use super::{established, permissions, Connection, Store, APPLICATION_ID, SCHEMA_VERSION};
use anyhow::{ensure, Result};
use rusqlite::OpenFlags;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceInspection {
    Unconfigured,
    Registered,
    Central,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseInspection {
    Missing,
    Empty,
    UpgradeRequired,
    WalInspectionSkipped,
    Ready(WorkspaceInspection),
}

/// Inspect the selected store without changing its bytes, permissions, or marker.
/// The owning storage layer retains all identity and establishment checks.
pub fn inspect_database(path: &Path, root: Option<&Path>) -> Result<DatabaseInspection> {
    established::check(path)?;
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DatabaseInspection::Missing);
        }
        Err(error) => return Err(error.into()),
        Ok(_) => permissions::validate(path)?,
    }
    // SQLite's read-only WAL access can still create a shared-memory sidecar.
    // Inspect the persisted header before opening and refuse that mode rather
    // than ignoring a potentially active WAL with immutable=1.
    use std::io::Read;
    let mut header = [0_u8; 20];
    let size = std::fs::File::open(path)?.read(&mut header)?;
    if size == header.len() && (header[18] == 2 || header[19] == 2) {
        return Ok(DatabaseInspection::WalInspectionSkipped);
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_secs(1))?;
    check_integrity(&connection)?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    ensure!(
        (0..=SCHEMA_VERSION).contains(&version),
        "unsupported database schema"
    );
    if version == 0 {
        let tables: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
            [],
            |row| row.get(0),
        )?;
        ensure!(tables == 0, "unrecognized nonempty database");
        return Ok(DatabaseInspection::Empty);
    }
    let application: i64 = connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    ensure!(application == APPLICATION_ID, "not a Switchbard database");
    if version < SCHEMA_VERSION {
        return Ok(DatabaseInspection::UpgradeRequired);
    }
    let store = Store {
        connection,
        path: path.to_path_buf(),
    };
    store.change_sequence()?;
    let workspace = match root
        .map(|root| store.repository(root))
        .transpose()?
        .flatten()
    {
        None => WorkspaceInspection::Unconfigured,
        Some(repo) if store.workspace_is_central(&repo)? => WorkspaceInspection::Central,
        Some(_) => WorkspaceInspection::Registered,
    };
    Ok(DatabaseInspection::Ready(workspace))
}

fn check_integrity(connection: &Connection) -> Result<()> {
    let interrupt = connection.get_interrupt_handle();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let timer = std::thread::spawn(move || {
        if receiver.recv_timeout(Duration::from_secs(5)).is_err() {
            interrupt.interrupt();
        }
    });
    let result: rusqlite::Result<String> =
        connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0));
    let _completed = sender.send(());
    ensure!(timer.join().is_ok(), "database inspection timer failed");
    ensure!(result? == "ok", "database integrity check failed");
    Ok(())
}
