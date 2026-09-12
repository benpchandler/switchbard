//! Explicit consistent backup and restore to a new database, never over current work.
use anyhow::{ensure, Context, Result};
use clap::Args;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use switchbard_core::storage::{RepositoryId, Store};

#[derive(Args)]
pub struct BackupArgs {
    /// New backup file. Existing files are never replaced.
    #[arg(long)]
    file: PathBuf,
}

#[derive(Args)]
pub struct RestoreArgs {
    /// A consistent backup file, preserved unchanged. --database must name a new destination.
    #[arg(long)]
    from: PathBuf,
}

#[derive(Args)]
pub struct RebindArgs {
    /// Existing central repository UUID whose checkout has moved.
    #[arg(long)]
    repo_id: String,
}

pub fn backup(_root: &Path, database: &Path, args: &BackupArgs) -> Result<()> {
    let store = Store::open_existing(database)?.context("no central database to back up")?;
    let before = store.recovery_fingerprint()?;
    store.backup_to(&args.file)?;
    let copy = Store::open_existing(&args.file)?.context("backup disappeared")?;
    let fingerprint = copy.recovery_fingerprint()?;
    let after = store.recovery_fingerprint()?;
    ensure!(before == fingerprint && after == fingerprint,
        "database changed while verifying backup; the consistent backup exists, retry verification during a quiet moment");
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"status":"backup verified","file":args.file,"fingerprint":fingerprint})
        )?
    );
    Ok(())
}

pub fn restore(_root: &Path, database: &Path, args: &RestoreArgs) -> Result<()> {
    require_new_destination(database)?;
    reject_active_source(&args.from)?;
    copy_backup(&args.from, database)?;
    ensure!(
        files_equal(&args.from, database)?,
        "backup changed while copying; destination is not verified"
    );
    let mut restored = Store::open_existing(database)?.context("restored database disappeared")?;
    let before = restored.recovery_fingerprint()?;
    let replica = restored.rotate_replica()?;
    ensure!(
        restored.recovery_fingerprint()? == before,
        "restored content changed during replica rotation"
    );
    drop(restored);
    let reopened = Store::open_existing(database)?.context("restored database disappeared")?;
    ensure!(
        reopened.recovery_fingerprint()? == before,
        "restored data failed reopen verification"
    );
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"status":"restored backup to new database","database":database,"fingerprint":before,"replica_id":replica,"scope":"exact backup state; later source-database edits are not included"})
        )?
    );
    Ok(())
}

pub fn rebind(root: &Path, database: &Path, args: &RebindArgs) -> Result<()> {
    let mut store = Store::open_existing(database)?.context("no central database")?;
    let repo = RepositoryId(args.repo_id.clone());
    store.rebind_repository(&repo, root)?;
    ensure!(
        store.repository(root)? == Some(repo.clone()),
        "repository rebind verification failed"
    );
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"status":"repository rebound","repo_id":repo,"root":root})
        )?
    );
    Ok(())
}

fn require_new_destination(path: &Path) -> Result<()> {
    ensure!(
        fs::symlink_metadata(path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "restore destination already exists or is inaccessible"
    );
    let mut marker = path.as_os_str().to_os_string();
    marker.push(".established");
    ensure!(
        fs::symlink_metadata(Path::new(&marker))
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "restore requires a new destination without an existing establishment marker"
    );
    Ok(())
}

fn reject_active_source(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "backup source must be a regular file"
    );
    let mut header = [0u8; 16];
    File::open(path)?
        .read_exact(&mut header)
        .context("backup is not a complete SQLite file")?;
    ensure!(
        &header == b"SQLite format 3\0",
        "backup does not have a SQLite header"
    );
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        ensure!(
            fs::symlink_metadata(Path::new(&sidecar))
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            "source has SQLite sidecars; use storage backup to create a consistent source first"
        );
    }
    Ok(())
}

fn copy_backup(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("restore destination needs a parent")?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(parent)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut destination_file = options.open(destination)?;
    let mut source_file = File::open(source)?;
    let size = source_file.metadata()?.len();
    ensure!(
        size <= 64 * 1024 * 1024 * 1024,
        "backup exceeds 64 GiB restore limit"
    );
    let copied = std::io::copy(
        &mut std::io::Read::by_ref(&mut source_file).take(size + 1),
        &mut destination_file,
    )?;
    ensure!(copied == size, "backup size changed during copy");
    destination_file.flush()?;
    destination_file.sync_all()?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn files_equal(left: &Path, right: &Path) -> Result<bool> {
    let mut left = File::open(left)?;
    let mut right = File::open(right)?;
    let size = left.metadata()?.len();
    if size != right.metadata()?.len() {
        return Ok(false);
    }
    let mut a = [0u8; 64 * 1024];
    let mut b = [0u8; 64 * 1024];
    for _ in 0..=(size / a.len() as u64 + 1) {
        let count = left.read(&mut a)?;
        if count == 0 {
            return Ok(true);
        }
        right.read_exact(&mut b[..count])?;
        if a[..count] != b[..count] {
            return Ok(false);
        }
    }
    Ok(false)
}
