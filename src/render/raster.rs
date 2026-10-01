//! Deterministic CPU rasterizer for the committed scene. It has no egui frame,
//! GPU or live interaction as inputs, so it runs headless (thumbnails, agent
//! pictures, tests). Faces are depth-tested and shaded exactly like the
//! viewport mesh; edges are depth-tested lines; an object buffer records which
//! board or hardware item covers each pixel.
use std::collections::{HashMap, HashSet};

use eframe::egui;
use uuid::Uuid;

use crate::domain::{HardwareKind, Project};
use crate::render::camera::{
    Bounds, Camera, Projection, Selection, board_corners, box_corners, dot, world_pose,
};
use crate::render::hardware_mesh::{Solid, outline_for, solids};
use crate::render::lighting::{Light, shade};
use crate::render::mesh::{
    BACKGROUND, BoxLook, FACE_FLOATS, LINE_FLOATS, Mesh, add_floor_shadow, add_grid,
    board_face_color, board_looks, relative,
};
use crate::render::surface::{self, Surface};
use crate::units::Pose;

/// An RGBA8 image with one object slot per pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct Raster {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    /// Index into [`Raster::objects`] per pixel, `u32::MAX` for background.
    pub object_at: Vec<u32>,
    /// Objects in draw order: every visible board, then placeholder hardware.
    pub objects: Vec<Uuid>,
}

impl Raster {
    pub fn new(width: usize, height: usize) -> Self {
        let mut rgba = vec![0_u8; width * height * 4];
        let background = BACKGROUND.map(|c| (c * 255.0).round() as u8);
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&[background[0], background[1], background[2], 255]);
        }
        Self {
            width,
            height,
            rgba,
            object_at: vec![u32::MAX; width * height],
            objects: Vec::new(),
        }
    }

    /// Visible pixel count and pixel-space centroid per object index.
    pub fn coverage(&self) -> Vec<(usize, [f64; 2])> {
        let mut sums = vec![(0_usize, [0.0_f64; 2]); self.objects.len()];
        for y in 0..self.height {
            for x in 0..self.width {
                let slot = self.object_at[y * self.width + x];
                if let Some(entry) = sums.get_mut(slot as usize) {
                    entry.0 += 1;
                    entry.1[0] += x as f64 + 0.5;
                    entry.1[1] += y as f64 + 0.5;
                }
            }
        }
        sums.into_iter()
            .map(|(n, [x, y])| {
                if n == 0 {
                    (0, [0.0, 0.0])
                } else {
                    (n, [x / n as f64, y / n as f64])
                }
            })
            .collect()
    }
}

/// What to draw. Visibility (hidden objects and their descendants) comes from
/// the [`Selection`]; `highlight` objects get the viewport's selection colors.
#[derive(Clone, Debug)]
pub struct SceneStyle<'a> {
    pub width: usize,
    pub height: usize,
    pub material_tint: bool,
    pub show_grid: bool,
    pub show_hardware: bool,
    pub show_shadow: bool,
    /// Posed objects (door opening preview); others use their committed pose.
    pub poses: Option<&'a HashMap<Uuid, Pose>>,
    /// Supersampling factor per axis (1 = none).
    pub supersample: usize,
    /// Pictures use [`Light::studio`], whatever the viewport's lighting.
    pub light: Light,
}

impl Default for SceneStyle<'_> {
    fn default() -> Self {
        Self {
            width: 1024,
            height: 768,
            material_tint: true,
            show_grid: false,
            show_hardware: true,
            show_shadow: true,
            poses: None,
            supersample: 2,
            light: Light::studio(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderError {
    EmptyScene,
    InvalidSize,
    Unprojectable,
}

/// A world-space box, or a shaped solid, with its object identity and colors.
struct Item {
    id: Uuid,
    geometry: Geometry,
    edge: [f32; 3],
}

enum Geometry {
    Box([[f64; 3]; 8], Box<BoxLook>),
    Solid(Box<Solid>, [f32; 3]),
}

impl Item {
    fn points(&self) -> Vec<[f64; 3]> {
        match &self.geometry {
            Geometry::Box(corners, _) => corners.to_vec(),
            Geometry::Solid(solid, _) => solid.points(),
        }
    }

    fn mesh(&self, target: [f64; 3]) -> Mesh {
        let mut mesh = Mesh::default();
        match &self.geometry {
            Geometry::Box(corners, look) => {
                mesh.box_mesh_look(corners.map(|p| relative(p, target)), look, self.edge, None);
            }
            Geometry::Solid(solid, face) => mesh.add_solid(solid, target, *face, self.edge),
        }
        mesh
    }
}

/// The selection tint pictures use.
fn picked(color: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| color[i] * 0.45 + [1.0, 0.70, 0.30][i] * 0.55)
}

fn items(project: &Project, selection: &Selection, style: &SceneStyle) -> Vec<Item> {
    let posed = |id: Uuid| style.poses.and_then(|p| p.get(&id).copied());
    let mut out = Vec::new();
    let looks = board_looks(project, style.material_tint);
    for board in &project.boards {
        if !selection.visible(project, board.id) {
            continue;
        }
        let corners = match posed(board.id) {
            Some(pose) => box_corners(
                pose,
                board
                    .blank_dimensions()
                    .map(|d| d.micrometres() as f64 / 1000.0),
            ),
            None => board_corners(project, board),
        };
        let Some(corners) = corners else { continue };
        let look = looks.get(&board.id).copied().unwrap_or_else(|| {
            BoxLook::plain(board_face_color(project, board, style.material_tint))
        });
        let highlighted = crate::render::camera::selected_board(project, &selection.ids, board.id);
        let look = if highlighted {
            look.map_colors(picked)
        } else {
            look
        };
        let edge = if highlighted {
            [0.79, 0.45, 0.12]
        } else {
            [0.40, 0.37, 0.33]
        };
        out.push(Item {
            id: board.id,
            geometry: Geometry::Box(corners, Box::new(look)),
            edge,
        });
    }
    if style.show_hardware {
        for hardware in &project.hardware {
            let HardwareKind::Placeholder { dimensions } = hardware.kind else {
                continue;
            };
            if !selection.visible(project, hardware.id) {
                continue;
            }
            let pose = posed(hardware.id)
                .or_else(|| crate::assembly_edit::world_pose(project, hardware.id).ok());
            let Some(corners) = pose.and_then(|pose| {
                box_corners(pose, dimensions.map(|d| d.micrometres() as f64 / 1000.0))
            }) else {
                continue;
            };
            let highlighted = selection.ids.contains(&hardware.id);
            let face = if highlighted {
                [0.95, 0.72, 0.45]
            } else {
                [0.66, 0.70, 0.69]
            };
            out.push(Item {
                id: hardware.id,
                geometry: Geometry::Box(corners, Box::new(BoxLook::plain(face))),
                edge: [0.35, 0.38, 0.38],
            });
        }
        for solid in solids(project, selection, style.poses) {
            let highlighted = selection.ids.contains(&solid.id);
            let face = if highlighted {
                picked(solid.base)
            } else {
                solid.base
            };
            let edge = if highlighted {
                [0.79, 0.45, 0.12]
            } else {
                outline_for(solid.base)
            };
            out.push(Item {
                id: solid.id,
                geometry: Geometry::Solid(Box::new(solid), face),
                edge,
            });
        }
    }
    out
}

/// World bounds of what [`render_scene`] would draw.
pub fn visible_bounds(
    project: &Project,
    selection: &Selection,
    style: &SceneStyle,
) -> Option<Bounds> {
    let mut bounds = Bounds::empty();
    for item in items(project, selection, style) {
        for p in item.points() {
            bounds.include(p);
        }
    }
    bounds.valid().then_some(bounds)
}

/// World-space corners of the visible objects in `only` (or all visible when
/// empty), for framing a camera.
pub fn framing_points(
    project: &Project,
    selection: &Selection,
    style: &SceneStyle,
    only: &HashSet<Uuid>,
) -> Vec<[f64; 3]> {
    items(project, selection, style)
        .into_iter()
        .filter(|item| {
            only.is_empty()
                || only.contains(&item.id)
                || project.boards.iter().any(|b| {
                    b.id == item.id && crate::render::camera::selected_board(project, only, b.id)
                })
                || project.hardware.iter().any(|h| {
                    h.id == item.id
                        && crate::render::camera::selected_ancestor(project, only, h.parent_id)
                })
        })
        .flat_map(|item| item.points())
        .collect()
}

/// Aim the camera at `points` and set the distance so their projection fills
/// `fill` (0..1) of the smaller image side. Orientation is kept.
pub fn fit_camera(
    camera: &mut Camera,
    points: &[[f64; 3]],
    width: usize,
    height: usize,
    fill: f64,
) {
    let mut bounds = Bounds::empty();
    for p in points {
        bounds.include(*p);
    }
    if !bounds.valid() {
        return;
    }
    camera.viewport_aspect = width as f64 / height.max(1) as f64;
    camera.viewport_height = height as f64;
    camera.pending_frame = false;
    camera.frame(bounds, camera.viewport_aspect);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32, height as f32));
    // Refine: the enclosing-sphere fit leaves a lot of margin. Recenter on the
    // projected box and scale the distance to the wanted fill.
    for _ in 0..4 {
        let projected: Option<Vec<egui::Pos2>> =
            points.iter().map(|p| camera.project(*p, rect)).collect();
        let Some(projected) = projected else { return };
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in &projected {
            min[0] = min[0].min(p.x as f64);
            min[1] = min[1].min(p.y as f64);
            max[0] = max[0].max(p.x as f64);
            max[1] = max[1].max(p.y as f64);
        }
        let extent_x = (max[0] - min[0]) / width as f64;
        let extent_y = (max[1] - min[1]) / height as f64;
        let extent = extent_x.max(extent_y).max(1e-6);
        // Pan so the projected box is centred.
        let (right, up, _) = camera.basis();
        let scale = match camera.projection {
            Projection::Perspective => camera.distance,
            Projection::Orthographic => camera.distance,
        } * (crate::render::camera::FOV / 2.0).tan()
            * 2.0
            / height as f64;
        let dx = ((min[0] + max[0]) / 2.0 - width as f64 / 2.0) * scale;
        let dy = ((min[1] + max[1]) / 2.0 - height as f64 / 2.0) * scale;
        for i in 0..3 {
            camera.target[i] += dx * right[i] - dy * up[i];
        }
        camera.distance = (camera.distance * extent / fill).clamp(1.0, 1.0e9);
    }
}

/// Rasterize the visible scene with the given camera.
pub fn render_scene(
    project: &Project,
    camera: &Camera,
    selection: &Selection,
    style: &SceneStyle,
) -> Result<Raster, RenderError> {
    if style.width == 0 || style.height == 0 || style.width > 4096 || style.height > 4096 {
        return Err(RenderError::InvalidSize);
    }
    let ss = style.supersample.clamp(1, 4);
    let (w, h) = (style.width * ss, style.height * ss);
    let items = items(project, selection, style);
    if items.is_empty() {
        return Err(RenderError::EmptyScene);
    }
    let mut target = Target::new(w, h, camera);
    target.raster.objects = items.iter().map(|i| i.id).collect();

    let mut background = Mesh::default();
    if style.show_grid {
        add_grid(&mut background, project, camera);
    }
    if style.show_shadow {
        let mut bounds = Bounds::empty();
        for item in &items {
            for p in item.points() {
                bounds.include(p);
            }
        }
        add_floor_shadow(&mut background, bounds, camera);
    }
    for triangle in background.shadow.as_chunks::<{ 3 * LINE_FLOATS }>().0 {
        target.triangle::<LINE_FLOATS>(triangle, u32::MAX, None);
    }
    for (index, item) in items.iter().enumerate() {
        let mesh = item.mesh(camera.target);
        for triangle in mesh.faces.as_chunks::<{ 3 * FACE_FLOATS }>().0 {
            target.triangle::<FACE_FLOATS>(triangle, index as u32, Some(&style.light));
        }
    }
    for line in background.lines.as_chunks::<{ 2 * LINE_FLOATS }>().0 {
        target.line(line, 0.0);
    }
    for item in &items {
        let mesh = item.mesh(camera.target);
        for line in mesh.lines.as_chunks::<{ 2 * LINE_FLOATS }>().0 {
            target.line(line, ss as f32 * 0.6);
        }
    }
    Ok(downsample(target.raster, ss))
}

struct Target<'a> {
    raster: Raster,
    depth: Vec<f64>,
    camera: &'a Camera,
    rect: egui::Rect,
    forward: [f64; 3],
}

impl<'a> Target<'a> {
    fn new(width: usize, height: usize, camera: &'a Camera) -> Self {
        let (_, _, forward) = camera.basis();
        Self {
            raster: Raster::new(width, height),
            depth: vec![f64::INFINITY; width * height],
            camera,
            rect: egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width as f32, height as f32),
            ),
            forward,
        }
    }

    /// Screen position and view depth of a vertex relative to the camera target.
    fn vertex(&self, v: &[f32]) -> Option<(egui::Pos2, f64)> {
        let local = [v[0] as f64, v[1] as f64, v[2] as f64];
        let world = std::array::from_fn(|i| self.camera.target[i] + local[i]);
        let position = self.camera.project(world, self.rect)?;
        let depth = self.camera.distance + dot(local, self.forward);
        (depth.is_finite() && depth > 0.0).then_some((position, depth))
    }

    fn perspective(&self) -> bool {
        self.camera.projection == Projection::Perspective
    }

    /// One triangle of `N`-float vertices: position first, then either a
    /// color (unlit, `light` is `None`: the floor shadow, which writes no
    /// depth) or the lit face attributes of [`Mesh::face_vertex`].
    fn triangle<const N: usize>(&mut self, t: &[f32], object: u32, light: Option<&Light>) {
        let write_depth = light.is_some();
        let mut v = [(egui::Pos2::ZERO, 0.0, [0.0_f32; N]); 3];
        for (vertex, slot) in t.as_chunks::<N>().0.iter().zip(v.iter_mut()) {
            // Geometry behind the eye is skipped rather than failing the picture.
            let Some((position, depth)) = self.vertex(vertex) else {
                return;
            };
            *slot = (position, depth, *vertex);
        }
        let edge = |a: egui::Pos2, b: egui::Pos2, p: egui::Pos2| {
            (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
        };
        let area = edge(v[0].0, v[1].0, v[2].0);
        if area.abs() < 1e-6 {
            return;
        }
        let surface = if N == FACE_FLOATS {
            Surface::from_index(v[0].2[FACE_FLOATS - 1])
        } else {
            None
        };
        let (w, h) = (self.raster.width, self.raster.height);
        let min_x = v
            .iter()
            .map(|p| p.0.x)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let max_x = v
            .iter()
            .map(|p| p.0.x)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(w as f32) as usize;
        let min_y = v
            .iter()
            .map(|p| p.0.y)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let max_y = v
            .iter()
            .map(|p| p.0.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(h as f32) as usize;
        let perspective = self.perspective();
        for y in min_y..max_y {
            for x in min_x..max_x {
                let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                let mut weights = [
                    edge(v[1].0, v[2].0, p) / area,
                    edge(v[2].0, v[0].0, p) / area,
                    edge(v[0].0, v[1].0, p) / area,
                ];
                if weights.iter().any(|weight| *weight < -1e-5) {
                    continue;
                }
                let depth = if perspective {
                    // Perspective-correct: 1/z is linear in screen space, and
                    // so is every attribute divided by z.
                    let inverse: f64 = (0..3).map(|i| weights[i] as f64 / v[i].1).sum();
                    for i in 0..3 {
                        weights[i] = (weights[i] as f64 / v[i].1 / inverse) as f32;
                    }
                    1.0 / inverse
                } else {
                    (0..3).map(|i| weights[i] as f64 * v[i].1).sum()
                };
                let index = y * w + x;
                if write_depth {
                    if depth >= self.depth[index] {
                        continue;
                    }
                    self.depth[index] = depth;
                    self.raster.object_at[index] = object;
                } else if depth >= self.depth[index] {
                    continue;
                }
                let at = |k: usize| -> f32 { (0..3).map(|i| weights[i] * v[i].2[k]).sum() };
                let color = match light {
                    None => [at(3), at(4), at(5)],
                    Some(light) => {
                        let n = [at(3), at(4), at(5)];
                        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
                        let detail = surface.map_or(1.0, |s| surface::sample(s, [at(9), at(10)]));
                        let base = [at(6), at(7), at(8)].map(|c| c * detail);
                        shade(base, light.brightness(n.map(|c| c / len)))
                    }
                };
                for (channel, value) in color.into_iter().enumerate() {
                    self.raster.rgba[index * 4 + channel] =
                        (value.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
    }

    /// A depth-tested line with a small bias so edges on visible faces win.
    fn line(&mut self, l: &[f32], half_width: f32) {
        let (Some(a), Some(b)) = (self.vertex(&l[0..6]), self.vertex(&l[6..12])) else {
            return;
        };
        let color = [l[3], l[4], l[5]].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
        let (w, h) = (self.raster.width as i64, self.raster.height as i64);
        let delta = a.0 - b.0;
        let steps = delta.x.abs().max(delta.y.abs()).ceil().max(1.0) as usize;
        let radius = half_width.max(0.0).floor() as i64;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let p = a.0 + (b.0 - a.0) * t;
            let depth = if self.perspective() {
                1.0 / ((1.0 - t as f64) / a.1 + t as f64 / b.1)
            } else {
                a.1 + (b.1 - a.1) * t as f64
            };
            let bias = 0.5 + depth * 0.003;
            for oy in -radius..=radius {
                for ox in -radius..=radius {
                    let (x, y) = (p.x.floor() as i64 + ox, p.y.floor() as i64 + oy);
                    if x < 0 || y < 0 || x >= w || y >= h {
                        continue;
                    }
                    let index = (y * w + x) as usize;
                    if depth > self.depth[index] + bias {
                        continue;
                    }
                    self.raster.rgba[index * 4..index * 4 + 3].copy_from_slice(&color);
                }
            }
        }
    }
}

/// Box-filter colors; the object slot is the most frequent one in each block.
fn downsample(source: Raster, factor: usize) -> Raster {
    if factor == 1 {
        return source;
    }
    let (w, h) = (source.width / factor, source.height / factor);
    let mut out = Raster::new(w, h);
    out.objects = source.objects;
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0_u32; 3];
            counts.clear();
            for sy in 0..factor {
                for sx in 0..factor {
                    let index = (y * factor + sy) * source.width + x * factor + sx;
                    for (c, total) in sum.iter_mut().enumerate() {
                        *total += u32::from(source.rgba[index * 4 + c]);
                    }
                    *counts.entry(source.object_at[index]).or_default() += 1;
                }
            }
            let n = (factor * factor) as u32;
            let index = y * w + x;
            for (c, total) in sum.into_iter().enumerate() {
                out.rgba[index * 4 + c] = ((total + n / 2) / n) as u8;
            }
            out.object_at[index] = counts
                .iter()
                .max_by_key(|(slot, count)| (**count, std::cmp::Reverse(**slot)))
                .map_or(u32::MAX, |(slot, _)| *slot);
        }
    }
    out
}

/// World pose used for a board, honoring overrides; `None` if unresolvable.
pub fn object_world_pose(
    project: &Project,
    id: Uuid,
    poses: Option<&HashMap<Uuid, Pose>>,
) -> Option<Pose> {
    if let Some(pose) = poses.and_then(|p| p.get(&id)) {
        return Some(*pose);
    }
    project
        .boards
        .iter()
        .find(|b| b.id == id)
        .and_then(|b| world_pose(project, b))
        .or_else(|| crate::assembly_edit::world_pose(project, id).ok())
}
