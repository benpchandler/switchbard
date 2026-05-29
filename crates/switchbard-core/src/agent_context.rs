//! Local filesystem scanner for agent-facing context.
//!
//! The scanner is intentionally read-only: it detects files that may influence
//! coding agents, classifies them by scope/type, and leaves exact vendor prompt
//! assembly as a best-effort UI concern.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const CACHE_RELATIVE_PATH: &str = ".switchbard/agent-context-cache.json";
const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AgentKind {
    Claude,
    Codex,
    Shared,
}

impl AgentKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Shared => "Shared",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContextScope {
    Global,
    Local,
    Directory,
}

impl ContextScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::Local => "Local repo",
            Self::Directory => "Nested",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContextKind {
    Instruction,
    Command,
    Skill,
    Config,
    Doc,
    Hook,
}

impl ContextKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Instruction => "Instructions",
            Self::Command => "Commands",
            Self::Skill => "Skills",
            Self::Config => "Config",
            Self::Doc => "Docs",
            Self::Hook => "Hooks",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentContextItem {
    pub id: String,
    pub agent: AgentKind,
    pub scope: ContextScope,
    pub kind: ContextKind,
    pub path: PathBuf,
    pub applies_to: Option<PathBuf>,
    pub title: String,
    pub size_bytes: u64,
    pub modified_at: Option<SystemTime>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentContextMap {
    pub worktree: PathBuf,
    pub items: Vec<AgentContextItem>,
    #[serde(default)]
    pub scanned_at: Option<SystemTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentContextCache {
    version: u32,
    maps: Vec<AgentContextMap>,
}

impl AgentContextMap {
    pub fn items_for(&self, scope: ContextScope, kind: ContextKind) -> Vec<&AgentContextItem> {
        self.items
            .iter()
            .filter(|i| i.scope == scope && i.kind == kind)
            .collect()
    }

    pub fn items_in_scope(&self, scope: ContextScope) -> Vec<&AgentContextItem> {
        self.items.iter().filter(|i| i.scope == scope).collect()
    }

    pub fn count_for(&self, scope: ContextScope, kind: ContextKind) -> usize {
        self.items
            .iter()
            .filter(|i| i.scope == scope && i.kind == kind)
            .count()
    }

    pub fn count_in_scope(&self, scope: ContextScope) -> usize {
        self.items.iter().filter(|i| i.scope == scope).count()
    }

    pub fn effective_instructions(&self, agent: AgentKind, cwd: &Path) -> Vec<&AgentContextItem> {
        let mut items: Vec<&AgentContextItem> = self
            .items
            .iter()
            .filter(|i| i.kind == ContextKind::Instruction && i.agent == agent)
            .filter(|i| match i.scope {
                ContextScope::Global | ContextScope::Local => true,
                ContextScope::Directory => {
                    i.applies_to.as_deref().is_some_and(|p| cwd.starts_with(p))
                }
            })
            .collect();
        items.sort_by(|a, b| {
            scope_rank(a.scope).cmp(&scope_rank(b.scope)).then_with(|| {
                a.path
                    .components()
                    .count()
                    .cmp(&b.path.components().count())
            })
        });
        items
    }
}

fn scope_rank(scope: ContextScope) -> u8 {
    match scope {
        ContextScope::Global => 0,
        ContextScope::Local => 1,
        ContextScope::Directory => 2,
    }
}

pub fn scan_agent_context(worktree: &Path) -> AgentContextMap {
    let mut items = Vec::new();
    scan_global(&mut items);
    scan_worktree(worktree, &mut items);
    mark_instruction_overlap(&mut items);
    items.sort_by(|a, b| {
        a.scope
            .cmp(&b.scope)
            .then(a.kind.cmp(&b.kind))
            .then(a.agent.cmp(&b.agent))
            .then(a.path.cmp(&b.path))
    });
    AgentContextMap {
        worktree: worktree.to_path_buf(),
        items,
        scanned_at: Some(SystemTime::now()),
    }
}

pub fn agent_context_cache_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(CACHE_RELATIVE_PATH))
}

pub fn load_agent_context_cache() -> io::Result<Vec<AgentContextMap>> {
    let Some(path) = agent_context_cache_path() else {
        return Ok(Vec::new());
    };
    load_agent_context_cache_from(&path)
}

pub fn load_agent_context_cache_from(path: &Path) -> io::Result<Vec<AgentContextMap>> {
    let text = fs::read_to_string(path)?;
    let cache: AgentContextCache =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if cache.version != CACHE_VERSION {
        return Ok(Vec::new());
    }
    Ok(cache.maps)
}

pub fn save_agent_context_cache(maps: &[AgentContextMap]) -> io::Result<()> {
    let Some(path) = agent_context_cache_path() else {
        return Ok(());
    };
    save_agent_context_cache_to(&path, maps)
}

pub fn save_agent_context_cache_to(path: &Path, maps: &[AgentContextMap]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let cache = AgentContextCache {
        version: CACHE_VERSION,
        maps: maps.to_vec(),
    };
    let text = serde_json::to_string_pretty(&cache)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, path)
}

pub fn agent_context_needs_rescan(
    map: &AgentContextMap,
    now: SystemTime,
    max_age: Duration,
) -> bool {
    map.scanned_at
        .and_then(|scanned_at| now.duration_since(scanned_at).ok())
        .is_none_or(|age| age > max_age)
}

fn scan_global(items: &mut Vec<AgentContextItem>) {
    let Some(home) = dirs::home_dir() else { return };
    add_if_file(
        items,
        AgentKind::Claude,
        ContextScope::Global,
        ContextKind::Instruction,
        home.join(".claude/CLAUDE.md"),
        None,
    );
    add_if_file(
        items,
        AgentKind::Claude,
        ContextScope::Global,
        ContextKind::Config,
        home.join(".claude/settings.json"),
        None,
    );
    add_if_file(
        items,
        AgentKind::Claude,
        ContextScope::Global,
        ContextKind::Config,
        home.join(".claude/settings.local.json"),
        None,
    );
    add_dir_files(
        items,
        AgentKind::Claude,
        ContextScope::Global,
        ContextKind::Command,
        &home.join(".claude/commands"),
    );
    add_dir_files(
        items,
        AgentKind::Claude,
        ContextScope::Global,
        ContextKind::Hook,
        &home.join(".claude/hooks"),
    );
    add_dir_files(
        items,
        AgentKind::Claude,
        ContextScope::Global,
        ContextKind::Doc,
        &home.join(".claude/agents"),
    );

    add_if_file(
        items,
        AgentKind::Codex,
        ContextScope::Global,
        ContextKind::Instruction,
        home.join(".codex/AGENTS.md"),
        None,
    );
    add_if_file(
        items,
        AgentKind::Codex,
        ContextScope::Global,
        ContextKind::Instruction,
        home.join(".codex/instructions.md"),
        None,
    );
    add_if_file(
        items,
        AgentKind::Codex,
        ContextScope::Global,
        ContextKind::Config,
        home.join(".codex/config.toml"),
        None,
    );

    if let Ok(entries) = fs::read_dir(home.join(".agents/skills")) {
        for entry in entries.flatten() {
            add_if_file(
                items,
                AgentKind::Shared,
                ContextScope::Global,
                ContextKind::Skill,
                entry.path().join("SKILL.md"),
                None,
            );
        }
    }
}

fn scan_worktree(worktree: &Path, items: &mut Vec<AgentContextItem>) {
    walk(worktree, &mut |path| {
        let Ok(rel) = path.strip_prefix(worktree) else {
            return;
        };
        let rel_s = rel.to_string_lossy();
        let file = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let parent = path.parent().unwrap_or(worktree);
        let scope = if parent == worktree {
            ContextScope::Local
        } else {
            ContextScope::Directory
        };
        let applies_to = Some(parent.to_path_buf());

        match file {
            "CLAUDE.md" => add_existing(
                items,
                AgentKind::Claude,
                scope,
                ContextKind::Instruction,
                path.to_path_buf(),
                applies_to,
            ),
            "AGENTS.md" => add_existing(
                items,
                AgentKind::Codex,
                scope,
                ContextKind::Instruction,
                path.to_path_buf(),
                applies_to,
            ),
            "README.md" | "CONVENTIONS.md" => add_existing(
                items,
                AgentKind::Shared,
                scope,
                ContextKind::Doc,
                path.to_path_buf(),
                applies_to,
            ),
            "settings.json" | "settings.local.json" if rel_s.starts_with(".claude/") => {
                add_existing(
                    items,
                    AgentKind::Claude,
                    ContextScope::Local,
                    ContextKind::Config,
                    path.to_path_buf(),
                    Some(worktree.to_path_buf()),
                )
            }
            "config.toml" if rel_s.starts_with(".codex/") => add_existing(
                items,
                AgentKind::Codex,
                ContextScope::Local,
                ContextKind::Config,
                path.to_path_buf(),
                Some(worktree.to_path_buf()),
            ),
            "instructions.md" if rel_s.starts_with(".codex/") => add_existing(
                items,
                AgentKind::Codex,
                ContextScope::Local,
                ContextKind::Instruction,
                path.to_path_buf(),
                Some(worktree.to_path_buf()),
            ),
            "SKILL.md" if rel_s.starts_with(".agents/skills/") => add_existing(
                items,
                AgentKind::Shared,
                ContextScope::Local,
                ContextKind::Skill,
                path.to_path_buf(),
                Some(worktree.to_path_buf()),
            ),
            _ if rel_s.starts_with(".claude/commands/") && file.ends_with(".md") => add_existing(
                items,
                AgentKind::Claude,
                ContextScope::Local,
                ContextKind::Command,
                path.to_path_buf(),
                Some(worktree.to_path_buf()),
            ),
            _ if rel_s.starts_with(".claude/hooks/") => add_existing(
                items,
                AgentKind::Claude,
                ContextScope::Local,
                ContextKind::Hook,
                path.to_path_buf(),
                Some(worktree.to_path_buf()),
            ),
            _ => {}
        }
    });
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if is_ignored_dir(name) {
                continue;
            }
            walk(&path, f);
        } else if path.is_file() {
            f(&path);
        }
    }
}

fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "target" | "dist" | "build" | ".next" | ".nuxt" | "vendor"
    )
}

fn add_dir_files(
    items: &mut Vec<AgentContextItem>,
    agent: AgentKind,
    scope: ContextScope,
    kind: ContextKind,
    dir: &Path,
) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                add_existing(items, agent, scope, kind, path, None);
            }
        }
    }
}

fn add_if_file(
    items: &mut Vec<AgentContextItem>,
    agent: AgentKind,
    scope: ContextScope,
    kind: ContextKind,
    path: PathBuf,
    applies_to: Option<PathBuf>,
) {
    if path.is_file() {
        add_existing(items, agent, scope, kind, path, applies_to);
    }
}

fn add_existing(
    items: &mut Vec<AgentContextItem>,
    agent: AgentKind,
    scope: ContextScope,
    kind: ContextKind,
    path: PathBuf,
    applies_to: Option<PathBuf>,
) {
    let metadata = fs::metadata(&path).ok();
    let title = title_for(kind, &path);
    items.push(AgentContextItem {
        id: path.to_string_lossy().into_owned(),
        agent,
        scope,
        kind,
        path,
        applies_to,
        title,
        size_bytes: metadata.as_ref().map_or(0, fs::Metadata::len),
        modified_at: metadata.and_then(|m| m.modified().ok()),
        warning: None,
    });
}

fn title_for(kind: ContextKind, path: &Path) -> String {
    match kind {
        ContextKind::Command => path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| format!("/{s}"))
            .unwrap_or_else(|| "command".to_string()),
        ContextKind::Skill => path
            .parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .unwrap_or("skill")
            .to_string(),
        _ => path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("context")
            .to_string(),
    }
}

fn mark_instruction_overlap(items: &mut [AgentContextItem]) {
    let has_local_claude = items.iter().any(|i| {
        i.scope == ContextScope::Local
            && i.kind == ContextKind::Instruction
            && i.agent == AgentKind::Claude
    });
    let has_local_codex = items.iter().any(|i| {
        i.scope == ContextScope::Local
            && i.kind == ContextKind::Instruction
            && i.agent == AgentKind::Codex
    });
    if has_local_claude && has_local_codex {
        for item in items
            .iter_mut()
            .filter(|i| i.scope == ContextScope::Local && i.kind == ContextKind::Instruction)
        {
            item.warning = Some("Repo has both Claude and Codex instruction files".to_string());
        }
    }
}

pub fn read_context_preview(path: &Path, max_bytes: usize) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    let end = bytes.len().min(max_bytes);
    let mut text = String::from_utf8_lossy(&bytes[..end]).into_owned();
    if bytes.len() > end {
        text.push_str("\n…");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn scans_repo_context() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("CLAUDE.md"));
        write_file(&dir.path().join("AGENTS.md"));
        fs::create_dir_all(dir.path().join(".claude/commands")).unwrap();
        write_file(&dir.path().join(".claude/commands/test.md"));
        fs::create_dir_all(dir.path().join("apps/web")).unwrap();
        write_file(&dir.path().join("apps/CLAUDE.md"));

        let map = scan_agent_context(dir.path());
        assert!(map.scanned_at.is_some());
        assert!(map.items.iter().any(|i| i.title == "CLAUDE.md"));
        assert!(map.items.iter().any(|i| i.title == "AGENTS.md"));
        assert!(map.items.iter().any(|i| i.title == "/test"));
        assert!(map.items.iter().any(|i| i.scope == ContextScope::Directory));
        assert!(map.items.iter().any(|i| i.warning.is_some()));
    }

    #[test]
    fn context_cache_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("CLAUDE.md"));
        let map = scan_agent_context(dir.path());
        let cache_path = dir.path().join("cache/agent-context-cache.json");

        save_agent_context_cache_to(&cache_path, std::slice::from_ref(&map)).unwrap();
        let loaded = load_agent_context_cache_from(&cache_path).unwrap();

        assert_eq!(loaded, vec![map]);
    }

    #[test]
    fn context_cache_staleness_uses_scanned_at() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let fresh = AgentContextMap {
            scanned_at: Some(now - Duration::from_secs(10)),
            ..AgentContextMap::default()
        };
        let stale = AgentContextMap {
            scanned_at: Some(now - Duration::from_secs(100)),
            ..AgentContextMap::default()
        };
        let missing = AgentContextMap::default();

        assert!(!agent_context_needs_rescan(
            &fresh,
            now,
            Duration::from_secs(30)
        ));
        assert!(agent_context_needs_rescan(
            &stale,
            now,
            Duration::from_secs(30)
        ));
        assert!(agent_context_needs_rescan(
            &missing,
            now,
            Duration::from_secs(30)
        ));
    }

    fn write_file(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        writeln!(f, "test").unwrap();
    }
}
