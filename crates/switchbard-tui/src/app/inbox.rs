//! Inbox input writes intentions through the core run store.
use super::{App, Mode, Page};
use crate::config::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use switchbard_core::bug_run::{RunOptions, RunState, MAX_TEXT};

impl App {
    pub(super) fn handle_publish_confirmation(&mut self, event: KeyEvent) {
        if event.code == KeyCode::Esc {
            self.inbox.publish_confirmation = None;
            return;
        }
        if event.code != KeyCode::Enter || !self.inbox.confirmation_visible {
            return;
        }
        let Some(run) = self.inbox.publish_confirmation.take() else {
            return;
        };
        match self
            .inbox
            .store()
            .and_then(|mut store| store.request_publish(&run.id, run.revision))
        {
            Ok(run) => {
                self.inbox.update(run);
                self.status = "PR publication queued".into();
            }
            Err(error) => self.fail(error.to_string()),
        }
    }
    pub(super) fn handle_inbox_binding(&mut self, event: KeyEvent) -> bool {
        let chord = crate::config::KeyChord {
            code: event.code,
            ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        };
        let Some(command) = self.config.inbox_keys.get(&chord).cloned() else {
            return false;
        };
        match command.as_str() {
            "diff" => self.inbox.show_diff(),
            "reconcile" => self.inbox.reconcile(),
            other => self.inbox_command(other),
        }
        true
    }
    pub(super) fn enqueue_bug(&mut self, repo: &std::path::Path, id: &str) {
        let prefix = if self.status.starts_with("filed ") {
            self.status.clone()
        } else {
            format!("bug {id}")
        };
        let result = self.inbox.store().and_then(|mut store| {
            store.enqueue(
                repo,
                id,
                RunOptions {
                    codex_binary: self.config.bug_codex_binary.clone(),
                    gate_command: self.config.bug_gate_command.clone(),
                },
            )
        });
        match result {
            Ok(run) => {
                self.status = format!("{prefix} · queued for Codex · Inbox has progress");
                self.inbox.update(run);
                self.inbox.tick(&self.repo_root, self.config.report_repo.as_deref());
            }
            Err(error) => self.fail(format!("{prefix}; could not dispatch: {error}. Report retained; :dispatch {id} retries dispatch.")),
        }
    }

    pub(super) fn apply_inbox_action(&mut self, action: &Action) -> bool {
        match action {
            Action::Down => self.inbox.move_by(1),
            Action::Up => self.inbox.move_by(-1),
            Action::Top => self.inbox.move_by(isize::MIN),
            Action::Bottom => self.inbox.move_by(isize::MAX),
            Action::PageDown => self.inbox.scroll = self.inbox.scroll.saturating_add(8),
            Action::PageUp => self.inbox.scroll = self.inbox.scroll.saturating_sub(8),
            Action::Open => {
                if let Some(url) = self
                    .inbox
                    .row()
                    .filter(|r| r.state == RunState::PrOpen)
                    .and_then(|r| r.pr_url.clone())
                {
                    if let Err(error) = switchbard_core::open_url(&url, None) {
                        self.fail(format!("Cannot open PR: {error}"));
                    }
                } else {
                    self.inbox.begin_reply();
                }
            }
            _ => return false,
        }
        true
    }

    pub(super) fn inbox_command(&mut self, command: &str) {
        if self.page != Page::Inbox {
            self.fail("Open Inbox to act on a request".into());
            return;
        }
        let Some(run) = self.inbox.row() else {
            self.fail("No Inbox request selected".into());
            return;
        };
        if command == "publish" && run.state == RunState::AwaitingReview && run.draft.is_empty() {
            self.inbox.publish_confirmation = Some(run.clone());
            self.inbox.confirmation_visible = false;
            return;
        }
        if command == "retry" && run.state.is_queued() {
            let id = run.id.clone();
            self.inbox.retry_launch(&id);
            self.status = "Launch retry queued".into();
            return;
        }
        let result = self.inbox.store().and_then(|mut store| match command {
            "publish" => store.request_publish(&run.id, run.revision),
            _ => store.retry(&run.id, run.revision),
        });
        match result {
            Ok(run) => {
                self.inbox.update(run);
                self.status = "Queued; Inbox will show the result".into();
            }
            Err(error) => self.fail(error.to_string()),
        }
    }

    pub(super) fn handle_inbox_reply(&mut self, event: KeyEvent) {
        if event.modifiers.contains(KeyModifiers::CONTROL) && event.code == KeyCode::Char('r') {
            match self.inbox.keep_both_drafts() {
                Ok(()) => self.status = "Both drafts saved together; edit before sending".into(),
                Err(error) => self.fail(format!("Draft retained: {error}")),
            }
            return;
        }
        if event.modifiers.contains(KeyModifiers::CONTROL) && event.code == KeyCode::Char('s') {
            match self.inbox.submit() {
                Ok(()) => {
                    self.status = "Reply saved and queued for Codex".into();
                    self.mode = Mode::Browse;
                }
                Err(error) => self.fail(format!("Reply retained: {error}")),
            }
            return;
        }
        match event.code {
            KeyCode::Esc => {
                match self.inbox.save_draft() {
                    Ok(()) => {
                        self.inbox.editing = false;
                        self.status = "Draft saved; Enter to continue".into();
                    }
                    Err(error) => self.fail(format!("Draft still here: {error}")),
                }
                return;
            }
            KeyCode::Backspace => {
                self.inbox.draft.pop();
            }
            KeyCode::Enter => self.inbox.draft.push('\n'),
            KeyCode::Char(ch)
                if !event
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if self.inbox.draft.len() + ch.len_utf8() <= MAX_TEXT {
                    self.inbox.draft.push(ch);
                } else {
                    self.fail("Reply limit reached; draft retained".into());
                    return;
                }
            }
            _ => return,
        }
        if let Err(error) = self.inbox.save_draft() {
            self.fail(format!(
                "Draft not saved; Ctrl-R keeps both drafts: {error}"
            ));
        }
    }
}
