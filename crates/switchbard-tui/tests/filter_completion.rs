//! Tab completion in the `/` filter editor: keys from the column catalog,
//! values from the same list the `p` menu and column filter pickers read.

mod harness;

use crossterm::event::KeyCode;
use harness::*;
use std::time::{Duration, Instant};

/// The PR page needs no live GitHub read to prove the shared editor works
/// there too: a temp repo with no GitHub remote resolves to "Unavailable"
/// immediately, which is enough to exercise key completion and Esc without
/// requiring authenticated network access this suite otherwise gates behind
/// `#[ignore]` (see `tests/pull_requests.rs`).
fn settle(h: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(100);
    while h.app.pull_requests.loading() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        h.app.tick();
    }
    assert!(!h.app.pull_requests.loading(), "PR read exceeded its bound");
}

/// The candidate line, when there is one: the footer's last rendered row
/// while the input line itself is the second-to-last (`filter_completion`
/// only grows the footer from one row to two).
fn hint_line(screen: &str) -> Option<&str> {
    let lines: Vec<&str> = screen.lines().collect();
    let input = lines
        .iter()
        .rposition(|line| line.trim_start().starts_with('/'))?;
    lines.get(input + 1).copied()
}

#[test]
fn empty_editor_offers_every_key_bounded_with_a_more_tail() {
    let mut h = Harness::new();
    let screen = h.press(KeyCode::Char('/'));
    let hint = hint_line(&screen).expect("candidates shown for an empty word");
    for key in [
        "ball", "blocked", "due", "filed", "goal", "id", "label", "planning",
    ] {
        assert!(hint.contains(key), "{key} missing: {hint}");
    }
    assert!(hint.contains("+4 more"), "{hint}");
    assert!(
        !hint.contains("status"),
        "capped list still shows status: {hint}"
    );
}

#[test]
fn unique_key_prefix_completes_on_tab() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("sta");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("/status:▏"), "{screen}");
    assert_eq!(h.app.filter_text(), "status:");
}

#[test]
fn ambiguous_key_prefix_lists_candidates_and_a_second_tab_only_narrows_the_shared_prefix() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("p");
    let hint = hint_line(&screen).expect("ambiguous key candidates shown");
    for key in ["planning", "pri", "project"] {
        assert!(hint.contains(key), "{key} missing: {hint}");
    }
    // No shared prefix beyond "p" itself: Tab cannot narrow further without
    // more typing.
    let screen = h.press(KeyCode::Tab);
    assert_eq!(h.app.filter_text(), "p");
    assert!(hint_line(&screen).is_some(), "still ambiguous: {screen}");

    let screen = h.type_text("ro");
    let hint = hint_line(&screen).expect("one candidate left, but not yet exact");
    assert!(hint.contains("project"), "{hint}");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("/project:▏"), "{screen}");
    assert_eq!(h.app.filter_text(), "project:");
}

#[test]
fn no_matching_key_leaves_tab_a_no_op() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("zz");
    assert!(hint_line(&screen).is_none(), "{screen}");
    let screen = h.press(KeyCode::Tab);
    assert_eq!(h.app.filter_text(), "zz");
    assert!(hint_line(&screen).is_none(), "{screen}");
}

#[test]
fn unique_value_completes_on_tab_and_applies_on_enter() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("status:i");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("/status:inprogress▏"), "{screen}");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("1/3"), "{screen}");
    assert_eq!(visible_titles(&h), ["Fix login redirect loop"]);
}

#[test]
fn ambiguous_value_narrows_as_the_key_prefix_does() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("pri:");
    let hint = hint_line(&screen).expect("all three priorities offered");
    for value in ["high", "low", "medium"] {
        assert!(hint.contains(value), "{value} missing: {hint}");
    }
    let screen = h.type_text("h");
    let hint = hint_line(&screen).expect("still one candidate above what is typed");
    assert!(hint.contains("high"), "{hint}");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("/pri:high▏"), "{screen}");
    let screen = h.press(KeyCode::Enter);
    assert_eq!(visible_titles(&h), ["Write onboarding guide"]);
    let _ = screen;
}

#[test]
fn negated_value_completes_after_the_bang() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("status:!t");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("/status:!todo▏"), "{screen}");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("1/3"), "{screen}");
    assert_eq!(visible_titles(&h), ["Fix login redirect loop"]);
}

#[test]
fn custom_field_key_and_value_both_complete() {
    let mut h = Harness::new();
    declare_field(
        &h.root,
        "counterparty",
        switchbard_core::FieldKind::Enum,
        &["GoSBA Loans", "Nick"],
        true,
    );
    seed_with_custom(
        &h.root,
        "Nick payoff letter",
        "To Do",
        &[],
        &[("counterparty", "Nick")],
    );
    h.app = open_app(&h.root, &h.config_path);
    h.render();

    h.press(KeyCode::Char('/'));
    h.type_text("count");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("/counterparty:▏"), "{screen}");

    h.type_text("n");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("/counterparty:nick▏"), "{screen}");
    let screen = h.press(KeyCode::Enter);
    assert!(screen.contains("1/4"), "{screen}");
    assert_eq!(visible_titles(&h), ["Nick payoff letter"]);
}

#[test]
fn esc_leaves_the_filter_exactly_as_it_was_before_the_editor_opened_even_mid_candidate_list() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("status:todo");
    h.press(KeyCode::Enter);
    assert_eq!(h.app.filter_text(), "status:todo");

    h.press(KeyCode::Char('/'));
    let screen = h.type_text("p");
    assert!(
        hint_line(&screen).is_some(),
        "candidate list should be up: {screen}"
    );
    let screen = h.press(KeyCode::Esc);

    assert_eq!(h.app.filter_text(), "status:todo");
    assert!(screen.contains("2/3"), "{screen}");
    assert!(
        hint_line(&screen).is_none(),
        "no candidate row after Esc: {screen}"
    );
}

#[test]
fn candidate_line_truncates_rather_than_overflows_a_narrow_terminal() {
    let mut h = Harness::new();
    h.terminal.backend_mut().resize(40, 12);
    let screen = h.press(KeyCode::Char('/'));
    let hint = hint_line(&screen).expect("candidates shown for an empty word");
    assert!(hint.chars().count() <= 40, "{hint}");
    assert!(hint.contains('…'), "{hint}");
}

#[test]
fn esc_after_typing_into_a_previously_empty_filter_reverts_to_empty() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    h.type_text("dark");
    let screen = h.press(KeyCode::Esc);
    assert_eq!(h.app.filter_text(), "");
    assert!(screen.contains("3/3"), "{screen}");
}

/// U+00A0 NO-BREAK SPACE reports `true` from `char::is_whitespace()` but is
/// two bytes in UTF-8: splitting the editor's current word at a byte offset
/// found via `rfind` (rather than a `char_indices` boundary) sliced mid
/// codepoint here and panicked on every render frame while typing.
#[test]
fn a_multibyte_whitespace_character_does_not_panic_the_editor() {
    let mut h = Harness::new();
    h.press(KeyCode::Char('/'));
    let screen = h.type_text("status:todo\u{a0}pri");
    assert!(screen.contains("status:todo\u{a0}pri"), "{screen}");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("pri:"), "{screen}");
}

#[test]
fn pr_page_key_completes_uniquely_and_esc_reverts_without_github_access() {
    let mut h = Harness::new();
    h.next_list_page();
    settle(&mut h);
    assert!(h.render().contains("Unavailable"));

    h.press(KeyCode::Char('/'));
    h.type_text("sta");
    let screen = h.press(KeyCode::Tab);
    assert!(screen.contains("/status:▏"), "{screen}");
    assert_eq!(h.app.filter_text(), "status:");

    let screen = h.press(KeyCode::Esc);
    assert_eq!(h.app.filter_text(), "");
    assert!(screen.contains("Unavailable"), "{screen}");
}
