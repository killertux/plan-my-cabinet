use std::collections::HashSet;

use plan_my_cabinet::allocation_diagnostics::{Reason, Status};
use plan_my_cabinet::design_read_models::{
    DesignInspector, DesignReadModel, DesignView, GrainProvenance, ObjectKind, ThicknessProvenance,
};
use plan_my_cabinet::domain::{
    BoardGrain, DomainError, Hardware, HardwareKind, Project, StockSource,
};
use plan_my_cabinet::measurements::MeasurementError;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::reference_fixture::{self, BACK_ID, HDF_ID, WHITE_STOCK_ID};
use plan_my_cabinet::stock_read_models::StockReadModel;
use plan_my_cabinet::units::Length;
use uuid::Uuid;

fn project_model(
    project: &Project,
    selected: &HashSet<Uuid>,
    active: Option<Uuid>,
    hidden: &HashSet<Uuid>,
    expanded: &HashSet<Uuid>,
) -> DesignReadModel {
    let stock = StockReadModel::build(project).unwrap();
    DesignReadModel::build(
        project,
        &stock,
        DesignView {
            selected,
            active,
            hidden,
            expanded,
        },
    )
    .unwrap()
}

#[test]
fn empty_project_and_empty_selection_do_not_invent_inspector_values() {
    let p = Project::new("Empty", Currency::Brl);
    let empty = HashSet::new();
    let model = project_model(&p, &empty, None, &empty, &empty);
    assert!(model.outliner.is_empty());
    assert!(model.materials.is_empty());
    assert!(model.stock.is_empty());
    assert!(matches!(model.inspector, DesignInspector::None));
}

#[test]
fn hidden_descendant_issue_and_independent_expansion_remain_discoverable() {
    let p = reference_fixture::project();
    let assembly = p
        .boards
        .iter()
        .find(|b| b.id == BACK_ID)
        .unwrap()
        .parent_id
        .unwrap();
    let hidden = HashSet::from([assembly]);
    let selected = HashSet::from([BACK_ID]);
    let empty = HashSet::new();
    let collapsed = project_model(&p, &selected, Some(BACK_ID), &hidden, &empty);
    let ancestor = collapsed
        .outliner
        .iter()
        .find(|r| r.id == assembly)
        .unwrap();
    let back = collapsed.outliner.iter().find(|r| r.id == BACK_ID).unwrap();
    assert_eq!(ancestor.expanded, Some(false));
    assert!(!ancestor.visible);
    assert!(ancestor.issue_count >= 1);
    assert_eq!(back.parent_id, Some(assembly));
    assert!(!back.visible);
    assert!(!back.hidden_directly);
    assert!(back.active && back.selected);
    assert_eq!(
        back.allocation.as_ref().unwrap().status,
        Status::Unallocated
    );
    assert_eq!(back.issue_count, 1);
    assert!(
        back.allocation
            .as_ref()
            .unwrap()
            .reasons
            .contains(&Reason::MissingAllocation)
    );
    let expanded = project_model(
        &p,
        &selected,
        Some(BACK_ID),
        &hidden,
        &HashSet::from([assembly]),
    );
    assert_eq!(
        expanded
            .outliner
            .iter()
            .find(|r| r.id == assembly)
            .unwrap()
            .expanded,
        Some(true)
    );
    assert!(
        !expanded
            .outliner
            .iter()
            .find(|r| r.id == BACK_ID)
            .unwrap()
            .visible
    );
    let DesignInspector::Board(inspector) = collapsed.inspector else {
        panic!("board inspector")
    };
    assert!(!inspector.visible);
    assert_eq!(inspector.material_id, HDF_ID);
    assert!(inspector.stock.is_empty());
    assert_eq!(inspector.allocation.status, Status::Unallocated);
}

#[test]
fn equal_names_exact_board_properties_and_stock_identity() {
    let mut p = reference_fixture::project();
    let id = p.allocations[0].board_id;
    let another = p
        .boards
        .iter()
        .find(|b| b.id == BACK_ID)
        .unwrap()
        .name
        .clone();
    let b = p.boards.iter_mut().find(|b| b.id == id).unwrap();
    b.name = another;
    b.thickness = Length::from_micrometres(18_125);
    b.grain_override = Some(BoardGrain::Width);
    let b = b.clone();
    let selected = HashSet::from([id]);
    let empty = HashSet::new();
    let model = project_model(&p, &selected, Some(id), &empty, &empty);
    assert_eq!(
        model.outliner.iter().filter(|r| r.name == b.name).count(),
        2
    );
    assert_eq!(
        model
            .outliner
            .iter()
            .filter(|r| r.active)
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        vec![id]
    );
    let DesignInspector::Board(board) = &model.inspector else {
        panic!("board inspector")
    };
    assert_eq!(board.id, id);
    assert_eq!(board.dimensions[2], Length::from_micrometres(18_125));
    assert_eq!(
        board.material_default_thickness,
        Length::from_micrometres(18_000)
    );
    assert_eq!(
        board.thickness_provenance,
        ThicknessProvenance::StoredBoardValueDiffers
    );
    assert_eq!(board.grain, BoardGrain::Width);
    assert_eq!(board.grain_provenance, GrainProvenance::BoardOverride);
    assert_eq!(board.stock[0].stock_id, p.allocations[0].stock_id);
    assert_eq!(board.stock[0].allocation_id, p.allocations[0].id);
    assert_eq!(board.stock[0].origin, p.allocations[0].origin);
    assert_eq!(board.frame.local_pose, b.pose);
    assert_eq!(board.frame.local_to, b.parent_id);
    assert_eq!(
        board.bounds_object.body.as_ref().unwrap().dimensions_mm,
        [
            b.length.micrometres() as f64 / 1000.0,
            b.width.micrometres() as f64 / 1000.0,
            18.125
        ]
    );
    let sheet = model.stock.iter().find(|s| s.id == WHITE_STOCK_ID).unwrap();
    assert_eq!(sheet.source, StockSource::ToPurchase);
    assert_eq!(sheet.alias, "S1");
    assert_eq!(sheet.part_count, 6);
    assert_eq!(
        model
            .materials
            .iter()
            .find(|m| m.id == HDF_ID)
            .unwrap()
            .unallocated_board_count,
        1
    );
}

#[test]
fn assembly_and_mixed_selection_use_scoped_bounds_without_double_counting() {
    let mut p = reference_fixture::project();
    let hardware_id = Uuid::new_v4();
    p.hardware.push(Hardware {
        id: hardware_id,
        name: "Foot".into(),
        parent_id: p.boards[0].parent_id,
        pose: p.boards[0].pose,
        kind: HardwareKind::Placeholder {
            dimensions: [Length::from_micrometres(10_000); 3],
        },
    });
    let board_id = p.boards[0].id;
    let assembly_id = p.boards[0].parent_id.unwrap();
    let empty = HashSet::new();
    let one = project_model(
        &p,
        &HashSet::from([assembly_id]),
        Some(assembly_id),
        &empty,
        &empty,
    );
    let DesignInspector::Assembly(assembly) = one.inspector else {
        panic!("assembly inspector")
    };
    assert_eq!(assembly.id, assembly_id);
    assert!(assembly.descendant_board_count > 0);
    assert!(assembly.bounds_world.body.is_ok());
    assert!(assembly.bounds_object.body.is_ok());
    let selected = HashSet::from([board_id, assembly_id, hardware_id]);
    let multi = project_model(&p, &selected, Some(board_id), &empty, &empty);
    let DesignInspector::Multi(m) = multi.inspector else {
        panic!("multi inspector")
    };
    assert_eq!(
        (m.board_count, m.assembly_count, m.hardware_count),
        (1, 1, 1)
    );
    assert_eq!(m.active, Some(board_id));
    assert_eq!(
        m.bounds_world.body.as_ref().unwrap().board_count,
        assembly.bounds_world.body.as_ref().unwrap().board_count
    );
    assert_eq!(
        m.bounds_world.body.as_ref().unwrap().scope,
        plan_my_cabinet::measurements::Scope::Body
    );
    assert_eq!(m.ids.len(), 3);
    assert!(
        multi
            .outliner
            .iter()
            .any(|r| r.kind == ObjectKind::Hardware)
    );
}

#[test]
fn current_default_is_only_an_equality_and_material_grain_can_change() {
    let mut p = reference_fixture::project();
    let id = p.boards[0].id;
    let material_id = p.boards[0].material_id;
    p.boards[0].grain_override = None;
    p.materials
        .iter_mut()
        .find(|m| m.id == material_id)
        .unwrap()
        .default_grain = BoardGrain::Width;
    let empty = HashSet::new();
    let model = project_model(&p, &HashSet::from([id]), Some(id), &empty, &empty);
    let DesignInspector::Board(board) = model.inspector else {
        panic!("board")
    };
    assert_eq!(
        board.thickness_provenance,
        ThicknessProvenance::MatchesCurrentMaterialDefault
    );
    assert_eq!(board.grain, BoardGrain::Width);
    assert_eq!(board.grain_provenance, GrainProvenance::MaterialDefault);
}

#[test]
fn undimensioned_hardware_reports_unavailable_overall_extent() {
    let mut p = reference_fixture::project();
    let id = Uuid::new_v4();
    p.hardware.push(Hardware {
        id,
        name: "Catalog item".into(),
        parent_id: None,
        pose: p.boards[0].pose,
        kind: HardwareKind::Catalog {
            catalog_id: p.catalog[0].id,
        },
    });
    let empty = HashSet::new();
    let model = project_model(&p, &HashSet::from([id]), Some(id), &empty, &empty);
    let DesignInspector::Hardware(hardware) = model.inspector else {
        panic!("hardware")
    };
    assert_eq!(hardware.dimensions, None);
    assert_eq!(
        hardware.bounds_world.overall,
        Err(MeasurementError::UndimensionedHardware(id))
    );
}

#[test]
fn dangling_material_and_stock_are_rejected_not_replaced_by_example_values() {
    let empty = HashSet::new();
    let mut p = reference_fixture::project();
    p.boards[0].material_id = Uuid::new_v4();
    assert!(StockReadModel::build(&p).is_err());
    let stock = StockReadModel::build(&reference_fixture::project()).unwrap();
    assert!(matches!(
        DesignReadModel::build(
            &p,
            &stock,
            DesignView {
                selected: &empty,
                active: None,
                hidden: &empty,
                expanded: &empty,
            }
        ),
        Err(DomainError::DanglingReference { .. })
    ));

    let mut p = reference_fixture::project();
    p.allocations[0].stock_id = Uuid::new_v4();
    assert!(StockReadModel::build(&p).is_err());
    assert!(matches!(
        DesignReadModel::build(
            &p,
            &stock,
            DesignView {
                selected: &empty,
                active: None,
                hidden: &empty,
                expanded: &empty,
            }
        ),
        Err(DomainError::DanglingReference { .. })
    ));
}

#[test]
fn hidden_conflicted_board_keeps_global_diagnostics() {
    let mut p = reference_fixture::project();
    p.allocations[1].origin = p.allocations[0].origin;
    let id = p.allocations[1].board_id;
    let hidden = HashSet::from([id]);
    let selected = HashSet::from([id]);
    let empty = HashSet::new();
    let model = project_model(&p, &selected, Some(id), &hidden, &empty);
    let row = model.outliner.iter().find(|r| r.id == id).unwrap();
    assert!(!row.visible);
    assert_eq!(row.issue_count, 1);
    assert_eq!(row.allocation.as_ref().unwrap().status, Status::Conflicted);
    assert!(
        row.allocation
            .as_ref()
            .unwrap()
            .reasons
            .contains(&Reason::Overlap)
    );
    let DesignInspector::Board(board) = model.inspector else {
        panic!("board")
    };
    assert_eq!(board.stock[0].stock_id, p.allocations[1].stock_id);
    assert_eq!(board.stock[0].stock_alias.as_deref(), Some("S1"));
    assert_eq!(board.allocation.status, Status::Conflicted);
}
