//! Saved list scope through actual keys, temporary Lua files and rendered output.
mod harness;
use crossterm::event::KeyCode;
use harness::*;

#[test]
fn legacy_slots_and_both_feature_records_survive_save_and_restart() {
    let mut h = Harness::new();
    std::fs::write(h.root.join("views-repo.lua"), r#"return { [1] = { filter="status:todo", sort="pri:descending", columns="id,status,pri,title", glyphs="status", paint="column:id=cyan", group="status" } }"#).unwrap();
    std::fs::write(h.root.join("views-repo.prs.lua"), r#"return { [1] = { filter="status:merged", sort="status:ascending", columns="id,status,title", glyphs="status", paint="column:status=green", pin=false, abbreviated="" } }"#).unwrap();
    h.app = open_app(&h.root, &h.config_path);
    let task = h.app.state.clone();
    assert!(h.render().contains("status:todo"));
    let screen = h.type_text("vsd");
    assert!(screen.contains("saved v1"), "{screen}");
    h.press(KeyCode::Tab);
    let pr = h.app.state.clone();
    let screen = h.press(KeyCode::Char('v'));
    assert!(screen.contains("status:merged"), "{screen}");
    h.press(KeyCode::Esc);
    assert!(h.type_text("vsd").contains("saved v1"));
    let text = std::fs::read_to_string(h.root.join("views-repo.prs.lua")).unwrap();
    assert!(text.contains("columns = \"id,lifecycle,title\""), "{text}");
    assert!(text.contains("column:lifecycle=green"), "{text}");
    let resume = h.app.resume_state();
    h.app = open_app(&h.root, &h.config_path);
    assert_eq!(h.app.state, task);
    h.press(KeyCode::Tab);
    assert_eq!(h.app.state, pr);
    h.app.resume_from(Some(&resume));
    assert_eq!(h.app.state, pr);
    h.press(KeyCode::Tab);
    assert_eq!(h.app.state, task);
}

#[test]
fn malformed_and_future_records_are_preserved_on_save_and_promotion() {
    for source in [
        "return { broken",
        "return { [1] = { columns='id,future,title' } }",
        "return { [1] = { future='value' } }",
        "return { [1] = { sort='id:future' } }",
    ] {
        let mut h = Harness::new();
        let path = h.root.join("views-repo.lua");
        std::fs::write(&path, source).unwrap();
        h.app = open_app(&h.root, &h.config_path);
        let screen = h.type_text("vsd");
        assert!(screen.contains("repair file and reopen"), "{screen}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        let screen = h.type_text("vgd");
        assert!(screen.contains("repair file and reopen"), "{screen}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        assert!(!h.root.join("views.lua").exists());
    }
}

#[test]
fn external_edit_blocks_save_and_reopen_recovers_without_changing_other_page() {
    let mut h = Harness::new();
    let external = "return { [1] = { filter='label:auth' } }";
    let path = h.root.join("views-repo.lua");
    std::fs::write(&path, external).unwrap();
    let screen = h.type_text("vsd");
    assert!(screen.contains("changed on disk"), "{screen}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), external);
    h.press(KeyCode::Tab);
    assert!(h.type_text("vsd").contains("saved v1"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), external);
    h.app = open_app(&h.root, &h.config_path);
    assert!(h.render().contains("label:auth"));
    assert!(h.type_text("vsd").contains("saved v1"));
}

#[test]
fn unsupported_feature_columns_and_grouping_do_not_get_overwritten() {
    for settings in [
        "columns='id,priority,title'",
        "columns='id,title',group='status'",
    ] {
        let mut h = Harness::new();
        let source = format!("return {{ [1] = {{ {settings} }} }}");
        let path = h.root.join("views-repo.prs.lua");
        std::fs::write(&path, &source).unwrap();
        h.app = open_app(&h.root, &h.config_path);
        h.press(KeyCode::Tab);
        let screen = h.type_text("vsd");
        assert!(screen.contains("repair file and reopen"), "{screen}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    }
}

#[test]
fn unsupported_saved_filter_fields_are_preserved_and_block_save() {
    for (file, filter) in [
        ("views-repo.lua", "merged:today"),
        ("views-repo.prs.lua", "goal:foo"),
    ] {
        let mut h = Harness::new();
        let path = h.root.join(file);
        let source = format!("return {{ [1] = {{ filter='{filter}' }} }}");
        std::fs::write(&path, &source).unwrap();
        h.app = open_app(&h.root, &h.config_path);
        if file.ends_with(".prs.lua") {
            h.press(KeyCode::Tab);
        }
        let screen = h.type_text("vsd");
        assert!(screen.contains("repair file and reopen"), "{screen}");
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    }
}

#[test]
fn supported_saved_filter_aliases_remain_compatible_per_page() {
    let mut h = Harness::new();
    let task_path = h.root.join("views-repo.lua");
    let task_source = "return { [1] = { filter='created:today' } }";
    std::fs::write(&task_path, task_source).unwrap();
    let pr_path = h.root.join("views-repo.prs.lua");
    let pr_source = "return { [1] = { filter='status:merged' } }";
    std::fs::write(&pr_path, pr_source).unwrap();
    h.app = open_app(&h.root, &h.config_path);
    assert_eq!(h.app.state.filter, "created:today");
    h.press(KeyCode::Tab);
    assert_eq!(h.app.pull_requests.filter, "status:merged");
}

#[test]
fn malformed_global_file_allows_repo_save_but_cannot_be_promoted_over() {
    let mut h = Harness::new();
    let path = h.root.join("views.lua");
    std::fs::write(&path, "return { future = 1 }").unwrap();
    h.app = open_app(&h.root, &h.config_path);
    assert!(h.type_text("vsd").contains("saved v1"));
    let saved = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    let screen = h.type_text("vgd");
    assert!(screen.contains("repair file and reopen"), "{screen}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "return { future = 1 }"
    );
    assert_eq!(
        std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap(),
        saved
    );
}

#[test]
fn partial_paint_decode_never_overwrites_original_rules() {
    for paint in [
        "column:id=cyan;column:future=green",
        "column:id=cyan;by:status=todo:green,done:invalidcolor",
        "column:id=cyan;column:title=invalidcolor",
    ] {
        let mut h = Harness::new();
        let source = format!("return {{ [1] = {{ paint='{paint}' }} }}");
        let path = h.root.join("views-repo.lua");
        std::fs::write(&path, &source).unwrap();
        h.app = open_app(&h.root, &h.config_path);
        let screen = h.type_text("vsd");
        assert!(screen.contains("repair file and reopen"), "{screen}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    }
}

#[test]
fn sparse_global_slots_are_preserved_instead_of_truncated_on_promotion() {
    let mut h = Harness::new();
    let source =
        "return { {filter='label:auth'}, nil, {filter='label:ui'}, {filter='label:docs'} }";
    let path = h.root.join("views.lua");
    std::fs::write(&path, source).unwrap();
    h.app = open_app(&h.root, &h.config_path);
    let screen = h.type_text("vgd");
    assert!(screen.contains("repair file and reopen"), "{screen}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
}

#[test]
fn promotion_reports_confirmed_global_write_and_retries_repo_removal() {
    let mut h = Harness::new();
    h.type_text("/label:auth");
    h.press(KeyCode::Enter);
    assert!(h.type_text("vsd").contains("saved v1"));
    let repo = h.root.join("views-repo.lua");
    let original = std::fs::read_to_string(&repo).unwrap();
    let temporary = h.root.join("views-repo.lua.tmp");
    std::fs::create_dir(&temporary).unwrap();
    let screen = h.type_text("vgd");
    assert!(
        screen.contains("global saved; repo override retained"),
        "{screen}"
    );
    assert!(std::fs::read_to_string(h.root.join("views.lua"))
        .unwrap()
        .contains("label:auth"));
    assert_eq!(std::fs::read_to_string(&repo).unwrap(), original);
    std::fs::remove_dir(&temporary).unwrap();
    let screen = h.type_text("vgd");
    assert!(screen.contains("slot 1 is now global"), "{screen}");
    assert!(!std::fs::read_to_string(&repo)
        .unwrap()
        .contains("label:auth"));
}

#[test]
fn sparse_repo_slot_keeps_nine_in_picker_help_load_save_and_restart() {
    let mut h = Harness::new();
    std::fs::write(
        h.root.join("views-repo.lua"),
        "return { [9]={filter='label:auth'} }",
    )
    .unwrap();
    h.app = open_app(&h.root, &h.config_path);
    let screen = h.type_text("v9");
    assert!(screen.contains("v9 · label:auth"), "{screen}");
    let screen = h.press(KeyCode::Char('?'));
    assert!(
        screen.contains("v9") && screen.contains("label:auth [repo]"),
        "{screen}"
    );
    h.press(KeyCode::Char('?'));
    let screen = h.type_text("vs9");
    assert!(screen.contains("saved v9"), "{screen}");
    let source = std::fs::read_to_string(h.root.join("views-repo.lua")).unwrap();
    assert!(source.contains("[9]"), "{source}");
    assert!(!source.contains("[6]"), "{source}");
    h.app = open_app(&h.root, &h.config_path);
    assert!(h.type_text("v9").contains("v9 · label:auth"));
    let screen = h.type_text("v6");
    assert!(screen.contains("no view in slot 6"), "{screen}");
}

#[test]
fn partial_promotion_retry_never_overwrites_later_external_global_edit() {
    let mut h = Harness::new();
    assert!(h.type_text("vsd").contains("saved v1"));
    let temporary = h.root.join("views-repo.lua.tmp");
    std::fs::create_dir(&temporary).unwrap();
    assert!(h
        .type_text("vgd")
        .contains("global saved; repo override retained"));
    let global = h.root.join("views.lua");
    let external = "return { {filter='label:docs'} }";
    std::fs::write(&global, external).unwrap();
    std::fs::remove_dir(&temporary).unwrap();
    let screen = h.type_text("vgd");
    assert!(screen.contains("changed on disk"), "{screen}");
    assert_eq!(std::fs::read_to_string(&global).unwrap(), external);
    assert!(std::fs::read_to_string(h.root.join("views-repo.lua"))
        .unwrap()
        .contains("[1]"));
}
