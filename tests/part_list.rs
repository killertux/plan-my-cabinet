//! The format-neutral part list and the CorteCloud file built from it, on
//! the reference cabinet: doors on hinges, a mixed cut plan, two MDFs.
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::{BoardEdge, EdgeBand, EdgeBanding, SrgbColor};
use plan_my_cabinet::export::{ExportMode, ExportSettings, OutputError, Overwrite, prepare_export};
use plan_my_cabinet::formats::{self, ExportFormat, FormatError, FormatOptions};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::machining::{MachiningOptions, OmissionReason, PilotHole};
use plan_my_cabinet::part_list::{self, PartListBlocked};
use plan_my_cabinet::reference_fixture::{self as fixture, HINGE_IDS, LEFT_DOOR_ID, WHITE_ID};
use plan_my_cabinet::units::{Length, Unit};

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}

fn banded() -> plan_my_cabinet::domain::Project {
    let mut project = fixture::project();
    let band = uuid::Uuid::from_u128(0xba4d);
    project.edge_bands.push(EdgeBand {
        id: band,
        name: "Fita Branca 1x22".into(),
        thickness: mm(1),
        height: mm(22),
        color: SrgbColor([255; 3]),
    });
    project
        .materials
        .iter_mut()
        .find(|m| m.id == WHITE_ID)
        .unwrap()
        .default_band = Some(band);
    project.validate().unwrap();
    project
}

#[test]
fn hinge_cups_drill_the_doors_the_pdf_would_guide() {
    let project = fixture::project();
    let list = part_list::build(&project, &MachiningOptions::default()).unwrap();
    assert_eq!(list.part_count(), project.boards.len());
    let door = list
        .groups
        .iter()
        .find(|g| g.boards.contains(&LEFT_DOOR_ID))
        .unwrap();
    // Two hinges per door, each one 35 mm cup.
    assert_eq!(door.machining.face_drills.len(), 2);
    assert!(
        door.machining
            .face_drills
            .iter()
            .all(|d| d.diameter == mm(35) && !d.through)
    );
    // The same hinges the workshop PDF prints guidance for, and no others.
    let prepared = prepare_export(
        &project,
        ExportSettings {
            language: Language::En,
            units: Unit::Mm,
        },
        ExportMode::Draft,
    )
    .unwrap();
    let drilled: std::collections::BTreeSet<_> = list
        .groups
        .iter()
        .flat_map(|g| &g.machining.face_drills)
        .map(|d| d.source.installation())
        .collect();
    let guided: std::collections::BTreeSet<_> = prepared
        .installation_guidance
        .iter()
        .map(|g| g.id)
        .collect();
    assert_eq!(drilled, guided);
    // Plate screws have no pilot size in the catalog: listed, not guessed.
    assert!(list.omissions.iter().any(|o| o.installation == HINGE_IDS[0]
        && o.reason == OmissionReason::PilotSizeUnknown
        && o.holes == 2));
}

#[test]
fn a_pilot_size_given_for_the_export_drills_the_plate_screws() {
    let project = fixture::project();
    let options = MachiningOptions {
        screw_pilot: Some(PilotHole {
            diameter: Length::from_micrometres(2_500),
            depth: mm(10),
        }),
    };
    let list = part_list::build(&project, &options).unwrap();
    let drills: Vec<_> = list
        .groups
        .iter()
        .flat_map(|g| &g.machining.face_drills)
        .collect();
    let cups = drills.iter().filter(|d| d.diameter == mm(35)).count();
    let plate_holes = drills
        .iter()
        .filter(|d| d.diameter == Length::from_micrometres(2_500))
        .count();
    assert!(cups > 0);
    assert_eq!(plate_holes, 2 * cups);
    // Only hinges with issues are left out now.
    assert!(
        list.omissions
            .iter()
            .all(|o| o.reason != OmissionReason::PilotSizeUnknown),
        "{:?}",
        list.omissions
    );
}

#[test]
fn identical_parts_group_and_banding_or_holes_split_them() {
    let project = banded();
    let list = part_list::build(&project, &MachiningOptions::default()).unwrap();
    let sides: Vec<_> = list
        .groups
        .iter()
        .filter(|g| g.name.ends_with("side"))
        .collect();
    // Left and right side have different names, so they stay apart.
    assert_eq!(sides.len(), 2);
    let mut editor = ProjectEditor::new(project).unwrap();
    // Rename the right side like the left: now they are one part of two...
    editor
        .rename_object(fixture::RIGHT_SIDE_ID, "Left side")
        .unwrap();
    let list = part_list::build(editor.project(), &MachiningOptions::default()).unwrap();
    let side = list.groups.iter().find(|g| g.name == "Left side").unwrap();
    assert_eq!(side.quantity(), 2, "{:?}", side.banding);
    // ...until one of them is banded differently.
    editor
        .set_edge_banding(
            &[fixture::RIGHT_SIDE_ID],
            &[BoardEdge::MaxX],
            EdgeBanding::Off,
        )
        .unwrap();
    let before = list.fingerprint();
    let list = part_list::build(editor.project(), &MachiningOptions::default()).unwrap();
    assert_eq!(
        list.groups.iter().filter(|g| g.name == "Left side").count(),
        2
    );
    assert_ne!(list.fingerprint(), before);
    // The cabinet goes with each part.
    assert!(list.groups.iter().all(|g| g.cabinet.is_some()));
}

#[test]
fn the_cortecloud_file_needs_only_a_valid_design() {
    // No stock, no cut plan, an unconfirmed kerf: still exportable.
    let mut project = banded();
    project.stock.clear();
    project.allocations.clear();
    project.stock_aliases.clear();
    project.confirmed_shop_kerf = None;
    project.validate().unwrap();
    let (list, bytes) = formats::render(
        ExportFormat::CorteCloudJson,
        &project,
        &FormatOptions::default(),
    )
    .unwrap();
    let file: formats::cortecloud::File = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(file.parts.len(), list.groups.len());
    for part in &file.parts {
        if let Some(machining) = &part.machining {
            assert_eq!((machining.x, machining.y), (part.c, part.l), "{part:?}");
        }
        assert!(
            part.material.ends_with(" 18") || part.material.ends_with(" 3"),
            "{part:?}"
        );
    }
    // The door whose hinges pass the checks carries its cups; the other
    // door's hinges have issues and are left out, as in the workshop PDF.
    let drilled: Vec<_> = file
        .parts
        .iter()
        .filter(|p| p.machining.is_some())
        .collect();
    assert_eq!(drilled.len(), 1, "{drilled:?}");
    assert!(drilled[0].function.starts_with("Door"));
    assert!(!list.omissions.is_empty());
    assert!(
        file.parts
            .iter()
            .any(|p| p.c1.as_deref() == Some("Fita Branca 1x22"))
    );

    // An empty design is refused, with the reason.
    let empty =
        plan_my_cabinet::domain::Project::new("Empty", plan_my_cabinet::money::Currency::Brl);
    assert_eq!(
        formats::render(
            ExportFormat::CorteCloudJson,
            &empty,
            &FormatOptions::default()
        )
        .unwrap_err(),
        FormatError::Blocked(PartListBlocked::NoBoards)
    );
}

#[test]
fn writing_records_a_receipt_that_goes_stale_with_the_design() {
    let dir = std::env::temp_dir().join(format!("pmcab-cortecloud-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(ExportFormat::CorteCloudJson.file_name("Kitchen base 800"));
    assert!(path.ends_with("Kitchen base 800-cortecloud.json"));
    let mut editor = ProjectEditor::new(banded()).unwrap();
    let options = FormatOptions::default();
    let record = formats::write(
        ExportFormat::CorteCloudJson,
        editor.project(),
        &options,
        &path,
        Overwrite::Decline,
    )
    .unwrap();
    assert!(record.is_current(editor.project(), &options));
    let revision = editor.project().revision;
    editor.record_file_export(record.clone()).unwrap();
    assert_eq!(
        editor.project().revision,
        revision,
        "a receipt is not an edit"
    );
    // Writing again needs consent.
    assert!(matches!(
        formats::write(
            ExportFormat::CorteCloudJson,
            editor.project(),
            &options,
            &path,
            Overwrite::Decline,
        ),
        Err(OutputError::OverwriteRequired)
    ));
    // Resizing a door changes the parts: the file is no longer current, and
    // undo keeps the receipt.
    editor
        .transact(|p| -> Result<(), ()> {
            p.board_mut(LEFT_DOOR_ID).unwrap().width = mm(390);
            Ok(())
        })
        .unwrap();
    assert!(!editor.project().file_exports[0].is_current(editor.project(), &options));
    editor.undo().unwrap();
    assert_eq!(editor.project().file_exports, [record]);
    assert!(editor.project().file_exports[0].is_current(editor.project(), &options));
    std::fs::remove_dir_all(dir).unwrap();
}
