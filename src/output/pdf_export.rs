//! Offline, vector PDF representation of a prepared manufacturing snapshot.
use std::collections::BTreeMap;

use printpdf::{
    FontId, Line, LinePoint, Mm, Op, ParsedFont, PdfDocument, PdfPage, PdfSaveOptions, Point, Pt,
    TextItem,
};
use uuid::Uuid;

use crate::cut_tree::{Axis, CutKind, CutTree, Edge};
use crate::document_layout::{
    self, Document, Face, Ink, Page, Primitive, Rect as PageRect, TextRun,
};
use crate::domain::{BoardGrain, HardwareKind, Project, Stock, StockGrain, StockSource};
use crate::export::{ExportIssue, ExportMode, PreparedExport};
use crate::i18n::{Language, Localizer};
use crate::money::{Money, MoneyLocale};
use crate::units::{Length, Unit};

const FONT: &[u8] = include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");

#[derive(Debug)]
pub enum PdfExportError {
    Font,
    EmptyDocument,
    MissingGlyph(char),
    InvalidGeometry,
    UnsupportedLink,
}

const MM_TO_PT: f32 = 72.0 / 25.4;
const DOCUMENT_FACES: [Face; 6] = [
    Face::Sans,
    Face::SansMedium,
    Face::SansSemibold,
    Face::Mono,
    Face::MonoMedium,
    Face::MonoSemibold,
];

fn ink_color(ink: Ink) -> printpdf::Color {
    printpdf::Color::Rgb(printpdf::Rgb::new(
        f32::from(ink.red) / 255.0,
        f32::from(ink.green) / 255.0,
        f32::from(ink.blue) / 255.0,
        None,
    ))
}

fn pdf_point(point: document_layout::Point, height: f32) -> Point {
    Point::new(Mm(point.x), Mm(height - point.y))
}

fn pdf_rect(bounds: PageRect, height: f32) -> printpdf::Rect {
    printpdf::Rect {
        x: Pt(bounds.x * MM_TO_PT),
        // LinkAnnotation serializes Rect.y as its lower edge (unlike
        // Rect::to_line, which treats it as the top edge).
        y: Pt((height - bounds.y - bounds.height) * MM_TO_PT),
        width: Pt(bounds.width * MM_TO_PT),
        height: Pt(bounds.height * MM_TO_PT),
    }
}

fn pdf_line(points: &[document_layout::Point], closed: bool, height: f32) -> Line {
    Line {
        points: points
            .iter()
            .map(|&point| LinePoint {
                p: pdf_point(point, height),
                bezier: false,
            })
            .collect(),
        is_closed: closed,
    }
}

fn box_points(bounds: PageRect) -> [document_layout::Point; 4] {
    [
        document_layout::Point {
            x: bounds.x,
            y: bounds.y,
        },
        document_layout::Point {
            x: bounds.x + bounds.width,
            y: bounds.y,
        },
        document_layout::Point {
            x: bounds.x + bounds.width,
            y: bounds.y + bounds.height,
        },
        document_layout::Point {
            x: bounds.x,
            y: bounds.y + bounds.height,
        },
    ]
}

fn face_index(face: Face) -> usize {
    DOCUMENT_FACES
        .iter()
        .position(|f| *f == face)
        .expect("document face")
}

fn glyph_face(fonts: &[ParsedFont], face: Face, character: char) -> Option<(usize, u16)> {
    // PositionedGlyph does not record which fallback face shaped a character.
    // Refuse an unsupported primary face instead of silently using another
    // face with different outlines/metrics from the native preview.
    let index = face_index(face);
    fonts[index]
        .lookup_glyph_index(character as u32)
        .map(|gid| (index, gid))
}

fn valid_rect(bounds: PageRect, page: &Page) -> bool {
    bounds.x.is_finite()
        && bounds.y.is_finite()
        && bounds.width.is_finite()
        && bounds.height.is_finite()
        && bounds.x >= 0.0
        && bounds.y >= 0.0
        && bounds.width >= 0.0
        && bounds.height >= 0.0
        && bounds.x + bounds.width <= page.width_mm + 0.001
        && bounds.y + bounds.height <= page.height_mm + 0.001
}

fn write_positioned_text(
    ops: &mut Vec<Op>,
    run: &TextRun,
    page: &Page,
    fonts: &[ParsedFont],
    ids: &[FontId],
) -> Result<(), PdfExportError> {
    if !valid_rect(run.bounds, page)
        || !run.style.size_pt.is_finite()
        || run.style.size_pt <= 0.0
        || run.glyphs.len() != run.text.chars().count()
        || run.glyphs.iter().map(|g| g.character).ne(run.text.chars())
    {
        return Err(PdfExportError::InvalidGeometry);
    }
    // One scalar per positioned operation: printpdf must not shape/wrap the
    // source string a second time. WriteText's font subset keeps ToUnicode.
    ops.push(Op::SetFillColor {
        col: ink_color(Ink::BLACK),
    });
    for glyph in &run.glyphs {
        let b = glyph.baseline;
        if !b.x.is_finite()
            || !b.y.is_finite()
            || !glyph.advance_mm.is_finite()
            || b.x < 0.0
            || b.x + glyph.advance_mm > page.width_mm + 0.001
            || b.y < 0.0
            || b.y > page.height_mm
        {
            return Err(PdfExportError::InvalidGeometry);
        }
        let (i, _gid) = glyph_face(fonts, run.style.face, glyph.character)
            .ok_or(PdfExportError::MissingGlyph(glyph.character))?;
        ops.extend([
            Op::StartTextSection,
            Op::SetTextCursor {
                pos: pdf_point(b, page.height_mm),
            },
            Op::SetFontSize {
                font: ids[i].clone(),
                size: Pt(run.style.size_pt),
            },
            Op::WriteText {
                font: ids[i].clone(),
                items: vec![TextItem::Text(glyph.character.to_string())],
            },
            Op::EndTextSection,
        ]);
    }
    Ok(())
}

/// Serialize exactly the given frozen pages, including their already resolved
/// glyph origins, page breaks and diagram geometry. This intentionally does not
/// call `build_workshop_document` or consult the live editor. The legacy
/// `render_pdf(prepared)` receipt path remains separate until its reviewed
/// sections and snapshot can be frozen by Handoff.
pub fn render_document_pdf(document: &Document) -> Result<Vec<u8>, PdfExportError> {
    if document.pages.is_empty() {
        return Err(PdfExportError::EmptyDocument);
    }
    let fonts = DOCUMENT_FACES
        .iter()
        .map(|face| {
            ParsedFont::from_bytes(face.bytes(), 0, &mut Vec::new()).ok_or(PdfExportError::Font)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut pdf = PdfDocument::new("Workshop packet");
    let ids = fonts
        .iter()
        .map(|font| pdf.add_font(font))
        .collect::<Vec<_>>();
    let mut pages = Vec::with_capacity(document.pages.len());
    for (number, page) in document.pages.iter().enumerate() {
        if page.number != number + 1
            || !page.width_mm.is_finite()
            || !page.height_mm.is_finite()
            || page.width_mm <= 0.0
            || page.height_mm <= 0.0
        {
            return Err(PdfExportError::InvalidGeometry);
        }
        let mut ops = Vec::new();
        for primitive in &page.primitives {
            match primitive {
                Primitive::Text(run) | Primitive::Notice(run) => {
                    write_positioned_text(&mut ops, run, page, &fonts, &ids)?;
                }
                Primitive::Path {
                    points,
                    closed,
                    stroke,
                } => {
                    if points.len() < 2
                        || !stroke.width_mm.is_finite()
                        || stroke.width_mm <= 0.0
                        || points.iter().any(|p| {
                            !p.x.is_finite()
                                || !p.y.is_finite()
                                || p.x < 0.0
                                || p.x > page.width_mm
                                || p.y < 0.0
                                || p.y > page.height_mm
                        })
                    {
                        return Err(PdfExportError::InvalidGeometry);
                    }
                    ops.extend([
                        Op::SetOutlineColor {
                            col: ink_color(stroke.ink),
                        },
                        Op::SetOutlineThickness {
                            pt: Pt(stroke.width_mm * MM_TO_PT),
                        },
                        Op::DrawLine {
                            line: pdf_line(points, *closed, page.height_mm),
                        },
                    ]);
                }
                Primitive::Box {
                    bounds,
                    stroke,
                    fill,
                } => {
                    if !valid_rect(*bounds, page) || bounds.width <= 0.0 || bounds.height <= 0.0 {
                        return Err(PdfExportError::InvalidGeometry);
                    }
                    if let Some(ink) = fill {
                        ops.push(Op::SetFillColor {
                            col: ink_color(*ink),
                        });
                    }
                    if let Some(stroke) = stroke {
                        if !stroke.width_mm.is_finite() || stroke.width_mm <= 0.0 {
                            return Err(PdfExportError::InvalidGeometry);
                        }
                        ops.extend([
                            Op::SetOutlineColor {
                                col: ink_color(stroke.ink),
                            },
                            Op::SetOutlineThickness {
                                pt: Pt(stroke.width_mm * MM_TO_PT),
                            },
                        ]);
                    }
                    if fill.is_some() || stroke.is_some() {
                        ops.push(Op::DrawPolygon {
                            polygon: printpdf::Polygon {
                                rings: vec![printpdf::PolygonRing {
                                    points: pdf_line(&box_points(*bounds), true, page.height_mm)
                                        .points,
                                }],
                                mode: match (fill, stroke) {
                                    (Some(_), Some(_)) => printpdf::PaintMode::FillStroke,
                                    (Some(_), None) => printpdf::PaintMode::Fill,
                                    _ => printpdf::PaintMode::Stroke,
                                },
                                winding_order: printpdf::WindingOrder::NonZero,
                            },
                        });
                    }
                }
                Primitive::Link {
                    bounds,
                    destination,
                } => {
                    if !valid_rect(*bounds, page) || bounds.width <= 0.0 || bounds.height <= 0.0 {
                        return Err(PdfExportError::InvalidGeometry);
                    }
                    let action = if let Some(index) = destination.strip_prefix("page:") {
                        let index = index
                            .parse::<usize>()
                            .map_err(|_| PdfExportError::UnsupportedLink)?;
                        if index == 0 || index > document.pages.len() {
                            return Err(PdfExportError::UnsupportedLink);
                        }
                        printpdf::Actions::Goto(printpdf::Destination::Xyz {
                            page: index,
                            left: None,
                            top: None,
                            zoom: None,
                        })
                    } else if destination.starts_with("https://") {
                        printpdf::Actions::Uri(destination.clone())
                    } else {
                        return Err(PdfExportError::UnsupportedLink);
                    };
                    ops.push(Op::LinkAnnotation {
                        link: printpdf::LinkAnnotation::new(
                            pdf_rect(*bounds, page.height_mm),
                            action,
                            None,
                            None,
                            None,
                        ),
                    });
                }
            }
        }
        pages.push(PdfPage::new(Mm(page.width_mm), Mm(page.height_mm), ops));
    }
    Ok(pdf
        .with_pages(pages)
        .save(&PdfSaveOptions::default(), &mut Vec::new()))
}

/// Paint one prepared page in egui points. `points_per_mm` controls only view
/// magnification; it never changes the source page or reflows text. The caller
/// must install the bundled native theme fonts before painting and clip the
/// painter to the page. Link hit-testing uses the same page `Primitive::Link`
/// bounds transformed by this scale; the drawing itself makes no UI decisions.
pub fn paint_document_page(
    painter: &eframe::egui::Painter,
    page: &Page,
    origin: eframe::egui::Pos2,
    points_per_mm: f32,
) -> Result<(), PdfExportError> {
    use eframe::egui::{self, Color32, FontFamily, FontId as EguiFontId, Shape};
    if !points_per_mm.is_finite() || points_per_mm <= 0.0 {
        return Err(PdfExportError::InvalidGeometry);
    }
    if !page.width_mm.is_finite()
        || !page.height_mm.is_finite()
        || page.width_mm <= 0.0
        || page.height_mm <= 0.0
    {
        return Err(PdfExportError::InvalidGeometry);
    }
    let at =
        |p: document_layout::Point| origin + egui::vec2(p.x * points_per_mm, p.y * points_per_mm);
    let rect = |b: PageRect| {
        egui::Rect::from_min_size(
            at(document_layout::Point { x: b.x, y: b.y }),
            egui::vec2(b.width * points_per_mm, b.height * points_per_mm),
        )
    };
    let color = |ink: Ink| Color32::from_rgb(ink.red, ink.green, ink.blue);
    for primitive in &page.primitives {
        match primitive {
            Primitive::Text(run) | Primitive::Notice(run) => {
                if !valid_rect(run.bounds, page)
                    || !run.style.size_pt.is_finite()
                    || run.style.size_pt <= 0.0
                    || run.glyphs.iter().map(|g| g.character).ne(run.text.chars())
                {
                    return Err(PdfExportError::InvalidGeometry);
                }
                for glyph in &run.glyphs {
                    if !glyph.baseline.x.is_finite()
                        || !glyph.baseline.y.is_finite()
                        || !glyph.advance_mm.is_finite()
                        || glyph.baseline.x < 0.0
                        || glyph.baseline.x + glyph.advance_mm > page.width_mm + 0.001
                        || glyph.baseline.y < 0.0
                        || glyph.baseline.y > page.height_mm
                    {
                        return Err(PdfExportError::InvalidGeometry);
                    }
                    let family = match run.style.face {
                        Face::Sans => FontFamily::Proportional,
                        Face::SansMedium => FontFamily::Name("noto-medium".into()),
                        Face::SansSemibold => FontFamily::Name("noto-semibold".into()),
                        Face::Mono => FontFamily::Monospace,
                        Face::MonoMedium => FontFamily::Name("jetbrains-medium".into()),
                        Face::MonoSemibold => FontFamily::Name("jetbrains-semibold".into()),
                    };
                    let font =
                        EguiFontId::new(run.style.size_pt * points_per_mm / MM_TO_PT, family);
                    // The galley is used only to rasterize this one glyph. Its
                    // advance and placement are never used for page layout.
                    let galley =
                        painter.layout_no_wrap(glyph.character.to_string(), font, Color32::BLACK);
                    if let Some(row) = galley.rows.first()
                        && let Some(local) = row.glyphs.first()
                    {
                        let pos = at(glyph.baseline) - (row.pos.to_vec2() + local.pos.to_vec2());
                        painter.galley(pos, galley, Color32::BLACK);
                    }
                }
            }
            Primitive::Path {
                points,
                closed,
                stroke,
            } => {
                if points.len() < 2
                    || !stroke.width_mm.is_finite()
                    || stroke.width_mm <= 0.0
                    || points.iter().any(|p| {
                        !p.x.is_finite()
                            || !p.y.is_finite()
                            || p.x < 0.0
                            || p.x > page.width_mm
                            || p.y < 0.0
                            || p.y > page.height_mm
                    })
                {
                    return Err(PdfExportError::InvalidGeometry);
                }
                let mut line = points.iter().copied().map(&at).collect::<Vec<_>>();
                if *closed && !line.is_empty() {
                    line.push(line[0]);
                }
                painter.add(Shape::line(
                    line,
                    egui::Stroke::new(stroke.width_mm * points_per_mm, color(stroke.ink)),
                ));
            }
            Primitive::Box {
                bounds,
                stroke,
                fill,
            } => {
                if !valid_rect(*bounds, page) || bounds.width <= 0.0 || bounds.height <= 0.0 {
                    return Err(PdfExportError::InvalidGeometry);
                }
                if stroke.is_some_and(|s| !s.width_mm.is_finite() || s.width_mm <= 0.0) {
                    return Err(PdfExportError::InvalidGeometry);
                }
                if let Some(ink) = fill {
                    painter.rect_filled(rect(*bounds), egui::CornerRadius::ZERO, color(*ink));
                }
                if let Some(stroke) = stroke {
                    painter.rect_stroke(
                        rect(*bounds),
                        egui::CornerRadius::ZERO,
                        egui::Stroke::new(stroke.width_mm * points_per_mm, color(stroke.ink)),
                        egui::StrokeKind::Middle,
                    );
                }
            }
            Primitive::Link { bounds, .. } => {
                if !valid_rect(*bounds, page) || bounds.width <= 0.0 || bounds.height <= 0.0 {
                    return Err(PdfExportError::InvalidGeometry);
                }
            }
        }
    }
    Ok(())
}

/// Preview-only selection and magnification. Never feed these values to the
/// document builder or PDF serializer; one prepared document is the source for
/// every thumbnail, full-size page and exported page.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentPreviewState {
    /// Zero-based selected page, clamped when a refreshed packet has fewer pages.
    pub page: usize,
    /// Logical egui points per physical millimetre of paper.
    pub points_per_mm: f32,
    /// Fit the entire page as the canvas or interface scale changes.
    pub fit_page: bool,
}

impl Default for DocumentPreviewState {
    fn default() -> Self {
        Self {
            page: 0,
            // An A4 sheet fits the Handoff canvas at the 1440 × 900 reference size.
            points_per_mm: 2.3,
            fit_page: true,
        }
    }
}

impl DocumentPreviewState {
    pub fn select(&mut self, page: usize, document: &Document) {
        self.page = page.min(document.pages.len().saturating_sub(1));
    }

    pub fn set_zoom(&mut self, points_per_mm: f32) -> Result<(), PdfExportError> {
        if !points_per_mm.is_finite() || !(0.8..=8.0).contains(&points_per_mm) {
            return Err(PdfExportError::InvalidGeometry);
        }
        self.points_per_mm = points_per_mm;
        self.fit_page = false;
        Ok(())
    }
}

/// Labels are supplied by the Handoff workspace in its UI language.
pub struct DocumentPreviewLabels<'a> {
    /// Toolbar title, e.g. "Preview".
    pub title: &'a str,
    /// Localized page count, e.g. "6 pages".
    pub pages: &'a str,
    /// Accessible prefix for page thumbnails, e.g. "Page" ("Page 2").
    pub page: &'a str,
    pub zoom_in: &'a str,
    pub zoom_out: &'a str,
    pub fit: &'a str,
}

/// Paint the selected A4 page and the complete strip of selectable page
/// thumbnails directly from the frozen positioned pages. Returns a clicked
/// HTTPS destination for the host to handle; internal page links navigate here.
/// Install the bundled theme fonts on `ui.ctx()` before the first frame.
pub fn show_document_preview(
    ui: &mut eframe::egui::Ui,
    document: &Document,
    state: &mut DocumentPreviewState,
    labels: &DocumentPreviewLabels<'_>,
) -> Result<Option<String>, PdfExportError> {
    show_document_preview_layout(ui, document, state, labels).map(|(link, _)| link)
}

#[cfg_attr(not(test), allow(dead_code))] // Bounds are inspected by headless layout tests.
struct PreviewAreas {
    rail: eframe::egui::Rect,
    center: eframe::egui::Rect,
    page: eframe::egui::Rect,
    thumbnails: Vec<eframe::egui::Rect>,
}

const PREVIEW_TOOLBAR_HEIGHT: f32 = 48.0;
const PREVIEW_PADDING: f32 = 18.0;
const PREVIEW_RAIL_WIDTH: f32 = 74.0;
const PREVIEW_THUMB_WIDTH: f32 = 62.0;
const PREVIEW_PAGE_MARGIN: f32 = 12.0;
/// Zoom presets in percent of physical paper size (100% = 72 pt per inch).
const PREVIEW_ZOOM_STEPS: [f32; 13] = [
    30.0, 40.0, 50.0, 67.0, 75.0, 90.0, 100.0, 110.0, 125.0, 150.0, 175.0, 200.0, 250.0,
];

fn zoom_percent(points_per_mm: f32) -> f32 {
    points_per_mm / MM_TO_PT * 100.0
}

fn paper_label(page: &Page) -> String {
    let is = |w: f32, h: f32| {
        (page.width_mm - w).abs() < 0.5 && (page.height_mm - h).abs() < 0.5
            || (page.width_mm - h).abs() < 0.5 && (page.height_mm - w).abs() < 0.5
    };
    if is(document_layout::A4_WIDTH_MM, document_layout::A4_HEIGHT_MM) {
        "A4".into()
    } else {
        format!("{:.0} × {:.0} mm", page.width_mm, page.height_mm)
    }
}

/// A 26×24 zoom step button with a painted − or + glyph (no text glyphs).
fn zoom_step_button(
    ui: &mut eframe::egui::Ui,
    plus: bool,
    label: &str,
    enabled: bool,
) -> eframe::egui::Response {
    use crate::theme_widgets as tw;
    use eframe::egui;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(26.0, 24.0),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    let response = response.on_hover_text(label);
    if enabled && (response.hovered() || response.has_focus()) {
        ui.painter().rect_filled(rect, 6.0, tw::VIEWPORT);
    }
    let color = if enabled {
        tw::SECONDARY
    } else {
        tw::SECONDARY.gamma_multiply(0.4)
    };
    let stroke = egui::Stroke::new(1.5, color);
    let c = rect.center();
    ui.painter()
        .line_segment([c - egui::vec2(4.5, 0.0), c + egui::vec2(4.5, 0.0)], stroke);
    if plus {
        ui.painter()
            .line_segment([c - egui::vec2(0.0, 4.5), c + egui::vec2(0.0, 4.5)], stroke);
    }
    response
}

fn show_document_preview_layout(
    ui: &mut eframe::egui::Ui,
    document: &Document,
    state: &mut DocumentPreviewState,
    labels: &DocumentPreviewLabels<'_>,
) -> Result<(Option<String>, PreviewAreas), PdfExportError> {
    use crate::theme_widgets as tw;
    use eframe::egui::{self, Color32, Sense, StrokeKind};

    if document.pages.is_empty() {
        return Err(PdfExportError::EmptyDocument);
    }
    if !state.points_per_mm.is_finite() || !(0.8..=8.0).contains(&state.points_per_mm) {
        return Err(PdfExportError::InvalidGeometry);
    }
    state.select(state.page, document);
    let mut external = None;

    // Claim the whole canvas once, then place explicit children in it.
    let mut available = ui.available_size_before_wrap();
    if !available.y.is_finite() {
        available.y = 800.0;
    }
    if !available.x.is_finite() {
        available.x = 1000.0;
    }
    let (full, _) = ui.allocate_exact_size(available.max(egui::vec2(1.0, 1.0)), Sense::hover());
    let toolbar = egui::Rect::from_min_size(
        full.min,
        egui::vec2(full.width(), PREVIEW_TOOLBAR_HEIGHT.min(full.height())),
    );
    let body = egui::Rect::from_min_max(
        egui::pos2(full.left() + PREVIEW_PADDING, toolbar.bottom() + 4.0),
        egui::pos2(
            full.right() - PREVIEW_PADDING,
            (full.bottom() - PREVIEW_PADDING).max(toolbar.bottom() + 5.0),
        ),
    );
    let rail_width = PREVIEW_RAIL_WIDTH.min((body.width() - PREVIEW_PADDING).max(1.0) * 0.28);
    let rail = egui::Rect::from_min_size(body.min, egui::vec2(rail_width, body.height()));
    let center = egui::Rect::from_min_max(
        egui::pos2(
            (rail.right() + PREVIEW_PADDING).min(body.right() - 1.0),
            body.top(),
        ),
        body.max,
    );

    let page = &document.pages[state.page];
    let fit_scale = ((center.width() - 2.0 * PREVIEW_PAGE_MARGIN).max(1.0) / page.width_mm)
        .min((center.height() - 2.0 * PREVIEW_PAGE_MARGIN).max(1.0) / page.height_mm)
        * 0.999;

    // Toolbar: title and page count on the left, zoom pill on the right.
    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(toolbar.shrink2(egui::vec2(16.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    bar.spacing_mut().item_spacing.x = 8.0;
    bar.add(
        egui::Label::new(tw::semibold(&bar, labels.title, 13.0).color(tw::TEXT)).selectable(false),
    );
    bar.add(
        egui::Label::new(
            egui::RichText::new(format!("{} · {}", labels.pages, paper_label(page)))
                .size(13.0)
                .color(tw::MUTED),
        )
        .selectable(false),
    );
    bar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        egui::Frame::new()
            .fill(tw::PANEL)
            .stroke(egui::Stroke::new(1.0, tw::BORDER))
            .corner_radius(9)
            .inner_margin(3)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let shown = if state.fit_page {
                    fit_scale
                } else {
                    state.points_per_mm
                };
                let percent = zoom_percent(shown);
                let to_ppm = |pct: f32| (pct / 100.0 * MM_TO_PT).clamp(0.8, 8.0);
                let lower = PREVIEW_ZOOM_STEPS
                    .iter()
                    .rev()
                    .copied()
                    .find(|step| *step < percent - 0.5 && to_ppm(*step) < shown - 0.001);
                let higher = PREVIEW_ZOOM_STEPS
                    .iter()
                    .copied()
                    .find(|step| *step > percent + 0.5 && to_ppm(*step) > shown + 0.001);
                // Right-to-left: the first widget is the rightmost.
                let fit_bg = ui.painter().add(egui::Shape::Noop);
                let fit = tw::ghost_icon_sized(
                    ui,
                    crate::icons::Icon::Frame,
                    labels.fit,
                    if state.fit_page {
                        tw::ACCENT_DARK
                    } else {
                        tw::SECONDARY
                    },
                    14.0,
                    24.0,
                    true,
                    false,
                );
                if state.fit_page {
                    ui.painter().set(
                        fit_bg,
                        egui::Shape::rect_filled(fit.rect, 6.0, tw::ACCENT_BG),
                    );
                }
                if fit.clicked() {
                    state.fit_page = true;
                }
                let (sep, _) = ui.allocate_exact_size(egui::vec2(5.0, 18.0), Sense::hover());
                ui.painter().vline(
                    sep.center().x,
                    sep.y_range(),
                    egui::Stroke::new(1.0, tw::BORDER_SOFT),
                );
                if zoom_step_button(ui, true, labels.zoom_in, higher.is_some()).clicked()
                    && let Some(step) = higher
                {
                    let _ = state.set_zoom(to_ppm(step));
                }
                ui.add(
                    egui::Label::new(tw::mono(format!("{percent:.0}%"), 12.0).color(tw::TEXT))
                        .selectable(false),
                );
                if zoom_step_button(ui, false, labels.zoom_out, lower.is_some()).clicked()
                    && let Some(step) = lower
                {
                    let _ = state.set_zoom(to_ppm(step));
                }
            });
    });

    // Thumbnail rail.
    let mut rail_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rail)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let thumbnails = egui::ScrollArea::vertical()
        .id_salt("document-preview-thumbnails")
        .max_height(rail.height())
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(
            &mut rail_ui,
            |ui| -> Result<Vec<egui::Rect>, PdfExportError> {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut thumbnails = Vec::with_capacity(document.pages.len());
                let thumb_width = PREVIEW_THUMB_WIDTH.min(rail_width - 4.0).max(1.0);
                for (index, page) in document.pages.iter().enumerate() {
                    let scale = thumb_width / page.width_mm;
                    let size = egui::vec2(thumb_width, page.height_mm * scale);
                    let (slot, _) = ui
                        .allocate_exact_size(egui::vec2(rail_width, size.y + 34.0), Sense::hover());
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(slot.center().x - size.x / 2.0, slot.top() + 2.0),
                        size,
                    );
                    let response = ui.interact(
                        rect,
                        ui.id().with(("document-preview-thumb", index)),
                        Sense::click(),
                    );
                    let selected = index == state.page;
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            response.enabled(),
                            selected,
                            format!("{} {}", labels.page, index + 1),
                        )
                    });
                    thumbnails.push(rect);
                    if !selected {
                        ui.painter().add(
                            egui::Shadow {
                                offset: [0, 1],
                                blur: 3,
                                spread: 0,
                                color: Color32::from_black_alpha(38),
                            }
                            .as_shape(rect, 0),
                        );
                    }
                    ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
                    paint_document_page(
                        &ui.painter().with_clip_rect(rect.intersect(ui.clip_rect())),
                        page,
                        rect.min,
                        scale,
                    )?;
                    if selected {
                        ui.painter().rect_stroke(
                            rect,
                            0.0,
                            egui::Stroke::new(2.0, tw::TEXT),
                            StrokeKind::Outside,
                        );
                    } else if response.hovered() || response.has_focus() {
                        ui.painter().rect_stroke(
                            rect,
                            0.0,
                            egui::Stroke::new(1.0, tw::BORDER_STRONG),
                            StrokeKind::Outside,
                        );
                    }
                    let number = (index + 1).to_string();
                    let font = if selected {
                        tw::weighted_font(ui, 11.0, crate::theme::Typeface::SansSemibold)
                    } else {
                        egui::FontId::proportional(11.0)
                    };
                    ui.painter().text(
                        egui::pos2(slot.center().x, rect.bottom() + 6.0),
                        egui::Align2::CENTER_TOP,
                        number,
                        font,
                        if selected { tw::TEXT } else { tw::MUTED },
                    );
                    if response.clicked() {
                        state.page = index;
                    }
                }
                Ok(thumbnails)
            },
        )
        .inner?;

    // Current page, centred on the canvas with a paper shadow.
    let mut center_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(center)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let page_index = state.page;
    let page_rect = egui::ScrollArea::both()
        .id_salt("document-preview-page")
        .max_height(center.height())
        .max_width(center.width())
        .auto_shrink([false, false])
        .show(&mut center_ui, |ui| -> Result<egui::Rect, PdfExportError> {
            let page = &document.pages[page_index];
            let scale = if state.fit_page {
                fit_scale
            } else {
                state.points_per_mm
            };
            let size = egui::vec2(page.width_mm * scale, page.height_mm * scale);
            let viewport = center.size();
            let content = egui::vec2(
                (size.x + 2.0 * PREVIEW_PAGE_MARGIN).max(viewport.x),
                (size.y + 2.0 * PREVIEW_PAGE_MARGIN).max(viewport.y),
            );
            let (area, _) = ui.allocate_exact_size(content, Sense::hover());
            let rect = egui::Rect::from_center_size(area.center(), size);
            let response =
                ui.interact(rect, ui.id().with("document-preview-sheet"), Sense::click());
            ui.painter().add(
                egui::Shadow {
                    offset: [0, 4],
                    blur: 18,
                    spread: 0,
                    color: Color32::from_rgba_unmultiplied(40, 30, 15, 46),
                }
                .as_shape(rect, 0),
            );
            ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
            paint_document_page(
                &ui.painter().with_clip_rect(rect.intersect(ui.clip_rect())),
                page,
                rect.min,
                scale,
            )?;
            if response.clicked()
                && let Some(pointer) = response.interact_pointer_pos()
            {
                for primitive in &page.primitives {
                    if let Primitive::Link {
                        bounds,
                        destination,
                    } = primitive
                    {
                        let hit = egui::Rect::from_min_size(
                            rect.min + egui::vec2(bounds.x * scale, bounds.y * scale),
                            egui::vec2(bounds.width * scale, bounds.height * scale),
                        );
                        if hit.contains(pointer) {
                            if let Some(index) = destination.strip_prefix("page:") {
                                let index = index
                                    .parse::<usize>()
                                    .map_err(|_| PdfExportError::UnsupportedLink)?;
                                if index == 0 || index > document.pages.len() {
                                    return Err(PdfExportError::UnsupportedLink);
                                }
                                state.page = index - 1;
                            } else if destination.starts_with("https://") {
                                external = Some(destination.clone());
                            } else {
                                return Err(PdfExportError::UnsupportedLink);
                            }
                            break;
                        }
                    }
                }
            }
            Ok(rect)
        })
        .inner?;
    Ok((
        external,
        PreviewAreas {
            rail,
            center,
            page: page_rect,
            thumbnails,
        },
    ))
}

fn label(loc: &Localizer, key: &str) -> String {
    loc.text(key)
}

// Prepared snapshots have the same normalized aliases as their receipt.
fn stock_alias(project: &Project, id: Uuid) -> Option<String> {
    project.stock_alias(id).map(str::to_owned)
}

fn stock_ref(project: &Project, id: Uuid) -> String {
    stock_alias(project, id)
        .map(|alias| format!("{alias} [{id}]"))
        .unwrap_or_else(|| id.to_string())
}

fn stock_description(project: &Project, stock: &Stock) -> String {
    format!("{} · {}", stock_ref(project, stock.id), stock.name)
}

fn length(value: Length, unit: Unit, language: Language) -> String {
    let (factor, suffix) = match unit {
        Unit::Mm => (1_000., "mm"),
        Unit::Cm => (10_000., "cm"),
        Unit::M => (1_000_000., "m"),
        Unit::Inch => (25_400., "in"),
        Unit::Foot => (304_800., "ft"),
    };
    let number = format!("{:.6}", value.micrometres() as f64 / factor);
    let number = number.trim_end_matches('0').trim_end_matches('.');
    format!(
        "{} {}",
        if language == Language::PtBr {
            number.replace('.', ",")
        } else {
            number.into()
        },
        suffix
    )
}

fn money(value: Option<Money>, language: Language) -> String {
    value
        .map(|v| {
            v.display(if language == Language::PtBr {
                MoneyLocale::PortugueseBrazil
            } else {
                MoneyLocale::English
            })
        })
        .unwrap_or_else(|| {
            if language == Language::PtBr {
                "desconhecido"
            } else {
                "unknown"
            }
            .into()
        })
}

struct Pages {
    pages: Vec<PdfPage>,
    ops: Vec<Op>,
    font: FontId,
    y: f32,
    draft: bool,
    language: Language,
    sheet_context: Option<Vec<String>>,
}
impl Pages {
    fn new(font: FontId, language: Language, draft: bool) -> Self {
        let mut p = Self {
            pages: Vec::new(),
            ops: Vec::new(),
            font,
            y: 277.,
            draft,
            language,
            sheet_context: None,
        };
        p.header();
        p
    }
    fn header(&mut self) {
        if self.draft {
            self.text_at(
                14.,
                287.,
                17.,
                if self.language == Language::PtBr {
                    "RASCUNHO / NÃO USAR PARA CORTE"
                } else {
                    "DRAFT / NOT FOR CUTTING"
                },
            );
        }
    }
    fn next(&mut self) {
        self.pages.push(PdfPage::new(
            Mm(210.),
            Mm(297.),
            std::mem::take(&mut self.ops),
        ));
        self.y = 277.;
        self.header();
        if let Some(context) = self.sheet_context.clone() {
            for line in context {
                self.line(line);
            }
            self.y -= 3.;
        }
    }
    fn line(&mut self, text: impl AsRef<str>) {
        // Break long metadata and issue descriptions rather than clipping IDs or warnings.
        let mut row = String::new();
        for word in text.as_ref().split_whitespace() {
            if !row.is_empty() && row.chars().count() + word.chars().count() + 1 > 78 {
                self.short_line(&row);
                row.clear();
            }
            if !row.is_empty() {
                row.push(' ');
            }
            row.push_str(word);
        }
        if !row.is_empty() {
            self.short_line(&row);
        }
    }
    fn short_line(&mut self, text: &str) {
        if self.y < 21. {
            self.next();
        }
        self.text_at(14., self.y, 9., text);
        self.y -= 5.;
    }
    fn title(&mut self, text: impl AsRef<str>) {
        if self.y < 35. {
            self.next();
        }
        self.y -= 3.;
        self.text_at(14., self.y, 13., text.as_ref());
        self.y -= 8.;
    }
    fn text_at(&mut self, x: f32, y: f32, size: f32, text: &str) {
        self.ops.extend([
            Op::StartTextSection,
            Op::SetTextCursor {
                pos: Point::new(Mm(x), Mm(y)),
            },
            Op::SetFontSize {
                font: self.font.clone(),
                size: Pt(size),
            },
            Op::WriteText {
                font: self.font.clone(),
                items: vec![TextItem::Text(text.to_owned())],
            },
            Op::EndTextSection,
        ]);
    }
    fn stroke(&mut self, coords: &[(f32, f32)], closed: bool) {
        self.ops.push(Op::DrawLine {
            line: Line {
                points: coords
                    .iter()
                    .map(|&(x, y)| LinePoint {
                        p: Point::new(Mm(x), Mm(y)),
                        bezier: false,
                    })
                    .collect(),
                is_closed: closed,
            },
        });
    }
    fn arrow(&mut self, x: f32, y: f32, dx: f32, dy: f32) {
        let tip = (x + dx, y + dy);
        self.stroke(&[(x, y), tip], false);
        self.stroke(
            &[
                (tip.0 - dx * 0.2 - dy * 0.2, tip.1 - dy * 0.2 + dx * 0.2),
                tip,
                (tip.0 - dx * 0.2 + dy * 0.2, tip.1 - dy * 0.2 - dx * 0.2),
            ],
            false,
        );
    }
    fn finish(mut self) -> Vec<PdfPage> {
        self.next();
        self.pages
    }
}

fn issue(issue: &ExportIssue, project: &Project, loc: &Localizer, unit: Unit) -> String {
    match issue {
        ExportIssue::Board {
            id, name, reasons, ..
        } => format!(
            "{name} [{id}]: {}",
            reasons
                .iter()
                .map(|r| loc.text(r.key()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ExportIssue::Sheet { id, name, reason } => format!(
            "{} · {name}: {}",
            stock_ref(project, *id),
            loc.text(match reason {
                crate::export::SheetIssue::BudgetExhausted => "sheet-feasibility-unknown",
                _ => "sheet-cut-conflict",
            })
        ),
        ExportIssue::KerfUnconfirmed(v) => {
            format!(
                "{}: {}",
                loc.text("export-kerf-unconfirmed"),
                length(*v, unit, loc.language())
            )
        }
        ExportIssue::UnknownPrice { stock_id } => format!(
            "{}: {}",
            loc.text("export-price-unknown"),
            stock_id.map_or_else(|| loc.text("export-cut-fee"), |id| stock_ref(project, id))
        ),
        ExportIssue::Hardware { id, name, reason } => {
            format!(
                "{name} [{id}]: {} — {}",
                loc.text(match reason {
                    crate::export::HardwareIssue::MissingCatalog(_) =>
                        "pdf-hardware-missing-catalog",
                    crate::export::HardwareIssue::UnverifiedInstallation =>
                        "pdf-hardware-unverified",
                    crate::export::HardwareIssue::InvalidReference => "pdf-hardware-invalid",
                }),
                loc.text("pdf-installation-withheld")
            )
        }
        ExportIssue::Installation { id, name, reason } => format!(
            "{name} [{id}]: {} — {}",
            loc.text(match reason {
                crate::hinge_installation::InstallationIssue::MissingPart(_) =>
                    "pdf-install-missing-part",
                crate::hinge_installation::InstallationIssue::MissingCatalog(_) =>
                    "pdf-hardware-missing-catalog",
                crate::hinge_installation::InstallationIssue::MissingVerifiedCatalog =>
                    "pdf-hardware-unverified",
                crate::hinge_installation::InstallationIssue::UnsupportedThickness =>
                    "pdf-install-thickness",
                crate::hinge_installation::InstallationIssue::UnsupportedOverlay =>
                    "pdf-install-overlay",
                crate::hinge_installation::InstallationIssue::CupOutsideDoor =>
                    "pdf-install-cup-outside",
                crate::hinge_installation::InstallationIssue::PlateOutsideMount =>
                    "pdf-install-plate-outside",
                crate::hinge_installation::InstallationIssue::InsetShallowerThanDoor =>
                    "pdf-install-inset-depth",
            }),
            loc.text("pdf-installation-withheld")
        ),
        ExportIssue::JointNeedsReview {
            id,
            installation_id,
        } => format!(
            "{installation_id} [{id}]: {} — {}",
            loc.text("pdf-joint-review"),
            loc.text("pdf-installation-withheld")
        ),
        ExportIssue::InvalidWood(_) => loc.text("pdf-invalid-wood"),
    }
}

fn draw_sheet(
    p: &mut Pages,
    tree: &CutTree,
    stock: &crate::domain::Stock,
    project: &crate::domain::Project,
    loc: &Localizer,
    unit: Unit,
) {
    p.next();
    let scale = (170. / (stock.length.micrometres() as f32 / 1000.))
        .min(105. / (stock.width.micrometres() as f32 / 1000.));
    let context = vec![
        format!(
            "{}: {}",
            label(loc, "pdf-sheet"),
            stock_description(project, stock)
        ),
        format!("ID: {}", stock.id),
        format!(
            "{}: 1 mm {} = {} mm {}; {}",
            label(loc, "pdf-scale"),
            label(loc, "pdf-on-paper"),
            if p.language == Language::PtBr {
                format!("{:.4}", 1. / scale).replace('.', ",")
            } else {
                format!("{:.4}", 1. / scale)
            },
            label(loc, "pdf-actual"),
            label(loc, "pdf-not-template")
        ),
        format!("{}: {}", label(loc, "pdf-cuts"), tree.cut_count()),
    ];
    for line in &context {
        p.line(line);
    }
    p.sheet_context = Some(context);
    let base_x = 17.;
    let base_y = 132.;
    let sx = |v: Length| base_x + v.micrometres() as f32 / 1000. * scale;
    let sy = |v: Length| base_y + v.micrometres() as f32 / 1000. * scale;
    let x1 = sx(stock.length);
    let y1 = sy(stock.width);
    // Dense, narrow sheets cannot fit cut IDs inside their cells. Keep the
    // physical drawing at its true scale and key each line from above instead.
    let dense_callouts = tree.cut_count() > 10;
    p.stroke(
        &[(base_x, base_y), (x1, base_y), (x1, y1), (base_x, y1)],
        true,
    );
    match stock.grain {
        StockGrain::AlongX => p.arrow(base_x + 5., base_y - 6., 15., 0.),
        StockGrain::AlongY => p.arrow(base_x - 4., base_y + 2., 0., 15.),
        _ => {}
    }
    for (id, node) in tree.nodes().iter().enumerate() {
        if let CutKind::Part(board_id) = node.kind {
            let r = node.rectangle;
            let x = sx(r.origin[0]);
            let y = sy(r.origin[1]);
            let right = sx(Length::from_micrometres(
                r.origin[0].micrometres() + r.extent[0].micrometres(),
            ));
            let top = sy(Length::from_micrometres(
                r.origin[1].micrometres() + r.extent[1].micrometres(),
            ));
            p.stroke(&[(x, y), (right, y), (right, top), (x, top)], true);
            if let Some(board) = project.boards.iter().find(|b| b.id == board_id) {
                // The full label belongs in the keyed legend: even a short name
                // can protrude into a neighbouring part on a small sheet cell.
                if right - x >= 9. && top - y >= 5. {
                    p.text_at(x + 1., (y + top) / 2., 7., &format!("P{id}"));
                }
                let material = project.materials.iter().find(|m| m.id == board.material_id);
                let grain = material
                    .map(|m| board.effective_grain(m))
                    .unwrap_or(BoardGrain::Unrestricted);
                let turned = project
                    .allocations
                    .iter()
                    .find(|a| a.board_id == board_id)
                    .is_some_and(|a| a.quarter_turn);
                match grain {
                    BoardGrain::Length => {
                        if right - x >= 12. && top - y >= 12. {
                            if turned {
                                p.arrow(x + 3., y + 3., 0., (top - y - 6.).min(12.));
                            } else {
                                p.arrow(x + 3., y + 3., (right - x - 6.).min(12.), 0.);
                            }
                        }
                    }
                    BoardGrain::Width => {
                        if right - x >= 12. && top - y >= 12. {
                            if turned {
                                p.arrow(x + 3., y + 3., (right - x - 6.).min(12.), 0.);
                            } else {
                                p.arrow(x + 3., y + 3., 0., (top - y - 6.).min(12.));
                            }
                        }
                    }
                    BoardGrain::Unrestricted => {}
                }
            }
        }
    }
    let mut callouts = Vec::new();
    for op in tree.operations() {
        let input = tree.node(op.input).expect("verified input").rectangle;
        let first = tree
            .node(op.outputs.first)
            .expect("verified output")
            .rectangle;
        let (x, y) = (sx(input.origin[0]), sy(input.origin[1]));
        let (right, top) = (
            sx(Length::from_micrometres(
                input.origin[0].micrometres() + input.extent[0].micrometres(),
            )),
            sy(Length::from_micrometres(
                input.origin[1].micrometres() + input.extent[1].micrometres(),
            )),
        );
        match op.axis {
            Axis::X => {
                let at = sx(Length::from_micrometres(
                    input.origin[0].micrometres() + first.extent[0].micrometres(),
                ));
                p.stroke(&[(at, y), (at, top)], false);
                if dense_callouts {
                    callouts.push((op.number, at, top));
                } else if top - y >= 6. && right - at >= 12. {
                    p.text_at(at + 1., (y + top) / 2., 7., &format!("C{}", op.number));
                }
            }
            Axis::Y => {
                let at = sy(Length::from_micrometres(
                    input.origin[1].micrometres() + first.extent[1].micrometres(),
                ));
                p.stroke(&[(x, at), (right, at)], false);
                if dense_callouts {
                    callouts.push((op.number, (x + right) / 2., at));
                } else if right - x >= 13. && top - at >= 5. {
                    p.text_at((x + right) / 2., at + 1., 7., &format!("C{}", op.number));
                }
            }
        }
    }
    // Assign rows by physical position, rather than operation order. Each
    // row tracks the last label's right edge, so even nearby cuts cannot
    // produce overlapping IDs. The leader ends just below its own ID.
    callouts.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut row_ends: Vec<f32> = Vec::new();
    for (number, cut_x, cut_y) in callouts {
        let id = format!("C{number}");
        // Noto Sans at 7 pt: 2.5 mm per glyph is a conservative bound.
        let width = id.len() as f32 * 2.5;
        let label_x = (cut_x - width / 2.).clamp(14., 196. - width);
        let row = row_ends
            .iter()
            .position(|end| label_x >= end + 2.)
            .unwrap_or_else(|| {
                row_ends.push(0.);
                row_ends.len() - 1
            });
        row_ends[row] = label_x + width;
        let label_y = y1 + 9. + row as f32 * 8.;
        p.stroke(&[(cut_x, cut_y), (cut_x, label_y - 3.)], false);
        p.text_at(label_x, label_y, 7., &id);
    }
    p.text_at(
        base_x,
        if dense_callouts {
            y1 + 12. + row_ends.len() as f32 * 8.
        } else {
            y1 + 3.
        },
        8.,
        &format!(
            "{} × {}",
            length(stock.length, unit, p.language),
            length(stock.width, unit, p.language)
        ),
    );
    p.y = 119.;
    p.line(label(loc, "pdf-part-key"));
    for (id, node) in tree.nodes().iter().enumerate() {
        if let CutKind::Part(board_id) = node.kind
            && let Some(board) = project.boards.iter().find(|b| b.id == board_id)
        {
            p.line(format!(
                "P{id}: {} [{}] — {} × {}",
                board.name,
                board.id,
                length(node.rectangle.extent[0], unit, p.language),
                length(node.rectangle.extent[1], unit, p.language)
            ));
        }
    }
    p.line(label(loc, "pdf-cut-key"));
    for op in tree.operations() {
        let edge = match (op.axis, op.reference_edge) {
            (Axis::X, Edge::Low) => "X-",
            (Axis::X, Edge::High) => "X+",
            (Axis::Y, Edge::Low) => "Y-",
            (Axis::Y, Edge::High) => "Y+",
        };
        let side = match (op.axis, op.kerf_side) {
            (Axis::X, Edge::Low) => "X-",
            (Axis::X, Edge::High) => "X+",
            (Axis::Y, Edge::Low) => "Y-",
            (Axis::Y, Edge::High) => "Y+",
        };
        p.line(format!(
            "C{} P{}: {} {} -> P{} {} (P{}, P{}); {} {} {} P{}: {}",
            op.number,
            op.input,
            edge,
            length(op.retained_extent, unit, p.language),
            op.retained_output,
            label(loc, "pdf-retained"),
            op.outputs.first,
            op.outputs.second,
            label(loc, "pdf-blade-strip"),
            side,
            label(loc, "pdf-from-retained"),
            op.retained_output,
            length(tree.kerf(), unit, p.language)
        ));
    }
    for (id, node) in tree.nodes().iter().enumerate() {
        let kind = match node.kind {
            CutKind::Part(board) => format!("{} {board}", label(loc, "pdf-part")),
            CutKind::Offcut => label(loc, "pdf-offcut"),
            CutKind::Waste => label(loc, "pdf-waste"),
            CutKind::Split { .. } => continue,
        };
        p.line(format!(
            "P{id} {kind}: {} × {}",
            length(node.rectangle.extent[0], unit, p.language),
            length(node.rectangle.extent[1], unit, p.language)
        ));
    }
    p.sheet_context = None;
}

/// Renders only the immutable project and witnesses in `prepared`; never reads live editor state.
pub fn render_pdf(prepared: &PreparedExport) -> Result<Vec<u8>, PdfExportError> {
    let project = prepared.snapshot.project();
    let settings = prepared.snapshot.settings();
    let loc = Localizer::new(settings.language);
    let font = ParsedFont::from_bytes(FONT, 0, &mut Vec::new()).ok_or(PdfExportError::Font)?;
    let mut doc = PdfDocument::new(&project.name);
    let mut p = Pages::new(
        doc.add_font(&font),
        settings.language,
        prepared.mode == ExportMode::Draft,
    );
    p.title(format!("{} — {}", project.name, label(&loc, "pdf-packet")));
    p.line(format!(
        "{}: {} / {}: {} / ID: {}",
        label(&loc, "pdf-project"),
        project.name,
        label(&loc, "pdf-revision"),
        prepared.snapshot.revision(),
        project.id
    ));
    p.line(format!(
        "{}: {:?}; {}: {}",
        label(&loc, "pdf-units"),
        match settings.units {
            Unit::Mm => "mm",
            Unit::Cm => "cm",
            Unit::M => "m",
            Unit::Inch => "in",
            Unit::Foot => "ft",
        },
        label(&loc, "pdf-currency"),
        project.currency.code()
    ));
    p.line(format!(
        "{}: {}",
        label(&loc, "pdf-kerf"),
        length(project.cutting_kerf, settings.units, settings.language)
    ));
    p.line(format!(
        "{}: {}",
        label(&loc, "pdf-assumptions"),
        label(&loc, "pdf-assumptions-detail")
    ));
    if !prepared.wood_issues.is_empty() || !prepared.notices.is_empty() {
        p.title(label(&loc, "pdf-issues"));
        for item in prepared.wood_issues.iter().chain(&prepared.notices) {
            p.line(issue(item, project, &loc, settings.units));
        }
    }
    p.title(label(&loc, "pdf-stock"));
    for stock in &project.stock {
        let used = project.allocations.iter().any(|a| a.stock_id == stock.id);
        let material = project
            .materials
            .iter()
            .find(|m| m.id == stock.material_id)
            .map_or_else(|| label(&loc, "pdf-missing"), |m| m.name.clone());
        p.line(format!(
            "{} — {} — {} × {} × {}; {}; {}; {}: {}{}",
            stock_description(project, stock),
            material,
            length(stock.length, settings.units, settings.language),
            length(stock.width, settings.units, settings.language),
            length(stock.thickness, settings.units, settings.language),
            label(
                &loc,
                if stock.source == StockSource::Owned {
                    "stock-owned"
                } else {
                    "stock-purchase"
                }
            ),
            label(
                &loc,
                match stock.grain {
                    StockGrain::AlongX => "stock-grain-x",
                    StockGrain::AlongY => "stock-grain-y",
                    StockGrain::Nondirectional => "stock-grain-none",
                    StockGrain::Unknown => "stock-grain-unknown",
                }
            ),
            label(&loc, "pdf-price"),
            if stock.source == StockSource::Owned {
                label(&loc, "stock-owned")
            } else {
                money(stock.price, settings.language)
            },
            if used {
                label(&loc, "pdf-used")
            } else {
                String::new()
            }
        ));
        p.line(format!(
            "{} {}: {} / {} / {} / {}",
            label(&loc, "pdf-trim"),
            label(&loc, "pdf-edges"),
            length(stock.trim[0], settings.units, settings.language),
            length(stock.trim[1], settings.units, settings.language),
            length(stock.trim[2], settings.units, settings.language),
            length(stock.trim[3], settings.units, settings.language)
        ));
    }
    p.title(label(&loc, "pdf-parts"));
    let mut groups: BTreeMap<(String, Uuid, i64, i64, i64), Vec<Uuid>> = BTreeMap::new();
    for b in &project.boards {
        groups
            .entry((
                b.name.clone(),
                b.material_id,
                b.length.micrometres(),
                b.width.micrometres(),
                b.thickness.micrometres(),
            ))
            .or_default()
            .push(b.id);
    }
    for ((name, material, l, w, t), ids) in groups {
        p.line(format!(
            "{name} × {} — {} × {} × {}",
            ids.len(),
            length(
                Length::from_micrometres(l),
                settings.units,
                settings.language
            ),
            length(
                Length::from_micrometres(w),
                settings.units,
                settings.language
            ),
            length(
                Length::from_micrometres(t),
                settings.units,
                settings.language
            )
        ));
        for id in ids {
            p.line(format!(
                "  {id} — {} {material}; {} {}",
                label(&loc, "material"),
                label(&loc, "stock"),
                project
                    .allocations
                    .iter()
                    .find(|a| a.board_id == id)
                    .map_or_else(
                        || label(&loc, "board-unallocated"),
                        |a| stock_ref(project, a.stock_id)
                    )
            ));
        }
    }
    p.title(label(&loc, "pdf-hardware"));
    for h in &project.hardware {
        p.line(format!(
            "{} [{}] — {}",
            h.name,
            h.id,
            match &h.kind {
                HardwareKind::Placeholder { dimensions } => format!(
                    "{} — {} × {} × {}",
                    label(&loc, "pdf-reference-hardware"),
                    length(dimensions[0], settings.units, settings.language),
                    length(dimensions[1], settings.units, settings.language),
                    length(dimensions[2], settings.units, settings.language),
                ),
                HardwareKind::Catalog { catalog_id } => project
                    .catalog
                    .iter()
                    .find(|c| c.id == *catalog_id)
                    .map_or_else(
                        || format!(
                            "{} {catalog_id} {}",
                            label(&loc, "pdf-catalog"),
                            label(&loc, "pdf-missing")
                        ),
                        |c| format!(
                            "{} / {} / {}",
                            c.product_id,
                            c.plate_id.as_deref().unwrap_or("—"),
                            c.revision
                        )
                    ),
            }
        ));
        if prepared.withheld_installation_guidance.contains(&h.id) {
            p.line(label(&loc, "pdf-installation-withheld"));
        }
    }
    if !project.hinge_installations.is_empty()
        || !project.door_joints.is_empty()
        || project
            .hardware
            .iter()
            .any(|h| matches!(h.kind, HardwareKind::Catalog { .. }))
    {
        p.line(label(&loc, "pdf-hinge-review-warning"));
        p.line(label(&loc, "pdf-motion-approximate"));
    }
    for installation in &project.hinge_installations {
        p.line(format!(
            "{} [{}] — {} / {}",
            label(&loc, "pdf-hinge-installation"),
            installation.id,
            project
                .boards
                .iter()
                .find(|b| b.id == installation.door_board_id)
                .map_or("—", |b| b.name.as_str()),
            project
                .boards
                .iter()
                .find(|b| b.id == installation.mounting_board_id)
                .map_or("—", |b| b.name.as_str()),
        ));
        if prepared
            .withheld_installation_guidance
            .contains(&installation.id)
        {
            p.line(label(&loc, "pdf-installation-withheld"));
            continue;
        }
        if let Some(g) = prepared
            .installation_guidance
            .iter()
            .find(|g| g.id == installation.id)
        {
            let r = &g.references;
            p.line(r.source_line(&loc));
            p.line(r.settings_line(&loc, g.cup_edge_setback, g.overlay, |v| {
                length(v, settings.units, settings.language)
            }));
            let coord = |point: [i128; 3]| -> String {
                point
                    .iter()
                    .map(|v| {
                        length(
                            Length::from_micrometres(
                                i64::try_from(*v).expect("checked board-local reference"),
                            ),
                            settings.units,
                            settings.language,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" / ")
            };
            p.line(format!(
                "{}: {} {} — {}: {} ({})",
                label(&loc, "pdf-cup-center"),
                coord(r.cup_center_um),
                label(
                    &loc,
                    match r.cup_face {
                        crate::domain::BoardFace::MinZ => "pdf-face-min-z",
                        crate::domain::BoardFace::MaxZ => "pdf-face-max-z",
                    }
                ),
                label(&loc, "pdf-cup-edge"),
                length(g.cup_edge_setback, settings.units, settings.language),
                label(&loc, "pdf-board-local")
            ));
            for hole in r.plate_hole_centers_um {
                p.line(format!(
                    "{}: {} {} ({})",
                    label(&loc, "pdf-plate-center"),
                    coord(hole),
                    label(
                        &loc,
                        match r.plate_face {
                            crate::domain::BoardFace::MinZ => "pdf-face-min-z",
                            crate::domain::BoardFace::MaxZ => "pdf-face-max-z",
                        }
                    ),
                    label(&loc, "pdf-board-local")
                ));
            }
            p.line(label(&loc, "pdf-fasteners-unavailable"));
        }
    }
    let mut cuts = 0;
    for (id, tree) in &prepared.witnesses {
        if let Some(stock) = project.stock.iter().find(|s| s.id == *id) {
            cuts += tree.cut_count();
            draw_sheet(&mut p, tree, stock, project, &loc, settings.units);
        }
    }
    p.title(label(&loc, "pdf-cost"));
    p.line(format!("{}: {cuts}", label(&loc, "pdf-cuts")));
    let material = project
        .stock
        .iter()
        .filter(|s| {
            project.allocations.iter().any(|a| a.stock_id == s.id)
                && s.source == StockSource::ToPurchase
        })
        .try_fold(0_i64, |sum, s| sum.checked_add(s.price?.minor_units()));
    let fee = if cuts == 0 {
        Some(0)
    } else {
        project
            .cut_fee
            .and_then(|f| f.minor_units().checked_mul(cuts as i64))
    };
    let show = |v: Option<i64>| {
        money(
            v.and_then(|n| Money::new(project.currency, n).ok()),
            settings.language,
        )
    };
    p.line(format!(
        "{}: {} / {}: {} × {} = {}",
        label(&loc, "pdf-material-cost"),
        show(material),
        label(&loc, "pdf-cut-cost"),
        show(project.cut_fee.map(|v| v.minor_units())),
        cuts,
        show(fee)
    ));
    let complete = prepared.wood_issues.is_empty()
        && project.allocations.len() == project.boards.len()
        && prepared.witnesses.len()
            == project
                .stock
                .iter()
                .filter(|s| project.allocations.iter().any(|a| a.stock_id == s.id))
                .count();
    p.line(format!(
        "{}: {}",
        label(&loc, "pdf-total"),
        show(if complete {
            material.and_then(|m| m.checked_add(fee?))
        } else {
            None
        })
    ));
    if !complete || material.is_none() || (cuts > 0 && fee.is_none()) {
        p.line(label(&loc, "pdf-incomplete"));
    }
    let bytes = doc
        .with_pages(p.finish())
        .save(&PdfSaveOptions::default(), &mut Vec::new());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Allocation, Board, Material, Project, Stock};
    use crate::export::{ExportSettings, prepare_export};
    use crate::money::{Currency, Money};
    use crate::units::{Pose, Quaternion};
    use eframe::egui;

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }

    fn preview_document() -> Document {
        Document {
            pages: (1..=6)
                .map(|number| Page {
                    number,
                    width_mm: document_layout::A4_WIDTH_MM,
                    height_mm: document_layout::A4_HEIGHT_MM,
                    primitives: Vec::new(),
                })
                .collect(),
            layout_version: document_layout::LAYOUT_VERSION,
            font_metrics_version: document_layout::FONT_METRICS_VERSION,
        }
    }

    fn preview_frame(
        ctx: &egui::Context,
        document: &Document,
        state: &mut DocumentPreviewState,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> (PreviewAreas, Vec<egui::output::OutputEvent>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        let mut areas = None;
        let mut output = ctx.run_ui(input, |ui| {
            areas = Some(
                show_document_preview_layout(
                    ui,
                    document,
                    state,
                    &DocumentPreviewLabels {
                        title: "Preview",
                        pages: "6 pages",
                        page: "Page",
                        zoom_in: "Zoom in",
                        zoom_out: "Zoom out",
                        fit: "Fit page",
                    },
                )
                .unwrap()
                .1,
            );
        });
        output.textures_delta.clear();
        (areas.unwrap(), output.platform_output.events)
    }

    #[test]
    fn preview_has_vertical_rail_and_separate_page_at_reference_and_small_sizes() {
        let doc = preview_document();
        for size in [egui::vec2(928.0, 760.0), egui::vec2(430.0, 360.0)] {
            let ctx = egui::Context::default();
            let (areas, _) = preview_frame(
                &ctx,
                &doc,
                &mut DocumentPreviewState::default(),
                size,
                vec![],
            );
            assert!(areas.rail.width() <= 112.1, "rail: {:?}", areas.rail);
            assert!(
                areas.rail.right() < areas.center.left(),
                "areas overlap: {:?} {:?}",
                areas.rail,
                areas.center
            );
            assert!(
                areas.center.right() <= size.x + 1.0,
                "center escapes canvas: {:?}",
                areas.center
            );
            assert!(areas.center.bottom() <= size.y + 1.0);
            assert_eq!(areas.thumbnails.len(), 6);
            for pair in areas.thumbnails.windows(2) {
                assert!(
                    pair[0].bottom() < pair[1].top(),
                    "thumbnails not stacked: {pair:?}"
                );
                assert!((pair[0].left() - pair[1].left()).abs() < 1.0);
            }
            assert!(
                areas
                    .thumbnails
                    .iter()
                    .all(|r| r.right() < areas.center.left())
            );
            assert!(areas.page.left() >= areas.center.left());
            assert!(areas.page.top() >= areas.center.top());
            assert!((areas.page.width() / areas.page.height() - 210.0 / 297.0).abs() < 0.001);
            assert!(
                areas.center.contains_rect(areas.page),
                "Fitted A4 not visible: {:?} {:?}",
                areas.page,
                areas.center
            );
        }
    }

    #[test]
    fn preview_thumbnail_selection_and_zoom_leave_frozen_pages_unchanged() {
        let doc = preview_document();
        let original = doc.clone();
        let ctx = egui::Context::default();
        let mut state = DocumentPreviewState::default();
        let size = egui::vec2(928.0, 760.0);
        let (areas, _) = preview_frame(&ctx, &doc, &mut state, size, vec![]);
        let pos = areas.thumbnails[1].center();
        let (_, events) = preview_frame(
            &ctx,
            &doc,
            &mut state,
            size,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(state.page, 1, "thumbnail click must select its page");
        assert!(events.iter().any(|event| matches!(event,
            egui::output::OutputEvent::Clicked(info)
                if info.label.as_deref() == Some("Page 2") && info.typ == egui::WidgetType::Button
        )), "thumbnail must expose a named accessible button: {events:?}");
        state.set_zoom(4.0).unwrap();
        let (zoomed, _) = preview_frame(&ctx, &doc, &mut state, size, vec![]);
        assert_eq!(zoomed.page.size(), egui::vec2(840.0, 1188.0));
        assert!(zoomed.rail.right() < zoomed.center.left());
        assert_eq!(state.page, 1);
        assert_eq!(doc, original, "zoom and selection cannot repaginate");
        assert!(!render_document_pdf(&doc).unwrap().is_empty());
    }

    #[test]
    fn small_preview_wheel_scrolls_the_frozen_page_and_thumbnail_rail_independently() {
        let doc = preview_document();
        let original = doc.clone();
        let ctx = egui::Context::default();
        let mut state = DocumentPreviewState::default();
        state.set_zoom(4.0).unwrap();
        let size = egui::vec2(430.0, 360.0);
        let (initial, _) = preview_frame(&ctx, &doc, &mut state, size, vec![]);
        let wheel = |pos: egui::Pos2| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -110.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        preview_frame(&ctx, &doc, &mut state, size, wheel(initial.center.center()));
        let (page_scroll, _) = preview_frame(&ctx, &doc, &mut state, size, vec![]);
        assert!(
            page_scroll.page.top() < initial.page.top(),
            "page did not scroll: center {:?} {:?}; page {:?} {:?}",
            initial.center,
            page_scroll.center,
            initial.page,
            page_scroll.page
        );
        assert_eq!(page_scroll.thumbnails[0].top(), initial.thumbnails[0].top());
        preview_frame(&ctx, &doc, &mut state, size, wheel(initial.rail.center()));
        let (rail_scroll, _) = preview_frame(&ctx, &doc, &mut state, size, vec![]);
        assert!(rail_scroll.thumbnails[0].top() < initial.thumbnails[0].top());
        assert_eq!(state.page, 0);
        assert_eq!(doc, original);
    }

    #[test]
    fn sheet_issue_and_price_notice_identify_alias_without_losing_uuid() {
        let prepared = fixture(2, Language::En, ExportMode::Draft);
        let project = prepared.snapshot.project();
        let id = project.stock[0].id;
        let loc = Localizer::new(Language::En);
        let sheet = issue(
            &ExportIssue::Sheet {
                id,
                name: project.stock[0].name.clone(),
                reason: crate::export::SheetIssue::BudgetExhausted,
            },
            project,
            &loc,
            Unit::Mm,
        );
        let price = issue(
            &ExportIssue::UnknownPrice { stock_id: Some(id) },
            project,
            &loc,
            Unit::Mm,
        );
        assert!(
            sheet.contains(&format!("S1 [{id}] · Chapa útil")),
            "{sheet}"
        );
        assert!(price.contains(&format!("S1 [{id}]")), "{price}");
    }

    fn fixture(part_count: usize, language: Language, mode: ExportMode) -> PreparedExport {
        let mut project = Project::new("Armário café", Currency::Brl);
        let material = Material {
            id: Uuid::new_v4(),
            name: "Compensado".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Length,
        };
        let stock = Stock {
            id: Uuid::new_v4(),
            name: "Chapa útil".into(),
            material_id: material.id,
            length: mm(part_count as i64 * 30 + (part_count as i64 - 1) * 5),
            width: mm(50),
            thickness: mm(18),
            grain: StockGrain::AlongX,
            source: StockSource::ToPurchase,
            price: Some(Money::new(Currency::Brl, 20_000).unwrap()),
            priority: 0,
            trim: [Length::ZERO; 4],
        };
        for i in 0..part_count {
            let board = Board {
                id: Uuid::new_v4(),
                name: format!("Prateleira longa número {} — ação", i + 1),
                material_id: material.id,
                length: mm(30),
                width: mm(50),
                thickness: mm(18),
                grain_override: None,
                parent_id: None,
                pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
            };
            project.allocations.push(Allocation {
                id: Uuid::new_v4(),
                board_id: board.id,
                stock_id: stock.id,
                origin: [mm(i as i64 * 35), Length::ZERO],
                quarter_turn: false,
                locked: false,
            });
            project.boards.push(board);
        }
        project.stock.push(stock);
        project.materials.push(material);
        project.confirmed_shop_kerf = Some(project.cutting_kerf);
        prepare_export(
            &project,
            ExportSettings {
                language,
                units: Unit::Mm,
            },
            mode,
        )
        .unwrap()
    }

    #[test]
    fn layout_pagination_repeats_context_draft_marks_and_preserves_portuguese_glyphs() {
        for language in [Language::En, Language::PtBr] {
            for (name, count) in [("simple", 2), ("dense", 28)] {
                let prepared = fixture(count, language, ExportMode::Draft);
                assert_eq!(prepared.witnesses[0].1.cut_count(), count - 1);
                let bytes = render_pdf(&prepared).unwrap();
                if let Ok(dir) = std::env::var("PMCAB_PDF_FIXTURES_DIR") {
                    std::fs::write(
                        std::path::Path::new(&dir).join(format!(
                            "pdf-{name}-{}.pdf",
                            if language == Language::En {
                                "en"
                            } else {
                                "pt-BR"
                            }
                        )),
                        &bytes,
                    )
                    .unwrap();
                }
                let path =
                    std::env::temp_dir().join(format!("pmcab-layout-{}.pdf", Uuid::new_v4()));
                std::fs::write(&path, bytes).unwrap();
                if let Ok(output) = std::process::Command::new("pdftotext")
                    .args(["-layout", path.to_str().unwrap(), "-"])
                    .output()
                {
                    assert!(output.status.success());
                    let text = String::from_utf8(output.stdout).unwrap();
                    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    let loc = Localizer::new(language);
                    let pages: Vec<_> = text
                        .split('\u{c}')
                        .filter(|s| !s.trim().is_empty())
                        .collect();
                    assert!(pages.len() >= 2);
                    for page in &pages {
                        assert!(page.contains(if language == Language::PtBr {
                            "RASCUNHO / NÃO USAR PARA CORTE"
                        } else {
                            "DRAFT / NOT FOR CUTTING"
                        }));
                    }
                    let layout_pages: Vec<_> = pages
                        .iter()
                        .filter(|page| page.contains(&label(&loc, "pdf-scale")))
                        .collect();
                    assert!(layout_pages.len() >= if count > 2 { 2 } else { 1 });
                    for page in layout_pages {
                        for key in ["pdf-sheet", "pdf-not-template", "pdf-cuts"] {
                            assert!(
                                page.split_whitespace()
                                    .collect::<Vec<_>>()
                                    .join(" ")
                                    .contains(&label(&loc, key)),
                                "{key}: {page}"
                            );
                        }
                        assert!(page.contains("1 mm"));
                    }
                    for i in 1..=count {
                        assert!(
                            normalized.contains(&format!("Prateleira longa número {i} — ação"))
                        );
                    }
                    assert!(text.contains("P0"));
                    assert!(text.contains(&label(&loc, "pdf-blade-strip")));
                    assert!(normalized.contains(&label(&loc, "pdf-from-retained")));
                }
                std::fs::remove_file(path).unwrap();
            }
        }
    }

    #[test]
    fn dense_sheet_callouts_map_every_cut_without_text_collisions() {
        for language in [Language::En, Language::PtBr] {
            let prepared = fixture(28, language, ExportMode::Draft);
            let tree = &prepared.witnesses[0].1;
            let stock = &prepared.snapshot.project().stock[0];
            assert_eq!(tree.cut_count(), 27);
            assert_eq!(stock.length, mm(975));
            let path = std::env::temp_dir().join(format!("pmcab-callouts-{}.pdf", Uuid::new_v4()));
            std::fs::write(&path, render_pdf(&prepared).unwrap()).unwrap();
            if let Ok(output) = std::process::Command::new("pdftotext")
                .args(["-bbox-layout", path.to_str().unwrap(), "-"])
                .output()
            {
                assert!(output.status.success());
                let xml = String::from_utf8(output.stdout).unwrap();
                let scale = 170. / 975.;
                let top = 132. + 50. * scale;
                // Poppler coordinates have their origin at the top of the page.
                let band_top = (297. - top - 60.) * 72. / 25.4;
                let band_bottom = (297. - top) * 72. / 25.4;
                let words: Vec<_> = xml
                    .split("<page ")
                    .map(|page| {
                        page.lines()
                            .filter(|line| line.contains("<word "))
                            .filter_map(|line| {
                                let value = line.split_once('>')?.1.split_once("</word>")?.0;
                                let attr = |key: &str| -> f32 {
                                    line.split_once(&format!("{key}=\""))
                                        .unwrap()
                                        .1
                                        .split_once('"')
                                        .unwrap()
                                        .0
                                        .parse()
                                        .unwrap()
                                };
                                let rect = [attr("xMin"), attr("yMin"), attr("xMax"), attr("yMax")];
                                (value.starts_with('C')
                                    && value[1..].parse::<usize>().is_ok()
                                    && rect[1] >= band_top
                                    && rect[3] <= band_bottom)
                                    .then_some((value.to_owned(), rect))
                            })
                            .collect::<Vec<_>>()
                    })
                    .max_by_key(Vec::len)
                    .unwrap();
                assert_eq!(words.len(), 27, "missing diagram callouts in {language:?}");
                for op in tree.operations() {
                    let id = format!("C{}", op.number);
                    let matches: Vec<_> = words.iter().filter(|(text, _)| text == &id).collect();
                    assert_eq!(matches.len(), 1, "{id} has no unique diagram label");
                    let rect = matches[0].1;
                    assert!(rect[0] >= 0. && rect[2] <= 595. && rect[1] >= 0. && rect[3] <= 842.);
                    let input = tree.node(op.input).unwrap().rectangle;
                    let first = tree.node(op.outputs.first).unwrap().rectangle;
                    let cut_x = if op.axis == Axis::X {
                        input.origin[0].micrometres() + first.extent[0].micrometres()
                    } else {
                        input.origin[0].micrometres() + input.extent[0].micrometres() / 2
                    };
                    let expected_x = (17. + cut_x as f32 / 1000. * scale) * 72. / 25.4;
                    assert!(
                        ((rect[0] + rect[2]) / 2. - expected_x).abs() < 8.,
                        "{id} not aligned to its cut"
                    );
                }
                for (i, (_, a)) in words.iter().enumerate() {
                    for (_, b) in words.iter().skip(i + 1) {
                        assert!(
                            a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1],
                            "overlapping callouts: {a:?} {b:?}"
                        );
                    }
                }
            }
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn bilingual_pdf_preserves_individual_parts_cuts_cost_and_embedded_font() {
        let mut project = Project::new("Armário café", Currency::Brl);
        let material = Material {
            id: Uuid::new_v4(),
            name: "Compensado".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Length,
        };
        let stock = Stock {
            id: Uuid::new_v4(),
            name: "Chapa útil".into(),
            material_id: material.id,
            length: mm(205),
            width: mm(50),
            thickness: mm(18),
            grain: StockGrain::AlongX,
            source: StockSource::ToPurchase,
            price: Some(Money::new(Currency::Brl, 20_000).unwrap()),
            priority: 0,
            trim: [Length::ZERO; 4],
        };
        for x in [0, 105] {
            let board = Board {
                id: Uuid::new_v4(),
                name: "Prateleira".into(),
                material_id: material.id,
                length: mm(100),
                width: mm(50),
                thickness: mm(18),
                grain_override: None,
                parent_id: None,
                pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
            };
            project.allocations.push(Allocation {
                id: Uuid::new_v4(),
                board_id: board.id,
                stock_id: stock.id,
                origin: [mm(x), Length::ZERO],
                quarter_turn: false,
                locked: false,
            });
            project.boards.push(board);
        }
        project.stock.push(stock);
        project.materials.push(material);
        project.cut_fee = Some(Money::new(Currency::Brl, 500).unwrap());
        project.confirmed_shop_kerf = Some(project.cutting_kerf);
        let prepared = prepare_export(
            &project,
            ExportSettings {
                language: Language::PtBr,
                units: Unit::Mm,
            },
            ExportMode::ShopReady,
        )
        .unwrap();
        assert_eq!(prepared.witnesses[0].1.cut_count(), 1);
        let bytes = render_pdf(&prepared).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        let path = std::env::temp_dir().join(format!("pmcab-pdf-{}.pdf", Uuid::new_v4()));
        std::fs::write(&path, &bytes).unwrap();
        if let Ok(output) = std::process::Command::new("pdftotext")
            .args(["-layout", path.to_str().unwrap(), "-"])
            .output()
        {
            assert!(output.status.success());
            let text = String::from_utf8(output.stdout).unwrap();
            for expected in [
                "Armário café",
                "Prateleira × 2",
                "100 mm",
                "205 mm",
                "C1 P0",
                "Cortes físicos: 1",
                "BRL 205,00",
                "Ferragens",
            ] {
                assert!(text.contains(expected), "missing {expected}: {text}");
            }
            for board in &project.boards {
                assert!(text.contains(&board.id.to_string()));
            }
        } else {
            // Verify the structural PDF objects on hosts without poppler.
            assert!(bytes.windows(5).any(|w| w == b"/Page"));
        }
        if let Ok(output) = std::process::Command::new("pdffonts").arg(&path).output() {
            let fonts = String::from_utf8(output.stdout).unwrap();
            assert!(fonts.contains("yes"), "{fonts}");
        } else {
            assert!(bytes.windows(9).any(|w| w == b"/FontFile"));
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn unresolved_draft_marks_every_page_and_withholds_unverified_installation() {
        let mut project = Project::new("Draft", Currency::Usd);
        let hardware = crate::domain::Hardware {
            id: Uuid::new_v4(),
            name: "Hinge".into(),
            parent_id: None,
            pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
            kind: HardwareKind::Catalog {
                catalog_id: Uuid::new_v4(),
            },
        };
        project.hardware.push(hardware);
        let prepared = prepare_export(
            &project,
            ExportSettings {
                language: Language::En,
                units: Unit::Inch,
            },
            ExportMode::Draft,
        )
        .unwrap();
        let bytes = render_pdf(&prepared).unwrap();
        let path = std::env::temp_dir().join(format!("pmcab-draft-{}.pdf", Uuid::new_v4()));
        std::fs::write(&path, &bytes).unwrap();
        if let Ok(output) = std::process::Command::new("pdftotext")
            .args([path.to_str().unwrap(), "-"])
            .output()
        {
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(text.contains("DRAFT / NOT FOR CUTTING"));
            assert!(
                text.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .contains("Installation dimensions unverified or invalid")
            );
            assert!(
                text.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .contains("numeric drilling diagram omitted")
            );
            assert!(!text.contains("Complete total: USD 0.00"));
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn dimensioned_foot_is_in_separate_hardware_section() {
        let mut project = Project::new("Feet", Currency::Usd);
        project.hardware.push(crate::domain::Hardware {
            id: Uuid::new_v4(),
            name: "Adjustable foot".into(),
            parent_id: None,
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            kind: HardwareKind::Placeholder {
                dimensions: [
                    Length::from_micrometres(30_000),
                    Length::from_micrometres(40_000),
                    Length::from_micrometres(100_000),
                ],
            },
        });
        let prepared = prepare_export(
            &project,
            ExportSettings {
                language: Language::En,
                units: Unit::Mm,
            },
            ExportMode::Draft,
        )
        .unwrap();
        let bytes = render_pdf(&prepared).unwrap();
        let path = std::env::temp_dir().join(format!("pmcab-foot-{}.pdf", Uuid::new_v4()));
        std::fs::write(&path, bytes).unwrap();
        if let Ok(output) = std::process::Command::new("pdftotext")
            .args([path.to_str().unwrap(), "-"])
            .output()
        {
            assert!(output.status.success());
            let text = String::from_utf8(output.stdout).unwrap();
            let section = text.split("Hardware").nth(1).unwrap();
            assert!(section.contains("Adjustable foot"));
            assert!(
                section.contains("30 mm")
                    && section.contains("40 mm")
                    && section.contains("100 mm"),
                "{section}"
            );
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn portuguese_packet_from_english_ui_localizes_explanations_and_preserves_names() {
        let mut project = fixture(2, Language::En, ExportMode::Draft)
            .snapshot
            .project()
            .clone();
        project.name = "My Cabinet".into();
        project.boards[0].name = "Shelf A".into();
        project.boards.push(crate::domain::Board {
            name: "Unplaced Shelf".into(),
            ..project.boards[0].duplicate()
        });
        project.stock[0].grain = StockGrain::Unknown;
        project.stock[0].price = None;
        let prepared = prepare_export(
            &project,
            crate::export::ExportSettings {
                language: Language::PtBr,
                units: Unit::Cm,
            },
            ExportMode::Draft,
        )
        .unwrap();
        let path = std::env::temp_dir().join(format!("pmcab-locale-{}.pdf", Uuid::new_v4()));
        std::fs::write(&path, render_pdf(&prepared).unwrap()).unwrap();
        if let Ok(output) = std::process::Command::new("pdftotext")
            .args([path.to_str().unwrap(), "-"])
            .output()
        {
            assert!(output.status.success());
            let text = String::from_utf8(output.stdout).unwrap();
            for word in [
                "My Cabinet",
                "Shelf A",
                "Unplaced Shelf",
                "Plano para a oficina",
                "RASCUNHO / NÃO USAR PARA CORTE",
                "Estoque",
                "sem alocação",
                "desconhecido",
                "cm",
            ] {
                assert!(
                    text.to_lowercase().contains(&word.to_lowercase()),
                    "missing {word}: {text}"
                );
            }
            for leak in [
                "MissingAllocation",
                "Unknown",
                "ToPurchase",
                "AlongX",
                "offcut",
                "waste",
                "unallocated",
                "installation diagram withheld",
            ] {
                assert!(!text.contains(leak), "untranslated {leak}: {text}");
            }
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn export_unit_options_format_same_physical_stock_without_changing_project() {
        let project = fixture(2, Language::En, ExportMode::ShopReady)
            .snapshot
            .project()
            .clone();
        let before = serde_json::to_vec(&project).unwrap();
        for (unit, expected) in [
            (Unit::Mm, "65 mm"),
            (Unit::Cm, "6,5 cm"),
            (Unit::M, "0,065 m"),
            (Unit::Inch, "2,559055 in"),
            (Unit::Foot, "0,213255 ft"),
        ] {
            let prepared = prepare_export(
                &project,
                crate::export::ExportSettings {
                    language: Language::PtBr,
                    units: unit,
                },
                ExportMode::ShopReady,
            )
            .unwrap();
            let path = std::env::temp_dir().join(format!("pmcab-units-{}.pdf", Uuid::new_v4()));
            std::fs::write(&path, render_pdf(&prepared).unwrap()).unwrap();
            if let Ok(output) = std::process::Command::new("pdftotext")
                .args([path.to_str().unwrap(), "-"])
                .output()
            {
                let text = String::from_utf8(output.stdout).unwrap();
                assert!(text.contains(expected), "{unit:?}: {text}");
            }
            std::fs::remove_file(path).unwrap();
        }
        assert_eq!(serde_json::to_vec(&project).unwrap(), before);
    }
}
