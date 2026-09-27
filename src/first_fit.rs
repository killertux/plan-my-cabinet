//! Stable, bounded first-fit of one new physical board. Existing assignments are immutable.
use uuid::Uuid;

use crate::cut_tree::{CutKind, CutTree, Reconstruction, reconstruct_witness};
use crate::domain::{Allocation, BoardGrain, Project, StockGrain};
use crate::units::Length;

const WITNESS_BUDGET: usize = 20_000;
const CANDIDATE_BUDGET: usize = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirstFit {
    Allocated(Uuid),
    NoFit,
    /// The bounded search did not establish feasibility or impossibility.
    SearchExhausted,
}

pub(crate) fn candidate_axis(tree: &CutTree, axis: usize, extent: i64) -> Vec<i64> {
    let usable = tree.node(tree.usable()).expect("usable leaf").rectangle;
    let low = i128::from(usable.origin[axis].micrometres());
    let high = low + i128::from(usable.extent[axis].micrometres());
    let extent = i128::from(extent);
    let mut values = vec![low, high - extent];
    let kerf = i128::from(tree.kerf().micrometres());
    // Offcut boundaries and both sides of each physical cut are meaningful
    // anchors even when prior cuts leave no finished part at that boundary.
    for node in tree.nodes() {
        let r = node.rectangle;
        let start = i128::from(r.origin[axis].micrometres());
        let end = start + i128::from(r.extent[axis].micrometres());
        values.extend([start, end, start - extent, end - extent]);
        if matches!(node.kind, CutKind::Part(_)) {
            values.extend([start - kerf - extent, end + kerf]);
        }
    }
    values.retain(|&v| v >= low && v <= high && v + extent <= high && i64::try_from(v).is_ok());
    values.sort_unstable();
    values.dedup();
    values.into_iter().map(|v| v as i64).collect()
}

/// Attempts only finite, declared stock, in its visible priority order.
/// A failed attempt leaves `project` unchanged. The caller inserts the board
/// and calls this in the same editor transaction.
pub fn allocate_new_board(project: &mut Project, board_id: Uuid) -> FirstFit {
    allocate_with_budget(project, board_id, CANDIDATE_BUDGET, WITNESS_BUDGET)
}

fn allocate_with_budget(
    project: &mut Project,
    board_id: Uuid,
    candidate_budget: usize,
    witness_budget: usize,
) -> FirstFit {
    let Some(board) = project.boards.iter().find(|b| b.id == board_id) else {
        return FirstFit::NoFit;
    };
    let Some(material) = project.materials.iter().find(|m| m.id == board.material_id) else {
        return FirstFit::NoFit;
    };
    let grain = board.effective_grain(material);
    let (material_id, thickness, dimensions) = (
        board.material_id,
        board.thickness,
        [board.length, board.width],
    );
    let ordered: Vec<_> = project.ordered_stock().iter().map(|s| s.id).collect();
    let mut exhausted = false;
    let mut attempts = 0;
    for stock_id in ordered {
        let Some(stock) = project.stock_piece(stock_id) else {
            continue;
        };
        if stock.material_id != material_id || stock.thickness != thickness {
            continue;
        }
        let stock_grain = stock.grain;
        let tree =
            match reconstruct_witness(project, stock_id, project.cutting_kerf, witness_budget) {
                Reconstruction::Verified { tree, .. } => tree,
                Reconstruction::BudgetExhausted => {
                    exhausted = true;
                    continue;
                }
                Reconstruction::RuleViolation(_) => continue, // existing draft placements remain untouched
            };
        for quarter_turn in [false, true] {
            let required = match grain {
                BoardGrain::Unrestricted => None,
                BoardGrain::Length => Some(if quarter_turn {
                    StockGrain::AlongY
                } else {
                    StockGrain::AlongX
                }),
                BoardGrain::Width => Some(if quarter_turn {
                    StockGrain::AlongX
                } else {
                    StockGrain::AlongY
                }),
            };
            if required.is_some_and(|axis| {
                stock_grain != StockGrain::Nondirectional && stock_grain != axis
            }) {
                continue;
            }
            let extent = if quarter_turn {
                [dimensions[1], dimensions[0]]
            } else {
                dimensions
            };
            let xs = candidate_axis(&tree, 0, extent[0].micrometres());
            let ys = candidate_axis(&tree, 1, extent[1].micrometres());
            for y in ys {
                for &x in &xs {
                    if attempts == candidate_budget {
                        return FirstFit::SearchExhausted;
                    }
                    attempts += 1;
                    let allocation = Allocation {
                        id: Uuid::new_v4(),
                        board_id,
                        stock_id,
                        origin: [Length::from_micrometres(x), Length::from_micrometres(y)],
                        quarter_turn,
                        locked: false,
                    };
                    project.allocations.push(allocation);
                    let outcome = reconstruct_witness(
                        project,
                        stock_id,
                        project.cutting_kerf,
                        witness_budget,
                    );
                    if matches!(outcome, Reconstruction::Verified { .. }) {
                        return FirstFit::Allocated(stock_id);
                    }
                    exhausted |= matches!(outcome, Reconstruction::BudgetExhausted);
                    project.allocations.pop();
                }
            }
        }
    }
    if exhausted {
        FirstFit::SearchExhausted
    } else {
        FirstFit::NoFit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board_commands::{NewBoard, NewMaterial};
    use crate::commands::ProjectEditor;
    use crate::domain::{DEFAULT_CUTTING_KERF, StockSource};
    use crate::money::Currency;
    use crate::stock_commands::StockInput;
    use crate::units::{Pose, Quaternion};

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }
    fn fixture() -> (ProjectEditor, Uuid) {
        let mut editor = ProjectEditor::new(Project::new("fixture", Currency::Brl)).unwrap();
        let material = editor
            .create_material(NewMaterial {
                name: "ply".into(),
                thickness: mm(18),
                grain: BoardGrain::Length,
            })
            .unwrap();
        (editor, material)
    }
    fn stock(
        editor: &mut ProjectEditor,
        material_id: Uuid,
        length: i64,
        width: i64,
        grain: StockGrain,
        source: StockSource,
    ) -> Uuid {
        editor
            .create_stock(
                StockInput {
                    name: "sheet".into(),
                    material_id,
                    length: mm(length),
                    width: mm(width),
                    thickness: mm(18),
                    grain,
                    source,
                    price: None,
                    trim: [Length::ZERO; 4],
                },
                1,
            )
            .unwrap()[0]
    }
    fn board(
        editor: &mut ProjectEditor,
        material_id: Uuid,
        length: i64,
        width: i64,
    ) -> (Uuid, FirstFit) {
        editor
            .create_board_with_fit(NewBoard {
                name: "part".into(),
                material_id,
                length: mm(length),
                width: mm(width),
                pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap()
    }

    #[test]
    fn priority_and_existing_positions_are_stable_even_when_unlocked() {
        let (mut e, m) = fixture();
        let first = stock(&mut e, m, 100, 50, StockGrain::AlongX, StockSource::Owned);
        let second = stock(
            &mut e,
            m,
            105,
            50,
            StockGrain::AlongX,
            StockSource::ToPurchase,
        );
        let (_, fit) = board(&mut e, m, 100, 50);
        assert_eq!(fit, FirstFit::Allocated(first));
        let old = e.project().allocations.clone();
        let (id, fit) = board(&mut e, m, 100, 50);
        assert_eq!(fit, FirstFit::Allocated(second));
        assert_eq!(&e.project().allocations[..1], old.as_slice());
        assert_eq!(e.project().allocations[1].board_id, id);
        assert_eq!(e.project().stock.len(), 2);
        e.undo().unwrap();
        assert_eq!(e.project().allocations, old);
        e.redo().unwrap();
        assert_eq!(e.project().allocations[1].stock_id, second);
    }

    #[test]
    fn cut_edges_and_grain_restriction_and_duplicate() {
        let (mut e, m) = fixture();
        let s = stock(&mut e, m, 205, 50, StockGrain::AlongX, StockSource::Owned);
        let (original, _) = board(&mut e, m, 100, 50);
        let initial = e.project().allocations[0].clone();
        let (copy, fit) = e
            .duplicate_board_with_fit(original, Pose::new([0.; 3], Quaternion::IDENTITY).unwrap())
            .unwrap();
        assert_eq!(fit, FirstFit::Allocated(s));
        assert_ne!(copy, original);
        assert_eq!(e.project().allocations[0], initial);
        assert_eq!(e.project().allocations[1].origin, [mm(105), Length::ZERO]);
        assert!(matches!(
            reconstruct_witness(e.project(), s, mm(5), 1000),
            Reconstruction::Verified { .. }
        ));

        let (mut e, m) = fixture();
        let s = stock(&mut e, m, 50, 100, StockGrain::AlongX, StockSource::Owned);
        let (_, fit) = board(&mut e, m, 100, 50);
        assert_eq!(fit, FirstFit::NoFit);
        assert!(e.project().allocations.is_empty());
        e.edit_stock(
            s,
            StockInput {
                grain: StockGrain::Nondirectional,
                ..StockInput::from(&e.project().stock[0])
            },
        )
        .unwrap();
        let (_, fit) = board(&mut e, m, 100, 50);
        assert_eq!(fit, FirstFit::Allocated(s));
        assert!(e.project().allocations[0].quarter_turn);
    }

    #[test]
    fn visible_priority_beats_ownership_and_unknown_grain_is_not_free() {
        let (mut e, m) = fixture();
        let purchase = stock(
            &mut e,
            m,
            100,
            50,
            StockGrain::AlongX,
            StockSource::ToPurchase,
        );
        let owned = stock(&mut e, m, 100, 50, StockGrain::AlongX, StockSource::Owned);
        e.reorder_stock(owned, 0).unwrap();
        let (_, fit) = board(&mut e, m, 100, 50);
        assert_eq!(fit, FirstFit::Allocated(owned));
        let (_, fit) = board(&mut e, m, 100, 50);
        assert_eq!(fit, FirstFit::Allocated(purchase));
        let (mut e, m) = fixture();
        stock(&mut e, m, 100, 50, StockGrain::Unknown, StockSource::Owned);
        let (_, fit) = board(&mut e, m, 100, 50);
        assert_eq!(fit, FirstFit::NoFit);
        assert!(e.project().allocations.is_empty());
    }

    #[test]
    fn locked_placement_and_incompatible_stock_remain_untouched() {
        let (mut e, m) = fixture();
        let wrong = e
            .create_material(NewMaterial {
                name: "other".into(),
                thickness: mm(18),
                grain: BoardGrain::Unrestricted,
            })
            .unwrap();
        stock(
            &mut e,
            wrong,
            500,
            500,
            StockGrain::Nondirectional,
            StockSource::Owned,
        );
        let source = stock(&mut e, m, 205, 50, StockGrain::AlongX, StockSource::Owned);
        let (id, _) = board(&mut e, m, 100, 50);
        e.transact(|p| -> Result<(), ()> {
            p.allocations
                .iter_mut()
                .find(|a| a.board_id == id)
                .unwrap()
                .locked = true;
            Ok(())
        })
        .unwrap();
        let original = e.project().allocations[0].clone();
        let (_, fit) = board(&mut e, m, 100, 50);
        assert_eq!(fit, FirstFit::Allocated(source));
        assert_eq!(e.project().allocations[0], original);
    }

    #[test]
    fn insufficient_stock_and_kerf_round_trip() {
        let (mut e, m) = fixture();
        stock(&mut e, m, 105, 50, StockGrain::AlongX, StockSource::Owned);
        let (_, first) = board(&mut e, m, 100, 50);
        assert!(matches!(first, FirstFit::Allocated(_)));
        let (id, second) = board(&mut e, m, 100, 50);
        assert_eq!(second, FirstFit::NoFit);
        assert!(e.project().boards.iter().any(|b| b.id == id));
        assert_eq!(e.project().allocations.len(), 1);
        assert_eq!(e.project().cutting_kerf, DEFAULT_CUTTING_KERF);
        e.set_cutting_kerf(mm(4)).unwrap();
        let json = serde_json::to_value(e.project()).unwrap();
        let loaded: Project = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(loaded.cutting_kerf, mm(4));
        let mut legacy = json;
        legacy.as_object_mut().unwrap().remove("cutting_kerf");
        let old: Project = serde_json::from_value(legacy).unwrap();
        assert_eq!(old.cutting_kerf, DEFAULT_CUTTING_KERF);
        assert!(e.set_cutting_kerf(Length::ZERO).is_err());
        assert_eq!(e.project().cutting_kerf, mm(4));
    }

    #[test]
    fn exhausted_search_is_unknown_and_never_commits_a_trial() {
        let (mut e, m) = fixture();
        stock(&mut e, m, 205, 50, StockGrain::AlongX, StockSource::Owned);
        let (id, _) = board(&mut e, m, 100, 50);
        let original = e.project().allocations.clone();
        let mut project = e.project().clone();
        let mut duplicate = project.boards.iter().find(|b| b.id == id).unwrap().clone();
        let new_id = Uuid::new_v4();
        duplicate.id = new_id;
        project.boards.push(duplicate);
        assert_eq!(
            allocate_with_budget(&mut project, new_id, 0, 1000),
            FirstFit::SearchExhausted
        );
        assert_eq!(project.allocations, original);
        assert_eq!(
            allocate_with_budget(&mut project, new_id, 100, 0),
            FirstFit::SearchExhausted
        );
        assert_eq!(project.allocations, original);
    }
}
