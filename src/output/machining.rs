//! Drilling per board, for file formats that send machining to a shop.
//! Everything is in board-local micrometres (X along the length, Y along the
//! width, Z through the thickness), from hardware the workshop PDF would also
//! print: installations without issues, on doors that need no review.
//! Holes whose size nobody gave are left out and listed; nothing is guessed.
use std::collections::BTreeMap;

use uuid::Uuid;

use crate::domain::{BoardEdge, BoardFace, Project};
use crate::units::Length;

/// A hole into a board's broad face.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceDrill {
    pub face: BoardFace,
    /// Centre on the face: local X and Y.
    pub at_um: [i128; 2],
    pub diameter: Length,
    pub depth: Length,
    /// Goes all the way through.
    pub through: bool,
    pub source: DrillSource,
}

/// A hole into a board's edge, along the face plane. No hardware makes these
/// yet; joinery (dowels, cams) will.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeDrill {
    pub edge: BoardEdge,
    /// Position along the edge from its minimum end.
    pub along_um: i128,
    /// Height in the thickness, from the MinZ face.
    pub z_um: i128,
    pub diameter: Length,
    pub depth: Length,
    pub source: DrillSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DrillSource {
    HingeCup(Uuid),
    HingePlate(Uuid),
    /// A drawer slide screw, on the carcass or the drawer box.
    SlideScrew(Uuid),
}

impl DrillSource {
    pub fn installation(self) -> Uuid {
        match self {
            Self::HingeCup(id) | Self::HingePlate(id) | Self::SlideScrew(id) => id,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardMachining {
    pub face_drills: Vec<FaceDrill>,
    pub edge_drills: Vec<EdgeDrill>,
}

impl BoardMachining {
    pub fn is_empty(&self) -> bool {
        self.face_drills.is_empty() && self.edge_drills.is_empty()
    }
}

/// Why some drilling is not in the export.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OmissionReason {
    /// The hinge or slide has issues; fix them to include its holes.
    HasIssues,
    /// The door changed since its hinges were confirmed.
    DoorNeedsReview,
    /// The catalog gives no pilot diameter and depth for these screw holes,
    /// and none was set for the export.
    PilotSizeUnknown,
}

/// Drilling left out of the export, per installation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Omission {
    pub installation: Uuid,
    pub reason: OmissionReason,
    /// How many holes are left out.
    pub holes: usize,
}

/// A pilot hole size the user gives for screws the catalog does not size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PilotHole {
    pub diameter: Length,
    pub depth: Length,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MachiningOptions {
    /// Screw pilot holes the catalog does not size get this; `None` leaves
    /// them out.
    pub screw_pilot: Option<PilotHole>,
    /// Drill hinge cups and plate screws. Off leaves hinges to the user,
    /// without listing them as left out.
    pub hinges: bool,
    /// Drill drawer slide screws.
    pub slides: bool,
}

impl Default for MachiningOptions {
    fn default() -> Self {
        Self {
            screw_pilot: None,
            hinges: true,
            slides: true,
        }
    }
}

/// Drilling of every board, and what was left out.
pub fn board_machining(
    project: &Project,
    options: &MachiningOptions,
) -> (BTreeMap<Uuid, BoardMachining>, Vec<Omission>) {
    let mut out: BTreeMap<Uuid, BoardMachining> = BTreeMap::new();
    let mut omissions = Vec::new();
    let thickness = |board: Uuid| project.board(board).map_or(Length::ZERO, |b| b.thickness);
    let face = |out: &mut BTreeMap<Uuid, BoardMachining>,
                board: Uuid,
                face: BoardFace,
                at: [i128; 3],
                diameter: Length,
                depth: Length,
                source: DrillSource| {
        let t = thickness(board);
        let through = depth >= t;
        out.entry(board).or_default().face_drills.push(FaceDrill {
            face,
            at_um: [at[0], at[1]],
            diameter,
            depth: if through { t } else { depth },
            through,
            source,
        });
    };
    let hinges = if options.hinges {
        project.hinge_installations.as_slice()
    } else {
        &[]
    };
    let slides = if options.slides {
        project.slide_installations.as_slice()
    } else {
        &[]
    };
    for installation in hinges {
        let status = crate::hinge_installation::diagnose(project, installation);
        let joint = project
            .door_joints
            .iter()
            .find(|j| j.hinge_installation_ids.contains(&installation.id));
        let coherent = joint.is_some_and(|j| !crate::door_joint::needs_review(project, j));
        let holes = 3;
        let references = match status.references {
            Some(references) if status.issues.is_empty() && coherent => references,
            _ => {
                omissions.push(Omission {
                    installation: installation.id,
                    reason: if status.issues.is_empty() {
                        OmissionReason::DoorNeedsReview
                    } else {
                        OmissionReason::HasIssues
                    },
                    holes,
                });
                continue;
            }
        };
        face(
            &mut out,
            installation.door_board_id,
            references.cup_face,
            references.cup_center_um,
            references.cup_diameter,
            references.cup_depth,
            DrillSource::HingeCup(installation.id),
        );
        // Mounting plate screws: the hinge facts never size their pilots.
        match options.screw_pilot {
            Some(pilot) => {
                for at in references.plate_hole_centers_um {
                    face(
                        &mut out,
                        installation.mounting_board_id,
                        references.plate_face,
                        at,
                        pilot.diameter,
                        pilot.depth,
                        DrillSource::HingePlate(installation.id),
                    );
                }
            }
            None => omissions.push(Omission {
                installation: installation.id,
                reason: OmissionReason::PilotSizeUnknown,
                holes: references.plate_hole_centers_um.len(),
            }),
        }
    }
    for installation in slides {
        let status = crate::slide_installation::diagnose(project, installation);
        let spec = project
            .catalog
            .iter()
            .find(|c| c.id == installation.catalog_id)
            .and_then(|c| c.slide());
        let (Some(references), Some(spec)) = (status.references, spec) else {
            omissions.push(Omission {
                installation: installation.id,
                reason: OmissionReason::HasIssues,
                holes: 0,
            });
            continue;
        };
        if !status.issues.is_empty() {
            let holes = references
                .sides
                .iter()
                .map(|s| s.cabinet_holes_um.len() + s.drawer_holes_um.len())
                .sum();
            omissions.push(Omission {
                installation: installation.id,
                reason: OmissionReason::HasIssues,
                holes,
            });
            continue;
        }
        let mut unknown = 0;
        for side in &references.sides {
            for (board, board_face, holes, catalog) in [
                (
                    side.cabinet_board,
                    side.cabinet_face,
                    &side.cabinet_holes_um,
                    &spec.cabinet_holes,
                ),
                (
                    side.drawer_board,
                    side.drawer_face,
                    &side.drawer_holes_um,
                    &spec.drawer_holes,
                ),
            ] {
                for (n, at) in holes.iter().enumerate() {
                    let size = catalog
                        .get(n)
                        .and_then(|hole| hole.diameter.zip(hole.depth))
                        .map(|(diameter, depth)| PilotHole { diameter, depth })
                        .or(options.screw_pilot);
                    match size {
                        Some(pilot) => face(
                            &mut out,
                            board,
                            board_face,
                            *at,
                            pilot.diameter,
                            pilot.depth,
                            DrillSource::SlideScrew(installation.id),
                        ),
                        None => unknown += 1,
                    }
                }
            }
        }
        if unknown > 0 {
            omissions.push(Omission {
                installation: installation.id,
                reason: OmissionReason::PilotSizeUnknown,
                holes: unknown,
            });
        }
    }
    (out, omissions)
}
