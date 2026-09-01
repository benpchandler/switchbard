//! UI-level tests that drive the *real* Switchbard views through egui_kittest.
//!
//! These mount the whole window (`HiveApp::render_ui`) against a seeded,
//! thread-free app, then assert via the accesskit tree (`query_by_label`) and
//! via `harness.state()`. They run headless on CI — no GPU, no real
//! filesystem/process scanning — so they are deterministic and safe to gate
//! on. For pixel-level visual regression see `tests/ui_snapshot.rs`.

mod common;

use std::path::PathBuf;

use common::{app_with_items, harness, item, seeded_app, REPO_NAME, REPO_PATH};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use switchbard_core::config::Config;
use switchbard_core::{
    AgentHook, AgentKind, BacklogChecklistItem, BacklogRepo, BacklogTask, BacklogTaskSource,
    ContextKind, ContextScope, Repo, RepoRanking, WorktreeRef, DISPATCHED_LABEL, DISPATCHING_LABEL,
    DISPATCH_FAILED_LABEL, DISPATCH_LABEL,
};
use switchbard_gui::app::HiveApp;
use switchbard_gui::runtime::{AgentsSection, BacklogLens, Place, TasksView};
use switchbard_gui::ui::places::tasks::state::TasksViewMode;

fn seeded_backlog_task() -> BacklogTask {
    BacklogTask {
        id: "TASK-1".to_string(),
        title: "Seeded Backlog Task".to_string(),
        status: "To Do".to_string(),
        priority: "high".to_string(),
        assignees: vec!["ben".to_string()],
        labels: vec!["demo".to_string()],
        dependencies: vec![],
        references: vec![],
        project: None,
        parent: None,
        created_date: Some("2026-06-20 12:00".to_string()),
        updated_date: Some("2026-06-20 12:00".to_string()),
        description: "Task detail body".to_string(),
        implementation_plan: String::new(),
        implementation_notes: "Existing note".to_string(),
        final_summary: String::new(),
        acceptance_criteria: vec![BacklogChecklistItem {
            index: 1,
            checked: false,
            text: "Criterion renders".to_string(),
        }],
        definition_of_done: vec![],
        source: BacklogTaskSource::Active,
        path: PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-1.md")),
    }
}

/// Board lens (task-15 AC #1): switching to it should replace the list/detail
/// split with per-status columns, each showing the tasks currently in that
/// status as flight-strip cards.
#[test]
fn board_lens_renders_kanban_columns_with_the_seeded_task() {
    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.backlog_view.lens = BacklogLens::Board;
    app.tasks_place.view_mode = TasksViewMode::Board;
    app.backlog_view.selected_repo = Some(PathBuf::from(REPO_PATH));
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![seeded_backlog_task()],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Board").is_some(),
        "the Board lens tab should render"
    );
    assert!(
        harness.query_all_by_label("To Do").next().is_some(),
        "the To Do column header should render"
    );
    assert!(
        harness.query_all_by_label("In Progress").next().is_some(),
        "the In Progress column header should render even though it's empty"
    );
    // Owner UX pass (2026-08-05, post-dates this test): with a single task,
    // it's auto-selected and the persistent detail rail now also renders
    // its title as a heading, so this label is no longer unique — `query_
    // all` instead of the exactly-one query.
    assert!(
        harness
            .query_all_by_label("Seeded Backlog Task")
            .next()
            .is_some(),
        "the seeded task's flight strip should render in its status column"
    );
}

#[test]
fn hooks_section_surfaces_disabled_state_instead_of_registrations() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Hooks;
    app.agent_contexts
        .lock()
        .expect("invariant: seeded context cache lock")
        .get_mut(&PathBuf::from(REPO_PATH))
        .expect("invariant: seeded repo context")
        .hooks_disabled_by = Some(PathBuf::from(format!(
        "{REPO_PATH}/.claude/settings.local.json"
    )));
    let mut harness = harness(app);
    harness.set_size(egui::vec2(900.0, 620.0));
    harness.run();

    assert!(harness
        .query_by_label("Hooks are disabled for this worktree")
        .is_some());
}

/// Global search overlay (task-15 AC #2): opening it and matching a query
/// should surface results across every tracked repo, prefixed with the
/// repo id the same way the All-repos list rows are.
#[test]
fn global_search_overlay_finds_the_matching_task_across_repos() {
    let mut app = seeded_app();
    app.place = Place::Tasks;
    // The List lens's own row renders the same "repo:id  title" label the
    // search result does (see the assertion below); the Digest lens
    // (task-21's default) doesn't render that format at all.
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.search.open = true;
    app.backlog_view.search.query = "Seeded".to_string();
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![seeded_backlog_task()],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Search all repos").is_some(),
        "the search window should be open"
    );
    // TASK-97: the search overlay's own result row still spells "repo:id
    // title" (`search.rs`, unchanged) — but the underlying Tasks place's
    // list row no longer repo-prefixes its title (`list::
    // render_task_list_row`'s own note; the repo identifies itself via a
    // separate badge column instead), so the two are now distinct labels,
    // not the same one appearing twice.
    assert_eq!(
        harness
            .query_all_by_label(&format!("{REPO_NAME}:TASK-1  Seeded Backlog Task"))
            .count(),
        1,
        "the matching task should appear as a repo-prefixed search result"
    );
    assert!(
        harness
            .query_by_label("TASK-1  Seeded Backlog Task")
            .is_some(),
        "the underlying list row should also render, unprefixed"
    );

    harness.state_mut().backlog_view.search.query = "no-such-task-anywhere".to_string();
    harness.run();
    assert!(
        harness.query_by_label("No matches").is_some(),
        "a query with no hits should say so rather than showing stale results"
    );
}

#[test]
fn window_defaults_to_digest_place() {
    let harness = harness(seeded_app());

    assert_eq!(harness.state().place, Place::Digest);
    // All five places are always offered in the sidebar nav.
    assert!(
        harness.query_all_by_label("Digest").next().is_some(),
        "Digest place should be present (query_all: the active Digest place's own \
         heading shares the nav row's label)"
    );
    assert!(
        harness.query_by_label("Tasks").is_some(),
        "Tasks place should be present"
    );
    assert!(
        harness.query_by_label("Command").is_some(),
        "Command place should be present"
    );
    assert!(
        harness.query_by_label("Goals").is_some(),
        "Goals place should be present"
    );
    assert!(
        harness.query_by_label("Ops").is_some(),
        "Ops place should be present"
    );
}

#[test]
fn clicking_command_place_switches_view() {
    let mut harness = harness(seeded_app());

    // In the default Digest place the only "Command" widget is the nav row,
    // so this is unambiguous.
    harness.get_by_label("Command").click();
    harness.run();

    assert_eq!(harness.state().place, Place::Command);
    assert!(harness.query_all_by_label("Context").next().is_some());
    assert!(harness.query_by_label("Hooks").is_some());
}

#[test]
fn hooks_section_surfaces_configured_repo_hook() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Hooks;
    app.agent_contexts
        .lock()
        .expect("invariant: seeded context cache lock")
        .get_mut(&PathBuf::from(REPO_PATH))
        .expect("invariant: seeded repo context")
        .hooks
        .push(AgentHook {
            id: "repo-stop-hook".to_string(),
            agent: AgentKind::Claude,
            scope: ContextScope::Local,
            source_path: PathBuf::from(format!("{REPO_PATH}/.claude/settings.local.json")),
            event: "Stop".to_string(),
            matcher: Some("format|check".to_string()),
            hook_type: "command".to_string(),
            action:
                "./scripts/rebuild-and-reload.sh --verify-this-very-long-command-without-overflow"
                    .to_string(),
            arguments: vec!["--from-test".to_string()],
            condition: None,
            asynchronous: false,
            timeout_seconds: Some(30),
        });
    let mut harness = harness(app);
    harness.set_size(egui::vec2(900.0, 620.0));
    harness.run();

    assert!(harness.query_all_by_label("Hooks").next().is_some());
    assert!(harness.query_all_by_label("1 configured").next().is_some());
    assert!(harness.query_by_label("Stop").is_some());
    assert!(harness
        .query_by_label("Rebuilds and reloads the app")
        .is_some());
    assert!(harness
        .query_by_label("After Claude finishes responding")
        .is_some());
    assert!(harness
        .query_by_label("Claude ignores matchers for Stop")
        .is_some());
    assert!(harness.query_by_label("format|check").is_some());
    assert!(harness
        .query_by_label(
            "./scripts/rebuild-and-reload.sh --verify-this-very-long-command-without-overflow"
        )
        .is_some());
}

#[test]
fn hooks_section_explains_empty_registration_state() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Hooks;
    let mut harness = harness(app);
    harness.run();

    assert!(harness
        .query_by_label("No configured hooks detected for Claude in this worktree.")
        .is_some());
}

#[test]
fn context_search_filters_individual_assets_not_just_repo_cards() {
    let mut app = app_with_items(vec![
        item(
            "alpha-doc",
            AgentKind::Claude,
            ContextScope::Local,
            ContextKind::Doc,
            "alpha-notes.md",
        ),
        item(
            "beta-doc",
            AgentKind::Claude,
            ContextScope::Local,
            ContextKind::Doc,
            "beta-notes.md",
        ),
    ]);
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Context;
    *app.filter_mut() = "alpha".to_string();
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("alpha-notes.md").is_some());
    assert!(harness.query_by_label("beta-notes.md").is_none());
    assert!(harness.query_by_label("1 active").is_some());
}

#[test]
fn agents_queries_are_scoped_per_section_and_restore_on_return() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Context;
    *app.filter_mut() = "context-query".to_string();
    assert_eq!(app.filter(), "context-query");

    app.agent_context_view.section = AgentsSection::Hooks;
    assert_eq!(app.filter(), "");
    *app.filter_mut() = "hooks-query".to_string();

    app.agent_context_view.section = AgentsSection::Context;
    assert_eq!(app.filter(), "context-query");
    app.agent_context_view.section = AgentsSection::Hooks;
    assert_eq!(app.filter(), "hooks-query");
}

/// Regression for the Servers page's top-bar Clear: it must treat the
/// shipped `UiConfig` default (`show_non_servers: false`) as "nothing
/// filtered", not as an active filter to reset away from. Two widgets share
/// the "Clear filters" label on this page - this one in the top bar (painted
/// first) and the staleness bar's own narrower Clear (painted second, inside
/// the central panel) - so `get_all_by_label` plus paint order picks the
/// page-wide one, matching the pattern `clicking_command_place_switches_view`
/// already documents for disambiguating a shared label.
#[test]
fn servers_clear_filters_matches_the_shipped_defaults() {
    let mut app = seeded_app();
    app.place = Place::Ops;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness
            .get_all_by_label("Clear filters")
            .next()
            .unwrap()
            .accesskit_node()
            .is_disabled(),
        "fresh install has nothing narrowed from the shipped defaults, so Clear should start disabled"
    );

    harness.state_mut().show_non_servers = true;
    harness.run();
    assert!(!harness
        .get_all_by_label("Clear filters")
        .next()
        .unwrap()
        .accesskit_node()
        .is_disabled());

    harness
        .get_all_by_label("Clear filters")
        .next()
        .unwrap()
        .click();
    harness.run();

    assert!(!harness.state().show_non_servers);
    assert!(harness
        .get_all_by_label("Clear filters")
        .next()
        .unwrap()
        .accesskit_node()
        .is_disabled());
}

#[test]
fn context_clear_restores_the_persistable_filter_defaults() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Context;
    app.agent_context_view.scope = ContextScope::Directory;
    app.agent_context_view.kind = Some(ContextKind::Skill);
    *app.filter_mut() = "review".to_string();
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("3 active").is_some());
    harness.get_by_label("Clear filters").click();
    harness.run();

    assert_eq!(harness.state().filter(), "");
    assert_eq!(
        harness.state().agent_context_view.scope,
        ContextScope::Local
    );
    assert_eq!(harness.state().agent_context_view.kind, None);
}

#[test]
fn restart_drops_a_persisted_backlog_project_that_is_no_longer_tracked() {
    let mut cfg = Config::default();
    cfg.ui
        .filters
        .entry("backlog".to_string())
        .or_default()
        .facets
        .insert("repo".to_string(), "/tmp/removed-repo".to_string());
    let app = HiveApp::new_headless(
        cfg,
        vec![Repo {
            name: REPO_NAME.to_string(),
            path: PathBuf::from(REPO_PATH),
        }],
        Vec::new(),
    );

    assert_eq!(app.backlog_view.selected_repo, None);
}

#[test]
fn hooks_facets_compose_and_clear_from_the_shared_filter_bar() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Hooks;
    app.agent_context_view.hook_event = Some("PostToolUse".to_string());
    app.agent_context_view.hook_type = Some("command".to_string());
    app.agent_contexts
        .lock()
        .expect("invariant: seeded context cache lock")
        .get_mut(&PathBuf::from(REPO_PATH))
        .expect("invariant: seeded repo context")
        .hooks = vec![
        AgentHook {
            id: "post-command".to_string(),
            agent: AgentKind::Claude,
            scope: ContextScope::Local,
            source_path: PathBuf::from(format!("{REPO_PATH}/.claude/settings.json")),
            event: "PostToolUse".to_string(),
            matcher: Some("Edit".to_string()),
            hook_type: "command".to_string(),
            action: "./scripts/after-edit.sh".to_string(),
            arguments: Vec::new(),
            condition: None,
            asynchronous: false,
            timeout_seconds: None,
        },
        AgentHook {
            id: "stop-prompt".to_string(),
            agent: AgentKind::Claude,
            scope: ContextScope::Global,
            source_path: PathBuf::from("/Users/demo/.claude/settings.json"),
            event: "Stop".to_string(),
            matcher: None,
            hook_type: "prompt".to_string(),
            action: "Review the response".to_string(),
            arguments: Vec::new(),
            condition: None,
            asynchronous: false,
            timeout_seconds: None,
        },
    ];
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label("./scripts/after-edit.sh").is_some());
    assert!(harness.query_by_label("Review the response").is_none());
    assert!(harness.query_by_label("2 active").is_some());

    harness.get_by_label("Clear filters").click();
    harness.run();

    assert_eq!(harness.state().agent_context_view.hook_event, None);
    assert_eq!(harness.state().agent_context_view.hook_type, None);
    assert!(harness.query_by_label("Review the response").is_some());
}

#[test]
fn hooks_page_explains_itself_when_facets_hide_every_repo() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Hooks;
    app.agent_context_view.hook_scope = Some(ContextScope::Directory);
    app.agent_contexts
        .lock()
        .expect("invariant: seeded context cache lock")
        .get_mut(&PathBuf::from(REPO_PATH))
        .expect("invariant: seeded repo context")
        .hooks
        .push(AgentHook {
            id: "local-stop".to_string(),
            agent: AgentKind::Claude,
            scope: ContextScope::Local,
            source_path: PathBuf::from(format!("{REPO_PATH}/.claude/settings.json")),
            event: "Stop".to_string(),
            matcher: None,
            hook_type: "command".to_string(),
            action: "./scripts/notify.sh".to_string(),
            arguments: Vec::new(),
            condition: None,
            asynchronous: false,
            timeout_seconds: None,
        });
    let mut harness = harness(app);
    harness.run();

    assert!(harness.query_by_label(REPO_NAME).is_none());
    assert!(harness
        .query_by_label("No hooks match the current filters.")
        .is_some());
}

#[test]
fn clicking_tasks_place_switches_view() {
    let mut harness = harness(seeded_app());

    harness.get_by_label("Tasks").click();
    harness.run();

    assert_eq!(harness.state().place, Place::Tasks);
    assert_eq!(harness.state().tasks_view, TasksView::All);
}

#[test]
fn agent_context_view_surfaces_seeded_assets() {
    let mut app = seeded_app();
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Context;
    let mut harness = harness(app);
    harness.run();

    // Summary counts the two seeded assets, the repo heading renders, and the
    // seeded CLAUDE.md item shows in the explorer under the default selection.
    // The "N assets" count renders in both the page summary and the repo card,
    // and CLAUDE.md appears in both the item row and the effective-instruction
    // stack, so use the duplicate-tolerant `query_all_*` variants.
    assert!(
        harness.query_all_by_label("2 assets").next().is_some(),
        "summary should report the two seeded assets"
    );
    assert!(
        harness.query_all_by_label(REPO_NAME).next().is_some(),
        "repo heading should render"
    );
    assert!(
        harness.query_all_by_label("CLAUDE.md").next().is_some(),
        "seeded CLAUDE.md item should render in the explorer"
    );
}

#[test]
fn agent_context_estimate_uses_effective_instructions_not_all_assets() {
    let mut instruction = item(
        "claude-md",
        AgentKind::Claude,
        ContextScope::Local,
        ContextKind::Instruction,
        "CLAUDE.md",
    );
    instruction.size_bytes = 1_000;
    let mut skill = item(
        "large-skill",
        AgentKind::Claude,
        ContextScope::Local,
        ContextKind::Skill,
        "large-skill/SKILL.md",
    );
    skill.size_bytes = 8_000;
    let mut nested_instruction = item(
        "nested-claude-md",
        AgentKind::Claude,
        ContextScope::Directory,
        ContextKind::Instruction,
        "apps/web/CLAUDE.md",
    );
    nested_instruction.size_bytes = 4_000;
    nested_instruction.applies_to = Some(PathBuf::from(format!("{REPO_PATH}/apps/web")));

    let mut app = app_with_items(vec![instruction, skill, nested_instruction]);
    app.place = Place::Command;
    app.agent_context_view.section = AgentsSection::Context;
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness
            .query_all_by_label("startup context ~1.0k chars / ~250 tokens")
            .next()
            .is_some(),
        "startup estimate should include only instructions effective at the selected worktree root"
    );
    assert!(
        harness
            .query_all_by_label("startup context ~13.0k chars / ~3.3k tokens")
            .next()
            .is_none(),
        "startup estimate must not sum every context asset in the repo"
    );
}

#[test]
fn backlog_view_surfaces_seeded_task() {
    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![seeded_backlog_task()],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    app.backlog_view
        .bulk_selected_tasks
        .insert((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
    app.backlog_view.selected_repo = Some(PathBuf::from(REPO_PATH));
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness
            .query_by_label("TASK-1  Seeded Backlog Task")
            .is_some(),
        "task row should render"
    );
    assert!(
        harness.query_by_label("Sort").is_some(),
        "task sort controls should render"
    );
    assert!(
        harness.query_by_label("Ascending").is_some(),
        "sort direction control should render"
    );
    assert!(
        harness.query_by_label("1 selected").is_some(),
        "bulk-selection count should render"
    );
    assert!(
        harness.query_by_label("#1 Criterion renders").is_some(),
        "acceptance criterion should render in the detail pane"
    );
}

/// One task from each of two tracked repos, both numbered "TASK-1" — the
/// unified All-repos scope must merge them without id collisions, prefix
/// each row's id with its repo, and render a repo badge per row.
#[test]
fn backlog_all_projects_scope_merges_repos_with_a_repo_badge() {
    let repo_path = |name: &str| PathBuf::from(format!("/tmp/switchbard-ui-test/{name}"));
    let repos = vec![
        Repo {
            name: "alpha".to_string(),
            path: repo_path("alpha"),
        },
        Repo {
            name: "beta".to_string(),
            path: repo_path("beta"),
        },
    ];
    let worktrees = repos
        .iter()
        .map(|repo| WorktreeRef {
            repo_name: repo.name.clone(),
            path: repo.path.clone(),
            branch: Some("main".to_string()),
            head: "abc1234".to_string(),
        })
        .collect();
    let mut cfg = Config::default();
    cfg.ui.onboarding_dismissed = true;
    let mut app = HiveApp::new_headless(cfg, repos, worktrees);
    // See `common::app_with_items` doc comment: every test-constructed
    // `HiveApp` must redirect `save_config` away from the real
    // `~/.switchbard/config.toml` (TASK-22).
    app.config_save_path = Some(common::isolated_config_save_path());
    app.place = Place::Tasks;
    app.backlog_view.lens = BacklogLens::List;

    for (repo_name, title) in [("alpha", "Alpha task"), ("beta", "Beta task")] {
        app.backlog_repos.lock().unwrap().insert(
            repo_path(repo_name),
            BacklogRepo {
                root: repo_path(repo_name),
                tasks: vec![BacklogTask {
                    id: "TASK-1".to_string(),
                    title: title.to_string(),
                    status: "To Do".to_string(),
                    priority: "medium".to_string(),
                    assignees: vec![],
                    labels: vec![],
                    dependencies: vec![],
                    references: vec![],
                    project: None,
                    parent: None,
                    created_date: None,
                    updated_date: None,
                    description: String::new(),
                    implementation_plan: String::new(),
                    implementation_notes: String::new(),
                    final_summary: String::new(),
                    acceptance_criteria: vec![],
                    definition_of_done: vec![],
                    source: BacklogTaskSource::Active,
                    path: repo_path(repo_name).join("backlog/tasks/task-1.md"),
                }],
                warnings: vec![],
                project_defs: vec![],
                initiative_defs: vec![],
                goals: vec![],
                ranking: RepoRanking::default(),
                loaded_at_unix: 0,
                configured_statuses: vec![
                    "Icebox".into(),
                    "To Do".into(),
                    "In Progress".into(),
                    "In Review".into(),
                    "Done".into(),
                ],
            },
        );
    }

    let mut harness = harness(app);
    harness.run();

    assert_eq!(
        harness.state().backlog_view.selected_repo,
        None,
        "the Backlog view defaults to the All-repos scope"
    );
    // TASK-97: the row title never repo-prefixes (`list::
    // render_task_list_row`'s own note) — a same-numbered task from a
    // different repo is disambiguated by its own title text plus the
    // separate repo-badge column asserted below, not a "repo:id" spelling.
    assert!(
        harness.query_by_label("TASK-1  Alpha task").is_some(),
        "row id renders in the All-repos scope"
    );
    assert!(
        harness.query_by_label("TASK-1  Beta task").is_some(),
        "a same-numbered task from a different repo renders as a distinct row"
    );
    assert!(
        harness.query_all_by_label("alpha").next().is_some(),
        "repo badge should render on the alpha row"
    );
    assert!(
        harness.query_all_by_label("beta").next().is_some(),
        "repo badge should render on the beta row"
    );
}

/// Digest lens (task-21): the Backlog tab's default landing screen should
/// surface an in-progress task under its "In progress" section.
#[test]
fn digest_lens_is_the_backlog_default_and_surfaces_in_progress_tasks() {
    let mut app = seeded_app();
    app.place = Place::Digest;
    let mut in_progress_task = seeded_backlog_task();
    in_progress_task.status = "In Progress".to_string();
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![in_progress_task],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    let mut harness = harness(app);
    harness.run();

    assert_eq!(
        harness.state().place,
        Place::Digest,
        "Digest is its own place now (TASK-96/97)"
    );
    assert!(
        harness.query_all_by_label("In progress").next().is_some(),
        "the In progress section header should render"
    );
    // Owner UX pass (2026-08-05, post-dates this test): with a single task,
    // it's auto-selected and the persistent detail rail now also renders
    // its title as a heading, so this label is no longer unique — `query_
    // all` instead of the exactly-one query.
    assert!(
        harness
            .query_all_by_label("Seeded Backlog Task")
            .next()
            .is_some(),
        "the in-progress task should render as a digest strip"
    );

    // Sections render Overdue, Newly unblocked, In progress, Recently done in
    // that order, each with its own "View all" button.
    harness
        .get_all_by_label("View all")
        .nth(2)
        .expect("the In progress section's View all button")
        .click();
    harness.run();
    // TASK-97: Digest and Tasks are separate places — "View all" now
    // navigates to the Tasks place directly (`ui::backlog::digest::
    // render_section`'s click handler), rather than flipping a `lens`
    // field that has no effect from `Place::Digest`.
    assert_eq!(
        harness.state().place,
        Place::Tasks,
        "View all on a digest section should navigate to the Tasks place"
    );
}

/// Digest goals section (TASK-74): current-week goals lead the Digest with
/// a pace pill and check-in affordance; with no goals the section is absent
/// entirely.
#[test]
fn digest_leads_with_current_week_goals_and_omits_the_section_without_any() {
    let today = chrono::Local::now().date_naive();
    let week = switchbard_core::week_monday_of(today)
        .format("%Y-%m-%d")
        .to_string();
    let mut weeks = std::collections::BTreeMap::new();
    weeks.insert(
        week.clone(),
        switchbard_core::GoalWeek {
            target: 1,
            checkins: vec![switchbard_core::GoalCheckIn {
                date: week.clone(),
                value: 1,
            }],
        },
    );

    let mut app = seeded_app();
    app.place = Place::Digest;
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![seeded_backlog_task()],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![switchbard_core::GoalDef {
                name: "Onboard users".to_string(),
                unit: "users".to_string(),
                measure: switchbard_core::GoalMeasure::Manual,
                scope: None,
                inputs: switchbard_core::GoalInputs::default(),
                weeks,
            }],
            ranking: switchbard_core::RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec!["To Do".into(), "Done".into()],
        },
    );
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness
            .query_all_by_label("This week's goals")
            .next()
            .is_some(),
        "the goals section leads the Digest"
    );
    assert!(
        harness.query_all_by_label("Onboard users").next().is_some(),
        "the goal card renders its name"
    );
    // target 1 with a check-in of 1 is met whatever day the test runs.
    assert!(
        harness.query_all_by_label("met").next().is_some(),
        "the deterministic verdict pill renders"
    );
    assert!(
        harness.query_all_by_label("Check in").next().is_some(),
        "manual goals carry the inline check-in affordance"
    );

    // And with no goals at all, no empty shell.
    let mut goalless = seeded_app();
    goalless.place = Place::Digest;
    let mut bare = common::harness(goalless);
    bare.run();
    assert!(
        bare.query_all_by_label("This week's goals")
            .next()
            .is_none(),
        "a glance surface earns no empty section"
    );
}

/// The "+ Goal" entry points (TASK-75): with no goals, the Digest offers a
/// doorway instead of an empty section; with goals, the section header
/// carries the button. Both open the New Goal modal.
#[test]
fn digest_offers_goal_creation_from_both_entry_points() {
    // Zero goals (but a tracked backlog): the doorway button, no shell.
    let mut app = seeded_app();
    app.place = Place::Digest;
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![seeded_backlog_task()],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec!["To Do".into(), "Done".into()],
        },
    );
    let mut harness = harness(app);
    harness.run();
    assert!(
        harness
            .query_all_by_label("This week's goals")
            .next()
            .is_none(),
        "no empty section"
    );
    harness
        .get_all_by_label("+ Goal for this week")
        .next()
        .expect("the zero-goals doorway renders")
        .click();
    harness.run();
    assert!(
        harness.state().backlog_view.new_goal.open,
        "the doorway opens the New Goal modal"
    );
    assert!(
        harness.query_all_by_label("New Goal").next().is_some(),
        "the modal window renders"
    );

    // With a current-week goal, the section header carries "+ Goal".
    let today = chrono::Local::now().date_naive();
    let week = switchbard_core::week_monday_of(today)
        .format("%Y-%m-%d")
        .to_string();
    let mut weeks = std::collections::BTreeMap::new();
    weeks.insert(
        week,
        switchbard_core::GoalWeek {
            target: 1,
            checkins: vec![],
        },
    );
    let mut app = seeded_app();
    app.place = Place::Digest;
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![seeded_backlog_task()],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![switchbard_core::GoalDef {
                name: "Onboard users".to_string(),
                unit: "users".to_string(),
                measure: switchbard_core::GoalMeasure::Manual,
                scope: None,
                inputs: switchbard_core::GoalInputs::default(),
                weeks,
            }],
            ranking: switchbard_core::RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec!["To Do".into(), "Done".into()],
        },
    );
    let mut with_goals = common::harness(app);
    with_goals.run();
    assert!(
        with_goals.query_all_by_label("+ Goal").next().is_some(),
        "the section header carries the + Goal button"
    );
}

// TASK-97 removed `portfolio_lens_renders_per_repo_health`: the Portfolio
// lens is cut from the Tasks place (not part of List/Board, no equivalent
// group-by/facet — see `ui::places::tasks`'s module doc) and has no home
// anywhere else either. Its code keeps compiling (`ui::backlog::render`
// stays `pub fn`) but nothing routes there anymore.

/// Dependency/blocked visibility (task-18): a task with an open dependency
/// should show a "blocked" marker in the List lens row and a per-dependency
/// status in the detail pane's Dependencies section; the dependency itself
/// should list this task under "Blocks".
#[test]
fn blocked_task_shows_a_marker_and_dependency_status_in_detail() {
    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_repo = Some(PathBuf::from(REPO_PATH));

    let mut blocker = seeded_backlog_task();
    blocker.id = "TASK-1".to_string();
    blocker.title = "Blocking task".to_string();
    blocker.status = "To Do".to_string();

    let mut dependent = seeded_backlog_task();
    dependent.id = "TASK-2".to_string();
    dependent.title = "Dependent task".to_string();
    dependent.status = "To Do".to_string();
    dependent.dependencies = vec!["TASK-1".to_string()];
    dependent.path = PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-2.md"));

    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![blocker, dependent],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), "TASK-2".to_string()));
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_all_by_label("blocked").next().is_some(),
        "the dependent task's row should show a blocked marker"
    );
    assert!(
        harness.query_by_label("TASK-1 Blocking task").is_some(),
        "the detail pane's Dependencies section should name the open dependency"
    );

    harness.state_mut().backlog_view.selected_task =
        Some((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
    harness.run();
    assert!(
        harness.query_by_label("TASK-2 Dependent task").is_some(),
        "the blocking task's detail pane should list what it Blocks"
    );
}

/// Sub-task hierarchy (task-17): a parent with children collapses to a
/// single row with a roll-up badge; expanding it reveals the children
/// nested underneath, and the parent's detail pane offers "+ Subtask".
#[test]
fn parent_task_shows_rollup_and_expands_to_reveal_children() {
    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_repo = Some(PathBuf::from(REPO_PATH));

    let mut parent = seeded_backlog_task();
    parent.id = "TASK-1".to_string();
    parent.title = "Parent task".to_string();

    let mut done_child = seeded_backlog_task();
    done_child.id = "TASK-1.1".to_string();
    done_child.title = "Done child".to_string();
    done_child.status = "Done".to_string();
    done_child.parent = Some("TASK-1".to_string());
    done_child.path = PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-1.1.md"));

    let mut open_child = seeded_backlog_task();
    open_child.id = "TASK-1.2".to_string();
    open_child.title = "Open child".to_string();
    open_child.parent = Some("TASK-1".to_string());
    open_child.path = PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-1.2.md"));

    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![parent, done_child, open_child],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), "TASK-1".to_string()));
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness
            .query_by_label("TASK-1  Parent task  [1/2]")
            .is_some(),
        "the parent row should show a 1/2 roll-up badge"
    );
    // TASK-97 (decision record Q9 = A): sub-issues indent in place, always
    // expanded — no per-parent collapse step, so both children are already
    // visible with no `expanded_parents` state needed at all.
    assert!(
        harness.query_by_label("TASK-1.2  Open child").is_some(),
        "children should already be visible — always expanded, no click needed"
    );
    assert!(
        harness.query_by_label("TASK-1.1  Done child").is_some(),
        "the done child should also already be visible"
    );
    assert!(
        harness.query_by_label("+ Subtask").is_some(),
        "the parent's detail pane should offer to create a subtask"
    );

    harness.get_by_label("+ Subtask").click();
    harness.run();
    assert_eq!(
        harness.state().backlog_view.new_task.parent.as_deref(),
        Some("TASK-1"),
        "+ Subtask should pre-fill the new-task modal's parent"
    );
    assert!(harness.state().backlog_view.new_task.open);
}

// TASK-97 removed `saved_view_can_be_saved_and_deleted` (it drove
// `ui::backlog::saved_views::render_saved_views_bar`, unreachable at the
// time). A TASK-97 medic pass (task-107's fix) restored it inside the Tasks
// place's own facets frame — see `tests/tasks_place_saved_views.rs`'s
// `deleting_the_active_saved_view_removes_it_and_clears_the_selection` for
// the equivalent coverage on the now-reachable surface, and
// `backlog_controls.rs`'s identical note for the full reasoning.

/// Like [`harness_on_task`], but backed by a *real* task file in its own
/// temp repo, for the two dispatch-toggle tests whose background thread
/// genuinely writes the label. Since the format fork's native writes, that
/// thread can finish before the test's first status assertion (there is no
/// subprocess spawn to lose the race to), so the toggle must actually
/// succeed against disk — an in-memory-only fixture would race a failure
/// message into the status line.
fn harness_on_disk_task(labels: &[&str]) -> (tempfile::TempDir, Harness<'static, HiveApp>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    let tasks_dir = root.join("backlog/tasks");
    std::fs::create_dir_all(&tasks_dir).expect("repo layout");
    switchbard_core::write_new_task_file(
        &tasks_dir,
        "TASK",
        "1",
        &switchbard_core::NewBacklogTask {
            title: "Seeded Backlog Task".to_string(),
            description: "Task detail body".to_string(),
            status: "To Do".to_string(),
            priority: "high".to_string(),
            acceptance_criteria: vec![],
            parent: None,
            labels: labels.iter().map(|l| l.to_string()).collect(),
            assignees: vec!["ben".to_string()],
            project: None,
            dependencies: vec![],
        },
    )
    .expect("seed task file");
    let repo = switchbard_core::load_backlog_repo(&root).expect("seeded repo loads");

    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_repo = Some(root.clone());
    app.backlog_view.selected_task = Some((root.clone(), "TASK-1".to_string()));
    app.backlog_repos.lock().unwrap().insert(root, repo);
    let mut harness = harness(app);
    harness.run();
    (dir, harness)
}

/// Bounded poll until the on-disk task's labels match `expected` — proves
/// the toggle's background thread completed before the tempdir drops.
fn wait_for_labels(root: &std::path::Path, expected: &[&str]) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let repo = switchbard_core::load_backlog_repo(root).expect("repo reloads");
        let task = repo.tasks.first().expect("task still present");
        if task.labels == expected {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "labels never became {expected:?}, still {:?}",
            task.labels
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Mount the List lens on one task's detail pane — the shape every dispatch
/// state test below shares, varying only the seeded task's labels/notes.
fn harness_on_task(task: BacklogTask) -> Harness<'static, HiveApp> {
    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_repo = Some(PathBuf::from(REPO_PATH));
    app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), task.id.clone()));
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![task],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    let mut harness = harness(app);
    harness.run();
    harness
}

/// Dispatch (task-11 GUI wiring): a task with no dispatch label offers the
/// opt-in "Dispatch" button and shows no state pill anywhere.
#[test]
fn not_flagged_task_offers_a_dispatch_button_and_no_pill() {
    let harness = harness_on_task(seeded_backlog_task());
    assert!(
        harness.query_by_label("Dispatch").is_some(),
        "an unflagged task should offer the Dispatch button"
    );
    for pill in ["QUEUED", "DISPATCHING", "DISPATCHED", "DISPATCH FAILED"] {
        assert!(
            harness.query_by_label(pill).is_none(),
            "no dispatch pill should render for an unflagged task, saw {pill}"
        );
    }
}

/// Clicking "Dispatch" asks for confirmation before touching the label —
/// flagging a task hands it to an autonomous run, so it gets the same
/// inline-confirm treatment as Archive. Confirming sets the synchronous
/// status message (the CLI call itself runs on a spawned thread against
/// this test's fixture path, same as the saved_views Save/Delete flow).
// These two use `click_accesskit()`: the rail's dispatch controls sit below the
// scroll fold at this window size, out of reach of egui_kittest 0.36's
// pointer-based `click()`. See the note above `detail_harness_on` in
// `backlog_controls.rs`.
#[test]
fn dispatch_button_confirms_before_flagging() {
    let (fixture, mut harness) = harness_on_disk_task(&["demo"]);

    harness.get_by_label("Dispatch").click_accesskit();
    harness.run();
    assert!(
        harness
            .query_by_label("Hand TASK-1 to a headless agent run in an isolated worktree?")
            .is_some(),
        "clicking Dispatch should show the confirm prompt"
    );

    harness.get_by_label("Cancel").click_accesskit();
    harness.run();
    assert!(
        harness.query_by_label("Dispatch").is_some(),
        "Cancel should revert to the plain Dispatch button"
    );
    assert!(!harness.state().backlog_view.dispatch_confirm);

    harness.get_by_label("Dispatch").click_accesskit();
    harness.run();
    harness.get_by_label("Confirm dispatch").click_accesskit();
    harness.run();
    // The synchronous click-handler status and the background thread's
    // success status are the same string, so this holds whichever wins the
    // (now native-write-fast) race.
    assert_eq!(
        harness.state().backlog_status.snapshot().as_deref(),
        Some("flagged TASK-1 for dispatch"),
        "confirming should set the flagged-for-dispatch status"
    );
    assert!(!harness.state().backlog_view.dispatch_confirm);
    wait_for_labels(fixture.path(), &["demo", "dispatch"]);
}

/// A task labeled `dispatch` (queued, not yet claimed) shows the QUEUED pill,
/// a waiting message, and an Unflag escape hatch instead of the Dispatch
/// button.
#[test]
fn queued_task_shows_pill_and_offers_unflag() {
    let (fixture, mut harness) = harness_on_disk_task(&[DISPATCH_LABEL]);

    // The pill renders in both the List row and the detail pane for the
    // selected task, so query_all rather than the exactly-one query.
    assert!(harness.query_all_by_label("QUEUED").next().is_some());
    assert!(
        harness.query_by_label("Dispatch").is_none(),
        "a queued task should not re-offer the initial Dispatch button"
    );
    harness.get_by_label("Unflag").click_accesskit();
    harness.run();
    // Synchronous handler status and background success status are the same
    // string — see dispatch_button_confirms_before_flagging.
    assert_eq!(
        harness.state().backlog_status.snapshot().as_deref(),
        Some("unflagged TASK-1 for dispatch"),
        "Unflag should set its status"
    );
    wait_for_labels(fixture.path(), &[]);
}

/// A task labeled `dispatching` (claimed by the worker) shows the
/// DISPATCHING pill and an in-progress message, with no user action
/// available — the worker owns the task until it lands or fails.
#[test]
fn in_flight_task_shows_dispatching_pill_with_no_actions() {
    let mut task = seeded_backlog_task();
    task.labels = vec![DISPATCHING_LABEL.to_string()];
    let harness = harness_on_task(task);

    assert!(harness.query_all_by_label("DISPATCHING").next().is_some());
    assert!(harness.query_by_label("Dispatch").is_none());
    assert!(harness.query_by_label("Unflag").is_none());
}

/// A task labeled `dispatched` with a "Dispatch PR: <url>" note surfaces the
/// DISPATCHED pill and the PR link itself, read straight out of notes.
#[test]
fn dispatched_task_surfaces_the_pr_link() {
    let mut task = seeded_backlog_task();
    task.labels = vec![DISPATCHED_LABEL.to_string()];
    task.implementation_notes =
        "Dispatch PR: https://github.com/example/switchbard/pull/7".to_string();
    let harness = harness_on_task(task);

    assert!(harness.query_all_by_label("DISPATCHED").next().is_some());
    assert!(harness
        .query_by_label("https://github.com/example/switchbard/pull/7")
        .is_some());
}

/// A task labeled `dispatched` with no matching note falls back to an
/// explicit "not found" message rather than silently showing nothing.
#[test]
fn dispatched_task_without_a_pr_note_shows_the_fallback_message() {
    let mut task = seeded_backlog_task();
    task.labels = vec![DISPATCHED_LABEL.to_string()];
    task.implementation_notes = "Existing note".to_string();
    let harness = harness_on_task(task);

    assert!(harness.query_all_by_label("DISPATCHED").next().is_some());
    assert!(harness
        .query_by_label("(PR link not found in notes)")
        .is_some());
}

/// A task labeled `dispatch-failed` surfaces the failure reason and offers
/// the Dispatch button again — a failed run is retryable, unlike a
/// dispatched (already-landed) one.
#[test]
fn failed_task_shows_the_reason_and_offers_a_retry() {
    let mut task = seeded_backlog_task();
    task.labels = vec![DISPATCH_FAILED_LABEL.to_string()];
    task.implementation_notes = "Dispatch failed: headless run exited with status 1".to_string();
    let harness = harness_on_task(task);

    assert!(harness
        .query_all_by_label("DISPATCH FAILED")
        .next()
        .is_some());
    assert!(harness
        .query_by_label("headless run exited with status 1")
        .is_some());
    assert!(
        harness.query_by_label("Dispatch").is_some(),
        "a failed task should offer to retry via the same Dispatch button"
    );
}

/// The List row's compact pill (task-11 GUI wiring's List/Board surface, not
/// just the detail pane) renders for a queued task and stays absent for an
/// unflagged sibling in the same row list.
#[test]
fn list_row_shows_the_dispatch_pill_for_a_queued_task() {
    let mut queued = seeded_backlog_task();
    queued.id = "TASK-1".to_string();
    queued.labels = vec![DISPATCH_LABEL.to_string()];

    let mut plain = seeded_backlog_task();
    plain.id = "TASK-2".to_string();
    plain.title = "Plain task".to_string();
    plain.path = PathBuf::from(format!("{REPO_PATH}/backlog/tasks/task-2.md"));

    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.backlog_view.lens = BacklogLens::List;
    app.backlog_view.selected_repo = Some(PathBuf::from(REPO_PATH));
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![queued, plain],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking: RepoRanking::default(),
            loaded_at_unix: 0,
            configured_statuses: vec![
                "Icebox".into(),
                "To Do".into(),
                "In Progress".into(),
                "In Review".into(),
                "Done".into(),
            ],
        },
    );
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_all_by_label("QUEUED").next().is_some(),
        "the queued task's row should show the QUEUED pill"
    );
}

/// Stack ranking (trajectory: *Stack ranking*): a ranked + expedited fixture
/// in the Tasks place (grouped by Project — TASK-97 subsumed the old
/// Projects lens into this generic group-by) renders the expedite marker
/// without any click, and the selected task's detail rail offers the
/// lane's exit affordance.
#[test]
fn tasks_place_grouped_by_project_renders_the_expedite_marker_and_lane_toggle() {
    let mut app = seeded_app();
    app.place = Place::Tasks;
    app.backlog_view.selected_repo = Some(PathBuf::from(REPO_PATH));
    let mut task = seeded_backlog_task();
    task.project = Some("Ranked Project".to_string());
    app.backlog_view.selected_task = Some((PathBuf::from(REPO_PATH), task.id.clone()));
    let ranking = RepoRanking {
        projects: vec!["Ranked Project".to_string()],
        expedite: vec![task.id.clone()],
        tasks: std::iter::once(("Ranked Project".to_string(), vec![task.id.clone()])).collect(),
        ..RepoRanking::default()
    };
    app.backlog_repos.lock().unwrap().insert(
        PathBuf::from(REPO_PATH),
        BacklogRepo {
            root: PathBuf::from(REPO_PATH),
            tasks: vec![task],
            warnings: vec![],
            project_defs: vec![],
            initiative_defs: vec![],
            goals: vec![],
            ranking,
            loaded_at_unix: 0,
            configured_statuses: vec!["To Do".into(), "In Progress".into(), "Done".into()],
        },
    );
    let mut harness = harness(app);
    harness.run();

    assert!(
        harness.query_by_label("Ranked Project").is_some(),
        "the ranked project's group header renders"
    );
    assert!(
        harness.query_by_label("0/1 done").is_some(),
        "the group header's computed roll-up renders"
    );
    // Two nodes is correct: the Tasks place's own row pill and the detail
    // rail's own lane pill both render the marker.
    assert_eq!(
        harness.query_all_by_label("expedited").count(),
        2,
        "an expedited task's row and detail rail both wear the lane marker"
    );
    assert!(
        harness
            .query_by_label("Remove from expedite lane")
            .is_some(),
        "the detail rail offers the lane's exit for the selected expedited task"
    );
}
