//! Persisted Switchbard config — `~/.switchbard/config.toml`.
//!
//! On first run the file is missing and we return `Config::default()`. Users
//! add repos via the GUI (file picker), which writes here. The file is
//! intentionally hand-editable: it's TOML, well-formed, no machine-specific
//! magic. If a future version needs migration, bump `version` and branch on
//! it during load.
//!
//! There is exactly one canonical path so the GUI doesn't have to thread it
//! through every call site. Tests use `save_to` / `load_from` with a temp dir.

use crate::types::{Repo, WorktreeAlias};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const RELATIVE_PATH: &str = ".switchbard/config.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// Schema version. Reserved — currently always 1. Lets a future load()
    /// fork on shape changes without breaking older files.
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub repos: Vec<Repo>,
    #[serde(default)]
    pub worktrees: Vec<WorktreeAlias>,
    #[serde(default)]
    pub ui: UiConfig,
    /// Repos whose status list the user has explicitly chosen to leave as it
    /// is, so the standardization offer stops asking.
    ///
    /// A decline is per-repo and sticky by design: the alternative is a
    /// prompt that reappears every time the Backlog view opens, which trains
    /// people to dismiss dialogs without reading them. The board still shows
    /// exactly what such a repo declares — declining changes what we *ask*,
    /// never what we *show*.
    #[serde(default)]
    pub status_standardization_declined: Vec<PathBuf>,
}

// Note: not `Eq` — `ui_scale` is an `f32`. `PartialEq` is all the tests (and
// the change-detection in `update`) need.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiConfig {
    /// Selected browser. `None` or the empty string means "system default";
    /// otherwise one of the names in `BROWSER_APP_NAMES`.
    #[serde(default)]
    pub browser: Option<String>,
    /// Whether the Servers view shows NotServer-classified rows. Default false
    /// (i.e. hide them).
    #[serde(default)]
    pub show_non_servers: bool,
    /// True once the user has either accepted or explicitly dismissed the
    /// first-launch onboarding modal. We never re-open it later (would be
    /// annoying if they remove all repos), so this is a one-shot flag.
    #[serde(default)]
    pub onboarding_dismissed: bool,
    /// Which of the two named palettes (task-14) the GUI paints with: Flight
    /// Strips (light) or Operator's Console (dark). Plain data — no egui
    /// dependency here, matching this crate's zero-UI-deps rule; `ui::theme`
    /// is the only place that turns this into actual `Color32`s.
    #[serde(default)]
    pub theme: ThemeChoice,
    /// Global UI zoom applied via egui `set_zoom_factor` (1.0 = the display's
    /// native scale). Persisted here because eframe isn't built with its
    /// `persistence` feature, so its own zoom memory doesn't survive a restart.
    /// Clamped to a legible band on apply (`app::clamp_ui_scale`).
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    /// How many days without an update before the Backlog's staleness filter
    /// considers a task stale. Persisted rather than session-only because it
    /// encodes a judgement about a particular backlog's pace — a repo whose
    /// tasks turn over weekly and one that plans in quarters do not share a
    /// threshold — and because it gates a bulk action, where re-picking it
    /// from memory each session is exactly how the wrong set gets archived.
    #[serde(default = "default_stale_after_days")]
    pub stale_after_days: u32,
    /// Named Backlog filter+sort+lens combinations (task-20). Engagement
    /// state only — repos stay the system of record for task data, this is
    /// just "how I like to look at it" — so it's additive on the existing
    /// `Config::ui` single source of truth rather than a new store.
    #[serde(default)]
    pub saved_views: Vec<SavedView>,
    /// TASK-27 (owner-requested UX): whether the "Tracked repos" side panel
    /// is collapsed to a thin rail. Additive — defaults to `false`
    /// (expanded), so an existing config with no `sidebar_collapsed` key
    /// loads exactly as it did before this field existed.
    #[serde(default)]
    pub sidebar_collapsed: bool,
    /// Last-used filter state by stable UI surface key. The generic query +
    /// named-facet shape lets a new view participate without another config
    /// migration or a dependency from core onto GUI enums.
    #[serde(default)]
    pub filters: BTreeMap<String, FilterMemory>,
}

// Hand-written so the default scale is 1.0, not the f32 `Default` of 0.0 (which
// would blank the window). Also the value a missing `[ui]` section loads with.
impl Default for UiConfig {
    fn default() -> Self {
        Self {
            browser: None,
            show_non_servers: false,
            onboarding_dismissed: false,
            theme: ThemeChoice::default(),
            ui_scale: default_ui_scale(),
            stale_after_days: default_stale_after_days(),
            saved_views: Vec::new(),
            sidebar_collapsed: false,
            filters: BTreeMap::new(),
        }
    }
}

/// Persisted last-used state for one filter surface.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilterMemory {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub facets: BTreeMap<String, String>,
}

/// One named Backlog view: the filter/sort/lens combination task-20 lets a
/// user save and restore. `lens`, `sort_key`, and `sort_direction` are
/// stored as plain strings rather than referencing the GUI crate's
/// `BacklogLens`/`BacklogTaskSortKey`/`BacklogTaskSortDirection` enums —
/// this crate has zero UI dependencies by design (see the crate doc), so a
/// config type can't name a `switchbard-gui` type. The GUI layer owns
/// matching these strings back to its own enums (unrecognized values fall
/// back to that enum's default rather than erroring — a saved view from an
/// older Switchbard build should degrade gracefully, not corrupt the file).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedView {
    pub name: String,
    /// `None` means the All-repos scope. Named to match the GUI's own
    /// `BacklogViewState::selected_repo` — deliberately *not*
    /// `repo_filter`, which is a different, unrelated field there (the
    /// repo picker's free-text search string). Serialized under the
    /// pre-rename key `selected_project` so existing config files keep
    /// loading and older builds keep reading ours.
    #[serde(default, rename = "selected_project")]
    pub selected_repo: Option<PathBuf>,
    #[serde(default = "default_filter_all")]
    pub status_filter: String,
    #[serde(default = "default_filter_all")]
    pub priority_filter: String,
    #[serde(default = "default_filter_all")]
    pub milestone_filter: String,
    #[serde(default = "default_filter_all")]
    pub label_filter: String,
    #[serde(default)]
    pub sort_key: String,
    #[serde(default)]
    pub sort_direction: String,
    #[serde(default)]
    pub lens: String,
    #[serde(default)]
    pub show_completed: bool,
    #[serde(default)]
    pub show_archived: bool,
    #[serde(default = "default_true")]
    pub show_drafts: bool,
}

fn default_filter_all() -> String {
    "all".to_string()
}

fn default_true() -> bool {
    true
}

/// The two named palettes from the task-14 design-directions artifact:
/// direction B ("Flight Strips") for light, direction A's lamp language
/// ("Operator's Console") for dark — the owner-confirmed pairing recorded on
/// task-15's Implementation Notes.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    Light,
    Dark,
}

impl ThemeChoice {
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

fn default_version() -> u32 {
    1
}

fn default_ui_scale() -> f32 {
    1.0
}

/// A quarter. Long enough that ordinary in-flight work is never called stale,
/// short enough to surface a backlog nobody has revisited in a planning cycle.
fn default_stale_after_days() -> u32 {
    90
}

/// The single canonical config path. Returns `None` only if `dirs::home_dir`
/// can't find a home directory (essentially never on macOS).
pub fn default_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(RELATIVE_PATH))
}

/// Load the config from the canonical path. Returns `Config::default()` if the
/// file is missing OR malformed — the user shouldn't be locked out of the app
/// by a stray edit. Malformed loads also write a `.broken-<ts>.toml` backup
/// next to the file so the data isn't silently lost.
pub fn load() -> Config {
    let Some(path) = default_path() else {
        return Config::default();
    };
    match load_from(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Config::default(),
        Err(e) => {
            // Preserve the bad file before we overwrite it on next save.
            let backup = path.with_extension(format!(
                "broken-{}.toml",
                chrono::Utc::now().format("%Y%m%d-%H%M%S")
            ));
            let _ = fs::copy(&path, &backup);
            eprintln!(
                "switchbard: config load failed ({}); backed up to {} and starting fresh",
                e,
                backup.display()
            );
            Config::default()
        }
    }
}

pub fn load_from(path: &Path) -> io::Result<Config> {
    let text = fs::read_to_string(path)?;
    toml::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn save(config: &Config) -> io::Result<()> {
    let Some(path) = default_path() else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no home directory"));
    };
    save_to(&path, config)
}

pub fn save_to(path: &Path, config: &Config) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    tombstone_if_wiping_repos(path, config)?;
    let text = toml::to_string_pretty(config)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, path)
}

/// TASK-22 follow-up: the incident's root cause was a test harness writing
/// to the real config (fixed separately), but "an empty `repos` write
/// silently clobbers a non-empty on-disk list" was named as a risk in its
/// own right and left open. This is the guard: if `config` would write an
/// empty `repos` list over a file that currently has a non-empty one, the
/// existing file is preserved first as a timestamped `config.tombstone-
/// <ts>.toml` sidecar — mirroring `load()`'s `.broken-<ts>.toml` pattern —
/// so the write still proceeds (an intentional "remove all repos" is a
/// legitimate user action) but is never irrecoverable.
///
/// Never fails the save over this: an unreadable/unparsable/missing
/// existing file, or a tombstone write that itself fails, just means there
/// was nothing to protect (or protection isn't possible) — the save
/// continues either way rather than blocking the user's action.
fn tombstone_if_wiping_repos(path: &Path, config: &Config) -> io::Result<()> {
    if !config.repos.is_empty() {
        return Ok(());
    }
    let Ok(existing_text) = fs::read_to_string(path) else {
        return Ok(());
    };
    let Ok(existing) = toml::from_str::<Config>(&existing_text) else {
        return Ok(());
    };
    if existing.repos.is_empty() {
        return Ok(());
    }
    let tombstone = path.with_extension(format!(
        "tombstone-{}.toml",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));
    let _ = fs::write(&tombstone, existing_text);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/config.toml");
        let cfg = Config::default();
        save_to(&path, &cfg).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn round_trips_with_repos() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = Config {
            version: 1,
            repos: vec![
                Repo {
                    name: "foo".into(),
                    path: PathBuf::from("/Users/me/foo"),
                },
                Repo {
                    name: "bar".into(),
                    path: PathBuf::from("/Users/me/bar"),
                },
            ],
            worktrees: vec![],
            status_standardization_declined: Vec::new(),
            ui: UiConfig {
                browser: Some("Safari".into()),
                show_non_servers: true,
                onboarding_dismissed: true,
                theme: ThemeChoice::Dark,
                ui_scale: 1.25,
                stale_after_days: 45,
                saved_views: vec![SavedView {
                    name: "My high-priority queue".into(),
                    selected_repo: None,
                    status_filter: "all".into(),
                    priority_filter: "high".into(),
                    milestone_filter: "all".into(),
                    label_filter: "all".into(),
                    sort_key: "triage".into(),
                    sort_direction: "ascending".into(),
                    lens: "list".into(),
                    show_completed: false,
                    show_archived: false,
                    show_drafts: true,
                }],
                sidebar_collapsed: true,
                filters: BTreeMap::from([(
                    "agents.hooks".into(),
                    FilterMemory {
                        query: "format".into(),
                        facets: BTreeMap::from([("event".into(), "PostToolUse".into())]),
                    },
                )]),
            },
        };
        save_to(&path, &cfg).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn round_trips_worktree_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = Config {
            version: 1,
            repos: vec![Repo {
                name: "switchbard".into(),
                path: PathBuf::from("/Users/me/Dev/switchbard"),
            }],
            worktrees: vec![crate::types::WorktreeAlias {
                repo_path: PathBuf::from("/Users/me/Dev/switchbard"),
                worktree_path: PathBuf::from("/Users/me/Dev/.worktrees/switchbard/agents"),
                name: "agents".into(),
            }],
            status_standardization_declined: Vec::new(),
            ui: UiConfig::default(),
        };

        save_to(&path, &cfg).unwrap();
        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.worktrees[0].name, "agents");
        assert_eq!(
            loaded.worktrees[0].worktree_path,
            PathBuf::from("/Users/me/Dev/.worktrees/switchbard/agents")
        );
    }

    #[test]
    fn old_configs_load_with_empty_worktree_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"
version = 1

[[repos]]
name = "switchbard"
path = "/Users/me/Dev/switchbard"
"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();

        assert!(loaded.worktrees.is_empty());
    }

    #[test]
    fn missing_file_is_not_an_error_via_default_only() {
        // load_from returns Err, but the public `load()` would surface the
        // default config. We exercise the lower layer here.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.toml");
        let err = load_from(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn malformed_returns_invalid_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        fs::write(&path, "this is = ][ not toml").unwrap();
        let err = load_from(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn save_over_existing_replaces_content_and_leaves_no_tmp_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let tmp = path.with_extension("toml.tmp");

        // Write an initial config, then overwrite with a different one.
        let first = Config::default();
        save_to(&path, &first).unwrap();

        let second = Config {
            version: 1,
            repos: vec![Repo {
                name: "myrepo".into(),
                path: PathBuf::from("/Users/me/myrepo"),
            }],
            worktrees: vec![],
            status_standardization_declined: Vec::new(),
            ui: UiConfig::default(),
        };
        save_to(&path, &second).unwrap();

        // The live file reflects the second write.
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.repos.len(), 1);
        assert_eq!(loaded.repos[0].name, "myrepo");

        // The tmp sidecar must not linger after a successful save.
        assert!(
            !tmp.exists(),
            ".toml.tmp sidecar should not remain after successful save"
        );
    }

    #[test]
    fn wiping_repos_over_a_non_empty_config_writes_a_tombstone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let populated = Config {
            version: 1,
            repos: vec![Repo {
                name: "switchbard".into(),
                path: PathBuf::from("/Users/me/Dev/switchbard"),
            }],
            worktrees: vec![],
            status_standardization_declined: Vec::new(),
            ui: UiConfig::default(),
        };
        save_to(&path, &populated).unwrap();

        // Simulate a save that would wipe the repos list.
        let emptied = Config::default();
        save_to(&path, &emptied).unwrap();

        // The write still proceeds — removing all repos is a legitimate
        // user action — but a recovery point exists.
        let loaded = load_from(&path).unwrap();
        assert!(loaded.repos.is_empty());

        let tombstones: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains("tombstone"))
            .collect();
        assert_eq!(tombstones.len(), 1, "expected exactly one tombstone file");

        let tombstone_text = fs::read_to_string(dir.path().join(&tombstones[0])).unwrap();
        let recovered: Config = toml::from_str(&tombstone_text).unwrap();
        assert_eq!(recovered, populated);
    }

    #[test]
    fn wiping_repos_when_already_empty_writes_no_tombstone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        save_to(&path, &Config::default()).unwrap();
        save_to(&path, &Config::default()).unwrap();

        let tombstones = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("tombstone"))
            .count();
        assert_eq!(tombstones, 0);
    }

    #[test]
    fn saving_a_non_empty_repos_list_writes_no_tombstone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let cfg = Config {
            version: 1,
            repos: vec![Repo {
                name: "switchbard".into(),
                path: PathBuf::from("/Users/me/Dev/switchbard"),
            }],
            worktrees: vec![],
            status_standardization_declined: Vec::new(),
            ui: UiConfig::default(),
        };
        save_to(&path, &cfg).unwrap();
        // A second non-empty save (e.g. adding another repo) is not a wipe.
        save_to(&path, &cfg).unwrap();

        let tombstones = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("tombstone"))
            .count();
        assert_eq!(tombstones, 0);
    }

    #[test]
    fn first_ever_save_of_an_empty_config_writes_no_tombstone() {
        // No existing file at all — nothing to protect.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        save_to(&path, &Config::default()).unwrap();

        let tombstones = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("tombstone"))
            .count();
        assert_eq!(tombstones, 0);
    }

    #[test]
    fn ui_defaults_when_unset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.toml");
        // A user could hand-edit and leave [ui] off entirely — we must still
        // load with sensible defaults.
        fs::write(
            &path,
            "version = 1\n[[repos]]\nname = \"a\"\npath = \"/a\"\n",
        )
        .unwrap();
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg.repos.len(), 1);
        assert!(!cfg.ui.show_non_servers);
        assert_eq!(cfg.ui.browser, None);
    }
}
