use std::fs;

use plan_my_cabinet::domain::{BoardGrain, DomainError, Material, Project, SrgbColor};
use plan_my_cabinet::export::{
    ComparisonBaseline, ExportMode, ExportSettings, MAX_COMPARISON_BYTES, OutputError, Overwrite,
    ReceiptSections, prepare_export, write_pdf,
};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence::{
    MAX_DOCUMENT_BYTES, PersistenceError, prepare_bytes, serialize,
};
use plan_my_cabinet::units::Unit;
use uuid::Uuid;

fn settings() -> ExportSettings {
    ExportSettings {
        language: Language::En,
        units: Unit::Mm,
    }
}

fn loaded(json: &serde_json::Value) -> Result<Project, PersistenceError> {
    prepare_bytes(&serde_json::to_vec(json).unwrap()).map(|prepared| prepared.project().clone())
}

#[test]
fn successful_write_records_mode_sections_baseline_and_history_is_immutable() {
    let mut project = Project::new("cabinet", Currency::Brl);
    let prepared = prepare_export(&project, settings(), ExportMode::Draft).unwrap();
    let path = std::env::temp_dir().join(format!("receipt-{}.pdf", Uuid::new_v4()));
    assert!(matches!(
        write_pdf(&prepared, None, Overwrite::Decline),
        Err(OutputError::Cancelled)
    ));
    assert!(project.export_records.is_empty());
    let receipt = write_pdf(&prepared, Some(&path), Overwrite::Decline).unwrap();
    assert_eq!(receipt.metadata.mode, Some(ExportMode::Draft));
    assert_eq!(receipt.metadata.sections, Some(ReceiptSections::default()));
    assert_eq!(receipt.metadata.metadata_version, Some(1));
    assert_eq!(receipt.metadata.fingerprint_version, Some(5));
    assert_eq!(receipt.completed_at(), Some(receipt.completed_unix_ms));
    assert!(
        receipt
            .metadata
            .comparison_baseline
            .as_ref()
            .unwrap()
            .is_valid()
    );
    project.export_records.push(receipt.clone());
    let original = serialize(&project).unwrap();
    assert_eq!(
        loaded(&serde_json::from_slice(&original).unwrap())
            .unwrap()
            .export_records,
        vec![receipt.clone()]
    );
    project.name = "newer name".into();
    assert_eq!(project.export_records, vec![receipt]);
    fs::remove_file(path).unwrap();
}

#[test]
fn legacy_missing_fields_remain_unknown_and_original_receipt_is_lossless() {
    let project = Project::new("legacy", Currency::Brl);
    let mut value = serde_json::to_value(&project).unwrap();
    let id = project.id.to_string();
    value["schema_version"] = 1.into();
    value["export_records"] = serde_json::json!([{
        "project_id": id, "revision": 7,
        "wood_sha256": "a".repeat(64), "packet_sha256": "b".repeat(64),
        "settings": settings(), "path": "old.pdf", "file_sha256": "c".repeat(64)
    }]);
    let original = value["export_records"][0].clone();
    let reopened = loaded(&value).unwrap();
    let record = &reopened.export_records[0];
    assert_eq!(record.completed_at(), None);
    assert_eq!(record.metadata.mode, None);
    assert_eq!(record.metadata.sections, None);
    assert_eq!(record.changed_entries(&reopened).unwrap(), None);
    let saved: serde_json::Value = serde_json::from_slice(&serialize(&reopened).unwrap()).unwrap();
    assert_eq!(saved["export_records"][0], original);
}

#[test]
fn strict_version_alias_baseline_hash_and_size_validation() {
    let project = Project::new("valid", Currency::Brl);
    let plan = prepare_export(&project, settings(), ExportMode::Draft).unwrap();
    let path = std::env::temp_dir().join(format!("receipt-{}.pdf", Uuid::new_v4()));
    let receipt = write_pdf(&plan, Some(&path), Overwrite::Decline).unwrap();
    let mut project = project;
    project.export_records.push(receipt);
    let valid = serde_json::to_value(&project).unwrap();
    for (key, invalid) in [
        ("metadata_version", serde_json::json!(99)),
        ("fingerprint_version", serde_json::json!(99)),
        ("mode", serde_json::json!("Guess")),
        ("wood_sha256", serde_json::json!("bad")),
        (
            "stock_aliases",
            serde_json::json!({"00000000-0000-0000-0000-000000000001": "S00"}),
        ),
    ] {
        let mut candidate = valid.clone();
        candidate["export_records"][0][key] = invalid;
        assert!(loaded(&candidate).is_err(), "{key}");
    }
    let mut candidate = valid.clone();
    candidate["export_records"][0]["comparison_baseline"]["version"] = 99.into();
    assert!(matches!(
        loaded(&candidate),
        Err(PersistenceError::InvalidProject(
            DomainError::InvalidExportRecord
        ))
    ));
    let mut oversized = project.clone();
    oversized.name = "X".repeat(MAX_DOCUMENT_BYTES);
    assert!(matches!(
        serialize(&oversized),
        Err(PersistenceError::TooLarge)
    ));
    assert!(matches!(
        prepare_bytes(&vec![b' '; MAX_DOCUMENT_BYTES + 1]),
        Err(PersistenceError::TooLarge)
    ));
    let mut baseline = ComparisonBaseline::from_project(&project).unwrap();
    baseline
        .entries
        .push(plan_my_cabinet::export::ComparisonEntry {
            kind: plan_my_cabinet::export::ComparisonKind::Board,
            id: Uuid::new_v4(),
            name: "a".repeat(MAX_COMPARISON_BYTES),
            facts: Default::default(),
        });
    assert!(!baseline.is_valid());
    fs::remove_file(path).unwrap();
}

#[test]
fn comparison_uses_recorded_values_and_ignores_presentation_color() {
    let mut project = Project::new("evidence", Currency::Brl);
    let id = Uuid::new_v4();
    project.materials.push(Material {
        id,
        name: "Birch".into(),
        default_thickness: plan_my_cabinet::units::Length::from_micrometres(18_000),
        default_grain: BoardGrain::Length,
    });
    let baseline = ComparisonBaseline::from_project(&project).unwrap();
    project.material_colors.insert(id, SrgbColor([21, 42, 63]));
    assert!(baseline.changed_entries(&project).unwrap().is_empty());
    project.materials[0].name = "Maple".into();
    let changes = baseline.changed_entries(&project).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].previous.as_ref().unwrap().name, "Birch");
    assert_eq!(changes[0].current.as_ref().unwrap().name, "Maple");
    project.materials.clear();
    let removed = baseline.changed_entries(&project).unwrap();
    assert_eq!(removed.len(), 1);
    assert!(removed[0].current.is_none());
}

#[test]
fn oversized_evidence_blocks_export_before_destination_is_touched() {
    let mut project = Project::new("oversized", Currency::Brl);
    project.materials.push(Material {
        id: Uuid::new_v4(),
        name: "a".repeat(MAX_COMPARISON_BYTES),
        default_thickness: plan_my_cabinet::units::Length::from_micrometres(18_000),
        default_grain: BoardGrain::Length,
    });
    let prepared = prepare_export(&project, settings(), ExportMode::Draft).unwrap();
    let path = std::env::temp_dir().join(format!("receipt-{}.pdf", Uuid::new_v4()));
    fs::write(&path, b"previous").unwrap();
    assert!(matches!(
        write_pdf(&prepared, Some(&path), Overwrite::Confirm),
        Err(OutputError::Verify(
            plan_my_cabinet::export::ExportError::InvalidMetadata
        ))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"previous");
    fs::remove_file(path).unwrap();
}
