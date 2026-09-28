//! Headless woodworking project core. The desktop executable owns the UI and GPU.

// Production code states its invariants with `expect("why")` or handles the
// failure; a bare `unwrap` is reserved for tests.
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

/// Application title shared by desktop integration and core metadata.
pub const APPLICATION_NAME: &str = "Plan My Cabinet";

pub mod catalog;
pub mod editing;
pub mod i18n;
pub mod model;
pub mod optimize;
pub mod output;
pub mod read_models;
pub mod reference_fixture;
pub mod storage;
pub mod ui;

// Flat paths (`plan_my_cabinet::commands`, `crate::domain`) stay valid.
pub use catalog::{
    door_joint, hardware_catalog, hinge_installation, template_recipes, template_setup,
};
pub use editing::{
    assembly_edit, board_commands, board_dimensions, color_commands, commands, edit_drafts,
    material_changes, placement, sheet_edit, stock_commands,
};
pub use model::{dimension_input, domain, kerf_date, material_presets, measurements, money, units};
pub use optimize::{
    allocation_diagnostics, candidate_generation, candidate_ranking, cut_tree, first_fit,
    optimization_worker,
};
pub use output::{document_layout, export, pdf_export, workshop_document};
pub use read_models::{cost_estimate, design_read_models, receipt_read_models, stock_read_models};
pub use storage::{local_preferences, persistence, recent_projects, recovery};
pub use ui::{icons, settings_ui, theme, theme_widgets, welcome_ui};

#[cfg(test)]
mod tests {
    use super::APPLICATION_NAME;

    #[test]
    fn library_can_run_without_desktop_initialization() {
        assert_eq!(APPLICATION_NAME, "Plan My Cabinet");
    }
}
