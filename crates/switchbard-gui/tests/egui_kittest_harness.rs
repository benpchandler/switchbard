use eframe::egui::{self, accesskit::Toggled};
use egui_kittest::Harness;
use kittest::Queryable;

#[test]
fn harness_queries_and_clicks_egui_widgets() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(260.0, 120.0))
        .build_ui_state(
            |ui, checked| {
                ui.checkbox(checked, "Kittest checkbox");
                if *checked {
                    ui.label("checked state");
                }
            },
            false,
        );

    let checkbox = harness.get_by_label("Kittest checkbox");
    assert_eq!(checkbox.toggled(), Some(Toggled::False));
    checkbox.click();

    harness.run();

    let checkbox = harness.get_by_label("Kittest checkbox");
    assert_eq!(checkbox.toggled(), Some(Toggled::True));
    assert!(harness.query_by_label("checked state").is_some());
}
