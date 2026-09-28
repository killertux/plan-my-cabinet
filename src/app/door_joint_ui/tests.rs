use super::*;
use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
use plan_my_cabinet::domain::{
    BoardEdge, BoardFace, BoardGrain, HingeInstallation, HingeMountingSide,
};

fn fixture() -> (DesktopApp, Uuid, Uuid) {
    let mut app = DesktopApp::default();
    let material = app
        .editor
        .create_material(NewMaterial {
            name: "Wood".into(),
            thickness: Length::from_micrometres(18_000),
            grain: BoardGrain::Unrestricted,
        })
        .unwrap();
    let mut ids = Vec::new();
    for name in ["Door", "Mount"] {
        ids.push(
            app.editor
                .create_board(NewBoard {
                    name: name.into(),
                    material_id: material,
                    length: Length::from_micrometres(100_000),
                    width: Length::from_micrometres(100_000),
                    pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
                })
                .unwrap(),
        );
    }
    hardware_catalog::add_builtin(&mut app.editor).unwrap();
    let hinge = HingeInstallation {
        id: Uuid::new_v4(),
        door_board_id: ids[0],
        mounting_board_id: ids[1],
        catalog_id: app.editor.project().catalog[0].id,
        side: HingeMountingSide {
            door_edge: BoardEdge::MinX,
            door_face: BoardFace::MinZ,
            mount_front_edge: BoardEdge::MinX,
            mount_face: BoardFace::MaxZ,
        },
        door_y: Length::from_micrometres(50_000),
        mount_y: Length::from_micrometres(50_000),
        cup_edge_setback: Length::from_micrometres(3_000),
        overlay: Length::from_micrometres(15_000),
    };
    plan_my_cabinet::hinge_installation::create(&mut app.editor, hinge).unwrap();
    (app, ids[0], ids[1])
}

#[test]
fn angle_scale_tracks_slider_knob_and_keeps_labels_separate() {
    for width in [150.0, 300.0, 428.0] {
        for limit in [60.0, 90.0, 105.0, 110.0, 180.0] {
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            let mut angle = 45.0;
            let mut rail = egui::Rect::NOTHING;
            let mut travel = egui::Rangef::EVERYTHING;
            let output = ctx.run_ui(Default::default(), |ui| {
                ui.allocate_ui(egui::vec2(width, 200.0), |ui| {
                    let (response, range) = angle_slider(ui, &mut angle, limit, "Angle", true);
                    rail = response.rect;
                    travel = range;
                    show_motion_ticks(ui, travel, rail.x_range(), limit);
                });
            });
            let mut labels = Vec::new();
            let mut knob = None;
            for shape in &output.shapes {
                match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text().ends_with('°') => {
                        labels.push((
                            text.galley.text().to_owned(),
                            egui::Rect::from_min_size(text.pos, text.galley.size()),
                        ));
                    }
                    egui::Shape::Circle(circle)
                        if (circle.center.y - rail.center().y).abs() < 0.01
                            && (circle.radius - (KNOB_RADIUS - 1.0)).abs() < 0.01 =>
                    {
                        knob = Some(circle.center.x)
                    }
                    _ => {}
                }
            }
            let ticks = motion_ticks(limit);
            assert_eq!(labels.len(), ticks.len());
            assert!(
                (knob.expect("slider knob") - egui::lerp(travel, (45.0 / limit) as f32)).abs()
                    < 0.1,
                "knob must share the scale's range at width {width}, limit {limit}"
            );
            for (index, (text, label)) in labels.iter().enumerate() {
                assert!(
                    label.left() >= rail.left() - 0.1 && label.right() <= rail.right() + 0.1,
                    "{text} leaves the rail at width {width}"
                );
                assert!(
                    labels[index + 1..]
                        .iter()
                        .all(|(_, other)| !label.intersects(*other)),
                    "angle labels overlap at width {width}, limit {limit}: {labels:?}"
                );
                let value: f64 = text.trim_end_matches('°').parse().unwrap();
                let x = egui::lerp(travel, (value / limit) as f32);
                let clamped =
                    label.left() <= rail.left() + 0.1 || label.right() >= rail.right() - 0.1;
                assert!(
                    clamped || (label.center().x - x).abs() < 0.6,
                    "{text} is centred on its knob position"
                );
            }
            assert_eq!(angle, 45.0, "laying out the scale must not move the door");
            output.drop_without_applying_deltas();
        }
    }
}

#[test]
fn motion_controls_disclose_limit_and_exit_without_editing_selection_or_document() {
    assert_eq!(motion_ticks(90.0), [0.0, 45.0, 90.0]);
    assert_eq!(motion_ticks(105.0), [0.0, 45.0, 90.0, 105.0]);
    let (mut app, door, mount) = fixture();
    let proposed = door_joint::preview(
        app.editor.project(),
        Uuid::new_v4(),
        door,
        mount,
        vec![app.editor.project().hinge_installations[0].id],
    )
    .unwrap();
    door_joint::confirm(&mut app.editor, proposed).unwrap();
    app.selection.choose(Some(door), false);
    let original = app.editor.project().clone();
    let joint_id = original.door_joints[0].id;
    app.hardware.door_motion = Some((joint_id, 105.0));
    let ctx = egui::Context::default();
    for language in [Language::En, Language::PtBr] {
        app.localizer.set_language(language);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_door_motion_controls(ui)
        });
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        assert!(
            texts
                .iter()
                .any(|s| s == &app.localizer.text("hardware-motion-note"))
        );
        assert!(texts.iter().any(|s| s.contains("105")));
        output.drop_without_applying_deltas();
    }
    assert!(app.modal_open());
    assert_eq!(app.selection.active, Some(door));
    assert_eq!(app.editor.project(), &original);
    app.hardware.door_motion = None;
    assert!(!app.modal_open());
    assert_eq!(app.editor.project(), &original);
    assert_eq!(
        door_joint::derived_poses(app.editor.project(), &original.door_joints[0], 0.0)
            .unwrap()
            .into_iter()
            .find(|(id, _)| *id == door)
            .unwrap()
            .1,
        plan_my_cabinet::assembly_edit::world_pose(app.editor.project(), door).unwrap()
    );
}

#[test]
fn canvas_motion_card_is_bilingual_bounded_and_closed_restores_without_edit() {
    for language in [Language::En, Language::PtBr] {
        let mut app = DesktopApp {
            editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
            ..Default::default()
        };
        app.localizer.set_language(language);
        app.session.switch(Workspace::Hardware);
        app.session.inspector = Some(InspectorTarget::Installation(
            plan_my_cabinet::reference_fixture::HINGE_IDS[0],
        ));
        let original = app.editor.project().clone();
        let joint = original.door_joints[0].id;
        app.invoke(Request::with(A::StartMotion, Target::Door(joint)))
            .unwrap();
        app.invoke(
            Request::with(A::SetDoorAngle, Target::Door(joint)).argument(Argument::Angle(60.0)),
        )
        .unwrap();
        let ctx = egui::Context::default();
        let canvas = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(580.0, 620.0));
        let mut point = None;
        for _ in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640.0, 680.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    app.show_hardware_motion_overlay(ui.ctx(), canvas);
                },
            );
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape {
                    if text.galley.text() == app.localizer.text("hardware-closed") {
                        point = Some(text.pos + text.galley.size() * 0.5);
                    }
                    if text.galley.text() == app.localizer.text("hardware-motion-note") {
                        assert!(canvas.contains_rect(egui::Rect::from_min_size(
                            text.pos,
                            text.galley.size()
                        )));
                    }
                }
            }
            output.drop_without_applying_deltas();
        }
        let point = point.expect("Closed is reachable on the canvas");
        for pressed in [true, false] {
            ctx.run_ui(
                egui::RawInput {
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
                |ui| {
                    app.show_hardware_motion_overlay(ui.ctx(), canvas);
                },
            )
            .drop_without_applying_deltas();
        }
        assert!(app.hardware.door_motion.is_none());
        assert_eq!(app.editor.project(), &original);
    }
}

#[test]
fn relationship_draft_blocks_motion_without_losing_its_choices() {
    let mut app = DesktopApp {
        editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
        ..Default::default()
    };
    app.session.switch(Workspace::Hardware);
    let joint = app.editor.project().door_joints[0].id;
    let original = app.editor.project().clone();
    let draft = DoorDialog::new(&app, Some(joint));
    let selected = (draft.root, draft.mount, draft.hinges.clone());
    app.modals.set_door(Some(draft));
    assert!(
        app.invoke(Request::with(A::StartMotion, Target::Door(joint)))
            .is_err()
    );
    let retained = app.modals.door().unwrap();
    assert_eq!(
        (retained.root, retained.mount, retained.hinges.clone()),
        selected
    );
    assert!(app.hardware.door_motion.is_none());
    assert_eq!(app.editor.project(), &original);
}

#[test]
fn select_confirm_duplicate_cancel_and_delete_are_atomic() {
    let (mut app, door, mount) = fixture();
    let mut draft = DoorDialog::new(&app, None);
    draft.root = Some(door);
    draft.mount = Some(mount);
    draft.hinges = vec![app.editor.project().hinge_installations[0].id];
    let initial = app.editor.project().clone();
    let ctx = egui::Context::default();
    app.modals.set_door(Some(draft));
    ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_door_dialog(ui.ctx())
    })
    .drop_without_applying_deltas();
    assert_eq!(app.editor.project(), &initial);
    let draft = app.modals.take_door().unwrap();
    let proposal = draft.proposal(&app).unwrap();
    door_joint::confirm(&mut app.editor, proposal).unwrap();
    assert_eq!(app.editor.project().door_joints.len(), 1);
    let mut duplicate = DoorDialog::new(&app, None);
    duplicate.root = Some(door);
    duplicate.mount = Some(mount);
    duplicate.hinges = vec![app.editor.project().hinge_installations[0].id];
    assert!(duplicate.proposal(&app).is_err());
    let mut cycle = DoorDialog::new(&app, None);
    cycle.root = Some(door);
    cycle.mount = Some(door);
    cycle.hinges = duplicate.hinges;
    assert!(cycle.proposal(&app).is_err());
    let saved = serde_json::to_vec(app.editor.project()).unwrap();
    assert_eq!(
        plan_my_cabinet::persistence::prepare_bytes(&saved)
            .unwrap()
            .project()
            .door_joints,
        app.editor.project().door_joints
    );
    app.modals
        .set_removal(Some(RemovalDialog::new(&app, DoorRemoval::Object(door))));
    ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_removal_dialog(ui.ctx())
    })
    .drop_without_applying_deltas();
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
        |ui| app.show_removal_dialog(ui.ctx()),
    )
    .drop_without_applying_deltas();
    assert!(app.modals.removal().is_none());
    assert_eq!(app.editor.project().boards.len(), 2);
    door_joint::delete_object(&mut app.editor, door).unwrap();
    assert!(app.editor.project().door_joints.is_empty());
    assert!(app.editor.project().hinge_installations.is_empty());
    app.editor.undo().unwrap();
    assert_eq!(app.editor.project().door_joints.len(), 1);
    assert_eq!(app.editor.project().hinge_installations.len(), 1);
}
