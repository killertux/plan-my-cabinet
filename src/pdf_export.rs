//! Offline, vector PDF representation of a prepared manufacturing snapshot.
use std::collections::BTreeMap;

use printpdf::{
    FontId, Line, LinePoint, Mm, Op, ParsedFont, PdfDocument, PdfPage, PdfSaveOptions, Point, Pt,
    TextItem,
};
use uuid::Uuid;

use crate::cut_tree::{Axis, CutKind, CutTree, Edge};
use crate::domain::{BoardGrain, HardwareKind, StockGrain, StockSource};
use crate::export::{ExportIssue, ExportMode, PreparedExport};
use crate::i18n::{Language, Localizer};
use crate::money::{Money, MoneyLocale};
use crate::units::{Length, Unit};

const FONT: &[u8] = include_bytes!("../assets/fonts/NotoSans-Regular.ttf");

#[derive(Debug)]
pub enum PdfExportError {
    Font,
}

fn label(loc: &Localizer, key: &str) -> String {
    loc.text(key)
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

fn issue(issue: &ExportIssue, loc: &Localizer, unit: Unit) -> String {
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
            "{name} [{id}]: {}",
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
            stock_id.map_or_else(|| loc.text("export-cut-fee"), |id| id.to_string())
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
        format!("{}: {}", label(loc, "pdf-sheet"), stock.name),
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
            p.line(issue(item, &loc, settings.units));
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
            "{} [{}] — {} — {} × {} × {}; {}; {}; {}: {}{}",
            stock.name,
            stock.id,
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
                        |a| a.stock_id.to_string()
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
            p.line(format!(
                "{} / {} — {} ({}; {}: {}; PDF: {})",
                r.product_id,
                r.plate_id,
                r.attribution,
                r.source,
                label(&loc, "pdf-printed-page"),
                r.printed_page,
                r.pdf_page
            ));
            p.line(format!(
                "{}: K={} / R={}; {}: Ø{} / {}; {}: H0, {} / {}",
                label(&loc, "pdf-supported-pair"),
                length(g.cup_edge_setback, settings.units, settings.language),
                length(g.overlay, settings.units, settings.language),
                label(&loc, "pdf-cup"),
                length(r.cup_diameter, settings.units, settings.language),
                length(r.cup_depth, settings.units, settings.language),
                label(&loc, "pdf-plate"),
                length(r.plate_hole_pitch, settings.units, settings.language),
                length(r.plate_front_offset, settings.units, settings.language)
            ));
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

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
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
