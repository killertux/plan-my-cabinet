//! Material and independent-board creation through validated, undoable transactions.
use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::{Board, BoardGrain, Material};
use crate::first_fit::{FirstFit, allocate_new_board};
use crate::units::{Length, Pose, UnitError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardField {
    Material,
    Thickness,
    Length,
    Width,
    Pose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationError {
    MissingMaterial(Uuid),
    InvalidField(BoardField, UnitError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameError {
    EmptyName,
    /// Project names are limited to 256 bytes, as in the template setup.
    TooLong,
    MissingObject(Uuid),
}

/// A project name as stored: trimmed, non-empty and at most 256 bytes.
pub fn project_name(name: &str) -> Result<&str, RenameError> {
    match name.trim() {
        "" => Err(RenameError::EmptyName),
        name if name.len() > 256 => Err(RenameError::TooLong),
        name => Ok(name),
    }
}

pub struct NewMaterial {
    pub name: String,
    pub thickness: Length,
    pub grain: BoardGrain,
}

pub struct NewBoard {
    pub name: String,
    pub material_id: Uuid,
    pub length: Length,
    pub width: Length,
    pub pose: Pose,
}

impl ProjectEditor {
    pub fn create_material(
        &mut self,
        input: NewMaterial,
    ) -> Result<Uuid, EditError<CreationError>> {
        input.thickness.positive().map_err(|e| {
            EditError::Command(CreationError::InvalidField(BoardField::Thickness, e))
        })?;
        let id = Uuid::new_v4();
        self.transact(|project| {
            project.materials.push(Material {
                id,
                name: input.name,
                default_thickness: input.thickness,
                default_grain: input.grain,
            });
            Ok(())
        })?;
        Ok(id)
    }

    pub fn create_board(&mut self, input: NewBoard) -> Result<Uuid, EditError<CreationError>> {
        self.create_board_with_fit(input).map(|(id, _)| id)
    }

    pub fn create_board_with_fit(
        &mut self,
        input: NewBoard,
    ) -> Result<(Uuid, FirstFit), EditError<CreationError>> {
        input
            .length
            .positive()
            .map_err(|e| EditError::Command(CreationError::InvalidField(BoardField::Length, e)))?;
        input
            .width
            .positive()
            .map_err(|e| EditError::Command(CreationError::InvalidField(BoardField::Width, e)))?;
        let material = self
            .project()
            .materials
            .iter()
            .find(|material| material.id == input.material_id)
            .ok_or(EditError::Command(CreationError::MissingMaterial(
                input.material_id,
            )))?;
        let thickness = material.default_thickness.positive().map_err(|e| {
            EditError::Command(CreationError::InvalidField(BoardField::Thickness, e))
        })?;
        // Validate the supplied rigid pose and all eight extremities, not just its origin.
        let pose = Pose::new(input.pose.translation_mm, input.pose.rotation)
            .map_err(|e| EditError::Command(CreationError::InvalidField(BoardField::Pose, e)))?;
        let q = input.pose.rotation;
        let norm = q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z;
        if !norm.is_finite()
            || (norm - 1.0).abs() > 1e-12
            || [q.w, q.x, q.y, q.z]
                .iter()
                .zip([
                    pose.rotation.w,
                    pose.rotation.x,
                    pose.rotation.y,
                    pose.rotation.z,
                ])
                .any(|(a, b)| (a - b).abs() > 1e-12)
        {
            return Err(EditError::Command(CreationError::InvalidField(
                BoardField::Pose,
                UnitError::InvalidRotation,
            )));
        }
        let extents =
            [input.length, input.width, thickness].map(|v| v.micrometres() as f64 / 1000.0);
        for x in [0.0, extents[0]] {
            for y in [0.0, extents[1]] {
                for z in [0.0, extents[2]] {
                    pose.transform_point([x, y, z]).map_err(|e| {
                        EditError::Command(CreationError::InvalidField(BoardField::Pose, e))
                    })?;
                }
            }
        }
        let id = Uuid::new_v4();
        let mut fit = FirstFit::NoFit;
        self.transact(|project| {
            project.boards.push(Board {
                id,
                name: input.name,
                material_id: input.material_id,
                length: input.length,
                width: input.width,
                thickness,
                grain_override: None,
                parent_id: None,
                pose,
            });
            fit = allocate_new_board(project, id);
            Ok(())
        })?;
        Ok((id, fit))
    }

    /// Rename the project, as one undo step.
    pub fn rename_project(&mut self, name: &str) -> Result<bool, EditError<RenameError>> {
        let name = project_name(name).map_err(EditError::Command)?;
        self.transact(|project| {
            name.clone_into(&mut project.name);
            Ok(())
        })
    }

    /// Rename a board, assembly or hardware item. Surrounding whitespace is
    /// dropped; an unchanged name records no undo step.
    pub fn rename_object(&mut self, id: Uuid, name: &str) -> Result<bool, EditError<RenameError>> {
        let name = name.trim();
        if name.is_empty() {
            return Err(EditError::Command(RenameError::EmptyName));
        }
        self.transact(|project| {
            let slot = if let Some(board) = project.boards.iter_mut().find(|b| b.id == id) {
                &mut board.name
            } else if let Some(assembly) = project.assemblies.iter_mut().find(|a| a.id == id) {
                &mut assembly.name
            } else if let Some(item) = project.hardware.iter_mut().find(|h| h.id == id) {
                &mut item.name
            } else {
                return Err(RenameError::MissingObject(id));
            };
            name.clone_into(slot);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Project;
    use crate::money::Currency;
    use crate::units::Quaternion;

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }
    fn board(material_id: Uuid) -> NewBoard {
        NewBoard {
            name: "Side".into(),
            material_id,
            length: mm(2300),
            width: mm(600),
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        }
    }

    #[test]
    fn independent_boards_snapshot_thickness_and_undo_atomically() {
        let mut editor = ProjectEditor::new(Project::new("Cabinet", Currency::Brl)).unwrap();
        let material = editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: mm(18),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let first = editor.create_board(board(material)).unwrap();
        let second = editor.create_board(board(material)).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            editor.project().boards[0].blank_dimensions(),
            [mm(2300), mm(600), mm(18)]
        );
        assert_eq!(
            editor.project().boards[0].effective_grain(&editor.project().materials[0]),
            BoardGrain::Length
        );
        assert!(editor.project().allocations.is_empty());
        editor.undo().unwrap();
        assert_eq!(editor.project().boards.len(), 1);
        editor.redo().unwrap();
        assert_eq!(editor.project().boards[1].id, second);
        editor.project().validate().unwrap();
    }

    #[test]
    fn renaming_a_copy_leaves_the_original_and_undoes() {
        let mut editor = ProjectEditor::new(Project::new("Cabinet", Currency::Brl)).unwrap();
        let material = editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: mm(18),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let original = editor.create_board(board(material)).unwrap();
        let pose = editor.project().board(original).unwrap().pose;
        let copy = editor.duplicate_board(original, pose).unwrap();
        assert!(editor.rename_object(copy, "  Right side ").unwrap());
        assert_eq!(editor.project().board(copy).unwrap().name, "Right side");
        assert_eq!(editor.project().board(original).unwrap().name, "Side");
        assert!(!editor.rename_object(copy, "Right side").unwrap());
        editor.undo().unwrap();
        assert_eq!(editor.project().board(copy).unwrap().name, "Side");
        let before = editor.project().clone();
        assert!(matches!(
            editor.rename_object(copy, "   "),
            Err(EditError::Command(RenameError::EmptyName))
        ));
        assert!(matches!(
            editor.rename_object(Uuid::new_v4(), "Top"),
            Err(EditError::Command(RenameError::MissingObject(_)))
        ));
        assert_eq!(editor.project(), &before);
    }

    #[test]
    fn project_rename_trims_rejects_blank_or_long_and_undoes() {
        let mut editor = ProjectEditor::new(Project::new("Cabinet", Currency::Brl)).unwrap();
        assert!(editor.rename_project("  Kitchen  ").unwrap());
        assert_eq!(editor.project().name, "Kitchen");
        assert!(!editor.rename_project("Kitchen").unwrap());
        for bad in ["   ", &"x".repeat(257)] {
            assert!(editor.rename_project(bad).is_err());
        }
        assert_eq!(editor.project().name, "Kitchen");
        editor.undo().unwrap();
        assert_eq!(editor.project().name, "Cabinet");
    }

    #[test]
    fn invalid_fields_never_change_project_or_history() {
        let mut editor = ProjectEditor::new(Project::new("Cabinet", Currency::Brl)).unwrap();
        assert!(matches!(
            editor.create_material(NewMaterial {
                name: "Bad".into(),
                thickness: Length::ZERO,
                grain: BoardGrain::Length
            }),
            Err(EditError::Command(CreationError::InvalidField(
                BoardField::Thickness,
                _
            )))
        ));
        let material = editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: mm(18),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let before = editor.project().clone();
        let mut input = board(material);
        input.width = Length::ZERO;
        assert!(matches!(
            editor.create_board(input),
            Err(EditError::Command(CreationError::InvalidField(
                BoardField::Width,
                _
            )))
        ));
        let mut input = board(Uuid::new_v4());
        assert!(matches!(
            editor.create_board(input),
            Err(EditError::Command(CreationError::MissingMaterial(_)))
        ));
        input = board(material);
        input.pose.translation_mm[0] = f64::NAN;
        assert!(matches!(
            editor.create_board(input),
            Err(EditError::Command(CreationError::InvalidField(
                BoardField::Pose,
                UnitError::NonFinite
            )))
        ));
        let mut input = board(material);
        input.pose.translation_mm[0] = 999_000.0;
        assert!(matches!(
            editor.create_board(input),
            Err(EditError::Command(CreationError::InvalidField(
                BoardField::Pose,
                UnitError::OutOfBounds
            )))
        ));
        assert_eq!(editor.project(), &before);
        assert!(editor.project().boards.is_empty());
        assert!(editor.project().allocations.is_empty());
    }
}
