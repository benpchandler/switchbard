//! TASK-78: both palettes x four elevations, including live palette switches.
//! No surface migration: layout, lifecycle and input stress are not introduced.
use eframe::egui::{self, Color32};
use switchbard_gui::ui::{
    legibility,
    theme::{self, Elevation, ThemeChoice},
};

#[test]
fn elevation_hierarchy_preserves_existing_surface_authorities() {
    let ctx = egui::Context::default();
    for choice in [ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::Light] {
        theme::apply(&ctx, choice);
        let [well, panel, card, overlay] = [
            Elevation::Well,
            Elevation::Panel,
            Elevation::Card,
            Elevation::Overlay,
        ]
        .map(theme::elevation);
        assert_eq!(well.fill, theme::faint_bg());
        assert_eq!(panel.fill, ctx.global_style().visuals.panel_fill);
        assert_eq!(card.fill, theme::card_bg());
        assert_eq!(overlay.fill, card.fill);
        assert_eq!(well.shadow, egui::epaint::Shadow::NONE);
        assert_eq!(panel.shadow, egui::epaint::Shadow::NONE);
        assert_eq!(panel.stroke, egui::Stroke::NONE);
        assert_eq!(well.stroke, theme::surface_stroke());
        assert_eq!(card.stroke, theme::surface_stroke());
        assert_eq!(card.shadow, theme::card_shadow());
        assert!(overlay.stroke.width > card.stroke.width);
        assert_eq!(overlay.stroke.color, card.stroke.color);
        assert!(overlay.shadow.blur > card.shadow.blur);
        assert!(overlay.shadow.offset[1] > card.shadow.offset[1]);
        assert_ne!(well.fill, panel.fill);
        assert_ne!(panel.fill, card.fill);
    }
}

#[test]
fn semantic_text_clears_aa_on_every_elevation() {
    let ctx = egui::Context::default();
    for choice in [ThemeChoice::Light, ThemeChoice::Dark] {
        theme::apply(&ctx, choice);
        for level in [
            Elevation::Well,
            Elevation::Panel,
            Elevation::Card,
            Elevation::Overlay,
        ] {
            let fill = theme::elevation(level).fill;
            for text in semantic_text_colors() {
                let ratio = legibility::contrast_ratio(text, fill);
                assert!(
                    ratio >= 4.5,
                    "{choice:?}/{level:?}: {text:?} contrast {ratio}"
                );
            }
            let warn = theme::warn_orange();
            let tint = legibility::composite_over(theme::chip_tint(warn), fill);
            assert!(
                legibility::contrast_ratio(warn, tint) >= 4.5,
                "{choice:?}/{level:?}: warning chip"
            );
        }
    }
}

fn semantic_text_colors() -> [Color32; 10] {
    [
        theme::weak_text(),
        theme::muted_text(),
        theme::green(),
        theme::amber(),
        theme::amber_question(),
        theme::lavender(),
        theme::sky(),
        theme::dispatch_accent(),
        theme::warn_orange(),
        theme::danger(),
    ]
}
