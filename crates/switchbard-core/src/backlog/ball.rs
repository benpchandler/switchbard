//! Who holds the ball on a task: `me`, `agent`, or nobody.
//!
//! The ball is stored as one of two labels, [`BALL_ME_LABEL`] /
//! [`BALL_AGENT_LABEL`], so the files stay plain Backlog.md and every reader
//! that only knows labels still shows it. The dispatch pipeline's own
//! `dispatching` label counts as the agent holding it — a task an agent is
//! actively working is, by definition, in the agent's court.
//!
//! This module is the single authority for that vocabulary. The TUI's `b`
//! key and `sb edit --ball` both write through
//! [`super::mutations::set_backlog_ball`]; neither spells the label names.

use super::types::BacklogTask;
use crate::dispatch::DISPATCHING_LABEL;
use anyhow::{bail, Result};

pub const BALL_ME_LABEL: &str = "ball:me";
pub const BALL_AGENT_LABEL: &str = "ball:agent";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ball {
    Me,
    Agent,
}

impl Ball {
    /// Who holds the ball on `task`, reading its labels.
    pub fn of(task: &BacklogTask) -> Option<Ball> {
        if task.labels.iter().any(|label| label == BALL_ME_LABEL) {
            Some(Ball::Me)
        } else if task
            .labels
            .iter()
            .any(|label| label == BALL_AGENT_LABEL || label == DISPATCHING_LABEL)
        {
            Some(Ball::Agent)
        } else {
            None
        }
    }

    /// The word a surface prints: `me`, `agent`, or empty for nobody.
    pub fn text(ball: Option<Ball>) -> &'static str {
        match ball {
            Some(Ball::Me) => "me",
            Some(Ball::Agent) => "agent",
            None => "",
        }
    }

    /// The TUI's `b` cycle: nobody → me → agent → nobody.
    pub fn next(ball: Option<Ball>) -> Option<Ball> {
        match ball {
            None => Some(Ball::Me),
            Some(Ball::Me) => Some(Ball::Agent),
            Some(Ball::Agent) => None,
        }
    }

    /// The label that stores this holder.
    pub fn label(ball: Ball) -> &'static str {
        match ball {
            Ball::Me => BALL_ME_LABEL,
            Ball::Agent => BALL_AGENT_LABEL,
        }
    }

    /// Parse a CLI word: `me`, `agent`, or `none` (nobody), case-insensitive.
    pub fn parse(word: &str) -> Result<Option<Ball>> {
        match word.trim().to_ascii_lowercase().as_str() {
            "me" => Ok(Some(Ball::Me)),
            "agent" => Ok(Some(Ball::Agent)),
            "none" => Ok(None),
            other => bail!("--ball takes me, agent, or none (got `{other}`)"),
        }
    }
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
        // `me` wins when both are somehow present: a human claim is explicit.
        assert_eq!(
            Ball::of(&task_with_labels(&["ball:agent", "ball:me"])),
            Some(Ball::Me)
        );
    }

    #[test]
    fn cycle_and_words_round_trip() {
        assert_eq!(Ball::next(None), Some(Ball::Me));
        assert_eq!(Ball::next(Some(Ball::Me)), Some(Ball::Agent));
        assert_eq!(Ball::next(Some(Ball::Agent)), None);
        for holder in [None, Some(Ball::Me), Some(Ball::Agent)] {
            let word = if holder.is_none() {
                "none"
            } else {
                Ball::text(holder)
            };
            assert_eq!(Ball::parse(word).expect("valid word"), holder);
        }
        assert!(Ball::parse("them").is_err());
    }
}
