//! `[[drawer_slides]]` and `[[feet]]` tables: raw TOML shape, validation and
//! project snapshots. Same rules as hinges: exact millimetres, every problem
//! reported with its path, pinned copies never change with the pack.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{IssueKind, Pack, PackIssue, ReviewStatus, Severity, Source, issue, length, text};
use crate::domain::{
    CatalogItem, CatalogOrigin, CatalogReference, FootShape, FootSpec, Glide, Section,
    SlideExtension, SlideHole, SlideSpec, SrgbColor,
};
use crate::units::Length;

// ---- Raw TOML shape -------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSlide {
    pub id: String,
    pub name: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    pub height: f64,
    pub clearance: RawClearance,
    #[serde(default)]
    pub front_setback: f64,
    #[serde(default)]
    pub extension: SlideExtension,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members: Option<u8>,
    #[serde(default)]
    pub soft_close: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_kg: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rear_fixing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<super::RawAllow>,
    #[serde(default)]
    pub variants: Vec<RawSlideVariant>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawClearance {
    pub nominal: f64,
    #[serde(default)]
    pub minus: f64,
    #[serde(default)]
    pub plus: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSlideVariant {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<BTreeMap<String, String>>,
    pub length: f64,
    pub travel: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cabinet_holes: Vec<RawHole>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawer_holes: Vec<RawHole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_kg: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
}

/// A hole on a member's centre line (a number, mm from its front end), or a
/// table with an offset up from the centre line and a diameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RawHole {
    Along(f64),
    At(RawHoleAt),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawHoleAt {
    pub along: f64,
    #[serde(default)]
    pub offset: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawFoot {
    pub id: String,
    pub name: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    pub shape: RawShape,
    /// `#rrggbb`.
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
    #[serde(default)]
    pub adjustment: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounting_holes: Vec<(f64, f64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_kg: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub variants: Vec<RawFootVariant>,
}

/// Flat on purpose: `kind` picks which of the optional fields are required,
/// so a missing one is reported with its path.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawShape {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<RawSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<RawSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tube: Option<RawSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate: Option<RawSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate_thickness: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glide: Option<RawGlide>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tube_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tube_depth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crossbar_height: Option<f64>,
}

/// Round (`diameter`) or rectangular (`width` by `depth`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawGlide {
    pub diameter: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawFootVariant {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
    /// Overrides the shape's height (the same foot sold in several heights).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

// ---- Loaded model ---------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct SlideFamily {
    pub id: String,
    pub names: BTreeMap<String, String>,
    pub source: Option<Source>,
    pub attribution: Option<String>,
    pub height: Length,
    pub clearance: Length,
    pub clearance_minus: Length,
    pub clearance_plus: Length,
    pub front_setback: Length,
    pub extension: SlideExtension,
    pub soft_close: bool,
    pub capacity_kg: Option<u16>,
    pub finish: Option<String>,
    pub fixing: Option<String>,
    pub rear_fixing: Option<String>,
    pub notes: Option<String>,
    pub variants: Vec<SlideVariant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlideVariant {
    pub code: String,
    pub names: Option<BTreeMap<String, String>>,
    pub length: Length,
    pub travel: Length,
    pub cabinet_holes: Vec<SlideHole>,
    pub drawer_holes: Vec<SlideHole>,
    pub capacity_kg: Option<u16>,
    pub finish: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FootFamily {
    pub id: String,
    pub names: BTreeMap<String, String>,
    pub source: Option<Source>,
    pub attribution: Option<String>,
    pub shape: FootShape,
    pub color: SrgbColor,
    pub finish: Option<String>,
    pub adjustment: Length,
    pub mounting_holes: Vec<[Length; 2]>,
    pub load_kg: Option<u16>,
    pub notes: Option<String>,
    pub variants: Vec<FootVariant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FootVariant {
    pub code: String,
    pub names: Option<BTreeMap<String, String>>,
    pub color: Option<SrgbColor>,
    pub finish: Option<String>,
    pub height: Option<Length>,
}

impl SlideFamily {
    pub fn name(&self, language: &str) -> &str {
        super::localized(&self.names, language)
    }

    /// The pinned facts of one length.
    pub fn spec(&self, variant: &SlideVariant, language: &str) -> SlideSpec {
        let (printed_page, pdf_page, sha) =
            self.source.as_ref().map_or((0, 0, String::new()), |s| {
                (
                    s.printed_page.unwrap_or(0),
                    s.pdf_page.unwrap_or(0),
                    s.sha256.clone(),
                )
            });
        SlideSpec {
            family: self.name(language).to_owned(),
            length: variant.length,
            travel: variant.travel,
            height: self.height,
            clearance: self.clearance,
            clearance_minus: self.clearance_minus,
            clearance_plus: self.clearance_plus,
            front_setback: self.front_setback,
            extension: self.extension,
            soft_close: self.soft_close,
            capacity_kg: variant.capacity_kg.or(self.capacity_kg),
            finish: variant.finish.clone().or_else(|| self.finish.clone()),
            cabinet_holes: variant.cabinet_holes.clone(),
            drawer_holes: variant.drawer_holes.clone(),
            rear_fixing: self.rear_fixing.clone(),
            printed_page,
            pdf_page,
            source_sha256: sha,
            attribution: attribution(self.attribution.as_ref(), self.source.as_ref()),
        }
    }
}

impl FootFamily {
    pub fn name(&self, language: &str) -> &str {
        super::localized(&self.names, language)
    }

    pub fn spec(&self, variant: &FootVariant) -> FootSpec {
        FootSpec {
            shape: variant
                .height
                .map_or_else(|| self.shape.clone(), |h| with_height(&self.shape, h)),
            color: variant.color.unwrap_or(self.color),
            finish: variant.finish.clone().or_else(|| self.finish.clone()),
            adjustment: self.adjustment,
            mounting_holes: self.mounting_holes.clone(),
            load_kg: self.load_kg,
            attribution: attribution(self.attribution.as_ref(), self.source.as_ref()),
        }
    }
}

fn attribution(explicit: Option<&String>, source: Option<&Source>) -> String {
    explicit.cloned().unwrap_or_else(|| {
        source.map_or_else(String::new, |s| {
            let page = s
                .printed_page
                .map_or_else(String::new, |p| format!(", p. {p}"));
            format!("{}{page}", s.title)
        })
    })
}

fn with_height(shape: &FootShape, height: Length) -> FootShape {
    let mut shape = shape.clone();
    match &mut shape {
        FootShape::Tapered { height: h, .. }
        | FootShape::Post { height: h, .. }
        | FootShape::Frame { height: h, .. } => *h = height,
    }
    shape
}

// ---- Validation -----------------------------------------------------------

/// `#rrggbb` to sRGB.
pub fn parse_color(text: &str) -> Option<SrgbColor> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(SrgbColor([channel(0)?, channel(2)?, channel(4)?]))
}

pub fn color_text(color: SrgbColor) -> String {
    let [r, g, b] = color.0;
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn family_source(
    issues: &mut Vec<PackIssue>,
    path: &str,
    source: Option<&String>,
    sources: &[Source],
    status: ReviewStatus,
) -> Option<Source> {
    match source {
        Some(id) => {
            let found = sources.iter().find(|s| &s.id == id).cloned();
            if found.is_none() {
                issue(
                    issues,
                    format!("{path}.source"),
                    IssueKind::UnknownSource(id.clone()),
                );
            }
            found
        }
        None => {
            // Reviewed data cites the sheet it was checked against.
            if status == ReviewStatus::Reviewed {
                issue(
                    issues,
                    format!("{path}.source"),
                    IssueKind::Missing("source"),
                );
            }
            None
        }
    }
}

fn names(issues: &mut Vec<PackIssue>, path: &str, names: &BTreeMap<String, String>) {
    if names.values().all(|n| n.trim().is_empty()) {
        issue(issues, format!("{path}.name"), IssueKind::Missing("name"));
    }
}

fn holes(
    issues: &mut Vec<PackIssue>,
    path: &str,
    raw: &[RawHole],
    member_length: Length,
    member_height: Length,
) -> Vec<SlideHole> {
    let holes: Vec<SlideHole> = raw
        .iter()
        .enumerate()
        .map(|(n, hole)| {
            let hpath = format!("{path}[{n}]");
            let (along, offset, diameter) = match hole {
                RawHole::Along(along) => (*along, 0.0, None),
                RawHole::At(at) => (at.along, at.offset, at.diameter),
            };
            let along = length(issues, hpath.clone(), along, true);
            let offset = signed(issues, format!("{hpath}.offset"), offset);
            let diameter = diameter.map(|d| length(issues, format!("{hpath}.diameter"), d, false));
            if along > member_length || 2 * offset.micrometres().abs() > member_height.micrometres()
            {
                issue(issues, hpath, IssueKind::HoleOutsideMember);
            }
            SlideHole {
                along,
                offset,
                diameter,
            }
        })
        .collect();
    if holes.windows(2).any(|w| w[0].along > w[1].along) {
        issue(issues, path.to_owned(), IssueKind::HolesNotIncreasing);
    }
    holes
}

fn signed(issues: &mut Vec<PackIssue>, path: String, value: f64) -> Length {
    let magnitude = length(issues, path, value.abs(), true);
    if value < 0.0 {
        Length::from_micrometres(-magnitude.micrometres())
    } else {
        magnitude
    }
}

fn keep_allowed(own: &mut Vec<PackIssue>, allowed: &[(String, String)]) {
    own.retain(|i| {
        i.kind.severity() == Severity::Error
            || !allowed
                .iter()
                .any(|(code, reason)| code == i.kind.code() && !reason.trim().is_empty())
    });
}

pub(super) fn slide(
    s: RawSlide,
    path: &str,
    sources: &[Source],
    status: ReviewStatus,
    codes: &mut HashSet<String>,
    issues: &mut Vec<PackIssue>,
) -> SlideFamily {
    let mut own = Vec::new();
    names(&mut own, path, &s.name);
    let source = family_source(&mut own, path, s.source.as_ref(), sources, status);
    let height = length(&mut own, format!("{path}.height"), s.height, false);
    let clearance = length(
        &mut own,
        format!("{path}.clearance.nominal"),
        s.clearance.nominal,
        false,
    );
    let clearance_minus = length(
        &mut own,
        format!("{path}.clearance.minus"),
        s.clearance.minus,
        true,
    );
    let clearance_plus = length(
        &mut own,
        format!("{path}.clearance.plus"),
        s.clearance.plus,
        true,
    );
    if clearance.micrometres() > 0 && clearance_minus >= clearance {
        issue(
            &mut own,
            format!("{path}.clearance"),
            IssueKind::ClearanceRange,
        );
    }
    let front_setback = length(
        &mut own,
        format!("{path}.front_setback"),
        s.front_setback,
        true,
    );
    if s.variants.is_empty() {
        issue(&mut own, format!("{path}.variants"), IssueKind::NoVariants);
    }
    let variants = s
        .variants
        .into_iter()
        .enumerate()
        .map(|(j, v)| {
            let vpath = format!("{path}.variants[{j}]");
            text(&mut own, format!("{vpath}.code"), "code", &v.code);
            if !v.code.is_empty() && !codes.insert(v.code.clone()) {
                issue(
                    &mut own,
                    format!("{vpath}.code"),
                    IssueKind::DuplicateCode(v.code.clone()),
                );
            }
            let length_ = length(&mut own, format!("{vpath}.length"), v.length, false);
            let travel = length(&mut own, format!("{vpath}.travel"), v.travel, false);
            if travel.micrometres()
                > length_.micrometres() + crate::hardware_spec::MAX_OVER_TRAVEL.micrometres()
            {
                issue(
                    &mut own,
                    format!("{vpath}.travel"),
                    IssueKind::TravelTooLong,
                );
            }
            let cabinet_holes = holes(
                &mut own,
                &format!("{vpath}.cabinet_holes"),
                &v.cabinet_holes,
                length_,
                height,
            );
            let drawer_holes = holes(
                &mut own,
                &format!("{vpath}.drawer_holes"),
                &v.drawer_holes,
                length_,
                height,
            );
            SlideVariant {
                code: v.code,
                names: v.name,
                length: length_,
                travel,
                cabinet_holes,
                drawer_holes,
                capacity_kg: v.capacity_kg,
                finish: v.finish,
            }
        })
        .collect();
    let allowed: Vec<(String, String)> =
        s.allow.into_iter().map(|a| (a.warning, a.reason)).collect();
    keep_allowed(&mut own, &allowed);
    issues.extend(own);
    SlideFamily {
        id: s.id,
        names: s.name,
        source,
        attribution: s.attribution,
        height,
        clearance,
        clearance_minus,
        clearance_plus,
        front_setback,
        extension: s.extension,
        soft_close: s.soft_close,
        capacity_kg: s.capacity_kg,
        finish: s.finish,
        fixing: s.fixing,
        rear_fixing: s.rear_fixing,
        notes: s.notes,
        variants,
    }
}

fn section(
    issues: &mut Vec<PackIssue>,
    path: &str,
    field: &'static str,
    raw: Option<&RawSection>,
) -> Option<Section> {
    let Some(raw) = raw else {
        issue(issues, format!("{path}.{field}"), IssueKind::Missing(field));
        return None;
    };
    match (raw.diameter, raw.width, raw.depth) {
        (Some(d), None, None) => Some(Section::Round {
            diameter: length(issues, format!("{path}.{field}.diameter"), d, false),
        }),
        (None, Some(w), Some(d)) => Some(Section::Rect {
            width: length(issues, format!("{path}.{field}.width"), w, false),
            depth: length(issues, format!("{path}.{field}.depth"), d, false),
        }),
        _ => {
            issue(issues, format!("{path}.{field}"), IssueKind::ShapeGeometry);
            None
        }
    }
}

fn required(
    issues: &mut Vec<PackIssue>,
    path: &str,
    field: &'static str,
    value: Option<f64>,
) -> Option<Length> {
    match value {
        Some(v) => Some(length(issues, format!("{path}.{field}"), v, false)),
        None => {
            issue(issues, format!("{path}.{field}"), IssueKind::Missing(field));
            None
        }
    }
}

fn glide(issues: &mut Vec<PackIssue>, path: &str, raw: Option<&RawGlide>) -> Option<Glide> {
    raw.map(|g| Glide {
        diameter: length(issues, format!("{path}.glide.diameter"), g.diameter, false),
        height: length(issues, format!("{path}.glide.height"), g.height, false),
    })
}

/// Convert a raw shape; `None` (with issues) when it is incomplete.
pub(super) fn shape(issues: &mut Vec<PackIssue>, path: &str, raw: &RawShape) -> Option<FootShape> {
    let height = required(issues, path, "height", raw.height);
    let shape = match raw.kind.as_str() {
        "tapered" => {
            let top = section(issues, path, "top", raw.top.as_ref());
            let bottom = section(issues, path, "bottom", raw.bottom.as_ref());
            FootShape::Tapered {
                top: top?,
                bottom: bottom?,
                height: height?,
            }
        }
        "post" => {
            let tube = section(issues, path, "tube", raw.tube.as_ref());
            let plate = section(issues, path, "plate", raw.plate.as_ref());
            let plate_thickness = required(issues, path, "plate_thickness", raw.plate_thickness);
            FootShape::Post {
                tube: tube?,
                plate: plate?,
                plate_thickness: plate_thickness?,
                glide: glide(issues, path, raw.glide.as_ref()),
                height: height?,
            }
        }
        "frame" => {
            let top_width = required(issues, path, "top_width", raw.top_width);
            let bottom_width = raw
                .bottom_width
                .map(|v| length(issues, format!("{path}.bottom_width"), v, false));
            let tube_width = required(issues, path, "tube_width", raw.tube_width);
            let tube_depth = required(issues, path, "tube_depth", raw.tube_depth);
            let crossbar_height = raw
                .crossbar_height
                .map(|v| length(issues, format!("{path}.crossbar_height"), v, false));
            FootShape::Frame {
                height: height?,
                top_width: top_width?,
                bottom_width: bottom_width.or(top_width)?,
                tube_width: tube_width?,
                tube_depth: tube_depth?,
                crossbar_height,
                glide: glide(issues, path, raw.glide.as_ref()),
            }
        }
        other => {
            issue(
                issues,
                format!("{path}.kind"),
                IssueKind::UnknownShape(other.to_owned()),
            );
            return None;
        }
    };
    Some(shape)
}

pub(super) fn foot(
    f: RawFoot,
    path: &str,
    sources: &[Source],
    status: ReviewStatus,
    codes: &mut HashSet<String>,
    issues: &mut Vec<PackIssue>,
) -> Option<FootFamily> {
    let mut own = Vec::new();
    names(&mut own, path, &f.name);
    let source = family_source(&mut own, path, f.source.as_ref(), sources, status);
    let color = parse_color(&f.color).unwrap_or_else(|| {
        issue(
            &mut own,
            format!("{path}.color"),
            IssueKind::InvalidColor(f.color.clone()),
        );
        SrgbColor([0, 0, 0])
    });
    let adjustment = length(&mut own, format!("{path}.adjustment"), f.adjustment, true);
    let mounting_holes: Vec<[Length; 2]> = f
        .mounting_holes
        .iter()
        .enumerate()
        .map(|(n, &(x, y))| {
            let hpath = format!("{path}.mounting_holes[{n}]");
            [
                signed(&mut own, hpath.clone(), x),
                signed(&mut own, hpath, y),
            ]
        })
        .collect();
    let shape = shape(&mut own, &format!("{path}.shape"), &f.shape);
    if f.variants.is_empty() {
        issue(&mut own, format!("{path}.variants"), IssueKind::NoVariants);
    }
    let variants: Vec<FootVariant> = f
        .variants
        .into_iter()
        .enumerate()
        .map(|(j, v)| {
            let vpath = format!("{path}.variants[{j}]");
            text(&mut own, format!("{vpath}.code"), "code", &v.code);
            if !v.code.is_empty() && !codes.insert(v.code.clone()) {
                issue(
                    &mut own,
                    format!("{vpath}.code"),
                    IssueKind::DuplicateCode(v.code.clone()),
                );
            }
            let color = v.color.as_ref().map(|c| {
                parse_color(c).unwrap_or_else(|| {
                    issue(
                        &mut own,
                        format!("{vpath}.color"),
                        IssueKind::InvalidColor(c.clone()),
                    );
                    SrgbColor([0, 0, 0])
                })
            });
            let height = v
                .height
                .map(|h| length(&mut own, format!("{vpath}.height"), h, false));
            FootVariant {
                code: v.code,
                names: v.name,
                color,
                finish: v.finish,
                height,
            }
        })
        .collect();
    let Some(shape) = shape else {
        issues.extend(own);
        return None;
    };
    let family = FootFamily {
        id: f.id,
        names: f.name,
        source,
        attribution: f.attribution,
        shape,
        color,
        finish: f.finish,
        adjustment,
        mounting_holes,
        load_kg: f.load_kg,
        notes: f.notes,
        variants,
    };
    if own.iter().all(|i| i.kind.severity() != Severity::Error) {
        for (j, variant) in family.variants.iter().enumerate() {
            if !family.spec(variant).is_consistent() {
                issue(
                    &mut own,
                    format!("{path}.variants[{j}]"),
                    IssueKind::ShapeGeometry,
                );
            }
        }
    }
    issues.extend(own);
    Some(family)
}

// ---- Snapshots ------------------------------------------------------------

fn origin(pack: &Pack, family_id: &str, code: &str) -> CatalogOrigin {
    CatalogOrigin {
        pack_id: pack.id.clone(),
        pack_version: pack.version.clone(),
        manufacturer: pack.manufacturer.clone(),
        item_id: family_id.to_owned(),
        variant_code: code.to_owned(),
    }
}

fn provenance(pack: &Pack, source: Option<&Source>) -> (String, String) {
    source.map_or_else(
        || {
            (
                String::new(),
                format!("{} {}", pack.manufacturer, pack.version),
            )
        },
        |s| {
            (
                s.url.clone(),
                format!("{}; SHA-256 {}", s.revision, s.sha256),
            )
        },
    )
}

/// Copy one slide length into a project-owned snapshot with a fresh ID.
pub fn snapshot_slide(
    pack: &Pack,
    family: &SlideFamily,
    variant: &SlideVariant,
    language: &str,
) -> CatalogReference {
    let name = variant.names.as_ref().map_or_else(
        || {
            format!(
                "{} {} mm",
                family.name(language),
                super::mm_text(variant.length)
            )
        },
        |names| super::localized(names, language).to_owned(),
    );
    let (source, revision) = provenance(pack, family.source.as_ref());
    CatalogReference {
        id: Uuid::new_v4(),
        name,
        product_id: variant.code.clone(),
        plate_id: None,
        source,
        revision,
        installation_dimensions: Default::default(),
        verified_hinge: None,
        origin: Some(origin(pack, &family.id, &variant.code)),
        item: Some(CatalogItem::Slide(family.spec(variant, language))),
    }
}

/// Copy one foot into a project-owned snapshot with a fresh ID.
pub fn snapshot_foot(
    pack: &Pack,
    family: &FootFamily,
    variant: &FootVariant,
    language: &str,
) -> CatalogReference {
    let name = variant.names.as_ref().map_or_else(
        || family.name(language).to_owned(),
        |names| super::localized(names, language).to_owned(),
    );
    let (source, revision) = provenance(pack, family.source.as_ref());
    CatalogReference {
        id: Uuid::new_v4(),
        name,
        product_id: variant.code.clone(),
        plate_id: None,
        source,
        revision,
        installation_dimensions: Default::default(),
        verified_hinge: None,
        origin: Some(origin(pack, &family.id, &variant.code)),
        item: Some(CatalogItem::Foot(family.spec(variant))),
    }
}
