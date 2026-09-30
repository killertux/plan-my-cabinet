use std::path::PathBuf;

use plan_my_cabinet::domain::{Project, SrgbColor};
use plan_my_cabinet::export::{
    ComparisonBaseline, ComparisonKind, ExportMode, ExportRecord, ExportSettings, ReceiptMetadata,
    ReceiptSections, fingerprint,
};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::persistence::prepare_bytes;
use plan_my_cabinet::receipt_read_models::{ReceiptFreshness as Fresh, SinceThen, receipt_cards};
use plan_my_cabinet::units::Unit;

const LEGACY: &[u8] = include_bytes!("fixtures/schema-v1-cabinet.pmcab");

fn fixture() -> Project {
    let mut project = prepare_bytes(LEGACY).unwrap().project().clone();
    project.export_records.clear();
    project
}

fn receipt(project: &Project, name: &str, sections: ReceiptSections) -> ExportRecord {
    let hashes = fingerprint(project);
    ExportRecord {
        project_id: project.id,
        revision: project.revision,
        wood_sha256: hashes.wood,
        packet_sha256: hashes.packet,
        settings: ExportSettings {
            language: Language::PtBr,
            units: Unit::Foot,
        },
        path: PathBuf::from(format!("/exports/{name}")),
        completed_unix_ms: 1_700_000_000_000,
        file_sha256: "a".repeat(64),
        metadata: Box::new(ReceiptMetadata {
            metadata_version: Some(1),
            mode: Some(ExportMode::Draft),
            sections: Some(sections),
            fingerprint_version: Some(6),
            layout_version: Some(1),
            stock_aliases: Some(project.stock_aliases.clone()),
            comparison_baseline: Some(ComparisonBaseline::from_project(project).unwrap()),
        }),
    }
}

#[test]
fn legacy_values_stay_unknown_and_hashes_do_not_invent_details() {
    let project = prepare_bytes(LEGACY).unwrap().project().clone();
    let card = &receipt_cards(&project)[0];
    let record = &project.export_records[0];
    assert_eq!(
        card.filename,
        record.path.file_name().unwrap().to_string_lossy()
    );
    assert_eq!(card.mode, None);
    assert_eq!(card.sections, None);
    assert_eq!(card.completed_unix_ms, record.completed_at());
    assert_eq!(card.included_hardware, Fresh::Unavailable);
    assert_eq!(card.since_then, SinceThen::DetailsUnavailable);

    let mut changed = project;
    changed.boards[0].name.push_str(" changed");
    let stale = &receipt_cards(&changed)[0];
    assert_eq!(stale.wood, Fresh::Outdated);
    assert_eq!(stale.since_then, SinceThen::DetailsUnavailable);
    assert_eq!(stale.mode, None);

    changed.export_records[0].completed_unix_ms = 0;
    assert_eq!(receipt_cards(&changed)[0].completed_unix_ms, None);
}

#[test]
fn hardware_change_stales_included_guidance_but_not_wood() {
    let mut project = fixture();
    project
        .export_records
        .push(receipt(&project, "first.pdf", ReceiptSections::default()));
    let id = project.hardware[0].id;
    project.hardware[0].name = "Revised hinge".into();
    let card = &receipt_cards(&project)[0];
    assert_eq!(card.wood, Fresh::Current);
    assert_eq!(card.packet, Fresh::Outdated);
    assert_eq!(card.included_hardware, Fresh::Outdated);
    let SinceThen::RecordedChanges(changes) = &card.since_then else {
        panic!("expected recorded evidence");
    };
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ComparisonKind::Hardware);
    assert_eq!(changes[0].id, id);
    assert_eq!(changes[0].previous.as_ref().unwrap().name, "Pinned hinge");
    assert_eq!(changes[0].current.as_ref().unwrap().name, "Revised hinge");

    let mut omitted = fixture();
    let sections = ReceiptSections {
        hinge_references: false,
        ..ReceiptSections::default()
    };
    omitted
        .export_records
        .push(receipt(&omitted, "omitted.pdf", sections));
    omitted.hardware[0].name = "Revised hinge".into();
    assert_eq!(
        receipt_cards(&omitted)[0].included_hardware,
        Fresh::NotIncluded
    );
}

#[test]
fn color_only_edit_is_manufacturing_neutral_despite_revision() {
    let mut project = fixture();
    project
        .export_records
        .push(receipt(&project, "colors.pdf", ReceiptSections::default()));
    project
        .material_colors
        .insert(project.materials[0].id, SrgbColor([8, 9, 10]));
    project.revision += 1;
    let card = &receipt_cards(&project)[0];
    assert_eq!(card.packet, Fresh::Current);
    assert_eq!(card.wood, Fresh::Current);
    assert_eq!(card.included_hardware, Fresh::Current);
    assert_eq!(card.since_then, SinceThen::NoRecordedChanges);
    assert_eq!(card.revision, project.revision - 1);
}

#[test]
fn supersession_is_independent_of_freshness_and_keeps_recorded_fields() {
    let mut project = fixture();
    let first = receipt(&project, "one.pdf", ReceiptSections::default());
    project.export_records.push(first);
    let mut second = receipt(&project, "two.pdf", ReceiptSections::default());
    second.metadata.mode = Some(ExportMode::ShopReady);
    second.completed_unix_ms += 1;
    project.export_records.push(second);
    let cards = receipt_cards(&project);
    assert!(!cards[0].superseded);
    assert_eq!(cards[0].filename, "two.pdf");
    assert_eq!(cards[0].mode, Some(ExportMode::ShopReady));
    assert!(cards[1].superseded);
    assert_eq!(cards[1].filename, "one.pdf");
    assert_eq!(cards[1].packet, Fresh::Current);
    assert_eq!(cards[0].settings.units, Unit::Foot);
    assert_eq!(cards[0].file_sha256, "a".repeat(64));
}

#[test]
fn each_receipt_uses_its_own_hashes_not_the_newest_receipts() {
    let mut project = fixture();
    project
        .export_records
        .push(receipt(&project, "before.pdf", ReceiptSections::default()));
    project.boards[0].name.push_str(" revised");
    project
        .export_records
        .push(receipt(&project, "after.pdf", ReceiptSections::default()));
    let cards = receipt_cards(&project);
    assert_eq!(cards[0].wood, Fresh::Current);
    assert_eq!(cards[1].wood, Fresh::Outdated);
    assert!(cards[1].superseded);
    assert!(!cards[0].superseded);
    let SinceThen::RecordedChanges(changes) = &cards[1].since_then else {
        panic!("expected historical board evidence");
    };
    assert_eq!(changes[0].kind, ComparisonKind::Board);
}

#[test]
fn unrecorded_packet_cause_has_no_fabricated_change() {
    let mut project = fixture();
    project
        .export_records
        .push(receipt(&project, "old.pdf", ReceiptSections::default()));
    project.confirmed_shop_kerf = None;
    project.confirmed_shop_kerf_unix_ms = None;
    let card = &receipt_cards(&project)[0];
    assert_eq!(card.wood, Fresh::Current);
    assert_eq!(card.packet, Fresh::Outdated);
    assert_eq!(card.included_hardware, Fresh::Unavailable);
    assert_eq!(card.since_then, SinceThen::DetailsUnavailable);
}

#[test]
fn unavailable_comparison_still_retains_the_history_card() {
    let mut project = fixture();
    project.export_records.push(receipt(
        &project,
        "retained.pdf",
        ReceiptSections::default(),
    ));
    project.stock_aliases.clear();
    project.next_stock_s_alias = u64::MAX;
    let card = &receipt_cards(&project)[0];
    assert_eq!(card.filename, "retained.pdf");
    assert_eq!(card.packet, Fresh::Unavailable);
    assert_eq!(card.since_then, SinceThen::DetailsUnavailable);
}
