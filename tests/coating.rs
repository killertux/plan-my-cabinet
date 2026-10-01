//! Sheet coating: inferred from old files, decided per material, the coated
//! face of one-side boards chosen automatically or by hand, and the 3D look
//! that follows (coated faces in the material color, raw faces and unbanded
//! edges in the core, banded edges in the band).
use plan_my_cabinet::coating::CoatedFaceEdit;
use plan_my_cabinet::coating_rules::{self, Coated, Facing};
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::{BoardFace, CoatedFace, Coating, Project};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::material_presets;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence;
use plan_my_cabinet::reference_fixture as fixture;
use plan_my_cabinet::render::mesh::board_looks;
use plan_my_cabinet::render::surface::Surface;
use uuid::Uuid;

fn board(project: &Project, name: &str) -> Uuid {
    project.boards.iter().find(|b| b.name == name).unwrap().id
}

fn one_sided() -> Project {
    let mut project = fixture::project();
    for material in &mut project.materials {
        material.coating = Coating::OneSide;
    }
    project.validate().unwrap();
    project
}

#[test]
fn names_tell_the_coating_of_old_files() {
    for (name, coating) in [
        ("MDF Branco", Coating::BothSides),
        ("MDF Cru", Coating::None),
        ("Raw MDF", Coating::None),
        ("MDF Branco 1 face", Coating::OneSide),
        ("MDP Branco 1F", Coating::OneSide),
        ("White MDF one side", Coating::OneSide),
        ("MDF Branco uma face", Coating::OneSide),
        ("MDF 15 Branco TX", Coating::BothSides),
    ] {
        assert_eq!(Coating::infer(name), coating, "{name}");
    }

    let mut project = Project::new("Old", Currency::Brl);
    material_presets::seed_defaults(&mut project, Language::PtBr);
    let mut value = serde_json::to_value(&project).unwrap();
    value["schema_version"] = 5.into();
    for material in value["materials"].as_array_mut().unwrap() {
        material.as_object_mut().unwrap().remove("coating");
    }
    value["materials"][0]["name"] = "MDF Branco 1 face".into();
    let loaded = persistence::prepare_bytes(&serde_json::to_vec(&value).unwrap())
        .unwrap()
        .into_editor();
    let p = loaded.project();
    let coating = |name: &str| p.materials.iter().find(|m| m.name == name).unwrap().coating;
    assert_eq!(coating("MDF Branco 1 face"), Coating::OneSide);
    assert_eq!(coating("MDF Cru"), Coating::None);
    assert_eq!(coating("MDP Branco"), Coating::BothSides);
    // New projects seed raw MDF uncoated.
    let mut fresh = Project::new("New", Currency::Brl);
    material_presets::seed_defaults(&mut fresh, Language::En);
    assert_eq!(
        fresh
            .materials
            .iter()
            .find(|m| m.name == "Raw MDF")
            .unwrap()
            .coating,
        Coating::None
    );
}

#[test]
fn one_side_boards_are_coated_where_they_show() {
    let project = one_sided();
    let state = |name: &str| coating_rules::board_state(&project, board(&project, name)).unwrap();
    let left = state("Left side");
    let right = state("Right side");
    // Sides face outward: the two sides are coated on opposite local faces.
    assert_eq!(left.automatic, Some(Facing::Outside));
    assert_eq!(right.automatic, Some(Facing::Outside));
    assert_ne!(left.coated, right.coated);
    assert_eq!(state("Bottom").automatic, Some(Facing::Up));
    assert_eq!(state("Shelf").coated, Coated::One(BoardFace::MaxZ));
    for name in ["Door left", "Door right", "Back panel"] {
        assert_eq!(state(name).automatic, Some(Facing::Front), "{name}");
    }

    // Both sides and no coating leave nothing to choose.
    let mut both = one_sided();
    both.materials[0].coating = Coating::BothSides;
    let s = coating_rules::board_state(&both, board(&both, "Left side")).unwrap();
    assert_eq!(s.coated, Coated::Both);
    assert!(!s.choosable);
    // Plywood is never coated, whatever the field says.
    let mut plywood = one_sided();
    plywood.materials[0].kind = plan_my_cabinet::domain::MaterialKind::Plywood;
    let s = coating_rules::board_state(&plywood, board(&plywood, "Left side")).unwrap();
    assert_eq!(s.coated, Coated::Raw);
}

#[test]
fn flip_and_back_to_automatic_are_one_undo_step_each() {
    let mut editor = ProjectEditor::new(one_sided()).unwrap();
    let door = board(editor.project(), "Door left");
    let before = coating_rules::board_state(editor.project(), door).unwrap();
    let revision = editor.project().revision;
    let outcome = editor
        .set_coated_face(&[door], CoatedFaceEdit::Flip)
        .unwrap();
    assert_eq!(outcome.changed, [door]);
    assert_eq!(editor.project().revision, revision + 1);
    let flipped = coating_rules::board_state(editor.project(), door).unwrap();
    assert_ne!(flipped.coated, before.coated);
    assert_eq!(flipped.automatic, None);
    editor
        .set_coated_face(&[door], CoatedFaceEdit::Auto)
        .unwrap();
    assert_eq!(
        editor.project().board(door).unwrap().coated_face,
        CoatedFace::Auto
    );
    assert_eq!(
        coating_rules::board_state(editor.project(), door).unwrap(),
        before
    );
    editor.undo().unwrap();
    assert_ne!(
        editor.project().board(door).unwrap().coated_face,
        CoatedFace::Auto
    );
    editor.undo().unwrap();
    assert_eq!(
        editor.project().board(door).unwrap().coated_face,
        CoatedFace::Auto
    );

    // Boards not coated on one side are skipped; none at all is refused.
    let mut editor = ProjectEditor::new(fixture::project()).unwrap();
    let side = board(editor.project(), "Left side");
    assert!(
        editor
            .set_coated_face(&[side], CoatedFaceEdit::Flip)
            .is_err()
    );
    editor
        .set_material_coating(
            editor.project().board(side).unwrap().material_id,
            Coating::OneSide,
        )
        .unwrap();
    let back = board(editor.project(), "Back panel");
    let outcome = editor
        .set_coated_face(&[side, back], CoatedFaceEdit::Face(BoardFace::MinZ))
        .unwrap();
    assert_eq!(outcome.changed, [side]);
    assert_eq!(outcome.skipped, [back]);
}

#[test]
fn faces_show_coating_core_or_band() {
    let project = one_sided();
    let looks = board_looks(&project, true);
    let door = board(&project, "Door left");
    let look = looks[&door];
    // The door is coated on its front (MaxZ) face, raw MDF behind.
    assert_eq!(look.broad[1].surface, None);
    assert_eq!(look.broad[0].surface, Some(Surface::MdfFibre));
    // No band anywhere in the fixture: every edge shows the core.
    assert!(
        look.edges
            .iter()
            .all(|e| e.surface == Some(Surface::MdfFibre))
    );
    assert_eq!(
        looks[&board(&project, "Back panel")].broad[0].surface,
        Some(Surface::HdfFibre)
    );

    // A band replaces the core on the edges it covers, smooth, in its color.
    let mut banded = one_sided();
    let band = Uuid::from_u128(0xba4d);
    banded.edge_bands.push(plan_my_cabinet::domain::EdgeBand {
        id: band,
        name: "Fita Preta".into(),
        thickness: plan_my_cabinet::units::Length::from_micrometres(1000),
        height: plan_my_cabinet::units::Length::from_micrometres(22_000),
        color: plan_my_cabinet::domain::SrgbColor([10, 10, 10]),
    });
    let material = banded.board(door).unwrap().material_id;
    banded
        .materials
        .iter_mut()
        .find(|m| m.id == material)
        .unwrap()
        .default_band = Some(band);
    let look = board_looks(&banded, true)[&door];
    assert!(look.banded.iter().all(|b| *b), "door edges are free");
    for edge in look.edges {
        assert_eq!(edge.surface, None);
        assert!(edge.color[0] < 0.05);
    }
    // Without material tint, faces are neutral and untextured.
    let plain = board_looks(&project, false)[&door];
    assert!(
        plain
            .broad
            .iter()
            .chain(&plain.edges)
            .all(|f| f.surface.is_none())
    );
}

#[test]
fn agents_set_a_material_coating_and_flip_a_coated_face() {
    use plan_my_cabinet::service::{Workspace, WorkspaceConfig};
    use serde_json::{Value, json};
    fn input<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).unwrap()
    }
    let dir = std::env::temp_dir().join(format!("pmcab-coating-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut ws = Workspace::new(WorkspaceConfig {
        catalog_dir: Some(dir.join("catalogs")),
        user_data_dir: Some(dir.clone()),
        ..Default::default()
    });
    ws.generate_template(input(json!({
        "kind": "base",
        "name": "Base",
        "dimensions": { "width": 600, "depth": 560, "height": 720 }
    })))
    .unwrap();
    let side: Value = ws.get_board(input(json!({ "ref": "Left side" }))).unwrap();
    assert_eq!(side["coating"]["coated"], "both", "{side:#}");
    let material = side["material"].as_str().unwrap().to_owned();
    let materials = ws.list_materials().unwrap();
    let row = materials["materials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == material.as_str() && m["boards"].as_u64() > Some(0))
        .unwrap();
    let material = row["id"].as_str().unwrap().to_owned();
    assert_eq!(row["coating"], "both_sides");
    ws.update_material(input(
        json!({ "material": material, "coating": "one_side" }),
    ))
    .unwrap();
    let side: Value = ws.get_board(input(json!({ "ref": "Left side" }))).unwrap();
    assert_eq!(side["coating"]["face_choosable"], true, "{side:#}");
    assert_eq!(side["coating"]["automatic"], "facing outside the cabinet");
    let before = side["coating"]["coated"].clone();
    let changed = ws
        .set_board_coated_face(input(json!({ "boards": ["Left side"], "face": "flip" })))
        .unwrap();
    assert!(changed.committed);
    let side: Value = ws.get_board(input(json!({ "ref": "Left side" }))).unwrap();
    assert_ne!(side["coating"]["coated"], before);
    assert_eq!(side["coating"]["automatic"], Value::Null);
    std::fs::remove_dir_all(dir).unwrap();
}
