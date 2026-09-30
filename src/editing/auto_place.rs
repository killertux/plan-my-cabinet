//! Automatic sheet placement as single undoable edits.
use std::convert::Infallible;

use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::Project;
use crate::sheet_packer::{
    PackMode, SuggestedSheet, Unplaced, pack, suggest_sheets, suggested_stock,
};
use crate::units::Length;

/// Sheets `add_needed_sheets` added for one material and thickness.
#[derive(Clone, Debug, PartialEq)]
pub struct AddedSheets {
    pub material_id: Uuid,
    pub thickness: Length,
    pub sheet: SuggestedSheet,
}

/// Add, as To purchase, the sheets every material with waiting boards needs
/// (when a sheet size is known), then place the waiting boards around the
/// existing placements. Works on a bare project; callers wrap it in an edit.
pub fn add_needed_sheets(project: &mut Project) -> Vec<AddedSheets> {
    let mut added = Vec::new();
    for suggestion in suggest_sheets(project) {
        let Some(sheet) = suggestion.sheet.filter(|sheet| sheet.count > 0) else {
            continue;
        };
        let group = (suggestion.material_id, suggestion.thickness);
        let pieces = suggested_stock(project, group, &sheet);
        project.stock.extend(pieces);
        added.push(AddedSheets {
            material_id: suggestion.material_id,
            thickness: suggestion.thickness,
            sheet,
        });
    }
    if !added.is_empty() {
        project.allocations = pack(project, PackMode::FillGaps).allocations;
    }
    added
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceSummary {
    /// Boards placed or moved.
    pub placed: usize,
    pub sheets_added: usize,
    /// Boards still waiting, with the reason.
    pub unplaced: Vec<(Uuid, Unplaced)>,
}

impl ProjectEditor {
    /// Pack boards onto the declared stock and commit the result as one edit.
    /// `FillGaps` never moves a placed board; `Replan` never moves a locked one.
    pub fn auto_place(&mut self, mode: PackMode) -> Result<PlaceSummary, EditError<Infallible>> {
        let result = pack(self.project(), mode);
        self.transact(|project| {
            project.allocations = result.allocations.clone();
            Ok(())
        })?;
        Ok(PlaceSummary {
            placed: result.changed.len(),
            sheets_added: 0,
            unplaced: result.unplaced,
        })
    }

    /// Add `sheet.count` pieces To purchase for one material and thickness,
    /// then place waiting boards around the existing placements. One edit.
    pub fn add_sheets_and_place(
        &mut self,
        material_id: Uuid,
        thickness: Length,
        sheet: &SuggestedSheet,
    ) -> Result<PlaceSummary, EditError<Infallible>> {
        let mut staged = self.project().clone();
        let added = suggested_stock(&staged, (material_id, thickness), sheet);
        staged.stock.extend(added.iter().cloned());
        let result = pack(&staged, PackMode::FillGaps);
        self.transact(|project| {
            project.stock.extend(added.iter().cloned());
            project.allocations = result.allocations.clone();
            Ok(())
        })?;
        Ok(PlaceSummary {
            placed: result.changed.len(),
            sheets_added: added.len(),
            unplaced: result.unplaced,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allocation_diagnostics::{Status, diagnose};
    use crate::domain::{Board, BoardGrain, Material, Project};
    use crate::money::Currency;
    use crate::sheet_packer::suggest_sheets;
    use crate::units::{Pose, Quaternion};

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }

    fn editor() -> ProjectEditor {
        let mut project = Project::new("auto", Currency::Brl);
        let material = Uuid::new_v4();
        project.materials.push(Material {
            default_band: None,
            kind: Default::default(),
            id: material,
            name: "White MDF".into(),
            default_thickness: mm(15),
            default_grain: BoardGrain::Unrestricted,
        });
        for index in 0..5 {
            project.boards.push(Board {
                banding: Default::default(),
                id: Uuid::new_v4(),
                name: format!("b{index}"),
                material_id: material,
                length: mm(1300),
                width: mm(900),
                thickness: mm(15),
                grain_override: None,
                parent_id: None,
                pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
            });
        }
        ProjectEditor::new(project).unwrap()
    }

    #[test]
    fn suggested_sheets_are_added_and_filled_in_one_undo() {
        let mut editor = editor();
        let suggestion = suggest_sheets(editor.project()).remove(0);
        let sheet = suggestion.sheet.expect("preset size for White MDF 15 mm");
        // Four 1300 × 900 parts fit on a 2750 × 1840 sheet with a 5 mm kerf.
        assert_eq!(sheet.count, 2);
        let summary = editor
            .add_sheets_and_place(suggestion.material_id, suggestion.thickness, &sheet)
            .unwrap();
        assert_eq!((summary.placed, summary.sheets_added), (5, 2));
        assert!(summary.unplaced.is_empty());
        assert!(
            diagnose(editor.project())
                .iter()
                .all(|d| d.status == Status::AllocatedValid)
        );
        assert_eq!(editor.project().stock_aliases.len(), 2);
        editor.undo().unwrap();
        assert!(editor.project().stock.is_empty());
        assert!(editor.project().allocations.is_empty());
    }

    #[test]
    fn fill_gaps_is_a_no_op_when_everything_is_placed() {
        let mut editor = editor();
        let suggestion = suggest_sheets(editor.project()).remove(0);
        let sheet = suggestion.sheet.unwrap();
        editor
            .add_sheets_and_place(suggestion.material_id, suggestion.thickness, &sheet)
            .unwrap();
        let revision = editor.project().revision;
        let summary = editor.auto_place(PackMode::FillGaps).unwrap();
        assert_eq!(summary.placed, 0);
        assert_eq!(editor.project().revision, revision);
    }
}
