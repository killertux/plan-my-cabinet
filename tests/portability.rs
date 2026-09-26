//! Run locally with no environment variables, or exercise a transfer by setting
//! PMCAB_TRANSFER_DIR and PMCAB_TRANSFER_ROLE=write/read/verify in that order.
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::{
    Allocation, Board, BoardGrain, CatalogReference, Hardware, HardwareKind, Material, Project,
    Stock, StockGrain, StockSource,
};
use plan_my_cabinet::money::{Currency, Money};
use plan_my_cabinet::persistence::{prepare_reader, save};
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
        id: id(2),
        name: "Compensado".into(),
        default_thickness: mm(18),
        default_grain: BoardGrain::Length,
    });
    p.boards.push(Board {
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
    });
    p.hardware.push(Hardware {
        id: id(7),
        name: "Reference".into(),
        parent_id: None,
        pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        kind: HardwareKind::Catalog { catalog_id: id(6) },
    });
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
