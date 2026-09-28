//! Shared chrome for native `egui::Modal` dialogs.
//!
//! Keep `Modal` itself: its layer and backdrop establish the pointer and focus
//! boundary. Call `show` each frame while a dialog is active, and call `close`
//! only when the owning workflow has actually accepted/cancelled the action.
//!
//! Every dialog has one look: a header (34×34 icon tile, 15pt title, optional
//! one-line context, `Esc` keycap), a padded body and a footer on `bg_app` with
//! an optional faint hint on the left, "Cancel" and a primary button named for
//! the action with a `⏎` hint. The [`form`] helpers draw the body fields.
use eframe::egui::{self, Color32, CornerRadius, Frame, Id, Margin, RichText, Stroke};

use crate::theme::Typeface;
use crate::theme_widgets as colors;

/// Icon ink on the `accent_bg` tile.
pub const ICON_INK: Color32 = Color32::from_rgb(154, 91, 18);

/// Caller-supplied, localized verb labels (e.g. "Create board" / "Criar peça",
/// "Delete" / "Excluir"). No action text is inferred from the dialog title.
pub struct ModalActions<'a> {
    pub cancel: &'a str,
    pub confirm: &'a str,
}

/// Localized decisions for prompts which must distinguish leaving an edit
/// (Apply/Discard/Stay or Save/Discard/Cancel) from remaining in place.
/// The owner resolves the returned decision; chrome never commits a draft.
/// `secondary` is drawn as danger text on the left of the footer.
#[derive(Clone, Copy)]
pub struct ModalThreeActions<'a> {
    pub primary: &'a str,
    pub secondary: &'a str,
    pub stay: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalThreeAction {
    None,
    Primary,
    Secondary,
    Stay,
}

pub struct ModalThreeResult<T> {
    pub body: T,
    pub action: ModalThreeAction,
}

enum Footer<'a> {
    Reader(&'a str),
    Two(ModalActions<'a>),
    Three(ModalThreeActions<'a>),
}

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalAction {
    None,
    Cancel,
    Confirm,
}

pub struct ModalResult<T> {
    pub body: T,
    pub action: ModalAction,
}

/// One controller per dialog identity, retained with its draft across frames.
/// Nesting works by keeping the parent controller/draft alive while showing a
/// child with a different ID; closing the child restores its invoking widget.
pub struct ModalChrome {
    id: Id,
    width: f32,
    icon: crate::icons::Icon,
    first_focus: Option<Id>,
    visible: bool,
    invoking_focus: Option<Id>,
    last_focus: Option<Id>,
    context: Option<String>,
    hint: Option<String>,
    primary_hint: Option<String>,
    alert: bool,
}

/// Measured size of a footer button label.
fn text_width(ui: &egui::Ui, text: &str, font: egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, colors::TEXT)
        .size()
        .x
}

fn button_font(ui: &egui::Ui) -> egui::FontId {
    colors::weighted_font(ui, 13.0, Typeface::SansMedium)
}

const BUTTON_PAD: f32 = 14.0;
const BUTTON_HEIGHT: f32 = 32.0;

fn secondary_width(ui: &egui::Ui, text: &str) -> f32 {
    text_width(ui, text, button_font(ui)) + 2.0 * BUTTON_PAD + 2.0
}

fn primary_width(ui: &egui::Ui, text: &str, hint: &str) -> f32 {
    let hint_width = if hint.is_empty() {
        0.0
    } else {
        text_width(ui, hint, egui::FontId::monospace(11.0)) + 8.0
    };
    text_width(ui, text, button_font(ui)) + 2.0 * BUTTON_PAD + hint_width + 2.0
}

/// Secondary footer button (`bg_viewport` fill, `border` stroke, 32 high).
pub fn footer_secondary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let width = secondary_width(ui, text);
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .font(button_font(ui))
                .color(colors::TEXT),
        )
        .fill(colors::VIEWPORT)
        .stroke(Stroke::new(1.0, colors::BORDER))
        .corner_radius(7)
        .min_size(egui::vec2(width, BUTTON_HEIGHT)),
    )
}

/// Primary footer button named for the action, with a faint key hint (`⏎`,
/// `⌘S`, `Esc`). The accessible name is the action text only.
pub fn footer_primary(ui: &mut egui::Ui, text: &str, hint: &str, enabled: bool) -> egui::Response {
    let width = primary_width(ui, text, hint);
    let mut button = egui::Button::new(
        RichText::new(text)
            .font(button_font(ui))
            .color(colors::PANEL),
    )
    .fill(colors::TEXT)
    .stroke(Stroke::NONE)
    .corner_radius(7)
    .min_size(egui::vec2(width, BUTTON_HEIGHT));
    if !hint.is_empty() {
        button = button.right_text(
            RichText::new(hint)
                .font(egui::FontId::monospace(11.0))
                .color(colors::PANEL.gamma_multiply(0.6)),
        );
    }
    let response = ui.add_enabled(enabled, button);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, text));
    response
}

/// Destructive text action (e.g. "Discard") on the left of a footer.
pub fn footer_danger(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .font(button_font(ui))
                .color(colors::DANGER),
        )
        .frame_when_inactive(false)
        .corner_radius(7)
        .min_size(egui::vec2(0.0, BUTTON_HEIGHT)),
    )
}

/// The decorative `Esc` keycap shown in a dialog header.
fn esc_keycap(ui: &mut egui::Ui) {
    let galley = ui.painter().layout_no_wrap(
        "Esc".to_owned(),
        egui::FontId::monospace(11.0),
        colors::FAINT,
    );
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 12.0, galley.size().y + 4.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, colors::BORDER_SOFT),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, colors::FAINT);
}

/// 34×34 (or `size`) rounded `accent_bg` tile with the dialog icon.
pub fn icon_tile(ui: &mut egui::Ui, icon: crate::icons::Icon, size: f32) {
    let (tile, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter()
        .rect_filled(tile, if size > 36.0 { 9.0 } else { 8.0 }, colors::ACCENT_BG);
    let icon_size = (size * 0.5).round();
    crate::icons::icon(icon, ICON_INK, icon_size).paint_at(
        ui,
        egui::Rect::from_center_size(tile.center(), egui::Vec2::splat(icon_size)),
    );
}

impl ModalChrome {
    /// Moves the controller out of a draft for the duration of `show`, whose
    /// body borrows the rest of the draft. Put it back afterwards; the inert
    /// placeholder left behind is never shown.
    pub fn detach(&mut self) -> Self {
        std::mem::replace(self, Self::new(Id::NULL))
    }

    pub fn new(id: Id) -> Self {
        Self {
            id,
            width: 480.0,
            icon: crate::icons::Icon::Sliders,
            first_focus: None,
            visible: false,
            invoking_focus: None,
            last_focus: None,
            context: None,
            hint: None,
            primary_hint: None,
            alert: false,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn icon(mut self, icon: crate::icons::Icon) -> Self {
        self.icon = icon;
        self
    }

    /// Compact alert layout (no header row): icon tile, a large question as
    /// the title, the body and the footer. Used by "Save changes to …?".
    pub fn alert(mut self) -> Self {
        self.alert = true;
        self
    }

    /// Switch the alert layout per frame for a chrome shared by prompt kinds.
    pub fn set_alert(&mut self, alert: bool) {
        self.alert = alert;
    }

    /// Change the header icon for a dialog whose kind varies per draft.
    pub fn set_icon(&mut self, icon: crate::icons::Icon) {
        self.icon = icon;
    }

    /// One-line 12pt context under the title ("Left side, Right side").
    /// Set it each frame before `show`; `None` hides it.
    pub fn set_context(&mut self, context: Option<String>) {
        self.context = context;
    }

    /// Faint 11.5pt hint on the left of the footer (one short line).
    pub fn set_hint(&mut self, hint: Option<String>) {
        self.hint = hint;
    }

    /// Key hint painted inside the primary button (`⏎` by default).
    pub fn set_primary_hint(&mut self, hint: Option<String>) {
        self.primary_hint = hint;
    }

    /// ID of the first applicable field/action. The body must render a widget
    /// with this ID (e.g. `TextEdit::id`). Omit for button-only prompts.
    pub fn first_focus(mut self, id: Id) -> Self {
        self.first_focus = Some(id);
        self
    }

    /// Also guard any application code that reads raw `ctx.input` directly
    /// (viewport camera, Delete, undo/redo and project shortcuts) while active.
    /// egui's modal layer blocks widgets, not arbitrary application listeners.
    pub fn is_active(&self) -> bool {
        self.visible
    }

    /// An informational child has one Back/Done action, but shares the same
    /// scroll, popup isolation, initial focus and return-focus lifecycle.
    pub fn show_reader<T>(
        &mut self,
        ctx: &egui::Context,
        title: &str,
        back: &str,
        body: impl FnOnce(&mut egui::Ui) -> T,
    ) -> ModalResult<T> {
        let result =
            self.show_with_footer(ctx, title, Footer::Reader(back), |ui| (body(ui), false));
        ModalResult {
            body: result.body,
            action: if result.action == ModalThreeAction::None {
                ModalAction::None
            } else {
                ModalAction::Cancel
            },
        }
    }

    /// The body returns its result and whether confirmation is currently valid.
    /// Disabled confirmation cannot be activated by button or Enter. `body`
    /// remains scrollable independently of the title and footer.
    pub fn show<T>(
        &mut self,
        ctx: &egui::Context,
        title: &str,
        actions: ModalActions<'_>,
        body: impl FnOnce(&mut egui::Ui) -> (T, bool),
    ) -> ModalResult<T> {
        let result = self.show_with_footer(ctx, title, Footer::Two(actions), body);
        ModalResult {
            body: result.body,
            action: match result.action {
                ModalThreeAction::None => ModalAction::None,
                ModalThreeAction::Primary => ModalAction::Confirm,
                ModalThreeAction::Secondary | ModalThreeAction::Stay => ModalAction::Cancel,
            },
        }
    }

    /// Show a three-decision prompt. `primary` is the guarded Apply/Save
    /// action, `secondary` is Discard (danger text on the left, which may
    /// continue navigation), and `stay` (Cancel/Stay) keeps the edit and
    /// location. Enter chooses a valid primary; Escape chooses Stay. Call
    /// `close` only after the owner resolves a choice.
    pub fn show_three<T>(
        &mut self,
        ctx: &egui::Context,
        title: &str,
        actions: ModalThreeActions<'_>,
        body: impl FnOnce(&mut egui::Ui) -> (T, bool),
    ) -> ModalThreeResult<T> {
        self.show_with_footer(ctx, title, Footer::Three(actions), body)
    }

    fn header(&self, ui: &mut egui::Ui, title: &str, inner_width: f32) {
        let header = Frame::new()
            .inner_margin(Margin::symmetric(18, 16))
            .show(ui, |ui| {
                ui.set_width(inner_width);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    icon_tile(ui, self.icon, 34.0);
                    let keycap_width = 36.0;
                    let text_width = (ui.available_width() - keycap_width - 12.0).max(1.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(text_width, 34.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(text_width);
                            ui.spacing_mut().item_spacing.y = 1.0;
                            if self.context.is_none() {
                                ui.add_space(6.0);
                            }
                            ui.add(
                                egui::Label::new(
                                    colors::semibold(ui, title, 15.0).color(colors::TEXT),
                                )
                                .wrap()
                                .selectable(false),
                            );
                            if let Some(context) = &self.context {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(context).size(12.0).color(colors::MUTED),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                            }
                        },
                    );
                    esc_keycap(ui);
                });
            });
        let rect = header.response.rect;
        ui.painter().hline(
            ui.max_rect().x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, colors::BORDER_SOFT),
        );
    }

    fn show_with_footer<T>(
        &mut self,
        ctx: &egui::Context,
        title: &str,
        footer: Footer<'_>,
        body: impl FnOnce(&mut egui::Ui) -> (T, bool),
    ) -> ModalThreeResult<T> {
        let opening = !self.visible;
        if opening {
            self.invoking_focus = ctx.memory(|m| m.focused());
            self.visible = true;
        }
        // Inspect before the body: a child popup may close during this frame.
        let popup_before = egui::Popup::is_any_open(ctx);
        let width = self.width.min((ctx.content_rect().width() - 24.0).max(1.0));
        let pad_x: i8 = if self.alert { 20 } else { 18 };
        let inner_width = (width - 2.0 * f32::from(pad_x)).max(1.0);
        // Leave room for a wrapped title and localized actions (up to three
        // stacked rows in the compact three-decision variant).
        let three_actions = matches!(footer, Footer::Three(_));
        let body_height =
            (ctx.content_rect().height() - if three_actions { 230.0 } else { 190.0 }).max(1.0);
        let primary_hint = self.primary_hint.clone().unwrap_or_else(|| "⏎".to_owned());
        let modal = egui::Modal::new(self.id)
            .backdrop_color(Color32::from_rgba_unmultiplied(42, 37, 32, 64))
            .frame(
                Frame::new()
                    .fill(colors::PANEL)
                    .stroke(Stroke::new(1.0, colors::BORDER))
                    .corner_radius(12)
                    .inner_margin(Margin::ZERO)
                    .shadow(egui::Shadow {
                        offset: [0, 18],
                        blur: 44,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(60, 45, 25, 46),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                let spacing = ui.spacing().item_spacing;
                ui.spacing_mut().item_spacing.y = 0.0;
                if !self.alert {
                    self.header(ui, title, inner_width);
                }
                let (result, valid) = Frame::new()
                    .inner_margin(if self.alert {
                        Margin {
                            left: 20,
                            right: 20,
                            top: 20,
                            bottom: 16,
                        }
                    } else {
                        Margin::symmetric(18, 16)
                    })
                    .show(ui, |ui| {
                        ui.set_width(inner_width);
                        ui.spacing_mut().item_spacing = spacing;
                        if self.alert {
                            icon_tile(ui, self.icon, 38.0);
                            ui.add_space(10.0);
                            ui.add(
                                egui::Label::new(
                                    colors::semibold(ui, title, 16.0).color(colors::TEXT),
                                )
                                .wrap()
                                .selectable(false),
                            );
                            ui.add_space(6.0);
                        }
                        egui::ScrollArea::vertical()
                            .id_salt(self.id.with("body"))
                            .max_height(body_height)
                            .show(ui, |ui| {
                                ui.set_width(inner_width);
                                body(ui)
                            })
                            .inner
                    })
                    .inner;
                let footer_frame = Frame::new()
                    .fill(colors::APP)
                    .corner_radius(CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 11,
                        se: 11,
                    })
                    .inner_margin(Margin::symmetric(pad_x, 12))
                    .show(ui, |ui| {
                        ui.set_width(inner_width);
                        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                        ui.spacing_mut().button_padding = egui::vec2(BUTTON_PAD, 4.0);
                        self.footer(ui, footer, valid, &primary_hint, inner_width)
                    });
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    footer_frame.response.rect.top() + 0.5,
                    Stroke::new(1.0, colors::BORDER_SOFT),
                );
                let (secondary, primary, stay, initial_focus_id) = footer_frame.inner;
                (result, valid, secondary, primary, stay, initial_focus_id)
            });
        if opening {
            ctx.memory_mut(|m| m.request_focus(self.first_focus.unwrap_or(modal.inner.5)));
        } else if !popup_before
            && ctx.memory(|m| m.focused()).is_none()
            && let Some(id) = self.last_focus
        {
            // A suspended parent field can be dropped by egui's focus dead-man
            // switch on the frame after its child closes; re-establish it.
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if !popup_before {
            self.last_focus = ctx.memory(|m| m.focused());
        }
        let popup_active = popup_before || modal.any_popup_open || egui::Popup::is_any_open(ctx);
        let top = modal.is_top_modal || opening && ctx.memory(|m| m.top_modal_layer().is_none());
        let should_close = top && !popup_active && modal.should_close();
        let (body, valid, secondary, primary, stay, _) = modal.inner;
        let action = if !top || popup_active {
            ModalThreeAction::None
        } else if secondary {
            ModalThreeAction::Secondary
        } else if primary && valid {
            ModalThreeAction::Primary
        } else if stay || should_close {
            ModalThreeAction::Stay
        } else if !popup_active
            && valid
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            ModalThreeAction::Primary
        } else {
            if three_actions && !popup_active {
                // Invalid primary is not an invitation for a raw-input listener
                // behind the prompt to interpret this Enter as a scene action.
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            }
            ModalThreeAction::None
        };
        ModalThreeResult { body, action }
    }

    /// Returns (secondary clicked, primary clicked, stay clicked, initial focus).
    fn footer(
        &self,
        ui: &mut egui::Ui,
        footer: Footer<'_>,
        valid: bool,
        primary_hint: &str,
        width: f32,
    ) -> (bool, bool, bool, Id) {
        let hint_label = |ui: &mut egui::Ui, hint: &str, hint_width: f32| {
            ui.allocate_ui_with_layout(
                egui::vec2(hint_width, BUTTON_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_width(hint_width);
                    ui.add(
                        egui::Label::new(RichText::new(hint).size(11.5).color(colors::FAINT))
                            .wrap()
                            .selectable(false),
                    );
                },
            );
        };
        match footer {
            Footer::Reader(label) => {
                let back_width = secondary_width(ui, label);
                ui.horizontal(|ui| {
                    let lead = (width - back_width - 9.0).max(0.0);
                    match &self.hint {
                        Some(hint) if lead > 150.0 => {
                            hint_label(ui, hint, lead);
                        }
                        _ => ui.add_space(lead + 8.0),
                    }
                    let back = footer_secondary(ui, label);
                    (back.clicked(), false, false, back.id)
                })
                .inner
            }
            Footer::Two(actions) => {
                let buttons = secondary_width(ui, actions.cancel)
                    + 8.0
                    + primary_width(ui, actions.confirm, primary_hint);
                if buttons <= width {
                    ui.horizontal(|ui| {
                        let lead = (width - buttons - 9.0).max(0.0);
                        match &self.hint {
                            Some(hint) if lead > 150.0 => hint_label(ui, hint, lead),
                            _ => ui.add_space(lead + 8.0),
                        }
                        let cancel = footer_secondary(ui, actions.cancel);
                        let confirm = footer_primary(ui, actions.confirm, primary_hint, valid);
                        (cancel.clicked(), confirm.clicked(), false, cancel.id)
                    })
                    .inner
                } else {
                    // Long localized verbs at compact widths wrap instead of clipping.
                    ui.vertical(|ui| {
                        let result = ui
                            .horizontal_wrapped(|ui| {
                                let cancel = ui.add(
                                    egui::Button::new(
                                        RichText::new(actions.cancel).color(colors::TEXT),
                                    )
                                    .wrap()
                                    .fill(colors::VIEWPORT)
                                    .stroke(Stroke::new(1.0, colors::BORDER))
                                    .corner_radius(7)
                                    .min_size(egui::vec2(0.0, BUTTON_HEIGHT)),
                                );
                                let confirm = ui.add_enabled(
                                    valid,
                                    egui::Button::new(
                                        RichText::new(actions.confirm).color(colors::PANEL),
                                    )
                                    .wrap()
                                    .fill(colors::TEXT)
                                    .stroke(Stroke::NONE)
                                    .corner_radius(7)
                                    .min_size(egui::vec2(0.0, BUTTON_HEIGHT)),
                                );
                                (cancel.clicked(), confirm.clicked(), false, cancel.id)
                            })
                            .inner;
                        if let Some(hint) = &self.hint {
                            hint_label(ui, hint, width);
                        }
                        result
                    })
                    .inner
                }
            }
            Footer::Three(actions) => {
                let danger = text_width(ui, actions.secondary, button_font(ui)) + 24.0;
                let stay_width = secondary_width(ui, actions.stay);
                let primary_width = primary_width(ui, actions.primary, primary_hint);
                if danger + stay_width + primary_width + 24.0 <= width {
                    let (row, _) = ui.allocate_exact_size(
                        egui::vec2(width, BUTTON_HEIGHT),
                        egui::Sense::hover(),
                    );
                    let primary_rect = egui::Rect::from_min_size(
                        egui::pos2(row.right() - primary_width, row.top()),
                        egui::vec2(primary_width, BUTTON_HEIGHT),
                    );
                    let stay_rect = egui::Rect::from_min_size(
                        egui::pos2(primary_rect.left() - 8.0 - stay_width, row.top()),
                        egui::vec2(stay_width, BUTTON_HEIGHT),
                    );
                    let danger_rect = egui::Rect::from_min_size(
                        egui::pos2(row.left() - 6.0, row.top()),
                        egui::vec2(danger, BUTTON_HEIGHT),
                    );
                    // Registration order is the Tab order: Stay, Discard, primary.
                    let mut child = |rect: egui::Rect| {
                        ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(rect)
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        )
                    };
                    let stay = footer_secondary(&mut child(stay_rect), actions.stay);
                    let secondary = footer_danger(&mut child(danger_rect), actions.secondary);
                    let primary = footer_primary(
                        &mut child(primary_rect),
                        actions.primary,
                        primary_hint,
                        valid,
                    );
                    (
                        secondary.clicked(),
                        primary.clicked(),
                        stay.clicked(),
                        if valid { primary.id } else { stay.id },
                    )
                } else {
                    // Stack at compact widths so long pt-BR verbs stay reachable.
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        let stay = ui.add(egui::Button::new(actions.stay).wrap());
                        let secondary = ui.add(
                            egui::Button::new(
                                RichText::new(actions.secondary).color(colors::DANGER),
                            )
                            .wrap(),
                        );
                        let primary = ui.add_enabled(
                            valid,
                            egui::Button::new(RichText::new(actions.primary).color(colors::PANEL))
                                .fill(colors::TEXT)
                                .stroke(Stroke::NONE)
                                .corner_radius(7)
                                .wrap(),
                        );
                        (
                            secondary.clicked(),
                            primary.clicked(),
                            stay.clicked(),
                            if valid { primary.id } else { stay.id },
                        )
                    })
                    .inner
                }
            }
        }
    }

    /// Call after the owner resolves an action successfully; a failed domain
    /// validation can keep the draft and its focus by simply continuing `show`.
    pub fn close(&mut self, ctx: &egui::Context) {
        if self.visible {
            if let Some(id) = self.invoking_focus.take() {
                ctx.memory_mut(|m| m.request_focus(id));
            } else {
                ctx.memory_mut(|m| m.stop_text_input());
            }
            self.visible = false;
            self.last_focus = None;
        }
    }

    /// A wizard may replace its entire body while keeping the same modal. Do
    /// not let the next frame restore a TextEdit ID that no longer exists:
    /// macOS AccessKit requires its focused ID to be in the new node tree.
    pub fn body_replaced(&mut self, ctx: &egui::Context) {
        self.last_focus = None;
        ctx.memory_mut(|m| m.stop_text_input());
    }
}

/// Body building blocks shared by every dialog: 12pt muted labels above
/// fields, 34-high inputs with a unit suffix, full-width segmented controls,
/// live parse lines, preview strips and selectable radio cards.
pub mod form {
    use super::*;

    /// Vertical gap between field groups (14 with the default item spacing).
    pub fn gap(ui: &mut egui::Ui) {
        ui.add_space(10.0);
    }

    /// 12pt `muted` label above a field.
    pub fn label(ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.add(
            egui::Label::new(RichText::new(text).size(12.0).color(colors::MUTED))
                .wrap()
                .selectable(false),
        )
    }

    /// Label with a faint right-aligned note ("current: mixed (720, 764)").
    pub fn label_with_note(ui: &mut egui::Ui, text: &str, note: &str) {
        ui.horizontal(|ui| {
            ui.add(
                egui::Label::new(RichText::new(text).size(12.0).color(colors::MUTED))
                    .selectable(false),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(RichText::new(note).size(11.5).color(colors::FAINT))
                        .truncate()
                        .selectable(false),
                );
            });
        });
    }

    /// A labelled group: label, 6pt, content.
    pub fn field<R>(ui: &mut egui::Ui, text: &str, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            label(ui, text);
            content(ui)
        })
        .inner
    }

    /// One faint 11.5pt line (hints, units accepted, scope notes).
    pub fn hint(ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.add(
            egui::Label::new(RichText::new(text).size(11.5).color(colors::FAINT))
                .wrap()
                .selectable(false),
        )
    }

    /// 11.5pt muted explanatory line (result lines, derived readouts).
    pub fn note(ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.add(
            egui::Label::new(RichText::new(text).size(11.5).color(colors::MUTED))
                .wrap()
                .selectable(false),
        )
    }

    /// Live parse line under a unit expression (`= 539.750 mm exact`).
    pub fn parse_ok(ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.add(
            egui::Label::new(
                RichText::new(text)
                    .font(egui::FontId::monospace(11.0))
                    .color(colors::OK),
            )
            .selectable(false),
        )
    }

    /// Inline validation message (11.5pt `danger`).
    pub fn error(ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.add(
            egui::Label::new(RichText::new(text).size(11.5).color(colors::DANGER))
                .wrap()
                .selectable(false),
        )
    }

    /// Warning line (11.5pt `warn_ink`) for prospective allocation conflicts.
    pub fn warning(ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.add(
            egui::Label::new(RichText::new(text).size(11.5).color(colors::WARN_INK))
                .wrap()
                .selectable(false),
        )
    }

    pub struct Input<'a> {
        pub id: Id,
        pub accessible_name: &'a str,
        pub width: f32,
        pub mono: bool,
        pub prefix: Option<(&'a str, Color32)>,
        pub suffix: Option<&'a str>,
        pub placeholder: Option<&'a str>,
        pub invalid: bool,
        pub enabled: bool,
    }

    impl<'a> Input<'a> {
        pub fn new(id: impl Into<Id>, accessible_name: &'a str, width: f32) -> Self {
            Self {
                id: id.into(),
                accessible_name,
                width,
                mono: true,
                prefix: None,
                suffix: None,
                placeholder: None,
                invalid: false,
                enabled: true,
            }
        }

        /// Proportional text (names) instead of mono values.
        pub fn text(mut self) -> Self {
            self.mono = false;
            self
        }

        pub fn suffix(mut self, suffix: &'a str) -> Self {
            self.suffix = Some(suffix);
            self
        }

        /// Coloured axis letter before the value (X red, Y green, Z blue).
        pub fn axis(mut self, axis: usize) -> Self {
            self.prefix = Some((["X", "Y", "Z"][axis.min(2)], axis_color(axis)));
            self
        }

        pub fn prefix(mut self, text: &'a str, color: Color32) -> Self {
            self.prefix = Some((text, color));
            self
        }

        /// Faint placeholder shown while the field is empty ("unknown").
        #[allow(dead_code)] // Offered to dialog bodies owned by other workspaces.
        pub fn placeholder(mut self, text: &'a str) -> Self {
            self.placeholder = Some(text);
            self
        }

        pub fn invalid(mut self, invalid: bool) -> Self {
            self.invalid = invalid;
            self
        }

        pub fn show(self, ui: &mut egui::Ui, text: &mut String) -> egui::Response {
            input(ui, self, text)
        }
    }

    pub fn axis_color(axis: usize) -> Color32 {
        match axis {
            0 => colors::KERF,
            1 => Color32::from_rgb(78, 154, 87),
            _ => Color32::from_rgb(62, 111, 196),
        }
    }

    /// 34-high input: `bg_app` fill, soft stroke; focused = white with the
    /// `focus_stroke` and a 3pt `accent_bg` ring; invalid = danger stroke.
    fn input(ui: &mut egui::Ui, spec: Input<'_>, text: &mut String) -> egui::Response {
        let focused = ui.memory(|m| m.has_focus(spec.id));
        let width = spec.width.min(ui.available_width()).max(40.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::hover());
        let painter = ui.painter().clone();
        if focused {
            painter.rect_stroke(
                rect.expand(1.5),
                9.0,
                Stroke::new(3.0, colors::ACCENT_BG),
                egui::StrokeKind::Outside,
            );
        }
        painter.rect(
            rect,
            7.0,
            if focused { colors::CARD } else { colors::APP },
            Stroke::new(
                1.0,
                if spec.invalid {
                    colors::DANGER
                } else if focused {
                    colors::FOCUS
                } else {
                    colors::BORDER_SOFT
                },
            ),
            egui::StrokeKind::Inside,
        );
        let mut left = rect.left() + 10.0;
        if let Some((prefix, color)) = spec.prefix {
            let galley = painter.layout_no_wrap(
                prefix.to_owned(),
                colors::weighted_font(ui, 11.0, Typeface::SansSemibold),
                color,
            );
            painter.galley(
                egui::pos2(left, rect.center().y - galley.size().y / 2.0),
                galley.clone(),
                color,
            );
            left += galley.size().x + 8.0;
        }
        let mut right = rect.right() - 10.0;
        if let Some(suffix) = spec.suffix {
            let galley = painter.layout_no_wrap(
                suffix.to_owned(),
                egui::FontId::proportional(12.0),
                colors::FAINT,
            );
            painter.galley(
                egui::pos2(
                    right - galley.size().x,
                    rect.center().y - galley.size().y / 2.0,
                ),
                galley.clone(),
                colors::FAINT,
            );
            right -= galley.size().x + 6.0;
        }
        let inner = egui::Rect::from_min_max(
            egui::pos2(left, rect.top()),
            egui::pos2(right.max(left + 8.0), rect.bottom()),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let font = if spec.mono {
            egui::FontId::monospace(13.0)
        } else {
            egui::FontId::proportional(13.0)
        };
        let mut edit = egui::TextEdit::singleline(text)
            .id(spec.id)
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .font(font)
            .text_color(colors::TEXT)
            .desired_width(inner.width());
        if let Some(placeholder) = spec.placeholder {
            edit = edit.hint_text(RichText::new(placeholder).color(colors::DISABLED));
        }
        let response = child.add_enabled(spec.enabled, edit);
        let name = spec.accessible_name.to_owned();
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, spec.enabled, &name)
        });
        response
    }

    /// Full-width segmented control with near-equal segments (wider for long
    /// labels). Each segment is a focusable, accessible toggle button.
    pub fn segmented<T: Copy + PartialEq>(
        ui: &mut egui::Ui,
        id: impl std::hash::Hash + std::fmt::Debug,
        value: &mut T,
        options: &[(T, &str)],
        width: f32,
    ) -> egui::Response {
        segmented_sized(ui, id, value, options, width, 30.0, false)
    }

    /// As [`segmented`] with a mono font (e.g. "90% 100% 115% 130%").
    pub fn segmented_sized<T: Copy + PartialEq>(
        ui: &mut egui::Ui,
        id: impl std::hash::Hash + std::fmt::Debug,
        value: &mut T,
        options: &[(T, &str)],
        width: f32,
        height: f32,
        mono: bool,
    ) -> egui::Response {
        let width = width.min(ui.available_width()).max(40.0);
        let (rect, mut union) =
            ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        ui.painter().rect_filled(rect, 7.0, colors::VIEWPORT);
        let font = |ui: &egui::Ui, selected: bool| {
            if mono {
                egui::FontId::monospace(12.0)
            } else if selected {
                colors::weighted_font(ui, 12.5, Typeface::SansMedium)
            } else {
                egui::FontId::proportional(12.5)
            }
        };
        let naturals: Vec<f32> = options
            .iter()
            .map(|(_, label)| text_width(ui, label, font(ui, true)) + 20.0)
            .collect();
        let available = width - 4.0 - 2.0 * (options.len().saturating_sub(1)) as f32;
        let total: f32 = naturals.iter().sum();
        let widths: Vec<f32> = if total <= available {
            let extra = (available - total) / options.len().max(1) as f32;
            naturals.iter().map(|w| w + extra).collect()
        } else {
            naturals.iter().map(|w| w * available / total).collect()
        };
        let base = Id::new(("form-segmented", ui.id(), id));
        let mut x = rect.left() + 2.0;
        for (index, ((candidate, label), segment_width)) in options.iter().zip(widths).enumerate() {
            let segment = egui::Rect::from_min_size(
                egui::pos2(x, rect.top() + 2.0),
                egui::vec2(segment_width, height - 4.0),
            );
            x += segment_width + 2.0;
            let selected = *value == *candidate;
            let response = ui.interact(segment, base.with(index), egui::Sense::click());
            let name = (*label).to_owned();
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, &name)
            });
            if response.clicked() {
                *value = *candidate;
            }
            let painter = ui.painter();
            if selected {
                painter.rect_filled(
                    segment.translate(egui::vec2(0.0, 1.0)),
                    5.0,
                    Color32::from_rgba_unmultiplied(60, 45, 25, 26),
                );
                painter.rect_filled(segment, 5.0, colors::PANEL);
            } else if response.hovered() {
                painter.rect_filled(
                    segment,
                    5.0,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 90),
                );
            }
            if response.has_focus() {
                painter.rect_stroke(
                    segment,
                    5.0,
                    Stroke::new(1.0, colors::FOCUS),
                    egui::StrokeKind::Inside,
                );
            }
            let color = if selected {
                colors::TEXT
            } else {
                colors::SECONDARY
            };
            let galley = painter.layout(
                (*label).to_owned(),
                font(ui, selected),
                color,
                segment_width - 6.0,
            );
            painter.galley(segment.center() - galley.size() / 2.0, galley, color);
            union = union.union(response);
        }
        union
    }

    /// 34-high read-only derived value with a dashed `border_strong` outline.
    pub fn derived(ui: &mut egui::Ui, value: &str, suffix: &str, width: f32) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::hover());
        let painter = ui.painter();
        let r = rect.shrink(0.5);
        let stroke = Stroke::new(1.0, colors::BORDER_STRONG);
        for (a, b) in [
            (
                r.left_top() + egui::vec2(6.0, 0.0),
                r.right_top() - egui::vec2(6.0, 0.0),
            ),
            (
                r.left_bottom() + egui::vec2(6.0, 0.0),
                r.right_bottom() - egui::vec2(6.0, 0.0),
            ),
            (
                r.left_top() + egui::vec2(0.0, 6.0),
                r.left_bottom() - egui::vec2(0.0, 6.0),
            ),
            (
                r.right_top() + egui::vec2(0.0, 6.0),
                r.right_bottom() - egui::vec2(0.0, 6.0),
            ),
        ] {
            painter.extend(egui::Shape::dashed_line(&[a, b], stroke, 3.0, 2.5));
        }
        painter.text(
            rect.left_center() + egui::vec2(10.0, 0.0),
            egui::Align2::LEFT_CENTER,
            value,
            egui::FontId::monospace(13.0),
            colors::MUTED,
        );
        painter.text(
            rect.right_center() - egui::vec2(10.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            suffix,
            egui::FontId::proportional(11.0),
            colors::FAINT,
        );
        response
    }

    /// Soft `bg_app` strip for previews and derived readouts.
    pub fn strip<R>(
        ui: &mut egui::Ui,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        Frame::new()
            .fill(colors::APP)
            .corner_radius(8)
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                content(ui)
            })
    }

    /// Selectable card with a radio dot, a medium title and a muted detail.
    pub fn radio_card(
        ui: &mut egui::Ui,
        id: impl std::hash::Hash + std::fmt::Debug,
        selected: bool,
        title: &str,
        detail: Option<&str>,
    ) -> egui::Response {
        let id = Id::new(("form-radio-card", id));
        let focused = ui.memory(|m| m.has_focus(id));
        let hovered = ui.rect_contains_pointer(
            ui.ctx()
                .read_response(id)
                .map_or(egui::Rect::NOTHING, |r| r.rect),
        );
        let shown = Frame::new()
            .fill(if selected {
                colors::MULTI_ROW
            } else if hovered {
                colors::HOVER_ROW
            } else {
                colors::CARD
            })
            .stroke(Stroke::new(
                1.0,
                if selected || focused {
                    colors::FOCUS
                } else {
                    colors::BORDER_SOFT
                },
            ))
            .corner_radius(9)
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (dot, _) =
                        ui.allocate_exact_size(egui::vec2(16.0, 18.0), egui::Sense::hover());
                    let center = egui::pos2(dot.center().x, dot.top() + 9.0);
                    ui.painter().circle(
                        center,
                        7.5,
                        colors::CARD,
                        Stroke::new(
                            1.0,
                            if selected {
                                colors::TEXT
                            } else {
                                colors::BORDER_STRONG
                            },
                        ),
                    );
                    if selected {
                        ui.painter().circle_filled(center, 4.0, colors::TEXT);
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.add(
                            egui::Label::new(colors::medium(ui, title, 13.0).color(colors::TEXT))
                                .wrap()
                                .selectable(false),
                        );
                        if let Some(detail) = detail {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(detail).size(11.5).color(colors::MUTED),
                                )
                                .wrap()
                                .selectable(false),
                            );
                        }
                    });
                });
            });
        let response = ui.interact(shown.response.rect, id, egui::Sense::click());
        let name = title.to_owned();
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, selected, &name)
        });
        response
    }

    /// Small square colour legend (cyan source / pink target).
    pub fn legend_swatch(ui: &mut egui::Ui, color: Color32) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);
    }

    /// A 34-high select box drawn like the inputs (content on the left, a
    /// chevron on the right). Pair it with `egui::Popup::menu(&response)` for
    /// the option list; the response has the ComboBox accessibility role.
    pub fn select_box(
        ui: &mut egui::Ui,
        popup_id: Id,
        accessible_name: &str,
        width: f32,
        content: impl FnOnce(&mut egui::Ui),
    ) -> egui::Response {
        let width = width.min(ui.available_width()).max(40.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::hover());
        // Stable ID (`popup_id.with("select")`) so owners can give it first focus.
        let response = ui.interact(rect, popup_id.with("select"), egui::Sense::click());
        let name = accessible_name.to_owned();
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), &name)
        });
        let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
        let active = open || response.has_focus();
        let painter = ui.painter().clone();
        if active {
            painter.rect_stroke(
                rect.expand(1.5),
                9.0,
                Stroke::new(3.0, colors::ACCENT_BG),
                egui::StrokeKind::Outside,
            );
        }
        painter.rect(
            rect,
            7.0,
            if active {
                colors::CARD
            } else if response.hovered() {
                colors::HOVER_ROW
            } else {
                colors::APP
            },
            Stroke::new(
                1.0,
                if active {
                    colors::FOCUS
                } else if response.hovered() {
                    colors::BORDER_STRONG
                } else {
                    colors::BORDER_SOFT
                },
            ),
            egui::StrokeKind::Inside,
        );
        let chevron = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 16.0, rect.center().y),
            egui::Vec2::splat(13.0),
        );
        crate::icons::icon(crate::icons::Icon::ChevDown, colors::FAINT, 13.0).paint_at(ui, chevron);
        let inner = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 10.0, rect.top()),
            egui::pos2(rect.right() - 30.0, rect.bottom()),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        child.spacing_mut().item_spacing.x = 9.0;
        content(&mut child);
        response
    }

    /// A select-box option row (full width, highlighted when current).
    pub fn option(
        ui: &mut egui::Ui,
        selected: bool,
        label: &str,
        content: impl FnOnce(&mut egui::Ui),
    ) -> egui::Response {
        let width = ui.available_width().max(120.0);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::click());
        let name = label.to_owned();
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &name)
        });
        let fill = if selected {
            colors::ACCENT_BG
        } else if response.hovered() || response.has_focus() {
            colors::HOVER_ROW
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 5.0, fill);
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(egui::vec2(8.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        child.spacing_mut().item_spacing.x = 9.0;
        content(&mut child);
        if response.clicked() {
            ui.close();
        }
        response
    }
}
