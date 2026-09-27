//! Headless woodworking project core. The desktop executable owns the UI and GPU.

/// Application title shared by desktop integration and core metadata.
pub const APPLICATION_NAME: &str = "Plan My Cabinet";

pub mod allocation_diagnostics;
pub mod assembly_edit;
pub mod board_commands;
pub mod board_dimensions;
pub mod candidate_generation;
pub mod candidate_ranking;
pub mod color_commands;
pub mod commands;
pub mod cost_estimate;
pub mod cut_tree;
pub mod design_read_models;
pub mod dimension_input;
pub mod document_layout;
pub mod domain;
pub mod door_joint;
pub mod edit_drafts;
pub mod export;
pub mod first_fit;
pub mod hardware_catalog;
pub mod hinge_installation;
pub mod i18n;
pub mod icons;
pub mod kerf_date;
pub mod local_preferences;
pub mod material_changes;
pub mod material_presets;
pub mod measurements;
pub mod money;
pub mod optimization_worker;
pub mod pdf_export;
pub mod persistence;
pub mod placement;
pub mod receipt_read_models;
pub mod recent_projects;
pub mod recovery;
pub mod reference_fixture;
pub mod settings_ui;
pub mod sheet_edit;
pub mod stock_commands;
pub mod stock_read_models;
pub mod template_recipes;
pub mod template_setup;
pub mod theme;
pub mod theme_widgets;
pub mod units;
pub mod welcome_ui;
pub mod workshop_document;

#[cfg(test)]
mod tests {
    use super::APPLICATION_NAME;

    #[test]
    fn library_can_run_without_desktop_initialization() {
        assert_eq!(APPLICATION_NAME, "Plan My Cabinet");
    }
}
