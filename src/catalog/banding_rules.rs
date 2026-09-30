//! Which board edges carry edge band. An edge set to `Auto` is banded with
//! its material's default band when it is free: not joined to another board.
//! `On` and `Off` are the user's overrides and always win.
//!
//! An edge is *joined* when another board sits flat against its edge face
//! (a gap of at most `JOIN_GAP_MM`, or a slight overlap) and covers at least
//! `JOIN_COVERAGE` of that face. Only boards square to each other are
//! compared; a board at an odd angle never joins an edge. Doors and drawers
//! move, so their boards only join boards moving with them: a closed drawer
//! front does not hide the carcass edges behind it.
use std::collections::BTreeMap;

use uuid::Uuid;

use crate::board_frame::{BoardFrame, dot};
use crate::domain::{BoardEdge, EdgeBanding, Project};

/// Largest gap between an edge face and a board that still counts as a joint.
pub const JOIN_GAP_MM: f64 = 0.5;
/// Share of the edge face another board must cover to join the edge.
pub const JOIN_COVERAGE: f64 = 0.5;

const SQUARE: f64 = 1.0 - 1e-6;
const EPSILON: f64 = 1e-9;

/// What touches an edge face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Contact {
    Free,
    /// Another board covers part of the face, too little to join it.
    Partly {
        board: Uuid,
        coverage: f64,
    },
    Joined {
        board: Uuid,
        coverage: f64,
    },
}

impl Contact {
    pub fn is_joined(self) -> bool {
        matches!(self, Self::Joined { .. })
    }
}

/// One edge after applying the rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeState {
    pub setting: EdgeBanding,
    pub contact: Contact,
    /// The band on this edge, if any.
    pub band: Option<Uuid>,
}

impl EdgeState {
    pub fn is_manual(&self) -> bool {
        self.setting != EdgeBanding::Auto
    }
}

/// The outward normal of an edge face, in world space.
pub fn outward_normal(frame: &BoardFrame, edge: BoardEdge) -> [f64; 3] {
    let axis = 1 - edge.along_axis();
    let sign = if matches!(edge, BoardEdge::MinX | BoardEdge::MinY) {
        -1.0
    } else {
        1.0
    };
    frame.axes[axis].map(|v| v * sign)
}

/// The edges whose face points along `direction` (within a few degrees).
pub fn edges_facing(frame: &BoardFrame, direction: [f64; 3]) -> Vec<BoardEdge> {
    BoardEdge::ALL
        .into_iter()
        .filter(|&edge| dot(outward_normal(frame, edge), direction) > 0.99)
        .collect()
}

/// The world direction toward the front of a cabinet.
pub const FRONT: [f64; 3] = [0.0, -1.0, 0.0];

/// Edge states for every board, in `BoardEdge::ALL` order.
pub fn effective(project: &Project) -> BTreeMap<Uuid, [EdgeState; 4]> {
    let frames = frames(project);
    project
        .boards
        .iter()
        .map(|board| {
            let frame = frames
                .iter()
                .find(|(id, _, _)| *id == board.id)
                .map(|(_, _, f)| f);
            (board.id, states(project, board.id, frame, &frames))
        })
        .collect()
}

/// Edge states for one board.
pub fn board_states(project: &Project, id: Uuid) -> Option<[EdgeState; 4]> {
    project.board(id)?;
    let frames = frames(project);
    let frame = frames.iter().find(|(b, _, _)| *b == id).map(|(_, _, f)| f);
    Some(states(project, id, frame, &frames))
}

/// Each board with the moving part it belongs to (if any) and its frame.
type Frames = Vec<(Uuid, Option<Uuid>, BoardFrame)>;

fn frames(project: &Project) -> Frames {
    project
        .boards
        .iter()
        .filter_map(|b| {
            BoardFrame::new(project, b.id).map(|f| (b.id, moving_part(project, b.id), f))
        })
        .collect()
}

/// The door or drawer root a board moves with.
fn moving_part(project: &Project, board: Uuid) -> Option<Uuid> {
    let roots = project
        .door_joints
        .iter()
        .map(|j| j.moving_root_id)
        .chain(project.slide_installations.iter().map(|s| s.drawer_root_id));
    let mut ancestors = vec![board];
    let mut parent = project.board(board).and_then(|b| b.parent_id);
    while let Some(id) = parent {
        ancestors.push(id);
        parent = project
            .assemblies
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.parent_id);
    }
    // The innermost moving root wins (a door on a drawer moves with the door).
    let roots: Vec<Uuid> = roots.collect();
    ancestors.into_iter().find(|id| roots.contains(id))
}

/// The bands on a board's edges, in `BoardEdge::ALL` order.
pub fn bands(states: &[EdgeState; 4]) -> [Option<Uuid>; 4] {
    states.map(|s| s.band)
}

fn states(
    project: &Project,
    id: Uuid,
    frame: Option<&BoardFrame>,
    frames: &Frames,
) -> [EdgeState; 4] {
    let board = project.board(id).expect("board exists");
    let material = project.material(board.material_id);
    let bandable = material.is_some_and(|m| m.kind.accepts_banding());
    let default_band = material
        .and_then(|m| m.default_band)
        .filter(|band| project.edge_band(*band).is_some());
    BoardEdge::ALL.map(|edge| {
        let setting = board.banding.get(edge);
        let contact = frame.map_or(Contact::Free, |frame| contact(id, frame, edge, frames));
        let band = match setting {
            _ if !bandable => None,
            EdgeBanding::On(band) => Some(band),
            EdgeBanding::Off => None,
            EdgeBanding::Auto if contact.is_joined() => None,
            EdgeBanding::Auto => default_band,
        };
        EdgeState {
            setting,
            contact,
            band,
        }
    })
}

/// The board covering most of an edge face, and whether that joins it.
fn contact(id: Uuid, frame: &BoardFrame, edge: BoardEdge, frames: &Frames) -> Contact {
    let part = frames
        .iter()
        .find(|(b, _, _)| *b == id)
        .and_then(|(_, p, _)| *p);
    let mut best: Option<(Uuid, f64)> = None;
    for (other, other_part, other_frame) in frames {
        if *other == id || *other_part != part {
            continue;
        }
        let coverage = coverage(frame, edge, other_frame);
        if coverage > EPSILON && best.is_none_or(|(_, c)| coverage > c + EPSILON) {
            best = Some((*other, coverage));
        }
    }
    match best {
        None => Contact::Free,
        Some((board, coverage)) if coverage + EPSILON >= JOIN_COVERAGE => {
            Contact::Joined { board, coverage }
        }
        Some((board, coverage)) => Contact::Partly { board, coverage },
    }
}

/// Share of `edge`'s face that `other` sits against, from 0 to 1.
fn coverage(frame: &BoardFrame, edge: BoardEdge, other: &BoardFrame) -> f64 {
    // Only boards square to this one: each of their axes along one of ours.
    let square = other
        .axes
        .iter()
        .all(|axis| frame.axes.iter().any(|a| dot(*a, *axis).abs() > SQUARE));
    if !square {
        return 0.0;
    }
    // The other board as a box in this board's local frame.
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for corner in 0..8 {
        let local = std::array::from_fn(|i| {
            if corner & (1 << i) == 0 {
                0.0
            } else {
                other.size[i]
            }
        });
        let point = frame.local(other.world(local));
        for i in 0..3 {
            min[i] = min[i].min(point[i]);
            max[i] = max[i].max(point[i]);
        }
    }
    let size = frame.size;
    let normal_axis = 1 - edge.along_axis();
    let along_axis = edge.along_axis();
    // The face plane and the outward side of it.
    let (plane, outward) = match edge {
        BoardEdge::MinX | BoardEdge::MinY => (0.0, -1.0),
        BoardEdge::MaxX | BoardEdge::MaxY => (size[normal_axis], 1.0),
    };
    // The other board must reach the thin slab just outside the face, and
    // start no further inside than the face itself (a slight overlap is fine,
    // a board buried in this one is not a joint).
    let (near, far) = if outward < 0.0 {
        (max[normal_axis], min[normal_axis])
    } else {
        (-min[normal_axis], -max[normal_axis])
    };
    let plane = plane * -outward;
    let reaches = near - plane >= -JOIN_GAP_MM - EPSILON;
    let outside = far - plane <= EPSILON;
    if !reaches || !outside {
        return 0.0;
    }
    let span = |axis: usize| (max[axis].min(size[axis]) - min[axis].max(0.0)).max(0.0);
    let area = size[along_axis] * size[2];
    if area <= 0.0 {
        return 0.0;
    }
    (span(along_axis) * span(2) / area).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests;
