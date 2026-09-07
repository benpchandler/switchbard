//! Who holds the ball on a task: `me`, `agent`, a named person, or nobody.
//!
//! The ball is stored as a `ball:<holder>` label, so the files stay plain
//! Backlog.md and every reader that only knows labels still shows it. The
//! dispatch pipeline's `dispatching` label counts as the agent holding it.
//!
//! This module is the single authority for that vocabulary. The TUI's `b`
//! key and `sb edit --ball` both write through
//! [`super::mutations::set_backlog_ball`].

use super::types::BacklogTask;
use crate::dispatch::DISPATCHING_LABEL;
use anyhow::{bail, Result};

pub const BALL_ME_LABEL: &str = "ball:me";
pub const BALL_AGENT_LABEL: &str = "ball:agent";
pub const BALL_LABEL_PREFIX: &str = "ball:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ball {
    Me,
    Agent,
    Other(String),
}

impl Ball {
    /// Who holds the ball on `task`, reading its labels.
    pub fn of(task: &BacklogTask) -> Option<Ball> {
        if task.labels.iter().any(|label| label == BALL_ME_LABEL) {
            return Some(Ball::Me);
        }
        if let Some(holder) = task.labels.iter().find_map(|label| {
            label
                .strip_prefix(BALL_LABEL_PREFIX)
                .filter(|holder| !matches!(*holder, "me" | "agent" | "none"))
                .filter(|holder| is_holder(holder))
                .map(ToString::to_string)
        }) {
            return Some(Ball::Other(holder));
        }
        if task
            .labels
            .iter()
            .any(|label| label == BALL_AGENT_LABEL || label == DISPATCHING_LABEL)
        {
            return Some(Ball::Agent);
        }
        None
    }

    /// The word a surface prints. Named holders use their canonical label text.
    pub fn text(&self) -> &str {
        match self {
            Ball::Me => "me",
            Ball::Agent => "agent",
            Ball::Other(holder) => holder,
        }
    }

    /// The TUI's `b` cycle: nobody → me → agent → nobody.
    ///
    /// A named holder is not overwritten by the quick cycle: `b` drops it,
    /// after which the familiar cycle resumes. Assign names with
    /// `sb edit TASK-1 --ball <person>`.
    pub fn next(ball: Option<&Ball>) -> Option<Ball> {
        match ball {
            None => Some(Ball::Me),
            Some(Ball::Me) => Some(Ball::Agent),
            Some(Ball::Agent | Ball::Other(_)) => None,
        }
    }

    /// The label that stores this holder.
    pub fn label(&self) -> String {
        match self {
            Ball::Me => BALL_ME_LABEL.to_string(),
            Ball::Agent => BALL_AGENT_LABEL.to_string(),
            Ball::Other(holder) => format!("{BALL_LABEL_PREFIX}{holder}"),
        }
    }

    /// Parse a CLI word: `me`, `agent`, a person's name, or `none`.
    pub fn parse(word: &str) -> Result<Option<Ball>> {
        let holder = word
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
            .to_ascii_lowercase();
        match holder.as_str() {
            "me" => Ok(Some(Ball::Me)),
            "agent" => Ok(Some(Ball::Agent)),
            "none" => Ok(None),
            _ if is_holder(&holder) => Ok(Some(Ball::Other(holder))),
            _ => bail!("--ball takes me, agent, a named person, or none (got `{holder}`)"),
        }
    }
}

fn is_holder(holder: &str) -> bool {
    !holder.is_empty()
        && holder
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

#[cfg(test)]
mod tests {
    use super::super::types::BacklogTaskSource;
    use super::*;
    use std::path::PathBuf;

    fn task_with_labels(labels: &[&str]) -> BacklogTask {
        BacklogTask {
            id: "TASK-1".to_string(),
            title: "Fixture".to_string(),
            status: "To Do".to_string(),
            priority: "medium".to_string(),
            assignees: vec![],
            labels: labels.iter().map(|s| s.to_string()).collect(),
            dependencies: vec![],
            references: vec![],
            project: None,
            parent: None,
            created_date: None,
            updated_date: None,
            description: String::new(),
            implementation_plan: String::new(),
            implementation_notes: String::new(),
            final_summary: String::new(),
            acceptance_criteria: vec![],
            definition_of_done: vec![],
            source: BacklogTaskSource::Active,
            path: PathBuf::new(),
        }
    }

    #[test]
    fn reads_the_holder_from_labels_with_dispatching_as_agent() {
        assert_eq!(Ball::of(&task_with_labels(&[])), None);
        assert_eq!(Ball::of(&task_with_labels(&["ball:me"])), Some(Ball::Me));
        assert_eq!(
            Ball::of(&task_with_labels(&["ball:agent"])),
            Some(Ball::Agent)
        );
        assert_eq!(
            Ball::of(&task_with_labels(&["dispatching"])),
            Some(Ball::Agent)
        );
        assert_eq!(
            Ball::of(&task_with_labels(&["ball:nick"])),
            Some(Ball::Other("nick".to_string()))
        );
        assert_eq!(
            Ball::of(&task_with_labels(&["ball:agent", "ball:me"])),
            Some(Ball::Me)
        );
    }

    #[test]
    fn cycle_and_words_round_trip() {
        assert_eq!(Ball::next(None), Some(Ball::Me));
        assert_eq!(Ball::next(Some(&Ball::Me)), Some(Ball::Agent));
        assert_eq!(Ball::next(Some(&Ball::Agent)), None);
        assert_eq!(Ball::next(Some(&Ball::Other("nick".to_string()))), None);
        assert_eq!(Ball::parse("none").expect("none"), None);
        assert_eq!(Ball::parse("me").expect("me"), Some(Ball::Me));
        assert_eq!(Ball::parse("agent").expect("agent"), Some(Ball::Agent));
        assert_eq!(
            Ball::parse("Nick Doe").expect("name"),
            Some(Ball::Other("nick-doe".to_string()))
        );
        assert!(Ball::parse("ball:nick").is_err());
    }
}
