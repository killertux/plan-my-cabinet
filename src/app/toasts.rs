//! Short-lived messages for failures that have no other place in the UI.
//!
//! Actions that fail after a click, shortcut or palette selection report here
//! instead of being discarded, so the user always learns why nothing happened.
use eframe::egui;

use crate::icons::{Icon, icon};
use crate::theme_widgets as tw;

/// Seconds a toast stays visible unless the pointer rests on it.
const LIFETIME: f64 = 5.0;
const MAX_VISIBLE: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Error,
    Info,
}

#[derive(Clone, Debug)]
struct Toast {
    kind: ToastKind,
    text: String,
    /// Set on the first frame the toast is shown.
    shown_at: Option<f64>,
}

#[derive(Default)]
pub(crate) struct Toasts {
    items: Vec<Toast>,
}

impl Toasts {
    pub(crate) fn error(&mut self, text: impl Into<String>) {
        self.push(ToastKind::Error, text.into());
    }

    #[allow(dead_code)] // Informational toasts are part of the channel's API.
    pub(crate) fn info(&mut self, text: impl Into<String>) {
        self.push(ToastKind::Info, text.into());
    }

    fn push(&mut self, kind: ToastKind, text: String) {
        // Repeating the same failure restarts its timer instead of stacking.
        self.items.retain(|toast| toast.text != text);
        self.items.push(Toast {
            kind,
            text,
            shown_at: None,
        });
        if self.items.len() > MAX_VISIBLE {
            self.items.remove(0);
        }
    }

    #[cfg(test)]
    pub(crate) fn texts(&self) -> Vec<&str> {
        self.items.iter().map(|toast| toast.text.as_str()).collect()
    }

    /// Paints the stack at the bottom right of `area`, above the status bar.
    pub(crate) fn show(&mut self, ctx: &egui::Context, area: egui::Rect, close_label: &str) {
        if self.items.is_empty() {
            return;
        }
        let now = ctx.input(|input| input.time);
        let mut dismissed = None;
        let mut hovered = false;
        egui::Area::new(egui::Id::new("toasts"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .fixed_pos(area.right_bottom() - egui::vec2(16.0, 16.0))
            .interactable(true)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for (index, toast) in self.items.iter_mut().enumerate() {
                    toast.shown_at.get_or_insert(now);
                    let (symbol, color) = match toast.kind {
                        ToastKind::Error => (Icon::Warning, tw::DANGER),
                        ToastKind::Info => (Icon::Check, tw::OK),
                    };
                    let response = tw::floating_frame()
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_max_width(360.0);
                            ui.horizontal(|ui| {
                                ui.add(icon(symbol, color, 16.0));
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&toast.text).size(12.5).color(tw::TEXT),
                                    )
                                    .wrap(),
                                );
                                let close = ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("×").size(15.0).color(tw::MUTED),
                                        )
                                        .frame(false),
                                    )
                                    .on_hover_text(close_label);
                                if close.clicked() {
                                    dismissed = Some(index);
                                }
                            });
                        })
                        .response;
                    hovered |= response.contains_pointer();
                }
            });
        if let Some(index) = dismissed {
            self.items.remove(index);
        }
        if hovered {
            // Reading a message pauses every timer.
            for toast in &mut self.items {
                toast.shown_at = Some(now);
            }
        }
        self.items
            .retain(|toast| toast.shown_at.is_none_or(|at| now - at < LIFETIME));
        if !self.items.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_messages_do_not_stack_and_the_oldest_drops_first() {
        let mut toasts = Toasts::default();
        toasts.error("a");
        toasts.error("b");
        toasts.error("a");
        assert_eq!(toasts.texts(), ["b", "a"]);
        toasts.error("c");
        toasts.error("d");
        assert_eq!(toasts.texts(), ["a", "c", "d"]);
    }
}
