//! Ordinary task capture: keep the draft until the native writer succeeds.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use switchbard_core::{create_backlog_task, NewBacklogTask};

use super::{App, Mode, Pane};

const MAX_TITLE_BYTES: usize = 1024;

impl App {
    pub(super) fn open_new_task(&mut self) {
        self.mode = Mode::NewTask;
        self.input.clear();
        self.status.clear();
        self.pane = Pane::None;
    }

    pub(super) fn handle_new_task_key(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.input.clear();
                self.status = "new task cancelled".to_string();
                self.telemetry.record("action", "new_task_cancel");
            }
            KeyCode::Enter if event.kind == KeyEventKind::Press => self.create_task(),
            KeyCode::Backspace => {
                self.input.pop();
                self.status.clear();
            }
            KeyCode::Char(c)
                if !c.is_control()
                    && !event
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if self.input.len() + c.len_utf8() <= MAX_TITLE_BYTES {
                    self.input.push(c);
                    self.status.clear();
                } else {
                    self.fail(format!("title limit: {MAX_TITLE_BYTES} bytes"));
                }
            }
            _ => {}
        }
    }

    fn create_task(&mut self) {
        let title = self.input.trim();
        if title.is_empty() {
            self.fail("title is required".to_string());
            return;
        }
        let task = NewBacklogTask {
            title: title.to_string(),
            description: String::new(),
            status: String::new(),
            priority: String::new(),
            acceptance_criteria: Vec::new(),
            parent: None,
            labels: Vec::new(),
            assignees: Vec::new(),
            project: None,
            dependencies: Vec::new(),
            due_date: None,
        };
        match create_backlog_task(&self.repo_root, &task) {
            Ok(id) => self.finish_new_task(&id),
            Err(error) => self.fail(format!("could not create task: {error}")),
        }
    }

    fn finish_new_task(&mut self, id: &str) {
        self.mode = Mode::Browse;
        self.input.clear();
        self.reload_tasks();
        if !self.tasks.iter().any(|task| task.id == id) {
            self.fail(format!(
                "created {id}; could not reload it - reload to refresh"
            ));
            return;
        }
        let visible = self.visible.iter().any(|&index| self.tasks[index].id == id);
        self.select_task(id);
        self.status = if visible {
            format!("created {id}")
        } else {
            format!("created {id}; hidden by current filters")
        };
        self.telemetry
            .record("action", format!("new_task_create {id}"));
    }
}
