//! Adjacent operational marker prevents a lost database from restoring legacy authority.
use anyhow::{ensure, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const CONTENT: &[u8] = b"switchbard-document-database-established-v1\n";

fn marker_path(database: &Path) -> PathBuf {
    let mut path = database.as_os_str().to_os_string();
    path.push(".established");
    path.into()
}

pub(super) fn check(database: &Path) -> Result<()> {
    let marker = marker_path(database);
    match fs::symlink_metadata(&marker) {
        Ok(_) => {
            super::permissions::validate(&marker)?;
            ensure!(
                fs::metadata(&marker)?.len() == CONTENT.len() as u64,
                "invalid database establishment marker size"
            );
            ensure!(
                fs::read(&marker)? == CONTENT,
                "invalid database establishment marker"
            );
            ensure!(fs::symlink_metadata(database).is_ok(),
                "established Switchbard database is missing; restore its backup before continuing: {}", database.display());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

pub(super) fn record(database: &Path) -> Result<()> {
    let marker = marker_path(database);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&marker) {
        Ok(mut file) => {
            file.write_all(CONTENT)?;
            file.sync_all()?;
            if let Some(parent) = marker.parent() {
                fs::File::open(parent)?.sync_all()?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => check(database)?,
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
