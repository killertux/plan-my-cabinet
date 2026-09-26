//! Headless woodworking project core. The desktop executable owns the UI and GPU.

/// Application title shared by desktop integration and core metadata.
pub const APPLICATION_NAME: &str = "Plan My Cabinet";

pub mod allocation_diagnostics;
pub mod assembly_edit;
pub mod board_commands;
pub mod board_dimensions;
pub mod candidate_generation;
pub mod candidate_ranking;
pub mod commands;
pub mod cost_estimate;
pub mod cut_tree;
pub mod dimension_input;
pub mod domain;
pub mod door_joint;
pub mod export;
pub mod first_fit;
pub mod hardware_catalog;
pub mod hinge_installation;
pub mod i18n;
pub mod material_changes;
pub mod measurements;
pub mod money;
pub mod optimization_worker;
pub mod pdf_export;
pub mod persistence;
pub mod placement;
pub mod recovery;
pub mod sheet_edit;
pub mod stock_commands;
pub mod units;

#[cfg(test)]
mod tests {
    use super::APPLICATION_NAME;

    #[test]
    fn library_can_run_without_desktop_initialization() {
        assert_eq!(APPLICATION_NAME, "Plan My Cabinet");
    }
}
