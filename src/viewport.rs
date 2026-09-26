//! Project-driven, depth-tested 3D viewport. Camera state is presentation-only.
use eframe::{egui, egui_wgpu, wgpu};
use plan_my_cabinet::domain::{Board, HardwareKind, Project};
use plan_my_cabinet::placement::{BoardFace, Side};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use wgpu::util::DeviceExt;

const SCENE_SHADER: &str = r#"
struct Params {
    right: vec4<f32>, up: vec4<f32>, forward: vec4<f32>, eye: vec4<f32>,
    projection: vec4<f32>, // horizontal and vertical scale, near, far
    mode: vec4<f32>, // perspective = 1
};
@group(0) @binding(0) var<uniform> params: Params;
struct In { @location(0) position: vec3<f32>, @location(1) color: vec3<f32> };
struct Out { @builtin(position) clip: vec4<f32>, @location(0) color: vec3<f32> };
@vertex fn vs(v: In) -> Out {
    let d = v.position - params.eye.xyz;
    let depth = dot(d, params.forward.xyz);
    var o: Out;
    let w = select(1.0, depth, params.mode.x > 0.5);
    let z = select((depth - params.projection.z) / (params.projection.w - params.projection.z),
                   (depth * params.projection.w - params.projection.z * params.projection.w) /
                   (params.projection.w - params.projection.z), params.mode.x > 0.5);
    o.clip = vec4<f32>(dot(d, params.right.xyz) * params.projection.x,
                       dot(d, params.up.xyz) * params.projection.y, z, w);
    o.color = v.color;
    return o;
}
@fragment fn fs(v: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(v.color, 1.0);
}
"#;

const COMPOSITE_SHADER: &str = r#"
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
struct Out { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) index: u32) -> Out {
    var o: Out;
    let uv = array<vec2<f32>, 3>(vec2<f32>(0.0, 0.0), vec2<f32>(2.0, 0.0), vec2<f32>(0.0, 2.0));
    o.uv = uv[index];
    o.clip = vec4<f32>(o.uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return o;
}
@fragment fn fs(v: Out) -> @location(0) vec4<f32> {
    return textureSample(image, image_sampler, v.uv);
}
"#;

#[derive(Clone, Copy, PartialEq, Eq)]
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Preset {
    Isometric,
    Front,
    Right,
    Top,
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
}

#[derive(Default)]
pub struct MoveTool {
    pub enabled: bool,
    drag: Option<MoveDrag>,
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

impl MoveTool {
    pub fn cancel(&mut self) -> Option<DragAction> {
        self.drag
            .take()
            .map(|drag| DragAction::Cancel(drag.selection_ids, drag.selection_active))
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: [0.0; 3],
            yaw: -0.8,
            pitch: 0.55,
            distance: 1500.0,
            projection: Projection::Perspective,
            preset: Preset::Isometric,
            viewport_aspect: 1.0,
            viewport_height: 600.0,
        }
    }
}

const FOV: f64 = std::f64::consts::FRAC_PI_4;
const MIN_DISTANCE: f64 = 0.01;
const MAX_DISTANCE: f64 = 1.0e9;

impl Camera {
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
        self.preset = Preset::Isometric;
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
            Preset::Isometric => (-0.8, 0.55),
            Preset::Front => (-std::f64::consts::FRAC_PI_2, 0.0),
            Preset::Right => (0.0, 0.0),
            Preset::Top => (
                -std::f64::consts::FRAC_PI_2,
                std::f64::consts::FRAC_PI_2 - 0.001,
            ),
        };
    }

    fn frame(&mut self, bounds: Bounds, aspect: f64) {
        self.target = bounds.center();
        let radius = bounds.radius().max(10.0);
        // A sphere enclosing every corner fits at every preset/orbit angle.
        let half_vertical = (FOV / 2.0).tan() * aspect.clamp(0.01, 1.0);
        self.distance = (radius * 1.3 / half_vertical).clamp(MIN_DISTANCE, MAX_DISTANCE);
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
    drag_target_visible(
        project,
        camera,
        rect,
        source,
        free,
        start,
        bypass,
        &Selection::default(),
    )
}

#[allow(clippy::too_many_arguments)] // Camera, geometry and session visibility stay independent of project data.
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
    if bypass {
        return (free, None);
    }
    if let Some(face) = screen_snap_visible(project, camera, rect, source, free, selection) {
        return (face.world_pose, Some(DragSnap::Face(face)));
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

#[derive(Default)]
struct Mesh {
    faces: Vec<f32>,
    lines: Vec<f32>,
}

impl Mesh {
    fn vertex(out: &mut Vec<f32>, pos: [f32; 3], color: [f32; 3]) {
        out.extend(pos);
        out.extend(color);
    }

    fn line(&mut self, a: [f32; 3], b: [f32; 3], color: [f32; 3]) {
        Self::vertex(&mut self.lines, a, color);
        Self::vertex(&mut self.lines, b, color);
    }

    fn box_mesh(&mut self, corners: [[f32; 3]; 8], color: [f32; 3]) {
        // `board_corners` uses bit-coded XYZ indexes (2 = min-X/max-Y,
        // 3 = max-X/max-Y). The quad topology below uses perimeter order.
        // Convert once before emitting faces and edges; otherwise each broad
        // face becomes a self-crossing bow-tie of two long triangles.
        let corners = [
            corners[0], corners[1], corners[3], corners[2], corners[4], corners[5], corners[7],
            corners[6],
        ];
        for (face, shade) in [
            ([0, 3, 2, 1], 0.55),
            ([4, 5, 6, 7], 1.0),
            ([0, 1, 5, 4], 0.75),
            ([1, 2, 6, 5], 0.85),
            ([2, 3, 7, 6], 0.68),
            ([3, 0, 4, 7], 0.8),
        ] {
            let shaded = color.map(|c| c * shade);
            for i in [0, 1, 2, 0, 2, 3] {
                Self::vertex(&mut self.faces, corners[face[i]], shaded);
            }
        }
        for (a, b) in [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ] {
            self.line(corners[a], corners[b], [0.08, 0.11, 0.15]);
        }
    }
}

fn highlight_color(project: &Project, id: Uuid, selection: &Selection) -> [f32; 3] {
    if selection.active == Some(id) {
        [1.0, 0.78, 0.12]
    } else if selection.ids.contains(&id) {
        [0.35, 0.78, 0.94]
    } else if selected_board(project, &selection.ids, id) {
        [0.55, 0.76, 0.38]
    } else {
        [0.70, 0.47, 0.26]
    }
}

#[cfg(test)]
fn scene(project: &Project, camera: &Camera, selection: &Selection) -> (Mesh, f64) {
    scene_with_faces(project, camera, selection, None, None)
}

fn scene_with_faces(
    project: &Project,
    camera: &Camera,
    selection: &Selection,
    faces: Option<(Uuid, BoardFace, Uuid, BoardFace)>,
    poses: Option<&HashMap<Uuid, plan_my_cabinet::units::Pose>>,
) -> (Mesh, f64) {
    let mut mesh = Mesh::default();
    add_grid(&mut mesh, project, camera);
    for (end, color) in [
        ([1000.0, 0.0, 0.0], [0.9, 0.18, 0.18]),
        ([0.0, 1000.0, 0.0], [0.16, 0.67, 0.23]),
        ([0.0, 0.0, 1000.0], [0.18, 0.36, 0.94]),
    ] {
        mesh.line(
            relative([0.0; 3], camera.target),
            relative(end, camera.target),
            color,
        );
    }
    let mut all = Bounds::empty();
    for board in &project.boards {
        if !selection.visible(project, board.id) {
            continue;
        }
        let world = poses
            .and_then(|p| p.get(&board.id).copied())
            .map(|pose| {
                box_corners(
                    pose,
                    board
                        .blank_dimensions()
                        .map(|d| d.micrometres() as f64 / 1000.0),
                )
            })
            .unwrap_or_else(|| board_corners(project, board));
        if let Some(world) = world {
            let corners = world.map(|p| {
                all.include(p);
                relative(p, camera.target)
            });
            mesh.box_mesh(corners, highlight_color(project, board.id, selection));
            if let Some((source, source_face, target, target_face)) = faces {
                let selected = if source == board.id {
                    Some((source_face, [0.15, 0.95, 0.95]))
                } else if target == board.id {
                    Some((target_face, [1.0, 0.25, 0.8]))
                } else {
                    None
                };
                if let Some((face, color)) = selected {
                    let fixed = 1 << face.axis;
                    let side = face.side == Side::Positive;
                    let indexes: Vec<_> = (0..8).filter(|i| (*i & fixed != 0) == side).collect();
                    let loop_indices = [indexes[0], indexes[1], indexes[3], indexes[2]];
                    for i in 0..4 {
                        mesh.line(
                            corners[loop_indices[i]],
                            corners[loop_indices[(i + 1) % 4]],
                            color,
                        );
                    }
                    for a in [loop_indices[0], loop_indices[1]] {
                        mesh.line(corners[a], corners[loop_indices[2]], color);
                    }
                }
            }
        }
    }
    for hardware in &project.hardware {
        let HardwareKind::Placeholder { dimensions } = hardware.kind else {
            continue;
        };
        if !selection.visible(project, hardware.id) {
            continue;
        }
        if let Some(pose) = poses
            .and_then(|p| p.get(&hardware.id).copied())
            .or_else(|| plan_my_cabinet::assembly_edit::world_pose(project, hardware.id).ok())
            && let Some(world) =
                box_corners(pose, dimensions.map(|d| d.micrometres() as f64 / 1000.0))
        {
            let corners = world.map(|p| {
                all.include(p);
                relative(p, camera.target)
            });
            let color = if selection.active == Some(hardware.id) {
                [1.0, 0.78, 0.12]
            } else if selection.ids.contains(&hardware.id) {
                [0.35, 0.78, 0.94]
            } else {
                [0.36, 0.55, 0.68]
            };
            mesh.box_mesh(corners, color);
        }
    }
    let radius = if all.valid() {
        (0..3)
            .map(|i| {
                (all.min[i] - camera.target[i])
                    .abs()
                    .max((all.max[i] - camera.target[i]).abs())
                    .powi(2)
            })
            .sum::<f64>()
            .sqrt()
    } else {
        0.0
    };
    (mesh, radius.max(4000.0))
}

/// Draw lines on exact multiples of the project grid. At small spacings use
/// integer multiples of the spacing to keep line count and pixel density sane.
fn add_grid(mesh: &mut Mesh, project: &Project, camera: &Camera) {
    let spacing = project.grid_spacing.micrometres() as f64 / 1000.0;
    let visible_radius = (camera.distance * (FOV / 2.0).tan() * 2.5).clamp(100.0, 2_000_000.0);
    let half = visible_radius.max(1000.0);
    let desired = (visible_radius / 24.0).max(spacing);
    let multiple = 10_f64.powf((desired / spacing).log10().ceil().max(0.0));
    let step = spacing * multiple;
    let center = camera.target;
    let range = |axis: usize| {
        let low = ((center[axis] - half) / step).ceil() as i64;
        let high = ((center[axis] + half) / step).floor() as i64;
        low..=high.min(low + 100)
    };
    for axis in 0..2 {
        for i in range(axis) {
            let n = i as f64 * step;
            let mut a = [center[0] - half, center[1] - half, 0.0];
            let mut b = [center[0] + half, center[1] + half, 0.0];
            a[axis] = n;
            b[axis] = n;
            // World axes remain visible even when a coarse LOD skips fine lines.
            let color = if i == 0 {
                [0.40, 0.44, 0.48]
            } else if i.rem_euclid(10) == 0 {
                [0.57, 0.61, 0.65]
            } else {
                [0.74, 0.77, 0.80]
            };
            mesh.line(relative(a, center), relative(b, center), color);
        }
    }
}

fn relative(point: [f64; 3], origin: [f64; 3]) -> [f32; 3] {
    std::array::from_fn(|i| (point[i] - origin[i]) as f32)
}

fn bytes(data: &[f32]) -> Vec<u8> {
    data.iter().flat_map(|v| v.to_ne_bytes()).collect()
}

struct Targets {
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    bind: wgpu::BindGroup,
}

struct Resources {
    faces: wgpu::Buffer,
    face_count: u32,
    lines: wgpu::Buffer,
    line_count: u32,
    params: wgpu::Buffer,
    params_bind: wgpu::BindGroup,
    faces_pipeline: wgpu::RenderPipeline,
    lines_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    format: wgpu::TextureFormat,
    targets: Option<Targets>,
}

fn scene_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    topology: wgpu::PrimitiveTopology,
    depth_write: bool,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("viewport scene"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 24,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(format.into())],
        }),
        primitive: wgpu::PrimitiveState {
            topology,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24Plus,
            depth_write_enabled: Some(depth_write),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState {
                constant: if depth_write { 1 } else { 0 },
                ..Default::default()
            },
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl Resources {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let empty = [0_u8; 24];
        let faces = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("box faces"),
            contents: &empty,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let lines = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("outlines, axes and grid"),
            contents: &empty,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport camera"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport parameters"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let params_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params.as_entire_binding(),
            }],
        });
        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&params_layout)],
            immediate_size: 0,
        });
        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene shader"),
            source: wgpu::ShaderSource::Wgsl(SCENE_SHADER.into()),
        });
        let faces_pipeline = scene_pipeline(
            device,
            &scene_shader,
            &scene_layout,
            format,
            wgpu::PrimitiveTopology::TriangleList,
            true,
        );
        let lines_pipeline = scene_pipeline(
            device,
            &scene_shader,
            &scene_layout,
            format,
            wgpu::PrimitiveTopology::LineList,
            false,
        );
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport image"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&texture_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport composite shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport composite"),
            layout: Some(&composite_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            faces,
            face_count: 0,
            lines,
            line_count: 0,
            params,
            params_bind,
            faces_pipeline,
            lines_pipeline,
            composite_pipeline,
            texture_layout,
            sampler,
            format,
            targets: None,
        }
    }

    fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = |label, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            "viewport color",
            self.format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        )
        .create_view(&Default::default());
        let depth = texture(
            "viewport depth",
            wgpu::TextureFormat::Depth24Plus,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
        .create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport image binding"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.targets = Some(Targets {
            size,
            color,
            depth,
            bind,
        });
    }
}

pub fn install(state: &egui_wgpu::RenderState) {
    state
        .renderer
        .write()
        .callback_resources
        .insert(Resources::new(&state.device, state.target_format));
}

struct ViewportCallback {
    size: [u32; 2],
    mesh: Mesh,
    uniform: Vec<u8>,
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(r) = resources.get_mut::<Resources>() else {
            return Vec::new();
        };
        r.resize(device, self.size);
        queue.write_buffer(&r.params, 0, &self.uniform);
        if !self.mesh.faces.is_empty() {
            r.faces = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("board faces"),
                contents: &bytes(&self.mesh.faces),
                usage: wgpu::BufferUsages::VERTEX,
            });
        }
        r.face_count = (self.mesh.faces.len() / 6) as u32;
        r.lines = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("board edges and grid"),
            contents: &bytes(&self.mesh.lines),
            usage: wgpu::BufferUsages::VERTEX,
        });
        r.line_count = (self.mesh.lines.len() / 6) as u32;
        let target = r.targets.as_ref().expect("viewport target allocated");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3D viewport"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.color,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.92,
                        g: 0.94,
                        b: 0.96,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &r.params_bind, &[]);
        pass.set_pipeline(&r.faces_pipeline);
        pass.set_vertex_buffer(0, r.faces.slice(..));
        pass.draw(0..r.face_count, 0..1);
        pass.set_pipeline(&r.lines_pipeline);
        pass.set_vertex_buffer(0, r.lines.slice(..));
        pass.draw(0..r.line_count, 0..1);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::epaint::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(r) = resources.get::<Resources>()
            && let Some(target) = &r.targets
        {
            pass.set_pipeline(&r.composite_pipeline);
            pass.set_bind_group(0, &target.bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

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
        faces,
        None,
    );
}

#[allow(clippy::too_many_arguments)] // View state and interaction state are kept separate from project data.
pub fn show_move(
    ui: &mut egui::Ui,
    camera: &mut Camera,
    project: &Project,
    selection: &mut Selection,
    tool: &mut MoveTool,
    modal: bool,
    language: plan_my_cabinet::i18n::Language,
    faces: Option<(Uuid, BoardFace, Uuid, BoardFace)>,
    poses: Option<&HashMap<Uuid, plan_my_cabinet::units::Pose>>,
) -> Option<DragAction> {
    use plan_my_cabinet::i18n::Language;
    let pt = language == Language::PtBr;
    let mut action = None;
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!modal && poses.is_none() && tool.drag.is_none(), |ui| {
            ui.selectable_value(
                &mut tool.enabled,
                false,
                if pt { "Navegar" } else { "Navigate" },
            );
            ui.selectable_value(
                &mut tool.enabled,
                true,
                if pt { "Mover peça" } else { "Move board" },
            );
        });
        ui.label(if tool.enabled && poses.is_none() {
            if pt {
                "Arraste a peça selecionada · Alt ignora encaixe · Esc cancela"
            } else {
                "Drag selected board · Alt bypasses snap · Esc cancels"
            }
        } else if pt {
            "Arraste para orbitar"
        } else {
            "Drag to orbit"
        });
    });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !modal,
                egui::Button::new(if pt {
                    "Enquadrar seleção/cena"
                } else {
                    "Frame selection/scene"
                }),
            )
            .clicked()
        {
            if let Some(b) = bounds_visible(project, &selection.ids, selection)
                .or_else(|| bounds_visible(project, &HashSet::new(), selection))
            {
                camera.frame(b, camera.viewport_aspect);
            } else {
                camera.target = [0.0; 3];
                camera.distance = 1500.0;
            }
        }
        ui.add_enabled_ui(!modal, |ui| {
            egui::ComboBox::from_id_salt("viewport-preset")
                .selected_text(match (pt, camera.preset) {
                    (false, Preset::Isometric) => "Isometric",
                    (true, Preset::Isometric) => "Isométrica",
                    (false, Preset::Front) => "Front",
                    (true, Preset::Front) => "Frontal",
                    (false, Preset::Right) => "Right",
                    (true, Preset::Right) => "Direita",
                    (false, Preset::Top) => "Top",
                    (true, Preset::Top) => "Superior",
                })
                .show_ui(ui, |ui| {
                    for (preset, en, br) in [
                        (Preset::Isometric, "Isometric", "Isométrica"),
                        (Preset::Front, "Front", "Frontal"),
                        (Preset::Right, "Right", "Direita"),
                        (Preset::Top, "Top", "Superior"),
                    ] {
                        if ui
                            .selectable_label(camera.preset == preset, if pt { br } else { en })
                            .clicked()
                        {
                            camera.set_preset(preset);
                            ui.close();
                        }
                    }
                });
            egui::ComboBox::from_id_salt("viewport-projection")
                .selected_text(match (pt, camera.projection) {
                    (false, Projection::Perspective) => "Perspective",
                    (true, Projection::Perspective) => "Perspectiva",
                    (false, Projection::Orthographic) => "Orthographic",
                    (true, Projection::Orthographic) => "Ortográfica",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut camera.projection,
                        Projection::Perspective,
                        if pt { "Perspectiva" } else { "Perspective" },
                    );
                    ui.selectable_value(
                        &mut camera.projection,
                        Projection::Orthographic,
                        if pt { "Ortográfica" } else { "Orthographic" },
                    );
                });
        });
    });
    ui.add_enabled_ui(!modal, |ui| {
        ui.horizontal(|ui| {
            ui.label(if pt { "Órbita" } else { "Orbit" });
            if ui.button(if pt { "Esq" } else { "Left" }).clicked() {
                camera.orbit(-20.0, 0.0);
            }
            if ui.button(if pt { "Dir" } else { "Right" }).clicked() {
                camera.orbit(20.0, 0.0);
            }
            ui.label(if pt { "Deslocar" } else { "Pan" });
            if ui.button(if pt { "Esq" } else { "Left" }).clicked() {
                camera.pan(-28.0, 0.0, camera.viewport_height);
            }
            if ui.button(if pt { "Dir" } else { "Right" }).clicked() {
                camera.pan(28.0, 0.0, camera.viewport_height);
            }
            ui.label("Zoom");
            if ui.button("+").clicked() {
                camera.zoom(1.2);
            }
            if ui.button("-").clicked() {
                camera.zoom(1.0 / 1.2);
            }
        });
    });
    let available = ui.available_size();
    let size = egui::vec2(available.x.max(1.0), available.y.max(1.0));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    camera.viewport_aspect = (rect.width() as f64 / rect.height().max(1.0) as f64).max(0.01);
    camera.viewport_height = rect.height() as f64;
    if modal {
        action = tool.cancel();
    } else {
        if tool.drag.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = tool.cancel();
        }
        if action.is_none()
            && tool.enabled
            && poses.is_none()
            && response.drag_started_by(egui::PointerButton::Primary)
            && !ui.input(|i| i.modifiers.shift || i.modifiers.command)
            && !ui.ctx().text_edit_focused()
            && let Some(start) = ui.input(|i| i.pointer.press_origin())
            && let Some(id) = selection.active
            && pick_visible(project, camera, start, rect, selection) == Some(id)
            && let Ok(world) = plan_my_cabinet::placement::world_pose(project, id)
        {
            tool.drag = Some(MoveDrag {
                board_id: id,
                start,
                world,
                selection_ids: selection.ids.clone(),
                selection_active: selection.active,
                snap: None,
                last_pose: None,
            });
        }
        if action.is_none()
            && let Some(drag) = &mut tool.drag
        {
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                let final_pose = response
                    .interact_pointer_pos()
                    .and_then(|pointer| camera.drag_pose(drag.start, pointer, rect, drag.world))
                    .map(|free| {
                        drag_target_visible(
                            project,
                            camera,
                            rect,
                            drag.board_id,
                            free,
                            drag.world,
                            ui.input(|i| i.modifiers.alt),
                            selection,
                        )
                        .0
                    })
                    .or(drag.last_pose);
                action = Some(DragAction::Accept(drag.board_id, final_pose));
                tool.drag = None;
            } else if let Some(pointer) = response.interact_pointer_pos()
                && let Some(free) = camera.drag_pose(drag.start, pointer, rect, drag.world)
            {
                let (pose, snap) = drag_target_visible(
                    project,
                    camera,
                    rect,
                    drag.board_id,
                    free,
                    drag.world,
                    ui.input(|i| i.modifiers.alt),
                    selection,
                );
                drag.snap = snap;
                drag.last_pose = Some(pose);
                action = Some(DragAction::Preview(drag.board_id, pose));
            }
        }
        if poses.is_none()
            && response.clicked_by(egui::PointerButton::Primary)
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            selection.choose(
                pick_visible(project, camera, pointer, rect, selection),
                additive,
            );
        }
        if response.clicked() || response.drag_started() {
            response.request_focus();
            // egui only accepts a focus lock after the widget was focused in a
            // previous pass. Schedule that pass even if there is no other input.
            ui.ctx().request_repaint();
        }
        let delta = ui.input(|i| i.pointer.delta());
        if response.dragged_by(egui::PointerButton::Secondary)
            || (response.dragged_by(egui::PointerButton::Primary)
                && ui.input(|i| i.modifiers.shift))
        {
            camera.pan(delta.x as f64, delta.y as f64, rect.height() as f64);
        } else if response.dragged_by(egui::PointerButton::Primary)
            && (!tool.enabled || poses.is_some())
        {
            camera.orbit(delta.x as f64, delta.y as f64);
        }
        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            camera.zoom((scroll as f64 * 0.002).exp() * pinch as f64);
        }
        if response.has_focus() && !egui::Popup::is_any_open(ui.ctx()) {
            let plain_arrow_pressed = ui.input(|i| {
                !i.modifiers.any()
                    && [
                        egui::Key::ArrowLeft,
                        egui::Key::ArrowRight,
                        egui::Key::ArrowUp,
                        egui::Key::ArrowDown,
                    ]
                    .into_iter()
                    .any(|key| i.key_pressed(key))
            });
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        tab: false,
                        ..Default::default()
                    },
                );
                // On the first pass after focus is requested the filter is not
                // yet active at begin_pass. Prevent its arrow direction from
                // moving focus at end_pass, even if the user typed immediately.
                if plain_arrow_pressed {
                    memory.move_focus(egui::FocusDirection::None);
                }
            });
            ui.input(|i| {
                let step = if i.modifiers.shift { 28.0 } else { 20.0 };
                let x = (i.key_pressed(egui::Key::ArrowRight) as i32
                    - i.key_pressed(egui::Key::ArrowLeft) as i32) as f64
                    * step;
                let y = (i.key_pressed(egui::Key::ArrowDown) as i32
                    - i.key_pressed(egui::Key::ArrowUp) as i32) as f64
                    * step;
                if i.modifiers.shift {
                    camera.pan(x, y, rect.height() as f64);
                } else {
                    camera.orbit(x, y);
                }
                if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                    camera.zoom(1.2);
                }
                if i.key_pressed(egui::Key::Minus) {
                    camera.zoom(1.0 / 1.2);
                }
            });
        }
    }
    let pixels = ui.ctx().pixels_per_point();
    let size = [
        (rect.width() * pixels).round().max(1.0) as u32,
        (rect.height() * pixels).round().max(1.0) as u32,
    ];
    let faces = tool
        .drag
        .as_ref()
        .and_then(|drag| {
            drag.snap.and_then(|snap| match snap {
                DragSnap::Grid => None,
                DragSnap::Face(snap) => Some((
                    drag.board_id,
                    snap.source_face,
                    snap.target_id,
                    snap.target_face,
                )),
            })
        })
        .or(faces);
    let (mesh, radius) = scene_with_faces(project, camera, selection, faces, poses);
    ui.painter().add(egui_wgpu::Callback::new_paint_callback(
        rect,
        ViewportCallback {
            size,
            mesh,
            uniform: camera.uniform(size, radius),
        },
    ));
    if let Some(drag) = &tool.drag {
        if matches!(drag.snap, Some(DragSnap::Grid))
            && let Some(pose) = drag.last_pose
            && let Some(point) =
                camera.project([pose.translation_mm[0], pose.translation_mm[1], 0.0], rect)
            && rect.contains(point)
        {
            // At distant zoom levels the rendered grid omits fine lines. Mark
            // the exact candidate intersection so the target remains visible.
            let painter = ui.painter().with_clip_rect(rect);
            let color = egui::Color32::from_rgb(240, 72, 170);
            painter.circle_stroke(point, 7.0, egui::Stroke::new(2.0, color));
            painter.line_segment(
                [
                    point + egui::vec2(-11.0, 0.0),
                    point + egui::vec2(11.0, 0.0),
                ],
                egui::Stroke::new(2.0, color),
            );
            painter.line_segment(
                [
                    point + egui::vec2(0.0, -11.0),
                    point + egui::vec2(0.0, 11.0),
                ],
                egui::Stroke::new(2.0, color),
            );
        }
        let label = if let Some(DragSnap::Face(snap)) = drag.snap {
            let name = project
                .boards
                .iter()
                .find(|b| b.id == snap.target_id)
                .map_or("—", |b| b.name.as_str());
            if pt {
                format!("Encaixe: {name} · solte para aceitar")
            } else {
                format!("Snap: {name} · release to accept")
            }
        } else if let Some(DragSnap::Grid) = drag.snap {
            if pt {
                "Grade XY · solte para aceitar".into()
            } else {
                "XY grid · release to accept".into()
            }
        } else if ui.input(|i| i.modifiers.alt) {
            if pt {
                "Encaixe ignorado · solte para aceitar".into()
            } else {
                "Snap bypassed · release to accept".into()
            }
        } else if pt {
            "Posição livre · solte para aceitar".into()
        } else {
            "Free position · release to accept".into()
        };
        ui.painter().text(
            rect.left_top() + egui::vec2(12.0, 12.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(15.0),
            egui::Color32::WHITE,
        );
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::commands::ProjectEditor;
    use plan_my_cabinet::domain::{Assembly, Board, BoardGrain, Material};
    use plan_my_cabinet::i18n::Language;
    use plan_my_cabinet::money::Currency;
    use plan_my_cabinet::units::{Length, Pose, Quaternion};

    #[test]
    fn scene_override_moves_only_target_mesh_and_exit_recovers_identical_closed_mesh() {
        let mut project = Project::new("preview", Currency::Brl);
        let moving = Uuid::new_v4();
        let fixed = Uuid::new_v4();
        project.boards.push(board(moving, [20.0, 0.0, 0.0], None));
        project.boards.push(board(fixed, [200.0, 0.0, 0.0], None));
        let camera = Camera::default();
        let selection = Selection::default();
        let (closed, _) = scene_with_faces(&project, &camera, &selection, None, None);
        let poses = HashMap::from([(
            moving,
            Pose::new([50.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
        )]);
        let (open, _) = scene_with_faces(&project, &camera, &selection, None, Some(&poses));
        // Grid and axes precede the two board boxes; only the first box changes.
        let board_floats = 36 * 6;
        assert_ne!(open.faces[..board_floats], closed.faces[..board_floats]);
        assert_eq!(open.faces[board_floats..], closed.faces[board_floats..]);
        assert_eq!(
            scene_with_faces(&project, &camera, &selection, None, None)
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
            enabled: true,
            ..Default::default()
        };
        let poses = HashMap::from([(id, Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap())]);
        let mut frame = |events| {
            let mut action = None;
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
                            None,
                            Some(&poses),
                        );
                    });
                },
            )
            .drop_without_applying_deltas();
            assert!(action.is_none());
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
    fn rectangular_board_faces_have_full_area_and_edges_follow_box_axes() {
        let corners = std::array::from_fn(|i| {
            [
                if i & 1 != 0 { 100.0 } else { 0.0 },
                if i & 2 != 0 { 50.0 } else { 0.0 },
                if i & 4 != 0 { 18.0 } else { 0.0 },
            ]
        });
        let mut mesh = Mesh::default();
        mesh.box_mesh(corners, [1.0; 3]);
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
            enabled: true,
            ..Default::default()
        };
        fn frame(
            ctx: &egui::Context,
            camera: &mut Camera,
            editor: &mut ProjectEditor,
            selection: &mut Selection,
            tool: &mut MoveTool,
            events: Vec<egui::Event>,
        ) {
            let mut result = None;
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
                        result = show_move(
                            ui,
                            camera,
                            editor.preview().unwrap_or(editor.project()),
                            selection,
                            tool,
                            false,
                            Language::En,
                            None,
                            None,
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
        draw!(vec![]);
        let pos = egui::pos2(400.0, 330.0);
        let moved = egui::pos2(455.0, 330.0);
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
        assert_ne!(mesh.faces, highlighted.faces);
        selection.hidden.insert(id);
        assert_eq!(
            pick_visible(&project, &camera, pointer, rect, &selection),
            None
        );
        let (hidden, _) = scene(&project, &camera, &selection);
        assert!(hidden.faces.is_empty());
    }
}
