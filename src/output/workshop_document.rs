//! Frozen workshop packet assembled from prepared, checked evidence.

use std::collections::BTreeMap;

use uuid::Uuid;

use crate::cut_tree::{Axis, CutKind, CutTree, Edge, account_tree};
use crate::document_layout::{
    Document, DocumentBuilder, Ink, LayoutError, PageContext, Point, Rect, Stroke, TextStyle,
};
use crate::domain::{
    BoardEdge, BoardFace, BoardGrain, HardwareKind, Project, Stock, StockGrain, StockSource,
};
use crate::export::{
    ExportIssue, ExportMode, HardwareIssue, PreparedExport, ReceiptSections, SheetIssue,
};
use crate::i18n::{Language, Localizer};
use crate::money::{Money, MoneyLocale};
use crate::units::{Length, Unit};

#[derive(Debug, PartialEq, Eq)]
pub enum WorkshopDocumentError {
    Layout(LayoutError),
    EstimateOverflow,
}

impl From<LayoutError> for WorkshopDocumentError {
    fn from(value: LayoutError) -> Self {
        Self::Layout(value)
    }
}

fn translated(language: Language, en: &'static str, pt: &'static str) -> &'static str {
    if language == Language::PtBr { pt } else { en }
}

fn length(value: Length, unit: Unit, language: Language) -> String {
    let (scale, suffix) = match unit {
        Unit::Mm => (1_000., "mm"),
        Unit::Cm => (10_000., "cm"),
        Unit::M => (1_000_000., "m"),
        Unit::Inch => (25_400., "in"),
        Unit::Foot => (304_800., "ft"),
    };
    let formatted = format!("{:.6}", value.micrometres() as f64 / scale);
    let formatted = formatted.trim_end_matches('0').trim_end_matches('.');
    format!(
        "{} {suffix}",
        if language == Language::PtBr {
            formatted.replace('.', ",")
        } else {
            formatted.into()
        }
    )
}

fn money(value: Option<Money>, language: Language) -> String {
    value
        .map(|amount| {
            amount.display(if language == Language::PtBr {
                MoneyLocale::PortugueseBrazil
            } else {
                MoneyLocale::English
            })
        })
        .unwrap_or_else(|| translated(language, "unknown", "desconhecido").into())
}

fn stock_ref(project: &Project, id: Uuid) -> String {
    project.stock_alias(id).unwrap_or("?").to_owned()
}

/// Short, human part and hinge numbers used consistently across the packet:
/// the parts list, sheet diagrams, cut keys and issues all say `#3`, so the
/// shop can match a label on a sheet to its row without machine identifiers.
pub(crate) struct Labels {
    parts: BTreeMap<Uuid, usize>,
    hinges: BTreeMap<Uuid, usize>,
}

impl Labels {
    fn new(project: &Project) -> Self {
        let parts = part_groups(project)
            .into_values()
            .flatten()
            .enumerate()
            .map(|(index, board)| (board.id, index + 1))
            .collect();
        let hinges = project
            .hinge_installations
            .iter()
            .enumerate()
            .map(|(index, installation)| (installation.id, index + 1))
            .collect();
        Self { parts, hinges }
    }

    fn part(&self, id: Uuid) -> String {
        self.parts
            .get(&id)
            .map_or_else(|| "#?".into(), |n| format!("#{n}"))
    }

    fn named_part(&self, project: &Project, id: Uuid) -> String {
        project.boards.iter().find(|b| b.id == id).map_or_else(
            || self.part(id),
            |board| format!("{} {}", self.part(id), board.name),
        )
    }

    fn hinge(&self, project: &Project, loc: &Localizer, id: Uuid) -> String {
        let number = self.hinges.get(&id).copied().unwrap_or(0);
        let installation = project.hinge_installations.iter().find(|i| i.id == id);
        let name = |board: Option<Uuid>| {
            board
                .and_then(|b| project.boards.iter().find(|x| x.id == b))
                .map_or("—", |b| b.name.as_str())
        };
        format!(
            "{} {number} ({} › {})",
            loc.text("pdf-hinge-installation"),
            name(installation.map(|i| i.door_board_id)),
            name(installation.map(|i| i.mounting_board_id))
        )
    }
}

type PartKey = (String, Uuid, i64, i64, i64, u8);

/// Name keeps distinct design labels distinct; physical grouping additionally
/// requires identical material, finished dimensions and effective grain.
fn part_groups(project: &Project) -> BTreeMap<PartKey, Vec<&crate::domain::Board>> {
    let mut groups: BTreeMap<PartKey, Vec<_>> = BTreeMap::new();
    for board in &project.boards {
        let grain = project
            .materials
            .iter()
            .find(|m| m.id == board.material_id)
            .map(|m| board.effective_grain(m));
        groups
            .entry((
                board.name.clone(),
                board.material_id,
                board.length.micrometres(),
                board.width.micrometres(),
                board.thickness.micrometres(),
                match grain {
                    Some(BoardGrain::Length) => 0,
                    Some(BoardGrain::Width) => 1,
                    Some(BoardGrain::Unrestricted) => 2,
                    None => 3,
                },
            ))
            .or_default()
            .push(board);
    }
    groups
}

fn issue_text(
    issue: &ExportIssue,
    project: &Project,
    labels: &Labels,
    loc: &Localizer,
    unit: Unit,
) -> String {
    match issue {
        ExportIssue::Board {
            id,
            name,
            stock_id,
            reasons,
        } => format!(
            "{} {name} — {}{}",
            labels.part(*id),
            reasons
                .iter()
                .map(|reason| loc.text(reason.key()))
                .collect::<Vec<_>>()
                .join(", "),
            stock_id.map_or_else(String::new, |id| format!(
                " · {}: {}",
                loc.text("pdf-stock"),
                stock_ref(project, id)
            ))
        ),
        ExportIssue::Sheet { id, name, reason } => format!(
            "{} · {name}: {}",
            stock_ref(project, *id),
            loc.text(if matches!(reason, SheetIssue::BudgetExhausted) {
                "sheet-feasibility-unknown"
            } else {
                "sheet-cut-conflict"
            })
        ),
        ExportIssue::KerfUnconfirmed(value) => format!(
            "{}: {}",
            loc.text("export-kerf-unconfirmed"),
            length(*value, unit, loc.language())
        ),
        ExportIssue::UnknownPrice { stock_id } => format!(
            "{}: {}",
            loc.text("export-price-unknown"),
            stock_id.map_or_else(|| loc.text("export-cut-fee"), |id| stock_ref(project, id))
        ),
        ExportIssue::Hardware { name, reason, .. } => format!(
            "{name}: {} — {}",
            loc.text(match reason {
                HardwareIssue::MissingCatalog(_) => "pdf-hardware-missing-catalog",
                HardwareIssue::UnverifiedInstallation => "pdf-hardware-unverified",
                HardwareIssue::InvalidReference => "pdf-hardware-invalid",
            }),
            loc.text("pdf-installation-withheld")
        ),
        ExportIssue::Installation { id, reason, .. } => format!(
            "{}: {} — {}",
            labels.hinge(project, loc, *id),
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
            installation_id, ..
        } => format!(
            "{}: {} — {}",
            labels.hinge(project, loc, *installation_id),
            loc.text("pdf-joint-review"),
            loc.text("pdf-installation-withheld")
        ),
        ExportIssue::InvalidWood(_) => loc.text("pdf-invalid-wood"),
    }
}

struct Estimate {
    cuts: Option<usize>,
    material: Option<Money>,
    cutting: Option<Money>,
    total: Option<Money>,
}

/// Uses only witnesses already verified by preparation; never estimates cuts
/// from a conflicting or proof-exhausted sheet. Unknown amounts stay unknown.
fn estimate(prepared: &PreparedExport) -> Result<Estimate, WorkshopDocumentError> {
    let project = prepared.snapshot.project();
    let used: Vec<_> = project
        .stock
        .iter()
        .filter(|stock| project.allocations.iter().any(|a| a.stock_id == stock.id))
        .collect();
    let zero =
        Money::new(project.currency, 0).map_err(|_| WorkshopDocumentError::EstimateOverflow)?;
    let mut material = Some(zero);
    for stock in &used {
        let purchase = if stock.source == StockSource::Owned {
            Some(zero)
        } else {
            stock.price
        };
        material = material
            .zip(purchase)
            .map(|(sum, cost)| sum.checked_add(cost))
            .transpose()
            .map_err(|_| WorkshopDocumentError::EstimateOverflow)?;
    }
    let cuts = (prepared.witnesses.len() == used.len()).then(|| {
        prepared
            .witnesses
            .iter()
            .map(|(_, tree)| tree.cut_count())
            .sum::<usize>()
    });
    let cutting = match cuts {
        Some(0) => Some(zero),
        Some(count) => project
            .cut_fee
            .map(|fee| fee.checked_mul(count as u64))
            .transpose()
            .map_err(|_| WorkshopDocumentError::EstimateOverflow)?,
        None => None,
    };
    let complete = prepared.wood_issues.is_empty()
        && project.allocations.len() == project.boards.len()
        && cuts.is_some();
    let total = if complete {
        material
            .zip(cutting)
            .map(|(a, b)| a.checked_add(b))
            .transpose()
            .map_err(|_| WorkshopDocumentError::EstimateOverflow)?
    } else {
        None
    };
    Ok(Estimate {
        cuts,
        material,
        cutting,
        total,
    })
}

fn grain_name(grain: BoardGrain, language: Language) -> &'static str {
    match grain {
        BoardGrain::Length => translated(language, "along length", "no comprimento"),
        BoardGrain::Width => translated(language, "along width", "na largura"),
        BoardGrain::Unrestricted => translated(language, "unrestricted", "livre"),
    }
}

fn edge(axis: Axis, side: Edge) -> &'static str {
    match (axis, side) {
        (Axis::X, Edge::Low) => "X-",
        (Axis::X, Edge::High) => "X+",
        (Axis::Y, Edge::Low) => "Y-",
        (Axis::Y, Edge::High) => "Y+",
    }
}

/// Saw-cut strip fill in sheet drawings.
pub const KERF_BAND: Ink = Ink {
    red: 220,
    green: 220,
    blue: 220,
};
/// Background of a cut label tag, so it stays readable over lines.
const TAG_FILL: Ink = Ink {
    red: 255,
    green: 255,
    blue: 255,
};

fn sheet_pages(
    builder: &mut DocumentBuilder,
    tree: &CutTree,
    stock: &Stock,
    project: &Project,
    labels: &Labels,
    loc: &Localizer,
    unit: Unit,
) -> Result<(), WorkshopDocumentError> {
    let language = loc.language();
    builder.set_continuation(Vec::new());
    builder.page_break()?;
    let sheet = format!(
        "{}: {} · {}",
        loc.text("pdf-sheet"),
        stock_ref(project, stock.id),
        stock.name
    );
    builder.section(&sheet)?;
    // A diagram has a fixed, bounded footprint; the legend and sequence flow
    // independently, with this context repeated after each page break.
    let root = tree
        .node(tree.root())
        .expect("verified witness root")
        .rectangle;
    let actual_x = root.extent[0].micrometres() as f64 / 1000.0;
    let actual_y = root.extent[1].micrometres() as f64 / 1000.0;
    let scale = (166.0 / actual_x).min(105.0 / actual_y);
    let scale_label = format!(
        "{}: 1 mm {} = {} mm {}; {}",
        loc.text("pdf-scale"),
        loc.text("pdf-on-paper"),
        if language == Language::PtBr {
            format!("{:.4}", 1.0 / scale).replace('.', ",")
        } else {
            format!("{:.4}", 1.0 / scale)
        },
        loc.text("pdf-actual"),
        loc.text("pdf-not-template")
    );
    let kerf_label = format!(
        "{}: {} · {}: {}",
        loc.text("pdf-kerf"),
        length(tree.kerf(), unit, language),
        loc.text("pdf-cuts"),
        tree.cut_count()
    );
    let trim_label = format!(
        "{} (X- / X+ / Y- / Y+): {} / {} / {} / {} ({})",
        loc.text("pdf-trim"),
        length(stock.trim[0], unit, language),
        length(stock.trim[1], unit, language),
        length(stock.trim[2], unit, language),
        length(stock.trim[3], unit, language),
        translated(
            language,
            "edge loss includes blade strip",
            "perda na borda inclui a faixa da serra"
        )
    );
    let accounting =
        account_tree(tree, stock.trim).expect("prepared witness was independently verified");
    let area = |value: i128| {
        let whole = value / 1_000_000;
        let fraction = value % 1_000_000;
        let number = if fraction == 0 {
            whole.to_string()
        } else {
            format!("{whole}.{fraction:06}")
                .trim_end_matches('0')
                .to_owned()
        };
        if language == Language::PtBr {
            number.replace('.', ",")
        } else {
            number
        }
    };
    let accounting_label = format!(
        "{}: {} mm² · {}: {} mm²",
        translated(language, "Trim loss", "Perda por refilo"),
        area(accounting.trim_loss),
        translated(language, "Other blade loss", "Outras perdas da serra"),
        area(accounting.kerf_loss)
    );
    let context = vec![scale_label, kerf_label, trim_label, accounting_label];
    for line in &context {
        builder.paragraph(line)?;
    }
    builder.set_continuation(context);

    // Labels are laid out in drawing coordinates first, then drawn. Part
    // numbers go inside their part; each cut label sits on its own cut line as
    // a small tag, moved along the line to avoid other labels. Only a label
    // with no free spot falls back to a keyed row below the drawing.
    let drawing_height = (actual_y * scale) as f32;
    let caption = TextStyle::CAPTION.leading_mm;
    let at = |value: crate::units::Length| (value.micrometres() as f64 / 1000.0 * scale) as f32;
    let local = |r: crate::cut_tree::Rectangle| Rect {
        x: at(r.origin[0]),
        y: at(r.origin[1]),
        width: at(r.extent[0]),
        height: at(r.extent[1]),
    };
    let overlaps = |a: &Rect, b: &Rect| {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    };
    let sheet = local(root);
    // The grain arrow occupies the sheet's top-left corner.
    let mut taken: Vec<Rect> = match stock.grain {
        StockGrain::AlongX => vec![Rect {
            x: 0.0,
            y: 0.0,
            width: 17.0,
            height: 4.0,
        }],
        StockGrain::AlongY => vec![Rect {
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 17.0,
        }],
        _ => Vec::new(),
    };
    let mut part_labels = Vec::new();
    for node in tree.nodes() {
        if let CutKind::Part(board_id) = node.kind {
            let part = local(node.rectangle);
            let label = labels.part(board_id);
            let width = builder.measure(&label, TextStyle::CAPTION)? + 0.1;
            let y = part.y + (part.height - caption).clamp(0.0, 0.4);
            let spot = [part.x + 0.8, part.x + 18.0, part.x + 0.8]
                .into_iter()
                .zip([y, y, part.y + 4.4])
                .map(|(x, y)| Rect {
                    x,
                    y,
                    width,
                    height: caption,
                })
                .find(|spot| {
                    spot.x + spot.width <= part.x + part.width - 0.4
                        && spot.y + spot.height <= part.y + part.height
                        && !taken.iter().any(|other| overlaps(spot, other))
                });
            if let Some(spot) = spot {
                taken.push(spot);
                part_labels.push((label, spot));
            }
        }
    }
    let mut operations = tree.operations();
    operations.sort_by_key(|op| op.number);
    let mut bands = Vec::new();
    let mut tags = Vec::new();
    let mut unplaced = Vec::new();
    for op in &operations {
        let input = local(tree.node(op.input).expect("verified input").rectangle);
        let first = local(
            tree.node(op.outputs.first)
                .expect("verified first output")
                .rectangle,
        );
        let band = match op.axis {
            Axis::X => Rect {
                x: first.x + first.width,
                y: input.y,
                width: at(tree.kerf()),
                height: input.height,
            },
            Axis::Y => Rect {
                x: input.x,
                y: first.y + first.height,
                width: input.width,
                height: at(tree.kerf()),
            },
        };
        bands.push(band);
        let label = format!("C{}", op.number);
        let label_width = builder.measure(&label, TextStyle::CAPTION)? + 0.2;
        let (w, h) = (label_width + 1.0, caption);
        let spot = [0.5, 0.3, 0.7, 0.15, 0.85, 0.4, 0.6]
            .into_iter()
            .map(|t| match op.axis {
                Axis::X => Rect {
                    x: band.x + band.width / 2.0 - w / 2.0,
                    y: band.y + band.height * t - h / 2.0,
                    width: w,
                    height: h,
                },
                Axis::Y => Rect {
                    x: band.x + band.width * t - w / 2.0,
                    y: band.y + band.height / 2.0 - h / 2.0,
                    width: w,
                    height: h,
                },
            })
            .find(|spot| {
                spot.x >= sheet.x
                    && spot.y >= sheet.y
                    && spot.x + spot.width <= sheet.x + sheet.width
                    && spot.y + spot.height <= sheet.y + sheet.height
                    && !taken.iter().any(|other| overlaps(spot, other))
            });
        match spot {
            Some(spot) => {
                taken.push(spot);
                tags.push((label, label_width, spot));
            }
            None => unplaced.push((
                label,
                label_width,
                Point {
                    x: band.x + band.width / 2.0,
                    y: band.y + band.height / 2.0,
                },
            )),
        }
    }
    let rows = unplaced.len().div_ceil(5);
    let height = 12.0
        + drawing_height
        + if rows == 0 {
            0.0
        } else {
            rows as f32 * 4.5 + 7.0
        };
    let frame = builder.diagram(height)?;
    let x0 = frame.x + 5.0;
    let y0 = frame.y + 5.0;
    let place = |r: Rect| Rect {
        x: x0 + r.x,
        y: y0 + r.y,
        ..r
    };
    builder.box_at(place(sheet), Some(Stroke::STANDARD), None)?;
    let arrow = match stock.grain {
        StockGrain::AlongX => Some((
            Point {
                x: x0 + 2.0,
                y: y0 + 2.0,
            },
            Point {
                x: x0 + 15.0,
                y: y0 + 2.0,
            },
        )),
        StockGrain::AlongY => Some((
            Point {
                x: x0 + 2.0,
                y: y0 + 2.0,
            },
            Point {
                x: x0 + 2.0,
                y: y0 + 15.0,
            },
        )),
        _ => None,
    };
    if let Some((a, b)) = arrow {
        builder.path(vec![a, b], false)?;
        builder.path(
            vec![
                Point {
                    x: b.x - 1.5,
                    y: b.y - 1.5,
                },
                b,
                Point {
                    x: b.x + 1.5,
                    y: b.y + 1.5,
                },
            ],
            false,
        )?;
    }
    for node in tree.nodes() {
        if let CutKind::Part(_) = node.kind {
            builder.box_at(place(local(node.rectangle)), Some(Stroke::STANDARD), None)?;
        }
    }
    for (label, spot) in &part_labels {
        let spot = place(*spot);
        builder.label_at(label, TextStyle::CAPTION, spot.x, spot.y, spot.width)?;
    }
    // Very narrow physical kerfs remain exact in the model, not inflated to a
    // display-only minimum.
    for band in &bands {
        builder.box_at(place(*band), None, Some(KERF_BAND))?;
    }
    let tag_stroke = Stroke {
        ink: Ink {
            red: 150,
            green: 150,
            blue: 150,
        },
        width_mm: 0.12,
    };
    for (label, label_width, spot) in &tags {
        let spot = place(*spot);
        builder.box_at(spot, Some(tag_stroke), Some(TAG_FILL))?;
        builder.label_at(
            label,
            TextStyle::CAPTION,
            spot.x + 0.5,
            spot.y,
            *label_width,
        )?;
    }
    for (i, (label, label_width, centre)) in unplaced.iter().enumerate() {
        let label_x = frame.x + 3.0 + (i % 5) as f32 * 35.0;
        let label_y = y0 + drawing_height + 3.0 + (i / 5) as f32 * 4.5;
        builder.path_styled(
            vec![
                Point {
                    x: x0 + centre.x,
                    y: y0 + centre.y,
                },
                Point {
                    x: label_x + label_width / 2.0,
                    y: label_y,
                },
            ],
            false,
            Stroke {
                ink: Ink {
                    red: 130,
                    green: 130,
                    blue: 130,
                },
                width_mm: 0.12,
            },
        )?;
        builder.label_at(label, TextStyle::CAPTION, label_x, label_y, *label_width)?;
    }
    builder.paragraph(&format!(
        "{} × {}",
        length(stock.length, unit, language),
        length(stock.width, unit, language)
    ))?;
    builder.paragraph(&loc.text("pdf-part-key"))?;
    for (id, node) in tree.nodes().iter().enumerate() {
        if let CutKind::Part(board_id) = node.kind {
            builder.paragraph(&format!(
                "{} (P{id}): {} × {}",
                labels.named_part(project, board_id),
                length(node.rectangle.extent[0], unit, language),
                length(node.rectangle.extent[1], unit, language)
            ))?;
        }
    }
    builder.paragraph(&loc.text("pdf-cut-key"))?;
    for op in operations {
        builder.paragraph(&format!(
            "C{} P{}: {} {} -> P{} {} (P{}, P{}); {} {} {} P{}: {}",
            op.number,
            op.input,
            edge(op.axis, op.reference_edge),
            length(op.retained_extent, unit, language),
            op.retained_output,
            loc.text("pdf-retained"),
            op.outputs.first,
            op.outputs.second,
            loc.text("pdf-blade-strip"),
            edge(op.axis, op.kerf_side),
            loc.text("pdf-from-retained"),
            op.retained_output,
            length(tree.kerf(), unit, language)
        ))?;
    }
    for (id, node) in tree.nodes().iter().enumerate() {
        let kind = match node.kind {
            CutKind::Part(board) => labels.named_part(project, board),
            CutKind::Offcut => loc.text("pdf-offcut"),
            CutKind::Waste => loc.text("pdf-waste"),
            CutKind::Split { .. } => continue,
        };
        builder.paragraph(&format!(
            "P{id} {kind}: {} × {}",
            length(node.rectangle.extent[0], unit, language),
            length(node.rectangle.extent[1], unit, language)
        ))?;
    }
    Ok(())
}

fn hardware_pages(
    builder: &mut DocumentBuilder,
    prepared: &PreparedExport,
    labels: &Labels,
    loc: &Localizer,
    unit: Unit,
) -> Result<(), WorkshopDocumentError> {
    let project = prepared.snapshot.project();
    let language = loc.language();
    builder.section(&loc.text("pdf-hardware"))?;
    for hardware in &project.hardware {
        let description = match &hardware.kind {
            HardwareKind::Placeholder { dimensions } => format!(
                "{} — {} × {} × {}",
                loc.text("pdf-reference-hardware"),
                length(dimensions[0], unit, language),
                length(dimensions[1], unit, language),
                length(dimensions[2], unit, language)
            ),
            HardwareKind::Catalog { catalog_id } => project
                .catalog
                .iter()
                .find(|c| c.id == *catalog_id)
                .map_or_else(
                    || format!("{} {}", loc.text("pdf-catalog"), loc.text("pdf-missing")),
                    |c| {
                        format!(
                            "{} / {} / {}",
                            c.product_id,
                            c.plate_id.as_deref().unwrap_or("—"),
                            c.revision
                        )
                    },
                ),
        };
        builder.paragraph(&format!("{} — {description}", hardware.name))?;
        if prepared
            .withheld_installation_guidance
            .contains(&hardware.id)
        {
            withheld_reason(builder, prepared, labels, hardware.id, loc, unit)?;
        }
    }
    for installation in &project.hinge_installations {
        builder.paragraph(&labels.hinge(project, loc, installation.id))?;
        let guidance = prepared
            .installation_guidance
            .iter()
            .find(|g| g.id == installation.id);
        if prepared
            .withheld_installation_guidance
            .contains(&installation.id)
            || guidance.is_none()
        {
            withheld_reason(builder, prepared, labels, installation.id, loc, unit)?;
            continue;
        }
        let g = guidance.expect("checked above");
        let r = &g.references;
        builder.paragraph(&format!(
            "{} / {} — {} ({}; {}: {}; PDF: {})",
            r.product_id,
            r.plate_id,
            r.attribution,
            r.source,
            loc.text("pdf-printed-page"),
            r.printed_page,
            r.pdf_page
        ))?;
        builder.paragraph(&format!(
            "{}: K={} / R={}; {}: Ø{} / {}; {}: H0, {} / {}",
            loc.text("pdf-supported-pair"),
            length(g.cup_edge_setback, unit, language),
            length(g.overlay, unit, language),
            loc.text("pdf-cup"),
            length(r.cup_diameter, unit, language),
            length(r.cup_depth, unit, language),
            loc.text("pdf-plate"),
            length(r.plate_hole_pitch, unit, language),
            length(r.plate_front_offset, unit, language)
        ))?;
        let coord = |values: [i128; 3]| -> Result<String, WorkshopDocumentError> {
            values
                .iter()
                .map(|v| {
                    i64::try_from(*v)
                        .map(|n| length(Length::from_micrometres(n), unit, language))
                        .map_err(|_| WorkshopDocumentError::Layout(LayoutError::InvalidGeometry))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|v| v.join(" / "))
        };
        let face = |f: BoardFace| {
            loc.text(match f {
                BoardFace::MinZ => "pdf-face-min-z",
                BoardFace::MaxZ => "pdf-face-max-z",
            })
        };
        let edge_name = |e: BoardEdge| match e {
            BoardEdge::MinX => "X-",
            BoardEdge::MaxX => "X+",
        };
        builder.paragraph(&format!(
            "{}: {} {} · {} {}: {} ({})",
            loc.text("pdf-cup-center"),
            coord(r.cup_center_um)?,
            face(r.cup_face),
            loc.text("pdf-cup-edge"),
            edge_name(installation.side.door_edge),
            length(g.cup_edge_setback, unit, language),
            loc.text("pdf-board-local")
        ))?;
        for hole in r.plate_hole_centers_um {
            builder.paragraph(&format!(
                "{}: {} {} · {} {} ({})",
                loc.text("pdf-plate-center"),
                coord(hole)?,
                face(r.plate_face),
                translated(language, "mount front edge", "borda frontal do suporte"),
                edge_name(installation.side.mount_front_edge),
                loc.text("pdf-board-local")
            ))?;
        }
        builder.notice(&loc.text("pdf-fasteners-unavailable"))?;
    }
    Ok(())
}

fn withheld_reason(
    builder: &mut DocumentBuilder,
    prepared: &PreparedExport,
    labels: &Labels,
    id: Uuid,
    loc: &Localizer,
    unit: Unit,
) -> Result<(), WorkshopDocumentError> {
    let reasons: Vec<_> = prepared
        .notices
        .iter()
        .filter(|issue| match issue {
            ExportIssue::Hardware { id: item, .. } | ExportIssue::Installation { id: item, .. } => {
                *item == id
            }
            ExportIssue::JointNeedsReview {
                installation_id, ..
            } => *installation_id == id,
            _ => false,
        })
        .map(|issue| issue_text(issue, prepared.snapshot.project(), labels, loc, unit))
        .collect();
    if reasons.is_empty() {
        builder.notice(&format!(
            "{} — {}",
            loc.text("pdf-installation-withheld"),
            translated(
                loc.language(),
                "validated references unavailable",
                "referências validadas indisponíveis"
            )
        ))?;
    } else {
        for reason in reasons {
            builder.notice(&reason)?;
        }
    }
    Ok(())
}

/// Builds the shared pages from a frozen preparation and explicit section
/// selection. The existing PDF path still uses its own layout until task 11.4.
pub fn build_workshop_document(
    prepared: &PreparedExport,
    sections: ReceiptSections,
) -> Result<Document, WorkshopDocumentError> {
    let project = prepared.snapshot.project();
    let settings = prepared.snapshot.settings();
    let language = settings.language;
    let loc = Localizer::new(language);
    let estimate = estimate(prepared)?;
    let labels = Labels::new(project);
    let draft = prepared.mode == ExportMode::Draft;
    let mut notices: Vec<String> = prepared
        .wood_issues
        .iter()
        .chain(&prepared.notices)
        .map(|issue| issue_text(issue, project, &labels, &loc, settings.units))
        .collect();
    let has_hinges = !project.hinge_installations.is_empty()
        || !project.door_joints.is_empty()
        || project
            .hardware
            .iter()
            .any(|h| matches!(h.kind, crate::domain::HardwareKind::Catalog { .. }));
    if has_hinges {
        notices.push(loc.text("pdf-hinge-review-warning"));
        notices.push(loc.text("pdf-motion-approximate"));
    }
    if estimate.total.is_none() {
        notices.push(loc.text("pdf-incomplete"));
    }
    notices.push(translated(
        language,
        "Estimate excludes taxes, freight, preparation fees and discounts; verify amounts with the shop.",
        "A estimativa não inclui impostos, frete, taxas de preparo ou descontos; confirme os valores com a oficina.",
    ).into());
    let omitted: Vec<_> = [
        (
            !sections.parts_and_costs,
            translated(language, "parts list & costs", "lista de peças e custos"),
        ),
        (
            !sections.sheets_and_cut_steps,
            translated(
                language,
                "sheet diagrams + cut steps",
                "desenhos das chapas + sequência de cortes",
            ),
        ),
        (
            !sections.hinge_references,
            translated(language, "hinge references", "referências das dobradiças"),
        ),
    ]
    .into_iter()
    .filter_map(|(off, name)| off.then_some(name))
    .collect();
    if !omitted.is_empty() {
        notices.push(format!(
            "{}: {}",
            translated(
                language,
                "Optional detail omitted",
                "Detalhes opcionais omitidos"
            ),
            omitted.join(", ")
        ));
    }
    let mut builder = DocumentBuilder::new(PageContext {
        project: project.name.clone(),
        revision: format!(
            "{} {}",
            loc.text("pdf-revision"),
            prepared.snapshot.revision()
        ),
        packet: format!(
            "{} · {}",
            loc.text("pdf-packet"),
            translated(
                language,
                if draft { "Draft" } else { "Shop-ready" },
                if draft {
                    "Rascunho"
                } else {
                    "Pronto para oficina"
                }
            )
        ),
        stamp: draft.then(|| {
            translated(
                language,
                "DRAFT / NOT FOR CUTTING",
                "RASCUNHO / NÃO USAR PARA CORTE",
            )
            .to_owned()
        }),
        safety: loc.text("pdf-not-template"),
        notices: Vec::new(),
    })?;
    builder.section(&format!("{} — {}", project.name, loc.text("pdf-packet")))?;
    // The project ID appears once so a printed packet can be traced back to
    // its file and export receipt; everything else uses short labels.
    builder.paragraph(&format!(
        "{}: {} · {}: {} · {}: {} · ID: {}",
        loc.text("pdf-project"),
        project.name,
        loc.text("pdf-revision"),
        prepared.snapshot.revision(),
        loc.text("pdf-currency"),
        project.currency.code(),
        project.id
    ))?;
    builder.paragraph(&format!(
        "{}: {} · {}: {} · {}: {}",
        loc.text("pdf-units"),
        match settings.units {
            Unit::Mm => "mm",
            Unit::Cm => "cm",
            Unit::M => "m",
            Unit::Inch => "in",
            Unit::Foot => "ft",
        },
        translated(language, "Language", "Idioma"),
        if language == Language::PtBr {
            "pt-BR"
        } else {
            "en"
        },
        loc.text("pdf-kerf"),
        length(project.cutting_kerf, settings.units, language)
    ))?;
    builder.paragraph(&format!(
        "{}: {}",
        loc.text("pdf-assumptions"),
        loc.text("pdf-assumptions-detail")
    ))?;
    if !notices.is_empty() {
        builder.section(&loc.text("pdf-issues"))?;
        for notice in notices {
            builder.notice(&notice)?;
        }
    }
    builder.paragraph(&format!(
        "{}: {} · {}: {} · {}: {} · {}: {}",
        translated(language, "Materials", "Materiais"),
        project.materials.len(),
        loc.text("pdf-stock"),
        project.stock.len(),
        loc.text("pdf-parts"),
        project.boards.len(),
        loc.text("pdf-cuts"),
        estimate
            .cuts
            .map_or_else(|| loc.text("pdf-missing"), |n| n.to_string())
    ))?;
    // Scope survives every optional toggle, including all-off and empty input.
    let scope = project
        .materials
        .iter()
        .map(|material| {
            let aliases = project
                .stock
                .iter()
                .filter(|s| s.material_id == material.id)
                .map(|s| project.stock_alias(s.id).unwrap_or("?"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{} {} ({})",
                material.name,
                length(material.default_thickness, settings.units, language),
                if aliases.is_empty() { "—" } else { &aliases }
            )
        })
        .collect::<Vec<_>>()
        .join(" · ");
    if !scope.is_empty() {
        builder.paragraph(&format!(
            "{}: {scope}",
            translated(language, "Materials and sheets", "Materiais e chapas")
        ))?;
    }
    if sections.parts_and_costs {
        builder.section(&loc.text("pdf-stock"))?;
        builder.table(
            &[
                translated(language, "Material", "Material").into(),
                translated(language, "Thickness / grain", "Espessura / veio").into(),
                translated(language, "Stock pieces", "Chapas").into(),
            ],
            &project
                .materials
                .iter()
                .map(|m| {
                    vec![
                        m.name.clone(),
                        format!(
                            "{} · {}",
                            length(m.default_thickness, settings.units, language),
                            grain_name(m.default_grain, language)
                        ),
                        project
                            .stock
                            .iter()
                            .filter(|s| s.material_id == m.id)
                            .count()
                            .to_string(),
                    ]
                })
                .collect::<Vec<_>>(),
        )?;
        builder.table(
            &[
                translated(language, "Stock / alias", "Chapa / código").into(),
                translated(language, "Material / dimensions", "Material / medidas").into(),
                translated(language, "Source / grain / price", "Origem / veio / preço").into(),
            ],
            &project
                .stock
                .iter()
                .map(|s| {
                    let material = project
                        .materials
                        .iter()
                        .find(|m| m.id == s.material_id)
                        .map_or_else(|| loc.text("pdf-missing"), |m| m.name.clone());
                    vec![
                        format!("{} · {}", project.stock_alias(s.id).unwrap_or("?"), s.name),
                        format!(
                            "{material} · {} × {} × {}",
                            length(s.length, settings.units, language),
                            length(s.width, settings.units, language),
                            length(s.thickness, settings.units, language)
                        ),
                        format!(
                            "{} · {} · {}",
                            loc.text(if s.source == StockSource::Owned {
                                "stock-owned"
                            } else {
                                "stock-purchase"
                            }),
                            loc.text(match s.grain {
                                crate::domain::StockGrain::AlongX => "stock-grain-x",
                                crate::domain::StockGrain::AlongY => "stock-grain-y",
                                crate::domain::StockGrain::Nondirectional => "stock-grain-none",
                                crate::domain::StockGrain::Unknown => "stock-grain-unknown",
                            }),
                            if s.source == StockSource::Owned {
                                loc.text("stock-owned")
                            } else {
                                money(s.price, language)
                            }
                        ),
                    ]
                })
                .collect::<Vec<_>>(),
        )?;
        for stock in project
            .stock
            .iter()
            .filter(|s| s.trim.iter().any(|t| t.micrometres() != 0))
        {
            builder.paragraph(&format!(
                "{} · {} (X- / X+ / Y- / Y+): {} / {} / {} / {}",
                stock_ref(project, stock.id),
                loc.text("pdf-trim"),
                length(stock.trim[0], settings.units, language),
                length(stock.trim[1], settings.units, language),
                length(stock.trim[2], settings.units, language),
                length(stock.trim[3], settings.units, language)
            ))?;
        }
        builder.section(&loc.text("pdf-parts"))?;
        let groups = part_groups(project);
        let rows = groups
            .iter()
            .map(|((name, material_id, l, w, t, grain), boards)| {
                let numbers = boards
                    .iter()
                    .map(|b| labels.part(b.id))
                    .collect::<Vec<_>>()
                    .join(", ");
                vec![
                    format!("{numbers} {name} × {}", boards.len()),
                    format!(
                        "{} · {}",
                        project
                            .materials
                            .iter()
                            .find(|m| m.id == *material_id)
                            .map_or("?", |m| m.name.as_str()),
                        match grain {
                            0 => grain_name(BoardGrain::Length, language),
                            1 => grain_name(BoardGrain::Width, language),
                            2 => grain_name(BoardGrain::Unrestricted, language),
                            _ => translated(language, "unknown", "desconhecido"),
                        }
                    ),
                    format!(
                        "{} × {} × {}",
                        length(Length::from_micrometres(*l), settings.units, language),
                        length(Length::from_micrometres(*w), settings.units, language),
                        length(Length::from_micrometres(*t), settings.units, language)
                    ),
                ]
            })
            .collect::<Vec<_>>();
        builder.table(
            &[
                translated(language, "Part / quantity", "Peça / quantidade").into(),
                translated(language, "Material / grain", "Material / veio").into(),
                translated(language, "Finished dimensions", "Medidas finais").into(),
            ],
            &rows,
        )?;
        let placements = groups
            .values()
            .flatten()
            .map(|board| {
                let allocation = project.allocations.iter().find(|a| a.board_id == board.id);
                vec![
                    labels.named_part(project, board.id),
                    allocation.map_or_else(
                        || loc.text("board-unallocated"),
                        |a| stock_ref(project, a.stock_id),
                    ),
                    allocation.map_or_else(
                        || "—".into(),
                        |a| {
                            format!(
                                "X {} · Y {}{}",
                                length(a.origin[0], settings.units, language),
                                length(a.origin[1], settings.units, language),
                                if a.quarter_turn {
                                    translated(language, " · turned 90°", " · girada 90°")
                                } else {
                                    ""
                                }
                            )
                        },
                    ),
                ]
            })
            .collect::<Vec<_>>();
        builder.table(
            &[
                translated(language, "Part", "Peça").into(),
                translated(language, "Sheet", "Chapa").into(),
                translated(language, "Position on sheet", "Posição na chapa").into(),
            ],
            &placements,
        )?;
        builder.section(&loc.text("pdf-cost"))?;
        builder.paragraph(&format!(
            "{}: {} · {}: {} · {}: {} · {}: {}",
            loc.text("pdf-material-cost"),
            money(estimate.material, language),
            loc.text("pdf-cut-cost"),
            money(estimate.cutting, language),
            loc.text("pdf-cuts"),
            estimate
                .cuts
                .map_or_else(|| loc.text("pdf-missing"), |n| n.to_string()),
            loc.text("pdf-total"),
            money(estimate.total, language)
        ))?;
    } else {
        // Incomplete estimates remain explicit in PageContext.notices. Summarize
        // the amounts even if the optional costing breakdown was switched off.
        builder.paragraph(&format!(
            "{}: {} · {}: {}",
            loc.text("pdf-material-cost"),
            money(estimate.material, language),
            loc.text("pdf-total"),
            money(estimate.total, language)
        ))?;
    }
    if sections.sheets_and_cut_steps {
        for (id, tree) in &prepared.witnesses {
            if let Some(stock) = project.stock.iter().find(|s| s.id == *id) {
                sheet_pages(
                    &mut builder,
                    tree,
                    stock,
                    project,
                    &labels,
                    &loc,
                    settings.units,
                )?;
            }
        }
    }
    if sections.hinge_references {
        hardware_pages(&mut builder, prepared, &labels, &loc, settings.units)?;
    }
    Ok(builder.finish())
}
