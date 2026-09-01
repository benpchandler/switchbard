//! Named filter+sort+lens combinations (task-20): save the current Backlog
//! view under a name, pick a saved one back up later, or delete it.
//!
//! Repos stay the system of record for task data; a saved view is
//! engagement-only state — which tasks you're looking at, not the tasks
//! themselves — so it's persisted additively on `Config::ui.saved_views`,
//! the single existing config source of truth, rather than a new store.
//!
//! ## TASK-97 medic pass (MAJOR finding): restored inside the Tasks place
//!
//! `render_saved_views_bar`'s only caller used to be the orphaned legacy
//! `ui::backlog::render` body — dead-reachable, so saving/browsing/deleting
//! a view had no UI anywhere the app actually routes to. `ui::places::
//! tasks::mod::render_facets_row` is now this bar's caller, which is also
//! why `current_as_saved_view`/`apply_saved_view` below capture and restore
//! the Tasks place's own state (`tasks_place.{filters,group_by,view_mode}`),
//! not just the four legacy single-value facets task-20 originally covered.

use super::reset_task_selection;
use crate::app::HiveApp;
use crate::runtime::{BacklogLens, BacklogTaskSortDirection, BacklogTaskSortKey};
use crate::ui::places::tasks::fields::TaskField;
use crate::ui::places::tasks::state::{FilterPredicate, TasksViewMode};
use crate::ui::theme;
use eframe::egui;
use std::path::Path;
use switchbard_core::config::{SavedFilterPredicate, SavedView};

pub(crate) fn render_saved_views_bar(app: &mut HiveApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("View").color(theme::muted_text()));
        let current_label = app
            .backlog_view
            .active_saved_view
            .clone()
            .unwrap_or_else(|| "Unsaved".to_string());
        egui::ComboBox::from_id_salt("backlog_saved_views")
            .selected_text(current_label)
            .width(160.0)
            .show_ui(ui, |ui| {
                for view in app.config.ui.saved_views.clone() {
                    let selected =
                        app.backlog_view.active_saved_view.as_deref() == Some(view.name.as_str());
                    if ui.selectable_label(selected, &view.name).clicked() {
                        apply_saved_view(app, &view);
                    }
                }
                if app.config.ui.saved_views.is_empty() {
                    ui.label(egui::RichText::new("No saved views yet").color(theme::muted_text()));
                }
            });
        // TASK-96: the view-kind favorite star — only meaningful once a
        // named saved view is actually active (an "Unsaved" combo state has
        // nothing to favorite yet). `FavoriteRef::repo` is the empty string
        // for this kind: a saved view is a top-level named entry in
        // `Config::ui.saved_views`, not scoped to one repo the way a task,
        // goal, or project is, so there is nothing else to key on.
        if let Some(name) = app.backlog_view.active_saved_view.clone() {
            let favorited = app.is_favorited(
                switchbard_core::config::FavoriteKind::View,
                Path::new(""),
                &name,
            );
            let hover = if favorited {
                "Remove from Favorites"
            } else {
                "Add to Favorites"
            };
            if theme::favorite_star_button(ui, favorited)
                .on_hover_text(hover)
                .clicked()
            {
                app.toggle_favorite(
                    switchbard_core::config::FavoriteKind::View,
                    Path::new(""),
                    &name,
                );
            }
        }
        if app.backlog_view.active_saved_view.is_some()
            && ui
                .small_button("Delete")
                .on_hover_text("Delete this saved view")
                .clicked()
        {
            if let Some(name) = app.backlog_view.active_saved_view.take() {
                app.config.ui.saved_views.retain(|v| v.name != name);
                app.save_config();
            }
        }

        ui.separator();
        // Enter commits, rather than a separate Save button that spends
        // almost all of its life disabled next to an empty field. The name
        // field is the whole input, so there is nothing else for a click to
        // disambiguate; `lost_focus` + Enter is egui's idiom for it.
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.backlog_view.saved_view_name_draft)
                .hint_text("Save current view as…  ⏎")
                .desired_width(180.0),
        );
        let name = app.backlog_view.saved_view_name_draft.trim().to_string();
        let submitted = response.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && !name.is_empty();
        if submitted {
            let view = current_as_saved_view(app, name.clone());
            // Saving under an existing name overwrites it — "Save" doubles
            // as "update", matching how most apps treat named-view saving.
            app.config.ui.saved_views.retain(|v| v.name != name);
            app.config.ui.saved_views.push(view);
            app.backlog_view.active_saved_view = Some(name);
            app.backlog_view.saved_view_name_draft.clear();
            app.save_config();
        }
    });
}

fn current_as_saved_view(app: &HiveApp, name: String) -> SavedView {
    SavedView {
        name,
        // IA V2 rejected one-repo-at-a-time switching. Repo scope belongs
        // to the sidebar and task-level repo filters live in `tasks_filters`
        // as visible predicates, so new saved views never write the removed
        // legacy picker field.
        selected_repo: None,
        // The legacy four single-value facets are dead weight for a view
        // saved from the Tasks place now (`tasks_place.filters` below is the
        // real source of truth) — written as "all" rather than mirroring
        // `tasks_place.filters`, so there is exactly one encoding of the
        // active predicate set on a freshly-saved view, not two that could
        // drift apart. `apply_saved_view`'s legacy-translation fallback only
        // ever fires for a view saved *before* this field existed.
        status_filter: "all".to_string(),
        priority_filter: "all".to_string(),
        project_filter: "all".to_string(),
        label_filter: "all".to_string(),
        sort_key: app.backlog_view.sort_key.as_saved_id().to_string(),
        sort_direction: app.backlog_view.sort_direction.as_saved_id().to_string(),
        lens: app.backlog_view.lens.as_saved_id().to_string(),
        show_completed: app.backlog_view.show_completed,
        show_archived: app.backlog_view.show_archived,
        show_drafts: app.backlog_view.show_drafts,
        tasks_filters: app
            .tasks_place
            .filters
            .iter()
            .map(|predicate| SavedFilterPredicate {
                field: predicate.field.as_id().to_string(),
                value: predicate.value.clone(),
            })
            .collect(),
        tasks_group_by: app
            .tasks_place
            .group_by
            .map(TaskField::as_id)
            .unwrap_or("")
            .to_string(),
        tasks_view_mode: app.tasks_place.view_mode.as_id().to_string(),
    }
}

/// `pub(super)`, not private: TASK-96's favorite-view navigation
/// (`ui::backlog::apply_saved_view_by_name`, called from `ui::nav`) applies
/// a saved view the same way this module's own combo box does — one
/// definition of "apply a saved view", not two.
pub(super) fn apply_saved_view(app: &mut HiveApp, view: &SavedView) {
    // `SavedView::selected_repo` remains readable for config compatibility,
    // but IA V2 has no single-repo picker. Applying an old view must not
    // resurrect a hidden scope underneath the sidebar's "All repos" state.
    app.backlog_view.selected_repo = None;
    // TASK-97 medic pass: these four are never read for anything meaningful
    // once the Tasks place renders (`tasks::neutralize_legacy_filters`
    // forces them back to "all" every frame it's active) — reset here too so
    // a moment-of-apply glance at `backlog_view` never shows a stale value
    // this crate no longer honors.
    app.backlog_view.status_filter = "all".to_string();
    app.backlog_view.priority_filter = "all".to_string();
    app.backlog_view.project_filter = "all".to_string();
    app.backlog_view.label_filter = "all".to_string();
    app.backlog_view.sort_key = BacklogTaskSortKey::from_saved_id(&view.sort_key);
    app.backlog_view.sort_direction = BacklogTaskSortDirection::from_saved_id(&view.sort_direction);
    app.backlog_view.lens = BacklogLens::from_saved_id(&view.lens);
    app.backlog_view.show_completed = view.show_completed;
    app.backlog_view.show_archived = view.show_archived;
    app.backlog_view.show_drafts = view.show_drafts;
    app.backlog_view.active_saved_view = Some(view.name.clone());
    reset_task_selection(app);

    // TASK-97 medic pass (BLOCKER finding): every reachable caller of
    // `apply_saved_view` (this bar's own combo, and `navigate_to_favorite`'s
    // `View` arm) puts the Tasks place on screen either just before or in
    // this same call — it is the only surface a saved view ever applies to
    // now. Guarded on `app.place` anyway rather than assumed, so this stays
    // honest if a future caller ever applies one from somewhere else.
    if app.place == crate::runtime::Place::Tasks {
        apply_tasks_place_state(app, view);
    }
}

/// Restore `tasks_place.{filters,group_by,view_mode}` from `view`. New-
/// format views (saved since this field existed) carry `tasks_filters`
/// directly — exact fidelity, no lossy round-trip. A view saved before it
/// existed has an empty `tasks_filters` *and* every legacy facet at "all"
/// (nothing else ever wrote a non-"all" legacy facet and left `tasks_filters`
/// empty — `current_as_saved_view` always writes both together), so falling
/// back to translating the legacy four facets in that case is unambiguous:
/// either both sides agree there were no predicates, or the legacy facets
/// are the only record of what the user actually saved.
fn apply_tasks_place_state(app: &mut HiveApp, view: &SavedView) {
    let predicates: Vec<FilterPredicate> = if !view.tasks_filters.is_empty() {
        view.tasks_filters
            .iter()
            .filter_map(|predicate| {
                Some(FilterPredicate {
                    field: TaskField::from_id(&predicate.field)?,
                    value: predicate.value.clone(),
                })
            })
            .collect()
    } else {
        let mut translated = Vec::new();
        let mut push = |field: TaskField, value: &str| {
            if value != "all" {
                translated.push(FilterPredicate {
                    field,
                    value: value.to_string(),
                });
            }
        };
        push(TaskField::Status, &view.status_filter);
        push(TaskField::Priority, &view.priority_filter);
        push(TaskField::Project, &view.project_filter);
        push(TaskField::Label, &view.label_filter);
        translated
    };
    let previous = std::mem::replace(&mut app.tasks_place.filters, predicates);
    app.tasks_place.remember_recent(previous);
    app.tasks_place.group_by = if view.tasks_group_by.is_empty() {
        None
    } else {
        TaskField::from_id(&view.tasks_group_by)
    };
    app.tasks_place.view_mode = TasksViewMode::from_id(&view.tasks_view_mode);
}
