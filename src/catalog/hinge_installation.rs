//! Read-only, board-local hinge annotations and atomic installation edits.
//! Coordinates below are integer micrometres from each board's minimum XYZ corner.
use uuid::Uuid;

use crate::board_frame::{BoardFrame, dot, face_z, inside};
pub use crate::board_frame::{edge_length, edge_point};
use crate::commands::{EditError, ProjectEditor};
use crate::domain::{
    BoardEdge, BoardFace, HingeArm, HingeInstallation, HingeMountingSide, Project,
};
use crate::hardware_catalog;
use crate::units::Length;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallationIssue {
    MissingPart(Uuid),
    MissingCatalog(Uuid),
    MissingVerifiedCatalog,
    UnsupportedThickness,
    UnsupportedOverlay,
    CupOutsideDoor,
    PlateOutsideMount,
    /// Inset arm with E smaller than the door thickness: the door would
    /// stand proud of the cabinet front.
    InsetShallowerThanDoor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallationStatus {
    pub id: Uuid,
    pub issues: Vec<InstallationIssue>,
    /// Available only for coherent catalog facts and a K/R (or K/F) pair from its table.
    /// Geometry may still be out of bounds; check `issues` before shop use.
    pub references: Option<InstallationReferences>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallationReferences {
    pub cup_center_um: [i128; 3],
    /// Cup recess extends from this face inward by `cup_depth`.
    pub cup_face: BoardFace,
    pub cup_diameter: Length,
    pub cup_depth: Length,
    pub plate_hole_centers_um: [[i128; 3]; 2],
    pub plate_face: BoardFace,
    pub plate_height: Length,
    /// Zero when the source does not dimension it: centre line only.
    pub plate_hole_pitch: Length,
    pub plate_front_offset: Length,
    pub arm: HingeArm,
    /// E for an inset arm, zero otherwise. The plate sits at front offset + E.
    pub inset_depth: Length,
    pub product_id: String,
    pub plate_id: Option<String>,
    pub source: String,
    pub attribution: String,
    pub printed_page: u16,
    pub pdf_page: u16,
    /// No pilot diameter, pilot depth, fastener or cup screw coordinates are supplied.
    pub fasteners_available: bool,
    pub trust: hardware_catalog::Trust,
    /// "Manufacturer pack-version" when the record came from a catalog pack.
    pub pack: Option<String>,
}

impl InstallationReferences {
    /// Printed provenance: codes, source, pages and, for pack records, the
    /// pack and whether the data was user supplied. Shared by both PDFs.
    pub fn source_line(&self, loc: &crate::i18n::Localizer) -> String {
        let page = |n: u16| {
            if n == 0 {
                "—".to_owned()
            } else {
                n.to_string()
            }
        };
        let mut line = format!(
            "{} / {} — {} ({}; {}: {}; PDF: {})",
            self.product_id,
            self.plate_id.as_deref().unwrap_or("—"),
            self.attribution,
            self.source,
            loc.text("pdf-printed-page"),
            page(self.printed_page),
            page(self.pdf_page)
        );
        if let Some(pack) = &self.pack {
            line.push_str(&format!(" — {}: {pack}", loc.text("pdf-catalog-pack")));
        }
        if self.trust == hardware_catalog::Trust::UserSupplied {
            line.push_str(&format!(" — {}", loc.text("pdf-user-supplied")));
        }
        line
    }

    /// The chosen table pair, cup and plate, with lengths formatted by `length`.
    pub fn settings_line(
        &self,
        loc: &crate::i18n::Localizer,
        cup_edge_setback: Length,
        table_value: Length,
        length: impl Fn(Length) -> String,
    ) -> String {
        let inset = self.arm.is_inset();
        let pitch = if self.plate_hole_pitch == Length::ZERO {
            "—".to_owned()
        } else {
            length(self.plate_hole_pitch)
        };
        let front = if inset {
            format!(
                "{} + E {} = {}",
                length(self.plate_front_offset),
                length(self.inset_depth),
                length(Length::from_micrometres(
                    self.plate_front_offset.micrometres() + self.inset_depth.micrometres()
                ))
            )
        } else {
            length(self.plate_front_offset)
        };
        format!(
            "{}: K={} / {}={}; {}: Ø{} / {}; {}: H{}, {} / {}",
            loc.text(if inset {
                "pdf-supported-pair-inset"
            } else {
                "pdf-supported-pair"
            }),
            length(cup_edge_setback),
            if inset { "F" } else { "R" },
            length(table_value),
            loc.text("pdf-cup"),
            length(self.cup_diameter),
            length(self.cup_depth),
            loc.text("pdf-plate"),
            length(self.plate_height),
            pitch,
            front
        )
    }
}

pub fn diagnose(project: &Project, installation: &HingeInstallation) -> InstallationStatus {
    let mut issues = Vec::new();
    let door = project
        .boards
        .iter()
        .find(|b| b.id == installation.door_board_id);
    let mount = project
        .boards
        .iter()
        .find(|b| b.id == installation.mounting_board_id);
    if door.is_none() {
        issues.push(InstallationIssue::MissingPart(installation.door_board_id));
    }
    if mount.is_none() {
        issues.push(InstallationIssue::MissingPart(
            installation.mounting_board_id,
        ));
    }
    let catalog = project
        .catalog
        .iter()
        .find(|c| c.id == installation.catalog_id);
    if catalog.is_none() {
        issues.push(InstallationIssue::MissingCatalog(installation.catalog_id));
    }
    let verified = catalog.and_then(|c| Some((c, hardware_catalog::facts(c)?)));
    if catalog.is_some() && verified.is_none() {
        issues.push(InstallationIssue::MissingVerifiedCatalog);
    }
    let mut references = None;
    if let (Some(door), Some(mount), Some((entry, facts))) = (door, mount, verified) {
        if door.thickness < facts.door_thickness_min
            || door.thickness > facts.door_thickness_max
            || door.thickness < facts.cup_depth
        {
            issues.push(InstallationIssue::UnsupportedThickness);
        }
        let supported = hardware_catalog::supported_setting(
            entry,
            installation.cup_edge_setback,
            installation.overlay,
        );
        if !supported {
            issues.push(InstallationIssue::UnsupportedOverlay);
        }
        // Even with an unsupported K, the known cup envelope can be checked. Do
        // not emit a numeric reference for an unverified K/R combination.
        let radius = i128::from(facts.cup_diameter.micrometres()) / 2;
        let k = i128::from(installation.cup_edge_setback.micrometres());
        let [cup_x, cup_y] = edge_point(
            installation.side.door_edge,
            door.length,
            door.width,
            k + radius,
            i128::from(installation.door_y.micrometres()),
        );
        if !inside([cup_x, cup_y], door.length, door.width, [radius, radius])
            || i128::from(facts.cup_depth.micrometres()) > i128::from(door.thickness.micrometres())
        {
            issues.push(InstallationIssue::CupOutsideDoor);
        }
        let inset_depth = if facts.arm.is_inset() {
            if installation.inset_depth < door.thickness {
                issues.push(InstallationIssue::InsetShallowerThanDoor);
            }
            installation.inset_depth
        } else {
            Length::ZERO
        };
        let front = installation.side.mount_front_edge;
        let [plate_x, plate_y] = edge_point(
            front,
            mount.length,
            mount.width,
            i128::from(facts.plate_front_offset.micrometres())
                + i128::from(inset_depth.micrometres()),
            i128::from(installation.mount_y.micrometres()),
        );
        // The two plate holes sit either side of the centre, along the hinge line.
        let half_pitch = i128::from(facts.plate_hole_pitch.micrometres()) / 2;
        let mut margin = [0, 0];
        margin[front.along_axis()] = half_pitch;
        if !inside([plate_x, plate_y], mount.length, mount.width, margin) {
            issues.push(InstallationIssue::PlateOutsideMount);
        }
        let hole = |sign: i128| {
            let mut point = [plate_x, plate_y];
            point[front.along_axis()] += sign * half_pitch;
            [
                point[0],
                point[1],
                face_z(installation.side.mount_face, mount.thickness),
            ]
        };
        if supported {
            references = Some(InstallationReferences {
                cup_center_um: [
                    cup_x,
                    cup_y,
                    face_z(installation.side.door_face, door.thickness),
                ],
                cup_face: installation.side.door_face,
                cup_diameter: facts.cup_diameter,
                cup_depth: facts.cup_depth,
                plate_hole_centers_um: [hole(-1), hole(1)],
                plate_face: installation.side.mount_face,
                plate_height: facts.plate_height,
                plate_hole_pitch: facts.plate_hole_pitch,
                plate_front_offset: facts.plate_front_offset,
                arm: facts.arm,
                inset_depth,
                product_id: entry.product_id.clone(),
                plate_id: entry.plate_id.clone(),
                source: format!("{}; {}", entry.source, entry.revision),
                attribution: facts.attribution.clone(),
                printed_page: facts.printed_page,
                pdf_page: facts.pdf_page,
                fasteners_available: false,
                trust: hardware_catalog::trust(entry)
                    .unwrap_or(hardware_catalog::Trust::UserSupplied),
                pack: entry
                    .origin
                    .as_ref()
                    .map(|o| format!("{} {}", o.manufacturer, o.pack_version)),
            });
        }
    }
    InstallationStatus {
        id: installation.id,
        issues,
        references,
    }
}

/// Door and cabinet side for trying a catalog record outside any project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BenchSetup {
    pub door_thickness: Length,
    pub side_thickness: Length,
    pub cup_edge_setback: Length,
    /// R for overlay arms, F for inset.
    pub table_value: Length,
    /// E, used by inset arms only.
    pub inset_depth: Length,
}

/// Diagnose `entry` on a synthetic 600 × 400 mm door hinged on a 560 × 400 mm
/// side, with the hinge centred on both. The same checks as a real project,
/// so the catalog test bench can never disagree with an installation.
pub fn bench(entry: &crate::domain::CatalogReference, setup: BenchSetup) -> InstallationStatus {
    use crate::domain::{Board, BoardGrain, HingeMountingSide, Material};
    use crate::units::Pose;
    let mut project = Project::new("Bench", crate::money::Currency::Brl);
    let material = Uuid::new_v4();
    project.materials.push(Material {
        id: material,
        name: "Bench".into(),
        default_thickness: setup.door_thickness,
        default_grain: BoardGrain::Unrestricted,
    });
    let board = |name: &str, length_mm: i64, thickness: Length| Board {
        id: Uuid::new_v4(),
        name: name.into(),
        material_id: material,
        length: Length::from_micrometres(length_mm * 1_000),
        width: Length::from_micrometres(400_000),
        thickness,
        grain_override: None,
        parent_id: None,
        pose: Pose::IDENTITY,
    };
    let door = board("Door", 600, setup.door_thickness);
    let side = board("Side", 560, setup.side_thickness);
    let mut entry = entry.clone();
    entry.id = Uuid::new_v4();
    let installation = HingeInstallation {
        id: Uuid::new_v4(),
        door_board_id: door.id,
        mounting_board_id: side.id,
        catalog_id: entry.id,
        side: HingeMountingSide {
            door_edge: BoardEdge::MinX,
            door_face: BoardFace::MinZ,
            mount_front_edge: BoardEdge::MinX,
            mount_face: BoardFace::MaxZ,
        },
        door_y: Length::from_micrometres(200_000),
        mount_y: Length::from_micrometres(200_000),
        cup_edge_setback: setup.cup_edge_setback,
        overlay: setup.table_value,
        inset_depth: setup.inset_depth,
    };
    project.boards.extend([door, side]);
    project.catalog.push(entry);
    diagnose(&project, &installation)
}

pub fn diagnose_all(project: &Project) -> Vec<InstallationStatus> {
    project
        .hinge_installations
        .iter()
        .map(|i| diagnose(project, i))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallationEditError {
    MissingInstallation,
    InvalidDistance,
    IncompatibleJoint,
}

/// Why the door and cabinet side could not be matched automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitError {
    MissingPart,
    /// No door edge lies parallel to the side's face, e.g. the side is
    /// turned 90° to the door.
    NotParallel,
    /// The cup's height falls beyond the ends of the side.
    OutsideMount,
}

/// Mounting sides and plate position that line the plate up with a cup
/// `door_y` along the door's hinge edge, found from where the two boards sit
/// in the model: the hinge edge is the door edge nearest the side, the cup
/// goes on the door face toward the side, the plate on the side face toward
/// the door, measured from the side's edge nearest the door.
pub fn fit(
    project: &Project,
    door_id: Uuid,
    mount_id: Uuid,
    door_y: Length,
) -> Result<(HingeMountingSide, Length), FitError> {
    let door = BoardFrame::new(project, door_id).ok_or(FitError::MissingPart)?;
    let mount = BoardFrame::new(project, mount_id).ok_or(FitError::MissingPart)?;
    let door_edge = BoardEdge::ALL
        .into_iter()
        .filter(|edge| {
            let along = door.axes[edge.along_axis()];
            dot(along, mount.axes[2]).abs() < 0.02
                && (0..2).any(|a| dot(along, mount.axes[a]).abs() > 0.999)
        })
        .min_by(|a, b| {
            mount
                .distance(door.edge_mid(*a))
                .total_cmp(&mount.distance(door.edge_mid(*b)))
        })
        .ok_or(FitError::NotParallel)?;
    let hinge_line = door.axes[door_edge.along_axis()];
    let mount_along = if dot(hinge_line, mount.axes[1]).abs() > 0.999 {
        1
    } else {
        0
    };
    let front_candidates = if mount_along == 1 {
        [BoardEdge::MinX, BoardEdge::MaxX]
    } else {
        [BoardEdge::MinY, BoardEdge::MaxY]
    };
    let [a, b] = front_candidates.map(|edge| door.distance(mount.edge_mid(edge)));
    let mount_front_edge = if a <= b {
        front_candidates[0]
    } else {
        front_candidates[1]
    };
    let (door_centre, mount_centre) = (door.centre(), mount.centre());
    let towards = |from: [f64; 3], to: [f64; 3]| std::array::from_fn(|i| to[i] - from[i]);
    let face = |positive: bool| {
        if positive {
            BoardFace::MaxZ
        } else {
            BoardFace::MinZ
        }
    };
    let side = HingeMountingSide {
        door_edge,
        door_face: face(dot(door.axes[2], towards(door_centre, mount_centre)) >= 0.0),
        mount_front_edge,
        mount_face: face(dot(mount.axes[2], towards(mount_centre, door_centre)) >= 0.0),
    };
    let [x, y] = edge_point(
        door_edge,
        Length::from_micrometres((door.size[0] * 1000.0).round() as i64),
        Length::from_micrometres((door.size[1] * 1000.0).round() as i64),
        0,
        i128::from(door_y.micrometres()),
    )
    .map(|v| v as f64 / 1000.0);
    let along = mount.local(door.world([x, y, 0.0]))[mount_along];
    if along < -0.0005 || along > mount.size[mount_along] + 0.0005 {
        return Err(FitError::OutsideMount);
    }
    let mount_y = Length::from_micrometres((along.max(0.0) * 1000.0).round() as i64);
    Ok((side, mount_y))
}

/// The board a door most likely hangs from: the nearest board standing at
/// right angles to it that a hinge can be fitted to, preferring the door's
/// long edges (a neighbouring door in the same plane never qualifies).
pub fn likely_mount(project: &Project, door_id: Uuid) -> Option<Uuid> {
    let door = BoardFrame::new(project, door_id)?;
    project
        .boards
        .iter()
        .filter(|b| b.id != door_id)
        .filter_map(|b| {
            let mount = BoardFrame::new(project, b.id)?;
            if dot(door.axes[2], mount.axes[2]).abs() > 0.02 {
                return None;
            }
            // Probe halfway along either door axis: one is the hinge edge.
            let (side, _) = door.size[..2].iter().find_map(|half| {
                let y = Length::from_micrometres((half * 500.0).round() as i64);
                fit(project, door_id, b.id, y).ok()
            })?;
            // Doors normally hang from a long edge; top-hung flaps are rarer.
            let along = side.door_edge.along_axis();
            let short_edge = door.size[along] < door.size[1 - along];
            Some((
                short_edge,
                mount.distance(door.edge_mid(side.door_edge)),
                b.id,
            ))
        })
        .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))
        .map(|(_, _, id)| id)
}

/// The same hinge with its sides and plate refitted for its door position.
pub fn fitted(
    project: &Project,
    installation: &HingeInstallation,
) -> Result<HingeInstallation, FitError> {
    let (side, mount_y) = fit(
        project,
        installation.door_board_id,
        installation.mounting_board_id,
        installation.door_y,
    )?;
    Ok(HingeInstallation {
        side,
        mount_y,
        ..installation.clone()
    })
}

/// Usual number of hinges for a door edge of this length.
pub fn recommended_count(edge: Length) -> usize {
    match edge.micrometres() {
        ..=900_000 => 2,
        900_001..=1_500_000 => 3,
        1_500_001..=2_000_000 => 4,
        _ => 5,
    }
}

/// `count` hinge positions along an edge: 100 mm in from each end (less on
/// short doors), the rest evenly between, rounded to whole millimetres.
pub fn standard_positions(edge: Length, count: usize) -> Vec<Length> {
    let edge = edge.micrometres();
    let end = 100_000.min(edge / 4);
    let mm = |um: i64| Length::from_micrometres((um + 500).div_euclid(1000) * 1000);
    match count {
        0 => Vec::new(),
        1 => vec![mm(edge / 2)],
        n => (0..n)
            .map(|i| mm(end + (edge - 2 * end) * i as i64 / (n as i64 - 1)))
            .collect(),
    }
}

/// A free spot for one more hinge on the door: of the standard positions
/// for one more hinge (at least two), the first that lies farthest from the
/// existing ones.
pub fn next_position(edge: Length, taken: &[Length]) -> Length {
    let gap = |candidate: Length| {
        taken
            .iter()
            .map(|t| (t.micrometres() - candidate.micrometres()).abs())
            .min()
            .unwrap_or(i64::MAX)
    };
    standard_positions(edge, (taken.len() + 1).max(2))
        .into_iter()
        .fold(None, |best: Option<Length>, candidate| match best {
            Some(best) if gap(best) >= gap(candidate) => Some(best),
            _ => Some(candidate),
        })
        .unwrap_or(Length::ZERO)
}

fn check_inputs(i: &HingeInstallation) -> Result<(), InstallationEditError> {
    if [
        i.door_y,
        i.mount_y,
        i.cup_edge_setback,
        i.overlay,
        i.inset_depth,
    ]
    .iter()
    .any(|n| n.micrometres() < 0)
    {
        Err(InstallationEditError::InvalidDistance)
    } else {
        Ok(())
    }
}

/// Disposable preview, including diagnostics for references not yet in the project.
pub fn preview(
    project: &Project,
    proposed: &HingeInstallation,
) -> Result<InstallationStatus, InstallationEditError> {
    check_inputs(proposed)?;
    Ok(diagnose(project, proposed))
}

pub fn create(
    editor: &mut ProjectEditor,
    installation: HingeInstallation,
) -> Result<InstallationStatus, EditError<InstallationEditError>> {
    check_inputs(&installation).map_err(EditError::Command)?;
    let id = installation.id;
    editor.transact(|p| -> Result<(), InstallationEditError> {
        p.hinge_installations.push(installation);
        Ok(())
    })?;
    Ok(diagnose(
        editor.project(),
        editor
            .project()
            .hinge_installations
            .iter()
            .find(|i| i.id == id)
            .expect("committed installation"),
    ))
}

pub fn update(
    editor: &mut ProjectEditor,
    installation: HingeInstallation,
) -> Result<InstallationStatus, EditError<InstallationEditError>> {
    check_inputs(&installation).map_err(EditError::Command)?;
    let id = installation.id;
    editor.transact(|p| {
        let current = p
            .hinge_installations
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or(InstallationEditError::MissingInstallation)?;
        *current = installation;
        crate::door_joint::validate_joints(p)
            .map_err(|_| InstallationEditError::IncompatibleJoint)?;
        Ok(())
    })?;
    Ok(diagnose(
        editor.project(),
        editor
            .project()
            .hinge_installations
            .iter()
            .find(|i| i.id == id)
            .expect("committed installation"),
    ))
}

/// Move a hinge to `door_y` on its door and bring its plate along. The plate
/// and sides are refitted from the model when possible; otherwise the plate
/// moves by the same distance as the cup.
pub fn move_to(
    editor: &mut ProjectEditor,
    id: Uuid,
    door_y: Length,
) -> Result<InstallationStatus, EditError<InstallationEditError>> {
    let current = editor
        .project()
        .hinge_installations
        .iter()
        .find(|i| i.id == id)
        .cloned()
        .ok_or(EditError::Command(
            InstallationEditError::MissingInstallation,
        ))?;
    update(editor, placed(editor.project(), &current, door_y))
}

fn placed(project: &Project, current: &HingeInstallation, door_y: Length) -> HingeInstallation {
    let moved = HingeInstallation {
        door_y,
        ..current.clone()
    };
    fitted(project, &moved).unwrap_or_else(|_| HingeInstallation {
        mount_y: Length::from_micrometres(
            (current.mount_y.micrometres() + door_y.micrometres() - current.door_y.micrometres())
                .max(0),
        ),
        ..moved
    })
}

/// Spread every hinge on a door evenly along its hinge edge, in their
/// current order, and line each plate up. One undo step.
pub fn space_evenly(
    editor: &mut ProjectEditor,
    door_board_id: Uuid,
) -> Result<bool, EditError<InstallationEditError>> {
    let project = editor.project();
    let mut hinges: Vec<_> = project
        .hinge_installations
        .iter()
        .filter(|i| i.door_board_id == door_board_id)
        .cloned()
        .collect();
    let door = project
        .boards
        .iter()
        .find(|b| b.id == door_board_id)
        .ok_or(EditError::Command(
            InstallationEditError::MissingInstallation,
        ))?;
    let Some(first) = hinges.first() else {
        return Err(EditError::Command(
            InstallationEditError::MissingInstallation,
        ));
    };
    let edge = edge_length(first.side.door_edge, door.length, door.width);
    hinges.sort_by_key(|h| h.door_y);
    let positions = standard_positions(edge, hinges.len());
    let moved: Vec<_> = hinges
        .iter()
        .zip(positions)
        .map(|(hinge, y)| placed(project, hinge, y))
        .collect();
    for hinge in &moved {
        check_inputs(hinge).map_err(EditError::Command)?;
    }
    editor.transact(|p| {
        for hinge in moved {
            if let Some(slot) = p.hinge_installations.iter_mut().find(|i| i.id == hinge.id) {
                *slot = hinge;
            }
        }
        crate::door_joint::validate_joints(p).map_err(|_| InstallationEditError::IncompatibleJoint)
    })
}

pub fn remove(
    editor: &mut ProjectEditor,
    id: Uuid,
) -> Result<bool, EditError<InstallationEditError>> {
    editor.transact(|p| {
        let index = p
            .hinge_installations
            .iter()
            .position(|i| i.id == id)
            .ok_or(InstallationEditError::MissingInstallation)?;
        p.hinge_installations.remove(index);
        for joint in &mut p.door_joints {
            joint.hinge_installation_ids.retain(|hinge| *hinge != id);
        }
        p.door_joints
            .retain(|joint| !joint.hinge_installation_ids.is_empty());
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Board, BoardEdge, BoardFace, BoardGrain, HingeArm, HingeMountingSide, Material,
    };
    use crate::money::Currency;
    use crate::persistence::prepare_bytes;
    use crate::units::{Pose, Quaternion};

    fn mm(v: i64) -> Length {
        Length::from_micrometres(v * 1000)
    }

    fn fixture() -> (Project, HingeInstallation) {
        let mut p = Project::new("Hinges", Currency::Brl);
        let material = Uuid::new_v4();
        p.materials.push(Material {
            id: material,
            name: "ply".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Unrestricted,
        });
        let pose = Pose::new([500.0, 12.0, -30.0], Quaternion::IDENTITY).unwrap();
        for name in ["door", "mount"] {
            p.boards.push(Board {
                id: Uuid::new_v4(),
                name: name.into(),
                material_id: material,
                length: mm(100),
                width: mm(100),
                thickness: mm(18),
                grain_override: None,
                parent_id: None,
                pose,
            });
        }
        let catalog = hardware_catalog::builtin_hinge();
        let i = HingeInstallation {
            id: Uuid::new_v4(),
            door_board_id: p.boards[0].id,
            mounting_board_id: p.boards[1].id,
            catalog_id: catalog.id,
            side: HingeMountingSide {
                door_edge: BoardEdge::MinX,
                door_face: BoardFace::MinZ,
                mount_front_edge: BoardEdge::MinX,
                mount_face: BoardFace::MaxZ,
            },
            door_y: mm(50),
            mount_y: mm(50),
            cup_edge_setback: mm(3),
            overlay: mm(15),
            inset_depth: Default::default(),
        };
        p.catalog.push(catalog);
        (p, i)
    }

    /// World cup centre and plate centre of a fitted hinge.
    fn world_points(p: &Project, i: &HingeInstallation) -> ([f64; 3], [f64; 3]) {
        let status = diagnose(p, i);
        assert!(status.issues.is_empty(), "{:?}", status.issues);
        let r = status.references.unwrap();
        let door = BoardFrame::new(p, i.door_board_id).unwrap();
        let mount = BoardFrame::new(p, i.mounting_board_id).unwrap();
        let w = |frame: &BoardFrame, v: [i128; 3]| frame.world(v.map(|n| n as f64 / 1000.0));
        let [a, b] = r.plate_hole_centers_um.map(|h| w(&mount, h));
        (
            w(&door, r.cup_center_um),
            std::array::from_fn(|k| (a[k] + b[k]) / 2.0),
        )
    }

    #[test]
    fn fit_reproduces_the_reference_cabinet_hinges() {
        let p = crate::reference_fixture::project();
        for original in &p.hinge_installations {
            let refit = fitted(&p, original).unwrap();
            assert_eq!(refit.side.door_edge, original.side.door_edge);
            assert_eq!(refit.side.door_face, original.side.door_face);
            assert_eq!(refit.side.mount_face, original.side.mount_face);
            // Doors start 2 mm above the sides.
            assert_eq!(refit.mount_y, mm(original.door_y.micrometres() / 1000 + 2));
        }
    }

    #[test]
    fn fit_lines_up_plate_on_a_side_whose_height_runs_along_x() {
        // Template sides: local X is height, Y is depth (front at Y = 0), Z
        // points out of the cabinet. The door stands in front, grain upright.
        let (mut p, mut i) = fixture();
        let side = Quaternion::normalized(1.0, 0.0, -1.0, 0.0).unwrap();
        let upright = Quaternion::normalized(1.0, 1.0, 0.0, 0.0).unwrap();
        p.boards[1].length = mm(720);
        p.boards[1].width = mm(560);
        p.boards[1].pose = Pose::new([18.0, 0.0, 0.0], side).unwrap();
        p.boards[0].length = mm(400);
        p.boards[0].width = mm(716);
        p.boards[0].pose = Pose::new([0.0, 0.0, 2.0], upright).unwrap();
        i.door_y = mm(100);
        let fit = fitted(&p, &i).unwrap();
        assert_eq!(fit.side.door_edge, BoardEdge::MinX);
        assert_eq!(fit.side.mount_front_edge, BoardEdge::MinY);
        assert_eq!(fit.mount_y, mm(102));
        let (cup, plate) = world_points(&p, &fit);
        // Same height, plate inside the cabinet behind the door's hinge edge.
        assert!((cup[2] - plate[2]).abs() < 1e-6, "{cup:?} {plate:?}");
        assert!(plate[0] > 0.0 && plate[0] < 18.0 + 1e-6 && plate[1] > 0.0);
    }

    #[test]
    fn fit_refuses_a_side_turned_across_the_door() {
        let (mut p, i) = fixture();
        // Turned 45° about Z: no door edge runs along the side.
        let turn = std::f64::consts::FRAC_PI_8;
        p.boards[1].pose = Pose::new(
            [0.0, 0.0, 0.0],
            Quaternion::normalized(turn.cos(), 0.0, 0.0, turn.sin()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            fit(&p, i.door_board_id, i.mounting_board_id, i.door_y),
            Err(FitError::NotParallel)
        );
    }

    #[test]
    fn likely_mount_is_the_side_not_the_neighbouring_door() {
        let p = crate::reference_fixture::project();
        use crate::reference_fixture::{LEFT_DOOR_ID, RIGHT_DOOR_ID};
        let side = |door| {
            p.hinge_installations
                .iter()
                .find(|h| h.door_board_id == door)
                .unwrap()
                .mounting_board_id
        };
        assert_eq!(likely_mount(&p, LEFT_DOOR_ID), Some(side(LEFT_DOOR_ID)));
        assert_eq!(likely_mount(&p, RIGHT_DOOR_ID), Some(side(RIGHT_DOOR_ID)));
    }

    #[test]
    fn standard_positions_keep_100_mm_ends_and_even_gaps() {
        assert_eq!(standard_positions(mm(716), 2), vec![mm(100), mm(616)]);
        assert_eq!(
            standard_positions(mm(1400), 3),
            vec![mm(100), mm(700), mm(1300)]
        );
        assert_eq!(standard_positions(mm(200), 2), vec![mm(50), mm(150)]);
        assert_eq!(recommended_count(mm(716)), 2);
        assert_eq!(recommended_count(mm(1800)), 4);
        assert_eq!(next_position(mm(716), &[]), mm(100));
        assert_eq!(next_position(mm(716), &[mm(100)]), mm(616));
        assert_eq!(next_position(mm(716), &[mm(100), mm(616)]), mm(358));
    }

    #[test]
    fn moving_a_hinge_brings_its_plate_and_spacing_is_one_undo() {
        let p = crate::reference_fixture::project();
        let mut editor = ProjectEditor::new(p).unwrap();
        let first = editor.project().hinge_installations[0].id;
        let left = &editor.project().door_joints[0];
        assert!(!crate::door_joint::needs_review(editor.project(), left));
        move_to(&mut editor, first, mm(150)).unwrap();
        let moved = &editor.project().hinge_installations[0];
        assert_eq!((moved.door_y, moved.mount_y), (mm(150), mm(152)));
        let door = moved.door_board_id;
        let revision = editor.project().revision;
        assert!(space_evenly(&mut editor, door).unwrap());
        assert_eq!(editor.project().revision, revision + 1);
        let ys: Vec<_> = editor
            .project()
            .hinge_installations
            .iter()
            .filter(|h| h.door_board_id == door)
            .map(|h| (h.door_y, h.mount_y))
            .collect();
        assert_eq!(ys, vec![(mm(100), mm(102)), (mm(616), mm(618))]);
        // Sliding hinges along the hinge line leaves the door confirmed.
        let joint = editor
            .project()
            .door_joints
            .iter()
            .find(|j| j.moving_root_id == door)
            .unwrap();
        assert!(!crate::door_joint::needs_review(editor.project(), joint));
        editor.undo().unwrap();
        assert_eq!(editor.project().hinge_installations[0].door_y, mm(150));
    }

    #[test]
    fn inset_plate_moves_by_e_and_shallow_e_is_flagged() {
        let registry = crate::catalog_pack::CatalogRegistry::bundled();
        let pack = registry.pack("fgvtn").unwrap();
        let family = pack
            .hinges
            .iter()
            .find(|f| f.id == "tn-ms-slow-calco-fixo")
            .unwrap();
        let alta = family
            .variants
            .iter()
            .find(|v| v.arm == HingeArm::Inset)
            .unwrap();
        let entry = crate::catalog_pack::snapshot(pack, family, alta, "en");
        let setup = BenchSetup {
            door_thickness: mm(18),
            side_thickness: mm(18),
            cup_edge_setback: mm(4),
            table_value: mm(3),
            inset_depth: mm(18),
        };
        let status = bench(&entry, setup);
        assert_eq!(status.issues, []);
        let refs = status.references.unwrap();
        assert_eq!(refs.arm, HingeArm::Inset);
        // Plate at 37 + E from the side's front edge; K + Ø/2 on the door.
        assert_eq!(refs.plate_hole_centers_um[0][0], 55_000);
        assert_eq!(refs.cup_center_um[0], 21_500);
        // No pitch on this sheet: both references are the plate centre line.
        assert_eq!(refs.plate_hole_centers_um[0], refs.plate_hole_centers_um[1]);
        let shallow = bench(
            &entry,
            BenchSetup {
                inset_depth: mm(10),
                ..setup
            },
        );
        assert!(
            shallow
                .issues
                .contains(&InstallationIssue::InsetShallowerThanDoor)
        );
        // An overlay value is not in the inset table.
        let wrong = bench(
            &entry,
            BenchSetup {
                table_value: mm(15),
                ..setup
            },
        );
        assert!(
            wrong
                .issues
                .contains(&InstallationIssue::UnsupportedOverlay)
        );
        assert!(wrong.references.is_none());
        // Overlay arms ignore E entirely.
        let reta = &family.variants[0];
        let overlay = crate::catalog_pack::snapshot(pack, family, reta, "en");
        let refs = bench(
            &overlay,
            BenchSetup {
                table_value: mm(15),
                ..setup
            },
        )
        .references
        .unwrap();
        assert_eq!(refs.plate_hole_centers_um[0][0], 37_000);
        assert_eq!(refs.inset_depth, Length::ZERO);
    }

    #[test]
    fn exact_references_face_and_edge_geometry() {
        let (mut p, mut i) = fixture();
        let status = diagnose(&p, &i);
        assert!(status.issues.is_empty());
        let refs = status.references.unwrap();
        assert_eq!(refs.cup_center_um, [20_500, 50_000, 0]);
        assert_eq!(
            refs.plate_hole_centers_um,
            [[37_000, 34_000, 18_000], [37_000, 66_000, 18_000]]
        );
        assert_eq!(refs.cup_depth.micrometres(), 11_300);
        assert!(!refs.fasteners_available);
        i.side.door_edge = BoardEdge::MaxX;
        i.side.door_face = BoardFace::MaxZ;
        i.side.mount_front_edge = BoardEdge::MaxX;
        i.side.mount_face = BoardFace::MinZ;
        let refs = diagnose(&p, &i).references.unwrap();
        assert_eq!(refs.cup_center_um, [79_500, 50_000, 18_000]);
        assert_eq!(refs.plate_hole_centers_um[0], [63_000, 34_000, 0]);
        p.boards[0].length = mm(37);
        i.cup_edge_setback = mm(6);
        i.overlay = mm(18);
        assert!(
            diagnose(&p, &i)
                .issues
                .contains(&InstallationIssue::CupOutsideDoor)
        );
        p.boards[1].length = mm(36);
        assert!(
            diagnose(&p, &i)
                .issues
                .contains(&InstallationIssue::PlateOutsideMount)
        );
    }

    #[test]
    fn warnings_are_drafts_and_edits_undo_without_moving_annotations() {
        let (p, i) = fixture();
        let mut editor = ProjectEditor::new(p).unwrap();
        let created = create(&mut editor, i.clone()).unwrap();
        assert!(created.issues.is_empty());
        let mut other = i.clone();
        other.id = Uuid::new_v4();
        other.door_y = mm(90);
        assert!(
            create(&mut editor, other.clone())
                .unwrap()
                .issues
                .contains(&InstallationIssue::CupOutsideDoor)
        );
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].thickness = mm(14);
                p.boards[1].length = mm(36);
                Ok(())
            })
            .unwrap();
        let issues = &diagnose_all(editor.project())[0].issues;
        assert!(issues.contains(&InstallationIssue::UnsupportedThickness));
        assert!(issues.contains(&InstallationIssue::PlateOutsideMount));
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].thickness = mm(23);
                Ok(())
            })
            .unwrap();
        assert!(
            diagnose_all(editor.project())[0]
                .issues
                .contains(&InstallationIssue::UnsupportedThickness)
        );
        assert_eq!(editor.project().hinge_installations[0].door_y, mm(50));
        let mut edited = i.clone();
        edited.cup_edge_setback = mm(4);
        edited.overlay = mm(15);
        assert!(
            update(&mut editor, edited.clone())
                .unwrap()
                .issues
                .contains(&InstallationIssue::UnsupportedOverlay)
        );
        assert!(diagnose_all(editor.project())[0].references.is_none());
        assert_eq!(editor.undo(), Ok(true));
        assert_eq!(editor.project().hinge_installations[0], i);
        assert!(remove(&mut editor, other.id).unwrap());
        assert_eq!(editor.undo(), Ok(true));
        assert_eq!(editor.project().hinge_installations.len(), 2);
        edited.door_y = mm(-1);
        assert_eq!(
            update(&mut editor, edited),
            Err(EditError::Command(InstallationEditError::InvalidDistance))
        );
    }

    #[test]
    fn missing_references_rejected_on_load_and_legacy_catalog_has_no_guidance() {
        let (mut p, i) = fixture();
        p.hinge_installations.push(i.clone());
        assert_eq!(p.validate(), Ok(()));
        let reopened = prepare_bytes(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(reopened.project().hinge_installations, vec![i.clone()]);
        p.catalog[0].verified_hinge = None;
        let status = diagnose(&p, &i);
        assert_eq!(
            status.issues,
            vec![InstallationIssue::MissingVerifiedCatalog]
        );
        assert!(status.references.is_none());
        let missing = Uuid::new_v4();
        p.hinge_installations[0].door_board_id = missing;
        assert_eq!(
            p.validate(),
            Err(crate::domain::DomainError::DanglingReference {
                owner: i.id,
                target: missing
            })
        );
        assert!(
            diagnose(&p, &p.hinge_installations[0])
                .issues
                .contains(&InstallationIssue::MissingPart(missing))
        );
    }

    #[test]
    fn catalog_refresh_returns_rechecked_installations() {
        let (mut p, i) = fixture();
        p.catalog[0].verified_hinge = None;
        p.hinge_installations.push(i.clone());
        let mut editor = ProjectEditor::new(p).unwrap();
        let (changed, statuses) =
            hardware_catalog::update_from_builtin_with_status(&mut editor, i.catalog_id).unwrap();
        assert!(changed);
        assert_eq!(statuses.len(), 1);
        assert!(statuses[0].issues.is_empty());
        assert!(statuses[0].references.is_some());
        editor.undo().unwrap();
        assert_eq!(
            diagnose_all(editor.project())[0].issues,
            vec![InstallationIssue::MissingVerifiedCatalog]
        );
    }

    #[test]
    fn installation_changes_packet_identity_but_not_wood_identity() {
        let (mut p, i) = fixture();
        let before = crate::export::fingerprint(&p);
        p.hinge_installations.push(i.clone());
        let added = crate::export::fingerprint(&p);
        assert_eq!(before.wood, added.wood);
        assert_ne!(before.packet, added.packet);
        p.hinge_installations[0].mount_y = mm(49);
        let moved = crate::export::fingerprint(&p);
        assert_eq!(added.wood, moved.wood);
        assert_ne!(added.packet, moved.packet);
    }
}
