//! Boards as world-space boxes and board-local edge coordinates, shared by
//! hinge and slide installations. Board-local coordinates are integer
//! micrometres from the board's minimum XYZ corner.
use uuid::Uuid;

use crate::domain::{BoardEdge, BoardFace, Project};
use crate::units::Length;

/// Board-local XY (micrometres) of a point `offset` in from `edge` and
/// `along` the edge from its minimum end.
pub fn edge_point(
    edge: BoardEdge,
    length: Length,
    width: Length,
    offset: i128,
    along: i128,
) -> [i128; 2] {
    let (l, w) = (
        i128::from(length.micrometres()),
        i128::from(width.micrometres()),
    );
    match edge {
        BoardEdge::MinX => [offset, along],
        BoardEdge::MaxX => [l - offset, along],
        BoardEdge::MinY => [along, offset],
        BoardEdge::MaxY => [along, w - offset],
    }
}

/// Length of the board along `edge`: the room available for positions on it.
pub fn edge_length(edge: BoardEdge, length: Length, width: Length) -> Length {
    if edge.along_axis() == 1 {
        width
    } else {
        length
    }
}

pub fn inside(point: [i128; 2], length: Length, width: Length, margin: [i128; 2]) -> bool {
    point[0] - margin[0] >= 0
        && point[0] + margin[0] <= i128::from(length.micrometres())
        && point[1] - margin[1] >= 0
        && point[1] + margin[1] <= i128::from(width.micrometres())
}

pub fn face_z(face: BoardFace, thickness: Length) -> i128 {
    match face {
        BoardFace::MinZ => 0,
        BoardFace::MaxZ => i128::from(thickness.micrometres()),
    }
}

/// A board as a world-space box: origin, local axes and extents in mm.
pub struct BoardFrame {
    pub origin: [f64; 3],
    pub axes: [[f64; 3]; 3],
    pub size: [f64; 3],
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl BoardFrame {
    pub fn new(project: &Project, id: Uuid) -> Option<Self> {
        let board = project.boards.iter().find(|b| b.id == id)?;
        let pose = crate::assembly_edit::world_pose(project, id).ok()?;
        let origin = pose.transform_point([0.0; 3]).ok()?;
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
            .map(|axis| pose.rotation.rotate(axis));
        let size =
            [board.length, board.width, board.thickness].map(|v| v.micrometres() as f64 / 1000.0);
        Some(Self { origin, axes, size })
    }

    pub fn world(&self, local: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| {
            self.origin[i] + (0..3).map(|a| self.axes[a][i] * local[a]).sum::<f64>()
        })
    }

    pub fn local(&self, world: [f64; 3]) -> [f64; 3] {
        let d = std::array::from_fn(|i| world[i] - self.origin[i]);
        self.axes.map(|axis| dot(d, axis))
    }

    pub fn centre(&self) -> [f64; 3] {
        self.world(self.size.map(|v| v / 2.0))
    }

    /// Distance from a world point to the nearest point of the board.
    pub fn distance(&self, world: [f64; 3]) -> f64 {
        let local = self.local(world);
        (0..3)
            .map(|i| (local[i] - local[i].clamp(0.0, self.size[i])).powi(2))
            .sum::<f64>()
            .sqrt()
    }

    /// World midpoint of an edge, on the mid-thickness plane.
    pub fn edge_mid(&self, edge: BoardEdge) -> [f64; 3] {
        let [l, w, t] = self.size;
        self.world(match edge {
            BoardEdge::MinX => [0.0, w / 2.0, t / 2.0],
            BoardEdge::MaxX => [l, w / 2.0, t / 2.0],
            BoardEdge::MinY => [l / 2.0, 0.0, t / 2.0],
            BoardEdge::MaxY => [l / 2.0, w, t / 2.0],
        })
    }
}
