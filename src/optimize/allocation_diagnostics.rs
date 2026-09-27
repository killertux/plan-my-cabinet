//! Project-wide manufacturing obligations, independent of scene visibility.
use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::cut_tree::{
    CutError, Reconstruction, ReconstructionViolation, WitnessError, reconstruct_witness,
};
use crate::domain::Project;
use crate::material_changes::{ConflictReason, allocation_conflicts};

pub const WITNESS_BUDGET: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    AllocatedValid,
    Unallocated,
    Conflicted,
    UnknownSearchBudget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reason {
    MissingAllocation,
    DuplicateAllocation,
    MissingStock,
    Material,
    Thickness,
    Grain,
    Bounds,
    Overlap,
    Kerf,
    Cutting,
    SearchBudget,
}

impl Reason {
    pub fn key(self) -> &'static str {
        match self {
            Self::MissingAllocation => "board-unallocated",
            Self::DuplicateAllocation => "global-duplicate",
            Self::MissingStock => "global-missing-stock",
            Self::Material => "conflict-material-identity",
            Self::Thickness => "conflict-thickness",
            Self::Grain => "conflict-grain",
            Self::Bounds => "conflict-outside-stock",
            Self::Overlap => "conflict-overlap",
            Self::Kerf => "sheet-kerf-conflict",
            Self::Cutting => "sheet-cut-conflict",
            Self::SearchBudget => "sheet-feasibility-unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardDiagnostic {
    pub board_id: Uuid,
    pub status: Status,
    pub reasons: Vec<Reason>,
}

fn add(reasons: &mut HashMap<Uuid, HashSet<Reason>>, id: Uuid, reason: Reason) {
    reasons.entry(id).or_default().insert(reason);
}

/// Exactly one result per required board, even if its allocation has several failures.
/// Recomputed from the supplied project so edits, undo, and repair previews stay current.
pub fn diagnose(project: &Project) -> Vec<BoardDiagnostic> {
    let mut reasons: HashMap<Uuid, HashSet<Reason>> = HashMap::new();
    let mut counts: HashMap<Uuid, usize> = HashMap::new();
    for allocation in &project.allocations {
        *counts.entry(allocation.board_id).or_default() += 1;
        if !project.stock.iter().any(|s| s.id == allocation.stock_id) {
            add(&mut reasons, allocation.board_id, Reason::MissingStock);
        }
    }
    for conflict in allocation_conflicts(project) {
        for reason in conflict.reasons {
            add(
                &mut reasons,
                conflict.board_id,
                match reason {
                    ConflictReason::MaterialIdentity => Reason::Material,
                    ConflictReason::EffectiveThickness => Reason::Thickness,
                    ConflictReason::Grain => Reason::Grain,
                    ConflictReason::OutsideStock => Reason::Bounds,
                    ConflictReason::Overlap => Reason::Overlap,
                },
            );
        }
    }
    for stock in &project.stock {
        let allocations: Vec<_> = project
            .allocations
            .iter()
            .filter(|a| a.stock_id == stock.id)
            .collect();
        if allocations.is_empty() {
            continue;
        }
        // A too-narrow gap implicates both boards, even when reconstruction
        // reports only the first failed cut.
        for (index, a) in allocations.iter().enumerate() {
            for b in &allocations[index + 1..] {
                let footprint = |allocation: &crate::domain::Allocation| {
                    project
                        .boards
                        .iter()
                        .find(|board| board.id == allocation.board_id)
                        .map(|board| {
                            let extent = if allocation.quarter_turn {
                                [board.width, board.length]
                            } else {
                                [board.length, board.width]
                            };
                            [
                                allocation.origin[0].micrometres(),
                                allocation.origin[1].micrometres(),
                                extent[0].micrometres(),
                                extent[1].micrometres(),
                            ]
                        })
                };
                let (Some(x), Some(y)) = (footprint(a), footprint(b)) else {
                    continue;
                };
                for axis in 0..2 {
                    let other = 1 - axis;
                    let (a0, b0) = (i128::from(x[axis]), i128::from(y[axis]));
                    let (a1, b1) = (a0 + i128::from(x[axis + 2]), b0 + i128::from(y[axis + 2]));
                    let cross = i128::from(x[other])
                        < i128::from(y[other]) + i128::from(y[other + 2])
                        && i128::from(y[other]) < i128::from(x[other]) + i128::from(x[other + 2]);
                    let gap = if a1 <= b0 {
                        b0 - a1
                    } else if b1 <= a0 {
                        a0 - b1
                    } else {
                        -1
                    };
                    if cross && gap >= 0 && gap < i128::from(project.cutting_kerf.micrometres()) {
                        add(&mut reasons, a.board_id, Reason::Kerf);
                        add(&mut reasons, b.board_id, Reason::Kerf);
                    }
                }
            }
        }
        let violation =
            match reconstruct_witness(project, stock.id, project.cutting_kerf, WITNESS_BUDGET) {
                Reconstruction::Verified { .. } => None,
                Reconstruction::BudgetExhausted => Some(Reason::SearchBudget),
                Reconstruction::RuleViolation(ReconstructionViolation::Cut(
                    CutError::SubKerfEdge,
                )) => Some(Reason::Kerf),
                Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                    WitnessError::MaterialMismatch(id),
                )) => {
                    add(&mut reasons, id, Reason::Material);
                    None
                }
                Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                    WitnessError::ThicknessMismatch(id),
                )) => {
                    add(&mut reasons, id, Reason::Thickness);
                    None
                }
                Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                    WitnessError::GrainMismatch(id),
                )) => {
                    add(&mut reasons, id, Reason::Grain);
                    None
                }
                Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                    WitnessError::PlacementMismatch(id),
                )) => {
                    add(&mut reasons, id, Reason::Bounds);
                    None
                }
                Reconstruction::RuleViolation(_) => Some(Reason::Cutting),
            };
        if let Some(reason) = violation {
            for allocation in allocations {
                add(&mut reasons, allocation.board_id, reason);
            }
        }
    }
    project
        .boards
        .iter()
        .map(|board| {
            let count = counts.get(&board.id).copied().unwrap_or(0);
            let mut reasons = reasons.remove(&board.id).unwrap_or_default();
            if count == 0 {
                reasons.insert(Reason::MissingAllocation);
            }
            if count > 1 {
                reasons.insert(Reason::DuplicateAllocation);
            }
            let status = if count == 0 {
                Status::Unallocated
            } else if reasons.iter().any(|r| *r != Reason::SearchBudget) {
                Status::Conflicted
            } else if reasons.contains(&Reason::SearchBudget) {
                Status::UnknownSearchBudget
            } else {
                Status::AllocatedValid
            };
            let mut reasons: Vec<_> = reasons.into_iter().collect();
            reasons.sort_by_key(|r| r.key());
            BoardDiagnostic {
                board_id: board.id,
                status,
                reasons,
            }
        })
        .collect()
}
