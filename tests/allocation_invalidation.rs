//! Design edits retain physical placements; feasibility is derived from current inputs.
use plan_my_cabinet::board_dimensions::BoardDimension;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::cut_tree::{
    CutError, Reconstruction, ReconstructionViolation, reconstruct_witness,
};
use plan_my_cabinet::domain::{
    Allocation, Board, BoardGrain, Material, Project, Stock, StockGrain, StockSource,
};
use plan_my_cabinet::material_changes::{ConflictReason, DependantChoice, allocation_conflicts};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::stock_commands::StockInput;
use plan_my_cabinet::units::{Anchor, Length, Pose, Quaternion};
use uuid::Uuid;

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}

fn fixture() -> ProjectEditor {
    let mut project = Project::new("Cabinet", Currency::Brl);
    project.cutting_kerf = mm(5);
    let material = Uuid::new_v4();
    let stock = Uuid::new_v4();
    project.materials.push(Material {
        id: material,
        name: "plywood".into(),
        default_thickness: mm(18),
        default_grain: BoardGrain::Length,
    });
    project.stock.push(Stock {
        id: stock,
        name: "sheet".into(),
        material_id: material,
        length: mm(205),
        width: mm(50),
        thickness: mm(18),
        grain: StockGrain::AlongX,
        source: StockSource::Owned,
        price: None,
        priority: 0,
        trim: [Length::ZERO; 4],
    });
    for x in [0, 105] {
        let board = Uuid::new_v4();
        project.boards.push(Board {
            id: board,
            name: "shelf".into(),
            material_id: material,
            length: mm(100),
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        });
        project.allocations.push(Allocation {
            id: Uuid::new_v4(),
            board_id: board,
            stock_id: stock,
            origin: [mm(x), Length::ZERO],
            quarter_turn: false,
            locked: x == 0,
        });
    }
    ProjectEditor::new(project).unwrap()
}

fn witness(editor: &ProjectEditor) -> Reconstruction {
    let p = editor.project();
    reconstruct_witness(p, p.stock[0].id, p.cutting_kerf, 20_000)
}

fn assert_restored(editor: &mut ProjectEditor, before: &[Allocation]) {
    assert_eq!(editor.project().allocations, before);
    assert!(!matches!(witness(editor), Reconstruction::Verified { .. }));
    editor.undo().unwrap();
    assert_eq!(editor.project().allocations, before);
    assert!(allocation_conflicts(editor.project()).is_empty());
    assert!(matches!(witness(editor), Reconstruction::Verified { .. }));
}

#[test]
fn locked_shelf_growth_implicates_both_allocations_and_one_undo_restores_the_witness() {
    let mut editor = fixture();
    let before = editor.project().allocations.clone();
    assert!(matches!(witness(&editor), Reconstruction::Verified { .. }));
    let preview = editor
        .preview_board_dimension(
            before[0].board_id,
            BoardDimension::Length,
            mm(108),
            Anchor::Start,
        )
        .unwrap();
    let conflicts = editor.edit_board_dimension(preview).unwrap();
    for allocation in &before {
        assert!(
            conflicts.iter().any(|c| c.board_id == allocation.board_id
                && c.reasons.contains(&ConflictReason::Overlap))
        );
    }
    assert_eq!(editor.project().boards[0].length, mm(108));
    assert_restored(&mut editor, &before);
    assert_eq!(editor.project().boards[0].length, mm(100));
}

#[test]
fn grain_and_stock_property_edits_invalidate_in_place_and_undo() {
    for change in 0..5 {
        let mut editor = fixture();
        let before = editor.project().allocations.clone();
        match change {
            0 => {
                editor
                    .set_board_grain_override(before[0].board_id, Some(BoardGrain::Width))
                    .unwrap();
            }
            1..=4 => {
                let stock = editor.project().stock[0].clone();
                let mut input = StockInput::from(&stock);
                match change {
                    1 => input.length = mm(200),
                    2 => {
                        input.material_id = editor
                            .create_material(plan_my_cabinet::board_commands::NewMaterial {
                                name: "MDF".into(),
                                thickness: mm(18),
                                grain: BoardGrain::Length,
                            })
                            .unwrap()
                    }
                    3 => input.thickness = mm(15),
                    _ => input.grain = StockGrain::AlongY,
                }
                editor.edit_stock(stock.id, input).unwrap();
            }
            _ => unreachable!(),
        }
        let reasons: Vec<_> = allocation_conflicts(editor.project())
            .into_iter()
            .flat_map(|c| c.reasons)
            .collect();
        assert!(reasons.contains(&match change {
            0 | 4 => ConflictReason::Grain,
            1 => ConflictReason::OutsideStock,
            2 => ConflictReason::MaterialIdentity,
            _ => ConflictReason::EffectiveThickness,
        }));
        assert_restored(&mut editor, &before);
    }
}

#[test]
fn applied_material_default_invalidates_locked_board_without_touching_measured_stock() {
    let mut editor = fixture();
    let before = editor.project().allocations.clone();
    let stock = editor.project().stock[0].clone();
    let material = editor.project().materials[0].id;
    let preview = editor
        .preview_material_change(
            material,
            "plywood".into(),
            mm(15),
            BoardGrain::Width,
            Anchor::Centre,
        )
        .unwrap();
    let issues = editor
        .apply_material_change(preview, DependantChoice::ApplyAll)
        .unwrap();
    assert_eq!(editor.project().stock[0], stock);
    assert!(
        issues
            .iter()
            .all(|issue| issue.reasons.contains(&ConflictReason::EffectiveThickness))
    );
    assert_restored(&mut editor, &before);
    assert_eq!(editor.project().materials[0].default_thickness, mm(18));
}

#[test]
fn enlarged_kerf_and_trim_changes_discard_old_witness_without_repacking() {
    for change in 0..2 {
        let mut editor = fixture();
        let before = editor.project().allocations.clone();
        if change == 0 {
            editor.set_cutting_kerf(mm(6)).unwrap();
        } else {
            let stock = editor.project().stock[0].clone();
            let mut input = StockInput::from(&stock);
            input.trim[0] = mm(5);
            editor.edit_stock(stock.id, input).unwrap();
        }
        assert_restored(&mut editor, &before);
        assert!(matches!(witness(&editor), Reconstruction::Verified { .. }));
    }

    // A trim that becomes smaller than the current blade is a cut violation,
    // even though both placement records remain structurally valid.
    let mut editor = fixture();
    let stock = editor.project().stock[0].clone();
    let mut input = StockInput::from(&stock);
    input.trim[0] = mm(5);
    editor.edit_stock(stock.id, input).unwrap();
    editor.set_cutting_kerf(mm(6)).unwrap();
    assert_eq!(editor.project().allocations.len(), 2);
    assert!(matches!(
        witness(&editor),
        Reconstruction::RuleViolation(ReconstructionViolation::Cut(CutError::SubKerfEdge))
    ));
}
