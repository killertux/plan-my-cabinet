//! The one form dialog that may be open at a time.
//!
//! Every dialog used to be its own `Option` field on the app, so "is anything
//! open?" was a long chain of checks and nothing stopped two from opening. Now
//! a single slot holds the active dialog. The typed accessors keep call sites
//! as short as the old field access: `modals.hinge()`, `modals.take_hinge()`,
//! `modals.set_hinge(Some(draft))`.
//!
//! Settings, the palette, the navigation prompt and project prompts are not
//! form dialogs and keep their own state.
use crate::{
    BatchDialog, BoardDimensionDialog, BoardMaterialDialog, CreationDialog, GridDialog,
    MaterialEditDialog, assembly_ui::AssemblyDialog, currency_ui::CurrencyDialog,
    door_joint_ui::{DoorDialog, RemovalDialog}, hardware_ui::HardwareDialog,
    hinge_ui::HingeDialog, kerf_confirmation_ui::KerfConfirmation, placement_ui::PlacementDialog,
    stock_ui::{CutFeeDialog, StockDialog},
};

#[derive(Default)]
pub(crate) struct Modals {
    active: Option<Modal>,
}

impl Modals {
    pub(crate) fn is_open(&self) -> bool {
        self.active.is_some()
    }

    fn open(&mut self, modal: Modal) {
        debug_assert!(
            self.active
                .as_ref()
                .is_none_or(|active| std::mem::discriminant(active) == std::mem::discriminant(&modal)),
            "opening a dialog while another one is open"
        );
        self.active = Some(modal);
    }
}

macro_rules! modals {
    ($($variant:ident($ty:ty) => $get:ident, $get_mut:ident, $take:ident, $set:ident;)*) => {
        pub(crate) enum Modal {
            $($variant($ty),)*
        }

        impl Modals {
            $(
                #[allow(dead_code)]
                pub(crate) fn $get(&self) -> Option<&$ty> {
                    match &self.active {
                        Some(Modal::$variant(dialog)) => Some(dialog),
                        _ => None,
                    }
                }

                #[allow(dead_code)]
                pub(crate) fn $get_mut(&mut self) -> Option<&mut $ty> {
                    match &mut self.active {
                        Some(Modal::$variant(dialog)) => Some(dialog),
                        _ => None,
                    }
                }

                /// Removes the dialog only when it is the open one.
                #[allow(dead_code)]
                pub(crate) fn $take(&mut self) -> Option<$ty> {
                    match self.active.take() {
                        Some(Modal::$variant(dialog)) => Some(dialog),
                        other => {
                            self.active = other;
                            None
                        }
                    }
                }

                /// `Some` opens (or keeps) the dialog; `None` closes it if open.
                #[allow(dead_code)]
                pub(crate) fn $set(&mut self, dialog: Option<$ty>) {
                    match dialog {
                        Some(dialog) => self.open(Modal::$variant(dialog)),
                        None => {
                            self.$take();
                        }
                    }
                }
            )*
        }
    };
}

modals! {
    Creation(CreationDialog) => creation, creation_mut, take_creation, set_creation;
    MaterialEdit(MaterialEditDialog) => material_edit, material_edit_mut, take_material_edit, set_material_edit;
    BoardMaterial(BoardMaterialDialog) => board_material, board_material_mut, take_board_material, set_board_material;
    BoardDimension(BoardDimensionDialog) => board_dimension, board_dimension_mut, take_board_dimension, set_board_dimension;
    BatchDimension(BatchDialog) => batch_dimension, batch_dimension_mut, take_batch_dimension, set_batch_dimension;
    Placement(PlacementDialog) => placement, placement_mut, take_placement, set_placement;
    Grid(GridDialog) => grid, grid_mut, take_grid, set_grid;
    KerfConfirmation(KerfConfirmation) => kerf_confirmation, kerf_confirmation_mut, take_kerf_confirmation, set_kerf_confirmation;
    Stock(StockDialog) => stock, stock_mut, take_stock, set_stock;
    CutFee(CutFeeDialog) => cut_fee, cut_fee_mut, take_cut_fee, set_cut_fee;
    Currency(CurrencyDialog) => currency, currency_mut, take_currency, set_currency;
    Assembly(AssemblyDialog) => assembly, assembly_mut, take_assembly, set_assembly;
    Hardware(HardwareDialog) => hardware, hardware_mut, take_hardware, set_hardware;
    Hinge(HingeDialog) => hinge, hinge_mut, take_hinge, set_hinge;
    Door(DoorDialog) => door, door_mut, take_door, set_door;
    Removal(RemovalDialog) => removal, removal_mut, take_removal, set_removal;
}
