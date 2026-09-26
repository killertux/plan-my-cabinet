//! Estimate new spending from current, independently verified physical-stock witnesses.
use uuid::Uuid;

use crate::allocation_diagnostics::WITNESS_BUDGET;
use crate::cut_tree::{Reconstruction, reconstruct_witness};
use crate::domain::{DomainError, Project, StockSource};
use crate::money::{Money, MoneyError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feasibility {
    Verified,
    Incomplete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StockExpense {
    pub stock_id: Uuid,
    pub source: StockSource,
    pub purchase: Option<Money>,
    pub cuts: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectEstimate {
    pub used_stock: Vec<StockExpense>,
    pub feasibility: Feasibility,
    pub material: Option<Money>,
    pub cutting: Option<Money>,
    /// Only present for a complete, feasible allocation with all required amounts known.
    pub total: Option<Money>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EstimateError {
    InvalidProject(DomainError),
    Money(MoneyError),
}

/// Charges the full price of each used to-purchase piece, and each witnessed split
/// (including trims) on both purchased and owned pieces exactly once.
pub fn estimate(project: &Project) -> Result<ProjectEstimate, EstimateError> {
    project.validate().map_err(EstimateError::InvalidProject)?;
    let zero = Money::new(project.currency, 0).map_err(EstimateError::Money)?;
    let mut material = Some(zero);
    let mut cuts = Some(0_u64);
    let mut used_stock = Vec::new();
    for stock in project.ordered_stock() {
        if !project.allocations.iter().any(|a| a.stock_id == stock.id) {
            continue;
        }
        let purchase = if stock.source == StockSource::Owned {
            Some(zero)
        } else {
            stock.price
        };
        material = match (material, purchase) {
            (Some(sum), Some(price)) => Some(sum.checked_add(price).map_err(EstimateError::Money)?),
            _ => None,
        };
        let sheet_cuts =
            match reconstruct_witness(project, stock.id, project.cutting_kerf, WITNESS_BUDGET) {
                Reconstruction::Verified { tree, .. } => Some(
                    u64::try_from(tree.cut_count())
                        .map_err(|_| EstimateError::Money(MoneyError::Overflow))?,
                ),
                _ => None,
            };
        cuts = match (cuts, sheet_cuts) {
            (Some(sum), Some(count)) => Some(
                sum.checked_add(count)
                    .ok_or(EstimateError::Money(MoneyError::Overflow))?,
            ),
            _ => None,
        };
        used_stock.push(StockExpense {
            stock_id: stock.id,
            source: stock.source,
            purchase,
            cuts: sheet_cuts,
        });
    }
    let feasibility = if project.allocations.len() == project.boards.len() && cuts.is_some() {
        Feasibility::Verified
    } else {
        Feasibility::Incomplete
    };
    let cutting = match cuts {
        Some(0) => Some(zero),
        Some(count) => project
            .cut_fee
            .map(|fee| fee.checked_mul(count))
            .transpose()
            .map_err(EstimateError::Money)?,
        None => None,
    };
    let total = if feasibility == Feasibility::Verified {
        match (material, cutting) {
            (Some(a), Some(b)) => Some(a.checked_add(b).map_err(EstimateError::Money)?),
            _ => None,
        }
    } else {
        None
    };
    Ok(ProjectEstimate {
        used_stock,
        feasibility,
        material,
        cutting,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{EditError, ProjectEditor};
    use crate::domain::{Allocation, Board, BoardGrain, Material, Stock, StockGrain};
    use crate::persistence::{prepare_bytes, serialize};
    use crate::units::{Length, Pose, Quaternion};

    fn mm(value: i64) -> Length {
        Length::from_micrometres(value * 1000)
    }
    fn brl(value: i64) -> Money {
        Money::new(crate::money::Currency::Brl, value).unwrap()
    }

    fn fixture() -> Project {
        let mut p = Project::new("Cut costs", crate::money::Currency::Brl);
        let material = Material {
            id: Uuid::new_v4(),
            name: "plywood".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Unrestricted,
        };
        for (source, stock_size, part_size, trim, price) in [
            (StockSource::Owned, 100, 90, [mm(5); 4], None),
            (
                StockSource::ToPurchase,
                110,
                100,
                [Length::ZERO; 4],
                Some(brl(20_000)),
            ),
        ] {
            let stock = Stock {
                id: Uuid::new_v4(),
                name: "stock".into(),
                material_id: material.id,
                length: mm(stock_size),
                width: mm(stock_size),
                thickness: mm(18),
                grain: StockGrain::Nondirectional,
                source,
                price,
                priority: p.stock.len() as u32,
                trim,
            };
            let board = Board {
                id: Uuid::new_v4(),
                name: "part".into(),
                material_id: material.id,
                length: mm(part_size),
                width: mm(part_size),
                thickness: mm(18),
                grain_override: None,
                parent_id: None,
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            };
            p.allocations.push(Allocation {
                id: Uuid::new_v4(),
                board_id: board.id,
                stock_id: stock.id,
                origin: [trim[0], trim[2]],
                quarter_turn: false,
                locked: false,
            });
            p.boards.push(board);
            p.stock.push(stock);
        }
        p.materials.push(material);
        p.cut_fee = Some(brl(500));
        p
    }

    #[test]
    fn purchase_plus_six_verified_cuts_including_owned_trim_passes() {
        let p = fixture();
        let result = estimate(&p).unwrap();
        assert_eq!(result.feasibility, Feasibility::Verified);
        assert_eq!(
            result
                .used_stock
                .iter()
                .map(|s| s.cuts.unwrap())
                .sum::<u64>(),
            6
        );
        assert_eq!(result.used_stock[0].cuts, Some(4));
        assert_eq!(result.used_stock[0].purchase, Some(brl(0)));
        assert_eq!(result.used_stock[1].cuts, Some(2));
        assert_eq!(result.material, Some(brl(20_000)));
        assert_eq!(result.cutting, Some(brl(3_000)));
        assert_eq!(result.total, Some(brl(23_000)));
    }

    #[test]
    fn missing_amounts_are_not_zero_but_explicit_zero_is_complete() {
        let mut p = fixture();
        p.stock[1].price = None;
        let result = estimate(&p).unwrap();
        assert_eq!(result.material, None);
        assert_eq!(result.cutting, Some(brl(3_000)));
        assert_eq!(result.total, None);
        p.stock[1].price = Some(brl(0));
        p.cut_fee = None;
        assert_eq!(estimate(&p).unwrap().cutting, None);
        assert_eq!(estimate(&p).unwrap().total, None);
        p.cut_fee = Some(brl(0));
        assert_eq!(estimate(&p).unwrap().total, Some(brl(0)));
    }

    #[test]
    fn incomplete_or_conflicted_allocation_cannot_have_complete_total() {
        let mut p = fixture();
        p.allocations.pop();
        let partial = estimate(&p).unwrap();
        assert_eq!(partial.feasibility, Feasibility::Incomplete);
        assert_eq!(partial.total, None);
        p.allocations.push(Allocation {
            id: Uuid::new_v4(),
            board_id: p.boards[1].id,
            stock_id: p.stock[1].id,
            origin: [mm(7), Length::ZERO],
            quarter_turn: false,
            locked: false,
        });
        let conflict = estimate(&p).unwrap();
        assert_eq!(conflict.feasibility, Feasibility::Incomplete);
        assert_eq!(conflict.used_stock[1].cuts, None);
        assert_eq!(conflict.total, None);
        let mut changed_kerf = fixture();
        changed_kerf.cutting_kerf = mm(6);
        assert_eq!(estimate(&changed_kerf).unwrap().total, None);
    }

    #[test]
    fn invalid_currency_and_checked_overflow() {
        let mut p = fixture();
        p.cut_fee = Some(Money::new(crate::money::Currency::Usd, 500).unwrap());
        assert_eq!(
            estimate(&p),
            Err(EstimateError::InvalidProject(DomainError::InvalidCutFee))
        );
        p.cut_fee = Some(brl(i64::MAX));
        assert_eq!(
            estimate(&p),
            Err(EstimateError::Money(MoneyError::Overflow))
        );
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        assert_eq!(
            editor.set_cut_fee(
                p.cut_fee
                    .map(|_| Money::new(crate::money::Currency::Usd, 1).unwrap())
            ),
            Err(EditError::Command(MoneyError::CurrencyMismatch))
        );
        assert_eq!(editor.project().cut_fee, Some(brl(500)));
        editor.set_cut_fee(Some(brl(0))).unwrap();
        assert_eq!(editor.project().cut_fee, Some(brl(0)));
        editor.undo().unwrap();
        assert_eq!(editor.project().cut_fee, Some(brl(500)));
    }

    #[test]
    fn fee_survives_save_reopen_and_old_documents_default_to_unknown() {
        let p = fixture();
        let reopened = prepare_bytes(&serialize(&p).unwrap()).unwrap();
        assert_eq!(reopened.project().cut_fee, Some(brl(500)));
        assert_eq!(
            estimate(reopened.project()).unwrap().total,
            Some(brl(23_000))
        );
        let mut old: serde_json::Value = serde_json::from_slice(&serialize(&p).unwrap()).unwrap();
        old.as_object_mut().unwrap().remove("cut_fee");
        let reopened = prepare_bytes(&serde_json::to_vec(&old).unwrap()).unwrap();
        assert_eq!(reopened.project().cut_fee, None);
        assert_eq!(estimate(reopened.project()).unwrap().total, None);
    }
}
