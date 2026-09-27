//! Project-local physical stock creation, correction and user-defined ordering.
use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::{Project, Stock, StockGrain, StockSource};
use crate::money::{Currency, CurrencyChange, Money, MoneyError, ProjectMoney};
use crate::units::Length;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StockField {
    Material,
    Length,
    Width,
    Thickness,
    Trim,
    Price,
    Quantity,
    Priority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StockError {
    MissingStock(Uuid),
    MissingMaterial(Uuid),
    Invalid(StockField),
    PriorityExhausted,
    AllocatedStock(Uuid),
}

/// The replacement form names every physical piece by identity. An unknown
/// price is `None`; `Some(0)` is an explicitly known free piece.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectCurrencyChange {
    ConfirmRelabelWithoutConversion,
    Replace {
        cut_fee: Option<Money>,
        stock_prices: Vec<(Uuid, Option<Money>)>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrencyChangeError {
    DuplicateStock(Uuid),
    UnknownStock(Uuid),
    MissingStock(Uuid),
    Money(MoneyError),
}

impl From<MoneyError> for CurrencyChangeError {
    fn from(error: MoneyError) -> Self {
        Self::Money(error)
    }
}

/// Prevent a single user action from allocating an unbounded number of records.
pub const MAX_STOCK_QUANTITY: u32 = 10_000;

#[derive(Clone, Debug)]
pub struct StockInput {
    pub name: String,
    pub material_id: Uuid,
    pub length: Length,
    pub width: Length,
    pub thickness: Length,
    pub grain: StockGrain,
    pub source: StockSource,
    pub price: Option<Money>,
    /// Total loss from left, right, bottom and top edges.
    pub trim: [Length; 4],
}

fn validate(project: &Project, input: &StockInput) -> Result<(), StockError> {
    if !project.materials.iter().any(|m| m.id == input.material_id) {
        return Err(StockError::MissingMaterial(input.material_id));
    }
    for (value, field) in [
        (input.length, StockField::Length),
        (input.width, StockField::Width),
        (input.thickness, StockField::Thickness),
    ] {
        if value.micrometres() <= 0 {
            return Err(StockError::Invalid(field));
        }
    }
    if input.trim.iter().any(|v| v.micrometres() < 0)
        || i128::from(input.trim[0].micrometres()) + i128::from(input.trim[1].micrometres())
            >= i128::from(input.length.micrometres())
        || i128::from(input.trim[2].micrometres()) + i128::from(input.trim[3].micrometres())
            >= i128::from(input.width.micrometres())
    {
        return Err(StockError::Invalid(StockField::Trim));
    }
    if input
        .price
        .is_some_and(|p| p.currency() != project.currency || p.minor_units() < 0)
    {
        return Err(StockError::Invalid(StockField::Price));
    }
    Ok(())
}

fn from_input(id: Uuid, priority: u32, input: StockInput) -> Stock {
    Stock {
        id,
        priority,
        name: input.name,
        material_id: input.material_id,
        length: input.length,
        width: input.width,
        thickness: input.thickness,
        grain: input.grain,
        source: input.source,
        price: input.price,
        trim: input.trim,
    }
}

impl From<&Stock> for StockInput {
    fn from(piece: &Stock) -> Self {
        Self {
            name: piece.name.clone(),
            material_id: piece.material_id,
            length: piece.length,
            width: piece.width,
            thickness: piece.thickness,
            grain: piece.grain,
            source: piece.source,
            price: piece.price,
            trim: piece.trim,
        }
    }
}

impl ProjectEditor {
    /// Change the project's currency and all known amounts in one undo step.
    /// Relabel requires explicit confirmation and retains minor-unit numbers;
    /// replacement requires one UUID-keyed entry per piece, including unknowns.
    /// Neither decision performs an exchange-rate conversion.
    pub fn change_project_currency(
        &mut self,
        target: Currency,
        decision: ProjectCurrencyChange,
    ) -> Result<bool, EditError<CurrencyChangeError>> {
        let project = self.project();
        let mut ledger = ProjectMoney::new(project.currency, project.stock.len());
        ledger
            .set_cut_charge(project.cut_fee)
            .map_err(CurrencyChangeError::from)
            .map_err(EditError::Command)?;
        for (index, piece) in project.stock.iter().enumerate() {
            ledger
                .set_stock_price(index, piece.price)
                .map_err(CurrencyChangeError::from)
                .map_err(EditError::Command)?;
        }
        let decision = match decision {
            ProjectCurrencyChange::ConfirmRelabelWithoutConversion => {
                CurrencyChange::ConfirmRelabelWithoutConversion
            }
            ProjectCurrencyChange::Replace {
                cut_fee,
                stock_prices,
            } => {
                let mut replacements = HashMap::with_capacity(stock_prices.len());
                let known_ids: HashSet<_> = project.stock.iter().map(|piece| piece.id).collect();
                for (id, price) in stock_prices {
                    if !known_ids.contains(&id) {
                        return Err(EditError::Command(CurrencyChangeError::UnknownStock(id)));
                    }
                    if replacements.insert(id, price).is_some() {
                        return Err(EditError::Command(CurrencyChangeError::DuplicateStock(id)));
                    }
                }
                let prices = project
                    .stock
                    .iter()
                    .map(|piece| {
                        replacements
                            .get(&piece.id)
                            .copied()
                            .ok_or(EditError::Command(CurrencyChangeError::MissingStock(
                                piece.id,
                            )))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                CurrencyChange::Replace {
                    cut_charge: cut_fee,
                    stock_prices: prices,
                }
            }
        };
        ledger
            .change_currency(target, decision)
            .map_err(CurrencyChangeError::from)
            .map_err(EditError::Command)?;
        self.transact(|project| -> Result<(), CurrencyChangeError> {
            project.currency = target;
            project.cut_fee = ledger.cut_charge();
            for (piece, price) in project.stock.iter_mut().zip(ledger.stock_prices()) {
                piece.price = *price;
            }
            Ok(())
        })
    }

    /// One transaction for all copies; each returned ID denotes one physical piece.
    pub fn create_stock(
        &mut self,
        input: StockInput,
        quantity: u32,
    ) -> Result<Vec<Uuid>, EditError<StockError>> {
        if quantity == 0 || quantity > MAX_STOCK_QUANTITY {
            return Err(EditError::Command(StockError::Invalid(
                StockField::Quantity,
            )));
        }
        validate(self.project(), &input).map_err(EditError::Command)?;
        let count = u32::try_from(self.project().stock.len())
            .map_err(|_| EditError::Command(StockError::PriorityExhausted))?;
        count
            .checked_add(quantity)
            .ok_or(EditError::Command(StockError::PriorityExhausted))?;
        let ids: Vec<_> = (0..quantity).map(|_| Uuid::new_v4()).collect();
        self.transact(|project| {
            // Normalize earlier priorities too, preserving their visible order.
            let ordered: Vec<_> = project.ordered_stock().iter().map(|s| s.id).collect();
            reprioritize(project, &ordered);
            for (offset, id) in ids.iter().enumerate() {
                project
                    .stock
                    .push(from_input(*id, count + offset as u32, input.clone()));
            }
            Ok(())
        })?;
        Ok(ids)
    }

    /// An edit affects only the named physical piece; quantity is a creation input.
    pub fn edit_stock(
        &mut self,
        id: Uuid,
        input: StockInput,
    ) -> Result<bool, EditError<StockError>> {
        if !self.project().stock.iter().any(|s| s.id == id) {
            return Err(EditError::Command(StockError::MissingStock(id)));
        }
        validate(self.project(), &input).map_err(EditError::Command)?;
        self.transact(|project| {
            let piece = project
                .stock_piece_mut(id)
                .ok_or(StockError::MissingStock(id))?;
            *piece = from_input(id, piece.priority, input);
            Ok(())
        })
    }

    /// A distinct physical piece starts with the original measurements and
    /// current ownership, but receives its own UUID, priority and fresh alias.
    pub fn duplicate_stock(&mut self, id: Uuid) -> Result<Uuid, EditError<StockError>> {
        let original = self
            .project()
            .stock
            .iter()
            .find(|piece| piece.id == id)
            .ok_or(EditError::Command(StockError::MissingStock(id)))?;
        let input = StockInput::from(original);
        Ok(self.create_stock(input, 1)?[0])
    }

    /// Refuse removal while a board still references this physical piece.
    /// The caller can explicitly unallocate or repair those boards first.
    pub fn delete_stock(&mut self, id: Uuid) -> Result<bool, EditError<StockError>> {
        if !self.project().stock.iter().any(|piece| piece.id == id) {
            return Err(EditError::Command(StockError::MissingStock(id)));
        }
        if self
            .project()
            .allocations
            .iter()
            .any(|allocation| allocation.stock_id == id)
        {
            return Err(EditError::Command(StockError::AllocatedStock(id)));
        }
        self.transact(|project| {
            project.stock.retain(|piece| piece.id != id);
            Ok(())
        })
    }

    /// Place one stock ID at a zero-based position in the global priority list.
    pub fn reorder_stock(
        &mut self,
        id: Uuid,
        target: usize,
    ) -> Result<bool, EditError<StockError>> {
        let mut order: Vec<_> = self
            .project()
            .ordered_stock()
            .iter()
            .map(|s| s.id)
            .collect();
        let Some(position) = order.iter().position(|candidate| *candidate == id) else {
            return Err(EditError::Command(StockError::MissingStock(id)));
        };
        if target >= order.len() {
            return Err(EditError::Command(StockError::Invalid(
                StockField::Priority,
            )));
        }
        order.remove(position);
        order.insert(target, id);
        self.transact(|project| {
            reprioritize(project, &order);
            Ok(())
        })
    }

    /// Reorder a filtered or grouped subset without moving stock occupying
    /// other global slots. `visible_ids` must contain exactly the displayed
    /// subset in its current global priority order (not name or UUID order).
    /// `target` is a zero-based position *within that subset*. The UI can show
    /// the destination's actual global rank before invoking this transaction.
    pub fn reorder_stock_subset(
        &mut self,
        id: Uuid,
        target: usize,
        visible_ids: &[Uuid],
    ) -> Result<bool, EditError<StockError>> {
        let mut global: Vec<_> = self
            .project()
            .ordered_stock()
            .iter()
            .map(|piece| piece.id)
            .collect();
        if !global.contains(&id) {
            return Err(EditError::Command(StockError::MissingStock(id)));
        }
        if visible_ids.is_empty() || target >= visible_ids.len() || !visible_ids.contains(&id) {
            return Err(EditError::Command(StockError::Invalid(
                StockField::Priority,
            )));
        }
        let mut seen = HashSet::with_capacity(visible_ids.len());
        if !visible_ids.iter().all(|candidate| seen.insert(*candidate)) {
            return Err(EditError::Command(StockError::Invalid(
                StockField::Priority,
            )));
        }
        let slots: Vec<_> = global
            .iter()
            .enumerate()
            .filter_map(|(position, candidate)| seen.contains(candidate).then_some(position))
            .collect();
        if slots.len() != visible_ids.len()
            || slots
                .iter()
                .map(|position| global[*position])
                .collect::<Vec<_>>()
                != visible_ids
        {
            return Err(EditError::Command(StockError::Invalid(
                StockField::Priority,
            )));
        }
        let mut reordered = visible_ids.to_vec();
        reordered.remove(
            reordered
                .iter()
                .position(|candidate| *candidate == id)
                .expect("validated member"),
        );
        reordered.insert(target, id);
        for (position, replacement) in slots.into_iter().zip(reordered) {
            global[position] = replacement;
        }
        self.transact(|project| {
            for (rank, id) in global.iter().enumerate() {
                project
                    .stock
                    .iter_mut()
                    .find(|piece| piece.id == *id)
                    .expect("validated stock identity")
                    .priority = rank as u32;
            }
            Ok(())
        })
    }
}


/// Priority follows the position in `order`; pieces not listed keep theirs.
fn reprioritize(project: &mut Project, order: &[Uuid]) {
    for piece in &mut project.stock {
        if let Some(index) = order.iter().position(|id| *id == piece.id) {
            piece.priority = index as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board_commands::{NewBoard, NewMaterial};
    use crate::domain::BoardGrain;
    use crate::money::{Currency, Money};
    use crate::units::{Pose, Quaternion};

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }
    fn fixture() -> (ProjectEditor, StockInput) {
        let mut editor = ProjectEditor::new(Project::new("Cabinet", Currency::Brl)).unwrap();
        let material_id = editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: mm(18),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let input = StockInput {
            name: "Offcut".into(),
            material_id,
            length: mm(900),
            width: mm(240),
            thickness: mm(18),
            grain: StockGrain::AlongX,
            source: StockSource::Owned,
            price: None,
            trim: [Length::ZERO; 4],
        };
        (editor, input)
    }

    #[test]
    fn currency_replacement_is_uuid_keyed_atomic_and_undoable() {
        let (mut editor, mut input) = fixture();
        input.source = StockSource::ToPurchase;
        input.price = Some(Money::new(Currency::Brl, 0).unwrap());
        let ids = editor.create_stock(input, 2).unwrap();
        editor
            .edit_stock(
                ids[1],
                StockInput {
                    price: Some(Money::new(Currency::Brl, 1250).unwrap()),
                    ..StockInput::from(&editor.project().stock[1])
                },
            )
            .unwrap();
        editor
            .set_cut_fee(Some(Money::new(Currency::Brl, 0).unwrap()))
            .unwrap();
        let board = editor
            .create_board(NewBoard {
                name: "part".into(),
                material_id: editor.project().materials[0].id,
                length: mm(900),
                width: mm(240),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        assert_eq!(editor.project().allocations[0].board_id, board);
        let before = editor.project().clone();
        let usd = |units| Some(Money::new(Currency::Usd, units).unwrap());
        assert!(
            editor
                .change_project_currency(
                    Currency::Usd,
                    ProjectCurrencyChange::Replace {
                        cut_fee: usd(25),
                        // The UI can submit in any order; UUIDs, not row positions, bind prices.
                        stock_prices: vec![(ids[1], usd(2400)), (ids[0], usd(0))],
                    },
                )
                .unwrap()
        );
        let after = editor.project().clone();
        assert_eq!(after.revision, before.revision + 1);
        assert_eq!(after.currency, Currency::Usd);
        assert_eq!(after.cut_fee, usd(25));
        assert_eq!(after.stock[0].price, usd(0));
        assert_eq!(after.stock[1].price, usd(2400));
        assert_eq!(after.allocations, before.allocations);
        assert_eq!(after.stock_aliases, before.stock_aliases);
        editor.undo().unwrap();
        assert_eq!(editor.project().currency, before.currency);
        assert_eq!(editor.project().cut_fee, before.cut_fee);
        assert_eq!(editor.project().stock, before.stock);
        assert_eq!(editor.project().allocations, before.allocations);
        editor.redo().unwrap();
        let mut redone = after;
        redone.revision = editor.project().revision;
        assert_eq!(editor.project(), &redone);
    }

    #[test]
    fn currency_relabel_preserves_unknown_and_known_zero_without_conversion() {
        let (mut editor, mut input) = fixture();
        input.price = Some(Money::new(Currency::Brl, 0).unwrap());
        let ids = editor.create_stock(input, 2).unwrap();
        let unknown = StockInput {
            price: None,
            ..StockInput::from(&editor.project().stock[1])
        };
        editor.edit_stock(ids[1], unknown).unwrap();
        let before = editor.project().clone();
        assert!(
            editor
                .change_project_currency(
                    Currency::Usd,
                    ProjectCurrencyChange::ConfirmRelabelWithoutConversion,
                )
                .unwrap()
        );
        assert_eq!(editor.project().cut_fee, None);
        assert_eq!(
            editor.project().stock[0].price,
            Some(Money::new(Currency::Usd, 0).unwrap())
        );
        assert_eq!(editor.project().stock[1].price, None);
        assert_eq!(editor.project().allocations, before.allocations);
        assert!(
            !editor
                .change_project_currency(
                    Currency::Usd,
                    ProjectCurrencyChange::ConfirmRelabelWithoutConversion
                )
                .unwrap()
        );
        editor.undo().unwrap();
        assert_eq!(editor.project().currency, Currency::Brl);
        assert_eq!(
            editor.project().stock[0].price,
            Some(Money::new(Currency::Brl, 0).unwrap())
        );
        assert_eq!(editor.project().stock[1].price, None);
        assert_eq!(editor.project().stock[0].id, ids[0]);
    }

    #[test]
    fn currency_replacement_can_leave_unknowns_unknown_and_set_a_known_zero() {
        let (mut editor, input) = fixture();
        let ids = editor.create_stock(input, 2).unwrap();
        assert!(
            editor
                .change_project_currency(
                    Currency::Usd,
                    ProjectCurrencyChange::Replace {
                        cut_fee: Some(Money::new(Currency::Usd, 0).unwrap()),
                        stock_prices: vec![
                            (ids[1], None),
                            (ids[0], Some(Money::new(Currency::Usd, 0).unwrap())),
                        ],
                    },
                )
                .unwrap()
        );
        assert_eq!(
            editor.project().cut_fee,
            Some(Money::new(Currency::Usd, 0).unwrap())
        );
        assert_eq!(
            editor.project().stock[0].price,
            Some(Money::new(Currency::Usd, 0).unwrap())
        );
        assert_eq!(editor.project().stock[1].price, None);
    }

    #[test]
    fn invalid_currency_replacements_do_not_mutate_history_or_allocations() {
        let (mut editor, mut input) = fixture();
        input.price = Some(Money::new(Currency::Brl, 0).unwrap());
        let ids = editor.create_stock(input, 2).unwrap();
        editor
            .set_cut_fee(Some(Money::new(Currency::Brl, 10).unwrap()))
            .unwrap();
        let before = editor.project().clone();
        let usd = |n| Some(Money::new(Currency::Usd, n).unwrap());
        let brl = |n| Some(Money::new(Currency::Brl, n).unwrap());
        let missing = Uuid::new_v4();
        for (decision, error) in [
            (
                ProjectCurrencyChange::Replace {
                    cut_fee: usd(20),
                    stock_prices: vec![(ids[0], usd(0))],
                },
                CurrencyChangeError::MissingStock(ids[1]),
            ),
            (
                ProjectCurrencyChange::Replace {
                    cut_fee: usd(20),
                    stock_prices: vec![(ids[0], usd(0)), (ids[0], usd(10))],
                },
                CurrencyChangeError::DuplicateStock(ids[0]),
            ),
            (
                ProjectCurrencyChange::Replace {
                    cut_fee: usd(20),
                    stock_prices: vec![(ids[0], usd(0)), (missing, usd(0))],
                },
                CurrencyChangeError::UnknownStock(missing),
            ),
            (
                ProjectCurrencyChange::Replace {
                    cut_fee: None,
                    stock_prices: vec![(ids[0], usd(0)), (ids[1], None)],
                },
                CurrencyChangeError::Money(MoneyError::MissingReplacement),
            ),
            (
                ProjectCurrencyChange::Replace {
                    cut_fee: usd(20),
                    stock_prices: vec![(ids[0], None), (ids[1], usd(0))],
                },
                CurrencyChangeError::Money(MoneyError::MissingReplacement),
            ),
            (
                ProjectCurrencyChange::Replace {
                    cut_fee: usd(20),
                    stock_prices: vec![(ids[0], brl(0)), (ids[1], usd(0))],
                },
                CurrencyChangeError::Money(MoneyError::CurrencyMismatch),
            ),
        ] {
            assert_eq!(
                editor.change_project_currency(Currency::Usd, decision),
                Err(EditError::Command(error))
            );
            assert_eq!(editor.project(), &before);
        }
        // Errors do not clear the redo branch or create an undo entry.
        editor.undo().unwrap();
        assert_eq!(editor.project().cut_fee, None);
        editor.redo().unwrap();
        let mut redone = before;
        redone.revision = editor.project().revision;
        assert_eq!(editor.project(), &redone);
    }

    #[test]
    fn identical_owned_offcuts_have_individual_ids_and_allocations() {
        let (mut editor, input) = fixture();
        let ids = editor.create_stock(input, 2).unwrap();
        assert_ne!(ids[0], ids[1]);
        assert_eq!(
            editor.project().stock[0].name,
            editor.project().stock[1].name
        );
        for id in &ids {
            let board = editor
                .create_board(NewBoard {
                    name: "part".into(),
                    material_id: editor.project().materials[0].id,
                    length: mm(900),
                    width: mm(240),
                    pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
                })
                .unwrap();
            assert_eq!(
                editor
                    .project()
                    .allocations
                    .iter()
                    .find(|a| a.board_id == board)
                    .unwrap()
                    .stock_id,
                *id
            );
        }
        assert_eq!(editor.project().allocations[0].stock_id, ids[0]);
        assert_eq!(editor.project().allocations[1].stock_id, ids[1]);
        assert!(
            editor
                .project()
                .stock
                .iter()
                .all(|s| s.source == StockSource::Owned)
        );
    }

    #[test]
    fn quantity_is_one_undo_step_and_reorder_is_persistent() {
        let (mut editor, input) = fixture();
        let ids = editor.create_stock(input, 3).unwrap();
        assert_eq!(
            editor
                .project()
                .ordered_stock()
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            ids
        );
        editor.reorder_stock(ids[2], 0).unwrap();
        assert_eq!(editor.project().ordered_stock()[0].id, ids[2]);
        editor.undo().unwrap();
        assert_eq!(editor.project().ordered_stock()[0].id, ids[0]);
        editor.undo().unwrap();
        assert!(editor.project().stock.is_empty());
        editor.redo().unwrap();
        assert_eq!(
            editor
                .project()
                .ordered_stock()
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            ids
        );
        editor.redo().unwrap();
        assert_eq!(editor.project().ordered_stock()[0].id, ids[2]);
        let serialized = serde_json::to_string(editor.project()).unwrap();
        let loaded: Project = serde_json::from_str(&serialized).unwrap();
        assert_eq!(loaded.ordered_stock()[0].id, ids[2]);
    }

    #[test]
    fn subset_reordering_keeps_hidden_global_slots_and_aliases() {
        let (mut editor, input) = fixture();
        let ids = editor.create_stock(input, 5).unwrap();
        editor
            .create_board(NewBoard {
                name: "fixed placement".into(),
                material_id: editor.project().materials[0].id,
                length: mm(900),
                width: mm(240),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        let allocations = editor.project().allocations.clone();
        let aliases: Vec<_> = ids
            .iter()
            .map(|id| editor.project().stock_alias(*id).unwrap().to_owned())
            .collect();
        let revision = editor.project().revision;
        assert!(
            editor
                .reorder_stock_subset(ids[4], 0, &[ids[0], ids[2], ids[4]])
                .unwrap()
        );
        let ordered = |editor: &ProjectEditor| {
            editor
                .project()
                .ordered_stock()
                .iter()
                .map(|piece| piece.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ordered(&editor), [ids[4], ids[1], ids[0], ids[3], ids[2]]);
        assert_eq!(editor.project().revision, revision + 1);
        assert_eq!(editor.project().allocations, allocations);
        assert_eq!(
            ids.iter()
                .map(|id| editor.project().stock_alias(*id).unwrap().to_owned())
                .collect::<Vec<_>>(),
            aliases
        );
        editor.undo().unwrap();
        assert_eq!(ordered(&editor), ids);
        editor.redo().unwrap();
        assert_eq!(ordered(&editor), [ids[4], ids[1], ids[0], ids[3], ids[2]]);
    }

    #[test]
    fn stale_or_misordered_subset_cannot_change_priorities() {
        let (mut editor, input) = fixture();
        let ids = editor.create_stock(input, 5).unwrap();
        let before = editor.project().clone();
        for subset in [
            vec![ids[2], ids[0], ids[4]],
            vec![ids[0], ids[0], ids[4]],
            vec![ids[0], Uuid::new_v4(), ids[4]],
            vec![ids[0], ids[2]],
        ] {
            assert_eq!(
                editor.reorder_stock_subset(ids[4], 0, &subset),
                Err(EditError::Command(StockError::Invalid(
                    StockField::Priority
                )))
            );
        }
        assert_eq!(editor.project(), &before);
    }

    #[test]
    fn project_independence_and_invalid_inputs_are_atomic() {
        let (mut first, input) = fixture();
        let (mut second, _) = fixture();
        let original = first.project().clone();
        let mut bad = input.clone();
        bad.width = Length::ZERO;
        assert_eq!(
            first.create_stock(bad, 2),
            Err(EditError::Command(StockError::Invalid(StockField::Width)))
        );
        assert_eq!(
            first.create_stock(input.clone(), 0),
            Err(EditError::Command(StockError::Invalid(
                StockField::Quantity
            )))
        );
        bad = input.clone();
        bad.trim[0] = mm(-1);
        assert_eq!(
            first.create_stock(bad, 1),
            Err(EditError::Command(StockError::Invalid(StockField::Trim)))
        );
        bad = input.clone();
        bad.price = Some(Money::new(Currency::Usd, 500).unwrap());
        assert_eq!(
            first.create_stock(bad, 1),
            Err(EditError::Command(StockError::Invalid(StockField::Price)))
        );
        assert_eq!(first.project(), &original);
        let ids = first.create_stock(input, 2).unwrap();
        assert!(second.project().stock.is_empty());
        let before = first.project().clone();
        assert_eq!(
            first.reorder_stock(ids[0], 99),
            Err(EditError::Command(StockError::Invalid(
                StockField::Priority
            )))
        );
        assert_eq!(first.project(), &before);
        let other_material = second.project().materials[0].id;
        assert_ne!(other_material, first.project().materials[0].id);
        let foreign = StockInput {
            material_id: other_material,
            ..StockInput::from(&first.project().stock[0])
        };
        assert!(matches!(
            first.edit_stock(ids[0], foreign),
            Err(EditError::Command(StockError::MissingMaterial(_)))
        ));
        assert!(
            second
                .create_stock(
                    StockInput {
                        material_id: other_material,
                        ..StockInput::from(&before.stock[0])
                    },
                    1
                )
                .is_ok()
        );
        assert_eq!(first.project().stock.len(), 2);
    }

    #[test]
    fn price_and_measured_thickness_are_independent_of_material_defaults() {
        let (mut editor, mut input) = fixture();
        input.source = StockSource::ToPurchase;
        input.price = Some(Money::new(Currency::Brl, 0).unwrap());
        input.thickness = mm(15);
        let id = editor.create_stock(input.clone(), 1).unwrap()[0];
        assert_eq!(editor.project().stock[0].thickness, mm(15));
        assert_eq!(editor.project().materials[0].default_thickness, mm(18));
        assert_eq!(editor.project().stock[0].price.unwrap().minor_units(), 0);
        input.price = None;
        editor.edit_stock(id, input.clone()).unwrap();
        assert_eq!(editor.project().stock[0].price, None);
        editor.undo().unwrap();
        assert_eq!(editor.project().stock[0].price.unwrap().minor_units(), 0);
        let previous = editor.project().clone();
        input.trim = [mm(500), mm(400), Length::ZERO, Length::ZERO];
        assert_eq!(
            editor.edit_stock(id, input),
            Err(EditError::Command(StockError::Invalid(StockField::Trim)))
        );
        assert_eq!(editor.project(), &previous);
    }
}
