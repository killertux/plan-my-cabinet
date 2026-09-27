use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::{BoardGrain, Project, StockGrain, StockSource};
use plan_my_cabinet::export::{ExportMode, ExportSettings, prepare_export};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::pdf_export::render_pdf;
use plan_my_cabinet::stock_commands::StockInput;
use plan_my_cabinet::units::{Length, Pose, Quaternion, Unit};

fn mm(value: i64) -> Length {
    Length::from_micrometres(value * 1000)
}

#[test]
fn pdf_uses_stable_aliases_after_ownership_flip_and_priority_reorder() {
    let mut editor = ProjectEditor::new(Project::new("Alias packet", Currency::Usd)).unwrap();
    let material_id = editor
        .create_material(NewMaterial {
            name: "Birch".into(),
            thickness: mm(18),
            grain: BoardGrain::Unrestricted,
        })
        .unwrap();
    let mut input = StockInput {
        name: "Sheet".into(),
        material_id,
        length: mm(105),
        width: mm(50),
        thickness: mm(18),
        grain: StockGrain::Unknown,
        source: StockSource::ToPurchase,
        price: None,
        trim: [Length::ZERO; 4],
    };
    let purchased = editor.create_stock(input.clone(), 1).unwrap()[0];
    input.source = StockSource::Owned;
    input.name = "Offcut".into();
    let owned = editor.create_stock(input.clone(), 1).unwrap()[0];
    for name in ["Left", "Right"] {
        editor
            .create_board(NewBoard {
                name: name.into(),
                material_id,
                length: mm(100),
                width: mm(50),
                pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
    }
    assert_eq!(editor.project().allocations.len(), 2);
    input.source = StockSource::ToPurchase;
    editor.edit_stock(owned, input.clone()).unwrap();
    input.source = StockSource::Owned;
    input.name = "Renamed sheet".into();
    editor.edit_stock(purchased, input).unwrap();
    editor.reorder_stock(owned, 0).unwrap();
    assert_eq!(editor.project().ordered_stock()[0].id, owned);
    assert_eq!(editor.project().stock_alias(owned), Some("O1"));
    assert_eq!(editor.project().stock_alias(purchased), Some("S1"));

    let prepared = prepare_export(
        editor.project(),
        ExportSettings {
            language: Language::En,
            units: Unit::Mm,
        },
        ExportMode::Draft,
    )
    .unwrap();
    let bytes = render_pdf(&prepared).unwrap();
    let path = std::env::temp_dir().join(format!("stock-alias-{}.pdf", uuid::Uuid::new_v4()));
    std::fs::write(&path, bytes).unwrap();
    let output = std::process::Command::new("pdftotext")
        .args(["-layout", path.to_str().unwrap(), "-"])
        .output()
        .expect("pdftotext is required for PDF text verification");
    std::fs::remove_file(path).unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for (alias, id, name) in [("S1", purchased, "Renamed sheet"), ("O1", owned, "Offcut")] {
        assert!(text.contains(&format!("{alias} [{id}] · {name}")), "{text}");
        assert!(text.contains(&format!("{alias} [{id}]")), "{text}");
    }
    assert!(text.contains("Cost estimate incomplete: O1"), "{text}");
    assert!(
        text.contains("S1 ["),
        "part allocation should use the alias: {text}"
    );
    assert!(
        text.contains("O1 ["),
        "part allocation should use the alias: {text}"
    );
}
