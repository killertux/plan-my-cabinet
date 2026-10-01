//! Transient board placement. Coordinates are millimetres; board-local X/Y/Z
//! correspond to length/width/thickness. No snap or face relationship is stored.
use std::collections::HashSet;

use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::{Board, Project};
use crate::units::{Pose, Quaternion, UnitError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinateFrame {
    LocalParent,
    World,
}

/// Intrinsic X, then Y, then Z rotations, expressed in degrees (matrix Rz*Ry*Rx).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumericPose {
    pub position_mm: [f64; 3],
    pub rotation_degrees_xyz: [f64; 3],
    pub frame: CoordinateFrame,
}

/// Named numeric-editor proposals. Orientations are expressed in the selected
/// frame; the board pose origin remains fixed (not its centre).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PosePreset {
    StandUp,
    LayFlat,
    Turn90Z,
}

pub fn preset_pose(current: Pose, preset: PosePreset) -> Result<Pose, PlacementError> {
    let orientation = match preset {
        PosePreset::LayFlat => Quaternion::IDENTITY,
        PosePreset::StandUp => {
            let half = -std::f64::consts::FRAC_PI_4;
            Quaternion::normalized(half.cos(), 0.0, half.sin(), 0.0)
                .map_err(PlacementError::InvalidPose)?
        }
        PosePreset::Turn90Z => {
            let half = std::f64::consts::FRAC_PI_4;
            let turn = Quaternion::normalized(half.cos(), 0.0, 0.0, half.sin())
                .map_err(PlacementError::InvalidPose)?;
            turn.compose(current.rotation)
                .map_err(PlacementError::InvalidPose)?
        }
    };
    Pose::new(current.translation_mm, orientation).map_err(PlacementError::InvalidPose)
}

#[cfg(test)]
mod preset_tests {
    use super::*;

    fn close(actual: [f64; 3], expected: [f64; 3]) {
        for (a, b) in actual.into_iter().zip(expected) {
            assert!((a - b).abs() < 1e-9, "{a} != {b}");
        }
    }

    #[test]
    fn absolute_orientations_and_relative_frame_z_keep_origin_fixed() {
        let source = Pose::new([15.0, 20.0, 35.0], Quaternion::IDENTITY).unwrap();
        let standing = preset_pose(source, PosePreset::StandUp).unwrap();
        close(standing.translation_mm, source.translation_mm);
        close(standing.rotation.rotate([1.0, 0.0, 0.0]), [0.0, 0.0, 1.0]);
        close(standing.rotation.rotate([0.0, 1.0, 0.0]), [0.0, 1.0, 0.0]);
        close(standing.rotation.rotate([0.0, 0.0, 1.0]), [-1.0, 0.0, 0.0]);
        let turned = preset_pose(standing, PosePreset::Turn90Z).unwrap();
        close(turned.translation_mm, source.translation_mm);
        close(turned.rotation.rotate([1.0, 0.0, 0.0]), [0.0, 0.0, 1.0]);
        close(turned.rotation.rotate([0.0, 1.0, 0.0]), [-1.0, 0.0, 0.0]);
        let flat = preset_pose(turned, PosePreset::LayFlat).unwrap();
        assert_eq!(flat.rotation, Quaternion::IDENTITY);
        close(flat.translation_mm, source.translation_mm);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Negative,
    Positive,
}

impl Side {
    fn sign(self) -> f64 {
        match self {
            Self::Negative => -1.0,
            Self::Positive => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardFace {
    /// 0 = length, 1 = width, 2 = thickness.
    pub axis: usize,
    pub side: Side,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Centre,
    End,
}

impl Align {
    fn fraction(self) -> f64 {
        match self {
            Self::Start => 0.0,
            Self::Centre => 0.5,
            Self::End => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FacePlacement {
    pub source_face: BoardFace,
    pub target_id: Uuid,
    pub target_face: BoardFace,
    /// In-plane axes are the two increasing board axes other than the face axis.
    pub source_align: [Align; 2],
    pub target_align: [Align; 2],
    /// Millimetres along the target's two in-plane positive axes.
    pub offset_mm: [f64; 2],
    /// Millimetres outward along the target face normal.
    pub gap_mm: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapCandidate {
    pub target_id: Uuid,
    pub target_face: BoardFace,
    pub source_face: BoardFace,
    /// Geometric distance from the unsnapped board origin, in mm.
    pub distance_mm: f64,
    pub world_pose: Pose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementError {
    MissingBoard(Uuid),
    MissingParent(Uuid),
    InvalidFace,
    SameBoard,
    InvalidRotation,
    OffGrid,
    InvalidPose(UnitError),
}

fn axes(face: BoardFace) -> Result<[usize; 2], PlacementError> {
    if face.axis >= 3 {
        return Err(PlacementError::InvalidFace);
    }
    Ok(match face.axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    })
}

fn board(project: &Project, id: Uuid) -> Result<&Board, PlacementError> {
    project
        .boards
        .iter()
        .find(|b| b.id == id)
        .ok_or(PlacementError::MissingBoard(id))
}

fn parent_world(project: &Project, mut parent: Option<Uuid>) -> Result<Pose, PlacementError> {
    let mut ancestors = Vec::new();
    let mut seen = HashSet::new();
    while let Some(id) = parent {
        if !seen.insert(id) {
            return Err(PlacementError::MissingParent(id));
        }
        let assembly = project
            .assemblies
            .iter()
            .find(|a| a.id == id)
            .ok_or(PlacementError::MissingParent(id))?;
        ancestors.push(assembly.pose);
        parent = assembly.parent_id;
    }
    ancestors
        .into_iter()
        .try_fold(identity(), |child, ancestor| {
            ancestor.compose(child).map_err(PlacementError::InvalidPose)
        })
}

fn identity() -> Pose {
    Pose {
        translation_mm: [0.0; 3],
        rotation: Quaternion::IDENTITY,
    }
}

fn inverse(pose: Pose) -> Result<Pose, PlacementError> {
    let q = pose.rotation;
    let conjugate = Quaternion {
        w: q.w,
        x: -q.x,
        y: -q.y,
        z: -q.z,
    };
    let position = conjugate.rotate(pose.translation_mm.map(|v| -v));
    Pose::new(position, conjugate).map_err(PlacementError::InvalidPose)
}

pub fn world_pose(project: &Project, id: Uuid) -> Result<Pose, PlacementError> {
    let board = board(project, id)?;
    parent_world(project, board.parent_id)?
        .compose(board.pose)
        .map_err(PlacementError::InvalidPose)
}

fn validated_pose(project: &Project, id: Uuid, world: Pose) -> Result<Pose, PlacementError> {
    let board = board(project, id)?;
    let dimensions = board
        .blank_dimensions()
        .map(|length| length.micrometres() as f64 / 1000.0);
    for x in [0.0, dimensions[0]] {
        for y in [0.0, dimensions[1]] {
            for z in [0.0, dimensions[2]] {
                world
                    .transform_point([x, y, z])
                    .map_err(PlacementError::InvalidPose)?;
            }
        }
    }
    let parent = parent_world(project, board.parent_id)?;
    inverse(parent)?
        .compose(world)
        .map_err(PlacementError::InvalidPose)
}

/// Convert a tentative world pose to the numeric editor's chosen frame
/// without committing or reconstructing its quaternion from display text.
pub fn pose_in_frame(
    project: &Project,
    id: Uuid,
    world: Pose,
    frame: CoordinateFrame,
) -> Result<Pose, PlacementError> {
    match frame {
        CoordinateFrame::World => Ok(world),
        CoordinateFrame::LocalParent => validated_pose(project, id, world),
    }
}

/// Intrinsic X, then Y, then Z rotations in degrees (matrix Rz*Ry*Rx), as the
/// numeric editor enters them. Each angle must be finite and within ±360°.
pub fn rotation_from_degrees_xyz(degrees: [f64; 3]) -> Result<Quaternion, PlacementError> {
    if degrees.iter().any(|a| !a.is_finite() || a.abs() > 360.0) {
        return Err(PlacementError::InvalidRotation);
    }
    let [x, y, z] = degrees.map(f64::to_radians);
    let (sx, cx) = (x / 2.0).sin_cos();
    let (sy, cy) = (y / 2.0).sin_cos();
    let (sz, cz) = (z / 2.0).sin_cos();
    Quaternion::normalized(
        cz * cy * cx + sz * sy * sx,
        cz * cy * sx - sz * sy * cx,
        cz * sy * cx + sz * cy * sx,
        sz * cy * cx - cz * sy * sx,
    )
    .map_err(PlacementError::InvalidPose)
}

/// The numeric editor's display angles for a rotation: the inverse of
/// [`rotation_from_degrees_xyz`] (pitch clamped at ±90°).
pub fn euler_degrees_xyz(q: Quaternion) -> [f64; 3] {
    let sin_pitch = 2.0 * (q.w * q.y - q.z * q.x);
    [
        (2.0 * (q.w * q.x + q.y * q.z))
            .atan2(1.0 - 2.0 * (q.x * q.x + q.y * q.y))
            .to_degrees(),
        sin_pitch.clamp(-1.0, 1.0).asin().to_degrees(),
        (2.0 * (q.w * q.z + q.x * q.y))
            .atan2(1.0 - 2.0 * (q.y * q.y + q.z * q.z))
            .to_degrees(),
    ]
}

fn numeric_world(project: &Project, id: Uuid, input: NumericPose) -> Result<Pose, PlacementError> {
    if input
        .rotation_degrees_xyz
        .iter()
        .any(|a| !a.is_finite() || a.abs() > 360.0)
    {
        return Err(PlacementError::InvalidRotation);
    }
    let mut position = input.position_mm;
    for value in &mut position {
        if !value.is_finite() {
            return Err(PlacementError::InvalidPose(UnitError::NonFinite));
        }
        if value.abs() > crate::units::WORLD_LIMIT_MM {
            return Err(PlacementError::InvalidPose(UnitError::OutOfBounds));
        }
        if (*value * 1000.0 - (*value * 1000.0).round()).abs() > 1e-7 {
            return Err(PlacementError::OffGrid);
        }
        *value = (*value * 1000.0).round() / 1000.0;
    }
    let q = rotation_from_degrees_xyz(input.rotation_degrees_xyz)?;
    let entered = Pose::new(position, q).map_err(PlacementError::InvalidPose)?;
    match input.frame {
        CoordinateFrame::World => Ok(entered),
        CoordinateFrame::LocalParent => parent_world(project, board(project, id)?.parent_id)?
            .compose(entered)
            .map_err(PlacementError::InvalidPose),
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}

fn axis(index: usize, sign: f64) -> [f64; 3] {
    let mut v = [0.0; 3];
    v[index] = sign;
    v
}

// Orthonormal source/target frames determine the twist as well as the normal.
fn facing_rotation(
    source: BoardFace,
    target: BoardFace,
    target_rotation: Quaternion,
) -> Result<Quaternion, PlacementError> {
    let source_axes = axes(source)?;
    let target_axes = axes(target)?;
    let sn = axis(source.axis, source.side.sign());
    let st = axis(source_axes[0], 1.0);
    let sb = cross(sn, st);
    let tn = target_rotation.rotate(axis(target.axis, -target.side.sign()));
    let tt = target_rotation.rotate(axis(target_axes[0], 1.0));
    let tb = cross(tn, tt);
    let source_basis = [st, sb, sn];
    let target_basis = [tt, tb, tn];
    let m: [[f64; 3]; 3] = std::array::from_fn(|row| {
        std::array::from_fn(|col| {
            (0..3)
                .map(|k| target_basis[k][row] * source_basis[k][col])
                .sum()
        })
    });
    // Matrix to quaternion, stable including 180-degree flips.
    let (w, x, y, z) = if 1.0 + m[0][0] + m[1][1] + m[2][2] > 1e-10 {
        let s = (1.0 + m[0][0] + m[1][1] + m[2][2]).sqrt() * 2.0;
        (
            s / 4.0,
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
        )
    } else {
        let i = (0..3)
            .max_by(|a, b| m[*a][*a].total_cmp(&m[*b][*b]))
            .expect("a 3x3 matrix has a diagonal");
        let j = (i + 1) % 3;
        let k = (i + 2) % 3;
        let s = (1.0 + m[i][i] - m[j][j] - m[k][k]).max(0.0).sqrt() * 2.0;
        let mut q = [0.0; 4];
        q[i + 1] = s / 4.0;
        q[0] = (m[k][j] - m[j][k]) / s;
        q[j + 1] = (m[j][i] + m[i][j]) / s;
        q[k + 1] = (m[k][i] + m[i][k]) / s;
        (q[0], q[1], q[2], q[3])
    };
    Quaternion::normalized(w, x, y, z).map_err(PlacementError::InvalidPose)
}

pub fn face_pose(
    project: &Project,
    source_id: Uuid,
    placement: FacePlacement,
) -> Result<Pose, PlacementError> {
    if source_id == placement.target_id {
        return Err(PlacementError::SameBoard);
    }
    let sa = axes(placement.source_face)?;
    let ta = axes(placement.target_face)?;
    if placement
        .offset_mm
        .iter()
        .chain([&placement.gap_mm])
        .any(|v| !v.is_finite())
    {
        return Err(PlacementError::InvalidPose(UnitError::NonFinite));
    }
    let source = board(project, source_id)?;
    let target = board(project, placement.target_id)?;
    let target_world = world_pose(project, target.id)?;
    let rotation = facing_rotation(
        placement.source_face,
        placement.target_face,
        target_world.rotation,
    )?;
    let sd = source
        .blank_dimensions()
        .map(|v| v.micrometres() as f64 / 1000.0);
    let td = target
        .blank_dimensions()
        .map(|v| v.micrometres() as f64 / 1000.0);
    let mut source_point = [0.0; 3];
    let mut target_point = [0.0; 3];
    if placement.source_face.side == Side::Positive {
        source_point[placement.source_face.axis] = sd[placement.source_face.axis];
    }
    if placement.target_face.side == Side::Positive {
        target_point[placement.target_face.axis] = td[placement.target_face.axis];
    }
    for i in 0..2 {
        source_point[sa[i]] = sd[sa[i]] * placement.source_align[i].fraction();
        target_point[ta[i]] =
            td[ta[i]] * placement.target_align[i].fraction() + placement.offset_mm[i];
    }
    let point = target_world
        .transform_point(target_point)
        .map_err(PlacementError::InvalidPose)?;
    let normal = target_world.rotation.rotate(axis(
        placement.target_face.axis,
        placement.target_face.side.sign(),
    ));
    let rotated = rotation.rotate(source_point);
    let translation = std::array::from_fn(|i| point[i] + normal[i] * placement.gap_mm - rotated[i]);
    let world = Pose::new(translation, rotation).map_err(PlacementError::InvalidPose)?;
    validated_pose(project, source_id, world)?;
    Ok(world)
}

/// Geometric candidates can be supplied directly to a UI or ranked further by
/// projected screen distance. `max_distance_mm` is an interaction threshold only.
pub fn snap_candidates(
    project: &Project,
    source_id: Uuid,
    free_world: Pose,
    max_distance_mm: f64,
) -> Result<Vec<SnapCandidate>, PlacementError> {
    validated_pose(project, source_id, free_world)?;
    if !max_distance_mm.is_finite() || max_distance_mm < 0.0 {
        return Err(PlacementError::InvalidPose(UnitError::OutOfBounds));
    }
    let mut candidates = Vec::new();
    for target in &project.boards {
        if target.id == source_id {
            continue;
        }
        for source_axis in 0..3 {
            for source_side in [Side::Negative, Side::Positive] {
                for target_axis in 0..3 {
                    for target_side in [Side::Negative, Side::Positive] {
                        let source_face = BoardFace {
                            axis: source_axis,
                            side: source_side,
                        };
                        let target_face = BoardFace {
                            axis: target_axis,
                            side: target_side,
                        };
                        let request = FacePlacement {
                            source_face,
                            target_id: target.id,
                            target_face,
                            source_align: [Align::Centre; 2],
                            target_align: [Align::Centre; 2],
                            offset_mm: [0.0; 2],
                            gap_mm: 0.0,
                        };
                        if let Ok(world_pose) = face_pose(project, source_id, request) {
                            let distance_mm = dot(
                                std::array::from_fn(|i| {
                                    world_pose.translation_mm[i] - free_world.translation_mm[i]
                                }),
                                std::array::from_fn(|i| {
                                    world_pose.translation_mm[i] - free_world.translation_mm[i]
                                }),
                            )
                            .sqrt();
                            if distance_mm <= max_distance_mm {
                                candidates.push(SnapCandidate {
                                    target_id: target.id,
                                    target_face,
                                    source_face,
                                    distance_mm,
                                    world_pose,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    candidates.sort_by(|a, b| {
        a.distance_mm
            .total_cmp(&b.distance_mm)
            .then_with(|| a.target_id.cmp(&b.target_id))
            .then_with(|| a.target_face.axis.cmp(&b.target_face.axis))
    });
    Ok(candidates)
}

pub struct PlacementSession<'a> {
    editor: &'a mut ProjectEditor,
    board_id: Uuid,
    cancel_on_drop: bool,
}

impl<'a> PlacementSession<'a> {
    pub fn begin(editor: &'a mut ProjectEditor, board_id: Uuid) -> Result<Self, PlacementError> {
        board(editor.project(), board_id)?;
        editor.begin_preview();
        Ok(Self {
            editor,
            board_id,
            cancel_on_drop: true,
        })
    }

    pub fn resume(editor: &'a mut ProjectEditor, board_id: Uuid) -> Result<Self, PlacementError> {
        board(editor.project(), board_id)?;
        if editor.preview().is_none() {
            editor.begin_preview();
        }
        Ok(Self {
            editor,
            board_id,
            cancel_on_drop: true,
        })
    }

    pub fn project(&self) -> &Project {
        self.editor.preview().expect("active placement preview")
    }

    pub fn preview_world(&mut self, world: Pose) -> Result<Pose, PlacementError> {
        let local = validated_pose(self.project(), self.board_id, world)?;
        let id = self.board_id;
        self.editor
            .update_preview(|project| {
                project
                    .board_mut(id)
                    .ok_or(PlacementError::MissingBoard(id))?
                    .pose = local;
                Ok::<_, PlacementError>(())
            })
            .map_err(|error| match error {
                EditError::Command(reason) => reason,
                _ => unreachable!("active preview"),
            })?;
        Ok(world)
    }

    pub fn preview_numeric(&mut self, input: NumericPose) -> Result<Pose, PlacementError> {
        let world = numeric_world(self.project(), self.board_id, input)?;
        self.preview_world(world)
    }

    /// Preserve a preset's exact quaternion while validating a position typed
    /// in the selected frame; only explicit rotation text edits use Euler input.
    pub fn preview_exact_framed(
        &mut self,
        frame: CoordinateFrame,
        entered: Pose,
        position_edited: [bool; 3],
    ) -> Result<Pose, PlacementError> {
        for (value, edited) in entered.translation_mm.into_iter().zip(position_edited) {
            if !value.is_finite() {
                return Err(PlacementError::InvalidPose(UnitError::NonFinite));
            }
            if value.abs() > crate::units::WORLD_LIMIT_MM {
                return Err(PlacementError::InvalidPose(UnitError::OutOfBounds));
            }
            if edited && (value * 1000.0 - (value * 1000.0).round()).abs() > 1e-7 {
                return Err(PlacementError::OffGrid);
            }
        }
        let world = match frame {
            CoordinateFrame::World => entered,
            CoordinateFrame::LocalParent => {
                let project = self.editor.project();
                let parent = parent_world(project, board(project, self.board_id)?.parent_id)?;
                parent
                    .compose(entered)
                    .map_err(PlacementError::InvalidPose)?
            }
        };
        self.preview_world(world)
    }

    /// Merge explicitly edited values with the exact committed pose, rather than
    /// round-tripping the displayed decimal and Euler representations.
    pub fn preview_numeric_edited(
        &mut self,
        input: NumericPose,
        position_edited: [bool; 3],
        rotation_edited: bool,
    ) -> Result<Pose, PlacementError> {
        let project = self.editor.project();
        let original = board(project, self.board_id)?.pose;
        let parent = parent_world(project, board(project, self.board_id)?.parent_id)?;
        let source = match input.frame {
            CoordinateFrame::LocalParent => original,
            CoordinateFrame::World => parent
                .compose(original)
                .map_err(PlacementError::InvalidPose)?,
        };
        if !position_edited.contains(&true) && !rotation_edited {
            return parent
                .compose(original)
                .map_err(PlacementError::InvalidPose);
        }
        let mut position = source.translation_mm;
        for i in 0..3 {
            if position_edited[i] {
                let value = input.position_mm[i];
                if !value.is_finite() {
                    return Err(PlacementError::InvalidPose(UnitError::NonFinite));
                }
                if value.abs() > crate::units::WORLD_LIMIT_MM {
                    return Err(PlacementError::InvalidPose(UnitError::OutOfBounds));
                }
                if (value * 1000.0 - (value * 1000.0).round()).abs() > 1e-7 {
                    return Err(PlacementError::OffGrid);
                }
                position[i] = (value * 1000.0).round() / 1000.0;
            }
        }
        let rotation = if rotation_edited {
            rotation_from_degrees_xyz(input.rotation_degrees_xyz)?
        } else {
            source.rotation
        };
        let entered = Pose::new(position, rotation).map_err(PlacementError::InvalidPose)?;
        let (local, world) = match input.frame {
            CoordinateFrame::LocalParent => {
                let world = parent
                    .compose(entered)
                    .map_err(PlacementError::InvalidPose)?;
                (entered, world)
            }
            CoordinateFrame::World => {
                let mut local = validated_pose(project, self.board_id, entered)?;
                if !rotation_edited {
                    local.rotation = original.rotation;
                }
                if !position_edited.contains(&true) {
                    local.translation_mm = original.translation_mm;
                }
                (local, entered)
            }
        };
        validated_pose(project, self.board_id, world)?;
        let id = self.board_id;
        self.editor
            .update_preview(|project| {
                project
                    .board_mut(id)
                    .ok_or(PlacementError::MissingBoard(id))?
                    .pose = local;
                Ok::<_, PlacementError>(())
            })
            .map_err(|error| match error {
                EditError::Command(reason) => reason,
                _ => unreachable!("active preview"),
            })?;
        Ok(world)
    }

    pub fn preview_face(&mut self, placement: FacePlacement) -> Result<Pose, PlacementError> {
        let world = face_pose(self.project(), self.board_id, placement)?;
        self.preview_world(world)
    }

    pub fn preview_snap(&mut self, candidate: SnapCandidate) -> Result<Pose, PlacementError> {
        // Recompute from visible target data rather than trusting a stale pose.
        self.preview_face(FacePlacement {
            source_face: candidate.source_face,
            target_id: candidate.target_id,
            target_face: candidate.target_face,
            source_align: [Align::Centre; 2],
            target_align: [Align::Centre; 2],
            offset_mm: [0.0; 2],
            gap_mm: 0.0,
        })
    }

    /// Bypass snapping by supplying the unmodified free-drag pose.
    pub fn preview_free(&mut self, world: Pose) -> Result<Pose, PlacementError> {
        self.preview_world(world)
    }

    pub fn cancel(self) {
        self.editor.cancel_preview();
    }

    /// Hand a live preview back to a frame-based UI without committing it.
    pub fn pause(mut self) {
        self.cancel_on_drop = false;
    }

    pub fn accept(self) -> Result<bool, EditError<()>> {
        self.editor.commit_preview()
    }
}

impl Drop for PlacementSession<'_> {
    fn drop(&mut self) {
        if self.cancel_on_drop {
            self.editor.cancel_preview();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Assembly, BoardGrain, Material};
    use crate::money::Currency;
    use crate::units::Length;

    fn fixture(nested: bool) -> (ProjectEditor, Uuid, Uuid) {
        let mut project = Project::new("placement", Currency::Brl);
        let material = Uuid::new_v4();
        project.materials.push(Material {
            coating: Default::default(),
            default_band: None,
            kind: Default::default(),
            id: material,
            name: "wood".into(),
            default_thickness: Length::from_micrometres(20_000),
            default_grain: BoardGrain::Unrestricted,
        });
        let parent = if nested {
            let outer = Uuid::new_v4();
            let inner = Uuid::new_v4();
            let turn = Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap();
            project.assemblies.push(Assembly {
                id: outer,
                name: "outer".into(),
                parent_id: None,
                pose: Pose::new([300.0, 200.0, 0.0], turn).unwrap(),
            });
            project.assemblies.push(Assembly {
                id: inner,
                name: "inner".into(),
                parent_id: Some(outer),
                pose: Pose::new([10.0, 0.0, 0.0], turn).unwrap(),
            });
            Some(inner)
        } else {
            None
        };
        let source = Uuid::new_v4();
        let target = Uuid::new_v4();
        for (id, position, parent_id) in [
            (source, [0.0; 3], parent),
            (target, [150.0, 10.0, 0.0], None),
        ] {
            project.boards.push(Board {
                coated_face: Default::default(),
                banding: Default::default(),
                id,
                name: "board".into(),
                material_id: material,
                length: Length::from_micrometres(100_000),
                width: Length::from_micrometres(50_000),
                thickness: Length::from_micrometres(20_000),
                grain_override: None,
                parent_id,
                pose: Pose::new(position, Quaternion::IDENTITY).unwrap(),
            });
        }
        (ProjectEditor::new(project).unwrap(), source, target)
    }

    fn near(a: [f64; 3], b: [f64; 3]) {
        for i in 0..3 {
            assert!(
                (a[i] - b[i]).abs() <= 1e-6,
                "axis {i}: {} vs {}",
                a[i],
                b[i]
            );
        }
    }

    #[test]
    fn rotated_parent_presets_are_tentative_and_exact_until_one_accept() {
        let (mut editor, source, _) = fixture(true);
        let initial = editor.project().clone();
        let revision = initial.revision;
        let standing = preset_pose(initial.boards[0].pose, PosePreset::StandUp).unwrap();
        let turned = preset_pose(standing, PosePreset::Turn90Z).unwrap();
        {
            let mut session = PlacementSession::begin(&mut editor, source).unwrap();
            session
                .preview_exact_framed(CoordinateFrame::LocalParent, standing, [false; 3])
                .unwrap();
            assert_eq!(session.project().boards[0].pose.rotation, standing.rotation);
            session
                .preview_exact_framed(CoordinateFrame::LocalParent, turned, [false; 3])
                .unwrap();
            assert_eq!(session.project().boards[0].pose, turned);
            session.cancel();
        }
        assert!(editor.preview().is_none());
        assert_eq!(editor.project(), &initial);
        assert_eq!(editor.project().revision, revision);
        {
            let mut session = PlacementSession::begin(&mut editor, source).unwrap();
            session
                .preview_exact_framed(CoordinateFrame::LocalParent, turned, [false; 3])
                .unwrap();
            assert_eq!(session.accept(), Ok(true));
        }
        assert_eq!(editor.project().revision, revision + 1);
        assert_eq!(editor.project().boards[0].pose, turned);
        editor.undo().unwrap();
        assert_eq!(editor.project().boards[0].pose, initial.boards[0].pose);
    }

    #[test]
    fn exact_preset_position_rejects_off_grid_without_mutation() {
        let (mut editor, source, _) = fixture(false);
        let initial = editor.project().clone();
        let proposal = Pose::new([0.0005, 0.0, 0.0], Quaternion::IDENTITY).unwrap();
        let mut session = PlacementSession::begin(&mut editor, source).unwrap();
        assert!(matches!(
            session.preview_exact_framed(CoordinateFrame::World, proposal, [true, false, false]),
            Err(PlacementError::OffGrid)
        ));
        session.cancel();
        assert_eq!(editor.project(), &initial);
    }

    #[test]
    fn preset_rotation_preserves_untouched_half_grid_origin() {
        let (mut editor, source, _) = fixture(false);
        let origin = [0.0005, -1.0005, 3.0005];
        editor
            .transact(|project| {
                project.boards[0].pose.translation_mm = origin;
                Ok::<_, ()>(())
            })
            .unwrap();
        let before = editor.project().clone();
        let standing = preset_pose(before.boards[0].pose, PosePreset::StandUp).unwrap();
        let mut session = PlacementSession::begin(&mut editor, source).unwrap();
        session
            .preview_exact_framed(CoordinateFrame::World, standing, [false; 3])
            .unwrap();
        assert_eq!(session.project().boards[0].pose.translation_mm, origin);
        assert_eq!(session.accept(), Ok(true));
        assert_eq!(editor.project().boards[0].pose.translation_mm, origin);
        editor.undo().unwrap();
        assert_eq!(editor.project().boards, before.boards);
    }

    #[test]
    fn world_frame_presets_respect_rotated_parent_and_absolute_alignment() {
        let (mut editor, source, _) = fixture(true);
        let original = editor.project().clone();
        let current_world = world_pose(&original, source).unwrap();
        let flat_world = preset_pose(current_world, PosePreset::LayFlat).unwrap();
        assert_eq!(flat_world.rotation, Quaternion::IDENTITY);
        let turned_world = preset_pose(flat_world, PosePreset::Turn90Z).unwrap();
        near(
            turned_world.rotation.rotate([1.0, 0.0, 0.0]),
            [0.0, 1.0, 0.0],
        );
        {
            let mut session = PlacementSession::begin(&mut editor, source).unwrap();
            session
                .preview_exact_framed(CoordinateFrame::World, flat_world, [false; 3])
                .unwrap();
            assert_eq!(
                world_pose(session.project(), source).unwrap().rotation,
                Quaternion::IDENTITY
            );
            session
                .preview_exact_framed(CoordinateFrame::World, turned_world, [false; 3])
                .unwrap();
            near(
                world_pose(session.project(), source)
                    .unwrap()
                    .rotation
                    .rotate([1.0, 0.0, 0.0]),
                [0.0, 1.0, 0.0],
            );
            session.cancel();
        }
        assert_eq!(editor.project(), &original);
    }

    #[test]
    fn edited_numeric_components_preserve_derived_pose_precision() {
        let (mut editor, source, _) = fixture(true);
        let exact_rotation = Quaternion::normalized(0.91, 0.12, 0.2, 0.31).unwrap();
        let exact = Pose::new([0.0005, -1.0005, 3.0005], exact_rotation).unwrap();
        editor
            .transact(|project| {
                project
                    .boards
                    .iter_mut()
                    .find(|b| b.id == source)
                    .unwrap()
                    .pose = exact;
                Ok::<_, ()>(())
            })
            .unwrap();
        let before = editor.project().clone();
        let revision = before.revision;
        let input = NumericPose {
            position_mm: [0.001, -1.001, 3.001],
            rotation_degrees_xyz: [0.0, 0.0, 0.0],
            frame: CoordinateFrame::LocalParent,
        };
        {
            let mut session = PlacementSession::begin(&mut editor, source).unwrap();
            session
                .preview_numeric_edited(input, [false; 3], false)
                .unwrap();
            assert_eq!(
                session
                    .project()
                    .boards
                    .iter()
                    .find(|b| b.id == source)
                    .unwrap()
                    .pose,
                exact
            );
            assert_eq!(session.accept(), Ok(false));
        }
        assert_eq!(editor.project(), &before);
        assert_eq!(editor.project().revision, revision);
        {
            let mut session = PlacementSession::begin(&mut editor, source).unwrap();
            session
                .preview_numeric_edited(
                    NumericPose {
                        position_mm: [2.0, -1.001, 3.001],
                        ..input
                    },
                    [true, false, false],
                    false,
                )
                .unwrap();
            let pose = session
                .project()
                .boards
                .iter()
                .find(|b| b.id == source)
                .unwrap()
                .pose;
            assert_eq!(
                pose.translation_mm,
                [2.0, exact.translation_mm[1], exact.translation_mm[2]]
            );
            assert_eq!(pose.rotation, exact_rotation);
            session.cancel();
        }
        {
            let mut session = PlacementSession::begin(&mut editor, source).unwrap();
            session
                .preview_numeric_edited(
                    NumericPose {
                        rotation_degrees_xyz: [10.0, 20.0, 30.0],
                        ..input
                    },
                    [false; 3],
                    true,
                )
                .unwrap();
            let pose = session
                .project()
                .boards
                .iter()
                .find(|b| b.id == source)
                .unwrap()
                .pose;
            assert_eq!(pose.translation_mm, exact.translation_mm);
            session.cancel();
        }
        {
            let mut session = PlacementSession::begin(&mut editor, source).unwrap();
            session
                .preview_numeric_edited(
                    NumericPose {
                        frame: CoordinateFrame::World,
                        position_mm: [25.0, 0.0, 0.0],
                        ..input
                    },
                    [true, false, false],
                    false,
                )
                .unwrap();
            assert_eq!(
                session
                    .project()
                    .boards
                    .iter()
                    .find(|b| b.id == source)
                    .unwrap()
                    .pose
                    .rotation,
                exact_rotation
            );
            session.cancel();
        }
        {
            let mut session = PlacementSession::begin(&mut editor, source).unwrap();
            session
                .preview_numeric_edited(
                    NumericPose {
                        frame: CoordinateFrame::World,
                        rotation_degrees_xyz: [10.0, 20.0, 30.0],
                        ..input
                    },
                    [false; 3],
                    true,
                )
                .unwrap();
            assert_eq!(
                session
                    .project()
                    .boards
                    .iter()
                    .find(|b| b.id == source)
                    .unwrap()
                    .pose
                    .translation_mm,
                exact.translation_mm
            );
            session.cancel();
        }
        assert_eq!(editor.project(), &before);
    }

    #[test]
    fn numeric_validation_and_cancel_leave_history_and_previous_preview_intact() {
        let (mut editor, source, _) = fixture(false);
        let before = editor.project().clone();
        let mut session = PlacementSession::begin(&mut editor, source).unwrap();
        let valid = NumericPose {
            position_mm: [30.0, -10.0, 2.0],
            rotation_degrees_xyz: [0.0, 0.0, 90.0],
            frame: CoordinateFrame::World,
        };
        session.preview_numeric(valid).unwrap();
        let preview = session.project().clone();
        for input in [
            NumericPose {
                position_mm: [f64::NAN, 0.0, 0.0],
                ..valid
            },
            NumericPose {
                position_mm: [1_000_001.0, 0.0, 0.0],
                ..valid
            },
            NumericPose {
                position_mm: [0.0001, 0.0, 0.0],
                ..valid
            },
            NumericPose {
                rotation_degrees_xyz: [0.0, f64::INFINITY, 0.0],
                ..valid
            },
            NumericPose {
                rotation_degrees_xyz: [361.0, 0.0, 0.0],
                ..valid
            },
        ] {
            assert!(session.preview_numeric(input).is_err());
            assert_eq!(session.project(), &preview);
        }
        session.cancel();
        assert_eq!(editor.project(), &before);
        assert_eq!(editor.preview(), None);
        assert!(!editor.can_undo());
        assert!(!editor.is_dirty());
    }

    #[test]
    fn face_alignment_offsets_normals_and_commits_only_pose() {
        let (mut editor, source, target) = fixture(false);
        let placement = FacePlacement {
            source_face: BoardFace {
                axis: 0,
                side: Side::Negative,
            },
            target_id: target,
            target_face: BoardFace {
                axis: 0,
                side: Side::Positive,
            },
            source_align: [Align::End, Align::Centre],
            target_align: [Align::Start, Align::Centre],
            offset_mm: [7.0, -3.0],
            gap_mm: 2.0,
        };
        let original = editor.project().clone();
        let mut session = PlacementSession::begin(&mut editor, source).unwrap();
        let pose = session.preview_face(placement).unwrap();
        near(pose.translation_mm, [252.0, -33.0, -3.0]);
        near(pose.rotation.rotate([-1.0, 0.0, 0.0]), [-1.0, 0.0, 0.0]);
        assert_eq!(session.editor.project(), &original);
        assert_eq!(session.accept(), Ok(true));
        assert_eq!(editor.project().revision, 1);
        assert_eq!(editor.project().boards[1], original.boards[1]);
        editor.undo().unwrap();
        assert_eq!(editor.project().boards, original.boards);
    }

    #[test]
    fn snap_bypass_and_former_target_are_independent() {
        let (mut editor, source, target) = fixture(false);
        let free = Pose::new([248.0, 10.0, 0.0], Quaternion::IDENTITY).unwrap();
        let candidates = snap_candidates(editor.project(), source, free, 100.0).unwrap();
        assert!(!candidates.is_empty());
        assert!(
            candidates
                .windows(2)
                .all(|pair| pair[0].distance_mm <= pair[1].distance_mm)
        );
        let mut session = PlacementSession::begin(&mut editor, source).unwrap();
        session.preview_snap(candidates[0]).unwrap();
        session.preview_free(free).unwrap();
        assert_eq!(session.accept(), Ok(true));
        near(
            world_pose(editor.project(), source).unwrap().translation_mm,
            free.translation_mm,
        );
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards
                    .iter_mut()
                    .find(|b| b.id == target)
                    .unwrap()
                    .pose
                    .translation_mm[0] += 200.0;
                Ok(())
            })
            .unwrap();
        near(
            world_pose(editor.project(), source).unwrap().translation_mm,
            free.translation_mm,
        );
    }

    #[test]
    fn nested_parent_numeric_and_face_roundtrip_in_world_space() {
        let (mut editor, source, target) = fixture(true);
        let parent = parent_world(editor.project(), editor.project().boards[0].parent_id).unwrap();
        let desired = Pose::new([40.0, 80.0, 0.0], Quaternion::IDENTITY).unwrap();
        let mut session = PlacementSession::begin(&mut editor, source).unwrap();
        session.preview_world(desired).unwrap();
        near(
            world_pose(session.project(), source)
                .unwrap()
                .translation_mm,
            desired.translation_mm,
        );
        session
            .preview_numeric(NumericPose {
                position_mm: [5.0, 3.0, 0.0],
                rotation_degrees_xyz: [0.0, 0.0, 90.0],
                frame: CoordinateFrame::LocalParent,
            })
            .unwrap();
        near(
            world_pose(session.project(), source)
                .unwrap()
                .translation_mm,
            parent.transform_point([5.0, 3.0, 0.0]).unwrap(),
        );
        let request = FacePlacement {
            source_face: BoardFace {
                axis: 2,
                side: Side::Negative,
            },
            target_id: target,
            target_face: BoardFace {
                axis: 2,
                side: Side::Positive,
            },
            source_align: [Align::Centre; 2],
            target_align: [Align::Centre; 2],
            offset_mm: [2.0, -4.0],
            gap_mm: 0.0,
        };
        let face_world = session.preview_face(request).unwrap();
        near(
            world_pose(session.project(), source)
                .unwrap()
                .translation_mm,
            face_world.translation_mm,
        );
        session.accept().unwrap();
        near(
            world_pose(editor.project(), source).unwrap().translation_mm,
            face_world.translation_mm,
        );
    }

    #[test]
    fn world_bounds_check_corners_including_nested_rotation() {
        let (mut editor, source, _) = fixture(true);
        let mut session = PlacementSession::begin(&mut editor, source).unwrap();
        assert_eq!(
            session.preview_world(Pose::new([999_950.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap()),
            Err(PlacementError::InvalidPose(UnitError::OutOfBounds))
        );
        session.cancel();
        assert_eq!(editor.project().revision, 0);
    }

    #[test]
    fn different_face_axes_have_opposing_normals_and_aligned_tangents() {
        let (editor, source, target) = fixture(false);
        for source_face in [
            BoardFace {
                axis: 0,
                side: Side::Positive,
            },
            BoardFace {
                axis: 1,
                side: Side::Negative,
            },
        ] {
            for target_face in [
                BoardFace {
                    axis: 2,
                    side: Side::Negative,
                },
                BoardFace {
                    axis: 1,
                    side: Side::Positive,
                },
            ] {
                let world = face_pose(
                    editor.project(),
                    source,
                    FacePlacement {
                        source_face,
                        target_id: target,
                        target_face,
                        source_align: [Align::Centre; 2],
                        target_align: [Align::Centre; 2],
                        offset_mm: [0.0; 2],
                        gap_mm: 0.0,
                    },
                )
                .unwrap();
                let target_world = world_pose(editor.project(), target).unwrap();
                let source_normal = world
                    .rotation
                    .rotate(axis(source_face.axis, source_face.side.sign()));
                let target_normal = target_world
                    .rotation
                    .rotate(axis(target_face.axis, target_face.side.sign()));
                near(source_normal, target_normal.map(|v| -v));
                near(
                    world
                        .rotation
                        .rotate(axis(axes(source_face).unwrap()[0], 1.0)),
                    target_world
                        .rotation
                        .rotate(axis(axes(target_face).unwrap()[0], 1.0)),
                );
            }
        }
    }
}
