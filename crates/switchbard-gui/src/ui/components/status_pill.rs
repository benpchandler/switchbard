//! Colored "status pill" labels used for STATUS / STATE / ACTIVITY columns.
//!
//! Replaces a family of inline `ui.colored_label(color, text)` + optional
//! `.on_hover_text(...)` calls. One named call per pill kind so a future
//! "what does 'Running' look like" change is one diff.

use crate::ui::theme;
use eframe::egui;

/// Semantic kind of pill. Determines color + (optional) hover text. The
/// caller still provides the body text since the wording often carries
/// context (pid, port, uptime).
#[derive(Debug, Clone, Copy)]
pub enum StatusKind {
    /// Healthy / running / clean.
    Good,
    /// Has user attention (dirty, drifted, slow).
    Warn,
    /// Different-network status (external-live, drift).
    Info,
    /// Failure / blocked.
    Danger,
    /// Neutral / idle / placeholder.
    Neutral,
}

impl StatusKind {
    fn color(self) -> egui::Color32 {
        match self {
            Self::Good => theme::GREEN,
            Self::Warn => theme::AMBER,
            Self::Info => theme::SKY,
            Self::Danger => theme::WARN_ORANGE,
            Self::Neutral => theme::WEAK_TEXT,
        }
    }
}

/// Render a colored label with optional hover text.
pub fn status_pill(
    ui: &mut egui::Ui,
    kind: StatusKind,
    text: impl Into<String>,
    hover: Option<&str>,
) -> egui::Response {
    let resp = ui.colored_label(kind.color(), text.into());
    if let Some(h) = hover {
        resp.on_hover_text(h)
    } else {
        resp
    }
}
