//! Triangle geometry for catalog hardware that is not a plain box: feet
//! shaped like the product (tapered plastic feet, posts with a plate and a
//! glide, tube frames) and drawer slides as three telescoping members.
//!
//! Shading comes from each surface's normal, so round parts read as smooth;
//! edge lines are drawn only at rims and outlines, never along facet seams.
use std::collections::HashMap;

use uuid::Uuid;

use crate::domain::{FootShape, FootSpec, Project, Section};
use crate::render::camera::{Selection, box_corners};
use crate::render::mesh::Mesh;
use crate::render::mesh::relative;
use crate::units::Pose;

/// Segments around a round part.
const SEGMENTS: usize = 32;
/// Polyurethane glides and similar dark parts, whatever the foot's color.
const GLIDE: [f32; 3] = [0.13, 0.13, 0.14];
/// Nickel-plated hinge, and the tint used when the hinge has issues.
const NICKEL: [f32; 3] = [0.72, 0.73, 0.75];
const HINGE_ISSUE: [f32; 3] = [0.93, 0.66, 0.25];
/// How far the cup rim stands proud of the door face, so it stays visible.
const CUP_LIP_MM: f64 = 1.5;

/// Zinc-plated steel, per slide member.
const ZINC: [[f32; 3]; 3] = [[0.70, 0.72, 0.74], [0.60, 0.62, 0.65], [0.80, 0.82, 0.84]];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolidKind {
    Foot,
    Slide,
    /// A hinge cup and plate: representational only, never drilling data.
    Hinge,
}

/// One triangle in world millimetres with a shade per vertex. `color`
/// overrides the solid's color for fixed-color parts (glides, slide members).
#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub points: [[f64; 3]; 3],
    pub shade: [f32; 3],
    pub color: Option<[f32; 3]>,
}

/// A drawable object: triangles, outline edges and the boxes it occupies (for
/// bounds, framing and picking).
#[derive(Clone, Debug)]
pub struct Solid {
    pub id: Uuid,
    pub kind: SolidKind,
    pub boxes: Vec<(Pose, [f64; 3])>,
    pub tris: Vec<Tri>,
    pub edges: Vec<[[f64; 3]; 2]>,
    pub base: [f32; 3],
}

impl Solid {
    /// Corners of the boxes the solid occupies.
    pub fn points(&self) -> Vec<[f64; 3]> {
        self.boxes
            .iter()
            .filter_map(|(pose, size)| box_corners(*pose, *size))
            .flatten()
            .collect()
    }
}

/// Soft hemisphere light: brightest from above-front-right, never black.
fn shade(normal: [f64; 3]) -> f32 {
    const LIGHT: [f64; 3] = [0.35, -0.5, 0.8];
    let len = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let (ln, nn) = (len(LIGHT), len(normal));
    if nn < 1e-12 {
        return 0.8;
    }
    let d = (0..3).map(|i| LIGHT[i] * normal[i]).sum::<f64>() / (ln * nn);
    (0.55 + 0.45 * (0.5 + 0.5 * d)) as f32
}

/// A local triangle: corners, vertex normals and an optional fixed color.
type LocalTri = ([[f64; 3]; 3], [[f64; 3]; 3], Option<[f32; 3]>);

/// Geometry in a part's local frame, placed in the world afterwards.
#[derive(Default)]
struct Builder {
    tris: Vec<LocalTri>,
    edges: Vec<[[f64; 3]; 2]>,
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-12 { v } else { v.map(|c| c / len) }
}

impl Builder {
    fn flat(&mut self, points: [[f64; 3]; 3], color: Option<[f32; 3]>) {
        let n = normalize(cross(sub(points[1], points[0]), sub(points[2], points[0])));
        self.tris.push((points, [n; 3], color));
    }

    fn quad(&mut self, q: [[f64; 3]; 4], color: Option<[f32; 3]>) {
        self.flat([q[0], q[1], q[2]], color);
        self.flat([q[0], q[2], q[3]], color);
    }

    fn outline(&mut self, points: &[[f64; 3]]) {
        for i in 0..points.len() {
            self.edges.push([points[i], points[(i + 1) % points.len()]]);
        }
    }

    /// A box from 8 corners in bit order (bit 0 = +u, 1 = +v, 2 = +w), with
    /// outward faces and all 12 edges.
    fn hexahedron(&mut self, c: [[f64; 3]; 8], color: Option<[f32; 3]>) {
        // Each face listed counter-clockwise seen from outside.
        for face in [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ] {
            self.quad(face.map(|i| c[i]), color);
        }
        for (a, b) in [
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
        ] {
            self.edges.push([c[a], c[b]]);
        }
    }

    /// An axis-aligned rectangular frustum centred on `(cx, cy)`.
    fn rect_frustum(
        &mut self,
        centre: [f64; 2],
        z: [f64; 2],
        bottom: [f64; 2],
        top: [f64; 2],
        color: Option<[f32; 3]>,
    ) {
        let corner = |i: usize| {
            let (size, zz) = if i & 4 == 0 {
                (bottom, z[0])
            } else {
                (top, z[1])
            };
            [
                centre[0] + if i & 1 == 0 { -size[0] } else { size[0] } / 2.0,
                centre[1] + if i & 2 == 0 { -size[1] } else { size[1] } / 2.0,
                zz,
            ]
        };
        self.hexahedron(std::array::from_fn(corner), color);
    }

    /// A round frustum (cylinder when both radii match) with smooth sides,
    /// flat caps and rim edges only.
    fn frustum(
        &mut self,
        centre: [f64; 2],
        z: [f64; 2],
        radius: [f64; 2],
        color: Option<[f32; 3]>,
    ) {
        let slope = (radius[0] - radius[1]) / (z[1] - z[0]).max(1e-9);
        let ring = |r: f64, zz: f64| -> Vec<[f64; 3]> {
            (0..SEGMENTS)
                .map(|k| {
                    let a = std::f64::consts::TAU * k as f64 / SEGMENTS as f64;
                    [centre[0] + r * a.cos(), centre[1] + r * a.sin(), zz]
                })
                .collect()
        };
        let (low, high) = (ring(radius[0], z[0]), ring(radius[1], z[1]));
        let normal = |k: usize| {
            let a = std::f64::consts::TAU * k as f64 / SEGMENTS as f64;
            normalize([a.cos(), a.sin(), slope])
        };
        for k in 0..SEGMENTS {
            let j = (k + 1) % SEGMENTS;
            let (nk, nj) = (normal(k), normal(j));
            self.tris
                .push(([low[k], low[j], high[j]], [nk, nj, nj], color));
            self.tris
                .push(([low[k], high[j], high[k]], [nk, nj, nk], color));
            let (cb, ct) = ([centre[0], centre[1], z[0]], [centre[0], centre[1], z[1]]);
            self.tris
                .push(([cb, low[j], low[k]], [[0.0, 0.0, -1.0]; 3], color));
            self.tris
                .push(([ct, high[k], high[j]], [[0.0, 0.0, 1.0]; 3], color));
        }
        self.outline(&low);
        self.outline(&high);
    }

    fn section(
        &mut self,
        centre: [f64; 2],
        z: [f64; 2],
        bottom: Section,
        top: Section,
        color: Option<[f32; 3]>,
    ) {
        let size = |s: Section| s.size().map(|v| v.micrometres() as f64 / 1000.0);
        if bottom.is_round() && top.is_round() {
            self.frustum(
                centre,
                z,
                [size(bottom)[0] / 2.0, size(top)[0] / 2.0],
                color,
            );
        } else {
            self.rect_frustum(centre, z, size(bottom), size(top), color);
        }
    }

    /// A rectangular tube from `a` to `b` in the local X–Z plane, `width`
    /// across the tube in that plane and `depth` along Y, centred on `y`.
    fn tube(&mut self, a: [f64; 2], b: [f64; 2], y: f64, width: f64, depth: f64) {
        let u = normalize([b[0] - a[0], 0.0, b[1] - a[1]]);
        // Across the tube, in the X–Z plane.
        let w = [-u[2], 0.0, u[0]];
        let corner = |i: usize| {
            let end = if i & 1 == 0 { a } else { b };
            let side = if i & 2 == 0 { -0.5 } else { 0.5 } * width;
            let dy = if i & 4 == 0 { -0.5 } else { 0.5 } * depth;
            [end[0] + w[0] * side, y + dy, end[1] + w[2] * side]
        };
        let c: [[f64; 3]; 8] = std::array::from_fn(corner);
        // Keep faces outward whatever the tube's direction.
        let flipped = {
            let ex = sub(c[1], c[0]);
            let ey = sub(c[2], c[0]);
            let ez = sub(c[4], c[0]);
            (0..3).map(|i| cross(ex, ey)[i] * ez[i]).sum::<f64>() < 0.0
        };
        let c = if flipped {
            [c[0], c[2], c[1], c[3], c[4], c[6], c[5], c[7]]
        } else {
            c
        };
        self.hexahedron(c, None);
    }

    fn place(self, pose: Pose) -> (Vec<Tri>, Vec<[[f64; 3]; 2]>) {
        let world = |p: [f64; 3]| {
            let r = pose.rotation.rotate(p);
            std::array::from_fn(|i| pose.translation_mm[i] + r[i])
        };
        let tris = self
            .tris
            .into_iter()
            .map(|(points, normals, color)| Tri {
                points: points.map(world),
                shade: normals.map(|n| shade(pose.rotation.rotate(n))),
                color,
            })
            .collect();
        let edges = self.edges.into_iter().map(|e| e.map(world)).collect();
        (tris, edges)
    }
}

fn mm(length: crate::units::Length) -> f64 {
    length.micrometres() as f64 / 1000.0
}

/// Foot geometry in its local frame: box from the origin, Z up, mounting face
/// on top.
fn foot_builder(spec: &FootSpec) -> Builder {
    let size = spec.local_size().map(mm);
    let centre = [size[0] / 2.0, size[1] / 2.0];
    let mut b = Builder::default();
    match &spec.shape {
        FootShape::Tapered {
            top,
            bottom,
            height,
        } => {
            b.section(centre, [0.0, mm(*height)], *bottom, *top, None);
        }
        FootShape::Post {
            tube,
            plate,
            plate_thickness,
            glide,
            height,
        } => {
            let h = mm(*height);
            let plate_bottom = h - mm(*plate_thickness);
            let tube_bottom = glide.map_or(0.0, |g| mm(g.height));
            b.section(centre, [plate_bottom, h], *plate, *plate, None);
            b.section(centre, [tube_bottom, plate_bottom], *tube, *tube, None);
            if let Some(g) = glide {
                let r = mm(g.diameter) / 2.0;
                // A steel stem and a dark pad, like a levelling glide.
                let pad = (mm(g.height) * 0.45).max(1.0).min(mm(g.height));
                b.frustum(centre, [0.0, pad], [r, r], Some(GLIDE));
                if pad < mm(g.height) {
                    let stem = (r * 0.45).max(2.0);
                    b.frustum(centre, [pad, mm(g.height)], [r * 0.8, stem], None);
                }
            }
        }
        FootShape::Frame {
            height,
            top_width,
            bottom_width,
            tube_width,
            tube_depth,
            crossbar_height,
            glide,
        } => {
            let (w, h) = (size[0], mm(*height));
            let (tw, td) = (mm(*tube_width), mm(*tube_depth));
            let (top, bottom) = (mm(*top_width), mm(*bottom_width));
            let floor = glide.map_or(0.0, |g| mm(g.height));
            let y = centre[1];
            let top_x = [(w - top) / 2.0, (w + top) / 2.0];
            let bottom_x = [(w - bottom) / 2.0, (w + bottom) / 2.0];
            // Top and bottom bars.
            b.tube(
                [top_x[0], h - tw / 2.0],
                [top_x[1], h - tw / 2.0],
                y,
                tw,
                td,
            );
            if bottom > 2.0 * tw {
                b.tube(
                    [bottom_x[0], floor + tw / 2.0],
                    [bottom_x[1], floor + tw / 2.0],
                    y,
                    tw,
                    td,
                );
            }
            // Legs between the bars' ends (vertical for a rectangle).
            let legs = [
                (
                    [top_x[0] + tw / 2.0, h - tw],
                    [bottom_x[0] + tw / 2.0, floor + tw],
                ),
                (
                    [top_x[1] - tw / 2.0, h - tw],
                    [bottom_x[1] - tw / 2.0, floor + tw],
                ),
            ];
            if bottom > 2.0 * tw {
                for (a, c) in legs {
                    b.tube(a, c, y, tw, td);
                }
            } else {
                // A V: both legs meet at the floor. Ending half a tube up
                // keeps the slanted tube's corners above the floor.
                for (a, _) in legs {
                    b.tube(a, [w / 2.0, floor + tw / 2.0], y, tw, td);
                }
            }
            if let Some(ch) = crossbar_height {
                let z = mm(*ch);
                let t = ((h - tw) - z) / ((h - tw) - (floor + tw)).max(1e-9);
                let x = |i: usize| legs[i].0[0] + (legs[i].1[0] - legs[i].0[0]) * t;
                b.tube([x(0) + tw / 2.0, z], [x(1) - tw / 2.0, z], y, tw, td);
            }
            if let Some(g) = glide {
                let r = mm(g.diameter) / 2.0;
                let ends: Vec<f64> = if bottom > 2.0 * tw {
                    vec![bottom_x[0] + tw / 2.0, bottom_x[1] - tw / 2.0]
                } else {
                    vec![w / 2.0]
                };
                for x in ends {
                    b.frustum([x, y], [0.0, floor], [r, r], Some(GLIDE));
                }
            }
        }
    }
    b
}

/// A foot at `pose` (its box's minimum corner).
pub fn foot_solid(id: Uuid, spec: &FootSpec, pose: Pose) -> Solid {
    let size = spec.local_size().map(mm);
    let (tris, edges) = foot_builder(spec).place(pose);
    Solid {
        id,
        kind: SolidKind::Foot,
        boxes: vec![(pose, size)],
        tris,
        edges,
        base: spec.color.0.map(|c| f32::from(c) / 255.0),
    }
}

/// A slide pair's six members, the drawer ones pulled out by `extension_mm`.
pub fn slide_solid(
    project: &Project,
    installation: &crate::domain::SlideInstallation,
    extension_mm: f64,
) -> Option<Solid> {
    let members = crate::slide_installation::member_boxes(project, installation, extension_mm)?;
    let mut tris = Vec::new();
    let mut edges = Vec::new();
    let mut boxes = Vec::new();
    for side in members {
        for (i, (pose, size)) in side.into_iter().enumerate() {
            let mut b = Builder::default();
            b.rect_frustum(
                [size[0] / 2.0, size[1] / 2.0],
                [0.0, size[2]],
                [size[0], size[1]],
                [size[0], size[1]],
                Some(ZINC[i]),
            );
            let (t, e) = b.place(pose);
            tris.extend(t);
            edges.extend(e);
            boxes.push((pose, size));
        }
    }
    Some(Solid {
        id: installation.id,
        kind: SolidKind::Slide,
        boxes,
        tris,
        edges,
        base: ZINC[0],
    })
}

fn um3(p: [i128; 3]) -> [f64; 3] {
    p.map(|v| v as f64 / 1000.0)
}

/// The hinge's cup (in the door) and plate (on the cabinet side), from its
/// checked references. `None` when the hinge has no references (unsupported
/// settings or missing parts): nothing is drawn rather than a guess.
pub fn hinge_solid(
    project: &Project,
    installation: &crate::domain::HingeInstallation,
    poses: Option<&HashMap<Uuid, Pose>>,
    show_plate: bool,
) -> Option<Solid> {
    let status = crate::hinge_installation::diagnose(project, installation);
    let r = status.references.as_ref()?;
    let pose_of = |id: Uuid| {
        poses
            .and_then(|p| p.get(&id).copied())
            .or_else(|| crate::assembly_edit::world_pose(project, id).ok())
    };
    let door = project.board(installation.door_board_id)?;
    let mount = project.board(installation.mounting_board_id)?;
    let (door_pose, mount_pose) = (
        pose_of(installation.door_board_id)?,
        pose_of(installation.mounting_board_id)?,
    );
    let color = if status.issues.is_empty() {
        NICKEL
    } else {
        HINGE_ISSUE
    };
    let mut tris = Vec::new();
    let mut edges = Vec::new();
    let mut boxes = Vec::new();
    // Cup: a short cylinder sunk into the door face, its rim just proud.
    let cup = um3(r.cup_center_um);
    let t = mm(door.thickness);
    let depth = mm(r.cup_depth);
    let (z0, z1) = match r.cup_face {
        crate::domain::BoardFace::MaxZ => (t - depth, t + CUP_LIP_MM),
        crate::domain::BoardFace::MinZ => (-CUP_LIP_MM, depth),
    };
    let radius = mm(r.cup_diameter) / 2.0;
    let mut b = Builder::default();
    b.frustum([cup[0], cup[1]], [z0, z1], [radius, radius], Some(color));
    let (t1, e1) = b.place(door_pose);
    tris.extend(t1);
    edges.extend(e1);
    let corner = Pose::new(
        door_pose
            .transform_point([cup[0] - radius, cup[1] - radius, z0])
            .ok()?,
        door_pose.rotation,
    )
    .ok()?;
    boxes.push((corner, [2.0 * radius, 2.0 * radius, z1 - z0]));
    if show_plate {
        // Plate: a small block around the two screw holes on the side.
        let [a, c] = r.plate_hole_centers_um.map(um3);
        let centre = [(a[0] + c[0]) / 2.0, (a[1] + c[1]) / 2.0];
        let pitch = ((a[0] - c[0]).powi(2) + (a[1] - c[1]).powi(2)).sqrt();
        let along_x = (a[0] - c[0]).abs() >= (a[1] - c[1]).abs() && pitch > 0.0;
        let long = (pitch + 12.0).max(32.0);
        let size = if along_x { [long, 14.0] } else { [14.0, long] };
        let h = mm(r.plate_height).max(CUP_LIP_MM);
        let mt = mm(mount.thickness);
        let (p0, p1) = match r.plate_face {
            crate::domain::BoardFace::MaxZ => (mt, mt + h),
            crate::domain::BoardFace::MinZ => (-h, 0.0),
        };
        let mut b = Builder::default();
        b.rect_frustum(centre, [p0, p1], size, size, Some(color));
        let (t2, e2) = b.place(mount_pose);
        tris.extend(t2);
        edges.extend(e2);
        let corner = Pose::new(
            mount_pose
                .transform_point([centre[0] - size[0] / 2.0, centre[1] - size[1] / 2.0, p0])
                .ok()?,
            mount_pose.rotation,
        )
        .ok()?;
        boxes.push((corner, [size[0], size[1], p1 - p0]));
    }
    Some(Solid {
        id: installation.id,
        kind: SolidKind::Hinge,
        boxes,
        tris,
        edges,
        base: color,
    })
}

/// One pickable fitting: its id, kind and posed boxes (size in mm).
pub type PickBoxes = (Uuid, SolidKind, Vec<(Pose, [f64; 3])>);

/// Boxes to pick slides and hinges by, without building their triangles.
pub fn pick_boxes(
    project: &Project,
    selection: &Selection,
    poses: Option<&HashMap<Uuid, Pose>>,
) -> Vec<PickBoxes> {
    let mut out = Vec::new();
    for installation in &project.slide_installations {
        if !installation
            .drawer_sides
            .iter()
            .all(|id| selection.visible(project, *id))
        {
            continue;
        }
        let extension = crate::slide_installation::extension_in(project, installation, poses);
        if let Some(members) =
            crate::slide_installation::member_boxes(project, installation, extension)
        {
            out.push((
                installation.id,
                SolidKind::Slide,
                members.into_iter().flatten().collect(),
            ));
        }
    }
    for hinge in &project.hinge_installations {
        if !selection.visible(project, hinge.door_board_id) {
            continue;
        }
        let plate = selection.visible(project, hinge.mounting_board_id);
        if let Some(solid) = hinge_solid(project, hinge, poses, plate) {
            out.push((hinge.id, SolidKind::Hinge, solid.boxes));
        }
    }
    out
}

/// Visible feet and slides, posed like the rest of the scene.
pub fn solids(
    project: &Project,
    selection: &Selection,
    poses: Option<&HashMap<Uuid, Pose>>,
) -> Vec<Solid> {
    let mut out = Vec::new();
    for hardware in &project.hardware {
        let Some(spec) = project.foot_spec(hardware).filter(|s| s.is_consistent()) else {
            continue;
        };
        if !selection.visible(project, hardware.id) {
            continue;
        }
        let pose = poses
            .and_then(|p| p.get(&hardware.id).copied())
            .or_else(|| crate::assembly_edit::world_pose(project, hardware.id).ok());
        if let Some(pose) = pose {
            out.push(foot_solid(hardware.id, spec, pose));
        }
    }
    for installation in &project.slide_installations {
        if !installation
            .drawer_sides
            .iter()
            .all(|id| selection.visible(project, *id))
        {
            continue;
        }
        let extension = crate::slide_installation::extension_in(project, installation, poses);
        if let Some(solid) = slide_solid(project, installation, extension) {
            out.push(solid);
        }
    }
    for hinge in &project.hinge_installations {
        if !selection.visible(project, hinge.door_board_id) {
            continue;
        }
        let plate = selection.visible(project, hinge.mounting_board_id);
        if let Some(solid) = hinge_solid(project, hinge, poses, plate) {
            out.push(solid);
        }
    }
    out
}

impl Mesh {
    /// Append a solid's triangles and edges relative to the camera target.
    pub fn add_solid(&mut self, solid: &Solid, target: [f64; 3], face: [f32; 3], edge: [f32; 3]) {
        for tri in &solid.tris {
            let color = tri.color.unwrap_or(face);
            for k in 0..3 {
                let s = tri.shade[k];
                let shaded = std::array::from_fn(|i| color[i] * s + 0.12 * (1.0 - s));
                Self::vertex(&mut self.faces, relative(tri.points[k], target), shaded);
            }
        }
        for [a, b] in &solid.edges {
            self.line(relative(*a, target), relative(*b, target), edge);
        }
    }
}

/// Outline color that stays visible on dark parts.
pub fn outline_for(base: [f32; 3]) -> [f32; 3] {
    let luma = 0.3 * base[0] + 0.59 * base[1] + 0.11 * base[2];
    if luma < 0.35 {
        [0.52, 0.52, 0.54]
    } else {
        base.map(|c| c * 0.55)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Glide, SrgbColor};
    use crate::units::Length;

    fn l(v: f64) -> Length {
        Length::from_micrometres((v * 1000.0).round() as i64)
    }

    fn spec(shape: FootShape) -> FootSpec {
        FootSpec {
            shape,
            color: SrgbColor([30, 30, 30]),
            finish: None,
            adjustment: Length::ZERO,
            mounting_holes: Vec::new(),
            load_kg: None,
            attribution: String::new(),
        }
    }

    fn shapes() -> Vec<FootShape> {
        vec![
            FootShape::Tapered {
                top: Section::Round { diameter: l(50.0) },
                bottom: Section::Round { diameter: l(30.0) },
                height: l(40.0),
            },
            FootShape::Post {
                tube: Section::Rect {
                    width: l(32.0),
                    depth: l(32.0),
                },
                plate: Section::Rect {
                    width: l(60.0),
                    depth: l(60.0),
                },
                plate_thickness: l(2.0),
                glide: Some(Glide {
                    diameter: l(38.0),
                    height: l(12.0),
                }),
                height: l(100.0),
            },
            FootShape::Frame {
                height: l(750.0),
                top_width: l(500.0),
                bottom_width: l(500.0),
                tube_width: l(30.0),
                tube_depth: l(30.0),
                crossbar_height: Some(l(200.0)),
                glide: None,
            },
            FootShape::Frame {
                height: l(710.0),
                top_width: l(500.0),
                bottom_width: l(400.0),
                tube_width: l(30.0),
                tube_depth: l(30.0),
                crossbar_height: None,
                glide: Some(Glide {
                    diameter: l(30.0),
                    height: l(10.0),
                }),
            },
            FootShape::Frame {
                height: l(710.0),
                top_width: l(500.0),
                bottom_width: l(30.0),
                tube_width: l(30.0),
                tube_depth: l(30.0),
                crossbar_height: None,
                glide: None,
            },
        ]
    }

    #[test]
    fn geometry_fills_exactly_the_foot_box() {
        for shape in shapes() {
            let spec = spec(shape);
            assert!(spec.is_consistent(), "{spec:?}");
            let size = spec.local_size().map(mm);
            let solid = foot_solid(Uuid::new_v4(), &spec, Pose::IDENTITY);
            let mut min = [f64::INFINITY; 3];
            let mut max = [f64::NEG_INFINITY; 3];
            for p in solid.tris.iter().flat_map(|t| t.points) {
                for i in 0..3 {
                    min[i] = min[i].min(p[i]);
                    max[i] = max[i].max(p[i]);
                }
            }
            for i in 0..3 {
                assert!(min[i] > -1e-6, "{i}: {min:?}");
                assert!(max[i] < size[i] + 1e-6, "{i}: {max:?} {size:?}");
            }
            // Stands on the floor and reaches the mounting face.
            assert!(min[2] < 15.0 && (max[2] - size[2]).abs() < 1e-6);
        }
    }

    #[test]
    fn closed_parts_face_outward() {
        for shape in shapes() {
            let solid = foot_solid(Uuid::new_v4(), &spec(shape), Pose::IDENTITY);
            // Divergence theorem: the signed volume of a closed outward mesh is positive.
            let volume: f64 = solid
                .tris
                .iter()
                .map(|t| {
                    let [a, b, c] = t.points;
                    (0..3).map(|i| a[i] * cross(b, c)[i]).sum::<f64>() / 6.0
                })
                .sum();
            assert!(volume > 0.0, "{volume}");
        }
    }

    #[test]
    fn round_parts_draw_rims_not_seams() {
        let solid = foot_solid(Uuid::new_v4(), &spec(shapes().remove(0)), Pose::IDENTITY);
        assert_eq!(solid.edges.len(), 2 * SEGMENTS);
        let tops: Vec<f32> = solid
            .tris
            .iter()
            .filter(|t| t.points.iter().all(|p| (p[2] - 40.0).abs() < 1e-9))
            .map(|t| t.shade[0])
            .collect();
        assert!(tops.iter().all(|s| *s > 0.9));
    }
}
