//! Presentation helpers for the Shop handoff workspace (packet cards, checklist
//! lines, compact checkboxes). They own appearance and egui responses only; the
//! host decides what a click means.
use eframe::egui::{self, Color32, Response, RichText, Stroke, Ui};

use crate::icons::{Icon, icon};
use crate::theme_widgets as tw;

/// Radio outline for an unselected packet card (`#BFB5A5`).
const RADIO_IDLE: Color32 = Color32::from_rgb(191, 181, 165);

/// A packet-type radio card. The whole card is the click target (registered
/// from the previous frame's bounds so inner links stay on top); the returned
/// response is the card's. `locked` shows a lock with its reason as tooltip.
pub(crate) fn radio_card(
    ui: &mut Ui,
    id: egui::Id,
    selected: bool,
    locked: Option<&str>,
    title: &str,
    body: impl FnOnce(&mut Ui),
) -> Option<Response> {
    let enabled = locked.is_none();
    let previous: Option<egui::Rect> = ui.ctx().data(|d| d.get_temp(id));
    let response = previous.map(|rect| {
        let response = ui.interact(
            rect,
            id,
            if enabled {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, title)
        });
        response
    });
    let hovered = enabled
        && !selected
        && response
            .as_ref()
            .is_some_and(|r| r.hovered() || r.has_focus());
    let (fill, stroke) = if selected {
        (tw::CARD, Stroke::new(1.5, tw::TEXT))
    } else if !enabled {
        (tw::APP, Stroke::new(1.0, tw::BORDER_SOFT))
    } else if hovered {
        (tw::CARD, Stroke::new(1.0, tw::BORDER_STRONG))
    } else {
        (tw::CARD, Stroke::new(1.0, tw::BORDER_SOFT))
    };
    let shown = egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(9)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (dot, _) = ui.allocate_exact_size(egui::vec2(16.0, 18.0), egui::Sense::hover());
                let center = dot.center();
                if selected {
                    ui.painter().circle_filled(center, 8.0, tw::TEXT);
                    ui.painter().circle_filled(center, 3.0, tw::CARD);
                } else {
                    ui.painter()
                        .circle_stroke(center, 7.25, Stroke::new(1.5, RADIO_IDLE));
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.add(
                            egui::Label::new(tw::semibold(ui, title, 13.0).color(if enabled {
                                tw::TEXT
                            } else {
                                tw::SECONDARY
                            }))
                            .selectable(false),
                        );
                        if let Some(reason) = locked {
                            ui.add(icon(Icon::Lock, tw::FAINT, 12.0))
                                .on_hover_text(reason);
                        }
                    });
                    body(ui);
                });
            });
        });
    ui.ctx()
        .data_mut(|d| d.insert_temp(id, shown.response.rect));
    response
}

/// Status of one checklist line.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Check {
    Ok,
    Warn,
    Note,
    Pending,
}

/// One 12px checklist line with an optional trailing "Fix" link. Long text is
/// truncated; `detail` (or the text itself) shows on hover. Returns whether
/// the link was clicked.
pub(crate) fn check_line(
    ui: &mut Ui,
    check: Check,
    text: &str,
    detail: Option<&str>,
    fix: Option<(&str, &str, bool)>,
) -> bool {
    let mut clicked = false;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 20.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let (symbol, tint, ink) = match check {
                Check::Ok => (Some(Icon::Check), tw::OK, tw::OK),
                Check::Warn => (Some(Icon::Warning), tw::WARN, tw::WARN_INK),
                Check::Note => (Some(Icon::Warning), tw::DISABLED, tw::MUTED),
                Check::Pending => (None, tw::FAINT, tw::FAINT),
            };
            match symbol {
                Some(symbol) => {
                    ui.add(icon(symbol, tint, 12.0));
                }
                None => {
                    ui.add(egui::Spinner::new().size(12.0).color(tint));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some((short, accessible, enabled)) = fix {
                    clicked = link(ui, short, accessible, tw::ACCENT_DARK, enabled).clicked();
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(text).size(12.0).color(ink))
                            .truncate()
                            .selectable(false),
                    )
                    .on_hover_text(detail.unwrap_or(text));
                });
            });
        },
    );
    clicked
}

/// Frameless text link with its own accessible name (e.g. "Fix" labelled
/// "Fix in workspace").
pub(crate) fn link(
    ui: &mut Ui,
    text: &str,
    accessible: &str,
    color: Color32,
    enabled: bool,
) -> Response {
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), egui::FontId::proportional(12.0), color);
    let (rect, response) = ui.allocate_exact_size(
        galley.size() + egui::vec2(4.0, 4.0),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, accessible));
    let response = if accessible != text {
        response.on_hover_text(accessible)
    } else {
        response
    };
    let color = if enabled {
        color
    } else {
        color.gamma_multiply(0.4)
    };
    let pos = rect.center() - galley.size() / 2.0;
    if enabled && (response.hovered() || response.has_focus()) {
        ui.painter().hline(
            pos.x..=pos.x + galley.size().x,
            pos.y + galley.size().y - 1.0,
            Stroke::new(1.0, color),
        );
    }
    ui.painter().galley(pos, galley, color);
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// 15px checkbox (filled `text` with a white check when on) and a 12.5px label.
pub(crate) fn checkbox(ui: &mut Ui, value: &mut bool, label: &str) -> Response {
    let galley = ui.painter().layout(
        label.to_owned(),
        egui::FontId::proportional(12.5),
        tw::TEXT,
        (ui.available_width() - 23.0).max(20.0),
    );
    let height = galley.size().y.max(20.0);
    let (rect, mut response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *value, label)
    });
    let boxed = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.top() + (height.min(20.0) - 15.0) / 2.0),
        egui::vec2(15.0, 15.0),
    );
    if *value {
        ui.painter().rect_filled(boxed, 4.0, tw::TEXT);
        icon(Icon::Check, tw::PANEL, 10.0).paint_at(
            ui,
            egui::Rect::from_center_size(boxed.center(), egui::vec2(10.0, 10.0)),
        );
    } else {
        ui.painter().rect(
            boxed,
            4.0,
            tw::CARD,
            Stroke::new(
                1.0,
                if response.hovered() {
                    tw::MUTED
                } else {
                    tw::BORDER_STRONG
                },
            ),
            egui::StrokeKind::Inside,
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            boxed.expand(2.0),
            5.0,
            Stroke::new(1.0, tw::FOCUS),
            egui::StrokeKind::Outside,
        );
    }
    ui.painter().galley(
        egui::pos2(
            rect.left() + 23.0,
            rect.top() + (height - galley.size().y) / 2.0,
        ),
        galley,
        tw::TEXT,
    );
    response
}

/// Label column (`width`) + control, vertically centred, for the PDF OUTPUT grid.
pub(crate) fn field_row<R>(
    ui: &mut Ui,
    label: &str,
    width: f32,
    top_aligned: bool,
    content: impl FnOnce(&mut Ui) -> R,
) -> R {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let align = if top_aligned {
            egui::Align::Min
        } else {
            egui::Align::Center
        };
        ui.allocate_ui_with_layout(
            egui::vec2(width, 30.0),
            egui::Layout::left_to_right(align),
            |ui| {
                ui.set_width(width);
                ui.add(
                    egui::Label::new(RichText::new(label).size(13.0).color(tw::MUTED))
                        .truncate()
                        .selectable(false),
                );
            },
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            content(ui)
        })
        .inner
    })
    .inner
}
