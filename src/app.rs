//! Desktop application modules: workspaces, dialogs, viewport and shell.
//! Headless project logic lives in the library crate (`plan_my_cabinet`).

pub(crate) mod actions;
pub(crate) mod assembly_ui;
pub(crate) mod board_dialogs;
pub(crate) mod capture;
pub(crate) mod command_palette;
pub(crate) mod currency_ui;
pub(crate) mod door_joint_ui;
pub(crate) mod export_flow;
pub(crate) mod handoff_ui;
pub(crate) mod hardware_ui;
pub(crate) mod hinge_ui;
pub(crate) mod kerf_confirmation_ui;
pub(crate) mod modal_chrome;
pub(crate) mod modals;
pub(crate) mod navigation;
pub(crate) mod optimization_ui;
pub(crate) mod pending_navigation;
pub(crate) mod placement_ui;
pub(crate) mod project_ui;
pub(crate) mod receipt_ui;
pub(crate) mod recovery_cleanup_ui;
pub(crate) mod settings_host;
pub(crate) mod sheet_ui;
pub(crate) mod shell;
pub(crate) mod state;
pub(crate) mod stock_ui;
pub(crate) mod template_setup_ui;
pub(crate) mod toasts;
pub(crate) mod viewport;
pub(crate) mod welcome_host;
pub(crate) mod widget_gallery;
pub(crate) mod workspace_shell;
pub(crate) mod workspace_state;
