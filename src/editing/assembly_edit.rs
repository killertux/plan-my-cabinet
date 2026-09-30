//! Atomic hierarchy edits and world-space transforms for boards, assemblies and hardware.
use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::{Assembly, Hardware, HardwareKind, Project};
use crate::units::{Length, Pose, Quaternion, UnitError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssemblyEditError {
    MissingObject(Uuid),
    MissingParent(Uuid),
    EmptySelection,
    Cycle(Uuid),
    InvalidPose(UnitError),
    PoseNotPreserved(Uuid),
    InvalidDimensions,
    /// The catalog entry is missing or is not a coherent foot.
    NotAFoot(Uuid),
}

fn identity() -> Pose {
    Pose {
        translation_mm: [0.0; 3],
        rotation: Quaternion::IDENTITY,
    }
}

fn parent(project: &Project, id: Uuid) -> Result<Option<Uuid>, AssemblyEditError> {
    if let Some(a) = project.assemblies.iter().find(|a| a.id == id) {
        return Ok(a.parent_id);
    }
    if let Some(b) = project.boards.iter().find(|b| b.id == id) {
        return Ok(b.parent_id);
    }
    if let Some(h) = project.hardware.iter().find(|h| h.id == id) {
        return Ok(h.parent_id);
    }
    Err(AssemblyEditError::MissingObject(id))
}

fn local(project: &Project, id: Uuid) -> Result<Pose, AssemblyEditError> {
    project
        .assemblies
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.pose)
        .or_else(|| project.boards.iter().find(|b| b.id == id).map(|b| b.pose))
        .or_else(|| project.hardware.iter().find(|h| h.id == id).map(|h| h.pose))
        .ok_or(AssemblyEditError::MissingObject(id))
}

fn set(
    project: &mut Project,
    id: Uuid,
    new_parent: Option<Uuid>,
    pose: Pose,
) -> Result<(), AssemblyEditError> {
    if let Some(a) = project.assemblies.iter_mut().find(|a| a.id == id) {
        a.parent_id = new_parent;
        a.pose = pose;
        return Ok(());
    }
    if let Some(b) = project.boards.iter_mut().find(|b| b.id == id) {
        b.parent_id = new_parent;
        b.pose = pose;
        return Ok(());
    }
    if let Some(h) = project.hardware.iter_mut().find(|h| h.id == id) {
        h.parent_id = new_parent;
        h.pose = pose;
        return Ok(());
    }
    Err(AssemblyEditError::MissingObject(id))
}

fn assembly_parent(project: &Project, id: Option<Uuid>) -> Result<Pose, AssemblyEditError> {
    match id {
        None => Ok(identity()),
        Some(id) if project.assemblies.iter().any(|a| a.id == id) => world_pose(project, id),
        Some(id) => Err(AssemblyEditError::MissingParent(id)),
    }
}

/// World pose of an object, including all ancestor assembly rotations.
pub fn world_pose(project: &Project, id: Uuid) -> Result<Pose, AssemblyEditError> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = Some(id);
    while let Some(id) = current {
        if !seen.insert(id) {
            return Err(AssemblyEditError::Cycle(id));
        }
        chain.push(local(project, id)?);
        current = parent(project, id)?;
    }
    chain.into_iter().rev().try_fold(identity(), |world, pose| {
        world.compose(pose).map_err(AssemblyEditError::InvalidPose)
    })
}

fn inverse(pose: Pose) -> Result<Pose, AssemblyEditError> {
    let q = pose.rotation;
    let inverse = Quaternion {
        w: q.w,
        x: -q.x,
        y: -q.y,
        z: -q.z,
    };
    Pose::new(inverse.rotate(pose.translation_mm.map(|x| -x)), inverse)
        .map_err(AssemblyEditError::InvalidPose)
}

fn relative(parent_world: Pose, world: Pose) -> Result<Pose, AssemblyEditError> {
    inverse(parent_world)?
        .compose(world)
        .map_err(AssemblyEditError::InvalidPose)
}

fn descendants(project: &Project, id: Uuid, ancestor: Uuid) -> Result<bool, AssemblyEditError> {
    let mut current = parent(project, id)?;
    let mut seen = HashSet::new();
    while let Some(p) = current {
        if p == ancestor {
            return Ok(true);
        }
        if !seen.insert(p) {
            return Err(AssemblyEditError::Cycle(p));
        }
        current = parent(project, p)?;
    }
    Ok(false)
}

/// Stable de-duplicated roots; a selected descendant of a selected assembly is omitted.
pub fn selection_roots(
    project: &Project,
    selection: &[Uuid],
) -> Result<Vec<Uuid>, AssemblyEditError> {
    if selection.is_empty() {
        return Err(AssemblyEditError::EmptySelection);
    }
    let selected: HashSet<_> = selection.iter().copied().collect();
    let mut unique = HashSet::new();
    let mut roots = Vec::new();
    for &id in selection {
        parent(project, id)?;
        if !unique.insert(id) {
            continue;
        }
        let mut covered = false;
        for &candidate in &selected {
            if candidate != id && descendants(project, id, candidate)? {
                covered = true;
                break;
            }
        }
        if covered {
            continue;
        }
        roots.push(id);
    }
    Ok(roots)
}

fn check_world_bounds(project: &Project) -> Result<(), AssemblyEditError> {
    for assembly in &project.assemblies {
        world_pose(project, assembly.id)?;
    }
    for board in &project.boards {
        let world = world_pose(project, board.id)?;
        let d = board
            .blank_dimensions()
            .map(|v| v.micrometres() as f64 / 1000.0);
        for x in [0.0, d[0]] {
            for y in [0.0, d[1]] {
                for z in [0.0, d[2]] {
                    world
                        .transform_point([x, y, z])
                        .map_err(AssemblyEditError::InvalidPose)?;
                }
            }
        }
    }
    for hardware in &project.hardware {
        let world = world_pose(project, hardware.id)?;
        if let Some(dimensions) = project.hardware_dimensions(hardware) {
            let d = dimensions.map(|v| v.micrometres() as f64 / 1000.0);
            for x in [0.0, d[0]] {
                for y in [0.0, d[1]] {
                    for z in [0.0, d[2]] {
                        world
                            .transform_point([x, y, z])
                            .map_err(AssemblyEditError::InvalidPose)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn all_world(project: &Project) -> Result<Vec<(Uuid, Pose)>, AssemblyEditError> {
    project
        .assemblies
        .iter()
        .map(|a| a.id)
        .chain(project.boards.iter().map(|b| b.id))
        .chain(project.hardware.iter().map(|h| h.id))
        .map(|id| Ok((id, world_pose(project, id)?)))
        .collect()
}

fn preserved(before: &[(Uuid, Pose)], project: &Project) -> Result<(), AssemblyEditError> {
    for &(id, old) in before {
        let new = world_pose(project, id)?;
        if (0..3).any(|i| (old.translation_mm[i] - new.translation_mm[i]).abs() > 1e-6) {
            return Err(AssemblyEditError::PoseNotPreserved(id));
        }
        let a = old.rotation;
        let b = new.rotation;
        let q = Quaternion {
            w: a.w * b.w + a.x * b.x + a.y * b.y + a.z * b.z,
            x: a.w * b.x - a.x * b.w - a.y * b.z + a.z * b.y,
            y: a.w * b.y + a.x * b.z - a.y * b.w - a.z * b.x,
            z: a.w * b.z - a.x * b.y + a.y * b.x - a.z * b.w,
        };
        if 2.0 * (q.x * q.x + q.y * q.y + q.z * q.z).sqrt().atan2(q.w.abs()) > 1e-9 {
            return Err(AssemblyEditError::PoseNotPreserved(id));
        }
    }
    Ok(())
}

fn reparent_one(
    project: &mut Project,
    id: Uuid,
    target: Option<Uuid>,
) -> Result<(), AssemblyEditError> {
    let original = world_pose(project, id)?;
    let target_world = assembly_parent(project, target)?;
    if let Some(target) = target
        && (target == id || descendants(project, target, id)?)
    {
        return Err(AssemblyEditError::Cycle(id));
    }
    let local = relative(target_world, original)?;
    set(project, id, target, local)
}

impl ProjectEditor {
    /// Create a dimensioned non-wooden reference in one undoable edit.
    pub fn create_placeholder(
        &mut self,
        name: String,
        dimensions: [Length; 3],
        parent_id: Option<Uuid>,
        world: Pose,
    ) -> Result<Uuid, EditError<AssemblyEditError>> {
        let id = Uuid::new_v4();
        self.transact(|p| {
            if dimensions.iter().any(|d| d.micrometres() <= 0) {
                return Err(AssemblyEditError::InvalidDimensions);
            }
            let pose = relative(assembly_parent(p, parent_id)?, world)?;
            p.hardware.push(Hardware {
                id,
                name,
                parent_id,
                pose,
                kind: HardwareKind::Placeholder { dimensions },
            });
            check_world_bounds(p)
        })?;
        Ok(id)
    }

    /// Place a catalog foot in one undoable edit. `pin` is added to the
    /// project catalog first when it is not there yet (its id is kept), so a
    /// model and its first foot are one undo step. `world` is the pose of the
    /// foot's box minimum corner.
    pub fn create_foot(
        &mut self,
        name: String,
        catalog_id: Uuid,
        pin: Option<crate::domain::CatalogReference>,
        parent_id: Option<Uuid>,
        world: Pose,
    ) -> Result<Uuid, EditError<AssemblyEditError>> {
        let id = Uuid::new_v4();
        self.transact(|p| {
            if let Some(pin) = pin
                && !p.catalog.iter().any(|c| c.id == pin.id)
            {
                p.catalog.push(pin);
            }
            if !p
                .catalog
                .iter()
                .any(|c| c.id == catalog_id && crate::hardware_catalog::foot_spec(c).is_some())
            {
                return Err(AssemblyEditError::NotAFoot(catalog_id));
            }
            let pose = relative(assembly_parent(p, parent_id)?, world)?;
            p.hardware.push(Hardware {
                id,
                name,
                parent_id,
                pose,
                kind: HardwareKind::Catalog { catalog_id },
            });
            check_world_bounds(p)
        })?;
        Ok(id)
    }

    /// Change a foot's name, model, parent or pose in one transaction.
    pub fn edit_foot(
        &mut self,
        id: Uuid,
        name: String,
        catalog_id: Uuid,
        pin: Option<crate::domain::CatalogReference>,
        parent_id: Option<Uuid>,
        world: Pose,
    ) -> Result<bool, EditError<AssemblyEditError>> {
        self.transact(|p| {
            if let Some(pin) = pin
                && !p.catalog.iter().any(|c| c.id == pin.id)
            {
                p.catalog.push(pin);
            }
            if !p
                .catalog
                .iter()
                .any(|c| c.id == catalog_id && crate::hardware_catalog::foot_spec(c).is_some())
            {
                return Err(AssemblyEditError::NotAFoot(catalog_id));
            }
            let pose = relative(assembly_parent(p, parent_id)?, world)?;
            let is_foot = |h: &Hardware, p: &Project| p.foot_spec(h).is_some();
            let index = p
                .hardware
                .iter()
                .position(|h| h.id == id && is_foot(h, p))
                .ok_or(AssemblyEditError::MissingObject(id))?;
            let item = &mut p.hardware[index];
            item.name = name;
            item.kind = HardwareKind::Catalog { catalog_id };
            item.parent_id = parent_id;
            item.pose = pose;
            check_world_bounds(p)
        })
    }

    /// Edit the reference, including a possible parent change, in one transaction.
    pub fn edit_placeholder(
        &mut self,
        id: Uuid,
        name: String,
        dimensions: [Length; 3],
        parent_id: Option<Uuid>,
        world: Pose,
    ) -> Result<bool, EditError<AssemblyEditError>> {
        self.transact(|p| {
            if dimensions.iter().any(|d| d.micrometres() <= 0) {
                return Err(AssemblyEditError::InvalidDimensions);
            }
            let pose = relative(assembly_parent(p, parent_id)?, world)?;
            let item = p
                .hardware
                .iter_mut()
                .find(|h| h.id == id && matches!(h.kind, HardwareKind::Placeholder { .. }))
                .ok_or(AssemblyEditError::MissingObject(id))?;
            item.name = name;
            item.kind = HardwareKind::Placeholder { dimensions };
            item.parent_id = parent_id;
            item.pose = pose;
            check_world_bounds(p)
        })
    }

    /// Remove a hardware item (placeholder or foot) in one undoable edit.
    pub fn remove_placeholder(&mut self, id: Uuid) -> Result<bool, EditError<AssemblyEditError>> {
        self.transact(|p| {
            let index = p
                .hardware
                .iter()
                .position(|h| h.id == id)
                .ok_or(AssemblyEditError::MissingObject(id))?;
            p.hardware.remove(index);
            Ok(())
        })
    }

    /// Duplicate under the original parent at its effective world pose.
    pub fn duplicate_placeholder(
        &mut self,
        id: Uuid,
    ) -> Result<Uuid, EditError<AssemblyEditError>> {
        let new_id = Uuid::new_v4();
        self.transact(|p| {
            let source = p
                .hardware
                .iter()
                .find(|h| h.id == id)
                .ok_or(AssemblyEditError::MissingObject(id))?
                .clone();
            let world = world_pose(p, id)?;
            let pose = relative(assembly_parent(p, source.parent_id)?, world)?;
            p.hardware.push(Hardware {
                id: new_id,
                pose,
                ..source
            });
            check_world_bounds(p)
        })?;
        Ok(new_id)
    }

    /// Copy an entire assembly subtree in one transaction. The copied root remains
    /// under the same parent; the offset is in world coordinates. Stock allocations
    /// describe physical sheets and deliberately remain attached to the originals.
    pub fn duplicate_assembly(
        &mut self,
        root: Uuid,
        offset_mm: [f64; 3],
    ) -> Result<Uuid, EditError<AssemblyEditError>> {
        let new_root = Uuid::new_v4();
        self.transact(|project| {
            let source = project
                .assemblies
                .iter()
                .find(|a| a.id == root)
                .ok_or(AssemblyEditError::MissingObject(root))?;
            if offset_mm.iter().any(|v| !v.is_finite()) {
                return Err(AssemblyEditError::InvalidPose(UnitError::NonFinite));
            }
            let world = world_pose(project, root)?;
            let translated = Pose::new(
                std::array::from_fn(|i| world.translation_mm[i] + offset_mm[i]),
                world.rotation,
            )
            .map_err(AssemblyEditError::InvalidPose)?;
            let root_pose = relative(assembly_parent(project, source.parent_id)?, translated)?;
            let mut mapping = HashMap::from([(root, new_root)]);
            // Traverse only assembly ancestry, not IDs or names. Original order is
            // retained for stable object-list ordering and independent nested copies.
            let mut pending = vec![root];
            while let Some(parent_id) = pending.pop() {
                for child in project
                    .assemblies
                    .iter()
                    .filter(|a| a.parent_id == Some(parent_id))
                {
                    mapping.insert(child.id, Uuid::new_v4());
                    pending.push(child.id);
                }
            }
            let assemblies = project
                .assemblies
                .iter()
                .filter(|a| mapping.contains_key(&a.id))
                .map(|a| {
                    let mut copy = a.clone();
                    copy.id = mapping[&a.id];
                    copy.parent_id = if a.id == root {
                        a.parent_id
                    } else {
                        Some(mapping[&a.parent_id.expect("descendant has parent")])
                    };
                    if a.id == root {
                        copy.pose = root_pose;
                    }
                    copy
                })
                .collect::<Vec<_>>();
            let boards = project
                .boards
                .iter()
                .filter(|b| b.parent_id.is_some_and(|id| mapping.contains_key(&id)))
                .map(|b| {
                    let mut copy = b.duplicate();
                    copy.parent_id = b.parent_id.map(|id| mapping[&id]);
                    copy
                })
                .collect::<Vec<_>>();
            let hardware = project
                .hardware
                .iter()
                .filter(|h| h.parent_id.is_some_and(|id| mapping.contains_key(&id)))
                .map(|h| {
                    let mut copy = h.clone();
                    copy.id = Uuid::new_v4();
                    copy.parent_id = h.parent_id.map(|id| mapping[&id]);
                    copy
                })
                .collect::<Vec<_>>();
            project.assemblies.extend(assemblies);
            let new_board_ids: Vec<_> = boards.iter().map(|b| b.id).collect();
            project.boards.extend(boards);
            project.hardware.extend(hardware);
            check_world_bounds(project)?;
            // Treat every cloned descendant as a newly created physical part.
            // The allocation attempts and the entire subtree are one transaction.
            for id in new_board_ids {
                crate::first_fit::allocate_new_board(project, id);
            }
            Ok(())
        })?;
        Ok(new_root)
    }

    /// Move selected roots to an assembly (or world root), preserving every object's world pose.
    pub fn reparent_objects(
        &mut self,
        selection: &[Uuid],
        target: Option<Uuid>,
    ) -> Result<bool, EditError<AssemblyEditError>> {
        self.transact(|p| {
            assembly_parent(p, target)?;
            let roots = selection_roots(p, selection)?;
            let before = all_world(p)?;
            for id in roots {
                reparent_one(p, id, target)?;
            }
            check_world_bounds(p)?;
            preserved(&before, p)
        })
    }

    /// Create a new identity-oriented assembly at the displayed world pivot.
    pub fn group_objects(
        &mut self,
        selection: &[Uuid],
        target: Option<Uuid>,
        name: impl Into<String>,
        pivot_mm: [f64; 3],
    ) -> Result<Uuid, EditError<AssemblyEditError>> {
        let id = Uuid::new_v4();
        let name = name.into();
        self.transact(|p| {
            let roots = selection_roots(p, selection)?;
            let before = all_world(p)?;
            // Reject grouping under any selected subtree before inserting the group.
            assembly_parent(p, target)?;
            if let Some(target) = target {
                for &root in &roots {
                    if root == target || descendants(p, target, root)? {
                        return Err(AssemblyEditError::Cycle(target));
                    }
                }
            }
            let world = Pose::new(pivot_mm, Quaternion::IDENTITY)
                .map_err(AssemblyEditError::InvalidPose)?;
            let pose = relative(assembly_parent(p, target)?, world)?;
            p.assemblies.push(Assembly {
                id,
                name,
                parent_id: target,
                pose,
            });
            for root in roots {
                reparent_one(p, root, Some(id))?;
            }
            check_world_bounds(p)?;
            preserved(&before, p)
        })?;
        Ok(id)
    }

    /// Remove an assembly, promoting its direct children to its former parent.
    pub fn ungroup_assembly(&mut self, id: Uuid) -> Result<bool, EditError<AssemblyEditError>> {
        self.transact(|p| {
            let parent_id = p
                .assemblies
                .iter()
                .find(|a| a.id == id)
                .ok_or(AssemblyEditError::MissingObject(id))?
                .parent_id;
            let before = all_world(p)?;
            let children: Vec<_> = before
                .iter()
                .map(|(id, _)| *id)
                .filter(|child| *child != id && parent(p, *child) == Ok(Some(id)))
                .collect();
            for child in children {
                reparent_one(p, child, parent_id)?;
            }
            p.assemblies.retain(|a| a.id != id);
            check_world_bounds(p)?;
            preserved(
                &before
                    .into_iter()
                    .filter(|(child, _)| *child != id)
                    .collect::<Vec<_>>(),
                p,
            )
        })
    }

    /// Apply a rigid world transform once per selected subtree, about the explicit displayed pivot.
    pub fn transform_selection(
        &mut self,
        selection: &[Uuid],
        translation_mm: [f64; 3],
        rotation: Quaternion,
        pivot_mm: [f64; 3],
    ) -> Result<bool, EditError<AssemblyEditError>> {
        self.transact(|p| {
            let roots = selection_roots(p, selection)?;
            let rotation = Quaternion::normalized(rotation.w, rotation.x, rotation.y, rotation.z)
                .map_err(AssemblyEditError::InvalidPose)?;
            Pose::new(pivot_mm, rotation).map_err(AssemblyEditError::InvalidPose)?;
            if translation_mm.iter().any(|v| !v.is_finite()) {
                return Err(AssemblyEditError::InvalidPose(UnitError::NonFinite));
            }
            for id in roots {
                let old = world_pose(p, id)?;
                let rotated =
                    rotation.rotate(std::array::from_fn(|i| old.translation_mm[i] - pivot_mm[i]));
                let position =
                    std::array::from_fn(|i| pivot_mm[i] + rotated[i] + translation_mm[i]);
                let world = Pose::new(
                    position,
                    rotation
                        .compose(old.rotation)
                        .map_err(AssemblyEditError::InvalidPose)?,
                )
                .map_err(AssemblyEditError::InvalidPose)?;
                let local = relative(assembly_parent(p, parent(p, id)?)?, world)?;
                set(p, id, parent(p, id)?, local)?;
            }
            check_world_bounds(p)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Allocation, Board, BoardGrain, Hardware, Material, Stock, StockGrain, StockSource,
    };
    use crate::money::Currency;
    use crate::units::Length;

    fn fixture() -> (ProjectEditor, [Uuid; 5]) {
        let mut p = Project::new("hierarchy", Currency::Brl);
        let material = Uuid::new_v4();
        p.materials.push(Material {
            id: material,
            name: "wood".into(),
            default_thickness: Length::from_micrometres(1000),
            default_grain: BoardGrain::Unrestricted,
        });
        let ids = std::array::from_fn(|_| Uuid::new_v4());
        let turn = Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap();
        p.assemblies.push(Assembly {
            id: ids[0],
            name: "A".into(),
            parent_id: None,
            pose: Pose::new([100.0, 20.0, 0.0], turn).unwrap(),
        });
        p.assemblies.push(Assembly {
            id: ids[1],
            name: "B".into(),
            parent_id: Some(ids[0]),
            pose: Pose::new([10.0, 0.0, 0.0], turn).unwrap(),
        });
        p.assemblies.push(Assembly {
            id: ids[2],
            name: "C".into(),
            parent_id: None,
            pose: Pose::new([-200.0, 0.0, 0.0], turn).unwrap(),
        });
        p.boards.push(Board {
            id: ids[3],
            name: "board".into(),
            material_id: material,
            length: Length::from_micrometres(100_000),
            width: Length::from_micrometres(50_000),
            thickness: Length::from_micrometres(1000),
            grain_override: None,
            parent_id: Some(ids[1]),
            pose: Pose::new([3.0005, 4.0, 0.0], turn).unwrap(),
        });
        p.hardware.push(Hardware {
            id: ids[4],
            name: "foot".into(),
            parent_id: Some(ids[1]),
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            kind: HardwareKind::Placeholder {
                dimensions: [Length::from_micrometres(1000); 3],
            },
        });
        (ProjectEditor::new(p).unwrap(), ids)
    }

    #[test]
    fn reparent_across_rotated_parents_and_ungroup_preserve_all_world_poses() {
        let (mut editor, [a, b, c, board, foot]) = fixture();
        let before = all_world(editor.project()).unwrap();
        assert_eq!(editor.reparent_objects(&[board], Some(c)), Ok(true));
        preserved(&before, editor.project()).unwrap();
        assert_eq!(editor.project().boards[0].parent_id, Some(c));
        let group = editor
            .group_objects(&[b, foot], Some(a), "group", [50.0, 10.0, 0.0])
            .unwrap();
        // Foot is already below B, so the selected subtree is only B.
        assert_eq!(editor.project().hardware[0].parent_id, Some(b));
        preserved(&before, editor.project()).unwrap();
        assert_eq!(editor.ungroup_assembly(group), Ok(true));
        preserved(&before, editor.project()).unwrap();
        assert_eq!(
            editor
                .project()
                .assemblies
                .iter()
                .find(|v| v.id == b)
                .unwrap()
                .parent_id,
            Some(a)
        );
        assert_eq!(editor.ungroup_assembly(b), Ok(true));
        preserved(
            &before
                .iter()
                .copied()
                .filter(|(id, _)| *id != b)
                .collect::<Vec<_>>(),
            editor.project(),
        )
        .unwrap();
        assert_eq!(editor.project().hardware[0].parent_id, Some(a));
        assert_eq!(editor.project().boards[0].parent_id, Some(c));
        editor.project().validate().unwrap();
        editor.undo().unwrap();
        assert_eq!(editor.project().hardware[0].parent_id, Some(b));
    }

    #[test]
    fn parent_child_selected_once_and_shared_pivot_rotation() {
        let (mut editor, [a, b, c, board, _]) = fixture();
        let before = all_world(editor.project()).unwrap();
        assert_eq!(
            selection_roots(editor.project(), &[board, b, b, a, c]).unwrap(),
            vec![a, c]
        );
        editor
            .transform_selection(
                &[a, b, board],
                [100.0, 0.0, 0.0],
                Quaternion::IDENTITY,
                [0.0; 3],
            )
            .unwrap();
        for id in [a, b, board] {
            let old = before.iter().find(|(key, _)| *key == id).unwrap().1;
            let new = world_pose(editor.project(), id).unwrap();
            assert!((new.translation_mm[0] - old.translation_mm[0] - 100.0).abs() < 1e-6);
        }
        assert_eq!(editor.project().boards[0].length.micrometres(), 100_000);
        editor.undo().unwrap();
        preserved(&before, editor.project()).unwrap();
        let turn = Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap();
        let pivot = [10.0, 20.0, 0.0];
        editor
            .transform_selection(&[a, c], [0.0; 3], turn, pivot)
            .unwrap();
        for id in [a, c] {
            let old = before.iter().find(|(key, _)| *key == id).unwrap().1;
            let expected = turn.rotate(std::array::from_fn(|i| old.translation_mm[i] - pivot[i]));
            let actual = world_pose(editor.project(), id).unwrap();
            for i in 0..3 {
                assert!((actual.translation_mm[i] - pivot[i] - expected[i]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn invalid_inputs_and_cycles_do_not_write_partially() {
        let (mut editor, [a, b, c, board, _]) = fixture();
        let before = editor.project().clone();
        assert!(editor.reparent_objects(&[board, a], Some(b)).is_err());
        assert!(
            editor
                .group_objects(&[a], Some(b), "cycle", [0.0; 3])
                .is_err()
        );
        assert!(editor.reparent_objects(&[c, Uuid::new_v4()], None).is_err());
        assert!(editor.reparent_objects(&[board], Some(board)).is_err());
        assert!(
            editor
                .transform_selection(
                    &[a, c],
                    [f64::NAN, 0.0, 0.0],
                    Quaternion::IDENTITY,
                    [0.0; 3]
                )
                .is_err()
        );
        assert!(
            editor
                .transform_selection(
                    &[a],
                    [1_000_000.0, 0.0, 0.0],
                    Quaternion::IDENTITY,
                    [0.0; 3]
                )
                .is_err()
        );
        assert!(
            editor
                .transform_selection(
                    &[a],
                    [0.0; 3],
                    Quaternion::IDENTITY,
                    [f64::INFINITY, 0.0, 0.0]
                )
                .is_err()
        );
        assert!(
            editor
                .group_objects(&[board], None, "out", [1_000_001.0, 0.0, 0.0])
                .is_err()
        );
        assert_eq!(editor.project(), &before);
        assert!(!editor.can_undo());
    }

    #[test]
    fn duplicate_nested_subtree_maps_parents_and_keeps_physical_demand_independent() {
        let (mut editor, [root, nested, outside, board, foot]) = fixture();
        let stock_id = Uuid::new_v4();
        let allocation_id = Uuid::new_v4();
        let material_id = editor.project().boards[0].material_id;
        editor
            .transact::<()>(|p| {
                p.stock.push(Stock {
                    id: stock_id,
                    name: "sheet".into(),
                    material_id,
                    length: Length::from_micrometres(200_000),
                    width: Length::from_micrometres(100_000),
                    thickness: Length::from_micrometres(1000),
                    grain: StockGrain::Nondirectional,
                    source: StockSource::Owned,
                    price: None,
                    priority: 0,
                    trim: [Length::from_micrometres(0); 4],
                });
                p.allocations.push(Allocation {
                    id: allocation_id,
                    board_id: board,
                    stock_id,
                    origin: [Length::from_micrometres(0); 2],
                    quarter_turn: false,
                    locked: true,
                });
                Ok(())
            })
            .unwrap();
        let before = editor.project().clone();
        let offset = [300.0, -120.0, 25.0];
        let copy = editor.duplicate_assembly(root, offset).unwrap();
        let p = editor.project();
        p.validate().unwrap();
        let copied_nested = p
            .assemblies
            .iter()
            .find(|a| a.parent_id == Some(copy))
            .unwrap();
        let copied_board = p
            .boards
            .iter()
            .find(|b| b.parent_id == Some(copied_nested.id))
            .unwrap();
        let copied_foot = p
            .hardware
            .iter()
            .find(|h| h.parent_id == Some(copied_nested.id))
            .unwrap();
        let old_ids = [root, nested, outside, board, foot, stock_id, allocation_id];
        for id in [copy, copied_nested.id, copied_board.id, copied_foot.id] {
            assert!(!old_ids.contains(&id));
        }
        assert_eq!(
            copied_board.blank_dimensions(),
            p.boards[0].blank_dimensions()
        );
        assert_eq!(copied_board.material_id, material_id);
        assert_eq!(copied_foot.kind, p.hardware[0].kind);
        assert_eq!(p.allocations[0], before.allocations[0]);
        assert_eq!(p.boards.len(), 2); // Both boards contribute to fabrication demand.
        assert_eq!(p.allocations.len(), 2);
        assert_eq!(p.allocations[1].board_id, copied_board.id);
        assert_eq!(p.allocations[1].stock_id, stock_id);
        assert!(!p.allocations[1].locked);
        let copied_board_id = copied_board.id;
        for (old, new) in [
            (root, copy),
            (nested, copied_nested.id),
            (board, copied_board.id),
            (foot, copied_foot.id),
        ] {
            let a = world_pose(&before, old).unwrap();
            let b = world_pose(p, new).unwrap();
            for (axis, _) in offset.iter().enumerate() {
                assert!(
                    (b.translation_mm[axis] - a.translation_mm[axis] - offset[axis]).abs() < 1e-6
                );
            }
            assert_eq!(a.rotation, b.rotation);
        }
        editor
            .transact::<()>(|p| {
                p.boards
                    .iter_mut()
                    .find(|b| b.id == copied_board_id)
                    .unwrap()
                    .length = Length::from_micrometres(110_000);
                Ok(())
            })
            .unwrap();
        assert_eq!(editor.project().boards[0].length, before.boards[0].length);
        editor.undo().unwrap();
        editor.undo().unwrap();
        let mut restored = editor.project().clone();
        restored.revision = before.revision; // Undo revisions are monotonic.
        assert_eq!(restored, before);
        editor.redo().unwrap();
        assert_eq!(editor.project().allocations[0], before.allocations[0]);
        assert_eq!(editor.project().allocations[1].board_id, copied_board_id);
    }

    #[test]
    fn duplicate_rejects_invalid_offset_atomically() {
        let (mut editor, [root, ..]) = fixture();
        let before = editor.project().clone();
        for offset in [[f64::NAN, 0.0, 0.0], [1_000_000.0, 0.0, 0.0]] {
            assert!(editor.duplicate_assembly(root, offset).is_err());
            assert_eq!(editor.project(), &before);
        }
    }

    #[test]
    fn copy_under_rotated_parent_applies_world_offset_without_copying_siblings() {
        let (mut editor, [parent, nested, _, board, _]) = fixture();
        let original = world_pose(editor.project(), board).unwrap();
        let new_root = editor
            .duplicate_assembly(nested, [42.0, -17.0, 0.0])
            .unwrap();
        let copy = editor
            .project()
            .assemblies
            .iter()
            .find(|a| a.id == new_root)
            .unwrap();
        assert_eq!(copy.parent_id, Some(parent));
        assert_eq!(editor.project().assemblies.len(), 4);
        let copied_board = editor
            .project()
            .boards
            .iter()
            .find(|b| b.parent_id == Some(new_root))
            .unwrap();
        let world = world_pose(editor.project(), copied_board.id).unwrap();
        assert!((world.translation_mm[0] - original.translation_mm[0] - 42.0).abs() < 1e-6);
        assert!((world.translation_mm[1] - original.translation_mm[1] + 17.0).abs() < 1e-6);
        assert_eq!(world.rotation, original.rotation);
    }

    #[test]
    fn placeholder_workflow_nested_pose_undo_persistence_and_no_wood_charges() {
        let (mut editor, [_, nested, _, _, _]) = fixture();
        let material_id = editor.project().boards[0].material_id;
        let board_id = editor.project().boards[0].id;
        let stock_id = Uuid::new_v4();
        editor
            .transact::<()>(|p| {
                p.stock.push(Stock {
                    id: stock_id,
                    name: "Paid sheet".into(),
                    material_id,
                    length: Length::from_micrometres(200_000),
                    width: Length::from_micrometres(100_000),
                    thickness: Length::from_micrometres(1000),
                    grain: StockGrain::Nondirectional,
                    source: StockSource::ToPurchase,
                    price: Some(crate::money::Money::new(Currency::Brl, 5000).unwrap()),
                    priority: 0,
                    trim: [Length::ZERO; 4],
                });
                p.allocations.push(Allocation {
                    id: Uuid::new_v4(),
                    board_id,
                    stock_id,
                    origin: [Length::ZERO; 2],
                    quarter_turn: false,
                    locked: false,
                });
                p.cut_fee = Some(crate::money::Money::new(Currency::Brl, 100).unwrap());
                Ok(())
            })
            .unwrap();
        let initial = editor.project().clone();
        let world = Pose::new([210.0, -25.0, -80.0], Quaternion::IDENTITY).unwrap();
        let dimensions = [
            Length::from_micrometres(30_000),
            Length::from_micrometres(30_000),
            Length::from_micrometres(80_000),
        ];
        assert!(
            editor
                .create_placeholder("invalid".into(), [Length::ZERO; 3], Some(nested), world)
                .is_err()
        );
        assert_eq!(editor.project(), &initial);
        let foot = editor
            .create_placeholder("Foot".into(), dimensions, Some(nested), world)
            .unwrap();
        let copy = editor.duplicate_placeholder(foot).unwrap();
        assert_ne!(foot, copy);
        assert_eq!(
            world_pose(editor.project(), copy).unwrap(),
            world_pose(editor.project(), foot).unwrap()
        );
        assert_eq!(
            editor
                .project()
                .hardware
                .iter()
                .find(|h| h.id == copy)
                .unwrap()
                .parent_id,
            Some(nested)
        );
        let changed = Pose::new([250.0, 0.0, -90.0], Quaternion::IDENTITY).unwrap();
        editor
            .edit_placeholder(copy, "Other foot".into(), dimensions, Some(nested), changed)
            .unwrap();
        for (a, b) in world_pose(editor.project(), copy)
            .unwrap()
            .translation_mm
            .into_iter()
            .zip(changed.translation_mm)
        {
            assert!((a - b).abs() < 1e-6);
        }
        editor.undo().unwrap();
        for (a, b) in world_pose(editor.project(), copy)
            .unwrap()
            .translation_mm
            .into_iter()
            .zip(world.translation_mm)
        {
            assert!((a - b).abs() < 1e-6);
        }
        editor.redo().unwrap();
        let saved = crate::persistence::serialize(editor.project()).unwrap();
        let reopened = crate::persistence::prepare_bytes(&saved).unwrap();
        assert_eq!(reopened.project().hardware, editor.project().hardware);
        let mut without = editor.project().clone();
        without.hardware.clear();
        assert_eq!(
            crate::cost_estimate::estimate(editor.project()).unwrap(),
            crate::cost_estimate::estimate(&without).unwrap()
        );
        assert_eq!(
            crate::cost_estimate::estimate(editor.project())
                .unwrap()
                .used_stock
                .len(),
            1
        );
        assert_eq!(editor.project().boards.len(), initial.boards.len());
        assert_eq!(
            editor.project().allocations.len(),
            initial.allocations.len()
        );
    }
}
