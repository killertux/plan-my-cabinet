//! 2-D stock sheet diagrams: outline, trims, placed parts, the numbered
//! guillotine cut sequence with its kerf bands, and reusable offcuts.
use std::collections::HashMap;
use std::fmt::Write as _;

use serde::Serialize;
use uuid::Uuid;

use crate::cut_tree::{Axis, CutKind, CutTree, Rectangle};
use crate::domain::StockGrain;
use crate::read_models::stock_read_models::{SheetProof, StockPieceReadModel};
use crate::render::picture::{PictureError, svg_to_png};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartLabels {
    /// Part names inside the rectangles (numbers when they do not fit).
    Names,
    /// Legend numbers only.
    Numbers,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SheetLegendEntry {
    pub number: usize,
    pub board_id: Uuid,
    pub name: String,
    pub origin_mm: [f64; 2],
    pub size_mm: [f64; 2],
    pub quarter_turn: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SheetPicture {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub legend: Vec<SheetLegendEntry>,
    pub cut_count: Option<usize>,
    /// Finished part area over the whole sheet, in percent, when verified.
    pub utilization_percent: Option<f64>,
    pub status: &'static str,
}

fn mm(v: crate::units::Length) -> f64 {
    v.micrometres() as f64 / 1000.0
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Draw one stock piece. Sheet X (length) runs right, Y (width) runs up.
pub fn render_sheet(
    piece: &StockPieceReadModel,
    labels: PartLabels,
    show_cuts: bool,
    width_px: u32,
) -> Result<SheetPicture, PictureError> {
    let (length, width) = (mm(piece.length), mm(piece.width));
    let margin_top = 0.10 * length.max(width).max(1.0) * 0.6;
    let pad = 0.03 * length.max(width).max(1.0);
    let (view_w, view_h) = (length + 2.0 * pad, width + pad + margin_top);
    // Font sizes in sheet millimetres, scaled to roughly constant pixels.
    let px = view_w / width_px.max(1) as f64;
    let font = 13.0 * px;
    let small = 11.0 * px;
    let x = |v: f64| pad + v;
    let y = |v: f64| margin_top + (width - v);
    let rect = |r: &Rectangle| {
        let (ox, oy) = (mm(r.origin[0]), mm(r.origin[1]));
        let (ex, ey) = (mm(r.extent[0]), mm(r.extent[1]));
        (x(ox), y(oy + ey), ex, ey)
    };
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{view_w:.3}" height="{view_h:.3}" viewBox="0 0 {view_w:.3} {view_h:.3}" font-family="Noto Sans">"#
    );
    let _ = write!(
        svg,
        r##"<defs><pattern id="hatch" patternUnits="userSpaceOnUse" width="{s:.3}" height="{s:.3}" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="{s:.3}" stroke="#7fae86" stroke-width="{w:.3}"/></pattern></defs>"##,
        s = 8.0 * px,
        w = 2.0 * px
    );
    let grain = match piece.grain {
        StockGrain::AlongX => "grain along length",
        StockGrain::AlongY => "grain along width",
        StockGrain::Nondirectional => "no grain",
        StockGrain::Unknown => "grain unknown",
    };
    let status = match &piece.proof {
        SheetProof::Unused => "unused",
        SheetProof::Verified { .. } => "verified",
        SheetProof::Violation(_) => "violation",
        SheetProof::SearchExhausted => "search_exhausted",
    };
    let _ = write!(
        svg,
        r##"<text x="{:.3}" y="{:.3}" font-size="{:.3}" font-weight="600" fill="#1f2a30">{} · {} · {:.0} × {:.0} × {:.1} mm · {} · {}</text>"##,
        pad,
        margin_top * 0.55,
        font * 1.15,
        escape(&piece.alias),
        escape(&piece.name),
        length,
        width,
        mm(piece.measured_thickness),
        grain,
        status.replace('_', " ")
    );
    // Sheet body and trims.
    let _ = write!(
        svg,
        r##"<rect x="{:.3}" y="{:.3}" width="{length:.3}" height="{width:.3}" fill="#e9e4da" stroke="#1f2a30" stroke-width="{:.3}"/>"##,
        x(0.0),
        y(width),
        1.5 * px
    );
    let [left, right, bottom, top] = piece.trim.map(mm);
    for (tx, ty, tw, th) in [
        (0.0, 0.0, left, width),
        (length - right, 0.0, right, width),
        (0.0, 0.0, length, bottom),
        (0.0, width - top, length, top),
    ] {
        if tw > 0.0 && th > 0.0 {
            let _ = write!(
                svg,
                r##"<rect x="{:.3}" y="{:.3}" width="{tw:.3}" height="{th:.3}" fill="#b9b2a6"/>"##,
                x(tx),
                y(ty + th)
            );
        }
    }
    let tree: Option<&CutTree> = piece.proof.verified().map(|(tree, _)| tree);
    // Offcuts (reusable) and waste from the verified tree.
    if let Some(tree) = tree {
        for node in tree.nodes() {
            let (rx, ry, rw, rh) = rect(&node.rectangle);
            if rw <= 0.0 || rh <= 0.0 {
                continue;
            }
            match node.kind {
                CutKind::Offcut => {
                    let _ = write!(
                        svg,
                        r##"<rect x="{rx:.3}" y="{ry:.3}" width="{rw:.3}" height="{rh:.3}" fill="url(#hatch)" stroke="#7fae86" stroke-width="{:.3}"/>"##,
                        px
                    );
                    if rw > 60.0 * px && rh > 16.0 * px {
                        let _ = write!(
                            svg,
                            r##"<text x="{:.3}" y="{:.3}" font-size="{small:.3}" fill="#3f6b46" text-anchor="middle" dominant-baseline="central">offcut {:.0}×{:.0}</text>"##,
                            rx + rw / 2.0,
                            ry + rh / 2.0,
                            rw,
                            rh
                        );
                    }
                }
                CutKind::Waste => {
                    let _ = write!(
                        svg,
                        r##"<rect x="{rx:.3}" y="{ry:.3}" width="{rw:.3}" height="{rh:.3}" fill="#cfc8bd"/>"##
                    );
                }
                _ => {}
            }
        }
    }
    // Parts.
    let mut legend = Vec::new();
    for (index, part) in piece.parts.iter().enumerate() {
        let number = index + 1;
        let (pl, pw) = if part.quarter_turn {
            (mm(part.width), mm(part.length))
        } else {
            (mm(part.length), mm(part.width))
        };
        let (ox, oy) = (mm(part.origin[0]), mm(part.origin[1]));
        let _ = write!(
            svg,
            r##"<rect x="{:.3}" y="{:.3}" width="{pl:.3}" height="{pw:.3}" fill="#c9b99a" stroke="#1f2a30" stroke-width="{:.3}"/>"##,
            x(ox),
            y(oy + pw),
            1.2 * px
        );
        let (cx, cy) = (x(ox + pl / 2.0), y(oy + pw / 2.0));
        let name = escape(&part.name);
        let text_fits = labels == PartLabels::Names
            && pw > 2.6 * font
            && pl > (name.chars().count() as f64 * 0.6 + 2.0) * font;
        if text_fits {
            let _ = write!(
                svg,
                r##"<text x="{cx:.3}" y="{:.3}" font-size="{font:.3}" font-weight="600" fill="#1f2a30" text-anchor="middle">{number}. {name}</text><text x="{cx:.3}" y="{:.3}" font-size="{small:.3}" fill="#3a352e" text-anchor="middle">{:.0} × {:.0}{}</text>"##,
                cy - 0.2 * font,
                cy + 1.1 * font,
                mm(part.length),
                mm(part.width),
                if part.locked { " · locked" } else { "" }
            );
        } else {
            let r = (7.5 * px).min(pl / 2.0).min(pw / 2.0).max(3.0 * px);
            let _ = write!(
                svg,
                r##"<circle cx="{cx:.3}" cy="{cy:.3}" r="{r:.3}" fill="#1f2a30"/><text x="{cx:.3}" y="{cy:.3}" font-size="{:.3}" fill="#ffffff" text-anchor="middle" dominant-baseline="central">{number}</text>"##,
                r * 1.1
            );
        }
        legend.push(SheetLegendEntry {
            number,
            board_id: part.board_id,
            name: part.name.clone(),
            origin_mm: [ox, oy],
            size_mm: [mm(part.length), mm(part.width)],
            quarter_turn: part.quarter_turn,
            locked: part.locked,
        });
    }
    // Cut sequence: kerf bands between split outputs, numbered in cutting order.
    if show_cuts && let Some(tree) = tree {
        let numbers: HashMap<usize, usize> = tree
            .operations()
            .into_iter()
            .map(|op| (op.input, op.number))
            .collect();
        for (id, node) in tree.nodes().iter().enumerate() {
            let CutKind::Split {
                axis,
                kerf,
                first,
                second,
                ..
            } = node.kind
            else {
                continue;
            };
            let (Some(a), Some(b)) = (tree.node(first), tree.node(second)) else {
                continue;
            };
            let input = node.rectangle;
            let (band_origin, band_extent) = match axis {
                Axis::X => {
                    let low = a.rectangle.origin[0].min(b.rectangle.origin[0]);
                    let first_end =
                        low.checked_add(if a.rectangle.origin[0] <= b.rectangle.origin[0] {
                            a.rectangle.extent[0]
                        } else {
                            b.rectangle.extent[0]
                        });
                    let Ok(start) = first_end else { continue };
                    (
                        [mm(start), mm(input.origin[1])],
                        [mm(kerf), mm(input.extent[1])],
                    )
                }
                Axis::Y => {
                    let low = a.rectangle.origin[1].min(b.rectangle.origin[1]);
                    let first_end =
                        low.checked_add(if a.rectangle.origin[1] <= b.rectangle.origin[1] {
                            a.rectangle.extent[1]
                        } else {
                            b.rectangle.extent[1]
                        });
                    let Ok(start) = first_end else { continue };
                    (
                        [mm(input.origin[0]), mm(start)],
                        [mm(input.extent[0]), mm(kerf)],
                    )
                }
            };
            let (bx, by) = (x(band_origin[0]), y(band_origin[1] + band_extent[1]));
            let (bw, bh) = (band_extent[0].max(1.5 * px), band_extent[1].max(1.5 * px));
            let _ = write!(
                svg,
                r##"<rect x="{bx:.3}" y="{by:.3}" width="{bw:.3}" height="{bh:.3}" fill="#c4453b" fill-opacity="0.85"/>"##
            );
            if let Some(number) = numbers.get(&id) {
                // Tag at the band start (bottom or left end of the pass).
                let (tx, ty) = match axis {
                    Axis::X => (bx + bw / 2.0, by + bh - 9.0 * px),
                    Axis::Y => (bx + 9.0 * px, by + bh / 2.0),
                };
                let _ = write!(
                    svg,
                    r##"<rect x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" rx="{:.3}" fill="#c4453b"/><text x="{tx:.3}" y="{ty:.3}" font-size="{small:.3}" font-weight="600" fill="#ffffff" text-anchor="middle" dominant-baseline="central">{number}</text>"##,
                    tx - 8.0 * px,
                    ty - 7.0 * px,
                    16.0 * px,
                    14.0 * px,
                    3.0 * px
                );
            }
        }
    }
    svg.push_str("</svg>");
    let (png, w, h) = svg_to_png(&svg, width_px)?;
    let utilization_percent = piece.proof.utilization().and_then(|(part, root)| {
        (root > 0).then(|| (part as f64 / root as f64 * 1000.0).round() / 10.0)
    });
    Ok(SheetPicture {
        png,
        width: w,
        height: h,
        legend,
        cut_count: piece.proof.cut_count(),
        utilization_percent,
        status,
    })
}
