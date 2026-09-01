//! TASK-97: the "+ Filter" builder — add/remove AND-combined field/value
//! predicates over the same generic `TaskField` set `groups.rs` groups by,
//! plus a "recent:" row of the last few filter sets (binding directive: no
//! hardcoded chips — every value offered here comes from the current
//! sidebar scope before the builder's own predicates are applied, not a
//! fixed list or the already-narrowed result set).

use eframe::egui;

use crate::app::HiveApp;
use crate::ui::backlog::TaskRow;
use crate::ui::theme;

use super::fields::{self, TaskField};
use super::state::FilterPredicate;

/// Whether `row` satisfies every active predicate (AND).
pub(super) fn matches(row: &TaskRow<'_>, predicates: &[FilterPredicate]) -> bool {
    predicates.iter().all(|predicate| {
        fields::field_values(row.task, row.repo, predicate.field)
            .iter()
            .any(|value| value == &predicate.value)
    })
}

/// The facets row's filter-builder controls: the active predicate chips
/// (each removable), the "+ Filter" affordance that opens the add-predicate
/// popup, and the "recent:" row. Returns `true` when the active set changed
/// (the caller re-derives grouped rows either way, so this is only used to
/// decide whether to push the *previous* set into `recent_filter_sets`).
pub(super) fn render(app: &mut HiveApp, ui: &mut egui::Ui, all_tasks: &[TaskRow<'_>]) {
    ui.horizontal_wrapped(|ui| {
        let popup_id = ui.make_persistent_id("tasks_place_filter_builder");
        let button = ui.add_sized(
            [56.0, ui.spacing().interact_size.y],
            egui::Button::new("+ Filter"),
        );
        if button.clicked() {
            app.tasks_place.filter_builder_open = !app.tasks_place.filter_builder_open;
        }
        if app.tasks_place.filter_builder_open {
            render_add_predicate_popup(app, &button, popup_id, all_tasks);
        }

        let mut remove_index = None;
        for (index, predicate) in app.tasks_place.filters.iter().enumerate() {
            let chip = format!("{}: {} ✕", predicate.field.label(), predicate.value);
            if ui
                .small_button(chip)
                .on_hover_text("Remove this filter")
                .clicked()
            {
                remove_index = Some(index);
            }
        }
        if let Some(index) = remove_index {
            let mut cleared = app.tasks_place.filters.clone();
            cleared.remove(index);
            let previous = std::mem::replace(&mut app.tasks_place.filters, cleared);
            app.tasks_place.remember_recent(previous);
        }
    });

    if !app.tasks_place.recent_filter_sets.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new("recent:")
                    .small()
                    .color(theme::muted_text()),
            );
            let mut apply_index = None;
            for (index, set) in app.tasks_place.recent_filter_sets.iter().enumerate() {
                let label = set
                    .iter()
                    .map(|p| format!("{}: {}", p.field.label(), p.value))
                    .collect::<Vec<_>>()
                    .join(", ");
                if ui
                    .small_button(label)
                    .on_hover_text("Apply this filter set")
                    .clicked()
                {
                    apply_index = Some(index);
                }
            }
            if let Some(index) = apply_index {
                let set = app.tasks_place.recent_filter_sets[index].clone();
                let previous = std::mem::replace(&mut app.tasks_place.filters, set);
                app.tasks_place.remember_recent(previous);
            }
        });
    }
}

fn render_add_predicate_popup(
    app: &mut HiveApp,
    button: &egui::Response,
    popup_id: egui::Id,
    all_tasks: &[TaskRow<'_>],
) {
    // `from_response` renders unconditionally whenever `.show()` runs (its
    // own doc: "the popup will be always open") — `app.tasks_place.
    // filter_builder_open` is what actually gates the call, and the Add/
    // Cancel buttons below are what clears it, so there is no separate
    // egui-memory-based open state to reconcile with ours.
    egui::Popup::from_response(button).id(popup_id).show(|ui| {
        ui.set_min_width(220.0);
        egui::ComboBox::from_id_salt("tasks_place_filter_field")
            .selected_text(app.tasks_place.draft_field.label())
            .show_ui(ui, |ui| {
                for field in TaskField::ALL {
                    if ui
                        .selectable_value(&mut app.tasks_place.draft_field, field, field.label())
                        .clicked()
                    {
                        app.tasks_place.draft_value.clear();
                    }
                }
            });
        let options = fields::distinct_values(
            all_tasks.iter().map(|row| (row.task, row.repo)),
            app.tasks_place.draft_field,
        );
        egui::ComboBox::from_id_salt("tasks_place_filter_value")
            .selected_text(if app.tasks_place.draft_value.is_empty() {
                "Choose a value"
            } else {
                app.tasks_place.draft_value.as_str()
            })
            .show_ui(ui, |ui| {
                for value in &options {
                    ui.selectable_value(&mut app.tasks_place.draft_value, value.clone(), value);
                }
            });
        ui.horizontal(|ui| {
            let can_add = !app.tasks_place.draft_value.is_empty();
            if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                app.tasks_place.filters.push(FilterPredicate {
                    field: app.tasks_place.draft_field,
                    value: std::mem::take(&mut app.tasks_place.draft_value),
                });
                app.tasks_place.filter_builder_open = false;
            }
            if ui.button("Cancel").clicked() {
                app.tasks_place.filter_builder_open = false;
            }
        });
    });
}
