//! Validated setup choices and safe native config serialization.
use anyhow::{ensure, Result};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySetupOptions {
    pub task_prefix: String,
    pub statuses: Vec<String>,
}

impl Default for RepositorySetupOptions {
    fn default() -> Self {
        Self {
            task_prefix: "TASK".into(),
            statuses: vec!["To Do".into(), "In Progress".into(), "Done".into()],
        }
    }
}

impl RepositorySetupOptions {
    pub fn validated(&self) -> Result<Self> {
        Ok(Self {
            task_prefix: validated_prefix(&self.task_prefix)?,
            statuses: validated_statuses(&self.statuses)?,
        })
    }

    pub(crate) fn config_bytes(&self) -> Result<Vec<u8>> {
        Ok(format!(
            "task_prefix: {}\ndefault_status: {}\nstatuses: {}\n",
            serde_json::to_string(&self.task_prefix)?,
            serde_json::to_string(&self.statuses[0])?,
            serde_json::to_string(&self.statuses)?
        )
        .into_bytes())
    }
}

impl RepositorySetupOptions {
    pub(crate) fn matches_config(&self, bytes: &[u8]) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct Config {
            task_prefix: Option<String>,
            default_status: Option<String>,
            statuses: Option<Vec<String>>,
        }
        let config: Config = serde_yaml::from_slice(bytes)?;
        Ok(
            config.task_prefix.as_deref() == Some(self.task_prefix.as_str())
                && config.default_status.as_deref() == Some(self.statuses[0].as_str())
                && config.statuses.as_ref() == Some(&self.statuses),
        )
    }
}

fn validated_prefix(value: &str) -> Result<String> {
    let prefix = value.trim().to_ascii_uppercase();
    ensure!(
        !prefix.is_empty()
            && !prefix.starts_with('-')
            && prefix.len() <= 24
            && prefix
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
        "task prefix must contain 1-24 characters from A-Z, 0-9, hyphens or underscores, and cannot start with a hyphen"
    );
    Ok(prefix)
}

fn validated_statuses(values: &[String]) -> Result<Vec<String>> {
    ensure!(
        (1..=20).contains(&values.len()),
        "provide between 1 and 20 workflow statuses"
    );
    let mut seen = BTreeSet::new();
    let mut statuses = Vec::new();
    for label in values {
        let label = label.trim();
        ensure!(
            !label.is_empty()
                && label.len() <= 256
                && label.chars().count() <= 64
                && !label.chars().any(char::is_control),
            "each status must contain 1–64 printable characters"
        );
        ensure!(
            seen.insert(label.to_lowercase()),
            "workflow statuses must be unique: {label}"
        );
        statuses.push(label.to_string());
    }
    Ok(statuses)
}
