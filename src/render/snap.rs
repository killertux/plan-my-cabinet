//! Measure-tool snapping: exact corners, edge midpoints, face centres, edges
//! and faces of the scene's boxes, found in screen space and hidden by nearer
//! boxes. Presentation-only; nothing here edits the project.
use eframe::egui;
use uuid::Uuid;

use super::camera::{Camera, Projection, Ray, Selection, add_scaled, dot, world_pose};
use crate::domain::Project;
use crate::units::{Pose, Quaternion};

/// Screen radius within which a corner, midpoint or face centre attracts the pointer.
pub const POINT_APERTURE_PX: f32 = 12.0;
/// Screen radius within which an edge attracts the pointer.
pub const EDGE_APERTURE_PX: f32 = 8.0;
/// Screen radius within which a world axis through the first point attracts the pointer.
pub const AXIS_APERTURE_PX: f32 = 10.0;
/// A feature this close behind the nearest surface still counts as visible.
const DEPTH_TOLERANCE_MM: f64 = 0.05;
/// A point this close to an axis line through the first point lies on it.
const ON_AXIS_MM: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
    FaceCenter,
    OnEdge,
    OnFace,
    /// On the locked or inferred direction through the first point.
    OnAxis,
    Ground,
    Free,
}

impl SnapKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Endpoint => "measure-snap-endpoint",
            Self::Midpoint => "measure-snap-midpoint",
            Self::FaceCenter => "measure-snap-face-center",
            Self::OnEdge => "measure-snap-edge",
            Self::OnFace => "measure-snap-face",
            Self::OnAxis => "measure-snap-axis",
            Self::Ground => "measure-snap-ground",
            Self::Free => "measure-snap-free",
        }
    }

    /// Exact features of an edge or corner, which a locked direction projects from.
    pub fn is_feature(self) -> bool {
        matches!(
            self,
            Self::Endpoint | Self::Midpoint | Self::FaceCenter | Self::OnEdge
        )
    }

    /// Pixels added to the pointer distance, so a corner wins over a nearby
    /// midpoint or face centre unless the pointer is clearly on the latter.
    fn bias(self) -> f32 {
        match self {
            Self::Endpoint => 0.0,
            Self::Midpoint => 3.0,
            Self::FaceCenter => 5.0,
            _ => 0.0,
        }
    }
}

/// A point fixed in an object's local frame, so a kept measurement follows edits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub id: Uuid,
    pub local_mm: [f64; 3],
}

/// A measured end: attached to a board or hardware item, or fixed in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeasurePoint {
    World([f64; 3]),
    On(Anchor),
}

impl MeasurePoint {
    /// The current world position; `None` once the anchoring object is gone.
    pub fn resolve(self, project: &Project) -> Option<[f64; 3]> {
        match self {
            Self::World(point) => Some(point),
            Self::On(anchor) => object_pose(project, anchor.id)?
                .transform_point(anchor.local_mm)
                .ok(),
        }
    }
}

fn object_pose(project: &Project, id: Uuid) -> Option<Pose> {
    if let Some(board) = project.boards.iter().find(|b| b.id == id) {
        return world_pose(project, board);
    }
    project
        .hardware
        .iter()
        .any(|h| h.id == id)
        .then(|| crate::assembly_edit::world_pose(project, id).ok())
        .flatten()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub point: [f64; 3],
    pub kind: SnapKind,
    pub anchor: Option<Anchor>,
    /// The world axis (0 = X, 1 = Y, 2 = Z) the point lies on from the first point.
    pub axis: Option<usize>,
    /// Unit direction of the edge under a midpoint or on-edge snap.
    pub edge: Option<[f64; 3]>,
    /// The hovered feature a locked direction projected this point from.
    pub source: Option<[f64; 3]>,
}

impl Snap {
    fn at(point: [f64; 3], kind: SnapKind) -> Self {
        Self {
            point,
            kind,
            anchor: None,
            axis: None,
            edge: None,
            source: None,
        }
    }

    pub fn measure_point(&self) -> MeasurePoint {
        self.anchor
            .map_or(MeasurePoint::World(self.point), MeasurePoint::On)
    }
}

/// One pickable box: a board, a dimensioned hardware item, or a fitting part.
#[derive(Clone, Copy, Debug)]
pub struct SceneBox {
    pub id: Uuid,
    pub pose: Pose,
    pub size: [f64; 3],
    /// Boards and hardware own their pose; fitting parts are derived, so
    /// points on them are kept in world coordinates.
    pub anchored: bool,
}

/// Every visible box the viewport picks: boards, dimensioned hardware, and
/// drawer slides and hinges by the boxes they occupy.
pub fn scene_boxes(project: &Project, selection: &Selection) -> Vec<SceneBox> {
    let mm = |v: crate::units::Length| v.micrometres() as f64 / 1000.0;
    let boards = project
        .boards
        .iter()
        .filter(|board| selection.visible(project, board.id))
        .filter_map(|board| {
            Some(SceneBox {
                id: board.id,
                pose: world_pose(project, board)?,
                size: board.blank_dimensions().map(mm),
                anchored: true,
            })
        });
    let hardware = project
        .hardware
        .iter()
        .filter(|h| selection.visible(project, h.id))
        .filter_map(|h| {
            Some(SceneBox {
                id: h.id,
                pose: crate::assembly_edit::world_pose(project, h.id).ok()?,
                size: project.hardware_dimensions(h)?.map(mm),
                anchored: true,
            })
        });
    let fittings = super::hardware_mesh::pick_boxes(project, selection, None)
        .into_iter()
        .flat_map(|(id, _, boxes)| {
            boxes.into_iter().map(move |(pose, size)| SceneBox {
                id,
                pose,
                size,
                anchored: false,
            })
        });
    boards.chain(hardware).chain(fittings).collect()
}

fn inverse(q: Quaternion) -> Quaternion {
    Quaternion {
        w: q.w,
        x: -q.x,
        y: -q.y,
        z: -q.z,
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn length(v: [f64; 3]) -> f64 {
    dot(v, v).sqrt()
}

fn to_local(pose: Pose, point: [f64; 3]) -> [f64; 3] {
    inverse(pose.rotation).rotate(sub(point, pose.translation_mm))
}

/// Return the closest positive surface crossing, including exit faces when the eye is inside.
/// Each face is tested independently so parallel rays and edge hits have stable results.
pub fn board_hit(ray: Ray, pose: Pose, dimensions: [f64; 3]) -> Option<(f64, usize)> {
    let inverse = inverse(pose.rotation);
    let origin = inverse.rotate(sub(ray.origin, pose.translation_mm));
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

/// The nearest box the ray crosses: `(distance, box index, face)`. Ties go to the lower id.
pub fn nearest_hit(boxes: &[SceneBox], ray: Ray) -> Option<(f64, usize, usize)> {
    boxes
        .iter()
        .enumerate()
        .filter_map(|(index, b)| board_hit(ray, b.pose, b.size).map(|(t, face)| (t, index, face)))
        .min_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then_with(|| boxes[a.1].id.cmp(&boxes[b.1].id))
        })
}

/// The ray from the eye through `point` and the point's distance along it.
fn eye_ray(camera: &Camera, point: [f64; 3]) -> (Ray, f64) {
    let (_, _, forward) = camera.basis();
    let eye = add_scaled(camera.target, forward, -camera.distance);
    match camera.projection {
        Projection::Perspective => {
            let offset = sub(point, eye);
            let depth = length(offset);
            let direction = if depth > 0.0 {
                offset.map(|v| v / depth)
            } else {
                forward
            };
            (
                Ray {
                    origin: eye,
                    direction,
                },
                depth,
            )
        }
        Projection::Orthographic => {
            let depth = dot(sub(point, eye), forward);
            (
                Ray {
                    origin: add_scaled(point, forward, -depth),
                    direction: forward,
                },
                depth,
            )
        }
    }
}

/// Whether nothing nearer than `point` hides it from the eye.
pub fn unobstructed(boxes: &[SceneBox], camera: &Camera, point: [f64; 3]) -> bool {
    let (ray, depth) = eye_ray(camera, point);
    depth > 0.0
        && nearest_hit(boxes, ray)
            .is_none_or(|(t, _, _)| t >= depth - DEPTH_TOLERANCE_MM.max(depth * 1e-9))
}

/// Pairs of corner indices (bit 0 = +X, bit 1 = +Y, bit 2 = +Z) joined by an edge.
const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (2, 3),
    (4, 5),
    (6, 7),
    (0, 2),
    (1, 3),
    (4, 6),
    (5, 7),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

fn local_corner(size: [f64; 3], index: usize) -> [f64; 3] {
    std::array::from_fn(|axis| {
        if index & (1 << axis) == 0 {
            0.0
        } else {
            size[axis]
        }
    })
}

/// Parameter `s` of the point on `start + s·direction` closest to the ray, or
/// `None` when they are parallel.
fn closest_on_line(start: [f64; 3], direction: [f64; 3], ray: Ray) -> Option<f64> {
    let w = sub(start, ray.origin);
    let a = dot(direction, direction);
    let b = dot(direction, ray.direction);
    let c = dot(ray.direction, ray.direction);
    let denominator = a * c - b * b;
    if denominator <= 1e-12 * a * c {
        return None;
    }
    Some((b * dot(ray.direction, w) - c * dot(direction, w)) / denominator)
}

/// The best exact feature near the pointer: a corner, midpoint or face centre
/// within [`POINT_APERTURE_PX`], else an edge within [`EDGE_APERTURE_PX`],
/// else the face under the pointer. Hidden features are skipped.
pub fn snap_point(
    boxes: &[SceneBox],
    camera: &Camera,
    rect: egui::Rect,
    pointer: egui::Pos2,
) -> Option<Snap> {
    let ray = camera.ray(pointer, rect);
    let screen = |point| camera.project(point, rect).filter(|s| rect.contains(*s));
    let mut points = Vec::new();
    let mut edges = Vec::new();
    for b in boxes {
        let world = |local| b.pose.transform_point(local).ok();
        let feature = |local, kind, edge| {
            let point = world(local)?;
            let distance = screen(point)?.distance(pointer);
            Some((
                distance,
                Snap {
                    anchor: b.anchored.then_some(Anchor {
                        id: b.id,
                        local_mm: local,
                    }),
                    edge,
                    ..Snap::at(point, kind)
                },
            ))
        };
        let edge_direction = |axis: usize| {
            let mut local = [0.0; 3];
            local[axis] = 1.0;
            b.pose.rotation.rotate(local)
        };
        for index in 0..8 {
            points.extend(feature(
                local_corner(b.size, index),
                SnapKind::Endpoint,
                None,
            ));
        }
        for (from, to) in EDGES {
            let (a, c) = (local_corner(b.size, from), local_corner(b.size, to));
            let axis = (from ^ to).trailing_zeros() as usize;
            let middle = std::array::from_fn(|i| (a[i] + c[i]) / 2.0);
            points.extend(feature(
                middle,
                SnapKind::Midpoint,
                Some(edge_direction(axis)),
            ));
            let (Some(start), Some(end)) = (world(a), world(c)) else {
                continue;
            };
            let Some(s) = closest_on_line(start, sub(end, start), ray) else {
                continue;
            };
            let s = s.clamp(0.0, 1.0);
            let local = std::array::from_fn(|i| a[i] + (c[i] - a[i]) * s);
            if let Some((distance, snap)) =
                feature(local, SnapKind::OnEdge, Some(edge_direction(axis)))
            {
                edges.push((distance, snap));
            }
        }
        for axis in 0..3 {
            for side in [0.0, b.size[axis]] {
                let mut local = b.size.map(|v| v / 2.0);
                local[axis] = side;
                points.extend(feature(local, SnapKind::FaceCenter, None));
            }
        }
    }
    points.retain(|(distance, _)| *distance <= POINT_APERTURE_PX);
    points.sort_by(|a, b| (a.0 + a.1.kind.bias()).total_cmp(&(b.0 + b.1.kind.bias())));
    edges.retain(|(distance, _)| *distance <= EDGE_APERTURE_PX);
    edges.sort_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, snap)) = points
        .into_iter()
        .chain(edges)
        .find(|(_, snap)| unobstructed(boxes, camera, snap.point))
    {
        return Some(snap);
    }
    face_point(boxes, ray)
}

/// The surface point under the ray, attached to its box when it can be.
fn face_point(boxes: &[SceneBox], ray: Ray) -> Option<Snap> {
    let (t, index, _) = nearest_hit(boxes, ray)?;
    let b = boxes[index];
    let point = add_scaled(ray.origin, ray.direction, t);
    Some(Snap {
        anchor: b.anchored.then(|| Anchor {
            id: b.id,
            local_mm: to_local(b.pose, point),
        }),
        ..Snap::at(point, SnapKind::OnFace)
    })
}

/// Where the ray meets the plane through `through` that faces the camera.
fn view_plane_point(camera: &Camera, ray: Ray, through: [f64; 3]) -> [f64; 3] {
    let (_, _, forward) = camera.basis();
    let t = dot(sub(through, ray.origin), forward) / dot(ray.direction, forward);
    add_scaled(
        ray.origin,
        ray.direction,
        if t.is_finite() { t } else { 0.0 },
    )
}

/// A point off the scene: the floor (Z = 0) for a first point seen from
/// above, else the camera-facing plane through `from` or the view target.
fn fallback(camera: &Camera, ray: Ray, from: Option<[f64; 3]>) -> Snap {
    if let Some(from) = from {
        return Snap::at(view_plane_point(camera, ray, from), SnapKind::Free);
    }
    let t = -ray.origin[2] / ray.direction[2];
    if t.is_finite() && t > 0.0 {
        let mut point = add_scaled(ray.origin, ray.direction, t);
        point[2] = 0.0;
        return Snap::at(point, SnapKind::Ground);
    }
    Snap::at(view_plane_point(camera, ray, camera.target), SnapKind::Free)
}

/// How the pointer is constrained while placing a measurement point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Constraint {
    /// The first point, once placed.
    pub from: Option<[f64; 3]>,
    /// A locked unit direction through `from`.
    pub lock: Option<[f64; 3]>,
    /// Alt: no snapping or inference, only the surface under the pointer.
    pub free: bool,
}

fn unit(axis: usize) -> [f64; 3] {
    std::array::from_fn(|i| f64::from(u8::from(i == axis)))
}

/// The world axis a unit direction runs along, if any.
pub fn axis_of(direction: [f64; 3]) -> Option<usize> {
    (0..3).find(|&axis| direction[axis].abs() >= 1.0 - 1e-9)
}

/// The point a measurement would use under the pointer: a snapped feature,
/// constrained to a locked direction or tagged with the world axis it shares
/// with the first point.
pub fn measure_target(
    boxes: &[SceneBox],
    camera: &Camera,
    rect: egui::Rect,
    pointer: egui::Pos2,
    constraint: Constraint,
) -> Snap {
    let ray = camera.ray(pointer, rect);
    let base = if constraint.free {
        face_point(boxes, ray)
    } else {
        snap_point(boxes, camera, rect, pointer)
    }
    .unwrap_or_else(|| fallback(camera, ray, constraint.from));
    let Some(from) = constraint.from else {
        return base;
    };
    if let Some(direction) = constraint.lock {
        let axis = axis_of(direction);
        if !constraint.free && base.kind.is_feature() {
            // A hovered feature is projected onto the locked line, so a gap is
            // measured exactly along it without hitting a matching corner.
            let foot = add_scaled(from, direction, dot(sub(base.point, from), direction));
            return Snap {
                axis,
                source: (length(sub(base.point, foot)) > ON_AXIS_MM).then_some(base.point),
                ..Snap::at(foot, base.kind)
            };
        }
        let s = closest_on_line(from, direction, ray).unwrap_or(0.0);
        return Snap {
            axis,
            ..Snap::at(add_scaled(from, direction, s), SnapKind::OnAxis)
        };
    }
    if constraint.free {
        return base;
    }
    let offset = sub(base.point, from);
    if length(offset) > ON_AXIS_MM
        && let Some(axis) = (0..3).find(|&axis| {
            (0..3)
                .filter(|&i| i != axis)
                .all(|i| offset[i].abs() <= ON_AXIS_MM)
        })
    {
        return Snap {
            axis: Some(axis),
            ..base
        };
    }
    if matches!(
        base.kind,
        SnapKind::Endpoint | SnapKind::Midpoint | SnapKind::FaceCenter
    ) {
        return base;
    }
    let Some(origin) = camera.project(from, rect) else {
        return base;
    };
    (0..3)
        .filter_map(|axis| {
            // An axis pointing at the viewer collapses on screen; skip it.
            let along = camera.project(add_scaled(from, unit(axis), 100.0), rect)?;
            if along.distance(origin) < 4.0 {
                return None;
            }
            let s = closest_on_line(from, unit(axis), ray)?;
            let point = add_scaled(from, unit(axis), s);
            let distance = camera.project(point, rect)?.distance(pointer);
            (distance <= AXIS_APERTURE_PX).then_some((distance, axis, point))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(base, |(_, axis, point)| Snap {
            axis: Some(axis),
            ..Snap::at(point, SnapKind::OnAxis)
        })
}

#[cfg(test)]
mod tests;
