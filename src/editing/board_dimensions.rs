//! Local-axis board dimension edits with world-space preflight and undoable commit.
use std::collections::HashSet;
use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::{Board, Project};
use crate::material_changes::{AllocationConflict, allocation_conflicts};
use crate::units::{Anchor, Length, Pose, UnitError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardDimension {
    Length,
    Width,
    Thickness,
}

impl BoardDimension {
    pub fn axis(self) -> usize {
        match self {
            Self::Length => 0,
            Self::Width => 1,
            Self::Thickness => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardSelection {
    Board(Uuid),
    Assembly(Uuid),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionValue {
    Uniform(Length),
    Mixed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedBoards {
    pub board_ids: Vec<Uuid>,
    pub dimensions: [SelectionValue; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchDimensionError {
    MissingSelection(Uuid),
    EmptySelection,
    MissingAnchor(Uuid),
    Target {
        board_id: Uuid,
        reason: DimensionEditError,
    },
    StalePreview,
}

pub struct BatchDimensionPreview {
    project_id: Uuid,
    revision: u64,
    pub dimension: BoardDimension,
    pub value: Length,
    pub targets: Vec<(Uuid, Anchor)>,
    pub conflicts: Vec<AllocationConflict>,
}

fn selected_ids(
    project: &Project,
    selection: &[BoardSelection],
) -> Result<Vec<Uuid>, BatchDimensionError> {
    let mut selected = HashSet::new();
    let mut assemblies = HashSet::new();
    for item in selection {
        match *item {
            BoardSelection::Board(id) => {
                if !project.boards.iter().any(|board| board.id == id) {
                    return Err(BatchDimensionError::MissingSelection(id));
                }
                selected.insert(id);
            }
            BoardSelection::Assembly(id) => {
                if !project.assemblies.iter().any(|assembly| assembly.id == id) {
                    return Err(BatchDimensionError::MissingSelection(id));
                }
                assemblies.insert(id);
            }
        }
    }
    let ids = project
        .boards
        .iter()
        .filter(|board| {
            if selected.contains(&board.id) {
                return true;
            }
            let mut parent = board.parent_id;
            while let Some(id) = parent {
                if assemblies.contains(&id) {
                    return true;
                }
                parent = project
                    .assemblies
                    .iter()
                    .find(|a| a.id == id)
                    .and_then(|a| a.parent_id);
            }
            false
        })
        .map(|board| board.id)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        Err(BatchDimensionError::EmptySelection)
    } else {
        Ok(ids)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DimensionEditError {
    MissingBoard(Uuid),
    InvalidDimension(UnitError),
    InvalidPose(UnitError),
    StalePreview,
}

pub struct DimensionPreview {
    project_id: Uuid,
    revision: u64,
    board_id: Uuid,
    pub dimension: BoardDimension,
    pub anchor: Anchor,
    pub value: Length,
    pub pose: Pose,
    pub conflicts: Vec<AllocationConflict>,
}

fn resized_board(
    project: &Project,
    board: &Board,
    dimension: BoardDimension,
    value: Length,
    anchor: Anchor,
) -> Result<Board, DimensionEditError> {
    value
        .positive()
        .map_err(DimensionEditError::InvalidDimension)?;
    let axis = dimension.axis();
    let mut result = board.clone();
    result.pose = board
        .pose
        .resized(axis, board.blank_dimensions()[axis], value, anchor)
        .map_err(DimensionEditError::InvalidPose)?;
    match dimension {
        BoardDimension::Length => result.length = value,
        BoardDimension::Width => result.width = value,
        BoardDimension::Thickness => result.thickness = value,
    }

    // Fold from the immediate parent toward the root: each parent wraps the
    // already composed child transform, yielding outer(inner(board)).
    let mut ancestors = Vec::new();
    let mut parent_id = board.parent_id;
    while let Some(id) = parent_id {
        let parent = project
            .assemblies
            .iter()
            .find(|assembly| assembly.id == id)
            .ok_or(DimensionEditError::InvalidPose(UnitError::OutOfBounds))?;
        ancestors.push(parent.pose);
        parent_id = parent.parent_id;
    }
    let world = ancestors
        .into_iter()
        .try_fold(result.pose, |child, parent| {
            parent
                .compose(child)
                .map_err(DimensionEditError::InvalidPose)
        })?;
    let [x, y, z] = result
        .blank_dimensions()
        .map(|length| length.micrometres() as f64 / 1000.0);
    for x in [0.0, x] {
        for y in [0.0, y] {
            for z in [0.0, z] {
                world
                    .transform_point([x, y, z])
                    .map_err(DimensionEditError::InvalidPose)?;
            }
        }
    }
    Ok(result)
}

impl ProjectEditor {
    /// Copy the part with a new physical identity and requested local pose. Allocations
    /// refer to the old identity and are never copied.
    pub fn duplicate_board(
        &mut self,
        board_id: Uuid,
        pose: Pose,
    ) -> Result<Uuid, EditError<DimensionEditError>> {
        self.duplicate_board_with_fit(board_id, pose)
            .map(|(id, _)| id)
    }

    pub fn duplicate_board_with_fit(
        &mut self,
        board_id: Uuid,
        pose: Pose,
    ) -> Result<(Uuid, crate::first_fit::FirstFit), EditError<DimensionEditError>> {
        let id = Uuid::new_v4();
        let mut fit = crate::first_fit::FirstFit::NoFit;
        self.transact(|project| {
            let original = project
                .boards
                .iter()
                .find(|b| b.id == board_id)
                .ok_or(DimensionEditError::MissingBoard(board_id))?;
            let mut copy = original.duplicate();
            copy.id = id;
            copy.pose = pose;
            // Also check the corners in world coordinates through all parent assemblies.
            let copy = resized_board(
                project,
                &copy,
                BoardDimension::Length,
                copy.length,
                Anchor::Start,
            )?;
            project.boards.push(copy);
            fit = crate::first_fit::allocate_new_board(project, id);
            Ok(())
        })?;
        Ok((id, fit))
    }

    pub fn selected_boards(
        &self,
        selection: &[BoardSelection],
    ) -> Result<SelectedBoards, BatchDimensionError> {
        let project = self.project();
        let board_ids = selected_ids(project, selection)?;
        let blanks = board_ids
            .iter()
            .map(|&id| {
                project
                    .board(id)
                    .map(|board| board.blank_dimensions())
                    .ok_or(BatchDimensionError::MissingSelection(id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let &first_blank = blanks.first().ok_or(BatchDimensionError::EmptySelection)?;
        let dimensions = [0, 1, 2].map(|axis| {
            let first = first_blank[axis];
            if blanks.iter().all(|blank| blank[axis] == first) {
                SelectionValue::Uniform(first)
            } else {
                SelectionValue::Mixed
            }
        });
        Ok(SelectedBoards {
            board_ids,
            dimensions,
        })
    }

    /// Preflight the entire selection against the same immutable project snapshot.
    /// The anchor is associated with each distinct board ID.
    pub fn preview_batch_board_dimension(
        &self,
        selection: &[BoardSelection],
        dimension: BoardDimension,
        value: Length,
        anchors: &[(Uuid, Anchor)],
    ) -> Result<BatchDimensionPreview, BatchDimensionError> {
        let project = self.project();
        let ids = selected_ids(project, selection)?;
        let mut candidate = project.clone();
        let mut targets = Vec::with_capacity(ids.len());
        for id in ids {
            let anchor = anchors
                .iter()
                .find(|(target, _)| *target == id)
                .map(|(_, anchor)| *anchor)
                .ok_or(BatchDimensionError::MissingAnchor(id))?;
            let index = candidate
                .boards
                .iter()
                .position(|b| b.id == id)
                .ok_or(BatchDimensionError::MissingSelection(id))?;
            candidate.boards[index] =
                resized_board(project, &project.boards[index], dimension, value, anchor).map_err(
                    |reason| BatchDimensionError::Target {
                        board_id: id,
                        reason,
                    },
                )?;
            targets.push((id, anchor));
        }
        let conflicts = allocation_conflicts(&candidate);
        Ok(BatchDimensionPreview {
            project_id: project.id,
            revision: project.revision,
            dimension,
            value,
            targets,
            conflicts,
        })
    }

    pub fn edit_batch_board_dimension(
        &mut self,
        preview: BatchDimensionPreview,
    ) -> Result<Vec<AllocationConflict>, EditError<BatchDimensionError>> {
        if self.project().id != preview.project_id || self.project().revision != preview.revision {
            return Err(EditError::Command(BatchDimensionError::StalePreview));
        }
        self.transact(|project| {
            for (id, anchor) in &preview.targets {
                let index = project.boards.iter().position(|b| b.id == *id).ok_or(
                    BatchDimensionError::Target {
                        board_id: *id,
                        reason: DimensionEditError::MissingBoard(*id),
                    },
                )?;
                project.boards[index] = resized_board(
                    project,
                    &project.boards[index],
                    preview.dimension,
                    preview.value,
                    *anchor,
                )
                .map_err(|reason| BatchDimensionError::Target {
                    board_id: *id,
                    reason,
                })?;
            }
            Ok(())
        })?;
        Ok(allocation_conflicts(self.project()))
    }

    pub fn preview_board_dimension(
        &self,
        board_id: Uuid,
        dimension: BoardDimension,
        value: Length,
        anchor: Anchor,
    ) -> Result<DimensionPreview, DimensionEditError> {
        let project = self.project();
        let board = project
            .boards
            .iter()
            .find(|board| board.id == board_id)
            .ok_or(DimensionEditError::MissingBoard(board_id))?;
        let changed = resized_board(project, board, dimension, value, anchor)?;
        let pose = changed.pose;
        let mut candidate = project.clone();
        *candidate
            .board_mut(board_id)
            .ok_or(DimensionEditError::MissingBoard(board_id))? = changed;
        let conflicts = allocation_conflicts(&candidate)
            .into_iter()
            .filter(|issue| issue.board_id == board_id)
            .collect();
        Ok(DimensionPreview {
            project_id: project.id,
            revision: project.revision,
            board_id,
            dimension,
            anchor,
            value,
            pose,
            conflicts,
        })
    }

    pub fn edit_board_dimension(
        &mut self,
        preview: DimensionPreview,
    ) -> Result<Vec<AllocationConflict>, EditError<DimensionEditError>> {
        if self.project().id != preview.project_id || self.project().revision != preview.revision {
            return Err(EditError::Command(DimensionEditError::StalePreview));
        }
        self.transact(|project| {
            let index = project
                .boards
                .iter()
                .position(|board| board.id == preview.board_id)
                .ok_or(DimensionEditError::MissingBoard(preview.board_id))?;
            let result = resized_board(
                project,
                &project.boards[index],
                preview.dimension,
                preview.value,
                preview.anchor,
            )?;
            project.boards[index] = result;
            Ok(())
        })?;
        Ok(allocation_conflicts(self.project()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Allocation, Assembly, BoardGrain, Material, Stock, StockGrain, StockSource,
    };
    use crate::material_changes::ConflictReason;
    use crate::money::Currency;
    use crate::units::Quaternion;

    fn mm(value: i64) -> Length {
        Length::from_micrometres(value * 1000)
    }

    fn fixture() -> ProjectEditor {
        let mut project = Project::new("Cabinet", Currency::Brl);
        let material_id = Uuid::new_v4();
        project.materials.push(Material {
            default_band: None,
            kind: Default::default(),
            id: material_id,
            name: "Plywood".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Unrestricted,
        });
        let outer = Uuid::new_v4();
        let inner = Uuid::new_v4();
        let rotation = Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap();
        project.assemblies.push(Assembly {
            id: outer,
            name: "Outer".into(),
            parent_id: None,
            pose: Pose::new([100.0, 200.0, 0.0], rotation).unwrap(),
        });
        project.assemblies.push(Assembly {
            id: inner,
            name: "Inner".into(),
            parent_id: Some(outer),
            pose: Pose::new([10.0, 20.0, 0.0], rotation).unwrap(),
        });
        let board_id = Uuid::new_v4();
        project.boards.push(Board {
            banding: Default::default(),
            id: board_id,
            name: "Shelf".into(),
            material_id,
            length: mm(100),
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: Some(inner),
            pose: Pose::new([5.0, 6.0, 0.0], Quaternion::IDENTITY).unwrap(),
        });
        let stock_id = Uuid::new_v4();
        project.stock.push(Stock {
            id: stock_id,
            name: "Sheet".into(),
            material_id,
            length: mm(110),
            width: mm(100),
            thickness: mm(18),
            grain: StockGrain::Nondirectional,
            source: StockSource::Owned,
            price: None,
            priority: 0,
            trim: [Length::ZERO; 4],
        });
        project.allocations.push(Allocation {
            id: Uuid::new_v4(),
            board_id,
            stock_id,
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: true,
        });
        ProjectEditor::new(project).unwrap()
    }

    fn world(project: &Project) -> Pose {
        project.assemblies[0]
            .pose
            .compose(project.assemblies[1].pose)
            .unwrap()
            .compose(project.boards[0].pose)
            .unwrap()
    }

    #[test]
    fn anchors_preserve_world_faces_and_half_grid_in_nested_rotated_assemblies() {
        for (axis, dimension, new, anchor, preserved) in [
            (0, BoardDimension::Length, 120.0, Anchor::End, 100.0),
            (0, BoardDimension::Length, 120.0, Anchor::Start, 0.0),
            (2, BoardDimension::Thickness, 15.0, Anchor::Centre, 9.0),
        ] {
            let mut editor = fixture();
            let before = editor.project().clone();
            let id = before.boards[0].id;
            let mut point = [0.0; 3];
            point[axis] = preserved;
            let before_face = world(&before).transform_point(point).unwrap();
            let preview = editor
                .preview_board_dimension(id, dimension, mm(new as i64), anchor)
                .unwrap();
            assert_eq!(editor.project(), &before);
            editor.edit_board_dimension(preview).unwrap();
            let after = editor.project();
            point[axis] = if anchor == Anchor::End {
                new
            } else if anchor == Anchor::Centre {
                new / 2.0
            } else {
                0.0
            };
            let after_face = world(after).transform_point(point).unwrap();
            for i in 0..3 {
                assert!((before_face[i] - after_face[i]).abs() < 1e-6);
            }
            assert_eq!(
                after.boards[0].pose.rotation,
                before.boards[0].pose.rotation
            );
            for i in 0..3 {
                if i != axis {
                    assert_eq!(
                        after.boards[0].blank_dimensions()[i],
                        before.boards[0].blank_dimensions()[i]
                    );
                }
            }
            assert_eq!(after.allocations, before.allocations);
            editor.undo().unwrap();
            assert_eq!(editor.project().boards, before.boards);
        }
        let mut editor = fixture();
        let id = editor.project().boards[0].id;
        let preview = editor
            .preview_board_dimension(
                id,
                BoardDimension::Length,
                Length::from_micrometres(100_001),
                Anchor::Centre,
            )
            .unwrap();
        assert!((preview.pose.translation_mm[0] - 4.9995).abs() < 1e-9);
        editor.edit_board_dimension(preview).unwrap();
        assert!((editor.project().boards[0].pose.translation_mm[0] - 4.9995).abs() < 1e-9);
    }

    #[test]
    fn nested_world_bound_uses_outer_parent_after_inner_parent() {
        let mut editor = fixture();
        editor
            .transact(|p| -> Result<(), ()> {
                p.assemblies[0].pose = Pose::new(
                    [999_900.0, 0.0, 0.0],
                    Quaternion::normalized(0.0, 0.0, 0.0, 1.0).unwrap(),
                )
                .unwrap();
                p.assemblies[1].pose = Pose::new([-99.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap();
                p.boards[0].pose = Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap();
                Ok(())
            })
            .unwrap();
        let board = editor.project().boards[0].id;
        let before = editor.project().clone();
        assert!(matches!(
            editor.preview_board_dimension(board, BoardDimension::Length, mm(200), Anchor::End),
            Err(DimensionEditError::InvalidPose(UnitError::OutOfBounds))
        ));
        assert_eq!(editor.project(), &before);
    }

    #[test]
    fn invalid_and_world_bounds_reject_without_history_or_partial_allocation_change() {
        let mut editor = fixture();
        let id = editor.project().boards[0].id;
        let before = editor.project().clone();
        assert_eq!(
            editor
                .preview_board_dimension(id, BoardDimension::Length, Length::ZERO, Anchor::End)
                .err(),
            Some(DimensionEditError::InvalidDimension(
                UnitError::NonPositiveDimension
            ))
        );
        assert_eq!(
            editor
                .preview_board_dimension(
                    id,
                    BoardDimension::Width,
                    Length::from_micrometres(-1),
                    Anchor::Start
                )
                .err(),
            Some(DimensionEditError::InvalidDimension(
                UnitError::NonPositiveDimension
            ))
        );
        assert_eq!(editor.project(), &before);
        assert!(!editor.can_undo());
        editor
            .transact(|p| -> Result<(), ()> {
                p.assemblies[0].pose =
                    Pose::new([999_800.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap();
                p.assemblies[1].pose.rotation = Quaternion::IDENTITY;
                Ok(())
            })
            .unwrap();
        let near_edge = editor.project().clone();
        assert_eq!(
            editor
                .preview_board_dimension(id, BoardDimension::Length, mm(200), Anchor::Start)
                .err(),
            Some(DimensionEditError::InvalidPose(UnitError::OutOfBounds))
        );
        assert_eq!(editor.project(), &near_edge);
    }

    #[test]
    fn resizing_retains_locked_allocation_and_derives_footprint_thickness_and_overlap_conflicts() {
        let mut editor = fixture();
        let id = editor.project().boards[0].id;
        let placement = editor.project().allocations[0].clone();
        let preview = editor
            .preview_board_dimension(id, BoardDimension::Length, mm(120), Anchor::Start)
            .unwrap();
        assert_eq!(
            preview.conflicts[0].reasons,
            vec![ConflictReason::OutsideStock]
        );
        editor.edit_board_dimension(preview).unwrap();
        assert_eq!(editor.project().allocations[0], placement);
        editor.undo().unwrap();
        let preview = editor
            .preview_board_dimension(id, BoardDimension::Thickness, mm(15), Anchor::Centre)
            .unwrap();
        assert_eq!(
            preview.conflicts[0].reasons,
            vec![ConflictReason::EffectiveThickness]
        );
        editor.edit_board_dimension(preview).unwrap();
        assert_eq!(editor.project().allocations[0], placement);
        editor.undo().unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                let mut other = p.boards[0].duplicate();
                other.length = mm(5);
                other.pose.translation_mm[0] = 0.0;
                let other_id = other.id;
                p.boards.push(other);
                p.allocations.push(Allocation {
                    id: Uuid::new_v4(),
                    board_id: other_id,
                    stock_id: placement.stock_id,
                    origin: [mm(103), Length::ZERO],
                    quarter_turn: false,
                    locked: false,
                });
                Ok(())
            })
            .unwrap();
        let preview = editor
            .preview_board_dimension(id, BoardDimension::Length, mm(105), Anchor::Start)
            .unwrap();
        assert!(
            preview.conflicts[0]
                .reasons
                .contains(&ConflictReason::Overlap)
        );
    }

    #[test]
    fn stale_preview_cannot_overwrite_a_newer_edit() {
        let mut editor = fixture();
        let id = editor.project().boards[0].id;
        let preview = editor
            .preview_board_dimension(id, BoardDimension::Length, mm(105), Anchor::End)
            .unwrap();
        editor
            .transact(|project| -> Result<(), ()> {
                project.boards[0].name = "Renamed".into();
                Ok(())
            })
            .unwrap();
        let current = editor.project().clone();
        assert_eq!(
            editor.edit_board_dimension(preview),
            Err(EditError::Command(DimensionEditError::StalePreview))
        );
        assert_eq!(editor.project(), &current);
    }

    #[test]
    fn duplication_is_independent_and_unallocated_even_when_original_is_allocated() {
        let mut editor = fixture();
        let original = editor.project().boards[0].clone();
        let placement = editor.project().allocations.clone();
        let pose = Pose::new([30.0, 40.0, 0.0], Quaternion::IDENTITY).unwrap();
        let copy_id = editor.duplicate_board(original.id, pose).unwrap();
        let copy = editor.project().boards.last().unwrap();
        assert_ne!(copy.id, original.id);
        assert_eq!(copy.pose, pose);
        assert_eq!(copy.blank_dimensions(), original.blank_dimensions());
        assert_eq!(copy.grain_override, original.grain_override);
        assert_eq!(copy.material_id, original.material_id);
        assert_eq!(editor.project().allocations, placement);
        assert!(
            !editor
                .project()
                .allocations
                .iter()
                .any(|a| a.board_id == copy_id)
        );
        // A failed attempt to assign the copy to the already occupied stock
        // cannot discard this independent design part.
        let before = editor.project().clone();
        assert!(
            editor
                .transact(|p| -> Result<(), ()> {
                    let mut allocation = placement[0].clone();
                    allocation.id = Uuid::new_v4();
                    allocation.board_id = copy_id;
                    p.allocations.push(allocation);
                    Err(())
                })
                .is_err()
        );
        assert_eq!(editor.project(), &before);
        let preview = editor
            .preview_board_dimension(copy_id, BoardDimension::Length, mm(75), Anchor::Start)
            .unwrap();
        editor.edit_board_dimension(preview).unwrap();
        assert_eq!(editor.project().boards[0], original);
        editor.undo().unwrap();
        editor.undo().unwrap();
        assert_eq!(editor.project().boards.len(), 1);
    }

    #[test]
    fn mixed_assembly_selection_deduplicates_and_commits_once() {
        let mut editor = fixture();
        let original = editor.project().clone();
        let inner = original.assemblies[1].id;
        let first = original.boards[0].id;
        let second = editor
            .duplicate_board(first, original.boards[0].pose)
            .unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[1].width = mm(60);
                Ok(())
            })
            .unwrap();
        let selection = [
            BoardSelection::Assembly(inner),
            BoardSelection::Board(first),
            BoardSelection::Board(first),
        ];
        let summary = editor.selected_boards(&selection).unwrap();
        assert_eq!(summary.board_ids, vec![first, second]);
        assert_eq!(summary.dimensions[1], SelectionValue::Mixed);
        let before = editor.project().clone();
        let preview = editor
            .preview_batch_board_dimension(
                &selection,
                BoardDimension::Width,
                mm(70),
                &[(first, Anchor::Start), (second, Anchor::End)],
            )
            .unwrap();
        assert_eq!(preview.targets.len(), 2);
        editor.edit_batch_board_dimension(preview).unwrap();
        assert!(editor.project().boards.iter().all(|b| b.width == mm(70)));
        assert_eq!(editor.project().boards[0].pose, before.boards[0].pose);
        editor.undo().unwrap();
        assert_eq!(editor.project().boards, before.boards);
    }

    #[test]
    fn invalid_batch_target_does_not_commit_any_board_or_allocation() {
        let mut editor = fixture();
        let first = editor.project().boards[0].id;
        let second = editor
            .duplicate_board(first, editor.project().boards[0].pose)
            .unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[1].pose = Pose::new([999_950.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap();
                Ok(())
            })
            .unwrap();
        let before = editor.project().clone();
        let selection = [BoardSelection::Board(first), BoardSelection::Board(second)];
        assert!(
            matches!(editor.preview_batch_board_dimension(&selection, BoardDimension::Length, mm(100), &[(first, Anchor::Start), (second, Anchor::Start)]), Err(BatchDimensionError::Target { board_id, reason: DimensionEditError::InvalidPose(UnitError::OutOfBounds) }) if board_id == second)
        );
        assert_eq!(editor.project(), &before);
        assert!(
            matches!(editor.preview_batch_board_dimension(&selection, BoardDimension::Length, Length::ZERO, &[(first, Anchor::Start), (second, Anchor::Start)]), Err(BatchDimensionError::Target { board_id, .. }) if board_id == first)
        );
        assert_eq!(editor.project(), &before);
    }

    #[test]
    fn batch_reports_conflicts_and_rejects_stale_preview() {
        let mut editor = fixture();
        let first = editor.project().boards[0].id;
        let second = editor
            .duplicate_board(first, editor.project().boards[0].pose)
            .unwrap();
        let selection = [BoardSelection::Board(first), BoardSelection::Board(second)];
        let anchors = [(first, Anchor::Start), (second, Anchor::Centre)];
        let preview = editor
            .preview_batch_board_dimension(&selection, BoardDimension::Length, mm(120), &anchors)
            .unwrap();
        assert!(
            preview
                .conflicts
                .iter()
                .any(|c| c.board_id == first && c.reasons.contains(&ConflictReason::OutsideStock))
        );
        editor.edit_batch_board_dimension(preview).unwrap();
        assert_eq!(editor.project().allocations.len(), 1);
        let stale = editor
            .preview_batch_board_dimension(&selection, BoardDimension::Width, mm(70), &anchors)
            .unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.name = "Updated".into();
                Ok(())
            })
            .unwrap();
        let before = editor.project().clone();
        assert!(matches!(
            editor.edit_batch_board_dimension(stale),
            Err(EditError::Command(BatchDimensionError::StalePreview))
        ));
        assert_eq!(editor.project(), &before);
    }
}
