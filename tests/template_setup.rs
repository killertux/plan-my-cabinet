use std::collections::HashSet;

use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::{BoardGrain, Project, SrgbColor};
use plan_my_cabinet::first_fit::FirstFit;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::template_setup::{
    MaterialRole, ProposedLength, SetupError, TemplateField, TemplateKind, TemplateSetup,
};
use plan_my_cabinet::units::{Conversion, Length, Unit};

fn mm(value: i64) -> ProposedLength {
    ProposedLength::new(Conversion::Exact(Length::from_micrometres(value * 1000)))
}

fn setup(kind: TemplateKind) -> TemplateSetup {
    let mut stage = TemplateSetup::new(kind, "Kitchen", Currency::Brl, Unit::Cm);
    for (field, value) in [
        (TemplateField::Width, 600),
        (TemplateField::Depth, 560),
        (TemplateField::Height, 720),
        (TemplateField::RailWidth, 80),
        (TemplateField::ShelfHeight, 320),
        (TemplateField::BoxDepth, 500),
        (TemplateField::SideClearance, 13),
        (TemplateField::RearClearance, 20),
        (TemplateField::VerticalClearance, 8),
        (TemplateField::FrontReveal, 3),
        (TemplateField::FrontGap, 3),
    ] {
        stage.dimensions.insert(field, mm(value));
    }
    let carcass = stage.add_material(
        "Carcass",
        mm(18),
        BoardGrain::Length,
        Some(SrgbColor([1, 2, 3])),
    );
    let back = stage.add_material("Back", mm(6), BoardGrain::Unrestricted, None);
    let box_material = stage.add_material("Box", mm(12), BoardGrain::Length, None);
    let front = stage.add_material("Front", mm(19), BoardGrain::Length, None);
    for (role, id) in [
        (MaterialRole::Carcass, carcass),
        (MaterialRole::Back, back),
        (MaterialRole::Box, box_material),
        (MaterialRole::BoxBottom, back),
        (MaterialRole::ExternalFront, front),
    ] {
        stage.roles.insert(role, id);
    }
    stage.drawer_count = Some(3);
    stage
}

#[test]
fn first_run_review_and_generation_are_nonmutating_and_one_undo() {
    let old = ProjectEditor::new(Project::new("Unsaved old work", Currency::Usd)).unwrap();
    let before = old.project().clone();
    let stage = setup(TemplateKind::Drawers);
    let first = stage.review().unwrap();
    let second = stage.review().unwrap();
    assert_eq!(first.candidate.boards.len(), 23);
    assert_ne!(first.candidate.boards[0].id, second.candidate.boards[0].id);
    assert_eq!(
        first.datums.front_outside,
        Length::from_micrometres(-19_000)
    );
    assert_eq!(first.datums.carcass_rear, Length::from_micrometres(535_000));
    assert!(
        first
            .fits
            .iter()
            .all(|(fit, allocation)| *fit == FirstFit::NoFit && allocation.is_none())
    );
    assert_eq!(old.project(), &before);
    let mut result = stage.generate().unwrap();
    assert_eq!(old.project(), &before); // host has not swapped its active editor yet
    let another = stage.generate().unwrap();
    assert_ne!(result.editor.project().id, another.editor.project().id);
    assert_ne!(result.assembly_id, another.assembly_id);
    assert_ne!(result.fits[0].0, another.fits[0].0);
    let p = result.editor.project();
    assert_eq!(p.name, "Kitchen");
    assert_eq!(p.display_unit, Unit::Cm);
    assert_eq!(p.materials.len(), 4);
    assert_eq!(p.assemblies.len(), 4);
    assert_eq!(p.boards.len(), 23);
    assert!(result.editor.is_dirty());
    assert!(result.editor.can_undo());
    assert!(p.stock.is_empty() && p.allocations.is_empty());
    assert!(result.fits.iter().all(|(_, fit)| *fit == FirstFit::NoFit));
    let material_ids: HashSet<_> = stage.materials.iter().map(|m| m.id).collect();
    assert!(p.materials.iter().all(|m| !material_ids.contains(&m.id)));
    assert!(
        p.boards
            .iter()
            .all(|b| !first.candidate.boards.iter().any(|old| old.id == b.id))
    );
    assert_eq!(p.material_colors.len(), 1);
    assert!(p.assemblies.iter().any(|a| a.id == result.assembly_id));
    let saved = p.clone();
    result.editor.undo().unwrap();
    assert!(result.editor.project().materials.is_empty());
    assert!(result.editor.project().boards.is_empty());
    assert!(result.editor.project().assemblies.is_empty());
    assert!(result.editor.project().material_colors.is_empty());
    assert!(!result.editor.is_dirty());
    result.editor.redo().unwrap();
    assert_eq!(result.editor.project().boards, saved.boards);
    assert_eq!(result.editor.project().materials, saved.materials);
    result
        .editor
        .transact(|p| -> Result<(), ()> {
            p.boards[0].length = Length::from_micrometres(710_000);
            Ok(())
        })
        .unwrap();
    assert_eq!(result.editor.project().boards[1], saved.boards[1]);
    result.editor.undo().unwrap();
    assert_eq!(result.editor.project().boards, saved.boards);
    result.editor.undo().unwrap();
    assert!(result.editor.project().boards.is_empty());
}

#[test]
fn missing_roles_invalid_geometry_and_rounding_never_commit() {
    let mut stage = setup(TemplateKind::Base);
    stage.roles.remove(&MaterialRole::Back);
    assert!(
        stage
            .review()
            .unwrap_err()
            .contains(&SetupError::MissingRole(MaterialRole::Back))
    );
    stage
        .roles
        .insert(MaterialRole::Back, stage.materials[1].id);
    stage.dimensions.insert(TemplateField::Width, mm(20));
    assert!(
        stage
            .review()
            .unwrap_err()
            .iter()
            .any(|e| matches!(e, SetupError::Geometry(_)))
    );
    stage.dimensions.insert(TemplateField::Width, mm(600));
    stage.dimensions.insert(
        TemplateField::RailWidth,
        ProposedLength::new(Conversion::NeedsConfirmation(Length::from_micrometres(
            80_001,
        ))),
    );
    assert!(
        stage
            .review()
            .unwrap_err()
            .contains(&SetupError::Rounding(TemplateField::RailWidth))
    );
    stage
        .dimensions
        .get_mut(&TemplateField::RailWidth)
        .unwrap()
        .confirm_rounding();
    assert_eq!(stage.review().unwrap().candidate.boards.len(), 6);
    stage.dimensions.insert(
        TemplateField::RailWidth,
        ProposedLength::new(Conversion::NeedsConfirmation(Length::from_micrometres(
            80_002,
        ))),
    );
    assert!(stage.generate().is_err()); // editing a proposal resets consent
    stage.materials[1].thickness = mm(0);
    assert!(
        stage
            .review()
            .unwrap_err()
            .contains(&SetupError::MaterialThickness(stage.materials[1].id))
    );
    stage.materials[1].thickness = ProposedLength::new(Conversion::NeedsConfirmation(
        Length::from_micrometres(6_001),
    ));
    assert!(
        stage
            .review()
            .unwrap_err()
            .contains(&SetupError::MaterialRounding(stage.materials[1].id))
    );
}

#[test]
fn nested_material_cancel_keeps_parent_draft_and_roles_can_share_material() {
    let mut stage = setup(TemplateKind::Wall);
    let before = stage.clone();
    let mut nested = stage.clone();
    nested.add_material("Discard", mm(5), BoardGrain::Width, None);
    drop(nested); // nested dialog cancelled; host retains original setup
    assert_eq!(stage.materials.len(), before.materials.len());
    assert_eq!(stage.roles, before.roles);
    let shared = stage.materials[0].id;
    stage.roles.insert(MaterialRole::Back, shared);
    let result = stage.generate().unwrap();
    let p = result.editor.project();
    assert_eq!(p.boards[0].material_id, p.boards[5].material_id);
    assert_eq!(p.boards[5].thickness, Length::from_micrometres(18_000));
}
