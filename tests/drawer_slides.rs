//! Drawer slides end to end: the Drawers template installs a catalog slide on
//! every drawer; checks, hole references, slide-out motion, cascade on delete.
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::{BoardGrain, Project, SlideInstallation};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::slide_installation::{self, SlideIssue};
use plan_my_cabinet::template_setup::{
    MaterialRole, ProposedLength, SetupError, SlideChoice, TemplateField, TemplateKind,
    TemplateSetup,
};
use plan_my_cabinet::units::{Conversion, Length, Pose, Unit};

fn mm(value: i64) -> ProposedLength {
    ProposedLength::new(Conversion::Exact(Length::from_micrometres(value * 1000)))
}

fn drawers(depth: i64, box_depth: i64) -> TemplateSetup {
    let mut stage = TemplateSetup::new(TemplateKind::Drawers, "Chest", Currency::Brl, Unit::Mm);
    for (field, value) in [
        (TemplateField::Width, 600),
        (TemplateField::Depth, depth),
        (TemplateField::Height, 720),
        (TemplateField::BoxDepth, box_depth),
        (TemplateField::SideClearance, 13),
        (TemplateField::RearClearance, 20),
        (TemplateField::VerticalClearance, 8),
        (TemplateField::FrontReveal, 3),
        (TemplateField::FrontGap, 3),
    ] {
        stage.dimensions.insert(field, mm(value));
    }
    let carcass = stage.add_material("Carcass", mm(18), BoardGrain::Length, None);
    let back = stage.add_material("Back", mm(6), BoardGrain::Unrestricted, None);
    let box_material = stage.add_material("Box", mm(15), BoardGrain::Length, None);
    let front = stage.add_material("Front", mm(18), BoardGrain::Length, None);
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

fn generated(depth: i64, box_depth: i64) -> ProjectEditor {
    drawers(depth, box_depth).generate().unwrap().editor
}

fn board_named<'a>(project: &'a Project, name: &str) -> &'a plan_my_cabinet::domain::Board {
    project.boards.iter().find(|b| b.name == name).unwrap()
}

#[test]
fn template_installs_the_longest_tt45_that_fits_on_every_drawer() {
    let editor = generated(560, 500);
    let p = editor.project();
    assert_eq!(p.slide_installations.len(), 3);
    assert_eq!(p.catalog.len(), 1);
    let entry = &p.catalog[0];
    assert_eq!(entry.product_id, "0073.045500SX");
    let spec = entry.slide().unwrap();
    assert_eq!(spec.clearance, Length::from_micrometres(12_700));
    // The box is exactly two clearances narrower than the opening.
    let left = board_named(p, "Drawer 1 left box side");
    let right = board_named(p, "Drawer 1 right box side");
    let width = right.pose.translation_mm[0] - left.pose.translation_mm[0] + 15.0;
    assert!((width - (600.0 - 36.0 - 25.4)).abs() < 1e-9, "{width}");
    for status in slide_installation::diagnose_all(p) {
        assert_eq!(status.issues, [], "{status:?}");
        let references = status.references.unwrap();
        for side in &references.sides {
            assert_eq!(side.gap, Length::from_micrometres(12_700));
            // 2 mm setback: the first cabinet hole is 37 mm from the front edge.
            assert_eq!(
                side.cabinet_hole_distances[0],
                Length::from_micrometres(37_000)
            );
            assert_eq!(
                side.drawer_hole_distances[0],
                Length::from_micrometres(34_000)
            );
            assert_eq!(side.cabinet_holes_um.len(), 5);
        }
    }
}

#[test]
fn a_shallow_cabinet_gets_a_shorter_slide_or_an_error() {
    let p = generated(460, 400);
    assert_eq!(
        p.project().catalog[0].product_id,
        "0073.045400SX",
        "400 box in a 435 deep carcass"
    );
    let too_short = drawers(300, 240).review().unwrap_err();
    assert!(too_short.contains(&SetupError::NoSlideFits));
    let mut none = drawers(300, 240);
    none.slides = SlideChoice::None;
    let review = none.review().unwrap();
    assert!(review.slides.is_none());
}

#[test]
fn detection_finds_the_same_boards_the_template_used() {
    let editor = generated(560, 500);
    let p = editor.project();
    for installation in &p.slide_installations {
        let detected = slide_installation::detect(p, installation.drawer_root_id).unwrap();
        assert_eq!(detected.drawer_sides, installation.drawer_sides);
        assert_eq!(detected.cabinet_sides, installation.cabinet_sides);
        assert_eq!(detected.sides(), installation.sides);
        // A board of the drawer finds the drawer too.
        let via_board = slide_installation::detect(p, installation.drawer_sides[1]).unwrap();
        assert_eq!(via_board.drawer_root, installation.drawer_root_id);
    }
}

#[test]
fn moving_a_box_side_or_the_height_raises_issues() {
    let mut editor = generated(560, 500);
    let installation = editor.project().slide_installations[0].clone();
    let side = installation.drawer_sides[0];
    editor
        .transact(|p| -> Result<(), ()> {
            let board = p.board_mut(side).unwrap();
            board.pose.translation_mm[0] += 1.0;
            Ok(())
        })
        .unwrap();
    let status = slide_installation::diagnose(editor.project(), &installation);
    assert!(status.issues.iter().any(|i| matches!(
        i,
        SlideIssue::ClearanceOutOfRange { side: 0, measured } if *measured == Length::from_micrometres(13_700)
    )));
    let high = SlideInstallation {
        height: Length::from_micrometres(500_000),
        ..installation
    };
    let status = slide_installation::diagnose(editor.project(), &high);
    assert!(status.issues.contains(&SlideIssue::HeightExceedsBoxSide));
}

#[test]
fn drawers_slide_out_by_the_travel_and_back() {
    let editor = generated(560, 500);
    let p = editor.project();
    let installation = &p.slide_installations[0];
    let closed = slide_installation::derived_poses(p, installation, 0.0).unwrap();
    let open = slide_installation::derived_poses(p, installation, 500.0).unwrap();
    let front = board_named(p, "Drawer 1 external front").id;
    let at = |poses: &[(uuid::Uuid, Pose)]| poses.iter().find(|(id, _)| *id == front).unwrap().1;
    let moved = at(&open).translation_mm[1] - at(&closed).translation_mm[1];
    assert!((moved + 500.0).abs() < 1e-9, "{moved}");
    assert!(slide_installation::derived_poses(p, installation, 501.0).is_err());
    // The drawer member follows the drawer in pictures; the cabinet member stays.
    let boxes_closed = slide_installation::member_boxes(p, installation, 0.0).unwrap();
    let boxes_open = slide_installation::member_boxes(p, installation, 500.0).unwrap();
    let dy =
        |i: usize| boxes_open[0][i].0.translation_mm[1] - boxes_closed[0][i].0.translation_mm[1];
    assert!((dy(0)).abs() < 1e-9 && (dy(1) + 250.0).abs() < 1e-9 && (dy(2) + 500.0).abs() < 1e-9);
    // Members fill the clearance exactly.
    let thickness: f64 = boxes_closed[0].iter().map(|(_, size)| size[2]).sum();
    assert!((thickness - 12.7).abs() < 1e-9);
}

#[test]
fn deleting_a_drawer_removes_its_slides_and_undo_restores_them() {
    let mut editor = generated(560, 500);
    let drawer = editor.project().slide_installations[1].drawer_root_id;
    plan_my_cabinet::door_joint::delete_object(&mut editor, drawer).unwrap();
    assert_eq!(editor.project().slide_installations.len(), 2);
    editor.undo().unwrap();
    assert_eq!(editor.project().slide_installations.len(), 3);
    // Deleting a carcass side removes every slide on it.
    let left = board_named(editor.project(), "Left side").id;
    plan_my_cabinet::door_joint::delete_object(&mut editor, left).unwrap();
    assert!(editor.project().slide_installations.is_empty());
}

#[test]
fn saved_projects_keep_slides() {
    let editor = generated(560, 500);
    let bytes = plan_my_cabinet::persistence::serialize(editor.project()).unwrap();
    let reopened = plan_my_cabinet::persistence::prepare_bytes(&bytes)
        .unwrap()
        .into_editor();
    assert_eq!(
        reopened.project().slide_installations,
        editor.project().slide_installations
    );
    assert_eq!(reopened.project().catalog, editor.project().catalog);
}

#[test]
fn inserting_twice_pins_the_slide_once() {
    let mut editor = ProjectEditor::new(Project::new("Room", Currency::Brl)).unwrap();
    let stage = drawers(560, 500);
    stage.insert_into(&mut editor, [0.0; 3]).unwrap();
    stage.insert_into(&mut editor, [700.0, 0.0, 0.0]).unwrap();
    let p = editor.project();
    assert_eq!(p.catalog.len(), 1);
    assert_eq!(p.slide_installations.len(), 6);
    assert!(
        slide_installation::diagnose_all(p)
            .iter()
            .all(|s| s.issues.is_empty())
    );
}

fn pdf_text(bytes: &[u8]) -> String {
    let path = std::env::temp_dir().join(format!("slides-{}.pdf", uuid::Uuid::new_v4()));
    std::fs::write(&path, bytes).unwrap();
    let output = std::process::Command::new("pdftotext")
        .args(["-layout", path.to_str().unwrap(), "-"])
        .output()
        .expect("pdftotext required for PDF verification");
    std::fs::remove_file(path).unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn both_pdfs_list_slides_to_buy_and_their_holes() {
    use plan_my_cabinet::catalog_pack::{CatalogRegistry, snapshot_foot};
    use plan_my_cabinet::export::{ExportMode, ExportSettings, ReceiptSections, prepare_export};
    use plan_my_cabinet::i18n::Language;
    use plan_my_cabinet::units::Quaternion;

    let mut editor = generated(560, 500);
    let registry = CatalogRegistry::bundled();
    let pack = registry.pack("generic-feet").unwrap();
    let family = pack.feet.iter().find(|f| f.id == "post-square").unwrap();
    let entry = snapshot_foot(pack, family, &family.variants[1], "en");
    let catalog_id = entry.id;
    let mut pin = Some(entry);
    for x in [0.0, 540.0] {
        editor
            .create_foot(
                "Foot".into(),
                catalog_id,
                pin.take(),
                None,
                Pose::new([x, 0.0, -100.0], Quaternion::IDENTITY).unwrap(),
            )
            .unwrap();
    }
    for language in [Language::En, Language::PtBr] {
        let prepared = prepare_export(
            editor.project(),
            ExportSettings {
                language,
                units: Unit::Mm,
            },
            ExportMode::Draft,
        )
        .unwrap();
        assert_eq!(prepared.slide_guidance.len(), 3);
        let simple = pdf_text(&plan_my_cabinet::pdf_export::render_pdf(&prepared).unwrap());
        let document = plan_my_cabinet::workshop_document::build_workshop_document(
            &prepared,
            ReceiptSections::default(),
        )
        .unwrap();
        let workshop =
            pdf_text(&plan_my_cabinet::pdf_export::render_document_pdf(&document).unwrap());
        for text in [&simple, &workshop] {
            // Word wrapping and kerning add spaces in extracted text.
            let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            let has = |needle: &str| {
                let needle: String = needle.chars().filter(|c| !c.is_whitespace()).collect();
                text.contains(&needle)
            };
            assert!(has("0073.045500SX"), "{text}");
            assert!(has("generic-post-square-100"), "{text}");
            match language {
                Language::En => {
                    assert!(has("3 pairs"), "{text}");
                    assert!(has("2 units"), "{text}");
                    assert!(has("holes from the front edge: 37 mm, 53 mm"), "{text}");
                    assert!(has("Generic reference dimensions"), "{text}");
                }
                Language::PtBr => {
                    assert!(has("3 pares"), "{text}");
                    assert!(has("furos a partir da borda da frente: 37 mm"), "{text}");
                }
            }
        }
    }
}

#[test]
fn slide_and_foot_edits_make_exports_stale() {
    use plan_my_cabinet::export::fingerprint;
    let mut editor = generated(560, 500);
    let before = fingerprint(editor.project());
    let mut slide = editor.project().slide_installations[0].clone();
    slide.height = Length::from_micrometres(slide.height.micrometres() + 5_000);
    slide_installation::update(&mut editor, slide).unwrap();
    let after = fingerprint(editor.project());
    assert_eq!(before.wood, after.wood, "slides are not wood");
    assert_ne!(before.packet, after.packet);
}

#[test]
fn propose_and_update_with_catalog_are_what_the_ui_uses() {
    let mut editor = generated(560, 500);
    let installation = editor.project().slide_installations[0].clone();
    let registry = plan_my_cabinet::catalog_pack::CatalogRegistry::bundled();
    let tt35 = registry.slide_lengths("fgvtn-slides", "tt35-slowmotion", "en");
    let proposal =
        slide_installation::propose(editor.project(), installation.drawer_root_id, &tt35).unwrap();
    assert_eq!(proposal.catalog.product_id, "0073.035500SX");
    let revision = editor.project().revision;
    slide_installation::update_with_catalog(
        &mut editor,
        Some(proposal.catalog.clone()),
        SlideInstallation {
            catalog_id: proposal.catalog.id,
            ..installation
        },
    )
    .unwrap();
    assert_eq!(editor.project().revision, revision + 1);
    assert_eq!(editor.project().catalog.len(), 2);
}
