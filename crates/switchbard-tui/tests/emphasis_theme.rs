//! User Lua overrides must survive loading and reach real task cells.
mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::style::{Color, Modifier};

fn command(h: &mut Harness, text: &str) {
    h.type_text(&format!(":{text}"));
    h.press(KeyCode::Enter);
}

fn modifiers(h: &Harness, needle: &str) -> Modifier {
    let buffer = h.terminal.backend().buffer();
    for row in buffer.content.chunks(usize::from(buffer.area.width)) {
        let text: String = row.iter().map(|cell| cell.symbol()).collect();
        if let Some(index) = text.find(needle) {
            return row[text[..index].chars().count()].modifier;
        }
    }
    panic!("missing rendered text {needle}");
}

#[test]
fn custom_role_composition_clears_inherited_bold_and_keeps_strikethrough() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        r##"return { theme = { emphasis = {
        strong = { fg = "#bada55", bold = true, italic = true },
        struck = { bold = false, underline = true, strikethrough = true }
    } } }"##,
    )
    .unwrap();
    h.app.tick();
    command(&mut h, "paint column:title=strong+struck");
    let style = modifiers(&h, "Write onboarding guide");
    assert!(!style.contains(Modifier::BOLD));
    assert!(style.contains(Modifier::ITALIC | Modifier::UNDERLINED | Modifier::CROSSED_OUT));
    assert_eq!(
        cell_fg(&h, "Write onboarding guide"),
        Some(Color::Rgb(0xba, 0xda, 0x55))
    );
    std::fs::write(
        &h.config_path,
        r##"return { theme = { emphasis = {
        strong = { bold = false, italic = false }, struck = { strikethrough = false }
    } } }"##,
    )
    .unwrap();
    h.app.tick();
    h.render();
    assert!(!modifiers(&h, "Write onboarding guide")
        .intersects(Modifier::BOLD | Modifier::ITALIC | Modifier::CROSSED_OUT));
}

#[test]
fn bad_role_reports_file_and_key_without_losing_the_board() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        "return { theme = { emphasis = { typo = { bold = true } } } }",
    )
    .unwrap();
    h.app.tick();
    let screen = h.render();
    assert!(screen.contains("Fix login"), "{screen}");
    assert!(
        h.app
            .config
            .warnings
            .iter()
            .any(|warning| warning.contains("tui.lua") && warning.contains("theme.emphasis.typo")),
        "{:?}",
        h.app.config.warnings
    );
}

#[test]
fn presets_switch_live_and_plain_does_not_force_a_canvas() {
    let mut h = Harness::new();
    for name in ["berg", "bloomberg", "darkroom", "light", "plain"] {
        command(&mut h, &format!("theme {name}"));
        assert!(h.render().contains("Fix login"));
        let canvas = h.terminal.backend().buffer()[(50, 18)].bg;
        if name == "plain" {
            assert_eq!(canvas, Color::Reset);
        } else {
            assert_eq!(Some(canvas), h.app.config.theme.background());
            assert_ne!(canvas, Color::Reset);
        }
        command(&mut h, "paint column:title=quiet");
        assert_eq!(
            modifiers(&h, "Write onboarding guide").contains(Modifier::DIM),
            name == "plain"
        );
    }
}

#[test]
fn only_band_can_fill_a_row_even_when_custom_roles_request_backgrounds() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        r##"return { theme = { emphasis = {
        strong = { bg = "#abcdef" }, quiet = { reverse = true }
    } } }"##,
    )
    .unwrap();
    h.app.tick();
    command(&mut h, "paint column:title=strong");
    assert!(modifiers(&h, "Write onboarding guide").contains(Modifier::BOLD));
    assert_eq!(
        cell_bg(&h, "Write onboarding guide"),
        h.app.config.theme.background()
    );
    command(&mut h, "paint column:title=quiet");
    assert!(!modifiers(&h, "Write onboarding guide").contains(Modifier::REVERSED));
    assert_eq!(
        cell_fg(&h, "Write onboarding guide"),
        Some(Color::Rgb(0xac, 0xac, 0xae))
    );
    for key in ["theme.emphasis.strong.bg", "theme.emphasis.quiet.reverse"] {
        assert!(
            h.app
                .config
                .warnings
                .iter()
                .any(|warning| warning.contains("tui.lua") && warning.contains(key)),
            "missing warning for {key}: {:?}",
            h.app.config.warnings
        );
    }
}

#[test]
fn legacy_custom_themes_gain_safe_roles_and_invalid_overrides_keep_the_fallback() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        r##"return { theme = "legacy", themes = { legacy = {
        background = "#101214", text = "#e6edf3", hint = "#acacae", accent = "#ffb8aa",
        working = { bg = "#163b30" }, emphasis = { strong = { bg = "#abcdef" } }
    } } }"##,
    )
    .unwrap();
    h.app.tick();
    command(&mut h, "paint column:title=strong");
    assert!(modifiers(&h, "Write onboarding guide").contains(Modifier::BOLD));
    assert_eq!(
        cell_bg(&h, "Write onboarding guide"),
        Some(Color::Rgb(0x10, 0x12, 0x14))
    );
    command(&mut h, "paint column:title=quiet");
    assert_eq!(
        cell_fg(&h, "Write onboarding guide"),
        Some(Color::Rgb(0xac, 0xac, 0xae))
    );
    assert!(!modifiers(&h, "Write onboarding guide").contains(Modifier::DIM));
    command(&mut h, "paint column:title=alert+struck");
    assert_eq!(
        cell_fg(&h, "Write onboarding guide"),
        Some(Color::Rgb(0xff, 0xb8, 0xaa))
    );
    assert!(
        modifiers(&h, "Write onboarding guide").contains(Modifier::BOLD | Modifier::CROSSED_OUT)
    );
    command(&mut h, "paint column:title=band");
    assert_eq!(
        cell_bg(&h, "Write onboarding guide"),
        Some(Color::Rgb(0x16, 0x3b, 0x30))
    );
    assert!(h
        .app
        .config
        .warnings
        .iter()
        .any(|warning| warning.contains("theme.emphasis.strong.bg")));
}

#[test]
fn minimal_terminal_owned_theme_has_modifier_only_fallback_roles() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        "return { theme = 'minimal', themes = { minimal = {} } }",
    )
    .unwrap();
    h.app.tick();
    command(&mut h, "paint column:title=quiet");
    assert!(modifiers(&h, "Write onboarding guide").contains(Modifier::DIM));
    command(&mut h, "paint column:title=band+strong+struck");
    assert!(modifiers(&h, "Write onboarding guide")
        .contains(Modifier::REVERSED | Modifier::BOLD | Modifier::CROSSED_OUT));
}
