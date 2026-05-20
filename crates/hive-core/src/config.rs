//! Persisted Hive config — `~/.hive/config.toml`.
//!
//! On first run the file is missing and we return `Config::default()`. Users
//! add repos via the GUI (file picker), which writes here. The file is
//! intentionally hand-editable: it's TOML, well-formed, no machine-specific
//! magic. If a future version needs migration, bump `version` and branch on
//! it during load.
//!
//! There is exactly one canonical path so the GUI doesn't have to thread it
//! through every call site. Tests use `save_to` / `load_from` with a temp dir.

use crate::types::Repo;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const RELATIVE_PATH: &str = ".hive/config.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Schema version. Reserved — currently always 1. Lets a future load()
    /// fork on shape changes without breaking older files.
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub repos: Vec<Repo>,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiConfig {
    /// Selected browser. `None` or the empty string means "system default";
    /// otherwise one of the names in `BROWSER_APP_NAMES`.
    #[serde(default)]
    pub browser: Option<String>,
    /// Whether the Listeners view groups by repo/worktree. Default true.
    #[serde(default = "default_true")]
    pub group_listeners: bool,
    /// Whether the Servers view hides NotServer-classified rows. Default false
    /// (i.e. hide them — matches the current GUI default).
    #[serde(default)]
    pub show_non_servers: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            browser: None,
            group_listeners: true,
            show_non_servers: false,
        }
    }
}

fn default_version() -> u32 {
    1
}
fn default_true() -> bool {
    true
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
                "hive: config load failed ({}); backed up to {} and starting fresh",
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
    let text = toml::to_string_pretty(config)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, text)
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
            ui: UiConfig {
                browser: Some("Safari".into()),
                group_listeners: false,
                show_non_servers: true,
            },
        };
        save_to(&path, &cfg).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded, cfg);
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
        assert!(
            cfg.ui.group_listeners,
            "ui.group_listeners should default to true"
        );
        assert!(!cfg.ui.show_non_servers);
        assert_eq!(cfg.ui.browser, None);
    }
}
