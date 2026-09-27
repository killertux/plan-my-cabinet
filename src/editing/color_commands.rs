//! Undoable material appearance edits, separate from physical material changes.
use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::SrgbColor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorEditError {
    MissingMaterial(Uuid),
}

impl ProjectEditor {
    /// `None` removes the custom color and restores the neutral fallback.
    /// No-op choices do not advance revision or history.
    pub fn set_material_color(
        &mut self,
        material_id: Uuid,
        color: Option<SrgbColor>,
    ) -> Result<bool, EditError<ColorEditError>> {
        self.transact(|project| {
            if !project
                .materials
                .iter()
                .any(|material| material.id == material_id)
            {
                return Err(ColorEditError::MissingMaterial(material_id));
            }
            match color {
                Some(color) => {
                    project.material_colors.insert(material_id, color);
                }
                None => {
                    project.material_colors.remove(&material_id);
                }
            }
            Ok(())
        })
    }
}
