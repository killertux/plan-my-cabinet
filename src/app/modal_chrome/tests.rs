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
                assert!(ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)));
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
