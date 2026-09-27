use plan_my_cabinet::allocation_diagnostics::{Reason, Status};
use plan_my_cabinet::cost_estimate::Feasibility;
use plan_my_cabinet::domain::{Project, StockGrain, StockSource};
use plan_my_cabinet::money::{Currency, Money};
use plan_my_cabinet::reference_fixture::{
    self, BACK_ID, HDF_ID, OAK_STOCK_ID, OWNED_STOCK_ID, SPARE_STOCK_ID, WHITE_STOCK_ID,
};
use plan_my_cabinet::stock_read_models::{ReadModelError, SheetProof, StockReadModel};
use plan_my_cabinet::units::Length;

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}

#[test]
fn reference_rows_cards_and_miniature_share_actual_proofs_and_costs() {
    let p = reference_fixture::project();
    let model = StockReadModel::build(&p).unwrap();
    let ids = [WHITE_STOCK_ID, OWNED_STOCK_ID, SPARE_STOCK_ID, OAK_STOCK_ID];
    assert_eq!(model.pieces.iter().map(|s| s.id).collect::<Vec<_>>(), ids);
    assert_eq!(model.table_rows(None).len(), model.sheet_cards().len());
    for (index, card) in model.sheet_cards().iter().enumerate() {
        let row = model.table_rows(None)[index];
        let miniature = model.miniature(card.id).unwrap();
        assert!(std::ptr::eq(card, row));
        assert!(std::ptr::eq(card, miniature));
        assert_eq!(card.global_rank, index + 1);
        assert_eq!(
            card.parts.len(),
            p.allocations
                .iter()
                .filter(|a| a.stock_id == card.id)
                .count()
        );
        assert_eq!(card.alias, ["S1", "O1", "S3", "S2"][index]);
    }
    let white = model.miniature(WHITE_STOCK_ID).unwrap();
    let oak = model.miniature(OAK_STOCK_ID).unwrap();
    assert_eq!(white.parts.len(), 6);
    assert_eq!(white.parts[0].effective_thickness, mm(18));
    assert_eq!(white.proof.cut_count(), Some(9));
    assert_eq!(oak.proof.cut_count(), Some(4));
    assert_eq!(
        white.proof.utilization(),
        Some((1_797_308_000_000, 5_032_500_000_000))
    );
    assert_eq!(
        oak.proof.utilization(),
        Some((568_504_000_000, 5_032_500_000_000))
    );
    for piece in [
        model.miniature(OWNED_STOCK_ID).unwrap(),
        model.miniature(SPARE_STOCK_ID).unwrap(),
    ] {
        assert_eq!(piece.proof, SheetProof::Unused);
        assert_eq!(piece.proof.cut_count(), None);
        assert_eq!(piece.proof.utilization(), None);
        assert!(piece.parts.is_empty());
    }
    let owned = model.miniature(OWNED_STOCK_ID).unwrap();
    assert_eq!(owned.grain, StockGrain::Unknown);
    assert_eq!(owned.measured_thickness, Length::from_micrometres(18_200));
    assert_eq!(owned.material_default_thickness, mm(18));
    assert_eq!(owned.price, None);
    assert_eq!(model.estimate.used_stock.len(), 2);
    assert_eq!(
        model.estimate.material,
        Some(Money::new(Currency::Brl, 57_980).unwrap())
    );
    assert_eq!(model.estimate.cutting, None);
    assert_eq!(model.estimate.total, None);
    assert_eq!(model.estimate.feasibility, Feasibility::Incomplete);
    assert_eq!(model.usage_summary().used_purchased_pieces, 2);
    assert_eq!(model.usage_summary().consumed_owned_pieces, 0);
    assert_eq!(model.usage_summary().physical_cuts, Some(13));
    assert_eq!(model.cut_fee, None);
    assert_eq!(
        model
            .materials
            .iter()
            .find(|m| m.id == HDF_ID)
            .unwrap()
            .unallocated_board_count,
        1
    );
    assert_eq!(
        model
            .boards
            .iter()
            .find(|b| b.board_id == BACK_ID)
            .unwrap()
            .status,
        Status::Unallocated
    );
}

#[test]
fn empty_invalid_and_unknown_price_are_not_zero_or_verified() {
    let empty = StockReadModel::build(&Project::new("Empty", Currency::Brl)).unwrap();
    assert!(empty.pieces.is_empty());
    assert!(empty.materials.is_empty());
    assert!(empty.boards.is_empty());
    assert_eq!(
        empty.estimate.total,
        Some(Money::new(Currency::Brl, 0).unwrap())
    );

    let mut p = reference_fixture::project();
    p.allocations[1].origin = p.allocations[0].origin;
    p.stock
        .iter_mut()
        .find(|s| s.id == WHITE_STOCK_ID)
        .unwrap()
        .price = None;
    p.cut_fee = Some(Money::new(Currency::Brl, 0).unwrap());
    let conflicted = StockReadModel::build(&p).unwrap();
    assert!(matches!(
        conflicted.miniature(WHITE_STOCK_ID).unwrap().proof,
        SheetProof::Violation(_)
    ));
    assert_eq!(
        conflicted
            .miniature(WHITE_STOCK_ID)
            .unwrap()
            .proof
            .cut_count(),
        None
    );
    assert_eq!(
        conflicted
            .miniature(WHITE_STOCK_ID)
            .unwrap()
            .proof
            .utilization(),
        None
    );
    assert_eq!(conflicted.estimate.material, None);
    assert_eq!(conflicted.estimate.total, None);
    assert_eq!(conflicted.usage_summary().physical_cuts, None);
    assert!(
        conflicted
            .boards
            .iter()
            .any(|b| b.reasons.contains(&Reason::Overlap))
    );
    assert_eq!(
        conflicted
            .miniature(OAK_STOCK_ID)
            .unwrap()
            .proof
            .cut_count(),
        Some(4)
    );

    p.allocations[1].origin = reference_fixture::project().allocations[1].origin;
    p.stock
        .iter_mut()
        .find(|s| s.id == WHITE_STOCK_ID)
        .unwrap()
        .price = Some(Money::new(Currency::Brl, 0).unwrap());
    let zero = StockReadModel::build(&p).unwrap();
    assert_eq!(
        zero.estimate.material,
        Some(Money::new(Currency::Brl, 28_990).unwrap())
    );
    assert_eq!(
        zero.estimate.cutting,
        Some(Money::new(Currency::Brl, 0).unwrap())
    );
    assert_eq!(zero.estimate.total, None); // back is still unallocated

    p.stock[0].length = Length::ZERO;
    assert!(matches!(
        StockReadModel::build(&p),
        Err(ReadModelError::InvalidProject(_))
    ));
}

#[test]
fn count_and_identity_invariants_hold_across_orders_and_allocation_subsets() {
    // Exercise every prefix and all cyclic physical priorities without relying on
    // rendered counts or recomputing a separate card/miniature interpretation.
    for shift in 0..4 {
        for retained in 0..=8 {
            let mut p = reference_fixture::project();
            p.allocations.truncate(retained);
            for (index, piece) in p.stock.iter_mut().enumerate() {
                piece.priority = ((index + shift) % 4) as u32;
            }
            let model = StockReadModel::build(&p).unwrap();
            assert_eq!(model.pieces.len(), p.stock.len());
            assert_eq!(
                model.pieces.iter().map(|s| s.parts.len()).sum::<usize>(),
                retained
            );
            assert_eq!(
                model.materials.iter().map(|m| m.board_count).sum::<usize>(),
                p.boards.len()
            );
            assert_eq!(
                model
                    .materials
                    .iter()
                    .map(|m| m.stock_piece_count)
                    .sum::<usize>(),
                p.stock.len()
            );
            assert_eq!(model.boards.len(), p.boards.len());
            for material in &model.materials {
                assert_eq!(
                    model.table_rows(Some(material.id)).len(),
                    material.stock_piece_count
                );
            }
            for (rank, card) in model.sheet_cards().iter().enumerate() {
                assert_eq!(card.global_rank, rank + 1);
                assert!(std::ptr::eq(card, model.miniature(card.id).unwrap()));
                if !card.is_used() {
                    assert_eq!(card.proof, SheetProof::Unused);
                }
            }
            assert_eq!(
                model.estimate.used_stock.len(),
                model.pieces.iter().filter(|s| s.is_used()).count()
            );
        }
    }
}

#[test]
fn source_ownership_and_grain_do_not_change_persisted_alias() {
    let mut p = reference_fixture::project();
    let first = StockReadModel::build(&p).unwrap();
    let alias = first.miniature(WHITE_STOCK_ID).unwrap().alias.clone();
    p.stock_aliases = first
        .pieces
        .iter()
        .map(|piece| (piece.id, piece.alias.clone()))
        .collect();
    p.next_stock_s_alias = 4;
    p.next_stock_o_alias = 2;
    p.stock[0].source = StockSource::Owned;
    p.stock[0].grain = StockGrain::Unknown;
    let changed = StockReadModel::build(&p).unwrap();
    assert_eq!(changed.miniature(WHITE_STOCK_ID).unwrap().alias, alias);
    assert_eq!(
        changed.miniature(WHITE_STOCK_ID).unwrap().source,
        StockSource::Owned
    );
    assert_eq!(
        changed.miniature(WHITE_STOCK_ID).unwrap().grain,
        StockGrain::Unknown
    );
}

#[test]
fn effective_board_thickness_and_usable_trim_are_independent_of_material_defaults() {
    let mut p = reference_fixture::project();
    let white_id = p.stock[0].material_id;
    p.materials
        .iter_mut()
        .find(|material| material.id == white_id)
        .unwrap()
        .default_thickness = mm(19);
    let spare = p.stock.iter_mut().find(|s| s.id == SPARE_STOCK_ID).unwrap();
    spare.trim = [mm(5), mm(10), mm(15), mm(20)];
    let model = StockReadModel::build(&p).unwrap();
    let white = model.miniature(WHITE_STOCK_ID).unwrap();
    assert_eq!(white.material_default_thickness, mm(19));
    assert!(
        white
            .parts
            .iter()
            .all(|part| part.effective_thickness == mm(18))
    );
    let spare = model.miniature(SPARE_STOCK_ID).unwrap();
    assert_eq!(spare.usable_extent, [mm(2735), mm(1795)]);
    assert_eq!(spare.proof, SheetProof::Unused);
    assert_eq!(spare.proof.utilization(), None);
}
