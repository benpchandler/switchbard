//! The shared end-to-end harness: a real temp backlog, the real app, a test
//! terminal, and helpers that read the rendered screen. Every test file uses it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use switchbard_core::{
    create_project_def, create_task_allocating_id, rank_project, NewBacklogTask, NewProjectDef,
    RankPlacement,
};
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
        Some(root.join("settings.lua")),
        Some(root.join("settings-repo.lua")),
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

/// A task inside a project, optionally as a sub-issue of `parent`.
pub fn seed_in_project(
    root: &Path,
    title: &str,
    status: &str,
    project: &str,
    parent: Option<&str>,
) {
    let task = NewBacklogTask {
        title: title.to_string(),
        description: format!("Description of {title}."),
        status: status.to_string(),
        priority: "medium".to_string(),
        acceptance_criteria: vec!["It works".to_string()],
        parent: parent.map(str::to_string),
        labels: Vec::new(),
        assignees: Vec::new(),
        project: Some(project.to_string()),
        dependencies: Vec::new(),
    };
    create_task_allocating_id(root, &task).unwrap();
}

/// A project def under `initiative`, ranked in the order these calls are made.
pub fn seed_project(root: &Path, name: &str, status: &str, initiative: Option<&str>) {
    create_project_def(
        root,
        &NewProjectDef {
            name: name.to_string(),
            status: status.to_string(),
            target_date: None,
            initiative: initiative.map(str::to_string),
            lead: None,
            description: String::new(),
        },
    )
    .unwrap();
    let placement = match last_ranked_project(root) {
        Some(previous) => RankPlacement::After(previous),
        None => RankPlacement::Top,
    };
    let _ = rank_project(root, name, &placement).unwrap();
}

fn last_ranked_project(root: &Path) -> Option<String> {
    switchbard_core::load_backlog_repo(root)
        .ok()?
        .ranking
        .projects
        .last()
        .cloned()
}

/// Task titles in screen order, headings excluded.
pub fn visible_titles(h: &Harness) -> Vec<String> {
    (0..h.app.rows.len())
        .filter_map(|row| h.app.task(row).map(|task| task.title.clone()))
        .collect()
}

/// Every table row as the screen shows it: headings and task titles alike.
pub fn screen_rows(h: &Harness) -> Vec<String> {
    h.app
        .rows
        .iter()
        .enumerate()
        .map(|(row, entry)| match entry {
            switchbard_tui::group::Row::Heading { text, depth } => {
                format!("{}# {text}", "  ".repeat(*depth))
            }
            switchbard_tui::group::Row::Task(_) => h.app.task(row).unwrap().title.clone(),
        })
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

/// A tasks-measured goal for the current week, scoped to `scope` (a project
/// name or label) with `attached` task ids as explicit inputs.
pub fn seed_goal(
    root: &Path,
    name: &str,
    unit: &str,
    target: i64,
    scope: Option<&str>,
    attached: &[&str],
) {
    let week = switchbard_core::week_monday_of(chrono::Local::now().date_naive())
        .format("%Y-%m-%d")
        .to_string();
    switchbard_core::create_goal(
        root,
        &switchbard_core::NewGoal {
            name: name.to_string(),
            unit: unit.to_string(),
            measure: switchbard_core::GoalMeasure::Tasks,
            scope: scope.map(str::to_string),
            week,
            target,
        },
    )
    .unwrap();
    if !attached.is_empty() {
        let ids: Vec<String> = attached.iter().map(|s| s.to_string()).collect();
        switchbard_core::attach_goal_inputs(root, name, &ids, &[]).unwrap();
    }
}
