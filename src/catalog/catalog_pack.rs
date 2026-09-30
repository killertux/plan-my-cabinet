//! Manufacturer catalog packs: TOML files of hardware facts with their sources.
//!
//! A pack is data, never code. Loading converts every millimetre value to an
//! exact `Length` and collects *all* problems, so an author sees the whole
//! list at once. A pack with errors is listed but offers nothing to add.
//! Adding an item copies it into the project as a pinned `CatalogReference`;
//! later pack edits never change a saved project.
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::{
    CatalogOrigin, CatalogReference, HingeArm, OverlaySetting, UnavailableDetail, VerifiedHinge,
};
use crate::units::{Length, Unit};

pub const PACK_SCHEMA: u32 = 1;
pub const MAX_PACK_BYTES: usize = 1024 * 1024;

/// Packs compiled into the application, reviewed against the cited sources.
pub const BUNDLED: &[(&str, &str)] = &[
    ("fgvtn.toml", include_str!("../../catalogs/fgvtn.toml")),
    (
        "fgvtn-slides.toml",
        include_str!("../../catalogs/fgvtn-slides.toml"),
    ),
    (
        "generic-feet.toml",
        include_str!("../../catalogs/generic-feet.toml"),
    ),
];

mod hardware;
pub use hardware::{
    FootFamily, FootVariant, RawClearance, RawFoot, RawFootVariant, RawGlide, RawHole, RawHoleAt,
    RawSection, RawShape, RawSlide, RawSlideVariant, SlideFamily, SlideVariant, color_text,
    parse_color, snapshot_foot, snapshot_slide,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    #[default]
    Reviewed,
    Draft,
    /// Representative dimensions of a common product type, not taken from a
    /// manufacturer sheet. Labelled as such wherever it is shown.
    Generic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mounting {
    Clip,
    SlideOn,
    FixedPlate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackOrigin {
    Bundled(&'static str),
    User(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// One problem found while loading. `path` points into the TOML document,
/// such as `hinges[1].variants[2].k_table[0]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackIssue {
    pub path: String,
    pub kind: IssueKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssueKind {
    /// Not TOML, or a field of the wrong shape. The text comes from the parser.
    Syntax(String),
    UnsupportedSchema(u32),
    TooLarge,
    Unreadable(String),
    InvalidId(String),
    Missing(&'static str),
    DuplicateId(String),
    DuplicateCode(String),
    UnknownSource(String),
    InvalidSha256,
    /// A millimetre value that is negative where it must not be, zero where it
    /// must be positive, or finer than one micrometre.
    InvalidLength(String),
    ThicknessRange,
    CupDeeperThanDoor,
    KNotIncreasing,
    KOutsideRange,
    OpeningAngle(u16),
    NoVariants,
    ClearanceRange,
    TravelTooLong,
    HoleOutsideMember,
    UnknownShape(String),
    ShapeGeometry,
    InvalidColor(String),
    // Warnings.
    HolesNotIncreasing,
    NotMonotonic,
    NoOpeningAngle,
    Draft,
    ShadowsBundled,
}

impl IssueKind {
    pub fn severity(&self) -> Severity {
        match self {
            Self::NotMonotonic
            | Self::NoOpeningAngle
            | Self::Draft
            | Self::ShadowsBundled
            | Self::HolesNotIncreasing => Severity::Warning,
            _ => Severity::Error,
        }
    }

    /// Stable name used by `allow` lists and as the i18n key suffix.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Syntax(_) => "syntax",
            Self::UnsupportedSchema(_) => "unsupported-schema",
            Self::TooLarge => "too-large",
            Self::Unreadable(_) => "unreadable",
            Self::InvalidId(_) => "invalid-id",
            Self::Missing(_) => "missing",
            Self::DuplicateId(_) => "duplicate-id",
            Self::DuplicateCode(_) => "duplicate-code",
            Self::UnknownSource(_) => "unknown-source",
            Self::InvalidSha256 => "invalid-sha256",
            Self::InvalidLength(_) => "invalid-length",
            Self::ThicknessRange => "thickness-range",
            Self::CupDeeperThanDoor => "cup-deeper-than-door",
            Self::KNotIncreasing => "k-not-increasing",
            Self::KOutsideRange => "k-outside-range",
            Self::OpeningAngle(_) => "opening-angle",
            Self::NoVariants => "no-variants",
            Self::ClearanceRange => "clearance-range",
            Self::TravelTooLong => "travel-too-long",
            Self::HoleOutsideMember => "hole-outside-member",
            Self::UnknownShape(_) => "unknown-shape",
            Self::ShapeGeometry => "shape-geometry",
            Self::InvalidColor(_) => "invalid-color",
            Self::HolesNotIncreasing => "holes-not-increasing",
            Self::NotMonotonic => "not-monotonic",
            Self::NoOpeningAngle => "no-opening-angle",
            Self::Draft => "draft",
            Self::ShadowsBundled => "shadows-bundled",
        }
    }

    /// Free-form detail shown after the localized message, if any.
    pub fn detail(&self) -> Option<String> {
        match self {
            Self::Syntax(text) | Self::Unreadable(text) => Some(text.clone()),
            Self::UnsupportedSchema(n) => Some(n.to_string()),
            Self::InvalidId(id)
            | Self::DuplicateId(id)
            | Self::DuplicateCode(id)
            | Self::UnknownSource(id)
            | Self::UnknownShape(id)
            | Self::InvalidColor(id) => Some(id.clone()),
            Self::Missing(field) => Some((*field).into()),
            Self::InvalidLength(value) => Some(value.clone()),
            Self::OpeningAngle(degrees) => Some(format!("{degrees}°")),
            _ => None,
        }
    }
}

impl fmt::Display for PackIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.kind.severity() {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let message = match &self.kind {
            IssueKind::Syntax(_) => "not a valid catalog pack",
            IssueKind::UnsupportedSchema(_) => "unsupported pack schema",
            IssueKind::TooLarge => "file is larger than 1 MB",
            IssueKind::Unreadable(_) => "file cannot be read",
            IssueKind::InvalidId(_) => "ids use lowercase letters, digits and '-'",
            IssueKind::Missing(_) => "required value is empty",
            IssueKind::DuplicateId(_) => "id is used twice",
            IssueKind::DuplicateCode(_) => "product code is used twice",
            IssueKind::UnknownSource(_) => "no [[sources]] entry has this id",
            IssueKind::InvalidSha256 => "sha256 must be 64 hexadecimal digits",
            IssueKind::InvalidLength(_) => {
                "length must be positive (or zero where allowed) with at most 3 decimals"
            }
            IssueKind::ThicknessRange => "door thickness min is greater than max",
            IssueKind::CupDeeperThanDoor => "cup is as deep as the thinnest door",
            IssueKind::KNotIncreasing => "K values must be strictly increasing",
            IssueKind::KOutsideRange => "K is not smaller than the cup diameter",
            IssueKind::OpeningAngle(_) => "opening angle must be between 1 and 180",
            IssueKind::NoVariants => "item has no variants",
            IssueKind::ClearanceRange => "clearance minus is not smaller than the clearance",
            IssueKind::TravelTooLong => "travel is more than 100 mm beyond the slide length",
            IssueKind::HoleOutsideMember => "hole lies outside the slide member",
            IssueKind::UnknownShape(_) => "shape kind must be tapered, post or frame",
            IssueKind::ShapeGeometry => {
                "shape parts do not fit (sizes, plate and glide heights, frame tubes)"
            }
            IssueKind::InvalidColor(_) => "color must be #rrggbb",
            IssueKind::HolesNotIncreasing => "holes are not listed front to back",
            IssueKind::NotMonotonic => "R/F values do not change steadily with K",
            IssueKind::NoOpeningAngle => "no opening angle: door motion preview is unavailable",
            IssueKind::Draft => "pack is a draft",
            IssueKind::ShadowsBundled => "replaces the bundled pack with the same id",
        };
        write!(f, "{severity}: {}: {message}", self.path)?;
        if let Some(detail) = self.kind.detail() {
            write!(f, " ({detail})")?;
        }
        Ok(())
    }
}

// ---- Raw TOML shape -------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawPack {
    pub schema: u32,
    pub id: String,
    pub manufacturer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    pub version: String,
    pub review: RawReview,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<RawSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hinges: Vec<RawHinge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawer_slides: Vec<RawSlide>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feet: Vec<RawFoot>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawReview {
    pub status: ReviewStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSource {
    id: String,
    title: String,
    url: String,
    sha256: String,
    revision: String,
    #[serde(default)]
    printed_page: Option<u16>,
    #[serde(default)]
    pdf_page: Option<u16>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawHinge {
    id: String,
    name: BTreeMap<String, String>,
    source: String,
    #[serde(default)]
    attribution: Option<String>,
    #[serde(default)]
    soft_close: bool,
    #[serde(default)]
    finish: Option<String>,
    #[serde(default)]
    mounting: Option<Mounting>,
    #[serde(default)]
    opening_degrees: Option<u16>,
    cup: RawCup,
    door_thickness: RawRange,
    plate: RawPlate,
    #[serde(default)]
    adjustment: Option<RawAdjustment>,
    #[serde(default)]
    fasteners: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    allow: Vec<RawAllow>,
    #[serde(default)]
    variants: Vec<RawVariant>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCup {
    diameter: f64,
    depth: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRange {
    min: f64,
    max: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawPlate {
    front_offset: f64,
    #[serde(default)]
    hole_pitch: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawAdjustment {
    #[serde(default)]
    vertical: Option<f64>,
    #[serde(default)]
    frontal: Option<f64>,
    #[serde(default)]
    overlay: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawAllow {
    pub warning: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawVariant {
    arm: HingeArm,
    code: String,
    #[serde(default)]
    plate_code: Option<String>,
    #[serde(default)]
    name: Option<BTreeMap<String, String>>,
    plate_height: f64,
    k_table: Vec<(f64, f64)>,
}

// ---- Loaded model ---------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Pack {
    pub id: String,
    pub manufacturer: String,
    pub country: Option<String>,
    pub version: String,
    pub review: Review,
    pub sources: Vec<Source>,
    pub hinges: Vec<HingeFamily>,
    pub slides: Vec<SlideFamily>,
    pub feet: Vec<FootFamily>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Review {
    pub status: ReviewStatus,
    pub by: Option<String>,
    pub date: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub id: String,
    pub title: String,
    pub url: String,
    pub sha256: String,
    pub revision: String,
    pub printed_page: Option<u16>,
    pub pdf_page: Option<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HingeFamily {
    pub id: String,
    /// Language tag (`en`, `pt-BR`) to display name.
    pub names: BTreeMap<String, String>,
    pub source: Source,
    pub attribution: Option<String>,
    pub soft_close: bool,
    pub finish: Option<String>,
    pub mounting: Option<Mounting>,
    pub opening_degrees: Option<u16>,
    pub cup_diameter: Length,
    pub cup_depth: Length,
    pub door_thickness_min: Length,
    pub door_thickness_max: Length,
    pub plate_front_offset: Length,
    /// Distance between the two plate screw centres; `None` when the source
    /// does not dimension it. Snapshots then carry zero: centre line only.
    pub plate_hole_pitch: Option<Length>,
    pub adjustment_vertical: Option<Length>,
    pub adjustment_frontal: Option<Length>,
    pub adjustment_overlay: Option<Length>,
    pub fasteners: Option<String>,
    pub notes: Option<String>,
    /// Warnings the pack author accepted, with the reason they gave.
    pub allowed: Vec<(String, String)>,
    pub variants: Vec<HingeVariant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HingeVariant {
    pub arm: HingeArm,
    pub code: String,
    pub plate_code: Option<String>,
    pub names: Option<BTreeMap<String, String>>,
    pub plate_height: Length,
    /// K → R for overlay arms, K → F for inset.
    pub k_table: Vec<(Length, Length)>,
}

impl HingeFamily {
    /// The name in `language`, else English, else any.
    pub fn name(&self, language: &str) -> &str {
        localized(&self.names, language)
    }
}

/// Millimetres without trailing zeros: "500", "12.7".
pub(crate) fn mm_text(value: Length) -> String {
    let um = value.micrometres();
    if um % 1000 == 0 {
        (um / 1000).to_string()
    } else {
        format!("{}", um as f64 / 1000.0)
    }
}

fn localized<'a>(names: &'a BTreeMap<String, String>, language: &str) -> &'a str {
    names
        .get(language)
        .or_else(|| names.get("en"))
        .or_else(|| names.values().next())
        .map_or("", String::as_str)
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadedPack {
    pub origin: PackOrigin,
    /// SHA-256 of the file bytes, shown so users can tell two copies apart.
    pub content_sha256: String,
    pub issues: Vec<PackIssue>,
    /// Present when the file parsed; usable only without errors.
    pub pack: Option<Pack>,
}

impl LoadedPack {
    pub fn errors(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.kind.severity() == Severity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.issues.len() - self.errors()
    }

    /// The pack's items can be added to a project only when it has no errors.
    pub fn usable(&self) -> Option<&Pack> {
        self.pack.as_ref().filter(|_| self.errors() == 0)
    }

    pub fn is_bundled(&self) -> bool {
        matches!(self.origin, PackOrigin::Bundled(_))
    }

    pub fn file_name(&self) -> String {
        match &self.origin {
            PackOrigin::Bundled(name) => (*name).into(),
            PackOrigin::User(path) => path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into(),
            ),
        }
    }
}

// ---- Loading and validation ----------------------------------------------

fn issue(issues: &mut Vec<PackIssue>, path: impl Into<String>, kind: IssueKind) {
    issues.push(PackIssue {
        path: path.into(),
        kind,
    });
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Exact conversion of a TOML millimetre number. `f64` display is the shortest
/// round-trip form, so `9.8` converts from the text "9.8", not a binary fraction.
fn length(
    issues: &mut Vec<PackIssue>,
    path: impl Into<String>,
    value: f64,
    allow_zero: bool,
) -> Length {
    let exact = value
        .is_finite()
        .then(|| Length::from_decimal(&format!("{value}"), Unit::Mm).ok())
        .flatten()
        .and_then(|c| c.exact())
        .filter(|l| l.micrometres() > 0 || (allow_zero && l.micrometres() == 0));
    exact.unwrap_or_else(|| {
        issue(issues, path, IssueKind::InvalidLength(format!("{value}")));
        Length::ZERO
    })
}

fn text(issues: &mut Vec<PackIssue>, path: impl Into<String>, field: &'static str, value: &str) {
    if value.trim().is_empty() {
        issue(issues, path, IssueKind::Missing(field));
    }
}

/// Parse and validate one pack. Never fails: problems are in `issues`.
pub fn load(origin: PackOrigin, bytes: &[u8]) -> LoadedPack {
    let content_sha256 = hex(&Sha256::digest(bytes));
    let mut issues = Vec::new();
    let pack = if bytes.len() > MAX_PACK_BYTES {
        issue(&mut issues, "", IssueKind::TooLarge);
        None
    } else {
        match std::str::from_utf8(bytes)
            .map_err(|e| e.to_string())
            .and_then(|s| toml::from_str::<RawPack>(s).map_err(|e| e.to_string()))
        {
            Ok(raw) => convert(raw, &mut issues),
            Err(message) => {
                issue(&mut issues, "", IssueKind::Syntax(message.trim().into()));
                None
            }
        }
    };
    LoadedPack {
        origin,
        content_sha256,
        issues,
        pack,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn convert(raw: RawPack, issues: &mut Vec<PackIssue>) -> Option<Pack> {
    if raw.schema != PACK_SCHEMA {
        issue(issues, "schema", IssueKind::UnsupportedSchema(raw.schema));
        return None;
    }
    if !valid_id(&raw.id) {
        issue(issues, "id", IssueKind::InvalidId(raw.id.clone()));
    }
    text(issues, "manufacturer", "manufacturer", &raw.manufacturer);
    text(issues, "version", "version", &raw.version);
    if raw.review.status == ReviewStatus::Draft {
        issue(issues, "review.status", IssueKind::Draft);
    }
    let mut source_ids = HashSet::new();
    let sources: Vec<Source> = raw
        .sources
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let path = format!("sources[{i}]");
            if !valid_id(&s.id) {
                issue(
                    issues,
                    format!("{path}.id"),
                    IssueKind::InvalidId(s.id.clone()),
                );
            } else if !source_ids.insert(s.id.clone()) {
                issue(
                    issues,
                    format!("{path}.id"),
                    IssueKind::DuplicateId(s.id.clone()),
                );
            }
            text(issues, format!("{path}.title"), "title", &s.title);
            text(issues, format!("{path}.url"), "url", &s.url);
            text(issues, format!("{path}.revision"), "revision", &s.revision);
            if s.sha256.len() != 64 || !s.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                issue(issues, format!("{path}.sha256"), IssueKind::InvalidSha256);
            }
            Source {
                id: s.id,
                title: s.title,
                url: s.url,
                sha256: s.sha256.to_ascii_lowercase(),
                revision: s.revision,
                printed_page: s.printed_page,
                pdf_page: s.pdf_page,
            }
        })
        .collect();
    let mut family_ids = HashSet::new();
    let mut codes = HashSet::new();
    let hinges = raw
        .hinges
        .into_iter()
        .enumerate()
        .filter_map(|(i, h)| {
            let path = format!("hinges[{i}]");
            if !valid_id(&h.id) {
                issue(
                    issues,
                    format!("{path}.id"),
                    IssueKind::InvalidId(h.id.clone()),
                );
            } else if !family_ids.insert(h.id.clone()) {
                issue(
                    issues,
                    format!("{path}.id"),
                    IssueKind::DuplicateId(h.id.clone()),
                );
            }
            hinge(h, &path, &sources, &mut codes, issues)
        })
        .collect();
    let status = raw.review.status;
    let slides = raw
        .drawer_slides
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let path = format!("drawer_slides[{i}]");
            family_id(issues, &path, &s.id, &mut family_ids);
            hardware::slide(s, &path, &sources, status, &mut codes, issues)
        })
        .collect();
    let feet = raw
        .feet
        .into_iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let path = format!("feet[{i}]");
            family_id(issues, &path, &f.id, &mut family_ids);
            hardware::foot(f, &path, &sources, status, &mut codes, issues)
        })
        .collect();
    Some(Pack {
        id: raw.id,
        manufacturer: raw.manufacturer,
        country: raw.country,
        version: raw.version,
        review: Review {
            status: raw.review.status,
            by: raw.review.by,
            date: raw.review.date,
            notes: raw.review.notes,
        },
        sources,
        hinges,
        slides,
        feet,
    })
}

fn family_id(issues: &mut Vec<PackIssue>, path: &str, id: &str, seen: &mut HashSet<String>) {
    if !valid_id(id) {
        issue(
            issues,
            format!("{path}.id"),
            IssueKind::InvalidId(id.into()),
        );
    } else if !seen.insert(id.into()) {
        issue(
            issues,
            format!("{path}.id"),
            IssueKind::DuplicateId(id.into()),
        );
    }
}

fn hinge(
    h: RawHinge,
    path: &str,
    sources: &[Source],
    codes: &mut HashSet<String>,
    issues: &mut Vec<PackIssue>,
) -> Option<HingeFamily> {
    if h.name.values().all(|n| n.trim().is_empty()) {
        issue(issues, format!("{path}.name"), IssueKind::Missing("name"));
    }
    let Some(source) = sources.iter().find(|s| s.id == h.source).cloned() else {
        issue(
            issues,
            format!("{path}.source"),
            IssueKind::UnknownSource(h.source.clone()),
        );
        return None;
    };
    let mut own = Vec::new();
    let cup_diameter = length(
        &mut own,
        format!("{path}.cup.diameter"),
        h.cup.diameter,
        false,
    );
    let cup_depth = length(&mut own, format!("{path}.cup.depth"), h.cup.depth, false);
    let door_min = length(
        &mut own,
        format!("{path}.door_thickness.min"),
        h.door_thickness.min,
        false,
    );
    let door_max = length(
        &mut own,
        format!("{path}.door_thickness.max"),
        h.door_thickness.max,
        false,
    );
    let front_offset = length(
        &mut own,
        format!("{path}.plate.front_offset"),
        h.plate.front_offset,
        false,
    );
    let hole_pitch = h
        .plate
        .hole_pitch
        .map(|v| length(&mut own, format!("{path}.plate.hole_pitch"), v, false));
    let adjustment = |own: &mut Vec<PackIssue>, name: &str, value: Option<f64>| {
        value.map(|v| length(own, format!("{path}.adjustment.{name}"), v, true))
    };
    let (vertical, frontal, overlay) = h.adjustment.as_ref().map_or((None, None, None), |a| {
        (
            adjustment(&mut own, "vertical", a.vertical),
            adjustment(&mut own, "frontal", a.frontal),
            adjustment(&mut own, "overlay", a.overlay),
        )
    });
    if own.is_empty() {
        if door_min > door_max {
            issue(
                &mut own,
                format!("{path}.door_thickness"),
                IssueKind::ThicknessRange,
            );
        }
        if cup_depth >= door_min {
            issue(
                &mut own,
                format!("{path}.cup.depth"),
                IssueKind::CupDeeperThanDoor,
            );
        }
    }
    match h.opening_degrees {
        None => issue(
            &mut own,
            format!("{path}.opening_degrees"),
            IssueKind::NoOpeningAngle,
        ),
        Some(d) if d == 0 || d > 180 => issue(
            &mut own,
            format!("{path}.opening_degrees"),
            IssueKind::OpeningAngle(d),
        ),
        Some(_) => {}
    }
    if h.variants.is_empty() {
        issue(&mut own, format!("{path}.variants"), IssueKind::NoVariants);
    }
    let variants = h
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
            let plate_height = length(
                &mut own,
                format!("{vpath}.plate_height"),
                v.plate_height,
                true,
            );
            if v.k_table.is_empty() {
                issue(
                    &mut own,
                    format!("{vpath}.k_table"),
                    IssueKind::Missing("k_table"),
                );
            }
            let k_table: Vec<_> = v
                .k_table
                .iter()
                .enumerate()
                .map(|(n, &(k, value))| {
                    let row = format!("{vpath}.k_table[{n}]");
                    (
                        length(&mut own, row.clone(), k, false),
                        length(&mut own, row, value, v.arm.is_inset()),
                    )
                })
                .collect();
            check_table(&mut own, &vpath, &k_table, cup_diameter);
            HingeVariant {
                arm: v.arm,
                code: v.code,
                plate_code: v.plate_code,
                names: v.name,
                plate_height,
                k_table,
            }
        })
        .collect();
    let allowed: Vec<(String, String)> =
        h.allow.into_iter().map(|a| (a.warning, a.reason)).collect();
    // An author may accept a specific warning for this family with a reason.
    own.retain(|i| {
        i.kind.severity() == Severity::Error
            || !allowed
                .iter()
                .any(|(code, reason)| code == i.kind.code() && !reason.trim().is_empty())
    });
    issues.extend(own);
    Some(HingeFamily {
        id: h.id,
        names: h.name,
        source,
        attribution: h.attribution,
        soft_close: h.soft_close,
        finish: h.finish,
        mounting: h.mounting,
        opening_degrees: h.opening_degrees,
        cup_diameter,
        cup_depth,
        door_thickness_min: door_min,
        door_thickness_max: door_max,
        plate_front_offset: front_offset,
        plate_hole_pitch: hole_pitch,
        adjustment_vertical: vertical,
        adjustment_frontal: frontal,
        adjustment_overlay: overlay,
        fasteners: h.fasteners,
        notes: h.notes,
        allowed,
        variants,
    })
}

fn check_table(
    issues: &mut Vec<PackIssue>,
    path: &str,
    table: &[(Length, Length)],
    cup_diameter: Length,
) {
    let path = format!("{path}.k_table");
    if table.windows(2).any(|w| w[0].0 >= w[1].0) {
        issue(issues, path.clone(), IssueKind::KNotIncreasing);
    }
    // A setback as large as the cup is never a hinge table: it catches K
    // typed in the wrong unit or swapped columns.
    if table.iter().any(|(k, _)| *k >= cup_diameter) {
        issue(issues, path.clone(), IssueKind::KOutsideRange);
    }
    let steps: Vec<i64> = table
        .windows(2)
        .map(|w| w[1].1.micrometres() - w[0].1.micrometres())
        .collect();
    if steps.iter().any(|s| *s > 0) && steps.iter().any(|s| *s < 0)
        || steps.contains(&0)
        || steps.windows(2).any(|w| w[0] != w[1])
    {
        issue(issues, path, IssueKind::NotMonotonic);
    }
}

/// Internal consistency of pinned facts, shared by projects and packs. This is
/// what "usable" means for a snapshot: the numbers are coherent, not that they
/// were reviewed (see `hardware_catalog::trust`).
pub fn facts_are_consistent(facts: &VerifiedHinge) -> bool {
    let positive = [
        facts.cup_diameter,
        facts.cup_depth,
        facts.door_thickness_min,
        facts.plate_front_offset,
    ];
    positive.iter().all(|l| l.micrometres() > 0)
        && facts.source_sha256.len() == 64
        && facts.source_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        && facts.plate_height.micrometres() >= 0
        && facts.plate_hole_pitch.micrometres() >= 0
        && facts.door_thickness_min <= facts.door_thickness_max
        && facts.cup_depth < facts.door_thickness_min
        && facts.opening_limit_degrees <= 180
        && !facts.overlay_by_cup_edge.is_empty()
        && facts
            .overlay_by_cup_edge
            .windows(2)
            .all(|w| w[0].cup_edge_setback < w[1].cup_edge_setback)
        && facts.overlay_by_cup_edge.iter().all(|s| {
            s.cup_edge_setback.micrometres() > 0
                && (s.overlay.micrometres() > 0
                    || (facts.arm.is_inset() && s.overlay.micrometres() == 0))
        })
}

/// Copy one variant into a project-owned snapshot with a fresh ID.
pub fn snapshot(
    pack: &Pack,
    family: &HingeFamily,
    variant: &HingeVariant,
    language: &str,
) -> CatalogReference {
    let source = &family.source;
    let name = variant.names.as_ref().map_or_else(
        || format!("{} {}", pack.manufacturer, family.name(language)),
        |names| localized(names, language).to_owned(),
    );
    let attribution = family.attribution.clone().unwrap_or_else(|| {
        let page = source
            .printed_page
            .map_or_else(String::new, |p| format!(", p. {p}"));
        format!("{}{page}", source.title)
    });
    CatalogReference {
        id: Uuid::new_v4(),
        name,
        product_id: variant.code.clone(),
        plate_id: variant.plate_code.clone(),
        source: source.url.clone(),
        revision: format!("{}; SHA-256 {}", source.revision, source.sha256),
        installation_dimensions: Default::default(),
        item: None,
        verified_hinge: Some(VerifiedHinge {
            printed_page: source.printed_page.unwrap_or(0),
            pdf_page: source.pdf_page.unwrap_or(0),
            source_sha256: source.sha256.clone(),
            attribution,
            arm: variant.arm,
            plate_height: variant.plate_height,
            overlay_by_cup_edge: variant
                .k_table
                .iter()
                .map(|&(cup_edge_setback, overlay)| OverlaySetting {
                    cup_edge_setback,
                    overlay,
                })
                .collect(),
            door_thickness_min: family.door_thickness_min,
            door_thickness_max: family.door_thickness_max,
            cup_diameter: family.cup_diameter,
            cup_depth: family.cup_depth,
            plate_hole_pitch: family.plate_hole_pitch.unwrap_or(Length::ZERO),
            plate_front_offset: family.plate_front_offset,
            opening_limit_degrees: family.opening_degrees.unwrap_or(0),
            screw_details: UnavailableDetail::Unavailable,
        }),
        origin: Some(CatalogOrigin {
            pack_id: pack.id.clone(),
            pack_version: pack.version.clone(),
            manufacturer: pack.manufacturer.clone(),
            item_id: family.id.clone(),
            variant_code: variant.code.clone(),
        }),
    }
}

// ---- Registry -------------------------------------------------------------

/// Every pack the application knows about: bundled first, then the user's.
#[derive(Clone, Debug, Default)]
pub struct CatalogRegistry {
    pub packs: Vec<LoadedPack>,
    /// Folder scanned for user packs, if one was given.
    pub user_dir: Option<PathBuf>,
}

impl CatalogRegistry {
    pub fn bundled() -> Self {
        Self {
            packs: BUNDLED
                .iter()
                .map(|(name, text)| load(PackOrigin::Bundled(name), text.as_bytes()))
                .collect(),
            user_dir: None,
        }
    }

    /// Bundled packs plus every `*.toml` in `user_dir`, sorted by file name.
    /// A missing folder is not an error: there are simply no user packs.
    pub fn load(user_dir: Option<&Path>) -> Self {
        let mut registry = Self::bundled();
        registry.user_dir = user_dir.map(Path::to_path_buf);
        let Some(dir) = user_dir else {
            return registry;
        };
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
            })
            .collect();
        files.sort();
        for path in files {
            registry.packs.push(load_file(&path));
        }
        registry.mark_shadowed();
        registry
    }

    fn mark_shadowed(&mut self) {
        let bundled: Vec<String> = self
            .packs
            .iter()
            .filter(|p| p.is_bundled())
            .filter_map(|p| p.pack.as_ref().map(|p| p.id.clone()))
            .collect();
        for loaded in self.packs.iter_mut().filter(|p| !p.is_bundled()) {
            if loaded
                .pack
                .as_ref()
                .is_some_and(|p| bundled.contains(&p.id))
            {
                issue(&mut loaded.issues, "id", IssueKind::ShadowsBundled);
            }
        }
    }

    /// The pack in effect for an id: a usable user pack replaces a bundled one.
    pub fn pack(&self, id: &str) -> Option<&Pack> {
        let mut found = None;
        for pack in self.packs.iter().filter_map(LoadedPack::usable) {
            if pack.id == id {
                found = Some(pack);
            }
        }
        found
    }

    /// Usable packs in effect, one per id.
    pub fn usable(&self) -> Vec<&Pack> {
        let mut seen = HashSet::new();
        let mut packs: Vec<&Pack> = self
            .packs
            .iter()
            .rev()
            .filter_map(LoadedPack::usable)
            .filter(|p| seen.insert(p.id.clone()))
            .collect();
        packs.reverse();
        packs
    }

    pub fn find_variant(
        &self,
        origin: &CatalogOrigin,
    ) -> Option<(&Pack, &HingeFamily, &HingeVariant)> {
        let pack = self.pack(&origin.pack_id)?;
        let family = pack.hinges.iter().find(|f| f.id == origin.item_id)?;
        let variant = family
            .variants
            .iter()
            .find(|v| v.code == origin.variant_code)?;
        Some((pack, family, variant))
    }

    pub fn find_slide(
        &self,
        origin: &CatalogOrigin,
    ) -> Option<(&Pack, &SlideFamily, &SlideVariant)> {
        let pack = self.pack(&origin.pack_id)?;
        let family = pack.slides.iter().find(|f| f.id == origin.item_id)?;
        let variant = family
            .variants
            .iter()
            .find(|v| v.code == origin.variant_code)?;
        Some((pack, family, variant))
    }

    pub fn find_foot(&self, origin: &CatalogOrigin) -> Option<(&Pack, &FootFamily, &FootVariant)> {
        let pack = self.pack(&origin.pack_id)?;
        let family = pack.feet.iter().find(|f| f.id == origin.item_id)?;
        let variant = family
            .variants
            .iter()
            .find(|v| v.code == origin.variant_code)?;
        Some((pack, family, variant))
    }

    /// A slide family in effect, by pack and family id.
    pub fn slide_family(&self, pack: &str, family: &str) -> Option<(&Pack, &SlideFamily)> {
        let pack = self.pack(pack)?;
        Some((pack, pack.slides.iter().find(|f| f.id == family)?))
    }

    /// Every length of a slide family as snapshots, shortest first.
    pub fn slide_lengths(&self, pack: &str, family: &str, language: &str) -> Vec<CatalogReference> {
        self.slide_family(pack, family)
            .map(|(pack, family)| {
                let mut lengths: Vec<_> = family
                    .variants
                    .iter()
                    .map(|v| snapshot_slide(pack, family, v, language))
                    .collect();
                lengths.sort_by_key(|c| c.slide().map(|s| s.length));
                lengths
            })
            .unwrap_or_default()
    }
}

pub fn load_file(path: &Path) -> LoadedPack {
    let origin = PackOrigin::User(path.to_path_buf());
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_PACK_BYTES as u64 => LoadedPack {
            origin,
            content_sha256: String::new(),
            issues: vec![PackIssue {
                path: String::new(),
                kind: IssueKind::TooLarge,
            }],
            pack: None,
        },
        _ => match std::fs::read(path) {
            Ok(bytes) => load(origin, &bytes),
            Err(e) => LoadedPack {
                origin,
                content_sha256: String::new(),
                issues: vec![PackIssue {
                    path: String::new(),
                    kind: IssueKind::Unreadable(e.to_string()),
                }],
                pack: None,
            },
        },
    }
}

#[cfg(test)]
mod tests;
