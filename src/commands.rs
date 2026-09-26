//! Atomic, headless project edits and session-local history.
//!
//! All committed state is private to this editor. Preview copies are disposable;
//! only a validated transaction can replace the authoritative project.

use crate::domain::{DomainError, Project, validate_grid_spacing};
use crate::export::{ExportError, ExportRecord, ExportSettings, ExportSnapshot, ExportStatus};
use crate::money::{Money, MoneyError};
use crate::units::{Length, UnitError};

#[derive(Debug, PartialEq, Eq)]
pub enum EditError<E> {
    Command(E),
    InvalidProject(DomainError),
    ProjectIdentityChanged,
    RevisionExhausted,
    NoPreview,
}

pub struct ProjectEditor {
    project: Project,
    undo: Vec<Project>,
    redo: Vec<Project>,
    preview: Option<Project>,
    saved: Option<Project>,
}

impl ProjectEditor {
    /// A loaded document must be validated before it becomes the active project.
    pub fn new(project: Project) -> Result<Self, DomainError> {
        project.validate()?;
        Ok(Self {
            saved: Some(project.clone()),
            project,
            undo: Vec::new(),
            redo: Vec::new(),
            preview: None,
        })
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Snapshots always use committed data, never an in-progress preview.
    pub fn export_snapshot(&self, settings: ExportSettings) -> Result<ExportSnapshot, DomainError> {
        ExportSnapshot::new(&self.project, settings)
    }

    pub fn last_export_status(&self) -> ExportStatus {
        ExportStatus::for_project(&self.project)
    }

    /// Called by the output writer only after the destination is fully committed.
    /// The receipt is checked against the on-disk file and the original snapshot.
    pub fn record_completed_export(
        &mut self,
        snapshot: &ExportSnapshot,
        path: &std::path::Path,
        completed_unix_ms: u64,
        file_sha256: &str,
    ) -> Result<(), ExportError> {
        let record =
            ExportRecord::verify_completed(snapshot, path, completed_unix_ms, file_sha256)?;
        if record.project_id != self.project.id {
            return Err(ExportError::WrongProject);
        }
        self.project.export_records.push(record);
        Ok(())
    }

    /// Change only the project-local editing grid; existing poses remain untouched.
    /// Callers convert text to an exact or explicitly confirmed `Length` first.
    pub fn set_grid_spacing(&mut self, spacing: Length) -> Result<bool, EditError<UnitError>> {
        self.transact(|project| {
            validate_grid_spacing(spacing)?;
            project.grid_spacing = spacing;
            Ok(())
        })
    }

    /// Set the measured blade width. This changes the manufacturing assumption;
    /// existing placement records remain visible even if their witnesses become invalid.
    pub fn set_cutting_kerf(&mut self, kerf: Length) -> Result<bool, EditError<UnitError>> {
        kerf.positive().map_err(EditError::Command)?;
        self.transact(|project| {
            if project.cutting_kerf != kerf {
                project.confirmed_shop_kerf = None;
            }
            project.cutting_kerf = kerf;
            Ok(())
        })
    }

    /// Acknowledges the currently configured shop kerf, not the feasibility of any layout.
    pub fn confirm_shop_kerf(&mut self) -> Result<bool, EditError<UnitError>> {
        self.transact(|project| -> Result<(), UnitError> {
            project.confirmed_shop_kerf = Some(project.cutting_kerf);
            Ok(())
        })
    }

    /// Set a flat per-physical-pass fee; `None` means the shop rate is unknown.
    pub fn set_cut_fee(&mut self, fee: Option<Money>) -> Result<bool, EditError<MoneyError>> {
        self.transact(|project| {
            if fee.is_some_and(|value| value.currency() != project.currency) {
                return Err(MoneyError::CurrencyMismatch);
            }
            if fee.is_some_and(|value| value.minor_units() < 0) {
                return Err(MoneyError::NegativeAmount);
            }
            project.cut_fee = fee;
            Ok(())
        })
    }

    pub fn preview(&self) -> Option<&Project> {
        self.preview.as_ref()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The revision last explicitly saved; it does not move backwards on undo.
    pub fn saved_revision(&self) -> Option<u64> {
        self.saved.as_ref().map(|p| p.revision)
    }

    /// Compare content rather than revision: undoing to saved content is clean,
    /// even though the current revision is necessarily newer.
    pub fn is_dirty(&self) -> bool {
        self.saved
            .as_ref()
            .is_none_or(|saved| !same_content(saved, &self.project))
    }

    /// Call only after a successful save of the current revision.
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.project.clone());
    }

    /// Record exactly the snapshot written to disk. An edit made while a save
    /// was in progress remains dirty rather than being mistaken for saved data.
    pub(crate) fn mark_saved_snapshot(&mut self, snapshot: Project) {
        if snapshot.id == self.project.id {
            self.saved = Some(snapshot);
        }
    }

    /// Apply a batch against a private copy. Closure errors, structural errors,
    /// and revision overflow leave the project and both history stacks intact.
    /// Returns false for a content no-op (which does not clear the redo branch).
    pub fn transact<E>(
        &mut self,
        edit: impl FnOnce(&mut Project) -> Result<(), E>,
    ) -> Result<bool, EditError<E>> {
        let mut candidate = self.project.clone();
        edit(&mut candidate).map_err(EditError::Command)?;
        if candidate.id != self.project.id {
            return Err(EditError::ProjectIdentityChanged);
        }
        // Revision belongs to the editor, not to callers or preview data.
        candidate.revision = self.project.revision;
        candidate.export_records = self.project.export_records.clone();
        candidate.validate().map_err(EditError::InvalidProject)?;
        if candidate == self.project {
            return Ok(false);
        }
        candidate.revision = self
            .project
            .revision
            .checked_add(1)
            .ok_or(EditError::RevisionExhausted)?;
        self.undo.push(self.project.clone());
        self.project = candidate;
        self.redo.clear();
        self.preview = None;
        Ok(true)
    }

    /// Start or restart a transient drag/edit from the committed state.
    pub fn begin_preview(&mut self) {
        self.preview = Some(self.project.clone());
    }

    /// Failed preview steps preserve the preceding preview. Intermediate preview
    /// geometry need not validate; confirmation always goes through `transact`.
    pub fn update_preview<E>(
        &mut self,
        edit: impl FnOnce(&mut Project) -> Result<(), E>,
    ) -> Result<(), EditError<E>> {
        let mut candidate = self.preview.clone().ok_or(EditError::NoPreview)?;
        edit(&mut candidate).map_err(EditError::Command)?;
        self.preview = Some(candidate);
        Ok(())
    }

    pub fn cancel_preview(&mut self) {
        self.preview = None;
    }

    /// An invalid preview remains available for correction or cancellation.
    pub fn commit_preview(&mut self) -> Result<bool, EditError<()>> {
        let candidate = self.preview.clone().ok_or(EditError::NoPreview)?;
        let changed = self.transact(|project| {
            *project = candidate;
            Ok(())
        })?;
        self.preview = None;
        Ok(changed)
    }

    pub fn undo(&mut self) -> Result<bool, EditError<()>> {
        let Some(before) = self.undo.last() else {
            return Ok(false);
        };
        let revision = self
            .project
            .revision
            .checked_add(1)
            .ok_or(EditError::RevisionExhausted)?;
        let mut before = before.clone();
        before.revision = revision;
        before.export_records = self.project.export_records.clone();
        self.undo.pop();
        self.redo.push(self.project.clone());
        self.project = before;
        self.preview = None;
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, EditError<()>> {
        let Some(after) = self.redo.last() else {
            return Ok(false);
        };
        let revision = self
            .project
            .revision
            .checked_add(1)
            .ok_or(EditError::RevisionExhausted)?;
        let mut after = after.clone();
        after.revision = revision;
        after.export_records = self.project.export_records.clone();
        self.redo.pop();
        self.undo.push(self.project.clone());
        self.project = after;
        self.preview = None;
        Ok(true)
    }
}

fn same_content(a: &Project, b: &Project) -> bool {
    let mut a = a.clone();
    a.revision = b.revision;
    a == *b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Allocation, Board, BoardGrain, CatalogReference, Hardware, HardwareKind, Material, Stock,
        StockGrain, StockSource,
    };
    use crate::money::Currency;
    use crate::units::{Length, Pose, Quaternion};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }

    fn fixture() -> Project {
        let mut p = Project::new("Cabinet", Currency::Brl);
        let material = Uuid::new_v4();
        let stock = Uuid::new_v4();
        let catalog = Uuid::new_v4();
        p.materials.push(Material {
            id: material,
            name: "Plywood".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Length,
        });
        p.stock.push(Stock {
            id: stock,
            name: "Sheet".into(),
            material_id: material,
            length: mm(200),
            width: mm(100),
            thickness: mm(18),
            grain: StockGrain::AlongX,
            source: StockSource::Owned,
            price: None,
            priority: 0,
            trim: [Length::ZERO; 4],
        });
        for name in ["Side", "Side"] {
            let board = Uuid::new_v4();
            p.boards.push(Board {
                id: board,
                name: name.into(),
                material_id: material,
                length: mm(100),
                width: mm(50),
                thickness: mm(18),
                grain_override: None,
                parent_id: None,
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            });
            p.allocations.push(Allocation {
                id: Uuid::new_v4(),
                board_id: board,
                stock_id: stock,
                origin: [Length::ZERO; 2],
                quarter_turn: false,
                locked: false,
            });
        }
        p.catalog.push(CatalogReference {
            id: catalog,
            name: "Example".into(),
            product_id: "example".into(),
            plate_id: None,
            source: "fixture".into(),
            revision: "1".into(),
            installation_dimensions: HashMap::new(),
            verified_hinge: None,
        });
        p.hardware.push(Hardware {
            id: Uuid::new_v4(),
            name: "Hinge".into(),
            parent_id: None,
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            kind: HardwareKind::Catalog {
                catalog_id: catalog,
            },
        });
        p.validate().unwrap();
        p
    }

    #[test]
    fn batch_edit_undoes_all_records_and_revisions_are_monotonic() {
        let original = fixture();
        let mut editor = ProjectEditor::new(original.clone()).unwrap();
        assert_eq!(editor.saved_revision(), Some(0));
        assert!(!editor.is_dirty());
        assert_eq!(
            editor.transact(|p| -> Result<(), ()> {
                for board in &mut p.boards {
                    board.length = mm(210); // conflicting placement remains a valid draft
                }
                p.allocations[0].locked = true;
                p.stock[0].priority = 5;
                p.catalog[0].revision = "2".into();
                p.hardware[0].name = "Updated hinge".into();
                Ok(())
            }),
            Ok(true)
        );
        let changed = editor.project().clone();
        assert_eq!(changed.revision, 1);
        assert!(editor.is_dirty());
        assert_eq!(editor.undo(), Ok(true));
        assert!(same_content(editor.project(), &original));
        assert_eq!(editor.project().revision, 2);
        assert!(!editor.is_dirty());
        assert_eq!(editor.undo(), Ok(false));
        assert_eq!(editor.redo(), Ok(true));
        assert!(same_content(editor.project(), &changed));
        assert_eq!(editor.project().revision, 3);
        assert_eq!(editor.saved_revision(), Some(0));
        editor.mark_saved();
        assert_eq!(editor.saved_revision(), Some(3));
        assert!(!editor.is_dirty());
    }

    #[test]
    fn failures_leave_project_history_and_redo_untouched() {
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.name = "A".into();
                Ok(())
            })
            .unwrap();
        editor.undo().unwrap();
        let before = editor.project().clone();
        assert_eq!(
            editor.transact(|p| -> Result<(), &str> {
                p.boards.clear();
                Err("cancelled")
            }),
            Err(EditError::Command("cancelled"))
        );
        assert!(matches!(
            editor.transact(|p| -> Result<(), ()> {
                p.boards.clear();
                Ok(())
            }),
            Err(EditError::InvalidProject(
                DomainError::DanglingReference { .. }
            ))
        ));
        assert_eq!(editor.project(), &before);
        assert!(!editor.can_undo());
        assert!(editor.can_redo());
        assert_eq!(editor.redo(), Ok(true));
    }

    #[test]
    fn cancelled_and_invalid_previews_do_not_touch_history() {
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        let before = editor.project().clone();
        editor.begin_preview();
        editor
            .update_preview(|p| -> Result<(), ()> {
                p.boards[0].length = mm(300);
                Ok(())
            })
            .unwrap();
        assert_ne!(editor.preview().unwrap(), &before);
        assert_eq!(editor.project(), &before);
        editor.cancel_preview();
        assert_eq!(editor.preview(), None);
        assert!(!editor.can_undo());
        editor.begin_preview();
        editor
            .update_preview(|p| -> Result<(), ()> {
                p.boards[0].width = Length::ZERO;
                Ok(())
            })
            .unwrap();
        assert!(matches!(
            editor.commit_preview(),
            Err(EditError::InvalidProject(DomainError::InvalidDimension(_)))
        ));
        assert!(editor.preview().is_some());
        editor.cancel_preview();
        assert_eq!(editor.project(), &before);
        assert!(!editor.can_undo());
        assert_eq!(editor.project().revision, 0);
    }

    #[test]
    fn redo_restores_created_ids_and_dependent_references() {
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        let board = editor.project().boards[0].duplicate();
        let allocation = Allocation {
            id: Uuid::new_v4(),
            board_id: board.id,
            stock_id: editor.project().stock[0].id,
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: false,
        };
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards.push(board.clone());
                p.allocations.push(allocation.clone());
                Ok(())
            })
            .unwrap();
        editor.undo().unwrap();
        editor.redo().unwrap();
        assert_eq!(editor.project().boards.last().unwrap().id, board.id);
        assert_eq!(editor.project().allocations.last().unwrap(), &allocation);
        editor.project().validate().unwrap();
    }

    #[test]
    fn no_op_preserves_redo_but_successful_edit_invalidates_it() {
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.name = "Other".into();
                Ok(())
            })
            .unwrap();
        editor.undo().unwrap();
        let revision = editor.project().revision;
        assert_eq!(
            editor.transact(|p| -> Result<(), ()> {
                p.revision = 999;
                Ok(())
            }),
            Ok(false)
        );
        assert_eq!(editor.project().revision, revision);
        assert!(editor.can_redo());
        editor.begin_preview();
        editor
            .update_preview(|p| -> Result<(), ()> {
                p.name = "Third".into();
                Ok(())
            })
            .unwrap();
        assert_eq!(editor.commit_preview(), Ok(true));
        assert!(!editor.can_redo());
        assert_eq!(editor.project().revision, revision + 1);
    }

    #[test]
    fn revision_overflow_is_atomic_even_for_undo() {
        let mut project = fixture();
        project.revision = u64::MAX - 1;
        let mut editor = ProjectEditor::new(project).unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.name = "Changed".into();
                Ok(())
            })
            .unwrap();
        let before = editor.project().clone();
        assert_eq!(editor.undo(), Err(EditError::RevisionExhausted));
        assert_eq!(editor.project(), &before);
        assert!(editor.can_undo());
        assert_eq!(
            editor.transact(|p| -> Result<(), ()> {
                p.name = "Again".into();
                Ok(())
            }),
            Err(EditError::RevisionExhausted)
        );
        assert_eq!(editor.project(), &before);
    }

    #[test]
    fn project_identity_cannot_be_replaced_by_a_transaction() {
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        let before = editor.project().clone();
        assert_eq!(
            editor.transact(|p| -> Result<(), ()> {
                p.id = Uuid::new_v4();
                Ok(())
            }),
            Err(EditError::ProjectIdentityChanged)
        );
        assert_eq!(editor.project(), &before);
        assert!(!editor.can_undo());
    }

    #[test]
    fn grid_spacing_is_atomic_undoable_and_does_not_touch_poses() {
        let mut project = fixture();
        project.boards[0].pose.translation_mm[0] = 0.0005;
        let poses: Vec<_> = project.boards.iter().map(|board| board.pose).collect();
        let mut editor = ProjectEditor::new(project).unwrap();
        let initial = editor.project().clone();
        assert_eq!(editor.project().grid_spacing, mm(10));
        for (spacing, reason) in [
            (Length::ZERO, UnitError::NonPositiveDimension),
            (
                Length::from_micrometres(-1),
                UnitError::NonPositiveDimension,
            ),
            (
                Length::from_micrometres(1_000_000_001),
                UnitError::OutOfBounds,
            ),
        ] {
            assert_eq!(
                editor.set_grid_spacing(spacing),
                Err(EditError::Command(reason))
            );
            assert_eq!(editor.project(), &initial);
            assert!(!editor.can_undo());
        }
        let half_inch = Length::from_inch_fraction("1/2").unwrap().exact().unwrap();
        assert_eq!(editor.set_grid_spacing(half_inch), Ok(true));
        assert_eq!(editor.project().revision, 1);
        assert!(editor.is_dirty());
        assert_eq!(
            editor
                .project()
                .boards
                .iter()
                .map(|b| b.pose)
                .collect::<Vec<_>>(),
            poses
        );
        assert_eq!(editor.set_grid_spacing(half_inch), Ok(false));
        assert_eq!(editor.project().revision, 1);
        assert_eq!(editor.undo(), Ok(true));
        assert_eq!(editor.project().grid_spacing, mm(10));
        assert_eq!(editor.project().revision, 2);
        assert!(!editor.is_dirty());
        assert_eq!(editor.redo(), Ok(true));
        assert_eq!(editor.project().grid_spacing, half_inch);
        assert_eq!(editor.project().revision, 3);
        assert_eq!(
            editor
                .project()
                .boards
                .iter()
                .map(|b| b.pose)
                .collect::<Vec<_>>(),
            poses
        );
    }

    #[test]
    fn grid_input_uses_exact_units_and_requires_rounding_confirmation() {
        use crate::dimension_input::{CommitError, DimensionInput, Locale, parse_length};
        use crate::units::{Conversion, Unit};

        let mut editor = ProjectEditor::new(fixture()).unwrap();
        assert_eq!(
            parse_length("1/2 in", Unit::Mm).unwrap().conversion,
            Conversion::Exact(Length::from_micrometres(12_700))
        );
        assert_eq!(
            parse_length("12,7 mm", Unit::Inch).unwrap().conversion,
            Conversion::Exact(Length::from_micrometres(12_700))
        );
        let mut input = DimensionInput::new(editor.project().grid_spacing, Unit::Mm, Locale::En, 3);
        input.edit("1/64 in");
        assert_eq!(
            input.commit(false),
            Err(CommitError::NeedsConfirmation(Length::from_micrometres(
                397
            )))
        );
        assert_eq!(editor.project().grid_spacing, mm(10));
        assert!(!editor.can_undo());
        input.cancel();
        assert_eq!(input.committed(), mm(10));
        input.edit("1/64 in");
        editor
            .set_grid_spacing(input.commit(true).unwrap())
            .unwrap();
        assert_eq!(editor.project().grid_spacing.micrometres(), 397);
    }
}
