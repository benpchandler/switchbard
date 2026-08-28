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
const CACHE_VERSION: u32 = 2;
const MAX_HOOK_EVENTS: usize = 64;
const MAX_HOOK_GROUPS_PER_EVENT: usize = 128;
const MAX_HOOKS_PER_GROUP: usize = 128;

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
}

impl ContextKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Instruction => "Instructions",
            Self::Command => "Commands",
            Self::Skill => "Skills",
            Self::Config => "Config",
            Self::Doc => "Docs",
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

/// One hook registration declared in settings for a worktree.
///
/// Hook files alone are not registrations. This type is populated from the
/// agent's settings, preserving the event, matcher, executable action, and
/// source so the UI can explain the detected registration and its trigger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHook {
    pub id: String,
    pub agent: AgentKind,
    pub scope: ContextScope,
    pub source_path: PathBuf,
    pub event: String,
    pub matcher: Option<String>,
    pub hook_type: String,
    pub action: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub condition: Option<String>,
    #[serde(default)]
    pub asynchronous: bool,
    pub timeout_seconds: Option<u64>,
}

impl AgentHook {
    /// A short human-readable effect inferred from the handler type and name.
    pub fn purpose_summary(&self) -> String {
        match self.hook_type.as_str() {
            "http" => format!("Sends hook data to {}", self.action),
            "mcp_tool" => format!("Calls the {} MCP tool", self.action),
            "prompt" => "Asks Claude to evaluate a hook condition".to_string(),
            "agent" => "Runs an agent to evaluate a hook condition".to_string(),
            _ => command_purpose(self),
        }
    }

    /// A human-readable explanation of when this registration fires.
    pub fn trigger_summary(&self) -> String {
        let matcher = self
            .matcher_applies()
            .then(|| self.matcher.as_deref().map(humanize_matcher))
            .flatten();
        let mut trigger = match self.event.as_str() {
            "PreToolUse" => tool_event_summary("Before Claude uses", matcher.as_deref()),
            "PostToolUse" => tool_event_summary("After Claude uses", matcher.as_deref()),
            "PostToolUseFailure" => {
                tool_event_summary("After a Claude tool fails for", matcher.as_deref())
            }
            "PermissionRequest" => {
                tool_event_summary("When Claude requests permission for", matcher.as_deref())
            }
            "SessionStart" => "When a Claude session starts or resumes".to_string(),
            "UserPromptSubmit" => "When the user submits a prompt".to_string(),
            "Notification" => "When Claude sends a notification".to_string(),
            "Stop" => "After Claude finishes responding".to_string(),
            "StopFailure" => "When a Claude turn ends with an error".to_string(),
            "SessionEnd" => "When a Claude session ends".to_string(),
            event => format!("When the {event} event fires"),
        };
        if self.condition_applies() {
            if let Some(condition) = &self.condition {
                trigger.push_str(&format!("; {}", humanize_condition(condition)));
            }
        }
        trigger
    }

    /// Explains a registration that Claude will ignore or never execute.
    pub fn configuration_warning(&self) -> Option<String> {
        if self.condition.is_some() && !self.condition_applies() {
            return Some(format!(
                "Will not run: an 'if' condition is unsupported for {}",
                self.event
            ));
        }
        if self.matcher.is_some() && !self.matcher_applies() {
            return Some(format!("Claude ignores matchers for {}", self.event));
        }
        None
    }

    fn matcher_applies(&self) -> bool {
        !matches!(
            self.event.as_str(),
            "CwdChanged"
                | "MessageDisplay"
                | "PostToolBatch"
                | "Stop"
                | "TaskCompleted"
                | "TaskCreated"
                | "TeammateIdle"
                | "UserPromptSubmit"
                | "WorktreeCreate"
                | "WorktreeRemove"
        )
    }

    fn condition_applies(&self) -> bool {
        matches!(
            self.event.as_str(),
            "PermissionDenied"
                | "PermissionRequest"
                | "PostToolUse"
                | "PostToolUseFailure"
                | "PreToolUse"
        )
    }
}

fn command_purpose(hook: &AgentHook) -> String {
    let candidate = command_subject(hook);
    let normalized = Path::new(candidate)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(candidate)
        .replace(['-', '_'], " ")
        .to_lowercase();
    if normalized.contains("rebuild") && normalized.contains("reload") {
        "Rebuilds and reloads the app".to_string()
    } else if normalized.starts_with("check write") {
        "Checks agent writes".to_string()
    } else if normalized.starts_with("verify") {
        format!("Verifies{}", suffix_after(&normalized, "verify"))
    } else if normalized.starts_with("format") {
        "Formats changed files".to_string()
    } else if normalized.starts_with("lint") {
        "Lints changed files".to_string()
    } else if normalized.starts_with("test") {
        "Runs tests".to_string()
    } else if normalized.starts_with("notify") {
        "Sends a notification".to_string()
    } else if normalized.starts_with("bundle") {
        "Builds the app bundle".to_string()
    } else {
        format!("Runs {normalized}")
    }
}

fn command_subject(hook: &AgentHook) -> &str {
    let executable = Path::new(&hook.action)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(&hook.action);
    let is_interpreter = matches!(
        executable,
        "bash" | "bun" | "node" | "perl" | "python" | "python3" | "ruby" | "sh" | "zsh"
    );
    if is_interpreter {
        hook.arguments
            .iter()
            .find(|argument| !argument.starts_with('-'))
            .map_or(hook.action.as_str(), String::as_str)
    } else {
        &hook.action
    }
}

fn suffix_after(value: &str, prefix: &str) -> String {
    value
        .strip_prefix(prefix)
        .filter(|suffix| !suffix.is_empty())
        .map(|suffix| format!(" {suffix}"))
        .unwrap_or_default()
}

fn humanize_matcher(matcher: &str) -> String {
    if matcher == "*" || matcher.is_empty() {
        "any tool".to_string()
    } else {
        matcher.replace('|', " or ").replace(',', " or")
    }
}

fn tool_event_summary(prefix: &str, matcher: Option<&str>) -> String {
    match matcher {
        Some(matcher) => format!("{prefix} {matcher}"),
        None => format!("{prefix} any tool"),
    }
}

fn humanize_condition(condition: &str) -> String {
    let Some((tool, pattern)) = condition.split_once('(') else {
        return format!("only when {condition} matches");
    };
    let pattern = pattern.strip_suffix(')').unwrap_or(pattern);
    format!("only for {tool} calls matching {pattern}")
}

/// A settings source that could not be fully interpreted as hook config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHookWarning {
    pub source_path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentContextMap {
    pub worktree: PathBuf,
    pub items: Vec<AgentContextItem>,
    #[serde(default)]
    pub hooks: Vec<AgentHook>,
    #[serde(default)]
    pub hook_warnings: Vec<AgentHookWarning>,
    #[serde(default)]
    pub hooks_disabled_by: Option<PathBuf>,
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
    let mut hooks = Vec::new();
    let mut hook_warnings = Vec::new();
    let mut disable_setting = None;
    scan_global(
        &mut items,
        &mut hooks,
        &mut hook_warnings,
        &mut disable_setting,
    );
    scan_worktree(
        worktree,
        &mut items,
        &mut hooks,
        &mut hook_warnings,
        &mut disable_setting,
    );
    deduplicate_hooks(&mut hooks);
    let hooks_disabled_by = disable_setting
        .filter(|(disabled, _)| *disabled)
        .map(|(_, path)| path);
    if hooks_disabled_by.is_some() {
        hooks.clear();
    }
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
        hooks,
        hook_warnings,
        hooks_disabled_by,
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

fn scan_global(
    items: &mut Vec<AgentContextItem>,
    hooks: &mut Vec<AgentHook>,
    warnings: &mut Vec<AgentHookWarning>,
    disable_setting: &mut Option<(bool, PathBuf)>,
) {
    let Some(home) = dirs::home_dir() else { return };
    add_if_file(
        items,
        AgentKind::Claude,
        ContextScope::Global,
        ContextKind::Instruction,
        home.join(".claude/CLAUDE.md"),
        None,
    );
    scan_claude_settings(
        &home.join(".claude/settings.json"),
        ContextScope::Global,
        hooks,
        warnings,
        disable_setting,
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

fn scan_worktree(
    worktree: &Path,
    items: &mut Vec<AgentContextItem>,
    hooks: &mut Vec<AgentHook>,
    warnings: &mut Vec<AgentHookWarning>,
    disable_setting: &mut Option<(bool, PathBuf)>,
) {
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
            _ => {}
        }
    });
    scan_claude_settings(
        &worktree.join(".claude/settings.json"),
        ContextScope::Local,
        hooks,
        warnings,
        disable_setting,
    );
    scan_claude_settings(
        &worktree.join(".claude/settings.local.json"),
        ContextScope::Local,
        hooks,
        warnings,
        disable_setting,
    );
}

fn scan_claude_settings(
    path: &Path,
    scope: ContextScope,
    hooks: &mut Vec<AgentHook>,
    warnings: &mut Vec<AgentHookWarning>,
    disable_setting: &mut Option<(bool, PathBuf)>,
) {
    if !path.is_file() {
        return;
    }
    let value: serde_json::Value = match fs::read_to_string(path)
        .and_then(|text| serde_json::from_str(&text).map_err(io::Error::other))
    {
        Ok(value) => value,
        Err(error) => {
            warnings.push(AgentHookWarning {
                source_path: path.to_path_buf(),
                message: format!("could not read hook settings: {error}"),
            });
            return;
        }
    };
    read_disable_setting(path, &value, disable_setting, warnings);
    let Some(events) = value.get("hooks") else {
        return;
    };
    let Some(events) = events.as_object() else {
        warnings.push(AgentHookWarning {
            source_path: path.to_path_buf(),
            message: "`hooks` must be an object keyed by event".to_string(),
        });
        return;
    };
    if events.len() > MAX_HOOK_EVENTS {
        warnings.push(AgentHookWarning {
            source_path: path.to_path_buf(),
            message: format!("only the first {MAX_HOOK_EVENTS} hook events were scanned"),
        });
    }
    for (event, groups) in events.iter().take(MAX_HOOK_EVENTS) {
        scan_hook_event(path, scope, event, groups, hooks, warnings);
    }
}

fn read_disable_setting(
    path: &Path,
    value: &serde_json::Value,
    disable_setting: &mut Option<(bool, PathBuf)>,
    warnings: &mut Vec<AgentHookWarning>,
) {
    let Some(disabled) = value.get("disableAllHooks") else {
        return;
    };
    let Some(disabled) = disabled.as_bool() else {
        push_hook_shape_warning(
            path,
            warnings,
            "'disableAllHooks' must be true or false".to_string(),
        );
        return;
    };
    *disable_setting = Some((disabled, path.to_path_buf()));
}

fn scan_hook_event(
    path: &Path,
    scope: ContextScope,
    event: &str,
    groups: &serde_json::Value,
    hooks: &mut Vec<AgentHook>,
    warnings: &mut Vec<AgentHookWarning>,
) {
    let Some(groups) = groups.as_array() else {
        push_hook_shape_warning(
            path,
            warnings,
            format!("hook event `{event}` must be an array"),
        );
        return;
    };
    if groups.len() > MAX_HOOK_GROUPS_PER_EVENT {
        push_hook_shape_warning(
            path,
            warnings,
            format!(
                "only the first {MAX_HOOK_GROUPS_PER_EVENT} matcher groups for `{event}` were scanned"
            ),
        );
    }
    for (group_index, group) in groups.iter().take(MAX_HOOK_GROUPS_PER_EVENT).enumerate() {
        scan_hook_group(path, scope, event, group_index, group, hooks, warnings);
    }
}

fn scan_hook_group(
    path: &Path,
    scope: ContextScope,
    event: &str,
    group_index: usize,
    group: &serde_json::Value,
    hooks: &mut Vec<AgentHook>,
    warnings: &mut Vec<AgentHookWarning>,
) {
    let Some(group) = group.as_object() else {
        push_hook_shape_warning(
            path,
            warnings,
            format!("`{event}` matcher group must be an object"),
        );
        return;
    };
    let matcher = group
        .get("matcher")
        .and_then(serde_json::Value::as_str)
        .filter(|matcher| !matcher.is_empty())
        .map(str::to_string);
    let Some(entries) = group.get("hooks").and_then(serde_json::Value::as_array) else {
        push_hook_shape_warning(
            path,
            warnings,
            format!("`{event}` matcher group is missing a hook array"),
        );
        return;
    };
    if entries.len() > MAX_HOOKS_PER_GROUP {
        push_hook_shape_warning(
            path,
            warnings,
            format!("only the first {MAX_HOOKS_PER_GROUP} hooks in `{event}` were scanned"),
        );
    }
    for (hook_index, entry) in entries.iter().take(MAX_HOOKS_PER_GROUP).enumerate() {
        let Some(hook) = parse_hook(
            path,
            scope,
            event,
            group_index,
            hook_index,
            matcher.clone(),
            entry,
        ) else {
            push_hook_shape_warning(
                path,
                warnings,
                format!("`{event}` contains a hook without a supported action"),
            );
            continue;
        };
        hooks.push(hook);
    }
}

fn parse_hook(
    path: &Path,
    scope: ContextScope,
    event: &str,
    group_index: usize,
    hook_index: usize,
    matcher: Option<String>,
    value: &serde_json::Value,
) -> Option<AgentHook> {
    let object = value.as_object()?;
    let hook_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("command")
        .to_string();
    let action = hook_action(&hook_type, object)?;
    let arguments = object
        .get("args")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Some(AgentHook {
        id: format!("{}#{event}:{group_index}:{hook_index}", path.display()),
        agent: AgentKind::Claude,
        scope,
        source_path: path.to_path_buf(),
        event: event.to_string(),
        matcher,
        hook_type,
        action,
        arguments,
        condition: object
            .get("if")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        asynchronous: object
            .get("async")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        timeout_seconds: object.get("timeout").and_then(serde_json::Value::as_u64),
    })
}

fn hook_action(
    hook_type: &str,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    match hook_type {
        "command" => string_field(object, "command"),
        "http" => string_field(object, "url"),
        "prompt" | "agent" => string_field(object, "prompt"),
        "mcp_tool" => {
            let server = string_field(object, "server")?;
            let tool = string_field(object, "tool")?;
            Some(format!("{server} / {tool}"))
        }
        _ => ["command", "prompt", "url"]
            .iter()
            .find_map(|key| string_field(object, key)),
    }
}

fn string_field(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn deduplicate_hooks(hooks: &mut Vec<AgentHook>) {
    let mut deduplicated = Vec::with_capacity(hooks.len());
    for hook in hooks.drain(..) {
        if let Some(index) = deduplicated
            .iter()
            .position(|existing| same_hook_registration(existing, &hook))
        {
            deduplicated[index] = hook;
        } else {
            deduplicated.push(hook);
        }
    }
    *hooks = deduplicated;
}

fn same_hook_registration(left: &AgentHook, right: &AgentHook) -> bool {
    left.agent == right.agent
        && left.event == right.event
        && left.matcher == right.matcher
        && left.hook_type == right.hook_type
        && left.action == right.action
        && left.arguments == right.arguments
        && left.condition == right.condition
        && left.asynchronous == right.asynchronous
        && left.timeout_seconds == right.timeout_seconds
}

fn push_hook_shape_warning(path: &Path, warnings: &mut Vec<AgentHookWarning>, message: String) {
    warnings.push(AgentHookWarning {
        source_path: path.to_path_buf(),
        message,
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
        write_text(
            &dir.path().join(".claude/settings.local.json"),
            r#"{
                "hooks": {
                    "PreToolUse": [{
                        "matcher": "Write|Edit",
                        "hooks": [{
                            "type": "command",
                            "command": "./scripts/check-write.sh",
                            "timeout": 15
                        }]
                    }]
                }
            }"#,
        );
        fs::create_dir_all(dir.path().join("apps/web")).unwrap();
        write_file(&dir.path().join("apps/CLAUDE.md"));

        let map = scan_agent_context(dir.path());
        assert!(map.scanned_at.is_some());
        assert!(map.items.iter().any(|i| i.title == "CLAUDE.md"));
        assert!(map.items.iter().any(|i| i.title == "AGENTS.md"));
        assert!(map.items.iter().any(|i| i.title == "/test"));
        assert!(map.items.iter().any(|i| i.scope == ContextScope::Directory));
        assert!(map.items.iter().any(|i| i.warning.is_some()));
        let hook = map
            .hooks
            .iter()
            .find(|hook| hook.source_path.starts_with(dir.path()))
            .expect("invariant: repo hook should be detected");
        assert_eq!(hook.event, "PreToolUse");
        assert_eq!(hook.matcher.as_deref(), Some("Write|Edit"));
        assert_eq!(hook.action, "./scripts/check-write.sh");
        assert_eq!(hook.timeout_seconds, Some(15));
    }

    #[test]
    fn malformed_hook_group_keeps_other_registrations() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join(".claude/settings.json");
        write_text(
            &settings,
            r#"{
                "hooks": {
                    "Stop": [
                        {"hooks": [{"type": "command", "command": "./valid.sh"}]},
                        {"hooks": "not-an-array"}
                    ]
                }
            }"#,
        );

        let map = scan_agent_context(dir.path());
        let local_hooks: Vec<&AgentHook> = map
            .hooks
            .iter()
            .filter(|hook| hook.source_path == settings)
            .collect();

        assert_eq!(local_hooks.len(), 1);
        assert_eq!(local_hooks[0].action, "./valid.sh");
        assert!(map
            .hook_warnings
            .iter()
            .any(|warning| warning.source_path == settings));
    }

    #[test]
    fn hook_without_supported_action_is_reported_not_invented() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join(".claude/settings.json");
        write_text(
            &settings,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"unknown","payload":true}]}]}}"#,
        );

        let map = scan_agent_context(dir.path());

        assert!(map.hooks.iter().all(|hook| hook.source_path != settings));
        assert!(map.hook_warnings.iter().any(|warning| {
            warning.source_path == settings && warning.message.contains("supported action")
        }));
    }

    #[test]
    fn higher_precedence_false_reenables_merged_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join(".claude/settings.json");
        let local = dir.path().join(".claude/settings.local.json");
        write_text(
            &shared,
            r#"{
                "disableAllHooks": true,
                "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "./shared.sh"}]}]}
            }"#,
        );
        write_text(&local, r#"{"disableAllHooks": false}"#);

        let map = scan_agent_context(dir.path());

        assert!(map.hooks_disabled_by.is_none());
        assert!(map
            .hooks
            .iter()
            .any(|hook| hook.source_path == shared && hook.action == "./shared.sh"));
    }

    #[test]
    fn higher_precedence_true_hides_configured_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join(".claude/settings.json");
        let local = dir.path().join(".claude/settings.local.json");
        write_text(
            &shared,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"./shared.sh"}]}]}}"#,
        );
        write_text(&local, r#"{"disableAllHooks": true}"#);

        let map = scan_agent_context(dir.path());

        assert!(map.hooks.is_empty());
        assert_eq!(map.hooks_disabled_by.as_deref(), Some(local.as_path()));
    }

    #[test]
    fn identical_settings_handlers_run_once_at_the_highest_source() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join(".claude/settings.json");
        let local = dir.path().join(".claude/settings.local.json");
        let config = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"./same.sh"}]}]}}"#;
        write_text(&shared, config);
        write_text(&local, config);

        let map = scan_agent_context(dir.path());
        let matching: Vec<&AgentHook> = map
            .hooks
            .iter()
            .filter(|hook| hook.action == "./same.sh")
            .collect();

        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].source_path, local);
    }

    #[test]
    fn parses_current_hook_handler_shapes() {
        let dir = tempfile::tempdir().unwrap();
        write_text(
            &dir.path().join(".claude/settings.json"),
            r#"{
                "hooks": {
                    "PostToolUse": [{
                        "matcher": "Write|Edit",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "python3",
                                "args": ["./check.py", "--strict"],
                                "if": "Edit(*.rs)",
                                "async": true
                            },
                            {
                                "type": "mcp_tool",
                                "server": "security",
                                "tool": "scan"
                            }
                        ]
                    }]
                }
            }"#,
        );

        let map = scan_agent_context(dir.path());
        let command = map
            .hooks
            .iter()
            .find(|hook| hook.action == "python3")
            .expect("invariant: command hook should parse");
        let mcp = map
            .hooks
            .iter()
            .find(|hook| hook.hook_type == "mcp_tool")
            .expect("invariant: MCP hook should parse");

        assert_eq!(command.arguments, ["./check.py", "--strict"]);
        assert_eq!(command.condition.as_deref(), Some("Edit(*.rs)"));
        assert!(command.asynchronous);
        assert_eq!(mcp.action, "security / scan");
    }

    #[test]
    fn hook_summary_leads_with_effect_and_plain_english_trigger() {
        let hook = AgentHook {
            id: "summary".to_string(),
            agent: AgentKind::Claude,
            scope: ContextScope::Local,
            source_path: PathBuf::from(".claude/settings.json"),
            event: "PostToolUse".to_string(),
            matcher: Some("Write|Edit".to_string()),
            hook_type: "command".to_string(),
            action: "python3".to_string(),
            arguments: vec![".claude/hooks/check-write.py".to_string()],
            condition: Some("Edit(*.rs)".to_string()),
            asynchronous: false,
            timeout_seconds: None,
        };

        assert_eq!(hook.purpose_summary(), "Checks agent writes");
        assert_eq!(
            hook.trigger_summary(),
            "After Claude uses Write or Edit; only for Edit calls matching *.rs"
        );
    }

    #[test]
    fn hook_summary_humanizes_the_repo_reload_script() {
        let hook = AgentHook {
            id: "reload".to_string(),
            agent: AgentKind::Claude,
            scope: ContextScope::Local,
            source_path: PathBuf::from(".claude/settings.local.json"),
            event: "Stop".to_string(),
            matcher: None,
            hook_type: "command".to_string(),
            action: "./scripts/rebuild-and-reload.sh".to_string(),
            arguments: Vec::new(),
            condition: None,
            asynchronous: true,
            timeout_seconds: None,
        };

        assert_eq!(hook.purpose_summary(), "Rebuilds and reloads the app");
        assert_eq!(hook.trigger_summary(), "After Claude finishes responding");
    }

    #[test]
    fn hook_summary_calls_out_config_that_cannot_run() {
        let hook = AgentHook {
            id: "invalid-stop".to_string(),
            agent: AgentKind::Claude,
            scope: ContextScope::Local,
            source_path: PathBuf::from(".claude/settings.json"),
            event: "Stop".to_string(),
            matcher: Some("Write".to_string()),
            hook_type: "command".to_string(),
            action: "./stop.sh".to_string(),
            arguments: Vec::new(),
            condition: Some("Edit(*.rs)".to_string()),
            asynchronous: false,
            timeout_seconds: None,
        };

        assert_eq!(
            hook.configuration_warning().as_deref(),
            Some("Will not run: an 'if' condition is unsupported for Stop")
        );
        assert_eq!(hook.trigger_summary(), "After Claude finishes responding");
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
        write_text(path, "test\n");
    }

    fn write_text(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        write!(f, "{text}").unwrap();
    }
}
