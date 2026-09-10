//! Real app, terminal and GitHub observations. No substituted clients or row counts.
mod harness;
use crossterm::event::KeyCode;
use harness::*;
use ratatui::{backend::TestBackend, style::Color, Terminal};
use std::time::{Duration, Instant};
use switchbard_tui::{config::Surface, page::Page};

fn settle(h: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(140);
    while h.app.pull_requests.loading() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        h.app.tick();
    }
    assert!(!h.app.pull_requests.loading(), "bounded worker finished");
}

fn badges(h: &Harness) -> Vec<String> {
    let style = h.app.config.theme.style(Surface::AttentionBadge);
    h.terminal.backend().buffer().content[..h.terminal.size().unwrap().width as usize]
        .split(|cell| Some(cell.bg) != style.bg)
        .map(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>())
        .filter(|text| text.trim().chars().all(|c| c.is_ascii_digit()) && !text.trim().is_empty())
        .collect()
}

#[test]
fn tasks_start_observation_and_unknown_is_not_an_empty_count() {
    let mut h = Harness::new();
    assert_eq!(h.app.page, Page::Tasks);
    assert!(h.render().lines().next().unwrap().contains('?'));
    h.app.tick();
    assert!(h.app.pull_requests.loading());
    assert!(h.render().lines().next().unwrap().contains('…'));
    settle(&mut h);
    assert!(h.app.pull_requests.error.is_some());
    assert!(h.render().lines().next().unwrap().contains('?'));
    assert!(badges(&h).is_empty());
    for (width, height) in [(40, 8), (80, 24), (120, 40), (180, 50)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        for _ in 0..3 {
            let screen = h.render();
            let nav = screen.lines().next().unwrap();
            assert!(nav.contains("Tasks") && nav.contains("Inbox"), "{screen}");
            assert!(
                nav.contains(if width < 52 { "PRs" } else { "Pull Requests" }),
                "{screen}"
            );
            h.press(KeyCode::Tab);
        }
    }
}

#[test]
#[ignore = "requires authenticated GitHub and SBT_PR_REPO; reads only"]
fn live_total_survives_page_filters_theme_reload_and_failed_refresh() {
    let root = std::env::var_os("SBT_PR_REPO").expect("SBT_PR_REPO");
    let mut h = Harness::new();
    h.app = open_app(std::path::Path::new(&root), &h.config_path);
    h.app.tick();
    settle(&mut h);
    let total = *h
        .app
        .pull_requests
        .snapshot
        .as_ref()
        .expect("snapshot")
        .open_count
        .as_ref()
        .expect("exact total");
    assert_eq!(h.app.page, Page::Tasks, "no PR-page visit needed");
    let style = h.app.config.theme.style(Surface::AttentionBadge);
    assert_eq!(style.fg, Some(Color::Black));
    assert_eq!(style.bg, Some(Color::Rgb(244, 159, 49)));
    let mut evidence = String::new();
    for (width, height) in [(40, 8), (80, 24), (120, 40), (180, 50)] {
        h.terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        for _ in 0..3 {
            let screen = h.render();
            let counts = badges(&h);
            if total == 0 {
                assert!(counts.is_empty(), "{screen}");
            } else {
                assert_eq!(counts, vec![format!(" {total} ")], "{screen}");
            }
            assert!(screen.lines().next().unwrap().contains("Inbox"), "{screen}");
            evidence.push_str(&format!("{:?} {width}x{height}\n{screen}\n", h.app.page));
            h.press(KeyCode::Tab);
        }
    }
    h.press(KeyCode::Tab);
    h.press(KeyCode::Char('/'));
    h.type_text("zzzz-no-pr-matches-this");
    h.press(KeyCode::Enter);
    assert!(h.app.pull_requests.visible.is_empty());
    assert_eq!(
        h.app.pull_requests.snapshot.as_ref().unwrap().open_count,
        Ok(total)
    );
    if total > 0 {
        assert_eq!(badges(&h), vec![format!(" {total} ")]);
    }
    std::fs::write(&h.config_path, "return { theme = 'legacy', themes = { legacy = { chip = { fg = 'black', bg = '#f49f31' } } } }").unwrap();
    h.app.tick();
    h.render();
    assert_eq!(
        h.app.config.theme.style(Surface::AttentionBadge).bg,
        Some(Color::Rgb(244, 159, 49))
    );
    if total > 0 {
        assert_eq!(badges(&h), vec![format!(" {total} ")]);
    }
    std::fs::write(
        &h.config_path,
        "return { theme = { attention_badge = { fg = '#010203', bg = '#abcdef' } } }",
    )
    .unwrap();
    h.app.tick();
    h.render();
    let style = h.app.config.theme.style(Surface::AttentionBadge);
    assert_eq!(style.bg, Some(Color::Rgb(171, 205, 239)));
    if total > 0 {
        assert_eq!(badges(&h), vec![format!(" {total} ")]);
    }
    h.app.pull_requests.refresh(&h.root); // real invalid repo; retain live cache
    settle(&mut h);
    let screen = h.render();
    assert!(h.app.pull_requests.error.is_some());
    assert!(screen.lines().next().unwrap().contains('?'), "{screen}");
    assert_eq!(
        h.app.pull_requests.snapshot.as_ref().unwrap().open_count,
        Ok(total)
    );
    evidence.push_str(&screen);
    let path = std::env::temp_dir().join(format!("sbt-navigation-badges-{total}.txt"));
    std::fs::write(path, evidence).unwrap();
}
