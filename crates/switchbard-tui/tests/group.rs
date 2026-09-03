//! `o`: one section per project (or per value of another column), composed over
//! the filtered and sorted list.

mod harness;

use crossterm::event::KeyCode;
use harness::*;

fn grouped_harness() -> Harness {
    let mut h = Harness::new();
    seed_project(&h.root, "Chase", "In Progress", Some("Lenders"));
    seed_project(&h.root, "Ally", "Planned", Some("Lenders"));
    seed_in_project(&h.root, "Ally intake form", "To Do", "Ally", None);
    seed_in_project(&h.root, "Chase rate sheet", "Done", "Chase", None);
    seed_in_project(&h.root, "Chase portal login", "To Do", "Chase", None);
    seed_in_project(
        &h.root,
        "Chase portal MFA",
        "To Do",
        "Chase",
        Some("TASK-6"),
    );
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    h
}

#[test]
fn flat_view_hints_at_grouping_only_when_there_are_projects_to_group() {
    let mut h = Harness::new();
    assert!(!h.render().contains("o groups by project"));
    let mut h = grouped_harness();
    let screen = h.render();
    assert!(
        screen.contains("2 projects · o groups by project"),
        "{screen}"
    );
}

#[test]
fn o_sections_by_project_in_rank_order_with_status_and_progress() {
    let mut h = grouped_harness();
    let screen = h.press(KeyCode::Char('o'));
    assert_eq!(
        screen_rows(&h),
        [
            "# Chase · In Progress · 1/3",
            "Chase portal login",
            "Chase portal MFA",
            "Chase rate sheet",
            "# Ally · Planned · 0/1",
            "Ally intake form",
            "# no project",
            "Fix login redirect loop",
            "Write onboarding guide",
            "Add dark theme",
        ]
    );
    assert!(screen.contains("group:project · Lenders"), "{screen}");
    assert!(screen.contains("Chase · In Progress · 1/3"), "{screen}");
    assert_eq!(h.app.status, "grouped by project · o flattens");
    let screen = h.press(KeyCode::Char('o'));
    assert!(!screen.contains("Chase · In Progress"), "{screen}");
    assert_eq!(visible_titles(&h).len(), 7);
}

#[test]
fn cursor_skips_headings_and_lands_on_tasks() {
    let mut h = grouped_harness();
    h.press(KeyCode::Char('o'));
    assert_eq!(
        h.selected_title(),
        "Fix login redirect loop",
        "grouping keeps the cursor's task"
    );
    h.press(KeyCode::Char('g'));
    assert_eq!(h.selected_title(), "Chase portal login");
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('j'));
    h.press(KeyCode::Char('j'));
    assert_eq!(h.selected_title(), "Ally intake form");
    h.press(KeyCode::Char('k'));
    assert_eq!(h.selected_title(), "Chase rate sheet");
    h.press(KeyCode::Char('G'));
    assert_eq!(h.selected_title(), "Add dark theme");
    h.press(KeyCode::Char('g'));
    assert_eq!(h.selected_title(), "Chase portal login");
}

#[test]
fn grouping_composes_with_filter_and_sort_and_omits_empty_sections() {
    let mut h = grouped_harness();
    h.press(KeyCode::Char('o'));
    h.press(KeyCode::Char('/'));
    h.type_text("status:todo");
    h.press(KeyCode::Enter);
    assert_eq!(
        screen_rows(&h),
        [
            "# Chase · In Progress · 1/3",
            "Chase portal login",
            "Chase portal MFA",
            "# Ally · Planned · 0/1",
            "Ally intake form",
            "# no project",
            "Write onboarding guide",
            "Add dark theme",
        ]
    );
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('4'));
    h.press(KeyCode::Char('2'));
    assert_eq!(
        visible_titles(&h),
        [
            "Chase portal login",
            "Chase portal MFA",
            "Ally intake form",
            "Write onboarding guide",
            "Add dark theme",
        ],
        "descending title inside each section, sections still ranked, sub-issue under parent"
    );
    h.press(KeyCode::Char('/'));
    h.type_text(" nothing-matches");
    let screen = h.press(KeyCode::Enter);
    assert!(h.app.rows.is_empty(), "{screen}");
}

#[test]
fn a_sub_issue_stays_under_its_parent_even_when_sort_would_split_them() {
    let mut h = grouped_harness();
    h.press(KeyCode::Char('o'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('4'));
    h.press(KeyCode::Char('1'));
    let rows = screen_rows(&h);
    let parent = rows.iter().position(|r| r == "Chase portal login").unwrap();
    assert_eq!(rows[parent + 1], "Chase portal MFA", "{rows:?}");
    h.press(KeyCode::Char('/'));
    h.type_text("MFA");
    h.press(KeyCode::Enter);
    assert_eq!(
        screen_rows(&h),
        ["# Chase · In Progress · 1/3", "Chase portal MFA"],
        "a filtered-out parent is not resurrected"
    );
}

#[test]
fn group_by_another_column_and_the_command_form_and_saved_views() {
    let mut h = grouped_harness();
    h.press(KeyCode::Char('2'));
    h.press(KeyCode::Char('o'));
    let rows = screen_rows(&h);
    assert_eq!(rows[0], "# To Do", "{rows:?}");
    assert!(rows.contains(&"# In Progress".to_string()) && rows.contains(&"# Done".to_string()));
    h.press(KeyCode::Char(':'));
    h.type_text("group off");
    h.press(KeyCode::Enter);
    assert!(h.app.state.group.is_none());
    h.press(KeyCode::Char(':'));
    h.type_text("group labels");
    h.press(KeyCode::Enter);
    assert_eq!(
        h.app.status,
        "group by one of status, priority, project, ball, or off"
    );
    h.press(KeyCode::Char(':'));
    h.type_text("group project");
    h.press(KeyCode::Enter);
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('2'));
    let reopened = open_app(&h.root, &h.config_path);
    assert!(reopened.views.get(1).unwrap().group == Some(switchbard_tui::columns::Column::Project));
    let mut h2 = Harness {
        _dir: h._dir,
        root: h.root,
        config_path: h.config_path,
        app: reopened,
        terminal: h.terminal,
    };
    h2.press(KeyCode::Char('v'));
    let screen = h2.press(KeyCode::Char('2'));
    assert!(screen.contains("group:project"), "{screen}");
    assert_eq!(screen_rows(&h2)[0], "# Chase · In Progress · 1/3");
}

#[test]
fn headings_use_their_own_theme_color_not_the_rows_paint() {
    let mut h = grouped_harness();
    h.press(KeyCode::Char('o'));
    h.press(KeyCode::Char('/'));
    h.press(KeyCode::Esc);
    h.press(KeyCode::Char(':'));
    h.type_text("group project");
    h.press(KeyCode::Enter);
    let screen = h.render();
    assert!(screen.contains("▸ Chase · In Progress · 1/3"), "{screen}");
    let heading = cell_fg(&h, "▸ Chase").unwrap();
    assert_eq!(heading, ratatui::style::Color::Rgb(0xe6, 0xed, 0xf3));
    std::fs::write(
        &h.config_path,
        "return { theme = { heading = \"magenta\" } }",
    )
    .unwrap();
    h.app.tick();
    h.render();
    assert_eq!(cell_fg(&h, "▸ Chase"), Some(ratatui::style::Color::Magenta));
}

#[test]
fn a_surface_can_be_reshaded_as_a_table_with_bg_and_modifiers() {
    let mut h = grouped_harness();
    std::fs::write(
        &h.config_path,
        "return { theme = { heading = { fg = \"black\", bg = \"yellow\", bold = true }, selected = { bg = \"blue\" }, columns = { title = \"link\" } } }",
    )
    .unwrap();
    h.app.tick();
    h.press(KeyCode::Char('o'));
    h.render();
    let buffer = h.terminal.backend().buffer();
    let width = buffer.area.width as usize;
    let line = buffer
        .content
        .chunks(width)
        .position(|cells| {
            cells
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
                .contains("▸ Chase")
        })
        .unwrap();
    let cell = &buffer.content[line * width + 2];
    assert_eq!(cell.fg, ratatui::style::Color::Black);
    assert_eq!(cell.bg, ratatui::style::Color::Yellow);
    assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
    assert_eq!(
        cell_fg(&h, "Chase portal login"),
        Some(ratatui::style::Color::Rgb(0x58, 0xa6, 0xff)),
        "title column reassigned to the link surface"
    );
}
