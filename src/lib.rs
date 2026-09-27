//! Headless woodworking project core. The desktop executable owns the UI and GPU.

// Production code states its invariants with `expect("why")` or handles the
// failure; a bare `unwrap` is reserved for tests.
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

/// Application title shared by desktop integration and core metadata.
pub const APPLICATION_NAME: &str = "Plan My Cabinet";

pub mod i18n;
pub mod reference_fixture;
pub mod model;
pub mod editing;
pub mod optimize;
pub mod read_models;
pub mod output;
pub mod storage;
pub mod catalog;
pub mod ui;

// Flat paths (`plan_my_cabinet::commands`, `crate::domain`) stay valid.
pub use model::{domain, units, money, measurements, kerf_date, dimension_input, material_presets};
pub use editing::{commands, board_commands, color_commands, stock_commands, assembly_edit, sheet_edit, edit_drafts, material_changes, board_dimensions, placement};
pub use optimize::{candidate_generation, candidate_ranking, cut_tree, first_fit, optimization_worker, allocation_diagnostics};
pub use read_models::{design_read_models, stock_read_models, receipt_read_models, cost_estimate};
pub use output::{export, pdf_export, document_layout, workshop_document};
pub use storage::{persistence, recovery, recent_projects, local_preferences};
pub use catalog::{hardware_catalog, hinge_installation, door_joint, template_recipes, template_setup};
pub use ui::{theme, theme_widgets, icons, settings_ui, welcome_ui};

#[cfg(test)]
mod tests {
    use super::APPLICATION_NAME;

    #[test]
    fn library_can_run_without_desktop_initialization() {
        assert_eq!(APPLICATION_NAME, "Plan My Cabinet");
    }
}
