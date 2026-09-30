use std::fs;

use plan_my_cabinet::document_layout::{Document, FONT_METRICS_VERSION, LAYOUT_VERSION, Primitive};
use plan_my_cabinet::domain::{Board, BoardGrain, Material, Project, SrgbColor};
use plan_my_cabinet::export::{
    ExportMode, ExportSettings, ExportStatus, OutputError, Overwrite, ReceiptSections,
    ReviewPreparationError, ReviewedPacket, ReviewedPacketKey, write_reviewed_pdf,
};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::units::{Length, Pose, Quaternion, Unit};
use printpdf::{Op, PdfDocument, PdfParseOptions, TextItem};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn fixture() -> Project {
    let mut project = Project::new("Workshop São João", Currency::Brl);
    let material = Uuid::new_v4();
    project.materials.push(Material {
        default_band: None,
        kind: Default::default(),
        id: material,
        name: "Birch".into(),
        default_thickness: Length::from_micrometres(18_000),
        default_grain: BoardGrain::Length,
    });
    project.boards.push(Board {
        banding: Default::default(),
        id: Uuid::new_v4(),
        name: "Unallocated hidden shelf".into(),
        material_id: material,
        length: Length::from_micrometres(500_000),
        width: Length::from_micrometres(300_000),
        thickness: Length::from_micrometres(18_000),
        grain_override: None,
        parent_id: None,
        pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
    });
    project
}

fn settings() -> ExportSettings {
    ExportSettings {
        language: Language::En,
        units: Unit::Mm,
    }
}

fn all_off() -> ReceiptSections {
    ReceiptSections {
        parts_and_costs: false,
        sheets_and_cut_steps: false,
        hinge_references: false,
    }
}

fn text(document: &Document) -> String {
    document
        .pages
        .iter()
        .flat_map(|page| &page.primitives)
        .filter_map(|p| match p {
            Primitive::Text(run) | Primitive::Notice(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn atomic_selection_retains_mandatory_content_and_eligibility() {
    let project = fixture();
    let packet =
        ReviewedPacket::prepare(&project, ExportMode::Draft, settings(), all_off()).unwrap();
    let content = text(packet.document());
    assert!(content.contains(&project.id.to_string()));
    assert!(content.contains("DRAFT / NOT FOR CUTTING"));
    assert!(content.contains("Unallocated hidden shelf"));
    assert!(content.contains("Optional detail omitted"));
    assert!(
        content.contains("not a cutting template") || content.contains("NOT A CUTTING TEMPLATE"),
        "{content}"
    );
    assert!(!packet.wood_issues().is_empty());
    assert!(matches!(
        ReviewedPacket::prepare(&project, ExportMode::ShopReady, settings(), all_off()),
        Err(ReviewPreparationError::Blocked(_))
    ));
    assert_eq!(packet.snapshot().sections(), Some(all_off()));
    assert_eq!(packet.document().layout_version, LAYOUT_VERSION);
    assert_eq!(packet.document().font_metrics_version, FONT_METRICS_VERSION);
}

#[test]
fn cache_identity_and_content_change_with_controls_and_edits() {
    let mut project = fixture();
    let base = ReviewedPacket::prepare(
        &project,
        ExportMode::Draft,
        settings(),
        ReceiptSections::default(),
    )
    .unwrap();
    let selected =
        ReviewedPacket::prepare(&project, ExportMode::Draft, settings(), all_off()).unwrap();
    assert_ne!(base.key(), selected.key());
    assert_ne!(base.document(), selected.document());
    let other_settings = ExportSettings {
        language: Language::PtBr,
        units: Unit::Foot,
    };
    let localized =
        ReviewedPacket::prepare(&project, ExportMode::Draft, other_settings, all_off()).unwrap();
    assert_ne!(localized.key(), selected.key());
    assert_ne!(localized.document(), selected.document());
    assert!(!selected.matches_source(&project, ExportMode::Draft, other_settings, all_off()));
    assert!(!selected.matches_source(
        &project,
        ExportMode::Draft,
        settings(),
        ReceiptSections::default()
    ));
    assert!(selected.matches_source(&project, ExportMode::Draft, settings(), all_off()));
    assert_eq!(selected.key().layout_version, LAYOUT_VERSION);
    assert_eq!(selected.key().font_metrics_version, FONT_METRICS_VERSION);
    project.name.push_str(" revised");
    project.revision += 1;
    assert!(!selected.matches_source(&project, ExportMode::Draft, settings(), all_off()));
    let refreshed =
        ReviewedPacketKey::for_project(&project, ExportMode::Draft, settings(), all_off()).unwrap();
    assert_ne!(refreshed.fingerprint, selected.key().fingerprint);
    assert!(matches!(
        ReviewedPacket::prepare_cancellable(
            &project,
            ExportMode::Draft,
            settings(),
            all_off(),
            || true
        ),
        Err(ReviewPreparationError::Cancelled)
    ));
}

#[test]
fn picker_errors_cancellation_and_atomic_overwrite_never_issue_receipt() {
    let project = fixture();
    let packet =
        ReviewedPacket::prepare(&project, ExportMode::Draft, settings(), all_off()).unwrap();
    let dir = std::env::temp_dir().join(format!("reviewed-packet-{}", Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let path = dir.join("reviewed.pdf");
    let original = b"previous bytes";
    fs::write(&path, original).unwrap();
    assert!(matches!(
        write_reviewed_pdf(&packet, None, Overwrite::Decline, || false),
        Err(OutputError::Cancelled)
    ));
    assert!(matches!(
        write_reviewed_pdf(&packet, Some(&path), Overwrite::Decline, || false),
        Err(OutputError::OverwriteRequired)
    ));
    assert!(matches!(
        write_reviewed_pdf(&packet, Some(&path), Overwrite::Confirm, || true),
        Err(OutputError::Cancelled)
    ));
    let calls = std::cell::Cell::new(0);
    assert!(matches!(
        write_reviewed_pdf(&packet, Some(&path), Overwrite::Confirm, || {
            calls.set(calls.get() + 1);
            calls.get() > 1
        }),
        Err(OutputError::Cancelled)
    ));
    assert!(
        calls.get() > 1,
        "cancelled after serializing, before commit"
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    let bad = dir.join("absent").join("packet.pdf");
    assert!(matches!(
        write_reviewed_pdf(&packet, Some(&bad), Overwrite::Decline, || false),
        Err(OutputError::Write(_))
    ));
    assert!(!bad.exists());
    assert!(project.export_records.is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn successful_write_serializes_reviewed_pages_and_records_actual_sections() {
    let mut project = fixture();
    let packet =
        ReviewedPacket::prepare(&project, ExportMode::Draft, settings(), all_off()).unwrap();
    let path = std::env::temp_dir().join(format!("reviewed-{}.pdf", Uuid::new_v4()));
    let receipt = write_reviewed_pdf(&packet, Some(&path), Overwrite::Decline, || false).unwrap();
    let bytes = fs::read(&path).unwrap();
    let pdf = PdfDocument::parse(&bytes, &PdfParseOptions::default(), &mut Vec::new()).unwrap();
    assert_eq!(pdf.pages.len(), packet.document().pages.len());
    for (source, serialized) in packet.document().pages.iter().zip(&pdf.pages) {
        let expected = source
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Text(run) | Primitive::Notice(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<String>();
        let actual = serialized
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::WriteText { items, .. } => Some(items),
                _ => None,
            })
            .flat_map(|items| items.iter())
            .filter_map(|item| match item {
                TextItem::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(actual, expected, "page {} differs", source.number);
    }
    assert_eq!(receipt.file_sha256, format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(receipt.metadata.mode, Some(ExportMode::Draft));
    assert_eq!(receipt.metadata.sections, Some(all_off()));
    assert_eq!(receipt.metadata.layout_version, Some(LAYOUT_VERSION as u16));
    assert_eq!(receipt.settings, settings());
    assert_eq!(receipt.revision, packet.key().revision);
    assert_eq!(receipt.project_id, packet.key().project_id);
    assert!(
        project.export_records.is_empty(),
        "caller persists a verified receipt separately"
    );
    project.export_records.push(receipt);
    assert_eq!(ExportStatus::for_project(&project), ExportStatus::Current);
    project
        .material_colors
        .insert(project.materials[0].id, SrgbColor([10, 40, 70]));
    project.revision += 1;
    assert_eq!(ExportStatus::for_project(&project), ExportStatus::Current);
    fs::remove_file(path).unwrap();
}

#[test]
fn color_edit_invalidates_review_key_but_not_manufacturing_freshness() {
    let mut project = fixture();
    let packet =
        ReviewedPacket::prepare(&project, ExportMode::Draft, settings(), all_off()).unwrap();
    let previous = packet.key().fingerprint.clone();
    project
        .material_colors
        .insert(project.materials[0].id, SrgbColor([10, 40, 70]));
    project.revision += 1;
    assert!(!packet.matches_source(&project, ExportMode::Draft, settings(), all_off()));
    let key =
        ReviewedPacketKey::for_project(&project, ExportMode::Draft, settings(), all_off()).unwrap();
    assert_eq!(key.fingerprint, previous);
    assert_ne!(key.revision, packet.key().revision);
}
