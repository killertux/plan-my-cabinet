//! Localized hardware text shared by both PDF writers: the purchase list,
//! drawer slide hole references and foot descriptions.
use std::collections::BTreeMap;

use crate::domain::{CatalogReference, FootShape, HardwareKind, Project};
use crate::export::ExportSlideGuidance;
use crate::hardware_catalog::Trust;
use crate::i18n::Localizer;
use crate::units::Length;

/// One row per product to buy: (code, name, quantity text).
pub fn purchase_rows(project: &Project, loc: &Localizer) -> Vec<(String, String, String)> {
    let entry = |id| project.catalog.iter().find(|c| c.id == id);
    let mut hinges: BTreeMap<(String, String), usize> = BTreeMap::new();
    for h in &project.hinge_installations {
        if let Some(c) = entry(h.catalog_id) {
            let code = match &c.plate_id {
                Some(plate) => format!("{} + {plate}", c.product_id),
                None => c.product_id.clone(),
            };
            *hinges.entry((code, c.name.clone())).or_default() += 1;
        }
    }
    let mut slides: BTreeMap<(String, String), usize> = BTreeMap::new();
    for s in &project.slide_installations {
        if let Some(c) = entry(s.catalog_id) {
            *slides
                .entry((c.product_id.clone(), c.name.clone()))
                .or_default() += 1;
        }
    }
    let mut feet: BTreeMap<(String, String), usize> = BTreeMap::new();
    for h in &project.hardware {
        if let HardwareKind::Catalog { catalog_id } = h.kind
            && let Some(c) = entry(catalog_id).filter(|c| c.foot().is_some())
        {
            *feet
                .entry((c.product_id.clone(), c.name.clone()))
                .or_default() += 1;
        }
    }
    let count = |key: &str, n: usize| {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("count", n);
        loc.format(key, Some(&args))
    };
    let mut rows = Vec::new();
    for ((code, name), n) in hinges {
        rows.push((code, name, count("pdf-count-units", n)));
    }
    for ((code, name), n) in slides {
        rows.push((code, name, count("pdf-count-pairs", n)));
    }
    for ((code, name), n) in feet {
        rows.push((code, name, count("pdf-count-units", n)));
    }
    rows
}

/// "code — name — 3 pairs" lines.
pub fn purchase_lines(project: &Project, loc: &Localizer) -> Vec<String> {
    purchase_rows(project, loc)
        .into_iter()
        .map(|(code, name, qty)| format!("{code} — {name} — {qty}"))
        .collect()
}

fn trust_note(trust: Trust, loc: &Localizer) -> Option<String> {
    match trust {
        Trust::Reviewed => None,
        Trust::UserSupplied => Some(loc.text("pdf-user-supplied")),
        Trust::Generic => Some(loc.text("pdf-generic-dimensions")),
    }
}

fn join(lengths: &[Length], fmt: &impl Fn(Length) -> String) -> String {
    lengths
        .iter()
        .map(|l| fmt(*l))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Hole references for one drawer's slides, with provenance.
pub fn slide_lines(
    project: &Project,
    guidance: &ExportSlideGuidance,
    loc: &Localizer,
    fmt: impl Fn(Length) -> String,
) -> Vec<String> {
    let r = &guidance.references;
    let name = |id| {
        project
            .board(id)
            .map_or_else(|| id.to_string(), |b| b.name.clone())
    };
    let face = |face| {
        loc.text(match face {
            crate::domain::BoardFace::MinZ => "pdf-face-min-z",
            crate::domain::BoardFace::MaxZ => "pdf-face-max-z",
        })
    };
    let mut lines = vec![format!(
        "{} — {} {} — {}: {} — {}: {} ({} {}) — {}: {}",
        guidance.drawer_name,
        r.product_id,
        r.family,
        loc.text("pdf-slide-length"),
        fmt(r.length),
        loc.text("pdf-slide-clearance"),
        fmt(r.clearance),
        format_args!("+{}", fmt(r.clearance_plus)),
        format_args!("-{}", fmt(r.clearance_minus)),
        loc.text("pdf-slide-setback"),
        fmt(r.setback),
    )];
    for (i, side) in r.sides.iter().enumerate() {
        let which = loc.text(if i == 0 { "pdf-left" } else { "pdf-right" });
        lines.push(format!(
            "{which} · {} ({}): {} {} — {} {}",
            name(side.cabinet_board),
            face(side.cabinet_face),
            loc.text("pdf-slide-holes-from-front"),
            join(&side.cabinet_hole_distances, &fmt),
            loc.text("pdf-slide-centre-line"),
            fmt(side.cabinet_centre_from_bottom),
        ));
        lines.push(format!(
            "{which} · {} ({}): {} {} — {} {}",
            name(side.drawer_board),
            face(side.drawer_face),
            loc.text("pdf-slide-holes-from-front"),
            join(&side.drawer_hole_distances, &fmt),
            loc.text("pdf-slide-centre-line"),
            fmt(side.drawer_centre_from_bottom),
        ));
    }
    let page = |n: u16| {
        if n == 0 {
            "—".to_owned()
        } else {
            n.to_string()
        }
    };
    let mut source = format!(
        "{} — {} ({}; {}: {}; PDF: {})",
        r.product_id,
        r.attribution,
        r.source,
        loc.text("pdf-printed-page"),
        page(r.printed_page),
        page(r.pdf_page)
    );
    if let Some(pack) = &r.pack {
        source.push_str(&format!(" — {}: {pack}", loc.text("pdf-catalog-pack")));
    }
    if let Some(note) = trust_note(r.trust, loc) {
        source.push_str(&format!(" — {note}"));
    }
    lines.push(source);
    if let Some(rear) = &r.rear_fixing {
        lines.push(format!("{}: {rear}", loc.text("pdf-slide-rear-fixing")));
    }
    lines
}

/// "Square chrome adjustable foot 100 mm — post 60 × 60 × 100 mm, chrome".
pub fn foot_line(
    entry: &CatalogReference,
    loc: &Localizer,
    fmt: impl Fn(Length) -> String,
) -> String {
    let Some(spec) = entry.foot() else {
        return entry.name.clone();
    };
    let size = spec.local_size();
    let shape = loc.text(match spec.shape {
        FootShape::Tapered { .. } => "pdf-foot-tapered",
        FootShape::Post { .. } => "pdf-foot-post",
        FootShape::Frame { .. } => "pdf-foot-frame",
    });
    let mut line = format!(
        "{} ({}) — {shape} {} × {} × {}",
        entry.name,
        entry.product_id,
        fmt(size[0]),
        fmt(size[1]),
        fmt(size[2])
    );
    if let Some(finish) = &spec.finish {
        line.push_str(&format!(", {finish}"));
    }
    if spec.adjustment.micrometres() > 0 {
        line.push_str(&format!(
            ", {} {}",
            loc.text("pdf-foot-adjustment"),
            fmt(spec.adjustment)
        ));
    }
    if let Some(note) = crate::hardware_catalog::trust(entry).and_then(|t| trust_note(t, loc)) {
        line.push_str(&format!(" — {note}"));
    }
    line
}
