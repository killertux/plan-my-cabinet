//! Shared chrome for native `egui::Modal` dialogs.
//!
//! Keep `Modal` itself: its layer and backdrop establish the pointer and focus
//! boundary. Call `show` each frame while a dialog is active, and call `close`
//! only when the owning workflow has actually accepted/cancelled the action.
use eframe::egui::{self, Frame, Id, Margin, RichText, Stroke};

use crate::{theme, theme_widgets as colors};

/// Caller-supplied, localized verb labels (e.g. "Create board" / "Criar peça",
/// "Delete" / "Excluir"). No action text is inferred from the dialog title.
pub struct ModalActions<'a> {
    pub cancel: &'a str,
    pub confirm: &'a str,
}

/// Localized decisions for prompts which must distinguish leaving an edit
/// (Apply/Discard/Stay or Accept/Cancel/Stay) from remaining in place.
/// The owner resolves the returned decision; chrome never commits a draft.
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

// The three-way branch is an API hook until the owning dialogs adopt it.
#[allow(dead_code)]
enum Footer<'a> {
    Reader(&'a str),
    Two(ModalActions<'a>),
    Three(ModalThreeActions<'a>),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::RawInput {
        egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        }
    }

    fn draw(ctx: &egui::Context, chrome: &mut ModalChrome, valid: bool) -> ModalAction {
        chrome
            .show(
                ctx,
                "New board",
                ModalActions {
                    cancel: "Cancel",
                    confirm: "Create board",
                },
                |ui| {
                    ui.add(egui::TextEdit::singleline(&mut String::new()).id(Id::new("name")));
                    ((), valid)
                },
            )
            .action
    }

    fn draw_three(ctx: &egui::Context, chrome: &mut ModalChrome, valid: bool) -> ModalThreeAction {
        chrome
            .show_three(
                ctx,
                "Pending navigation",
                ModalThreeActions {
                    primary: "Apply",
                    secondary: "Discard",
                    stay: "Stay",
                },
                |ui| {
                    ui.add(egui::TextEdit::singleline(&mut String::new()).id(Id::new("draft")));
                    ((), valid)
                },
            )
            .action
    }

    fn frame(ctx: &egui::Context, input: egui::RawInput, f: impl FnMut(&mut egui::Ui)) {
        let mut output = ctx.run_ui(input, f);
        output.textures_delta.clear();
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx
    }

    #[test]
    fn reader_focus_back_and_escape_restore_invoker_without_affirmative_action() {
        for closing_key in [egui::Key::Enter, egui::Key::Escape] {
            let ctx = context();
            ctx.enable_accesskit();
            let mut chrome = ModalChrome::new(Id::new("reader"));
            let mut invoker = None;
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let button = ui.button("Examples");
                button.request_focus();
                invoker = Some(button.id);
                assert_eq!(
                    chrome
                        .show_reader(&ctx, "Worked examples", "Back", |ui| {
                            ui.label("Read only");
                        })
                        .action,
                    ModalAction::None
                );
            });
            let update = output.platform_output.accesskit_update.as_ref().unwrap();
            assert!(
                update
                    .nodes
                    .iter()
                    .any(|(id, node)| *id == update.focus && node.label() == Some("Back"))
            );
            output.drop_without_applying_deltas();
            frame(&ctx, key(closing_key, egui::Modifiers::NONE), |ui| {
                ui.button("Examples").on_hover_text("invoker");
                assert_eq!(
                    chrome
                        .show_reader(&ctx, "Worked examples", "Back", |ui| {
                            ui.label("Read only");
                        })
                        .action,
                    ModalAction::Cancel
                );
                chrome.close(&ctx);
                assert_eq!(ctx.memory(|m| m.focused()), invoker);
            });
        }
    }

    #[test]
    fn tab_and_shift_tab_stay_on_modal_layer_and_restore_invoker() {
        let ctx = context();
        let name = Id::new("name");
        let mut chrome = ModalChrome::new(Id::new("test-dialog")).first_focus(name);
        let mut invoker = None;
        frame(&ctx, egui::RawInput::default(), |ui| {
            let button = ui.button("Open");
            invoker = Some(button.id);
            button.request_focus();
            draw(&ctx, &mut chrome, true);
            assert_eq!(ctx.memory(|m| m.focused()), Some(name));
        });
        assert!(chrome.is_active());
        for (index, input) in [
            key(egui::Key::Tab, egui::Modifiers::NONE),
            key(egui::Key::Tab, egui::Modifiers::SHIFT),
        ]
        .into_iter()
        .enumerate()
        {
            let before = ctx.memory(|m| m.focused());
            frame(&ctx, input, |_ui| {
                draw(&ctx, &mut chrome, true);
            });
            let focused = ctx.memory(|m| m.focused());
            assert_ne!(focused, invoker);
            assert!(focused.is_some());
            if index == 0 {
                assert_ne!(focused, before);
            }
        }
        chrome.close(&ctx);
        assert!(!chrome.is_active());
        assert_eq!(ctx.memory(|m| m.focused()), invoker);
    }

    #[test]
    fn enter_escape_and_popup_events_do_not_fall_through() {
        let ctx = context();
        let mut chrome = ModalChrome::new(Id::new("test-dialog")).first_focus(Id::new("name"));
        frame(&ctx, egui::RawInput::default(), |_ui| {
            draw(&ctx, &mut chrome, true);
        });
        let popup = Id::new("nested-combo");
        egui::Popup::open_id(&ctx, popup);
        frame(&ctx, key(egui::Key::Enter, egui::Modifiers::NONE), |_ui| {
            let result = chrome.show(
                &ctx,
                "New board",
                ModalActions {
                    cancel: "Cancel",
                    confirm: "Create board",
                },
                |ui| {
                    ui.add(egui::TextEdit::singleline(&mut String::new()).id(Id::new("name")));
                    egui::Popup::close_id(&ctx, popup); // option selected this frame
                    ((), true)
                },
            );
            assert_eq!(result.action, ModalAction::None);
        });
        egui::Popup::open_id(&ctx, popup);
        frame(&ctx, key(egui::Key::Escape, egui::Modifiers::NONE), |_ui| {
            let result = chrome.show(
                &ctx,
                "New board",
                ModalActions {
                    cancel: "Cancel",
                    confirm: "Create board",
                },
                |ui| {
                    ui.add(egui::TextEdit::singleline(&mut String::new()).id(Id::new("name")));
                    egui::Popup::close_id(&ctx, popup); // popup dismissed this frame
                    ((), true)
                },
            );
            assert_eq!(result.action, ModalAction::None);
        });
        frame(&ctx, key(egui::Key::Enter, egui::Modifiers::NONE), |_ui| {
            assert_eq!(draw(&ctx, &mut chrome, false), ModalAction::None);
        });
        frame(&ctx, key(egui::Key::Enter, egui::Modifiers::NONE), |_ui| {
            assert_eq!(draw(&ctx, &mut chrome, true), ModalAction::Confirm);
        });
        frame(&ctx, key(egui::Key::Escape, egui::Modifiers::NONE), |_ui| {
            let result = chrome.show(
                &ctx,
                "New board",
                ModalActions {
                    cancel: "Cancel",
                    confirm: "Create board",
                },
                |ui| {
                    ui.add(egui::TextEdit::singleline(&mut String::new()).id(Id::new("name")));
                    // An active draft/editing layer handles this Escape first.
                    assert!(
                        ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                    );
                    ((), true)
                },
            );
            assert_eq!(result.action, ModalAction::None);
        });
        frame(&ctx, key(egui::Key::Escape, egui::Modifiers::NONE), |_ui| {
            assert_eq!(draw(&ctx, &mut chrome, true), ModalAction::Cancel);
        });
    }

    #[test]
    fn nested_dialog_restores_parent_draft_focus() {
        let ctx = context();
        let board_field = Id::new("board-name");
        let material_field = Id::new("material-name");
        let mut board = ModalChrome::new(Id::new("board-dialog")).first_focus(board_field);
        let mut material = ModalChrome::new(Id::new("material-dialog")).first_focus(material_field);
        let mut board_text = String::from("My shelf");
        frame(&ctx, egui::RawInput::default(), |_ui| {
            board.show(
                &ctx,
                "New board",
                ModalActions {
                    cancel: "Cancel",
                    confirm: "Create board",
                },
                |ui| {
                    ui.add(egui::TextEdit::singleline(&mut board_text).id(board_field));
                    ((), true)
                },
            );
            assert_eq!(ctx.memory(|m| m.focused()), Some(board_field));
        });
        frame(&ctx, egui::RawInput::default(), |_ui| {
            material.show(
                &ctx,
                "New material",
                ModalActions {
                    cancel: "Cancel",
                    confirm: "Create material",
                },
                |ui| {
                    ui.add(egui::TextEdit::singleline(&mut String::new()).id(material_field));
                    ((), true)
                },
            );
            assert_eq!(ctx.memory(|m| m.focused()), Some(material_field));
        });
        material.close(&ctx);
        assert_eq!(ctx.memory(|m| m.focused()), Some(board_field));
        frame(&ctx, egui::RawInput::default(), |_ui| {
            board.show(
                &ctx,
                "New board",
                ModalActions {
                    cancel: "Cancel",
                    confirm: "Create board",
                },
                |ui| {
                    ui.add(egui::TextEdit::singleline(&mut board_text).id(board_field));
                    ((), true)
                },
            );
        });
        assert_eq!(board_text, "My shelf");
        assert_eq!(ctx.memory(|m| m.focused()), Some(board_field));
    }

    #[test]
    fn long_body_stays_bounded_with_reachable_actions() {
        let ctx = context();
        let mut chrome = ModalChrome::new(Id::new("long-dialog")).width(400.0);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(780.0, 560.0),
            )),
            ..Default::default()
        };
        frame(&ctx, input, |_ui| {
            chrome.show(
                &ctx,
                "Long form",
                ModalActions {
                    cancel: "Cancel",
                    confirm: "Apply changes",
                },
                |ui| {
                    for row in 0..120 {
                        ui.label(format!("Field {row}"));
                    }
                    ((), true)
                },
            );
        });
        let rect = ctx.memory(|m| m.area_rect(Id::new("long-dialog"))).unwrap();
        assert!(rect.height() <= ctx.content_rect().height());
    }

    #[test]
    fn underlying_layer_is_not_interactable_with_modal_open() {
        let ctx = context();
        let mut chrome = ModalChrome::new(Id::new("test-dialog"));
        let draw_scene = |ui: &mut egui::Ui| {
            let button_rect = ui.button("Scene action").rect;
            let scroll = egui::ScrollArea::vertical()
                .max_height(100.0)
                .show(ui, |ui| {
                    for i in 0..30 {
                        ui.label(format!("Scene row {i}"));
                    }
                });
            (scroll.state.offset.y, scroll.inner_rect, button_rect)
        };
        frame(&ctx, egui::RawInput::default(), |ui| {
            draw_scene(ui);
            draw(&ctx, &mut chrome, true);
        });
        let mut baseline = 0.0;
        let mut scroll_rect = egui::Rect::NOTHING;
        let mut button_rect = egui::Rect::NOTHING;
        frame(&ctx, egui::RawInput::default(), |ui| {
            let scene_layer = ui.layer_id();
            (baseline, scroll_rect, button_rect) = draw_scene(ui);
            draw(&ctx, &mut chrome, true);
            assert!(!ctx.memory(|m| m.allows_interaction(scene_layer)));
        });
        let point = button_rect.center();
        let input = egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        frame(&ctx, input, |ui| {
            let scene_layer = ui.layer_id();
            assert!(!ui.button("Scene action").clicked());
            let scroll = egui::ScrollArea::vertical()
                .max_height(100.0)
                .show(ui, |ui| {
                    for i in 0..30 {
                        ui.label(format!("Scene row {i}"));
                    }
                });
            assert_eq!(scroll.state.offset.y, baseline);
            draw(&ctx, &mut chrome, true);
            assert!(!ctx.memory(|m| m.allows_interaction(scene_layer)));
        });
        let wheel = egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(scroll_rect.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -80.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        frame(&ctx, wheel, |ui| {
            assert_eq!(draw_scene(ui).0, baseline);
            draw(&ctx, &mut chrome, true);
        });
        frame(&ctx, key(egui::Key::Delete, egui::Modifiers::NONE), |ui| {
            let scene_layer = ui.layer_id();
            draw(&ctx, &mut chrome, true);
            assert!(!ctx.memory(|m| m.allows_interaction(scene_layer)));
        });
        // Application-level raw-input shortcuts must still be guarded by the
        // owner: egui's modal layer does not remove keys from `ctx.input`.
    }

    #[test]
    fn three_decisions_guard_primary_and_escape_stays_with_focus_restoration() {
        let ctx = context();
        let mut chrome = ModalChrome::new(Id::new("three-dialog")).first_focus(Id::new("draft"));
        let mut invoker = None;
        frame(&ctx, egui::RawInput::default(), |ui| {
            let button = ui.button("Navigate");
            invoker = Some(button.id);
            button.request_focus();
            assert_eq!(draw_three(&ctx, &mut chrome, false), ModalThreeAction::None);
            assert_eq!(ctx.memory(|m| m.focused()), Some(Id::new("draft")));
        });
        frame(&ctx, key(egui::Key::Enter, egui::Modifiers::NONE), |_ui| {
            assert_eq!(draw_three(&ctx, &mut chrome, false), ModalThreeAction::None);
            assert!(!ctx.input(|i| i.key_pressed(egui::Key::Enter)));
        });
        frame(&ctx, key(egui::Key::Enter, egui::Modifiers::NONE), |_ui| {
            assert_eq!(
                draw_three(&ctx, &mut chrome, true),
                ModalThreeAction::Primary
            );
        });
        frame(&ctx, key(egui::Key::Escape, egui::Modifiers::NONE), |_ui| {
            assert_eq!(draw_three(&ctx, &mut chrome, true), ModalThreeAction::Stay);
        });
        assert!(chrome.is_active(), "owner closes after resolving Stay");
        chrome.close(&ctx);
        assert_eq!(ctx.memory(|m| m.focused()), invoker);
    }

    #[test]
    fn three_decisions_respect_nested_popups_and_consumed_escape() {
        let ctx = context();
        let mut chrome = ModalChrome::new(Id::new("three-popup"));
        frame(&ctx, egui::RawInput::default(), |_ui| {
            draw_three(&ctx, &mut chrome, true);
        });
        let popup = Id::new("three-child-popup");
        for key_code in [egui::Key::Enter, egui::Key::Escape] {
            egui::Popup::open_id(&ctx, popup);
            frame(&ctx, key(key_code, egui::Modifiers::NONE), |_ui| {
                let result = chrome.show_three(
                    &ctx,
                    "Pending navigation",
                    ModalThreeActions {
                        primary: "Accept",
                        secondary: "Cancel",
                        stay: "Stay",
                    },
                    |ui| {
                        ui.label("A draft survives");
                        egui::Popup::close_id(&ctx, popup);
                        ((), true)
                    },
                );
                assert_eq!(result.action, ModalThreeAction::None);
            });
        }
        frame(&ctx, key(egui::Key::Escape, egui::Modifiers::NONE), |_ui| {
            assert_eq!(
                chrome
                    .show_three(
                        &ctx,
                        "Pending navigation",
                        ModalThreeActions {
                            primary: "Accept",
                            secondary: "Cancel",
                            stay: "Stay",
                        },
                        |_ui| {
                            assert!(ctx.input_mut(|i| {
                                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            }));
                            ((), true)
                        },
                    )
                    .action,
                ModalThreeAction::None
            );
        });
    }

    #[test]
    fn three_footer_buttons_are_distinct_and_invalid_primary_is_disabled() {
        let ctx = context();
        let mut chrome = ModalChrome::new(Id::new("three-buttons"));
        let actions = ModalThreeActions {
            primary: "Apply",
            secondary: "Discard",
            stay: "Stay",
        };
        frame(&ctx, egui::RawInput::default(), |_ui| {
            assert_eq!(
                chrome
                    .show_three(&ctx, "Leave?", actions, |ui| {
                        ui.label("Unfinished edit");
                        ((), false)
                    })
                    .action,
                ModalThreeAction::None
            );
        });
        let stay_id = ctx.memory(|m| m.focused()).unwrap();
        frame(&ctx, egui::RawInput::default(), |ui| {
            let _ = ui.button("Background");
            chrome.show_three(&ctx, "Leave?", actions, |_ui| ((), false));
            assert!(!ctx.memory(|m| m.allows_interaction(ui.layer_id())));
        });
        frame(&ctx, key(egui::Key::Tab, egui::Modifiers::NONE), |_ui| {
            chrome.show_three(
                &ctx,
                "Leave?",
                ModalThreeActions {
                    primary: "Apply",
                    secondary: "Discard",
                    stay: "Stay",
                },
                |_ui| ((), false),
            );
        });
        let secondary_id = ctx.memory(|m| m.focused()).unwrap();
        assert_ne!(secondary_id, stay_id);
        frame(&ctx, key(egui::Key::Enter, egui::Modifiers::NONE), |_ui| {
            assert_eq!(
                chrome
                    .show_three(
                        &ctx,
                        "Leave?",
                        ModalThreeActions {
                            primary: "Apply",
                            secondary: "Discard",
                            stay: "Stay",
                        },
                        |_ui| ((), false)
                    )
                    .action,
                ModalThreeAction::Secondary
            );
        });
    }

    #[test]
    fn valid_button_only_prompt_defaults_enter_to_primary() {
        let ctx = context();
        let mut chrome = ModalChrome::new(Id::new("three-button-only"));
        let actions = ModalThreeActions {
            primary: "Save",
            secondary: "Discard",
            stay: "Stay",
        };
        frame(&ctx, egui::RawInput::default(), |_ui| {
            assert_eq!(
                chrome
                    .show_three(&ctx, "Unsaved work", actions, |_ui| ((), true))
                    .action,
                ModalThreeAction::None
            );
        });
        frame(&ctx, key(egui::Key::Enter, egui::Modifiers::NONE), |_ui| {
            assert_eq!(
                chrome
                    .show_three(&ctx, "Unsaved work", actions, |_ui| ((), true))
                    .action,
                ModalThreeAction::Primary
            );
        });
    }

    #[test]
    fn three_footer_is_reachable_at_compact_size_with_long_localized_verbs() {
        let ctx = context();
        let mut chrome = ModalChrome::new(Id::new("three-compact")).width(480.0);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 400.0),
            )),
            ..Default::default()
        };
        frame(&ctx, input, |_ui| {
            chrome.show_three(
                &ctx,
                "Alterações pendentes para este projeto",
                ModalThreeActions {
                    primary: "Aplicar alterações",
                    secondary: "Descartar alterações",
                    stay: "Permanecer nesta edição",
                },
                |ui| {
                    for row in 0..120 {
                        ui.label(format!("Campo {row}"));
                    }
                    ((), false)
                },
            );
        });
        let rect = ctx
            .memory(|m| m.area_rect(Id::new("three-compact")))
            .unwrap();
        assert!(ctx.content_rect().contains_rect(rect));
        assert!(rect.height() <= ctx.content_rect().height());
    }
}

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
}

impl ModalChrome {
    pub fn new(id: Id) -> Self {
        Self {
            id,
            width: 480.0,
            icon: crate::icons::Icon::Sliders,
            first_focus: None,
            visible: false,
            invoking_focus: None,
            last_focus: None,
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

    /// Show a three-decision prompt. `primary` is the guarded Apply/Accept
    /// action, `secondary` is Discard/Cancel (which may continue navigation),
    /// and `stay` keeps the edit and location. Enter chooses a valid primary;
    /// Escape chooses Stay. Call `close` only after the owner resolves a choice.
    #[allow(dead_code)] // Callers migrate independently of this shared primitive.
    pub fn show_three<T>(
        &mut self,
        ctx: &egui::Context,
        title: &str,
        actions: ModalThreeActions<'_>,
        body: impl FnOnce(&mut egui::Ui) -> (T, bool),
    ) -> ModalThreeResult<T> {
        self.show_with_footer(ctx, title, Footer::Three(actions), body)
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
        // Leave room for a wrapped title and localized actions (up to three
        // stacked rows in the compact three-decision variant).
        let three_actions = matches!(footer, Footer::Three(_));
        let body_height =
            (ctx.content_rect().height() - if three_actions { 230.0 } else { 190.0 }).max(1.0);
        let modal = egui::Modal::new(self.id)
            .backdrop_color(egui::Color32::from_rgba_unmultiplied(42, 37, 32, 64))
            .frame(
                Frame::new()
                    .fill(colors::PANEL)
                    .stroke(Stroke::new(1.0, colors::BORDER))
                    .corner_radius(12)
                    .inner_margin(Margin::ZERO),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                Frame::new()
                    .inner_margin(Margin::symmetric(18, 16))
                    .show(ui, |ui| {
                        ui.set_width((width - 36.0).max(1.0));
                        let font = ui
                            .style()
                            .text_styles
                            .get(&egui::TextStyle::Name("Title".into()))
                            .cloned()
                            .unwrap_or_else(|| egui::FontId::proportional(theme::TITLE.size));
                        ui.horizontal(|ui| {
                            let (tile, _) = ui
                                .allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                            ui.painter().rect_filled(tile, 8.0, colors::ACCENT_BG);
                            crate::icons::icon(
                                self.icon,
                                egui::Color32::from_rgb(154, 91, 18),
                                20.0,
                            )
                            .paint_at(
                                ui,
                                egui::Rect::from_center_size(
                                    tile.center(),
                                    egui::Vec2::splat(20.0),
                                ),
                            );
                            let keycap_width = 30.0;
                            let label_width =
                                (ui.available_width() - keycap_width - ui.spacing().item_spacing.x)
                                    .max(1.0);
                            ui.add_sized(
                                [label_width, 34.0],
                                egui::Label::new(RichText::new(title).font(font))
                                    .wrap()
                                    .halign(egui::Align::Min),
                            );
                            Frame::new()
                                .fill(colors::APP)
                                .stroke(Stroke::new(1.0, colors::BORDER))
                                .corner_radius(4)
                                .inner_margin(Margin::symmetric(4, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new("Esc")
                                            .monospace()
                                            .size(11.0)
                                            .color(colors::FAINT),
                                    );
                                });
                        });
                    });
                ui.separator();
                let (result, valid) = Frame::new()
                    .inner_margin(Margin::symmetric(18, 16))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(self.id.with("body"))
                            .max_height(body_height)
                            .show(ui, |ui| {
                                ui.set_width((width - 36.0).max(1.0));
                                body(ui)
                            })
                            .inner
                    })
                    .inner;
                ui.separator();
                let (secondary, primary, stay, initial_focus_id) = Frame::new()
                    .fill(colors::APP)
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 12,
                        se: 12,
                    })
                    .inner_margin(Margin::symmetric(18, 12))
                    .show(ui, |ui| {
                        ui.set_width((width - 36.0).max(1.0));
                        match footer {
                            Footer::Reader(label) => {
                                let back = colors::secondary_button(ui, label);
                                (back.clicked(), false, false, back.id)
                            }
                            Footer::Two(actions) => {
                                let primary_font = ui
                                    .style()
                                    .text_styles
                                    .get(&egui::TextStyle::Name("PrimaryButton".into()))
                                    .cloned()
                                    .unwrap_or_else(|| {
                                        egui::FontId::proportional(theme::PRIMARY_BUTTON.size)
                                    });
                                let cancel_width = ui
                                    .painter()
                                    .layout_no_wrap(
                                        actions.cancel.to_owned(),
                                        egui::TextStyle::Button.resolve(ui.style()),
                                        colors::TEXT,
                                    )
                                    .size()
                                    .x;
                                let confirm_width = ui
                                    .painter()
                                    .layout_no_wrap(
                                        actions.confirm.to_owned(),
                                        primary_font,
                                        colors::PANEL,
                                    )
                                    .size()
                                    .x;
                                let leading = (ui.available_width()
                                    - cancel_width
                                    - confirm_width
                                    - 4.0 * ui.spacing().button_padding.x
                                    - 2.0 * ui.spacing().item_spacing.x)
                                    .max(0.0);
                                let (secondary, primary, id) = ui
                                    .horizontal_wrapped(|ui| {
                                        if leading > 0.0 {
                                            ui.add_space(leading);
                                        }
                                        let cancel = colors::secondary_button(ui, actions.cancel);
                                        let confirm =
                                            colors::primary_button(ui, actions.confirm, valid);
                                        (cancel.clicked(), confirm.clicked(), cancel.id)
                                    })
                                    .inner;
                                (secondary, primary, false, id)
                            }
                            Footer::Three(actions) => {
                                // Wrapping individual buttons and stacking at compact widths
                                // keeps long pt-BR verbs reachable without clipping the footer.
                                let available = (width - 40.0).max(1.0);
                                let draw = |ui: &mut egui::Ui| {
                                    let stay = ui.add(egui::Button::new(actions.stay).wrap());
                                    let secondary =
                                        ui.add(egui::Button::new(actions.secondary).wrap());
                                    let primary = ui.add_enabled(
                                        valid,
                                        egui::Button::new(
                                            RichText::new(actions.primary).color(colors::PANEL),
                                        )
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
                                };
                                if available < 360.0 {
                                    ui.vertical(|ui| {
                                        ui.set_width(available);
                                        draw(ui)
                                    })
                                    .inner
                                } else {
                                    ui.horizontal_wrapped(draw).inner
                                }
                            }
                        }
                    })
                    .inner;
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
