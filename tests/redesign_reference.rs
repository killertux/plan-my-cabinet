//! The capture fixture must remain real, portable manufacturing input.
use plan_my_cabinet::{
    allocation_diagnostics::{self, Reason, Status, WITNESS_BUDGET},
    cost_estimate::{self, Feasibility},
    cut_tree::{CutKind, Reconstruction, reconstruct_witness, validate_witness},
    domain::{StockGrain, StockSource},
    door_joint,
    export::{ExportMode, ExportSettings, prepare_export},
    hardware_catalog,
    hinge_installation::{self, InstallationIssue},
    i18n::Language,
    measurements::{Frame, Scope, measure},
    persistence::{prepare_bytes, serialize},
    reference_fixture::*,
    units::Unit,
};

#[test]
fn stable_identity_and_serialization_round_trip() {
    let p = project();
    assert_eq!(p.validate(), Ok(()));
    let bytes = serialize(&p).unwrap();
    for _ in 0..3 {
        assert_eq!(bytes, serialize(&project()).unwrap());
    }
    let restored = prepare_bytes(&bytes).unwrap().into_editor();
    let loaded = restored.project();
    assert_eq!(loaded, &p, "opening must preserve derived axes exactly");
    assert_eq!(loaded.boards, p.boards);
    assert_eq!(loaded.materials, p.materials);
    assert_eq!(loaded.stock, p.stock);
    assert_eq!(loaded.allocations, p.allocations);
    assert_eq!(loaded.catalog, p.catalog);
    assert_eq!(loaded.hinge_installations, p.hinge_installations);
    for (actual, expected) in loaded.door_joints.iter().zip(&p.door_joints) {
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.moving_root_id, expected.moving_root_id);
        assert_eq!(actual.mounting_board_id, expected.mounting_board_id);
        assert_eq!(
            actual.hinge_installation_ids,
            expected.hinge_installation_ids
        );
        assert_eq!(actual.closed_local_pose, expected.closed_local_pose);
        assert_eq!(actual.closed_world_pose, expected.closed_world_pose);
        close(actual.axis_origin_mm, expected.axis_origin_mm);
        close(actual.axis_direction, expected.axis_direction);
    }
    assert_eq!(p.id, PROJECT_ID);
    assert_eq!(p.boards.len(), 9);
    assert_eq!(
        p.allocations.iter().map(|a| a.id).collect::<Vec<_>>(),
        ALLOCATION_IDS
    );
    assert_eq!(
        p.materials.iter().map(|m| m.id).collect::<Vec<_>>(),
        [WHITE_ID, OAK_ID, HDF_ID]
    );
    assert_eq!(
        p.ordered_stock().iter().map(|s| s.id).collect::<Vec<_>>(),
        [WHITE_STOCK_ID, OWNED_STOCK_ID, SPARE_STOCK_ID, OAK_STOCK_ID]
    );
    let owned = &p.stock[1];
    assert_eq!(owned.thickness.micrometres(), 18200);
    assert_eq!(owned.grain, StockGrain::Unknown);
    assert_eq!(owned.source, StockSource::Owned);
    assert_eq!(owned.price, None);
    assert!(p.export_records.is_empty());
}

#[test]
fn saved_reference_door_remains_previewable_after_repeated_reopen() {
    let mut current = project();
    for _ in 0..3 {
        let bytes = serialize(&current).unwrap();
        let loaded = prepare_bytes(&bytes).unwrap().into_editor();
        let left = loaded
            .project()
            .door_joints
            .iter()
            .find(|j| j.id == LEFT_JOINT_ID)
            .unwrap();
        assert!(
            !door_joint::needs_review(loaded.project(), left),
            "unchanged saved left door must not require reconfirmation"
        );
        assert_eq!(door_joint::opening_limit(loaded.project(), left), Ok(105.0));
        assert!(door_joint::derived_poses(loaded.project(), left, 60.0).is_ok());
        let right = loaded
            .project()
            .door_joints
            .iter()
            .find(|j| j.id == RIGHT_JOINT_ID)
            .unwrap();
        assert!(
            door_joint::needs_review(loaded.project(), right),
            "the intentionally invalid upper-right cup must still block motion"
        );
        assert_eq!(serialize(loaded.project()).unwrap(), bytes);
        assert!(!loaded.is_dirty());
        current = loaded.project().clone();
    }
}

fn close(actual: [f64; 3], expected: [f64; 3]) {
    for (a, e) in actual.into_iter().zip(expected) {
        assert!((a - e).abs() < 1e-9, "{actual:?} != {expected:?}");
    }
}

#[test]
fn world_bounds_match_html_solids_including_closed_doors() {
    let p = project();
    for (id, min, max) in [
        (LEFT_SIDE_ID, [0., 0., 0.], [18., 560., 720.]),
        (RIGHT_SIDE_ID, [782., 0., 0.], [800., 560., 720.]),
        (BOTTOM_ID, [18., 0., 0.], [782., 560., 18.]),
        (FRONT_RAIL_ID, [18., 0., 702.], [782., 100., 720.]),
        (BACK_RAIL_ID, [18., 460., 702.], [782., 560., 720.]),
        (SHELF_ID, [18., 20., 350.], [782., 557., 368.]),
        (BACK_ID, [18., 557., 18.], [782., 560., 702.]),
        (LEFT_DOOR_ID, [0., -18., 2.], [397., 0., 718.]),
        (RIGHT_DOOR_ID, [403., -18., 2.], [800., 0., 718.]),
    ] {
        let bounds = measure(&p, &[id], Scope::Body, Frame::World).unwrap();
        close(bounds.minimum_mm, min);
        close(bounds.maximum_mm, max);
    }
    let body = measure(&p, &[CARCASS_ID], Scope::Body, Frame::World).unwrap();
    close(body.dimensions_mm, [800., 560., 720.]);
    let all = measure(&p, &[CARCASS_ID, DOORS_ID], Scope::Body, Frame::World).unwrap();
    close(all.dimensions_mm, [800., 578., 720.]);
    assert_eq!(all.board_count, 9);
}

#[test]
fn real_witnesses_account_for_every_allocated_part_and_unused_piece() {
    let p = project();
    for stock in &p.stock {
        let Reconstruction::Verified { tree, .. } =
            reconstruct_witness(&p, stock.id, p.cutting_kerf, WITNESS_BUDGET)
        else {
            panic!("no real witness for {}", stock.name)
        };
        let area = validate_witness(&tree, &p, stock.id).unwrap();
        let (cuts, part_area, offcut_area, kerf_loss) = match stock.id {
            WHITE_STOCK_ID => (9, 1_797_308_000_000, 3_185_262_000_000, 49_930_000_000),
            OAK_STOCK_ID => (4, 568_504_000_000, 4_438_536_000_000, 25_460_000_000),
            OWNED_STOCK_ID => (0, 0, 540_000_000_000, 0),
            SPARE_STOCK_ID => (0, 0, 5_032_500_000_000, 0),
            _ => unreachable!(),
        };
        assert_eq!(tree.cut_count(), cuts);
        assert_eq!(
            (area.part_area, area.offcut_area, area.kerf_loss),
            (part_area, offcut_area, kerf_loss)
        );
        assert_eq!(
            area.root_area,
            area.part_area + area.offcut_area + area.waste_area + area.kerf_loss + area.trim_loss
        );
        assert_eq!(area.trim_loss, 0);
        assert_eq!(area.waste_area, 0);
        let mut parts: Vec<_> = tree
            .nodes()
            .iter()
            .filter_map(|n| {
                if let CutKind::Part(id) = n.kind {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();
        let mut allocated: Vec<_> = p
            .allocations
            .iter()
            .filter(|a| a.stock_id == stock.id)
            .map(|a| a.board_id)
            .collect();
        parts.sort();
        allocated.sort();
        assert_eq!(parts, allocated);
        assert_eq!(tree.operations().len(), tree.cut_count());
        if stock.id == OWNED_STOCK_ID || stock.id == SPARE_STOCK_ID {
            assert_eq!(tree.cut_count(), 0);
            assert_eq!(area.part_area, 0);
            assert_eq!(area.offcut_area, area.root_area);
        }
    }
    let diagnostics = allocation_diagnostics::diagnose(&p);
    assert_eq!(diagnostics.len(), 9);
    for d in diagnostics {
        if d.board_id == BACK_ID {
            assert_eq!(d.status, Status::Unallocated);
            assert_eq!(d.reasons, [Reason::MissingAllocation]);
        } else {
            assert_eq!(d.status, Status::AllocatedValid, "{d:?}");
            assert!(d.reasons.is_empty());
        }
    }
    let estimate = cost_estimate::estimate(&p).unwrap();
    assert_eq!(estimate.feasibility, Feasibility::Incomplete);
    assert_eq!(estimate.material.unwrap().minor_units(), 57980);
    assert_eq!(estimate.used_stock.len(), 2);
    assert_eq!(
        estimate
            .used_stock
            .iter()
            .map(|s| s.cuts.unwrap())
            .sum::<u64>(),
        13
    );
    assert_eq!(estimate.cutting, None);
    assert_eq!(estimate.total, None);
    let settings = ExportSettings {
        language: Language::En,
        units: Unit::Mm,
    };
    assert!(prepare_export(&p, settings, ExportMode::Draft).is_ok());
    assert!(prepare_export(&p, settings, ExportMode::ShopReady).is_err());
}

#[test]
fn four_pinned_hinges_and_two_relationships_retain_real_warning() {
    let p = project();
    assert_eq!(p.hinge_installations.len(), 4);
    assert_eq!(p.door_joints.len(), 2);
    assert!(
        p.hardware.is_empty(),
        "installations already count the four physical hinges"
    );
    assert!(hardware_catalog::is_verified(&p.catalog[0]));
    for (index, status) in hinge_installation::diagnose_all(&p).into_iter().enumerate() {
        assert_eq!(status.id, HINGE_IDS[index]);
        if index == 3 {
            assert_eq!(status.issues, [InstallationIssue::CupOutsideDoor]);
        } else {
            assert!(status.issues.is_empty(), "{status:?}");
        }
        let references = status.references.unwrap();
        assert_eq!(references.cup_diameter.micrometres(), 35000);
        assert_eq!(references.cup_depth.micrometres(), 11300);
        assert_eq!(references.plate_hole_pitch.micrometres(), 32000);
        assert_eq!(references.plate_front_offset.micrometres(), 37000);
        assert!(!references.fasteners_available);
    }
    let before = serialize(&p).unwrap();
    let left = &p.door_joints[0];
    assert_eq!(left.id, LEFT_JOINT_ID);
    close(left.axis_origin_mm, [0., 0., 102.]);
    // This fixture's MinX/MinZ cup face opens outward (world -Y),
    // rather than following the former always-positive local-Y axis.
    close(left.axis_direction, [0., 0., -1.]);
    assert!(!door_joint::needs_review(&p, left));
    let preview = door_joint::derived_poses(&p, left, 60.).unwrap();
    assert_eq!(preview.len(), 1);
    assert_eq!(preview[0].0, LEFT_DOOR_ID);
    assert_ne!(preview[0].1, left.closed_world_pose);
    let right = &p.door_joints[1];
    assert_eq!(right.id, RIGHT_JOINT_ID);
    assert!(door_joint::needs_review(&p, right));
    assert!(door_joint::derived_poses(&p, right, 60.).is_err());
    assert_eq!(serialize(&p).unwrap(), before);
}
