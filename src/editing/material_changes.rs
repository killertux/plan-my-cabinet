//! Explicit material-default edits and derived compatibility diagnostics.
//! Previews are immutable proposals; committing one is a single editor transaction.
use std::collections::HashSet;

use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::{Allocation, Board, BoardGrain, Material, Project, Stock, StockGrain};
use crate::units::{Anchor, Length, Pose, UnitError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialChangeError {
    MissingMaterial(Uuid),
    MissingBoard(Uuid),
    InvalidThickness(UnitError),
    InvalidPose { board_id: Uuid, reason: UnitError },
    StalePreview,
    DuplicateSelection(Uuid),
    NotDependent(Uuid),
}

/// Why a material cannot be deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialDeleteError {
    MissingMaterial(Uuid),
    /// Boards and stock pieces still use it; reassign or delete them first.
    InUse {
        boards: Vec<Uuid>,
        stock: Vec<Uuid>,
    },
}

/// The dimensions and grain before/after an explicit decision, including a
/// diagnostic for an allocation retained in its original position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffectedBoard {
    pub id: Uuid,
    pub name: String,
    pub thickness_before: Length,
    pub thickness_if_applied: Length,
    pub grain_before: BoardGrain,
    pub grain_if_applied: BoardGrain,
    /// Applying this board would exceed the supported pose bounds.
    pub apply_error: Option<UnitError>,
    pub allocation_if_applied: Option<AllocationConflict>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictReason {
    MaterialIdentity,
    EffectiveThickness,
    Grain,
    OutsideStock,
    Overlap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllocationConflict {
    pub allocation_id: Uuid,
    pub board_id: Uuid,
    pub stock_id: Uuid,
    pub reasons: Vec<ConflictReason>,
}

/// Compatibility is keyed by IDs and measured thicknesses, never by labels or
/// today's material default. These are derived issues, not structural errors:
/// invalidated placements remain serialized and visible for repair.
pub fn allocation_conflicts(project: &Project) -> Vec<AllocationConflict> {
    project
        .allocations
        .iter()
        .filter_map(|allocation| {
            let board = project
                .boards
                .iter()
                .find(|v| v.id == allocation.board_id)?;
            let stock = project.stock.iter().find(|v| v.id == allocation.stock_id)?;
            let material = project
                .materials
                .iter()
                .find(|v| v.id == board.material_id)?;
            let mut issue = conflict(allocation, board, stock, material);
            let overlaps = project.allocations.iter().any(|other| {
                if other.id == allocation.id || other.stock_id != allocation.stock_id {
                    return false;
                }
                let Some(other_board) = project.boards.iter().find(|b| b.id == other.board_id)
                else {
                    return false;
                };
                let footprint = |part: &Board, turn: bool| {
                    if turn {
                        [part.width, part.length]
                    } else {
                        [part.length, part.width]
                    }
                };
                let a = footprint(board, allocation.quarter_turn);
                let b = footprint(other_board, other.quarter_turn);
                (0..2).all(|axis| {
                    let a0 = i128::from(allocation.origin[axis].micrometres());
                    let b0 = i128::from(other.origin[axis].micrometres());
                    a0 < b0 + i128::from(b[axis].micrometres())
                        && b0 < a0 + i128::from(a[axis].micrometres())
                })
            });
            if overlaps {
                issue
                    .get_or_insert_with(|| AllocationConflict {
                        allocation_id: allocation.id,
                        board_id: board.id,
                        stock_id: stock.id,
                        reasons: Vec::new(),
                    })
                    .reasons
                    .push(ConflictReason::Overlap);
            }
            issue
        })
        .collect()
}

fn conflict(
    allocation: &Allocation,
    board: &Board,
    stock: &Stock,
    material: &Material,
) -> Option<AllocationConflict> {
    let mut reasons = Vec::new();
    if board.material_id != stock.material_id {
        reasons.push(ConflictReason::MaterialIdentity);
    }
    if board.thickness != stock.thickness {
        reasons.push(ConflictReason::EffectiveThickness);
    }
    let grain = board.effective_grain(material);
    if grain != BoardGrain::Unrestricted && stock.grain == StockGrain::Unknown {
        reasons.push(ConflictReason::Grain);
    } else if matches!(stock.grain, StockGrain::AlongX | StockGrain::AlongY) {
        let along_x = (grain == BoardGrain::Length) != allocation.quarter_turn;
        if grain != BoardGrain::Unrestricted && (stock.grain == StockGrain::AlongX) != along_x {
            reasons.push(ConflictReason::Grain);
        }
    }
    let (length, width) = if allocation.quarter_turn {
        (board.width, board.length)
    } else {
        (board.length, board.width)
    };
    let end_x = i128::from(allocation.origin[0].micrometres()) + i128::from(length.micrometres());
    let end_y = i128::from(allocation.origin[1].micrometres()) + i128::from(width.micrometres());
    if end_x > i128::from(stock.length.micrometres() - stock.trim[1].micrometres())
        || end_y > i128::from(stock.width.micrometres() - stock.trim[3].micrometres())
        || allocation.origin[0] < stock.trim[0]
        || allocation.origin[1] < stock.trim[2]
    {
        reasons.push(ConflictReason::OutsideStock);
    }
    (!reasons.is_empty()).then_some(AllocationConflict {
        allocation_id: allocation.id,
        board_id: board.id,
        stock_id: stock.id,
        reasons,
    })
}

#[derive(Clone, Debug)]
pub struct MaterialChangePreview {
    project_id: Uuid,
    revision: u64,
    material_id: Uuid,
    name: String,
    thickness: Length,
    grain: BoardGrain,
    anchor: Anchor,
    /// All boards assigned to this identity, including boards with explicit grain overrides.
    pub affected: Vec<AffectedBoard>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependantChoice {
    Preserve,
    ApplyAll,
    ApplySelected(Vec<Uuid>),
}

fn resized_pose(board: &Board, thickness: Length, anchor: Anchor) -> Result<Pose, UnitError> {
    let pose = board.pose.resized(2, board.thickness, thickness, anchor)?;
    // Bounds on the complete changed blank, including rotated boards.
    for x in [0.0, board.length.micrometres() as f64 / 1000.0] {
        for y in [0.0, board.width.micrometres() as f64 / 1000.0] {
            for z in [0.0, thickness.micrometres() as f64 / 1000.0] {
                pose.transform_point([x, y, z])?;
            }
        }
    }
    Ok(pose)
}

impl ProjectEditor {
    /// Delete a material no board or stock piece uses, with its display color,
    /// as one undo step.
    pub fn delete_material(&mut self, id: Uuid) -> Result<bool, EditError<MaterialDeleteError>> {
        self.transact(|project| {
            let index = project
                .materials
                .iter()
                .position(|m| m.id == id)
                .ok_or(MaterialDeleteError::MissingMaterial(id))?;
            let boards: Vec<_> = project
                .boards
                .iter()
                .filter(|b| b.material_id == id)
                .map(|b| b.id)
                .collect();
            let stock: Vec<_> = project
                .stock
                .iter()
                .filter(|s| s.material_id == id)
                .map(|s| s.id)
                .collect();
            if !boards.is_empty() || !stock.is_empty() {
                return Err(MaterialDeleteError::InUse { boards, stock });
            }
            project.materials.remove(index);
            project.material_colors.remove(&id);
            Ok(())
        })
    }

    /// Change a board's local grain rule in one undoable edit. `None` follows
    /// the material default; the original allocation (including its lock and
    /// quarter-turn) remains a draft if the new rule makes it incompatible.
    pub fn set_board_grain_override(
        &mut self,
        board_id: Uuid,
        grain_override: Option<BoardGrain>,
    ) -> Result<Option<AllocationConflict>, EditError<MaterialChangeError>> {
        self.transact(|project| {
            let board = project
                .boards
                .iter_mut()
                .find(|board| board.id == board_id)
                .ok_or(MaterialChangeError::MissingBoard(board_id))?;
            board.grain_override = grain_override;
            Ok(())
        })?;
        Ok(allocation_conflicts(self.project())
            .into_iter()
            .find(|issue| issue.board_id == board_id))
    }

    /// List dependants and prospective conflicts without mutating the project or history.
    pub fn preview_material_change(
        &self,
        material_id: Uuid,
        name: String,
        thickness: Length,
        grain: BoardGrain,
        anchor: Anchor,
    ) -> Result<MaterialChangePreview, MaterialChangeError> {
        thickness
            .positive()
            .map_err(MaterialChangeError::InvalidThickness)?;
        let project = self.project();
        let material = project
            .materials
            .iter()
            .find(|v| v.id == material_id)
            .ok_or(MaterialChangeError::MissingMaterial(material_id))?;
        let mut affected = Vec::new();
        for board in project
            .boards
            .iter()
            .filter(|v| v.material_id == material_id)
        {
            let pose = resized_pose(board, thickness, anchor);
            let mut proposed = board.clone();
            proposed.thickness = thickness;
            if let Ok(pose) = pose {
                proposed.pose = pose;
            }
            let changed_material = Material {
                name: name.clone(),
                default_thickness: thickness,
                default_grain: grain,
                ..material.clone()
            };
            let allocation_if_applied = project
                .allocations
                .iter()
                .find(|v| v.board_id == board.id)
                .filter(|_| pose.is_ok())
                .and_then(|allocation| {
                    project
                        .stock
                        .iter()
                        .find(|v| v.id == allocation.stock_id)
                        .and_then(|stock| conflict(allocation, &proposed, stock, &changed_material))
                });
            affected.push(AffectedBoard {
                id: board.id,
                name: board.name.clone(),
                thickness_before: board.thickness,
                thickness_if_applied: thickness,
                grain_before: board.effective_grain(material),
                grain_if_applied: proposed.effective_grain(&changed_material),
                apply_error: pose.err(),
                allocation_if_applied,
            });
        }
        Ok(MaterialChangePreview {
            project_id: project.id,
            revision: project.revision,
            material_id,
            name,
            thickness,
            grain,
            anchor,
            affected,
        })
    }

    /// A selection must be drawn from the preview's affected IDs. Unselected
    /// default-followers become explicit overrides of their old effective grain.
    pub fn apply_material_change(
        &mut self,
        preview: MaterialChangePreview,
        choice: DependantChoice,
    ) -> Result<Vec<AllocationConflict>, EditError<MaterialChangeError>> {
        let project = self.project();
        if project.id != preview.project_id || project.revision != preview.revision {
            return Err(EditError::Command(MaterialChangeError::StalePreview));
        }
        let all: HashSet<_> = preview.affected.iter().map(|v| v.id).collect();
        let selected: HashSet<_> = match choice {
            DependantChoice::Preserve => HashSet::new(),
            DependantChoice::ApplyAll => all.clone(),
            DependantChoice::ApplySelected(ids) => {
                let mut seen = HashSet::new();
                for id in ids {
                    if !seen.insert(id) {
                        return Err(EditError::Command(MaterialChangeError::DuplicateSelection(
                            id,
                        )));
                    }
                    if !all.contains(&id) {
                        return Err(EditError::Command(MaterialChangeError::NotDependent(id)));
                    }
                }
                seen
            }
        };
        let original = project
            .materials
            .iter()
            .find(|v| v.id == preview.material_id)
            .ok_or(EditError::Command(MaterialChangeError::MissingMaterial(
                preview.material_id,
            )))?;
        let original_grain = original.default_grain;
        // Preflight even for a partial selection, before modifying any candidate.
        for board in project.boards.iter().filter(|v| selected.contains(&v.id)) {
            resized_pose(board, preview.thickness, preview.anchor).map_err(|reason| {
                EditError::Command(MaterialChangeError::InvalidPose {
                    board_id: board.id,
                    reason,
                })
            })?;
        }
        self.transact(|project| {
            let material = project
                .materials
                .iter_mut()
                .find(|v| v.id == preview.material_id)
                .ok_or(MaterialChangeError::MissingMaterial(preview.material_id))?;
            material.name = preview.name;
            material.default_thickness = preview.thickness;
            material.default_grain = preview.grain;
            for board in project
                .boards
                .iter_mut()
                .filter(|v| v.material_id == preview.material_id)
            {
                if selected.contains(&board.id) {
                    board.pose = resized_pose(board, preview.thickness, preview.anchor).map_err(
                        |reason| MaterialChangeError::InvalidPose {
                            board_id: board.id,
                            reason,
                        },
                    )?;
                    board.thickness = preview.thickness;
                } else if board.grain_override.is_none() && original_grain != preview.grain {
                    board.grain_override = Some(original_grain);
                }
            }
            Ok(())
        })?;
        Ok(allocation_conflicts(self.project()))
    }

    /// Explicit single-board material assignment snapshots the new default.
    /// A separate preview supplies the resulting thickness before confirmation.
    pub fn preview_board_material(
        &self,
        board_id: Uuid,
        material_id: Uuid,
        anchor: Anchor,
    ) -> Result<(Length, Option<AllocationConflict>), MaterialChangeError> {
        let project = self.project();
        let board = project
            .boards
            .iter()
            .find(|v| v.id == board_id)
            .ok_or(MaterialChangeError::MissingBoard(board_id))?;
        let material = project
            .materials
            .iter()
            .find(|v| v.id == material_id)
            .ok_or(MaterialChangeError::MissingMaterial(material_id))?;
        resized_pose(board, material.default_thickness, anchor)
            .map_err(|reason| MaterialChangeError::InvalidPose { board_id, reason })?;
        let mut changed = board.clone();
        changed.material_id = material_id;
        changed.thickness = material.default_thickness;
        let conflict = project
            .allocations
            .iter()
            .find(|v| v.board_id == board_id)
            .and_then(|allocation| {
                project
                    .stock
                    .iter()
                    .find(|v| v.id == allocation.stock_id)
                    .and_then(|stock| conflict(allocation, &changed, stock, material))
            });
        Ok((changed.thickness, conflict))
    }

    pub fn assign_board_material(
        &mut self,
        board_id: Uuid,
        material_id: Uuid,
        anchor: Anchor,
    ) -> Result<Vec<AllocationConflict>, EditError<MaterialChangeError>> {
        self.preview_board_material(board_id, material_id, anchor)
            .map_err(EditError::Command)?;
        self.transact(|project| {
            let material = project
                .materials
                .iter()
                .find(|v| v.id == material_id)
                .ok_or(MaterialChangeError::MissingMaterial(material_id))?;
            let board = project
                .boards
                .iter_mut()
                .find(|v| v.id == board_id)
                .ok_or(MaterialChangeError::MissingBoard(board_id))?;
            board.pose = resized_pose(board, material.default_thickness, anchor)
                .map_err(|reason| MaterialChangeError::InvalidPose { board_id, reason })?;
            board.thickness = material.default_thickness;
            board.material_id = material_id;
            Ok(())
        })?;
        Ok(allocation_conflicts(self.project()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Assembly, StockSource};
    use crate::money::Currency;
    use crate::persistence::{prepare_bytes, serialize};
    use crate::units::Quaternion;

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }

    fn fixture() -> ProjectEditor {
        let mut p = Project::new("Cabinet", Currency::Brl);
        let material = Uuid::new_v4();
        let parent = Uuid::new_v4();
        p.materials.push(Material {
            id: material,
            name: "Plywood".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Length,
        });
        p.assemblies.push(Assembly {
            id: parent,
            name: "Rotated".into(),
            parent_id: None,
            pose: Pose::new(
                [100.0, 200.0, 0.0],
                Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap(),
            )
            .unwrap(),
        });
        for index in 0..3 {
            let board = Uuid::new_v4();
            let stock = Uuid::new_v4();
            p.boards.push(Board {
                id: board,
                name: "Side".into(),
                material_id: material,
                length: mm(100),
                width: mm(50),
                thickness: mm(18),
                grain_override: if index == 2 {
                    Some(BoardGrain::Width)
                } else {
                    None
                },
                parent_id: Some(parent),
                pose: Pose::new([10.0, 20.0, 0.0], Quaternion::IDENTITY).unwrap(),
            });
            p.stock.push(Stock {
                id: stock,
                name: "Plywood".into(),
                material_id: material,
                length: mm(200),
                width: mm(100),
                thickness: mm(18),
                grain: StockGrain::Nondirectional,
                source: StockSource::Owned,
                price: None,
                priority: index,
                trim: [Length::ZERO; 4],
            });
            p.allocations.push(Allocation {
                id: Uuid::new_v4(),
                board_id: board,
                stock_id: stock,
                origin: [Length::ZERO; 2],
                quarter_turn: false,
                locked: true,
            });
        }
        ProjectEditor::new(p).unwrap()
    }

    fn preview(editor: &ProjectEditor) -> MaterialChangePreview {
        editor
            .preview_material_change(
                editor.project().materials[0].id,
                "Renamed".into(),
                mm(15),
                BoardGrain::Width,
                Anchor::Centre,
            )
            .unwrap()
    }

    #[test]
    fn local_width_override_survives_parent_rotation_default_edit_undo_and_save_load() {
        let mut editor = fixture();
        let board_id = editor.project().boards[0].id;
        let allocation = editor.project().allocations[0].clone();
        editor
            .transact(|p| -> Result<(), ()> {
                p.stock[0].grain = StockGrain::AlongY;
                p.assemblies[0].pose.rotation = Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap();
                p.boards[0].pose.rotation = Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap();
                Ok(())
            })
            .unwrap();
        assert_eq!(editor.project().boards[0].grain_override, None);
        let issue = editor
            .set_board_grain_override(board_id, Some(BoardGrain::Width))
            .unwrap();
        assert_eq!(issue, None);
        assert_eq!(editor.project().allocations[0], allocation);
        let rotated_pose = editor.project().boards[0].pose;
        let proposal = editor
            .preview_material_change(
                editor.project().materials[0].id,
                "Plywood".into(),
                mm(18),
                BoardGrain::Unrestricted,
                Anchor::Centre,
            )
            .unwrap();
        editor
            .apply_material_change(proposal, DependantChoice::ApplyAll)
            .unwrap();
        assert_eq!(
            editor.project().boards[0].grain_override,
            Some(BoardGrain::Width)
        );
        assert_eq!(editor.project().boards[0].pose, rotated_pose);
        assert_eq!(
            editor.project().boards[0].effective_grain(&editor.project().materials[0]),
            BoardGrain::Width
        );
        let loaded = prepare_bytes(&serialize(editor.project()).unwrap())
            .unwrap()
            .into_editor();
        assert_eq!(
            loaded.project().boards[0].grain_override,
            Some(BoardGrain::Width)
        );
        assert_eq!(loaded.project().allocations[0], allocation);
        assert!(
            allocation_conflicts(loaded.project())
                .iter()
                .all(|v| v.board_id != board_id)
        );
        editor.undo().unwrap();
        assert_eq!(
            editor.project().boards[0].grain_override,
            Some(BoardGrain::Width)
        );
        editor.undo().unwrap();
        assert_eq!(editor.project().boards[0].grain_override, None);
        editor.redo().unwrap();
        assert_eq!(
            editor.project().boards[0].grain_override,
            Some(BoardGrain::Width)
        );
    }

    #[test]
    fn grain_edits_keep_locked_draft_placement_and_unknown_stock_requires_unrestricted() {
        let mut editor = fixture();
        let id = editor.project().boards[0].id;
        let allocation = editor.project().allocations[0].clone();
        editor
            .transact(|p| -> Result<(), ()> {
                p.stock[0].grain = StockGrain::Unknown;
                p.stock[0].width = mm(200);
                p.allocations[0].quarter_turn = true;
                p.allocations[0].origin = [mm(10), mm(20)];
                Ok(())
            })
            .unwrap();
        let placement = editor.project().allocations[0].clone();
        let issue = editor
            .set_board_grain_override(id, Some(BoardGrain::Width))
            .unwrap()
            .unwrap();
        assert_eq!(issue.reasons, vec![ConflictReason::Grain]);
        assert_eq!(editor.project().allocations[0], placement);
        assert!(placement.locked && allocation.locked);
        let reopened = prepare_bytes(&serialize(editor.project()).unwrap())
            .unwrap()
            .into_editor();
        assert_eq!(allocation_conflicts(reopened.project())[0], issue);
        assert_eq!(
            editor
                .set_board_grain_override(id, Some(BoardGrain::Unrestricted))
                .unwrap(),
            None
        );
        assert_eq!(editor.project().allocations[0], placement);
        editor.undo().unwrap();
        assert_eq!(allocation_conflicts(editor.project())[0], issue);
        editor.set_board_grain_override(id, None).unwrap();
        assert_eq!(
            editor.project().boards[0].effective_grain(&editor.project().materials[0]),
            BoardGrain::Length
        );
        assert_eq!(
            allocation_conflicts(editor.project())[0].reasons,
            vec![ConflictReason::Grain]
        );
        assert_eq!(editor.project().allocations[0], placement);
        let before = editor.project().clone();
        let missing = Uuid::new_v4();
        assert_eq!(
            editor.set_board_grain_override(missing, None),
            Err(EditError::Command(MaterialChangeError::MissingBoard(
                missing
            )))
        );
        assert_eq!(editor.project(), &before);
    }

    #[test]
    fn directional_stock_grain_conflict_is_not_reoriented() {
        let mut editor = fixture();
        let id = editor.project().boards[0].id;
        editor
            .transact(|p| -> Result<(), ()> {
                p.stock[0].grain = StockGrain::AlongX;
                Ok(())
            })
            .unwrap();
        let placement = editor.project().allocations[0].clone();
        let issue = editor
            .set_board_grain_override(id, Some(BoardGrain::Width))
            .unwrap()
            .unwrap();
        assert_eq!(issue.reasons, vec![ConflictReason::Grain]);
        assert_eq!(editor.project().allocations[0], placement);
        assert_eq!(editor.set_board_grain_override(id, None).unwrap(), None);
        assert_eq!(editor.project().allocations[0], placement);
    }

    #[test]
    fn following_default_switches_only_with_explicit_dependant_update() {
        let mut editor = fixture();
        let material_id = editor.project().materials[0].id;
        editor
            .transact(|p| -> Result<(), ()> {
                p.stock[0].grain = StockGrain::AlongX;
                Ok(())
            })
            .unwrap();
        let placement = editor.project().allocations[0].clone();
        let change = |editor: &ProjectEditor| {
            editor
                .preview_material_change(
                    material_id,
                    "Plywood".into(),
                    mm(18),
                    BoardGrain::Width,
                    Anchor::Centre,
                )
                .unwrap()
        };
        editor
            .apply_material_change(change(&editor), DependantChoice::Preserve)
            .unwrap();
        assert_eq!(
            editor.project().boards[0].grain_override,
            Some(BoardGrain::Length)
        );
        assert!(allocation_conflicts(editor.project()).is_empty());
        editor.undo().unwrap();
        assert_eq!(editor.project().boards[0].grain_override, None);
        let conflicts = editor
            .apply_material_change(change(&editor), DependantChoice::ApplyAll)
            .unwrap();
        assert!(conflicts.iter().any(|v| {
            v.board_id == placement.board_id && v.reasons == vec![ConflictReason::Grain]
        }));
        assert_eq!(editor.project().boards[0].grain_override, None);
        assert_eq!(editor.project().allocations[0], placement);
        editor.undo().unwrap();
        assert!(allocation_conflicts(editor.project()).is_empty());
    }

    #[test]
    fn preserve_all_keeps_effective_properties_and_allocations_through_save_load() {
        let mut editor = fixture();
        let original = editor.project().clone();
        let proposal = preview(&editor);
        assert_eq!(proposal.affected.len(), 3);
        assert!(proposal.affected.iter().all(|v| {
            v.allocation_if_applied
                .as_ref()
                .unwrap()
                .reasons
                .contains(&ConflictReason::EffectiveThickness)
        }));
        assert!(!editor.can_undo());
        assert!(
            editor
                .apply_material_change(proposal, DependantChoice::Preserve)
                .unwrap()
                .is_empty()
        );
        assert_eq!(editor.project().materials[0].default_thickness, mm(15));
        assert_eq!(editor.project().stock, original.stock);
        assert_eq!(editor.project().allocations, original.allocations);
        for board in &editor.project().boards {
            assert_eq!(board.thickness, mm(18));
            assert_eq!(
                board.effective_grain(&editor.project().materials[0]),
                original
                    .boards
                    .iter()
                    .find(|v| v.id == board.id)
                    .unwrap()
                    .effective_grain(&original.materials[0])
            );
        }
        assert_eq!(
            editor.project().boards[0].grain_override,
            Some(BoardGrain::Length)
        );
        let loaded = prepare_bytes(&serialize(editor.project()).unwrap())
            .unwrap()
            .into_editor();
        assert_eq!(loaded.project(), editor.project());
        assert!(allocation_conflicts(loaded.project()).is_empty());
        editor.undo().unwrap();
        let mut restored = editor.project().clone();
        restored.revision = original.revision;
        assert_eq!(restored, original);
    }

    #[test]
    fn all_and_selected_change_only_confirmed_boards_and_preserve_stock() {
        for choice in [
            DependantChoice::ApplyAll,
            DependantChoice::ApplySelected(vec![]),
        ] {
            let mut editor = fixture();
            let ids: Vec<_> = editor.project().boards.iter().map(|v| v.id).collect();
            let choice = match choice {
                DependantChoice::ApplySelected(_) => {
                    DependantChoice::ApplySelected(vec![ids[0], ids[2]])
                }
                other => other,
            };
            let before = editor.project().clone();
            let conflicts = editor
                .apply_material_change(preview(&editor), choice.clone())
                .unwrap();
            let count = if matches!(choice, DependantChoice::ApplyAll) {
                3
            } else {
                2
            };
            assert_eq!(conflicts.len(), count);
            for (index, board) in editor.project().boards.iter().enumerate() {
                let applied = count == 3 || index != 1;
                assert_eq!(board.thickness, if applied { mm(15) } else { mm(18) });
                assert_eq!(
                    board.pose.translation_mm[2],
                    if applied { 1.5 } else { 0.0 }
                );
                if !applied {
                    assert_eq!(board.grain_override, Some(BoardGrain::Length));
                }
            }
            assert_eq!(editor.project().stock, before.stock);
            assert_eq!(editor.project().allocations, before.allocations);
            let loaded = prepare_bytes(&serialize(editor.project()).unwrap())
                .unwrap()
                .into_editor();
            assert_eq!(allocation_conflicts(loaded.project()), conflicts);
            editor.undo().unwrap();
            assert!(allocation_conflicts(editor.project()).is_empty());
        }
    }

    #[test]
    fn identity_and_effective_thickness_not_names_or_default() {
        let mut editor = fixture();
        editor
            .apply_material_change(preview(&editor), DependantChoice::Preserve)
            .unwrap();
        assert!(allocation_conflicts(editor.project()).is_empty());
        editor
            .transact(|p| -> Result<(), ()> {
                p.stock[0].name = "Different label".into();
                p.materials.push(Material {
                    id: Uuid::new_v4(),
                    name: p.materials[0].name.clone(),
                    default_thickness: mm(18),
                    default_grain: BoardGrain::Length,
                });
                p.stock[1].material_id = p.materials[1].id;
                Ok(())
            })
            .unwrap();
        let conflicts = allocation_conflicts(editor.project());
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].reasons, vec![ConflictReason::MaterialIdentity]);
        assert_eq!(conflicts[0].board_id, editor.project().boards[1].id);
        // A direct board-only edit retains the physical stock and its placement.
        let id = editor.project().boards[0].id;
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].thickness = mm(15);
                Ok(())
            })
            .unwrap();
        assert_eq!(editor.project().allocations[0].board_id, id);
        assert!(
            allocation_conflicts(editor.project()).iter().any(
                |v| v.board_id == id && v.reasons.contains(&ConflictReason::EffectiveThickness)
            )
        );
    }

    #[test]
    fn stale_invalid_or_wrong_selections_are_atomic() {
        let mut editor = fixture();
        let before = editor.project().clone();
        let invalid = editor.preview_material_change(
            before.materials[0].id,
            "X".into(),
            Length::ZERO,
            BoardGrain::Length,
            Anchor::End,
        );
        assert!(matches!(
            invalid,
            Err(MaterialChangeError::InvalidThickness(_))
        ));
        let proposal = preview(&editor);
        let id = before.boards[0].id;
        assert_eq!(
            editor.apply_material_change(
                proposal.clone(),
                DependantChoice::ApplySelected(vec![id, id])
            ),
            Err(EditError::Command(MaterialChangeError::DuplicateSelection(
                id
            )))
        );
        assert!(matches!(
            editor.apply_material_change(
                proposal.clone(),
                DependantChoice::ApplySelected(vec![Uuid::new_v4()])
            ),
            Err(EditError::Command(MaterialChangeError::NotDependent(_)))
        ));
        assert_eq!(editor.project(), &before);
        assert!(!editor.can_undo());
        editor
            .apply_material_change(proposal.clone(), DependantChoice::Preserve)
            .unwrap();
        let changed = editor.project().clone();
        assert_eq!(
            editor.apply_material_change(proposal, DependantChoice::ApplyAll),
            Err(EditError::Command(MaterialChangeError::StalePreview))
        );
        assert_eq!(editor.project(), &changed);
    }

    #[test]
    fn assignment_previews_thickness_and_retains_incompatible_allocation() {
        let mut editor = fixture();
        let other = Uuid::new_v4();
        editor
            .transact(|p| -> Result<(), ()> {
                p.materials.push(Material {
                    id: other,
                    name: "Plywood".into(),
                    default_thickness: mm(15),
                    default_grain: BoardGrain::Length,
                });
                Ok(())
            })
            .unwrap();
        let id = editor.project().boards[0].id;
        let (thickness, issue) = editor
            .preview_board_material(id, other, Anchor::End)
            .unwrap();
        assert_eq!(thickness, mm(15));
        assert_eq!(
            issue.unwrap().reasons,
            vec![
                ConflictReason::MaterialIdentity,
                ConflictReason::EffectiveThickness
            ]
        );
        let original_allocation = editor.project().allocations[0].clone();
        editor
            .assign_board_material(id, other, Anchor::End)
            .unwrap();
        assert_eq!(editor.project().boards[0].pose.translation_mm[2], 3.0);
        assert_eq!(editor.project().allocations[0], original_allocation);
    }

    #[test]
    fn assignment_cancel_and_confirm_survive_round_trip_and_undo() {
        let mut editor = fixture();
        let other = Uuid::new_v4();
        editor
            .transact(|p| -> Result<(), ()> {
                p.materials.push(Material {
                    id: other,
                    name: "Plywood".into(),
                    default_thickness: mm(15),
                    default_grain: BoardGrain::Length,
                });
                Ok(())
            })
            .unwrap();
        let before = editor.project().clone();
        let id = before.boards[1].id;
        let history_before = editor.can_undo();
        let (thickness, warning) = editor
            .preview_board_material(id, other, Anchor::Centre)
            .unwrap();
        assert_eq!(thickness, mm(15));
        assert_eq!(warning.unwrap().board_id, id);
        // Closing the preview performs no transaction.
        assert_eq!(editor.project(), &before);
        assert_eq!(editor.can_undo(), history_before);
        let conflicts = editor
            .assign_board_material(id, other, Anchor::Centre)
            .unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].board_id, id);
        assert_eq!(editor.project().boards[0], before.boards[0]);
        assert_eq!(editor.project().stock, before.stock);
        assert_eq!(editor.project().allocations, before.allocations);
        let loaded = prepare_bytes(&serialize(editor.project()).unwrap())
            .unwrap()
            .into_editor();
        assert_eq!(allocation_conflicts(loaded.project()), conflicts);
        editor.undo().unwrap();
        assert_eq!(editor.project().boards, before.boards);
        assert!(allocation_conflicts(editor.project()).is_empty());
    }

    #[test]
    fn end_anchor_preserves_world_face_inside_rotated_parent() {
        let mut editor = fixture();
        let original = editor.project().clone();
        let parent = original.assemblies[0].pose;
        let board = &original.boards[0];
        let old_face = parent
            .compose(board.pose)
            .unwrap()
            .transform_point([0.0, 0.0, 18.0])
            .unwrap();
        let proposal = editor
            .preview_material_change(
                original.materials[0].id,
                "Plywood".into(),
                mm(15),
                BoardGrain::Length,
                Anchor::End,
            )
            .unwrap();
        editor
            .apply_material_change(proposal, DependantChoice::ApplySelected(vec![board.id]))
            .unwrap();
        let resized = &editor.project().boards[0];
        assert_eq!(resized.pose.translation_mm, [10.0, 20.0, 3.0]);
        let new_face = parent
            .compose(resized.pose)
            .unwrap()
            .transform_point([0.0, 0.0, 15.0])
            .unwrap();
        for axis in 0..3 {
            assert!((new_face[axis] - old_face[axis]).abs() < 1e-6);
        }
        assert_eq!(resized.pose.rotation, board.pose.rotation);
        assert_eq!((resized.length, resized.width), (board.length, board.width));
    }

    #[test]
    fn out_of_bounds_dependant_preflight_does_not_partially_commit() {
        let mut editor = fixture();
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[2].pose.translation_mm[2] = 999_980.0;
                Ok(())
            })
            .unwrap();
        let before = editor.project().clone();
        let proposal = editor
            .preview_material_change(
                before.materials[0].id,
                "Too thick".into(),
                mm(30),
                BoardGrain::Length,
                Anchor::Start,
            )
            .unwrap();
        assert_eq!(
            proposal.affected[2].apply_error,
            Some(UnitError::OutOfBounds)
        );
        assert_eq!(proposal.affected[0].apply_error, None);
        assert_eq!(
            editor.apply_material_change(proposal.clone(), DependantChoice::ApplyAll),
            Err(EditError::Command(MaterialChangeError::InvalidPose {
                board_id: before.boards[2].id,
                reason: UnitError::OutOfBounds
            }))
        );
        assert_eq!(editor.project(), &before);
        assert_eq!(
            editor.apply_material_change(
                proposal.clone(),
                DependantChoice::ApplySelected(vec![before.boards[2].id])
            ),
            Err(EditError::Command(MaterialChangeError::InvalidPose {
                board_id: before.boards[2].id,
                reason: UnitError::OutOfBounds
            }))
        );
        assert_eq!(editor.project(), &before);
        editor
            .apply_material_change(proposal, DependantChoice::Preserve)
            .unwrap();
        assert_eq!(editor.project().materials[0].default_thickness, mm(30));
        assert_eq!(editor.project().boards, before.boards);
        editor.undo().unwrap();
        assert_eq!(editor.project().materials[0].default_thickness, mm(18));

        let proposal = editor
            .preview_material_change(
                before.materials[0].id,
                "Selected".into(),
                mm(30),
                BoardGrain::Length,
                Anchor::Start,
            )
            .unwrap();
        editor
            .apply_material_change(
                proposal,
                DependantChoice::ApplySelected(vec![before.boards[0].id]),
            )
            .unwrap();
        assert_eq!(editor.project().boards[0].thickness, mm(30));
        assert_eq!(editor.project().boards[2].thickness, mm(18));
    }
}
