//! Library commands behind the Hardware inspectors and "add then edit": each
//! is one undoable step, drafts commit only on accept, and catalog models
//! in use cannot be removed.
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::dimension_input::Locale;
use plan_my_cabinet::domain::{CatalogReference, HardwareKind};
use plan_my_cabinet::edit_drafts::{EditDrafts, FittingTarget};
use plan_my_cabinet::hardware_catalog::{self, CatalogKind};
use plan_my_cabinet::reference_fixture::{self, BOTTOM_ID, CARCASS_ID, LEFT_DOOR_ID, LEFT_SIDE_ID};
use plan_my_cabinet::units::{Length, Pose, Quaternion, Unit};
use plan_my_cabinet::{door_joint, hinge_installation};

fn editor() -> ProjectEditor {
    ProjectEditor::new(reference_fixture::project()).unwrap()
}

fn mm(v: i64) -> Length {
    Length::from_micrometres(v * 1000)
}

fn foot_entry() -> CatalogReference {
    let registry = plan_my_cabinet::catalog_pack::CatalogRegistry::bundled();
    let pack = registry.pack("generic-feet").unwrap();
    let family = pack.feet.iter().find(|f| f.id == "post-square").unwrap();
    plan_my_cabinet::catalog_pack::snapshot_foot(pack, family, &family.variants[1], "en")
}

#[test]
fn a_door_with_standard_hinges_is_one_step() {
    let mut editor = editor();
    let catalog = editor.project().hinge_installations[0].catalog_id;
    // Start from a door without hinges or a relationship.
    let door = LEFT_DOOR_ID;
    let joint = editor
        .project()
        .door_joints
        .iter()
        .find(|j| j.moving_root_id == door)
        .unwrap()
        .id;
    door_joint::remove(&mut editor, joint).unwrap();
    let hinges: Vec<_> = editor
        .project()
        .hinge_installations
        .iter()
        .filter(|h| h.door_board_id == door)
        .map(|h| h.id)
        .collect();
    for id in hinges {
        hinge_installation::remove(&mut editor, id).unwrap();
    }
    let set = hinge_installation::standard_set(editor.project(), door, LEFT_SIDE_ID, catalog, None)
        .unwrap();
    assert_eq!(set.len(), 2);
    let before = editor.project().revision;
    door_joint::create_with_hinges(
        &mut editor,
        uuid::Uuid::new_v4(),
        door,
        LEFT_SIDE_ID,
        set,
        Vec::new(),
    )
    .unwrap();
    assert_eq!(editor.project().revision, before + 1);
    let joint = editor
        .project()
        .door_joints
        .iter()
        .find(|j| j.moving_root_id == door)
        .unwrap()
        .clone();
    assert_eq!(joint.hinge_installation_ids.len(), 2);
    // One more hinge joins the relationship in one step.
    let extra =
        hinge_installation::standard_set(editor.project(), door, LEFT_SIDE_ID, catalog, Some(3))
            .unwrap()
            .remove(1);
    door_joint::add_hinge(&mut editor, joint.id, extra).unwrap();
    assert_eq!(editor.project().revision, before + 2);
    let joint = &editor
        .project()
        .door_joints
        .iter()
        .find(|j| j.id == joint.id)
        .unwrap();
    assert_eq!(joint.hinge_installation_ids.len(), 3);
    editor.undo().unwrap();
    editor.undo().unwrap();
    assert!(
        !editor
            .project()
            .door_joints
            .iter()
            .any(|j| j.moving_root_id == door)
    );
}

#[test]
fn catalog_models_in_use_are_kept_and_unused_ones_removed() {
    let mut editor = editor();
    let hinge_catalog = editor.project().hinge_installations[0].catalog_id;
    let entry = editor
        .project()
        .catalog
        .iter()
        .find(|c| c.id == hinge_catalog)
        .unwrap()
        .clone();
    assert_eq!(hardware_catalog::kind(&entry), CatalogKind::Hinge);
    let usage = hardware_catalog::usage(editor.project(), hinge_catalog);
    assert_eq!(
        usage.hinges.len(),
        editor.project().hinge_installations.len()
    );
    assert!(hardware_catalog::remove(&mut editor, hinge_catalog).is_err());
    let foot = hardware_catalog::add(&mut editor, foot_entry()).unwrap();
    let pinned = editor
        .project()
        .catalog
        .iter()
        .find(|c| c.id == foot)
        .unwrap();
    assert_eq!(hardware_catalog::kind(pinned), CatalogKind::Foot);
    assert!(hardware_catalog::usage(editor.project(), foot).is_empty());
    hardware_catalog::remove(&mut editor, foot).unwrap();
    assert!(!editor.project().catalog.iter().any(|c| c.id == foot));
}

#[test]
fn feet_default_under_the_parent_and_move_in_one_step() {
    let mut editor = editor();
    let entry = foot_entry();
    let size = entry
        .foot()
        .unwrap()
        .local_size()
        .map(|v| v.micrometres() as f64 / 1000.0);
    let world = plan_my_cabinet::assembly_edit::default_foot_world(
        editor.project(),
        Some(CARCASS_ID),
        size,
    );
    let bottom = plan_my_cabinet::measurements::measure(
        editor.project(),
        &[CARCASS_ID],
        plan_my_cabinet::measurements::Scope::Body,
        plan_my_cabinet::measurements::Frame::World,
    )
    .unwrap();
    assert!((world.translation_mm[2] + size[2] - bottom.minimum_mm[2]).abs() < 1e-9);
    assert!((world.translation_mm[0] - bottom.minimum_mm[0] - 20.0).abs() < 1e-9);
    let catalog = entry.id;
    let foot = editor
        .create_foot("Foot".into(), catalog, Some(entry), Some(CARCASS_ID), world)
        .unwrap();
    let revision = editor.project().revision;
    let moved = Pose::new([100.0, 50.0, -100.0], Quaternion::IDENTITY).unwrap();
    editor.preview_hardware_world(foot, moved).unwrap();
    assert_eq!(
        editor.project().revision,
        revision,
        "a drag preview is not an edit"
    );
    editor.cancel_preview();
    editor.move_hardware(foot, moved).unwrap();
    assert_eq!(editor.project().revision, revision + 1);
    let now = plan_my_cabinet::assembly_edit::world_pose(editor.project(), foot).unwrap();
    assert!((now.translation_mm[0] - 100.0).abs() < 1e-9);
    let _ = BOTTOM_ID;
}

#[test]
fn hardware_drafts_commit_only_on_accept() {
    let mut editor = editor();
    let id = editor
        .create_placeholder(
            "Handle".into(),
            [mm(160), mm(30), mm(20)],
            None,
            Pose::IDENTITY,
        )
        .unwrap();
    let mut drafts = EditDrafts::default();
    let target = FittingTarget::Hardware(id);
    let revision = editor.project().revision;
    {
        let draft = drafts
            .fitting(&editor, target, Unit::Mm, Locale::En)
            .unwrap();
        assert_eq!(draft.fields.len(), 6);
        draft.fields[0].edit("250");
        draft.fields[3].edit("200");
        assert!(draft.dirty());
        draft.preview(&editor).unwrap();
    }
    assert_eq!(editor.project().revision, revision, "typing does not edit");
    assert!(drafts.dirty_fitting(editor.project().id).is_some());
    drafts
        .existing_fitting_mut(editor.project().id, target)
        .unwrap()
        .accept(&mut editor)
        .unwrap();
    assert_eq!(editor.project().revision, revision + 1);
    let item = editor
        .project()
        .hardware
        .iter()
        .find(|h| h.id == id)
        .unwrap();
    assert_eq!(item.pose.translation_mm[0], 250.0);
    assert!(
        matches!(item.kind, HardwareKind::Placeholder { dimensions } if dimensions[0] == mm(200))
    );
    // A bad value keeps the draft and changes nothing.
    let draft = drafts
        .fitting(&editor, target, Unit::Mm, Locale::En)
        .unwrap();
    draft.fields[4].edit("-5");
    assert!(draft.accept(&mut editor).is_err());
    assert_eq!(editor.project().revision, revision + 1);
    draft.cancel();
    assert!(!draft.dirty());
}

#[test]
fn hinge_drafts_move_the_hinge_with_its_plate() {
    let mut editor = editor();
    let hinge = editor.project().hinge_installations[0].clone();
    let mut drafts = EditDrafts::default();
    let draft = drafts
        .fitting(
            &editor,
            FittingTarget::Hinge(hinge.id),
            Unit::Mm,
            Locale::En,
        )
        .unwrap();
    draft.fields[0].edit("150");
    draft.accept(&mut editor).unwrap();
    let moved = editor
        .project()
        .hinge_installations
        .iter()
        .find(|h| h.id == hinge.id)
        .unwrap();
    assert_eq!(moved.door_y, mm(150));
    assert_ne!(moved.mount_y, hinge.mount_y, "the plate follows the cup");
}
