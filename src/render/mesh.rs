//! Scene mesh generation: shaded board and hardware boxes, edges, grid, axes
//! and floor shadow, relative to the camera target. Renderer independent.
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::domain::{
    Board, BoardEdge, BoardFace as SheetFace, BoardGrain, HardwareKind, MaterialKind, Project,
};
use crate::placement::{BoardFace, Side};
use crate::render::camera::*;
use crate::render::surface::Surface;

/// Floats per face vertex: position, world normal, color, UV (mm), surface.
pub const FACE_FLOATS: usize = 12;
/// Floats per line or shadow vertex: position and color.
pub const LINE_FLOATS: usize = 6;

/// Scene geometry relative to the camera target. `faces` are lit by the
/// renderer from their normals; `shadow` and `lines` are drawn as they are.
#[derive(Default)]
pub struct Mesh {
    pub faces: Vec<f32>,
    pub shadow: Vec<f32>,
    pub lines: Vec<f32>,
}

/// How one face is painted: a base color, optionally textured with a raw
/// surface. `turn` runs the texture along V instead of U (grain across).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceLook {
    pub color: [f32; 3],
    pub surface: Option<Surface>,
    pub turn: bool,
}

impl FaceLook {
    pub const fn plain(color: [f32; 3]) -> Self {
        Self {
            color,
            surface: None,
            turn: false,
        }
    }
}

/// How a board box is painted: its broad faces (MinZ, MaxZ) and its edge
/// faces in `BoardEdge::ALL` order, and which edges carry a band (they get a
/// marker line).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxLook {
    pub broad: [FaceLook; 2],
    pub edges: [FaceLook; 4],
    pub banded: [bool; 4],
}

impl BoxLook {
    pub const fn plain(color: [f32; 3]) -> Self {
        Self {
            broad: [FaceLook::plain(color); 2],
            edges: [FaceLook::plain(color); 4],
            banded: [false; 4],
        }
    }

    /// Every face's color through `f` (selection and hover tints).
    pub fn map_colors(mut self, f: impl Fn([f32; 3]) -> [f32; 3]) -> Self {
        for face in self.broad.iter_mut().chain(self.edges.iter_mut()) {
            face.color = f(face.color);
        }
        self
    }
}

impl Mesh {
    pub fn vertex(out: &mut Vec<f32>, pos: [f32; 3], color: [f32; 3]) {
        out.extend(pos);
        out.extend(color);
    }

    /// A face vertex: lit from `normal` (world, unit) and textured with
    /// `surface` at `uv` (millimetres on the face).
    pub fn face_vertex(
        &mut self,
        pos: [f32; 3],
        normal: [f32; 3],
        color: [f32; 3],
        uv: [f32; 2],
        surface: Option<Surface>,
    ) {
        self.faces.extend(pos);
        self.faces.extend(normal);
        self.faces.extend(color);
        self.faces.extend(uv);
        self.faces.push(surface.map_or(-1.0, |s| s.index() as f32));
    }

    pub fn line(&mut self, a: [f32; 3], b: [f32; 3], color: [f32; 3]) {
        Self::vertex(&mut self.lines, a, color);
        Self::vertex(&mut self.lines, b, color);
    }

    pub fn box_mesh(&mut self, corners: [[f32; 3]; 8], color: [f32; 3], edge: [f32; 3]) {
        self.box_mesh_look(corners, &BoxLook::plain(color), edge, None);
    }

    /// A board box painted per face. Banded edge faces get a marker line
    /// along their middle; `marked` outlines one edge face (the Band tool's
    /// hover).
    pub fn box_mesh_look(
        &mut self,
        corners: [[f32; 3]; 8],
        look: &BoxLook,
        edge: [f32; 3],
        marked: Option<BoardEdge>,
    ) {
        // `board_corners` uses bit-coded XYZ indexes (bit 0 = max X, bit 1 =
        // max Y, bit 2 = max Z). Texture axes come from those local axes.
        let sub = |a: [f32; 3], b: [f32; 3]| -> [f32; 3] { std::array::from_fn(|i| a[i] - b[i]) };
        let unit = |v: [f32; 3]| -> [f32; 3] {
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if len > 1e-9 {
                v.map(|c| c / len)
            } else {
                [0.0; 3]
            }
        };
        let axes = [
            unit(sub(corners[1], corners[0])),
            unit(sub(corners[2], corners[0])),
            unit(sub(corners[4], corners[0])),
        ];
        let centre: [f32; 3] =
            std::array::from_fn(|i| corners.iter().map(|c| c[i]).sum::<f32>() / 8.0);
        let origin = corners[0];
        // The quad topology below uses perimeter order. Convert once before
        // emitting faces and edges; otherwise each broad face becomes a
        // self-crossing bow-tie of two long triangles.
        let corners = [
            corners[0], corners[1], corners[3], corners[2], corners[4], corners[5], corners[7],
            corners[6],
        ];
        // Faces in perimeter order, their look, the local axes their texture
        // runs along (U, V), and the board edge each side face is.
        let faces = [
            ([0, 3, 2, 1], look.broad[0], [0, 1], None),
            ([4, 5, 6, 7], look.broad[1], [0, 1], None),
            ([0, 1, 5, 4], look.edges[2], [0, 2], Some(BoardEdge::MinY)),
            ([1, 2, 6, 5], look.edges[1], [1, 2], Some(BoardEdge::MaxX)),
            ([2, 3, 7, 6], look.edges[3], [0, 2], Some(BoardEdge::MaxY)),
            ([3, 0, 4, 7], look.edges[0], [1, 2], Some(BoardEdge::MinX)),
        ];
        for (face, paint, [u_axis, v_axis], board_edge) in faces {
            let quad = face.map(|i| corners[i]);
            let middle: [f32; 3] =
                std::array::from_fn(|i| quad.iter().map(|c| c[i]).sum::<f32>() / 4.0);
            let outward = sub(middle, centre);
            // The face's own axis, pointing away from the box.
            let normal_axis = 3 - u_axis - v_axis;
            let mut normal = axes[normal_axis];
            if (0..3).map(|i| normal[i] * outward[i]).sum::<f32>() < 0.0 {
                normal = normal.map(|c| -c);
            }
            let (u_axis, v_axis) = if paint.turn {
                (v_axis, u_axis)
            } else {
                (u_axis, v_axis)
            };
            for i in [0, 1, 2, 0, 2, 3] {
                let p = quad[i];
                let d = sub(p, origin);
                let along = |axis: [f32; 3]| (0..3).map(|k| d[k] * axis[k]).sum::<f32>();
                let uv = [along(axes[u_axis]), along(axes[v_axis])];
                self.face_vertex(p, normal, paint.color, uv, paint.surface);
            }
            let mid = |a: usize, b: usize| -> [f32; 3] {
                std::array::from_fn(|i| (corners[a][i] + corners[b][i]) / 2.0)
            };
            // The side face runs from the bottom pair (0,1) to the top pair
            // (2,3) of its quad: a line through the middle of the thickness.
            if board_edge.is_some_and(|e| look.banded[e.index()]) {
                self.line(mid(face[0], face[3]), mid(face[1], face[2]), BAND_MARK);
            }
            if marked.is_some() && board_edge == marked {
                for i in 0..4 {
                    self.line(corners[face[i]], corners[face[(i + 1) % 4]], BAND_HOVER);
                }
                self.line(mid(face[0], face[3]), mid(face[1], face[2]), BAND_HOVER);
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
            self.line(corners[a], corners[b], edge);
        }
    }
}

pub fn highlight_color(project: &Project, id: Uuid, selection: &Selection) -> [f32; 3] {
    if selection.active == Some(id) {
        [0.79, 0.45, 0.12]
    } else if selection.ids.contains(&id) {
        [0.16, 0.59, 0.67]
    } else if selected_board(project, &selection.ids, id) {
        [0.40, 0.57, 0.59]
    } else {
        [0.55, 0.51, 0.45]
    }
}

/// The marker line along a banded edge face.
pub const BAND_MARK: [f32; 3] = [0.10, 0.58, 0.50];
/// The Band tool's outline of the edge face under the pointer.
pub const BAND_HOVER: [f32; 3] = [0.93, 0.30, 0.62];

/// How every board is painted. Coated faces take the material color; raw
/// faces and unbanded edges show the sheet's core (MDF fibre, MDP chips,
/// plywood plies, wood grain); banded edges take the band's color. Without
/// material tint every face is the neutral color, bands still show.
pub fn board_looks(project: &Project, material_tint: bool) -> HashMap<Uuid, BoxLook> {
    let banding = crate::banding_rules::effective(project);
    let coating = crate::coating_rules::effective(project);
    project
        .boards
        .iter()
        .map(|board| {
            let color = board_face_color(project, board, material_tint);
            let mut look = BoxLook::plain(color);
            if let Some(material) = project
                .material(board.material_id)
                .filter(|_| material_tint)
            {
                let kind = material.kind;
                let core = crate::render::surface::core_color(kind).unwrap_or(color);
                let raw_edge = Surface::edge(kind).map(|surface| FaceLook {
                    color: core,
                    surface: Some(surface),
                    turn: false,
                });
                let grain_across = board.effective_grain(material) == BoardGrain::Width;
                let coated = coating
                    .get(&board.id)
                    .map_or(crate::coating_rules::Coated::Both, |c| c.coated);
                for (slot, face) in look
                    .broad
                    .iter_mut()
                    .zip([SheetFace::MinZ, SheetFace::MaxZ])
                {
                    *slot = if kind.accepts_coating() {
                        if coated.covers(face) {
                            FaceLook::plain(color)
                        } else {
                            FaceLook {
                                color: core,
                                surface: Surface::face(kind),
                                turn: false,
                            }
                        }
                    } else {
                        // Veneer and solid wood: the material color, with grain.
                        FaceLook {
                            color,
                            surface: Surface::face(kind).filter(|_| kind != MaterialKind::Other),
                            turn: grain_across,
                        }
                    };
                }
                if let Some(raw_edge) = raw_edge {
                    look.edges = [raw_edge; 4];
                }
            }
            if let Some(states) = banding.get(&board.id) {
                for (i, state) in states.iter().enumerate() {
                    if let Some(band) = state.band.and_then(|band| project.edge_band(band)) {
                        look.edges[i] = FaceLook::plain(band.color.0.map(|c| f32::from(c) / 255.0));
                        look.banded[i] = true;
                    }
                }
            }
            (board.id, look)
        })
        .collect()
}

pub const BACKGROUND: [f32; 3] = [236.0 / 255.0, 232.0 / 255.0, 225.0 / 255.0];
pub const NEUTRAL: [f32; 3] = [200.0 / 255.0, 196.0 / 255.0, 187.0 / 255.0];

pub fn board_face_color(project: &Project, board: &Board, material_tint: bool) -> [f32; 3] {
    if material_tint {
        project
            .material_color(board.material_id)
            .0
            .map(|channel| f32::from(channel) / 255.0)
    } else {
        NEUTRAL
    }
}

/// The pointer's hover tint on a face color.
pub fn hover_face_color(base: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| base[i] * 0.78 + [1.0, 0.86, 0.66][i] * 0.22)
}

pub fn selection_face_color(base: [f32; 3], active: bool) -> [f32; 3] {
    if active {
        std::array::from_fn(|i| base[i] * 0.38 + [1.0, 0.70, 0.30][i] * 0.62)
    } else {
        base
    }
}

/// A softly faded grounding patch, drawn at world Z=-2 below the grid and
/// cabinet. It has no picking surface or physical/shadow-map interpretation.
pub fn add_floor_shadow(mesh: &mut Mesh, bounds: Bounds, camera: &Camera) {
    if !bounds.valid() {
        return;
    }
    let pad = 65.0;
    let inner = [
        bounds.min[0] - pad,
        bounds.min[1] - pad,
        bounds.max[0] + pad,
        bounds.max[1] + pad,
    ];
    let outer = [
        inner[0] - pad,
        inner[1] - pad,
        inner[2] + pad,
        inner[3] + pad,
    ];
    let ring = |r: [f64; 4]| {
        [
            [r[0], r[1], -2.0],
            [r[2], r[1], -2.0],
            [r[2], r[3], -2.0],
            [r[0], r[3], -2.0],
        ]
        .map(|p| relative(p, camera.target))
    };
    let inner = ring(inner);
    let outer = ring(outer);
    let shade = [0.78, 0.76, 0.71];
    for i in [0, 1, 2, 0, 2, 3] {
        Mesh::vertex(&mut mesh.shadow, inner[i], shade);
    }
    for i in 0..4 {
        let next = (i + 1) % 4;
        for (point, color) in [
            (outer[i], BACKGROUND),
            (inner[i], shade),
            (inner[next], shade),
            (outer[i], BACKGROUND),
            (inner[next], shade),
            (outer[next], BACKGROUND),
        ] {
            Mesh::vertex(&mut mesh.shadow, point, color);
        }
    }
}

#[doc(hidden)]
pub fn scene(project: &Project, camera: &Camera, selection: &Selection) -> (Mesh, f64) {
    scene_with_faces(project, camera, selection, None, None, true)
}

pub fn scene_with_faces(
    project: &Project,
    camera: &Camera,
    selection: &Selection,
    faces: Option<(Uuid, BoardFace, Uuid, BoardFace)>,
    poses: Option<&HashMap<Uuid, crate::units::Pose>>,
    material_tint: bool,
) -> (Mesh, f64) {
    scene_with_hover(
        project,
        camera,
        selection,
        faces,
        poses,
        material_tint,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)] // Scene state is passed flat, as for the other scene builders.
pub fn scene_with_hover(
    project: &Project,
    camera: &Camera,
    selection: &Selection,
    faces: Option<(Uuid, BoardFace, Uuid, BoardFace)>,
    poses: Option<&HashMap<Uuid, crate::units::Pose>>,
    material_tint: bool,
    hovered: Option<Uuid>,
    band_hover: Option<(Uuid, BoardEdge)>,
) -> (Mesh, f64) {
    let hovered: HashSet<Uuid> = hovered.into_iter().collect();
    let looks = board_looks(project, material_tint);
    let mut mesh = Mesh::default();
    add_grid(&mut mesh, project, camera);
    for (end, color) in [
        ([1000.0, 0.0, 0.0], [0.77, 0.27, 0.23]),
        ([0.0, 1000.0, 0.0], [0.31, 0.60, 0.34]),
        ([0.0, 0.0, 1000.0], [0.24, 0.44, 0.77]),
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
            let look = looks
                .get(&board.id)
                .copied()
                .unwrap_or_else(|| BoxLook::plain(board_face_color(project, board, material_tint)));
            // A selected broad face needs a readable warm fill as well as an
            // edge: a shelf inside the carcass otherwise blends into the same
            // white material behind it. Selection remains session-only and
            // never changes the persisted display colour or stock identity.
            let active = selection.active == Some(board.id);
            let hover = !active
                && !hovered.is_empty()
                && crate::render::camera::selected_board(project, &hovered, board.id);
            let look = if hover {
                look.map_colors(hover_face_color)
            } else {
                look.map_colors(|c| selection_face_color(c, active))
            };
            let edge = if hover && !selection.ids.contains(&board.id) {
                [0.79, 0.45, 0.12]
            } else {
                highlight_color(project, board.id, selection)
            };
            mesh.box_mesh_look(
                corners,
                &look,
                edge,
                band_hover.filter(|(id, _)| *id == board.id).map(|(_, e)| e),
            );
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
            .or_else(|| crate::assembly_edit::world_pose(project, hardware.id).ok())
            && let Some(world) =
                box_corners(pose, dimensions.map(|d| d.micrometres() as f64 / 1000.0))
        {
            let corners = world.map(|p| {
                all.include(p);
                relative(p, camera.target)
            });
            mesh.box_mesh(
                corners,
                [0.66, 0.70, 0.69],
                highlight_color(project, hardware.id, selection),
            );
        }
    }
    for solid in crate::render::hardware_mesh::solids(project, selection, poses) {
        for p in solid.points() {
            all.include(p);
        }
        let active = selection.active == Some(solid.id);
        let hover = !active && hovered.contains(&solid.id);
        let face = if hover {
            hover_face_color(solid.base)
        } else {
            selection_face_color(solid.base, active)
        };
        let edge = if active || selection.ids.contains(&solid.id) || hover {
            highlight_color(project, solid.id, selection)
        } else {
            crate::render::hardware_mesh::outline_for(solid.base)
        };
        mesh.add_solid(&solid, camera.target, face, edge);
    }
    add_floor_shadow(&mut mesh, all, camera);
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
pub fn grid_display_interval(project: &Project, camera: &Camera) -> f64 {
    let spacing = project.grid_spacing.micrometres() as f64 / 1000.0;
    let visible_radius = (camera.distance * (FOV / 2.0).tan() * 2.5).clamp(100.0, 2_000_000.0);
    let desired = (visible_radius / 24.0).max(spacing);
    let multiple = 10_f64.powf((desired / spacing).log10().ceil().max(0.0));
    spacing * multiple
}

pub fn add_grid(mesh: &mut Mesh, project: &Project, camera: &Camera) {
    let visible_radius = (camera.distance * (FOV / 2.0).tan() * 2.5).clamp(100.0, 2_000_000.0);
    let half = visible_radius.max(1000.0);
    let step = grid_display_interval(project, camera);
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
                [0.65, 0.59, 0.51]
            } else if i.rem_euclid(10) == 0 {
                [0.81, 0.78, 0.73]
            } else {
                [0.87, 0.84, 0.80]
            };
            mesh.line(relative(a, center), relative(b, center), color);
        }
    }
}

pub fn relative(point: [f64; 3], origin: [f64; 3]) -> [f32; 3] {
    std::array::from_fn(|i| (point[i] - origin[i]) as f32)
}
