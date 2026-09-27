use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::{Project, SrgbColor};
use plan_my_cabinet::export::{
    ComparisonBaseline, ExportMode, ExportSettings, ExportStatus, Overwrite, ReceiptMetadata,
    ReceiptSections, fingerprint, prepare_export, write_pdf,
};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::pdf_export::render_pdf;
use plan_my_cabinet::persistence::{prepare_bytes, serialize};
use plan_my_cabinet::units::{Length, Unit};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const LEGACY: &[u8] = include_bytes!("fixtures/schema-v1-cabinet.pmcab");

fn settings() -> ExportSettings {
    ExportSettings {
        language: Language::En,
        units: Unit::Mm,
    }
}

fn pdf_text(project: &Project) -> String {
    let prepared = prepare_export(project, settings(), ExportMode::Draft).unwrap();
    let path = std::env::temp_dir().join(format!("freshness-text-{}.pdf", Uuid::new_v4()));
    std::fs::write(&path, render_pdf(&prepared).unwrap()).unwrap();
    let output = std::process::Command::new("pdftotext")
        .args(["-layout", path.to_str().unwrap(), "-"])
        .output()
        .expect("pdftotext required for PDF verification");
    std::fs::remove_file(path).unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn v1_and_v2_legacy_receipts_remain_comparable_across_color_edits() {
    let mut editor = prepare_bytes(LEGACY).unwrap().into_editor();
    let original = editor.project().export_records[0].clone();
    assert_eq!(original.metadata.fingerprint_version, None);
    assert_eq!(
        ExportStatus::for_project(editor.project()),
        ExportStatus::Current
    );
    let new_hash = fingerprint(editor.project());
    assert_ne!(original.wood_sha256, new_hash.wood);
    let id = editor.project().materials[0].id;
    editor
        .set_material_color(id, Some(SrgbColor([90, 20, 30])))
        .unwrap();
    assert!(editor.is_dirty());
    assert_eq!(
        ExportStatus::for_project(editor.project()),
        ExportStatus::Current
    );
    editor
        .set_cutting_kerf(Length::from_micrometres(4_000))
        .unwrap();
    assert_eq!(
        ExportStatus::for_project(editor.project()),
        ExportStatus::WoodStale
    );
    editor.undo().unwrap();
    assert_eq!(
        ExportStatus::for_project(editor.project()),
        ExportStatus::Current
    );
    let mut unmappable = editor.project().clone();
    unmappable
        .stock_aliases
        .insert(unmappable.stock[0].id, "S2".into());
    unmappable.next_stock_s_alias = 3;
    assert_eq!(
        ExportStatus::for_project(&unmappable),
        ExportStatus::Unknown
    );

    // Version-2 receipts used the same hash projection, but also captured the
    // alias map. It must catch a printed-label change that the old hash missed.
    let mut project = editor.project().clone();
    let record = &mut project.export_records[0];
    *record.metadata = ReceiptMetadata {
        metadata_version: Some(1),
        mode: Some(ExportMode::Draft),
        sections: Some(ReceiptSections::default()),
        fingerprint_version: Some(2),
        layout_version: Some(1),
        stock_aliases: Some(project.stock_aliases.clone()),
        comparison_baseline: Some(ComparisonBaseline::from_project(editor.project()).unwrap()),
    };
    let reopened = prepare_bytes(&serialize(&project).unwrap()).unwrap();
    assert_eq!(
        ExportStatus::for_project(reopened.project()),
        ExportStatus::Current
    );
    let mut changed_alias = reopened.project().clone();
    changed_alias
        .stock_aliases
        .insert(changed_alias.stock[0].id, "S2".into());
    changed_alias.next_stock_s_alias = 3;
    changed_alias.validate().unwrap();
    assert_eq!(
        ExportStatus::for_project(&changed_alias),
        ExportStatus::WoodStale
    );
    // A genuine output label change is not silently treated as a color edit.
    let mut changed_name = reopened.project().clone();
    changed_name.stock[0].name.push_str(" renamed");
    assert_eq!(
        ExportStatus::for_project(&changed_name),
        ExportStatus::WoodStale
    );
    let mut hardware_only = reopened.project().clone();
    hardware_only.hardware[0].name.push_str(" renamed");
    assert_eq!(
        ExportStatus::for_project(&hardware_only),
        ExportStatus::PacketStale
    );
}

#[test]
fn v4_receipt_hashes_printed_aliases_and_keeps_historical_bytes() {
    let mut project = prepare_bytes(LEGACY).unwrap().project().clone();
    let plan = prepare_export(
        &project,
        ExportSettings {
            language: Language::En,
            units: Unit::Mm,
        },
        ExportMode::Draft,
    )
    .unwrap();
    let path = std::env::temp_dir().join(format!("freshness-{}.pdf", Uuid::new_v4()));
    let receipt = write_pdf(&plan, Some(&path), Overwrite::Decline).unwrap();
    assert_eq!(receipt.metadata.fingerprint_version, Some(4));
    project.export_records.push(receipt.clone());
    let mut editor = ProjectEditor::new(project).unwrap();
    assert_eq!(
        ExportStatus::for_project(editor.project()),
        ExportStatus::Current
    );
    let old_record = editor.project().export_records.last().unwrap().clone();
    editor
        .set_material_color(editor.project().materials[0].id, Some(SrgbColor([1, 2, 3])))
        .unwrap();
    assert_eq!(
        ExportStatus::for_project(editor.project()),
        ExportStatus::Current
    );
    assert_eq!(editor.project().export_records.last(), Some(&old_record));
    let mut changed = editor.project().clone();
    changed
        .stock_aliases
        .insert(changed.stock[0].id, "S2".into());
    changed.next_stock_s_alias = 3;
    assert_eq!(ExportStatus::for_project(&changed), ExportStatus::WoodStale);
    let mut foreign = editor.project().clone();
    foreign.id = Uuid::new_v4();
    assert_eq!(ExportStatus::for_project(&foreign), ExportStatus::Unknown);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn placeholder_catalog_verification_changes_packet_and_pdf_warning() {
    let mut project = prepare_bytes(LEGACY).unwrap().project().clone();
    project.export_records.clear();
    let initial = prepare_export(&project, settings(), ExportMode::Draft).unwrap();
    let path = std::env::temp_dir().join(format!("hinge-freshness-{}.pdf", Uuid::new_v4()));
    let receipt = write_pdf(&initial, Some(&path), Overwrite::Decline).unwrap();
    std::fs::remove_file(path).unwrap();
    project.export_records.push(receipt);
    assert_eq!(ExportStatus::for_project(&project), ExportStatus::Current);
    let before = pdf_text(&project);
    assert!(!before.contains("Installation not verified"), "{before}");
    project.catalog[0].verified_hinge = None;
    assert_eq!(
        ExportStatus::for_project(&project),
        ExportStatus::PacketStale
    );
    assert_eq!(
        fingerprint(&project).wood,
        initial.snapshot.fingerprint().wood
    );
    let after = pdf_text(&project);
    assert!(after.contains("Installation not verified"), "{after}");
    assert!(
        after.contains("Installation dimensions unverified or invalid"),
        "{after}"
    );
}

#[test]
fn kerf_confirmation_warning_stales_packet_only_and_date_is_ignored() {
    let mut project = prepare_bytes(LEGACY).unwrap().project().clone();
    project.export_records.clear();
    let prepared = prepare_export(&project, settings(), ExportMode::Draft).unwrap();
    let path = std::env::temp_dir().join(format!("kerf-freshness-{}.pdf", Uuid::new_v4()));
    let receipt = write_pdf(&prepared, Some(&path), Overwrite::Decline).unwrap();
    std::fs::remove_file(path).unwrap();
    project.export_records.push(receipt);
    project.confirmed_shop_kerf_unix_ms = Some(1_700_000_000_000);
    assert_eq!(ExportStatus::for_project(&project), ExportStatus::Current);
    project.confirmed_shop_kerf = None;
    project.confirmed_shop_kerf_unix_ms = None;
    assert_eq!(
        fingerprint(&project).wood,
        prepared.snapshot.fingerprint().wood
    );
    assert_eq!(
        ExportStatus::for_project(&project),
        ExportStatus::PacketStale
    );
    assert!(pdf_text(&project).contains("Confirm the actual shop kerf"));
    project.confirmed_shop_kerf = Some(project.cutting_kerf);
    project.confirmed_shop_kerf_unix_ms = Some(1_700_000_000_001);
    assert_eq!(ExportStatus::for_project(&project), ExportStatus::Current);
}

#[test]
fn editor_snapshot_receipt_with_noninitial_alias_is_immediately_current() {
    let mut project = prepare_bytes(LEGACY).unwrap().project().clone();
    project.export_records.clear();
    let stock_id = project.stock[0].id;
    project.stock_aliases.insert(stock_id, "S2".into());
    project.next_stock_s_alias = 3;
    let mut editor = ProjectEditor::new(project).unwrap();
    let snapshot = editor.export_snapshot(settings()).unwrap();
    assert_eq!(snapshot.project().stock_alias(stock_id), Some("S2"));
    let path = std::env::temp_dir().join(format!("snapshot-{}.pdf", Uuid::new_v4()));
    let bytes = b"committed PDF bytes";
    std::fs::write(&path, bytes).unwrap();
    let hash = format!("{:x}", Sha256::digest(bytes));
    editor
        .record_completed_export(&snapshot, &path, 1_700_000_000_000, &hash)
        .unwrap();
    std::fs::remove_file(path).unwrap();
    let record = editor.project().export_records.last().unwrap();
    assert_eq!(record.metadata.fingerprint_version, Some(4));
    assert_eq!(
        record
            .metadata
            .stock_aliases
            .as_ref()
            .unwrap()
            .get(&stock_id)
            .unwrap(),
        "S2"
    );
    assert_eq!(
        ExportStatus::for_project(editor.project()),
        ExportStatus::Current
    );
}

#[test]
fn alias_counter_exhaustion_is_an_error_or_unknown_not_a_panic() {
    let mut project = prepare_bytes(LEGACY).unwrap().project().clone();
    project.stock_aliases.clear();
    project.next_stock_s_alias = u64::MAX;
    project.validate().unwrap();
    assert!(ProjectEditor::new(project.clone()).is_err());
    assert!(plan_my_cabinet::export::ExportSnapshot::new(&project, settings()).is_err());
    assert!(prepare_export(&project, settings(), ExportMode::Draft).is_err());
    assert!(prepare_export(&project, settings(), ExportMode::ShopReady).is_err());
    assert_eq!(ExportStatus::for_project(&project), ExportStatus::Unknown);
}

#[test]
fn direct_snapshot_pdf_and_receipt_share_advanced_counter_alias() {
    let mut project = prepare_bytes(LEGACY).unwrap().project().clone();
    project.export_records.clear();
    project.stock_aliases.clear();
    project.next_stock_s_alias = 7;
    let id = project.stock[0].id;
    let prepared = prepare_export(&project, settings(), ExportMode::Draft).unwrap();
    assert_eq!(prepared.snapshot.project().stock_alias(id), Some("S7"));
    let text = pdf_text(&project);
    assert!(text.contains(&format!("S7 [{id}]")), "{text}");
    assert!(!text.contains(&format!("S1 [{id}]")), "{text}");
    let path = std::env::temp_dir().join(format!("advanced-alias-{}.pdf", Uuid::new_v4()));
    let receipt = write_pdf(&prepared, Some(&path), Overwrite::Decline).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        receipt
            .metadata
            .stock_aliases
            .as_ref()
            .unwrap()
            .get(&id)
            .unwrap(),
        "S7"
    );
    project.export_records.push(receipt);
    assert_eq!(ExportStatus::for_project(&project), ExportStatus::Current);
}
