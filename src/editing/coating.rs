//! Coating edits: a material's coating, and which face of a one-side coated
//! board is coated. Every command is one editor transaction, so one undo step.
use uuid::Uuid;

use crate::coating_rules::{self, Coated};
use crate::commands::{EditError, ProjectEditor};
use crate::domain::{BoardFace, CoatedFace, Coating};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoatingError {
    MissingBoard(Uuid),
    MissingMaterial(Uuid),
    /// None of the boards is on a material coated on one side only.
    NotOneSided,
}

/// What a coated-face edit did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoatingOutcome {
    pub changed: Vec<Uuid>,
    /// Boards left alone because their material is not coated on one side.
    pub skipped: Vec<Uuid>,
}

/// How to change the coated face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoatedFaceEdit {
    /// Back to the automatic rule.
    Auto,
    Face(BoardFace),
    /// The other face from the one coated now.
    Flip,
}

impl ProjectEditor {
    pub fn set_material_coating(
        &mut self,
        material: Uuid,
        coating: Coating,
    ) -> Result<bool, EditError<CoatingError>> {
        self.transact(|project| {
            let slot = project
                .materials
                .iter_mut()
                .find(|m| m.id == material)
                .ok_or(CoatingError::MissingMaterial(material))?;
            slot.coating = coating;
            Ok(())
        })
    }

    /// Change the coated face of every one-side coated board in `boards`;
    /// the others are skipped.
    pub fn set_coated_face(
        &mut self,
        boards: &[Uuid],
        edit: CoatedFaceEdit,
    ) -> Result<CoatingOutcome, EditError<CoatingError>> {
        let mut outcome = CoatingOutcome::default();
        self.transact(|project| {
            let mut writes = Vec::new();
            for &id in boards {
                let state = coating_rules::board_state(project, id)
                    .ok_or(CoatingError::MissingBoard(id))?;
                if !state.choosable {
                    outcome.skipped.push(id);
                    continue;
                }
                let value = match edit {
                    CoatedFaceEdit::Auto => CoatedFace::Auto,
                    CoatedFaceEdit::Face(BoardFace::MinZ) => CoatedFace::MinZ,
                    CoatedFaceEdit::Face(BoardFace::MaxZ) => CoatedFace::MaxZ,
                    CoatedFaceEdit::Flip => match state.coated {
                        Coated::One(BoardFace::MaxZ) => CoatedFace::MinZ,
                        _ => CoatedFace::MaxZ,
                    },
                };
                writes.push((id, value));
            }
            if writes.is_empty() && !boards.is_empty() {
                return Err(CoatingError::NotOneSided);
            }
            for (id, value) in writes {
                project.board_mut(id).expect("checked").coated_face = value;
                outcome.changed.push(id);
            }
            Ok(())
        })?;
        Ok(outcome)
    }
}
