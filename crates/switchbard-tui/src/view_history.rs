//! Bounded, per-page arrangement history. Named slots remain a separate store.
//! Files are data-only canonical Lua records inside versioned JSON. Refuse writes
//! after malformed/future data or external changes, preserving the original file.
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::columns::ColumnRegistry;
use crate::page::Page;
use crate::views::ViewState;

pub const MAX_ENTRIES: usize = 1000;
pub const MAX_AGE_SECONDS: u64 = 30 * 24 * 60 * 60;
const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENTRY_BYTES: usize = 64 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryEntry {
    page: HistoryPage,
    pub visited_at: u64,
    lua: String,
}

impl HistoryEntry {
    pub fn view(&self, registry: &ColumnRegistry) -> Result<ViewState, String> {
        ViewState::try_from_lua(&self.lua, registry)
    }

    pub fn label(&self, registry: &ColumnRegistry) -> Result<String, String> {
        self.view(registry).map(|view| view.label(registry))
    }

    pub fn view_lua(&self) -> &str {
        &self.lua
    }

    fn current(&self, now: u64) -> bool {
        now.saturating_sub(self.visited_at) <= MAX_AGE_SECONDS
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HistoryPage {
    Tasks,
    PullRequests,
}

impl HistoryPage {
    fn from_page(page: Page) -> Option<Self> {
        match page {
            Page::Tasks => Some(Self::Tasks),
            Page::PullRequests => Some(Self::PullRequests),
            Page::Agents => None,
            Page::Inbox => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    entries: Vec<HistoryEntry>,
}

#[derive(Clone)]
pub struct HistoryStore {
    path: Option<PathBuf>,
    source: Result<Option<Vec<u8>>, String>,
    entries: Vec<HistoryEntry>,
    blocked: bool,
    last_seen: [Option<String>; 2],
}

impl HistoryStore {
    pub fn load(path: Option<PathBuf>, registry: &ColumnRegistry) -> (Self, Vec<String>) {
        let source = read_source(path.as_deref());
        let decoded = match &source {
            Ok(Some(bytes)) => decode(bytes, registry),
            Ok(None) => Ok(Vec::new()),
            Err(error) => Err(error.clone()),
        };
        let blocked = decoded.is_err();
        let (entries, warnings) = match decoded {
            Ok(entries) => (entries, Vec::new()),
            Err(error) => (
                Vec::new(),
                vec![format!("view history: {error}; repair file and reopen")],
            ),
        };
        (
            Self {
                path,
                source,
                entries,
                blocked,
                last_seen: [None, None],
            },
            warnings,
        )
    }

    pub fn entries(&self, page: Page, now: u64) -> Vec<&HistoryEntry> {
        let page = HistoryPage::from_page(page);
        self.entries
            .iter()
            .take(MAX_ENTRIES)
            .filter(|entry| Some(entry.page) == page && entry.current(now))
            .collect()
    }

    /// Record session visits and transitions; idle checkpoints retain timestamps
    /// while the current arrangement remains inside the retention window.
    /// A returning arrangement replaces its older occurrence anywhere in history.
    pub fn capture(
        &mut self,
        page: Page,
        state: &ViewState,
        registry: &ColumnRegistry,
        now: u64,
    ) -> Result<(), String> {
        let Some(page) = HistoryPage::from_page(page) else {
            return Ok(());
        };
        let mut arrangement = state.clone();
        arrangement.name.clear();
        let lua = arrangement.to_lua(registry);
        validate_record(&lua, registry)?;
        self.check_source()?;
        let same = self.last_seen[page as usize].as_ref() == Some(&lua)
            && self
                .entries
                .iter()
                .any(|entry| entry.page == page && entry.lua == lua && entry.current(now));
        let mut next = self.entries.clone();
        next.retain(|entry| entry.current(now));
        if !same {
            next.retain(|entry| entry.page != page || entry.lua != lua);
            next.insert(
                0,
                HistoryEntry {
                    page,
                    visited_at: now,
                    lua: lua.clone(),
                },
            );
        }
        next.truncate(MAX_ENTRIES);
        if same && next.len() == self.entries.len() {
            return Ok(());
        }
        self.commit(next)?;
        self.last_seen[page as usize] = Some(lua);
        Ok(())
    }

    fn check_source(&self) -> Result<(), String> {
        if self.blocked || self.source.is_err() {
            return Err("view history unreadable or unsupported; repair file and reopen".into());
        }
        if self.source != read_source(self.path.as_deref()) {
            return Err("view history changed on disk; reopen before saving".into());
        }
        Ok(())
    }

    fn commit(&mut self, mut entries: Vec<HistoryEntry>) -> Result<(), String> {
        trim_to_byte_budget(&mut entries)?;
        let bytes = encode(&entries)?;
        assert!(bytes.len() <= MAX_FILE_BYTES);
        if let Some(path) = &self.path {
            write_atomic(path, &bytes, self)?;
            self.source = Ok(Some(bytes));
        }
        self.entries = entries;
        Ok(())
    }
}

/// Count JSON envelope, entries, and commas exactly once before trimming.
/// A large new arrangement can evict hundreds of small old ones without
/// serializing the entire multi-megabyte document after every removal.
fn trim_to_byte_budget(entries: &mut Vec<HistoryEntry>) -> Result<(), String> {
    let sizes = entries
        .iter()
        .map(|entry| {
            serde_json::to_vec(entry)
                .map(|bytes| bytes.len())
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut total =
        encode(&[])?.len() + sizes.iter().sum::<usize>() + entries.len().saturating_sub(1);
    for size in sizes.into_iter().rev() {
        if total <= MAX_FILE_BYTES {
            break;
        }
        entries.pop();
        total -= size + usize::from(!entries.is_empty());
    }
    assert!(total <= MAX_FILE_BYTES);
    Ok(())
}

fn encode(entries: &[HistoryEntry]) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&Document {
        version: 1,
        entries: entries.to_vec(),
    })
    .map_err(|e| e.to_string())
}

fn decode(bytes: &[u8], registry: &ColumnRegistry) -> Result<Vec<HistoryEntry>, String> {
    let document: Document = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if document.version != 1 || document.entries.len() > MAX_ENTRIES {
        return Err("unsupported version or entry count".into());
    }
    let mut entries = document.entries;
    for entry in &mut entries {
        let mut view = ViewState::try_from_lua(&entry.lua, registry)?;
        view.name.clear();
        entry.lua = view.to_lua(registry);
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.visited_at));
    let mut seen = std::collections::HashSet::new();
    entries.retain(|entry| seen.insert((entry.page as u8, entry.lua.clone())));
    Ok(entries)
}

fn validate_record(text: &str, registry: &ColumnRegistry) -> Result<(), String> {
    if text.len() > MAX_ENTRY_BYTES {
        return Err("arrangement exceeds 64 KiB".into());
    }
    ViewState::try_from_lua(text, registry).map(|_| ())
}

fn read_source(path: Option<&Path>) -> Result<Option<Vec<u8>>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err("history exceeds 4 MiB".into());
    }
    Ok(Some(bytes))
}

fn lock_history(path: &Path) -> Result<fs::File, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("history.lock"))
        .map_err(|e| e.to_string())?;
    // A timer checkpoint and a graceful-exit checkpoint can legitimately meet
    // at the same persistence boundary. Give the writer holding the lock a
    // short chance to finish before reporting a real contention failure.
    let mut last_error = None;
    for _ in 0..50 {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(error) => last_error = Some(error),
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err(format!(
        "view history busy: {}",
        last_error.expect("lock attempts always run")
    ))
}

fn temporary_path(path: &Path) -> PathBuf {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.with_extension(format!(
        "history.{}.{stamp}.{sequence}.tmp",
        std::process::id()
    ))
}

fn write_atomic(path: &Path, bytes: &[u8], store: &HistoryStore) -> Result<(), String> {
    let _lock = lock_history(path)?;
    store.check_source()?;
    // A killed process releases its OS lock. Unique temporary names ensure its
    // unfinished write never prevents the next session from checkpointing.
    let temporary = temporary_path(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| format!("view history temporary file {}: {e}", temporary.display()))?;
    let result = (|| {
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        store.check_source()?;
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        fs::remove_file(&temporary).map_err(|e| {
            format!("view history cleanup failed: {e}; original failure: {result:?}")
        })?;
    }
    result.map_err(|e| format!("could not save view history: {e}"))
}
