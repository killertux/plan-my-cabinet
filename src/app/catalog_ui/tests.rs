use super::*;
use crate::hinge_ui::HingeDialog;
use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
use plan_my_cabinet::domain::BoardGrain;
use plan_my_cabinet::hinge_installation::InstallationIssue;

fn enter() -> egui::RawInput {
    egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    }
}

fn frame(app: &mut DesktopApp, ctx: &egui::Context, input: egui::RawInput) {
    ctx.run_ui(input, |ui| app.show_catalog_dialog(ui.ctx()))
        .drop_without_applying_deltas();
}

fn with_boards() -> DesktopApp {
    let mut app = DesktopApp::default();
    let material = app
        .editor
        .create_material(NewMaterial {
            name: "MDF".into(),
            thickness: Length::from_micrometres(18_000),
            grain: BoardGrain::Unrestricted,
        })
        .unwrap();
    for name in ["Door", "Side"] {
        app.editor
            .create_board(NewBoard {
                name: name.into(),
                material_id: material,
                length: Length::from_micrometres(300_000),
                width: Length::from_micrometres(300_000),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
    }
    app
}

#[test]
fn add_catalog_opens_the_browser_and_enter_pins_the_selected_variant() {
    let mut app = DesktopApp::default();
    app.invoke(Request::new(A::AddCatalog)).unwrap();
    assert!(app.modals.catalog().is_some());
    assert!(app.editor.project().catalog.is_empty());
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, egui::RawInput::default());
    frame(&mut app, &ctx, enter());
    assert!(app.modals.catalog().is_none());
    let entry = &app.editor.project().catalog[0];
    // The first bundled record is the reviewed Click 3D Slow kit.
    assert_eq!(entry.product_id, hardware_catalog::KIT_ID);
    assert_eq!(hardware_catalog::trust(entry), Some(Trust::Reviewed));
    assert_eq!(app.toasts.texts().len(), 1);
    assert_eq!(app.editor.undo(), Ok(true));
    assert!(app.editor.project().catalog.is_empty());
}

#[test]
fn inset_variant_pins_with_its_arm_and_the_hinge_dialog_asks_for_e() {
    let mut app = with_boards();
    let pack = app.hardware.catalogs.pack("fgvtn").unwrap();
    let family = pack
        .hinges
        .iter()
        .position(|f| f.id == "tn-ms-slow-calco-fixo")
        .unwrap();
    let inset = pack.hinges[family]
        .variants
        .iter()
        .position(|v| v.arm == HingeArm::Inset)
        .unwrap();
    let mut dialog = CatalogDialog::new();
    dialog.family = family;
    dialog.variant = inset;
    app.modals.set_catalog(Some(dialog));
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, egui::RawInput::default());
    frame(&mut app, &ctx, enter());
    let entry = app.editor.project().catalog[0].clone();
    assert_eq!(entry.product_id, "51MS15XTN2215BF");
    let facts = hardware_catalog::facts(&entry).unwrap();
    assert_eq!(facts.arm, HingeArm::Inset);

    let mut draft = HingeDialog::new(&app, None);
    assert_eq!(draft.catalog_id(), Some(entry.id));
    draft.set_values("4", "3", "10");
    let proposed = draft.proposed(&app).unwrap();
    assert_eq!(proposed.inset_depth, Length::from_micrometres(10_000));
    let status = hinge_installation::preview(app.editor.project(), &proposed).unwrap();
    assert!(
        status
            .issues
            .contains(&InstallationIssue::InsetShallowerThanDoor)
    );
    draft.set_values("4", "3", "18");
    let proposed = draft.proposed(&app).unwrap();
    let status = hinge_installation::preview(app.editor.project(), &proposed).unwrap();
    assert_eq!(status.issues, []);
    // 37 mm front offset + E from the side's front edge.
    assert_eq!(
        status.references.unwrap().plate_hole_centers_um[0][0],
        55_000
    );
    draft.set_values("4", "3", "not a length");
    assert!(draft.proposed(&app).is_none());
}

#[test]
fn a_broken_user_pack_shows_its_errors_and_offers_nothing_to_add() {
    let dir = std::env::temp_dir().join(format!("catalog-ui-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("broken.toml"), "schema = 1\nid = \"Bad Id\"").unwrap();
    let mut app = DesktopApp::default();
    app.hardware.catalogs = CatalogRegistry::load(Some(&dir));
    std::fs::remove_dir_all(&dir).unwrap();
    let index = app
        .hardware
        .catalogs
        .packs
        .iter()
        .position(|p| !p.is_bundled())
        .unwrap();
    assert!(app.hardware.catalogs.packs[index].errors() > 0);
    let mut dialog = CatalogDialog::new();
    dialog.pack = index;
    app.modals.set_catalog(Some(dialog));
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, egui::RawInput::default());
    frame(&mut app, &ctx, enter());
    assert!(app.modals.catalog().is_some());
    assert!(app.editor.project().catalog.is_empty());
}

#[test]
fn import_copies_a_pack_into_the_user_folder() {
    let root = std::env::temp_dir().join(format!("catalog-import-{}", Uuid::new_v4()));
    let outside = root.join("downloads");
    std::fs::create_dir_all(&outside).unwrap();
    let source = outside.join("acme.toml");
    std::fs::write(&source, plan_my_cabinet::catalog_pack::BUNDLED[0].1).unwrap();
    let folder = root.join("catalogs");
    let target = import_pack(&source, &folder).unwrap();
    assert_eq!(target, folder.join("acme.toml"));
    let registry = CatalogRegistry::load(Some(&folder));
    std::fs::remove_dir_all(&root).unwrap();
    let user = registry.packs.iter().find(|p| !p.is_bundled()).unwrap();
    assert_eq!(user.errors(), 0);
    // Same id as the bundled pack: usable, flagged as replacing it.
    assert!(
        user.issues
            .iter()
            .any(|i| i.kind.code() == "shadows-bundled")
    );
}

#[test]
fn every_issue_code_and_arm_has_bilingual_text() {
    use plan_my_cabinet::catalog_pack::IssueKind;
    let kinds = [
        IssueKind::Syntax(String::new()),
        IssueKind::UnsupportedSchema(2),
        IssueKind::TooLarge,
        IssueKind::Unreadable(String::new()),
        IssueKind::InvalidId(String::new()),
        IssueKind::Missing("x"),
        IssueKind::DuplicateId(String::new()),
        IssueKind::DuplicateCode(String::new()),
        IssueKind::UnknownSource(String::new()),
        IssueKind::InvalidSha256,
        IssueKind::InvalidLength(String::new()),
        IssueKind::ThicknessRange,
        IssueKind::CupDeeperThanDoor,
        IssueKind::KNotIncreasing,
        IssueKind::KOutsideRange,
        IssueKind::OpeningAngle(0),
        IssueKind::NoVariants,
        IssueKind::NotMonotonic,
        IssueKind::NoOpeningAngle,
        IssueKind::Draft,
        IssueKind::ShadowsBundled,
    ];
    for language in [Language::En, Language::PtBr] {
        let localizer = Localizer::new(language);
        for kind in &kinds {
            let key = format!("catalogs-issue-{}", kind.code());
            assert_ne!(localizer.text(&key), key);
        }
        for arm in HingeArm::ALL {
            assert!(!arm_label(&localizer, arm).starts_with("arm-"));
        }
    }
}
