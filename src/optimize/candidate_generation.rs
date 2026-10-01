//! Bounded, reproducible allocation alternatives on an immutable document snapshot.
//! A candidate is certified only after an independent pass over every used stock.
use std::collections::HashSet;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::cut_tree::{
    AreaAccounting, Axis, CutTree, Reconstruction, ReconstructionViolation, WitnessError,
    reconstruct_witness_cancellable, validate_witness,
};
use crate::domain::{
    Allocation, Board, BoardGrain, DomainError, Project, Stock, StockGrain, StockSource,
};
use crate::first_fit::candidate_axis;
use crate::sheet_packer;
use crate::units::Length;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchBudget {
    /// Maximum placement proposals across all ordering and orientation passes.
    pub placements: usize,
    /// Maximum reconstruction states per witness attempt.
    pub witness_states: usize,
    /// Maximum live alternatives after each board is placed.
    pub beam_width: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StockWitness {
    pub stock_id: Uuid,
    pub tree: CutTree,
    pub accounting: AreaAccounting,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub allocations: Vec<Allocation>,
    /// One independently checked tree per used physical stock piece.
    pub witnesses: Vec<StockWitness>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerationError {
    Cancelled,
    InvalidProject(DomainError),
    InvalidLockedStock {
        stock_id: Uuid,
        reason: Box<ReconstructionViolation>,
    },
    LockedStockUnproven(Uuid),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Generation {
    /// Includes the original complete layout when its placements can be proven feasible.
    /// No candidate is committed by generation or ranking.
    pub complete: Vec<Candidate>,
    /// Best verified placed subset if no full candidate was found.
    pub partial: Option<Candidate>,
    /// True when the placement or witness search bound prevented proof.
    pub exhausted: bool,
}

fn area(board: &Board) -> i128 {
    i128::from(board.length.micrometres()) * i128::from(board.width.micrometres())
}

fn grain_rank(project: &Project, board: &Board) -> u8 {
    let material = project
        .materials
        .iter()
        .find(|m| m.id == board.material_id)
        .expect("validated material");
    u8::from(board.effective_grain(material) == BoardGrain::Unrestricted)
}

pub(crate) fn stock_order(project: &Project, cost_aware: bool) -> Vec<&Stock> {
    let mut stock = project.ordered_stock();
    if cost_aware {
        // Unknown purchase prices come last; ownership is a known zero material expense.
        stock.sort_by_key(|s| {
            let cost = match s.source {
                StockSource::Owned => Some(0),
                StockSource::ToPurchase => s.price.map(|p| p.minor_units()),
            };
            (cost.is_none(), cost.unwrap_or(i64::MAX), s.priority, s.id)
        });
    }
    stock
}

pub(crate) fn allocation_id(board: Uuid, stock: Uuid, origin: [Length; 2], turn: bool) -> Uuid {
    let mut hash = Sha256::new();
    hash.update(b"candidate allocation v1");
    hash.update(board.as_bytes());
    hash.update(stock.as_bytes());
    for coordinate in origin {
        hash.update(coordinate.micrometres().to_be_bytes());
    }
    hash.update([u8::from(turn)]);
    Uuid::from_slice(&hash.finalize()[..16]).expect("16-byte digest prefix")
}

fn snapshot(project: &Project, allocations: &[Allocation]) -> Project {
    let mut copy = project.clone();
    copy.allocations = allocations.to_vec();
    copy
}

fn certify(
    project: &Project,
    allocations: &[Allocation],
    budget: usize,
    axis_order: [Axis; 2],
    cancelled: &impl Fn() -> bool,
) -> Result<Candidate, Option<Uuid>> {
    let copy = snapshot(project, allocations);
    let mut witnesses = Vec::new();
    for stock in project.ordered_stock() {
        if cancelled() {
            return Err(None);
        }
        if !allocations.iter().any(|a| a.stock_id == stock.id) {
            continue;
        }
        match reconstruct_witness_cancellable(
            &copy,
            stock.id,
            project.cutting_kerf,
            budget,
            axis_order,
            cancelled,
        ) {
            Ok(Reconstruction::Verified { tree, .. }) => {
                let accounting = validate_witness(&tree, &copy, stock.id).map_err(|_| None)?;
                witnesses.push(StockWitness {
                    stock_id: stock.id,
                    tree,
                    accounting,
                });
            }
            Ok(Reconstruction::BudgetExhausted) => return Err(Some(stock.id)),
            Err(()) | Ok(Reconstruction::RuleViolation(_)) => return Err(None),
        }
    }
    Ok(Candidate {
        allocations: allocations.to_vec(),
        witnesses,
    })
}

/// Verify an externally retained result against the same snapshot. Completeness,
/// identity, locks and each tree's exact leaves are all checked independently.
pub fn validate_complete(project: &Project, candidate: &Candidate) -> Result<(), WitnessError> {
    let assigned: HashSet<_> = candidate.allocations.iter().map(|a| a.board_id).collect();
    if assigned.len() != project.boards.len()
        || candidate.allocations.len() != project.boards.len()
        || project.boards.iter().any(|b| !assigned.contains(&b.id))
    {
        return Err(WitnessError::MissingAllocation(
            project
                .boards
                .iter()
                .find(|b| !assigned.contains(&b.id))
                .map_or(project.id, |b| b.id),
        ));
    }
    for lock in project.allocations.iter().filter(|a| a.locked) {
        if !candidate.allocations.contains(lock) {
            return Err(WitnessError::PlacementMismatch(lock.board_id));
        }
    }
    let copy = snapshot(project, &candidate.allocations);
    copy.validate()
        .map_err(|_| WitnessError::InvalidStock(project.id))?;
    let used: HashSet<_> = candidate.allocations.iter().map(|a| a.stock_id).collect();
    if candidate.witnesses.len() != used.len() {
        return Err(WitnessError::InvalidStock(project.id));
    }
    let mut seen = HashSet::new();
    for witness in &candidate.witnesses {
        if !used.contains(&witness.stock_id)
            || !seen.insert(witness.stock_id)
            || validate_witness(&witness.tree, &copy, witness.stock_id)? != witness.accounting
        {
            return Err(WitnessError::InvalidStock(witness.stock_id));
        }
    }
    Ok(())
}

fn signature(allocations: &[Allocation]) -> Vec<(Uuid, Uuid, [i64; 2], bool)> {
    let mut key: Vec<_> = allocations
        .iter()
        .map(|a| {
            (
                a.board_id,
                a.stock_id,
                a.origin.map(Length::micrometres),
                a.quarter_turn,
            )
        })
        .collect();
    key.sort();
    key
}

const MIN_PROOF_STATES: usize = 20_000;
const MAX_PROOF_STATES: usize = 100_000;

fn proof_states(budget: SearchBudget) -> usize {
    budget
        .witness_states
        .clamp(MIN_PROOF_STATES, MAX_PROOF_STATES)
}

fn certify_both_axes(
    project: &Project,
    allocations: &[Allocation],
    states: usize,
    cancelled: &impl Fn() -> bool,
) -> Result<Candidate, bool> {
    let first = certify(project, allocations, states, [Axis::X, Axis::Y], cancelled);
    if let Ok(candidate) = first {
        return Ok(candidate);
    }
    if cancelled() {
        return Err(false);
    }
    let second = certify(project, allocations, states, [Axis::Y, Axis::X], cancelled);
    match second {
        Ok(candidate) => Ok(candidate),
        Err(reason) => Err(matches!(first, Err(Some(_))) || reason.is_some()),
    }
}

/// Several stable board/stock passes and low/high origin directions. A beam
/// carries only witnessed placed subsets; every emitted complete result gets a
/// fresh certification pass. Existing unlocked positions are free to change.
pub fn generate(project: &Project, budget: SearchBudget) -> Result<Generation, GenerationError> {
    generate_cancellable(project, budget, &|| false, &|_| {})
}

/// The predicate is polled inside recursive proof and every placement loop.
/// Progress counts placement proposals; proof of an incumbent may precede the first proposal.
pub fn generate_cancellable(
    project: &Project,
    budget: SearchBudget,
    cancelled: &impl Fn() -> bool,
    progress: &impl Fn(usize),
) -> Result<Generation, GenerationError> {
    generate_impl(project, budget, proof_states(budget), cancelled, progress)
}

#[cfg(test)]
fn generate_with_proof_states(
    project: &Project,
    budget: SearchBudget,
    proof_states: usize,
) -> Result<Generation, GenerationError> {
    generate_impl(project, budget, proof_states, &|| false, &|_| {})
}

fn generate_impl(
    project: &Project,
    budget: SearchBudget,
    proof_states: usize,
    cancelled: &impl Fn() -> bool,
    progress: &impl Fn(usize),
) -> Result<Generation, GenerationError> {
    let check = || {
        if cancelled() {
            Err(GenerationError::Cancelled)
        } else {
            Ok(())
        }
    };
    check()?;
    project
        .validate()
        .map_err(GenerationError::InvalidProject)?;
    let locked: Vec<_> = project
        .allocations
        .iter()
        .filter(|a| a.locked)
        .cloned()
        .collect();
    let locked_copy = snapshot(project, &locked);
    // Proof of committed placements is independent of alternative-search budgets,
    // but still bounded. Never infer feasibility from completeness alone.
    let (incumbent, incumbent_exhausted) = if project.allocations.len() == project.boards.len() {
        match certify_both_axes(project, &project.allocations, proof_states, cancelled) {
            Ok(candidate) if validate_complete(project, &candidate).is_ok() => {
                (Some(candidate), false)
            }
            Ok(_) => (None, false),
            Err(exhausted) => (None, exhausted),
        }
    } else {
        (None, false)
    };
    check()?;
    for stock in project.ordered_stock() {
        check()?;
        if !locked.iter().any(|a| a.stock_id == stock.id) {
            continue;
        }
        let first = reconstruct_witness_cancellable(
            &locked_copy,
            stock.id,
            project.cutting_kerf,
            proof_states,
            [Axis::X, Axis::Y],
            cancelled,
        )
        .map_err(|_| GenerationError::Cancelled)?;
        check()?;
        let outcome = if matches!(first, Reconstruction::Verified { .. }) {
            first
        } else {
            let second = reconstruct_witness_cancellable(
                &locked_copy,
                stock.id,
                project.cutting_kerf,
                proof_states,
                [Axis::Y, Axis::X],
                cancelled,
            )
            .map_err(|_| GenerationError::Cancelled)?;
            check()?;
            if matches!(second, Reconstruction::Verified { .. }) {
                second
            } else if matches!(first, Reconstruction::BudgetExhausted) {
                first
            } else {
                second
            }
        };
        match outcome {
            Reconstruction::Verified { .. } => {}
            Reconstruction::BudgetExhausted => {
                return Err(GenerationError::LockedStockUnproven(stock.id));
            }
            Reconstruction::RuleViolation(reason) => {
                return Err(GenerationError::InvalidLockedStock {
                    stock_id: stock.id,
                    reason: Box::new(reason),
                });
            }
        }
    }
    let mut result = Generation {
        complete: incumbent.into_iter().collect(),
        partial: None,
        exhausted: incumbent_exhausted,
    };
    let mut attempts = 0;
    let mut complete_keys: HashSet<_> = result
        .complete
        .iter()
        .map(|candidate| signature(&candidate.allocations))
        .collect();
    // Constructive packing proves each sheet as it builds it and handles
    // real cabinets in microseconds. The placement search below is only a
    // fallback when packing leaves boards that the declared stock could hold.
    // A zero budget asks for no alternatives at all.
    let (packed, unreachable) = if budget.placements > 0 && budget.witness_states > 0 {
        sheet_packer::candidates(project)
    } else {
        (Vec::new(), false)
    };
    for candidate in packed {
        check()?;
        attempts += 1;
        progress(attempts);
        check()?;
        let key = signature(&candidate.allocations);
        if !complete_keys.contains(&key) && validate_complete(project, &candidate).is_ok() {
            complete_keys.insert(key);
            result.complete.push(candidate);
        }
    }
    let search = result.complete.is_empty() && !unreachable;
    for ordering in (0..4).filter(|_| search) {
        check()?;
        let mut boards: Vec<_> = project
            .boards
            .iter()
            .filter(|b| !locked.iter().any(|a| a.board_id == b.id))
            .collect();
        boards.sort_by(|a, b| match ordering {
            0 => area(b).cmp(&area(a)).then(a.id.cmp(&b.id)),
            1 => b
                .length
                .max(b.width)
                .cmp(&a.length.max(a.width))
                .then(a.id.cmp(&b.id)),
            2 => grain_rank(project, a)
                .cmp(&grain_rank(project, b))
                .then(area(b).cmp(&area(a)))
                .then(a.id.cmp(&b.id)),
            _ => a.id.cmp(&b.id),
        });
        for reverse in [false, true] {
            check()?;
            let axes = if reverse {
                [Axis::Y, Axis::X]
            } else {
                [Axis::X, Axis::Y]
            };
            let stock = stock_order(project, ordering == 3);
            let mut beam = vec![locked.clone()];
            for board in &boards {
                check()?;
                let mut next = Vec::new();
                let mut seen = HashSet::new();
                for state in &beam {
                    check()?;
                    let copy = snapshot(project, state);
                    for piece in &stock {
                        check()?;
                        if piece.material_id != board.material_id
                            || piece.thickness != board.thickness
                        {
                            continue;
                        }
                        let base = match reconstruct_witness_cancellable(
                            &copy,
                            piece.id,
                            project.cutting_kerf,
                            budget.witness_states,
                            axes,
                            cancelled,
                        ) {
                            Err(()) => return Err(GenerationError::Cancelled),
                            Ok(Reconstruction::Verified { tree, .. }) => tree,
                            Ok(Reconstruction::BudgetExhausted) => {
                                result.exhausted = true;
                                continue;
                            }
                            Ok(Reconstruction::RuleViolation(_)) => continue,
                        };
                        let material = project
                            .materials
                            .iter()
                            .find(|m| m.id == board.material_id)
                            .expect("validated material");
                        for turn in if reverse {
                            [true, false]
                        } else {
                            [false, true]
                        } {
                            check()?;
                            let required = match board.effective_grain(material) {
                                BoardGrain::Unrestricted => None,
                                BoardGrain::Length => Some(if turn {
                                    StockGrain::AlongY
                                } else {
                                    StockGrain::AlongX
                                }),
                                BoardGrain::Width => Some(if turn {
                                    StockGrain::AlongX
                                } else {
                                    StockGrain::AlongY
                                }),
                            };
                            if required.is_some_and(|r| {
                                piece.grain != StockGrain::Nondirectional && piece.grain != r
                            }) {
                                continue;
                            }
                            let extent = if turn {
                                [board.width, board.length]
                            } else {
                                [board.length, board.width]
                            };
                            let mut xs = candidate_axis(&base, 0, extent[0].micrometres());
                            let mut ys = candidate_axis(&base, 1, extent[1].micrometres());
                            if reverse {
                                xs.reverse();
                                ys.reverse();
                            }
                            for y in ys {
                                check()?;
                                for &x in &xs {
                                    check()?;
                                    if attempts >= budget.placements {
                                        result.exhausted = true;
                                        break;
                                    }
                                    attempts += 1;
                                    progress(attempts);
                                    let origin =
                                        [Length::from_micrometres(x), Length::from_micrometres(y)];
                                    let mut trial = state.clone();
                                    trial.push(Allocation {
                                        id: allocation_id(board.id, piece.id, origin, turn),
                                        board_id: board.id,
                                        stock_id: piece.id,
                                        origin,
                                        quarter_turn: turn,
                                        locked: false,
                                    });
                                    let test = snapshot(project, &trial);
                                    match reconstruct_witness_cancellable(
                                        &test,
                                        piece.id,
                                        project.cutting_kerf,
                                        budget.witness_states,
                                        axes,
                                        cancelled,
                                    ) {
                                        Err(()) => {
                                            return Err(GenerationError::Cancelled);
                                        }
                                        Ok(Reconstruction::Verified { .. }) => {
                                            if seen.insert(signature(&trial))
                                                && next.len() < budget.beam_width
                                            {
                                                next.push(trial);
                                            }
                                        }
                                        Ok(Reconstruction::BudgetExhausted) => {
                                            result.exhausted = true
                                        }
                                        Ok(Reconstruction::RuleViolation(_)) => {}
                                    }
                                }
                                if attempts >= budget.placements {
                                    break;
                                }
                            }
                        }
                    }
                }
                if next.is_empty() {
                    break;
                }
                beam = next;
                if beam.is_empty() {
                    break;
                }
                if result
                    .partial
                    .as_ref()
                    .is_none_or(|p| p.allocations.len() < beam[0].len())
                    && let Ok(candidate) =
                        certify(project, &beam[0], budget.witness_states, axes, cancelled)
                {
                    result.partial = Some(candidate);
                }
                check()?;
            }
            for state in beam {
                check()?;
                if state.len() != project.boards.len() {
                    continue;
                }
                let key = signature(&state);
                if complete_keys.contains(&key) {
                    continue;
                }
                match certify(project, &state, budget.witness_states, axes, cancelled) {
                    Ok(candidate) if validate_complete(project, &candidate).is_ok() => {
                        complete_keys.insert(key);
                        result.complete.push(candidate);
                    }
                    Err(Some(_)) => result.exhausted = true,
                    _ => {}
                }
            }
        }
    }
    if result.complete.is_empty()
        && result.partial.is_none()
        && let Ok(candidate) = certify(
            project,
            &locked,
            budget.witness_states,
            [Axis::X, Axis::Y],
            cancelled,
        )
    {
        result.partial = Some(candidate);
    }
    check()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate_ranking::{Objective, rank};
    use crate::cut_tree::reconstruct_witness;
    use crate::domain::{Material, Stock};
    use crate::money::Currency;
    use crate::units::{Pose, Quaternion};

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }
    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }
    fn fixture() -> Project {
        let mut project = Project::new("candidates", Currency::Brl);
        project.id = id(1);
        project.materials.push(Material {
            coating: Default::default(),
            default_band: None,
            kind: Default::default(),
            id: id(2),
            name: "ply".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Unrestricted,
        });
        for (index, size) in [(0, 100), (1, 100)] {
            project.boards.push(Board {
                coated_face: Default::default(),
                banding: Default::default(),
                id: id(10 + index),
                name: "part".into(),
                material_id: id(2),
                length: mm(size),
                width: mm(50),
                thickness: mm(18),
                grain_override: None,
                parent_id: None,
                pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
            });
        }
        for (index, size) in [(0, 205), (1, 100)] {
            project.stock.push(Stock {
                id: id(20 + index),
                name: "stock".into(),
                material_id: id(2),
                length: mm(size),
                width: mm(50),
                thickness: mm(18),
                grain: StockGrain::Nondirectional,
                source: StockSource::Owned,
                price: None,
                priority: index as u32,
                trim: [Length::ZERO; 4],
            });
        }
        project
    }
    fn budget() -> SearchBudget {
        SearchBudget {
            placements: 500,
            witness_states: 500,
            beam_width: 6,
        }
    }

    #[test]
    fn cancellation_interrupts_incumbent_and_locked_proofs() {
        use std::cell::Cell;
        let mut project = fixture();
        project.boards.truncate(1);
        project.allocations.push(Allocation {
            id: id(30),
            board_id: id(10),
            stock_id: id(20),
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: true,
        });
        for threshold in [2, 30] {
            let count = Cell::new(0);
            let result = generate_cancellable(
                &project,
                budget(),
                &|| {
                    count.set(count.get() + 1);
                    count.get() >= threshold
                },
                &|_| {},
            );
            assert_eq!(result, Err(GenerationError::Cancelled));
        }
    }

    #[test]
    fn cancellation_after_a_placement_interrupts_search() {
        use std::cell::Cell;
        let project = fixture();
        let stop = Cell::new(false);
        let result = generate_cancellable(&project, budget(), &|| stop.get(), &|count| {
            if count == 1 {
                stop.set(true);
            }
        });
        assert_eq!(result, Err(GenerationError::Cancelled));
        assert!(project.allocations.is_empty());
    }

    #[test]
    fn deterministic_complete_and_independently_certified() {
        let project = fixture();
        let before = project.clone();
        let a = generate(&project, budget()).unwrap();
        assert_eq!(a, generate(&project, budget()).unwrap());
        assert_eq!(project, before);
        assert!(!a.complete.is_empty());
        assert!(
            a.complete
                .iter()
                .any(|c| c.allocations.iter().all(|a| a.stock_id == id(20)))
        );
        for candidate in &a.complete {
            validate_complete(&project, candidate).unwrap();
            assert_eq!(candidate.allocations.len(), 2);
            for witness in &candidate.witnesses {
                assert_eq!(
                    witness.accounting,
                    validate_witness(
                        &witness.tree,
                        &snapshot(&project, &candidate.allocations),
                        witness.stock_id
                    )
                    .unwrap()
                );
            }
        }
        let mut tampered = a.complete[0].clone();
        let witness = &mut tampered.witnesses[0];
        let leaf = witness
            .tree
            .nodes()
            .iter()
            .position(|node| matches!(node.kind, crate::cut_tree::CutKind::Part(_)))
            .unwrap();
        // A valid tree for a different allocation is not a witness for this candidate.
        let part = match witness.tree.node(leaf).unwrap().kind {
            crate::cut_tree::CutKind::Part(id) => id,
            _ => unreachable!(),
        };
        tampered
            .allocations
            .iter_mut()
            .find(|a| a.board_id == part)
            .unwrap()
            .origin[0] = mm(1);
        assert!(validate_complete(&project, &tampered).is_err());
    }

    #[test]
    fn finite_inventory_partial_and_exhaustion_are_distinct() {
        let mut project = fixture();
        project.stock.truncate(1);
        project.stock[0].length = mm(100);
        let partial = generate(&project, budget()).unwrap();
        assert!(partial.complete.is_empty());
        assert_eq!(partial.partial.unwrap().allocations.len(), 1);
        assert!(!partial.exhausted);
        let exhausted = generate(
            &project,
            SearchBudget {
                placements: 0,
                ..budget()
            },
        )
        .unwrap();
        assert!(exhausted.complete.is_empty());
        assert!(exhausted.exhausted);
        assert!(exhausted.partial.unwrap().allocations.is_empty());
        assert_eq!(project.stock.len(), 1);
        let unknown = generate(
            &fixture(),
            SearchBudget {
                witness_states: 0,
                ..budget()
            },
        )
        .unwrap();
        assert!(unknown.complete.is_empty());
        assert!(unknown.exhausted);
    }

    #[test]
    fn locks_remain_exact_and_invalid_locks_stop_generation() {
        let mut project = fixture();
        let lock = Allocation {
            id: id(30),
            board_id: id(10),
            stock_id: id(20),
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: true,
        };
        project.allocations.push(lock.clone());
        let generated = generate(&project, budget()).unwrap();
        assert!(!generated.complete.is_empty());
        assert!(
            generated
                .complete
                .iter()
                .all(|c| c.allocations.contains(&lock))
        );
        project.allocations[0].origin[0] = mm(3);
        assert!(matches!(
            generate(&project, budget()),
            Err(GenerationError::InvalidLockedStock { .. })
        ));
    }

    #[test]
    fn locked_position_and_identity_survive_cheaper_and_fewer_cut_alternatives() {
        let mut project = fixture();
        project.boards.truncate(1);
        project.stock[0].length = mm(110);
        project.stock[0].source = StockSource::ToPurchase;
        project.stock[0].price = Some(crate::money::Money::new(Currency::Brl, 100).unwrap());
        project.cut_fee = Some(crate::money::Money::new(Currency::Brl, 1).unwrap());
        let lock = Allocation {
            id: id(30),
            board_id: id(10),
            stock_id: id(20),
            origin: [mm(5), Length::ZERO],
            quarter_turn: false,
            locked: true,
        };
        project.allocations.push(lock.clone());
        let generated = generate(&project, budget()).unwrap();
        assert!(!generated.complete.is_empty());
        assert!(
            generated
                .complete
                .iter()
                .all(|c| c.allocations == [lock.clone()])
        );
        let mut moved = generated.complete[0].clone();
        moved.allocations[0].stock_id = id(21);
        moved.allocations[0].origin = [Length::ZERO; 2];
        moved.allocations[0].locked = false;
        moved.allocations[0].id = id(31);
        assert_eq!(
            validate_complete(&project, &moved),
            Err(WitnessError::PlacementMismatch(lock.board_id))
        );
        for change in [
            Allocation {
                origin: [Length::ZERO; 2],
                ..lock.clone()
            },
            Allocation {
                quarter_turn: true,
                ..lock.clone()
            },
            Allocation {
                id: id(31),
                ..lock.clone()
            },
        ] {
            let mut altered = generated.complete[0].clone();
            altered.allocations[0] = change;
            assert_eq!(
                validate_complete(&project, &altered),
                Err(WitnessError::PlacementMismatch(lock.board_id))
            );
        }
        // Even a valid tree for the cheaper, cut-free stock cannot authorize
        // moving this locked board.
        let moved_copy = snapshot(&project, &moved.allocations);
        let Reconstruction::Verified { tree, accounting } =
            reconstruct_witness(&moved_copy, id(21), project.cutting_kerf, 100)
        else {
            panic!("exact stock should be feasible");
        };
        moved.witnesses = vec![StockWitness {
            stock_id: id(21),
            tree,
            accounting,
        }];
        assert_eq!(
            validate_complete(&project, &moved),
            Err(WitnessError::PlacementMismatch(lock.board_id))
        );
    }

    #[test]
    fn incumbent_is_kept_with_zero_search_and_witness_budget_and_deduplicated() {
        let mut project = fixture();
        project.allocations = vec![
            Allocation {
                id: id(30),
                board_id: id(10),
                stock_id: id(20),
                origin: [Length::ZERO; 2],
                quarter_turn: false,
                locked: false,
            },
            Allocation {
                id: id(31),
                board_id: id(11),
                stock_id: id(20),
                origin: [mm(105), Length::ZERO],
                quarter_turn: false,
                locked: false,
            },
        ];
        let zero = generate(
            &project,
            SearchBudget {
                placements: 0,
                witness_states: 0,
                beam_width: 0,
            },
        )
        .unwrap();
        assert_eq!(zero.complete.len(), 1);
        assert_eq!(zero.complete[0].allocations, project.allocations);
        validate_complete(&project, &zero.complete[0]).unwrap();
        let ranked = rank(&project, &zero.complete, Objective::FewestCuts).unwrap();
        assert_eq!(ranked.candidates.len(), 1);
        assert_eq!(
            ranked.candidates[0].candidate.allocations,
            project.allocations
        );
        let full = generate(&project, budget()).unwrap();
        assert_eq!(full, generate(&project, budget()).unwrap());
        assert_eq!(full.complete[0].allocations, project.allocations);
        assert_eq!(
            full.complete
                .iter()
                .filter(|c| signature(&c.allocations) == signature(&project.allocations))
                .count(),
            1
        );
        // A structurally valid but geometrically impossible incumbent is a
        // draft, never a verified complete result.
        project.allocations[1].origin[0] = mm(100);
        assert!(project.validate().is_ok());
        assert!(
            generate(
                &project,
                SearchBudget {
                    placements: 0,
                    witness_states: 0,
                    beam_width: 0,
                }
            )
            .unwrap()
            .complete
            .is_empty()
        );
    }

    #[test]
    fn incumbent_proof_exhaustion_is_reported_without_changing_layout() {
        let mut project = fixture();
        project.stock[0].width = mm(105);
        for number in [12, 13] {
            let mut board = project.boards[0].clone();
            board.id = id(number);
            project.boards.push(board);
        }
        project.allocations = vec![
            Allocation {
                id: id(30),
                board_id: id(10),
                stock_id: id(20),
                origin: [Length::ZERO; 2],
                quarter_turn: false,
                locked: false,
            },
            Allocation {
                id: id(31),
                board_id: id(11),
                stock_id: id(20),
                origin: [mm(105), Length::ZERO],
                quarter_turn: false,
                locked: false,
            },
            Allocation {
                id: id(32),
                board_id: id(12),
                stock_id: id(20),
                origin: [Length::ZERO, mm(55)],
                quarter_turn: false,
                locked: false,
            },
            Allocation {
                id: id(33),
                board_id: id(13),
                stock_id: id(20),
                origin: [mm(105), mm(55)],
                quarter_turn: false,
                locked: false,
            },
        ];
        let before = project.clone();
        let zero = SearchBudget {
            placements: 0,
            witness_states: 0,
            beam_width: 0,
        };
        // Exercise the production proof path with a deliberately tiny bound.
        let result = generate_with_proof_states(&project, zero, 3).unwrap();
        assert!(result.exhausted);
        assert!(result.complete.is_empty());
        assert_eq!(project, before);
        assert_eq!(proof_states(zero), MIN_PROOF_STATES);
        assert_eq!(
            proof_states(SearchBudget {
                witness_states: usize::MAX,
                ..zero
            }),
            MAX_PROOF_STATES
        );
    }

    #[test]
    fn locked_proof_exhaustion_is_not_an_invalid_lock() {
        let mut project = fixture();
        project.allocations.push(Allocation {
            id: id(30),
            board_id: id(10),
            stock_id: id(20),
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: true,
        });
        project.allocations.push(Allocation {
            id: id(31),
            board_id: id(11),
            stock_id: id(20),
            origin: [mm(105), Length::ZERO],
            quarter_turn: false,
            locked: true,
        });
        assert_eq!(
            generate_with_proof_states(
                &project,
                SearchBudget {
                    placements: 0,
                    witness_states: 0,
                    beam_width: 0,
                },
                1,
            ),
            Err(GenerationError::LockedStockUnproven(id(20)))
        );
    }

    #[test]
    fn locked_incumbent_is_verified_even_when_alternative_witness_budget_is_zero() {
        let mut project = fixture();
        project.boards.truncate(1);
        project.allocations.push(Allocation {
            id: id(30),
            board_id: id(10),
            stock_id: id(20),
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: true,
        });
        let result = generate(
            &project,
            SearchBudget {
                placements: 0,
                witness_states: 0,
                beam_width: 0,
            },
        )
        .unwrap();
        assert_eq!(result.complete.len(), 1);
        assert_eq!(result.complete[0].allocations, project.allocations);
    }

    #[test]
    fn trims_grain_and_sub_kerf_edges_do_not_create_false_leaves() {
        let mut project = fixture();
        project.stock.truncate(1);
        project.stock[0].length = mm(103);
        project.boards.truncate(1);
        assert!(generate(&project, budget()).unwrap().complete.is_empty());
        project.stock[0].length = mm(105);
        project.stock[0].trim[0] = mm(5);
        let complete = generate(&project, budget()).unwrap().complete;
        assert!(!complete.is_empty());
        assert!(
            complete
                .iter()
                .all(|c| validate_complete(&project, c).is_ok())
        );
        project.materials[0].default_grain = BoardGrain::Length;
        project.stock[0].grain = StockGrain::Unknown;
        assert!(generate(&project, budget()).unwrap().complete.is_empty());
    }

    #[test]
    fn cost_aware_pass_can_choose_owned_stock_before_priced_stock() {
        let mut project = fixture();
        project.boards.truncate(1);
        project.stock[0].source = StockSource::ToPurchase;
        project.stock[0].price = Some(crate::money::Money::new(Currency::Brl, 10_000).unwrap());
        let alternatives = generate(&project, budget()).unwrap();
        assert!(
            alternatives
                .complete
                .iter()
                .any(|c| c.allocations[0].stock_id == id(20))
        );
        assert!(
            alternatives
                .complete
                .iter()
                .any(|c| c.allocations[0].stock_id == id(21))
        );
        assert_eq!(project.allocations.len(), 0);
    }
}
