//! Read-only, board-local hinge annotations and atomic installation edits.
//! Coordinates below are integer micrometres from each board's minimum XYZ corner.
use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::{BoardEdge, BoardFace, HingeInstallation, Project};
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallationStatus {
    pub id: Uuid,
    pub issues: Vec<InstallationIssue>,
    /// Available only for an exactly verified kit, its plate and a supported K/R pair.
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
    pub plate_hole_pitch: Length,
    pub plate_front_offset: Length,
    pub product_id: String,
    pub plate_id: String,
    pub source: String,
    pub attribution: String,
    pub printed_page: u16,
    pub pdf_page: u16,
    /// No pilot diameter, pilot depth, fastener or cup screw coordinates are supplied.
    pub fasteners_available: bool,
}

fn along_x(edge: BoardEdge, length: Length, offset: i128) -> i128 {
    match edge {
        BoardEdge::MinX => offset,
        BoardEdge::MaxX => i128::from(length.micrometres()) - offset,
    }
}

fn face_z(face: BoardFace, thickness: Length) -> i128 {
    match face {
        BoardFace::MinZ => 0,
        BoardFace::MaxZ => i128::from(thickness.micrometres()),
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
    let verified = catalog.filter(|c| hardware_catalog::is_verified(c));
    if catalog.is_some() && verified.is_none() {
        issues.push(InstallationIssue::MissingVerifiedCatalog);
    }
    let mut references = None;
    if let (Some(door), Some(mount), Some(entry)) = (door, mount, verified) {
        let facts = entry
            .verified_hinge
            .as_ref()
            .expect("verified catalog facts");
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
        let cup_x = along_x(installation.side.door_edge, door.length, k + radius);
        let cup_y = i128::from(installation.door_y.micrometres());
        if cup_x - radius < 0
            || cup_x + radius > i128::from(door.length.micrometres())
            || cup_y - radius < 0
            || cup_y + radius > i128::from(door.width.micrometres())
            || i128::from(facts.cup_depth.micrometres()) > i128::from(door.thickness.micrometres())
        {
            issues.push(InstallationIssue::CupOutsideDoor);
        }
        let plate_x = along_x(
            installation.side.mount_front_edge,
            mount.length,
            i128::from(facts.plate_front_offset.micrometres()),
        );
        let plate_y = i128::from(installation.mount_y.micrometres());
        let half_pitch = i128::from(facts.plate_hole_pitch.micrometres()) / 2;
        if plate_x < 0
            || plate_x > i128::from(mount.length.micrometres())
            || plate_y - half_pitch < 0
            || plate_y + half_pitch > i128::from(mount.width.micrometres())
        {
            issues.push(InstallationIssue::PlateOutsideMount);
        }
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
                plate_hole_centers_um: [
                    [
                        plate_x,
                        plate_y - half_pitch,
                        face_z(installation.side.mount_face, mount.thickness),
                    ],
                    [
                        plate_x,
                        plate_y + half_pitch,
                        face_z(installation.side.mount_face, mount.thickness),
                    ],
                ],
                plate_face: installation.side.mount_face,
                plate_height: facts.plate_height,
                plate_hole_pitch: facts.plate_hole_pitch,
                plate_front_offset: facts.plate_front_offset,
                product_id: entry.product_id.clone(),
                plate_id: entry.plate_id.clone().expect("verified plate"),
                source: format!("{}; {}", entry.source, entry.revision),
                attribution: facts.attribution.clone(),
                printed_page: facts.printed_page,
                pdf_page: facts.pdf_page,
                fasteners_available: false,
            });
        }
    }
    InstallationStatus {
        id: installation.id,
        issues,
        references,
    }
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

fn check_inputs(i: &HingeInstallation) -> Result<(), InstallationEditError> {
    if [i.door_y, i.mount_y, i.cup_edge_setback, i.overlay]
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
    use crate::domain::{Board, BoardEdge, BoardFace, BoardGrain, HingeMountingSide, Material};
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
        };
        p.catalog.push(catalog);
        (p, i)
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
