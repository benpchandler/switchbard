//! Who holds the ball on a task: `me`, `agent`, or nobody. Read from a `ball:` label,
//! with the dispatch pipeline's own `dispatching` label counting as the agent.

use switchbard_core::BacklogTask;

pub const ME_LABEL: &str = "ball:me";
pub const AGENT_LABEL: &str = "ball:agent";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ball {
    Me,
    Agent,
}

impl Ball {
    pub fn of(task: &BacklogTask) -> Option<Ball> {
        if task.labels.iter().any(|label| label == ME_LABEL) {
            Some(Ball::Me)
        } else if task
            .labels
            .iter()
            .any(|label| label == AGENT_LABEL || label == "dispatching")
        {
            Some(Ball::Agent)
        } else {
            None
        }
    }

    pub fn text(ball: Option<Ball>) -> &'static str {
        match ball {
            Some(Ball::Me) => "me",
            Some(Ball::Agent) => "agent",
            None => "",
        }
    }

    /// `b` cycles nobody → me → agent → nobody.
    pub fn next(ball: Option<Ball>) -> Option<Ball> {
        match ball {
            None => Some(Ball::Me),
            Some(Ball::Me) => Some(Ball::Agent),
            Some(Ball::Agent) => None,
        }
    }

    pub fn label(ball: Ball) -> &'static str {
        match ball {
            Ball::Me => ME_LABEL,
            Ball::Agent => AGENT_LABEL,
        }
    }
}
