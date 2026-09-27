//! Project-driven, depth-tested 3D viewport. Camera state is presentation-only.
use eframe::egui;
use plan_my_cabinet::domain::{Board, HardwareKind, Project};
use plan_my_cabinet::placement::{BoardFace, Side};
use sha2::Digest;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

mod measure_ui;
mod thumbnail;
pub(crate) use measure_ui::measurement_readout;
pub(crate) use thumbnail::saved_thumbnail;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projection {
    Perspective,
    Orthographic,
}

/// Transient editor selection; IDs refer directly to project objects.
#[derive(Default)]
pub struct Selection {
    pub ids: HashSet<Uuid>,
    pub active: Option<Uuid>,
    /// Session-only scene exclusions; descendants inherit an assembly's hiding.
    pub hidden: HashSet<Uuid>,
}

impl Selection {
    pub fn visible(&self, project: &Project, id: Uuid) -> bool {
        let mut current = Some(id);
        let mut seen = HashSet::new();
        while let Some(object) = current {
            if !seen.insert(object) || self.hidden.contains(&object) {
                return false;
            }
            current = project
                .assemblies
                .iter()
                .find(|a| a.id == object)
                .and_then(|a| a.parent_id)
                .or_else(|| {
                    project
                        .boards
                        .iter()
                        .find(|b| b.id == object)
                        .and_then(|b| b.parent_id)
                })
                .or_else(|| {
                    project
                        .hardware
                        .iter()
                        .find(|h| h.id == object)
                        .and_then(|h| h.parent_id)
                });
        }
        true
    }

    pub fn reveal(&mut self, project: &Project, id: Uuid) {
        let mut current = Some(id);
        while let Some(object) = current {
            self.hidden.remove(&object);
            current = project
                .assemblies
                .iter()
                .find(|a| a.id == object)
                .and_then(|a| a.parent_id)
                .or_else(|| {
                    project
                        .boards
                        .iter()
                        .find(|b| b.id == object)
                        .and_then(|b| b.parent_id)
                })
                .or_else(|| {
                    project
                        .hardware
                        .iter()
                        .find(|h| h.id == object)
                        .and_then(|h| h.parent_id)
                });
        }
    }

    pub fn choose(&mut self, id: Option<Uuid>, additive: bool) {
        if !additive {
            self.ids.clear();
        }
        match id {
            Some(id) if additive && self.ids.contains(&id) => {
                self.ids.remove(&id);
                self.active = self.ids.iter().copied().min();
            }
            Some(id) => {
                self.ids.insert(id);
                self.active = Some(id);
            }
            None if !additive => self.active = None,
            None => {}
        }
    }

    pub fn retain_objects(&mut self, project: &Project) {
        self.hidden.retain(|id| {
            project.boards.iter().any(|b| b.id == *id)
                || project.assemblies.iter().any(|a| a.id == *id)
                || project.hardware.iter().any(|h| h.id == *id)
        });
        self.ids.retain(|id| {
            project.boards.iter().any(|b| b.id == *id)
                || project.assemblies.iter().any(|a| a.id == *id)
                || project.hardware.iter().any(|h| h.id == *id)
        });
        if self.active.is_some_and(|id| !self.ids.contains(&id)) {
            self.active = self.ids.iter().copied().min();
        }
    }
}

#[derive(Clone, Copy)]
struct Ray {
    origin: [f64; 3],
    direction: [f64; 3],
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}

fn add_scaled(a: [f64; 3], b: [f64; 3], scale: f64) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i] * scale)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Preset {
    Isometric,
    Front,
    Right,
    Top,
    Free,
}

pub struct Camera {
    target: [f64; 3],
    yaw: f64,
    pitch: f64,
    distance: f64,
    projection: Projection,
    preset: Preset,
    viewport_aspect: f64,
    viewport_height: f64,
    pending_frame: bool,
}

pub struct MoveTool {
    pub mode: ToolMode,
    pub face_snap: bool,
    pub grid_snap: bool,
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

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: [0.0; 3],
            yaw: -std::f64::consts::FRAC_PI_4,
            pitch: (1.0_f64 / 3.0_f64.sqrt()).asin(),
            distance: 1500.0,
            projection: Projection::Perspective,
            preset: Preset::Isometric,
            viewport_aspect: 1.0,
            pending_frame: false,
            viewport_height: 600.0,
        }
    }
}

const FOV: f64 = std::f64::consts::FRAC_PI_4;
const MIN_DISTANCE: f64 = 0.01;
const MAX_DISTANCE: f64 = 1.0e9;

impl Camera {
    /// Defer a new-project fit until the target workspace has a real canvas.
    pub(crate) fn request_frame(&mut self) {
        self.pending_frame = true;
    }

    #[cfg(test)]
    pub(crate) fn framed_target(&self) -> Option<[f64; 3]> {
        (!self.pending_frame).then_some(self.target)
    }

    #[cfg(test)]
    pub(crate) fn navigation_state(&self) -> ([f64; 3], f64, f64, f64) {
        (self.target, self.distance, self.yaw, self.pitch)
    }

    #[cfg(test)]
    pub(crate) fn assert_framed_occupancy(&self, project: &Project, selection: &Selection) {
        let rect = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(
                (self.viewport_height * self.viewport_aspect) as f32,
                self.viewport_height as f32,
            ),
        );
        let mut projected = egui::Rect::NOTHING;
        for board in &project.boards {
            if selection.visible(project, board.id)
                && selected_board(project, &selection.ids, board.id)
            {
                for corner in board_corners(project, board).unwrap() {
                    let point = self.project(corner, rect).unwrap();
                    assert!(rect.contains(point), "{point:?} outside {rect:?}");
                    projected.extend_with(point);
                }
            }
        }
        let occupancy = projected.height() / rect.height().min(rect.width());
        assert!(
            (0.3..0.95).contains(&occupancy),
            "postage stamp or clipped fit: {occupancy}"
        );
        assert!(projected.center().distance(rect.center()) < rect.width().min(rect.height()) * 0.1);
    }
    /// Stable presentation-only camera for native baseline captures.
    pub fn reference_baseline() -> Self {
        Self {
            target: [400.0, 280.0, 360.0],
            yaw: -std::f64::consts::FRAC_PI_4,
            pitch: (1.0_f64 / 3.0_f64.sqrt()).asin(),
            distance: 1800.0,
            projection: Projection::Orthographic,
            ..Self::default()
        }
    }

    fn project(&self, point: [f64; 3], rect: egui::Rect) -> Option<egui::Pos2> {
        let (right, up, forward) = self.basis();
        let offset = std::array::from_fn(|i| point[i] - self.target[i]);
        let depth = self.distance + dot(offset, forward);
        if depth <= 0.0 {
            return None;
        }
        let scale = match self.projection {
            Projection::Perspective => depth,
            Projection::Orthographic => self.distance,
        } * (FOV / 2.0).tan();
        Some(egui::pos2(
            rect.center().x + (dot(offset, right) / scale * rect.height() as f64 / 2.0) as f32,
            rect.center().y - (dot(offset, up) / scale * rect.height() as f64 / 2.0) as f32,
        ))
    }

    fn drag_pose(
        &self,
        start: egui::Pos2,
        pointer: egui::Pos2,
        rect: egui::Rect,
        world: plan_my_cabinet::units::Pose,
    ) -> Option<plan_my_cabinet::units::Pose> {
        let (_, _, normal) = self.basis();
        let intersect = |position| {
            let ray = self.ray(position, rect);
            let t = dot(
                std::array::from_fn(|i| world.translation_mm[i] - ray.origin[i]),
                normal,
            ) / dot(ray.direction, normal);
            add_scaled(ray.origin, ray.direction, t)
        };
        let a = intersect(start);
        let b = intersect(pointer);
        let translation = std::array::from_fn(|i| {
            world.translation_mm[i] + ((b[i] - a[i]) * 1000.0).round() / 1000.0
        });
        plan_my_cabinet::units::Pose::new(translation, world.rotation).ok()
    }
    fn ray(&self, pointer: egui::Pos2, rect: egui::Rect) -> Ray {
        let (right, up, forward) = self.basis();
        let x = (2.0 * (pointer.x - rect.left()) as f64 / rect.width() as f64 - 1.0)
            * self.viewport_aspect
            * (FOV / 2.0).tan();
        let y = (1.0 - 2.0 * (pointer.y - rect.top()) as f64 / rect.height() as f64)
            * (FOV / 2.0).tan();
        let eye = add_scaled(self.target, forward, -self.distance);
        match self.projection {
            Projection::Perspective => {
                let raw = add_scaled(add_scaled(forward, right, x), up, y);
                let length = dot(raw, raw).sqrt();
                Ray {
                    origin: eye,
                    direction: raw.map(|v| v / length),
                }
            }
            Projection::Orthographic => Ray {
                origin: add_scaled(
                    add_scaled(eye, right, x * self.distance),
                    up,
                    y * self.distance,
                ),
                direction: forward,
            },
        }
    }

    fn basis(&self) -> ([f64; 3], [f64; 3], [f64; 3]) {
        let (s, c) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let forward = [-cp * c, -cp * s, -sp];
        let right = [-s, c, 0.0];
        let up = [-sp * c, -sp * s, cp];
        (right, up, forward)
    }

    fn orbit(&mut self, dx: f64, dy: f64) {
        self.yaw = (self.yaw - dx * 0.006).rem_euclid(std::f64::consts::TAU);
        self.pitch = (self.pitch + dy * 0.006).clamp(-1.55, 1.55);
        self.preset = Preset::Free;
    }

    fn pan(&mut self, dx: f64, dy: f64, height: f64) {
        let (right, up, _) = self.basis();
        let scale = 2.0 * self.distance * (FOV / 2.0).tan() / height.max(1.0);
        for i in 0..3 {
            self.target[i] += (-dx * right[i] + dy * up[i]) * scale;
        }
    }

    fn zoom(&mut self, factor: f64) {
        if factor.is_finite() && factor > 0.0 {
            self.distance = (self.distance / factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
        }
    }

    fn set_preset(&mut self, preset: Preset) {
        self.preset = preset;
        (self.yaw, self.pitch) = match preset {
            Preset::Isometric => (
                -std::f64::consts::FRAC_PI_4,
                (1.0_f64 / 3.0_f64.sqrt()).asin(),
            ),
            Preset::Front => (-std::f64::consts::FRAC_PI_2, 0.0),
            Preset::Right => (0.0, 0.0),
            Preset::Top => (-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2),
            Preset::Free => (self.yaw, self.pitch),
        };
    }

    fn frame(&mut self, bounds: Bounds, aspect: f64) {
        self.target = bounds.center();
        let radius = bounds.radius().max(10.0);
        // A sphere enclosing every corner fits at every preset/orbit angle.
        let half_angle = ((FOV / 2.0).tan() * aspect.clamp(0.01, 1.0)).atan();
        self.distance = (radius * 1.3
            / match self.projection {
                Projection::Perspective => half_angle.sin(),
                Projection::Orthographic => half_angle.tan(),
            })
        .clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    fn uniform(&self, size: [u32; 2], radius: f64) -> Vec<u8> {
        let aspect = (size[0] as f64 / size[1].max(1) as f64).max(0.01);
        let (right, up, forward) = self.basis();
        let eye = forward.map(|v| -v * self.distance);
        // Keep a useful depth range even when zoomed far beyond the scene. A
        // nearly equal f32 near/far pair loses all depth precision on Metal.
        let near = (self.distance - radius * 2.0)
            .max((self.distance * 0.01).max(0.001))
            .min(self.distance * 0.5);
        let far = (self.distance + radius * 2.0 + 1000.0).max(self.distance * 1.5);
        let scale = match self.projection {
            Projection::Perspective => 1.0 / (FOV / 2.0).tan(),
            Projection::Orthographic => 1.0 / (self.distance * (FOV / 2.0).tan()),
        };
        let mut data = Vec::with_capacity(96);
        for v in [
            [right[0], right[1], right[2], 0.0],
            [up[0], up[1], up[2], 0.0],
            [forward[0], forward[1], forward[2], 0.0],
            [eye[0], eye[1], eye[2], 0.0],
            [scale / aspect, scale, near, far],
            [
                f64::from(self.projection == Projection::Perspective),
                0.0,
                0.0,
                0.0,
            ],
        ] {
            for component in v {
                data.extend_from_slice(&(component as f32).to_ne_bytes());
            }
        }
        data
    }
}

#[derive(Clone, Copy)]
struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }
    fn include(&mut self, p: [f64; 3]) {
        for (i, value) in p.into_iter().enumerate() {
            self.min[i] = self.min[i].min(value);
            self.max[i] = self.max[i].max(value);
        }
    }
    fn valid(&self) -> bool {
        self.min[0].is_finite()
    }
    fn center(&self) -> [f64; 3] {
        std::array::from_fn(|i| (self.min[i] + self.max[i]) / 2.0)
    }
    fn radius(&self) -> f64 {
        self.min
            .iter()
            .zip(self.max)
            .map(|(a, b)| ((b - a) / 2.0).powi(2))
            .sum::<f64>()
            .sqrt()
    }
}

fn world_pose(project: &Project, board: &Board) -> Option<plan_my_cabinet::units::Pose> {
    let mut pose = board.pose;
    let mut parent = board.parent_id;
    let mut visited = HashSet::new();
    while let Some(id) = parent {
        if !visited.insert(id) {
            return None;
        }
        let assembly = project.assemblies.iter().find(|a| a.id == id)?;
        pose = assembly.pose.compose(pose).ok()?;
        parent = assembly.parent_id;
    }
    Some(pose)
}

fn board_corners(project: &Project, board: &Board) -> Option<[[f64; 3]; 8]> {
    let pose = world_pose(project, board)?;
    let dimensions = board
        .blank_dimensions()
        .map(|v| v.micrometres() as f64 / 1000.0);
    box_corners(pose, dimensions)
}

fn box_corners(pose: plan_my_cabinet::units::Pose, [x, y, z]: [f64; 3]) -> Option<[[f64; 3]; 8]> {
    let mut corners = [[0.0; 3]; 8];
    for (i, corner) in corners.iter_mut().enumerate() {
        *corner = pose
            .transform_point([
                if i & 1 == 0 { 0.0 } else { x },
                if i & 2 == 0 { 0.0 } else { y },
                if i & 4 == 0 { 0.0 } else { z },
            ])
            .ok()?;
    }
    Some(corners)
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
            let HardwareKind::Placeholder { dimensions } = h.kind else {
                return None;
            };
            let pose = plan_my_cabinet::assembly_edit::world_pose(project, h.id).ok()?;
            let (distance, _) = board_hit(
                ray,
                pose,
                dimensions.map(|d| d.micrometres() as f64 / 1000.0),
            )?;
            Some((distance, h.id))
        });
    boards
        .chain(hardware)
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

fn bounds_visible(
    project: &Project,
    selected: &HashSet<Uuid>,
    visibility: &Selection,
) -> Option<Bounds> {
    let mut result = Bounds::empty();
    for board in &project.boards {
        if !visibility.visible(project, board.id) {
            continue;
        }
        if !selected.is_empty() && !selected_board(project, selected, board.id) {
            continue;
        }
        if let Some(corners) = board_corners(project, board) {
            for p in corners {
                result.include(p);
            }
        }
    }
    for hardware in &project.hardware {
        let HardwareKind::Placeholder { dimensions } = hardware.kind else {
            continue;
        };
        if !visibility.visible(project, hardware.id)
            || (!selected.is_empty()
                && !selected.contains(&hardware.id)
                && !selected_ancestor(project, selected, hardware.parent_id))
        {
            continue;
        }
        if let Ok(pose) = plan_my_cabinet::assembly_edit::world_pose(project, hardware.id)
            && let Some(world) =
                box_corners(pose, dimensions.map(|d| d.micrometres() as f64 / 1000.0))
        {
            for p in world {
                result.include(p);
            }
        }
    }
    result.valid().then_some(result)
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

fn selected_board(project: &Project, selected: &HashSet<Uuid>, board_id: Uuid) -> bool {
    if selected.contains(&board_id) {
        return true;
    }
    let parent = project
        .boards
        .iter()
        .find(|b| b.id == board_id)
        .and_then(|b| b.parent_id);
    selected_ancestor(project, selected, parent)
}

fn selected_ancestor(
    project: &Project,
    selected: &HashSet<Uuid>,
    mut parent: Option<Uuid>,
) -> bool {
    let mut seen = HashSet::new();
    while let Some(id) = parent {
        if !seen.insert(id) {
            break;
        }
        if selected.contains(&id) {
            return true;
        }
        parent = project
            .assemblies
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.parent_id);
    }
    false
}

mod annotations;
mod canvas;
mod controls;
mod hardware_annotations;
mod scene_render;
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
mod tests {
    use super::*;
    use plan_my_cabinet::commands::ProjectEditor;
    use plan_my_cabinet::domain::{Assembly, Board, BoardGrain, Material};
    use plan_my_cabinet::i18n::Language;
    use plan_my_cabinet::measurements::{Frame, Scope};
    use plan_my_cabinet::money::Currency;
    use plan_my_cabinet::units::{Length, Pose, Quaternion};

    #[test]
    fn hardware_camera_overlay_preserves_canvas_height_and_blocks_pointer_pick() {
        for pt in [false, true] {
            let ctx = egui::Context::default();
            let project = plan_my_cabinet::reference_fixture::project();
            let mut camera = Camera::reference_baseline();
            let mut selection = Selection::default();
            let mut tool = MoveTool::default();
            let mut point = egui::Pos2::ZERO;
            for frame in 0..5 {
                let events = if frame < 3 {
                    vec![]
                } else {
                    vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed: frame == 3,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                };
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(750.0, 800.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let before = ui.available_rect_before_wrap();
                        let blocked = controls::hardware_overlay(
                            ui,
                            &mut camera,
                            &project,
                            &selection,
                            &mut tool,
                            false,
                            pt,
                            false,
                        );
                        let (rect, action) = canvas::interact_with_selection(
                            ui,
                            &mut camera,
                            &project,
                            &mut selection,
                            &mut tool,
                            blocked,
                            false,
                            false,
                        );
                        assert!((rect.height() - before.height()).abs() < 1.0);
                        assert!(
                            action.selection.is_none(),
                            "camera overlay must not pick the scene"
                        );
                    },
                );
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape
                        && text.galley.text() == if pt { "Frontal" } else { "Front" }
                    {
                        point = text.pos + text.galley.size() * 0.5;
                    }
                }
                output.drop_without_applying_deltas();
            }
            assert_ne!(point, egui::Pos2::ZERO);
            assert_eq!(camera.preset, Preset::Front);
        }
    }

    #[test]
    fn measure_tool_switches_are_guarded_read_only_and_keep_scope_and_frame() {
        use crate::actions::{self, ActionId as A, Request, Unavailable};
        let editor = ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap();
        let original = editor.project().clone();
        let revision = original.revision;
        let undo = editor.can_undo();
        let root = editor.project().assemblies[0].id;
        let mut selection = Selection::default();
        selection.choose(Some(root), false);
        let scope = Scope::Overall;
        let frame = Frame::Object(root);
        let mut camera = Camera::default();
        let mut tool = MoveTool::default();
        for (action, expected) in [
            (A::ViewMeasure, ToolMode::Measure),
            (A::ViewNavigate, ToolMode::Navigate),
            (A::ViewMove, ToolMode::Move),
            (A::ViewMeasure, ToolMode::Measure),
        ] {
            actions::viewport_control(
                Request::new(action),
                &mut camera,
                &mut tool,
                editor.project(),
                &selection,
                false,
                false,
            )
            .unwrap();
            assert_eq!(tool.mode, expected);
            assert_eq!((scope, frame), (Scope::Overall, Frame::Object(root)));
        }
        let localizer = plan_my_cabinet::i18n::Localizer::new(Language::En);
        let (label, value) =
            measurement_readout(editor.project(), &selection, scope, frame, &localizer);
        assert!(label.contains("Overall") && label.contains(&root.to_string()[..8]));
        assert!(value.contains("X × Y × Z") && value.contains("mm"));
        for (modal, preview) in [(true, false), (false, true)] {
            assert_eq!(
                actions::viewport_control(
                    Request::new(A::ViewNavigate),
                    &mut camera,
                    &mut tool,
                    editor.project(),
                    &selection,
                    modal,
                    preview,
                ),
                Err(if modal {
                    Unavailable::ModalOpen
                } else {
                    Unavailable::Busy
                })
            );
            assert_eq!(tool.mode, ToolMode::Measure);
        }
        assert_eq!(editor.project(), &original);
        assert_eq!(editor.project().revision, revision);
        assert_eq!(editor.can_undo(), undo);
    }

    #[test]
    fn measurement_readout_uses_nested_rotated_bounds_and_discloses_missing_hardware() {
        use plan_my_cabinet::domain::{Hardware, HardwareKind};
        let mut project = Project::new("Measure", Currency::Brl);
        let root = Uuid::new_v4();
        let nested = Uuid::new_v4();
        let panel = Uuid::new_v4();
        let foot = Uuid::new_v4();
        let turn = Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap();
        project.assemblies.push(Assembly {
            id: root,
            name: "Root".into(),
            parent_id: None,
            pose: Pose::new([100.0, 0.0, 0.0], turn).unwrap(),
        });
        project.assemblies.push(Assembly {
            id: nested,
            name: "Nested".into(),
            parent_id: Some(root),
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        });
        let mut part = board(panel, [0.0, 0.0, 100.0], Some(nested));
        part.length = Length::from_micrometres(100_000);
        part.width = Length::from_micrometres(50_000);
        let material_id = part.material_id;
        project.materials.push(Material {
            id: material_id,
            name: "Panel".into(),
            default_thickness: part.thickness,
            default_grain: BoardGrain::Unrestricted,
        });
        project.boards.push(part);
        project.hardware.push(Hardware {
            id: foot,
            name: "Foot".into(),
            parent_id: Some(root),
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            kind: HardwareKind::Placeholder {
                dimensions: [
                    Length::from_micrometres(10_000),
                    Length::from_micrometres(10_000),
                    Length::from_micrometres(20_000),
                ],
            },
        });
        let mut selection = Selection::default();
        selection.choose(Some(root), false);
        selection.choose(Some(panel), true); // Explicit descendant must not be counted twice.
        selection.hidden.insert(nested); // Hiding must not change measurement.
        let localizer = plan_my_cabinet::i18n::Localizer::new(Language::En);
        let original = project.clone();
        let (_, body) = measurement_readout(
            &project,
            &selection,
            Scope::Body,
            Frame::Object(root),
            &localizer,
        );
        let (_, overall) = measurement_readout(
            &project,
            &selection,
            Scope::Overall,
            Frame::Object(root),
            &localizer,
        );
        assert!(body.contains("100.000 × 50.000 × 20.000 mm"), "{body}");
        assert!(
            overall.contains("100.000 × 50.000 × 120.000 mm"),
            "{overall}"
        );
        let (_, world) =
            measurement_readout(&project, &selection, Scope::Body, Frame::World, &localizer);
        assert!(world.contains("50.000 × 100.000 × 20.000 mm"), "{world}");
        project.hardware[0].kind = HardwareKind::Catalog {
            catalog_id: Uuid::new_v4(),
        };
        let (heading, unknown) = measurement_readout(
            &project,
            &selection,
            Scope::Overall,
            Frame::Object(root),
            &localizer,
        );
        assert!(heading.contains("Overall") && heading.contains("Root"));
        assert!(unknown.contains("unavailable") && unknown.contains("hardware"));
        let (_, body_again) = measurement_readout(
            &project,
            &selection,
            Scope::Body,
            Frame::Object(root),
            &localizer,
        );
        assert_eq!(body, body_again);
        project.display_unit = plan_my_cabinet::units::Unit::Inch;
        let pt = plan_my_cabinet::i18n::Localizer::new(Language::PtBr);
        let (label, imperial) =
            measurement_readout(&project, &selection, Scope::Body, Frame::Object(root), &pt);
        assert!(label.contains("Referencial"));
        assert!(imperial.contains("in") && imperial.contains(','));
        assert_eq!(original.boards[0].pose, project.boards[0].pose);
    }

    #[test]
    fn capture_snap_uses_real_candidates_and_never_edits_the_fixture() {
        let editor = ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap();
        let before = editor.project().clone();
        let revision = editor.project().revision;
        let undo = editor.can_undo();
        let camera = Camera::reference_baseline();
        let rect = egui::Rect::from_min_size(egui::pos2(330.0, 110.0), egui::vec2(740.0, 380.0));
        for mode in [CaptureSnap::Face, CaptureSnap::Grid] {
            let mut tool = MoveTool::default();
            let mut selection = Selection::default();
            selection
                .hidden
                .insert(plan_my_cabinet::reference_fixture::DOORS_ID);
            tool.configure_capture_snap(mode);
            tool.prepare_capture_snap(editor.project(), &camera, rect, &mut selection);
            let evidence = tool.capture_evidence().unwrap().unwrap();
            let drag = tool.drag.as_ref().unwrap();
            assert_eq!(evidence["candidate"], mode.as_str());
            assert_eq!(evidence["face_precedes_grid_when_both"], true);
            assert_eq!(evidence["grid_when_face_disabled"], true);
            assert_eq!(evidence["alt_bypass"], true);
            assert_ne!(evidence["source_id"], evidence["target_id"]);
            assert_eq!(selection.active, Some(drag.board_id));
            let (mesh, _) = scene_with_faces(
                editor.project(),
                &camera,
                &selection,
                scene_render::highlighted_faces(&tool, None),
                Some(&HashMap::from([(drag.board_id, drag.last_pose.unwrap())])),
                true,
            );
            if mode == CaptureSnap::Face {
                assert!(matches!(drag.snap, Some(DragSnap::Face(_))));
                assert!(
                    mesh.lines
                        .chunks_exact(6)
                        .any(|v| v[3..] == [0.15, 0.95, 0.95])
                );
                assert!(
                    mesh.lines
                        .chunks_exact(6)
                        .any(|v| v[3..] == [1.0, 0.25, 0.8])
                );
            } else {
                assert!(matches!(drag.snap, Some(DragSnap::Grid)));
                assert!(scene_render::highlighted_faces(&tool, None).is_none());
                assert!(
                    camera
                        .project(
                            [
                                drag.last_pose.unwrap().translation_mm[0],
                                drag.last_pose.unwrap().translation_mm[1],
                                0.0
                            ],
                            rect
                        )
                        .is_some_and(|p| rect.contains(p))
                );
            }
        }
        assert_eq!(editor.project(), &before);
        assert_eq!(editor.project().revision, revision);
        assert_eq!(editor.can_undo(), undo);
    }

    #[test]
    fn tint_is_only_face_presentation_and_hidden_geometry_casts_no_shadow() {
        use plan_my_cabinet::domain::SrgbColor;
        let mut project = Project::new("Tint", Currency::Brl);
        let board_id = Uuid::new_v4();
        let part = board(board_id, [0.0; 3], None);
        let material_id = part.material_id;
        project.materials.push(Material {
            id: material_id,
            name: "Veneer".into(),
            default_thickness: part.thickness,
            default_grain: BoardGrain::Unrestricted,
        });
        project.boards.push(part);
        project
            .material_colors
            .insert(material_id, SrgbColor([185, 125, 83]));
        let before = project.clone();
        let fingerprint = plan_my_cabinet::export::fingerprint(&project);
        let camera = Camera::default();
        let selection = Selection::default();
        let (tinted, _) = scene_with_faces(&project, &camera, &selection, None, None, true);
        let (neutral, _) = scene_with_faces(&project, &camera, &selection, None, None, false);
        assert_ne!(tinted.faces, neutral.faces);
        assert_eq!(tinted.lines, neutral.lines);
        assert_eq!(tinted.shadow, neutral.shadow);
        assert!(!tinted.shadow.is_empty());
        project.material_colors.remove(&material_id);
        let (fallback, _) = scene_with_faces(&project, &camera, &selection, None, None, true);
        assert_eq!(neutral.faces, fallback.faces);
        project
            .material_colors
            .insert(material_id, SrgbColor([185, 125, 83]));
        assert_eq!(
            project, before,
            "rendering and preference choice never edit the model"
        );
        assert_eq!(plan_my_cabinet::export::fingerprint(&project), fingerprint);
        let mut hidden = Selection::default();
        hidden.hidden.insert(board_id);
        let (mesh, _) = scene_with_faces(&project, &camera, &hidden, None, None, true);
        assert!(mesh.faces.is_empty());
        assert!(mesh.shadow.is_empty());
        assert_eq!(
            mesh.lines,
            scene_with_faces(&project, &camera, &Selection::default(), None, None, true)
                .0
                .lines[..mesh.lines.len()]
        );
    }

    #[test]
    fn active_warm_face_and_secondary_edges_keep_material_identity() {
        let mut project = Project::new("Edges", Currency::Brl);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        project.boards.push(board(a, [0.0; 3], None));
        project.boards.push(board(b, [200.0, 0.0, 0.0], None));
        let camera = Camera::default();
        let mut selection = Selection::default();
        let (plain, _) = scene(&project, &camera, &selection);
        selection.choose(Some(a), false);
        selection.choose(Some(b), true);
        let (selected, _) = scene(&project, &camera, &selection);
        // Each box has 36 vertices of six floats. Only the active board's
        // faces receive the presentation-only amber mix; secondary selection
        // keeps its material fill and cyan edge.
        let first_box = 36 * 6;
        assert_eq!(&plain.faces[..first_box], &selected.faces[..first_box]);
        assert_ne!(&plain.faces[first_box..], &selected.faces[first_box..]);
        assert_ne!(plain.lines, selected.lines);
        assert_eq!(highlight_color(&project, a, &selection), [0.16, 0.59, 0.67]);
        assert_eq!(highlight_color(&project, b, &selection), [0.79, 0.45, 0.12]);
    }

    #[test]
    fn scene_override_moves_only_target_mesh_and_exit_recovers_identical_closed_mesh() {
        let mut project = Project::new("preview", Currency::Brl);
        let moving = Uuid::new_v4();
        let fixed = Uuid::new_v4();
        project.boards.push(board(moving, [20.0, 0.0, 0.0], None));
        project.boards.push(board(fixed, [200.0, 0.0, 0.0], None));
        let camera = Camera::default();
        let selection = Selection::default();
        let (closed, _) = scene_with_faces(&project, &camera, &selection, None, None, true);
        let poses = HashMap::from([(
            moving,
            Pose::new([50.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
        )]);
        let (open, _) = scene_with_faces(&project, &camera, &selection, None, Some(&poses), true);
        // Grid and axes precede the two board boxes; only the first box changes.
        let board_floats = 36 * 6;
        assert_ne!(open.faces[..board_floats], closed.faces[..board_floats]);
        assert_eq!(open.faces[board_floats..], closed.faces[board_floats..]);
        assert_eq!(
            scene_with_faces(&project, &camera, &selection, None, None, true)
                .0
                .faces,
            closed.faces
        );
        assert_eq!(project.boards[0].pose.translation_mm, [20.0, 0.0, 0.0]);
    }

    #[test]
    fn preview_blocks_viewport_selection_and_move_but_keeps_orbit_available() {
        let ctx = egui::Context::default();
        let id = Uuid::new_v4();
        let mut project = Project::new("preview", Currency::Brl);
        project.boards.push(board(id, [0.0; 3], None));
        let mut camera = Camera::default();
        let mut selection = Selection::default();
        let mut tool = MoveTool {
            mode: ToolMode::Move,
            ..Default::default()
        };
        let poses = HashMap::from([(id, Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap())]);
        let mut frame = |events| {
            let mut action = ViewportInteraction::default();
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        action = show_move(
                            ui,
                            &mut camera,
                            &project,
                            &mut selection,
                            &mut tool,
                            false,
                            Language::En,
                            false,
                            true,
                            None,
                            Some(&poses),
                            plan_my_cabinet::measurements::Scope::Body,
                            plan_my_cabinet::measurements::Frame::World,
                        );
                    });
                },
            )
            .drop_without_applying_deltas();
            assert!(action.drag.is_none());
            assert!(action.selection.is_none());
        };
        let pos = egui::pos2(400.0, 330.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![]);
        frame(vec![button(pos, true)]);
        frame(vec![egui::Event::PointerMoved(egui::pos2(450.0, 330.0))]);
        frame(vec![button(egui::pos2(450.0, 330.0), false)]);
        assert!(selection.ids.is_empty());
        assert!(tool.drag.is_none());
        assert_ne!(camera.yaw, Camera::default().yaw);
    }

    #[test]
    fn canvas_proposes_picks_and_background_clicks_without_changing_selection() {
        let ctx = egui::Context::default();
        let id = Uuid::from_u128(42);
        let mut project = Project::new("Click proposals", Currency::Brl);
        project.boards.push(board(id, [0.0; 3], None));
        let mut camera = Camera::default();
        camera.set_preset(Preset::Top);
        camera.target = [50.0, 50.0, 10.0];
        camera.distance = 400.0;
        let mut selection = Selection::default();
        let existing = Uuid::from_u128(43);
        selection.choose(Some(existing), false);
        let mut tool = MoveTool::default();
        #[allow(clippy::too_many_arguments)] // Exercises each independent viewport input explicitly.
        fn frame(
            ctx: &egui::Context,
            camera: &mut Camera,
            project: &Project,
            selection: &mut Selection,
            tool: &mut MoveTool,
            events: Vec<egui::Event>,
            modifiers: egui::Modifiers,
            modal: bool,
            preview_active: bool,
        ) -> (egui::Rect, ViewportInteraction) {
            let mut rect = egui::Rect::NOTHING;
            let mut result = ViewportInteraction::default();
            let mut events = events;
            events.insert(0, egui::Event::ModifiersChanged(modifiers));
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        (rect, result) = canvas::interact_with_selection(
                            ui,
                            camera,
                            project,
                            selection,
                            tool,
                            modal,
                            false,
                            preview_active,
                        );
                    });
                },
            )
            .drop_without_applying_deltas();
            (rect, result)
        }
        macro_rules! draw {
            ($events:expr, $modifiers:expr, $modal:expr, $preview:expr) => {
                frame(
                    &ctx,
                    &mut camera,
                    &project,
                    &mut selection,
                    &mut tool,
                    $events,
                    $modifiers,
                    $modal,
                    $preview,
                )
            };
        }
        let (rect, _) = draw!(vec![], egui::Modifiers::NONE, false, false);
        let center = rect.center();
        let background = rect.left_top() + egui::vec2(8.0, 8.0);
        let click = |pos, pressed, modifiers| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        };
        draw!(
            vec![click(center, true, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
            false,
            false
        );
        let (_, result) = draw!(
            vec![click(center, false, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
            false,
            false
        );
        assert_eq!(
            result.selection,
            Some(SelectionProposal {
                picked: Some(id),
                additive: false
            })
        );
        assert!(result.drag.is_none());
        assert_eq!(selection.active, Some(existing));
        assert_eq!(selection.ids, HashSet::from([existing]));

        let shift = egui::Modifiers {
            shift: true,
            ..Default::default()
        };
        draw!(vec![click(center, true, shift)], shift, false, false);
        let (_, result) = draw!(vec![click(center, false, shift)], shift, false, false);
        assert_eq!(
            result.selection,
            Some(SelectionProposal {
                picked: Some(id),
                additive: true
            })
        );
        draw!(
            vec![click(background, true, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
            false,
            false
        );
        let (_, result) = draw!(
            vec![click(background, false, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
            false,
            false
        );
        assert_eq!(
            result.selection,
            Some(SelectionProposal {
                picked: None,
                additive: false
            })
        );
        assert_eq!(selection.ids, HashSet::from([existing]));
        assert_eq!(selection.active, Some(existing));

        draw!(
            vec![click(center, true, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
            true,
            false
        );
        let (_, result) = draw!(
            vec![click(center, false, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
            true,
            false
        );
        assert!(result.selection.is_none());
        draw!(
            vec![click(center, true, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
            false,
            true
        );
        let (_, result) = draw!(
            vec![click(center, false, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
            false,
            true
        );
        assert!(result.selection.is_none());
    }

    #[test]
    fn rectangular_board_faces_have_full_area_and_edges_follow_box_axes() {
        let corners = std::array::from_fn(|i| {
            [
                if i & 1 != 0 { 100.0 } else { 0.0 },
                if i & 2 != 0 { 50.0 } else { 0.0 },
                if i & 4 != 0 { 18.0 } else { 0.0 },
            ]
        });
        let mut mesh = Mesh::default();
        mesh.box_mesh(corners, [1.0; 3], [0.5; 3]);
        let triangle_area = |vertices: &[f32]| {
            let a = [vertices[0], vertices[1], vertices[2]];
            let b = [vertices[6], vertices[7], vertices[8]];
            let c = [vertices[12], vertices[13], vertices[14]];
            let u = std::array::from_fn::<_, 3, _>(|i| b[i] - a[i]);
            let v = std::array::from_fn::<_, 3, _>(|i| c[i] - a[i]);
            let cross = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            cross.into_iter().map(|n| n * n).sum::<f32>().sqrt() * 0.5
        };
        let areas: Vec<_> = mesh
            .faces
            .chunks_exact(36)
            .map(|face| triangle_area(&face[..18]) + triangle_area(&face[18..]))
            .collect();
        assert_eq!(areas, [5000.0, 5000.0, 1800.0, 900.0, 1800.0, 900.0]);
        assert_eq!(mesh.lines.len() / 12, 12);
        for edge in mesh.lines.chunks_exact(12) {
            let changed_axes = (0..3).filter(|i| edge[*i] != edge[6 + i]).count();
            assert_eq!(changed_axes, 1, "box outline must follow one local axis");
        }
    }

    #[test]
    fn snap_ranks_visible_candidates_in_pixels_and_alt_bypasses() {
        let source = Uuid::new_v4();
        let target = Uuid::new_v4();
        let mut project = Project::new("Snap", Currency::Brl);
        project.boards.push(board(source, [0.0; 3], None));
        project.boards.push(board(target, [160.0, 0.0, 0.0], None));
        let mut camera = Camera::default();
        camera.set_preset(Preset::Front);
        camera.target = [100.0, 50.0, 10.0];
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let seed = Pose::new([60.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap();
        let candidate =
            plan_my_cabinet::placement::snap_candidates(&project, source, seed, 1_000.0)
                .unwrap()
                .into_iter()
                .find(|c| visible_face(&project, &camera, rect, source, c))
                .unwrap();
        let free = Pose::new(
            std::array::from_fn(|i| {
                candidate.world_pose.translation_mm[i] + if i == 0 { 2.0 } else { 0.0 }
            }),
            Quaternion::IDENTITY,
        )
        .unwrap();
        let (snapped, selected) = drag_target(&project, &camera, rect, source, free, seed, false);
        let Some(DragSnap::Face(selected)) = selected else {
            panic!("face should win")
        };
        assert_eq!(selected.target_id, target);
        assert_eq!(snapped, selected.world_pose);
        let origin = camera.project(free.translation_mm, rect).unwrap();
        let chosen_distance =
            origin.distance(camera.project(snapped.translation_mm, rect).unwrap());
        for c in plan_my_cabinet::placement::snap_candidates(&project, source, free, 1_000_000.0)
            .unwrap()
        {
            if visible_face(&project, &camera, rect, source, &c) {
                assert!(
                    chosen_distance
                        <= origin
                            .distance(camera.project(c.world_pose.translation_mm, rect).unwrap())
                            + 1e-4
                );
            }
        }
        assert_eq!(
            drag_target(&project, &camera, rect, source, free, seed, true).0,
            free
        );
        assert!(
            drag_target(&project, &camera, rect, source, free, seed, true)
                .1
                .is_none()
        );
        project
            .boards
            .push(board(Uuid::new_v4(), [160.0, -50.0, 0.0], None));
        assert!(!visible_face(&project, &camera, rect, source, &selected));
        assert_ne!(
            screen_snap(&project, &camera, rect, source, free).map(|c| c.target_id),
            Some(target)
        );
    }

    #[test]
    fn independent_snap_modes_alt_and_spacing_leave_committed_poses_alone() {
        let source = Uuid::new_v4();
        let target = Uuid::new_v4();
        let mut project = Project::new("Snap options", Currency::Brl);
        project.boards.push(board(source, [0.0; 3], None));
        project.boards.push(board(target, [160.0, 0.0, 0.0], None));
        project.grid_spacing = Length::from_micrometres(20_000);
        let mut tool = MoveTool::default();
        assert!(tool.face_snap && tool.grid_snap);
        tool.face_snap = false;
        tool.grid_snap = true;
        assert!(!tool.take_grid_edit_request());
        for board in &project.boards {
            project.materials.push(Material {
                id: board.material_id,
                name: board.name.clone(),
                default_thickness: board.thickness,
                default_grain: BoardGrain::Unrestricted,
            });
        }
        let before = project.clone();
        let mut camera = Camera::default();
        camera.set_preset(Preset::Front);
        camera.target = [100.0, 50.0, 10.0];
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let start = Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap();
        let seed = Pose::new([60.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap();
        let candidate =
            plan_my_cabinet::placement::snap_candidates(&project, source, seed, 1_000.0)
                .unwrap()
                .into_iter()
                .find(|c| visible_face(&project, &camera, rect, source, c))
                .unwrap();
        let mut translation = candidate.world_pose.translation_mm;
        translation[0] += 2.0;
        let free = Pose::new(translation, Quaternion::IDENTITY).unwrap();
        let choose = |project: &Project, face, grid, alt| {
            drag_target_with_modes_visible(
                project,
                &camera,
                rect,
                source,
                free,
                start,
                alt,
                face,
                grid,
                &Selection::default(),
            )
        };
        assert!(matches!(
            choose(&project, true, true, false).1,
            Some(DragSnap::Face(_))
        ));
        let (grid_pose, grid_snap) = choose(&project, false, true, false);
        assert!(matches!(grid_snap, Some(DragSnap::Grid)));
        assert_eq!(
            grid_pose.translation_mm[0],
            (free.translation_mm[0] / 20.0).round() * 20.0
        );
        assert!(matches!(
            choose(&project, true, false, false).1,
            Some(DragSnap::Face(_))
        ));
        assert!(matches!(choose(&project, false, false, false), (pose, None) if pose == free));
        for (face, grid) in [(true, true), (true, false), (false, true), (false, false)] {
            assert!(matches!(choose(&project, face, grid, true), (pose, None) if pose == free));
        }
        assert!(matches!(
            choose(&project, true, true, false).1,
            Some(DragSnap::Face(_))
        ));
        assert_eq!(project, before);
        let probe = Pose::new([43.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap();
        let grid_at = |project: &Project| {
            let (pose, candidate) = drag_target_with_modes_visible(
                project,
                &camera,
                rect,
                source,
                probe,
                start,
                false,
                false,
                true,
                &Selection::default(),
            );
            assert!(matches!(candidate, Some(DragSnap::Grid)));
            pose.translation_mm[0]
        };
        assert_eq!(grid_at(&project), 40.0);
        let mut editor = ProjectEditor::new(project).unwrap();
        let old_poses: Vec<_> = editor.project().boards.iter().map(|b| b.pose).collect();
        editor
            .set_grid_spacing(Length::from_micrometres(30_000))
            .unwrap();
        assert_eq!(
            editor
                .project()
                .boards
                .iter()
                .map(|b| b.pose)
                .collect::<Vec<_>>(),
            old_poses
        );
        assert_eq!(editor.project().grid_spacing.micrometres(), 30_000);
        assert_eq!(grid_at(editor.project()), 30.0);
        assert!(matches!(
            choose(editor.project(), false, true, false).1,
            Some(DragSnap::Grid)
        ));
        assert_eq!(
            choose(editor.project(), false, true, false)
                .0
                .translation_mm[0],
            (free.translation_mm[0] / 30.0).round() * 30.0
        );
        editor.undo().unwrap();
        assert_eq!(
            editor
                .project()
                .boards
                .iter()
                .map(|b| b.pose)
                .collect::<Vec<_>>(),
            old_poses
        );
    }

    #[test]
    fn grid_snap_is_screen_limited_and_preserves_z_rotation_and_rotated_parent() {
        let id = Uuid::new_v4();
        let parent = Uuid::new_v4();
        let mut project = Project::new("Grid", Currency::Brl);
        project.grid_spacing = Length::from_micrometres(20_000);
        let quarter = Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap();
        project.assemblies.push(Assembly {
            id: parent,
            name: "Turned".into(),
            parent_id: None,
            pose: Pose::new([100.0, 100.0, 0.0], quarter).unwrap(),
        });
        project
            .boards
            .push(board(id, [15.0, 25.0, 0.0005], Some(parent)));
        project.materials.push(Material {
            id: project.boards[0].material_id,
            name: "Wood".into(),
            default_thickness: project.boards[0].thickness,
            default_grain: BoardGrain::Unrestricted,
        });
        let start = plan_my_cabinet::placement::world_pose(&project, id).unwrap();
        let mut camera = Camera::default();
        camera.set_preset(Preset::Top);
        camera.target = start.translation_mm;
        camera.distance = 250.0;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        assert!(
            matches!(drag_target(&project, &camera, rect, id, start, start, false), (pose, None) if pose == start)
        );
        let free = Pose::new([77.0, 116.0, start.translation_mm[2]], start.rotation).unwrap();
        let (snapped, target) = drag_target(&project, &camera, rect, id, free, start, false);
        assert!(matches!(target, Some(DragSnap::Grid)));
        assert_eq!(
            snapped.translation_mm,
            [80.0, 120.0, start.translation_mm[2]]
        );
        assert_eq!(snapped.rotation, start.rotation);
        assert_eq!(
            drag_target(&project, &camera, rect, id, free, start, true).0,
            free
        );
        let distant = Pose::new([51.0, 51.0, start.translation_mm[2]], start.rotation).unwrap();
        assert!(
            matches!(drag_target(&project, &camera, rect, id, distant, start, false), (pose, None) if pose == distant)
        );
        let mut editor = ProjectEditor::new(project).unwrap();
        {
            let mut session =
                plan_my_cabinet::placement::PlacementSession::begin(&mut editor, id).unwrap();
            session.preview_free(snapped).unwrap();
            session.pause();
        }
        assert!(!editor.can_undo());
        assert_eq!(editor.project().boards[0].pose.translation_mm[2], 0.0005);
        {
            let session =
                plan_my_cabinet::placement::PlacementSession::resume(&mut editor, id).unwrap();
            session.accept().unwrap();
        }
        let committed = plan_my_cabinet::placement::world_pose(editor.project(), id).unwrap();
        for axis in 0..3 {
            assert!((committed.translation_mm[axis] - snapped.translation_mm[axis]).abs() < 1e-6);
        }
        assert_eq!(committed.rotation, snapped.rotation);
        editor.undo().unwrap();
        assert_eq!(editor.project().boards[0].pose.translation_mm[2], 0.0005);
        assert!(!editor.can_undo());
    }

    #[test]
    fn rendered_grid_uses_project_spacing_and_bounded_density() {
        let mut project = Project::new("Grid", Currency::Brl);
        let camera = Camera {
            distance: 100.0,
            ..Camera::default()
        };
        project.grid_spacing = Length::from_micrometres(500_000);
        let mut coarse = Mesh::default();
        add_grid(&mut coarse, &project, &camera);
        assert!(
            coarse
                .lines
                .chunks_exact(12)
                .any(|line| line[0] == 500.0 && line[6] == 500.0)
        );
        project.grid_spacing = Length::from_micrometres(1);
        let mut fine = Mesh::default();
        add_grid(&mut fine, &project, &camera);
        assert!(scene_render::grid_display_interval(&project, &camera) > 0.000001);
        assert!(fine.lines.len() / 12 <= 202);
        assert!(
            fine.lines
                .chunks_exact(12)
                .any(|line| line[0] == 0.0 && line[6] == 0.0)
        );
    }

    #[test]
    fn move_drag_previews_without_mutation_and_cancel_or_release_is_atomic() {
        let ctx = egui::Context::default();
        let id = Uuid::new_v4();
        let mut project = Project::new("Move", Currency::Brl);
        let source = board(id, [0.0; 3], None);
        project.materials.push(Material {
            id: source.material_id,
            name: "Wood".into(),
            default_thickness: source.thickness,
            default_grain: BoardGrain::Unrestricted,
        });
        project.boards.push(source);
        let mut editor = ProjectEditor::new(project.clone()).unwrap();
        let mut selection = Selection::default();
        selection.choose(Some(id), false);
        let mut camera = Camera::default();
        camera.set_preset(Preset::Front);
        camera.target = [50.0, 25.0, 10.0];
        let mut tool = MoveTool {
            mode: ToolMode::Move,
            ..Default::default()
        };
        fn frame(
            ctx: &egui::Context,
            camera: &mut Camera,
            editor: &mut ProjectEditor,
            selection: &mut Selection,
            tool: &mut MoveTool,
            events: Vec<egui::Event>,
        ) -> egui::Rect {
            let mut result = None;
            let mut canvas_rect = egui::Rect::NOTHING;
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        let project = editor.preview().unwrap_or(editor.project());
                        controls::show(ui, camera, project, selection, tool, false, false, false);
                        (canvas_rect, result) = canvas::interact(
                            ui, camera, project, selection, tool, false, false, false,
                        );
                    });
                },
            )
            .drop_without_applying_deltas();
            match result {
                Some(DragAction::Preview(board, pose)) => {
                    let mut session =
                        plan_my_cabinet::placement::PlacementSession::resume(editor, board)
                            .unwrap();
                    session.preview_free(pose).unwrap();
                    session.pause();
                }
                Some(DragAction::Accept(board, Some(pose))) => {
                    let mut session =
                        plan_my_cabinet::placement::PlacementSession::resume(editor, board)
                            .unwrap();
                    session.preview_free(pose).unwrap();
                    session.accept().unwrap();
                }
                Some(DragAction::Cancel(ids, active)) => {
                    editor.cancel_preview();
                    selection.ids = ids;
                    selection.active = active;
                }
                _ => {}
            }
            canvas_rect
        }
        macro_rules! draw {
            ($events:expr) => {
                frame(
                    &ctx,
                    &mut camera,
                    &mut editor,
                    &mut selection,
                    &mut tool,
                    $events,
                )
            };
        }
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let rect = draw!(vec![]);
        let pos = camera.project([50.0, 50.0, 10.0], rect).unwrap();
        let moved = pos + egui::vec2(55.0, 0.0);
        draw!(vec![button(pos, true)]);
        draw!(vec![egui::Event::PointerMoved(moved)]);
        assert!(editor.preview().is_some());
        assert_eq!(editor.project().boards[0].pose, project.boards[0].pose);
        assert!(!editor.can_undo());
        draw!(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(editor.preview().is_none());
        assert_eq!(editor.project().boards[0].pose, project.boards[0].pose);
        assert_eq!(selection.active, Some(id));
        draw!(vec![button(moved, false)]);
        // Moving must still work after the viewport already has keyboard focus.
        draw!(vec![button(pos, true)]);
        draw!(vec![button(pos, false)]);
        assert!(ctx.memory(|m| m.focused().is_some()));
        draw!(vec![button(pos, true)]);
        draw!(vec![egui::Event::PointerMoved(moved)]);
        assert!(editor.preview().is_some());
        draw!(vec![button(moved, false)]);
        assert!(editor.can_undo());
        assert_eq!(editor.project().revision, 1);
        assert_ne!(editor.project().boards[0].pose, project.boards[0].pose);
        editor.undo().unwrap();
        assert_eq!(editor.project().boards[0].pose, project.boards[0].pose);
    }

    fn board(id: Uuid, translation: [f64; 3], parent_id: Option<Uuid>) -> Board {
        Board {
            id,
            name: "Part".into(),
            material_id: Uuid::new_v4(),
            length: Length::from_micrometres(100_000),
            width: Length::from_micrometres(100_000),
            thickness: Length::from_micrometres(20_000),
            grain_override: None,
            parent_id,
            pose: Pose::new(translation, Quaternion::IDENTITY).unwrap(),
        }
    }

    #[test]
    fn face_hits_include_front_back_and_exit_but_reject_parallel_misses() {
        let pose = Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap();
        let dims = [100.0, 100.0, 20.0];
        assert_eq!(
            board_hit(
                Ray {
                    origin: [50.0, 50.0, 100.0],
                    direction: [0.0, 0.0, -1.0]
                },
                pose,
                dims
            ),
            Some((80.0, 5))
        );
        assert_eq!(
            board_hit(
                Ray {
                    origin: [50.0, 50.0, -100.0],
                    direction: [0.0, 0.0, 1.0]
                },
                pose,
                dims
            ),
            Some((100.0, 4))
        );
        assert_eq!(
            board_hit(
                Ray {
                    origin: [50.0, 50.0, 10.0],
                    direction: [0.0, 0.0, 1.0]
                },
                pose,
                dims
            ),
            Some((10.0, 5))
        );
        assert_eq!(
            board_hit(
                Ray {
                    origin: [150.0, 50.0, 100.0],
                    direction: [0.0, 0.0, -1.0]
                },
                pose,
                dims
            ),
            None
        );
    }

    #[test]
    fn picking_uses_nearest_board_and_stable_id_on_coplanar_faces_in_both_views() {
        let mut project = Project::new("Pick", Currency::Brl);
        let near = Uuid::from_u128(2);
        let far = Uuid::from_u128(3);
        project.boards.push(board(far, [0.0, 150.0, 0.0], None));
        project.boards.push(board(near, [0.0, 0.0, 0.0], None));
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let pointer = rect.center();
        for projection in [Projection::Perspective, Projection::Orthographic] {
            let mut camera = Camera {
                projection,
                ..Camera::default()
            };
            camera.set_preset(Preset::Front);
            camera.target = [50.0, 50.0, 10.0];
            assert_eq!(pick(&project, &camera, pointer, rect), Some(near));
            project.boards[1].pose.translation_mm[0] = 150.0;
            assert_eq!(pick(&project, &camera, pointer, rect), Some(far));
            project.boards[1].pose.translation_mm[0] = 0.0;
        }
        project.boards[0].pose = project.boards[1].pose;
        project.boards[0].id = Uuid::from_u128(1);
        let mut camera = Camera::default();
        camera.set_preset(Preset::Front);
        camera.target = [50.0, 50.0, 10.0];
        assert_eq!(
            pick(&project, &camera, pointer, rect),
            Some(Uuid::from_u128(1))
        );
    }

    #[test]
    fn nested_rotations_are_used_for_picking() {
        let mut project = Project::new("Nested", Currency::Brl);
        let outer = Uuid::new_v4();
        let inner = Uuid::new_v4();
        let quarter = Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap();
        project.assemblies.push(Assembly {
            id: outer,
            name: "Outer".into(),
            parent_id: None,
            pose: Pose::new([300.0, 200.0, 0.0], quarter).unwrap(),
        });
        project.assemblies.push(Assembly {
            id: inner,
            name: "Inner".into(),
            parent_id: Some(outer),
            pose: Pose::new([10.0, 0.0, 0.0], quarter).unwrap(),
        });
        let id = Uuid::new_v4();
        project
            .boards
            .push(board(id, [20.0, 0.0, 0.0], Some(inner)));
        let pose = world_pose(&project, &project.boards[0]).unwrap();
        let center = pose.transform_point([50.0, 50.0, 10.0]).unwrap();
        let ray = Ray {
            origin: add_scaled(center, [0.0, 0.0, 1.0], 100.0),
            direction: [0.0, 0.0, -1.0],
        };
        assert_eq!(board_hit(ray, pose, [100.0, 100.0, 20.0]).unwrap().1, 5);
        let mut camera = Camera::default();
        camera.set_preset(Preset::Top);
        camera.target = center;
        assert_eq!(
            pick(
                &project,
                &camera,
                egui::pos2(400.0, 300.0),
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
            ),
            Some(id)
        );
    }

    #[test]
    fn selection_additive_empty_and_active_highlight() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let mut selection = Selection::default();
        selection.choose(Some(a), false);
        selection.choose(Some(b), true);
        assert_eq!(selection.ids, HashSet::from([a, b]));
        assert_eq!(selection.active, Some(b));
        assert_ne!(
            highlight_color(
                &Project::new("test", plan_my_cabinet::money::Currency::Brl),
                a,
                &selection
            ),
            highlight_color(
                &Project::new("test", plan_my_cabinet::money::Currency::Brl),
                b,
                &selection
            )
        );
        selection.choose(Some(b), true);
        assert_eq!(selection.active, Some(a));
        selection.choose(None, false);
        assert!(selection.ids.is_empty());
        assert_eq!(selection.active, None);
    }

    #[test]
    fn viewport_drag_orbits_without_selecting_and_modal_blocks_scene_clicks() {
        let ctx = egui::Context::default();
        let mut camera = Camera::default();
        camera.set_preset(Preset::Front);
        camera.target = [50.0, 50.0, 10.0];
        let mut project = Project::new("Pick", Currency::Brl);
        let id = Uuid::new_v4();
        project.boards.push(board(id, [0.0; 3], None));
        let mut selection = Selection::default();
        fn frame(
            ctx: &egui::Context,
            camera: &mut Camera,
            project: &Project,
            selection: &mut Selection,
            events: Vec<egui::Event>,
            modal: bool,
        ) {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        show(ui, camera, project, selection, modal, Language::En, None);
                    });
                },
            )
            .drop_without_applying_deltas();
        }
        macro_rules! draw {
            ($events:expr, $modal:expr $(,)?) => {
                frame(&ctx, &mut camera, &project, &mut selection, $events, $modal)
            };
        }
        let pos = egui::pos2(400.0, 400.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        draw!(vec![], false);
        draw!(
            vec![egui::Event::PointerMoved(pos), button(pos, true)],
            false,
        );
        draw!(
            vec![egui::Event::PointerMoved(egui::pos2(440.0, 400.0))],
            false,
        );
        draw!(
            vec![egui::Event::PointerMoved(egui::pos2(450.0, 400.0))],
            false
        );
        draw!(vec![button(egui::pos2(440.0, 400.0), false)], false);
        assert!(selection.ids.is_empty());
        assert_ne!(camera.yaw, -std::f64::consts::FRAC_PI_2);
        // Reset the camera so the central ray still crosses the board.
        camera.set_preset(Preset::Top);
        draw!(
            vec![egui::Event::PointerMoved(pos), button(pos, true)],
            true,
        );
        draw!(vec![button(pos, false)], true);
        assert!(selection.ids.is_empty());
        assert_ne!(
            pick(
                &project,
                &camera,
                egui::pos2(400.0, 300.0),
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
            ),
            None
        );
    }

    fn keyboard_frame(ctx: &egui::Context, camera: &mut Camera, events: Vec<egui::Event>) {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let mut text = String::new();
                    ui.add(egui::TextEdit::singleline(&mut text).id(egui::Id::new("other-field")));
                    show(
                        ui,
                        camera,
                        &Project::new("View", Currency::Brl),
                        &mut Selection::default(),
                        false,
                        Language::En,
                        None,
                    );
                });
            },
        )
        .drop_without_applying_deltas();
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn focused_viewport_keeps_arrows_and_tab_still_leaves() {
        let ctx = egui::Context::default();
        let mut camera = Camera::default();
        keyboard_frame(&ctx, &mut camera, vec![]);
        // Click inside the image, rather than on its toolbar or the text field.
        let pos = egui::pos2(400.0, 400.0);
        keyboard_frame(
            &ctx,
            &mut camera,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        keyboard_frame(
            &ctx,
            &mut camera,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let viewport_id = ctx.memory(|m| m.focused()).expect("click focuses viewport");
        let yaw = camera.yaw;
        // The very next pass is the one where egui has not installed the
        // focus filter yet; this arrow must still orbit and retain focus.
        keyboard_frame(
            &ctx,
            &mut camera,
            vec![key(egui::Key::ArrowRight, egui::Modifiers::NONE)],
        );
        assert_ne!(camera.yaw, yaw);
        assert_eq!(ctx.memory(|m| m.focused()), Some(viewport_id));
        let pitch = camera.pitch;
        keyboard_frame(
            &ctx,
            &mut camera,
            vec![key(egui::Key::ArrowDown, egui::Modifiers::NONE)],
        );
        assert_ne!(camera.pitch, pitch);
        assert_eq!(ctx.memory(|m| m.focused()), Some(viewport_id));

        let target = camera.target;
        keyboard_frame(
            &ctx,
            &mut camera,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
                key(egui::Key::ArrowRight, egui::Modifiers::SHIFT),
            ],
        );
        assert_ne!(camera.target, target);
        assert_eq!(ctx.memory(|m| m.focused()), Some(viewport_id));

        keyboard_frame(
            &ctx,
            &mut camera,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                key(egui::Key::Tab, egui::Modifiers::NONE),
            ],
        );
        assert_ne!(ctx.memory(|m| m.focused()), Some(viewport_id));
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("other-field")));
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(egui::Id::new("other-field"))
        );
        keyboard_frame(&ctx, &mut camera, vec![]);
        let yaw = camera.yaw;
        let pitch = camera.pitch;
        keyboard_frame(
            &ctx,
            &mut camera,
            vec![key(egui::Key::ArrowUp, egui::Modifiers::NONE)],
        );
        assert_eq!((camera.yaw, camera.pitch), (yaw, pitch));
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(egui::Id::new("other-field"))
        );
        camera.target = [120.0, 70.0, 30.0];
        keyboard_frame(
            &ctx,
            &mut camera,
            vec![key(egui::Key::F, egui::Modifiers::NONE)],
        );
        assert_eq!(
            camera.target,
            [120.0, 70.0, 30.0],
            "text focus isolates frame shortcut"
        );
        ctx.memory_mut(|m| m.request_focus(viewport_id));
        keyboard_frame(&ctx, &mut camera, vec![]);
        keyboard_frame(
            &ctx,
            &mut camera,
            vec![key(egui::Key::F, egui::Modifiers::NONE)],
        );
        assert_eq!(
            camera.target, [0.0; 3],
            "focused empty viewport frames scene"
        );
    }

    #[test]
    fn camera_basis_is_orthonormal_and_zoom_bounded() {
        let mut camera = Camera::default();
        camera.orbit(9000.0, 9000.0);
        let (right, up, forward) = camera.basis();
        let dot = |a: [f64; 3], b: [f64; 3]| a.into_iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
        for axis in [right, up, forward] {
            assert!((dot(axis, axis) - 1.0).abs() < 1e-12);
        }
        assert!(dot(right, up).abs() < 1e-12);
        assert!(dot(up, forward).abs() < 1e-12);
        camera.zoom(1e300);
        assert_eq!(camera.distance, MIN_DISTANCE);
        camera.zoom(1e-300);
        assert_eq!(camera.distance, MAX_DISTANCE);
    }

    #[test]
    fn framing_tall_and_wide_bounds_keeps_all_corners_in_view() {
        let mut camera = Camera::default();
        for projection in [Projection::Perspective, Projection::Orthographic] {
            camera.projection = projection;
            for preset in [Preset::Isometric, Preset::Front, Preset::Right, Preset::Top] {
                camera.set_preset(preset);
                for (b, aspect) in [
                    (
                        Bounds {
                            min: [-100.0, -100.0, 0.0],
                            max: [100.0, 100.0, 2000.0],
                        },
                        0.25,
                    ),
                    (
                        Bounds {
                            min: [-2000.0, -100.0, 0.0],
                            max: [2000.0, 100.0, 100.0],
                        },
                        4.0,
                    ),
                ] {
                    camera.frame(b, aspect);
                    let (right, up, forward) = camera.basis();
                    for x in [b.min[0], b.max[0]] {
                        for y in [b.min[1], b.max[1]] {
                            for z in [b.min[2], b.max[2]] {
                                let p = [
                                    x - camera.target[0],
                                    y - camera.target[1],
                                    z - camera.target[2],
                                ];
                                let dot = |axis: [f64; 3]| {
                                    axis.into_iter().zip(p).map(|(a, b)| a * b).sum::<f64>()
                                };
                                let depth = camera.distance + dot(forward);
                                assert!(depth > 0.0);
                                let vertical = if camera.projection == Projection::Perspective {
                                    depth
                                } else {
                                    camera.distance
                                } * (FOV / 2.0).tan();
                                assert!(dot(up).abs() < vertical);
                                assert!(dot(right).abs() < vertical * aspect);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn guarded_camera_actions_match_shader_projection_and_picking_without_project_edits() {
        use crate::actions::{self, ActionId as A, Argument, Request, Unavailable};
        let id = Uuid::new_v4();
        let mut project = Project::new("Camera", Currency::Brl);
        project.boards.push(board(id, [150.0, -80.0, 30.0], None));
        let source = &project.boards[0];
        project.materials.push(Material {
            id: source.material_id,
            name: "Wood".into(),
            default_thickness: source.thickness,
            default_grain: BoardGrain::Unrestricted,
        });
        let editor = ProjectEditor::new(project).unwrap();
        let original = editor.project().boards[0].pose;
        let revision = editor.project().revision;
        let mut selection = Selection::default();
        selection.choose(Some(id), false);
        let mut tool = MoveTool::default();
        let mut camera = Camera {
            viewport_aspect: 800.0 / 600.0,
            ..Camera::default()
        };
        let rect = egui::Rect::from_min_size(egui::pos2(72.0, 95.0), egui::vec2(800.0, 600.0));
        for projection in [Projection::Perspective, Projection::Orthographic] {
            actions::viewport_control(
                Request::new(A::ViewProjection).argument(Argument::Projection(projection)),
                &mut camera,
                &mut tool,
                editor.project(),
                &selection,
                false,
                false,
            )
            .unwrap();
            for preset in [Preset::Isometric, Preset::Front, Preset::Right, Preset::Top] {
                actions::viewport_control(
                    Request::new(A::ViewPreset).argument(Argument::Preset(preset)),
                    &mut camera,
                    &mut tool,
                    editor.project(),
                    &selection,
                    false,
                    false,
                )
                .unwrap();
                actions::viewport_control(
                    Request::new(A::ViewFrame),
                    &mut camera,
                    &mut tool,
                    editor.project(),
                    &selection,
                    false,
                    false,
                )
                .unwrap();
                let corners = board_corners(editor.project(), &editor.project().boards[0]).unwrap();
                for point in corners {
                    let screen = camera.project(point, rect).unwrap();
                    assert!(
                        rect.contains(screen),
                        "{projection:?} {preset:?}: {screen:?}"
                    );
                    // Evaluate the same uniform and perspective divide used by the native WGSL vertex shader.
                    let uniform = camera.uniform([800, 600], 4000.0);
                    let floats: Vec<f32> = uniform
                        .chunks_exact(4)
                        .map(|chunk| f32::from_ne_bytes(chunk.try_into().unwrap()))
                        .collect();
                    let relative =
                        std::array::from_fn::<_, 3, _>(|i| (point[i] - camera.target[i]) as f32);
                    let d = std::array::from_fn::<_, 3, _>(|i| relative[i] - floats[12 + i]);
                    let axis_dot =
                        |base: usize| (0..3).map(|i| d[i] * floats[base + i]).sum::<f32>();
                    let depth = axis_dot(8);
                    let w = if projection == Projection::Perspective {
                        depth
                    } else {
                        1.0
                    };
                    let gpu = egui::pos2(
                        rect.center().x + axis_dot(0) * floats[16] / w * rect.width() / 2.0,
                        rect.center().y - axis_dot(4) * floats[17] / w * rect.height() / 2.0,
                    );
                    assert!(
                        (screen - gpu).length() < 0.01,
                        "{projection:?} {preset:?}: {screen:?} vs {gpu:?}"
                    );
                }
                let center = world_pose(editor.project(), &editor.project().boards[0])
                    .unwrap()
                    .transform_point([50.0, 50.0, 10.0])
                    .unwrap();
                let screen = camera.project(center, rect).unwrap();
                assert_eq!(
                    pick_visible(editor.project(), &camera, screen, rect, &selection),
                    Some(id)
                );
            }
        }
        assert_eq!(
            actions::viewport_control(
                Request::new(A::ViewMove),
                &mut camera,
                &mut tool,
                editor.project(),
                &selection,
                true,
                false
            ),
            Err(Unavailable::ModalOpen),
        );
        assert_eq!(tool.mode, ToolMode::Navigate);
        actions::viewport_control(
            Request::new(A::ViewMove),
            &mut camera,
            &mut tool,
            editor.project(),
            &selection,
            false,
            false,
        )
        .unwrap();
        assert_eq!(tool.mode, ToolMode::Move);
        assert_eq!(
            actions::viewport_control(
                Request::new(A::ViewNavigate),
                &mut camera,
                &mut tool,
                editor.project(),
                &selection,
                false,
                true
            ),
            Err(Unavailable::Busy),
        );
        assert_eq!(tool.mode, ToolMode::Move);
        assert_eq!(editor.project().boards[0].pose, original);
        assert_eq!(editor.project().revision, revision);
        assert!(!editor.can_undo());
    }

    #[test]
    fn top_frame_uses_nested_assembly_bounds_and_rejects_hidden_selection() {
        use crate::actions::{self, ActionId as A, Argument, Request, Unavailable};
        let group = Uuid::new_v4();
        let mut project = Project::new("Assembly frame", Currency::Brl);
        let mut panel = board(Uuid::new_v4(), [40.0, 10.0, 25.0], Some(group));
        panel.width = Length::from_micrometres(840_000);
        project.materials.push(Material {
            id: panel.material_id,
            name: "Panel".into(),
            default_thickness: panel.thickness,
            default_grain: BoardGrain::Unrestricted,
        });
        project.boards.push(panel);
        project.assemblies.push(Assembly {
            id: group,
            name: "Tall assembly".into(),
            parent_id: None,
            pose: Pose::new(
                [300.0, -250.0, 110.0],
                Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap(),
            )
            .unwrap(),
        });
        let editor = ProjectEditor::new(project).unwrap();
        let mut selection = Selection::default();
        selection.choose(Some(group), false);
        let mut camera = Camera {
            viewport_aspect: 0.65,
            ..Camera::default()
        };
        let mut tool = MoveTool::default();
        actions::viewport_control(
            Request::new(A::ViewPreset).argument(Argument::Preset(Preset::Top)),
            &mut camera,
            &mut tool,
            editor.project(),
            &selection,
            false,
            false,
        )
        .unwrap();
        actions::viewport_control(
            Request::new(A::ViewFrame),
            &mut camera,
            &mut tool,
            editor.project(),
            &selection,
            false,
            false,
        )
        .unwrap();
        let b = bounds_visible(editor.project(), &selection.ids, &selection).unwrap();
        assert_eq!(camera.target, b.center());
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(390.0, 600.0));
        for point in board_corners(editor.project(), &editor.project().boards[0]).unwrap() {
            assert!(rect.contains(camera.project(point, rect).unwrap()));
        }
        selection.hidden.insert(group);
        let before = (camera.target, camera.distance);
        assert_eq!(
            actions::viewport_control(
                Request::new(A::ViewFrame),
                &mut camera,
                &mut tool,
                editor.project(),
                &selection,
                false,
                false
            ),
            Err(Unavailable::NoSelection),
        );
        assert_eq!((camera.target, camera.distance), before);
        assert_eq!(editor.project().revision, 0);
        assert!(!editor.can_undo());
    }

    #[test]
    fn project_mesh_and_selection_bounds_follow_parent_pose_and_dimensions() {
        let mut project = Project::new("View", Currency::Brl);
        let parent = Uuid::new_v4();
        let pose = Pose::new(
            [300.0, 200.0, 0.0],
            Quaternion::normalized(2.0_f64.sqrt() / 2.0, 0.0, 0.0, 2.0_f64.sqrt() / 2.0).unwrap(),
        )
        .unwrap();
        project.assemblies.push(Assembly {
            id: parent,
            name: "Group".into(),
            parent_id: None,
            pose,
        });
        let board_id = Uuid::new_v4();
        project.boards.push(Board {
            id: board_id,
            name: "Shelf".into(),
            material_id: Uuid::new_v4(),
            length: Length::from_micrometres(100_000),
            width: Length::from_micrometres(50_000),
            thickness: Length::from_micrometres(18_000),
            grain_override: None,
            parent_id: Some(parent),
            pose: Pose::new([20.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
        });
        let selected = HashSet::from([board_id]);
        let b = bounds(&project, &selected).unwrap();
        let assembly_selected = HashSet::from([parent]);
        assert!(selected_board(&project, &assembly_selected, board_id));
        assert_eq!(bounds(&project, &assembly_selected).unwrap().min, b.min);
        assert_eq!(bounds(&project, &assembly_selected).unwrap().max, b.max);
        assert_ne!(
            highlight_color(
                &project,
                board_id,
                &Selection {
                    ids: assembly_selected,
                    active: Some(parent),
                    hidden: HashSet::new(),
                }
            ),
            highlight_color(&project, board_id, &Selection::default())
        );
        assert!((b.min[0] - 250.0).abs() < 1e-8);
        assert!((b.max[1] - 320.0).abs() < 1e-8);
        let mut camera = Camera::default();
        camera.frame(b, 1.0);
        assert!((camera.target[0] - 275.0).abs() < 1e-8);
        let (mesh, _) = scene(
            &project,
            &camera,
            &Selection {
                ids: selected,
                active: Some(board_id),
                hidden: HashSet::new(),
            },
        );
        assert_eq!(mesh.faces.len() / 6, 36);
        assert!(mesh.lines.len() / 6 >= 24 + 6);
        assert!(mesh.faces.iter().all(|v| v.is_finite()));
        let mut visibility = Selection::default();
        visibility.choose(Some(board_id), false);
        visibility.hidden.insert(parent);
        assert!(!visibility.visible(&project, board_id));
        assert!(bounds_visible(&project, &visibility.ids, &visibility).is_none());
        let (hidden_mesh, _) = scene(&project, &camera, &visibility);
        assert!(hidden_mesh.faces.is_empty());
        let center = b.center();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 800.0));
        let pointer = camera.project(center, rect).unwrap();
        assert_eq!(
            pick_visible(&project, &camera, pointer, rect, &visibility),
            None
        );
        // Selection remains available through the object list, and revealing a
        // descendant clears its hidden ancestors without changing the project.
        visibility.reveal(&project, board_id);
        assert!(visibility.visible(&project, board_id));
        assert_eq!(visibility.active, Some(board_id));
        assert!(bounds_visible(&project, &visibility.ids, &visibility).is_some());
    }

    #[test]
    fn placeholder_is_rendered_pickable_and_hidden_independently() {
        let mut project = Project::new("Feet", Currency::Brl);
        let id = Uuid::new_v4();
        project.hardware.push(plan_my_cabinet::domain::Hardware {
            id,
            name: "Foot".into(),
            parent_id: None,
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            kind: HardwareKind::Placeholder {
                dimensions: [Length::from_micrometres(100_000); 3],
            },
        });
        let camera = Camera {
            target: [50.0; 3],
            ..Camera::default()
        };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let pointer = camera.project([50.0; 3], rect).unwrap();
        let mut selection = Selection::default();
        assert_eq!(
            pick_visible(&project, &camera, pointer, rect, &selection),
            Some(id)
        );
        let (mesh, _) = scene(&project, &camera, &selection);
        assert!(!mesh.faces.is_empty());
        selection.choose(Some(id), false);
        let (highlighted, _) = scene(&project, &camera, &selection);
        assert_eq!(mesh.faces, highlighted.faces);
        assert_ne!(mesh.lines, highlighted.lines);
        selection.hidden.insert(id);
        assert_eq!(
            pick_visible(&project, &camera, pointer, rect, &selection),
            None
        );
        let (hidden, _) = scene(&project, &camera, &selection);
        assert!(hidden.faces.is_empty());
        assert!(hidden.shadow.is_empty());
    }
}
