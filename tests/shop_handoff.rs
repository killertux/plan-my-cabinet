//! Cabinet-like assembly through public editing/allocation/export APIs, with historical PDF bytes.
use std::{fs, process::Command};

use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
use plan_my_cabinet::board_dimensions::BoardDimension;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::{BoardGrain, Project, StockGrain, StockSource};
use plan_my_cabinet::export::{
    ExportMode, ExportSettings, ExportStatus, OutputError, Overwrite, prepare_export, write_pdf,
};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::money::{Currency, Money};
use plan_my_cabinet::stock_commands::StockInput;
use plan_my_cabinet::units::{Anchor, Length, Pose, Quaternion, Unit};
use uuid::Uuid;

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}

#[test]
fn cabinet_handoff_preserves_sent_packet_then_regenerates_current_revision() {
    let dir = std::env::temp_dir().join(format!("pmcab-handoff-{}", Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let old_path = dir.join("cabinet-revision-1.pdf");
    let new_path = dir.join("cabinet-revision-2.pdf");
    let mut editor = ProjectEditor::new(Project::new("Cabinet body", Currency::Brl)).unwrap();
    let material = editor
        .create_material(NewMaterial {
            name: "18 mm plywood".into(),
            thickness: mm(18),
            grain: BoardGrain::Length,
        })
        .unwrap();
    editor.set_cutting_kerf(mm(5)).unwrap();
    editor.confirm_shop_kerf().unwrap();
    editor
        .set_cut_fee(Some(Money::new(Currency::Brl, 500).unwrap()))
        .unwrap();
    for (name, length, price) in [("Side sheet", 2300, 20_000), ("Shelf sheet", 784, 10_000)] {
        editor
            .create_stock(
                StockInput {
                    name: name.into(),
                    material_id: material,
                    length: mm(length),
                    width: mm(1205),
                    thickness: mm(18),
                    grain: StockGrain::AlongX,
                    source: StockSource::ToPurchase,
                    price: Some(Money::new(Currency::Brl, price).unwrap()),
                    trim: [Length::ZERO; 4],
                },
                1,
            )
            .unwrap();
    }
    let mut ids = Vec::new();
    for (name, length, z) in [
        ("Left side", 2300, 0.),
        ("Right side", 2300, 0.),
        ("Shelf", 784, 1100.),
        ("Shelf", 784, 1500.),
    ] {
        let (id, _) = editor
            .create_board_with_fit(NewBoard {
                name: name.into(),
                material_id: material,
                length: mm(length),
                width: mm(600),
                pose: Pose::new([0., 0., z], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        ids.push(id);
    }
    editor
        .group_objects(&ids, None, "Cabinet body assembly", [0.; 3])
        .unwrap();
    assert_eq!(editor.project().allocations.len(), 4);
    let settings = ExportSettings {
        language: Language::PtBr, // independent of the caller's English workflow
        units: Unit::Cm,
    };
    let prepared = prepare_export(editor.project(), settings, ExportMode::ShopReady).unwrap();
    assert_eq!(prepared.witnesses.len(), 2);
    assert_eq!(
        prepared
            .witnesses
            .iter()
            .map(|(_, t)| t.cut_count())
            .sum::<usize>(),
        2
    );
    assert_eq!(editor.last_export_status(), ExportStatus::NeverExported);
    assert!(matches!(
        write_pdf(&prepared, None, Overwrite::Decline),
        Err(OutputError::Cancelled)
    ));
    assert_eq!(editor.project().export_records.len(), 0);
    assert!(matches!(
        write_pdf(
            &prepared,
            Some(&dir.join("missing/packet.pdf")),
            Overwrite::Decline
        ),
        Err(OutputError::Write(_))
    ));
    assert_eq!(editor.project().export_records.len(), 0);
    let first = write_pdf(&prepared, Some(&old_path), Overwrite::Decline).unwrap();
    editor
        .record_completed_export(
            &prepared.snapshot,
            &old_path,
            first.completed_unix_ms,
            &first.file_sha256,
        )
        .unwrap();
    let old_bytes = fs::read(&old_path).unwrap();
    assert!(old_bytes.starts_with(b"%PDF-"));
    assert_eq!(editor.last_export_status(), ExportStatus::Current);
    assert_eq!(first.revision, prepared.snapshot.revision());
    assert_eq!(first.settings, settings);
    assert_eq!(first.wood_sha256, prepared.snapshot.fingerprint().wood);
    assert!(matches!(
        write_pdf(&prepared, Some(&old_path), Overwrite::Decline),
        Err(OutputError::OverwriteRequired)
    ));
    assert_eq!(fs::read(&old_path).unwrap(), old_bytes);
    assert_eq!(editor.project().export_records.len(), 1);

    // A label alone stales the packet; a dimension edit also changes the wood fingerprint.
    editor
        .transact(|p| -> Result<(), ()> {
            p.boards.iter_mut().find(|b| b.id == ids[2]).unwrap().name = "Upper shelf".into();
            Ok(())
        })
        .unwrap();
    assert_eq!(editor.last_export_status(), ExportStatus::WoodStale);
    let resize = editor
        .preview_board_dimension(ids[2], BoardDimension::Width, mm(590), Anchor::Start)
        .unwrap();
    editor.edit_board_dimension(resize).unwrap();
    assert_eq!(editor.last_export_status(), ExportStatus::WoodStale);
    assert_eq!(fs::read(&old_path).unwrap(), old_bytes);
    // A draft remains available during review of the changed dimension.
    let draft = prepare_export(editor.project(), settings, ExportMode::Draft).unwrap();
    assert_eq!(draft.mode, ExportMode::Draft);
    // Restore the original dimension while retaining the new label.
    let resize = editor
        .preview_board_dimension(ids[2], BoardDimension::Width, mm(600), Anchor::Start)
        .unwrap();
    editor.edit_board_dimension(resize).unwrap();
    let current = prepare_export(editor.project(), settings, ExportMode::ShopReady).unwrap();
    assert_ne!(current.snapshot.fingerprint().wood, first.wood_sha256);
    let second = write_pdf(&current, Some(&new_path), Overwrite::Decline).unwrap();
    editor
        .record_completed_export(
            &current.snapshot,
            &new_path,
            second.completed_unix_ms,
            &second.file_sha256,
        )
        .unwrap();
    assert_eq!(editor.last_export_status(), ExportStatus::Current);
    assert!(second.revision > first.revision);
    assert_ne!(second.file_sha256, first.file_sha256);
    assert_eq!(fs::read(&old_path).unwrap(), old_bytes);
    assert_eq!(editor.project().export_records.len(), 2);

    if let Ok(output) = Command::new("pdftotext")
        .args(["-layout", new_path.to_str().unwrap(), "-"])
        .output()
    {
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for expected in [
            "Cabinet body",
            "Upper shelf",
            "230 cm",
            "78,4 cm",
            "60 cm",
            "0,5 cm",
            "C1 P0",
            "Y- 60 cm",
            "Y+ da peça mantida",
            "Cortes físicos: 2",
            "BRL 310,00",
            "NÃO É GABARITO",
        ] {
            assert!(compact.contains(expected), "missing {expected}: {compact}");
        }
        for id in ids {
            assert!(
                compact.contains(&id.to_string()),
                "missing part identity {id}"
            );
        }
    } else {
        // Poppler is optional on CI hosts; the PDF is still validated by the renderer tests.
        assert!(fs::read(&new_path).unwrap().starts_with(b"%PDF-"));
    }
    fs::remove_dir_all(dir).unwrap();
}
