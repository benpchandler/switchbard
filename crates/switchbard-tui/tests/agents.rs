//! The Agents page (TASK-225): this repo's live agent sessions, through the
//! real app, real keys and rendered screens. Sessions are injected through
//! the page's own `accept` with polling switched off, because a test cannot
//! start a real `claude` inside its temp repo.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::path::PathBuf;
use std::time::Instant;
use switchbard_core::{
    claim_work, AgentActivity, AgentProcessKind, AgentSession, AgentStatus, WorkIdentity,
};
use switchbard_tui::agents::Observation;
use switchbard_tui::app::Pane;
use switchbard_tui::page::Page;

fn session(
    root: &std::path::Path,
    pid: u32,
    id: &str,
    activity: AgentActivity,
    title: &str,
) -> AgentSession {
    AgentSession {
        pid,
        process_identity: None,
        kind: AgentProcessKind::Claude,
        cwd: Some(root.to_path_buf()),
        repo_name: Some("fixture".into()),
        worktree_path: Some(root.to_path_buf()),
        worktree_branch: Some("main".into()),
        started_unix: Some(1_700_000_000),
        pgid: Some(pid as i32),
        session_id: Some(id.to_string()),
        name: Some("fixture-3f".into()),
        activity,
        title: Some(title.to_string()),
    }
}

fn inject(h: &mut Harness, sessions: Vec<AgentSession>) {
    inject_with_status(h, sessions, Vec::new());
}

fn inject_with_status(h: &mut Harness, sessions: Vec<AgentSession>, statuses: Vec<AgentStatus>) {
    h.app.agents.live = false;
    h.app.agents.accept(
        Observation {
            sessions,
            listing_error: None,
            statuses: statuses
                .into_iter()
                .map(|status| (status.session_id.clone(), status))
                .collect(),
            observed_unix: NOW_UNIX,
        },
        Instant::now(),
    );
}

const NOW_UNIX: u64 = 1_800_000_000;

fn status(id: &str, model: &str, ctx: f64, cost: f64, seconds_ago: u64) -> AgentStatus {
    AgentStatus {
        session_id: id.to_string(),
        pid: None,
        cwd: None,
        model_id: Some(format!("claude-{}", model.to_lowercase())),
        model_name: Some(model.to_string()),
        context_used_percent: Some(ctx),
        total_cost_usd: Some(cost),
        session_name: None,
        claude_version: Some("2.1.269".into()),
        updated_at_unix: NOW_UNIX - seconds_ago,
    }
}

fn to_agents(h: &mut Harness) -> String {
    h.press(KeyCode::Tab);
    let screen = h.press(KeyCode::Tab);
    assert_eq!(h.app.page, Page::Agents);
    screen
}

fn nav_after_agents(screen: &str) -> String {
    let nav = screen.lines().next().unwrap_or_default();
    let start = nav.find("Agents").expect("Agents label on the nav line");
    nav[start + "Agents".len()..].to_string()
}

#[test]
fn the_agents_page_is_in_the_cycle_and_names_its_keys() {
    let mut h = Harness::new();
    let first = h.render();
    assert!(first.contains("Agents"), "{first}");
    let screen = to_agents(&mut h);
    assert!(screen.contains("[Agents]"), "{screen}");
    assert!(screen.contains("Polling agent sessions"), "{screen}");
    assert!(screen.contains("detail"), "{screen}");
    assert!(screen.contains("poll"), "{screen}");
    assert!(screen.contains("tab page"), "{screen}");
    let help = h.press(KeyCode::Char('?'));
    assert!(help.contains("open"), "{help}");
    assert!(help.contains("reload"), "{help}");
    assert!(!help.contains("new_task"), "{help}");
    assert!(!help.contains("filter_column"), "{help}");
    h.press(KeyCode::Esc);
    assert!(h.press(KeyCode::Tab).contains("[Inbox]"));
    assert!(h.press(KeyCode::Tab).contains("[Tasks]"));
    h.terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
    to_agents(&mut h);
    assert!(h.render().contains("[Agents]"));
}

#[test]
fn rows_show_state_worktree_title_and_held_tasks_idle_first() {
    let mut h = Harness::new();
    let root = h.root.clone();
    let task_id = h.app.selected_task().unwrap().id.clone();
    let task_title = h.selected_title();
    claim_work(
        &h.root.join("work"),
        &WorkIdentity {
            session_id: "sid-busy".into(),
            pid: std::process::id(),
            agent: "claude".into(),
        },
        &h.root,
        &task_id,
    )
    .unwrap();
    h.app.tick();
    inject(
        &mut h,
        vec![
            session(&root, 4242, "sid-busy", AgentActivity::Busy, &task_title),
            session(
                &root,
                4343,
                "sid-idle",
                AgentActivity::Idle,
                "why is login slow",
            ),
            session(
                &root,
                4444,
                "sid-unknown",
                AgentActivity::Unknown,
                "codex-ish",
            ),
        ],
    );
    h.terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
    let screen = to_agents(&mut h);
    let rows: Vec<&str> = screen
        .lines()
        .filter(|line| line.contains("idle") || line.contains("busy") || line.contains("unknown"))
        .collect();
    let idle_at = screen.find("✳ idle").expect("idle row");
    let busy_at = screen.find("◐ busy").expect("busy row");
    let unknown_at = screen.find("? unknown").expect("unknown row");
    assert!(idle_at < busy_at && busy_at < unknown_at, "{screen}");
    assert!(screen.contains("why is login slow"), "{screen}");
    assert!(screen.contains(&task_title), "{screen}");
    assert!(
        screen.contains(&task_id),
        "held task id in the tasks column: {screen}"
    );
    assert!(screen.contains("main"), "{screen}");
    assert!(
        screen.contains("3 sessions in this repo · 1 idle · 1 busy"),
        "{screen}"
    );
    assert!(!rows.is_empty());
    h.terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
    let medium = h.render();
    assert!(medium.contains("model"), "medium keeps model: {medium}");
    assert!(medium.contains("ctx"), "medium keeps ctx: {medium}");
    assert!(
        !medium.contains("worktree"),
        "medium drops worktree: {medium}"
    );
    assert!(!medium.contains("tasks"), "medium drops tasks: {medium}");
    h.terminal = Terminal::new(TestBackend::new(56, 12)).unwrap();
    let narrow = h.render();
    assert!(narrow.contains("state"), "{narrow}");
    assert!(narrow.contains("title"), "{narrow}");
    assert!(!narrow.contains("model"), "narrow drops model: {narrow}");
    assert!(
        !narrow.contains("worktree"),
        "narrow drops the worktree column: {narrow}"
    );
    assert!(narrow.contains("✳ idle"), "{narrow}");
}

#[test]
fn status_reports_fill_model_context_and_last_activity_and_are_absent_honestly() {
    let mut h = Harness::new();
    let root = h.root.clone();
    h.terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
    inject_with_status(
        &mut h,
        vec![
            session(&root, 1, "sid-reported", AgentActivity::Idle, "reported"),
            session(&root, 2, "sid-silent", AgentActivity::Idle, "silent"),
        ],
        vec![status("sid-reported", "Opus", 34.4, 1.2345, 754)],
    );
    let screen = to_agents(&mut h);
    let reported = screen
        .lines()
        .find(|line| line.contains("reported"))
        .expect("reported row");
    assert!(reported.contains("Opus"), "{reported}");
    assert!(reported.contains("34%"), "{reported}");
    assert!(
        reported.contains("12m"),
        "last activity from the report: {reported}"
    );
    let silent = screen
        .lines()
        .find(|line| line.contains("silent"))
        .expect("silent row");
    assert!(
        !silent.contains("Opus") && !silent.contains('%'),
        "{silent}"
    );
    assert!(
        !silent.contains("12m") && !silent.contains("0s"),
        "no report and no observed change: no age at all: {silent}"
    );
    let detail = h.press(KeyCode::Enter);
    assert!(detail.contains("Opus (claude-opus)"), "{detail}");
    assert!(detail.contains("34% context"), "{detail}");
    assert!(detail.contains("$1.23"), "{detail}");
    assert!(detail.contains("v2.1.269"), "{detail}");
    assert!(detail.contains("last activity 12m ago"), "{detail}");
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('j'));
    let detail = h.press(KeyCode::Enter);
    assert!(detail.contains("no status report"), "{detail}");
}

#[test]
fn the_badge_counts_idle_sessions_on_every_page_and_hides_at_zero() {
    let mut h = Harness::new();
    let root = h.root.clone();
    let fresh = nav_after_agents(&h.render());
    assert!(fresh.starts_with("  …"), "never polled: {fresh:?}");
    inject(
        &mut h,
        vec![
            session(&root, 1, "a", AgentActivity::Idle, "one"),
            session(&root, 2, "b", AgentActivity::Idle, "two"),
            session(&root, 3, "c", AgentActivity::Busy, "three"),
        ],
    );
    assert_eq!(h.app.page, Page::Tasks);
    let after = nav_after_agents(&h.render());
    assert!(
        after.starts_with("   2 "),
        "badge on the Tasks page: {after:?}"
    );
    h.press(KeyCode::Tab);
    assert!(
        nav_after_agents(&h.render()).starts_with("   2 "),
        "badge on the PR page"
    );
    inject(
        &mut h,
        vec![session(&root, 3, "c", AgentActivity::Busy, "three")],
    );
    let none = nav_after_agents(&h.render());
    assert!(!none.contains('2') && !none.contains('…'), "{none:?}");
    h.app.agents.error = Some("boom".into());
    assert!(
        nav_after_agents(&h.render()).starts_with("  ?"),
        "failed poll"
    );
}

#[test]
fn cursor_detail_and_guarded_commands_on_the_agents_page() {
    let mut h = Harness::new();
    let root = h.root.clone();
    let tasks = h.app.state.clone();
    inject(
        &mut h,
        vec![
            session(&root, 4242, "sid-a", AgentActivity::Idle, "first"),
            session(&root, 4343, "sid-b", AgentActivity::Idle, "second"),
        ],
    );
    to_agents(&mut h);
    assert_eq!(h.app.agents.selected, 0);
    h.press(KeyCode::Char('j'));
    assert_eq!(h.app.agents.selected, 1);
    let screen = h.press(KeyCode::Enter);
    assert_eq!(h.app.pane, Pane::Detail);
    assert!(screen.contains("second"), "{screen}");
    assert!(screen.contains("session sid-b"), "{screen}");
    assert!(screen.contains("pid 4343"), "{screen}");
    assert!(screen.contains("listed as fixture-3f"), "{screen}");
    assert!(screen.contains("no claimed tasks"), "{screen}");
    assert!(
        screen.contains(&h.root.display().to_string()) || screen.contains("main"),
        "{screen}"
    );
    h.press(KeyCode::Esc);
    assert_eq!(h.app.pane, Pane::None);
    let writes_before_bug = std::fs::read_dir(h.root.join("backlog/tasks"))
        .unwrap()
        .count();
    let bug_screen = h.press(KeyCode::Char('b'));
    assert_eq!(h.app.page, Page::Agents);
    assert!(bug_screen.contains(":bug "), "{bug_screen}");
    h.press(KeyCode::Esc);
    assert_eq!(h.app.pane, Pane::None);
    assert_eq!(
        std::fs::read_dir(h.root.join("backlog/tasks"))
            .unwrap()
            .count(),
        writes_before_bug
    );
    std::fs::write(&h.config_path, "return { keys = { B = 'ball' } }").unwrap();
    h.app = harness::open_app(&h.root, &h.config_path);
    to_agents(&mut h);
    inject(
        &mut h,
        vec![
            session(&root, 4242, "sid-a", AgentActivity::Idle, "first"),
            session(&root, 4343, "sid-b", AgentActivity::Idle, "second"),
        ],
    );
    h.press(KeyCode::Char('j'));
    assert_eq!(h.app.agents.selected, 1);
    let ball_screen = h.press(KeyCode::Char('B'));
    assert!(ball_screen.contains("ball"), "{ball_screen}");
    h.press(KeyCode::Esc);
    for key in ['1', 't', 'v', 'w', 'm', '/', 'f', 's', 'p', ','] {
        h.press(KeyCode::Char(key));
        assert!(h.app.picker.is_none(), "{key} opened a picker");
        assert_eq!(h.app.state, tasks, "{key} changed the task view");
        assert_eq!(h.app.page, Page::Agents, "{key} left the page");
    }
    for command in [
        ":goal nope",
        ":group status",
        ":theme berg",
        ":more",
        ":open",
    ] {
        h.type_text(command);
        let screen = h.press(KeyCode::Enter);
        assert!(
            screen.contains("Switch to Tasks or Pull Requests"),
            "{command}: {screen}"
        );
        assert_eq!(h.app.pane, Pane::None);
    }
    inject(
        &mut h,
        vec![session(&root, 4343, "sid-b", AgentActivity::Idle, "second")],
    );
    assert_eq!(
        h.app.agents.row().unwrap().pid,
        4343,
        "cursor followed its session"
    );
    inject(&mut h, vec![]);
    let screen = h.render();
    assert!(
        screen.contains("No agent sessions are running in this repo."),
        "{screen}"
    );
}

#[test]
fn the_agents_page_survives_a_self_restart() {
    let mut h = Harness::new();
    to_agents(&mut h);
    let resume = h.app.resume_state();
    h.app = open_app(&h.root, &h.config_path);
    h.app.resume_from(Some(&resume));
    assert_eq!(h.app.page, Page::Agents);
    h.app.resume_from(Some("pages3=agents\t{}"));
    assert_eq!(h.app.page, Page::Agents);
}

#[test]
fn a_session_whose_worktree_is_elsewhere_is_never_a_row() {
    let mut h = Harness::new();
    let root = h.root.clone();
    let mut elsewhere = session(&root, 9, "sid-x", AgentActivity::Idle, "elsewhere");
    elsewhere.worktree_path = Some(PathBuf::from("/definitely/another/repo"));
    // The worker drops unattributed sessions before they reach `accept`;
    // `attribute_to_repo` is the pure gate and is unit-tested in the page
    // module. Here: a row the worker did pass through renders as given,
    // proving the page itself adds no second filter that could disagree.
    inject(&mut h, vec![elsewhere]);
    to_agents(&mut h);
    assert!(h.render().contains("elsewhere"));
}
