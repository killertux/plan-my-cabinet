use plan_my_cabinet::cut_tree::{Axis, CutKind, Edge};
use plan_my_cabinet::document_layout::{Document, Primitive};
use plan_my_cabinet::domain::{
    Allocation, Board, BoardEdge, BoardFace, BoardGrain, HingeInstallation, HingeMountingSide,
    Material, Project, Stock, StockGrain, StockSource,
};
use plan_my_cabinet::export::{ExportMode, ExportSettings, ReceiptSections, prepare_export};
use plan_my_cabinet::hardware_catalog;
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::units::{Length, Pose, Quaternion, Unit};
use plan_my_cabinet::workshop_document::build_workshop_document;
use uuid::Uuid;

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}
fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
fn text(page: &plan_my_cabinet::document_layout::Page) -> String {
    page.primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Text(t) | Primitive::Notice(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn all(document: &Document) -> String {
    document
        .pages
        .iter()
        .map(text)
        .collect::<Vec<_>>()
        .join(" ")
}
fn sections(diagrams: bool, hardware: bool) -> ReceiptSections {
    ReceiptSections {
        parts_and_costs: false,
        sheets_and_cut_steps: diagrams,
        hinge_references: hardware,
    }
}
fn stock_fixture(count: usize) -> Project {
    let mut p = Project::new("Oficina São João", Currency::Brl);
    p.materials.push(Material {
        id: id(1),
        name: "Madeira".into(),
        default_thickness: mm(18),
        default_grain: BoardGrain::Length,
    });
    p.stock.push(Stock {
        id: id(2),
        name: "Chapa de ação".into(),
        material_id: id(1),
        length: mm(count as i64 * 30 + (count as i64 - 1) * 5),
        width: mm(50),
        thickness: mm(18),
        grain: StockGrain::AlongX,
        source: StockSource::Owned,
        price: None,
        priority: 0,
        trim: [Length::ZERO; 4],
    });
    for n in 0..count {
        p.boards.push(Board {
            id: id(100 + n as u128),
            name: format!("Prateleira número {n:02} — ação"),
            material_id: id(1),
            length: mm(30),
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
        });
        p.allocations.push(Allocation {
            id: id(1000 + n as u128),
            board_id: id(100 + n as u128),
            stock_id: id(2),
            origin: [mm(n as i64 * 35), Length::ZERO],
            quarter_turn: false,
            locked: false,
        });
    }
    p.confirmed_shop_kerf = Some(p.cutting_kerf);
    p
}

#[test]
fn dense_bilingual_cut_callouts_and_continuations_retain_witness_identity() {
    for language in [Language::En, Language::PtBr] {
        let p = stock_fixture(28);
        let prepared = prepare_export(
            &p,
            ExportSettings {
                language,
                units: Unit::Mm,
            },
            ExportMode::Draft,
        )
        .unwrap();
        let tree = &prepared.witnesses[0].1;
        assert_eq!(tree.cut_count(), 27);
        let doc = build_workshop_document(&prepared, sections(true, false)).unwrap();
        let whole = all(&doc);
        let loc = Localizer::new(language);
        let sheet_pages: Vec<_> = doc
            .pages
            .iter()
            .filter(|page| text(page).contains(&loc.text("pdf-scale")))
            .collect();
        assert!(
            sheet_pages.len() >= 2,
            "expected continued cut instructions"
        );
        for page in &sheet_pages {
            let content = text(page);
            for key in ["pdf-scale", "pdf-kerf", "pdf-trim", "pdf-not-template"] {
                assert!(
                    content.contains(&loc.text(key)),
                    "{key} missing on continuation: {content}"
                );
            }
            assert!(
                content.contains(prepared.snapshot.project().stock_alias(id(2)).unwrap()),
                "{content}"
            );
        }
        assert!(whole.contains(&loc.text("pdf-blade-strip")));
        for (i, op) in tree.operations().iter().enumerate() {
            assert_eq!(op.number, i + 1);
            assert!(whole.contains(&format!("C{} P{}: ", op.number, op.input)));
            assert!(whole.contains(&format!(
                "P{} {} (P{}, P{})",
                op.retained_output,
                loc.text("pdf-retained"),
                op.outputs.first,
                op.outputs.second
            )));
            let axis = match op.axis {
                Axis::X => "X",
                Axis::Y => "Y",
            };
            let suffix = |edge| if edge == Edge::Low { "-" } else { "+" };
            assert!(whole.contains(&format!("{axis}{}", suffix(op.reference_edge))));
            assert!(whole.contains(&format!(
                "{} {axis}{}",
                loc.text("pdf-blade-strip"),
                suffix(op.kerf_side)
            )));
        }
        let bands: Vec<_> = doc
            .pages
            .iter()
            .flat_map(|page| &page.primitives)
            .filter_map(|primitive| match primitive {
                Primitive::Box {
                    bounds,
                    fill: Some(_),
                    ..
                } => Some(bounds),
                _ => None,
            })
            .collect();
        assert_eq!(
            bands.len(),
            27,
            "each witness blade strip must be represented once"
        );
        let scale = 166.0 / 975.0;
        let stock_outline = doc
            .pages
            .iter()
            .flat_map(|page| &page.primitives)
            .filter_map(|primitive| match primitive {
                Primitive::Box {
                    bounds, fill: None, ..
                } if (bounds.width - 166.0).abs() < 0.001 => Some(bounds),
                _ => None,
            })
            .next()
            .unwrap();
        for (band, op) in bands.iter().zip(tree.operations()) {
            let input = tree.node(op.input).unwrap().rectangle;
            let first = tree.node(op.outputs.first).unwrap().rectangle;
            let kerf_width = tree.kerf().micrometres() as f32 / 1000.0 * scale;
            let actual_span = match op.axis {
                Axis::X => band.width,
                Axis::Y => band.height,
            };
            let actual_position = match op.axis {
                Axis::X => band.x,
                Axis::Y => band.y,
            };
            let witness_position = match op.axis {
                Axis::X => first.origin[0].micrometres() + first.extent[0].micrometres(),
                Axis::Y => first.origin[1].micrometres() + first.extent[1].micrometres(),
            } as f32
                / 1000.0
                * scale;
            let diagram_origin = match op.axis {
                Axis::X => stock_outline.x,
                Axis::Y => stock_outline.y,
            };
            assert!((actual_span - kerf_width).abs() < 0.001);
            assert!((actual_position - diagram_origin - witness_position).abs() < 0.002);
            let other_span = match op.axis {
                Axis::X => band.height,
                Axis::Y => band.width,
            };
            let expected_other = match op.axis {
                Axis::X => input.extent[1],
                Axis::Y => input.extent[0],
            }
            .micrometres() as f32
                / 1000.0
                * scale;
            assert!((other_span - expected_other).abs() < 0.001);
        }
        for n in 0..28 {
            let node = tree
                .nodes()
                .iter()
                .enumerate()
                .find(|(_, node)| node.kind == CutKind::Part(id(100 + n)))
                .unwrap()
                .0;
            // Parts are keyed by short part numbers; machine identifiers never print.
            assert!(whole.contains(&format!(
                "Prateleira número {n:02} — ação (P{node})"
            )));
            assert!(!whole.contains(&id(100 + n).to_string()));
            assert!(!whole.contains(&id(1000 + n).to_string()));
        }
    }
}

#[test]
fn trims_are_counted_as_real_witness_cuts_and_toggles_remove_diagram_and_steps_together() {
    let mut p = stock_fixture(2);
    p.stock[0].length = mm(220);
    p.stock[0].width = mm(60);
    p.stock[0].trim = [mm(5), mm(5), mm(5), mm(5)];
    p.boards[0].width = mm(50);
    p.boards[1].width = mm(50);
    p.allocations[0].origin = [mm(5), mm(5)];
    p.allocations[1].origin = [mm(40), mm(5)];
    let prepared = prepare_export(
        &p,
        ExportSettings {
            language: Language::En,
            units: Unit::Mm,
        },
        ExportMode::Draft,
    )
    .unwrap();
    assert_eq!(prepared.witnesses.len(), 1, "fixture requires actual proof");
    let count = prepared.witnesses[0].1.cut_count();
    assert!(count >= 4, "trim passes must be in the witness");
    let on = build_workshop_document(&prepared, sections(true, false)).unwrap();
    let off = build_workshop_document(&prepared, sections(false, false)).unwrap();
    assert!(all(&on).contains("edge loss includes blade strip"));
    assert!(all(&on).contains("Trim loss"));
    assert!(all(&on).contains("C1 P0"));
    assert!(
        on.pages
            .iter()
            .flat_map(|page| &page.primitives)
            .any(|p| matches!(p, Primitive::Box { fill: Some(_), .. }))
    );
    assert!(!all(&off).contains("C1 P0"));
    assert!(
        !off.pages
            .iter()
            .flat_map(|page| &page.primitives)
            .any(|p| matches!(p, Primitive::Box { .. }))
    );
    assert!(all(&off).contains("sheet diagrams + cut steps"));
}

fn hinge_fixture() -> Project {
    let mut p = Project::new("Porta", Currency::Brl);
    p.materials.push(Material {
        id: id(10),
        name: "MDF".into(),
        default_thickness: mm(18),
        default_grain: BoardGrain::Unrestricted,
    });
    for (n, name) in [(11, "Door"), (12, "Mount")] {
        p.boards.push(Board {
            id: id(n),
            name: name.into(),
            material_id: id(10),
            length: mm(100),
            width: mm(100),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
        });
    }
    let catalog = hardware_catalog::builtin_hinge();
    let hinge = HingeInstallation {
        id: id(13),
        door_board_id: id(11),
        mounting_board_id: id(12),
        catalog_id: catalog.id,
        side: HingeMountingSide {
            door_edge: BoardEdge::MinX,
            door_face: BoardFace::MinZ,
            mount_front_edge: BoardEdge::MinX,
            mount_face: BoardFace::MaxZ,
        },
        door_y: mm(50),
        mount_y: mm(50),
        cup_edge_setback: mm(3),
        overlay: mm(15),
    };
    p.catalog.push(catalog);
    p.hinge_installations.push(hinge.clone());
    let joint =
        plan_my_cabinet::door_joint::preview(&p, id(14), id(11), id(12), vec![hinge.id]).unwrap();
    p.door_joints.push(joint.joint);
    p
}

#[test]
fn only_prepared_valid_hinge_guidance_emits_numeric_references() {
    for language in [Language::En, Language::PtBr] {
        let settings = ExportSettings {
            language,
            units: Unit::Mm,
        };
        let valid = prepare_export(&hinge_fixture(), settings, ExportMode::Draft).unwrap();
        assert_eq!(valid.installation_guidance.len(), 1);
        let valid_text = all(&build_workshop_document(&valid, sections(false, true)).unwrap());
        assert!(valid_text.contains("K=3 mm / R=15 mm"));
        assert!(
            valid_text.contains("20,5 mm / 50 mm / 0 mm")
                || valid_text.contains("20.5 mm / 50 mm / 0 mm")
        );
        assert!(valid_text.contains(&valid.installation_guidance[0].references.product_id));
        assert!(valid_text.contains(&Localizer::new(language).text("pdf-fasteners-unavailable")));
        let mut invalid = hinge_fixture();
        invalid.hinge_installations[0].door_y = mm(99);
        let withheld = prepare_export(&invalid, settings, ExportMode::Draft).unwrap();
        assert!(withheld.installation_guidance.is_empty());
        let invalid_text = all(&build_workshop_document(&withheld, sections(false, true)).unwrap());
        assert!(invalid_text.contains(&Localizer::new(language).text("pdf-install-cup-outside")));
        assert!(!invalid_text.contains("K=3"));
        assert!(!invalid_text.contains(&Localizer::new(language).text("pdf-cup-center")));
        let disabled = all(&build_workshop_document(&withheld, sections(false, false)).unwrap());
        assert!(disabled.contains(&Localizer::new(language).text("pdf-install-cup-outside")));
        assert!(!disabled.contains(&Localizer::new(language).text("pdf-hardware")));

        let mut unsupported = hinge_fixture();
        unsupported.hinge_installations[0].overlay = mm(99);
        let unsupported = prepare_export(&unsupported, settings, ExportMode::Draft).unwrap();
        let unsupported_text =
            all(&build_workshop_document(&unsupported, sections(false, true)).unwrap());
        assert!(unsupported_text.contains(&Localizer::new(language).text("pdf-install-overlay")));
        assert!(!unsupported_text.contains("K=3"));

        let mut missing = hinge_fixture();
        missing.catalog.clear();
        let missing = prepare_export(&missing, settings, ExportMode::Draft).unwrap();
        let missing_text = all(&build_workshop_document(&missing, sections(false, true)).unwrap());
        assert!(
            missing_text.contains(&Localizer::new(language).text("pdf-hardware-missing-catalog"))
        );
        assert!(!missing_text.contains("K=3"));
    }
}
