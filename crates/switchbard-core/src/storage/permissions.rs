//! Privacy checks are owned here, not duplicated by each document consumer.
use anyhow::{ensure, Context, Result};
use std::fs::{self, OpenOptions};
use std::path::Path;

pub(super) fn prepare(path: &Path) -> Result<()> {
    let parent = path.parent().context("database needs a parent directory")?;
    if !parent.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(parent)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(file) => file.sync_all()?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    validate(path)?;
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        let sidecar = Path::new(&name);
        match fs::symlink_metadata(sidecar) {
            Ok(_) => validate(sidecar)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) fn validate(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "database path must be a regular, non-symlink file: {}",
        path.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // home metadata gives the effective user's expected ownership without unsafe libc.
        let owner =
            fs::metadata(dirs::home_dir().context("cannot determine home directory")?)?.uid();
        ensure!(
            metadata.uid() == owner && metadata.mode() & 0o077 == 0,
            "database must be owned by this user and mode 0600; repair {}",
            path.display()
        );
    }
    Ok(())
}
