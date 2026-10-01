//! Run locally with no environment variables, or exercise a transfer by setting
//! PMCAB_TRANSFER_DIR and PMCAB_TRANSFER_ROLE=write/read/verify in that order.
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::{
    Allocation, Board, BoardGrain, CatalogReference, Hardware, HardwareKind, Material, Project,
    SCHEMA_VERSION, Stock, StockGrain, StockSource,
};
use plan_my_cabinet::money::{Currency, Money};
use plan_my_cabinet::persistence::{prepare_bytes, prepare_reader, save, serialize};
use plan_my_cabinet::units::{Length, Pose, Quaternion};
use uuid::Uuid;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn mm(value: i64) -> Length {
    Length::from_micrometres(value * 1000)
}

fn fixture() -> Project {
    let mut p = Project::new("Armário / Cabinet", Currency::Brl);
    p.id = id(1);
    p.materials.push(Material {
        coating: Default::default(),
        default_band: None,
        kind: Default::default(),
        id: id(2),
        name: "Compensado".into(),
        default_thickness: mm(18),
        default_grain: BoardGrain::Length,
    });
    p.boards.push(Board {
        coated_face: Default::default(),
        banding: Default::default(),
        id: id(3),
        name: "Prateleira".into(),
        material_id: id(2),
        length: mm(100),
        width: mm(50),
        thickness: mm(18),
        grain_override: Some(BoardGrain::Width),
        parent_id: None,
        pose: Pose::new([1.0005, -2.0, 0.0], Quaternion::IDENTITY).unwrap(),
    });
    p.stock.push(Stock {
        id: id(4),
        name: "Chapa".into(),
        material_id: id(2),
        length: mm(200),
        width: mm(100),
        thickness: mm(18),
        grain: StockGrain::AlongY,
        source: StockSource::ToPurchase,
        price: Some(Money::new(Currency::Brl, 20_005).unwrap()),
        priority: 0,
        trim: [Length::ZERO; 4],
    });
    p.allocations.push(Allocation {
        id: id(5),
        board_id: id(3),
        stock_id: id(4),
        origin: [mm(10), mm(20)],
        quarter_turn: true,
        locked: true,
    });
    p.catalog.push(CatalogReference {
        id: id(6),
        name: "Test fixture — not a verified hinge".into(),
        product_id: "TEST-ONLY".into(),
        plate_id: None,
        source: "test fixture".into(),
        revision: "1".into(),
        installation_dimensions: HashMap::new(),
        verified_hinge: None,
        origin: None,
        item: None,
    });
    p.hardware.push(Hardware {
        id: id(7),
        name: "Reference".into(),
        parent_id: None,
        pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        kind: HardwareKind::Catalog { catalog_id: id(6) },
    });
    p.stock_aliases.insert(id(4), "S1".into());
    p.next_stock_s_alias = 2;
    p
}

fn reopen(path: &Path) -> ProjectEditor {
    prepare_reader(File::open(path).unwrap())
        .unwrap()
        .into_editor()
}

#[test]
fn offline_save_transfer_reopen() {
    let directory = std::env::var_os("PMCAB_TRANSFER_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("pmcab-portable-{}", Uuid::new_v4())));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("source.pmcab");
    let returned = directory.join("returned.pmcab");
    match std::env::var("PMCAB_TRANSFER_ROLE").as_deref() {
        Ok("write") => {
            let mut editor = ProjectEditor::new(fixture()).unwrap();
            save(&mut editor, &source).unwrap();
            assert!(!editor.is_dirty());
        }
        Ok("read") => {
            let mut editor = reopen(&source);
            assert_eq!(editor.project(), &fixture());
            editor
                .transact(|p| -> Result<(), ()> {
                    p.name = "Offline Linux edit".into();
                    Ok(())
                })
                .unwrap();
            save(&mut editor, &returned).unwrap();
        }
        Ok("verify") => {
            let mut expected = fixture();
            expected.name = "Offline Linux edit".into();
            expected.revision = 1;
            assert_eq!(reopen(&source).project(), &fixture());
            assert_eq!(reopen(&returned).project(), &expected);
        }
        Err(_) => {
            let mut editor = ProjectEditor::new(fixture()).unwrap();
            save(&mut editor, &source).unwrap();
            assert_eq!(reopen(&source).project(), &fixture());
            std::fs::remove_dir_all(&directory).unwrap();
        }
        Ok(other) => panic!("unknown transfer role: {other}"),
    }
}

const LEGACY: &[u8] = include_bytes!("fixtures/schema-v1-cabinet.pmcab");

#[test]
fn legacy_golden_migrates_losslessly_offline_and_saves_only_on_request() {
    let directory = std::env::temp_dir().join(format!("pmcab-v1-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let source = directory.join("source.pmcab");
    let transferred = directory.join("transferred.pmcab");
    std::fs::write(&source, LEGACY).unwrap();
    let mut editor = reopen(&source);
    let p = editor.project();
    assert_eq!(p.schema_version, SCHEMA_VERSION);
    assert_eq!(p.revision, 17);
    assert!(!editor.is_dirty());
    assert!(!editor.can_undo());
    assert!(!editor.can_redo());
    assert_eq!(editor.saved_revision(), Some(17));
    // Independent typed decoding proves no migration quantization or catalog refresh.
    let mut expected: Project = serde_json::from_slice(LEGACY).unwrap();
    expected.schema_version = SCHEMA_VERSION;
    expected.stock_aliases.insert(id(4), "S1".into());
    expected.next_stock_s_alias = 2;
    // Material kinds predate nothing in the file: they are inferred from names.
    expected.materials[0].kind = plan_my_cabinet::domain::MaterialKind::Plywood;
    assert_eq!(p, &expected);
    assert_eq!(p.boards[0].thickness.micrometres(), 18_200);
    assert_eq!(p.materials[0].default_thickness.micrometres(), 19_000);
    assert_eq!(
        p.boards[0].effective_grain(&p.materials[0]),
        BoardGrain::Width
    );
    assert_eq!(
        p.boards[0].pose.translation_mm,
        [1.0005, -2.000000125, 3.123456789]
    );
    let world = plan_my_cabinet::assembly_edit::world_pose(p, id(3)).unwrap();
    let hashes = plan_my_cabinet::export::fingerprint(p);
    // Newly emitted hashes include printed aliases; historical hashes remain
    // comparable by the legacy algorithm without rewriting receipt bytes.
    assert_ne!(hashes.wood, p.export_records[0].wood_sha256);
    assert_ne!(hashes.packet, p.export_records[0].packet_sha256);
    assert_eq!(
        plan_my_cabinet::export::ExportStatus::for_project(p),
        plan_my_cabinet::export::ExportStatus::Current
    );
    assert_eq!(std::fs::read(&source).unwrap(), LEGACY);

    save(&mut editor, &transferred).unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), LEGACY);
    let loaded = reopen(&transferred);
    assert_eq!(loaded.project(), &expected);
    assert_eq!(
        plan_my_cabinet::assembly_edit::world_pose(loaded.project(), id(3)).unwrap(),
        world
    );
    assert!(!loaded.is_dirty());
    // Compare all JSON fields as well, including IDs, prices and historical receipts.
    let mut original: serde_json::Value = serde_json::from_slice(LEGACY).unwrap();
    original["schema_version"] = SCHEMA_VERSION.into();
    // Version 3 writes the catalog-pack fields with their exact v1 defaults.
    for entry in original["catalog"].as_array_mut().unwrap() {
        entry["origin"] = serde_json::Value::Null;
        if let Some(facts) = entry.get_mut("verified_hinge").filter(|f| !f.is_null()) {
            facts["arm"] = "full_overlay".into();
        }
    }
    for installation in original["hinge_installations"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        installation["inset_depth"] = 0.into();
    }
    original["stock_aliases"] = serde_json::json!({ id(4).to_string(): "S1" });
    original["next_stock_s_alias"] = 2.into();
    original["next_stock_o_alias"] = 1.into();
    original["materials"][0]["kind"] = "Plywood".into();
    original["materials"][0]["coating"] = "BothSides".into();
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&transferred).unwrap()).unwrap();
    assert_eq!(saved, original);
    save(&mut editor, &source).unwrap();
    assert_eq!(reopen(&source).project(), &expected);
    assert_ne!(std::fs::read(&source).unwrap(), LEGACY);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn legacy_missing_optional_fields_keep_original_defaults_without_dirtying() {
    let mut value: serde_json::Value = serde_json::from_slice(LEGACY).unwrap();
    for key in [
        "grid_spacing",
        "cutting_kerf",
        "confirmed_shop_kerf",
        "cut_fee",
        "hinge_installations",
        "door_joints",
        "export_records",
    ] {
        value.as_object_mut().unwrap().remove(key);
    }
    let bytes = serde_json::to_vec(&value).unwrap();
    let editor = prepare_bytes(&bytes).unwrap().into_editor();
    let p = editor.project();
    assert_eq!(p.schema_version, SCHEMA_VERSION);
    assert_eq!(
        p.grid_spacing,
        plan_my_cabinet::domain::DEFAULT_GRID_SPACING
    );
    assert_eq!(
        p.cutting_kerf,
        plan_my_cabinet::domain::DEFAULT_CUTTING_KERF
    );
    assert_eq!(p.confirmed_shop_kerf, None);
    assert_eq!(p.cut_fee, None);
    assert!(p.export_records.is_empty());
    assert!(!editor.is_dirty());
    assert_eq!(prepare_bytes(&serialize(p).unwrap()).unwrap().project(), p);
}

#[test]
fn legacy_invalid_and_future_documents_preserve_files_and_active_edits() {
    use plan_my_cabinet::persistence::{MAX_DOCUMENT_BYTES, PersistenceError};
    let mut active = ProjectEditor::new(fixture()).unwrap();
    active.set_grid_spacing(mm(20)).unwrap();
    active.begin_preview();
    let before = active.project().clone();
    let original: serde_json::Value = serde_json::from_slice(LEGACY).unwrap();
    let mut cases = Vec::new();
    for (pointer, bad) in [
        ("/boards/0/length", serde_json::json!(0)),
        ("/boards/0/id", serde_json::json!(id(2))),
        ("/boards/0/material_id", serde_json::json!(id(999))),
        (
            "/boards/0/pose/translation_mm/0",
            serde_json::json!(1000001.0),
        ),
        ("/boards/0/pose/rotation/w", serde_json::json!(2.0)),
        ("/assemblies/0/parent_id", serde_json::json!(id(8))),
        ("/stock/0/price/minor_units", serde_json::json!(-1)),
        ("/grid_spacing", serde_json::json!(0)),
        ("/cutting_kerf", serde_json::json!(0)),
        ("/confirmed_shop_kerf", serde_json::json!(5)),
        (
            "/catalog/0/verified_hinge/source_sha256",
            serde_json::json!("bad"),
        ),
        ("/export_records/0/file_sha256", serde_json::json!("bad")),
        ("/schema_version", serde_json::json!(SCHEMA_VERSION + 1)),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(pointer).unwrap() = bad;
        cases.push(serde_json::to_vec(&value).unwrap());
    }
    let text = std::str::from_utf8(LEGACY).unwrap();
    for text in [
        text.replacen(
            "\"schema_version\": 1",
            "\"schema_version\": 1, \"schema_version\": 2",
            1,
        ),
        text.replacen("\"length\": 100001", "\"length\": 100001, \"length\": 1", 1),
        text.replacen(
            "\"installation_dimensions\": {}",
            "\"installation_dimensions\": {\"cup\": 1, \"cup\": 2}",
            1,
        ),
    ] {
        assert!(matches!(
            prepare_bytes(text.as_bytes()),
            Err(PersistenceError::Json(_))
        ));
        cases.push(text.into_bytes());
    }
    let mut oversized = LEGACY.to_vec();
    oversized.resize(MAX_DOCUMENT_BYTES + 1, b' ');
    assert!(matches!(
        prepare_bytes(&oversized),
        Err(PersistenceError::TooLarge)
    ));
    cases.push(oversized);
    let path = std::env::temp_dir().join(format!("pmcab-invalid-v1-{}.pmcab", Uuid::new_v4()));
    for bytes in cases {
        std::fs::write(&path, &bytes).unwrap();
        assert!(prepare_reader(File::open(&path).unwrap()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(active.project(), &before);
        assert!(active.is_dirty());
        assert!(active.can_undo());
        assert!(active.preview().is_some());
    }
    std::fs::remove_file(path).unwrap();
}
