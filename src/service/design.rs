//! Design tools: materials, boards, positions, assemblies, reference hardware
//! and cabinet templates.
use std::collections::HashSet;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::allocation_diagnostics::{Status, diagnose};
use crate::board_commands::{NewBoard, NewBoardOptions, NewMaterial};
use crate::board_dimensions::{BoardDimension, BoardSelection};
use crate::commands::ProjectEditor;
use crate::domain::{Board, BoardGrain, HardwareKind, Project};
use crate::first_fit::FirstFit;
use crate::material_changes::{AllocationConflict, ConflictReason, DependantChoice};
use crate::material_presets::{BR_STANDARD, preset_for, seed_defaults};
use crate::measurements::{self, MeasurementError, Scope};
use crate::placement::{
    self, CoordinateFrame, FacePlacement, NumericPose, PlacementSession, rotation_from_degrees_xyz,
};
use crate::service::dto::{
    AlignName, AnchorName, Change, ColorInput, CurrencyCode, Euler, FaceName, Frame, Grain,
    LengthInput, LengthOut, PoseInput, PoseOut, PosePresetName, UnitName, color_hex, grain_name,
    mm_f64, snake,
};
use crate::service::error::{ErrorCode, ServiceError, ServiceResult};
use crate::service::project::{LanguageName, summary};
use crate::service::workspace::{Document, Kind, Workspace, object_name, parent_name};
use crate::template_setup::{
    MaterialRole, ProposedLength, TemplateField, TemplateKind, TemplateSetup,
};
use crate::units::{Conversion, Length, Pose, Quaternion};

impl From<MeasurementError> for ServiceError {
    fn from(error: MeasurementError) -> Self {
        match error {
            MeasurementError::EmptySelection => ServiceError::invalid("no objects were given"),
            MeasurementError::MissingObject(id) => ServiceError::not_found("object", id),
            MeasurementError::UndimensionedHardware(id) => {
                ServiceError::invalid(format!("hardware {id} has no dimensions"))
            }
            MeasurementError::Pose(e) => e.into(),
        }
    }
}

// ---------------------------------------------------------------- shared JSON

pub(crate) fn conflicts_json(project: &Project, conflicts: &[AllocationConflict]) -> Vec<Value> {
    conflicts
        .iter()
        .map(|c| {
            json!({
                "board_id": c.board_id,
                "board": object_name(project, c.board_id),
                "stock": project.stock_alias(c.stock_id),
                "reasons": c.reasons.iter().map(|r| match r {
                    ConflictReason::MaterialIdentity => "material_identity",
                    ConflictReason::EffectiveThickness => "thickness",
                    ConflictReason::Grain => "grain",
                    ConflictReason::OutsideStock => "outside_stock",
                    ConflictReason::Overlap => "overlap",
                }).collect::<Vec<_>>(),
            })
        })
        .collect()
}

pub(crate) fn fit_text(project: &Project, fit: Option<FirstFit>) -> Value {
    match fit {
        None => json!("not_requested"),
        Some(FirstFit::Allocated(stock)) => {
            json!({ "placed_on": project.stock_alias(stock).map_or_else(|| stock.to_string(), str::to_owned) })
        }
        Some(FirstFit::NoFit) => json!("no_fit"),
        Some(FirstFit::SearchExhausted) => json!("search_exhausted"),
    }
}

fn world_of(project: &Project, id: Uuid) -> Option<Pose> {
    crate::assembly_edit::world_pose(project, id).ok()
}

fn pose_out(project: &Project, id: Uuid, local: Pose) -> PoseOut {
    PoseOut {
        local: local.into(),
        world: world_of(project, id).unwrap_or(local).into(),
    }
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::AllocatedValid => "valid",
        Status::Unallocated => "unallocated",
        Status::Conflicted => "conflicted",
        Status::UnknownSearchBudget => "unknown",
    }
}

pub(crate) fn board_row(
    project: &Project,
    board: &Board,
    statuses: &[crate::allocation_diagnostics::BoardDiagnostic],
) -> Value {
    let unit = project.display_unit;
    let material = project.material(board.material_id);
    let allocation = project.allocations.iter().find(|a| a.board_id == board.id);
    let status = statuses
        .iter()
        .find(|d| d.board_id == board.id)
        .map_or("unknown", |d| status_name(d.status));
    let world = world_of(project, board.id).map(Euler::from);
    json!({
        "id": board.id,
        "name": board.name,
        "parent": parent_name(project, board.id),
        "material": material.map(|m| m.name.clone()),
        "length": LengthOut::new(board.length, unit),
        "width": LengthOut::new(board.width, unit),
        "thickness": LengthOut::new(board.thickness, unit),
        "grain": material.map(|m| grain_name(board.effective_grain(m))),
        "world_position": world,
        "cut_plan": {
            "status": status,
            "sheet": allocation.and_then(|a| project.stock_alias(a.stock_id)),
            "origin_mm": allocation.map(|a| a.origin.map(mm_f64)),
            "quarter_turn": allocation.map(|a| a.quarter_turn),
            "locked": allocation.map(|a| a.locked),
        },
    })
}

// ------------------------------------------------------------------- inputs

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CreateMaterialInput {
    /// e.g. "Plywood 18".
    pub name: String,
    /// Default thickness of boards made of it.
    pub thickness: LengthInput,
    /// Which board direction the grain must follow. Default unrestricted.
    #[serde(default)]
    pub grain: Option<Grain>,
    /// Display color: "#rrggbb" or [r, g, b].
    #[serde(default)]
    pub color: Option<ColorInput>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SeedMaterialsInput {
    #[serde(default)]
    pub language: Option<LanguageName>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DependantsName {
    /// Boards keep their current thickness and grain.
    #[default]
    Preserve,
    /// Every board that follows the material default changes.
    ApplyAll,
    /// Only `selected_boards` change.
    ApplySelected,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct UpdateMaterialInput {
    pub material: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub thickness: Option<LengthInput>,
    #[serde(default)]
    pub grain: Option<Grain>,
    #[serde(default)]
    pub color: Option<ColorInput>,
    /// What happens to boards already made of this material.
    #[serde(default)]
    pub dependants: DependantsName,
    #[serde(default)]
    pub selected_boards: Vec<String>,
    /// Which face stays put when a board's thickness changes.
    #[serde(default)]
    pub anchor: AnchorName,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct RefInput {
    /// Id, unique name, id prefix (8+ hex) or stock alias.
    #[serde(rename = "ref", alias = "id")]
    pub reference: String,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct ListBoardsInput {
    /// Only boards inside this assembly (at any depth).
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub material: Option<String>,
    /// valid | unallocated | conflicted | unknown
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct GetInput {
    #[serde(rename = "ref", alias = "id")]
    pub reference: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CreateBoardInput {
    pub name: String,
    /// Material ref; the board takes its default thickness.
    pub material: String,
    /// Along the board's local X.
    pub length: LengthInput,
    /// Along the board's local Y.
    pub width: LengthInput,
    /// Position (mm) and rotation (degrees). Default origin, lying flat.
    #[serde(default)]
    pub pose: Option<PoseInput>,
    /// Whether `pose` is relative to `parent` (default) or the world.
    #[serde(default)]
    pub frame: Frame,
    /// Orientation shortcut applied at the given position (overrides rx/ry/rz):
    /// stand_up (a side panel), lay_flat, turn_90_z.
    #[serde(default)]
    pub preset: Option<PosePresetName>,
    /// Assembly to put the board in.
    #[serde(default)]
    pub parent: Option<String>,
    /// Grain override; default follows the material.
    #[serde(default)]
    pub grain: Option<Grain>,
    /// Place it on declared stock right away (first fit). Default true.
    #[serde(default = "yes")]
    pub auto_fit: bool,
    /// Ignored inside create_boards.
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CreateBoardsInput {
    pub boards: Vec<CreateBoardInput>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct DuplicateBoardInput {
    pub board: String,
    /// Offset of each copy from the previous one, in the parent frame.
    /// Default [25, 0, 0].
    #[serde(default)]
    pub offset_mm: Option<[f64; 3]>,
    /// Number of copies (default 1).
    #[serde(default)]
    pub count: Option<usize>,
    /// Name for the copies (numbered when count > 1). Default: the original's.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ResizeBoardInput {
    pub board: String,
    #[serde(default)]
    pub length: Option<LengthInput>,
    #[serde(default)]
    pub width: Option<LengthInput>,
    /// Usually follows the material; set to override.
    #[serde(default)]
    pub thickness: Option<LengthInput>,
    /// Which face stays put: start (low face, default), centre or end.
    #[serde(default)]
    pub anchor: AnchorName,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DimensionName {
    Length,
    Width,
    Thickness,
}

impl From<DimensionName> for BoardDimension {
    fn from(d: DimensionName) -> Self {
        match d {
            DimensionName::Length => BoardDimension::Length,
            DimensionName::Width => BoardDimension::Width,
            DimensionName::Thickness => BoardDimension::Thickness,
        }
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ResizeBoardsInput {
    /// Boards and/or assemblies (all their boards).
    pub objects: Vec<String>,
    pub dimension: DimensionName,
    pub value: LengthInput,
    #[serde(default)]
    pub anchor: AnchorName,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SetBoardMaterialInput {
    pub board: String,
    pub material: String,
    #[serde(default)]
    pub anchor: AnchorName,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrainOverride {
    MaterialDefault,
    Length,
    Width,
    Unrestricted,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SetBoardGrainInput {
    pub board: String,
    pub grain: GrainOverride,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct RenameInput {
    /// Board, assembly or hardware item.
    pub object: String,
    pub name: String,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SetBoardPoseInput {
    pub board: String,
    /// Omitted position/rotation values keep their current value.
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub z: Option<f64>,
    /// Degrees, intrinsic X then Y then Z.
    #[serde(default)]
    pub rx: Option<f64>,
    #[serde(default)]
    pub ry: Option<f64>,
    #[serde(default)]
    pub rz: Option<f64>,
    #[serde(default)]
    pub frame: Frame,
    /// Applied after the values above, keeping the board origin fixed.
    #[serde(default)]
    pub preset: Option<PosePresetName>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct PlaceOnFaceInput {
    /// The board that moves.
    pub board: String,
    /// The board it is placed against (it does not move).
    pub target: String,
    /// Face of `board` that touches: -x/+x (length ends), -y/+y (width
    /// edges), -z/+z (broad faces).
    pub source_face: FaceName,
    pub target_face: FaceName,
    /// Alignment along the two in-plane axes of each face (the two remaining
    /// local axes in increasing order). Default [start, start].
    #[serde(default)]
    pub source_align: Option<[AlignName; 2]>,
    #[serde(default)]
    pub target_align: Option<[AlignName; 2]>,
    /// Shift along the target face's two in-plane axes, mm.
    #[serde(default)]
    pub offset_mm: Option<[f64; 2]>,
    /// Distance out from the target face, mm (0 = touching).
    #[serde(default)]
    pub gap: Option<LengthInput>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct TransformInput {
    /// Boards, assemblies or hardware; each moves rigidly with its contents.
    pub objects: Vec<String>,
    /// World translation, mm.
    #[serde(default)]
    pub translate_mm: Option<[f64; 3]>,
    /// World rotation in degrees (X, Y, Z) about `pivot_mm`.
    #[serde(default)]
    pub rotate_degrees: Option<[f64; 3]>,
    #[serde(default)]
    pub pivot_mm: Option<[f64; 3]>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScopeName {
    /// Boards only.
    #[default]
    Body,
    /// Boards and hardware.
    Overall,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct MeasureInput {
    pub objects: Vec<String>,
    #[serde(default)]
    pub scope: ScopeName,
    /// Measure in the axes of this object instead of the world.
    #[serde(default)]
    pub relative_to: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct FindInput {
    /// Case-insensitive substring of the name (or an id prefix).
    pub query: String,
    /// board | assembly | hardware | material | stock | door | catalog
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct GroupInput {
    pub objects: Vec<String>,
    pub name: String,
    /// Put the new assembly inside this one.
    #[serde(default)]
    pub parent: Option<String>,
    /// The assembly origin in world mm (default [0,0,0]).
    #[serde(default)]
    pub pivot_mm: Option<[f64; 3]>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ReparentInput {
    pub objects: Vec<String>,
    /// Target assembly, or null for the top level.
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct DuplicateAssemblyInput {
    pub assembly: String,
    /// World offset of the copy, mm.
    #[serde(default)]
    pub offset_mm: Option<[f64; 3]>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct PlaceholderInput {
    /// For update: the hardware item to change.
    #[serde(default)]
    pub hardware: Option<String>,
    pub name: String,
    /// Size along its local X, Y, Z.
    pub dimensions: [LengthInput; 3],
    #[serde(default)]
    pub parent: Option<String>,
    /// World pose.
    #[serde(default)]
    pub pose: Option<PoseInput>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateName {
    /// Kitchen base cabinet: sides, bottom, two top rails, overlay back.
    Base,
    /// Wall cabinet with one shelf.
    Wall,
    /// Drawer stack: carcass, drawer boxes and external fronts.
    Drawers,
}

impl From<TemplateName> for TemplateKind {
    fn from(t: TemplateName) -> Self {
        match t {
            TemplateName::Base => TemplateKind::Base,
            TemplateName::Wall => TemplateKind::Wall,
            TemplateName::Drawers => TemplateKind::Drawers,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct TemplateDimensions {
    /// Default 600.
    #[serde(default)]
    pub width: Option<LengthInput>,
    /// Includes the overlay back (and the drawer fronts). Default 560.
    #[serde(default)]
    pub depth: Option<LengthInput>,
    /// Default 720.
    #[serde(default)]
    pub height: Option<LengthInput>,
    /// Base only: top rail width. Default 80.
    #[serde(default)]
    pub rail_width: Option<LengthInput>,
    /// Wall only: shelf height from the bottom. Default 320.
    #[serde(default)]
    pub shelf_height: Option<LengthInput>,
    /// Drawers only. Defaults: box_depth 500, side_clearance 13,
    /// rear_clearance 20, vertical_clearance 8, front_reveal 3, front_gap 3.
    #[serde(default)]
    pub box_depth: Option<LengthInput>,
    #[serde(default)]
    pub side_clearance: Option<LengthInput>,
    #[serde(default)]
    pub rear_clearance: Option<LengthInput>,
    #[serde(default)]
    pub vertical_clearance: Option<LengthInput>,
    #[serde(default)]
    pub front_reveal: Option<LengthInput>,
    #[serde(default)]
    pub front_gap: Option<LengthInput>,
}

/// A material for a template role: an existing/standard material name or id,
/// or a new material.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MaterialSpec {
    Ref(String),
    New {
        name: String,
        thickness: LengthInput,
        #[serde(default)]
        grain: Option<Grain>,
        #[serde(default)]
        color: Option<ColorInput>,
    },
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct TemplateMaterials {
    /// Sides, bottom, rails, shelf. Default "White MDF" 15 mm.
    #[serde(default)]
    pub carcass: Option<MaterialSpec>,
    /// Default "HDF" 3 mm.
    #[serde(default)]
    pub back: Option<MaterialSpec>,
    /// Drawers: box sides. Default "White MDF" 15 mm.
    #[serde(default, rename = "box")]
    pub drawer_box: Option<MaterialSpec>,
    /// Drawers: box bottoms. Default "HDF" 3 mm.
    #[serde(default)]
    pub box_bottom: Option<MaterialSpec>,
    /// Drawers: fronts. Default "White MDF" 18 mm.
    #[serde(default)]
    pub external_front: Option<MaterialSpec>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateTarget {
    /// Replace the open project with a new one (default).
    #[default]
    NewProject,
    /// Add the cabinet to the open project.
    CurrentProject,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct GenerateTemplateInput {
    pub kind: TemplateName,
    /// Project name (new_project) or cabinet assembly name (current_project).
    pub name: String,
    #[serde(default)]
    pub dimensions: TemplateDimensions,
    /// Drawers only (default 3).
    #[serde(default)]
    pub drawer_count: Option<usize>,
    #[serde(default)]
    pub materials: TemplateMaterials,
    /// Add standard sheets (to purchase) for the boards and place them.
    /// Default true. Only standard materials have known sheet sizes.
    #[serde(default = "yes")]
    pub add_sheets: bool,
    #[serde(default)]
    pub into: TemplateTarget,
    /// new_project only.
    #[serde(default)]
    pub currency: Option<CurrencyCode>,
    #[serde(default)]
    pub display_unit: Option<UnitName>,
    #[serde(default)]
    pub language: Option<LanguageName>,
    /// current_project only: where the cabinet goes (world mm).
    #[serde(default)]
    pub offset_mm: Option<[f64; 3]>,
    #[serde(default)]
    pub discard_changes: bool,
    #[serde(default)]
    pub allow_rounding: bool,
}

// ---------------------------------------------------------------- helpers

fn inverse(pose: Pose) -> ServiceResult<Pose> {
    let q = pose.rotation;
    let conjugate = Quaternion {
        w: q.w,
        x: -q.x,
        y: -q.y,
        z: -q.z,
    };
    Ok(Pose::new(
        conjugate.rotate(pose.translation_mm.map(|v| -v)),
        conjugate,
    )?)
}

fn parent_world(project: &Project, parent: Option<Uuid>) -> ServiceResult<Pose> {
    match parent {
        None => Ok(Pose::IDENTITY),
        Some(id) => Ok(crate::assembly_edit::world_pose(project, id)?),
    }
}

/// The pose of a new board or item, relative to its parent.
fn entered_pose(
    project: &Project,
    pose: Option<PoseInput>,
    preset: Option<PosePresetName>,
    frame: Frame,
    parent: Option<Uuid>,
) -> ServiceResult<Pose> {
    let mut entered = pose.unwrap_or_default().pose()?;
    if let Some(preset) = preset {
        entered = placement::preset_pose(entered, preset.into())?;
    }
    Ok(match frame {
        Frame::Parent => entered,
        Frame::World => inverse(parent_world(project, parent)?)?.compose(entered)?,
    })
}

fn descendants_of(project: &Project, root: Uuid) -> HashSet<Uuid> {
    let mut out = HashSet::from([root]);
    loop {
        let before = out.len();
        for a in &project.assemblies {
            if a.parent_id.is_some_and(|p| out.contains(&p)) {
                out.insert(a.id);
            }
        }
        if out.len() == before {
            break;
        }
    }
    out
}

impl Workspace {
    fn opt_ref(&self, kind: Kind, reference: &Option<String>) -> ServiceResult<Option<Uuid>> {
        reference
            .as_ref()
            .map(|r| self.resolve(kind, r))
            .transpose()
    }

    // -------------------------------------------------------------- materials

    pub fn list_materials(&self) -> ServiceResult<Value> {
        let project = self.project()?;
        let unit = project.display_unit;
        let rows: Vec<_> = project
            .materials
            .iter()
            .map(|m| {
                let preset = preset_for(m);
                json!({
                    "id": m.id,
                    "name": m.name,
                    "thickness": LengthOut::new(m.default_thickness, unit),
                    "grain": grain_name(m.default_grain),
                    "color": project.material_colors.get(&m.id).copied().map(color_hex),
                    "boards": project.boards.iter().filter(|b| b.material_id == m.id).count(),
                    "stock_pieces": project.stock.iter().filter(|s| s.material_id == m.id).count(),
                    "standard_sheet_mm": preset.map(|p| p.sheet_mm),
                })
            })
            .collect();
        Ok(json!({ "revision": project.revision, "materials": rows }))
    }

    pub fn create_material(&mut self, input: CreateMaterialInput) -> ServiceResult<Change<Value>> {
        let thickness = input
            .thickness
            .positive("thickness", input.allow_rounding)?;
        let color = input.color.as_ref().map(ColorInput::resolve).transpose()?;
        let grain = input
            .grain
            .map_or(BoardGrain::Unrestricted, BoardGrain::from);
        self.change(input.expected_revision, |editor| {
            let id = editor.create_material_with_color(
                NewMaterial {
                    name: input.name.trim().to_owned(),
                    thickness,
                    grain,
                },
                color,
            )?;
            Ok((
                json!({ "material_id": id }),
                format!("Created material '{}'.", input.name.trim()),
            ))
        })
    }

    pub fn seed_standard_materials(
        &mut self,
        input: SeedMaterialsInput,
    ) -> ServiceResult<Change<Value>> {
        let language = input.language.map_or(self.language, Into::into);
        self.change(None, |editor| {
            let mut added = 0;
            editor.transact(|p| -> Result<(), ()> {
                added = seed_defaults(p, language);
                Ok(())
            })?;
            let summary = if added == 0 {
                "The project already has materials; nothing was added.".to_owned()
            } else {
                format!("Added {added} standard materials.")
            };
            Ok((json!({ "added": added }), summary))
        })
    }

    pub fn update_material(&mut self, input: UpdateMaterialInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Material, &input.material)?;
        let project = self.project()?;
        let material = project
            .material(id)
            .cloned()
            .ok_or_else(|| ServiceError::not_found("material", id))?;
        let name = input.name.clone().unwrap_or(material.name.clone());
        let thickness = match &input.thickness {
            Some(t) => t.positive("thickness", input.allow_rounding)?,
            None => material.default_thickness,
        };
        let grain = input.grain.map_or(material.default_grain, BoardGrain::from);
        let color = input.color.as_ref().map(ColorInput::resolve).transpose()?;
        let choice = match input.dependants {
            DependantsName::Preserve => DependantChoice::Preserve,
            DependantsName::ApplyAll => DependantChoice::ApplyAll,
            DependantsName::ApplySelected => DependantChoice::ApplySelected(
                self.resolve_all(Kind::Board, &input.selected_boards)?,
            ),
        };
        let unit = project.display_unit;
        let preview = self.editor()?.preview_material_change(
            id,
            name.clone(),
            thickness,
            grain,
            input.anchor.into(),
        )?;
        let affected: Vec<Value> = preview
            .affected
            .iter()
            .map(|a| {
                json!({
                    "board_id": a.id,
                    "name": a.name,
                    "thickness_before": LengthOut::new(a.thickness_before, unit),
                    "thickness_if_applied": LengthOut::new(a.thickness_if_applied, unit),
                    "grain_before": grain_name(a.grain_before),
                    "grain_if_applied": grain_name(a.grain_if_applied),
                })
            })
            .collect();
        if input.dry_run {
            return self.preview_only(
                json!({ "affected_boards": affected }),
                format!("{} board(s) use this material.", affected.len()),
            );
        }
        self.change(input.expected_revision, |editor| {
            let conflicts = editor.apply_material_change(preview, choice)?;
            if let Some(color) = color {
                editor.set_material_color(id, Some(color))?;
            }
            let project = editor.project();
            Ok((
                json!({ "affected_boards": affected, "conflicts": conflicts_json(project, &conflicts) }),
                format!("Updated material '{name}'."),
            ))
        })
    }

    pub fn delete_material(&mut self, input: RefInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Material, &input.reference)?;
        let name = object_name(self.project()?, id);
        self.change(input.expected_revision, |editor| {
            editor.delete_material(id)?;
            Ok((
                json!({ "deleted": id }),
                format!("Deleted material '{name}'."),
            ))
        })
    }

    // ----------------------------------------------------------------- boards

    pub fn list_boards(&self, input: ListBoardsInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let parent = self.opt_ref(Kind::Assembly, &input.parent)?;
        let material = self.opt_ref(Kind::Material, &input.material)?;
        let inside = parent.map(|p| descendants_of(project, p));
        let statuses = diagnose(project);
        let rows: Vec<_> = project
            .boards
            .iter()
            .filter(|b| {
                inside
                    .as_ref()
                    .is_none_or(|set| b.parent_id.is_some_and(|p| set.contains(&p)))
            })
            .filter(|b| material.is_none_or(|m| b.material_id == m))
            .map(|b| board_row(project, b, &statuses))
            .filter(|row| {
                input
                    .status
                    .as_deref()
                    .is_none_or(|s| row["cut_plan"]["status"] == s)
            })
            .collect();
        Ok(json!({ "revision": project.revision, "count": rows.len(), "boards": rows }))
    }

    pub fn get_board(&self, input: GetInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let id = self.resolve(Kind::Board, &input.reference)?;
        let board = project
            .board(id)
            .ok_or_else(|| ServiceError::not_found("board", id))?;
        let statuses = diagnose(project);
        let mut row = board_row(project, board, &statuses);
        let reasons: Vec<&str> = statuses
            .iter()
            .find(|d| d.board_id == id)
            .map(|d| d.reasons.iter().map(|r| r.key()).collect())
            .unwrap_or_default();
        let measured =
            measurements::measure(project, &[id], Scope::Body, measurements::Frame::World).ok();
        row["pose"] =
            serde_json::to_value(pose_out(project, id, board.pose)).unwrap_or(Value::Null);
        row["grain_override"] = json!(board.grain_override.map(grain_name));
        row["world_bounds"] =
            json!(measured.map(|m| json!({ "min": m.minimum_mm, "max": m.maximum_mm })));
        row["cut_plan"]["reasons"] = json!(reasons);
        row["hinges"] = json!(
            project
                .hinge_installations
                .iter()
                .filter(|h| h.door_board_id == id || h.mounting_board_id == id)
                .map(|h| h.id)
                .collect::<Vec<_>>()
        );
        Ok(row)
    }

    fn create_one(
        editor: &mut ProjectEditor,
        board: &CreateBoardInput,
        allow_rounding: bool,
    ) -> ServiceResult<Value> {
        let project = editor.project();
        let material =
            crate::service::workspace::resolve(project, Kind::Material, &board.material)?;
        let parent = board
            .parent
            .as_ref()
            .map(|p| crate::service::workspace::resolve(project, Kind::Assembly, p))
            .transpose()?;
        let pose = entered_pose(project, board.pose, board.preset, board.frame, parent)?;
        let length = board.length.positive("length", allow_rounding)?;
        let width = board.width.positive("width", allow_rounding)?;
        let (id, fit) = editor.create_board_detailed(
            NewBoard {
                name: board.name.trim().to_owned(),
                material_id: material,
                length,
                width,
                pose,
            },
            NewBoardOptions {
                grain_override: board.grain.map(BoardGrain::from),
                parent_id: parent,
                fit: board.auto_fit,
            },
        )?;
        let project = editor.project();
        let created = project
            .board(id)
            .ok_or_else(|| ServiceError::internal("board vanished"))?;
        Ok(json!({
            "board_id": id,
            "name": created.name,
            "thickness": LengthOut::new(created.thickness, project.display_unit),
            "fit": fit_text(project, fit),
            "world": world_of(project, id).map(Euler::from),
        }))
    }

    pub fn create_board(&mut self, input: CreateBoardInput) -> ServiceResult<Change<Value>> {
        let name = input.name.clone();
        let mut change = self.change(input.expected_revision, |editor| {
            let result = Self::create_one(editor, &input, input.allow_rounding)?;
            Ok((result, format!("Created board '{}'.", name.trim())))
        })?;
        if change.result["fit"] == "no_fit" {
            change.warnings.push(
                "The board is not on any sheet: no declared stock of its material and thickness has room. Use add_needed_sheets or create_stock, then auto_place.".into(),
            );
        }
        Ok(change)
    }

    pub fn create_boards(&mut self, input: CreateBoardsInput) -> ServiceResult<Change<Value>> {
        if input.boards.is_empty() {
            return Err(ServiceError::invalid("boards is empty"));
        }
        let rounding = input.allow_rounding;
        let mut change = self.change(input.expected_revision, |editor| {
            let mut created = Vec::new();
            for (index, board) in input.boards.iter().enumerate() {
                match Self::create_one(editor, board, rounding) {
                    Ok(row) => created.push(row),
                    Err(mut e) => {
                        e.message = format!(
                            "boards[{index}] '{}': {} ({} created before it remain)",
                            board.name,
                            e.message,
                            created.len()
                        );
                        e.details = Some(json!({ "created": created, "failed_index": index }));
                        return Err(e);
                    }
                }
            }
            let count = created.len();
            Ok((
                json!({ "boards": created }),
                format!("Created {count} boards."),
            ))
        })?;
        let unplaced = change.result["boards"].as_array().map_or(0, |rows| {
            rows.iter().filter(|r| r["fit"] == "no_fit").count()
        });
        if unplaced > 0 {
            change.warnings.push(format!(
                "{unplaced} board(s) are not on any sheet yet; use add_needed_sheets or create_stock + auto_place."
            ));
        }
        Ok(change)
    }

    pub fn duplicate_board(&mut self, input: DuplicateBoardInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Board, &input.board)?;
        let count = input.count.unwrap_or(1).clamp(1, 200);
        let offset = input.offset_mm.unwrap_or([25.0, 0.0, 0.0]);
        self.change(input.expected_revision, |editor| {
            let original = editor
                .project()
                .board(id)
                .cloned()
                .ok_or_else(|| ServiceError::not_found("board", id))?;
            let mut copies = Vec::new();
            for n in 1..=count {
                let t = original.pose.translation_mm;
                let pose = Pose::new(
                    std::array::from_fn(|i| t[i] + offset[i] * n as f64),
                    original.pose.rotation,
                )?;
                let (copy, fit) = editor.duplicate_board_with_fit(id, pose)?;
                if let Some(name) = &input.name {
                    let name = if count > 1 { format!("{name} {n}") } else { name.clone() };
                    editor.rename_object(copy, &name)?;
                }
                let project = editor.project();
                copies.push(json!({ "board_id": copy, "name": object_name(project, copy), "fit": fit_text(project, Some(fit)) }));
            }
            Ok((json!({ "copies": copies }), format!("Made {count} copies of '{}'.", original.name)))
        })
    }

    pub fn resize_board(&mut self, input: ResizeBoardInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Board, &input.board)?;
        let mut targets = Vec::new();
        for (dimension, value) in [
            (BoardDimension::Length, &input.length),
            (BoardDimension::Width, &input.width),
            (BoardDimension::Thickness, &input.thickness),
        ] {
            if let Some(value) = value {
                let field = snake(&format!("{dimension:?}"));
                targets.push((dimension, value.positive(&field, input.allow_rounding)?));
            }
        }
        if targets.is_empty() {
            return Err(ServiceError::invalid(
                "give at least one of length, width, thickness",
            ));
        }
        let anchor = input.anchor.into();
        if input.dry_run {
            let editor = self.editor()?;
            let mut previews = Vec::new();
            for (dimension, value) in &targets {
                let preview = editor.preview_board_dimension(id, *dimension, *value, anchor)?;
                previews.push(json!({
                    "dimension": snake(&format!("{dimension:?}")),
                    "value_mm": mm_f64(*value),
                    "new_pose": Euler::from(preview.pose),
                    "conflicts": conflicts_json(editor.project(), &preview.conflicts),
                }));
            }
            return self.preview_only(
                json!({ "previews": previews }),
                "Dry run: nothing changed.".into(),
            );
        }
        let mut change = self.change(input.expected_revision, |editor| {
            let mut conflicts = Vec::new();
            for (dimension, value) in targets {
                let preview = editor.preview_board_dimension(id, dimension, value, anchor)?;
                conflicts = editor.edit_board_dimension(preview)?;
            }
            let project = editor.project();
            let board = project
                .board(id)
                .ok_or_else(|| ServiceError::not_found("board", id))?;
            let unit = project.display_unit;
            Ok((
                json!({
                    "board_id": id,
                    "length": LengthOut::new(board.length, unit),
                    "width": LengthOut::new(board.width, unit),
                    "thickness": LengthOut::new(board.thickness, unit),
                    "pose": pose_out(project, id, board.pose),
                    "conflicts": conflicts_json(project, &conflicts),
                }),
                format!("Resized '{}'.", board.name),
            ))
        })?;
        if let Some(c) = change.result["conflicts"].as_array()
            && !c.is_empty()
        {
            change.warnings.push(format!(
                "{} placement(s) on sheets no longer fit; re-run auto_place.",
                c.len()
            ));
        }
        Ok(change)
    }

    pub fn resize_boards(&mut self, input: ResizeBoardsInput) -> ServiceResult<Change<Value>> {
        let project = self.project()?;
        let mut selection = Vec::new();
        for reference in &input.objects {
            let id = self.resolve(Kind::Object, reference)?;
            if project.board(id).is_some() {
                selection.push(BoardSelection::Board(id));
            } else if project.assemblies.iter().any(|a| a.id == id) {
                selection.push(BoardSelection::Assembly(id));
            } else {
                return Err(ServiceError::invalid(format!(
                    "'{reference}' is not a board or assembly"
                )));
            }
        }
        let value = input.value.positive("value", input.allow_rounding)?;
        let dimension: BoardDimension = input.dimension.into();
        let editor = self.editor()?;
        let selected = editor.selected_boards(&selection)?;
        let anchors: Vec<_> = selected
            .board_ids
            .iter()
            .map(|id| (*id, input.anchor.into()))
            .collect();
        let preview =
            editor.preview_batch_board_dimension(&selection, dimension, value, &anchors)?;
        let names: Vec<String> = preview
            .targets
            .iter()
            .map(|(id, _)| object_name(editor.project(), *id))
            .collect();
        if input.dry_run {
            let conflicts = conflicts_json(editor.project(), &preview.conflicts);
            return self.preview_only(
                json!({ "boards": names, "conflicts": conflicts }),
                format!("Would resize {} board(s).", names.len()),
            );
        }
        let mut change = self.change(input.expected_revision, |editor| {
            let conflicts = editor.edit_batch_board_dimension(preview)?;
            let count = names.len();
            Ok((
                json!({ "boards": names, "conflicts": conflicts_json(editor.project(), &conflicts) }),
                format!("Resized {count} board(s)."),
            ))
        })?;
        if let Some(c) = change.result["conflicts"].as_array()
            && !c.is_empty()
        {
            change.warnings.push(format!(
                "{} placement(s) on sheets no longer fit; re-run auto_place.",
                c.len()
            ));
        }
        Ok(change)
    }

    pub fn set_board_material(
        &mut self,
        input: SetBoardMaterialInput,
    ) -> ServiceResult<Change<Value>> {
        let board = self.resolve(Kind::Board, &input.board)?;
        let material = self.resolve(Kind::Material, &input.material)?;
        let anchor = input.anchor.into();
        if input.dry_run {
            let editor = self.editor()?;
            let (thickness, conflict) = editor.preview_board_material(board, material, anchor)?;
            let project = editor.project();
            return self.preview_only(
                json!({
                    "thickness": LengthOut::new(thickness, project.display_unit),
                    "conflicts": conflicts_json(project, conflict.as_slice()),
                }),
                "Dry run: nothing changed.".into(),
            );
        }
        let mut change = self.change(input.expected_revision, |editor| {
            let conflicts = editor.assign_board_material(board, material, anchor)?;
            let project = editor.project();
            Ok((
                json!({ "conflicts": conflicts_json(project, &conflicts) }),
                format!(
                    "'{}' is now {}.",
                    object_name(project, board),
                    object_name(project, material)
                ),
            ))
        })?;
        let count = change.result["conflicts"].as_array().map_or(0, Vec::len);
        if count > 0 {
            change.warnings.push(format!(
                "{count} placement(s) no longer fit; re-run auto_place."
            ));
        }
        Ok(change)
    }

    pub fn set_board_grain(&mut self, input: SetBoardGrainInput) -> ServiceResult<Change<Value>> {
        let board = self.resolve(Kind::Board, &input.board)?;
        let grain = match input.grain {
            GrainOverride::MaterialDefault => None,
            GrainOverride::Length => Some(BoardGrain::Length),
            GrainOverride::Width => Some(BoardGrain::Width),
            GrainOverride::Unrestricted => Some(BoardGrain::Unrestricted),
        };
        self.change(input.expected_revision, |editor| {
            let conflict = editor.set_board_grain_override(board, grain)?;
            let project = editor.project();
            Ok((
                json!({ "conflicts": conflicts_json(project, conflict.as_slice()) }),
                format!("Set the grain of '{}'.", object_name(project, board)),
            ))
        })
    }

    pub fn rename_object(&mut self, input: RenameInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Object, &input.object)?;
        self.change(input.expected_revision, |editor| {
            editor.rename_object(id, &input.name)?;
            Ok((
                json!({ "id": id, "name": input.name.trim() }),
                format!("Renamed to '{}'.", input.name.trim()),
            ))
        })
    }

    pub fn delete_object(&mut self, input: RefInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Object, &input.reference)?;
        let project = self.project()?;
        let name = object_name(project, id);
        let is_hardware = project.hardware.iter().any(|h| h.id == id);
        // What goes with it.
        let scope = if is_hardware {
            HashSet::from([id])
        } else {
            descendants_of(project, id)
        };
        let boards: Vec<_> = project
            .boards
            .iter()
            .filter(|b| b.id == id || b.parent_id.is_some_and(|p| scope.contains(&p)))
            .map(|b| b.id)
            .collect();
        let hinges: Vec<_> = project
            .hinge_installations
            .iter()
            .filter(|h| boards.contains(&h.door_board_id) || boards.contains(&h.mounting_board_id))
            .map(|h| h.id)
            .collect();
        let report = json!({
            "boards": boards.iter().map(|b| object_name(project, *b)).collect::<Vec<_>>(),
            "sheet_placements": project.allocations.iter().filter(|a| boards.contains(&a.board_id)).count(),
            "hinges": hinges.len(),
        });
        if input.dry_run {
            return self.preview_only(
                report,
                format!("Would delete '{name}' and everything listed."),
            );
        }
        self.change(input.expected_revision, |editor| {
            if is_hardware {
                editor.remove_placeholder(id)?;
            } else {
                crate::door_joint::delete_object(editor, id)?;
            }
            Ok((report, format!("Deleted '{name}'.")))
        })
    }

    // -------------------------------------------------------------- placement

    pub fn set_board_pose(&mut self, input: SetBoardPoseInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Board, &input.board)?;
        let frame = match input.frame {
            Frame::Parent => CoordinateFrame::LocalParent,
            Frame::World => CoordinateFrame::World,
        };
        let project = self.project()?;
        let board = project
            .board(id)
            .ok_or_else(|| ServiceError::not_found("board", id))?;
        let current = match frame {
            CoordinateFrame::LocalParent => board.pose,
            CoordinateFrame::World => placement::world_pose(project, id)?,
        };
        let current_euler = Euler::from(current);
        let position_edited = [input.x.is_some(), input.y.is_some(), input.z.is_some()];
        let rotation_edited = input.rx.is_some() || input.ry.is_some() || input.rz.is_some();
        let numeric = NumericPose {
            position_mm: [
                input.x.unwrap_or(current.translation_mm[0]),
                input.y.unwrap_or(current.translation_mm[1]),
                input.z.unwrap_or(current.translation_mm[2]),
            ],
            rotation_degrees_xyz: [
                input.rx.unwrap_or(current_euler.rx),
                input.ry.unwrap_or(current_euler.ry),
                input.rz.unwrap_or(current_euler.rz),
            ],
            frame,
        };
        self.change(input.expected_revision, |editor| {
            let mut session = PlacementSession::begin(editor, id)?;
            session.preview_numeric_edited(numeric, position_edited, rotation_edited)?;
            if let Some(preset) = input.preset {
                let project = session.project();
                let now = match frame {
                    CoordinateFrame::LocalParent => project
                        .board(id)
                        .map(|b| b.pose)
                        .ok_or_else(|| ServiceError::not_found("board", id))?,
                    CoordinateFrame::World => placement::world_pose(project, id)?,
                };
                let target = placement::preset_pose(now, preset.into())?;
                session.preview_exact_framed(frame, target, [false; 3])?;
            }
            session.accept()?;
            let project = editor.project();
            let board = project
                .board(id)
                .ok_or_else(|| ServiceError::not_found("board", id))?;
            Ok((
                json!({ "board_id": id, "pose": pose_out(project, id, board.pose) }),
                format!("Moved '{}'.", board.name),
            ))
        })
    }

    pub fn place_board_on_face(&mut self, input: PlaceOnFaceInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Board, &input.board)?;
        let target = self.resolve(Kind::Board, &input.target)?;
        let gap = input
            .gap
            .as_ref()
            .map(|g| g.millimetres("gap", true))
            .transpose()?
            .unwrap_or(0.0);
        let face = FacePlacement {
            source_face: input.source_face.into(),
            target_id: target,
            target_face: input.target_face.into(),
            source_align: input.source_align.unwrap_or_default().map(Into::into),
            target_align: input.target_align.unwrap_or_default().map(Into::into),
            offset_mm: input.offset_mm.unwrap_or([0.0; 2]),
            gap_mm: gap,
        };
        if input.dry_run {
            let project = self.project()?;
            let world = placement::face_pose(project, id, face)?;
            return self.preview_only(
                json!({ "world": Euler::from(world) }),
                "Dry run: nothing changed.".into(),
            );
        }
        self.change(input.expected_revision, |editor| {
            let mut session = PlacementSession::begin(editor, id)?;
            session.preview_face(face)?;
            session.accept()?;
            let project = editor.project();
            let board = project
                .board(id)
                .ok_or_else(|| ServiceError::not_found("board", id))?;
            Ok((
                json!({ "board_id": id, "pose": pose_out(project, id, board.pose) }),
                format!(
                    "Placed '{}' against '{}'.",
                    board.name,
                    object_name(project, target)
                ),
            ))
        })
    }

    pub fn transform_objects(&mut self, input: TransformInput) -> ServiceResult<Change<Value>> {
        let ids = self.resolve_all(Kind::Object, &input.objects)?;
        let rotation = rotation_from_degrees_xyz(input.rotate_degrees.unwrap_or([0.0; 3]))?;
        self.change(input.expected_revision, |editor| {
            editor.transform_selection(
                &ids,
                input.translate_mm.unwrap_or([0.0; 3]),
                rotation,
                input.pivot_mm.unwrap_or([0.0; 3]),
            )?;
            let project = editor.project();
            let poses: Vec<_> = ids
                .iter()
                .map(|id| json!({ "id": id, "name": object_name(project, *id), "world": world_of(project, *id).map(Euler::from) }))
                .collect();
            Ok((json!({ "objects": poses }), format!("Moved {} object(s).", ids.len())))
        })
    }

    pub fn measure(&self, input: MeasureInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let ids = self.resolve_all(Kind::Object, &input.objects)?;
        let frame = match &input.relative_to {
            None => measurements::Frame::World,
            Some(r) => measurements::Frame::Object(self.resolve(Kind::Object, r)?),
        };
        let scope = match input.scope {
            ScopeName::Body => Scope::Body,
            ScopeName::Overall => Scope::Overall,
        };
        let m = measurements::measure(project, &ids, scope, frame)?;
        let round = |v: [f64; 3]| v.map(|x| (x * 1000.0).round() / 1000.0);
        Ok(json!({
            "min_mm": round(m.minimum_mm),
            "max_mm": round(m.maximum_mm),
            "size_mm": round(m.dimensions_mm),
            "boards": m.board_count,
            "hardware": m.hardware_count,
        }))
    }

    // ------------------------------------------------------------- assemblies

    pub fn get_outliner(&self) -> ServiceResult<Value> {
        let project = self.project()?;
        let statuses = diagnose(project);
        fn node(
            project: &Project,
            statuses: &[crate::allocation_diagnostics::BoardDiagnostic],
            parent: Option<Uuid>,
        ) -> Vec<Value> {
            let mut out = Vec::new();
            for a in project.assemblies.iter().filter(|a| a.parent_id == parent) {
                out.push(json!({
                    "id": a.id,
                    "name": a.name,
                    "kind": "assembly",
                    "children": node(project, statuses, Some(a.id)),
                }));
            }
            for b in project.boards.iter().filter(|b| b.parent_id == parent) {
                let unit = project.display_unit;
                out.push(json!({
                    "id": b.id,
                    "name": b.name,
                    "kind": "board",
                    "size": format!(
                        "{} × {} × {}",
                        crate::dimension_input::format_length(b.length, unit, crate::dimension_input::Locale::En, 1),
                        crate::dimension_input::format_length(b.width, unit, crate::dimension_input::Locale::En, 1),
                        crate::dimension_input::format_length(b.thickness, unit, crate::dimension_input::Locale::En, 1)
                    ),
                    "material": project.material(b.material_id).map(|m| m.name.clone()),
                    "cut_plan": statuses.iter().find(|d| d.board_id == b.id).map(|d| status_name(d.status)),
                }));
            }
            for h in project.hardware.iter().filter(|h| h.parent_id == parent) {
                out.push(json!({
                    "id": h.id,
                    "name": h.name,
                    "kind": match h.kind { HardwareKind::Placeholder { .. } => "hardware", HardwareKind::Catalog { .. } => "catalog_hardware" },
                }));
            }
            out
        }
        Ok(json!({ "revision": project.revision, "tree": node(project, &statuses, None) }))
    }

    pub fn find_objects(&self, input: FindInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let query = input.query.trim().to_lowercase();
        let kinds: Vec<(&str, Kind)> = [
            ("board", Kind::Board),
            ("assembly", Kind::Assembly),
            ("hardware", Kind::Hardware),
            ("material", Kind::Material),
            ("stock", Kind::Stock),
            ("door", Kind::Door),
            ("catalog", Kind::Catalog),
        ]
        .into_iter()
        .filter(|(name, _)| input.kind.as_deref().is_none_or(|k| k == *name))
        .collect();
        let mut rows = Vec::new();
        for (label, kind) in kinds {
            for (id, name) in crate::service::workspace::candidates(project, kind) {
                let alias = project.stock_alias(id).unwrap_or_default().to_lowercase();
                if name.to_lowercase().contains(&query)
                    || id.to_string().starts_with(&query)
                    || (!alias.is_empty() && alias == query)
                {
                    rows.push(json!({
                        "id": id,
                        "name": name,
                        "kind": label,
                        "parent": parent_name(project, id),
                        "alias": project.stock_alias(id),
                    }));
                }
            }
        }
        Ok(json!({ "matches": rows }))
    }

    pub fn get_assembly(&self, input: GetInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let id = self.resolve(Kind::Assembly, &input.reference)?;
        let assembly = project
            .assemblies
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| ServiceError::not_found("assembly", id))?;
        let scope = descendants_of(project, id);
        let boards: Vec<_> = project
            .boards
            .iter()
            .filter(|b| b.parent_id.is_some_and(|p| scope.contains(&p)))
            .map(|b| json!({ "id": b.id, "name": b.name }))
            .collect();
        let measured =
            measurements::measure(project, &[id], Scope::Body, measurements::Frame::World).ok();
        Ok(json!({
            "id": id,
            "name": assembly.name,
            "parent": parent_name(project, id),
            "pose": pose_out(project, id, assembly.pose),
            "boards": boards,
            "sub_assemblies": project.assemblies.iter().filter(|a| a.parent_id == Some(id)).map(|a| json!({ "id": a.id, "name": a.name })).collect::<Vec<_>>(),
            "bounds": measured.map(|m| json!({ "min": m.minimum_mm, "max": m.maximum_mm, "size": m.dimensions_mm })),
        }))
    }

    pub fn group_objects(&mut self, input: GroupInput) -> ServiceResult<Change<Value>> {
        let ids = self.resolve_all(Kind::Object, &input.objects)?;
        let parent = self.opt_ref(Kind::Assembly, &input.parent)?;
        self.change(input.expected_revision, |editor| {
            let id = editor.group_objects(
                &ids,
                parent,
                input.name.trim(),
                input.pivot_mm.unwrap_or([0.0; 3]),
            )?;
            Ok((
                json!({ "assembly_id": id }),
                format!(
                    "Grouped {} object(s) as '{}'.",
                    ids.len(),
                    input.name.trim()
                ),
            ))
        })
    }

    pub fn ungroup_assembly(&mut self, input: RefInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Assembly, &input.reference)?;
        let name = object_name(self.project()?, id);
        self.change(input.expected_revision, |editor| {
            editor.ungroup_assembly(id)?;
            Ok((json!({}), format!("Ungrouped '{name}'.")))
        })
    }

    pub fn reparent_objects(&mut self, input: ReparentInput) -> ServiceResult<Change<Value>> {
        let ids = self.resolve_all(Kind::Object, &input.objects)?;
        let parent = self.opt_ref(Kind::Assembly, &input.parent)?;
        self.change(input.expected_revision, |editor| {
            editor.reparent_objects(&ids, parent)?;
            Ok((
                json!({}),
                format!(
                    "Moved {} object(s) to {}.",
                    ids.len(),
                    input.parent.as_deref().unwrap_or("the top level")
                ),
            ))
        })
    }

    pub fn duplicate_assembly(
        &mut self,
        input: DuplicateAssemblyInput,
    ) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Assembly, &input.assembly)?;
        let mut change = self.change(input.expected_revision, |editor| {
            let before: HashSet<Uuid> = editor.project().boards.iter().map(|b| b.id).collect();
            let copy = editor.duplicate_assembly(id, input.offset_mm.unwrap_or([0.0; 3]))?;
            if let Some(name) = &input.name {
                editor.rename_object(copy, name)?;
            }
            let project = editor.project();
            let boards: Vec<_> = project
                .boards
                .iter()
                .filter(|b| !before.contains(&b.id))
                .map(|b| json!({ "id": b.id, "name": b.name }))
                .collect();
            Ok((
                json!({ "assembly_id": copy, "boards": boards }),
                format!("Copied '{}'.", object_name(project, id)),
            ))
        })?;
        change
            .warnings
            .push("Hinges and door relationships are not copied.".into());
        Ok(change)
    }

    pub fn placeholder(&mut self, input: PlaceholderInput) -> ServiceResult<Change<Value>> {
        let dims = [0, 1, 2].map(|i| {
            input.dimensions[i].positive(
                ["dimension x", "dimension y", "dimension z"][i],
                input.allow_rounding,
            )
        });
        let dims = [dims[0].clone()?, dims[1].clone()?, dims[2].clone()?];
        let parent = self.opt_ref(Kind::Assembly, &input.parent)?;
        let existing = self.opt_ref(Kind::Hardware, &input.hardware)?;
        let world = match (input.pose, existing) {
            (Some(p), _) => p.pose()?,
            (None, Some(id)) => world_of(self.project()?, id).unwrap_or(Pose::IDENTITY),
            (None, None) => Pose::IDENTITY,
        };
        self.change(input.expected_revision, |editor| match existing {
            Some(id) => {
                editor.edit_placeholder(id, input.name.clone(), dims, parent, world)?;
                Ok((
                    json!({ "hardware_id": id }),
                    format!("Updated '{}'.", input.name),
                ))
            }
            None => {
                let id = editor.create_placeholder(input.name.clone(), dims, parent, world)?;
                Ok((
                    json!({ "hardware_id": id }),
                    format!("Created '{}'.", input.name),
                ))
            }
        })
    }

    // -------------------------------------------------------------- templates

    pub fn list_templates(&self) -> Value {
        let describe = |kind: TemplateKind, about: &str| {
            json!({
                "kind": snake(&format!("{kind:?}")),
                "about": about,
                "dimensions": kind.fields().iter().map(|f| snake(&format!("{f:?}"))).collect::<Vec<_>>(),
                "material_roles": kind.roles().iter().map(|r| match r {
                    MaterialRole::Box => "box".to_owned(),
                    other => snake(&format!("{other:?}")),
                }).collect::<Vec<_>>(),
            })
        };
        json!({
            "templates": [
                describe(TemplateKind::Base, "Kitchen base cabinet: 2 sides, bottom, front and rear top rails, overlay back. No doors (add them as boards)."),
                describe(TemplateKind::Wall, "Wall cabinet: 2 sides, top, bottom, one shelf, overlay back. No doors."),
                describe(TemplateKind::Drawers, "Drawer stack: carcass, drawer boxes (sides, front, back, bottom) and external fronts."),
            ],
            "defaults_mm": { "width": 600, "depth": 560, "height": 720, "rail_width": 80, "shelf_height": 320, "box_depth": 500, "side_clearance": 13, "rear_clearance": 20, "vertical_clearance": 8, "front_reveal": 3, "front_gap": 3, "drawer_count": 3 },
            "default_materials": { "carcass": "White MDF 15 mm", "back": "HDF 3 mm", "box": "White MDF 15 mm", "box_bottom": "HDF 3 mm", "external_front": "White MDF 18 mm" },
            "standard_materials": BR_STANDARD.iter().map(|p| json!({ "name": p.name_en, "thickness_mm": p.thickness_mm, "sheet_mm": p.sheet_mm })).collect::<Vec<_>>(),
            "coordinates": "The cabinet front is at Y=0 facing -Y, X runs to the right from the left side, Z up from the floor.",
        })
    }

    pub fn generate_template(&mut self, input: GenerateTemplateInput) -> ServiceResult<Value> {
        let kind: TemplateKind = input.kind.into();
        let into_current = matches!(input.into, TemplateTarget::CurrentProject);
        if !into_current {
            self.check_can_replace(input.discard_changes)?;
        }
        let language = input.language.map_or(self.language, Into::into);
        let currency = input.currency.unwrap_or(match self.document.as_ref() {
            Some(d) if into_current => match d.editor.project().currency {
                crate::money::Currency::Brl => CurrencyCode::BRL,
                crate::money::Currency::Usd => CurrencyCode::USD,
            },
            _ => CurrencyCode::BRL,
        });
        let unit = input
            .display_unit
            .map_or(crate::units::Unit::Mm, Into::into);
        let mut setup = TemplateSetup::new(kind, input.name.trim(), currency.into(), unit);
        setup.seed_standard_materials(language);
        setup.add_sheets = input.add_sheets;
        if into_current {
            // Reuse project materials that match a seeded standard one.
            let project = self.project()?;
            for draft in &mut setup.materials {
                let thickness = draft.thickness.conversion.suggested();
                if let Some(existing) = project.materials.iter().find(|m| {
                    m.name.eq_ignore_ascii_case(&draft.name) && m.default_thickness == thickness
                }) {
                    let old = draft.id;
                    draft.id = existing.id;
                    for role in setup.roles.values_mut() {
                        if *role == old {
                            *role = existing.id;
                        }
                    }
                }
            }
            for m in &project.materials {
                if !setup.materials.iter().any(|d| d.id == m.id) {
                    setup.materials.push(crate::template_setup::DraftMaterial {
                        id: m.id,
                        name: m.name.clone(),
                        thickness: ProposedLength::new(Conversion::Exact(m.default_thickness)),
                        grain: m.default_grain,
                        color: project.material_colors.get(&m.id).copied(),
                    });
                }
            }
        }
        // Role overrides.
        let specs = [
            (MaterialRole::Carcass, &input.materials.carcass),
            (MaterialRole::Back, &input.materials.back),
            (MaterialRole::Box, &input.materials.drawer_box),
            (MaterialRole::BoxBottom, &input.materials.box_bottom),
            (MaterialRole::ExternalFront, &input.materials.external_front),
        ];
        for (role, spec) in specs {
            let Some(spec) = spec else { continue };
            if !kind.roles().contains(&role) {
                continue;
            }
            let id = match spec {
                MaterialSpec::Ref(reference) => {
                    let wanted = reference.trim();
                    let matches: Vec<_> = setup
                        .materials
                        .iter()
                        .filter(|d| {
                            d.id.to_string() == wanted
                                || d.name.eq_ignore_ascii_case(wanted)
                                || format!(
                                    "{} {}",
                                    d.name,
                                    crate::service::dto::mm_f64(d.thickness.conversion.suggested())
                                )
                                .eq_ignore_ascii_case(wanted)
                                || format!(
                                    "{} {} mm",
                                    d.name,
                                    crate::service::dto::mm_f64(d.thickness.conversion.suggested())
                                )
                                .eq_ignore_ascii_case(wanted)
                        })
                        .collect();
                    match matches.as_slice() {
                        [one] => one.id,
                        [] => {
                            return Err(ServiceError::not_found("material", wanted)
                                .hint("Use a standard name with thickness like \"White MDF 18\", an existing material, or {name, thickness}."));
                        }
                        many => {
                            return Err(ServiceError::new(ErrorCode::AmbiguousRef, format!("'{wanted}' matches {} materials", many.len()))
                                .hint("Add the thickness, e.g. \"White MDF 18\".")
                                .details(json!(many.iter().map(|d| json!({ "name": d.name, "thickness_mm": crate::service::dto::mm_f64(d.thickness.conversion.suggested()) })).collect::<Vec<_>>())));
                        }
                    }
                }
                MaterialSpec::New {
                    name,
                    thickness,
                    grain,
                    color,
                } => {
                    let t = thickness.positive("thickness", input.allow_rounding)?;
                    let color = color.as_ref().map(ColorInput::resolve).transpose()?;
                    setup.add_material(
                        name.trim(),
                        ProposedLength::new(Conversion::Exact(t)),
                        grain.map_or(BoardGrain::Unrestricted, Into::into),
                        color,
                    )
                }
            };
            setup.roles.insert(role, id);
        }
        if into_current {
            // Only add the materials the roles use.
            let used: HashSet<Uuid> = setup.roles.values().copied().collect();
            let project_ids: HashSet<Uuid> =
                self.project()?.materials.iter().map(|m| m.id).collect();
            setup
                .materials
                .retain(|d| used.contains(&d.id) || project_ids.contains(&d.id));
        }
        let defaults: [(TemplateField, i64, &Option<LengthInput>); 11] = [
            (TemplateField::Width, 600, &input.dimensions.width),
            (TemplateField::Depth, 560, &input.dimensions.depth),
            (TemplateField::Height, 720, &input.dimensions.height),
            (TemplateField::RailWidth, 80, &input.dimensions.rail_width),
            (
                TemplateField::ShelfHeight,
                320,
                &input.dimensions.shelf_height,
            ),
            (TemplateField::BoxDepth, 500, &input.dimensions.box_depth),
            (
                TemplateField::SideClearance,
                13,
                &input.dimensions.side_clearance,
            ),
            (
                TemplateField::RearClearance,
                20,
                &input.dimensions.rear_clearance,
            ),
            (
                TemplateField::VerticalClearance,
                8,
                &input.dimensions.vertical_clearance,
            ),
            (
                TemplateField::FrontReveal,
                3,
                &input.dimensions.front_reveal,
            ),
            (TemplateField::FrontGap, 3, &input.dimensions.front_gap),
        ];
        for (field, default, value) in defaults {
            if !kind.fields().contains(&field) {
                continue;
            }
            let length = match value {
                Some(v) => v.resolve(&snake(&format!("{field:?}")), input.allow_rounding)?,
                None => Length::from_micrometres(default * 1000),
            };
            setup
                .dimensions
                .insert(field, ProposedLength::new(Conversion::Exact(length)));
        }
        if kind == TemplateKind::Drawers {
            setup.drawer_count = Some(input.drawer_count.unwrap_or(3));
        }

        let (assembly_id, fits) = if into_current {
            let editor = self.editor_mut()?;
            let inserted = setup.insert_into(editor, input.offset_mm.unwrap_or([0.0; 3]))?;
            // Name the cabinet assembly after the request.
            editor.transact(|p| -> Result<(), ()> {
                if let Some(a) = p
                    .assemblies
                    .iter_mut()
                    .find(|a| a.id == inserted.assembly_id)
                {
                    input.name.trim().clone_into(&mut a.name);
                }
                Ok(())
            })?;
            (inserted.assembly_id, inserted.fits)
        } else {
            let mut generated = setup.generate()?;
            let root = generated.assembly_id;
            let name = input.name.trim().to_owned();
            generated.editor.transact(|p| -> Result<(), ()> {
                if let Some(a) = p.assemblies.iter_mut().find(|a| a.id == root) {
                    a.name = name;
                }
                Ok(())
            })?;
            // The generated project is new and unsaved but starts clean.
            generated.editor.mark_saved();
            self.document = Some(Document {
                editor: generated.editor,
                path: None,
            });
            self.searches.clear();
            (generated.assembly_id, generated.fits)
        };
        let document = self.document()?;
        let project = document.editor.project();
        let unit = project.display_unit;
        let boards: Vec<_> = fits
            .iter()
            .filter_map(|(id, _)| project.board(*id))
            .map(|b| {
                let allocation = project.allocations.iter().find(|a| a.board_id == b.id);
                json!({
                    "id": b.id,
                    "name": b.name,
                    "size": [LengthOut::new(b.length, unit).mm, LengthOut::new(b.width, unit).mm, LengthOut::new(b.thickness, unit).mm],
                    "material": project.material(b.material_id).map(|m| m.name.clone()),
                    "sheet": allocation.and_then(|a| project.stock_alias(a.stock_id)),
                })
            })
            .collect();
        let unplaced = boards.iter().filter(|b| b["sheet"].is_null()).count();
        let mut result = json!({
            "assembly_id": assembly_id,
            "boards": boards,
            "sheets": project.stock.iter().map(|s| json!({ "alias": project.stock_alias(s.id), "name": s.name, "material": object_name(project, s.material_id) })).collect::<Vec<_>>(),
            "project": summary(document),
        });
        if unplaced > 0 {
            result["warnings"] = json!([format!(
                "{unplaced} board(s) have no sheet: their material has no standard sheet size. Use create_stock, then auto_place."
            )]);
        }
        Ok(result)
    }

    fn check_can_replace(&self, discard: bool) -> ServiceResult<()> {
        if let Some(document) = &self.document
            && document.editor.is_dirty()
            && !discard
        {
            return Err(ServiceError::new(
                ErrorCode::UnsavedChanges,
                format!("'{}' has unsaved changes", document.editor.project().name),
            )
            .hint("Save it with save_project, pass discard_changes: true, or use into: \"current_project\"."));
        }
        Ok(())
    }
}
