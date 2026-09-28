//! Per-workspace state grouped out of `DesktopApp`, so each area owns its fields.
use crate::*;

/// Handoff workspace: export review, preview and PDF writing.
pub(crate) struct HandoffState {
    pub(crate) mode: ExportMode,
    pub(crate) sections: ReceiptSections,
    pub(crate) preview: DocumentPreviewState,
    pub(crate) preparation: Option<ExportPreparationCache>,
    pub(crate) candidate: Option<ExportPreparationCache>,
    pub(crate) preparation_pending: Option<ExportPreparationJob>,
    pub(crate) language: Language,
    pub(crate) units: Unit,
    pub(crate) activity: Option<ExportActivity>,
    pub(crate) overwrite_chrome: ModalChrome,
    pub(crate) picker_key: Option<ReviewedPacketKey>,
    pub(crate) events: Option<Receiver<ExportEvent>>,
    pub(crate) cancel: Option<Arc<AtomicBool>>,
    pub(crate) message: Option<String>,
}

impl Default for HandoffState {
    fn default() -> Self {
        Self {
            mode: ExportMode::Draft,
            sections: ReceiptSections::default(),
            preview: DocumentPreviewState::default(),
            preparation: None,
            candidate: None,
            preparation_pending: None,
            language: Language::En,
            units: Unit::Mm,
            activity: None,
            overwrite_chrome: ModalChrome::new(egui::Id::new("export-overwrite-dialog"))
                .width(520.0),
            picker_key: None,
            events: None,
            cancel: None,
            message: None,
        }
    }
}

/// The Settings window and the flows it hands off to.
#[derive(Default)]
pub(crate) struct SettingsHost {
    pub(crate) open: bool,
    pub(crate) resume_after_dialog: bool,
    pub(crate) state: SettingsState,
    pub(crate) worked_examples: bool,
    pub(crate) examples_chrome: Option<modal_chrome::ModalChrome>,
    pub(crate) message: Option<String>,
    pub(crate) cleanup: Option<CleanupUi>,
}

/// The cabinet template setup flow.
#[derive(Default)]
pub(crate) struct TemplateHost {
    pub(crate) setup: Option<TemplateSetupUi>,
    pub(crate) guard_pending: bool,
    pub(crate) message: Option<String>,
}

/// Controllers of the app-level dialogs. They outlive each dialog so focus
/// returns to the control that opened it.
pub(crate) struct DialogChromes {
    pub(crate) board_creation: ModalChrome,
    pub(crate) material_creation: ModalChrome,
    pub(crate) material_edit: ModalChrome,
    pub(crate) board_material: ModalChrome,
    pub(crate) board_dimension: ModalChrome,
    pub(crate) batch_dimension: ModalChrome,
    pub(crate) placement: ModalChrome,
    pub(crate) grid: ModalChrome,
}

impl Default for DialogChromes {
    fn default() -> Self {
        Self {
            board_creation: ModalChrome::new(egui::Id::new("board-creation-dialog"))
                .width(480.0)
                .icon(icons::Icon::Board)
                .first_focus(egui::Id::new("board-creation-name")),
            material_creation: ModalChrome::new(egui::Id::new("material-creation-dialog"))
                .width(400.0)
                .icon(icons::Icon::Material)
                .first_focus(egui::Id::new("material-creation-name")),
            material_edit: ModalChrome::new(egui::Id::new("material-edit-dialog"))
                .width(480.0)
                .icon(icons::Icon::Material)
                .first_focus(egui::Id::new("material-edit-name")),
            board_material: ModalChrome::new(egui::Id::new("board-material-dialog"))
                .width(440.0)
                .icon(icons::Icon::Material)
                .first_focus(
                    egui::Id::new("board-material-picker")
                        .with("popup")
                        .with("select"),
                ),
            board_dimension: ModalChrome::new(egui::Id::new("board-dimension-dialog"))
                .width(440.0)
                .icon(icons::Icon::Measure)
                .first_focus(egui::Id::new("board-dimension-value")),
            batch_dimension: ModalChrome::new(egui::Id::new("batch-dimension-dialog"))
                .width(440.0)
                .icon(icons::Icon::Measure)
                .first_focus(egui::Id::new("batch-dimension-value")),
            placement: ModalChrome::new(egui::Id::new("placement-dialog")).width(520.0),
            grid: ModalChrome::new(egui::Id::new("grid-spacing-dialog"))
                .width(420.0)
                .icon(icons::Icon::Grid)
                .first_focus(egui::Id::new("grid-spacing-value")),
        }
    }
}

/// Design workspace tools and caches.
pub(crate) struct DesignState {
    pub(crate) pose_frame: CoordinateFrame,
    pub(crate) pending_pose_frame: Option<CoordinateFrame>,
    pub(crate) move_tool: viewport::MoveTool,
    pub(crate) measurement_scope: Scope,
    pub(crate) measurement_frame: Frame,
    pub(crate) board_action_error: bool,
    pub(crate) first_fit_notice: Option<FirstFit>,
    pub(crate) scene_active_seen: Option<Uuid>,
    pub(crate) stock_snapshot: Option<((Uuid, u64), StockReadModel)>,
}

impl Default for DesignState {
    fn default() -> Self {
        Self {
            pose_frame: CoordinateFrame::LocalParent,
            pending_pose_frame: None,
            move_tool: viewport::MoveTool::default(),
            measurement_scope: Scope::Body,
            measurement_frame: Frame::World,
            board_action_error: false,
            first_fit_notice: None,
            scene_active_seen: None,
            stock_snapshot: None,
        }
    }
}

/// Cut plan workspace: sheet repair, optimizer and diagnostics.
#[derive(Default)]
pub(crate) struct CutPlanState {
    pub(crate) repair: sheet_ui::RepairUi,
    pub(crate) optimizer: optimization_ui::OptimizeUi,
    pub(crate) allocation_diagnostics: Option<(sheet_ui::DiagnosticsKey, Vec<BoardDiagnostic>)>,
    pub(crate) material_conflicts: Vec<AllocationConflict>,
}

/// Hardware workspace session state.
#[derive(Default)]
pub(crate) struct HardwareState {
    pub(crate) door_motion: Option<(Uuid, f64)>,
    pub(crate) catalog_update_notice: Option<String>,
}
