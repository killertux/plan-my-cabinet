//! Reproducible stock/assembly round trip through the public editor and file APIs.
use std::fs::File;

use plan_my_cabinet::assembly_edit::world_pose;
use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
use plan_my_cabinet::board_dimensions::BoardDimension;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::cut_tree::{Reconstruction, reconstruct_witness};
use plan_my_cabinet::domain::{BoardGrain, Project, StockGrain, StockSource};
use plan_my_cabinet::first_fit::FirstFit;
use plan_my_cabinet::material_changes::{ConflictReason, allocation_conflicts};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence::{prepare_reader, save};
use plan_my_cabinet::sheet_edit::{PlacementIssue, SheetEditError, SheetEditSession, SheetStatus};
use plan_my_cabinet::stock_commands::StockInput;
use plan_my_cabinet::units::{Anchor, Length, Pose, Quaternion};

fn mm(value: i64) -> Length {
    Length::from_micrometres(value * 1000)
}

#[test]
fn create_resize_conflict_repair_save_reopen_preserves_assembly_and_cut_witness() {
    let mut editor = ProjectEditor::new(Project::new("Cabinet", Currency::Brl)).unwrap();
    let material = editor
        .create_material(NewMaterial {
            name: "Plywood".into(),
            thickness: mm(18),
            grain: BoardGrain::Length,
        })
        .unwrap();
    let stock = editor
        .create_stock(
            StockInput {
                name: "Sheet".into(),
                material_id: material,
                length: mm(220),
                width: mm(50),
                thickness: mm(18),
                grain: StockGrain::AlongX,
                source: StockSource::Owned,
                price: None,
                trim: [Length::ZERO; 4],
            },
            1,
        )
        .unwrap()[0];
    let mut ids = Vec::new();
    for (index, x) in [10.0, 300.0].into_iter().enumerate() {
        let (id, fit) = editor
            .create_board_with_fit(NewBoard {
                name: format!("Shelf {index}"),
                material_id: material,
                length: mm(100),
                width: mm(50),
                pose: Pose::new([x, 20.0, 0.0], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        assert_eq!(fit, FirstFit::Allocated(stock));
        ids.push(id);
    }
    assert_eq!(editor.project().allocations[0].origin, [mm(0), mm(0)]);
    assert_eq!(editor.project().allocations[1].origin, [mm(105), mm(0)]);
    let assembly = editor
        .group_objects(&ids, None, "Cabinet body", [50.0, 10.0, 0.0])
        .unwrap();
    {
        let mut session = SheetEditSession::begin(&mut editor);
        session.set_lock(ids[1], true).unwrap();
        assert!(session.accept().unwrap());
    }
    let before = editor.project().clone();
    let end_before = world_pose(&before, ids[0])
        .unwrap()
        .transform_point([100.0, 0.0, 0.0])
        .unwrap();

    let resize = editor
        .preview_board_dimension(ids[0], BoardDimension::Length, mm(115), Anchor::End)
        .unwrap();
    let conflicts = editor.edit_board_dimension(resize).unwrap();
    assert!(ids.iter().all(|id| {
        conflicts
            .iter()
            .any(|c| c.board_id == *id && c.reasons.contains(&ConflictReason::Overlap))
    }));
    assert_eq!(editor.project().allocations, before.allocations);
    assert_eq!(editor.project().assemblies, before.assemblies);
    let resized = editor
        .project()
        .boards
        .iter()
        .find(|b| b.id == ids[0])
        .unwrap();
    assert_eq!(resized.parent_id, Some(assembly));
    assert_eq!(
        world_pose(editor.project(), ids[0])
            .unwrap()
            .transform_point([115.0, 0.0, 0.0])
            .unwrap(),
        end_before
    );
    let pose_after_resize = editor
        .project()
        .boards
        .iter()
        .map(|b| b.pose)
        .collect::<Vec<_>>();
    let draft = editor.project().clone();

    // A locked, design-invalidated allocation is still a conflict. Cancellation
    // discards both the tentative unlock and move, without changing the draft.
    {
        let mut session = SheetEditSession::begin(&mut editor);
        assert_eq!(
            session.place(ids[1], stock, [mm(120), mm(0)], false),
            Err(SheetEditError::Locked(ids[1]))
        );
        session.set_lock(ids[1], false).unwrap();
        session
            .place(ids[1], stock, [mm(110), mm(0)], false)
            .unwrap();
        assert!(matches!(
            session.diagnostics()[0].status,
            SheetStatus::Violation(PlacementIssue::Overlap(..))
        ));
        session.cancel();
    }
    assert_eq!(editor.project(), &draft);
    assert!(editor.project().allocations[1].locked);

    // Lock first, explicitly unlock, then stage a valid numeric sheet position.
    {
        let mut session = SheetEditSession::begin(&mut editor);
        assert_eq!(
            session.place(ids[1], stock, [mm(120), mm(0)], false),
            Err(SheetEditError::Locked(ids[1]))
        );
        session.set_lock(ids[1], false).unwrap();
        session
            .place(ids[1], stock, [mm(120), mm(0)], false)
            .unwrap();
        assert!(matches!(
            session.diagnostics()[0].status,
            SheetStatus::Verified(_)
        ));
        assert!(session.accept().unwrap());
    }
    let resolved = editor.project().clone();
    assert!(allocation_conflicts(&resolved).is_empty());
    assert_eq!(
        resolved.boards.iter().map(|b| b.pose).collect::<Vec<_>>(),
        pose_after_resize
    );
    assert_eq!(resolved.allocations[0], before.allocations[0]);
    assert_eq!(resolved.allocations[1].id, before.allocations[1].id);
    assert_eq!(resolved.allocations[1].origin, [mm(120), mm(0)]);
    let witness = match reconstruct_witness(&resolved, stock, resolved.cutting_kerf, 20_000) {
        Reconstruction::Verified { tree, .. } => tree,
        other => panic!("expected a cut witness: {other:?}"),
    };

    let directory = std::env::temp_dir().join(format!("pmcab-stock-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("cabinet.pmcab");
    save(&mut editor, &path).unwrap();
    let reopened = prepare_reader(File::open(&path).unwrap())
        .unwrap()
        .into_editor();
    assert_eq!(reopened.project().id, resolved.id);
    assert_eq!(reopened.project().boards, resolved.boards);
    assert_eq!(reopened.project().assemblies, resolved.assemblies);
    assert_eq!(reopened.project().stock, resolved.stock);
    assert_eq!(reopened.project().allocations, resolved.allocations);
    assert_eq!(
        world_pose(reopened.project(), ids[0]).unwrap(),
        world_pose(&resolved, ids[0]).unwrap()
    );
    assert!(matches!(
        reconstruct_witness(reopened.project(), stock, reopened.project().cutting_kerf, 20_000),
        Reconstruction::Verified { tree, .. } if tree == witness
    ));
    std::fs::remove_dir_all(directory).unwrap();
}
