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
            .corner_radius(7),
    )
}

pub fn secondary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(text)
            .fill(VIEWPORT)
            .stroke(Stroke::new(1.0, BORDER_SOFT))
            .corner_radius(7),
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
                    let r = ui.add(
                        egui::Button::new(label)
                            .selected(selected)
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
