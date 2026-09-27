use plan_my_cabinet::document_layout::{
    A4_HEIGHT_MM, A4_WIDTH_MM, Document, DocumentBuilder, FontMetrics, LayoutError, MARGIN_MM,
    PageContext, Point, Primitive, Rect, TextStyle,
};

fn context(language: &str) -> PageContext {
    let portuguese = language == "pt-BR";
    PageContext {
        project: if portuguese {
            "Armário de serviço — edição São João"
        } else {
            "Workshop cabinet — edition São João"
        }
        .into(),
        revision: "rev 42 · 9c91c1c0-73f1-4cdd-acb7-b4456d292f92".into(),
        packet: language.into(),
        stamp: Some(
            if portuguese {
                "RASCUNHO / NÃO USAR PARA CORTE"
            } else {
                "DRAFT / NOT FOR CUTTING"
            }
            .into(),
        ),
        safety: if portuguese {
            "Escala indicativa · Não é um gabarito de corte"
        } else {
            "Indicative scale · Not a cutting template"
        }
        .into(),
        notices: vec![
            if portuguese {
                "Aviso obrigatório: confira o corte e a direção do veio."
            } else {
                "Mandatory notice: verify the cut and grain direction."
            }
            .into(),
        ],
    }
}

fn runs(
    page: &plan_my_cabinet::document_layout::Page,
) -> impl Iterator<Item = &plan_my_cabinet::document_layout::TextRun> {
    page.primitives.iter().filter_map(|p| match p {
        Primitive::Text(run) | Primitive::Notice(run) => Some(run),
        _ => None,
    })
}

fn validate_geometry(doc: &Document) {
    assert!(!doc.pages.is_empty());
    for (index, page) in doc.pages.iter().enumerate() {
        assert_eq!(page.number, index + 1);
        assert_eq!((page.width_mm, page.height_mm), (A4_WIDTH_MM, A4_HEIGHT_MM));
        for run in runs(page) {
            let b = run.bounds;
            assert!(
                b.x >= MARGIN_MM && b.y >= 0.0 && b.x + b.width <= A4_WIDTH_MM - MARGIN_MM + 0.001,
                "{run:?}"
            );
            assert!(b.y + b.height <= A4_HEIGHT_MM - 3.0, "{run:?}");
            assert_eq!(
                run.text.chars().collect::<Vec<_>>(),
                run.glyphs.iter().map(|g| g.character).collect::<Vec<_>>()
            );
            for glyph in &run.glyphs {
                assert!(
                    glyph.baseline.x >= b.x - 0.001
                        && glyph.baseline.x + glyph.advance_mm <= b.x + b.width + 0.001,
                    "{glyph:?} in {run:?}"
                );
                assert!(glyph.baseline.y >= b.y && glyph.baseline.y <= b.y + b.height + 0.001);
            }
        }
    }
}

fn dense_packet(language: &str) -> Document {
    let mut builder = DocumentBuilder::new(context(language)).unwrap();
    builder
        .section(if language == "pt-BR" {
            "Chapas e peças"
        } else {
            "Sheets and parts"
        })
        .unwrap();
    builder
        .paragraph("Long names and exact source identifiers are never elided.")
        .unwrap();
    builder.diagram(45.0).unwrap();
    let headings = vec![
        "Part / Peça".into(),
        "Identity / Identificação".into(),
        "Stock / Chapa".into(),
    ];
    let rows: Vec<_> = (0..88)
        .map(|index| {
            vec![
                format!(
                    "{} {index}: prateleira de ação — seção de São José com identificação longa",
                    if language == "pt-BR" { "Peça" } else { "Part" }
                ),
                format!("ID-{index:03}-cf519f10-5a52-4b2e-a43e-80943de4792b"),
                format!("S{index} · 764 × 537 × 18 mm"),
            ]
        })
        .collect();
    builder.table(&headings, &rows).unwrap();
    builder
        .section("Cut sequence / Sequência de cortes")
        .unwrap();
    for index in 0..85 {
        builder
            .paragraph(&format!(
                "C{index}: X+ — faixa da lâmina, referência preservada."
            ))
            .unwrap();
    }
    builder.finish()
}

#[test]
fn dense_bilingual_pages_repeat_context_and_do_not_clip_or_omit_labels() {
    for language in ["en", "pt-BR"] {
        let first = dense_packet(language);
        assert_eq!(first.pages.len(), 7, "unexpected {language} pagination");
        assert_eq!(
            first,
            dense_packet(language),
            "non-deterministic {language} pagination"
        );
        assert!(
            first.pages.len() > 3,
            "must exercise row and cut continuation"
        );
        validate_geometry(&first);
        let ctx = context(language);
        for page in &first.pages {
            let text = runs(page)
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                text.contains(&ctx.project),
                "missing repeated project context on page {}",
                page.number
            );
            assert!(text.contains(ctx.stamp.as_ref().unwrap()));
            assert!(text.contains(&ctx.safety));
        }
        assert!(first.pages[0].primitives.iter().any(|p| match p {
            Primitive::Notice(run) =>
                run.text.contains("obrigatório") || run.text.contains("Mandatory"),
            _ => false,
        }));
        let all = first
            .pages
            .iter()
            .flat_map(runs)
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for index in 0..88 {
            assert!(
                all.contains(&format!("ID-{index:03}-")),
                "missing identity {index}"
            );
        }
        for index in 0..85 {
            assert!(all.contains(&format!("C{index}:")), "missing cut {index}");
        }
        let continued = first
            .pages
            .iter()
            .skip(1)
            .any(|page| runs(page).any(|r| r.text == "Part / Peça"));
        assert!(continued, "table headings not repeated at page break");
    }
}

#[test]
fn shaped_glyph_advance_and_long_words_use_actual_embedded_face() {
    let metrics = FontMetrics::new();
    let regular = metrics
        .width_mm("Ação — prateleira", TextStyle::BODY)
        .unwrap();
    let mono = metrics
        .width_mm("Ação — prateleira", TextStyle::MONO)
        .unwrap();
    assert!(regular > 0.0 && mono > 0.0 && regular != mono);
    let long = "ÁrvoreSãoJoséçãodePeças1234567890".repeat(3);
    let lines = metrics.wrap(&long, TextStyle::BODY, 21.0).unwrap();
    assert!(lines.len() > 3);
    assert_eq!(lines.concat(), long);
    assert!(
        lines
            .iter()
            .all(|line| metrics.width_mm(line, TextStyle::BODY).unwrap() <= 21.001)
    );
    let mut builder = DocumentBuilder::new(context("pt-BR")).unwrap();
    builder.paragraph(&long).unwrap();
    validate_geometry(&builder.finish());
}

#[test]
fn impossible_geometry_is_an_error_instead_of_clipped_content() {
    let metrics = FontMetrics::new();
    assert_eq!(
        metrics.wrap("name", TextStyle::BODY, 0.0),
        Err(LayoutError::InvalidGeometry)
    );
    let mut builder = DocumentBuilder::new(context("en")).unwrap();
    assert_eq!(builder.diagram(300.0), Err(LayoutError::BlockTooTall));
    let mut ctx = context("en");
    ctx.safety = "Safety disclosure ".repeat(1000);
    assert!(matches!(
        DocumentBuilder::new(ctx),
        Err(LayoutError::BlockTooTall)
    ));
}

#[test]
fn diagram_annotations_and_links_use_page_coordinates() {
    let mut builder = DocumentBuilder::new(context("en")).unwrap();
    let box_on_page = builder.diagram(52.0).unwrap();
    builder
        .path(
            vec![
                Point {
                    x: box_on_page.x + 2.0,
                    y: box_on_page.y + 2.0,
                },
                Point {
                    x: box_on_page.x + 25.0,
                    y: box_on_page.y + 2.0,
                },
            ],
            false,
        )
        .unwrap();
    builder
        .label_at(
            "C1 · Ø35",
            TextStyle::MONO,
            box_on_page.x + 2.0,
            box_on_page.y + 5.0,
            30.0,
        )
        .unwrap();
    builder
        .link(
            Rect {
                x: box_on_page.x,
                y: box_on_page.y,
                width: 30.0,
                height: 10.0,
            },
            "part:123".into(),
        )
        .unwrap();
    let doc = builder.finish();
    validate_geometry(&doc);
    assert!(
        doc.pages[0]
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Path { .. }))
    );
    assert!(
        doc.pages[0]
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Link { .. }))
    );
}
