//! One selected agent's confirmation and off-thread positive-PID signal.
use super::{App, Mode};
use crate::page::Page;
use crate::picker::{Payload, PickOption, PickerPurpose};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use switchbard_core::{AgentProcessIdentity, AgentTerminationOutcome, PreparedAgentTermination};

enum Completion {
    Prepared(Result<PreparedAgentTermination, String>),
    Sent(Result<AgentTerminationOutcome, String>),
}

#[derive(Default)]
pub struct AgentKillFlow {
    target: Option<AgentProcessIdentity>,
    prepared: Option<PreparedAgentTermination>,
    pending: Option<Receiver<Completion>>,
    submitting: bool,
    pub confirmation_visible: bool,
}

impl AgentKillFlow {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn is_submitting(&self) -> bool {
        self.submitting
    }

    pub fn confirmation_lines(&self) -> Vec<String> {
        self.prepared
            .as_ref()
            .map(|prepared| {
                let identity = prepared.identity();
                vec![
                    format!(
                        "Send SIGTERM to {} PID {}?",
                        identity.kind.label(),
                        identity.pid
                    ),
                    format!("cwd: {}", identity.cwd.display()),
                    format!("executable: {}", identity.executable.display()),
                    format!(
                        "OS birth token: {}:{}",
                        identity.start_token.0, identity.start_token.1
                    ),
                    "Session labels may be stale; verify the process identity above.".into(),
                    "Unsaved work may be lost. Only this agent process is signalled.".into(),
                    "Children may remain. No task edits or manual claim release.".into(),
                    "Live claims clear normally when the agent exits.".into(),
                ]
            })
            .unwrap_or_default()
    }
}

impl App {
    pub(super) fn open_agent_kill(&mut self) {
        if self.agent_kill.is_pending() {
            self.status = "Agent signal operation already pending".into();
            return;
        }
        let Some(row) = self.agents.row().cloned() else {
            self.status = "No agent selected".into();
            return;
        };
        self.agent_kill.target = row.process_identity.clone();
        if row.process_identity.is_none() {
            self.fail("Agent identity unavailable; refresh before retrying".into());
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.agent_kill.pending = Some(rx);
        self.status = "Checking agent identity".into();
        if let Err(error) = std::thread::Builder::new().name("agent-identity".into()).spawn(move || {
            let result =
                switchbard_core::prepare_agent_termination(&row).map_err(|e| e.to_string());
            if tx.send(Completion::Prepared(result)).is_err() { /* UI closed; preparation is read-only. */
            }
        }) {
            self.agent_kill = AgentKillFlow::default();
            self.fail(format!("Cannot start agent identity check: {error}"));
        }
    }

    pub(super) fn dismiss_agent_preparation(&mut self) {
        if !self.agent_kill.submitting
            && (self.agent_kill.pending.is_some() || self.agent_kill.prepared.is_some())
        {
            self.cancel_agent_kill();
        }
    }

    pub(super) fn reconcile_agent_preparation(&mut self) {
        if !self.agent_kill.submitting
            && (!self.agent_target_current()
                || (self.mode != Mode::Browse
                    && !self
                        .picker
                        .as_ref()
                        .is_some_and(|p| p.purpose == PickerPurpose::AgentKill)))
        {
            self.agent_kill = AgentKillFlow::default();
        }
    }

    pub(super) fn tick_agent_kill(&mut self) {
        let completion = match self.agent_kill.pending.as_ref().map(Receiver::try_recv) {
            Some(Ok(value)) => value,
            Some(Err(TryRecvError::Disconnected)) => {
                self.agent_kill = AgentKillFlow::default();
                self.fail("Agent operation outcome unavailable; refresh before retrying".into());
                return;
            }
            _ => return,
        };
        self.agent_kill.pending = None;
        match completion {
            Completion::Prepared(result) => self.finish_agent_preparation(result),
            Completion::Sent(result) => {
                let pid = self
                    .agent_kill
                    .target
                    .as_ref()
                    .map(|identity| identity.pid)
                    .expect("submitting retains target identity");
                self.agent_kill = AgentKillFlow::default();
                match result {
                    Ok(AgentTerminationOutcome::SignalSent) => {
                        self.status =
                            format!("SIGTERM sent to agent PID {pid}; exit not yet confirmed")
                    }
                    Ok(AgentTerminationOutcome::AlreadyGone) => {
                        self.status = "Agent already gone; no signal sent".into()
                    }
                    Err(error) => self.fail(format!("Agent not signalled: {error}")),
                }
                let held = self.held_task_titles();
                self.agents.poll_now(&self.repo_root, held);
            }
        }
    }

    fn agent_target_current(&self) -> bool {
        self.page == Page::Agents
            && self.agents.row().is_some_and(|row| {
                row.process_identity.is_some() && row.process_identity == self.agent_kill.target
            })
    }

    fn finish_agent_preparation(&mut self, result: Result<PreparedAgentTermination, String>) {
        match result {
            Ok(prepared) if self.agent_target_current() && self.mode == Mode::Browse => {
                self.agent_kill.prepared = Some(prepared);
                self.agent_kill.confirmation_visible = false;
                self.status.clear();
                self.open_picker(
                    PickerPurpose::AgentKill,
                    vec![
                        PickOption::numbered("Cancel", Payload::CancelAgentKill),
                        PickOption::numbered("Send SIGTERM", Payload::ConfirmAgentKill),
                    ],
                );
            }
            Ok(_) => {
                self.agent_kill = AgentKillFlow::default();
                self.status = "Agent selection changed; reopen confirmation".into();
            }
            Err(error) => {
                self.agent_kill = AgentKillFlow::default();
                self.fail(format!("Agent signal unavailable: {error}"));
            }
        }
    }

    pub(super) fn handle_agent_kill_key(&mut self, event: KeyEvent) {
        if event.kind != KeyEventKind::Press {
            return;
        }
        match event.code {
            KeyCode::Esc => self.cancel_agent_kill(),
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(picker) = &mut self.picker {
                    picker.selected = picker.selected.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(picker) = &mut self.picker {
                    picker.selected = (picker.selected + 1).min(1);
                }
            }
            KeyCode::Enter
                if self.picker.as_ref().is_some_and(|p| {
                    p.options
                        .get(p.selected)
                        .is_some_and(|option| option.payload == Payload::ConfirmAgentKill)
                }) =>
            {
                self.submit_agent_kill()
            }
            KeyCode::Enter => self.cancel_agent_kill(),
            _ => {}
        }
    }

    fn cancel_agent_kill(&mut self) {
        self.agent_kill = AgentKillFlow::default();
        self.close_agent_kill_picker();
        self.status = "Agent signal canceled".into();
    }

    fn close_agent_kill_picker(&mut self) {
        self.picker = None;
        self.picker_parents.clear();
        self.mode = Mode::Browse;
    }

    fn submit_agent_kill(&mut self) {
        if !self.agent_kill.confirmation_visible {
            self.status = "Show the full agent confirmation before sending SIGTERM".into();
            return;
        }
        if !self.agent_target_current() {
            self.cancel_agent_kill();
            self.fail("Agent selection changed; reopen confirmation".into());
            return;
        }
        let Some(prepared) = self.agent_kill.prepared.take() else {
            return;
        };
        self.close_agent_kill_picker();
        let (tx, rx) = mpsc::sync_channel(1);
        self.agent_kill.pending = Some(rx);
        self.agent_kill.submitting = true;
        self.status = "Sending SIGTERM to selected agent".into();
        if let Err(error) = std::thread::Builder::new()
            .name("agent-signal".into())
            .spawn(move || {
                let result = switchbard_core::terminate_agent(prepared).map_err(|e| e.to_string());
                if tx.send(Completion::Sent(result)).is_err() {
                    eprintln!("Agent signal result receiver closed");
                }
            })
        {
            self.agent_kill = AgentKillFlow::default();
            self.fail(format!("SIGTERM not sent; cannot start worker: {error}"));
        }
    }
}
