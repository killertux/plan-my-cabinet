//! Read-only axis-aligned extents in a chosen rigid coordinate frame.
use uuid::Uuid;

use crate::assembly_edit::{AssemblyEditError, world_pose};
use crate::domain::Project;
use crate::units::{Pose, Quaternion};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Body,
    Overall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    World,
    Object(Uuid),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Measurement {
    pub scope: Scope,
    pub frame: Frame,
    pub minimum_mm: [f64; 3],
    pub maximum_mm: [f64; 3],
    pub dimensions_mm: [f64; 3],
    pub board_count: usize,
    pub hardware_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasurementError {
    EmptySelection,
    MissingObject(Uuid),
    UndimensionedHardware(Uuid),
    Pose(AssemblyEditError),
}

impl From<AssemblyEditError> for MeasurementError {
    fn from(value: AssemblyEditError) -> Self {
        Self::Pose(value)
    }
}

/// Selected assemblies expand to their descendant boards and (for Overall) hardware.
/// Explicitly selected hardware contributes only to Overall. Hidden objects still measure.
pub fn measure(
    project: &Project,
    selected: &[Uuid],
    scope: Scope,
    frame: Frame,
) -> Result<Measurement, MeasurementError> {
    use std::collections::HashSet;
    if selected.is_empty() {
        return Err(MeasurementError::EmptySelection);
    }
    let frame_pose = match frame {
        Frame::World => Pose::IDENTITY,
        Frame::Object(id) => world_pose(project, id)?,
    };
    let inverse_rotation = Quaternion {
        w: frame_pose.rotation.w,
        x: -frame_pose.rotation.x,
        y: -frame_pose.rotation.y,
        z: -frame_pose.rotation.z,
    };
    let selected: HashSet<_> = selected.iter().copied().collect();
    for &id in &selected {
        if !project.boards.iter().any(|b| b.id == id)
            && !project.assemblies.iter().any(|a| a.id == id)
            && !project.hardware.iter().any(|h| h.id == id)
        {
            return Err(MeasurementError::MissingObject(id));
        }
    }
    let included = |id, parent: Option<Uuid>| {
        let mut current = parent;
        let mut seen = HashSet::new();
        if selected.contains(&id) {
            return true;
        }
        while let Some(ancestor) = current {
            if selected.contains(&ancestor) {
                return true;
            }
            if !seen.insert(ancestor) {
                break;
            }
            current = project
                .assemblies
                .iter()
                .find(|a| a.id == ancestor)
                .and_then(|a| a.parent_id);
        }
        false
    };
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut board_count = 0;
    let mut hardware_count = 0;
    let mut add_box = |id, dimensions: [f64; 3]| -> Result<(), MeasurementError> {
        let pose = world_pose(project, id)?;
        for x in [0.0, dimensions[0]] {
            for y in [0.0, dimensions[1]] {
                for z in [0.0, dimensions[2]] {
                    let world = pose
                        .transform_point([x, y, z])
                        .map_err(AssemblyEditError::InvalidPose)?;
                    let local = inverse_rotation.rotate(std::array::from_fn(|i| {
                        world[i] - frame_pose.translation_mm[i]
                    }));
                    for i in 0..3 {
                        min[i] = min[i].min(local[i]);
                        max[i] = max[i].max(local[i]);
                    }
                }
            }
        }
        Ok(())
    };
    for board in &project.boards {
        if included(board.id, board.parent_id) {
            add_box(
                board.id,
                board
                    .blank_dimensions()
                    .map(|v| v.micrometres() as f64 / 1000.0),
            )?;
            board_count += 1;
        }
    }
    if scope == Scope::Overall {
        for hardware in &project.hardware {
            if included(hardware.id, hardware.parent_id) {
                let Some(dimensions) = project.hardware_dimensions(hardware) else {
                    return Err(MeasurementError::UndimensionedHardware(hardware.id));
                };
                add_box(
                    hardware.id,
                    dimensions.map(|v| v.micrometres() as f64 / 1000.0),
                )?;
                hardware_count += 1;
            }
        }
    }
    if board_count + hardware_count == 0 {
        return Err(MeasurementError::EmptySelection);
    }
    Ok(Measurement {
        scope,
        frame,
        minimum_mm: min,
        maximum_mm: max,
        dimensions_mm: std::array::from_fn(|i| max[i] - min[i]),
        board_count,
        hardware_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Assembly, Board, BoardGrain, Hardware, HardwareKind, Material};
    use crate::money::Currency;
    use crate::units::Length;

    fn mm(v: i64) -> Length {
        Length::from_micrometres(v * 1000)
    }
    fn pose(x: f64, z: f64) -> Pose {
        Pose::new([x, 0.0, z], Quaternion::IDENTITY).unwrap()
    }

    #[test]
    fn body_and_overall_union_of_selected_subtree_and_hardware() {
        let mut p = Project::new("test", Currency::Brl);
        let root = Uuid::new_v4();
        let nested = Uuid::new_v4();
        let board = Uuid::new_v4();
        let foot = Uuid::new_v4();
        let material = Uuid::new_v4();
        p.materials.push(Material {
            coating: Default::default(),
            default_band: None,
            kind: Default::default(),
            id: material,
            name: "wood".into(),
            default_thickness: mm(2300),
            default_grain: BoardGrain::Unrestricted,
        });
        p.assemblies.push(Assembly {
            id: root,
            name: "cabinet".into(),
            parent_id: None,
            pose: pose(0.0, 0.0),
        });
        p.assemblies.push(Assembly {
            id: nested,
            name: "body".into(),
            parent_id: Some(root),
            pose: pose(0.0, 100.0),
        });
        p.boards.push(Board {
            coated_face: Default::default(),
            banding: Default::default(),
            id: board,
            name: "body".into(),
            material_id: material,
            length: mm(820),
            width: mm(600),
            thickness: mm(2300),
            grain_override: None,
            parent_id: Some(nested),
            pose: pose(0.0, 0.0),
        });
        p.hardware.push(Hardware {
            id: foot,
            name: "foot".into(),
            parent_id: Some(root),
            pose: pose(0.0, 0.0),
            kind: HardwareKind::Placeholder {
                dimensions: [mm(30), mm(30), mm(100)],
            },
        });
        let body = measure(&p, &[root], Scope::Body, Frame::World).unwrap();
        assert_eq!(body.dimensions_mm, [820.0, 600.0, 2300.0]);
        let overall = measure(
            &p,
            &[root, nested, foot, foot],
            Scope::Overall,
            Frame::World,
        )
        .unwrap();
        assert_eq!(overall.dimensions_mm, [820.0, 600.0, 2400.0]);
        assert_eq!((overall.board_count, overall.hardware_count), (1, 1));
        p.hardware[0].pose = pose(0.0, 200.0); // entirely within body bounds
        let inside = measure(&p, &[root], Scope::Overall, Frame::World).unwrap();
        assert_eq!(inside.dimensions_mm, body.dimensions_mm);
        assert_eq!(
            measure(&p, &[foot], Scope::Body, Frame::World),
            Err(MeasurementError::EmptySelection)
        );
        assert_eq!(
            measure(&p, &[foot], Scope::Overall, Frame::World)
                .unwrap()
                .hardware_count,
            1
        );
        let catalog = Uuid::new_v4();
        p.hardware[0].kind = HardwareKind::Catalog {
            catalog_id: catalog,
        };
        assert_eq!(
            measure(&p, &[root], Scope::Overall, Frame::World),
            Err(MeasurementError::UndimensionedHardware(foot))
        );
        assert_eq!(
            measure(&p, &[root], Scope::Body, Frame::World)
                .unwrap()
                .dimensions_mm,
            body.dimensions_mm
        );
    }

    #[test]
    fn rotated_nested_frame_transforms_all_eight_corners_without_rounding() {
        let mut p = Project::new("rotation", Currency::Brl);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let board = Uuid::new_v4();
        let material = Uuid::new_v4();
        let turn = Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap();
        p.assemblies.push(Assembly {
            id: a,
            name: "a".into(),
            parent_id: None,
            pose: Pose::new([10.0, 20.0, 0.0], turn).unwrap(),
        });
        p.assemblies.push(Assembly {
            id: b,
            name: "b".into(),
            parent_id: Some(a),
            pose: Pose::new([3.0, 4.0, 0.0], turn).unwrap(),
        });
        p.materials.push(Material {
            coating: Default::default(),
            default_band: None,
            kind: Default::default(),
            id: material,
            name: "wood".into(),
            default_thickness: mm(1),
            default_grain: BoardGrain::Length,
        });
        p.boards.push(Board {
            coated_face: Default::default(),
            banding: Default::default(),
            id: board,
            name: "board".into(),
            material_id: material,
            length: mm(100),
            width: mm(50),
            thickness: mm(1),
            grain_override: None,
            parent_id: Some(b),
            pose: Pose::new([0.0005, 0.0, 0.0], turn).unwrap(),
        });
        let local = measure(&p, &[a, board], Scope::Body, Frame::Object(board)).unwrap();
        for (actual, expected) in local.dimensions_mm.into_iter().zip([100.0, 50.0, 1.0]) {
            assert!((actual - expected).abs() < 1e-9);
        }
        let world = measure(&p, &[a], Scope::Body, Frame::World).unwrap();
        assert!((world.dimensions_mm[0] - 50.0).abs() < 1e-8);
        assert!((world.dimensions_mm[1] - 100.0).abs() < 1e-8);
        assert!((local.minimum_mm[0]).abs() < 1e-9);
        let missing = Uuid::new_v4();
        assert_eq!(
            measure(&p, &[missing], Scope::Body, Frame::World),
            Err(MeasurementError::MissingObject(missing))
        );
        let half = std::f64::consts::FRAC_PI_4 / 2.0;
        p.assemblies[0].pose.rotation =
            Quaternion::normalized(half.cos(), 0.0, 0.0, half.sin()).unwrap();
        let tilted = measure(&p, &[board], Scope::Body, Frame::World).unwrap();
        let board_world = world_pose(&p, board).unwrap();
        let mut points = Vec::new();
        for x in [0.0, 100.0] {
            for y in [0.0, 50.0] {
                for z in [0.0, 1.0] {
                    points.push(board_world.transform_point([x, y, z]).unwrap());
                }
            }
        }
        for axis in 0..3 {
            let expected_min = points
                .iter()
                .map(|point| point[axis])
                .fold(f64::INFINITY, f64::min);
            let expected_max = points
                .iter()
                .map(|point| point[axis])
                .fold(f64::NEG_INFINITY, f64::max);
            assert!((tilted.minimum_mm[axis] - expected_min).abs() < 1e-9);
            assert!((tilted.maximum_mm[axis] - expected_max).abs() < 1e-9);
        }
    }
}
