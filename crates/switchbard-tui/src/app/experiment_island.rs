//! Pinned experiment guidance and a durable feedback draft, independent of task edits.
use super::{App, Mode};
use crate::experiments::{catalog, ExperimentDecision};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

#[derive(Clone, Copy, Debug)]
pub enum IslandAction {
    Toggle,
    Review,
    Feedback,
    Hide,
    Submit,
    CloseFeedback,
    Next,
}

#[derive(Clone)]
pub struct IslandHit {
    pub area: Rect,
    pub action: IslandAction,
}

#[derive(Default)]
pub struct IslandUi {
    pub editing: bool,
    pub feedback_id: Option<String>,
    pub draft: String,
    pub dirty: bool,
    pub error: Option<String>,
    pub message: Option<String>,
    pub hits: Vec<IslandHit>,
}

impl App {
    pub(super) fn begin_experiment_trial(&mut self, id: &str) {
        let Some(spec) = catalog().iter().find(|spec| spec.id == id) else {
            return;
        };
        if let Err(error) = self.experiment_feedback.pin(Some(id)) {
            self.fail(format!("Could not pin experiment: {error:#}"));
            return;
        }
        let previous = self.detail_rows();
        if let Err(error) = self
            .experiments
            .set_decision(id, ExperimentDecision::Enable)
        {
            self.island.error = Some(format!("Pinned, but could not enable: {error:#}"));
        } else {
            self.island.error = None;
        }
        self.reconcile_experiment_detail_rows(previous);
        self.picker = None;
        self.picker_parents.clear();
        self.mode = Mode::Browse;
        if self.page != spec.page {
            self.switch_page(spec.page);
        }
        self.island.message =
            Some("Instructions pinned · Feedback saves your comments here".into());
        self.status.clear();
    }

    pub(super) fn open_experiment_feedback(&mut self) {
        if self.island.dirty {
            self.island.editing = true;
            return;
        }
        // Keep the clicked experiment even if another pane changes its pin.
        let Some(id) = self.experiment_feedback.active().map(str::to_string) else {
            self.fail("Choose an experiment and Try it first".into());
            return;
        };
        if !catalog().iter().any(|spec| spec.id == id) {
            self.fail("This experiment is unavailable in this build. Open :experiments and Try another; its feedback is preserved.".into());
            return;
        }
        if let Some(error) = self.experiment_feedback.reload() {
            self.fail(error);
            return;
        }
        self.island.draft = self.experiment_feedback.draft(&id).to_string();
        self.island.feedback_id = Some(id);
        self.island.editing = true;
        self.island.error = None;
        self.picker = None;
        self.picker_parents.clear();
        self.mode = Mode::Browse;
    }

    fn save_island_draft(&mut self) -> bool {
        let Some(id) = self.island.feedback_id.clone() else {
            return false;
        };
        match self.experiment_feedback.set_draft(&id, &self.island.draft) {
            Ok(()) => {
                self.island.dirty = false;
                self.island.error = None;
                true
            }
            Err(error) => {
                self.island.dirty = true;
                self.island.error = Some(format!("Draft not saved: {error:#}"));
                false
            }
        }
    }

    fn submit_island_feedback(&mut self) {
        if self.island.draft.trim().is_empty() {
            self.island.error = Some("Write what happened or what you want changed first".into());
            return;
        }
        if !self.save_island_draft() {
            return;
        }
        let Some(spec) = self
            .island
            .feedback_id
            .as_deref()
            .and_then(|id| catalog().iter().find(|spec| spec.id == id))
        else {
            return;
        };
        let build = switchbard_core::build_identity::VERSION_LINE;
        match self.experiment_feedback.submit(spec.id, spec.number, build) {
            Ok(()) => {
                self.island.draft.clear();
                self.island.editing = false;
                self.island.message = Some(format!("Feedback saved for E{:03}", spec.number));
                self.island.error = None;
            }
            Err(error) => self.island.error = Some(format!("Feedback not saved: {error:#}")),
        }
    }

    pub(super) fn handle_island_key(&mut self, event: KeyEvent) -> bool {
        if !self.island.editing {
            return false;
        }
        match event.code {
            KeyCode::Esc => {
                if !self.island.dirty || self.save_island_draft() {
                    self.island.editing = false;
                }
            }
            KeyCode::Enter if event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.submit_island_feedback()
            }
            KeyCode::Char('s') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.submit_island_feedback()
            }
            KeyCode::Enter => {
                if self.island.draft.chars().count() < crate::experiment_feedback::MAX_DRAFT_CHARS {
                    self.island.draft.push('\n');
                    self.save_island_draft();
                }
            }
            KeyCode::Backspace => {
                self.island.draft.pop();
                self.save_island_draft();
            }
            KeyCode::Char(ch)
                if !event
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if self.island.draft.chars().count() < crate::experiment_feedback::MAX_DRAFT_CHARS {
                    self.island.draft.push(ch);
                    self.save_island_draft();
                } else {
                    self.island.error = Some("Feedback limit: 4096 characters".into());
                }
            }
            _ => {}
        }
        true
    }

    pub(super) fn handle_island_mouse(&mut self, event: MouseEvent) -> bool {
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return self.island.editing;
        }
        let position = Position::new(event.column, event.row);
        let Some(hit) = self
            .island
            .hits
            .iter()
            .find(|hit| hit.area.contains(position))
            .cloned()
        else {
            return self.island.editing;
        };
        if !self.island.editing
            && (!matches!(self.mode, Mode::Browse | Mode::DetailFocus)
                || self.inbox.editing
                || self.inbox.publish_confirmation.is_some())
        {
            return false;
        }
        self.apply_island_action(hit.action);
        true
    }

    pub(super) fn apply_island_action(&mut self, action: IslandAction) {
        match action {
            IslandAction::Submit => {
                self.submit_island_feedback();
                return;
            }
            IslandAction::CloseFeedback => {
                if !self.island.dirty || self.save_island_draft() {
                    self.island.editing = false;
                }
                return;
            }
            _ => {}
        }
        let Some(id) = self.experiment_feedback.active().map(str::to_string) else {
            return;
        };
        match action {
            IslandAction::Feedback => self.open_experiment_feedback(),
            IslandAction::Submit => self.submit_island_feedback(),
            IslandAction::CloseFeedback => {
                if !self.island.dirty || self.save_island_draft() {
                    self.island.editing = false;
                }
            }
            IslandAction::Hide => match self.experiment_feedback.pin(None) {
                Ok(()) => self.island.message = None,
                Err(error) => self.island.error = Some(format!("Could not hide: {error:#}")),
            },
            IslandAction::Review => {
                if self.experiments.state(&id).review
                    == crate::experiments::ExperimentReview::Unreviewed
                {
                    let previous = self.detail_rows();
                    match self.experiments.set_decision(&id, ExperimentDecision::Keep) {
                        Ok(()) => {
                            self.reconcile_experiment_detail_rows(previous);
                            self.island.error = None;
                        }
                        Err(error) => {
                            self.island.error = Some(format!("Could not keep: {error:#}"))
                        }
                    }
                } else {
                    self.open_experiment(&id);
                }
            }
            IslandAction::Toggle => {
                let previous = self.detail_rows();
                let decision = if self.experiments.is_enabled(&id) {
                    ExperimentDecision::Disable
                } else {
                    ExperimentDecision::Enable
                };
                match self.experiments.set_decision(&id, decision) {
                    Ok(()) => {
                        self.reconcile_experiment_detail_rows(previous);
                        self.island.error = None;
                    }
                    Err(error) => self.island.error = Some(format!("Could not toggle: {error:#}")),
                }
            }
            IslandAction::Next => {
                let specs = catalog();
                if let Some(index) = specs.iter().position(|spec| spec.id == id) {
                    if let Err(error) = self
                        .experiment_feedback
                        .pin(Some(specs[(index + 1) % specs.len()].id))
                    {
                        self.island.error = Some(format!("Could not switch experiment: {error:#}"));
                    }
                }
            }
        }
    }
}
