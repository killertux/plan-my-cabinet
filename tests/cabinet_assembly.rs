//! An example body constructed through the same public edits as the sidebar,
//! without a cabinet preset or directly inserting project geometry.
use std::collections::HashSet;

use plan_my_cabinet::assembly_edit::world_pose;
use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
use plan_my_cabinet::board_dimensions::BoardDimension;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::{BoardGrain, Project};
use plan_my_cabinet::measurements::{Frame, Scope, measure};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::placement::{
    Align, BoardFace, CoordinateFrame, FacePlacement, NumericPose, PlacementSession, Side,
};
use plan_my_cabinet::units::{Anchor, Length, Pose, Quaternion};
use uuid::Uuid;

fn mm(value: i64) -> Length {
    Length::from_micrometres(value * 1000)
}

fn position(editor: &mut ProjectEditor, id: Uuid, at: [f64; 3], rotation: [f64; 3]) {
    let mut session = PlacementSession::begin(editor, id).unwrap();
    session
        .preview_numeric(NumericPose {
            frame: CoordinateFrame::World,
            position_mm: at,
            rotation_degrees_xyz: rotation,
        })
        .unwrap();
    session.accept().unwrap();
}

fn assert_position(editor: &ProjectEditor, id: Uuid, expected: [f64; 3]) {
    let actual = world_pose(editor.project(), id).unwrap().translation_mm;
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
    }
}

#[test]
fn builds_independent_cabinet_body_from_boards_and_explicit_placement() {
    let mut editor = ProjectEditor::new(Project::new("Example body", Currency::Brl)).unwrap();
    let material = editor
        .create_material(NewMaterial {
            name: "18 mm panel".into(),
            thickness: mm(18),
            grain: BoardGrain::Length,
        })
        .unwrap();
    let mut board = |name: &str, length| {
        editor
            .create_board(NewBoard {
                name: name.into(),
                material_id: material,
                length: mm(length),
                width: mm(600),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap()
    };
    let left = board("Left side", 2300);
    let bottom = board("Bottom", 784);
    let shelf = board("Shelf", 784);
    position(&mut editor, left, [18.0, 0.0, 0.0], [0.0, -90.0, 0.0]);
    position(&mut editor, bottom, [18.0, 0.0, 0.0], [0.0; 3]);

    // The UI initially offsets a duplicate locally by 25 mm; numeric World pose
    // then positions it precisely, preserving its orientation and dimensions.
    let mut right_pose = editor
        .project()
        .boards
        .iter()
        .find(|b| b.id == left)
        .unwrap()
        .pose;
    right_pose.translation_mm[0] += 25.0;
    let right = editor.duplicate_board(left, right_pose).unwrap();
    position(&mut editor, right, [820.0, 0.0, 0.0], [0.0, -90.0, 0.0]);

    let z_minus = BoardFace {
        axis: 2,
        side: Side::Negative,
    };
    let z_plus = BoardFace {
        axis: 2,
        side: Side::Positive,
    };
    let mut session = PlacementSession::begin(&mut editor, shelf).unwrap();
    session
        .preview_face(FacePlacement {
            source_face: z_minus,
            target_id: bottom,
            target_face: z_plus,
            source_align: [Align::Start; 2],
            target_align: [Align::Start; 2],
            offset_mm: [0.0; 2],
            gap_mm: 1082.0,
        })
        .unwrap();
    session.accept().unwrap();
    assert_position(&editor, shelf, [18.0, 0.0, 1100.0]);

    let shelf_copy = editor
        .project()
        .boards
        .iter()
        .find(|b| b.id == shelf)
        .unwrap()
        .pose;
    let upper_shelf = editor.duplicate_board(shelf, shelf_copy).unwrap();
    position(&mut editor, upper_shelf, [18.0, 0.0, 1500.0], [0.0; 3]);
    let bottom_pose = editor
        .project()
        .boards
        .iter()
        .find(|b| b.id == bottom)
        .unwrap()
        .pose;
    let top = editor.duplicate_board(bottom, bottom_pose).unwrap();
    position(&mut editor, top, [18.0, 0.0, 2282.0], [0.0; 3]);

    let ids = [left, right, bottom, shelf, upper_shelf, top];
    assert_eq!(ids.into_iter().collect::<HashSet<_>>().len(), 6);
    let body = editor.group_objects(&ids, None, "Body", [0.0; 3]).unwrap();
    for (id, expected, dimensions) in [
        (left, [18.0, 0.0, 0.0], [mm(2300), mm(600), mm(18)]),
        (right, [820.0, 0.0, 0.0], [mm(2300), mm(600), mm(18)]),
        (bottom, [18.0, 0.0, 0.0], [mm(784), mm(600), mm(18)]),
        (shelf, [18.0, 0.0, 1100.0], [mm(784), mm(600), mm(18)]),
        (upper_shelf, [18.0, 0.0, 1500.0], [mm(784), mm(600), mm(18)]),
        (top, [18.0, 0.0, 2282.0], [mm(784), mm(600), mm(18)]),
    ] {
        let part = editor.project().boards.iter().find(|b| b.id == id).unwrap();
        assert_eq!(part.parent_id, Some(body));
        assert_eq!(part.blank_dimensions(), dimensions);
        assert_eq!(part.material_id, material);
        assert_position(&editor, id, expected);
    }
    let bounds = measure(editor.project(), &[body], Scope::Body, Frame::World).unwrap();
    assert_eq!(bounds.board_count, 6);
    for (actual, expected) in bounds.dimensions_mm.into_iter().zip([820.0, 600.0, 2300.0]) {
        assert!((actual - expected).abs() < 1e-6);
    }
    assert!(editor.project().allocations.is_empty());
    assert!(editor.project().hardware.is_empty());

    // Placing against a face did not create an ongoing relationship: moving
    // the target board alone leaves the shelf at its committed world pose.
    position(&mut editor, bottom, [18.0, 10.0, 0.0], [0.0; 3]);
    assert_position(&editor, shelf, [18.0, 0.0, 1100.0]);
    let resize = editor
        .preview_board_dimension(right, BoardDimension::Length, mm(2200), Anchor::Start)
        .unwrap();
    editor.edit_board_dimension(resize).unwrap();
    assert_eq!(
        editor
            .project()
            .boards
            .iter()
            .find(|b| b.id == right)
            .unwrap()
            .length,
        mm(2200)
    );
    assert_eq!(
        editor
            .project()
            .boards
            .iter()
            .find(|b| b.id == left)
            .unwrap()
            .length,
        mm(2300)
    );
    assert_position(&editor, left, [18.0, 0.0, 0.0]);
    assert_eq!(editor.project().boards.len(), 6);
    editor.project().validate().unwrap();
}
