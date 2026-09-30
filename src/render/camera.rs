//! Presentation-only camera, selection/visibility and world geometry shared by
//! the desktop viewport, the saved thumbnail and headless renders.
use eframe::egui;
use std::collections::HashSet;
use uuid::Uuid;

use crate::domain::{Board, HardwareKind, Project};

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
pub struct Ray {
    pub origin: [f64; 3],
    pub direction: [f64; 3],
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}

pub fn add_scaled(a: [f64; 3], b: [f64; 3], scale: f64) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i] * scale)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Isometric,
    Front,
    Right,
    Top,
    Free,
}

pub struct Camera {
    pub target: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
    pub distance: f64,
    pub projection: Projection,
    pub preset: Preset,
    pub viewport_aspect: f64,
    pub viewport_height: f64,
    pub pending_frame: bool,
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

pub const FOV: f64 = std::f64::consts::FRAC_PI_4;
pub const MIN_DISTANCE: f64 = 0.01;
pub const MAX_DISTANCE: f64 = 1.0e9;

impl Camera {
    /// Defer a new-project fit until the target workspace has a real canvas.
    pub fn request_frame(&mut self) {
        self.pending_frame = true;
    }

    #[doc(hidden)]
    pub fn framed_target(&self) -> Option<[f64; 3]> {
        (!self.pending_frame).then_some(self.target)
    }

    #[doc(hidden)]
    pub fn navigation_state(&self) -> ([f64; 3], f64, f64, f64) {
        (self.target, self.distance, self.yaw, self.pitch)
    }

    #[doc(hidden)]
    pub fn assert_framed_occupancy(&self, project: &Project, selection: &Selection) {
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
                for corner in board_corners(project, board).expect("framed board") {
                    let point = self.project(corner, rect).expect("projectable corner");
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

    pub fn project(&self, point: [f64; 3], rect: egui::Rect) -> Option<egui::Pos2> {
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

    pub fn drag_pose(
        &self,
        start: egui::Pos2,
        pointer: egui::Pos2,
        rect: egui::Rect,
        world: crate::units::Pose,
    ) -> Option<crate::units::Pose> {
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
        crate::units::Pose::new(translation, world.rotation).ok()
    }
    pub fn ray(&self, pointer: egui::Pos2, rect: egui::Rect) -> Ray {
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

    pub fn basis(&self) -> ([f64; 3], [f64; 3], [f64; 3]) {
        let (s, c) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let forward = [-cp * c, -cp * s, -sp];
        let right = [-s, c, 0.0];
        let up = [-sp * c, -sp * s, cp];
        (right, up, forward)
    }

    pub fn orbit(&mut self, dx: f64, dy: f64) {
        self.yaw = (self.yaw - dx * 0.006).rem_euclid(std::f64::consts::TAU);
        self.pitch = (self.pitch + dy * 0.006).clamp(-1.55, 1.55);
        self.preset = Preset::Free;
    }

    pub fn pan(&mut self, dx: f64, dy: f64, height: f64) {
        let (right, up, _) = self.basis();
        let scale = 2.0 * self.distance * (FOV / 2.0).tan() / height.max(1.0);
        for i in 0..3 {
            self.target[i] += (-dx * right[i] + dy * up[i]) * scale;
        }
    }

    pub fn zoom(&mut self, factor: f64) {
        if factor.is_finite() && factor > 0.0 {
            self.distance = (self.distance / factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
        }
    }

    pub fn set_preset(&mut self, preset: Preset) {
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

    pub fn frame(&mut self, bounds: Bounds, aspect: f64) {
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

    pub fn uniform(&self, size: [u32; 2], radius: f64) -> Vec<u8> {
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
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds {
    pub fn empty() -> Self {
        Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }
    pub fn include(&mut self, p: [f64; 3]) {
        for (i, value) in p.into_iter().enumerate() {
            self.min[i] = self.min[i].min(value);
            self.max[i] = self.max[i].max(value);
        }
    }
    pub fn valid(&self) -> bool {
        self.min[0].is_finite()
    }
    pub fn center(&self) -> [f64; 3] {
        std::array::from_fn(|i| (self.min[i] + self.max[i]) / 2.0)
    }
    pub fn radius(&self) -> f64 {
        self.min
            .iter()
            .zip(self.max)
            .map(|(a, b)| ((b - a) / 2.0).powi(2))
            .sum::<f64>()
            .sqrt()
    }
}

pub fn world_pose(project: &Project, board: &Board) -> Option<crate::units::Pose> {
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

pub fn board_corners(project: &Project, board: &Board) -> Option<[[f64; 3]; 8]> {
    let pose = world_pose(project, board)?;
    let dimensions = board
        .blank_dimensions()
        .map(|v| v.micrometres() as f64 / 1000.0);
    box_corners(pose, dimensions)
}

pub fn box_corners(pose: crate::units::Pose, [x, y, z]: [f64; 3]) -> Option<[[f64; 3]; 8]> {
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

pub fn bounds_visible(
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
        if let Ok(pose) = crate::assembly_edit::world_pose(project, hardware.id)
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

pub fn selected_board(project: &Project, selected: &HashSet<Uuid>, board_id: Uuid) -> bool {
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

pub fn selected_ancestor(
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
