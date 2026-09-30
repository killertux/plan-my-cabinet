//! Project-driven, depth-tested 3D viewport. Camera state is presentation-only.
use eframe::egui;
use plan_my_cabinet::domain::{Board, Project};
use plan_my_cabinet::placement::{BoardFace, Side};
use sha2::Digest;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

mod measure_ui;
mod thumbnail;
pub(crate) use measure_ui::measurement_readout;
pub use plan_my_cabinet::render::camera::*;
pub(crate) use thumbnail::saved_thumbnail;

pub struct MoveTool {
    pub mode: ToolMode,
    pub face_snap: bool,
    pub grid_snap: bool,
    /// Set by the host each frame: face handles are offered only when a
    /// resize could be committed (Design, no other edit in progress).
    pub resize_enabled: bool,
    /// Set by the host each frame: in the Hardware workspace only hardware
    /// (feet, other hardware) can be dragged, never boards.
    pub hardware_only: bool,
    /// Set by the host each frame: false while an inspector position draft
    /// is unsaved, so a drag can't race the typed values.
    pub drag_enabled: bool,
    resize: Option<resize::ResizeDrag>,
    resize_hover: Option<(usize, bool)>,
    grid_edit_requested: bool,
    drag: Option<MoveDrag>,
    capture_snap: Option<CaptureSnap>,
    capture_evidence: Option<Result<serde_json::Value, String>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolMode {
    #[default]
    Navigate,
    Move,
    Measure,
}

/// Opt-in, capture-only transient drag state. Never persisted or accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureSnap {
    Face,
    Grid,
}

impl CaptureSnap {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Face => "face",
            Self::Grid => "grid",
        }
    }
}

impl Default for MoveTool {
    fn default() -> Self {
        Self {
            mode: ToolMode::Navigate,
            face_snap: true,
            grid_snap: true,
            resize_enabled: false,
            hardware_only: false,
            drag_enabled: true,
            resize: None,
            resize_hover: None,
            grid_edit_requested: false,
            drag: None,
            capture_snap: None,
            capture_evidence: None,
        }
    }
}

struct MoveDrag {
    board_id: Uuid,
    start: egui::Pos2,
    world: plan_my_cabinet::units::Pose,
    selection_ids: HashSet<Uuid>,
    selection_active: Option<Uuid>,
    snap: Option<DragSnap>,
    last_pose: Option<plan_my_cabinet::units::Pose>,
}

#[derive(Clone, Copy)]
enum DragSnap {
    Face(plan_my_cabinet::placement::SnapCandidate),
    Grid,
}

pub enum DragAction {
    Preview(Uuid, plan_my_cabinet::units::Pose),
    Accept(Uuid, Option<plan_my_cabinet::units::Pose>),
    Cancel(HashSet<Uuid>, Option<Uuid>),
}

/// A canvas click proposed to the owning editor; `picked: None` means background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionProposal {
    pub picked: Option<Uuid>,
    pub additive: bool,
}

/// Independent drag and selection events from a viewport frame.
#[derive(Default)]
pub struct ViewportInteraction {
    pub drag: Option<DragAction>,
    pub selection: Option<SelectionProposal>,
    pub resize: Option<ResizeAction>,
}

impl MoveTool {
    pub fn configure_capture_snap(&mut self, mode: CaptureSnap) {
        self.mode = ToolMode::Move;
        self.face_snap = mode == CaptureSnap::Face;
        self.grid_snap = true;
        self.capture_snap = Some(mode);
    }

    pub fn capture_evidence(&self) -> Option<Result<serde_json::Value, String>> {
        self.capture_evidence.clone()
    }

    fn prepare_capture_snap(
        &mut self,
        project: &Project,
        camera: &Camera,
        rect: egui::Rect,
        selection: &mut Selection,
    ) {
        let Some(mode) = self.capture_snap else {
            return;
        };
        // Recompute from the current canvas geometry, including the camera's
        // current aspect, before each frame. Never synthesize a SnapCandidate.
        self.capture_evidence =
            Some(self.capture_candidate(project, camera, rect, selection, mode));
    }

    fn capture_candidate(
        &mut self,
        project: &Project,
        camera: &Camera,
        rect: egui::Rect,
        selection: &mut Selection,
        mode: CaptureSnap,
    ) -> Result<serde_json::Value, String> {
        let mut sources: Vec<_> = project
            .boards
            .iter()
            .filter(|b| selection.visible(project, b.id))
            .collect();
        sources.sort_by_key(|b| (b.id != plan_my_cabinet::reference_fixture::SHELF_ID, b.id));
        for source in sources {
            let Ok(start) = plan_my_cabinet::placement::world_pose(project, source.id) else {
                continue;
            };
            let Ok(candidates) =
                plan_my_cabinet::placement::snap_candidates(project, source.id, start, 1_000_000.0)
            else {
                continue;
            };
            for candidate in candidates {
                if !visible_face_with_selection(
                    project, camera, rect, source.id, &candidate, selection,
                ) {
                    continue;
                }
                // An offset of two millimetres remains in the 32-point face
                // radius while moving the origin enough to enable grid snap.
                let mut translation = candidate.world_pose.translation_mm;
                translation[0] += 2.0;
                let Ok(free) = plan_my_cabinet::units::Pose::new(translation, start.rotation)
                else {
                    continue;
                };
                let choose = |face, grid, alt| {
                    drag_target_with_modes_visible(
                        project, camera, rect, source.id, free, start, alt, face, grid, selection,
                    )
                };
                let (face_pose, face_result) = choose(true, true, false);
                let (grid_pose, grid_result) = choose(false, true, false);
                let bypass = choose(true, true, true);
                let precedence = matches!(choose(true, true, false).1, Some(DragSnap::Face(_)));
                let Some(DragSnap::Face(face)) = face_result else {
                    continue;
                };
                if face.target_id != candidate.target_id
                    || !matches!(grid_result, Some(DragSnap::Grid))
                {
                    continue;
                }
                let (pose, snap) = match mode {
                    CaptureSnap::Face => (face_pose, DragSnap::Face(face)),
                    CaptureSnap::Grid => (grid_pose, DragSnap::Grid),
                };
                selection.choose(Some(source.id), false);
                self.drag = Some(MoveDrag {
                    board_id: source.id,
                    start: camera
                        .project(start.translation_mm, rect)
                        .unwrap_or(rect.center()),
                    world: start,
                    selection_ids: selection.ids.clone(),
                    selection_active: selection.active,
                    snap: Some(snap),
                    last_pose: Some(pose),
                });
                return Ok(serde_json::json!({
                    "mode": mode.as_str(), "redesign_acceptance": false,
                    "source_id": source.id, "target_id": face.target_id,
                    "configured_modes": {"face": self.face_snap, "grid": self.grid_snap},
                    "candidate": if mode == CaptureSnap::Face { "face" } else { "grid" },
                    "source_face": {"axis": face.source_face.axis, "side": format!("{:?}", face.source_face.side)},
                    "target_face": {"axis": face.target_face.axis, "side": format!("{:?}", face.target_face.side)},
                    "free_pose": free, "candidate_pose": pose,
                    "face_precedes_grid_when_both": precedence,
                    "grid_when_face_disabled": matches!(grid_result, Some(DragSnap::Grid)),
                    "alt_bypass": bypass.1.is_none() && bypass.0 == free,
                    "fixture_sha256": format!("{:x}", sha2::Sha256::digest(plan_my_cabinet::persistence::serialize(project).map_err(|e| e.to_string())?)),
                }));
            }
        }
        Err("No visible face and grid candidates share an eligible fixture drag at this canvas/camera".into())
    }
    pub(crate) fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn resizing(&self) -> bool {
        self.resize.is_some()
    }

    pub fn cancel(&mut self) -> Option<DragAction> {
        self.drag
            .take()
            .map(|drag| DragAction::Cancel(drag.selection_ids, drag.selection_active))
    }

    pub fn take_grid_edit_request(&mut self) -> bool {
        std::mem::take(&mut self.grid_edit_requested)
    }
}

pub(crate) fn has_frame_bounds(project: &Project, selection: &Selection) -> bool {
    bounds_visible(project, &selection.ids, selection).is_some()
}

/// Camera/tool commands are presentation-only; the action registry guards their invocation.
pub(crate) fn apply_control(
    request: crate::actions::Request,
    camera: &mut Camera,
    tool: &mut MoveTool,
    project: &Project,
    selection: &Selection,
) {
    use crate::actions::{ActionId as A, Argument};
    match (request.id, request.argument) {
        (A::ViewNavigate, _) => tool.mode = ToolMode::Navigate,
        (A::ViewMove, _) => tool.mode = ToolMode::Move,
        (A::ViewMeasure, _) => tool.mode = ToolMode::Measure,
        (A::ViewFrame, _) => {
            if let Some(b) = bounds_visible(project, &selection.ids, selection) {
                camera.frame(b, camera.viewport_aspect);
            } else if selection.ids.is_empty() {
                camera.target = [0.0; 3];
                camera.distance = 1500.0;
            }
        }
        (A::ViewPreset, Argument::Preset(preset)) => camera.set_preset(preset),
        (A::ViewProjection, Argument::Projection(projection)) => camera.projection = projection,
        _ => unreachable!("viewport action must pass registry guard"),
    }
}

/// Return the closest positive surface crossing, including exit faces when the eye is inside.
/// Each face is tested independently so parallel rays and edge hits have stable results.
fn board_hit(
    ray: Ray,
    pose: plan_my_cabinet::units::Pose,
    dimensions: [f64; 3],
) -> Option<(f64, usize)> {
    let q = pose.rotation;
    let inverse = plan_my_cabinet::units::Quaternion {
        w: q.w,
        x: -q.x,
        y: -q.y,
        z: -q.z,
    };
    let origin = inverse.rotate(std::array::from_fn(|i| {
        ray.origin[i] - pose.translation_mm[i]
    }));
    let direction = inverse.rotate(ray.direction);
    let mut best: Option<(f64, usize)> = None;
    for axis in 0..3 {
        if direction[axis].abs() <= 1e-14 {
            continue;
        }
        for side in 0..2 {
            let t =
                ((if side == 0 { 0.0 } else { dimensions[axis] }) - origin[axis]) / direction[axis];
            if !t.is_finite() || t <= 1e-9 {
                continue;
            }
            let inside = (0..3).filter(|&i| i != axis).all(|i| {
                let v = origin[i] + t * direction[i];
                v >= -1e-7 && v <= dimensions[i] + 1e-7
            });
            let face = axis * 2 + side;
            if inside
                && best.is_none_or(|(old, old_face)| {
                    t < old - 1e-9 || ((t - old).abs() <= 1e-9 && face < old_face)
                })
            {
                best = Some((t, face));
            }
        }
    }
    best
}

#[cfg(test)]
fn pick(project: &Project, camera: &Camera, pointer: egui::Pos2, rect: egui::Rect) -> Option<Uuid> {
    pick_visible(project, camera, pointer, rect, &Selection::default())
}

fn pick_visible(
    project: &Project,
    camera: &Camera,
    pointer: egui::Pos2,
    rect: egui::Rect,
    selection: &Selection,
) -> Option<Uuid> {
    let ray = camera.ray(pointer, rect);
    let boards = project
        .boards
        .iter()
        .filter(|board| selection.visible(project, board.id))
        .filter_map(|board| {
            let pose = world_pose(project, board)?;
            let dimensions = board
                .blank_dimensions()
                .map(|v| v.micrometres() as f64 / 1000.0);
            let (distance, _) = board_hit(ray, pose, dimensions)?;
            Some((distance, board.id))
        });
    let hardware = project
        .hardware
        .iter()
        .filter(|h| selection.visible(project, h.id))
        .filter_map(|h| {
            let dimensions = project.hardware_dimensions(h)?;
            let pose = plan_my_cabinet::assembly_edit::world_pose(project, h.id).ok()?;
            let (distance, _) = board_hit(
                ray,
                pose,
                dimensions.map(|d| d.micrometres() as f64 / 1000.0),
            )?;
            Some((distance, h.id))
        });
    // Drawer slides and hinges, by the boxes they occupy.
    let fittings = plan_my_cabinet::render::hardware_mesh::pick_boxes(project, selection, None)
        .into_iter()
        .filter_map(|(id, _, boxes)| {
            boxes
                .into_iter()
                .filter_map(|(pose, size)| board_hit(ray, pose, size).map(|(d, _)| d))
                .min_by(f64::total_cmp)
                .map(|distance| (distance, id))
        })
        .collect::<Vec<_>>();
    boards
        .chain(hardware)
        .chain(fittings)
        .min_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)))
        .map(|(_, id)| id)
}

#[cfg(test)]
fn visible_face(
    project: &Project,
    camera: &Camera,
    rect: egui::Rect,
    source: Uuid,
    candidate: &plan_my_cabinet::placement::SnapCandidate,
) -> bool {
    visible_face_with_selection(
        project,
        camera,
        rect,
        source,
        candidate,
        &Selection::default(),
    )
}

fn visible_face_with_selection(
    project: &Project,
    camera: &Camera,
    rect: egui::Rect,
    source: Uuid,
    candidate: &plan_my_cabinet::placement::SnapCandidate,
    selection: &Selection,
) -> bool {
    let Some(target) = project.boards.iter().find(|b| b.id == candidate.target_id) else {
        return false;
    };
    if !selection.visible(project, target.id) {
        return false;
    }
    let Some(pose) = world_pose(project, target) else {
        return false;
    };
    let dims = target
        .blank_dimensions()
        .map(|v| v.micrometres() as f64 / 1000.0);
    let face = candidate.target_face;
    let mut center = dims.map(|d| d / 2.0);
    center[face.axis] = if face.side == Side::Positive {
        dims[face.axis]
    } else {
        0.0
    };
    let Ok(point) = pose.transform_point(center) else {
        return false;
    };
    let Some(screen) = camera.project(point, rect) else {
        return false;
    };
    if !rect.contains(screen) {
        return false;
    }
    let ray = camera.ray(screen, rect);
    project
        .boards
        .iter()
        .filter(|b| b.id != source)
        .filter(|b| selection.visible(project, b.id))
        .filter_map(|b| {
            let p = world_pose(project, b)?;
            let d = b
                .blank_dimensions()
                .map(|v| v.micrometres() as f64 / 1000.0);
            board_hit(ray, p, d).map(|(distance, index)| (distance, b.id, index))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)))
        .is_some_and(|(_, id, index)| {
            id == target.id && index == face.axis * 2 + usize::from(face.side == Side::Positive)
        })
}

#[cfg(test)]
fn screen_snap(
    project: &Project,
    camera: &Camera,
    rect: egui::Rect,
    source: Uuid,
    free: plan_my_cabinet::units::Pose,
) -> Option<plan_my_cabinet::placement::SnapCandidate> {
    screen_snap_visible(project, camera, rect, source, free, &Selection::default())
}

fn screen_snap_visible(
    project: &Project,
    camera: &Camera,
    rect: egui::Rect,
    source: Uuid,
    free: plan_my_cabinet::units::Pose,
    selection: &Selection,
) -> Option<plan_my_cabinet::placement::SnapCandidate> {
    let origin = camera.project(free.translation_mm, rect)?;
    plan_my_cabinet::placement::snap_candidates(project, source, free, 1_000_000.0)
        .ok()?
        .into_iter()
        .filter(|candidate| selection.visible(project, candidate.target_id))
        .filter_map(|candidate| {
            let screen = camera.project(candidate.world_pose.translation_mm, rect)?;
            let distance = screen.distance(origin);
            (distance <= 32.0
                && visible_face_with_selection(
                    project, camera, rect, source, &candidate, selection,
                ))
            .then_some((distance, candidate))
        })
        .min_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then_with(|| a.1.distance_mm.total_cmp(&b.1.distance_mm))
        })
        .map(|(_, candidate)| candidate)
}

#[cfg(test)]
fn drag_target(
    project: &Project,
    camera: &Camera,
    rect: egui::Rect,
    source: Uuid,
    free: plan_my_cabinet::units::Pose,
    start: plan_my_cabinet::units::Pose,
    bypass: bool,
) -> (plan_my_cabinet::units::Pose, Option<DragSnap>) {
    drag_target_with_modes_visible(
        project,
        camera,
        rect,
        source,
        free,
        start,
        bypass,
        true,
        true,
        &Selection::default(),
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn drag_target_visible(
    project: &Project,
    camera: &Camera,
    rect: egui::Rect,
    source: Uuid,
    free: plan_my_cabinet::units::Pose,
    start: plan_my_cabinet::units::Pose,
    bypass: bool,
    selection: &Selection,
) -> (plan_my_cabinet::units::Pose, Option<DragSnap>) {
    drag_target_with_modes_visible(
        project, camera, rect, source, free, start, bypass, true, true, selection,
    )
}

#[allow(clippy::too_many_arguments)] // Camera, geometry and session visibility stay independent of project data.
fn drag_target_with_modes_visible(
    project: &Project,
    camera: &Camera,
    rect: egui::Rect,
    source: Uuid,
    free: plan_my_cabinet::units::Pose,
    start: plan_my_cabinet::units::Pose,
    bypass: bool,
    face_snap: bool,
    grid_snap: bool,
    selection: &Selection,
) -> (plan_my_cabinet::units::Pose, Option<DragSnap>) {
    if bypass {
        return (free, None);
    }
    if face_snap
        && let Some(face) = screen_snap_visible(project, camera, rect, source, free, selection)
    {
        return (face.world_pose, Some(DragSnap::Face(face)));
    }
    if !grid_snap {
        return (free, None);
    }
    // A click/first drag frame must never pull a pre-existing off-grid pose onto
    // the grid. Evaluate the grid only after a visible movement of its origin.
    let Some(origin) = camera.project(free.translation_mm, rect) else {
        return (free, None);
    };
    if camera
        .project(start.translation_mm, rect)
        .is_none_or(|initial| origin.distance(initial) < 4.0)
    {
        return (free, None);
    }
    let spacing = project.grid_spacing.micrometres() as f64 / 1000.0;
    let mut pose = free;
    for axis in 0..2 {
        pose.translation_mm[axis] = (free.translation_mm[axis] / spacing).round() * spacing;
    }
    if let (Ok(pose), Some(screen)) = (
        plan_my_cabinet::units::Pose::new(pose.translation_mm, pose.rotation),
        camera.project(pose.translation_mm, rect),
    ) && screen.distance(origin) <= 18.0
    {
        return (pose, Some(DragSnap::Grid));
    }
    (free, None)
}

#[cfg(test)]
fn bounds(project: &Project, selected: &HashSet<Uuid>) -> Option<Bounds> {
    bounds_visible(project, selected, &Selection::default())
}

/// Session-only pointer hover shared by the outliner and the 3D view. It is
/// stored per frame so a stale hover never survives the pointer leaving.
pub fn set_hover(ctx: &egui::Context, id: Option<Uuid>) {
    let frame = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("pmc-scene-hover"), (frame, id)));
}

pub fn hovered(ctx: &egui::Context) -> Option<Uuid> {
    let frame = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<(u64, Option<Uuid>)>(egui::Id::new("pmc-scene-hover")))
        .filter(|(stamp, _)| frame.saturating_sub(*stamp) <= 1)
        .and_then(|(_, id)| id)
}

mod annotations;
mod canvas;
mod controls;
mod hardware_annotations;
mod resize;
mod scene_render;
pub use resize::{ResizeAction, ResizeRequest};
pub use scene_render::install;
#[cfg(test)]
use scene_render::scene_with_faces;
#[cfg(test)]
use scene_render::{Mesh, add_grid, highlight_color, scene};

#[cfg(test)]
mod boundary_tests;

#[cfg(test)]
pub fn show(
    ui: &mut egui::Ui,
    camera: &mut Camera,
    project: &Project,
    selection: &mut Selection,
    modal: bool,
    language: plan_my_cabinet::i18n::Language,
    faces: Option<(Uuid, BoardFace, Uuid, BoardFace)>,
) {
    show_move(
        ui,
        camera,
        project,
        selection,
        &mut MoveTool::default(),
        modal,
        language,
        false,
        true,
        faces,
        None,
        plan_my_cabinet::measurements::Scope::Body,
        plan_my_cabinet::measurements::Frame::World,
    );
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)] // View state and interaction state are kept separate from project data.
pub fn show_move(
    ui: &mut egui::Ui,
    camera: &mut Camera,
    project: &Project,
    selection: &mut Selection,
    tool: &mut MoveTool,
    modal: bool,
    language: plan_my_cabinet::i18n::Language,
    inverse_scroll_zoom: bool,
    material_tint: bool,
    faces: Option<(Uuid, BoardFace, Uuid, BoardFace)>,
    poses: Option<&HashMap<Uuid, plan_my_cabinet::units::Pose>>,
    measurement_scope: plan_my_cabinet::measurements::Scope,
    measurement_frame: plan_my_cabinet::measurements::Frame,
) -> ViewportInteraction {
    show_move_with_hardware(
        ui,
        camera,
        project,
        selection,
        tool,
        modal,
        language,
        inverse_scroll_zoom,
        material_tint,
        faces,
        poses,
        measurement_scope,
        measurement_frame,
        None,
        false,
    )
}

/// The installation target is a typed inspector ID, independent of board scene selection.
/// `poses` is the disposable door display transform already used by the scene renderer.
#[allow(clippy::too_many_arguments)]
pub fn show_move_with_hardware(
    ui: &mut egui::Ui,
    camera: &mut Camera,
    project: &Project,
    selection: &mut Selection,
    tool: &mut MoveTool,
    modal: bool,
    language: plan_my_cabinet::i18n::Language,
    inverse_scroll_zoom: bool,
    material_tint: bool,
    faces: Option<(Uuid, BoardFace, Uuid, BoardFace)>,
    poses: Option<&HashMap<Uuid, plan_my_cabinet::units::Pose>>,
    measurement_scope: plan_my_cabinet::measurements::Scope,
    measurement_frame: plan_my_cabinet::measurements::Frame,
    installation_id: Option<Uuid>,
    hardware_workspace: bool,
) -> ViewportInteraction {
    use plan_my_cabinet::i18n::Language;
    let pt = language == Language::PtBr;
    let overlay_blocked = if hardware_workspace {
        controls::hardware_overlay(
            ui,
            camera,
            project,
            selection,
            tool,
            modal,
            pt,
            poses.is_some(),
        )
    } else {
        controls::show(
            ui,
            camera,
            project,
            selection,
            tool,
            modal,
            pt,
            poses.is_some(),
        )
    };
    let (rect, interaction) = canvas::interact_with_selection(
        ui,
        camera,
        project,
        selection,
        tool,
        modal || overlay_blocked,
        inverse_scroll_zoom,
        poses.is_some(),
    );
    tool.prepare_capture_snap(project, camera, rect, selection);
    scene_render::paint(
        ui,
        project,
        camera,
        selection,
        tool,
        faces,
        poses,
        material_tint,
        rect,
    );
    if !modal && !overlay_blocked && !hardware_workspace {
        let handles = resize::handles(project, camera, rect, selection, tool);
        resize::paint(ui, tool, &handles);
    }
    annotations::paint(
        ui,
        camera,
        project,
        selection,
        tool,
        rect,
        pt,
        measurement_scope,
        measurement_frame,
    );
    hardware_annotations::paint(
        ui,
        camera,
        project,
        selection,
        rect,
        poses,
        installation_id,
        pt,
    );
    interaction
}

#[cfg(test)]
mod tests;
