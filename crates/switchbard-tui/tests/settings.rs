//! `,`: standing preferences under every view, per repo with a global fallback.

mod harness;

use crossterm::event::KeyCode;
use harness::*;
use ratatui::style::Color;
use switchbard_tui::config::Surface;

#[test]
fn hiding_a_status_applies_under_every_view_and_a_view_that_names_status_wins() {
    let mut h = Harness::new();
    seed(&h.root, "Old finished thing", "Done", &[]);
    h.press(KeyCode::Char('r'));
    assert_eq!(visible_titles(&h).len(), 4);
    let screen = h.press(KeyCode::Char(','));
    assert!(screen.contains("┌ settings ─"), "{screen}");
    assert!(screen.contains(" hide Done"), "{screen}");
    h.type_text("done");
    let screen = h.press(KeyCode::Enter);
    assert!(
        screen.contains("✓hide Done"),
        "toggled and still open: {screen}"
    );
    assert!(screen.contains("hide:done"), "title says so: {screen}");
    assert_eq!(h.app.status, "hide:done · this repo");
    h.press(KeyCode::Esc);
    assert_eq!(visible_titles(&h).len(), 3);
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('2'));
    assert_eq!(
        visible_titles(&h).len(),
        2,
        "status:todo view, Done still hidden"
    );
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char('/'));
    h.type_text("status:done");
    h.press(KeyCode::Enter);
    assert_eq!(
        visible_titles(&h),
        ["Old finished thing"],
        "a view that asks for Done gets it"
    );
    let file = std::fs::read_to_string(h.root.join("settings-repo.lua")).unwrap();
    assert!(file.contains("hide_statuses = { \"Done\" }"), "{file}");
    let fresh = open_app(&h.root, &h.config_path);
    assert!(fresh.settings.effective().is_hidden("Done"));
}

#[test]
fn g_in_the_panel_promotes_this_repos_settings_to_every_repo() {
    let mut h = Harness::new();
    seed(&h.root, "Old finished thing", "Done", &[]);
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Char(','));
    h.type_text("done");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('g'));
    assert_eq!(h.app.status, "hide:done · every repo");
    assert!(!h.root.join("settings-repo.lua").exists());
    let file = std::fs::read_to_string(h.root.join("settings.lua")).unwrap();
    assert!(file.contains("hide_statuses = { \"Done\" }"), "{file}");
    h.press(KeyCode::Esc);
    assert!(h.render().contains("hide:done"));
}

/// `:theme` swaps sbt's own surfaces and leaves the paint palette alone. The
/// states that matter are the switch itself, a name that does not exist, an
/// empty name, and what a reload does to a runtime choice.
#[test]
fn theme_command_switches_surfaces_reports_unknown_names_and_yields_to_reload() {
    let mut h = Harness::new();
    seed(&h.root, "Ship the thing", "todo", &[]);
    h.press(KeyCode::Char('r'));

    let berg_border = h.app.config.theme.style(Surface::Border).fg;

    h.press(KeyCode::Char(':'));
    h.type_text("theme darkroom");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app.status,
        "theme darkroom · keep it: theme = \"darkroom\" in tui.lua"
    );
    let dark_border = h.app.config.theme.style(Surface::Border).fg;
    assert_eq!(
        dark_border,
        Some(Color::Rgb(0x48, 0x3D, 0x34)),
        "border takes the darkroom preset's value"
    );
    assert_ne!(
        berg_border, dark_border,
        "the switch actually changed something"
    );

    // The palette is a separate choice and a theme switch must not redecide it.
    let palette_before = h.app.config.palette.clone();
    h.press(KeyCode::Char(':'));
    h.type_text("theme plain");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app.config.palette, palette_before,
        "theme leaves palette alone"
    );

    // Unknown and empty names both fail the same way, listing what exists.
    for typed in ["theme nope", "theme"] {
        h.press(KeyCode::Char(':'));
        h.type_text(typed);
        h.press(KeyCode::Enter);
        assert_eq!(
            h.app.status, "theme: one of berg, bloomberg, darkroom, plain",
            "`:{typed}` should list the presets"
        );
    }

    // A runtime switch is in-memory only: reload puts the file back in charge.
    h.press(KeyCode::Char(':'));
    h.type_text("theme darkroom");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('r'));
    assert_eq!(
        h.app.config.theme.style(Surface::Border).fg,
        berg_border,
        "reload discards the runtime theme and re-reads the config"
    );
}
