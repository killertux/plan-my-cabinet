//! Edge banding end to end: bands on new projects and templates, per-edge
//! edits as single undo steps, the automatic rule, and materials that refuse
//! banding.
use plan_my_cabinet::banding::{BandingError, BandingPreset, band_lengths};
use plan_my_cabinet::banding_rules;
use plan_my_cabinet::commands::{EditError, ProjectEditor};
use plan_my_cabinet::domain::{
    BoardEdge, DomainError, EdgeBanding, MaterialKind, Project, SrgbColor,
};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::material_presets;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence;
use plan_my_cabinet::template_setup::{ProposedLength, TemplateField, TemplateKind, TemplateSetup};
use plan_my_cabinet::units::{Conversion, Length, Unit};
use uuid::Uuid;

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}

fn base_cabinet() -> ProjectEditor {
    let mut stage = TemplateSetup::new(TemplateKind::Base, "Base", Currency::Brl, Unit::Mm);
    for (field, value) in [
        (TemplateField::Width, 600),
        (TemplateField::Depth, 560),
        (TemplateField::Height, 720),
        (TemplateField::RailWidth, 80),
    ] {
        stage
            .dimensions
            .insert(field, ProposedLength::new(Conversion::Exact(mm(value))));
    }
    stage.seed_standard_materials(Language::PtBr);
    stage.generate().unwrap().editor
}

fn board(project: &Project, name: &str) -> Uuid {
    project.boards.iter().find(|b| b.name == name).unwrap().id
}

fn bands_of(project: &Project, id: Uuid) -> [Option<Uuid>; 4] {
    banding_rules::bands(&banding_rules::board_states(project, id).unwrap())
}

#[test]
fn new_projects_and_templates_band_white_sheets_with_white_band() {
    let mut project = Project::new("New", Currency::Brl);
    material_presets::seed_defaults(&mut project, Language::PtBr);
    assert_eq!(project.edge_bands[0].name, "Fita Branca 1x22");
    let white = project.edge_bands[0].id;
    for material in &project.materials {
        let expected = material.name.contains("Branco") && material.kind.accepts_banding();
        assert_eq!(
            material.default_band == Some(white),
            expected,
            "{}",
            material.name
        );
    }
    let kinds: Vec<_> = project.materials.iter().map(|m| m.kind).collect();
    assert!(kinds.contains(&MaterialKind::Mdp) && kinds.contains(&MaterialKind::Plywood));
    project.validate().unwrap();

    let editor = base_cabinet();
    let p = editor.project();
    assert_eq!(p.edge_bands.len(), 1);
    let side = board(p, "Left side");
    let band = Some(p.edge_bands[0].id);
    assert_eq!(bands_of(p, side), [band, band, band, None]);
    // The 3 mm HDF back is never banded.
    assert_eq!(bands_of(p, board(p, "Overlay back")), [None; 4]);
}

#[test]
fn toggling_an_edge_is_one_undo_step_and_twice_returns_to_automatic() {
    let mut editor = base_cabinet();
    let band = editor.project().edge_bands[0].id;
    let side = board(editor.project(), "Left side");
    let revision = editor.project().revision;
    // The front edge is banded automatically; a click turns it off.
    assert_eq!(
        editor
            .toggle_edge_banding(side, BoardEdge::MinY, band)
            .unwrap(),
        EdgeBanding::Off
    );
    assert_eq!(editor.project().revision, revision + 1);
    assert_eq!(bands_of(editor.project(), side)[2], None);
    // A second click bands it again, which is what automatic gives.
    assert_eq!(
        editor
            .toggle_edge_banding(side, BoardEdge::MinY, band)
            .unwrap(),
        EdgeBanding::Auto
    );
    assert!(editor.project().board(side).unwrap().banding.is_automatic());
    // The rear edge is against the back; a click forces a band on it.
    assert_eq!(
        editor
            .toggle_edge_banding(side, BoardEdge::MaxY, band)
            .unwrap(),
        EdgeBanding::On(band)
    );
    assert_eq!(bands_of(editor.project(), side)[3], Some(band));
    editor.undo().unwrap();
    assert_eq!(bands_of(editor.project(), side)[3], None);
}

#[test]
fn presets_apply_to_many_boards_and_skip_those_that_take_no_banding() {
    let mut editor = base_cabinet();
    let p = editor.project();
    let band = p.edge_bands[0].id;
    let boards = [
        board(p, "Left side"),
        board(p, "Bottom"),
        board(p, "Overlay back"),
    ];
    let revision = p.revision;
    let outcome = editor
        .apply_banding_preset(&boards, BandingPreset::AllFour, Some(band))
        .unwrap();
    assert_eq!(outcome.changed, boards[..2]);
    assert_eq!(outcome.skipped, [boards[2]]);
    assert_eq!(editor.project().revision, revision + 1);
    assert_eq!(bands_of(editor.project(), boards[1]), [Some(band); 4]);
    editor
        .apply_banding_preset(&boards[..2], BandingPreset::Front, Some(band))
        .unwrap();
    assert_eq!(
        bands_of(editor.project(), boards[0]),
        [None, None, Some(band), None]
    );
    editor
        .apply_banding_preset(&boards[..2], BandingPreset::None, None)
        .unwrap();
    assert_eq!(bands_of(editor.project(), boards[0]), [None; 4]);
    editor
        .apply_banding_preset(&boards[..2], BandingPreset::Auto, None)
        .unwrap();
    assert!(
        editor
            .project()
            .board(boards[0])
            .unwrap()
            .banding
            .is_automatic()
    );
    // Only boards that refuse banding: nothing to do.
    assert!(matches!(
        editor.apply_banding_preset(&boards[2..], BandingPreset::AllFour, Some(band)),
        Err(EditError::Command(BandingError::NotAccepted))
    ));
}

#[test]
fn a_material_that_takes_no_banding_drops_manual_bands_in_the_same_step() {
    let mut editor = base_cabinet();
    let p = editor.project();
    let band = p.edge_bands[0].id;
    let side = board(p, "Left side");
    let carcass = p.board(side).unwrap().material_id;
    editor
        .set_edge_banding(&[side], &[BoardEdge::MaxY], EdgeBanding::On(band))
        .unwrap();
    let revision = editor.project().revision;
    editor
        .set_material_banding(carcass, MaterialKind::Plywood, Some(band))
        .unwrap();
    assert_eq!(editor.project().revision, revision + 1);
    assert_eq!(bands_of(editor.project(), side), [None; 4]);
    assert!(editor.project().board(side).unwrap().banding.is_automatic());
    editor.undo().unwrap();
    assert_eq!(bands_of(editor.project(), side)[3], Some(band));

    // A document with banding on plywood is malformed.
    let mut broken = editor.project().clone();
    broken
        .materials
        .iter_mut()
        .find(|m| m.id == carcass)
        .unwrap()
        .kind = MaterialKind::Plywood;
    assert_eq!(
        broken.validate(),
        Err(DomainError::BandingNotAccepted(side))
    );
}

#[test]
fn bands_in_use_cannot_be_removed_and_lengths_add_up() {
    let mut editor = base_cabinet();
    let band = editor.project().edge_bands[0].id;
    assert!(matches!(
        editor.remove_edge_band(band),
        Err(EditError::Command(BandingError::BandInUse { .. }))
    ));
    let spare = editor
        .create_edge_band("Fita Preta 1x22", mm(1), mm(22), SrgbColor([20; 3]))
        .unwrap();
    assert!(editor.remove_edge_band(spare).unwrap());
    assert!(matches!(
        editor.create_edge_band("  ", mm(1), mm(22), SrgbColor([0; 3])),
        Err(EditError::Command(BandingError::EmptyName))
    ));
    // Base 600 × 560 × 720, sides 720 × 560, 18 mm: each side bands its
    // bottom, top (560 each) and front (720); the bottom, rails: their fronts.
    let total = band_lengths(editor.project())
        .into_iter()
        .find(|(id, _)| *id == band)
        .unwrap()
        .1;
    let p = editor.project();
    let expected: i128 = ["Left side", "Right side"]
        .iter()
        .map(|n| {
            let b = p.board(board(p, n)).unwrap();
            i128::from(2 * b.width.micrometres() + b.length.micrometres())
        })
        .chain(
            ["Bottom", "Rear top rail", "Front top rail"]
                .iter()
                .map(|n| {
                    let b = p.board(board(p, n)).unwrap();
                    i128::from(b.length.micrometres())
                }),
        )
        .sum();
    // The front rail's rear edge faces the inside and is free too.
    let front_rail = p.board(board(p, "Front top rail")).unwrap();
    assert_eq!(
        total,
        expected + i128::from(front_rail.length.micrometres())
    );
}

#[test]
fn files_before_banding_infer_material_kinds_and_band_nothing() {
    let mut project = Project::new("Old", Currency::Brl);
    material_presets::seed_defaults(&mut project, Language::PtBr);
    let mut value = serde_json::to_value(&project).unwrap();
    value["schema_version"] = 4.into();
    value.as_object_mut().unwrap().remove("edge_bands");
    for material in value["materials"].as_array_mut().unwrap() {
        let material = material.as_object_mut().unwrap();
        material.remove("kind");
        material.remove("default_band");
    }
    let loaded = persistence::prepare_bytes(&serde_json::to_vec(&value).unwrap())
        .unwrap()
        .into_editor();
    let p = loaded.project();
    let kind = |name: &str| p.materials.iter().find(|m| m.name == name).unwrap().kind;
    assert_eq!(kind("MDF Branco"), MaterialKind::Mdf);
    assert_eq!(kind("MDP Branco"), MaterialKind::Mdp);
    assert_eq!(kind("Compensado"), MaterialKind::Plywood);
    assert_eq!(kind("HDF"), MaterialKind::Hdf);
    assert!(p.edge_bands.is_empty());
    assert!(p.materials.iter().all(|m| m.default_band.is_none()));
}
