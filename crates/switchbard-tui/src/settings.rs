//! Standing preferences that apply under every view in every repo, edited from
//! the `,` panel and kept in `~/.switchbard/settings.lua` (a record file: sbt
//! writes it; hand edits are fine but the panel is the intended way).

use std::path::{Path, PathBuf};

use mlua::{Lua, Table};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// Statuses hidden everywhere unless a view's own filter names them.
    pub hidden_statuses: Vec<String>,
}

impl Settings {
    /// The filter terms applied under a view's filter. A view that filters on
    /// status explicitly wins, so `status:done` still shows Done tasks.
    pub fn base_filter(&self, view_filter: &str) -> String {
        if self.hidden_statuses.is_empty() || view_filter.contains("status:") {
            return String::new();
        }
        self.hidden_statuses
            .iter()
            .map(|status| format!("status:!{}", crate::tasks::Filter::loose_key(status)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn is_hidden(&self, status: &str) -> bool {
        let key = crate::tasks::Filter::loose_key(status);
        self.hidden_statuses
            .iter()
            .any(|known| crate::tasks::Filter::loose_key(known) == key)
    }

    pub fn toggle_hidden(&mut self, status: &str) {
        let key = crate::tasks::Filter::loose_key(status);
        match self
            .hidden_statuses
            .iter()
            .position(|known| crate::tasks::Filter::loose_key(known) == key)
        {
            Some(index) => {
                self.hidden_statuses.remove(index);
            }
            None => self.hidden_statuses.push(status.to_string()),
        }
    }

    /// What the title bar shows while something is hidden.
    pub fn label(&self) -> Option<String> {
        if self.hidden_statuses.is_empty() {
            return None;
        }
        Some(format!(
            "hide:{}",
            self.hidden_statuses
                .iter()
                .map(|s| crate::tasks::Filter::loose_key(s))
                .collect::<Vec<_>>()
                .join(",")
        ))
    }
}

pub fn global_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".switchbard").join("settings.lua"))
}

pub fn repo_path(repo_root: &Path) -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join(".switchbard")
            .join("settings")
            .join(format!("{}.lua", crate::views::repo_file_key(repo_root)))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Repo,
}

/// The global settings and this repo's override; the override wins whole when
/// it exists. The `,` panel writes the repo file; `g` in it promotes to global.
pub struct SettingsStore {
    global_path: Option<PathBuf>,
    repo_path: Option<PathBuf>,
    global: Settings,
    repo: Option<Settings>,
}

impl SettingsStore {
    pub fn load(
        global_path: Option<PathBuf>,
        repo_path: Option<PathBuf>,
    ) -> (SettingsStore, Vec<String>) {
        let mut warnings = Vec::new();
        let (global, warning) = read(global_path.as_deref());
        warnings.extend(warning);
        let repo = match repo_path.as_deref() {
            Some(path) if path.exists() => {
                let (settings, warning) = read(Some(path));
                warnings.extend(warning);
                Some(settings)
            }
            _ => None,
        };
        (
            SettingsStore {
                global_path,
                repo_path,
                global,
                repo,
            },
            warnings,
        )
    }

    pub fn effective(&self) -> &Settings {
        self.repo.as_ref().unwrap_or(&self.global)
    }

    pub fn scope(&self) -> Scope {
        if self.repo.is_some() {
            Scope::Repo
        } else {
            Scope::Global
        }
    }

    /// Edit this repo's settings (starting from the effective ones) and write them.
    pub fn edit_repo(&mut self, change: impl FnOnce(&mut Settings)) -> Result<(), String> {
        let mut settings = self.effective().clone();
        change(&mut settings);
        self.repo = Some(settings);
        write(
            self.repo_path.as_deref(),
            self.repo.as_ref().expect("just set"),
            Scope::Repo,
        )
    }

    /// Make the effective settings the global ones and drop the repo override.
    pub fn promote(&mut self) -> Result<(), String> {
        self.global = self.effective().clone();
        self.repo = None;
        write(self.global_path.as_deref(), &self.global, Scope::Global)?;
        if let Some(path) = self.repo_path.as_deref() {
            if path.exists() {
                std::fs::remove_file(path)
                    .map_err(|error| format!("could not remove {}: {error}", path.display()))?;
            }
        }
        Ok(())
    }
}

/// Missing file: defaults. Broken file: defaults plus the error.
fn read(path: Option<&Path>) -> (Settings, Option<String>) {
    let Some(path) = path else {
        return (Settings::default(), None);
    };
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (Settings::default(), None)
        }
        Err(error) => {
            return (
                Settings::default(),
                Some(format!("{}: {error}", path.display())),
            )
        }
    };
    let lua = Lua::new();
    let parsed = lua.load(&source).eval::<Table>().and_then(|table| {
        let hidden: Option<Table> = table.get("hide_statuses")?;
        Ok(Settings {
            hidden_statuses: match hidden {
                Some(list) => list.sequence_values::<String>().collect::<Result<_, _>>()?,
                None => Vec::new(),
            },
        })
    });
    match parsed {
        Ok(settings) => (settings, None),
        Err(error) => (
            Settings::default(),
            Some(format!("{}: {error}", path.display())),
        ),
    }
}

fn write(path: Option<&Path>, settings: &Settings, scope: Scope) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    let hidden = settings
        .hidden_statuses
        .iter()
        .map(|s| format!("\"{}\"", s.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    let header = match scope {
        Scope::Global => "-- sbt settings for every repo. `,` inside sbt edits a repo's; `g` there promotes them here.",
        Scope::Repo => "-- sbt settings for this repo, overriding ~/.switchbard/settings.lua. `,` inside sbt edits this.",
    };
    let text = format!("{header}\nreturn {{\n  hide_statuses = {{ {hidden} }},\n}}\n");
    let attempt = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("lua.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    };
    attempt().map_err(|error| format!("could not write {}: {error}", path.display()))
}
