//! Durable storage for the same encoded record carried through self-restarts.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::{decode, Restored};

const MAX_RECORD_BYTES: u64 = 1_048_576;

/// Per-repository session state, separate from deliberately saved view slots.
/// Unreadable records and externally changed records are preserved for recovery.
pub struct ResumeStore {
    path: Option<PathBuf>,
    source: Result<Option<String>, String>,
}

impl ResumeStore {
    pub fn load(repo_views: Option<&Path>) -> Self {
        let path = repo_views.map(|path| path.with_extension("resume"));
        let source = path.as_deref().map(read_record).unwrap_or(Ok(None));
        Self { path, source }
    }

    pub fn read(&self) -> Restored {
        match &self.source {
            Ok(source) => decode(source.as_deref()),
            Err(_) => Restored::Unreadable,
        }
    }

    pub fn block_writes(&mut self, reason: String) {
        self.source = Err(reason);
    }

    /// Persist only changed snapshots. Never replace unknown data with defaults.
    pub fn checkpoint(&mut self, state: &str) -> Result<(), String> {
        let Some(path) = self.path.as_deref() else {
            return Ok(());
        };
        let source = self.source.as_ref().map_err(Clone::clone)?;
        if matches!(decode(source.as_deref()), Restored::Unreadable) {
            return Err(format!(
                "preserved unreadable resume record: {}",
                path.display()
            ));
        }
        if source.as_deref() == Some(state) {
            return Ok(());
        }
        if state.len() as u64 > MAX_RECORD_BYTES
            || !matches!(decode(Some(state)), Restored::Record(_))
        {
            return Err("resume record is invalid or too large".into());
        }
        let _lock = lock_record(path)?;
        if read_record(path)?.as_ref() != source.as_ref() {
            return Err("resume record changed in another session; preserved it".into());
        }
        write_record(path, state)?;
        self.source = Ok(Some(state.to_owned()));
        Ok(())
    }
}

fn lock_record(path: &Path) -> Result<std::fs::File, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("resume.lock"))
        .map_err(|error| format!("could not open resume lock: {error}"))?;
    file.try_lock()
        .map_err(|error| format!("resume record busy: {error}"))?;
    Ok(file)
}

fn read_record(path: &Path) -> Result<Option<String>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("could not read {}: {error}", path.display())),
    };
    let mut text = String::new();
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    if text.len() as u64 > MAX_RECORD_BYTES {
        return Err(format!("resume record too large: {}", path.display()));
    }
    Ok(Some(text))
}

fn write_record(path: &Path, state: &str) -> Result<(), String> {
    let temporary = temporary_path(path);
    let mut created = false;
    let mut attempt = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        created = true;
        file.write_all(state.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    };
    let result = attempt();
    if result.is_err() && created {
        std::fs::remove_file(&temporary).map_err(|error| {
            format!("resume write failed and temporary cleanup failed: {error}")
        })?;
    }
    result.map_err(|error| format!("could not write {}: {error}", path.display()))
}

/// A previous crash may leave its temporary file behind, including after PID
/// reuse. Never claim that old file or let it prevent a new checkpoint.
fn temporary_path(path: &Path) -> PathBuf {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    path.with_extension(format!(
        "resume.{}.{timestamp}.{sequence}.tmp",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::resume::ResumeRecord;

    #[test]
    fn cold_resume_is_repo_scoped_and_never_changes_saved_views() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let views = dir.path().join("repo.lua");
        std::fs::write(&views, "deliberate default").expect("saved views");
        let record = ResumeRecord {
            task_slot: 3,
            task_view: "{filter='mine'}".into(),
            ..ResumeRecord::default()
        };
        let mut store = ResumeStore::load(Some(&views));
        assert_eq!(store.read(), Restored::Absent);
        store.checkpoint(&record.encode()).expect("checkpoint");
        assert_eq!(
            ResumeStore::load(Some(&views)).read(),
            Restored::Record(record)
        );
        assert_eq!(
            std::fs::read_to_string(&views).expect("saved views"),
            "deliberate default"
        );
        assert_eq!(
            ResumeStore::load(Some(&dir.path().join("other.lua"))).read(),
            Restored::Absent
        );
    }

    #[test]
    fn unreadable_and_oversized_sources_are_preserved() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let views = dir.path().join("repo.lua");
        let path = views.with_extension("resume");
        for source in [
            "sbt-resume-9={}".to_string(),
            "x".repeat(MAX_RECORD_BYTES as usize + 1),
        ] {
            std::fs::write(&path, &source).expect("broken record");
            let mut store = ResumeStore::load(Some(&views));
            assert_eq!(store.read(), Restored::Unreadable);
            assert!(store.checkpoint(&ResumeRecord::default().encode()).is_err());
            assert_eq!(
                std::fs::read_to_string(&path).expect("retained source"),
                source
            );
        }
    }

    #[test]
    fn a_concurrent_session_cannot_silently_replace_a_newer_record() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let views = dir.path().join("repo.lua");
        let mut first = ResumeStore::load(Some(&views));
        let mut second = ResumeStore::load(Some(&views));
        let first_state = ResumeRecord::default().encode();
        first.checkpoint(&first_state).expect("first checkpoint");
        let second_state = ResumeRecord {
            task_slot: 2,
            ..ResumeRecord::default()
        }
        .encode();
        assert!(second.checkpoint(&second_state).is_err());
        assert_eq!(
            std::fs::read_to_string(views.with_extension("resume")).expect("record"),
            first_state
        );
    }

    #[test]
    fn busy_record_fails_without_waiting_and_can_retry_after_release() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let views = dir.path().join("repo.lua");
        let guard = lock_record(&views.with_extension("resume")).expect("first writer");
        let mut store = ResumeStore::load(Some(&views));
        let state = ResumeRecord::default().encode();
        assert!(store.checkpoint(&state).expect_err("busy").contains("busy"));
        drop(guard);
        store.checkpoint(&state).expect("retry");
        assert!(matches!(store.read(), Restored::Record(_)));
    }

    #[test]
    fn stale_temporary_file_from_reused_pid_is_preserved_without_blocking_save() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let views = dir.path().join("repo.lua");
        let stale = views.with_extension(format!("resume.{}.tmp", std::process::id()));
        std::fs::write(&stale, "interrupted checkpoint").expect("stale temporary");
        let mut store = ResumeStore::load(Some(&views));
        let record = ResumeRecord::default();
        store.checkpoint(&record.encode()).expect("new checkpoint");
        assert_eq!(
            ResumeStore::load(Some(&views)).read(),
            Restored::Record(record)
        );
        assert_eq!(
            std::fs::read_to_string(stale).expect("preserved stale file"),
            "interrupted checkpoint"
        );
    }
}
