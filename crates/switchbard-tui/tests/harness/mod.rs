//! The shared end-to-end harness: a real temp backlog, the real app, a test
//! terminal, and helpers that read the rendered screen. Every test file uses it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use switchbard_core::{create_task_allocating_id, NewBacklogTask};
use switchbard_tui::app::App;
use switchbard_tui::telemetry::Telemetry;
use switchbard_tui::view;

pub struct Harness {
    pub _dir: tempfile::TempDir,
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub app: App,
    pub terminal: Terminal<TestBackend>,
}

impl Harness {
    pub fn new() -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("backlog/tasks")).unwrap();
        std::fs::write(
            root.join("backlog/config.yml"),
            "project_name: fixture\nstatuses: [\"To Do\", \"In Progress\", \"Done\"]\ntask_prefix: task\n",
        )
        .unwrap();
        seed_with_priority(
            &root,
            "Fix login redirect loop",
            "In Progress",
            &["auth", "bug"],
            "medium",
        );
        seed_with_priority(&root, "Add dark theme", "To Do", &["ui"], "low");
        seed_with_priority(&root, "Write onboarding guide", "To Do", &["docs"], "high");
        let config_path = root.join("tui.lua");
        let app = open_app(&root, &config_path);
        let terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        let mut harness = Harness {
            _dir: dir,
            root,
            config_path,
            app,
            terminal,
        };
        harness.render();
        harness
    }

    pub fn render(&mut self) -> String {
        let app = &mut self.app;
        self.terminal.draw(|frame| view::draw(frame, app)).unwrap();
        view::buffer_text(self.terminal.backend().buffer())
    }

    pub fn press(&mut self, code: KeyCode) -> String {
        self.app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
        self.render()
    }

    pub fn type_text(&mut self, text: &str) -> String {
        for c in text.chars() {
            self.app
                .handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        self.render()
    }

    pub fn selected_title(&self) -> String {
        self.app.selected_task().unwrap().title.clone()
    }
}

pub fn open_app(root: &Path, config_path: &Path) -> App {
    App::open(
        root,
        Some(config_path.to_path_buf()),
        Some(root.join("views.lua")),
        Some(root.join("views-repo.lua")),
        Telemetry::in_memory(),
    )
}

pub fn seed(root: &Path, title: &str, status: &str, labels: &[&str]) {
    seed_with_priority(root, title, status, labels, "medium");
}

pub fn seed_with_priority(root: &Path, title: &str, status: &str, labels: &[&str], priority: &str) {
    let task = NewBacklogTask {
        title: title.to_string(),
        description: format!("Description of {title}."),
        status: status.to_string(),
        priority: priority.to_string(),
        acceptance_criteria: vec!["It works".to_string()],
        parent: None,
        labels: labels.iter().map(|s| s.to_string()).collect(),
        assignees: Vec::new(),
        project: None,
        dependencies: Vec::new(),
    };
    create_task_allocating_id(root, &task).unwrap();
}

pub fn visible_titles(h: &Harness) -> Vec<String> {
    (0..h.app.visible.len())
        .map(|index| h.app.task(index).unwrap().title.clone())
        .collect()
}

pub fn cell_fg(h: &Harness, needle: &str) -> Option<ratatui::style::Color> {
    let buffer = h.terminal.backend().buffer();
    let width = buffer.area.width as usize;
    for (row, cells) in buffer.content.chunks(width).enumerate() {
        let line: String = cells.iter().map(|cell| cell.symbol()).collect();
        if let Some(col) = line.find(needle) {
            let col = line[..col].chars().count();
            return Some(buffer.content[row * width + col].fg);
        }
    }
    None
}

pub fn header_line(screen: &str) -> String {
    screen
        .lines()
        .find(|line| line.starts_with("│1 "))
        .unwrap_or_default()
        .to_string()
}
