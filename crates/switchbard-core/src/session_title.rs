//! What an agent session is *about*, in words — the one line a fleet row
//! or an `sbt` Agents row prints next to its state (TASK-225).
//!
//! ## Where a title can come from, in order of trust
//!
//! 1. **The session's name.** Two readers report the same fact: the
//!    status-line payload the session itself pushes on every update
//!    (`session_name`, stored by `sb agent status` in `agent_status`) and
//!    `claude agents --json`'s `name`. The payload is the fresher one -
//!    it is exactly what the status line prints, and it moves the moment
//!    the CLI renames the session (`/rename`, a generated title after a
//!    `/clear`) where the listing has been seen to lag by a whole task -
//!    so [`entitle_sessions`] takes it first and the listing's name only
//!    when no payload has been recorded. Either may be the *derived
//!    default* every unnamed interactive session gets: the working
//!    directory's basename plus a two-character suffix (`budget-c2`). The
//!    default is a display handle, not a description, so
//!    [`is_derived_session_name`] rejects it and resolution falls through.
//! 2. **A task the session holds.** A `work_sessions` claim names exactly
//!    what the session is working; the caller joins the claim on session
//!    id and passes the task's title in.
//! 3. **The first prompt of the transcript.** Claude Code stores each
//!    session's transcript at `~/.claude/projects/<dir>/<session>.jsonl`
//!    (documented location; the *entry format* is documented as internal
//!    and liable to change). [`first_user_prompt`] therefore reads a
//!    bounded head of the file, accepts only the shape it recognises, and
//!    returns `None` on anything else — a missing title, never a wrong
//!    one.
//! 4. **The listed name after all**, derived or not, so a row is never
//!    blank; and `"interactive session"` when there was no name at all.

use crate::agent_sessions::AgentSession;
use crate::agent_status::is_safe_file_stem;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A title longer than this is cut at a character boundary and marked with
/// an ellipsis: a row has one line, and a first prompt can be a page.
pub const MAX_TITLE_CHARS: usize = 120;

/// How much of a transcript [`first_user_prompt`] will read. The first
/// prompt is the first `user` entry, which sits within the opening few
/// kilobytes; the bound keeps a multi-megabyte transcript from being
/// buffered on every poll.
pub const MAX_TRANSCRIPT_HEAD_BYTES: u64 = 256 * 1024;

/// The label a surface prints for a session that has not been through
/// [`entitle_sessions`] at all.
pub const UNNAMED_SESSION_TITLE: &str = "interactive session";

/// True when `name` is the derived default Claude Code assigns an unnamed
/// interactive session: the lowercased basename of `cwd`, a hyphen, and a
/// two-character suffix. Without a cwd the shape alone cannot be trusted
/// (a human can name a session `auth-42`), so the answer is `false`.
pub fn is_derived_session_name(name: &str, cwd: Option<&Path>) -> bool {
    let Some(base) = cwd.and_then(Path::file_name).and_then(|n| n.to_str()) else {
        return false;
    };
    let Some((stem, suffix)) = name.rsplit_once('-') else {
        return false;
    };
    suffix.chars().count() == 2
        && suffix.chars().all(|c| c.is_ascii_alphanumeric())
        && stem == base.to_ascii_lowercase()
}

/// The one title for a session, from the inputs the caller could gather —
/// see the module doc for the order. `None` only when every input was
/// absent or blank; [`entitle_sessions`] then names the CLI instead. Pure.
pub fn resolve_session_title(
    name: Option<&str>,
    cwd: Option<&Path>,
    held_task_title: Option<&str>,
    first_prompt: Option<&str>,
) -> Option<String> {
    let name = name.map(str::trim).filter(|n| !n.is_empty());
    if let Some(name) = name.filter(|n| !is_derived_session_name(n, cwd)) {
        return Some(clip_title(name));
    }
    if let Some(title) = held_task_title.map(str::trim).filter(|t| !t.is_empty()) {
        return Some(clip_title(title));
    }
    if let Some(prompt) = first_prompt.map(str::trim).filter(|p| !p.is_empty()) {
        return Some(clip_title(prompt));
    }
    name.map(clip_title)
}

/// The title of a session nothing could name: which CLI it is.
pub fn unnamed_session_title(kind: crate::agent_sessions::AgentProcessKind) -> String {
    format!("{} session", kind.label())
}

/// Collapse whitespace runs to one space and bound the length.
fn clip_title(text: &str) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_TITLE_CHARS {
        return collapsed;
    }
    let mut cut: String = collapsed.chars().take(MAX_TITLE_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// `$CLAUDE_CONFIG_DIR`, else `~/.claude` — where Claude Code keeps its
/// per-project transcripts.
pub fn default_claude_home() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    dirs::home_dir().map(|home| home.join(".claude"))
}

/// The `<dir>` segment of a transcript path: the working directory with
/// every non-alphanumeric character replaced by `-` (documented). Claude
/// Code truncates and hashes names over 200 characters; that hash is not
/// reproduced here, so such a path yields `None` and the fallback is
/// simply unavailable.
pub fn project_dir_name(cwd: &Path) -> Option<String> {
    let name: String = cwd
        .to_str()?
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    (name.len() <= 200).then_some(name)
}

/// Where the transcript for `session_id` started in `cwd` lives.
pub fn transcript_path(claude_home: &Path, cwd: &Path, session_id: &str) -> Option<PathBuf> {
    if !is_safe_file_stem(session_id) {
        return None;
    }
    Some(
        claude_home
            .join("projects")
            .join(project_dir_name(cwd)?)
            .join(format!("{session_id}.jsonl")),
    )
}

/// Read up to [`MAX_TRANSCRIPT_HEAD_BYTES`] of `transcript` and return the
/// first human prompt in it. `None` for a missing or unreadable file, or
/// for a head that holds no entry of the expected shape.
pub fn first_user_prompt(transcript: &Path) -> Option<String> {
    let file = std::fs::File::open(transcript).ok()?;
    let mut head = String::new();
    file.take(MAX_TRANSCRIPT_HEAD_BYTES)
        .read_to_string(&mut head)
        .ok()?;
    parse_first_user_prompt(&head)
}

/// The first `user` entry's text in a JSONL transcript head. Pure. Skips
/// meta entries and content that is not plain prose (slash-command
/// envelopes and other `<tag>` payloads), so the answer is something a
/// human typed. A line that does not parse is skipped, not fatal: the
/// head may end mid-line.
pub fn parse_first_user_prompt(jsonl: &str) -> Option<String> {
    jsonl
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|entry| entry.get("type").and_then(|t| t.as_str()) == Some("user"))
        .filter(|entry| entry.get("isMeta").and_then(|m| m.as_bool()) != Some(true))
        .filter_map(|entry| user_entry_text(&entry))
        .map(|text| text.trim().to_string())
        .find(|text| !text.is_empty() && !text.starts_with('<'))
}

fn user_entry_text(entry: &serde_json::Value) -> Option<String> {
    let content = entry.get("message")?.get("content")?;
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    let blocks = content.as_array()?;
    let text: Vec<&str> = blocks
        .iter()
        .filter(|block| block.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
        .collect();
    (!text.is_empty()).then(|| text.join(" "))
}

/// The transcript fallback for one attributed session: needs its cwd and
/// session id, and the Claude home. `None` whenever any of those is
/// missing — the caller then resolves without a first prompt.
pub fn first_user_prompt_for(session: &AgentSession, claude_home: &Path) -> Option<String> {
    let cwd = session.cwd.as_deref()?;
    let session_id = session.session_id.as_deref()?;
    first_user_prompt(&transcript_path(claude_home, cwd, session_id)?)
}

/// Fill [`AgentSession::title`] for every session, in the module doc's
/// order. `reported_name` answers "what did this session last call itself"
/// from the status-line records the caller reads (the fresher reader of
/// the name, see the module doc); `held_title` answers "which task does
/// this session hold" from whatever claim store the caller reads;
/// `prompt_cache` remembers each session's first prompt across polls
/// (keyed by session id — a first prompt never changes, and reading it
/// costs a file open), so a poll re-reads only sessions it has not seen.
/// `claude_home` `None` disables the transcript fallback.
pub fn entitle_sessions(
    sessions: &mut [AgentSession],
    claude_home: Option<&Path>,
    reported_name: impl Fn(&AgentSession) -> Option<String>,
    held_title: impl Fn(&AgentSession) -> Option<String>,
    prompt_cache: &mut HashMap<String, Option<String>>,
) {
    for session in sessions.iter_mut() {
        let name = reported_name(session)
            .filter(|n| !n.trim().is_empty())
            .or_else(|| session.name.clone());
        let held = held_title(session);
        let needs_prompt = held.is_none()
            && name
                .as_deref()
                .is_none_or(|name| is_derived_session_name(name, session.cwd.as_deref()));
        let prompt = match (needs_prompt, claude_home, session.session_id.as_deref()) {
            (true, Some(home), Some(id)) => prompt_cache
                .entry(id.to_string())
                .or_insert_with(|| first_user_prompt_for(session, home))
                .clone(),
            _ => None,
        };
        session.title = Some(
            resolve_session_title(
                name.as_deref(),
                session.cwd.as_deref(),
                held.as_deref(),
                prompt.as_deref(),
            )
            .unwrap_or_else(|| unnamed_session_title(session.kind)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_sessions::{AgentActivity, AgentProcessKind};

    fn session(pid: u32, name: Option<&str>, cwd: &str, id: Option<&str>) -> AgentSession {
        AgentSession {
            pid,
            kind: AgentProcessKind::Claude,
            cwd: Some(PathBuf::from(cwd)),
            repo_name: None,
            worktree_path: None,
            worktree_branch: None,
            started_unix: None,
            pgid: None,
            session_id: id.map(str::to_string),
            name: name.map(str::to_string),
            activity: AgentActivity::Idle,
            title: None,
        }
    }

    #[test]
    fn entitle_fills_every_session_and_reads_a_transcript_once() {
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("projects").join("-w-budget");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("sid-1.jsonl"),
            "{\"type\":\"user\",\"message\":{\"content\":\"why is login slow\"}}\n",
        )
        .unwrap();
        let mut sessions = vec![
            session(1, Some("budget-c2"), "/w/budget", Some("sid-1")),
            session(2, Some("budget-0f"), "/w/budget", Some("sid-2")),
            session(3, Some("auth-refactor"), "/w/budget", Some("sid-3")),
            session(4, None, "/w/budget", None),
        ];
        let mut cache = HashMap::new();
        let none = |_: &AgentSession| None;
        let held = |s: &AgentSession| (s.pid == 2).then(|| "Fix login".to_string());

        entitle_sessions(&mut sessions, Some(home.path()), none, held, &mut cache);

        assert_eq!(sessions[0].title.as_deref(), Some("why is login slow"));
        assert_eq!(sessions[1].title.as_deref(), Some("Fix login"));
        assert_eq!(sessions[2].title.as_deref(), Some("auth-refactor"));
        assert_eq!(sessions[3].title.as_deref(), Some("claude session"));
        assert_eq!(
            cache.keys().collect::<Vec<_>>(),
            vec!["sid-1"],
            "only the session that needed the transcript was read"
        );

        std::fs::remove_file(project.join("sid-1.jsonl")).unwrap();
        entitle_sessions(&mut sessions, Some(home.path()), none, held, &mut cache);
        assert_eq!(
            sessions[0].title.as_deref(),
            Some("why is login slow"),
            "the cache answers the second poll without the file"
        );

        entitle_sessions(&mut sessions, None, none, held, &mut HashMap::new());
        assert_eq!(
            sessions[0].title.as_deref(),
            Some("budget-c2"),
            "no claude home: the derived name is the honest fallback"
        );
    }

    #[test]
    fn the_status_line_name_outranks_the_listing_name_and_moves_with_it() {
        let mut sessions = vec![
            session(
                1,
                Some("board-model-review-proposals"),
                "/w/budget",
                Some("sid-1"),
            ),
            session(2, Some("budget-c2"), "/w/budget", Some("sid-2")),
            session(3, Some("auth-refactor"), "/w/budget", Some("sid-3")),
        ];
        let none = |_: &AgentSession| None;
        let reported = |s: &AgentSession| match s.pid {
            1 => Some("Agent count and status line config".to_string()),
            2 => Some("budget-c2".to_string()),
            3 => Some("   ".to_string()),
            _ => None,
        };

        entitle_sessions(&mut sessions, None, reported, none, &mut HashMap::new());

        assert_eq!(
            sessions[0].title.as_deref(),
            Some("Agent count and status line config"),
            "the listing lagged a rename; the payload is what the status line shows"
        );
        assert_eq!(
            sessions[1].title.as_deref(),
            Some("budget-c2"),
            "a derived name is derived whichever reader reported it"
        );
        assert_eq!(
            sessions[2].title.as_deref(),
            Some("auth-refactor"),
            "a blank payload name defers to the listing"
        );
    }

    #[test]
    fn derived_default_names_are_recognised_only_against_their_cwd() {
        let cwd = Path::new("/Users/dev/code/Budget");
        assert!(is_derived_session_name("budget-c2", Some(cwd)));
        assert!(is_derived_session_name("budget-0f", Some(cwd)));
        assert!(
            !is_derived_session_name("budget-c2", None),
            "no cwd, no verdict"
        );
        assert!(!is_derived_session_name("budget-c2x", Some(cwd)));
        assert!(!is_derived_session_name("budget", Some(cwd)));
        assert!(!is_derived_session_name("auth-refactor", Some(cwd)));
        assert!(!is_derived_session_name(
            "budget-c2",
            Some(Path::new("/Users/dev/code/other"))
        ));
    }

    #[test]
    fn resolution_prefers_a_real_name_then_a_held_task_then_the_first_prompt() {
        let cwd = Some(Path::new("/Users/dev/code/budget"));
        assert_eq!(
            resolve_session_title(Some("auth-refactor"), cwd, Some("Fix login"), Some("hi"))
                .as_deref(),
            Some("auth-refactor")
        );
        assert_eq!(
            resolve_session_title(Some("budget-c2"), cwd, Some("Fix login"), Some("hi")).as_deref(),
            Some("Fix login"),
            "a derived name defers to the held task"
        );
        assert_eq!(
            resolve_session_title(Some("budget-c2"), cwd, None, Some("  why is  login\nslow "))
                .as_deref(),
            Some("why is login slow"),
            "then to the first prompt, whitespace collapsed"
        );
        assert_eq!(
            resolve_session_title(Some("budget-c2"), cwd, None, None).as_deref(),
            Some("budget-c2"),
            "the derived name is still better than nothing"
        );
        assert_eq!(resolve_session_title(None, cwd, None, None), None);
        assert_eq!(
            resolve_session_title(Some("   "), cwd, Some(""), None),
            None,
            "blank inputs count as absent"
        );
        assert_eq!(
            unnamed_session_title(crate::agent_sessions::AgentProcessKind::Codex),
            "codex session"
        );
    }

    #[test]
    fn long_titles_are_clipped_at_a_character_boundary() {
        let long = "ü".repeat(MAX_TITLE_CHARS * 2);
        let title = resolve_session_title(Some(&long), None, None, None).unwrap();
        assert_eq!(title.chars().count(), MAX_TITLE_CHARS);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn project_dir_name_replaces_every_non_alphanumeric_and_bounds_length() {
        assert_eq!(
            project_dir_name(Path::new("/Users/dev/code/my_app.v2")).as_deref(),
            Some("-Users-dev-code-my-app-v2")
        );
        let long = format!("/{}", "a".repeat(250));
        assert_eq!(project_dir_name(Path::new(&long)), None);
        assert_eq!(
            transcript_path(Path::new("/home/x/.claude"), Path::new("/w/app"), "abc").unwrap(),
            PathBuf::from("/home/x/.claude/projects/-w-app/abc.jsonl")
        );
        assert_eq!(
            transcript_path(
                Path::new("/home/x/.claude"),
                Path::new("/w/app"),
                "../../secrets"
            ),
            None
        );
    }

    #[test]
    fn first_prompt_skips_command_envelopes_meta_entries_and_bad_lines() {
        let jsonl = r#"{"type":"summary","summary":"x"}
not json at all
{"type":"user","isMeta":true,"message":{"role":"user","content":"caveat text"}}
{"type":"user","message":{"role":"user","content":"<command-message>clear</command-message>"}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"..."}]}}
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"why can't i see the PR page?"},{"type":"image"}]}}
{"type":"user","message":{"role":"user","content":"second prompt"}}"#;
        assert_eq!(
            parse_first_user_prompt(jsonl).as_deref(),
            Some("why can't i see the PR page?")
        );
        assert_eq!(parse_first_user_prompt(""), None);
        assert_eq!(
            parse_first_user_prompt(r#"{"type":"assistant","message":{"content":"hi"}}"#),
            None
        );
        let truncated = r#"{"type":"user","message":{"role":"user","content":"plain string prompt"}}
{"type":"user","message":{"role":"user","content":"cut off mid-l"#;
        assert_eq!(
            parse_first_user_prompt(truncated).as_deref(),
            Some("plain string prompt")
        );
    }

    #[test]
    fn first_prompt_reads_only_the_bounded_head_of_a_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let padding = format!(
            "{{\"type\":\"progress\",\"pad\":\"{}\"}}\n",
            "p".repeat(MAX_TRANSCRIPT_HEAD_BYTES as usize)
        );
        std::fs::write(
            &path,
            format!(
                "{padding}{{\"type\":\"user\",\"message\":{{\"content\":\"beyond the head\"}}}}\n"
            ),
        )
        .unwrap();
        assert_eq!(first_user_prompt(&path), None, "past the bound is not read");
        std::fs::write(
            &path,
            "{\"type\":\"user\",\"message\":{\"content\":\"within the head\"}}\n",
        )
        .unwrap();
        assert_eq!(first_user_prompt(&path).as_deref(), Some("within the head"));
        assert_eq!(first_user_prompt(&dir.path().join("missing.jsonl")), None);
    }
}
