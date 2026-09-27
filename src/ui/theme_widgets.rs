//! Shared native presentation primitives for the approved warm-light handoff.
//! These widgets own appearance and egui responses, never domain transactions.
use eframe::egui::{self, Color32, CornerRadius, FontId, Response, RichText, Stroke, Ui};

pub const APP: Color32 = Color32::from_rgb(244, 241, 236);
pub const PANEL: Color32 = Color32::from_rgb(251, 250, 247);
pub const VIEWPORT: Color32 = Color32::from_rgb(236, 232, 225);
pub const BORDER: Color32 = Color32::from_rgb(221, 215, 205);
pub const BORDER_SOFT: Color32 = Color32::from_rgb(230, 224, 214);
pub const TEXT: Color32 = Color32::from_rgb(42, 37, 32);
pub const SECONDARY: Color32 = Color32::from_rgb(90, 82, 72);
pub const MUTED: Color32 = Color32::from_rgb(110, 101, 90);
pub const FAINT: Color32 = Color32::from_rgb(140, 130, 118);
pub const ACCENT_BG: Color32 = Color32::from_rgb(246, 227, 203);
pub const ACCENT_INK: Color32 = Color32::from_rgb(92, 51, 9);
pub const ACCENT: Color32 = Color32::from_rgb(201, 115, 31);
pub const FOCUS: Color32 = Color32::from_rgb(217, 164, 94);
pub const DANGER: Color32 = Color32::from_rgb(180, 65, 47);
pub const OK_BG: Color32 = Color32::from_rgb(228, 238, 227);
pub const OK_INK: Color32 = Color32::from_rgb(63, 107, 69);
pub const WARN_BG: Color32 = Color32::from_rgb(252, 244, 231);
pub const WARN_INK: Color32 = Color32::from_rgb(138, 90, 18);

pub fn apply_visuals(ctx: &egui::Context) {
    let mut v = egui::Visuals::light();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = APP;
    v.faint_bg_color = APP;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(12);
    v.selection.bg_fill = ACCENT_BG;
    v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(138, 79, 14));
    v.hyperlink_color = Color32::from_rgb(154, 91, 18);
    v.warn_fg_color = Color32::from_rgb(183, 121, 31);
    v.error_fg_color = DANGER;
    v.override_text_color = Some(TEXT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SOFT);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.weak_bg_fill = VIEWPORT;
    v.widgets.inactive.bg_fill = APP;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SOFT);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(58, 51, 44));
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(228, 223, 214);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(212, 205, 193));
    v.widgets.active.weak_bg_fill = BORDER;
    v.widgets.open.weak_bg_fill = VIEWPORT;
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(6);
    }
    ctx.set_theme(egui::Theme::Light);
    ctx.set_visuals_of(egui::Theme::Light, v);
    ctx.style_mut_of(egui::Theme::Light, |s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 28.0;
        s.spacing.window_margin = egui::Margin::ZERO;
    });
}

pub const CARD: Color32 = Color32::WHITE;
pub const TEXT_2: Color32 = Color32::from_rgb(58, 51, 44);
pub const DISABLED: Color32 = Color32::from_rgb(181, 172, 159);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(212, 205, 193);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(138, 79, 14);
pub const HOVER_ROW: Color32 = Color32::from_rgb(244, 241, 236);
pub const MULTI_ROW: Color32 = Color32::from_rgb(251, 241, 227);
pub const OK: Color32 = Color32::from_rgb(63, 125, 78);
pub const WARN: Color32 = Color32::from_rgb(183, 121, 31);
pub const WARN_STROKE: Color32 = Color32::from_rgb(235, 210, 176);
pub const KERF: Color32 = Color32::from_rgb(196, 69, 58);
pub const RULE: Color32 = Color32::from_rgb(237, 232, 224);


/// Floating toolbars and HUDs over the viewport or sheet canvas.
pub fn floating_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, BORDER_SOFT))
        .corner_radius(10)
        .inner_margin(4)
        .shadow(egui::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_rgba_unmultiplied(60, 45, 25, 40),
        })
}

/// Warning callout (`warn_bg` fill, soft amber stroke).
pub fn warn_callout() -> egui::Frame {
    egui::Frame::new()
        .fill(WARN_BG)
        .stroke(Stroke::new(1.0, WARN_STROKE))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(10, 8))
}


pub fn mono(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::monospace(size))
}

/// Whether the app's named font families are installed in this context.
/// Bare test contexts only have egui's defaults.
pub fn weights_available(ui: &Ui) -> bool {
    ui.style()
        .text_styles
        .contains_key(&egui::TextStyle::Name("PrimaryButton".into()))
}

pub fn weighted_font(ui: &Ui, size: f32, typeface: super::theme::Typeface) -> FontId {
    if weights_available(ui) {
        FontId::new(size, typeface.family())
    } else {
        FontId::proportional(size)
    }
}

pub fn semibold(ui: &Ui, text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(weighted_font(ui, size, super::theme::Typeface::SansSemibold))
}

pub fn medium(ui: &Ui, text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(weighted_font(ui, size, super::theme::Typeface::SansMedium))
}


#[allow(clippy::too_many_arguments)]
pub fn ghost_icon_sized(
    ui: &mut Ui,
    symbol: crate::icons::Icon,
    accessible_name: &str,
    color: Color32,
    icon_size: f32,
    box_size: f32,
    enabled: bool,
    selected: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(box_size, box_size),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let response = response.on_hover_text(accessible_name);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, accessible_name)
    });
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            TEXT
        } else if enabled && (response.hovered() || response.has_focus()) {
            VIEWPORT
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 7.0, fill);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                7.0,
                Stroke::new(1.0, FOCUS),
                egui::StrokeKind::Inside,
            );
        }
        let tint = if selected {
            PANEL
        } else if enabled {
            color
        } else {
            color.gamma_multiply(0.4)
        };
        let icon_rect = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(icon_size));
        crate::icons::icon(symbol, tint, icon_size).paint_at(ui, icon_rect);
    }
    if enabled {
        response
    } else {
        // Keep keyboard/pointer semantics of a disabled widget.
        response.on_hover_cursor(egui::CursorIcon::Default)
    }
}

/// Button with a leading icon (secondary style unless `primary`).
pub fn icon_text_button(
    ui: &mut Ui,
    symbol: crate::icons::Icon,
    text: &str,
    primary: bool,
    enabled: bool,
) -> Response {
    let (fill, ink, stroke) = if primary {
        (TEXT, PANEL, Stroke::NONE)
    } else {
        (VIEWPORT, TEXT, Stroke::new(1.0, BORDER_SOFT))
    };
    let label = if primary {
        semibold(ui, text, 13.0).color(ink)
    } else {
        medium(ui, text, 13.0).color(ink)
    };
    ui.add_enabled(
        enabled,
        egui::Button::image_and_text(crate::icons::icon(symbol, ink, 15.0), label)
            .fill(fill)
            .stroke(stroke)
            .corner_radius(7)
            .min_size(egui::vec2(0.0, 30.0)),
    )
}

/// Centered empty state: an icon tile, one title, one short line and the
/// primary next step. Returns true when the action was clicked.
pub fn empty_state(
    ui: &mut Ui,
    symbol: crate::icons::Icon,
    title: &str,
    detail: &str,
    action: Option<(crate::icons::Icon, &str)>,
) -> bool {
    let mut clicked = false;
    let height = 44.0 + 14.0 + 22.0 + 40.0 + if action.is_some() { 46.0 } else { 0.0 };
    let top = ((ui.available_height() - height) / 2.0).max(24.0);
    ui.vertical_centered(|ui| {
        ui.add_space(top);
        let (tile, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
        ui.painter().rect_filled(tile, 10.0, ACCENT_BG);
        crate::icons::icon(symbol, ACCENT_DARK, 22.0)
            .paint_at(ui, egui::Rect::from_center_size(tile.center(), egui::vec2(22.0, 22.0)));
        ui.add_space(14.0);
        ui.label(semibold(ui, title, 15.0).color(TEXT));
        ui.add_space(4.0);
        ui.scope(|ui| {
            ui.set_max_width(340.0);
            ui.add(
                egui::Label::new(RichText::new(detail).size(12.5).color(MUTED))
                    .wrap()
                    .halign(egui::Align::Center),
            );
        });
        if let Some((icon, label)) = action {
            ui.add_space(16.0);
            clicked = icon_text_button(ui, icon, label, true, true).clicked();
        }
    });
    clicked
}

/// Plain text link-style button (no frame), e.g. "Discard" in danger colour.
pub fn text_button(ui: &mut Ui, text: &str, color: Color32, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(medium(ui, text, 13.0).color(color)).frame(false),
    )
}

/// Visual state of a list row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowState {
    Normal,
    /// Member of a multi-selection.
    Selected,
    /// The active (primary) selection.
    Active,
}

/// A full-width clickable row. The returned response is the row's click
/// response; interactive widgets added inside `content` take precedence.
pub fn list_row<R>(
    ui: &mut Ui,
    id: egui::Id,
    height: f32,
    state: RowState,
    enabled: bool,
    accessible_name: &str,
    content: impl FnOnce(&mut Ui) -> R,
) -> (Response, R) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
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
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            enabled,
            state != RowState::Normal,
            accessible_name,
        )
    });
    let fill = match state {
        RowState::Active => ACCENT_BG,
        RowState::Selected => MULTI_ROW,
        RowState::Normal if enabled && response.hovered() => HOVER_ROW,
        RowState::Normal => Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(rect, 5.0, fill);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(6.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 6.0;
    let inner = content(&mut child);
    (response, inner)
}

/// Section header: 36 high, uppercase tracked label and trailing actions.
pub fn section_bar<R>(ui: &mut Ui, label: &str, actions: impl FnOnce(&mut Ui) -> R) -> R {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &label.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: weighted_font(
                ui,
                super::theme::SECTION.size,
                super::theme::Typeface::SansSemibold,
            ),
            color: MUTED,
            extra_letter_spacing: super::theme::SECTION.size * super::theme::SECTION_TRACKING_EM,
            ..Default::default()
        },
    );
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 36.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add(egui::Label::new(job).selectable(false));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions)
                .inner
        },
    )
    .inner
}

/// A 1px full-width divider in `border`, used between panel sections.
pub fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, BORDER),
    );
}

/// Label column + value row used in inspectors (label column `label_width`).
pub fn prop_row<R>(
    ui: &mut Ui,
    label: &str,
    label_width: f32,
    value: impl FnOnce(&mut Ui) -> R,
) -> R {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 30.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_width(label_width);
                    ui.add(
                        egui::Label::new(RichText::new(label).color(MUTED))
                            .truncate()
                            .selectable(false),
                    );
                },
            );
            value(ui)
        },
    )
    .inner
}

/// Small painted swatch (radius 3) in an sRGB colour.
pub fn swatch(ui: &mut Ui, color: Color32, size: egui::Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect(
        rect,
        3.0,
        color,
        Stroke::new(1.0, Color32::from_black_alpha(28)),
        egui::StrokeKind::Inside,
    );
    response
}

/// A pill keycap such as `⌘K`.
pub fn keycap(ui: &mut Ui, text: &str) {
    egui::Frame::new()
        .stroke(Stroke::new(1.0, BORDER_STRONG))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(5, 0))
        .show(ui, |ui| {
            ui.label(mono(text, 11.0).color(FAINT));
        });
}

/// Compact inline value field (mono), optionally with an axis-coloured bottom
/// stroke and a unit suffix. Returns the text edit response.
#[allow(clippy::too_many_arguments)]
pub fn value_field(
    ui: &mut Ui,
    id: egui::Id,
    accessible_name: &str,
    text: &mut String,
    width: f32,
    suffix: Option<&str>,
    axis: Option<Color32>,
    enabled: bool,
    invalid: bool,
) -> Response {
    let focused = ui.memory(|m| m.has_focus(id));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::hover());
    let painter = ui.painter().clone();
    if focused {
        painter.rect_stroke(
            rect.expand(2.0),
            8.0,
            Stroke::new(3.0, ACCENT_BG),
            egui::StrokeKind::Outside,
        );
    }
    painter.rect(
        rect,
        6.0,
        if focused { CARD } else { APP },
        Stroke::new(
            1.0,
            if invalid {
                DANGER
            } else if focused {
                FOCUS
            } else {
                BORDER_SOFT
            },
        ),
        egui::StrokeKind::Inside,
    );
    if let Some(color) = axis {
        painter.line_segment(
            [
                rect.left_bottom() + egui::vec2(3.0, -1.0),
                rect.right_bottom() + egui::vec2(-3.0, -1.0),
            ],
            Stroke::new(2.0, color),
        );
    }
    let suffix_width = suffix.map_or(0.0, |suffix| {
        painter
            .layout_no_wrap(suffix.into(), FontId::monospace(11.5), FAINT)
            .size()
            .x
            + 6.0
    });
    if let Some(suffix) = suffix {
        painter.text(
            rect.right_center() - egui::vec2(8.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            suffix,
            FontId::monospace(11.5),
            FAINT,
        );
    }
    let inner = egui::Rect::from_min_max(
        rect.min + egui::vec2(8.0, 0.0),
        rect.max - egui::vec2(8.0 + suffix_width, 0.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let response = child.add_enabled(
        enabled,
        egui::TextEdit::singleline(text)
            .id(id)
            .frame(egui::Frame::NONE)
            .font(FontId::monospace(12.5))
            .text_color(TEXT)
            .desired_width(inner.width()),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, enabled, accessible_name)
    });
    response
}

/// Read-only derived value (dashed `border_strong` stroke, no fill).
pub fn derived_field(ui: &mut Ui, value: &str, suffix: &str, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::hover());
    let painter = ui.painter();
    let r = rect.shrink(0.5);
    let dash = |a: egui::Pos2, b: egui::Pos2| {
        painter.extend(egui::Shape::dashed_line(
            &[a, b],
            Stroke::new(1.0, BORDER_STRONG),
            3.0,
            2.5,
        ));
    };
    dash(r.left_top() + egui::vec2(5.0, 0.0), r.right_top() - egui::vec2(5.0, 0.0));
    dash(r.left_bottom() + egui::vec2(5.0, 0.0), r.right_bottom() - egui::vec2(5.0, 0.0));
    dash(r.left_top() + egui::vec2(0.0, 5.0), r.left_bottom() - egui::vec2(0.0, 5.0));
    dash(r.right_top() + egui::vec2(0.0, 5.0), r.right_bottom() - egui::vec2(0.0, 5.0));
    painter.text(
        rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        value,
        FontId::monospace(12.5),
        SECONDARY,
    );
    painter.text(
        rect.right_center() - egui::vec2(8.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        suffix,
        FontId::proportional(11.0),
        FAINT,
    );
    response
}

/// Inspector section title: small uppercase tracked label with trailing slot.
pub fn inspector_heading<R>(ui: &mut Ui, label: &str, trailing: impl FnOnce(&mut Ui) -> R) -> R {
    ui.add_space(10.0);
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &label.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: weighted_font(
                ui,
                super::theme::SECTION_COMPACT.size,
                super::theme::Typeface::SansSemibold,
            ),
            color: FAINT,
            extra_letter_spacing: super::theme::SECTION_COMPACT.size
                * super::theme::SECTION_TRACKING_EM,
            ..Default::default()
        },
    );
    let r = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 22.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add(egui::Label::new(job).selectable(false));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing)
                    .inner
            },
        )
        .inner;
    ui.add_space(2.0);
    r
}

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, BORDER_SOFT))
        .corner_radius(9)
        .inner_margin(10)
}

pub fn section_header(ui: &mut Ui, label: &str) -> Response {
    let mut job = egui::text::LayoutJob::default();
    let font = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Name("Section".into()))
        .cloned()
        .unwrap_or_else(|| FontId::proportional(super::theme::SECTION.size));
    job.append(
        &label.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: font,
            color: FAINT,
            extra_letter_spacing: super::theme::SECTION.size * super::theme::SECTION_TRACKING_EM,
            ..Default::default()
        },
    );
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 36.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| ui.add(egui::Label::new(job)),
    )
    .inner
}

pub fn primary_button(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    let font = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Name("PrimaryButton".into()))
        .cloned()
        .unwrap_or_else(|| FontId::proportional(super::theme::PRIMARY_BUTTON.size));
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).font(font).color(PANEL))
            .fill(TEXT)
            .stroke(Stroke::NONE)
            .corner_radius(7)
            .min_size(egui::vec2(0.0, 30.0)),
    )
}

pub fn secondary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(medium(ui, text, 13.0).color(TEXT))
            .fill(VIEWPORT)
            .stroke(Stroke::new(1.0, BORDER_SOFT))
            .corner_radius(7)
            .min_size(egui::vec2(0.0, 30.0)),
    )
}

pub fn secondary_button_enabled(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(medium(ui, text, 13.0).color(TEXT))
            .fill(VIEWPORT)
            .stroke(Stroke::new(1.0, BORDER_SOFT))
            .corner_radius(7)
            .min_size(egui::vec2(0.0, 30.0)),
    )
}

pub fn chip(ui: &mut Ui, text: &str, fill: Color32, ink: Color32) -> Response {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(9)
        .inner_margin(egui::Margin::symmetric(7, 2))
        .show(ui, |ui| ui.label(RichText::new(text).size(11.0).color(ink)))
        .inner
}

/// Stable caller-owned ID and value; choosing a segment only changes view/draft
/// state. The caller is responsible for validation and domain commitment.
pub fn segmented<T: Copy + PartialEq>(
    ui: &mut Ui,
    value: &mut T,
    options: &[(T, &str)],
) -> Response {
    egui::Frame::new()
        .fill(VIEWPORT)
        .corner_radius(7)
        .inner_margin(2)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let mut union: Option<Response> = None;
                for &(candidate, label) in options {
                    let selected = *value == candidate;
                    let text = if selected {
                        medium(ui, label, 12.5).color(TEXT)
                    } else {
                        RichText::new(label).size(12.5).color(SECONDARY)
                    };
                    let r = ui.add(
                        egui::Button::new(text)
                            .selected(selected)
                            .min_size(egui::vec2(0.0, 24.0))
                            .fill(if selected {
                                PANEL
                            } else {
                                Color32::TRANSPARENT
                            })
                            .stroke(Stroke::NONE)
                            .corner_radius(5),
                    );
                    if r.clicked() {
                        *value = candidate;
                    }
                    union = Some(union.map_or_else(|| r.clone(), |a| a.union(r.clone())));
                }
                union
                    .unwrap_or_else(|| ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover()))
            })
            .inner
        })
        .inner
}

/// Edits text only. Unit parsing, precision consent and commit/cancel belong to
/// the shared draft controller, not this presentation helper.
pub fn unit_field(
    ui: &mut Ui,
    id: egui::Id,
    label: &str,
    text: &mut String,
    unit: &str,
    error: Option<&str>,
) -> Response {
    let focused = ui.memory(|m| m.has_focus(id));
    let stroke = Stroke::new(
        1.0,
        if error.is_some() {
            DANGER
        } else if focused {
            FOCUS
        } else {
            BORDER_SOFT
        },
    );
    let frame = egui::Frame::new()
        .fill(if focused { Color32::WHITE } else { APP })
        .stroke(stroke)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 4));
    let shown = frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            let unit_width = ui
                .painter()
                .layout_no_wrap(unit.into(), FontId::proportional(12.0), FAINT)
                .size()
                .x;
            let width = (ui.available_width() - unit_width - 6.0).max(24.0);
            let response = ui.add(
                egui::TextEdit::singleline(text)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .font(FontId::monospace(13.0))
                    .desired_width(width),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, ui.is_enabled(), label)
            });
            ui.label(RichText::new(unit).size(12.0).color(FAINT));
            response
        })
        .inner
    });
    if focused {
        ui.painter().rect_stroke(
            shown.response.rect.expand(2.0),
            8,
            Stroke::new(3.0, ACCENT_BG),
            egui::StrokeKind::Outside,
        );
    }
    if let Some(error) = error {
        ui.label(RichText::new(error).size(11.5).color(DANGER));
    }
    shown.inner
}

pub fn icon_button(
    ui: &mut Ui,
    image: egui::Image<'_>,
    accessible_name: &str,
    selected: bool,
    enabled: bool,
) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::image(image)
            .selected(selected)
            .min_size(egui::vec2(32.0, 32.0))
            .corner_radius(7),
    )
    .on_hover_text(accessible_name)
    .tap_widget_label(accessible_name)
}

trait LabelResponse {
    fn tap_widget_label(self, name: &str) -> Self;
}
impl LabelResponse for Response {
    fn tap_widget_label(self, name: &str) -> Self {
        self.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, self.enabled(), name)
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_tokens_are_exact_and_repeatable() {
        let ctx = egui::Context::default();
        apply_visuals(&ctx);
        let a = ctx.style_of(egui::Theme::Light);
        assert_eq!(a.visuals.panel_fill.to_array(), [251, 250, 247, 255]);
        assert_eq!(a.visuals.selection.bg_fill.to_array(), [246, 227, 203, 255]);
        assert_eq!(a.spacing.interact_size.y, 28.0);
        apply_visuals(&ctx);
        assert_eq!(
            ctx.style_of(egui::Theme::Light).visuals.panel_fill,
            a.visuals.panel_fill
        );
    }

    #[test]
    fn icon_action_has_name_focus_and_keyboard_activation() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::icons::install_loaders(&ctx);
        let mut first = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = icon_button(
                ui,
                crate::icons::icon(crate::icons::Icon::Save, SECONDARY, 15.0)
                    .alt_text("Save project"),
                "Save project",
                false,
                true,
            );
            response.request_focus();
            assert!(response.has_focus());
        });
        assert!(
            first
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(_, node)| node.role() == egui::accesskit::Role::Button
                    && node.label() == Some("Save project"))
        );
        first.textures_delta.clear();
        let mut clicked = false;
        let mut next = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                clicked = icon_button(
                    ui,
                    crate::icons::icon(crate::icons::Icon::Save, SECONDARY, 15.0)
                        .alt_text("Save project"),
                    "Save project",
                    false,
                    true,
                )
                .clicked()
            },
        );
        assert!(clicked, "focused native icon button activates with Enter");
        next.textures_delta.clear();
    }
}
