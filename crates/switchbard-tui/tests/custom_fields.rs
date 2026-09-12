//! A field the repo declares for itself behaves like a built-in column: shown,
//! filtered, sorted, grouped, painted, and named in a saved view.

mod harness;

use crossterm::event::KeyCode;
use harness::*;
use switchbard_core::{create_task_allocating_id, NewBacklogTask};
use switchbard_tui::telemetry::Telemetry;

/// A task carrying a project and ball on top of a declared field, for the
/// three-deep nesting test — `fixture()`'s own tasks carry neither.
fn seed_project_task(
    root: &std::path::Path,
    title: &str,
    project: &str,
    ball_label: &str,
    custom: &[(&str, &str)],
) {
    let task = NewBacklogTask {
        title: title.to_string(),
        description: format!("Description of {title}."),
        status: "To Do".to_string(),
        priority: "medium".to_string(),
        acceptance_criteria: vec!["It works".to_string()],
        parent: None,
        labels: vec!["loans".to_string(), ball_label.to_string()],
        assignees: Vec::new(),
        project: Some(project.to_string()),
        dependencies: Vec::new(),
        due_date: None,
        custom: custom
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
    };
    create_task_allocating_id(root, &task).unwrap();
}

/// The fixture every test here shares: one groupable enum field whose declared
/// order is deliberately *not* alphabetical (so declared order and alphabetical
/// order disagree and a test can tell them apart), one text field, and three
/// tasks — two carrying values and one carrying none.
fn fixture() -> Harness {
    let mut h = Harness::new();
    declare_field(
        &h.root,
        "counterparty",
        switchbard_core::FieldKind::Enum,
        &["GoSBA Loans", "Nick"],
        true,
    );
    declare_field(
        &h.root,
        "note",
        switchbard_core::FieldKind::Text,
        &[],
        false,
    );
    seed_with_custom(
        &h.root,
        "Nick payoff letter",
        "To Do",
        &["loans"],
        &[("counterparty", "Nick"), ("note", "call first")],
    );
    seed_with_custom(
        &h.root,
        "GoSBA rate sheet",
        "To Do",
        &["loans"],
        &[("counterparty", "GoSBA Loans")],
    );
    seed_with_custom(&h.root, "Unassigned paperwork", "To Do", &["loans"], &[]);
    h.app = open_app(&h.root, &h.config_path);
    h.render();
    h
}

fn filter_to(h: &mut Harness, filter: &str) {
    h.press(KeyCode::Char('/'));
    h.type_text(filter);
    h.press(KeyCode::Enter);
}

/// Only the three tasks this fixture seeds; `Harness::new`'s own carry no
/// declared-field values and no `loans` label.
fn only_the_fixture_rows(h: &mut Harness) {
    filter_to(h, "label:loans");
}

fn show_column(h: &mut Harness, name: &str) {
    h.press(KeyCode::Char('c'));
    pick_labelled(h, name);
    h.press(KeyCode::Esc);
}

#[test]
fn a_declared_field_shows_as_a_column_with_its_own_values() {
    let mut h = fixture();
    show_column(&mut h, "counterparty");
    let screen = h.render();
    assert!(
        header_line(&screen).contains("counterparty"),
        "the declared field is a column header: {screen}"
    );
    assert!(screen.contains("GoSBA Loans"), "{screen}");
    assert!(screen.contains("Nick"), "{screen}");
}

#[test]
fn a_text_field_shows_as_a_column_too() {
    let mut h = fixture();
    show_column(&mut h, "note");
    let screen = h.render();
    assert!(header_line(&screen).contains("note"), "{screen}");
    assert!(screen.contains("call first"), "{screen}");
}

#[test]
fn typing_the_field_name_filters_by_its_values() {
    let mut h = fixture();
    filter_to(&mut h, "counterparty:nick");
    assert_eq!(visible_titles(&h), ["Nick payoff letter"]);
}

#[test]
fn a_negated_field_term_hides_only_that_value() {
    let mut h = fixture();
    only_the_fixture_rows(&mut h);
    h.press(KeyCode::Char('/'));
    h.type_text(" counterparty:!nick");
    h.press(KeyCode::Enter);
    let titles = visible_titles(&h);
    assert!(
        titles.contains(&"GoSBA rate sheet".to_string()),
        "{titles:?}"
    );
    assert!(
        !titles.contains(&"Nick payoff letter".to_string()),
        "{titles:?}"
    );
    assert!(
        titles.contains(&"Unassigned paperwork".to_string()),
        "a task with no value is not the value being hidden: {titles:?}"
    );
}

#[test]
fn an_any_of_field_term_matches_either_value() {
    let mut h = fixture();
    filter_to(&mut h, "counterparty:nick,gosbaloans");
    let mut titles = visible_titles(&h);
    titles.sort();
    assert_eq!(titles, ["GoSBA rate sheet", "Nick payoff letter"]);
}

/// `f` on a declared column offers the values the tasks actually carry, the
/// same as `f` on status or priority.
#[test]
fn f_on_a_declared_column_offers_its_values() {
    let mut h = fixture();
    h.press(KeyCode::Char('f'));
    pick_labelled(&mut h, "counterparty");
    let picker = h.app.picker.as_ref().expect("the value picker is open");
    let mut values: Vec<&str> = picker
        .options
        .iter()
        .map(|option| option.label.as_str())
        .collect();
    values.sort_unstable();
    assert_eq!(values, ["GoSBA Loans", "Nick"]);
}

/// Declared order, not alphabetical: `GoSBA Loans` is declared first even
/// though `Nick` would sort after it either way — the tie-break that proves it
/// is the unset task, which sits last whichever direction the rest run.
#[test]
fn semantic_sort_follows_the_declared_values_with_the_unset_task_last() {
    let mut h = fixture();
    only_the_fixture_rows(&mut h);
    h.press(KeyCode::Char('s'));
    pick_labelled(&mut h, "counterparty");
    let screen = h.render();
    assert!(
        screen.contains("semantic (counterparty order)"),
        "an enum field offers a semantic order: {screen}"
    );
    h.press(KeyCode::Char('1'));
    assert_eq!(
        visible_titles(&h),
        [
            "GoSBA rate sheet",
            "Nick payoff letter",
            "Unassigned paperwork"
        ]
    );
}

#[test]
fn descending_reverses_the_values_but_still_keeps_the_unset_task_last() {
    let mut h = fixture();
    only_the_fixture_rows(&mut h);
    h.press(KeyCode::Char('s'));
    pick_labelled(&mut h, "counterparty");
    h.press(KeyCode::Char('3'));
    assert_eq!(
        visible_titles(&h),
        [
            "Nick payoff letter",
            "GoSBA rate sheet",
            "Unassigned paperwork"
        ]
    );
}

#[test]
fn group_by_a_declared_field_sections_in_declared_order() {
    let mut h = fixture();
    only_the_fixture_rows(&mut h);
    h.type_text(":group counterparty");
    h.press(KeyCode::Enter);
    assert_eq!(
        screen_rows(&h),
        [
            "# GoSBA Loans",
            "GoSBA rate sheet",
            "# Nick",
            "Nick payoff letter",
            "# no counterparty",
            "Unassigned paperwork",
        ]
    );
}

/// `groupable: false` on the declaration keeps a field out of `o` and `:group`,
/// exactly as a non-groupable built-in column is kept out.
#[test]
fn a_field_declared_ungroupable_is_refused_as_a_section_level() {
    let mut h = fixture();
    h.type_text(":group note");
    h.press(KeyCode::Enter);
    assert!(
        h.app.status.starts_with("outline by one of"),
        "{}",
        h.app.status
    );
    assert!(
        h.app.status.contains("counterparty"),
        "the groupable field is offered: {}",
        h.app.status
    );
    assert!(
        !h.app.status.contains("note"),
        "the ungroupable one is not: {}",
        h.app.status
    );
}

/// `p` paints a declared column by value like any other categorical column,
/// and the rule saves under the field's own name.
#[test]
fn paint_by_a_declared_column_names_the_field_in_the_rule() {
    let mut h = fixture();
    show_column(&mut h, "counterparty");
    h.press(KeyCode::Char('p'));
    pick_labelled(&mut h, "counterparty");
    h.press(KeyCode::Char('1'));
    let rules: Vec<String> = h
        .app
        .state
        .paint
        .iter()
        .map(|rule| rule.to_text(h.app.registry()))
        .collect();
    assert!(
        rules
            .iter()
            .any(|rule| rule.starts_with("by:field:counterparty=")
                && rule.contains("nick:")
                && rule.contains("gosbaloans:")),
        "one color per declared value: {rules:?}"
    );
}

#[test]
fn a_saved_view_naming_a_declared_column_round_trips_through_views_lua() {
    let mut h = fixture();
    show_column(&mut h, "counterparty");
    h.type_text(":group counterparty");
    h.press(KeyCode::Enter);
    filter_to(&mut h, "counterparty:nick");
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Char('d'));
    assert!(screen.contains("saved v1 for this repo"), "{screen}");

    // A declared field is written under `field:` so a later load can tell sbt's
    // own column from one only a newer build would know — see
    // `ColumnRegistry::unknown_field_name`.
    let file = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(file.contains("field:counterparty"), "{file}");
    assert!(file.contains("group = \"field:counterparty\""), "{file}");

    let fresh = open_app(&h.root, &h.config_path);
    assert_eq!(fresh.state.filter, "counterparty:nick");
    assert!(
        fresh
            .state
            .columns
            .iter()
            .any(|column| column.name(fresh.registry()) == "counterparty"),
        "the column came back"
    );
    assert_eq!(
        fresh.state.group.text(fresh.registry()),
        "field:counterparty"
    );
    assert_eq!(fresh.state.group.name(fresh.registry()), "counterparty");
}

/// The degradation contract: a view naming a field the repo has since stopped
/// declaring loads with that column, term, and section level dropped, says so,
/// and leaves the file writable rather than demanding a repair.
#[test]
fn a_view_naming_an_undeclared_field_loads_with_a_note_instead_of_refusing() {
    let mut h = fixture();
    std::fs::write(
        h.root.join("views-repo.lua"),
        "return { [1] = { filter = \"counterparty:nick status:todo\", \
         columns = \"id,field:counterparty,title\", group = \"field:counterparty\" } }",
    )
    .unwrap();
    switchbard_core::remove_field_decl(&h.root, "counterparty").unwrap();

    h.app = open_app(&h.root, &h.config_path);
    let screen = h.render();
    assert!(
        h.app.status.contains("no longer declares counterparty"),
        "the status line says what was dropped: {}",
        h.app.status
    );
    assert_eq!(h.app.state.filter, "status:todo", "the term is gone");
    assert!(
        !h.app
            .state
            .columns
            .iter()
            .any(|column| column.name(h.app.registry()) == "counterparty"),
        "the column is gone"
    );
    assert!(h.app.state.group.is_flat(), "the section level is gone");
    assert!(!screen.is_empty());

    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Char('d'));
    assert!(
        screen.contains("saved v1 for this repo"),
        "a degraded view still saves: {screen}"
    );
}

/// A name that is not even shaped like a field declaration is still an outright
/// error: the file is broken, not merely out of date.
#[test]
fn a_view_naming_a_malformed_column_still_blocks_the_file() {
    let mut h = fixture();
    let source = "return { [1] = { columns = \"id,Bogus Name,title\" } }";
    std::fs::write(h.root.join("views-repo.lua"), source).unwrap();
    h.app = open_app(&h.root, &h.config_path);
    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    let screen = h.press(KeyCode::Char('d'));
    assert!(screen.contains("repair file and reopen"), "{screen}");
    assert_eq!(
        std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap(),
        source
    );
}

/// Declaring a field while sbt is open adds the column on the next reload,
/// without restarting and without renumbering anything already on screen.
#[test]
fn a_field_declared_while_running_joins_the_catalog_on_the_next_reload() {
    let mut h = fixture();
    assert!(h.app.registry().parse("stage").is_none());
    declare_field(
        &h.root,
        "stage",
        switchbard_core::FieldKind::Enum,
        &["Draft", "Signed"],
        true,
    );
    let counterparty = h
        .app
        .registry()
        .parse("counterparty")
        .expect("declared before the reload");
    h.press(KeyCode::Char('r'));
    h.press(KeyCode::Esc);
    assert!(h.app.registry().parse("stage").is_some(), "the new field");
    assert_eq!(
        h.app.registry().parse("counterparty"),
        Some(counterparty),
        "an existing field keeps the identity already handed out"
    );
}

/// Two repos declaring different fields see their own; nothing leaks between
/// them, because the registry is built per repo rather than globally.
#[test]
fn each_repo_sees_only_its_own_declared_fields() {
    let h = fixture();
    let other = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(other.path().join("backlog/tasks")).unwrap();
    std::fs::write(
        other.path().join("backlog/config.yml"),
        "project_name: other\nstatuses: [\"To Do\", \"Done\"]\ntask_prefix: task\n",
    )
    .unwrap();
    declare_field(
        other.path(),
        "stage",
        switchbard_core::FieldKind::Enum,
        &["Draft"],
        true,
    );
    let other_app = switchbard_tui::app::App::open(
        other.path(),
        switchbard_tui::app::AppPaths::default(),
        Telemetry::in_memory(),
    );
    assert!(other_app.registry().parse("stage").is_some());
    assert!(other_app.registry().parse("counterparty").is_none());
    assert!(h.app.registry().parse("stage").is_none());
    assert!(h.app.registry().parse("counterparty").is_some());
}

/// TASK-209.6: `MAX_DEPTH` is 4, and a declared field nests alongside two
/// built-ins as freely as any of them, three levels deep, and a saved view
/// keeps it.
#[test]
fn three_levels_nest_a_declared_field_between_two_built_ins_and_a_saved_view_keeps_it() {
    let mut h = fixture();
    seed_project_task(
        &h.root,
        "Ally payoff",
        "Ally",
        "ball:agent",
        &[("counterparty", "GoSBA Loans")],
    );
    seed_project_task(
        &h.root,
        "Chase intake",
        "Chase",
        "ball:me",
        &[("counterparty", "Nick")],
    );
    h.app = open_app(&h.root, &h.config_path);
    h.render();
    only_the_fixture_rows(&mut h);

    h.type_text(":group project,counterparty,ball");
    h.press(KeyCode::Enter);
    assert_eq!(
        screen_rows(&h),
        [
            "# Ally · no def · 0/1",
            "  # GoSBA Loans",
            "    # agent",
            "Ally payoff",
            "# Chase · no def · 0/1",
            "  # Nick",
            "    # me",
            "Chase intake",
            "# no project",
            "  # GoSBA Loans",
            "    # no ball",
            "GoSBA rate sheet",
            "  # Nick",
            "    # no ball",
            "Nick payoff letter",
            "  # no counterparty",
            "    # no ball",
            "Unassigned paperwork",
        ]
    );

    h.press(KeyCode::Char('v'));
    h.press(KeyCode::Char('s'));
    h.press(KeyCode::Char('2'));
    let reopened = open_app(&h.root, &h.config_path);
    let saved = reopened.views.get(1).unwrap();
    assert_eq!(
        saved.group.text(reopened.registry()),
        "project,field:counterparty,ball"
    );
    let mut h2 = Harness {
        _dir: h._dir,
        root: h.root,
        config_path: h.config_path,
        app: reopened,
        terminal: h.terminal,
    };
    h2.press(KeyCode::Char('v'));
    h2.press(KeyCode::Char('2'));
    assert_eq!(
        screen_rows(&h2)[0],
        "# Ally · no def · 0/1",
        "the three levels round-trip"
    );
}
