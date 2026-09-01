//! Tinted "status pill" chips used for STATUS / STATE / ACTIVITY columns.
//!
//! Replaces a family of inline `ui.colored_label(color, text)` + optional
//! `.on_hover_text(...)` calls with the design mock's actual `.chip`
//! language (`theme::painted_chip`: rounded, low-alpha tinted background,
//! colored text, no border) — TASK-76's headline parity gap. One named call
//! per pill kind so a future "what does 'Running' look like" change is one
//! diff, and every surface (Digest, Tasks, Dispatches, Command, Ops, Goals)
//! picks up the chip look for free by going through this function.

use crate::ui::theme;
use eframe::egui;

/// Semantic kind of pill. Determines color + tint + (optional) hover text.
/// The caller still provides the body text since the wording often carries
/// context (pid, port, uptime).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    /// Healthy / running / clean.
    Good,
    /// Has user attention (dirty, drifted, slow).
    Warn,
    /// Different-network status (external-live, drift).
    Info,
    /// Failure / blocked.
    Danger,
    /// Neutral / idle / placeholder — the mock's bordered, unfilled base
    /// `.chip` rather than one of its tinted color variants.
    Neutral,
    /// PR / delivery state ("PR", "PR merged") — the mock's `.chip.lav`.
    Delivered,
}

impl StatusKind {
    /// `pub(crate)`, not private: the compact Digest goal card and the
    /// Tasks-place expanded group-header meter (`ui::backlog::digest`,
    /// `ui::places::tasks::header`) both color a [`theme::painted_meter`]
    /// fill by a goal's pace using this exact mapping, so the meter and its
    /// neighboring pace pill can never show two different colors for the
    /// same state.
    pub(crate) fn color(self) -> egui::Color32 {
        match self {
            Self::Good => theme::green(),
            Self::Warn => theme::amber(),
            Self::Info => theme::sky(),
            Self::Danger => theme::warn_orange(),
            Self::Neutral => theme::weak_text(),
            Self::Delivered => theme::lavender(),
        }
    }

    /// The chip's tinted background, or `None` for [`Self::Neutral`], which
    /// renders as a bordered/unfilled pill instead (mock's base `.chip`).
    fn tint(self) -> Option<egui::Color32> {
        match self {
            Self::Neutral => None,
            other => Some(theme::chip_tint(other.color())),
        }
    }
}

/// Render a tinted status chip with optional hover text.
pub fn status_pill(
    ui: &mut egui::Ui,
    kind: StatusKind,
    text: impl Into<String>,
    hover: Option<&str>,
) -> egui::Response {
    let resp = theme::painted_chip(ui, kind.tint(), kind.color(), &text.into());
    if let Some(h) = hover {
        resp.on_hover_text(h)
    } else {
        resp
    }
}
