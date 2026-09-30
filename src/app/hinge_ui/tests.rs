use super::*;
use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
use plan_my_cabinet::domain::BoardGrain;

fn fixture() -> DesktopApp {
    let mut app = DesktopApp::default();
    let material = app
        .editor
        .create_material(NewMaterial {
            name: "Plywood".into(),
            thickness: Length::from_micrometres(18_000),
            grain: BoardGrain::Unrestricted,
        })
        .unwrap();
    for name in ["Door", "Side"] {
        app.editor
            .create_board(NewBoard {
                name: name.into(),
                material_id: material,
                length: Length::from_micrometres(100_000),
                width: Length::from_micrometres(100_000),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
    }
    hardware_catalog::add_builtin(&mut app.editor).unwrap();
    app
}

#[test]
fn invalid_draft_and_escape_leave_project_untouched() {
    let mut app = fixture();
    let before = app.editor.project().clone();
    let mut draft = HingeDialog::new(&app, None);
    assert!(draft.proposed(&app).is_some());
    draft.values[0] = "invalid".into();
    assert!(draft.proposed(&app).is_none());
    draft.values[0] = "-1".into();
    assert!(draft.proposed(&app).is_none());
    draft.values[0] = "50".into();
    draft.mount = draft.door;
    assert!(draft.proposed(&app).is_none());
    app.modals.set_hinge(Some(draft));
    let ctx = egui::Context::default();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_hinge_dialog(ui.ctx())
    })
    .drop_without_applying_deltas();
    assert_eq!(app.editor.project(), &before);
    ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| app.show_hinge_dialog(ui.ctx()),
    )
    .drop_without_applying_deltas();
    assert!(app.modals.hinge().is_none());
    assert_eq!(app.editor.project(), &before);
}

#[test]
fn valid_and_unsupported_preview_and_refresh() {
    let mut app = fixture();
    let mut draft = HingeDialog::new(&app, None);
    let proposed = draft.proposed(&app).unwrap();
    assert!(
        hinge_installation::preview(app.editor.project(), &proposed)
            .unwrap()
            .issues
            .is_empty()
    );
    draft.values[3] = "18".into();
    let unsupported =
        hinge_installation::preview(app.editor.project(), &draft.proposed(&app).unwrap()).unwrap();
    assert!(
        unsupported
            .issues
            .contains(&InstallationIssue::UnsupportedOverlay)
    );
    assert!(unsupported.references.is_none());
    let id = proposed.catalog_id;
    let door_y = proposed.door_y;
    hinge_installation::create(&mut app.editor, proposed).unwrap();
    let (_, statuses) =
        hardware_catalog::update_from_builtin_with_status(&mut app.editor, id).unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(statuses[0].issues.is_empty());
    app.editor
        .transact(|p| -> Result<(), ()> {
            p.boards[0].width = Length::from_micrometres(20_000);
            Ok(())
        })
        .unwrap();
    assert!(
        hinge_installation::diagnose_all(app.editor.project())[0]
            .issues
            .contains(&InstallationIssue::CupOutsideDoor)
    );
    let (_, statuses) =
        hardware_catalog::update_from_builtin_with_status(&mut app.editor, id).unwrap();
    assert!(
        statuses[0]
            .issues
            .contains(&InstallationIssue::CupOutsideDoor)
    );
    // Refreshing the catalog never moves the hinge.
    assert_eq!(app.editor.project().hinge_installations[0].door_y, door_y);
}

fn inspector_text(app: &mut DesktopApp, id: Uuid) -> String {
    let ctx = egui::Context::default();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600.0, 3000.0),
            )),
            ..Default::default()
        },
        |ui| {
            app.show_selected_installation_inspector(ui, id);
        },
    );
    let label = app.localizer.text("hardware-reference-details");
    let point = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap();
    for pressed in [true, false] {
        output.drop_without_applying_deltas();
        output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 3000.0),
                )),
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
            },
            |ui| app.show_selected_installation_inspector(ui, id),
        );
    }
    let text = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    output.drop_without_applying_deltas();
    text
}

fn tree_text(app: &mut DesktopApp) -> String {
    let ctx = egui::Context::default();
    let catalog = app.editor.project().catalog[0].id;
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_hardware_panel(ui);
        app.show_fitting_inspector(ui, InspectorTarget::Catalog(catalog));
    });
    let text = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    output.drop_without_applying_deltas();
    text
}

#[test]
fn offline_tree_groups_by_membership_and_tracks_selected_warning() {
    let mut app = fixture();
    let first = HingeDialog::new(&app, None).proposed(&app).unwrap();
    let first_id = first.id;
    hinge_installation::create(&mut app.editor, first).unwrap();
    let second = HingeDialog::new(&app, None).proposed(&app).unwrap();
    let second_id = second.id;
    hinge_installation::create(&mut app.editor, second).unwrap();
    let boards = &app.editor.project().boards;
    let proposal = plan_my_cabinet::door_joint::preview(
        app.editor.project(),
        Uuid::new_v4(),
        boards[0].id,
        boards[1].id,
        vec![first_id],
    )
    .unwrap();
    plan_my_cabinet::door_joint::confirm(&mut app.editor, proposal).unwrap();
    let (doors, standalone) = installation_groups(app.editor.project());
    assert_eq!(doors.len(), 1);
    assert_eq!(doors[0].1, vec![first_id]);
    assert_eq!(standalone, vec![second_id]);
    assert!(app.navigate_session(Destination::Installation(second_id)));
    assert_eq!(
        app.session.inspector,
        Some(InspectorTarget::Installation(second_id))
    );
    assert_eq!(app.session.active, Workspace::Hardware);
    app.editor
        .transact(|p| -> Result<(), ()> {
            p.boards[0].thickness = Length::from_micrometres(14_000);
            Ok(())
        })
        .unwrap();
    let text = tree_text(&mut app);
    assert!(text.contains(hardware_catalog::KIT_ID));
    assert!(text.contains(hardware_catalog::PLATE_ID));
    assert!(text.contains("rev. May 2025"));
    assert!(text.contains("outside this hinge's documented range"));
    assert!(text.lines().any(|line| line == "Door"), "{text}");
    assert!(text.contains("on Side"));
    assert!(text.contains("Not on a door"));
    assert!(text.contains("Hinge 1") && text.contains("Hinge 2"));
    assert!(!text.contains(&first_id.to_string()));
}

#[test]
fn legacy_snapshot_is_labelled_without_bundled_evidence() {
    let mut app = fixture();
    app.editor
        .transact(|p| -> Result<(), ()> {
            p.catalog[0].verified_hinge = None;
            Ok(())
        })
        .unwrap();
    let text = tree_text(&mut app);
    assert!(text.contains(hardware_catalog::KIT_ID));
    assert!(text.contains("Verified installation evidence unavailable"));
    assert!(!text.contains("Documented K/R pairs"));
}

#[test]
fn selected_inspector_discloses_only_supported_derived_references() {
    let mut app = fixture();
    let mut draft = HingeDialog::new(&app, None);
    draft.values[0] = "70".into();
    draft.values[1] = "40".into();
    draft.side.door_edge = BoardEdge::MaxX;
    draft.side.door_face = BoardFace::MaxZ;
    draft.side.mount_front_edge = BoardEdge::MaxX;
    draft.side.mount_face = BoardFace::MinZ;
    let proposed = draft.proposed(&app).unwrap();
    assert_eq!(proposed.id, draft.proposed(&app).unwrap().id);
    let id = proposed.id;
    hinge_installation::create(&mut app.editor, proposed).unwrap();
    let text = inspector_text(&mut app, id);
    assert!(text.lines().any(|line| line == "70"), "{text}");
    assert!(text.lines().any(|line| line == "40"), "{text}");
    assert!(text.contains("3 / 15") && text.contains("4 / 16"), "{text}");
    assert!(text.contains("X 79.5")); // cup X from the opposite door edge
    assert!(text.contains("X 63 ")); // plate X from the opposite front edge
    assert!(text.contains("May 2025 catalog"));
    assert!(text.contains("Fastener drilling not available"));
    assert!(!text.contains(".000"), "numbers are trimmed: {text}");
    assert!(!text.contains(&id.to_string()));

    app.editor
        .transact(|p| -> Result<(), ()> {
            p.hinge_installations[0].overlay = Length::from_micrometres(17_000);
            Ok(())
        })
        .unwrap();
    let text = inspector_text(&mut app, id);
    assert!(text.contains("is not in the hinge's table"));
    assert!(!text.contains("Board-local reference diagram"));
    assert!(!text.contains("79.5"));
    app.editor.undo().unwrap();

    app.editor
        .transact(|p| -> Result<(), ()> {
            p.boards[0].thickness = Length::from_micrometres(14_000);
            Ok(())
        })
        .unwrap();
    let text = inspector_text(&mut app, id);
    assert!(text.contains("Actual door thickness") && text.contains("14 mm"));
    assert!(text.contains("Numeric installation guidance unavailable"));
    assert!(!text.contains("79.5"));
    assert!(!text.contains("Board-local reference diagram"));

    app.editor
        .transact(|p| -> Result<(), ()> {
            p.catalog[0].verified_hinge = None;
            Ok(())
        })
        .unwrap();
    let text = inspector_text(&mut app, id);
    assert!(text.contains("Verified installation evidence unavailable"));
    assert!(!text.contains("Documented K/R pairs"));
    assert!(!text.contains("Ø35.000"));
}

#[test]
fn inspector_pair_segment_commits_one_undoable_edit() {
    let mut app = fixture();
    let proposed = HingeDialog::new(&app, None).proposed(&app).unwrap();
    let id = proposed.id;
    hinge_installation::create(&mut app.editor, proposed).unwrap();
    let before = app.editor.project().hinge_installations.clone();
    assert!(!app.apply_installation_edit(id, |_| {}));
    assert!(!app.apply_installation_edit(id, |i| { i.door_y = Length::from_micrometres(-1) }));
    assert_eq!(app.editor.project().hinge_installations, before);
    let ctx = egui::Context::default();
    let frame = |app: &mut DesktopApp, events: Vec<egui::Event>| {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320.0, 1400.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.show_selected_installation_inspector(ui, id),
        );
        let point = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "5 / 17" => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        });
        output.drop_without_applying_deltas();
        point
    };
    let point = frame(&mut app, vec![]).expect("K/R segment");
    for pressed in [true, false] {
        frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    let hinge = &app.editor.project().hinge_installations[0];
    assert_eq!(hinge.cup_edge_setback, Length::from_micrometres(5_000));
    assert_eq!(hinge.overlay, Length::from_micrometres(17_000));
    assert_eq!(hinge.door_y, before[0].door_y);
    app.editor.undo().unwrap();
    assert_eq!(app.editor.project().hinge_installations, before);
}

#[test]
fn independent_draft_coordinates_and_pairs_never_commit_on_preview() {
    let app = fixture();
    let before = app.editor.project().clone();
    let mut draft = HingeDialog::new(&app, None);
    draft.values[0] = "60".into();
    draft.values[1] = "45".into();
    draft.values[2] = "6".into();
    draft.values[3] = "18".into();
    draft.side.mount_face = BoardFace::MaxZ;
    let status =
        hinge_installation::preview(app.editor.project(), &draft.proposed(&app).unwrap()).unwrap();
    assert!(status.issues.is_empty());
    assert_eq!(
        status.references.unwrap().plate_hole_centers_um[0][1],
        29_000
    );
    draft.values[3] = "17".into();
    let status =
        hinge_installation::preview(app.editor.project(), &draft.proposed(&app).unwrap()).unwrap();
    assert!(
        status
            .issues
            .contains(&InstallationIssue::UnsupportedOverlay)
    );
    assert!(status.references.is_none());
    draft.values[0] = "1/64 in".into();
    assert!(draft.proposed(&app).is_none());
    assert_eq!(app.editor.project(), &before);
}

#[test]
fn new_hinge_finds_the_side_a_free_spot_and_lines_up() {
    use plan_my_cabinet::reference_fixture::{LEFT_DOOR_ID, LEFT_SIDE_ID};
    let mut app = DesktopApp {
        editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
        ..Default::default()
    };
    app.selection.active = Some(LEFT_DOOR_ID);
    let draft = HingeDialog::new(&app, None);
    assert_eq!(
        (draft.door, draft.mount),
        (Some(LEFT_DOOR_ID), Some(LEFT_SIDE_ID))
    );
    assert!(draft.auto && draft.fit_error.is_none());
    let proposed = draft.proposed(&app).unwrap();
    // The door already has hinges at 100 and 616 mm; the plate follows the
    // cup up the 2 mm the door sits above the side.
    assert_eq!(proposed.door_y, Length::from_micrometres(358_000));
    assert_eq!(proposed.mount_y, Length::from_micrometres(360_000));
    assert_eq!(
        proposed.side,
        app.editor.project().hinge_installations[0].side
    );
    assert!(
        hinge_installation::preview(app.editor.project(), &proposed)
            .unwrap()
            .issues
            .is_empty()
    );
}
