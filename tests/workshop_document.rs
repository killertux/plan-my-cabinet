use plan_my_cabinet::document_layout::{A4_HEIGHT_MM, A4_WIDTH_MM, Document, MARGIN_MM, Primitive};
use plan_my_cabinet::domain::{
    Allocation, Board, BoardGrain, Hardware, HardwareKind, Material, Project, Stock, StockGrain,
    StockSource,
};
use plan_my_cabinet::export::{ExportMode, ExportSettings, ReceiptSections, prepare_export};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::money::{Currency, Money};
use plan_my_cabinet::units::{Length, Pose, Quaternion, Unit};
use plan_my_cabinet::workshop_document::build_workshop_document;
use uuid::Uuid;

fn mm(value: i64) -> Length {
    Length::from_micrometres(value * 1000)
}
fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
fn fixture(count: usize) -> Project {
    let mut project = Project::new("Armário da oficina — São João", Currency::Brl);
    project.id = id(1);
    project.revision = 42;
    project.materials = vec![
        Material {
            id: id(2),
            name: "Carvalho Ação".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Length,
        },
        Material {
            id: id(3),
            name: "Carvalho Ação".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Length,
        },
    ];
    project.stock.push(Stock {
        id: id(4),
        name: "Chapa sem preço".into(),
        material_id: id(2),
        length: mm(500),
        width: mm(200),
        thickness: mm(18),
        grain: StockGrain::Unknown,
        source: StockSource::ToPurchase,
        price: None,
        priority: 0,
        trim: [Length::ZERO; 4],
    });
    for n in 0..count {
        project.boards.push(Board {
            id: id(100 + n as u128),
            name: if n < 5 {
                "Prateleira São João".into()
            } else {
                format!("Peça número {n:03} — identificação longa de ação")
            },
            material_id: if n == 2 { id(3) } else { id(2) },
            length: if n == 3 { mm(110) } else { mm(100) },
            width: mm(50),
            thickness: mm(18),
            grain_override: (n == 4).then_some(BoardGrain::Width),
            parent_id: None,
            pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
        });
    }
    // These two retain separate identity and mapping even when summarized × 2.
    for n in 0..2 {
        project.allocations.push(Allocation {
            id: id(1000 + n),
            board_id: id(100 + n),
            stock_id: id(4),
            origin: [mm(n as i64 * 110), Length::ZERO],
            quarter_turn: false,
            locked: false,
        });
    }
    project.hardware.push(Hardware {
        id: id(5),
        name: "Dobradiça não verificada".into(),
        parent_id: None,
        pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
        kind: HardwareKind::Catalog { catalog_id: id(6) },
    });
    project
}

fn text(document: &Document) -> String {
    document
        .pages
        .iter()
        .flat_map(|p| &p.primitives)
        .filter_map(|primitive| match primitive {
            Primitive::Text(run) | Primitive::Notice(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn sections(on: bool) -> ReceiptSections {
    ReceiptSections {
        parts_and_costs: on,
        sheets_and_cut_steps: on,
        hinge_references: on,
    }
}

#[test]
fn grouping_keeps_material_dimensions_grain_and_every_allocation_identity() {
    for language in [Language::En, Language::PtBr] {
        let project = fixture(5);
        let prepared = prepare_export(
            &project,
            ExportSettings {
                language,
                units: Unit::Foot,
            },
            ExportMode::Draft,
        )
        .unwrap();
        let document = build_workshop_document(&prepared, sections(true)).unwrap();
        let all = text(&document);
        assert_eq!(all.matches("Prateleira São João × 2").count(), 1, "{all}");
        assert_eq!(all.matches("Prateleira São João × 1").count(), 3, "{all}");
        // Every board keeps a unique short part number; machine IDs never print.
        for n in 1..=5 {
            assert!(all.contains(&format!("#{n} ")), "missing part #{n}");
        }
        for n in 0..5 {
            assert!(!all.contains(&id(100 + n).to_string()), "board UUID {n} printed");
        }
        for n in 0..2 {
            assert!(!all.contains(&id(1000 + n).to_string()), "allocation UUID {n} printed");
        }
        assert!(all.contains("S1"));
        assert!(!all.contains(&id(4).to_string()));
        assert!(all.contains("ft"));
        assert!(all.contains(if language == Language::PtBr {
            "na largura"
        } else {
            "along width"
        }));
    }
}

#[test]
fn all_off_keeps_scope_hidden_board_price_fee_hardware_and_draft_safety() {
    for language in [Language::En, Language::PtBr] {
        let prepared = prepare_export(
            &fixture(8),
            ExportSettings {
                language,
                units: Unit::Mm,
            },
            ExportMode::Draft,
        )
        .unwrap();
        let document = build_workshop_document(&prepared, sections(false)).unwrap();
        let all = text(&document);
        // Only the project ID is printed (once, for traceability).
        assert!(all.contains(&id(1).to_string()));
        assert!(all.contains("S1"), "stock scope lost");
        assert!(!all.contains(&id(4).to_string()));
        assert!(
            all.contains("#8 ") && all.contains("Peça número 007"),
            "hidden/unallocated board lost"
        );
        assert!(all.contains("Dobradiça não verificada"), "hardware issue lost");
        assert!(!all.contains(&id(5).to_string()));
        for phrase in if language == Language::PtBr {
            [
                "RASCUNHO / NÃO USAR PARA CORTE",
                "preço",
                "Estimativa incompleta",
                "Detalhes opcionais omitidos",
            ]
        } else {
            [
                "DRAFT / NOT FOR CUTTING",
                "Cost estimate incomplete",
                "Estimate incomplete",
                "Optional detail omitted",
            ]
        } {
            assert!(
                all.to_lowercase().contains(&phrase.to_lowercase()),
                "missing {phrase}: {all}"
            );
        }
        assert!(!all.contains("Prateleira São João × 2"));
        assert!(!all.contains(if language == Language::PtBr {
            "Peça / quantidade"
        } else {
            "Part / quantity"
        }));
        assert!(all.contains(if language == Language::PtBr {
            "desconhecido"
        } else {
            "unknown"
        }));
    }
}

#[test]
fn dense_bilingual_document_preserves_every_label_with_bounded_geometry() {
    for language in [Language::En, Language::PtBr] {
        let prepared = prepare_export(
            &fixture(76),
            ExportSettings {
                language,
                units: Unit::Mm,
            },
            ExportMode::Draft,
        )
        .unwrap();
        let document = build_workshop_document(&prepared, sections(true)).unwrap();
        assert_eq!(
            document,
            build_workshop_document(&prepared, sections(true)).unwrap()
        );
        assert!(document.pages.len() >= 5);
        let all = text(&document);
        let first_page = document.pages[0]
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Text(t) | Primitive::Notice(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            first_page.contains("Armário da oficina — São João —"),
            "cover title moved past notices"
        );
        for n in 0..76 {
            assert!(all.contains(&format!("#{} ", n + 1)), "missing part number {n}");
            assert!(!all.contains(&id(100 + n).to_string()), "UUID {n} printed");
            if n >= 5 {
                assert!(
                    all.contains(&format!("Peça número {n:03}")),
                    "missing label {n}"
                );
            }
        }
        for (index, page) in document.pages.iter().enumerate() {
            assert_eq!(page.number, index + 1);
            let page_text = page
                .primitives
                .iter()
                .filter_map(|p| match p {
                    Primitive::Text(t) | Primitive::Notice(t) => Some(t.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            assert!(page_text.contains("Armário da oficina"));
            assert!(page_text.contains(if language == Language::PtBr {
                "RASCUNHO"
            } else {
                "DRAFT"
            }));
            assert!(page_text.contains(if language == Language::PtBr {
                "NÃO É GABARITO"
            } else {
                "NOT A CUTTING TEMPLATE"
            }));
            for primitive in &page.primitives {
                if let Primitive::Text(run) | Primitive::Notice(run) = primitive {
                    assert!(run.bounds.x >= MARGIN_MM && run.bounds.y >= 0.);
                    assert!(run.bounds.x + run.bounds.width <= A4_WIDTH_MM - MARGIN_MM + 0.001);
                    assert!(run.bounds.y + run.bounds.height < A4_HEIGHT_MM);
                    assert_eq!(
                        run.glyphs.iter().map(|g| g.character).collect::<String>(),
                        run.text
                    );
                }
            }
        }
    }
}

#[test]
fn verified_estimate_uses_prepared_cuts_and_known_zero_is_not_unknown() {
    let mut project = fixture(2);
    project.hardware.clear();
    project.stock[0].length = mm(205);
    project.stock[0].width = mm(50);
    project.stock[0].grain = StockGrain::AlongX;
    project.stock[0].price = Some(Money::new(Currency::Brl, 20_000).unwrap());
    project.cut_fee = Some(Money::new(Currency::Brl, 500).unwrap());
    project.confirmed_shop_kerf = Some(project.cutting_kerf);
    project.allocations[1].origin = [mm(105), Length::ZERO];
    let prepared = prepare_export(
        &project,
        ExportSettings {
            language: Language::En,
            units: Unit::Mm,
        },
        ExportMode::ShopReady,
    )
    .unwrap();
    assert_eq!(prepared.witnesses[0].1.cut_count(), 1);
    let all = text(&build_workshop_document(&prepared, sections(true)).unwrap());
    assert!(all.contains("BRL 200.00"));
    assert!(all.contains("BRL 5.00"));
    assert!(all.contains("BRL 205.00"));
    assert!(!all.contains("Estimate incomplete"));
    project.cut_fee = Some(Money::new(Currency::Brl, 0).unwrap());
    let zero = prepare_export(
        &project,
        ExportSettings {
            language: Language::En,
            units: Unit::Mm,
        },
        ExportMode::ShopReady,
    )
    .unwrap();
    assert!(text(&build_workshop_document(&zero, sections(true)).unwrap()).contains("BRL 200.00"));
}
