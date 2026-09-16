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

/// Any role may own a fill (TASK-237). What is refused is a fill no ink in the
/// theme can be read on, which is checked, not assumed.
#[test]
fn a_role_fill_reaches_cells_and_a_fill_no_ink_reads_on_is_refused() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        r##"return { theme = { emphasis = {
        alert = { fg = "#8a1c24", bg = "#f6c8d4" }, quiet = { bg = "#a89c94" }
    } } }"##,
    )
    .unwrap();
    h.app.tick();
    command(&mut h, "paint column:title=alert");
    assert_eq!(
        cell_bg(&h, "Write onboarding guide"),
        Some(Color::Rgb(0xf6, 0xc8, 0xd4)),
        "the declared fill reaches the cell"
    );
    assert_eq!(
        cell_fg(&h, "Write onboarding guide"),
        Some(Color::Rgb(0x8a, 0x1c, 0x24)),
        "with the ink declared beside it"
    );
    command(&mut h, "paint column:title=quiet");
    assert_eq!(
        cell_bg(&h, "Write onboarding guide"),
        h.app.config.theme.background(),
        "a fill every ink fails on is dropped, not rendered"
    );
    assert!(
        h.app.config.warnings.iter().any(|warning| {
            warning.contains("tui.lua") && warning.contains("theme.emphasis.quiet.bg")
        }),
        "{:?}",
        h.app.config.warnings
    );
    assert!(
        !h.app
            .config
            .warnings
            .iter()
            .any(|warning| warning.contains("theme.emphasis.alert.bg")),
        "a readable fill draws no warning: {:?}",
        h.app.config.warnings
    );
}

/// Every colored preset declares its own slots; a slot a theme omits is derived
/// from the matching palette color at a fixed distance from the canvas.
#[test]
fn highlight_slots_come_from_the_preset_and_missing_ones_derive_from_the_palette() {
    let mut h = Harness::new();
    for name in ["berg", "bloomberg", "darkroom", "light"] {
        command(&mut h, &format!("theme {name}"));
        let theme = &h.app.config.theme;
        let slots = theme.highlight_slots();
        assert!(slots.len() >= 3, "{name} offers {slots:?}");
        let mut fills = std::collections::HashSet::new();
        for slot in &slots {
            let style = theme
                .highlight_style(*slot, &h.app.config.palette)
                .unwrap_or_else(|| panic!("{name} h{slot} resolves"));
            assert!(style.bg.is_some(), "{name} h{slot} is a fill");
            assert!(style.fg.is_some(), "{name} h{slot} carries default ink");
            fills.insert(format!("{:?}", style.bg));
        }
        assert_eq!(fills.len(), slots.len(), "{name} slots are distinguishable");
        let derived = theme
            .highlight_style(7, &h.app.config.palette)
            .expect("an undeclared slot still resolves");
        assert!(
            matches!(derived.bg, Some(Color::Rgb(..))),
            "{name} derives h7 from the palette: {derived:?}"
        );
        assert_ne!(derived.bg, theme.background(), "{name} h7 reads as a fill");
    }
    std::fs::write(&h.config_path, "return { theme = 'plain' }").unwrap();
    h.app.tick();
    let style = h
        .app
        .config
        .theme
        .highlight_style(1, &h.app.config.palette)
        .expect("plain declares a terminal-owned slot");
    assert!(
        style.add_modifier.contains(Modifier::REVERSED),
        "plain reverses the terminal's own colors: {style:?}"
    );
}

/// plain owns its colors, so its slots are declared rather than derived. The
/// reverse-video fallback must not leak into them: a slot that declares its own
/// fill renders exactly that, and only the slot that asks for reverse gets it.
#[test]
fn declared_slots_on_a_terminal_owned_theme_keep_the_fallback_out() {
    let mut h = Harness::new();
    std::fs::write(&h.config_path, "return { theme = 'plain' }").unwrap();
    h.app.tick();
    for (slot, fill, ink) in [
        (2, Color::Blue, Color::White),
        (3, Color::Magenta, Color::Black),
    ] {
        let style = h
            .app
            .config
            .theme
            .highlight_style(slot, &h.app.config.palette)
            .unwrap_or_else(|| panic!("plain declares h{slot}"));
        assert_eq!(style.bg, Some(fill), "h{slot} fill");
        assert_eq!(style.fg, Some(ink), "h{slot} ink");
        command(&mut h, &format!("paint column:title=h{slot}"));
        assert_eq!(cell_bg(&h, "Write onboarding guide"), Some(fill));
        assert_eq!(cell_fg(&h, "Write onboarding guide"), Some(ink));
        assert!(
            !modifiers(&h, "Write onboarding guide").contains(Modifier::REVERSED),
            "h{slot} renders its own colors, not inverted ones"
        );
    }
    command(&mut h, "paint column:title=h1");
    assert!(
        modifiers(&h, "Write onboarding guide").contains(Modifier::REVERSED),
        "h1 asks for reverse video and keeps it"
    );
}

/// A slot that names no position is reported, and the rest of the theme loads.
#[test]
fn an_unknown_highlight_slot_is_reported_without_losing_the_board() {
    let mut h = Harness::new();
    std::fs::write(
        &h.config_path,
        r##"return { theme = { highlights = { h99 = { bg = "#3b3023" } } } }"##,
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
            .any(|warning| warning.contains("theme.highlights.h99")),
        "{:?}",
        h.app.config.warnings
    );
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
    command(&mut h, "paint column:title=h1");
    assert!(
        matches!(cell_bg(&h, "Write onboarding guide"), Some(Color::Rgb(..))),
        "a theme that declares no slots still derives them"
    );
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
