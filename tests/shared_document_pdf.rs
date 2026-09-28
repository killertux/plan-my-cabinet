use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use plan_my_cabinet::document_layout::{
    Document, DocumentBuilder, Face, PageContext, Point, Primitive, Stroke, TextStyle,
};
use plan_my_cabinet::domain::{
    Allocation, Board, BoardGrain, Material, Project, Stock, StockGrain, StockSource,
};
use plan_my_cabinet::export::{ExportMode, ExportSettings, ReceiptSections, prepare_export};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::pdf_export::{
    DocumentPreviewLabels, DocumentPreviewState, paint_document_page, render_document_pdf,
    show_document_preview,
};
use plan_my_cabinet::units::{Length, Pose, Quaternion, Unit};
use plan_my_cabinet::workshop_document::build_workshop_document;
use printpdf::{Op, PdfDocument, PdfParseOptions, TextItem};
use uuid::Uuid;

const PT_TO_MM: f32 = 25.4 / 72.0;

fn parse(document: &Document) -> PdfDocument {
    let bytes = render_document_pdf(document).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    PdfDocument::parse(&bytes, &PdfParseOptions::default(), &mut Vec::new()).unwrap()
}

fn positioned_runs(document: &Document) -> Vec<Vec<(char, Point, Face)>> {
    document
        .pages
        .iter()
        .map(|page| {
            page.primitives
                .iter()
                .filter_map(|p| match p {
                    Primitive::Text(run) | Primitive::Notice(run) => Some(run),
                    _ => None,
                })
                .flat_map(|run| {
                    run.glyphs
                        .iter()
                        .map(|g| (g.character, g.baseline, run.style.face))
                })
                .collect()
        })
        .collect()
}

fn assert_pdf_geometry(document: &Document, pdf: &PdfDocument) {
    assert_eq!(document.pages.len(), pdf.pages.len());
    for (page, serialized) in document.pages.iter().zip(&pdf.pages) {
        // printpdf rounds the media box to whole points on serialization.
        assert!((page.width_mm - serialized.media_box.width.0 * PT_TO_MM).abs() < 0.2);
        assert!((page.height_mm - serialized.media_box.height.0 * PT_TO_MM).abs() < 0.2);
        let expected = positioned_runs(&Document {
            pages: vec![page.clone()],
            layout_version: document.layout_version,
            font_metrics_version: document.font_metrics_version,
        })
        .remove(0);
        let actual = serialized
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::SetTextCursor { pos } => Some(pos),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(expected.len(), actual.len(), "page {}", page.number);
        for ((_, baseline, _), pos) in expected.iter().zip(actual) {
            assert!((pos.x.0 * PT_TO_MM - baseline.x).abs() < 0.003);
            assert!((page.height_mm - pos.y.0 * PT_TO_MM - baseline.y).abs() < 0.003);
        }
        assert_eq!(
            serialized
                .ops
                .iter()
                .filter(|op| matches!(op, Op::WriteCodepoints { .. } | Op::WriteText { .. }))
                .count(),
            expected.len()
        );
        let extracted = serialized
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::WriteText { items, .. } => Some(items),
                _ => None,
            })
            .flat_map(|items| items.iter())
            .filter_map(|item| match item {
                TextItem::Text(s) => Some(s.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(
            extracted,
            expected.iter().map(|(ch, _, _)| *ch).collect::<String>()
        );
        let source_paths = page
            .primitives
            .iter()
            .filter(|p| matches!(p, Primitive::Path { .. }))
            .count();
        assert_eq!(
            serialized
                .ops
                .iter()
                .filter(|op| matches!(op, Op::DrawLine { .. }))
                .count(),
            source_paths
        );
        let actual_lines = serialized.ops.iter().filter_map(|op| match op {
            Op::DrawLine { line } => Some(line),
            _ => None,
        });
        let source_lines = page.primitives.iter().filter_map(|p| match p {
            Primitive::Path { points, closed, .. } => Some((points, closed)),
            _ => None,
        });
        for (line, (points, closed)) in actual_lines.zip(source_lines) {
            assert_eq!(line.is_closed, *closed);
            assert_eq!(line.points.len(), points.len());
            for (actual, source) in line.points.iter().zip(points) {
                assert!((actual.p.x.0 * PT_TO_MM - source.x).abs() < 0.003);
                assert!((page.height_mm - actual.p.y.0 * PT_TO_MM - source.y).abs() < 0.003);
            }
        }
        assert_eq!(
            serialized
                .ops
                .iter()
                .filter(|op| matches!(op, Op::DrawPolygon { .. }))
                .count(),
            page.primitives
                .iter()
                .filter(|p| matches!(
                    p,
                    Primitive::Box {
                        stroke: Some(_),
                        ..
                    } | Primitive::Box { fill: Some(_), .. }
                ))
                .count()
        );
        let source_boxes = page.primitives.iter().filter_map(|p| match p {
            Primitive::Box {
                bounds,
                stroke,
                fill,
            } if stroke.is_some() || fill.is_some() => Some(bounds),
            _ => None,
        });
        let drawn_boxes = serialized.ops.iter().filter_map(|op| match op {
            Op::DrawPolygon { polygon } => Some(polygon),
            _ => None,
        });
        for (polygon, bounds) in drawn_boxes.zip(source_boxes) {
            let points = &polygon.rings[0].points;
            assert!(points.len() >= 4);
            for (actual, (x, y)) in points.iter().take(4).zip([
                (bounds.x, bounds.y),
                (bounds.x + bounds.width, bounds.y),
                (bounds.x + bounds.width, bounds.y + bounds.height),
                (bounds.x, bounds.y + bounds.height),
            ]) {
                assert!((actual.p.x.0 * PT_TO_MM - x).abs() < 0.003);
                assert!((page.height_mm - actual.p.y.0 * PT_TO_MM - y).abs() < 0.003);
            }
        }
    }
}

fn dense_document(language: Language) -> Document {
    let mut project = Project::new("Oficina São João", Currency::Brl);
    let material_id = Uuid::from_u128(1);
    let stock_id = Uuid::from_u128(2);
    project.materials.push(Material {
        id: material_id,
        name: "Madeira de ação".into(),
        default_thickness: Length::from_micrometres(18_000),
        default_grain: BoardGrain::Length,
    });
    let mm = |n: i64| Length::from_micrometres(n * 1000);
    project.stock.push(Stock {
        id: stock_id,
        name: "Chapa útil".into(),
        material_id,
        length: mm(975),
        width: mm(50),
        thickness: mm(18),
        grain: StockGrain::AlongX,
        source: StockSource::Owned,
        price: None,
        priority: 0,
        trim: [Length::ZERO; 4],
    });
    for n in 0..28_u128 {
        let board_id = Uuid::from_u128(100 + n);
        project.boards.push(Board {
            id: board_id,
            name: format!("Prateleira número {n:02} — ação"),
            material_id,
            length: mm(30),
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        });
        project.allocations.push(Allocation {
            id: Uuid::from_u128(1000 + n),
            board_id,
            stock_id,
            origin: [mm(n as i64 * 35), Length::ZERO],
            quarter_turn: false,
            locked: false,
        });
    }
    project.confirmed_shop_kerf = Some(project.cutting_kerf);
    let prepared = prepare_export(
        &project,
        ExportSettings {
            language,
            units: Unit::Mm,
        },
        ExportMode::Draft,
    )
    .unwrap();
    assert_eq!(prepared.witnesses[0].1.cut_count(), 27);
    build_workshop_document(&prepared, ReceiptSections::default()).unwrap()
}

#[test]
fn bilingual_dense_packet_serializes_exact_pages_glyphs_and_diagrams() {
    for language in [Language::En, Language::PtBr] {
        let document = dense_document(language);
        assert!(document.pages.len() > 2);
        let pdf = parse(&document);
        assert_pdf_geometry(&document, &pdf);
        assert!(pdf.resources.fonts.map.len() >= 2);
        assert!(
            pdf.resources
                .fonts
                .map
                .values()
                .all(|font| !font.original_bytes.is_empty())
        );
        let source = positioned_runs(&document);
        for page in &source {
            assert!(!page.is_empty());
        }
        let all = document
            .pages
            .iter()
            .flat_map(|page| &page.primitives)
            .collect::<Vec<_>>();
        assert_eq!(
            all.iter()
                .filter(|p| matches!(
                    p,
                    Primitive::Box { fill: Some(fill), .. }
                        if *fill == plan_my_cabinet::workshop_document::KERF_BAND
                ))
                .count(),
            27
        );
        assert!(all.iter().any(|p| matches!(p, Primitive::Path { .. })));
        assert!(all.iter().any(|p| match p {
            Primitive::Text(run) | Primitive::Notice(run) => run.text.contains("C27"),
            _ => false,
        }));
    }
}

#[test]
fn dense_ligatures_have_distinct_scalar_origins_in_both_renderers() {
    let ctx = egui::Context::default();
    test_fonts(&ctx);
    for language in [Language::En, Language::PtBr] {
        let document = dense_document(language);
        let pdf = parse(&document);
        assert!(document.pages.len() > 2, "{language:?} dense pagination");
        assert_eq!(document.pages.len(), pdf.pages.len());
        let mut ligatures = 0;
        for (page, serialized) in document.pages.iter().zip(&pdf.pages) {
            let mut pdf_cursors = serialized.ops.iter().filter_map(|op| match op {
                Op::SetTextCursor { pos } => Some(pos),
                _ => None,
            });
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                paint_document_page(ui.painter(), page, egui::Pos2::ZERO, 2.0).unwrap();
            });
            output.textures_delta.clear();
            let mut preview = output.shapes.iter().filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => text.galley.rows.first().and_then(|row| {
                    row.glyphs
                        .first()
                        .map(|glyph| text.pos + row.pos.to_vec2() + glyph.pos.to_vec2())
                }),
                _ => None,
            });
            for run in page.primitives.iter().filter_map(|p| match p {
                Primitive::Text(run) | Primitive::Notice(run) => Some(run),
                _ => None,
            }) {
                // Reassigning continuation origins must not change the full
                // shaper's measured width (and therefore wrapping/pagination).
                // The run's right edge stays frozen even when a multi-scalar
                // ligature is divided into separately rendered glyphs.
                if let Some(last) = run.glyphs.last() {
                    assert!(
                        (last.baseline.x + last.advance_mm - run.bounds.x - run.bounds.width).abs()
                            < 0.003
                    );
                }
                for glyph in &run.glyphs {
                    assert!(
                        glyph.advance_mm > 0.0,
                        "{language:?} page {}: {run:?}",
                        page.number
                    );
                    let cursor = pdf_cursors.next().unwrap();
                    let painted = preview.next().unwrap();
                    assert!((cursor.x.0 * PT_TO_MM - glyph.baseline.x).abs() < 0.003);
                    assert!((painted.x / 2.0 - glyph.baseline.x).abs() < 0.02);
                }
                for window in run.glyphs.windows(3) {
                    if window.iter().map(|g| g.character).collect::<String>() == "fic" {
                        ligatures += 1;
                        assert!(window[0].baseline.x < window[1].baseline.x);
                        assert!(window[1].baseline.x < window[2].baseline.x);
                        assert!(
                            (window[0].baseline.x + window[0].advance_mm - window[1].baseline.x)
                                .abs()
                                < 0.003
                        );
                    }
                }
            }
            assert!(pdf_cursors.next().is_none());
            assert!(preview.next().is_none());
        }
        assert!(
            ligatures > 0,
            "{language:?} fixture must exercise fi ligatures"
        );
    }
}

fn test_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (face, name, family) in [
        (Face::Sans, "sans", FontFamily::Proportional),
        (
            Face::SansMedium,
            "medium",
            FontFamily::Name("noto-medium".into()),
        ),
        (
            Face::SansSemibold,
            "semibold",
            FontFamily::Name("noto-semibold".into()),
        ),
        (Face::Mono, "mono", FontFamily::Monospace),
        (
            Face::MonoMedium,
            "mono-medium",
            FontFamily::Name("jetbrains-medium".into()),
        ),
        (
            Face::MonoSemibold,
            "mono-semibold",
            FontFamily::Name("jetbrains-semibold".into()),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), FontData::from_static(face.bytes()).into());
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.into());
    }
    ctx.set_fonts(fonts);
}

#[test]
fn preview_paints_frozen_shapes_at_two_zooms_without_changing_pagination() {
    let mut builder = DocumentBuilder::new(PageContext {
        project: "Ação".into(),
        revision: "revision 3".into(),
        packet: "Draft".into(),
        stamp: Some("DRAFT".into()),
        safety: "Not a cutting template".into(),
        notices: vec!["Safety first".into()],
    })
    .unwrap();
    builder.section("Chapa — cut sequence").unwrap();
    let frame = builder.diagram(50.0).unwrap();
    builder
        .path_styled(
            vec![
                Point {
                    x: frame.x + 2.0,
                    y: frame.y + 2.0,
                },
                Point {
                    x: frame.x + 12.0,
                    y: frame.y + 20.0,
                },
            ],
            false,
            Stroke::STANDARD,
        )
        .unwrap();
    builder
        .label_at(
            "C1 · Ø35",
            TextStyle::MONO,
            frame.x + 3.0,
            frame.y + 5.0,
            35.0,
        )
        .unwrap();
    let document = builder.finish();
    let expected = document.clone();
    let pdf = parse(&document);
    assert_pdf_geometry(&document, &pdf);
    let ctx = egui::Context::default();
    test_fonts(&ctx);
    for zoom in [2.0_f32, 5.0] {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            paint_document_page(
                ui.painter(),
                &document.pages[0],
                egui::pos2(10.0, 20.0),
                zoom,
            )
            .unwrap();
        });
        assert!(!output.shapes.is_empty());
        assert!(
            output
                .shapes
                .iter()
                .any(|s| matches!(s.shape, egui::Shape::Path(_)))
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|s| matches!(s.shape, egui::Shape::Text(_)))
        );
        let native_glyphs = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => text.galley.rows.first().and_then(|row| {
                    row.glyphs
                        .first()
                        .map(|glyph| text.pos + row.pos.to_vec2() + glyph.pos.to_vec2())
                }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let source_glyphs = positioned_runs(&document);
        assert_eq!(native_glyphs.len(), source_glyphs[0].len());
        for (native, (_, source, _)) in native_glyphs.iter().zip(&source_glyphs[0]) {
            assert!((native.x - (10.0 + source.x * zoom)).abs() < 0.01);
            assert!((native.y - (20.0 + source.y * zoom)).abs() < 0.01);
        }
        assert_eq!(document, expected);
        assert_eq!(pdf.pages.len(), document.pages.len());
        output.textures_delta.clear();
    }
}

#[test]
fn every_dense_page_paints_the_pdf_source_glyphs_at_both_zooms() {
    let ctx = egui::Context::default();
    test_fonts(&ctx);
    for language in [Language::En, Language::PtBr] {
        let document = dense_document(language);
        let pdf = parse(&document);
        assert_pdf_geometry(&document, &pdf);
        let original = document.clone();
        let glyph_pages = positioned_runs(&document);
        for zoom in [0.8_f32, 3.0] {
            for (index, page) in document.pages.iter().enumerate() {
                let origin = egui::pos2(18.0, 21.0);
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    paint_document_page(ui.painter(), page, origin, zoom).unwrap();
                });
                let glyphs = output.shapes.iter().filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => text.galley.rows.first().and_then(|row| {
                        row.glyphs
                            .first()
                            .map(|glyph| text.pos + row.pos.to_vec2() + glyph.pos.to_vec2())
                    }),
                    _ => None,
                });
                let positions = glyphs.collect::<Vec<_>>();
                assert_eq!(positions.len(), glyph_pages[index].len(), "page {index}");
                for (actual, (_, source, _)) in positions.iter().zip(&glyph_pages[index]) {
                    assert!((actual.x - (origin.x + source.x * zoom)).abs() < 0.02);
                    assert!((actual.y - (origin.y + source.y * zoom)).abs() < 0.02);
                }
                // Each witness path and filled kerf band is emitted at either zoom.
                assert_eq!(
                    output
                        .shapes
                        .iter()
                        .filter(|shape| matches!(shape.shape, egui::Shape::Path(_)))
                        .count(),
                    page.primitives
                        .iter()
                        .filter(|p| matches!(p, Primitive::Path { .. }))
                        .count()
                );
                let paths = output.shapes.iter().filter_map(|shape| match &shape.shape {
                    egui::Shape::Path(path) => Some(path),
                    _ => None,
                });
                let sources = page.primitives.iter().filter_map(|p| match p {
                    Primitive::Path { points, closed, .. } => Some((points, closed)),
                    _ => None,
                });
                for (painted, (points, closed)) in paths.zip(sources) {
                    assert_eq!(painted.points.len(), points.len() + usize::from(*closed));
                    for (actual, point) in painted.points.iter().zip(points) {
                        assert!((actual.x - (origin.x + point.x * zoom)).abs() < 0.02);
                        assert!((actual.y - (origin.y + point.y * zoom)).abs() < 0.02);
                    }
                }
                for bounds in page.primitives.iter().filter_map(|p| match p {
                    Primitive::Box {
                        bounds,
                        fill: Some(_),
                        ..
                    } => Some(bounds),
                    _ => None,
                }) {
                    assert!(output.shapes.iter().any(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) => {
                            (rect.rect.min.x - (origin.x + bounds.x * zoom)).abs() < 0.02
                                && (rect.rect.min.y - (origin.y + bounds.y * zoom)).abs() < 0.02
                                && (rect.rect.width() - bounds.width * zoom).abs() < 0.02
                                && (rect.rect.height() - bounds.height * zoom).abs() < 0.02
                        }
                        _ => false,
                    }));
                }
                output.textures_delta.clear();
            }
            assert_eq!(document, original);
            assert_eq!(pdf.pages.len(), document.pages.len());
        }
    }
}

#[test]
fn native_preview_uses_the_frozen_page_set_and_clamps_selection_on_refresh() {
    let document = dense_document(Language::PtBr);
    let expected = document.clone();
    let pdf = parse(&document);
    let ctx = egui::Context::default();
    test_fonts(&ctx);
    let labels = DocumentPreviewLabels {
        title: "Preview",
        pages: "pages",
        page: "Page",
        zoom_in: "Zoom in",
        zoom_out: "Zoom out",
        fit: "Fit page",
    };
    let mut state = DocumentPreviewState::default();
    state.select(usize::MAX, &document);
    assert_eq!(state.page + 1, pdf.pages.len());
    for zoom in [1.0, 4.0] {
        state.set_zoom(zoom).unwrap();
        let mut destination = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            destination = show_document_preview(ui, &document, &mut state, &labels).unwrap();
        });
        assert!(destination.is_none());
        assert!(
            output
                .shapes
                .iter()
                .any(|s| matches!(s.shape, egui::Shape::Text(_)))
        );
        assert_eq!(document, expected);
        assert_eq!(pdf.pages.len(), document.pages.len());
        output.textures_delta.clear();
    }
    let first = Document {
        pages: document.pages[..1].to_vec(),
        ..document.clone()
    };
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        show_document_preview(ui, &first, &mut state, &labels).unwrap();
    });
    output.textures_delta.clear();
    assert_eq!(state.page, 0);
    assert!(state.set_zoom(f32::NAN).is_err());
}

#[test]
fn invalid_or_unresolved_content_fails_instead_of_being_silently_dropped() {
    let mut document = dense_document(Language::En);
    document.pages[0].primitives.push(Primitive::Link {
        bounds: plan_my_cabinet::document_layout::Rect {
            x: 14.0,
            y: 50.0,
            width: 10.0,
            height: 5.0,
        },
        destination: "part:123".into(),
    });
    assert!(matches!(
        render_document_pdf(&document),
        Err(plan_my_cabinet::pdf_export::PdfExportError::UnsupportedLink)
    ));
    document.pages[0].primitives.pop();
    document.pages[0].number = 4;
    assert!(matches!(
        render_document_pdf(&document),
        Err(plan_my_cabinet::pdf_export::PdfExportError::InvalidGeometry)
    ));
    document.pages[0].number = 1;
    let unsupported = '\u{10ffff}';
    let run = document.pages[0]
        .primitives
        .iter_mut()
        .find_map(|p| match p {
            Primitive::Text(run) | Primitive::Notice(run) => Some(run),
            _ => None,
        })
        .unwrap();
    run.text = unsupported.to_string();
    run.glyphs.truncate(1);
    run.glyphs[0].character = unsupported;
    assert!(matches!(
        render_document_pdf(&document),
        Err(plan_my_cabinet::pdf_export::PdfExportError::MissingGlyph(ch)) if ch == unsupported
    ));
}

#[test]
fn supported_links_emit_pdf_annotations() {
    let mut builder = DocumentBuilder::new(PageContext {
        project: "Project".into(),
        revision: "Rev 1".into(),
        packet: "Draft".into(),
        stamp: None,
        safety: "Not a template".into(),
        notices: vec![],
    })
    .unwrap();
    let link = plan_my_cabinet::document_layout::Rect {
        x: 14.0,
        y: 50.0,
        width: 30.0,
        height: 8.0,
    };
    builder.link(link, "page:2".into()).unwrap();
    builder.page_break().unwrap();
    let document = builder.finish();
    let bytes = render_document_pdf(&document).unwrap();
    assert_eq!(
        PdfDocument::parse(&bytes, &PdfParseOptions::default(), &mut Vec::new())
            .unwrap()
            .pages
            .len(),
        2
    );
    // printpdf's parser does not import link annotations into page.ops, but
    // its serialized annotation dictionary is visible in the PDF bytes.
    assert!(bytes.windows(7).any(|window| window == b"/Annots"));
    assert!(bytes.windows(5).any(|window| window == b"/Link"));
    assert!(bytes.windows(4).any(|window| window == b"/XYZ"));
    let pdf_text = String::from_utf8_lossy(&bytes);
    let raw_rect = pdf_text
        .split("/Rect[")
        .nth(1)
        .expect("PDF link rect")
        .split(']')
        .next()
        .unwrap();
    let coords = raw_rect
        .split_whitespace()
        .map(|n| n.parse::<f32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(coords.len(), 4);
    for (actual, expected) in coords.into_iter().zip([
        link.x,
        document.pages[0].height_mm - link.y - link.height,
        link.x + link.width,
        document.pages[0].height_mm - link.y,
    ]) {
        assert!(
            (actual * PT_TO_MM - expected).abs() < 0.005,
            "annotation bounds: {raw_rect}"
        );
    }
}
