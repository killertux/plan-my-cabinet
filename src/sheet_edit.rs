//! Transient, multi-placement sheet repair. Only acceptance writes allocation
//! records to the editor; design and assembly geometry are never previewed here.
use std::collections::HashSet;

use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::cut_tree::{Reconstruction, ReconstructionViolation, reconstruct_witness};
use crate::domain::{Allocation, Project};
use crate::units::Length;

pub const DEFAULT_WITNESS_BUDGET: usize = 20_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SheetEditError {
    MissingBoard(Uuid),
    MissingStock(Uuid),
    Locked(Uuid),
    NotAllocated(Uuid),
    InvalidPlacement(Vec<SheetDiagnostic>),
    Closed,
}

/// An overlap is distinguished from a layout that is disjoint but cannot be
/// isolated by full-span cuts. Other rule failures retain the witness reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlacementIssue {
    Overlap(Uuid, Uuid),
    KerfOrCutSequence,
    Rule(ReconstructionViolation),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SheetStatus {
    Verified(crate::cut_tree::CutTree),
    Violation(PlacementIssue),
    Exhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetDiagnostic {
    pub stock_id: Uuid,
    pub status: SheetStatus,
}

/// Holds exclusive access to the editor for the duration of a repair. A
/// dropped session cancels its preview; the committed project is never changed
/// by staging, including when an intermediate layout is invalid.
pub struct SheetEditSession<'a> {
    editor: &'a mut ProjectEditor,
    affected: HashSet<Uuid>,
    budget: usize,
    active: bool,
}

impl<'a> SheetEditSession<'a> {
    pub fn begin(editor: &'a mut ProjectEditor) -> Self {
        Self::with_budget(editor, DEFAULT_WITNESS_BUDGET)
    }

    pub fn with_budget(editor: &'a mut ProjectEditor, budget: usize) -> Self {
        editor.begin_preview();
        Self {
            editor,
            affected: HashSet::new(),
            budget,
            active: true,
        }
    }

    /// Reattach a UI frame to its still-active preview without resetting staged work.
    pub fn resume(editor: &'a mut ProjectEditor, affected: HashSet<Uuid>) -> Option<Self> {
        editor.preview()?;
        Some(Self {
            editor,
            affected,
            budget: DEFAULT_WITNESS_BUDGET,
            active: true,
        })
    }

    pub fn pause(mut self) -> HashSet<Uuid> {
        self.active = false;
        std::mem::take(&mut self.affected)
    }

    pub fn preview(&self) -> &Project {
        self.editor.preview().expect("active sheet preview")
    }

    /// Numeric stock coordinates are exact lengths; a UI drag can call the
    /// same method after converting its pointer position to lengths.
    /// Repositioning an existing allocation preserves its identity and lock.
    pub fn place(
        &mut self,
        board_id: Uuid,
        stock_id: Uuid,
        origin: [Length; 2],
        quarter_turn: bool,
    ) -> Result<(), SheetEditError> {
        if !self.active {
            return Err(SheetEditError::Closed);
        }
        let p = self.preview();
        if !p.boards.iter().any(|b| b.id == board_id) {
            return Err(SheetEditError::MissingBoard(board_id));
        }
        if !p.stock.iter().any(|s| s.id == stock_id) {
            return Err(SheetEditError::MissingStock(stock_id));
        }
        let old = p.allocations.iter().find(|a| a.board_id == board_id);
        if old.is_some_and(|a| {
            a.locked
                && (a.stock_id != stock_id || a.origin != origin || a.quarter_turn != quarter_turn)
        }) {
            return Err(SheetEditError::Locked(board_id));
        }
        let previous_stock = old.map(|a| a.stock_id);
        self.editor
            .update_preview(|p| -> Result<(), ()> {
                if let Some(a) = p.allocations.iter_mut().find(|a| a.board_id == board_id) {
                    a.stock_id = stock_id;
                    a.origin = origin;
                    a.quarter_turn = quarter_turn;
                } else {
                    p.allocations.push(Allocation {
                        id: Uuid::new_v4(),
                        board_id,
                        stock_id,
                        origin,
                        quarter_turn,
                        locked: false,
                    });
                }
                Ok(())
            })
            .expect("active preview update");
        self.affected.insert(stock_id);
        if let Some(id) = previous_stock {
            self.affected.insert(id);
        }
        Ok(())
    }

    /// An explicit removal is allowed even if the allocation is locked or
    /// other placements remain invalid. The board becomes an unallocated draft.
    pub fn unallocate(&mut self, board_id: Uuid) -> Result<(), SheetEditError> {
        if !self.active {
            return Err(SheetEditError::Closed);
        }
        let old = self
            .preview()
            .allocations
            .iter()
            .find(|a| a.board_id == board_id)
            .ok_or(SheetEditError::NotAllocated(board_id))?;
        let stock_id = old.stock_id;
        self.editor
            .update_preview(|p| -> Result<(), ()> {
                p.allocations.retain(|a| a.board_id != board_id);
                Ok(())
            })
            .expect("active preview update");
        self.affected.insert(stock_id);
        Ok(())
    }

    pub fn set_lock(&mut self, board_id: Uuid, locked: bool) -> Result<(), SheetEditError> {
        if !self.active {
            return Err(SheetEditError::Closed);
        }
        let stock_id = self
            .preview()
            .allocations
            .iter()
            .find(|a| a.board_id == board_id)
            .ok_or(SheetEditError::NotAllocated(board_id))?
            .stock_id;
        self.editor
            .update_preview(|p| -> Result<(), ()> {
                p.allocations
                    .iter_mut()
                    .find(|a| a.board_id == board_id)
                    .ok_or(())?
                    .locked = locked;
                Ok(())
            })
            .expect("active preview update");
        self.affected.insert(stock_id);
        Ok(())
    }

    pub fn diagnostics(&self) -> Vec<SheetDiagnostic> {
        let mut ids: Vec<_> = self.affected.iter().copied().collect();
        ids.sort_unstable();
        ids.into_iter()
            .map(|stock_id| {
                let result = reconstruct_witness(
                    self.preview(),
                    stock_id,
                    self.preview().cutting_kerf,
                    self.budget,
                );
                let status = match result {
                    Reconstruction::Verified { tree, .. } => SheetStatus::Verified(tree),
                    Reconstruction::BudgetExhausted => SheetStatus::Exhausted,
                    Reconstruction::RuleViolation(reason) => {
                        let issue = if reason == ReconstructionViolation::NoSlicing {
                            overlap(self.preview(), stock_id)
                                .map_or(PlacementIssue::KerfOrCutSequence, |(a, b)| {
                                    PlacementIssue::Overlap(a, b)
                                })
                        } else {
                            PlacementIssue::Rule(reason)
                        };
                        SheetStatus::Violation(issue)
                    }
                };
                SheetDiagnostic { stock_id, status }
            })
            .collect()
    }

    /// A failed acceptance leaves the preview intact for another repair step.
    /// Only affected sheets need witnesses: unrelated old design conflicts
    /// cannot prevent removal or repair on a different sheet.
    pub fn accept(&mut self) -> Result<bool, EditError<SheetEditError>> {
        if !self.active {
            return Err(EditError::Command(SheetEditError::Closed));
        }
        let diagnostics = self.diagnostics();
        if diagnostics
            .iter()
            .any(|d| !matches!(d.status, SheetStatus::Verified(_)))
        {
            return Err(EditError::Command(SheetEditError::InvalidPlacement(
                diagnostics,
            )));
        }
        let allocations = self.preview().allocations.clone();
        let changed = self.editor.transact(|p| -> Result<(), SheetEditError> {
            p.allocations = allocations;
            Ok(())
        })?;
        self.editor.cancel_preview();
        self.active = false;
        Ok(changed)
    }

    pub fn cancel(mut self) {
        self.editor.cancel_preview();
        self.active = false;
    }
}

impl Drop for SheetEditSession<'_> {
    fn drop(&mut self) {
        if self.active {
            self.editor.cancel_preview();
        }
    }
}

fn overlap(project: &Project, stock_id: Uuid) -> Option<(Uuid, Uuid)> {
    let placed: Vec<_> = project
        .allocations
        .iter()
        .filter(|a| a.stock_id == stock_id)
        .collect();
    for (i, a) in placed.iter().enumerate() {
        for b in &placed[i + 1..] {
            let Some(ba) = project.boards.iter().find(|v| v.id == a.board_id) else {
                continue;
            };
            let Some(bb) = project.boards.iter().find(|v| v.id == b.board_id) else {
                continue;
            };
            let ea = if a.quarter_turn {
                [ba.width, ba.length]
            } else {
                [ba.length, ba.width]
            };
            let eb = if b.quarter_turn {
                [bb.width, bb.length]
            } else {
                [bb.length, bb.width]
            };
            if (0..2).all(|axis| {
                let al = i128::from(a.origin[axis].micrometres());
                let bl = i128::from(b.origin[axis].micrometres());
                al < bl + i128::from(eb[axis].micrometres())
                    && bl < al + i128::from(ea[axis].micrometres())
            }) {
                return Some((a.board_id, b.board_id));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cut_tree::WitnessError;
    use crate::domain::{Board, BoardGrain, Material, Stock, StockGrain, StockSource};
    use crate::money::Currency;
    use crate::units::{Pose, Quaternion};

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }

    fn fixture() -> (ProjectEditor, [Uuid; 4], [Uuid; 3]) {
        let mut p = Project::new("repair", Currency::Brl);
        let material = Uuid::new_v4();
        p.materials.push(Material {
            id: material,
            name: "ply".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Unrestricted,
        });
        let stocks = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        for (i, id) in stocks.iter().enumerate() {
            p.stock.push(Stock {
                id: *id,
                name: "sheet".into(),
                material_id: material,
                length: mm(205),
                width: mm(100),
                thickness: mm(18),
                grain: StockGrain::Nondirectional,
                source: StockSource::Owned,
                price: None,
                priority: i as u32,
                trim: [Length::ZERO; 4],
            });
        }
        let boards = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        for (i, id) in boards.iter().enumerate() {
            p.boards.push(Board {
                id: *id,
                name: "part".into(),
                material_id: material,
                length: mm(100),
                width: mm(100),
                thickness: mm(18),
                grain_override: None,
                parent_id: None,
                pose: Pose::new([i as f64, 2.0, 3.0], Quaternion::IDENTITY).unwrap(),
            });
            p.allocations.push(Allocation {
                id: Uuid::new_v4(),
                board_id: *id,
                stock_id: stocks[if i < 2 { 0 } else { 1 }],
                origin: [mm(if i % 2 == 0 { 0 } else { 50 }), Length::ZERO],
                quarter_turn: false,
                locked: false,
            });
        }
        (ProjectEditor::new(p).unwrap(), boards, stocks)
    }

    #[test]
    fn independent_conflicts_repaired_atomically_with_undo_and_unchanged_poses() {
        let (mut editor, boards, stocks) = fixture();
        let before = editor.project().clone();
        let poses: Vec<_> = before.boards.iter().map(|b| b.pose).collect();
        let mut session = SheetEditSession::begin(&mut editor);
        assert!(session.diagnostics().is_empty());
        session
            .place(boards[1], stocks[0], [mm(60), Length::ZERO], false)
            .unwrap();
        // The first intermediate layout is still invalid; stage another repair.
        assert!(matches!(
            session.diagnostics()[0].status,
            SheetStatus::Violation(PlacementIssue::Overlap(..))
        ));
        session
            .place(boards[3], stocks[1], [mm(105), Length::ZERO], false)
            .unwrap();
        assert!(
            session
                .diagnostics()
                .iter()
                .any(|d| matches!(d.status, SheetStatus::Violation(_)))
        );
        session
            .place(boards[1], stocks[0], [mm(105), Length::ZERO], false)
            .unwrap();
        assert!(
            session
                .diagnostics()
                .iter()
                .all(|d| matches!(d.status, SheetStatus::Verified(_)))
        );
        assert!(session.accept().unwrap());
        drop(session);
        assert_eq!(editor.project().revision, before.revision + 1);
        assert_eq!(
            editor
                .project()
                .boards
                .iter()
                .map(|b| b.pose)
                .collect::<Vec<_>>(),
            poses
        );
        editor.undo().unwrap();
        assert_eq!(editor.project().allocations, before.allocations);
        editor.redo().unwrap();
        assert_eq!(editor.project().allocations[1].origin[0], mm(105));
        assert_eq!(editor.project().allocations[3].origin[0], mm(105));
    }

    #[test]
    fn invalid_intermediate_is_retained_on_rejection_and_cancel_restores() {
        let (mut editor, boards, stocks) = fixture();
        let before = editor.project().clone();
        {
            let mut session = SheetEditSession::begin(&mut editor);
            session
                .place(boards[0], stocks[0], [mm(20), Length::ZERO], false)
                .unwrap();
            assert!(matches!(
                session.diagnostics()[0].status,
                SheetStatus::Violation(PlacementIssue::Overlap(..))
            ));
            assert!(matches!(
                session.accept(),
                Err(EditError::Command(SheetEditError::InvalidPlacement(_)))
            ));
            assert_eq!(session.preview().allocations[0].origin[0], mm(20));
            session.cancel();
        }
        assert_eq!(editor.project(), &before);
        assert!(!editor.can_undo());
    }

    #[test]
    fn explicit_unallocation_of_locked_part_ignores_other_invalid_sheet() {
        let (mut editor, boards, stocks) = fixture();
        let before = editor.project().clone();
        let mut session = SheetEditSession::begin(&mut editor);
        session.set_lock(boards[0], true).unwrap();
        assert_eq!(
            session.place(boards[0], stocks[2], [Length::ZERO; 2], false),
            Err(SheetEditError::Locked(boards[0]))
        );
        session.unallocate(boards[0]).unwrap();
        // Board 1 remains on sheet 0. Sheet 1's unrelated overlap stays a draft.
        assert!(matches!(
            session
                .diagnostics()
                .iter()
                .find(|d| d.stock_id == stocks[0])
                .unwrap()
                .status,
            SheetStatus::Verified(_)
        ));
        session.accept().unwrap();
        drop(session);
        assert!(
            editor
                .project()
                .allocations
                .iter()
                .all(|a| a.board_id != boards[0])
        );
        assert_eq!(editor.project().allocations.len(), 3);
        editor.undo().unwrap();
        assert_eq!(editor.project().allocations, before.allocations);
    }

    #[test]
    fn transfer_rotation_grain_and_unlock() {
        let (mut editor, boards, stocks) = fixture();
        // Make a narrow directional destination where a rotated board is valid.
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].length = mm(100);
                p.boards[0].width = mm(50);
                p.boards[0].grain_override = Some(BoardGrain::Length);
                p.stock[2].length = mm(50);
                p.stock[2].width = mm(100);
                p.stock[2].grain = StockGrain::AlongY;
                Ok(())
            })
            .unwrap();
        let mut session = SheetEditSession::begin(&mut editor);
        session.set_lock(boards[0], true).unwrap();
        assert_eq!(
            session.place(boards[0], stocks[2], [Length::ZERO; 2], true),
            Err(SheetEditError::Locked(boards[0]))
        );
        session.set_lock(boards[0], false).unwrap();
        session
            .place(boards[0], stocks[2], [Length::ZERO; 2], false)
            .unwrap();
        assert!(matches!(
            session
                .diagnostics()
                .iter()
                .find(|d| d.stock_id == stocks[2])
                .unwrap()
                .status,
            SheetStatus::Violation(PlacementIssue::Rule(ReconstructionViolation::Witness(
                WitnessError::GrainMismatch(_)
            )))
        ));
        session
            .place(boards[0], stocks[2], [Length::ZERO; 2], true)
            .unwrap();
        assert!(matches!(
            session
                .diagnostics()
                .iter()
                .find(|d| d.stock_id == stocks[2])
                .unwrap()
                .status,
            SheetStatus::Verified(_)
        ));
        session.accept().unwrap();
        drop(session);
        assert_eq!(editor.project().allocations[0].stock_id, stocks[2]);
        assert!(editor.project().allocations[0].quarter_turn);
    }

    #[test]
    fn budget_exhaustion_is_not_commit_authority() {
        let (mut editor, boards, stocks) = fixture();
        let mut session = SheetEditSession::with_budget(&mut editor, 0);
        session.unallocate(boards[1]).unwrap();
        assert!(matches!(
            session.diagnostics()[0].status,
            SheetStatus::Exhausted
        ));
        assert!(session.accept().is_err());
        session.cancel();
        assert_eq!(editor.project().allocations[1].stock_id, stocks[0]);
    }
}
