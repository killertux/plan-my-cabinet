use super::*;
use crate::allocation_diagnostics::{Status, diagnose};
use crate::domain::{Board, Material};
use crate::money::Currency;
use crate::units::{Pose, Quaternion};

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}

fn project(boards: &[(i64, i64)], sheets: &[(i64, i64)]) -> Project {
    let mut project = Project::new("pack", Currency::Brl);
    let material = Uuid::from_u128(1);
    project.materials.push(Material {
        coating: Default::default(),
        default_band: None,
        kind: Default::default(),
        id: material,
        name: "MDF".into(),
        default_thickness: mm(15),
        default_grain: BoardGrain::Unrestricted,
    });
    for (index, (l, w)) in boards.iter().enumerate() {
        project.boards.push(Board {
            coated_face: Default::default(),
            banding: Default::default(),
            id: Uuid::from_u128(100 + index as u128),
            name: format!("b{index}"),
            material_id: material,
            length: mm(*l),
            width: mm(*w),
            thickness: mm(15),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
        });
    }
    for (index, (l, w)) in sheets.iter().enumerate() {
        project.stock.push(Stock {
            id: Uuid::from_u128(200 + index as u128),
            name: "sheet".into(),
            material_id: material,
            length: mm(*l),
            width: mm(*w),
            thickness: mm(15),
            grain: StockGrain::Nondirectional,
            source: StockSource::ToPurchase,
            price: None,
            priority: index as u32,
            trim: [Length::ZERO; 4],
        });
    }
    project
}

fn apply(project: &Project, result: &PackResult) -> Project {
    let mut copy = project.clone();
    copy.allocations = result.allocations.clone();
    copy
}

fn all_valid(project: &Project) -> bool {
    diagnose(project)
        .iter()
        .all(|d| d.status == Status::AllocatedValid)
}

#[test]
fn exact_kerf_fit_packs_and_proves() {
    // 100 + 5 + 100 + 5 + 100 = 310 with the default kerf
    let project = project(&[(100, 50), (100, 50), (100, 50)], &[(310, 50)]);
    let result = pack(&project, PackMode::Replan);
    assert!(result.is_complete(), "{:?}", result.unplaced);
    let placed = apply(&project, &result);
    assert!(all_valid(&placed));
    assert!(result.candidate(&project).is_some());
}

#[test]
fn many_parts_fill_sheets_and_every_sheet_is_proven() {
    let boards: Vec<_> = (0..40)
        .map(|i| (300 + (i * 37) % 500, 100 + (i * 53) % 400))
        .collect();
    let project = project(&boards, &[(2750, 1840), (2750, 1840), (2750, 1840)]);
    let result = pack(&project, PackMode::Replan);
    assert!(result.is_complete(), "{:?}", result.unplaced);
    assert!(all_valid(&apply(&project, &result)));
    let candidate = result.candidate(&project).expect("complete candidate");
    assert_eq!(candidate.witnesses.len(), result.sheets_used());
}

#[test]
fn fill_gaps_keeps_existing_placements() {
    let mut project = project(&[(1000, 500), (1000, 500)], &[(2750, 1840)]);
    let first = pack(&project, PackMode::Replan);
    project.allocations = first.allocations.clone();
    let before = project.allocations.clone();
    project.boards.push(Board {
        id: Uuid::from_u128(150),
        name: "new".into(),
        ..project.boards[0].clone()
    });
    let result = pack(&project, PackMode::FillGaps);
    assert!(result.is_complete());
    assert_eq!(result.changed, vec![Uuid::from_u128(150)]);
    for allocation in &before {
        assert!(result.allocations.contains(allocation));
    }
    assert!(all_valid(&apply(&project, &result)));
}

#[test]
fn replan_keeps_locked_placements() {
    let mut project = project(&[(1000, 500), (400, 300)], &[(2750, 1840)]);
    project.allocations.push(Allocation {
        id: Uuid::from_u128(900),
        board_id: Uuid::from_u128(100),
        stock_id: Uuid::from_u128(200),
        origin: [mm(700), mm(600)],
        quarter_turn: false,
        locked: true,
    });
    let result = pack(&project, PackMode::Replan);
    assert!(result.is_complete());
    assert!(result.allocations.contains(&project.allocations[0]));
    assert!(all_valid(&apply(&project, &result)));
}

#[test]
fn reasons_name_what_is_missing() {
    let mut project = project(&[(3000, 100), (500, 500), (500, 500)], &[(2750, 600)]);
    let result = pack(&project, PackMode::Replan);
    let reason = |id: u128| {
        result
            .unplaced
            .iter()
            .find(|(b, _)| *b == Uuid::from_u128(id))
            .map(|(_, r)| *r)
    };
    assert_eq!(reason(100), Some(Unplaced::TooLarge));
    assert_eq!(reason(101), None);
    project.stock.clear();
    let result = pack(&project, PackMode::Replan);
    assert!(result.unplaced.iter().all(|(_, r)| *r == Unplaced::NoStock));
}

#[test]
fn grain_blocks_the_only_orientation_that_fits() {
    let mut project = project(&[(500, 2000)], &[(2750, 1840)]);
    project.materials[0].default_grain = BoardGrain::Length;
    project.stock[0].grain = StockGrain::AlongX;
    let result = pack(&project, PackMode::Replan);
    assert_eq!(
        result.unplaced,
        vec![(Uuid::from_u128(100), Unplaced::Grain)]
    );
}

#[test]
fn suggestion_counts_the_sheets_that_make_it_fit() {
    // Six 1300 × 900 parts: two per 2750 × 1840 sheet at most (with kerf).
    let project = project(&[(1300, 900); 6], &[(2750, 1840)]);
    let suggestions = suggest_sheets(&project);
    assert_eq!(suggestions.len(), 1);
    let sheet = suggestions[0]
        .sheet
        .as_ref()
        .expect("sheet size from declared stock");
    assert_eq!((sheet.length, sheet.width), (mm(2750), mm(1840)));
    let mut trial = project.clone();
    trial.stock.extend(suggested_stock(
        &project,
        (project.materials[0].id, mm(15)),
        sheet,
    ));
    assert!(pack(&trial, PackMode::FillGaps).is_complete());
    let mut fewer = project.clone();
    let mut short = sheet.clone();
    short.count -= 1;
    fewer.stock.extend(suggested_stock(
        &project,
        (project.materials[0].id, mm(15)),
        &short,
    ));
    assert!(!pack(&fewer, PackMode::FillGaps).is_complete());
}

#[test]
fn no_stock_suggests_the_preset_sheet() {
    let mut project = project(&[(500, 400)], &[]);
    project.materials[0].name = "White MDF".into();
    let suggestions = suggest_sheets(&project);
    assert_eq!(suggestions[0].waiting, 1);
    // A custom material with no declared stock has no size to suggest.
    assert!(suggestions[0].sheet.is_none() || suggestions[0].sheet.as_ref().unwrap().count == 1);
}
