//! Serialize cooperating exporters and preserve independently edited targets.
use anyhow::{ensure, Context, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub(super) struct ExportLock(PathBuf);

impl ExportLock {
    pub(super) fn acquire(path: &Path) -> Result<Self> {
        let parent = path.parent().context("exchange path needs a parent")?;
        std::fs::create_dir_all(parent)?;
        let lock = path.with_extension("json.export-lock");
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&lock)
            .with_context(|| format!("another export holds {}; if its recorded process has ended, remove this abandoned lock and retry", lock.display()))?;
        writeln!(file, "{}", std::process::id())?;
        file.sync_all()?;
        Ok(Self(lock))
    }
}

impl Drop for ExportLock {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0) {
            eprintln!(
                "sb: cannot release export lock {}: {error}",
                self.0.display()
            );
        }
    }
}

pub(super) fn existing_bytes(path: &Path) -> Result<Option<Vec<u8>>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "exchange file exceeds size limit"
    );
    Ok(Some(bytes))
}

pub(super) fn replace(path: &Path, expected: Option<&[u8]>, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        ensure!(
            existing_bytes(path)?.as_deref() == expected,
            "exchange file changed during export; import/reconcile and retry"
        );
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        if let Err(error) = std::fs::remove_file(&temporary) {
            eprintln!(
                "sb: cannot remove export temp {}: {error}",
                temporary.display()
            );
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn competing_export_and_independent_file_change_are_preserved() {
        let dir = tempfile::tempdir().expect("fixture");
        let path = dir.path().join("tasks.json");
        let lock = ExportLock::acquire(&path).expect("first writer");
        assert!(ExportLock::acquire(&path).is_err());
        std::fs::write(&path, b"independent edit").expect("external writer");
        assert!(replace(&path, None, b"our export").is_err());
        assert_eq!(std::fs::read(&path).expect("read"), b"independent edit");
        drop(lock);
        assert!(ExportLock::acquire(&path).is_ok());
    }
}
