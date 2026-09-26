//! Project-local physical stock creation, correction and user-defined ordering.
use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::{Project, Stock, StockGrain, StockSource};
use crate::money::Money;
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
            for (index, id) in ordered.iter().enumerate() {
                project
                    .stock
                    .iter_mut()
                    .find(|s| s.id == *id)
                    .unwrap()
                    .priority = index as u32;
            }
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
        let previous = self
            .project()
            .stock
            .iter()
            .find(|s| s.id == id)
            .ok_or(EditError::Command(StockError::MissingStock(id)))?;
        validate(self.project(), &input).map_err(EditError::Command)?;
        let priority = previous.priority;
        self.transact(|project| {
            *project.stock.iter_mut().find(|s| s.id == id).unwrap() =
                from_input(id, priority, input);
            Ok(())
        })
    }

    /// Place one stock ID at an index in the visible priority list.
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
            for (index, id) in order.iter().enumerate() {
                project
                    .stock
                    .iter_mut()
                    .find(|piece| piece.id == *id)
                    .unwrap()
                    .priority = index as u32;
            }
            Ok(())
        })
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
