//! Scene mesh generation: shaded board and hardware boxes, edges, grid, axes
//! and floor shadow, relative to the camera target. Renderer independent.
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::domain::{Board, BoardEdge, HardwareKind, Project};
use crate::placement::{BoardFace, Side};
use crate::render::camera::*;

#[derive(Default)]
pub struct Mesh {
    pub faces: Vec<f32>,
    pub shadow: Vec<f32>,
    pub lines: Vec<f32>,
}

impl Mesh {
    pub fn vertex(out: &mut Vec<f32>, pos: [f32; 3], color: [f32; 3]) {
        out.extend(pos);
        out.extend(color);
    }

    pub fn line(&mut self, a: [f32; 3], b: [f32; 3], color: [f32; 3]) {
        Self::vertex(&mut self.lines, a, color);
        Self::vertex(&mut self.lines, b, color);
    }

    pub fn box_mesh(&mut self, corners: [[f32; 3]; 8], color: [f32; 3], edge: [f32; 3]) {
        self.box_mesh_banded(corners, color, edge, [None; 4], None);
    }

    /// A board box whose edge faces (in `BoardEdge::ALL` order) may carry
    /// edge band: a banded face takes the band colour and a marker line along
    /// its middle. `marked` outlines one edge face (the Band tool's hover).
    pub fn box_mesh_banded(
        &mut self,
        corners: [[f32; 3]; 8],
        color: [f32; 3],
        edge: [f32; 3],
        bands: [Option<[f32; 3]>; 4],
        marked: Option<BoardEdge>,
    ) {
        // `board_corners` uses bit-coded XYZ indexes (2 = min-X/max-Y,
        // 3 = max-X/max-Y). The quad topology below uses perimeter order.
        // Convert once before emitting faces and edges; otherwise each broad
        // face becomes a self-crossing bow-tie of two long triangles.
        let corners = [
            corners[0], corners[1], corners[3], corners[2], corners[4], corners[5], corners[7],
            corners[6],
        ];
        // Faces in perimeter order, with the board edge each side face is.
        let faces = [
            ([0, 3, 2, 1], 0.55, None),
            ([4, 5, 6, 7], 1.0, None),
            ([0, 1, 5, 4], 0.75, Some(BoardEdge::MinY)),
            ([1, 2, 6, 5], 0.85, Some(BoardEdge::MaxX)),
            ([2, 3, 7, 6], 0.68, Some(BoardEdge::MaxY)),
            ([3, 0, 4, 7], 0.8, Some(BoardEdge::MinX)),
        ];
        for (face, shade, board_edge) in faces {
            let band = board_edge.and_then(|e| bands[e.index()]);
            let base = band.unwrap_or(color);
            // Mix toward ambient warmth rather than multiplying sRGB channels to
            // black. This is display shading only; the saved color is unchanged.
            let shaded = std::array::from_fn(|i| base[i] * shade + 0.12 * (1.0 - shade));
            for i in [0, 1, 2, 0, 2, 3] {
                Self::vertex(&mut self.faces, corners[face[i]], shaded);
            }
            let mid = |a: usize, b: usize| -> [f32; 3] {
                std::array::from_fn(|i| (corners[a][i] + corners[b][i]) / 2.0)
            };
            // The side face runs from the bottom pair (0,1) to the top pair
            // (2,3) of its quad: a line through the middle of the thickness.
            if band.is_some() {
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

/// Band colours per board edge, from the effective banding.
pub fn band_colors(project: &Project) -> HashMap<Uuid, [Option<[f32; 3]>; 4]> {
    crate::banding_rules::effective(project)
        .into_iter()
        .map(|(id, states)| {
            let colors = states.map(|s| {
                s.band
                    .and_then(|band| project.edge_band(band))
                    .map(|band| band.color.0.map(|c| f32::from(c) / 255.0))
            });
            (id, colors)
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
    let bands = band_colors(project);
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
            let base = board_face_color(project, board, material_tint);
            // A selected broad face needs a readable warm fill as well as an
            // edge: a shelf inside the carcass otherwise blends into the same
            // white material behind it. Selection remains session-only and
            // never changes the persisted display colour or stock identity.
            let active = selection.active == Some(board.id);
            let hover = !active
                && !hovered.is_empty()
                && crate::render::camera::selected_board(project, &hovered, board.id);
            let face = if hover {
                std::array::from_fn(|i| base[i] * 0.78 + [1.0, 0.86, 0.66][i] * 0.22)
            } else {
                selection_face_color(base, active)
            };
            let edge = if hover && !selection.ids.contains(&board.id) {
                [0.79, 0.45, 0.12]
            } else {
                highlight_color(project, board.id, selection)
            };
            mesh.box_mesh_banded(
                corners,
                face,
                edge,
                bands.get(&board.id).copied().unwrap_or([None; 4]),
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
            std::array::from_fn(|i| solid.base[i] * 0.78 + [1.0, 0.86, 0.66][i] * 0.22)
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
