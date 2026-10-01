//! Which broad faces of a board are coated. A material coated on both faces
//! or on none decides alone. On a one-side coated material each board shows
//! its coating where it is seen: `Auto` picks the face toward the front for
//! boards standing across the cabinet (doors, fronts, backs), the upper face
//! for lying boards (bottoms, shelves, tops) and the outer face for boards
//! standing along it (sides). A manual choice always wins.
use std::collections::{BTreeMap, HashMap};

use uuid::Uuid;

use crate::board_frame::{BoardFrame, dot};
use crate::domain::{BoardFace, CoatedFace, Coating, Project};

/// The coated faces of a board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coated {
    /// No coating: both faces show the raw core. Also every board whose
    /// material kind is never coated.
    Raw,
    One(BoardFace),
    Both,
}

impl Coated {
    pub fn covers(self, face: BoardFace) -> bool {
        match self {
            Self::Raw => false,
            Self::One(coated) => coated == face,
            Self::Both => true,
        }
    }
}

/// Why `Auto` chose a face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    /// The board stands across the cabinet; the face toward the front.
    Front,
    /// The board lies flat; the upper face.
    Up,
    /// The board stands along the cabinet; the face away from its middle.
    Outside,
    /// No cabinet to tell outside from inside: the face toward +X.
    Right,
}

/// A board's coating and how it was decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoatingState {
    pub coated: Coated,
    /// The material is coated on one side only, so the face can be chosen.
    pub choosable: bool,
    /// `Some` when the face was chosen automatically.
    pub automatic: Option<Facing>,
}

/// The coating of every board.
pub fn effective(project: &Project) -> BTreeMap<Uuid, CoatingState> {
    let mut centres = CabinetCentres::default();
    project
        .boards
        .iter()
        .map(|board| (board.id, state(project, board.id, &mut centres)))
        .collect()
}

/// The coating of one board.
pub fn board_state(project: &Project, id: Uuid) -> Option<CoatingState> {
    project.board(id)?;
    Some(state(project, id, &mut CabinetCentres::default()))
}

/// The face `Auto` would choose, and why, for a one-side coated board.
pub fn automatic_face(project: &Project, id: Uuid) -> Option<(BoardFace, Facing)> {
    let frame = BoardFrame::new(project, id)?;
    Some(auto_face(
        project,
        id,
        &frame,
        &mut CabinetCentres::default(),
    ))
}

fn state(project: &Project, id: Uuid, centres: &mut CabinetCentres) -> CoatingState {
    let raw = CoatingState {
        coated: Coated::Raw,
        choosable: false,
        automatic: None,
    };
    let Some(board) = project.board(id) else {
        return raw;
    };
    let Some(coating) = project
        .material(board.material_id)
        .and_then(|m| m.effective_coating())
    else {
        return raw;
    };
    match coating {
        Coating::None => raw,
        Coating::BothSides => CoatingState {
            coated: Coated::Both,
            ..raw
        },
        Coating::OneSide => {
            let (face, automatic) = match board.coated_face {
                CoatedFace::MinZ => (BoardFace::MinZ, None),
                CoatedFace::MaxZ => (BoardFace::MaxZ, None),
                CoatedFace::Auto => match BoardFrame::new(project, id) {
                    Some(frame) => {
                        let (face, facing) = auto_face(project, id, &frame, centres);
                        (face, Some(facing))
                    }
                    None => (BoardFace::MaxZ, Some(Facing::Right)),
                },
            };
            CoatingState {
                coated: Coated::One(face),
                choosable: true,
                automatic,
            }
        }
    }
}

fn auto_face(
    project: &Project,
    id: Uuid,
    frame: &BoardFrame,
    centres: &mut CabinetCentres,
) -> (BoardFace, Facing) {
    // The MaxZ face looks along the board's local Z.
    let normal = frame.axes[2];
    let toward = |direction: [f64; 3]| {
        if dot(normal, direction) >= 0.0 {
            BoardFace::MaxZ
        } else {
            BoardFace::MinZ
        }
    };
    let [x, y, z] = normal.map(f64::abs);
    if z >= x && z >= y {
        (toward([0.0, 0.0, 1.0]), Facing::Up)
    } else if y >= x {
        (toward(crate::banding_rules::FRONT), Facing::Front)
    } else {
        let centre = frame.centre();
        match centres.of(project, id) {
            Some(middle) if (centre[0] - middle[0]).abs() > 1.0 => (
                toward([(centre[0] - middle[0]).signum(), 0.0, 0.0]),
                Facing::Outside,
            ),
            _ => (toward([1.0, 0.0, 0.0]), Facing::Right),
        }
    }
}

/// The middle of each cabinet's boards, computed once per cabinet.
#[derive(Default)]
struct CabinetCentres(HashMap<Uuid, Option<[f64; 3]>>);

impl CabinetCentres {
    fn of(&mut self, project: &Project, board: Uuid) -> Option<[f64; 3]> {
        let cabinet = top_assembly(project, project.board(board)?.parent_id)?;
        *self.0.entry(cabinet).or_insert_with(|| {
            let mut low = [f64::INFINITY; 3];
            let mut high = [f64::NEG_INFINITY; 3];
            for other in &project.boards {
                if top_assembly(project, other.parent_id) != Some(cabinet) {
                    continue;
                }
                let Some(frame) = BoardFrame::new(project, other.id) else {
                    continue;
                };
                for corner in 0..8 {
                    let local = std::array::from_fn(|i| {
                        if corner & (1 << i) != 0 {
                            frame.size[i]
                        } else {
                            0.0
                        }
                    });
                    let p = frame.world(local);
                    for i in 0..3 {
                        low[i] = low[i].min(p[i]);
                        high[i] = high[i].max(p[i]);
                    }
                }
            }
            low[0]
                .is_finite()
                .then(|| std::array::from_fn(|i| (low[i] + high[i]) / 2.0))
        })
    }
}

fn top_assembly(project: &Project, parent: Option<Uuid>) -> Option<Uuid> {
    let mut current = parent?;
    loop {
        let assembly = project.assemblies.iter().find(|a| a.id == current)?;
        match assembly.parent_id {
            Some(next) => current = next,
            None => return Some(current),
        }
    }
}
