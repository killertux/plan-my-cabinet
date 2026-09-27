//! Explicit, undoable door relationships and read-only derived motion poses.
use std::collections::HashSet;

use uuid::Uuid;

use crate::assembly_edit::world_pose;
use crate::commands::{EditError, ProjectEditor};
use crate::domain::{BoardEdge, BoardFace, DomainError, DoorJoint, Project};
use crate::hinge_installation::{InstallationStatus, diagnose};
use crate::units::{Pose, Quaternion};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JointError {
    MissingRoot,
    MissingMount,
    MissingHinge(Uuid),
    EmptyHinges,
    IncompatibleHinge(Uuid),
    OverlappingRoot,
    Cycle,
    InvalidAxis,
    InvalidPose,
    StalePreview,
    MissingJoint,
    UnverifiedLimit,
    NeedsReview,
    InvalidAngle,
}

/// IDs that receive the *same* display transform, including nested hardware.
pub fn moving_members(project: &Project, root: Uuid) -> Vec<Uuid> {
    let mut assemblies = HashSet::from([root]);
    loop {
        let before = assemblies.len();
        for a in &project.assemblies {
            if a.parent_id.is_some_and(|p| assemblies.contains(&p)) {
                assemblies.insert(a.id);
            }
        }
        if before == assemblies.len() {
            break;
        }
    }
    let mut members = Vec::new();
    if project.assemblies.iter().any(|a| a.id == root) {
        members.extend(
            project
                .assemblies
                .iter()
                .filter(|a| assemblies.contains(&a.id))
                .map(|a| a.id),
        );
    }
    members.extend(
        project
            .boards
            .iter()
            .filter(|b| b.id == root || b.parent_id.is_some_and(|p| assemblies.contains(&p)))
            .map(|b| b.id),
    );
    members.extend(
        project
            .hardware
            .iter()
            .filter(|h| h.parent_id.is_some_and(|p| assemblies.contains(&p)))
            .map(|h| h.id),
    );
    members
}

fn local_pose(project: &Project, root: Uuid) -> Option<Pose> {
    project
        .assemblies
        .iter()
        .find(|a| a.id == root)
        .map(|a| a.pose)
        .or_else(|| project.boards.iter().find(|b| b.id == root).map(|b| b.pose))
}

fn check(project: &Project, joint: &DoorJoint) -> Result<(), JointError> {
    if local_pose(project, joint.moving_root_id).is_none() {
        return Err(JointError::MissingRoot);
    }
    if !project
        .boards
        .iter()
        .any(|b| b.id == joint.mounting_board_id)
    {
        return Err(JointError::MissingMount);
    }
    if joint.hinge_installation_ids.is_empty() {
        return Err(JointError::EmptyHinges);
    }
    if moving_members(project, joint.moving_root_id).contains(&joint.mounting_board_id) {
        return Err(JointError::Cycle);
    }
    let norm = joint.axis_direction.iter().map(|v| v * v).sum::<f64>();
    if !norm.is_finite()
        || (norm - 1.0).abs() > 1e-9
        || joint
            .axis_origin_mm
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
    {
        return Err(JointError::InvalidAxis);
    }
    for pose in [joint.closed_local_pose, joint.closed_world_pose] {
        if Pose::new(pose.translation_mm, pose.rotation).is_err()
            || ((pose.rotation.w * pose.rotation.w
                + pose.rotation.x * pose.rotation.x
                + pose.rotation.y * pose.rotation.y
                + pose.rotation.z * pose.rotation.z)
                - 1.0)
                .abs()
                > 1e-9
        {
            return Err(JointError::InvalidPose);
        }
    }
    let mut ids = HashSet::new();
    let mut first = None;
    for &id in &joint.hinge_installation_ids {
        if !ids.insert(id) {
            return Err(JointError::IncompatibleHinge(id));
        }
        let hinge = project
            .hinge_installations
            .iter()
            .find(|h| h.id == id)
            .ok_or(JointError::MissingHinge(id))?;
        if hinge.mounting_board_id != joint.mounting_board_id
            || !moving_members(project, joint.moving_root_id).contains(&hinge.door_board_id)
        {
            return Err(JointError::IncompatibleHinge(id));
        }
        let key = (hinge.door_board_id, hinge.catalog_id, hinge.side);
        if first.is_some_and(|previous| previous != key) {
            return Err(JointError::IncompatibleHinge(id));
        }
        first = Some(key);
    }
    Ok(())
}

/// Structural invariants are checked on load and on every editor transaction.
pub(crate) fn validate_joints(project: &Project) -> Result<(), DomainError> {
    let mut roots = HashSet::new();
    let mut hinges = HashSet::new();
    for joint in &project.door_joints {
        check(project, joint).map_err(|_| DomainError::InvalidDoorJoint(joint.id))?;
        if !roots.insert(joint.moving_root_id)
            || joint
                .hinge_installation_ids
                .iter()
                .any(|id| !hinges.insert(*id))
        {
            return Err(DomainError::InvalidDoorJoint(joint.id));
        }
    }
    for joint in &project.door_joints {
        let members = moving_members(project, joint.moving_root_id);
        for other in &project.door_joints {
            if other.id != joint.id
                && (members.contains(&other.moving_root_id)
                    || members.contains(&other.mounting_board_id)
                        && moving_members(project, other.moving_root_id)
                            .contains(&joint.mounting_board_id))
            {
                return Err(DomainError::InvalidDoorJoint(joint.id));
            }
        }
    }
    // Longer dependency cycles: A mounts to a member of B, B to C, C to A.
    for start in &project.door_joints {
        let mut visited = HashSet::new();
        let mut pending = vec![start.id];
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            let current = project
                .door_joints
                .iter()
                .find(|j| j.id == id)
                .expect("joint exists");
            for upstream in &project.door_joints {
                if moving_members(project, upstream.moving_root_id)
                    .contains(&current.mounting_board_id)
                {
                    if upstream.id == start.id {
                        return Err(DomainError::InvalidDoorJoint(start.id));
                    }
                    pending.push(upstream.id);
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct JointPreview {
    pub joint: DoorJoint,
    pub project_revision: u64,
    pub moving_members: Vec<Uuid>,
    pub installation_statuses: Vec<InstallationStatus>,
}

/// Disposable attachment proposal; nothing is repositioned or resized.
pub fn preview(
    project: &Project,
    id: Uuid,
    root: Uuid,
    mount: Uuid,
    hinge_ids: Vec<Uuid>,
) -> Result<JointPreview, JointError> {
    let local = local_pose(project, root).ok_or(JointError::MissingRoot)?;
    let world = world_pose(project, root).map_err(|_| JointError::InvalidPose)?;
    let hinge = project
        .hinge_installations
        .iter()
        .find(|h| hinge_ids.contains(&h.id))
        .ok_or(JointError::EmptyHinges)?;
    let door = project
        .boards
        .iter()
        .find(|b| b.id == hinge.door_board_id)
        .ok_or(JointError::MissingRoot)?;
    let door_world = world_pose(project, door.id).map_err(|_| JointError::InvalidPose)?;
    let x = match hinge.side.door_edge {
        BoardEdge::MinX => 0.0,
        BoardEdge::MaxX => door.length.micrometres() as f64 / 1000.0,
    };
    let z = match hinge.side.door_face {
        BoardFace::MinZ => 0.0,
        BoardFace::MaxZ => door.thickness.micrometres() as f64 / 1000.0,
    };
    let origin = door_world
        .transform_point([x, hinge.door_y.micrometres() as f64 / 1000.0, z])
        .map_err(|_| JointError::InvalidAxis)?;
    // Positive opening moves the free edge away from the inside cup face.
    // A global sign would open one of a mirrored pair into the cabinet.
    // Persisted legacy axes are not rewritten: needs_review compares them
    // with this proposal and requires an explicit undoable reconfirmation.
    let direction = match (hinge.side.door_edge, hinge.side.door_face) {
        (BoardEdge::MinX, BoardFace::MinZ) | (BoardEdge::MaxX, BoardFace::MaxZ) => -1.0,
        _ => 1.0,
    };
    let joint = DoorJoint {
        id,
        moving_root_id: root,
        mounting_board_id: mount,
        hinge_installation_ids: hinge_ids,
        closed_world_pose: world,
        closed_local_pose: local,
        axis_origin_mm: origin,
        axis_direction: door_world.rotation.rotate([0.0, direction, 0.0]),
    };
    check(project, &joint)?;
    let mut candidate = project.clone();
    candidate.door_joints.retain(|j| j.id != id);
    candidate.door_joints.push(joint.clone());
    validate_joints(&candidate).map_err(|_| JointError::Cycle)?;
    let installation_statuses = joint
        .hinge_installation_ids
        .iter()
        .map(|id| {
            diagnose(
                project,
                project
                    .hinge_installations
                    .iter()
                    .find(|h| h.id == *id)
                    .expect("checked hinge"),
            )
        })
        .collect();
    Ok(JointPreview {
        moving_members: moving_members(project, root),
        joint,
        project_revision: project.revision,
        installation_statuses,
    })
}

/// Confirm a preview against exactly the project revision it was computed from.
pub fn confirm(
    editor: &mut ProjectEditor,
    proposal: JointPreview,
) -> Result<bool, EditError<JointError>> {
    editor.transact(|p| {
        if p.revision != proposal.project_revision {
            return Err(JointError::StalePreview);
        }
        let expected = preview(
            p,
            proposal.joint.id,
            proposal.joint.moving_root_id,
            proposal.joint.mounting_board_id,
            proposal.joint.hinge_installation_ids.clone(),
        )?;
        if expected.joint != proposal.joint {
            return Err(JointError::StalePreview);
        }
        p.door_joints.retain(|j| j.id != proposal.joint.id);
        p.door_joints.push(proposal.joint);
        Ok(())
    })
}

pub fn remove(editor: &mut ProjectEditor, id: Uuid) -> Result<bool, EditError<JointError>> {
    editor.transact(|p| {
        let index = p
            .door_joints
            .iter()
            .position(|j| j.id == id)
            .ok_or(JointError::MissingJoint)?;
        p.door_joints.remove(index);
        Ok(())
    })
}

/// Closed-pose drift and installation diagnostics are read-only review signals.
pub fn needs_review(project: &Project, joint: &DoorJoint) -> bool {
    local_pose(project, joint.moving_root_id) != Some(joint.closed_local_pose)
        || world_pose(project, joint.moving_root_id).ok() != Some(joint.closed_world_pose)
        || !preview(
            project,
            joint.id,
            joint.moving_root_id,
            joint.mounting_board_id,
            joint.hinge_installation_ids.clone(),
        )
        .is_ok_and(|fresh| {
            fresh.joint.axis_origin_mm == joint.axis_origin_mm
                && fresh.joint.axis_direction == joint.axis_direction
        })
        || joint.hinge_installation_ids.iter().any(|id| {
            project
                .hinge_installations
                .iter()
                .find(|h| h.id == *id)
                .is_none_or(|h| !diagnose(project, h).issues.is_empty())
        })
}

/// Derived display poses for one joint at an angle. Does not mutate the project.
pub fn opening_limit(project: &Project, joint: &DoorJoint) -> Result<f64, JointError> {
    if !project.door_joints.iter().any(|j| j == joint) {
        return Err(JointError::MissingJoint);
    }
    if needs_review(project, joint) {
        return Err(JointError::NeedsReview);
    }
    let mut limit = None;
    for id in &joint.hinge_installation_ids {
        let hinge = project
            .hinge_installations
            .iter()
            .find(|h| h.id == *id)
            .ok_or(JointError::MissingHinge(*id))?;
        let entry = project
            .catalog
            .iter()
            .find(|c| c.id == hinge.catalog_id)
            .ok_or(JointError::UnverifiedLimit)?;
        if !crate::hardware_catalog::is_verified(entry) {
            return Err(JointError::UnverifiedLimit);
        }
        let degrees = entry
            .verified_hinge
            .as_ref()
            .ok_or(JointError::UnverifiedLimit)?
            .opening_limit_degrees;
        if degrees == 0 {
            return Err(JointError::UnverifiedLimit);
        }
        limit = Some(limit.map_or(degrees, |previous: u16| previous.min(degrees)));
    }
    limit.map(f64::from).ok_or(JointError::UnverifiedLimit)
}

/// A checked, display-only opening. Zero returns the exact stored closed world poses.
pub fn derived_poses(
    project: &Project,
    joint: &DoorJoint,
    angle_degrees: f64,
) -> Result<Vec<(Uuid, Pose)>, JointError> {
    if !angle_degrees.is_finite() || angle_degrees < 0.0 {
        return Err(JointError::InvalidAngle);
    }
    if angle_degrees > opening_limit(project, joint)? {
        return Err(JointError::InvalidAngle);
    }
    if angle_degrees == 0.0 {
        return moving_members(project, joint.moving_root_id)
            .into_iter()
            .map(|id| {
                Ok((
                    id,
                    world_pose(project, id).map_err(|_| JointError::InvalidPose)?,
                ))
            })
            .collect();
    }
    let half = angle_degrees.to_radians() / 2.0;
    let rotation = Quaternion::normalized(
        half.cos(),
        joint.axis_direction[0] * half.sin(),
        joint.axis_direction[1] * half.sin(),
        joint.axis_direction[2] * half.sin(),
    )
    .map_err(|_| JointError::InvalidPose)?;
    moving_members(project, joint.moving_root_id)
        .into_iter()
        .map(|id| {
            let closed = world_pose(project, id).map_err(|_| JointError::InvalidPose)?;
            let origin = joint.axis_origin_mm;
            let relative = std::array::from_fn(|i| closed.translation_mm[i] - origin[i]);
            let rotated = rotation.rotate(relative);
            let position = std::array::from_fn(|i| origin[i] + rotated[i]);
            Ok((
                id,
                Pose::new(
                    position,
                    rotation
                        .compose(closed.rotation)
                        .map_err(|_| JointError::InvalidPose)?,
                )
                .map_err(|_| JointError::InvalidPose)?,
            ))
        })
        .collect()
}

/// Scoped cascade: remove a board or complete assembly subtree and all its
/// allocations, annotations and dependent joints in one undoable operation.
pub fn delete_object(editor: &mut ProjectEditor, id: Uuid) -> Result<bool, EditError<JointError>> {
    editor.transact(|p| {
        if !p.boards.iter().any(|b| b.id == id) && !p.assemblies.iter().any(|a| a.id == id) {
            return Err(JointError::MissingRoot);
        }
        let members: HashSet<_> = moving_members(p, id).into_iter().collect();
        let board_ids: HashSet<_> = p
            .boards
            .iter()
            .filter(|b| members.contains(&b.id))
            .map(|b| b.id)
            .collect();
        p.door_joints.retain(|j| {
            !members.contains(&j.moving_root_id)
                && !board_ids.contains(&j.mounting_board_id)
                && !j.hinge_installation_ids.iter().any(|hid| {
                    p.hinge_installations.iter().any(|h| {
                        h.id == *hid
                            && (board_ids.contains(&h.door_board_id)
                                || board_ids.contains(&h.mounting_board_id))
                    })
                })
        });
        p.hinge_installations.retain(|h| {
            !board_ids.contains(&h.door_board_id) && !board_ids.contains(&h.mounting_board_id)
        });
        p.allocations.retain(|a| !board_ids.contains(&a.board_id));
        p.boards.retain(|b| !members.contains(&b.id));
        p.assemblies.retain(|a| !members.contains(&a.id));
        p.hardware.retain(|h| !members.contains(&h.id));
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Allocation, Assembly, Board, BoardGrain, Hardware, HardwareKind, HingeInstallation,
        HingeMountingSide, Material, Stock, StockGrain, StockSource,
    };
    use crate::money::Currency;
    use crate::units::Length;

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }
    fn pose(x: f64) -> Pose {
        Pose::new([x, 0.0, 0.0], Quaternion::IDENTITY).unwrap()
    }

    fn fixture() -> (Project, Uuid, Uuid, Uuid, Uuid, Uuid) {
        let mut p = Project::new("door", Currency::Brl);
        let material = Uuid::new_v4();
        p.materials.push(Material {
            id: material,
            name: "ply".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Unrestricted,
        });
        let cabinet = Uuid::new_v4();
        let door_root = Uuid::new_v4();
        let inner = Uuid::new_v4();
        p.assemblies.extend([
            Assembly {
                id: cabinet,
                name: "cabinet".into(),
                parent_id: None,
                pose: pose(10.0),
            },
            Assembly {
                id: door_root,
                name: "door root".into(),
                parent_id: Some(cabinet),
                pose: pose(20.0),
            },
            Assembly {
                id: inner,
                name: "nested".into(),
                parent_id: Some(door_root),
                pose: pose(5.0),
            },
        ]);
        let door = Uuid::new_v4();
        let mount = Uuid::new_v4();
        let handle = Uuid::new_v4();
        for (id, parent, name) in [(door, Some(inner), "door"), (mount, Some(cabinet), "mount")] {
            p.boards.push(Board {
                id,
                name: name.into(),
                material_id: material,
                length: mm(100),
                width: mm(100),
                thickness: mm(18),
                grain_override: None,
                parent_id: parent,
                pose: pose(2.0),
            });
        }
        p.hardware.push(Hardware {
            id: handle,
            name: "handle".into(),
            parent_id: Some(inner),
            pose: pose(50.0),
            kind: HardwareKind::Placeholder {
                dimensions: [mm(10); 3],
            },
        });
        let catalog = crate::hardware_catalog::builtin_hinge();
        let side = HingeMountingSide {
            door_edge: BoardEdge::MinX,
            door_face: BoardFace::MinZ,
            mount_front_edge: BoardEdge::MinX,
            mount_face: BoardFace::MaxZ,
        };
        for y in [30, 70] {
            p.hinge_installations.push(HingeInstallation {
                id: Uuid::new_v4(),
                door_board_id: door,
                mounting_board_id: mount,
                catalog_id: catalog.id,
                side,
                door_y: mm(y),
                mount_y: mm(y),
                cup_edge_setback: mm(3),
                overlay: mm(15),
            });
        }
        p.catalog.push(catalog);
        (p, door_root, door, mount, handle, cabinet)
    }

    #[test]
    fn mirrored_edges_and_faces_open_away_from_cup_face_in_rotated_nested_roots() {
        for edge in [BoardEdge::MinX, BoardEdge::MaxX] {
            for face in [BoardFace::MinZ, BoardFace::MaxZ] {
                let (mut p, root, door, mount, handle, cabinet) = fixture();
                p.assemblies
                    .iter_mut()
                    .find(|a| a.id == cabinet)
                    .unwrap()
                    .pose
                    .rotation = Quaternion::normalized(0.7, 0.2, -0.3, 0.4).unwrap();
                for hinge in &mut p.hinge_installations {
                    hinge.side.door_edge = edge;
                    hinge.side.door_face = face;
                }
                let proposed = preview(
                    &p,
                    Uuid::new_v4(),
                    root,
                    mount,
                    p.hinge_installations.iter().map(|h| h.id).collect(),
                )
                .unwrap();
                assert!(
                    proposed
                        .installation_statuses
                        .iter()
                        .all(|s| s.issues.is_empty())
                );
                p.door_joints.push(proposed.joint);
                let original = p.clone();
                let joint = &p.door_joints[0];
                let closed = world_pose(&p, door).unwrap();
                let point = [
                    if edge == BoardEdge::MinX { 100.0 } else { 0.0 },
                    50.0,
                    if face == BoardFace::MinZ { 0.0 } else { 18.0 },
                ];
                let before = closed.transform_point(point).unwrap();
                let open = derived_poses(&p, joint, 60.0).unwrap();
                let after = open
                    .iter()
                    .find(|(id, _)| *id == door)
                    .unwrap()
                    .1
                    .transform_point(point)
                    .unwrap();
                let outward = closed.rotation.rotate([
                    0.0,
                    0.0,
                    if face == BoardFace::MinZ { 1.0 } else { -1.0 },
                ]);
                let displacement: f64 = (0..3).map(|i| (after[i] - before[i]) * outward[i]).sum();
                assert!(
                    (displacement - 100.0 * 60.0_f64.to_radians().sin()).abs() < 1e-8,
                    "{edge:?}/{face:?}: {displacement}"
                );
                assert!(open.iter().any(|(id, _)| *id == handle));
                assert!(!open.iter().any(|(id, _)| *id == mount || *id == cabinet));
                for (id, pose) in derived_poses(&p, joint, 0.0).unwrap() {
                    assert_eq!(pose, world_pose(&p, id).unwrap());
                }
                assert_eq!(p, original);
            }
        }
    }

    #[test]
    fn legacy_axis_load_is_unchanged_until_reconfirmation_and_undo_restores_review() {
        let (mut p, root, door, mount, _, _) = fixture();
        let proposed = preview(
            &p,
            Uuid::new_v4(),
            root,
            mount,
            p.hinge_installations.iter().map(|h| h.id).collect(),
        )
        .unwrap();
        let mut legacy = proposed.joint;
        legacy.axis_direction = world_pose(&p, door)
            .unwrap()
            .rotation
            .rotate([0.0, 1.0, 0.0]);
        p.door_joints.push(legacy.clone());
        let bytes = serde_json::to_vec(&p).unwrap();
        let mut editor = crate::persistence::prepare_bytes(&bytes)
            .unwrap()
            .into_editor();
        assert_eq!(editor.project().door_joints[0], legacy);
        assert_eq!(serde_json::to_vec(editor.project()).unwrap(), bytes);
        assert!(!editor.is_dirty());
        assert_eq!(
            opening_limit(editor.project(), &legacy),
            Err(JointError::NeedsReview)
        );
        let proposal = preview(
            editor.project(),
            legacy.id,
            root,
            mount,
            legacy.hinge_installation_ids.clone(),
        )
        .unwrap();
        confirm(&mut editor, proposal).unwrap();
        assert_eq!(
            opening_limit(editor.project(), &editor.project().door_joints[0]),
            Ok(105.0)
        );
        editor.undo().unwrap();
        assert_eq!(editor.project().door_joints[0], legacy);
        assert!(needs_review(editor.project(), &legacy));
        editor.redo().unwrap();
        assert!(!needs_review(
            editor.project(),
            &editor.project().door_joints[0]
        ));
    }

    #[test]
    fn nested_door_and_handle_share_one_derived_transform_and_mount_stays_fixed() {
        let (p, root, door, mount, handle, cabinet) = fixture();
        let hinges = p.hinge_installations.iter().map(|h| h.id).collect();
        let mut editor = ProjectEditor::new(p).unwrap();
        let proposed = preview(editor.project(), Uuid::new_v4(), root, mount, hinges).unwrap();
        assert!(
            proposed
                .installation_statuses
                .iter()
                .all(|s| s.issues.is_empty())
        );
        assert!(proposed.moving_members.contains(&handle));
        assert!(!proposed.moving_members.contains(&mount));
        let before_mount = world_pose(editor.project(), mount).unwrap();
        let before_door = world_pose(editor.project(), door).unwrap();
        assert!(confirm(&mut editor, proposed).unwrap());
        let joint = &editor.project().door_joints[0];
        let poses = derived_poses(editor.project(), joint, 45.0).unwrap();
        assert_eq!(poses.len(), 4); // root, nested assembly, door and handle
        assert!(
            poses
                .iter()
                .any(|(id, pose)| *id == door && *pose != before_door)
        );
        assert!(
            poses.iter().any(|(id, pose)| *id == handle
                && *pose != world_pose(editor.project(), handle).unwrap())
        );
        assert!(!poses.iter().any(|(id, _)| *id == mount || *id == cabinet));
        assert_eq!(world_pose(editor.project(), mount).unwrap(), before_mount);
        assert_eq!(editor.project().hinge_installations.len(), 2);
        assert!(!needs_review(editor.project(), joint));
        let reopened =
            crate::persistence::prepare_bytes(&serde_json::to_vec(editor.project()).unwrap())
                .unwrap();
        assert_eq!(reopened.project().door_joints, editor.project().door_joints);
        editor.undo().unwrap();
        assert!(editor.project().door_joints.is_empty());
    }

    #[test]
    fn bounded_motion_is_disposable_and_closed_pose_is_exact() {
        let (mut p, root, door, mount, handle, _) = fixture();
        let proposal = preview(
            &p,
            Uuid::new_v4(),
            root,
            mount,
            p.hinge_installations.iter().map(|h| h.id).collect(),
        )
        .unwrap();
        p.door_joints.push(proposal.joint.clone());
        let original = p.clone();
        let fingerprint = crate::export::fingerprint(&p);
        let measurement = crate::measurements::measure(
            &p,
            &[root],
            crate::measurements::Scope::Overall,
            crate::measurements::Frame::World,
        );
        let joint = &p.door_joints[0];
        assert_eq!(opening_limit(&p, joint), Ok(105.0));
        for (id, pose) in derived_poses(&p, joint, 0.0).unwrap() {
            assert_eq!(pose, world_pose(&p, id).unwrap());
        }
        let open = derived_poses(&p, joint, 105.0).unwrap();
        assert!(
            open.iter()
                .any(|(id, pose)| *id == door && *pose != world_pose(&p, door).unwrap())
        );
        assert!(
            open.iter()
                .any(|(id, pose)| *id == handle && *pose != world_pose(&p, handle).unwrap())
        );
        assert!(!open.iter().any(|(id, _)| *id == mount));
        for angle in [-0.01, 105.01, f64::INFINITY, f64::NAN] {
            assert_eq!(
                derived_poses(&p, joint, angle),
                Err(JointError::InvalidAngle)
            );
        }
        assert_eq!(
            derived_poses(&p, joint, 0.0)
                .unwrap()
                .into_iter()
                .find(|(id, _)| *id == door)
                .unwrap()
                .1,
            world_pose(&p, door).unwrap()
        );
        assert_eq!(p, original);
        assert_eq!(crate::export::fingerprint(&p), fingerprint);
        assert_eq!(
            crate::measurements::measure(
                &p,
                &[root],
                crate::measurements::Scope::Overall,
                crate::measurements::Frame::World
            ),
            measurement
        );
        p.catalog[0].verified_hinge = None;
        assert!(opening_limit(&p, &p.door_joints[0]).is_err());
        p = original;
        p.boards.iter_mut().find(|b| b.id == door).unwrap().pose = pose(4.0);
        assert_eq!(
            opening_limit(&p, &p.door_joints[0]),
            Err(JointError::NeedsReview)
        );
    }

    #[test]
    fn rejects_mount_in_subtree_and_incompatible_hinge_edits() {
        let (mut p, root, door, mount, _, _) = fixture();
        assert!(matches!(
            preview(
                &p,
                Uuid::new_v4(),
                root,
                door,
                vec![p.hinge_installations[0].id]
            ),
            Err(JointError::Cycle)
        ));
        let ids = p.hinge_installations.iter().map(|h| h.id).collect();
        let proposed = preview(&p, Uuid::new_v4(), root, mount, ids).unwrap();
        p.door_joints.push(proposed.joint);
        let mut editor = ProjectEditor::new(p).unwrap();
        let mut edited = editor.project().hinge_installations[0].clone();
        edited.side.door_edge = BoardEdge::MaxX;
        assert_eq!(
            crate::hinge_installation::update(&mut editor, edited),
            Err(EditError::Command(
                crate::hinge_installation::InstallationEditError::IncompatibleJoint
            ))
        );
        assert!(editor.project().validate().is_ok());
        let hinge_id = editor.project().hinge_installations[0].id;
        crate::hinge_installation::remove(&mut editor, hinge_id).unwrap();
        assert_eq!(
            editor.project().door_joints[0].hinge_installation_ids.len(),
            1
        );
    }

    #[test]
    fn rejects_two_joints_that_mount_on_each_other() {
        let (mut p, root, door, mount, _, _) = fixture();
        let first = preview(
            &p,
            Uuid::new_v4(),
            root,
            mount,
            vec![p.hinge_installations[0].id],
        )
        .unwrap();
        p.door_joints.push(first.joint);
        let mut reverse = p.hinge_installations[0].clone();
        reverse.id = Uuid::new_v4();
        reverse.door_board_id = mount;
        reverse.mounting_board_id = door;
        p.hinge_installations.push(reverse.clone());
        assert!(matches!(
            preview(&p, Uuid::new_v4(), mount, door, vec![reverse.id]),
            Err(JointError::Cycle)
        ));
        assert!(p.validate().is_ok());
    }

    #[test]
    fn deletion_cascades_allocations_installations_and_joint_in_one_undo() {
        let (mut p, root, door, mount, handle, _) = fixture();
        let stock = Uuid::new_v4();
        p.stock.push(Stock {
            id: stock,
            name: "sheet".into(),
            material_id: p.materials[0].id,
            length: mm(200),
            width: mm(200),
            thickness: mm(18),
            grain: StockGrain::Unknown,
            source: StockSource::Owned,
            price: None,
            priority: 0,
            trim: [Length::ZERO; 4],
        });
        p.allocations.push(Allocation {
            id: Uuid::new_v4(),
            board_id: door,
            stock_id: stock,
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: false,
        });
        let proposed = preview(
            &p,
            Uuid::new_v4(),
            root,
            mount,
            p.hinge_installations.iter().map(|h| h.id).collect(),
        )
        .unwrap();
        p.door_joints.push(proposed.joint);
        let mut editor = ProjectEditor::new(p).unwrap();
        let initial = editor.project().clone();
        delete_object(&mut editor, door).unwrap();
        assert!(editor.project().allocations.is_empty());
        assert!(editor.project().hinge_installations.is_empty());
        assert!(editor.project().door_joints.is_empty());
        assert!(editor.project().hardware.iter().any(|h| h.id == handle));
        editor.undo().unwrap();
        let mut restored = editor.project().clone();
        restored.revision = initial.revision;
        assert_eq!(restored, initial);
        delete_object(&mut editor, root).unwrap();
        assert!(!editor.project().hardware.iter().any(|h| h.id == handle));
        assert!(!editor.project().boards.iter().any(|b| b.id == door));
        assert!(editor.project().boards.iter().any(|b| b.id == mount));
    }
}
