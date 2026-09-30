//! Deterministic comparison of independently certified, complete cut plans.
use std::cmp::Ordering;

use uuid::Uuid;

use crate::candidate_generation::{Candidate, validate_complete};
use crate::cut_tree::{Rectangle, WitnessError};
use crate::domain::{Project, StockSource};
use crate::money::{Money, MoneyError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Objective {
    LowestNewSpending,
    FewestCuts,
    LeastUnusedStockArea,
}

/// Areas are square micrometres, summed over entire *used* stock roots.
/// Unused area includes offcuts and all irreversible losses. Waste is a
/// discarded leaf; kerf and trim loss are disjoint from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utilization {
    pub used_stock_area: i128,
    pub finished_part_area: i128,
    pub unused_stock_area: i128,
    pub recoverable_offcut_area: i128,
    pub waste_area: i128,
    pub kerf_loss_area: i128,
    pub trim_loss_area: i128,
}

impl Utilization {
    pub fn irreversible_loss_area(self) -> i128 {
        self.waste_area + self.kerf_loss_area + self.trim_loss_area
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RankedCandidate {
    pub candidate: Candidate,
    pub cuts: u64,
    pub utilization: Utilization,
    /// None means at least one used purchase price or a required cut fee is unknown.
    pub new_spending: Option<Money>,
    /// Actual reusable rectangular leaves (stock ID and full-stock coordinates).
    pub offcuts: Vec<(Uuid, Rectangle)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ranking {
    pub candidates: Vec<RankedCandidate>,
    /// Only meaningful for the spending objective. False when any feasible
    /// alternative has an unknown total, even if the first has a known price.
    pub lowest_spending_claim: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RankingError {
    InvalidCandidate { index: usize, reason: WitnessError },
    Money(MoneyError),
    AreaOverflow,
}

fn sum(target: &mut i128, value: i128) -> Result<(), RankingError> {
    *target = target
        .checked_add(value)
        .ok_or(RankingError::AreaOverflow)?;
    Ok(())
}

fn stable_key(candidate: &Candidate) -> Vec<(Uuid, Uuid, [i64; 2], bool)> {
    let mut key: Vec<_> = candidate
        .allocations
        .iter()
        .map(|a| {
            (
                a.stock_id,
                a.board_id,
                a.origin.map(crate::units::Length::micrometres),
                a.quarter_turn,
            )
        })
        .collect();
    key.sort();
    key
}

fn cost_order(a: &RankedCandidate, b: &RankedCandidate) -> Ordering {
    // Unknown is after a known total at an otherwise tied objective. It is
    // never interpreted as a numeric zero or compared to a known amount.
    match (a.new_spending, b.new_spending) {
        (Some(a), Some(b)) => a.minor_units().cmp(&b.minor_units()),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare(a: &RankedCandidate, b: &RankedCandidate, objective: Objective) -> Ordering {
    let area = || {
        a.utilization
            .unused_stock_area
            .cmp(&b.utilization.unused_stock_area)
    };
    let cuts = || a.cuts.cmp(&b.cuts);
    let primary = match objective {
        Objective::LowestNewSpending => cost_order(a, b).then_with(area).then_with(cuts),
        Objective::FewestCuts => cuts().then_with(area).then_with(|| cost_order(a, b)),
        Objective::LeastUnusedStockArea => area().then_with(cuts).then_with(|| cost_order(a, b)),
    };
    primary.then_with(|| stable_key(&a.candidate).cmp(&stable_key(&b.candidate)))
}

/// Rejects partial or stale witnesses before ranking. Spending is comparable
/// only when every used purchase price and the cut fee are explicitly known,
/// including an explicitly entered zero. Unknown-cost results remain useful
/// for cuts/area comparisons but never establish a cheapest-plan claim.
pub fn rank(
    project: &Project,
    candidates: &[Candidate],
    objective: Objective,
) -> Result<Ranking, RankingError> {
    let mut ranked = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        validate_complete(project, candidate)
            .map_err(|reason| RankingError::InvalidCandidate { index, reason })?;
        let zero = Money::new(project.currency, 0).map_err(RankingError::Money)?;
        let mut material = Some(zero);
        let mut cuts = 0_u64;
        let mut used_stock_area = 0;
        let mut finished_part_area = 0;
        let mut recoverable_offcut_area = 0;
        let mut waste_area = 0;
        let mut kerf_loss_area = 0;
        let mut trim_loss_area = 0;
        let mut offcuts = Vec::new();
        for witness in &candidate.witnesses {
            let stock = project
                .stock
                .iter()
                .find(|s| s.id == witness.stock_id)
                .expect("validated witness stock");
            if stock.source == StockSource::ToPurchase {
                material = match (material, stock.price) {
                    (Some(total), Some(price)) => {
                        Some(total.checked_add(price).map_err(RankingError::Money)?)
                    }
                    _ => None,
                };
            }
            cuts = cuts
                .checked_add(
                    u64::try_from(witness.tree.cut_count())
                        .map_err(|_| RankingError::AreaOverflow)?,
                )
                .ok_or(RankingError::AreaOverflow)?;
            let a = witness.accounting;
            sum(&mut used_stock_area, a.root_area)?;
            sum(&mut finished_part_area, a.part_area)?;
            sum(&mut recoverable_offcut_area, a.offcut_area)?;
            sum(&mut waste_area, a.waste_area)?;
            sum(&mut kerf_loss_area, a.kerf_loss)?;
            sum(&mut trim_loss_area, a.trim_loss)?;
            for node in witness.tree.nodes() {
                if matches!(node.kind, crate::cut_tree::CutKind::Offcut) {
                    offcuts.push((stock.id, node.rectangle));
                }
            }
        }
        offcuts.sort_by_key(|(id, rect)| (*id, rect.origin.map(crate::units::Length::micrometres)));
        let unused_stock_area = used_stock_area
            .checked_sub(finished_part_area)
            .ok_or(RankingError::AreaOverflow)?;
        let new_spending = match (material, project.cut_fee, cuts) {
            (Some(material), _, 0) => Some(material),
            (Some(material), Some(fee), _) => Some(
                material
                    .checked_add(fee.checked_mul(cuts).map_err(RankingError::Money)?)
                    .map_err(RankingError::Money)?,
            ),
            _ => None,
        };
        ranked.push(RankedCandidate {
            candidate: candidate.clone(),
            cuts,
            utilization: Utilization {
                used_stock_area,
                finished_part_area,
                unused_stock_area,
                recoverable_offcut_area,
                waste_area,
                kerf_loss_area,
                trim_loss_area,
            },
            new_spending,
            offcuts,
        });
    }
    let lowest_spending_claim = objective == Objective::LowestNewSpending
        && !ranked.is_empty()
        && ranked.iter().all(|c| c.new_spending.is_some());
    ranked.sort_by(|a, b| compare(a, b, objective));
    Ok(Ranking {
        candidates: ranked,
        lowest_spending_claim,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate_generation::StockWitness;
    use crate::cut_tree::{Reconstruction, reconstruct_witness};
    use crate::domain::{Allocation, Board, BoardGrain, Material, Stock, StockGrain};
    use crate::money::Currency;
    use crate::units::{Length, Pose, Quaternion};

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }
    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }
    fn brl(n: i64) -> Money {
        Money::new(Currency::Brl, n).unwrap()
    }
    fn fixture() -> Project {
        let mut p = Project::new("ranking", Currency::Brl);
        p.materials.push(Material {
            default_band: None,
            kind: Default::default(),
            id: id(1),
            name: "ply".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Unrestricted,
        });
        p.boards.push(Board {
            banding: Default::default(),
            id: id(2),
            name: "part".into(),
            material_id: id(1),
            length: mm(100),
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
        });
        for (n, length, width, source, price) in [
            (10, 100, 50, StockSource::Owned, None),
            (11, 105, 50, StockSource::Owned, None),
            (12, 110, 50, StockSource::ToPurchase, Some(brl(0))),
            (13, 100, 55, StockSource::ToPurchase, Some(brl(0))),
            (14, 120, 50, StockSource::ToPurchase, None),
        ] {
            p.stock.push(Stock {
                id: id(n),
                name: "stock".into(),
                material_id: id(1),
                length: mm(length),
                width: mm(width),
                thickness: mm(18),
                grain: StockGrain::Nondirectional,
                source,
                price,
                priority: n as u32,
                trim: [Length::ZERO; 4],
            });
        }
        p.cut_fee = Some(brl(1));
        p
    }
    fn candidate(p: &Project, stock_id: Uuid) -> Candidate {
        candidate_at(p, stock_id, Length::ZERO)
    }
    fn candidate_at(p: &Project, stock_id: Uuid, x: Length) -> Candidate {
        let allocations = vec![Allocation {
            id: id(99),
            board_id: id(2),
            stock_id,
            origin: [x, Length::ZERO],
            quarter_turn: false,
            locked: false,
        }];
        let mut copy = p.clone();
        copy.allocations = allocations.clone();
        let Reconstruction::Verified { tree, accounting } =
            reconstruct_witness(&copy, stock_id, p.cutting_kerf, 100)
        else {
            panic!("expected verified witness for {stock_id}");
        };
        Candidate {
            allocations,
            witnesses: vec![StockWitness {
                stock_id,
                tree,
                accounting,
            }],
        }
    }
    fn ids(r: &Ranking) -> Vec<Uuid> {
        r.candidates
            .iter()
            .map(|c| c.candidate.witnesses[0].stock_id)
            .collect()
    }

    #[test]
    fn objectives_and_ties_use_full_stock_area_and_stable_ids() {
        let p = fixture();
        let alternatives: Vec<_> = [13, 12, 11, 10].map(|n| candidate(&p, id(n))).into();
        let spending = rank(&p, &alternatives, Objective::LowestNewSpending).unwrap();
        assert!(spending.lowest_spending_claim);
        // Exact owned stock needs no cuts; the two zero-price purchased
        // pieces have equal cost and unused area, so cuts then IDs decide.
        assert_eq!(ids(&spending), [id(10), id(11), id(12), id(13)]);
        let fewest = rank(&p, &alternatives, Objective::FewestCuts).unwrap();
        assert_eq!(ids(&fewest)[0], id(10));
        let area = rank(&p, &alternatives, Objective::LeastUnusedStockArea).unwrap();
        assert_eq!(ids(&area), [id(10), id(11), id(12), id(13)]);
        assert_eq!(area.candidates[0].utilization.unused_stock_area, 0);
        assert_eq!(
            area.candidates[2].utilization.unused_stock_area,
            500_000_000
        );
        let mut reversed = alternatives.clone();
        reversed.reverse();
        assert_eq!(
            ids(&spending),
            ids(&rank(&p, &reversed, Objective::LowestNewSpending).unwrap())
        );
    }

    #[test]
    fn missing_price_and_fee_cannot_claim_lowest_spending_but_zero_can() {
        let mut p = fixture();
        let alternatives = [candidate(&p, id(14)), candidate(&p, id(12))];
        let result = rank(&p, &alternatives, Objective::LowestNewSpending).unwrap();
        assert_eq!(ids(&result), [id(12), id(14)]);
        assert_eq!(result.candidates[0].new_spending, Some(brl(1)));
        assert_eq!(result.candidates[1].new_spending, None);
        assert!(!result.lowest_spending_claim);
        assert_eq!(
            ids(&rank(&p, &alternatives, Objective::FewestCuts).unwrap()),
            [id(12), id(14)]
        );
        p.stock[4].price = Some(brl(0));
        p.cut_fee = Some(brl(0));
        let complete = rank(&p, &alternatives, Objective::LowestNewSpending).unwrap();
        assert!(complete.lowest_spending_claim);
        assert!(
            complete
                .candidates
                .iter()
                .all(|c| c.new_spending == Some(brl(0)))
        );
        p.cut_fee = None;
        let unknown_fee = rank(&p, &alternatives, Objective::LowestNewSpending).unwrap();
        assert!(!unknown_fee.lowest_spending_claim);
        assert!(
            unknown_fee
                .candidates
                .iter()
                .all(|c| c.new_spending.is_none())
        );
    }

    #[test]
    fn missing_cut_fee_is_irrelevant_to_exact_stock_with_no_cuts() {
        let mut p = fixture();
        p.cut_fee = None;
        let exact = candidate(&p, id(10));
        let cut = candidate(&p, id(12));
        let result = rank(&p, &[cut, exact], Objective::LowestNewSpending).unwrap();
        assert_eq!(result.candidates[0].cuts, 0);
        assert_eq!(result.candidates[0].new_spending, Some(brl(0)));
        assert_eq!(result.candidates[1].new_spending, None);
        assert!(!result.lowest_spending_claim);
    }

    #[test]
    fn offcuts_and_irreversible_losses_reconcile_to_full_root() {
        let mut p = fixture();
        // 120 x 50 contains a 15 x 50 reusable offcut after one 5 mm pass.
        let c = candidate(&p, id(14));
        let summary = &rank(&p, &[c], Objective::LeastUnusedStockArea)
            .unwrap()
            .candidates[0];
        let u = summary.utilization;
        assert_eq!(u.used_stock_area, 6_000_000_000);
        assert_eq!(u.finished_part_area, 5_000_000_000);
        assert_eq!(u.unused_stock_area, 1_000_000_000);
        assert_eq!(u.recoverable_offcut_area, 750_000_000);
        assert_eq!(u.kerf_loss_area, 250_000_000);
        assert_eq!(u.irreversible_loss_area(), 250_000_000);
        assert_eq!(summary.offcuts.len(), 1);
        assert_eq!(summary.offcuts[0].1.extent, [mm(15), mm(50)]);
        p.stock[4].trim = [mm(5), Length::ZERO, Length::ZERO, Length::ZERO];
        // The part now starts after the trim, with a recoverable 10 mm offcut.
        let mut trimmed = p.clone();
        trimmed.boards[0].length = mm(100);
        let mut allocation = candidate(&fixture(), id(14)).allocations[0].clone();
        allocation.origin[0] = mm(5);
        trimmed.allocations = vec![allocation.clone()];
        let Reconstruction::Verified { tree, accounting } =
            reconstruct_witness(&trimmed, id(14), trimmed.cutting_kerf, 100)
        else {
            panic!("trim witness")
        };
        let c = Candidate {
            allocations: vec![allocation],
            witnesses: vec![StockWitness {
                stock_id: id(14),
                tree,
                accounting,
            }],
        };
        let u = rank(&p, &[c], Objective::LeastUnusedStockArea)
            .unwrap()
            .candidates[0]
            .utilization;
        assert_eq!(u.trim_loss_area, 250_000_000);
        assert_eq!(
            u.recoverable_offcut_area + u.irreversible_loss_area(),
            u.unused_stock_area
        );
    }

    #[test]
    fn partial_or_tampered_candidates_are_rejected() {
        let p = fixture();
        assert!(matches!(
            rank(
                &p,
                &[Candidate {
                    allocations: vec![],
                    witnesses: vec![]
                }],
                Objective::FewestCuts
            ),
            Err(RankingError::InvalidCandidate { .. })
        ));
        let mut c = candidate(&p, id(10));
        c.witnesses[0].accounting.part_area = 0;
        assert!(matches!(
            rank(&p, &[c], Objective::LowestNewSpending),
            Err(RankingError::InvalidCandidate { .. })
        ));
    }

    #[test]
    fn cost_and_cut_ties_use_the_documented_secondary_keys() {
        let mut p = fixture();
        let mut stock = p.stock[1].clone();
        stock.id = id(16);
        stock.source = StockSource::ToPurchase;
        stock.price = Some(brl(10));
        p.stock.push(stock);
        let alternatives = [candidate(&p, id(16)), candidate(&p, id(11))];
        assert_eq!(
            ids(&rank(&p, &alternatives, Objective::FewestCuts).unwrap()),
            [id(11), id(16)]
        );
        assert_eq!(
            ids(&rank(&p, &alternatives, Objective::LeastUnusedStockArea).unwrap()),
            [id(11), id(16)]
        );

        // Identical stock roots and purchase prices, but isolation from the
        // middle takes two cuts instead of one. Tie on cost when fee is zero.
        p.stock[2].length = mm(115);
        p.cut_fee = Some(brl(0));
        let one_cut = candidate(&p, id(12));
        let two_cuts = candidate_at(&p, id(12), mm(5));
        let result = rank(&p, &[two_cuts, one_cut], Objective::LowestNewSpending).unwrap();
        assert_eq!(
            result.candidates.iter().map(|c| c.cuts).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(
            result.candidates[0].utilization.unused_stock_area,
            result.candidates[1].utilization.unused_stock_area
        );
    }

    #[test]
    fn equal_stock_plans_break_ties_by_part_id_not_input_order() {
        let mut p = fixture();
        let mut second = p.boards[0].clone();
        second.id = id(3);
        p.boards.push(second);
        p.stock[0].length = mm(205);
        let make = |first_x, second_x| {
            let allocations = [(id(2), first_x), (id(3), second_x)]
                .map(|(board_id, x)| Allocation {
                    id: id(99 + board_id.as_u128()),
                    board_id,
                    stock_id: id(10),
                    origin: [mm(x), Length::ZERO],
                    quarter_turn: false,
                    locked: false,
                })
                .to_vec();
            let mut copy = p.clone();
            copy.allocations = allocations.clone();
            let Reconstruction::Verified { tree, accounting } =
                reconstruct_witness(&copy, id(10), p.cutting_kerf, 100)
            else {
                panic!("two parts")
            };
            Candidate {
                allocations,
                witnesses: vec![StockWitness {
                    stock_id: id(10),
                    tree,
                    accounting,
                }],
            }
        };
        let left = make(0, 105);
        let right = make(105, 0);
        let ranked = rank(&p, &[right.clone(), left.clone()], Objective::FewestCuts).unwrap();
        assert_eq!(ranked.candidates[0].candidate, left);
        assert_eq!(ranked.candidates[1].candidate, right);
        assert_eq!(
            ranked,
            rank(&p, &[left, right], Objective::FewestCuts).unwrap()
        );
    }
}
