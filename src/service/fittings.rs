//! Drawer slides and feet: catalog browsing, new models, slide installations,
//! feet placement and drawer-opening pictures.
use std::collections::{BTreeMap, HashMap};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::catalog_pack::{
    self, FootFamily, Pack, RawClearance, RawFoot, RawFootVariant, RawGlide, RawHole, RawSection,
    RawShape, RawSlide, RawSlideVariant, SlideFamily, snapshot_foot, snapshot_slide,
};
use crate::commands::ProjectEditor;
use crate::domain::{
    CatalogReference, FootShape, FootSpec, Project, Section, SlideExtension, SlideInstallation,
};
use crate::hardware_catalog;
use crate::service::dto::{Change, ColorInput, LengthInput, PoseInput, color_hex, mm_f64};
use crate::service::error::{ErrorCode, ServiceError, ServiceResult};
use crate::service::hardware::trust_name;
use crate::service::vision::{RenderViewInput, Rendered};
use crate::service::workspace::{Kind, Workspace, object_name};
use crate::slide_installation::{self, SlideIssue, SlideNotice, SlideStatus, Suggestion};
use crate::units::{Length, Pose};
use crate::user_pack;

// ------------------------------------------------------------------ inputs

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HardwareKindName {
    Hinge,
    Slide,
    Foot,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct ListHardwareCatalogInput {
    /// Only this kind; default all.
    #[serde(default)]
    pub kind: Option<HardwareKindName>,
    /// Re-read user packs from disk.
    #[serde(default)]
    pub reload: bool,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct AddCatalogSlideInput {
    /// Pack id; default the first pack with the family.
    #[serde(default)]
    pub pack: Option<String>,
    /// Slide family id, e.g. "tt45-slowmotion" (default that one).
    #[serde(default)]
    pub slide: Option<String>,
    /// Product code of one length, e.g. "0073.045500SX".
    #[serde(default)]
    pub variant: Option<String>,
    /// Nominal length instead of a code.
    #[serde(default)]
    pub length: Option<LengthInput>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct AddCatalogFootInput {
    #[serde(default)]
    pub pack: Option<String>,
    /// Foot family id, e.g. "post-square".
    #[serde(default)]
    pub foot: Option<String>,
    /// Product code, e.g. "generic-post-square-100".
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ClearanceInput {
    /// Gap between drawer box side and cabinet side, per side.
    pub nominal: LengthInput,
    /// Allowed below nominal (default 0).
    #[serde(default)]
    pub minus: Option<LengthInput>,
    /// Allowed above nominal (default 0).
    #[serde(default)]
    pub plus: Option<LengthInput>,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionName {
    Full,
    Partial,
    Over,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SlideLengthInput {
    /// Nominal length (the closed cabinet member).
    pub length: LengthInput,
    /// Product code; default "<model id>-<length>".
    #[serde(default)]
    pub code: Option<String>,
    /// How far it opens; default the length.
    #[serde(default)]
    pub travel: Option<LengthInput>,
    /// Holes on the cabinet member, mm from its front end (centre line).
    #[serde(default)]
    pub cabinet_holes: Vec<LengthInput>,
    /// Holes on the drawer member, mm from its front end.
    #[serde(default)]
    pub drawer_holes: Vec<LengthInput>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CreateSlideModelInput {
    pub name: String,
    /// Model id (lowercase letters, digits, '-'); default from the name.
    #[serde(default)]
    pub id: Option<String>,
    /// Profile height.
    pub height: LengthInput,
    pub clearance: ClearanceInput,
    /// Slide front behind the cabinet front edge (default 2).
    #[serde(default)]
    pub front_setback: Option<LengthInput>,
    #[serde(default)]
    pub extension: Option<ExtensionName>,
    #[serde(default)]
    pub soft_close: bool,
    #[serde(default)]
    pub capacity_kg: Option<u16>,
    #[serde(default)]
    pub finish: Option<String>,
    /// Screws and mounting notes.
    #[serde(default)]
    pub fixing: Option<String>,
    #[serde(default)]
    pub rear_fixing: Option<String>,
    /// Where the data came from (a sheet, a listing).
    #[serde(default)]
    pub attribution: Option<String>,
    pub lengths: Vec<SlideLengthInput>,
    /// Also write the model to the app's user catalog (user-models.toml) so
    /// the app and future projects offer it.
    #[serde(default)]
    pub save_to_catalog: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

/// Round (`diameter`) or rectangular (`width` along X by `depth` along Y).
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct SectionInput {
    #[serde(default)]
    pub diameter: Option<LengthInput>,
    #[serde(default)]
    pub width: Option<LengthInput>,
    #[serde(default)]
    pub depth: Option<LengthInput>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct GlideInput {
    pub diameter: LengthInput,
    pub height: LengthInput,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FootShapeName {
    /// A solid frustum (plastic feet): top, bottom, height.
    Tapered,
    /// Tube with a top plate and an optional glide (chrome feet, straight
    /// table legs): tube, plate, plate_thickness, glide, height.
    Post,
    /// Closed tube frame in the X–Z plane (industrial legs): top_width,
    /// bottom_width (smaller = trapezoid, = tube_width = V), tube_width,
    /// tube_depth, crossbar_height, glide, height.
    Frame,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct FootShapeInput {
    pub kind: FootShapeName,
    /// Total height, floor to mounting face.
    pub height: LengthInput,
    #[serde(default)]
    pub top: Option<SectionInput>,
    #[serde(default)]
    pub bottom: Option<SectionInput>,
    #[serde(default)]
    pub tube: Option<SectionInput>,
    #[serde(default)]
    pub plate: Option<SectionInput>,
    #[serde(default)]
    pub plate_thickness: Option<LengthInput>,
    #[serde(default)]
    pub glide: Option<GlideInput>,
    #[serde(default)]
    pub top_width: Option<LengthInput>,
    #[serde(default)]
    pub bottom_width: Option<LengthInput>,
    #[serde(default)]
    pub tube_width: Option<LengthInput>,
    #[serde(default)]
    pub tube_depth: Option<LengthInput>,
    #[serde(default)]
    pub crossbar_height: Option<LengthInput>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CreateFootModelInput {
    pub name: String,
    /// Model id (lowercase letters, digits, '-'); default from the name.
    #[serde(default)]
    pub id: Option<String>,
    /// Product code; default the model id.
    #[serde(default)]
    pub code: Option<String>,
    pub shape: FootShapeInput,
    /// "#rrggbb" or [r, g, b]; default dark grey.
    #[serde(default)]
    pub color: Option<ColorInput>,
    #[serde(default)]
    pub finish: Option<String>,
    /// Levelling travel.
    #[serde(default)]
    pub adjustment: Option<LengthInput>,
    /// Screw positions on the mounting face, [x, y] mm from its centre.
    #[serde(default)]
    pub mounting_holes: Vec<[f64; 2]>,
    #[serde(default)]
    pub load_kg: Option<u16>,
    #[serde(default)]
    pub attribution: Option<String>,
    #[serde(default)]
    pub save_to_catalog: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SuggestSlidesInput {
    /// The drawer: its assembly, or any board of it.
    pub drawer: String,
    /// A pinned slide entry (any length of the family).
    #[serde(default)]
    pub catalog: Option<String>,
    #[serde(default)]
    pub pack: Option<String>,
    /// Slide family id; default the family of a pinned slide, else TT45.
    #[serde(default)]
    pub slide: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct AddSlidesInput {
    /// The drawer: its assembly, or any board of it.
    pub drawer: String,
    /// A pinned slide entry to use as is (fixes the length).
    #[serde(default)]
    pub catalog: Option<String>,
    #[serde(default)]
    pub pack: Option<String>,
    /// Slide family id; default the family of a pinned slide, else TT45.
    #[serde(default)]
    pub slide: Option<String>,
    /// Nominal length; default the longest that fits.
    #[serde(default)]
    pub length: Option<LengthInput>,
    /// Slide centre line above the box side's bottom edge; default centred.
    #[serde(default)]
    pub height: Option<LengthInput>,
    /// Slide front behind the carcass front edge; default the slide's.
    #[serde(default)]
    pub setback: Option<LengthInput>,
    /// Override detected boards: [left, right] box sides and carcass sides.
    #[serde(default)]
    pub box_sides: Option<[String; 2]>,
    #[serde(default)]
    pub carcass_sides: Option<[String; 2]>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct UpdateSlideInput {
    /// The slide pair (its drawer's name works).
    pub slide: String,
    /// Another length of the same family, or pass catalog.
    #[serde(default)]
    pub length: Option<LengthInput>,
    #[serde(default)]
    pub catalog: Option<String>,
    #[serde(default)]
    pub height: Option<LengthInput>,
    #[serde(default)]
    pub setback: Option<LengthInput>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SlideRefInput {
    pub slide: String,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct ListSlidesInput {
    #[serde(default)]
    pub drawer: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FootAnchor {
    /// The pose is the minimum corner of the foot's box (floor level).
    #[default]
    Origin,
    /// The pose is the centre of the mounting face (under the furniture).
    TopCenter,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct AddFootInput {
    /// A pinned foot entry, or a catalog product code / family id (pinned
    /// automatically), e.g. "generic-post-square-100".
    pub model: String,
    #[serde(default)]
    pub name: Option<String>,
    /// Parent assembly (the foot moves with it).
    #[serde(default)]
    pub parent: Option<String>,
    /// World pose; see anchor.
    #[serde(default)]
    pub pose: Option<PoseInput>,
    #[serde(default)]
    pub anchor: FootAnchor,
    /// Place copies at each of these world positions instead of `pose`
    /// (same rotation), e.g. the four corners.
    #[serde(default)]
    pub positions: Vec<[f64; 3]>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct UpdateFootInput {
    pub foot: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub pose: Option<PoseInput>,
    #[serde(default)]
    pub anchor: FootAnchor,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct DrawerOpeningInput {
    /// The drawer (its assembly or a board of it) or its slides.
    pub drawer: String,
    /// 0 = closed, 1 = fully open (default 1).
    #[serde(default)]
    pub fraction: Option<f64>,
    /// Or a distance.
    #[serde(default)]
    pub distance: Option<LengthInput>,
    #[serde(flatten)]
    pub view: RenderViewInput,
}

// ------------------------------------------------------------------ json

fn section_json(section: Section) -> Value {
    match section {
        Section::Round { diameter } => json!({ "diameter": mm_f64(diameter) }),
        Section::Rect { width, depth } => json!({ "width": mm_f64(width), "depth": mm_f64(depth) }),
    }
}

fn shape_json(shape: &FootShape) -> Value {
    match shape {
        FootShape::Tapered {
            top,
            bottom,
            height,
        } => json!({
            "kind": "tapered", "height": mm_f64(*height),
            "top": section_json(*top), "bottom": section_json(*bottom),
        }),
        FootShape::Post {
            tube,
            plate,
            plate_thickness,
            glide,
            height,
        } => json!({
            "kind": "post", "height": mm_f64(*height),
            "tube": section_json(*tube), "plate": section_json(*plate),
            "plate_thickness": mm_f64(*plate_thickness),
            "glide": glide.map(|g| json!({ "diameter": mm_f64(g.diameter), "height": mm_f64(g.height) })),
        }),
        FootShape::Frame {
            height,
            top_width,
            bottom_width,
            tube_width,
            tube_depth,
            crossbar_height,
            glide,
        } => json!({
            "kind": "frame", "height": mm_f64(*height),
            "top_width": mm_f64(*top_width), "bottom_width": mm_f64(*bottom_width),
            "tube_width": mm_f64(*tube_width), "tube_depth": mm_f64(*tube_depth),
            "crossbar_height": crossbar_height.map(mm_f64),
            "glide": glide.map(|g| json!({ "diameter": mm_f64(g.diameter), "height": mm_f64(g.height) })),
        }),
    }
}

fn foot_spec_json(spec: &FootSpec) -> Value {
    json!({
        "shape": shape_json(&spec.shape),
        "box_mm": spec.local_size().map(mm_f64),
        "color": color_hex(spec.color),
        "finish": spec.finish,
        "adjustment_mm": mm_f64(spec.adjustment),
        "mounting_holes_mm": spec.mounting_holes.iter().map(|h| h.map(mm_f64)).collect::<Vec<_>>(),
        "load_kg": spec.load_kg,
    })
}

fn extension_name(extension: SlideExtension) -> &'static str {
    match extension {
        SlideExtension::Full => "full",
        SlideExtension::Partial => "partial",
        SlideExtension::Over => "over",
    }
}

fn slide_family_json(pack: &Pack, family: &SlideFamily, language: &str) -> Value {
    json!({
        "pack": pack.id,
        "slide": family.id,
        "name": family.name(language),
        "height_mm": mm_f64(family.height),
        "clearance_mm": { "nominal": mm_f64(family.clearance), "minus": mm_f64(family.clearance_minus), "plus": mm_f64(family.clearance_plus) },
        "front_setback_mm": mm_f64(family.front_setback),
        "extension": extension_name(family.extension),
        "soft_close": family.soft_close,
        "capacity_kg": family.capacity_kg,
        "fixing": family.fixing,
        "rear_fixing": family.rear_fixing,
        "lengths": family.variants.iter().map(|v| json!({
            "variant": v.code,
            "length_mm": mm_f64(v.length),
            "travel_mm": mm_f64(v.travel),
            "capacity_kg": v.capacity_kg.or(family.capacity_kg),
            "finish": v.finish.clone().or_else(|| family.finish.clone()),
            "cabinet_holes_mm": v.cabinet_holes.iter().map(|h| mm_f64(h.along)).collect::<Vec<_>>(),
            "drawer_holes_mm": v.drawer_holes.iter().map(|h| mm_f64(h.along)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn foot_family_json(pack: &Pack, family: &FootFamily, language: &str) -> Value {
    json!({
        "pack": pack.id,
        "foot": family.id,
        "name": family.name(language),
        "generic": pack.review.status == catalog_pack::ReviewStatus::Generic,
        "notes": family.notes,
        "variants": family.variants.iter().map(|v| {
            let spec = family.spec(v);
            json!({
                "variant": v.code,
                "name": v.names.as_ref().map(|n| n.get(language).or_else(|| n.get("en")).cloned()),
                "spec": foot_spec_json(&spec),
            })
        }).collect::<Vec<_>>(),
    })
}

pub(crate) fn slide_entry_json(project: &Project, entry: &CatalogReference) -> Value {
    let spec = entry.slide();
    json!({
        "catalog_id": entry.id,
        "kind": "slide",
        "name": entry.name,
        "product": entry.product_id,
        "trust": trust_name(entry),
        "family": entry.origin.as_ref().map(|o| format!("{}/{}", o.pack_id, o.item_id)),
        "length_mm": spec.map(|s| mm_f64(s.length)),
        "travel_mm": spec.map(|s| mm_f64(s.travel)),
        "height_mm": spec.map(|s| mm_f64(s.height)),
        "clearance_mm": spec.map(|s| json!({ "nominal": mm_f64(s.clearance), "minus": mm_f64(s.clearance_minus), "plus": mm_f64(s.clearance_plus) })),
        "slides_using_it": project.slide_installations.iter().filter(|s| s.catalog_id == entry.id).count(),
    })
}

pub(crate) fn foot_entry_json(project: &Project, entry: &CatalogReference) -> Value {
    json!({
        "catalog_id": entry.id,
        "kind": "foot",
        "name": entry.name,
        "product": entry.product_id,
        "trust": trust_name(entry),
        "spec": entry.foot().map(foot_spec_json),
        "feet_using_it": project.hardware.iter().filter(|h| matches!(h.kind, crate::domain::HardwareKind::Catalog { catalog_id } if catalog_id == entry.id)).count(),
    })
}

fn issue_text(issue: &SlideIssue) -> String {
    let side = |s: &usize| if *s == 0 { "left" } else { "right" };
    match issue {
        SlideIssue::MissingPart(id) => format!("board {id} no longer exists"),
        SlideIssue::MissingCatalog => "the slide's catalog entry is missing".into(),
        SlideIssue::NotParallel => {
            "the box sides and carcass sides are not upright and parallel, or the drawer does not run along them".into()
        }
        SlideIssue::ClearanceOutOfRange { side: s, measured } => format!(
            "{} gap between box side and carcass side is {} mm, outside the slide's clearance",
            side(s),
            mm_f64(*measured)
        ),
        SlideIssue::TooLongForDepth { side: s, available } => format!(
            "{} carcass side has only {} mm behind the setback: use a shorter slide",
            side(s),
            mm_f64(*available)
        ),
        SlideIssue::LongerThanDrawer { side: s, available } => format!(
            "{} box side has only {} mm for the drawer member: use a shorter slide or a deeper box",
            side(s),
            mm_f64(*available)
        ),
        SlideIssue::HeightExceedsBoxSide => {
            "the slide does not fit on the box side at this height".into()
        }
        SlideIssue::OutsideMount { side: s } => {
            format!("the slide falls beyond the {} carcass side", side(s))
        }
        SlideIssue::Misaligned => "the two slides are not at the same height or depth".into(),
    }
}

pub(crate) fn slide_status_json(
    project: &Project,
    installation: &SlideInstallation,
    status: &SlideStatus,
) -> Value {
    let catalog = project
        .catalog
        .iter()
        .find(|c| c.id == installation.catalog_id);
    let sides = status.references.as_ref().map(|r| {
        r.sides
            .iter()
            .enumerate()
            .map(|(i, s)| {
                json!({
                    "side": if i == 0 { "left" } else { "right" },
                    "gap_mm": mm_f64(s.gap),
                    "cabinet_board": object_name(project, s.cabinet_board),
                    "cabinet_holes_from_front_mm": s.cabinet_hole_distances.iter().map(|d| mm_f64(*d)).collect::<Vec<_>>(),
                    "cabinet_centre_line_from_bottom_mm": mm_f64(s.cabinet_centre_from_bottom),
                    "drawer_board": object_name(project, s.drawer_board),
                    "drawer_holes_from_front_mm": s.drawer_hole_distances.iter().map(|d| mm_f64(*d)).collect::<Vec<_>>(),
                    "drawer_centre_line_from_bottom_mm": mm_f64(s.drawer_centre_from_bottom),
                })
            })
            .collect::<Vec<_>>()
    });
    json!({
        "slide_id": installation.id,
        "drawer": object_name(project, installation.drawer_root_id),
        "product": catalog.map(|c| c.product_id.clone()),
        "name": catalog.map(|c| c.name.clone()),
        "length_mm": catalog.and_then(CatalogReference::slide).map(|s| mm_f64(s.length)),
        "travel_mm": catalog.and_then(CatalogReference::slide).map(|s| mm_f64(s.travel)),
        "height_mm": mm_f64(installation.height),
        "setback_mm": mm_f64(installation.setback),
        "ok": status.issues.is_empty(),
        "issues": status.issues.iter().map(issue_text).collect::<Vec<_>>(),
        "notices": status.notices.iter().map(|n| match n {
            SlideNotice::PartialExtension { travel } => format!("partial extension: opens {} mm", mm_f64(*travel)),
        }).collect::<Vec<_>>(),
        "sides": sides,
        "note": "Hole distances are reference information from the catalog; check the manufacturer's sheet before drilling.",
    })
}

// --------------------------------------------------------------- raw models

fn mm_number(length: Length) -> f64 {
    length.micrometres() as f64 / 1000.0
}

fn length_of(
    value: &LengthInput,
    field: &str,
    rounding: bool,
    zero_ok: bool,
) -> ServiceResult<f64> {
    let l = if zero_ok {
        value.non_negative(field, rounding)?
    } else {
        value.positive(field, rounding)?
    };
    Ok(mm_number(l))
}

fn names(name: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("en".to_owned(), name.to_owned()),
        ("pt-BR".to_owned(), name.to_owned()),
    ])
}

fn pack_issues(error: user_pack::UserPackError) -> ServiceError {
    ServiceError::new(ErrorCode::CatalogError, error.to_string())
        .hint("Fix the listed fields; paths name the value (e.g. shape.plate_thickness).")
}

fn raw_section(
    input: &Option<SectionInput>,
    field: &str,
    r: bool,
) -> ServiceResult<Option<RawSection>> {
    input
        .as_ref()
        .map(|s| {
            Ok(RawSection {
                diameter: s
                    .diameter
                    .as_ref()
                    .map(|v| length_of(v, &format!("{field}.diameter"), r, false))
                    .transpose()?,
                width: s
                    .width
                    .as_ref()
                    .map(|v| length_of(v, &format!("{field}.width"), r, false))
                    .transpose()?,
                depth: s
                    .depth
                    .as_ref()
                    .map(|v| length_of(v, &format!("{field}.depth"), r, false))
                    .transpose()?,
            })
        })
        .transpose()
}

fn raw_foot(input: &CreateFootModelInput, id: &str) -> ServiceResult<RawFoot> {
    let r = input.allow_rounding;
    let opt = |v: &Option<LengthInput>, field: &str| {
        v.as_ref()
            .map(|v| length_of(v, field, r, false))
            .transpose()
    };
    let shape = &input.shape;
    let raw_shape = RawShape {
        kind: match shape.kind {
            FootShapeName::Tapered => "tapered",
            FootShapeName::Post => "post",
            FootShapeName::Frame => "frame",
        }
        .into(),
        height: Some(length_of(&shape.height, "shape.height", r, false)?),
        top: raw_section(&shape.top, "shape.top", r)?,
        bottom: raw_section(&shape.bottom, "shape.bottom", r)?,
        tube: raw_section(&shape.tube, "shape.tube", r)?,
        plate: raw_section(&shape.plate, "shape.plate", r)?,
        plate_thickness: opt(&shape.plate_thickness, "shape.plate_thickness")?,
        glide: shape
            .glide
            .as_ref()
            .map(|g| -> ServiceResult<RawGlide> {
                Ok(RawGlide {
                    diameter: length_of(&g.diameter, "shape.glide.diameter", r, false)?,
                    height: length_of(&g.height, "shape.glide.height", r, false)?,
                })
            })
            .transpose()?,
        top_width: opt(&shape.top_width, "shape.top_width")?,
        bottom_width: opt(&shape.bottom_width, "shape.bottom_width")?,
        tube_width: opt(&shape.tube_width, "shape.tube_width")?,
        tube_depth: opt(&shape.tube_depth, "shape.tube_depth")?,
        crossbar_height: opt(&shape.crossbar_height, "shape.crossbar_height")?,
    };
    let color = input
        .color
        .as_ref()
        .map(ColorInput::resolve)
        .transpose()?
        .unwrap_or(crate::domain::SrgbColor([38, 38, 40]));
    Ok(RawFoot {
        id: id.to_owned(),
        name: names(&input.name),
        source: None,
        attribution: Some(
            input
                .attribution
                .clone()
                .unwrap_or_else(|| "User-supplied model".into()),
        ),
        shape: raw_shape,
        color: color_hex(color),
        finish: input.finish.clone(),
        adjustment: input
            .adjustment
            .as_ref()
            .map(|v| length_of(v, "adjustment", r, true))
            .transpose()?
            .unwrap_or(0.0),
        mounting_holes: input.mounting_holes.iter().map(|h| (h[0], h[1])).collect(),
        load_kg: input.load_kg,
        notes: None,
        variants: vec![RawFootVariant {
            code: input.code.clone().unwrap_or_else(|| id.to_owned()),
            ..Default::default()
        }],
    })
}

fn raw_slide(input: &CreateSlideModelInput, id: &str) -> ServiceResult<RawSlide> {
    let r = input.allow_rounding;
    let holes = |list: &[LengthInput], field: &str| -> ServiceResult<Vec<RawHole>> {
        list.iter()
            .map(|h| Ok(RawHole::Along(length_of(h, field, r, true)?)))
            .collect()
    };
    let variants = input
        .lengths
        .iter()
        .map(|l| -> ServiceResult<RawSlideVariant> {
            let length = length_of(&l.length, "lengths.length", r, false)?;
            Ok(RawSlideVariant {
                code: l.code.clone().unwrap_or_else(|| format!("{id}-{length}")),
                name: Some(names(&format!("{} {length} mm", input.name))),
                length,
                travel: l
                    .travel
                    .as_ref()
                    .map(|t| length_of(t, "lengths.travel", r, false))
                    .transpose()?
                    .unwrap_or(length),
                cabinet_holes: holes(&l.cabinet_holes, "lengths.cabinet_holes")?,
                drawer_holes: holes(&l.drawer_holes, "lengths.drawer_holes")?,
                capacity_kg: None,
                finish: None,
            })
        })
        .collect::<ServiceResult<Vec<_>>>()?;
    let zero = LengthInput::Millimetres(0.0);
    Ok(RawSlide {
        id: id.to_owned(),
        name: names(&input.name),
        source: None,
        attribution: Some(
            input
                .attribution
                .clone()
                .unwrap_or_else(|| "User-supplied model".into()),
        ),
        height: length_of(&input.height, "height", r, false)?,
        clearance: RawClearance {
            nominal: length_of(&input.clearance.nominal, "clearance.nominal", r, false)?,
            minus: length_of(
                input.clearance.minus.as_ref().unwrap_or(&zero),
                "clearance.minus",
                r,
                true,
            )?,
            plus: length_of(
                input.clearance.plus.as_ref().unwrap_or(&zero),
                "clearance.plus",
                r,
                true,
            )?,
        },
        front_setback: input
            .front_setback
            .as_ref()
            .map(|v| length_of(v, "front_setback", r, true))
            .transpose()?
            .unwrap_or(2.0),
        extension: match input.extension {
            Some(ExtensionName::Partial) => SlideExtension::Partial,
            Some(ExtensionName::Over) => SlideExtension::Over,
            _ => SlideExtension::Full,
        },
        members: Some(3),
        soft_close: input.soft_close,
        capacity_kg: input.capacity_kg,
        finish: input.finish.clone(),
        fixing: input.fixing.clone(),
        rear_fixing: input.rear_fixing.clone(),
        notes: None,
        allow: Vec::new(),
        variants,
    })
}

/// A slide family's lengths as snapshots: from the registry or, for a
/// pinned entry, its pinned siblings (same origin family).
fn family_lengths(
    ws: &Workspace,
    project: &Project,
    catalog: Option<Uuid>,
    pack: Option<&str>,
    slide: Option<&str>,
) -> ServiceResult<Vec<CatalogReference>> {
    let language = ws.language.tag();
    if let Some(id) = catalog {
        let entry = project
            .catalog
            .iter()
            .find(|c| c.id == id)
            .filter(|c| c.slide().is_some())
            .ok_or_else(|| ServiceError::invalid("that catalog entry is not a slide"))?;
        return Ok(vec![entry.clone()]);
    }
    let wanted = slide.map(str::to_owned).or_else(|| {
        // Default to the family of a slide already pinned.
        project
            .catalog
            .iter()
            .filter(|c| c.slide().is_some())
            .find_map(|c| c.origin.as_ref().map(|o| o.item_id.clone()))
    });
    let (default_pack, default_family) = hardware_catalog::DEFAULT_SLIDE_FAMILY;
    let family = wanted.unwrap_or_else(|| default_family.to_owned());
    for p in ws.catalogs.usable() {
        if pack.is_some_and(|wanted| wanted != p.id) {
            continue;
        }
        if p.slides.iter().any(|f| f.id == family) {
            return Ok(ws.catalogs.slide_lengths(&p.id, &family, language));
        }
    }
    // A family pinned from a pack that is not loaded here.
    let pinned: Vec<_> = project
        .catalog
        .iter()
        .filter(|c| c.slide().is_some() && c.origin.as_ref().is_some_and(|o| o.item_id == family))
        .cloned()
        .collect();
    if pinned.is_empty() {
        Err(ServiceError::not_found(
            "slide family",
            format!("{}/{family}", pack.unwrap_or(default_pack)),
        )
        .hint("See list_hardware_catalog {\"kind\":\"slide\"}."))
    } else {
        Ok(pinned)
    }
}

/// Reuse a pinned entry with the same facts, else pin `entry` as given.
fn pinned_or_new(project: &Project, entry: &CatalogReference) -> (Uuid, Option<CatalogReference>) {
    project
        .catalog
        .iter()
        .find(|c| {
            c.product_id == entry.product_id && c.item == entry.item && c.origin == entry.origin
        })
        .map_or_else(
            || (entry.id, Some(entry.clone())),
            |existing| (existing.id, None),
        )
}

impl Workspace {
    // ----------------------------------------------------------- catalogs

    pub fn list_hardware_catalog(
        &mut self,
        input: ListHardwareCatalogInput,
    ) -> ServiceResult<Value> {
        if input.reload {
            self.catalogs = crate::catalog_pack::CatalogRegistry::load(self.catalog_dir.as_deref());
        }
        let language = self.language.tag().to_owned();
        let want = |kind| input.kind.is_none_or(|k| k == kind);
        let mut out = json!({
            "user_pack_dir": self.catalog_dir.as_ref().map(|d| d.display().to_string()),
            "user_models_file": self.catalog_dir.as_ref().map(|d| user_pack::path(d).display().to_string()),
        });
        if want(HardwareKindName::Hinge) {
            out["hinges"] = self.list_hinge_catalog(Default::default())?["packs"].clone();
        }
        let packs = self.catalogs.usable();
        if want(HardwareKindName::Slide) {
            out["slides"] = packs
                .iter()
                .flat_map(|p| p.slides.iter().map(|f| slide_family_json(p, f, &language)))
                .collect();
        }
        if want(HardwareKindName::Foot) {
            out["feet"] = packs
                .iter()
                .flat_map(|p| p.feet.iter().map(|f| foot_family_json(p, f, &language)))
                .collect();
        }
        out["problems"] = self
            .catalogs
            .packs
            .iter()
            .filter(|p| p.errors() > 0)
            .map(|p| json!({ "file": p.file_name(), "issues": p.issues.iter().map(ToString::to_string).collect::<Vec<_>>() }))
            .collect();
        Ok(out)
    }

    fn find_slide_variant(
        &self,
        pack: Option<&str>,
        family: Option<&str>,
        variant: Option<&str>,
        length: Option<Length>,
    ) -> ServiceResult<CatalogReference> {
        let language = self.language.tag();
        let family = family
            .or_else(|| (variant.is_none()).then_some(hardware_catalog::DEFAULT_SLIDE_FAMILY.1));
        for p in self.catalogs.usable() {
            if pack.is_some_and(|w| w != p.id) {
                continue;
            }
            for f in &p.slides {
                if family.is_some_and(|w| w != f.id) {
                    continue;
                }
                for v in &f.variants {
                    if variant.is_some_and(|w| !w.eq_ignore_ascii_case(&v.code))
                        || length.is_some_and(|l| l != v.length)
                    {
                        continue;
                    }
                    return Ok(snapshot_slide(p, f, v, language));
                }
            }
        }
        Err(ServiceError::not_found(
            "slide",
            format!("{pack:?}/{family:?}/{variant:?}/{:?}", length.map(mm_f64)),
        )
        .hint("See list_hardware_catalog {\"kind\":\"slide\"}."))
    }

    /// A foot model by pinned entry, product code or family id.
    fn find_foot(&self, reference: &str) -> ServiceResult<(Uuid, Option<CatalogReference>)> {
        let project = self.project()?;
        if let Ok(id) = self.resolve(Kind::Catalog, reference)
            && project
                .catalog
                .iter()
                .any(|c| c.id == id && c.foot().is_some())
        {
            return Ok((id, None));
        }
        if let Some(entry) = project
            .catalog
            .iter()
            .find(|c| c.foot().is_some() && c.product_id.eq_ignore_ascii_case(reference))
        {
            return Ok((entry.id, None));
        }
        let language = self.language.tag();
        for p in self.catalogs.usable() {
            for f in &p.feet {
                for v in &f.variants {
                    if v.code.eq_ignore_ascii_case(reference) || f.id == reference {
                        let entry = snapshot_foot(p, f, v, language);
                        return Ok(pinned_or_new(project, &entry));
                    }
                }
            }
        }
        Err(ServiceError::not_found("foot model", reference)
            .hint("Use a catalog code from list_hardware_catalog {\"kind\":\"foot\"}, a pinned entry, or create_foot_model."))
    }

    pub fn add_catalog_slide(
        &mut self,
        input: AddCatalogSlideInput,
    ) -> ServiceResult<Change<Value>> {
        let length = input
            .length
            .as_ref()
            .map(|l| l.positive("length", false))
            .transpose()?;
        let entry = self.find_slide_variant(
            input.pack.as_deref(),
            input.slide.as_deref(),
            input.variant.as_deref(),
            length,
        )?;
        self.change(input.expected_revision, |editor| {
            let id = hardware_catalog::add(editor, entry)?;
            let project = editor.project();
            let entry = project
                .catalog
                .iter()
                .find(|c| c.id == id)
                .ok_or_else(|| ServiceError::internal("entry vanished"))?;
            Ok((
                slide_entry_json(project, entry),
                format!("Pinned '{}' to the project.", entry.name),
            ))
        })
    }

    pub fn add_catalog_foot(&mut self, input: AddCatalogFootInput) -> ServiceResult<Change<Value>> {
        let language = self.language.tag().to_owned();
        let mut found = None;
        for p in self.catalogs.usable() {
            if input.pack.as_ref().is_some_and(|w| *w != p.id) {
                continue;
            }
            for f in &p.feet {
                if input.foot.as_ref().is_some_and(|w| *w != f.id) {
                    continue;
                }
                for v in &f.variants {
                    if input
                        .variant
                        .as_ref()
                        .is_some_and(|w| !w.eq_ignore_ascii_case(&v.code))
                    {
                        continue;
                    }
                    found.get_or_insert_with(|| snapshot_foot(p, f, v, &language));
                }
            }
        }
        let entry = found.ok_or_else(|| {
            ServiceError::not_found(
                "foot",
                format!("{:?}/{:?}/{:?}", input.pack, input.foot, input.variant),
            )
            .hint("See list_hardware_catalog {\"kind\":\"foot\"}.")
        })?;
        self.change(input.expected_revision, |editor| {
            let id = hardware_catalog::add(editor, entry)?;
            let project = editor.project();
            let entry = project
                .catalog
                .iter()
                .find(|c| c.id == id)
                .ok_or_else(|| ServiceError::internal("entry vanished"))?;
            Ok((
                foot_entry_json(project, entry),
                format!("Pinned '{}' to the project.", entry.name),
            ))
        })
    }

    fn model_id(&self, name: &str, id: Option<&str>) -> String {
        id.map_or_else(|| user_pack::slug(name), str::to_owned)
    }

    /// After saving to the user pack, the registry sees the new file.
    fn save_models(
        &mut self,
        slides: Vec<RawSlide>,
        feet: Vec<RawFoot>,
    ) -> ServiceResult<Option<String>> {
        let dir = self.catalog_dir.clone().ok_or_else(|| {
            ServiceError::new(
                ErrorCode::CatalogError,
                "no user catalog folder is configured",
            )
            .hint(
                "Start the server with --catalog-dir or --user-data-dir, or omit save_to_catalog.",
            )
        })?;
        let path = user_pack::save_models(&dir, slides, feet).map_err(pack_issues)?;
        self.catalogs = crate::catalog_pack::CatalogRegistry::load(Some(&dir));
        Ok(Some(path.display().to_string()))
    }

    pub fn create_slide_model(
        &mut self,
        input: CreateSlideModelInput,
    ) -> ServiceResult<Change<Value>> {
        if input.lengths.is_empty() {
            return Err(ServiceError::invalid("give at least one length"));
        }
        let id = self.model_id(&input.name, input.id.as_deref());
        let raw = raw_slide(&input, &id)?;
        let (pack, family, warnings) = user_pack::check_slide(&raw).map_err(pack_issues)?;
        let language = self.language.tag().to_owned();
        let entries: Vec<CatalogReference> = family
            .variants
            .iter()
            .map(|v| snapshot_slide(&pack, &family, v, &language))
            .collect();
        self.project()?;
        let saved = if input.save_to_catalog {
            self.save_models(vec![raw], Vec::new())?
        } else {
            None
        };
        let mut change = self.change(input.expected_revision, |editor| {
            let mut rows = Vec::new();
            for entry in entries {
                let id = hardware_catalog::add(editor, entry)?;
                let project = editor.project();
                if let Some(entry) = project.catalog.iter().find(|c| c.id == id) {
                    rows.push(slide_entry_json(project, entry));
                }
            }
            Ok((
                json!({ "model": id, "entries": rows, "saved_to": saved }),
                format!(
                    "Created slide model '{}' with {} length(s).",
                    input.name,
                    family.variants.len()
                ),
            ))
        })?;
        change
            .warnings
            .extend(warnings.iter().map(ToString::to_string));
        Ok(change)
    }

    pub fn create_foot_model(
        &mut self,
        input: CreateFootModelInput,
    ) -> ServiceResult<Change<Value>> {
        let id = self.model_id(&input.name, input.id.as_deref());
        let raw = raw_foot(&input, &id)?;
        let (pack, family, warnings) = user_pack::check_foot(&raw).map_err(pack_issues)?;
        let language = self.language.tag().to_owned();
        let entry = family
            .variants
            .first()
            .map(|v| snapshot_foot(&pack, &family, v, &language))
            .ok_or_else(|| ServiceError::internal("no variant"))?;
        self.project()?;
        let saved = if input.save_to_catalog {
            self.save_models(Vec::new(), vec![raw])?
        } else {
            None
        };
        let mut change = self.change(input.expected_revision, |editor| {
            let catalog_id = hardware_catalog::add(editor, entry)?;
            let project = editor.project();
            let entry = project
                .catalog
                .iter()
                .find(|c| c.id == catalog_id)
                .ok_or_else(|| ServiceError::internal("entry vanished"))?;
            Ok((
                json!({ "model": id, "entry": foot_entry_json(project, entry), "saved_to": saved }),
                format!(
                    "Created foot model '{}' ({}). Place it with add_foot {{\"model\":\"{}\"}}.",
                    input.name, entry.product_id, entry.id
                ),
            ))
        })?;
        change
            .warnings
            .extend(warnings.iter().map(ToString::to_string));
        Ok(change)
    }

    // ------------------------------------------------------------- slides

    fn detect_drawer(
        &self,
        drawer: &str,
        box_sides: &Option<[String; 2]>,
        carcass_sides: &Option<[String; 2]>,
    ) -> ServiceResult<slide_installation::Detected> {
        let project = self.project()?;
        let id = self.resolve(Kind::Object, drawer)?;
        match (box_sides, carcass_sides) {
            (Some(b), Some(c)) => {
                let root = slide_installation::drawer_root(project, id).ok_or_else(|| {
                    ServiceError::from(slide_installation::SlideFitError::NoDrawerAssembly)
                })?;
                Ok(slide_installation::measure(
                    project,
                    root,
                    [
                        self.resolve(Kind::Board, &b[0])?,
                        self.resolve(Kind::Board, &b[1])?,
                    ],
                    [
                        self.resolve(Kind::Board, &c[0])?,
                        self.resolve(Kind::Board, &c[1])?,
                    ],
                )?)
            }
            (None, None) => Ok(slide_installation::detect(project, id)?),
            _ => Err(ServiceError::invalid(
                "pass both box_sides and carcass_sides, or neither",
            )),
        }
    }

    fn suggestion_json(&self, suggestion: &Suggestion) -> ServiceResult<Value> {
        let project = self.project()?;
        let d = &suggestion.detected;
        let spec = suggestion.catalog.slide();
        Ok(json!({
            "drawer": object_name(project, d.drawer_root),
            "box_sides": d.drawer_sides.map(|id| object_name(project, id)),
            "carcass_sides": d.cabinet_sides.map(|id| object_name(project, id)),
            "gaps_mm": d.geometry.map(|g| mm_f64(g.gap)),
            "carcass_depth_mm": d.geometry.map(|g| mm_f64(g.cabinet_depth)),
            "box_side_mm": d.geometry.map(|g| json!({ "length": mm_f64(g.drawer_depth), "height": mm_f64(g.box_height), "front_behind_carcass_front": mm_f64(g.drawer_front_behind) })),
            "slide": {
                "product": suggestion.catalog.product_id,
                "name": suggestion.catalog.name,
                "length_mm": spec.map(|s| mm_f64(s.length)),
                "travel_mm": spec.map(|s| mm_f64(s.travel)),
                "clearance_mm": spec.map(|s| mm_f64(s.clearance)),
            },
            "height_mm": mm_f64(suggestion.height),
            "setback_mm": mm_f64(suggestion.setback),
            "ok": suggestion.status.issues.is_empty(),
            "issues": suggestion.status.issues.iter().map(issue_text).collect::<Vec<_>>(),
        }))
    }

    pub fn suggest_slides(&self, input: SuggestSlidesInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let catalog = input
            .catalog
            .as_ref()
            .map(|c| self.resolve(Kind::Catalog, c))
            .transpose()?;
        let family = family_lengths(
            self,
            project,
            catalog,
            input.pack.as_deref(),
            input.slide.as_deref(),
        )?;
        let detected = self.detect_drawer(&input.drawer, &None, &None)?;
        let suggestion = slide_installation::suggest(project, detected, &family)?;
        self.suggestion_json(&suggestion)
    }

    pub fn add_slides(&mut self, input: AddSlidesInput) -> ServiceResult<Change<Value>> {
        let r = input.allow_rounding;
        let length = input
            .length
            .as_ref()
            .map(|l| l.positive("length", r))
            .transpose()?;
        let height = input
            .height
            .as_ref()
            .map(|l| l.non_negative("height", r))
            .transpose()?;
        let setback = input
            .setback
            .as_ref()
            .map(|l| l.non_negative("setback", r))
            .transpose()?;
        let project = self.project()?;
        let catalog = input
            .catalog
            .as_ref()
            .map(|c| self.resolve(Kind::Catalog, c))
            .transpose()?;
        let mut family = family_lengths(
            self,
            project,
            catalog,
            input.pack.as_deref(),
            input.slide.as_deref(),
        )?;
        if let Some(length) = length {
            family.retain(|c| c.slide().is_some_and(|s| s.length == length));
            if family.is_empty() {
                return Err(ServiceError::invalid(format!(
                    "the family has no {} mm length",
                    mm_f64(length)
                )));
            }
        }
        let detected = self.detect_drawer(&input.drawer, &input.box_sides, &input.carcass_sides)?;
        let suggestion = match slide_installation::suggest(project, detected.clone(), &family) {
            Ok(s) => s,
            // With an explicit length, install it anyway so the checks explain.
            Err(slide_installation::SlideEditError::NoLengthFits) if family.len() == 1 => {
                Suggestion {
                    catalog: family[0].clone(),
                    height: Length::from_micrometres(
                        detected.geometry[0]
                            .box_height
                            .min(detected.geometry[1].box_height)
                            .micrometres()
                            / 2,
                    ),
                    setback: family[0].slide().map_or(Length::ZERO, |s| s.front_setback),
                    status: SlideStatus {
                        id: Uuid::nil(),
                        issues: Vec::new(),
                        notices: Vec::new(),
                        references: None,
                    },
                    detected,
                }
            }
            Err(e) => return Err(e.into()),
        };
        let (catalog_id, pin) = pinned_or_new(project, &suggestion.catalog);
        let installation = SlideInstallation {
            id: Uuid::new_v4(),
            catalog_id,
            drawer_root_id: suggestion.detected.drawer_root,
            drawer_sides: suggestion.detected.drawer_sides,
            cabinet_sides: suggestion.detected.cabinet_sides,
            sides: suggestion.detected.sides(),
            height: height.unwrap_or(suggestion.height),
            setback: setback.unwrap_or(suggestion.setback),
        };
        let pinned = pin.is_some();
        let run = |editor: &mut ProjectEditor| -> ServiceResult<(Value, String)> {
            match pin.clone() {
                Some(pin) => {
                    slide_installation::create_with_catalog(editor, pin, installation.clone())?
                }
                None => slide_installation::create(editor, installation.clone())?,
            };
            let project = editor.project();
            let status = slide_installation::diagnose(project, &installation);
            let json = slide_status_json(project, &installation, &status);
            let summary = format!(
                "Slides {} on '{}'{}.",
                project
                    .catalog
                    .iter()
                    .find(|c| c.id == installation.catalog_id)
                    .map_or("", |c| c.product_id.as_str()),
                object_name(project, installation.drawer_root_id),
                if status.issues.is_empty() {
                    String::new()
                } else {
                    format!(" — {} issue(s)", status.issues.len())
                }
            );
            Ok((json, summary))
        };
        if input.dry_run {
            let mut scratch = ProjectEditor::new(project.clone())?;
            let (json, summary) = run(&mut scratch)?;
            return self.preview_only(json, format!("Would add: {summary}"));
        }
        let mut change = self.change(input.expected_revision, run)?;
        if pinned {
            change
                .warnings
                .push("The slide length was pinned to the project catalog.".into());
        }
        Ok(change)
    }

    pub fn update_slide(&mut self, input: UpdateSlideInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Slide, &input.slide)?;
        let r = input.allow_rounding;
        let project = self.project()?;
        let current = project
            .slide_installations
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| ServiceError::not_found("drawer slides", &input.slide))?;
        let mut pin = None;
        let catalog_id = match (&input.catalog, &input.length) {
            (Some(c), _) => self.resolve(Kind::Catalog, c)?,
            (None, Some(length)) => {
                let length = length.positive("length", r)?;
                let entry = project.catalog.iter().find(|c| c.id == current.catalog_id);
                let origin = entry.and_then(|e| e.origin.clone());
                let mut family = family_lengths(
                    self,
                    project,
                    None,
                    origin.as_ref().map(|o| o.pack_id.as_str()),
                    origin.as_ref().map(|o| o.item_id.as_str()),
                )?;
                family.retain(|c| c.slide().is_some_and(|s| s.length == length));
                let chosen = family.first().ok_or_else(|| {
                    ServiceError::invalid(format!("the family has no {} mm length", mm_f64(length)))
                })?;
                let (id, new) = pinned_or_new(project, chosen);
                pin = new;
                id
            }
            (None, None) => current.catalog_id,
        };
        let refitted = slide_installation::refitted(project, &current)?;
        let updated = SlideInstallation {
            catalog_id,
            height: input
                .height
                .as_ref()
                .map(|h| h.non_negative("height", r))
                .transpose()?
                .unwrap_or(current.height),
            setback: input
                .setback
                .as_ref()
                .map(|h| h.non_negative("setback", r))
                .transpose()?
                .unwrap_or(current.setback),
            ..refitted
        };
        self.change(input.expected_revision, |editor| {
            if let Some(pin) = pin {
                hardware_catalog::add(editor, pin.clone()).map(|_| ())?;
                // `add` gives a fresh id: point at it.
                let fresh = editor
                    .project()
                    .catalog
                    .iter()
                    .rev()
                    .find(|c| c.product_id == pin.product_id)
                    .map(|c| c.id)
                    .ok_or_else(|| ServiceError::internal("pin vanished"))?;
                slide_installation::update(
                    editor,
                    SlideInstallation {
                        catalog_id: fresh,
                        ..updated.clone()
                    },
                )?;
            } else {
                slide_installation::update(editor, updated.clone())?;
            }
            let project = editor.project();
            let installation = project
                .slide_installations
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| ServiceError::internal("slides vanished"))?;
            let status = slide_installation::diagnose(project, installation);
            Ok((
                slide_status_json(project, installation, &status),
                format!(
                    "Updated the slides of '{}'.",
                    object_name(project, installation.drawer_root_id)
                ),
            ))
        })
    }

    pub fn remove_slide(&mut self, input: SlideRefInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Slide, &input.slide)?;
        self.change(input.expected_revision, |editor| {
            slide_installation::remove(editor, id)?;
            Ok((
                json!({ "removed": id }),
                "Removed the drawer slides.".into(),
            ))
        })
    }

    pub fn list_slides(&self, input: ListSlidesInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let only = input
            .drawer
            .as_ref()
            .map(|d| self.resolve(Kind::Slide, d))
            .transpose()?;
        let rows: Vec<_> = project
            .slide_installations
            .iter()
            .filter(|s| only.is_none_or(|id| id == s.id))
            .map(|s| slide_status_json(project, s, &slide_installation::diagnose(project, s)))
            .collect();
        Ok(json!({
            "slides": rows,
            "pinned": project.catalog.iter().filter(|c| c.slide().is_some()).map(|c| slide_entry_json(project, c)).collect::<Vec<_>>(),
        }))
    }

    // -------------------------------------------------------------- feet

    fn foot_pose(
        project: &Project,
        catalog_id: Uuid,
        pin: Option<&CatalogReference>,
        pose: Pose,
        anchor: FootAnchor,
    ) -> ServiceResult<Pose> {
        if anchor == FootAnchor::Origin {
            return Ok(pose);
        }
        let spec = pin
            .and_then(CatalogReference::foot)
            .or_else(|| {
                project
                    .catalog
                    .iter()
                    .find(|c| c.id == catalog_id)
                    .and_then(CatalogReference::foot)
            })
            .ok_or_else(|| ServiceError::invalid("that model is not a foot"))?;
        let size = spec.local_size().map(mm_number);
        let offset = pose
            .rotation
            .rotate([size[0] / 2.0, size[1] / 2.0, size[2]]);
        Ok(Pose::new(
            std::array::from_fn(|i| pose.translation_mm[i] - offset[i]),
            pose.rotation,
        )?)
    }

    pub fn add_foot(&mut self, input: AddFootInput) -> ServiceResult<Change<Value>> {
        let (catalog_id, pin) = self.find_foot(&input.model)?;
        let parent = self.opt_ref(Kind::Assembly, &input.parent)?;
        let project = self.project()?;
        let base = input.pose.unwrap_or_default().pose()?;
        let poses: Vec<Pose> = if input.positions.is_empty() {
            vec![base]
        } else {
            input
                .positions
                .iter()
                .map(|p| Pose::new(*p, base.rotation))
                .collect::<Result<_, _>>()?
        };
        let poses = poses
            .into_iter()
            .map(|p| Self::foot_pose(project, catalog_id, pin.as_ref(), p, input.anchor))
            .collect::<ServiceResult<Vec<_>>>()?;
        let model_name = pin
            .as_ref()
            .map(|p| p.name.clone())
            .or_else(|| {
                project
                    .catalog
                    .iter()
                    .find(|c| c.id == catalog_id)
                    .map(|c| c.name.clone())
            })
            .unwrap_or_default();
        let base_name = input.name.clone().unwrap_or_else(|| "Foot".into());
        let count = poses.len();
        let run = |editor: &mut ProjectEditor| -> ServiceResult<(Value, String)> {
            let mut pin = pin.clone();
            let mut ids = Vec::new();
            for (i, pose) in poses.iter().enumerate() {
                let name = if count == 1 {
                    base_name.clone()
                } else {
                    format!("{base_name} {}", i + 1)
                };
                ids.push(editor.create_foot(name, catalog_id, pin.take(), parent, *pose)?);
            }
            let project = editor.project();
            let rows: Vec<_> = ids
                .iter()
                .filter_map(|id| project.hardware.iter().find(|h| h.id == *id))
                .map(|h| foot_json(project, h))
                .collect();
            Ok((
                json!({ "feet": rows }),
                format!("Placed {count} × '{model_name}'."),
            ))
        };
        if input.dry_run {
            let mut scratch = ProjectEditor::new(project.clone())?;
            let (json, summary) = run(&mut scratch)?;
            return self.preview_only(json, format!("Would add: {summary}"));
        }
        self.change(input.expected_revision, run)
    }

    pub fn update_foot(&mut self, input: UpdateFootInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Foot, &input.foot)?;
        let project = self.project()?;
        let item = project
            .hardware
            .iter()
            .find(|h| h.id == id)
            .cloned()
            .ok_or_else(|| ServiceError::not_found("foot", &input.foot))?;
        let current_catalog = match item.kind {
            crate::domain::HardwareKind::Catalog { catalog_id } => catalog_id,
            crate::domain::HardwareKind::Placeholder { .. } => {
                return Err(ServiceError::invalid("that hardware item is not a foot"));
            }
        };
        let (catalog_id, pin) = match &input.model {
            Some(model) => self.find_foot(model)?,
            None => (current_catalog, None),
        };
        let parent = match &input.parent {
            Some(p) => self.opt_ref(Kind::Assembly, &Some(p.clone()))?,
            None => item.parent_id,
        };
        let world = match input.pose {
            Some(p) => Self::foot_pose(project, catalog_id, pin.as_ref(), p.pose()?, input.anchor)?,
            None => crate::assembly_edit::world_pose(project, id)?,
        };
        let name = input.name.clone().unwrap_or(item.name);
        self.change(input.expected_revision, |editor| {
            editor.edit_foot(id, name.clone(), catalog_id, pin.clone(), parent, world)?;
            let project = editor.project();
            let item = project
                .hardware
                .iter()
                .find(|h| h.id == id)
                .ok_or_else(|| ServiceError::internal("foot vanished"))?;
            Ok((foot_json(project, item), format!("Updated '{name}'.")))
        })
    }

    pub fn list_feet(&self) -> ServiceResult<Value> {
        let project = self.project()?;
        let feet: Vec<_> = project
            .hardware
            .iter()
            .filter(|h| project.foot_spec(h).is_some())
            .map(|h| foot_json(project, h))
            .collect();
        let mut counts: HashMap<String, usize> = HashMap::new();
        for h in project.hardware.iter() {
            if let crate::domain::HardwareKind::Catalog { catalog_id } = h.kind
                && let Some(entry) = project
                    .catalog
                    .iter()
                    .find(|c| c.id == catalog_id && c.foot().is_some())
            {
                *counts.entry(entry.product_id.clone()).or_default() += 1;
            }
        }
        Ok(json!({
            "feet": feet,
            "per_model": counts,
            "pinned": project.catalog.iter().filter(|c| c.foot().is_some()).map(|c| foot_entry_json(project, c)).collect::<Vec<_>>(),
        }))
    }

    // ------------------------------------------------------------ picture

    pub fn render_drawer_opening(&self, input: DrawerOpeningInput) -> ServiceResult<Rendered> {
        let project = self.project()?;
        let id = self.resolve(Kind::Slide, &input.drawer)?;
        let installation = project
            .slide_installations
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| ServiceError::not_found("drawer slides", &input.drawer))?;
        let travel =
            slide_installation::max_extension(project, installation).map_or(0.0, mm_number);
        let distance = match (&input.distance, input.fraction) {
            (Some(d), _) => mm_number(d.non_negative("distance", true)?),
            (None, Some(f)) => {
                if !(0.0..=1.0).contains(&f) {
                    return Err(ServiceError::invalid("fraction is between 0 and 1"));
                }
                travel * f
            }
            (None, None) => travel,
        };
        let poses: HashMap<Uuid, Pose> =
            slide_installation::derived_poses(project, installation, distance)?
                .into_iter()
                .collect();
        let (mut json, png) = self.picture(&input.view, Some(&poses))?;
        json["extension_mm"] = json!((distance * 1000.0).round() / 1000.0);
        json["travel_mm"] = json!(travel);
        Ok(Rendered {
            json,
            images: vec![png],
        })
    }
}

fn foot_json(project: &Project, item: &crate::domain::Hardware) -> Value {
    let entry = match item.kind {
        crate::domain::HardwareKind::Catalog { catalog_id } => {
            project.catalog.iter().find(|c| c.id == catalog_id)
        }
        crate::domain::HardwareKind::Placeholder { .. } => None,
    };
    let world = crate::assembly_edit::world_pose(project, item.id).ok();
    let size = project.hardware_dimensions(item).map(|s| s.map(mm_f64));
    let top_center = world.zip(size).map(|(pose, size)| {
        let p = pose
            .rotation
            .rotate([size[0] / 2.0, size[1] / 2.0, size[2]]);
        std::array::from_fn::<f64, 3, _>(|i| {
            ((pose.translation_mm[i] + p[i]) * 1000.0).round() / 1000.0
        })
    });
    json!({
        "foot_id": item.id,
        "name": item.name,
        "model": entry.map(|e| e.name.clone()),
        "product": entry.map(|e| e.product_id.clone()),
        "parent": item.parent_id.map(|p| object_name(project, p)),
        "box_mm": size,
        "world": world.map(crate::service::dto::Euler::from),
        "mounting_face_centre": top_center,
    })
}

impl From<slide_installation::SlideFitError> for ServiceError {
    fn from(error: slide_installation::SlideFitError) -> Self {
        use slide_installation::SlideFitError as E;
        let (message, hint) = match error {
            E::MissingPart => ("a board of the drawer no longer exists", None),
            E::NoDrawerAssembly => (
                "the drawer is not a group of boards",
                Some("Group the drawer box boards (group_objects) and pass the group."),
            ),
            E::NoSides => (
                "no pair of box sides with a carcass side next to each was found",
                Some("Check describe_scene gaps, or pass box_sides and carcass_sides."),
            ),
            E::NotParallel => (
                "the box sides and carcass sides are not upright and parallel",
                None,
            ),
        };
        let error = Self::new(ErrorCode::InstallationError, message);
        match hint {
            Some(hint) => error.hint(hint),
            None => error,
        }
    }
}

impl From<slide_installation::SlideEditError> for ServiceError {
    fn from(error: slide_installation::SlideEditError) -> Self {
        use slide_installation::SlideEditError as E;
        match error {
            E::MissingInstallation => Self::not_found("drawer slides", "?"),
            E::MissingCatalog => Self::new(ErrorCode::CatalogError, "the slide entry is missing"),
            E::NotASlide => Self::invalid("that catalog entry is not a drawer slide"),
            E::InvalidDistance => Self::new(ErrorCode::InvalidLength, "distances cannot be negative"),
            E::DrawerHasSlides => Self::new(
                ErrorCode::Conflict,
                "this drawer already runs on slides",
            )
            .hint("Use update_slide, or remove_slide first."),
            E::Fit(e) => e.into(),
            E::NoLengthFits => Self::new(
                ErrorCode::InstallationError,
                "no length of this slide fits the drawer and carcass depth",
            )
            .hint("Use a deeper carcass, a longer box, another family, or pass length to see the issues."),
            E::InvalidExtension => Self::invalid("the opening is beyond the slide's travel"),
        }
    }
}
