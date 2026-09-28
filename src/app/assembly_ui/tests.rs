use super::*;
use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};

fn app_with_board() -> (DesktopApp, Uuid) {
    let mut app = DesktopApp::default();
    let material_id = app
        .editor
        .create_material(NewMaterial {
            name: "Wood".into(),
            thickness: Length::from_micrometres(18_000),
            grain: BoardGrain::Length,
        })
        .unwrap();
    let board = app
        .editor
        .create_board(NewBoard {
            name: "Side".into(),
            material_id,
            length: Length::from_micrometres(100_000),
            width: Length::from_micrometres(50_000),
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        })
        .unwrap();
    (app, board)
}

#[test]
fn inspector_and_hud_render_one_pending_edit_without_committing() {
    let (mut app, board) = app_with_board();
    app.request_scene_selection(Some(board), false);
    assert_eq!(app.selection.active, Some(board));
    assert!(matches!(
        app.design_model().unwrap().inspector,
        DesignInspector::Board(_)
    ));
    let revision = app.editor.project().revision;
    app.edit_drafts
        .board(&app.editor, board, Unit::Mm, Locale::En)
        .unwrap()
        .length
        .edit("bad");
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_shared_board_dimensions(ui, board, "inspector");
        app.show_shared_board_dimensions(ui, board, "hud");
    });
    output.drop_without_applying_deltas();
    let draft = app
        .edit_drafts
        .existing_board(app.editor.project().id, board)
        .unwrap();
    assert_eq!(draft.length.display(), "bad");
    assert_eq!(app.editor.project().revision, revision);
    assert!(matches!(
        draft.values(),
        Err(DraftError::InvalidField { axis: 0, .. })
    ));
}

#[test]
fn inspector_pose_fields_are_session_only_and_keep_face_placement_available() {
    let (mut app, board) = app_with_board();
    let material_id = app.editor.project().materials[0].id;
    app.editor
        .create_board(NewBoard {
            name: "Target".into(),
            material_id,
            length: Length::from_micrometres(100_000),
            width: Length::from_micrometres(50_000),
            pose: Pose::new([200.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
        })
        .unwrap();
    app.request_scene_selection(Some(board), false);
    let original = app.editor.project().clone();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_shared_pose_fields(ui, board)
    })
    .drop_without_applying_deltas();
    assert_eq!(app.editor.project(), &original);
    assert!(
        !app.edit_drafts
            .existing_pose(original.id, board)
            .unwrap()
            .dirty()
    );
    assert!(
        app.action_availability(Request::with(A::PlaceFace, Target::Board(board)))
            .is_ok()
    );
    app.edit_drafts
        .existing_pose_mut(original.id, board)
        .unwrap()
        .position[0]
        .edit("bad");
    ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_shared_pose_fields(ui, board)
    })
    .drop_without_applying_deltas();
    assert_eq!(
        app.edit_drafts
            .existing_pose(original.id, board)
            .unwrap()
            .position[0]
            .display(),
        "bad"
    );
    assert_eq!(app.editor.project(), &original);
}

#[test]
fn inspector_miniature_uses_same_stock_identity_and_exact_part_records() {
    let project = plan_my_cabinet::reference_fixture::project();
    let unchanged = project.clone();
    let stock = StockReadModel::build(&project).unwrap();
    let allocation = &project.allocations[0];
    let piece = stock.miniature(allocation.stock_id).unwrap();
    assert!(piece.parts.iter().any(
        |part| part.board_id == allocation.board_id && part.allocation_id == allocation.id
    ));
    let ctx = egui::Context::default();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let response = sheet_miniature(
            ui,
            piece,
            allocation.board_id,
            &Localizer::new(Language::En),
        );
        assert!(response.rect.width() >= 120.0);
        assert!(response.rect.height() >= 64.0);
    });
    assert!(output.shapes.iter().any(|shape| match &shape.shape {
        egui::Shape::Rect(rect) => rect.fill == egui::Color32::from_rgb(244, 194, 122),
        _ => false,
    }));
    output.drop_without_applying_deltas();
    assert_eq!(project, unchanged);
}

#[test]
fn miniature_pointer_activation_routes_one_board_to_its_real_cut_sheet() {
    let project = plan_my_cabinet::reference_fixture::project();
    let stock = StockReadModel::build(&project).unwrap();
    let allocation = &project.allocations[0];
    let piece = stock.miniature(allocation.stock_id).unwrap();
    let ctx = egui::Context::default();
    let mut point = egui::Pos2::ZERO;
    let first = ctx.run_ui(egui::RawInput::default(), |ui| {
        point = sheet_miniature(
            ui,
            piece,
            allocation.board_id,
            &Localizer::new(Language::En),
        )
        .rect
        .center();
    });
    first.drop_without_applying_deltas();
    let pointer = |pressed| egui::RawInput {
        events: vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        ..Default::default()
    };
    ctx.run_ui(pointer(true), |ui| {
        assert!(
            !sheet_miniature(
                ui,
                piece,
                allocation.board_id,
                &Localizer::new(Language::En)
            )
            .clicked()
        );
    })
    .drop_without_applying_deltas();
    let mut activated = false;
    let click = ctx.run_ui(pointer(false), |ui| {
        activated = sheet_miniature(
            ui,
            piece,
            allocation.board_id,
            &Localizer::new(Language::En),
        )
        .clicked();
    });
    assert!(activated);
    click.drop_without_applying_deltas();
    let mut session = WorkspaceSession::new(&project);
    let mut selection = viewport::Selection::default();
    assert!(session.navigate(
        &project,
        &mut selection,
        Destination::BoardAllocation(allocation.board_id)
    ));
    assert_eq!(session.focused_sheet, Some(piece.id));
    assert_eq!(selection.ids, [allocation.board_id].into_iter().collect());
    assert_eq!(project, plan_my_cabinet::reference_fixture::project());
}

#[test]
fn hud_geometry_centers_on_canvas_and_keeps_secondary_windows_clear() {
    for canvas in [
        egui::Rect::from_min_max(egui::pos2(316.0, 46.0), egui::pos2(1148.0, 875.0)),
        egui::Rect::from_min_max(egui::pos2(60.0, 46.0), egui::pos2(700.0, 625.0)),
        egui::Rect::from_min_max(egui::pos2(55.0, 40.0), egui::pos2(540.0, 565.0)),
    ] {
        let strip = hud_rect(canvas).unwrap();
        assert_eq!(strip.center().x, canvas.center().x);
        assert!(canvas.contains_rect(strip));
        assert_eq!(strip.height(), 44.0);
        assert!(strip.width() <= 560.0);
    }
    assert!(
        hud_rect(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(180.0, 400.0)
        ))
        .is_none()
    );
}

#[test]
fn board_hud_only_renders_for_one_active_board_and_preserves_invalid_draft() {
    let (mut app, board) = app_with_board();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let canvas = egui::Rect::from_min_max(egui::pos2(316.0, 46.0), egui::pos2(1148.0, 875.0));
    let hud_id = egui::Id::new("design-board-hud");
    ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_design_hud(ui.ctx(), canvas)
    })
    .drop_without_applying_deltas();
    assert!(ctx.memory(|memory| memory.area_rect(hud_id)).is_none());
    app.request_scene_selection(Some(board), false);
    let revision = app.editor.project().revision;
    ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_design_hud(ui.ctx(), canvas)
    })
    .drop_without_applying_deltas();
    let pristine = ctx.memory(|memory| memory.area_rect(hud_id)).unwrap();
    assert!(pristine.width() <= 462.0, "strip expanded: {pristine:?}");
    assert!(
        pristine.height() <= 50.0,
        "HUD should be one row: {pristine:?}"
    );
    app.edit_drafts
        .board(&app.editor, board, Unit::Mm, Locale::En)
        .unwrap()
        .length
        .edit("bad");
    ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_design_hud(ui.ctx(), canvas)
    })
    .drop_without_applying_deltas();
    let rect = ctx.memory(|memory| memory.area_rect(hud_id)).unwrap();
    assert!(canvas.contains_rect(rect));
    assert_eq!(app.editor.project().revision, revision);
    assert_eq!(
        app.edit_drafts
            .existing_board(app.editor.project().id, board)
            .unwrap()
            .length
            .display(),
        "bad"
    );
    app.selection.choose(None, false);
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_design_hud(ui.ctx(), canvas)
    });
    assert!(
        !output
            .shapes
            .iter()
            .any(|shape| shape.clip_rect.intersects(rect)
                && matches!(&shape.shape, egui::Shape::Rect(r) if r.fill == theme_widgets::PANEL))
    );
    output.drop_without_applying_deltas();
}

#[test]
fn hud_actions_have_separate_pointer_targets_and_hide_without_editing_project() {
    for (language, canvas) in [
        (
            Language::En,
            egui::Rect::from_min_max(egui::pos2(316.0, 46.0), egui::pos2(1148.0, 875.0)),
        ),
        (
            Language::PtBr,
            egui::Rect::from_min_max(egui::pos2(60.0, 46.0), egui::pos2(700.0, 625.0)),
        ),
    ] {
        let (mut app, board) = app_with_board();
        app.localizer.set_language(language);
        app.request_scene_selection(Some(board), false);
        let before = app.editor.project().clone();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let screen =
            egui::Rect::from_min_max(egui::Pos2::ZERO, canvas.max + egui::vec2(100.0, 25.0));
        let mut frame = |events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| app.show_design_hud(ui.ctx(), canvas),
            )
            .drop_without_applying_deltas();
        };
        frame(vec![]);
        frame(vec![]);
        let strip = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("design-board-hud")))
            .unwrap();
        let actions = [
            A::PlaceFace,
            A::DuplicateBoard,
            A::ToggleVisibility,
            A::DeleteObject,
        ];
        let rects: Vec<_> = actions
            .iter()
            .map(|action| {
                ctx.data(|data| {
                    data.get_temp::<egui::Rect>(egui::Id::new(("hud-action", *action)))
                        .unwrap()
                })
            })
            .collect();
        for (index, rect) in rects.iter().enumerate() {
            assert!(
                strip.contains_rect(*rect),
                "{language:?}: {rect:?} outside {strip:?}"
            );
            assert!(rect.width() >= 22.0 && rect.height() >= 24.0);
            if index > 0 {
                assert!(rects[index - 1].right() < rect.left());
            }
        }
        let point = rects[2].center();
        for pressed in [true, false] {
            frame(vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        assert!(app.selection.hidden.contains(&board));
        assert_eq!(app.editor.project(), &before);
    }
}

#[test]
fn hierarchy_selection_and_group_transform_reparent_ungroup_are_atomic() {
    let (mut app, board) = app_with_board();
    app.selection.choose(Some(board), false);
    let group = app
        .editor
        .group_objects(&[board], None, "Cabinet", [0.0; 3])
        .unwrap();
    let rows = hierarchy(app.editor.project());
    assert_eq!(
        rows.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(),
        vec![(group, 0), (board, 1)]
    );
    app.selection.choose(Some(group), false);
    app.selection.choose(Some(board), true);
    let before = world_pose(app.editor.project(), board).unwrap();
    app.editor
        .transform_selection(
            &app.selection.ids.iter().copied().collect::<Vec<_>>(),
            [100.0, 0.0, 0.0],
            Quaternion::IDENTITY,
            [0.0; 3],
        )
        .unwrap();
    assert!(
        (world_pose(app.editor.project(), board)
            .unwrap()
            .translation_mm[0]
            - before.translation_mm[0]
            - 100.0)
            .abs()
            < 1e-6
    );
    app.editor.undo().unwrap();
    assert_eq!(world_pose(app.editor.project(), board).unwrap(), before);
    app.editor.redo().unwrap();
    let unchanged = app.editor.project().clone();
    assert!(app.editor.reparent_objects(&[group], Some(group)).is_err());
    assert_eq!(app.editor.project(), &unchanged);
    app.editor.reparent_objects(&[board], None).unwrap();
    app.editor.reparent_objects(&[board], Some(group)).unwrap();
    app.editor.ungroup_assembly(group).unwrap();
    assert_eq!(app.editor.project().boards[0].parent_id, None);
    assert!(
        (world_pose(app.editor.project(), board)
            .unwrap()
            .translation_mm[0]
            - 100.0)
            .abs()
            < 1e-6
    );
}

#[test]
fn modal_focus_invalid_and_escape_leave_project_untouched() {
    let (mut app, board) = app_with_board();
    app.selection.choose(Some(board), false);
    let initial = app.editor.project().clone();
    app.modals
        .set_assembly(Some(AssemblyDialog::new(&app, Operation::Transform)));
    let ctx = egui::Context::default();
    let draw = |app: &mut DesktopApp, events| {
        ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| app.show_assembly_dialog(ui.ctx()),
        )
        .drop_without_applying_deltas();
    };
    draw(&mut app, vec![]);
    assert!(ctx.memory(|m| m.focused()).is_some());
    app.modals.assembly_mut().unwrap().translation[0].text = "invalid".into();
    draw(&mut app, vec![]);
    assert_eq!(app.editor.project(), &initial);
    draw(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(app.modals.assembly().is_none());
    assert_eq!(app.editor.project(), &initial);
}

fn dialog_frame(app: &mut DesktopApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    ctx.run_ui(
        egui::RawInput {
            events,
            ..Default::default()
        },
        |ui| {
            app.show_assembly_dialog(ui.ctx());
        },
    )
    .drop_without_applying_deltas();
}

fn dialog_key(key: egui::Key) -> Vec<egui::Event> {
    vec![egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]
}

#[test]
fn hierarchy_modal_keys_validate_cancel_and_commit_one_undo() {
    let (mut app, board) = app_with_board();
    app.selection.choose(Some(board), false);
    let initial = app.editor.project().clone();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    app.modals
        .set_assembly(Some(AssemblyDialog::new(&app, Operation::Group)));
    dialog_frame(&mut app, &ctx, vec![]);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(egui::Id::new("assembly-hierarchy-name"))
    );
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
    assert!(app.modals.assembly().is_some()); // empty name
    assert_eq!(app.editor.project(), &initial);
    app.modals.assembly_mut().unwrap().name = "Cabinet".into();
    app.modals.assembly_mut().unwrap().pivot[0].text = "invalid".into();
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
    assert_eq!(app.editor.project(), &initial);
    app.modals.assembly_mut().unwrap().pivot[0].text = "0".into();
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
    assert!(app.modals.assembly().is_none());
    assert_eq!(app.editor.project().assemblies.len(), 1);
    assert_eq!(
        app.editor.project().boards[0].parent_id,
        app.selection.active
    );
    app.editor.undo().unwrap();
    assert_eq!(app.editor.project().assemblies, initial.assemblies);
    assert_eq!(app.editor.project().boards, initial.boards);
}

#[test]
fn hierarchy_modal_previews_nested_members_and_blocks_cycles() {
    let (mut app, board) = app_with_board();
    let root = app
        .editor
        .group_objects(&[board], None, "Cabinet", [0.0; 3])
        .unwrap();
    let child = app
        .editor
        .group_objects(&[board], Some(root), "Drawer", [0.0; 3])
        .unwrap();
    app.selection.choose(Some(root), false);
    let mut draft = AssemblyDialog::new(&app, Operation::Duplicate);
    assert_eq!(
        affected_objects(app.editor.project(), &draft),
        vec![(root, 0), (child, 1), (board, 2)]
    );
    assert_eq!(
        object_label(app.editor.project(), &app.localizer, board).unwrap(),
        format!("{} · Side", app.localizer.text("board-kind"))
    );
    draft.operation = Operation::Reparent;
    draft.target = Some(child);
    assert!(!hierarchy_target_valid(app.editor.project(), &draft));
    app.modals.set_assembly(Some(draft));
    let initial = app.editor.project().clone();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    dialog_frame(&mut app, &ctx, vec![]);
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
    assert_eq!(app.editor.project(), &initial);
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Escape));
    if app.modals.assembly().is_some() {
        // The focused parent chooser consumes the first Escape.
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Escape));
    }
    assert!(app.modals.assembly().is_none());
    assert_eq!(app.editor.project(), &initial);
}

#[test]
fn reparent_modal_popup_key_stays_in_draft_then_commits_with_undo() {
    let (mut app, board) = app_with_board();
    let group = app
        .editor
        .group_objects(&[board], None, "Cabinet", [0.0; 3])
        .unwrap();
    app.selection.choose(Some(board), false);
    app.modals
        .set_assembly(Some(AssemblyDialog::new(&app, Operation::Reparent)));
    let original = app.editor.project().clone();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    dialog_frame(&mut app, &ctx, vec![]);
    let popup = egui::Id::new("hierarchy-test-popup");
    egui::Popup::open_id(&ctx, popup);
    ctx.run_ui(
        egui::RawInput {
            events: dialog_key(egui::Key::Enter),
            ..Default::default()
        },
        |ui| {
            app.show_assembly_dialog(ui.ctx());
            egui::Popup::close_id(ui.ctx(), popup);
        },
    )
    .drop_without_applying_deltas();
    assert!(app.modals.assembly().is_some());
    assert_eq!(app.editor.project(), &original);
    // Move through the chooser and secondary action to the named confirmation.
    for _ in 0..3 {
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Tab));
    }
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
    assert!(app.modals.assembly().is_some()); // popup handles the first Enter
    assert_eq!(app.editor.project(), &original);
    app.modals.assembly_mut().unwrap().target = None;
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
    assert!(app.modals.assembly().is_none());
    assert_eq!(app.editor.project().boards[0].parent_id, None);
    app.editor.undo().unwrap();
    assert_eq!(app.editor.project().boards[0].parent_id, Some(group));
}

#[test]
fn duplicate_and_ungroup_modals_cancel_or_accept_atomically() {
    let (mut app, board) = app_with_board();
    let group = app
        .editor
        .group_objects(&[board], None, "Cabinet", [0.0; 3])
        .unwrap();
    app.selection.choose(Some(group), false);
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let initial = app.editor.project().clone();
    app.modals
        .set_assembly(Some(AssemblyDialog::new(&app, Operation::Duplicate)));
    dialog_frame(&mut app, &ctx, vec![]);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(egui::Id::new("assembly-duplicate-x"))
    );
    app.modals.assembly_mut().unwrap().translation[0].text = "1/64 in".into();
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
    assert!(app.modals.assembly().is_some()); // rounding needs explicit consent
    assert_eq!(app.editor.project(), &initial);
    app.modals.assembly_mut().unwrap().translation[0].text = "bad".into();
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
    assert_eq!(app.editor.project(), &initial);
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Escape));
    assert_eq!(app.editor.project(), &initial);
    app.modals
        .set_assembly(Some(AssemblyDialog::new(&app, Operation::Duplicate)));
    dialog_frame(&mut app, &ctx, vec![]);
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
    assert_eq!(app.editor.project().assemblies.len(), 2);
    assert_eq!(app.editor.project().boards.len(), 2);
    app.editor.undo().unwrap();
    assert_eq!(app.editor.project().boards, initial.boards);
    app.editor.redo().unwrap();
    app.selection.choose(Some(group), false);
    app.modals
        .set_assembly(Some(AssemblyDialog::new(&app, Operation::Ungroup)));
    assert_eq!(app.modals.assembly().unwrap().active, Some(group));
    dialog_frame(&mut app, &ctx, vec![]);
    dialog_frame(&mut app, &ctx, vec![]);
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Tab));
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
    assert!(
        app.editor
            .project()
            .assemblies
            .iter()
            .all(|a| a.id != group)
    );
    assert_eq!(
        app.editor
            .project()
            .boards
            .iter()
            .find(|b| b.id == board)
            .unwrap()
            .parent_id,
        None
    );
    app.editor.undo().unwrap();
    assert_eq!(
        app.editor
            .project()
            .boards
            .iter()
            .find(|b| b.id == board)
            .unwrap()
            .parent_id,
        Some(group)
    );
}

#[test]
fn hierarchy_modal_blocks_background_pointer_and_cancel_preserves_selection() {
    let (mut app, board) = app_with_board();
    app.selection.choose(Some(board), false);
    app.modals
        .set_assembly(Some(AssemblyDialog::new(&app, Operation::Group)));
    let initial = app.editor.project().clone();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
    let point = std::cell::Cell::new(egui::Pos2::ZERO);
    let primed = std::cell::Cell::new(false);
    let mut draw = |events: Vec<egui::Event>| {
        let mut clicked = false;
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            },
            |ui| {
                let button = ui.button("background scene selection");
                point.set(button.rect.center());
                clicked = button.clicked();
                let background = ui.layer_id();
                app.show_assembly_dialog(ui.ctx());
                if primed.get() {
                    assert!(!ctx.memory(|m| m.allows_interaction(background)));
                }
            },
        )
        .drop_without_applying_deltas();
        clicked
    };
    assert!(!draw(vec![]));
    primed.set(true);
    assert!(!draw(vec![]));
    for pressed in [true, false] {
        assert!(!draw(vec![
            egui::Event::PointerMoved(point.get()),
            egui::Event::PointerButton {
                pos: point.get(),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }
        ]));
    }
    assert_eq!(app.selection.active, Some(board));
    assert_eq!(app.editor.project(), &initial);
    dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Escape));
    assert!(app.modals.assembly().is_none());
    assert_eq!(app.editor.project(), &initial);
}

#[test]
fn hidden_group_keeps_unallocated_board_demand_and_list_selection() {
    let (mut app, board) = app_with_board();
    let group = app
        .editor
        .group_objects(&[board], None, "Body", [0.0; 3])
        .unwrap();
    let revision = app.editor.project().revision;
    let snapshot = app.editor.project().clone();
    app.selection.hidden.insert(group);
    let rows = hierarchy(app.editor.project());
    assert!(rows.iter().any(|row| row.0 == board));
    assert!(!app.selection.visible(app.editor.project(), board));
    assert_eq!(app.editor.project().boards.len(), 1);
    assert!(app.editor.project().allocations.is_empty());
    app.selection.choose(Some(board), false); // Object-list row remains selectable.
    assert_eq!(app.selection.active, Some(board));
    app.selection.reveal(app.editor.project(), board);
    assert!(app.selection.visible(app.editor.project(), board));
    assert_eq!(app.editor.project().revision, revision);
    assert_eq!(app.editor.project(), &snapshot);
}

#[test]
fn collapsed_hidden_issues_and_eye_are_independent_view_state() {
    let (mut app, board) = app_with_board();
    let assembly = app
        .editor
        .group_objects(&[board], None, "Body", [0.0; 3])
        .unwrap();
    app.session.design_expanded.insert(assembly);
    let revision = app.editor.project().revision;
    let original = app.editor.project().clone();
    app.selection.hidden.insert(assembly);
    app.session.design_expanded.remove(&assembly);
    let model = app.design_model().unwrap();
    let parent = model.outliner.iter().find(|r| r.id == assembly).unwrap();
    assert_eq!(parent.issue_count, 1);
    assert_eq!(parent.expanded, Some(false));
    assert!(!parent.visible);
    assert_eq!(visible_rows(&model.outliner).len(), 1);
    assert!(
        model
            .outliner
            .iter()
            .any(|r| r.id == board && !r.visible && r.issue_count == 1)
    );
    app.invoke(Request::with(A::ToggleVisibility, Target::Object(assembly)))
        .unwrap();
    let revealed = app.design_model().unwrap();
    assert!(
        revealed
            .outliner
            .iter()
            .find(|r| r.id == board)
            .unwrap()
            .visible
    );
    assert_eq!(revealed.outliner[0].expanded, Some(false));
    app.session.design_expanded.insert(assembly);
    assert_eq!(visible_rows(&app.design_model().unwrap().outliner).len(), 2);
    assert_eq!(app.editor.project().revision, revision);
    assert_eq!(app.editor.project(), &original);
}

#[test]
fn equal_names_and_selection_cardinality_never_infer_a_single_board() {
    let (mut app, board) = app_with_board();
    let second = app
        .editor
        .duplicate_board(
            board,
            Pose::new([140.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
        )
        .unwrap();
    let group = app
        .editor
        .group_objects(&[board], None, "Body", [0.0; 3])
        .unwrap();
    assert!(matches!(
        app.design_model().unwrap().inspector,
        DesignInspector::None
    ));
    app.selection.choose(Some(group), false);
    assert!(matches!(
        app.design_model().unwrap().inspector,
        DesignInspector::Assembly(_)
    ));
    app.selection.choose(Some(board), false);
    assert!(
        matches!(app.design_model().unwrap().inspector, DesignInspector::Board(ref b) if b.id == board)
    );
    app.selection.choose(Some(second), true);
    let model = app.design_model().unwrap();
    assert!(
        matches!(model.inspector, DesignInspector::Multi(ref m) if m.board_count == 2 && m.active == Some(second))
    );
    assert_eq!(
        model.outliner.iter().filter(|r| r.name == "Side").count(),
        2
    );
    assert_ne!(
        model
            .outliner
            .iter()
            .find(|r| r.id == board)
            .unwrap()
            .active,
        model
            .outliner
            .iter()
            .find(|r| r.id == second)
            .unwrap()
            .active
    );
}

#[test]
fn revision_invalidates_stock_snapshot_but_view_gestures_reuse_it() {
    let (mut app, board) = app_with_board();
    app.design_model().unwrap();
    let first = &app.design.stock_snapshot.as_ref().unwrap().1 as *const _;
    let revision = app.editor.project().revision;
    app.selection.choose(Some(board), false);
    app.selection.hidden.insert(board);
    app.design_model().unwrap();
    assert_eq!(
        first,
        &app.design.stock_snapshot.as_ref().unwrap().1 as *const _
    );
    assert_eq!(app.editor.project().revision, revision);
    app.editor
        .duplicate_board(
            board,
            Pose::new([200.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
        )
        .unwrap();
    app.design_model().unwrap();
    assert_eq!(
        app.design.stock_snapshot.as_ref().unwrap().0.1,
        app.editor.project().revision
    );
    assert_eq!(
        app.design.stock_snapshot.as_ref().unwrap().1.boards.len(),
        2
    );
}

#[test]
fn inspector_routes_obey_selection_and_modal_guards() {
    let (mut app, board) = app_with_board();
    assert!(app.action_availability(Request::new(A::Transform)).is_err());
    assert!(
        app.action_availability(Request::new(A::DeleteObject))
            .is_err()
    );
    app.selection.choose(Some(board), false);
    assert!(app.action_availability(Request::new(A::Transform)).is_ok());
    assert!(
        app.action_availability(Request::new(A::DeleteObject))
            .is_ok()
    );
    let group = app
        .editor
        .group_objects(&[board], None, "Body", [0.0; 3])
        .unwrap();
    app.selection.choose(Some(group), false);
    assert!(app.action_availability(Request::new(A::Ungroup)).is_ok());
    assert!(
        app.action_availability(Request::new(A::DuplicateAssembly))
            .is_ok()
    );
    app.modals
        .set_assembly(Some(AssemblyDialog::new(&app, Operation::Transform)));
    assert!(
        app.action_availability(Request::new(A::DeleteObject))
            .is_err()
    );
    assert!(app.action_availability(Request::new(A::Ungroup)).is_err());
}

#[test]
fn fixture_compact_sidebar_fits_stock_and_preserves_warning_selection_and_source() {
    use plan_my_cabinet::reference_fixture as fixture;
    let mut project = fixture::project();
    project.material_colors.insert(
        fixture::WHITE_ID,
        plan_my_cabinet::domain::SrgbColor([233, 231, 226]),
    );
    project.material_colors.insert(
        fixture::OAK_ID,
        plan_my_cabinet::domain::SrgbColor([185, 139, 94]),
    );
    let mut app = DesktopApp {
        editor: ProjectEditor::new(project).unwrap(),
        ..DesktopApp::default()
    };
    app.session = WorkspaceSession::new(app.editor.project());
    app.selection.choose(Some(fixture::SHELF_ID), false);
    let model = app.design_model().unwrap();
    let back = model
        .outliner
        .iter()
        .find(|r| r.id == fixture::BACK_ID)
        .unwrap();
    assert_eq!(back.issue_count, 1);
    assert!(!back.selected);
    assert!(back.visible);
    let shelf = model
        .outliner
        .iter()
        .find(|r| r.id == fixture::SHELF_ID)
        .unwrap();
    assert!(shelf.active);
    assert_eq!(
        model.materials.iter().map(|m| m.board_count).sum::<usize>(),
        9
    );
    assert_eq!(
        model
            .materials
            .iter()
            .map(|m| m.unallocated_board_count)
            .sum::<usize>(),
        1
    );
    assert_ne!(model.materials[0].color, model.materials[1].color);
    assert_eq!(model.stock.len(), 4);
    assert!(
        model
            .stock
            .iter()
            .any(|s| s.alias == "O1" && s.source == plan_my_cabinet::domain::StockSource::Owned)
    );
    assert!(
        model
            .stock
            .iter()
            .any(|s| s.alias == "S1" && s.part_count > 0)
    );

    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    crate::theme::install_fonts(&ctx);
    icons::install_loaders(&ctx);
    let mut height = 0.0;
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.set_width(256.0);
        app.show_hierarchy(ui);
        height = ui.min_rect().height();
    });
    assert!(
        height < 800.0,
        "stock controls must fit at the reference height: {}",
        height
    );
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    for alias in ["S1", "O1", "S2", "S3"] {
        assert!(
            update
                .nodes
                .iter()
                .any(|(_, node)| node.label().is_some_and(|l| l.starts_with(alias))),
            "stock row {alias} missing"
        );
    }
    assert!(
        !update
            .nodes
            .iter()
            .any(|(_, node)| node.value() == Some("#5"))
    );
    output.drop_without_applying_deltas();
}

#[test]
fn hardware_placeholder_actions_require_matching_target_and_respect_modal_guard() {
    let (mut app, _) = app_with_board();
    let hardware = app
        .editor
        .create_placeholder(
            "Foot".into(),
            [Length::from_micrometres(20_000); 3],
            None,
            Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        )
        .unwrap();
    app.selection.choose(Some(hardware), false);
    for action in [A::EditHardware, A::DuplicateHardware] {
        assert!(
            app.action_availability(Request::with(action, Target::Object(hardware)))
                .is_ok()
        );
        assert!(
            app.action_availability(Request::with(
                action,
                Target::Object(app.editor.project().boards[0].id)
            ))
            .is_err()
        );
    }
    app.modals
        .set_assembly(Some(AssemblyDialog::new(&app, Operation::Transform)));
    assert!(
        app.action_availability(Request::with(A::EditHardware, Target::Object(hardware)))
            .is_err()
    );
}
