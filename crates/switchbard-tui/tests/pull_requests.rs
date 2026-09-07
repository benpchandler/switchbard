//! Real key and real process journeys; no replacement GitHub clients.
mod harness;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use harness::*;
use std::time::{Duration, Instant};

fn settle(h: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(100);
    while h.app.pull_requests.loading() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        h.app.tick();
    }
    assert!(
        !h.app.pull_requests.loading(),
        "bounded refresh did not finish"
    );
}

#[test]
fn non_repository_is_unavailable_not_a_successful_empty_list() {
    let mut h = Harness::new();
    let selected = h.selected_title();
    h.press(KeyCode::Tab);
    settle(&mut h);
    let screen = h.render();
    assert!(screen.contains("Unavailable"), "{screen}");
    assert!(!screen.contains("No PRs match"), "{screen}");
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Tab);
    assert_eq!(h.selected_title(), selected);
    settle(&mut h);
    assert!(h.render().contains("[Tasks]"));
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO; reads only"]
fn live_repository_renders_actual_pull_requests() {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let mut h = Harness::new();
    h.app = open_app(std::path::Path::new(&root), &h.config_path);
    h.press(KeyCode::Tab);
    settle(&mut h);
    assert!(
        h.app.pull_requests.error.is_none(),
        "{:?}",
        h.app.pull_requests.error
    );
    let snapshot = h
        .app
        .pull_requests
        .snapshot
        .as_ref()
        .expect("live snapshot");
    let repo = snapshot.repository.clone();
    let first = snapshot.rows.first().map(|r| (r.number, r.title.clone()));
    let screen = h.render();
    assert!(screen.contains(&repo), "{screen}");
    if let Some((number, title)) = first {
        assert!(screen.contains(&format!("#{number}")), "{screen}");
        let detail = h.press(KeyCode::Enter);
        assert!(detail.contains("Linked tasks"), "{detail}");
        assert!(detail.contains(" PR details "), "{detail}");
        assert!(
            detail.contains("Status"),
            "PR list remains beside the detail: {detail}"
        );
        assert!(
            detail.contains(&title[..title.floor_char_boundary(20)]),
            "{detail}"
        );
        let previous = h.app.pull_requests.row().unwrap().id.clone();
        if h.app.pull_requests.visible.len() > 1 {
            let next = h.press(KeyCode::Char('j'));
            assert_ne!(h.app.pull_requests.row().unwrap().id, previous);
            assert!(next.contains(" PR details "), "{next}");
            h.press(KeyCode::Char('k'));
            assert_eq!(h.app.pull_requests.row().unwrap().id, previous);
        }
        println!("{detail}");
    } else {
        assert!(screen.contains("No PRs match"), "{screen}");
    }
    println!("{screen}");
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO with an open PR; reads only"]
fn live_task_links_and_failed_refresh_preserve_real_task_bytes() {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let live = switchbard_core::fetch_pull_requests(std::path::Path::new(&root)).unwrap();
    let row = live.rows.first().expect("live source must have an open PR");
    let mut h = Harness::new();
    for args in [
        vec!["init", "-q"],
        vec!["remote", "add", "origin", &live.repository_url],
    ] {
        assert!(std::process::Command::new("git")
            .arg("-C")
            .arg(&h.root)
            .args(args)
            .status()
            .unwrap()
            .success());
    }
    let id = h.app.selected_task().unwrap().id.clone();
    switchbard_core::edit_backlog_task(
        &h.root,
        &id,
        &switchbard_core::BacklogTaskPatch {
            references: Some(vec![row.url.clone()]),
            ..Default::default()
        },
    )
    .unwrap();
    h.app = open_app(&h.root, &h.config_path);
    let before: Vec<_> = std::fs::read_dir(h.root.join("backlog/tasks"))
        .unwrap()
        .map(|p| {
            let path = p.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    h.press(KeyCode::Tab);
    settle(&mut h);
    assert!(
        h.app.pull_requests.error.is_none(),
        "{:?}",
        h.app.pull_requests.error
    );
    h.type_text(&format!("/id:{}", row.number));
    h.press(KeyCode::Enter);
    for (width, height) in [(80, 24), (120, 40), (180, 50), (40, 12)] {
        h.terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        let screen = h.render();
        assert!(screen.contains(&id), "{screen}");
        assert!(screen.contains(&format!("#{}", row.number)), "{screen}");
        h.press(KeyCode::Enter);
        for _ in 0..20 {
            h.app
                .handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
            h.render();
        }
        let bottom = h.render();
        assert!(bottom.contains("progress."), "{bottom}");
        h.press(KeyCode::Enter);
    }
    h.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 20)).unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&h.root)
        .args(["remote", "remove", "origin"])
        .status()
        .unwrap()
        .success());
    h.press(KeyCode::Char('r'));
    settle(&mut h);
    let screen = h.render();
    assert!(screen.contains("STALE"), "{screen}");
    assert!(screen.contains(&format!("#{}", row.number)), "{screen}");
    for (path, bytes) in before {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    println!("{screen}");
}

#[test]
fn pr_filters_use_the_shared_picker_and_preserve_task_filter_and_restart() {
    let mut h = Harness::new();
    h.type_text("/theme");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Tab);
    h.type_text("f1");
    let screen = h.render();
    assert!(
        screen.contains("Open") && screen.contains("Closed") && screen.contains("Merged"),
        "{screen}"
    );
    h.press(KeyCode::Char('3'));
    assert_eq!(h.app.pull_requests.filter, "status:merged");
    assert_eq!(h.app.state.filter, "theme");
    let state = h.app.resume_state();
    h.app = open_app(&h.root, &h.config_path);
    h.app.resume_from(Some(&state));
    assert_eq!(h.app.pull_requests.filter, "status:merged");
    h.press(KeyCode::Esc);
    assert_eq!(h.app.pull_requests.filter, "");
    h.press(KeyCode::Tab);
    assert!(h.render().contains("Add dark theme"));
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO with historical PRs"]
fn live_all_states_can_be_filtered_without_refetching() {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let mut h = Harness::new();
    h.app = open_app(std::path::Path::new(&root), &h.config_path);
    h.press(KeyCode::Tab);
    settle(&mut h);
    let snapshot = h.app.pull_requests.snapshot.as_ref().expect("snapshot");
    let observed = snapshot.observed_at;
    assert!(snapshot
        .rows
        .iter()
        .any(|r| r.lifecycle == switchbard_core::PrLifecycle::Merged));
    assert_eq!(h.app.pull_requests.visible.len(), snapshot.rows.len());
    h.type_text("f1");
    h.press(KeyCode::Char('3'));
    assert!(!h.app.pull_requests.visible.is_empty());
    let snapshot = h.app.pull_requests.snapshot.as_ref().unwrap();
    assert!(h
        .app
        .pull_requests
        .visible
        .iter()
        .all(|&i| snapshot.rows[i].lifecycle == switchbard_core::PrLifecycle::Merged));
    assert_eq!(snapshot.observed_at, observed);
    println!("{}", h.render());
    h.press(KeyCode::Esc);
    h.type_text("/status:open");
    h.press(KeyCode::Enter);
    let snapshot = h.app.pull_requests.snapshot.as_ref().unwrap();
    assert!(h
        .app
        .pull_requests
        .visible
        .iter()
        .all(|&i| snapshot.rows[i].lifecycle == switchbard_core::PrLifecycle::Open));
    assert_eq!(snapshot.observed_at, observed);
    if snapshot.truncated {
        h.type_text(":more");
        h.press(KeyCode::Enter);
        settle(&mut h);
        let snapshot = h.app.pull_requests.snapshot.as_ref().unwrap();
        assert_eq!(snapshot.limit, 200);
        assert!(snapshot.rows.len() > 100);
        assert_eq!(h.app.pull_requests.filter, "status:open");
    }
    println!("{}", h.render());
}
