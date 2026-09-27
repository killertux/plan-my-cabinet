use eframe::{egui, egui_wgpu::WgpuSetup, wgpu};
use fluent_bundle::FluentArgs;
use plan_my_cabinet::allocation_diagnostics::{
    BoardDiagnostic, Status as AllocationStatus, diagnose,
};
#[cfg(test)]
use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
use plan_my_cabinet::board_dimensions::{
    BatchDimensionError, BoardDimension, BoardSelection, DimensionEditError, DimensionPreview,
    SelectionValue,
};
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::design_read_models::{DesignReadModel, DesignView};
use plan_my_cabinet::dimension_input::{InputError, Locale, format_length, parse_length};
use plan_my_cabinet::domain::{Board, BoardGrain, Project, SrgbColor, validate_grid_spacing};
use plan_my_cabinet::edit_drafts::EditDrafts;
use plan_my_cabinet::export::{
    ExportIssue, ExportMode, ExportSettings, ExportStatus, OutputError, Overwrite, ReceiptSections,
    ReviewPreparationError, ReviewedPacket, ReviewedPacketKey, SheetIssue, write_reviewed_pdf,
};
use plan_my_cabinet::first_fit::{FirstFit, allocate_new_board};
use plan_my_cabinet::hardware_catalog;
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::local_preferences::{InterfaceScale, LocalPreferences, PreferencesStore};
use plan_my_cabinet::material_changes::{
    AllocationConflict, ConflictReason, DependantChoice, MaterialChangeError,
    MaterialChangePreview, allocation_conflicts,
};
use plan_my_cabinet::measurements::{Frame, MeasurementError, Scope};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::pdf_export::{
    DocumentPreviewLabels, DocumentPreviewState, show_document_preview,
};
use plan_my_cabinet::placement::CoordinateFrame;
use plan_my_cabinet::settings_ui::{
    SOURCE_URL as APP_SOURCE_URL, Section as SettingsSection, SettingsIntent, SettingsState,
};
use plan_my_cabinet::stock_read_models::StockReadModel;
use plan_my_cabinet::units::{
    Anchor, Conversion, Length, Pose, Quaternion, Unit, UnitError, dimension,
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use uuid::Uuid;
mod actions;
mod assembly_ui;
mod capture;
mod command_palette;
mod currency_ui;
mod door_joint_ui;
mod hardware_ui;
mod hinge_ui;
mod icons;
mod kerf_confirmation_ui;
mod modal_chrome;
mod optimization_ui;
mod pending_navigation;
mod placement_ui;
mod project_ui;
mod receipt_ui;
mod recovery_cleanup_ui;
mod sheet_ui;
mod stock_ui;
mod template_setup_ui;
mod theme;
mod theme_widgets;
mod viewport;
mod welcome_host;
mod widget_gallery;
mod workspace_shell;
mod workspace_state;
use actions::{ActionId as A, Argument, Request, Target};
use modal_chrome::{ModalAction, ModalActions, ModalChrome, ModalThreeAction, ModalThreeActions};
use pending_navigation::{
    Decision as NavigationDecision, EditBlock, EditKind, NavigationGuard, NavigationIntent,
    Outcome, Revision, Route as NavigationRoute,
};
use placement_ui::PlacementDialog;
use recovery_cleanup_ui::{CleanupIntent, CleanupUi};
use template_setup_ui::{TemplateSetupIntent, TemplateSetupUi};
use workspace_state::{Destination, InspectorTarget, Workspace, WorkspaceSession};

fn unit_key(unit: Unit) -> &'static str {
    match unit {
        Unit::Mm => "unit-mm",
        Unit::Cm => "unit-cm",
        Unit::M => "unit-m",
        Unit::Inch => "unit-in",
        Unit::Foot => "unit-ft",
    }
}

fn stock_reference(project: &Project, id: Uuid) -> String {
    project
        .stock_alias(id)
        .map(|alias| format!("{alias} [{id}]"))
        .unwrap_or_else(|| id.to_string())
}

fn kerf_confirmation_label(project: &Project, localizer: &Localizer) -> Option<String> {
    (project.confirmed_shop_kerf == Some(project.cutting_kerf)).then(|| {
        if let Some(date) =
            plan_my_cabinet::kerf_date::confirmation_date_utc(project.confirmed_shop_kerf_unix_ms)
        {
            let mut args = FluentArgs::new();
            args.set("date", date);
            localizer.format("cutting-kerf-confirmed-on", Some(&args))
        } else {
            localizer.text("cutting-kerf-confirmed-unknown-date")
        }
    })
}

fn export_completion_key(status: ExportStatus) -> &'static str {
    match status {
        ExportStatus::Current => "export-saved",
        ExportStatus::PacketStale | ExportStatus::WoodStale => "export-saved-stale",
        ExportStatus::Unknown | ExportStatus::NeverExported => "export-saved-unknown",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DialogKind {
    Board,
    Material,
}

struct DimensionDraft {
    text: String,
    consent: bool,
}

type ExportPreparationKey = (
    Uuid,
    u64,
    ExportMode,
    Language,
    Unit,
    String,
    ReceiptSections,
    u32,
    u32,
);
type ExportPreparationCache = (
    ExportPreparationKey,
    Result<Arc<ReviewedPacket>, ReviewPreparationError>,
);

enum ExportEvent {
    Selected(Option<PathBuf>),
    Finished(
        PathBuf,
        Arc<ReviewedPacket>,
        Result<plan_my_cabinet::export::ExportRecord, OutputError>,
    ),
}

enum ExportActivity {
    Choosing(Box<Project>, ExportSettings, ExportMode),
    Confirming(PathBuf, Arc<ReviewedPacket>),
    Writing,
}

#[derive(Clone, Copy)]
enum HandoffFix {
    Navigate(Destination),
    Stock(Uuid),
    Hardware(Uuid),
    Kerf,
    CutFee,
}

type ExportPreparationJob = (
    ExportPreparationKey,
    Receiver<Result<Arc<ReviewedPacket>, ReviewPreparationError>>,
    Arc<AtomicBool>,
);

struct GridDialog {
    kerf: bool,
    project_id: Uuid,
    revision: u64,
    original: Length,
    unit: Unit,
    value: DimensionDraft,
    focus_on_open: bool,
    error: bool,
}

impl GridDialog {
    fn open(project: &Project, locale: Locale) -> Self {
        let spacing = project.grid_spacing;
        Self {
            kerf: false,
            project_id: project.id,
            revision: project.revision,
            original: spacing,
            unit: if matches!(project.display_unit, Unit::Inch | Unit::Foot) {
                Unit::Inch
            } else {
                Unit::Mm
            },
            value: DimensionDraft {
                text: format_length(spacing, Unit::Mm, locale, 3),
                consent: false,
            },
            focus_on_open: true,
            error: false,
        }
    }

    fn cutting_kerf(project: &Project, locale: Locale) -> Self {
        let mut draft = Self::open(project, locale);
        draft.kerf = true;
        draft.original = project.cutting_kerf;
        draft.value.text = format_length(project.cutting_kerf, Unit::Mm, locale, 3);
        draft
    }
}

impl DimensionDraft {
    fn new() -> Self {
        Self {
            text: String::new(),
            consent: false,
        }
    }

    fn value(&self, unit: Unit) -> Result<Length, InputError> {
        let parsed = parse_length(&self.text, unit)?;
        let converted = dimension(parsed.conversion).map_err(InputError::Unit)?;
        match converted {
            Conversion::Exact(value) => Ok(value),
            Conversion::NeedsConfirmation(value) if self.consent => Ok(value),
            Conversion::NeedsConfirmation(_) => Err(InputError::Unit(UnitError::InvalidNumber)),
        }
    }
}

struct CreationDialog {
    kind: DialogKind,
    name: String,
    length: DimensionDraft,
    width: DimensionDraft,
    thickness: DimensionDraft,
    unit: Option<Unit>,
    project_id: Option<Uuid>,
    material_id: Option<Uuid>,
    grain: BoardGrain,
    grain_override: Option<BoardGrain>,
    color: Option<SrgbColor>,
    preview: Option<(BoardPreviewKey, BoardPreview)>,
    error: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BoardPreviewKey {
    project_id: Uuid,
    revision: u64,
    material_id: Uuid,
    length: Length,
    width: Length,
    grain_override: Option<BoardGrain>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoardPreview {
    fit: FirstFit,
    placement: Option<([Length; 2], bool)>,
}

fn fit_new_board(project: &mut Project, name: String, key: BoardPreviewKey) -> BoardPreview {
    let material = project
        .materials
        .iter()
        .find(|material| material.id == key.material_id)
        .expect("validated material");
    let id = Uuid::new_v4();
    project.boards.push(Board {
        id,
        name,
        material_id: key.material_id,
        length: key.length,
        width: key.width,
        thickness: material.default_thickness,
        grain_override: key.grain_override,
        parent_id: None,
        pose: Pose::new([0.0; 3], Quaternion::IDENTITY).expect("identity pose"),
    });
    let fit = allocate_new_board(project, id);
    let placement = project
        .allocations
        .iter()
        .find(|allocation| allocation.board_id == id)
        .map(|allocation| (allocation.origin, allocation.quarter_turn));
    BoardPreview { fit, placement }
}

impl CreationDialog {
    fn board_key(&self, project: &Project) -> Option<BoardPreviewKey> {
        if self.project_id != Some(project.id) {
            return None;
        }
        let unit = self.unit?;
        let material_id = self.material_id?;
        let material = project.materials.iter().find(|m| m.id == material_id)?;
        material.default_thickness.positive().ok()?;
        let length = self.length.value(unit).ok()?;
        let width = self.width.value(unit).ok()?;
        let pose = Pose::new([0.0; 3], Quaternion::IDENTITY).ok()?;
        let extents = [length, width, material.default_thickness]
            .map(|value| value.micrometres() as f64 / 1000.0);
        for x in [0.0, extents[0]] {
            for y in [0.0, extents[1]] {
                for z in [0.0, extents[2]] {
                    pose.transform_point([x, y, z]).ok()?;
                }
            }
        }
        Some(BoardPreviewKey {
            project_id: project.id,
            revision: project.revision,
            material_id,
            length,
            width,
            grain_override: self.grain_override,
        })
    }

    fn preview(&mut self, project: &Project, key: BoardPreviewKey) -> BoardPreview {
        if let Some((cached_key, preview)) = self.preview
            && cached_key == key
        {
            return preview;
        }
        let mut snapshot = project.clone();
        let preview = fit_new_board(&mut snapshot, self.name.clone(), key);
        self.preview = Some((key, preview));
        preview
    }
}

struct MaterialEditDialog {
    focus_on_open: bool,
    id: Uuid,
    name: String,
    thickness: DimensionDraft,
    grain: BoardGrain,
    anchor: Anchor,
    choice: Option<DependantChoice>,
    error: Option<MaterialChangeError>,
}

struct BoardMaterialDialog {
    focus_on_open: bool,
    board_id: Uuid,
    project_id: Uuid,
    revision: u64,
    material_id: Option<Uuid>,
    anchor: Anchor,
    error: Option<MaterialChangeError>,
}

struct BoardDimensionDialog {
    focus_on_open: bool,
    board_id: Uuid,
    project_id: Uuid,
    revision: u64,
    dimension: BoardDimension,
    value: DimensionDraft,
    anchor: Anchor,
    error: Option<DimensionEditError>,
}

struct BatchDialog {
    focus_on_open: bool,
    project_id: Uuid,
    revision: u64,
    ids: Vec<Uuid>,
    dimension: BoardDimension,
    value: DimensionDraft,
    anchors: Vec<(Uuid, Anchor)>,
    error: Option<BatchDimensionError>,
}

impl CreationDialog {
    fn board(material_id: Option<Uuid>) -> Self {
        Self {
            kind: DialogKind::Board,
            name: String::new(),
            length: DimensionDraft::new(),
            width: DimensionDraft::new(),
            thickness: DimensionDraft::new(),
            unit: None,
            project_id: None,
            material_id,
            grain: BoardGrain::Length,
            grain_override: None,
            color: None,
            preview: None,
            error: false,
        }
    }
    fn material() -> Self {
        Self {
            kind: DialogKind::Material,
            ..Self::board(None)
        }
    }
}

struct DesktopApp {
    capture: Option<capture::Capture>,
    preferences: LocalPreferences,
    preferences_store: Option<PreferencesStore>,
    preferences_error: Option<String>,
    settings_open: bool,
    settings_resume_after_dialog: bool,
    settings_state: SettingsState,
    settings_worked_examples: bool,
    settings_examples_chrome: Option<modal_chrome::ModalChrome>,
    settings_message: Option<String>,
    settings_cleanup: Option<CleanupUi>,
    template_setup: Option<TemplateSetupUi>,
    template_guard_pending: bool,
    template_message: Option<String>,
    // The viewport renders through this slot; the active workspace owns its
    // camera between frames. Project replacement resets the session.
    camera: viewport::Camera,
    session: WorkspaceSession,
    open_drawer: Option<workspace_shell::Drawer>,
    controls_horizontal_scroll: [f32; 5],
    inspector_scroll: [egui::Vec2; 5],
    navigation: NavigationGuard,
    navigation_chrome: ModalChrome,
    navigation_error: Option<&'static str>,
    edit_drafts: EditDrafts,
    pose_frame: CoordinateFrame,
    pending_pose_frame: Option<CoordinateFrame>,
    pending_action: Option<Request>,
    pending_project_command: Option<project_ui::PendingProjectCommand>,
    palette: command_palette::Palette,
    palette_relationship_pending: Option<Uuid>,
    move_tool: viewport::MoveTool,
    localizer: Localizer,
    editor: ProjectEditor,
    dialog: Option<CreationDialog>,
    suspended_board: Option<CreationDialog>,
    board_creation_chrome: ModalChrome,
    material_creation_chrome: ModalChrome,
    material_edit: Option<MaterialEditDialog>,
    material_edit_chrome: ModalChrome,
    board_material: Option<BoardMaterialDialog>,
    board_material_chrome: ModalChrome,
    board_dimension: Option<BoardDimensionDialog>,
    board_dimension_chrome: ModalChrome,
    batch_dimension: Option<BatchDialog>,
    batch_dimension_chrome: ModalChrome,
    placement: Option<PlacementDialog>,
    placement_chrome: ModalChrome,
    grid_dialog: Option<GridDialog>,
    kerf_confirmation: Option<kerf_confirmation_ui::KerfConfirmation>,
    grid_chrome: ModalChrome,
    stock_dialog: Option<stock_ui::StockDialog>,
    cut_fee_dialog: Option<String>,
    currency_dialog: Option<currency_ui::CurrencyDialog>,
    assembly_dialog: Option<assembly_ui::AssemblyDialog>,
    hardware_dialog: Option<hardware_ui::HardwareDialog>,
    hinge_dialog: Option<hinge_ui::HingeDialog>,
    door_dialog: Option<door_joint_ui::DoorDialog>,
    removal_dialog: Option<door_joint_ui::RemovalDialog>,
    door_motion: Option<(Uuid, f64)>,
    catalog_update_notice: Option<String>,
    sheet_repair: sheet_ui::RepairUi,
    optimizer: optimization_ui::OptimizeUi,
    selection: viewport::Selection,
    scene_active_seen: Option<Uuid>,
    measurement_scope: Scope,
    measurement_frame: Frame,
    board_action_error: bool,
    first_fit_notice: Option<FirstFit>,
    material_conflicts: Vec<AllocationConflict>,
    allocation_diagnostics: Option<(sheet_ui::DiagnosticsKey, Vec<BoardDiagnostic>)>,
    allocation_preview_diagnostics: Option<(sheet_ui::DiagnosticsKey, Vec<BoardDiagnostic>)>,
    design_stock_snapshot: Option<((Uuid, u64), StockReadModel)>,
    shell_estimate: Option<(
        (Uuid, u64),
        Result<
            plan_my_cabinet::cost_estimate::ProjectEstimate,
            plan_my_cabinet::cost_estimate::EstimateError,
        >,
    )>,
    export_mode: ExportMode,
    export_sections: ReceiptSections,
    export_preview: DocumentPreviewState,
    export_preparation: Option<ExportPreparationCache>,
    export_candidate: Option<ExportPreparationCache>,
    export_preparation_pending: Option<ExportPreparationJob>,
    export_language: Language,
    export_units: Unit,
    export_activity: Option<ExportActivity>,
    export_overwrite_chrome: ModalChrome,
    export_picker_key: Option<ReviewedPacketKey>,
    export_events: Option<Receiver<ExportEvent>>,
    export_cancel: Option<Arc<AtomicBool>>,
    export_message: Option<String>,
    project_files: project_ui::ProjectFiles,
}

impl Default for DesktopApp {
    fn default() -> Self {
        let editor =
            ProjectEditor::new(Project::new("Cabinet", Currency::Brl)).expect("empty project");
        Self {
            capture: None,
            preferences: LocalPreferences::default(),
            preferences_store: None,
            preferences_error: None,
            settings_open: false,
            settings_resume_after_dialog: false,
            settings_state: SettingsState::default(),
            settings_worked_examples: false,
            settings_examples_chrome: None,
            settings_message: None,
            settings_cleanup: None,
            template_setup: None,
            template_guard_pending: false,
            template_message: None,
            camera: viewport::Camera::default(),
            session: WorkspaceSession::new(editor.project()),
            open_drawer: None,
            controls_horizontal_scroll: [0.0; 5],
            inspector_scroll: [egui::Vec2::ZERO; 5],
            navigation: NavigationGuard::default(),
            navigation_chrome: ModalChrome::new(egui::Id::new("pending-navigation")).width(500.0),
            navigation_error: None,
            edit_drafts: EditDrafts::default(),
            pose_frame: CoordinateFrame::LocalParent,
            pending_pose_frame: None,
            pending_action: None,
            pending_project_command: None,
            palette: command_palette::Palette::default(),
            palette_relationship_pending: None,
            move_tool: viewport::MoveTool::default(),
            localizer: Localizer::new(Language::En),
            editor,
            dialog: None,
            suspended_board: None,
            board_creation_chrome: ModalChrome::new(egui::Id::new("board-creation-dialog"))
                .width(480.0)
                .icon(icons::Icon::Board)
                .first_focus(egui::Id::new("board-creation-name")),
            material_creation_chrome: ModalChrome::new(egui::Id::new("material-creation-dialog"))
                .width(400.0)
                .icon(icons::Icon::Material)
                .first_focus(egui::Id::new("material-creation-name")),
            material_edit: None,
            material_edit_chrome: ModalChrome::new(egui::Id::new("material-edit-dialog"))
                .width(540.0),
            board_material: None,
            board_material_chrome: ModalChrome::new(egui::Id::new("board-material-dialog"))
                .width(480.0),
            board_dimension: None,
            board_dimension_chrome: ModalChrome::new(egui::Id::new("board-dimension-dialog"))
                .width(520.0),
            batch_dimension: None,
            batch_dimension_chrome: ModalChrome::new(egui::Id::new("batch-dimension-dialog"))
                .width(440.0)
                .icon(icons::Icon::Measure)
                .first_focus(egui::Id::new("batch-dimension-value")),
            placement: None,
            placement_chrome: ModalChrome::new(egui::Id::new("placement-dialog")).width(520.0),
            grid_dialog: None,
            kerf_confirmation: None,
            grid_chrome: ModalChrome::new(egui::Id::new("grid-spacing-dialog"))
                .width(480.0)
                .first_focus(egui::Id::new("grid-spacing-value")),
            stock_dialog: None,
            cut_fee_dialog: None,
            currency_dialog: None,
            assembly_dialog: None,
            hardware_dialog: None,
            hinge_dialog: None,
            door_dialog: None,
            removal_dialog: None,
            door_motion: None,
            catalog_update_notice: None,
            sheet_repair: sheet_ui::RepairUi::default(),
            optimizer: optimization_ui::OptimizeUi::default(),
            selection: viewport::Selection::default(),
            scene_active_seen: None,
            measurement_scope: Scope::Body,
            measurement_frame: Frame::World,
            board_action_error: false,
            first_fit_notice: None,
            material_conflicts: Vec::new(),
            allocation_diagnostics: None,
            allocation_preview_diagnostics: None,
            design_stock_snapshot: None,
            shell_estimate: None,
            export_mode: ExportMode::Draft,
            export_sections: ReceiptSections::default(),
            export_preview: DocumentPreviewState::default(),
            export_preparation: None,
            export_candidate: None,
            export_preparation_pending: None,
            export_language: Language::En,
            export_units: Unit::Mm,
            export_activity: None,
            export_overwrite_chrome: ModalChrome::new(egui::Id::new("export-overwrite-dialog"))
                .width(520.0),
            export_picker_key: None,
            export_events: None,
            export_cancel: None,
            export_message: None,
            project_files: project_ui::ProjectFiles::default(),
        }
    }
}

fn error_key(error: InputError) -> &'static str {
    match error {
        InputError::GroupingSeparators => "error-grouping-separators",
        InputError::InvalidNumber => "error-invalid-number",
        InputError::InvalidFraction => "error-invalid-fraction",
        InputError::FractionRequiresInches => "error-fraction-requires-inches",
        InputError::Unit(UnitError::Overflow) => "error-overflow",
        InputError::Unit(UnitError::NonPositiveDimension) => "error-non-positive-dimension",
        InputError::Unit(UnitError::NonFinite) => "error-non-finite",
        InputError::Unit(UnitError::OutOfBounds) => "error-out-of-bounds",
        _ => "error-invalid-number",
    }
}

fn dimension_field(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    label: &str,
    field: &mut DimensionDraft,
    unit: Unit,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(localizer.text(label));
        if ui.text_edit_singleline(&mut field.text).changed() {
            field.consent = false;
        }
    });
    match parse_length(&field.text, unit)
        .and_then(|v| dimension(v.conversion).map_err(InputError::Unit))
    {
        Ok(parsed) => {
            let value = parsed.suggested();
            let locale = match localizer.language() {
                Language::En => Locale::En,
                Language::PtBr => Locale::PtBr,
            };
            if let Conversion::NeedsConfirmation(_) = parsed {
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("entered", field.text.as_str());
                args.set("rounded", format_length(value, Unit::Mm, locale, 3));
                ui.checkbox(
                    &mut field.consent,
                    localizer.format("rounding-confirmation", Some(&args)),
                );
            } else {
                ui.small(format_length(value, Unit::Mm, locale, 3));
            }
            field.value(unit).is_ok()
        }
        Err(error) => {
            if !field.text.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, localizer.text(error_key(error)));
            }
            false
        }
    }
}

fn creation_dimension_field(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    id: &'static str,
    label: &str,
    field: &mut DimensionDraft,
    unit: Unit,
) -> bool {
    ui.label(localizer.text(label));
    let error = parse_length(&field.text, unit)
        .and_then(|parsed| dimension(parsed.conversion).map_err(InputError::Unit));
    let error_label = error.as_ref().err().filter(|_| !field.text.is_empty());
    if theme_widgets::unit_field(
        ui,
        egui::Id::new(id),
        &localizer.text(label),
        &mut field.text,
        &localizer.text(unit_key(unit)),
        error_label
            .map(|error| localizer.text(error_key(*error)))
            .as_deref(),
    )
    .changed()
    {
        field.consent = false;
    }
    let Ok(parsed) = parse_length(&field.text, unit)
        .and_then(|parsed| dimension(parsed.conversion).map_err(InputError::Unit))
    else {
        return false;
    };
    let locale = match localizer.language() {
        Language::En => Locale::En,
        Language::PtBr => Locale::PtBr,
    };
    if matches!(parsed, Conversion::NeedsConfirmation(_)) {
        let mut args = FluentArgs::new();
        args.set("entered", field.text.as_str());
        args.set(
            "rounded",
            format_length(parsed.suggested(), Unit::Mm, locale, 3),
        );
        ui.checkbox(
            &mut field.consent,
            localizer.format("rounding-confirmation", Some(&args)),
        );
    } else {
        ui.small(format_length(parsed.suggested(), Unit::Mm, locale, 3));
    }
    field.value(unit).is_ok()
}

/// Swatch grid plus a free colour picker. Returns whether the value changed.
fn creation_color_swatches(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    color: &mut Option<SrgbColor>,
) -> bool {
    let before = *color;
    ui.label(
        egui::RichText::new(localizer.text("material-color"))
            .size(12.0)
            .color(theme_widgets::MUTED),
    );
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        let mut swatch = |ui: &mut egui::Ui, value: Option<SrgbColor>, label: String| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
            let selected = *color == value;
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, selected, &label)
            });
            let painter = ui.painter();
            if selected {
                painter.rect_stroke(
                    rect.expand(2.0),
                    7.0,
                    egui::Stroke::new(2.0, theme_widgets::TEXT),
                    egui::StrokeKind::Inside,
                );
            }
            let inner = rect.shrink(if selected { 3.0 } else { 1.0 });
            match value {
                Some(SrgbColor([r, g, b])) => {
                    painter.rect(
                        inner,
                        5.0,
                        egui::Color32::from_rgb(r, g, b),
                        egui::Stroke::new(1.0, egui::Color32::from_black_alpha(30)),
                        egui::StrokeKind::Inside,
                    );
                }
                None => {
                    painter.rect(
                        inner,
                        5.0,
                        theme_widgets::CARD,
                        egui::Stroke::new(1.0, theme_widgets::BORDER_STRONG),
                        egui::StrokeKind::Inside,
                    );
                    painter.line_segment(
                        [inner.left_bottom() + egui::vec2(4.0, -4.0), inner.right_top() + egui::vec2(-4.0, 4.0)],
                        egui::Stroke::new(1.5, theme_widgets::DANGER),
                    );
                }
            }
            if response.on_hover_text(label).clicked() {
                *color = value;
            }
        };
        swatch(ui, None, localizer.text("color-none"));
        for (key, value) in plan_my_cabinet::material_presets::SWATCHES {
            swatch(ui, Some(*value), localizer.text(key));
        }
        let mut rgb = color.map_or([200, 196, 187], |c| c.0);
        let custom = egui::color_picker::color_edit_button_srgb(ui, &mut rgb)
            .on_hover_text(localizer.text("color-custom"));
        if custom.changed() {
            *color = Some(SrgbColor(rgb));
        }
    });
    ui.label(
        egui::RichText::new(localizer.text("material-color-hint"))
            .size(11.5)
            .color(theme_widgets::FAINT),
    );
    *color != before
}

fn conflict_labels(localizer: &Localizer, conflict: &AllocationConflict) -> String {
    conflict
        .reasons
        .iter()
        .map(|reason| {
            localizer.text(match reason {
                ConflictReason::MaterialIdentity => "conflict-material-identity",
                ConflictReason::EffectiveThickness => "conflict-thickness",
                ConflictReason::Grain => "conflict-grain",
                ConflictReason::OutsideStock => "conflict-outside-stock",
                ConflictReason::Overlap => "conflict-overlap",
            })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn grain_key(grain: BoardGrain) -> &'static str {
    match grain {
        BoardGrain::Length => "grain-length",
        BoardGrain::Width => "grain-width",
        BoardGrain::Unrestricted => "grain-unrestricted",
    }
}

fn board_grain_key(grain: Option<BoardGrain>) -> &'static str {
    grain.map_or("grain-follow-default", grain_key)
}

fn combo_option<Value: PartialEq>(
    ui: &mut egui::Ui,
    current: &mut Value,
    value: Value,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let response = ui.selectable_value(current, value, text);
    if response.clicked() {
        ui.close();
    }
    response
}

#[cfg(test)]
fn dialog_escape(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !egui::Popup::is_any_open(ctx)
}

fn pose_error_label(localizer: &Localizer, error: UnitError) -> String {
    localizer.text(match error {
        UnitError::OutOfBounds => "error-out-of-bounds",
        UnitError::NonFinite => "error-non-finite",
        UnitError::Overflow => "error-overflow",
        _ => "error-invalid-number",
    })
}

impl DesktopApp {
    /// The desktop host owns the platform path. Capture runs do not call this
    /// initializer and therefore never read or write the user's preferences.
    fn load_preferences(&mut self, ctx: &egui::Context, config_dir: &Path) {
        match PreferencesStore::new(config_dir) {
            Ok(store) => {
                let loaded = store.load();
                self.preferences_error = loaded.warning.map(|error| error.to_string());
                self.preferences = loaded.preferences;
                self.preferences_store = Some(store);
            }
            Err(error) => self.preferences_error = Some(error.to_string()),
        }
        self.localizer.set_language(self.preferences.language);
        ctx.set_zoom_factor(self.preferences.interface_scale.factor());
    }

    fn change_preferences(&mut self, change: impl FnOnce(&mut LocalPreferences)) {
        let mut updated = self.preferences.clone();
        change(&mut updated);
        if updated == self.preferences {
            return;
        }
        self.preferences = updated;
        // An unsuccessful save leaves the change active for this session and
        // visibly reports that it will not survive restart. No project command
        // or revision is involved.
        self.preferences_error = match &self.preferences_store {
            Some(store) => store
                .save(&self.preferences)
                .err()
                .map(|error| error.to_string()),
            None => Some("A platform configuration directory is unavailable".into()),
        };
    }

    fn set_ui_language(&mut self, language: Language) {
        self.change_preferences(|preferences| preferences.language = language);
        self.localizer.set_language(language);
    }

    // These app-local setters are the narrow integration points for General
    // Settings (14.3). Their UI and hint/tint effects are not mounted yet.
    #[allow(dead_code)] // General Settings controls arrive in task 14.3.
    fn set_navigation_hints(&mut self, enabled: bool) {
        self.change_preferences(|preferences| preferences.navigation_hints = enabled);
    }

    #[allow(dead_code)] // General Settings controls arrive in task 14.3.
    fn set_inverse_scroll_zoom(&mut self, enabled: bool) {
        self.change_preferences(|preferences| preferences.inverse_scroll_zoom = enabled);
    }

    #[allow(dead_code)] // General Settings controls arrive in task 14.3.
    fn set_material_tint(&mut self, enabled: bool) {
        self.change_preferences(|preferences| preferences.material_tint = enabled);
    }

    #[allow(dead_code)] // General Settings controls arrive in task 14.3.
    fn set_interface_scale(&mut self, ctx: &egui::Context, scale: InterfaceScale) {
        self.change_preferences(|preferences| preferences.interface_scale = scale);
        ctx.set_zoom_factor(scale.factor());
    }

    fn settings_project_action(&mut self, action: A) {
        self.settings_open = false;
        if self.invoke(Request::new(action)).is_ok() {
            self.settings_resume_after_dialog = true;
        } else {
            self.settings_open = true;
        }
    }

    fn apply_settings_intent(&mut self, ctx: &egui::Context, intent: SettingsIntent) {
        match intent {
            SettingsIntent::Done => self.settings_open = false,
            SettingsIntent::EditKerf => self.settings_project_action(A::EditKerf),
            SettingsIntent::ConfirmKerf => self.settings_project_action(A::ConfirmKerf),
            SettingsIntent::EditGrid => self.settings_project_action(A::EditGrid),
            SettingsIntent::EditCutFee => self.settings_project_action(A::EditCutFee),
            SettingsIntent::ChangeCurrency => self.settings_project_action(A::EditCurrency),
            SettingsIntent::SetDisplayUnit(unit) => self.editor.set_display_unit(unit),
            SettingsIntent::SetFreeCutFee => {
                if let Ok(free) =
                    plan_my_cabinet::money::Money::new(self.editor.project().currency, 0)
                {
                    let _ = self.editor.set_cut_fee(Some(free));
                }
            }
            SettingsIntent::SetLanguage(language) => self.set_ui_language(language),
            SettingsIntent::SetNavigationHints(enabled) => self.set_navigation_hints(enabled),
            SettingsIntent::SetInverseScrollZoom(enabled) => self.set_inverse_scroll_zoom(enabled),
            SettingsIntent::SetMaterialTint(enabled) => self.set_material_tint(enabled),
            SettingsIntent::SetScale(scale) => self.set_interface_scale(ctx, scale),
            SettingsIntent::HelpShortcuts => {
                self.settings_state.section = SettingsSection::Shortcuts
            }
            SettingsIntent::Source => ctx.open_url(egui::OpenUrl::new_tab(APP_SOURCE_URL)),
            SettingsIntent::WorkedExamples => {
                self.settings_open = false;
                self.settings_worked_examples = true;
            }
            SettingsIntent::ShowRecoveryFolder => {
                let folder = self
                    .project_files
                    .user_data_dir
                    .clone()
                    .or_else(project_ui::user_data_dir)
                    .map(|root| root.join("recovery"));
                self.settings_message = Some(match folder {
                    Some(folder) if folder.is_dir() => {
                        #[cfg(target_os = "macos")]
                        let command = "open";
                        #[cfg(target_os = "linux")]
                        let command = "xdg-open";
                        #[cfg(target_os = "windows")]
                        let command = "explorer";
                        #[cfg(not(any(
                            target_os = "macos",
                            target_os = "linux",
                            target_os = "windows"
                        )))]
                        let command = "";
                        std::process::Command::new(command)
                            .arg(&folder)
                            .status()
                            .ok()
                            .filter(std::process::ExitStatus::success)
                            .map_or_else(
                                || self.localizer.text("settings-recovery-open-failed"),
                                |_| {
                                    format!(
                                        "{}: {}",
                                        self.localizer.text("settings-recovery-folder"),
                                        folder.display()
                                    )
                                },
                            )
                    }
                    _ => self.localizer.text("settings-recovery-no-folder"),
                });
            }
            SettingsIntent::ReviewRecoveryCleanup => {
                match self
                    .project_files
                    .user_data_dir
                    .clone()
                    .or_else(project_ui::user_data_dir)
                {
                    Some(dir) => match CleanupUi::open(&dir) {
                        Ok(review) => {
                            self.settings_open = false;
                            self.settings_cleanup = Some(review);
                            self.settings_message = None;
                        }
                        Err(error) => self.settings_message = Some(error.to_string()),
                    },
                    None => {
                        self.settings_message =
                            Some(self.localizer.text("settings-recovery-open-failed"))
                    }
                }
            }
        }
    }

    fn show_settings(&mut self, ctx: &egui::Context) {
        if let Some(cleanup) = &mut self.settings_cleanup {
            match cleanup.show(ctx, &self.localizer) {
                CleanupIntent::None => {}
                CleanupIntent::Closed => {
                    self.settings_cleanup = None;
                    self.settings_open = true;
                }
                CleanupIntent::OpenFolder(path) => {
                    #[cfg(target_os = "macos")]
                    let command = "open";
                    #[cfg(target_os = "linux")]
                    let command = "xdg-open";
                    #[cfg(target_os = "windows")]
                    let command = "explorer";
                    #[cfg(not(any(
                        target_os = "macos",
                        target_os = "linux",
                        target_os = "windows"
                    )))]
                    let command = "";
                    if !path.is_dir()
                        || !std::process::Command::new(command)
                            .arg(path)
                            .status()
                            .is_ok_and(|status| status.success())
                    {
                        self.settings_message =
                            Some(self.localizer.text("settings-recovery-open-failed"));
                    }
                }
            }
            return;
        }
        if self.settings_worked_examples {
            let guide = if self.localizer.language() == Language::En {
                include_str!("../docs/stock-en.md")
            } else {
                include_str!("../docs/stock-pt-BR.md")
            };
            let excerpt = guide
                .split_once(if self.localizer.language() == Language::En {
                    "## Cutting assumptions and worked examples"
                } else {
                    "## Premissas de corte e exemplos"
                })
                .map_or(guide, |(_, text)| text);
            let chrome = self.settings_examples_chrome.get_or_insert_with(|| {
                modal_chrome::ModalChrome::new(egui::Id::new("settings-worked-examples"))
                    .width(760.0)
            });
            let result = chrome.show_reader(
                ctx,
                &self.localizer.text("settings-worked-examples"),
                &self.localizer.text("settings-back"),
                |ui| {
                    for paragraph in excerpt.split("\n\n") {
                        ui.label(paragraph);
                        ui.add_space(8.0);
                    }
                },
            );
            if result.action == modal_chrome::ModalAction::Cancel {
                chrome.close(ctx);
                self.settings_examples_chrome = None;
                self.settings_worked_examples = false;
                self.settings_open = true;
            }
            return;
        }
        if self.settings_resume_after_dialog
            && !self.other_modal_open()
            && !self.project_files.blocking()
            && self.navigation.pending().is_none()
        {
            self.settings_resume_after_dialog = false;
            self.settings_open = true;
        }
        if self.settings_open {
            let project = (self.capture.is_some() || !self.project_files.welcome.visible)
                .then(|| self.editor.project());
            let estimate = project.and_then(|p| plan_my_cabinet::cost_estimate::estimate(p).ok());
            let intents = self.settings_state.show(
                ctx,
                &self.localizer,
                project,
                &self.preferences,
                estimate.as_ref(),
                self.preferences_error.as_deref(),
                self.settings_message.as_deref(),
            );
            for intent in intents {
                self.apply_settings_intent(ctx, intent);
            }
        }
    }

    fn show_template_setup(&mut self, ctx: &egui::Context) {
        if self.template_guard_pending {
            if self.project_files.prompt.is_none()
                && !self.project_files.blocking()
                && self.navigation.pending().is_none()
                && self.pending_project_command.is_none()
            {
                self.template_guard_pending = false;
            } else {
                return;
            }
        }
        let Some(setup) = &mut self.template_setup else {
            return;
        };
        if let Some(message) = &self.template_message {
            egui::Area::new(egui::Id::new("template-setup-error"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -12.0))
                .show(ctx, |ui| {
                    ui.colored_label(theme_widgets::WARN_INK, message);
                });
        }
        match setup.show(ctx, &self.localizer) {
            Some(TemplateSetupIntent::Cancel) => {
                self.template_setup = None;
                self.template_message = None;
            }
            Some(TemplateSetupIntent::Accept(accepted)) => {
                setup.setup = accepted;
                self.template_guard_pending = true;
                self.request_project_action(project_ui::NextAction::Template);
            }
            None => {}
        }
    }

    // Every mounted draft is owned by the app session, not either editing surface.
    fn navigation_edit(&mut self) -> Option<EditBlock> {
        let (kind, target, can_commit) = if self.sheet_repair.active() {
            (
                EditKind::Preview,
                self.session.focused_sheet.map(InspectorTarget::Sheet),
                self.sheet_repair.can_accept(&mut self.editor),
            )
        } else if let Some(draft) = &self.board_dimension {
            let project = self.editor.project();
            let valid = project.id == draft.project_id
                && project.revision == draft.revision
                && draft.value.value(project.display_unit).is_ok_and(|value| {
                    self.editor
                        .preview_board_dimension(
                            draft.board_id,
                            draft.dimension,
                            value,
                            draft.anchor,
                        )
                        .is_ok()
                });
            (
                EditKind::Field,
                Some(InspectorTarget::Board(draft.board_id)),
                valid,
            )
        } else if let Some(placement) = &self.placement {
            (
                EditKind::Preview,
                Some(InspectorTarget::Board(placement.board_id)),
                placement.can_accept_navigation(self.editor.project()),
            )
        } else if self.editor.preview().is_some() {
            (
                EditKind::Preview,
                self.selection.active.map(InspectorTarget::Board),
                self.selection.active.is_some_and(|id| {
                    self.editor
                        .project()
                        .boards
                        .iter()
                        .any(|board| board.id == id)
                }),
            )
        } else if let Some(InspectorTarget::Board(id)) = self.session.inspector {
            if let Some(draft) = self
                .edit_drafts
                .existing_board(self.editor.project().id, id)
                .filter(|draft| draft.dirty())
            {
                (
                    EditKind::Field,
                    Some(InspectorTarget::Board(id)),
                    draft.preview(&self.editor).is_ok(),
                )
            } else if let Some(draft) = self
                .edit_drafts
                .existing_pose(self.editor.project().id, id)
                .filter(|draft| draft.dirty())
            {
                (
                    EditKind::Preview,
                    Some(InspectorTarget::Board(id)),
                    draft.preview(&self.editor).is_ok(),
                )
            } else {
                return None;
            }
        } else {
            return None;
        };
        Some(EditBlock {
            kind,
            source: Revision::of(self.editor.project()),
            workspace: self.session.active,
            target,
            can_commit,
        })
    }

    fn request_navigation(&mut self, route: NavigationRoute) -> Outcome {
        if self.project_files.blocking()
            || self.other_modal_open() && self.placement.is_none() && self.board_dimension.is_none()
        {
            return Outcome::Blocked(pending_navigation::Blocked::PendingDecision);
        }
        let block = self.navigation_edit();
        let outcome = self.navigation.request(
            NavigationIntent::at(self.editor.project(), route),
            block,
            self.editor.project(),
            &mut self.session,
            &mut self.selection,
        );
        self.after_navigation(outcome);
        outcome
    }

    fn request_scene_selection(&mut self, target: Option<Uuid>, additive: bool) -> Outcome {
        if self.navigation.pending().is_none()
            && !self.other_modal_open()
            && !additive
            && self.selection.active == target
            && self.selection.ids.len() == usize::from(target.is_some())
        {
            return Outcome::Navigated;
        }
        self.request_navigation(NavigationRoute::Selection { target, additive })
    }

    fn request_pose_frame(&mut self, frame: CoordinateFrame) {
        if frame == self.pose_frame {
            return;
        }
        if self.navigation_edit().is_none() {
            self.pose_frame = frame;
        } else if matches!(
            self.request_navigation(NavigationRoute::Workspace(self.session.active)),
            Outcome::Prompt { .. }
        ) {
            self.pending_pose_frame = Some(frame);
        }
    }

    /// Defer an incompatible command until the shared board edit has been
    /// explicitly resolved. The command is revalidated by `invoke` afterwards.
    fn resolve_draft_before_action(
        &mut self,
        request: Request,
    ) -> Result<bool, actions::Unavailable> {
        if self.navigation.pending().is_some() {
            return Err(actions::Unavailable::PendingEdit);
        }
        let Some(InspectorTarget::Board(id)) = self.session.inspector else {
            return Ok(false);
        };
        let dirty_dimensions = self
            .edit_drafts
            .existing_board(self.editor.project().id, id)
            .is_some_and(|draft| draft.dirty());
        let dirty_pose = self
            .edit_drafts
            .existing_pose(self.editor.project().id, id)
            .is_some_and(|draft| draft.dirty());
        if !dirty_dimensions && !dirty_pose {
            return Ok(false);
        }
        match self.request_navigation(NavigationRoute::Workspace(self.session.active)) {
            Outcome::Prompt { .. } => {
                self.pending_action = Some(request);
                Ok(true)
            }
            Outcome::Blocked(_) => Err(actions::Unavailable::PendingEdit),
            Outcome::Navigated | Outcome::Stayed => Ok(false),
        }
    }

    fn after_navigation(&mut self, outcome: Outcome) {
        if outcome == Outcome::Navigated {
            self.scene_active_seen = self.selection.active;
            if self.session.active != Workspace::Hardware {
                self.door_motion = None;
            }
            self.navigation_error = None;
            if let Some(id) = self.palette_relationship_pending.take()
                && self.session.active == Workspace::Hardware
            {
                let _ = self.invoke(Request::with(A::EditDoor, Target::Door(id)));
            }
        } else if self.navigation.pending().is_none() {
            self.palette_relationship_pending = None;
        }
    }

    fn resolve_navigation(&mut self, decision: NavigationDecision) -> Outcome {
        let block = self.navigation_edit();
        let draft_board = match block.and_then(|edit| edit.target) {
            Some(InspectorTarget::Board(id))
                if self
                    .edit_drafts
                    .existing_board(self.editor.project().id, id)
                    .is_some_and(|draft| draft.dirty()) =>
            {
                Some(id)
            }
            _ => None,
        };
        let draft_pose = match block {
            Some(EditBlock {
                kind: EditKind::Preview,
                target: Some(InspectorTarget::Board(id)),
                ..
            }) if self
                .edit_drafts
                .existing_pose(self.editor.project().id, id)
                .is_some_and(|draft| draft.dirty()) =>
            {
                Some(id)
            }
            _ => None,
        };
        let return_selection = self
            .placement
            .as_ref()
            .map(|dialog| (dialog.selection_ids.clone(), dialog.selection_active));
        let workspace_route = matches!(
            self.navigation.pending().map(|intent| intent.route),
            Some(NavigationRoute::Workspace(_))
        );
        let repair = &mut self.sheet_repair;
        let board_dimension = &mut self.board_dimension;
        let placement = &mut self.placement;
        let drafts = &mut self.edit_drafts;
        let mut conflicts = None;
        let outcome = self.navigation.resolve(
            decision,
            block,
            &mut self.editor,
            &mut self.session,
            &mut self.selection,
            |decision, editor| {
                if repair.active() {
                    if decision == NavigationDecision::Commit {
                        repair.accept_navigation(editor)?;
                    } else {
                        repair.cancel_navigation(editor);
                    }
                } else if let Some(draft) = board_dimension.as_mut() {
                    if decision == NavigationDecision::Commit {
                        let value = draft
                            .value
                            .value(editor.project().display_unit)
                            .map_err(|_| ())?;
                        let preview = editor
                            .preview_board_dimension(
                                draft.board_id,
                                draft.dimension,
                                value,
                                draft.anchor,
                            )
                            .map_err(|error| {
                                draft.error = Some(error);
                            })?;
                        conflicts =
                            Some(editor.edit_board_dimension(preview).map_err(|error| {
                                draft.error = Some(match error {
                                    plan_my_cabinet::commands::EditError::Command(reason) => reason,
                                    _ => DimensionEditError::StalePreview,
                                });
                            })?);
                    }
                    *board_dimension = None;
                } else if placement.is_some() {
                    if decision == NavigationDecision::Commit && editor.preview().is_some() {
                        editor.commit_preview().map_err(|_| ())?;
                    } else {
                        editor.cancel_preview();
                    }
                    *placement = None;
                } else if editor.preview().is_some() {
                    if decision == NavigationDecision::Commit {
                        editor.commit_preview().map_err(|_| ())?;
                    } else {
                        editor.cancel_preview();
                    }
                    self.move_tool.cancel();
                } else if let Some(id) = draft_board {
                    if decision == NavigationDecision::Commit {
                        drafts
                            .existing_board_mut(editor.project().id, id)
                            .ok_or(())?
                            .accept(editor)
                            .map_err(|_| ())?;
                    }
                    drafts.cancel_board(editor.project().id, id);
                } else if let Some(id) = draft_pose {
                    if decision == NavigationDecision::Commit {
                        drafts
                            .existing_pose_mut(editor.project().id, id)
                            .ok_or(())?
                            .accept(editor)
                            .map_err(|_| ())?;
                    }
                    drafts.cancel_pose(editor.project().id, id);
                }
                Ok::<_, ()>(())
            },
        );
        match outcome {
            Ok(outcome) => {
                if let Some(changes) = conflicts {
                    self.material_conflicts = changes;
                }
                if outcome == Outcome::Navigated
                    && decision == NavigationDecision::Abandon
                    && workspace_route
                    && let Some((ids, active)) = return_selection
                {
                    self.selection.ids = ids;
                    self.selection.active = active;
                }
                self.after_navigation(outcome);
                self.navigation_error = match outcome {
                    Outcome::Blocked(pending_navigation::Blocked::MissingDestination) => {
                        Some("navigation-missing")
                    }
                    Outcome::Blocked(
                        pending_navigation::Blocked::StaleSource
                        | pending_navigation::Blocked::StaleEdit,
                    ) => Some("navigation-stale"),
                    _ => None,
                };
                if outcome == Outcome::Navigated {
                    if let Some(frame) = self.pending_pose_frame.take() {
                        self.pose_frame = frame;
                    }
                    if let Some(request) = self.pending_action.take()
                        && self.invoke(request).is_err()
                    {
                        self.navigation_error = Some("navigation-accept-error");
                    }
                    if let Some(command) = self.pending_project_command.take() {
                        match command {
                            project_ui::PendingProjectCommand::Action(action) => {
                                self.request_project_action(action);
                            }
                            project_ui::PendingProjectCommand::Save(as_new) => {
                                self.request_save(as_new);
                            }
                        }
                    }
                } else if self.navigation.pending().is_none() {
                    self.pending_action = None;
                    self.pending_project_command = None;
                    self.pending_pose_frame = None;
                }
                outcome
            }
            Err(_) => {
                self.navigation_error = Some("navigation-accept-error");
                Outcome::Stayed
            }
        }
    }

    fn show_navigation_prompt(&mut self, ctx: &egui::Context) {
        if self.navigation.pending().is_none() {
            if self.navigation_chrome.is_active() {
                self.navigation_chrome.close(ctx);
            }
            return;
        }
        let block = self.navigation_edit();
        let kind = block.map_or(EditKind::Preview, |edit| edit.kind);
        let title = self.localizer.text("navigation-title");
        let description = self.localizer.text(if kind == EditKind::Field {
            "navigation-field"
        } else {
            "navigation-preview"
        });
        let primary = self.localizer.text(if kind == EditKind::Field {
            "navigation-apply"
        } else {
            "navigation-accept"
        });
        let secondary = self.localizer.text(if kind == EditKind::Field {
            "navigation-discard"
        } else {
            "navigation-cancel"
        });
        let stay = self.localizer.text("navigation-stay");
        let error = self.navigation_error.map(|key| self.localizer.text(key));
        let valid = block.is_some_and(|edit| edit.can_commit);
        let action = self
            .navigation_chrome
            .show_three(
                ctx,
                &title,
                ModalThreeActions {
                    primary: &primary,
                    secondary: &secondary,
                    stay: &stay,
                },
                |ui| {
                    ui.label(&description);
                    if let Some(error) = &error {
                        ui.colored_label(egui::Color32::LIGHT_RED, error);
                    }
                    ((), valid)
                },
            )
            .action;
        let choice = match action {
            ModalThreeAction::None => None,
            ModalThreeAction::Primary => Some(NavigationDecision::Commit),
            ModalThreeAction::Secondary => Some(NavigationDecision::Abandon),
            ModalThreeAction::Stay => Some(NavigationDecision::Stay),
        };
        if let Some(choice) = choice {
            self.resolve_navigation(choice);
            if self.navigation.pending().is_none() {
                self.navigation_chrome.close(ctx);
            }
        }
    }

    fn navigate_session(&mut self, destination: Destination) -> bool {
        matches!(
            self.request_navigation(NavigationRoute::Entity(destination)),
            Outcome::Navigated | Outcome::Prompt { .. }
        )
    }

    fn sync_scene_inspector(&mut self) {
        if !self.session.belongs_to(self.editor.project()) {
            self.navigation.clear();
            self.edit_drafts.clear();
            self.pending_action = None;
            self.pending_project_command = None;
            self.pending_pose_frame = None;
            self.navigation_error = None;
            self.palette_relationship_pending = None;
            self.open_drawer = None;
            self.controls_horizontal_scroll = [0.0; 5];
            self.inspector_scroll = [egui::Vec2::ZERO; 5];
            self.design_stock_snapshot = None;
        }
        self.session.retain_existing(self.editor.project());
        self.selection.retain_objects(self.editor.project());
        if self.scene_active_seen != self.selection.active {
            self.scene_active_seen = self.selection.active;
            self.session.inspector = self.selection.active.and_then(|id| {
                self.editor
                    .project()
                    .boards
                    .iter()
                    .any(|board| board.id == id)
                    .then_some(InspectorTarget::Board(id))
            });
        }
    }

    fn design_model(&mut self) -> Option<DesignReadModel> {
        let project = self.editor.project();
        let key = (project.id, project.revision);
        if self
            .design_stock_snapshot
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            self.design_stock_snapshot = StockReadModel::build(project)
                .ok()
                .map(|model| (key, model));
        }
        let stock = &self.design_stock_snapshot.as_ref()?.1;
        DesignReadModel::build(
            project,
            stock,
            DesignView {
                selected: &self.selection.ids,
                active: self.selection.active,
                hidden: &self.selection.hidden,
                expanded: &self.session.design_expanded,
            },
        )
        .ok()
    }

    /// Advanced ID-based routes stay reachable beside contextual workspace controls.
    fn show_session_routes(&mut self, ui: &mut egui::Ui) {
        let project = self.editor.project();
        let material_rows: Vec<_> = project
            .materials
            .iter()
            .map(|m| (m.id, m.name.clone()))
            .collect();
        let sheet_rows: Vec<_> = project
            .ordered_stock()
            .iter()
            .map(|s| (s.id, stock_reference(project, s.id)))
            .collect();
        let installation_rows: Vec<_> = project
            .hinge_installations
            .iter()
            .map(|h| (h.id, h.door_board_id))
            .collect();
        if let Some(id) = self.selection.active
            && self.editor.project().boards.iter().any(|b| b.id == id)
            && ui
                .add_enabled(
                    !self.other_modal_open() && !self.project_files.blocking(),
                    egui::Button::new(self.localizer.text("sheet-heading")),
                )
                .on_hover_text(id.to_string())
                .clicked()
        {
            self.navigate_session(Destination::BoardAllocation(id));
        }
        ui.collapsing(self.localizer.text("material-list"), |ui| {
            for (id, name) in &material_rows {
                if ui
                    .add_enabled(
                        !self.other_modal_open() && !self.project_files.blocking(),
                        egui::Button::new(format!("{name} ({id})")).selected(
                            self.session.inspector == Some(InspectorTarget::Material(*id)),
                        ),
                    )
                    .clicked()
                {
                    self.navigate_session(Destination::Material(*id));
                }
            }
        });
        ui.collapsing(self.localizer.text("sheet-heading"), |ui| {
            for (id, label) in &sheet_rows {
                if ui
                    .add_enabled(
                        !self.other_modal_open() && !self.project_files.blocking(),
                        egui::Button::new(label).selected(self.session.focused_sheet == Some(*id)),
                    )
                    .clicked()
                {
                    self.navigate_session(Destination::Sheet(*id));
                }
            }
        });
        ui.collapsing(self.localizer.text("hinge-list"), |ui| {
            for (id, door_id) in &installation_rows {
                if ui
                    .add_enabled(
                        !self.other_modal_open() && !self.project_files.blocking(),
                        egui::Button::new(format!("{door_id} ({id})")).selected(
                            self.session.inspector == Some(InspectorTarget::Installation(*id)),
                        ),
                    )
                    .clicked()
                {
                    self.navigate_session(Destination::Installation(*id));
                }
            }
        });
    }

    fn show_measurement(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.localizer.text("measurement-heading"));
        ui.label(self.localizer.text("measurement-selection"));
        let mut scope_choice = self.measurement_scope;
        let mut frame_choice = self.measurement_frame;
        ui.add_enabled_ui(!self.modal_open(), |ui| {
            ui.vertical(|ui| {
                ui.selectable_value(
                    &mut scope_choice,
                    Scope::Body,
                    self.localizer.text("measurement-body"),
                );
                ui.selectable_value(
                    &mut scope_choice,
                    Scope::Overall,
                    self.localizer.text("measurement-overall"),
                );
            });
            let frame_label = match self.measurement_frame {
                Frame::World => self.localizer.text("placement-world"),
                Frame::Object(id) => self
                    .editor
                    .project()
                    .assemblies
                    .iter()
                    .map(|a| (a.id, &a.name))
                    .chain(self.editor.project().boards.iter().map(|b| (b.id, &b.name)))
                    .find(|(object, _)| *object == id)
                    .map(|(_, name)| format!("{name} ({})", &id.to_string()[..8]))
                    .unwrap_or_else(|| self.localizer.text("placement-world")),
            };
            egui::ComboBox::from_id_salt("measurement-frame")
                .selected_text(format!(
                    "{}: {frame_label}",
                    self.localizer.text("measurement-frame")
                ))
                .show_ui(ui, |ui| {
                    combo_option(
                        ui,
                        &mut frame_choice,
                        Frame::World,
                        self.localizer.text("placement-world"),
                    );
                    for a in &self.editor.project().assemblies {
                        combo_option(
                            ui,
                            &mut frame_choice,
                            Frame::Object(a.id),
                            format!(
                                "{}: {} ({})",
                                self.localizer.text("assembly-kind"),
                                a.name,
                                &a.id.to_string()[..8]
                            ),
                        );
                    }
                    for b in &self.editor.project().boards {
                        combo_option(
                            ui,
                            &mut frame_choice,
                            Frame::Object(b.id),
                            format!(
                                "{}: {} ({})",
                                self.localizer.text("board-kind"),
                                b.name,
                                &b.id.to_string()[..8]
                            ),
                        );
                    }
                });
        });
        if scope_choice != self.measurement_scope {
            let _ = self.invoke(
                Request::new(A::SetMeasurementScope).argument(Argument::Scope(scope_choice)),
            );
        }
        if frame_choice != self.measurement_frame {
            let _ = self.invoke(
                Request::new(A::SetMeasurementFrame).argument(Argument::Frame(frame_choice)),
            );
        }
        if let Frame::Object(id) = self.measurement_frame
            && !self.editor.project().assemblies.iter().any(|a| a.id == id)
            && !self.editor.project().boards.iter().any(|b| b.id == id)
        {
            self.measurement_frame = Frame::World;
        }
        let ids: Vec<_> = self.selection.ids.iter().copied().collect();
        let mut names: Vec<_> = ids
            .iter()
            .filter_map(|id| {
                self.editor
                    .project()
                    .assemblies
                    .iter()
                    .map(|a| (a.id, "assembly-kind", a.name.as_str()))
                    .chain(
                        self.editor
                            .project()
                            .boards
                            .iter()
                            .map(|b| (b.id, "board-kind", b.name.as_str())),
                    )
                    .chain(
                        self.editor
                            .project()
                            .hardware
                            .iter()
                            .map(|h| (h.id, "hardware-kind", h.name.as_str())),
                    )
                    .find(|(candidate, _, _)| candidate == id)
                    .map(|(id, kind, name)| {
                        format!(
                            "{} {name} ({})",
                            self.localizer.text(kind),
                            &id.to_string()[..8]
                        )
                    })
            })
            .collect();
        names.sort();
        ui.small(format!(
            "{}: {}",
            self.localizer.text("measurement-objects"),
            names.join(", ")
        ));
        let (heading, content) = viewport::measurement_readout(
            self.editor.project(),
            &self.selection,
            self.measurement_scope,
            self.measurement_frame,
            &self.localizer,
        );
        ui.label(heading);
        ui.label(content);
    }

    fn other_modal_open(&self) -> bool {
        self.settings_open
            || self.settings_worked_examples
            || self.settings_cleanup.is_some()
            || (self.template_setup.is_some() && !self.template_guard_pending)
            || self.dialog.is_some()
            || self.material_edit.is_some()
            || self.board_material.is_some()
            || self.board_dimension.is_some()
            || self.batch_dimension.is_some()
            || self.placement.is_some()
            || self.grid_dialog.is_some()
            || self.kerf_confirmation.is_some()
            || self.stock_dialog.is_some()
            || self.cut_fee_dialog.is_some()
            || self.currency_dialog.is_some()
            || self.assembly_dialog.is_some()
            || self.hardware_dialog.is_some()
            || self.hinge_dialog.is_some()
            || self.door_dialog.is_some()
            || self.removal_dialog.is_some()
            || matches!(self.export_activity, Some(ExportActivity::Confirming(..)))
    }

    fn modal_open(&self) -> bool {
        self.external_modal_open() || self.optimizer.comparison_open()
    }

    fn external_modal_open(&self) -> bool {
        self.palette.open
            || self.navigation.pending().is_some()
            || self.other_modal_open()
            || self.sheet_repair.active()
            || self.door_motion.is_some()
            || self.project_files.blocking()
    }

    /// Mounted overlays block raw scene input even on their opening frame,
    /// before egui has established the new modal layer. Motion and repair are
    /// scene modes, not overlays; their hosts apply their own restrictions.
    fn blocking_surface_open(&self) -> bool {
        self.palette.open
            || self.navigation.pending().is_some()
            || self.other_modal_open()
            || self.project_files.blocking()
            || self.optimizer.comparison_open()
    }

    fn design_hud_available(&self) -> bool {
        self.session.active == Workspace::Design && self.open_drawer.is_none() && !self.modal_open()
    }

    fn show_grid_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.grid_dialog.take() else {
            // A draft can be dismissed by project replacement or a caller that
            // clears dialogs directly. Do not leave a modal layer or focus trap
            // alive after its owning draft is gone.
            if self.grid_chrome.is_active() {
                self.grid_chrome.close(ctx);
            }
            return;
        };
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let current = self.editor.project().id == draft.project_id
            && self.editor.project().revision == draft.revision;
        let mut proposal = None;
        let title = self.localizer.text(if draft.kerf {
            "cutting-kerf"
        } else {
            "grid-spacing"
        });
        let cancel_text = self.localizer.text("cancel");
        let confirm_text = self.localizer.text("confirm");
        let result = self.grid_chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &cancel_text,
                confirm: &confirm_text,
            },
            |ui| {
                ui.label(self.localizer.text(if draft.kerf {
                    "cutting-kerf"
                } else {
                    "grid-spacing"
                }));
                ui.label(self.localizer.text(if draft.kerf {
                    "cutting-kerf-hint"
                } else {
                    "grid-spacing-hint"
                }));
                let combo = egui::ComboBox::from_label(self.localizer.text("grid-input-unit"))
                    .selected_text(self.localizer.text(if draft.unit == Unit::Inch {
                        "unit-in"
                    } else {
                        "unit-mm"
                    }))
                    .show_ui(ui, |ui| {
                        combo_option(
                            ui,
                            &mut draft.unit,
                            Unit::Mm,
                            self.localizer.text("unit-mm"),
                        );
                        combo_option(
                            ui,
                            &mut draft.unit,
                            Unit::Inch,
                            self.localizer.text("unit-in"),
                        );
                    });
                let _ = combo;
                ui.horizontal(|ui| {
                    ui.label(self.localizer.text(if draft.kerf {
                        "cutting-kerf"
                    } else {
                        "grid-spacing"
                    }));
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut draft.value.text)
                                .id(egui::Id::new("grid-spacing-value")),
                        )
                        .changed()
                    {
                        draft.value.consent = false;
                        draft.error = false;
                    }
                });
                let unchanged =
                    draft.value.text == format_length(draft.original, Unit::Mm, locale, 3);
                match parse_length(&draft.value.text, draft.unit)
                    .and_then(|v| dimension(v.conversion).map_err(InputError::Unit))
                {
                    Ok(parsed) => {
                        let value = parsed.suggested();
                        if let Err(error) = if draft.kerf {
                            value.positive().map(|_| ())
                        } else {
                            validate_grid_spacing(value)
                        } {
                            ui.colored_label(
                                egui::Color32::LIGHT_RED,
                                self.localizer.text(match error {
                                    UnitError::OutOfBounds => "grid-spacing-range",
                                    _ => "error-non-positive-dimension",
                                }),
                            );
                        } else {
                            if matches!(parsed, Conversion::NeedsConfirmation(_)) && !unchanged {
                                let mut args = fluent_bundle::FluentArgs::new();
                                args.set("entered", draft.value.text.as_str());
                                args.set("rounded", format_length(value, Unit::Mm, locale, 3));
                                ui.checkbox(
                                    &mut draft.value.consent,
                                    self.localizer.format("rounding-confirmation", Some(&args)),
                                );
                            } else {
                                ui.small(format_length(value, Unit::Mm, locale, 3));
                            }
                            if unchanged || parsed.exact().is_some() || draft.value.consent {
                                proposal = Some(if unchanged { draft.original } else { value });
                            }
                        }
                    }
                    Err(error) => {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            self.localizer.text(error_key(error)),
                        );
                    }
                }
                if !current {
                    ui.colored_label(egui::Color32::LIGHT_RED, self.localizer.text("grid-stale"));
                }
                if draft.error {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text(if draft.kerf {
                            "cutting-kerf-invalid"
                        } else {
                            "grid-invalid"
                        }),
                    );
                }
                (proposal, current && proposal.is_some())
            },
        );
        draft.focus_on_open = false;
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            self.grid_chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm)
            && let Some(value) = result.body
        {
            if (if draft.kerf {
                self.editor.set_cutting_kerf(value)
            } else {
                self.editor.set_grid_spacing(value)
            })
            .is_ok()
            {
                self.grid_chrome.close(ctx);
                return;
            }
            draft.error = true;
        }
        self.grid_dialog = Some(draft);
    }

    fn show_batch_dimension(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.batch_dimension.take() else {
            return;
        };
        let mut accepted_preview = None;
        let project = self.editor.project();
        let current = project.id == draft.project_id && project.revision == draft.revision;
        let unit = project.display_unit;
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let selection: Vec<_> = draft
            .ids
            .iter()
            .copied()
            .map(BoardSelection::Board)
            .collect();
        let mut count = FluentArgs::new();
        count.set("count", draft.ids.len() as i64);
        let title = self.localizer.format("board-resize-title", Some(&count));
        let confirm_label = self.localizer.format("board-resize-action", Some(&count));
        let result = self.batch_dimension_chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &confirm_label,
            },
            |ui| {
                ui.label(
                    draft
                        .ids
                        .iter()
                        .filter_map(|id| {
                            project
                                .boards
                                .iter()
                                .find(|b| b.id == *id)
                                .map(|b| b.name.as_str())
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                let mut summary = None;
                if current {
                    summary = self.editor.selected_boards(&selection).ok();
                }
                ui.label(self.localizer.text("board-local-dimension"));
                let old_axis = draft.dimension;
                theme_widgets::segmented(
                    ui,
                    &mut draft.dimension,
                    &[
                        (BoardDimension::Length, &self.localizer.text("board-length")),
                        (BoardDimension::Width, &self.localizer.text("board-width")),
                        (
                            BoardDimension::Thickness,
                            &self.localizer.text("board-thickness"),
                        ),
                    ],
                );
                if draft.dimension != old_axis {
                    draft.value = DimensionDraft::new();
                    draft.error = None;
                }
                ui.label(self.localizer.text("board-resize-anchor"));
                let common = draft.anchors.first().and_then(|(_, first)| {
                    draft
                        .anchors
                        .iter()
                        .all(|(_, anchor)| anchor == first)
                        .then_some(*first)
                });
                let mut chosen = common;
                theme_widgets::segmented(
                    ui,
                    &mut chosen,
                    &[
                        (Some(Anchor::Start), &self.localizer.text("anchor-start")),
                        (Some(Anchor::Centre), &self.localizer.text("anchor-centre")),
                        (Some(Anchor::End), &self.localizer.text("anchor-end")),
                    ],
                );
                if chosen != common
                    && let Some(anchor) = chosen
                {
                    for (_, value) in &mut draft.anchors {
                        *value = anchor;
                    }
                    draft.error = None;
                }
                if let Some(summary) = &summary {
                    let text = match summary.dimensions[draft.dimension.axis()] {
                        SelectionValue::Uniform(value) => format_length(value, Unit::Mm, locale, 3),
                        SelectionValue::Mixed => format!(
                            "{} ({})",
                            self.localizer.text("board-mixed"),
                            draft
                                .ids
                                .iter()
                                .filter_map(|id| project.boards.iter().find(|b| b.id == *id))
                                .map(|board| format_length(
                                    board.blank_dimensions()[draft.dimension.axis()],
                                    Unit::Mm,
                                    locale,
                                    3
                                ))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    };
                    ui.label(format!(
                        "{}: {text}",
                        self.localizer.text("board-current-value")
                    ));
                }
                ui.label(self.localizer.text("board-dimension-preview"));
                let value_id = egui::Id::new("batch-dimension-value");
                if theme_widgets::unit_field(
                    ui,
                    value_id,
                    &self.localizer.text("board-dimension-preview"),
                    &mut draft.value.text,
                    &self.localizer.text(unit_key(unit)),
                    None,
                )
                .changed()
                {
                    draft.value.consent = false;
                    draft.error = None;
                }
                let parsed = parse_length(&draft.value.text, unit)
                    .and_then(|value| dimension(value.conversion).map_err(InputError::Unit));
                if let Ok(Conversion::NeedsConfirmation(value)) = &parsed {
                    let mut args = FluentArgs::new();
                    args.set("entered", draft.value.text.as_str());
                    args.set("rounded", format_length(*value, Unit::Mm, locale, 3));
                    theme_widgets::card().show(ui, |ui| {
                        ui.checkbox(
                            &mut draft.value.consent,
                            self.localizer.format("rounding-confirmation", Some(&args)),
                        );
                    });
                } else if let Err(error) = parsed
                    && !draft.value.text.is_empty()
                {
                    ui.colored_label(theme_widgets::DANGER, self.localizer.text(error_key(error)));
                }
                let valid = draft.value.value(unit).is_ok();
                ui.label(self.localizer.text("board-affected"));
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for (id, anchor) in &mut draft.anchors {
                            if let Some(board) = project.boards.iter().find(|b| b.id == *id) {
                                ui.label(format!("{} · {}", board.name, id));
                                theme_widgets::segmented(
                                    ui,
                                    anchor,
                                    &[
                                        (Anchor::Start, &self.localizer.text("anchor-start")),
                                        (Anchor::Centre, &self.localizer.text("anchor-centre")),
                                        (Anchor::End, &self.localizer.text("anchor-end")),
                                    ],
                                );
                                let before = board.blank_dimensions()[draft.dimension.axis()];
                                ui.monospace(format!(
                                    "{} → {}",
                                    format_length(before, Unit::Mm, locale, 3),
                                    if valid {
                                        format_length(
                                            draft.value.value(unit).expect("validated"),
                                            Unit::Mm,
                                            locale,
                                            3,
                                        )
                                    } else {
                                        "—".into()
                                    }
                                ));
                            }
                        }
                    });
                let mut preview = None;
                if current && valid {
                    match self.editor.preview_batch_board_dimension(
                        &selection,
                        draft.dimension,
                        draft.value.value(unit).expect("validated"),
                        &draft.anchors,
                    ) {
                        Ok(result) => {
                            draft.error = None;
                            preview = Some(result);
                        }
                        Err(error) => draft.error = Some(error),
                    }
                }
                if !current {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text("board-assignment-stale"),
                    );
                }
                if let Some(error) = &draft.error {
                    let label = match error {
                        BatchDimensionError::Target { board_id, reason } => format!(
                            "{}: {} ({reason:?})",
                            self.localizer.text("board-batch-invalid"),
                            board_id
                        ),
                        _ => self.localizer.text("error-board-dimension"),
                    };
                    ui.colored_label(egui::Color32::LIGHT_RED, label);
                }
                if let Some(proposal) = &preview {
                    for issue in &proposal.conflicts {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!(
                                "{}: {}",
                                issue.board_id,
                                conflict_labels(&self.localizer, issue)
                            ),
                        );
                    }
                }
                (preview, current && valid && draft.error.is_none())
            },
        );
        draft.focus_on_open = false;
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            self.batch_dimension_chrome.close(ctx);
            return;
        }
        let confirm = actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm);
        if confirm {
            accepted_preview = result.body;
        }
        if let Some(preview) = accepted_preview {
            match self.editor.edit_batch_board_dimension(preview) {
                Ok(conflicts) => {
                    self.material_conflicts = conflicts;
                    self.batch_dimension_chrome.close(ctx);
                    return;
                }
                Err(plan_my_cabinet::commands::EditError::Command(error)) => {
                    draft.error = Some(error);
                }
                Err(_) => {
                    draft.error = Some(BatchDimensionError::StalePreview);
                }
            }
        }
        self.batch_dimension = Some(draft);
    }

    fn show_board_dimension(&mut self, ctx: &egui::Context) {
        if self.navigation.pending().is_some() {
            return;
        }
        let Some(mut draft) = self.board_dimension.take() else {
            if self.board_dimension_chrome.is_active() {
                self.board_dimension_chrome.close(ctx);
            }
            return;
        };
        let mut destination = None;
        let project = self.editor.project();
        let current = project.id == draft.project_id && project.revision == draft.revision;
        let board = project
            .boards
            .iter()
            .find(|board| board.id == draft.board_id);
        let unit = project.display_unit;
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let mut preview: Option<DimensionPreview> = None;
        let title = self.localizer.text("board-edit-dimension");
        let cancel_label = self.localizer.text("cancel");
        let confirm_label = self.localizer.text("confirm");
        let modal = self.board_dimension_chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &cancel_label,
                confirm: &confirm_label,
            },
            |ui| {
                if let Some(board) = board {
                    ui.label(format!("{} · {}", board.name, board.id));
                }
                let key = match draft.dimension {
                    BoardDimension::Length => "board-length",
                    BoardDimension::Width => "board-width",
                    BoardDimension::Thickness => "board-thickness",
                };
                let axis = egui::ComboBox::from_label(self.localizer.text("board-local-dimension"))
                    .selected_text(self.localizer.text(key))
                    .show_ui(ui, |ui| {
                        for (dimension, key) in [
                            (BoardDimension::Length, "board-length"),
                            (BoardDimension::Width, "board-width"),
                            (BoardDimension::Thickness, "board-thickness"),
                        ] {
                            if combo_option(
                                ui,
                                &mut draft.dimension,
                                dimension,
                                self.localizer.text(key),
                            )
                            .changed()
                            {
                                if let Some(board) = board {
                                    let value = board.blank_dimensions()[match dimension {
                                        BoardDimension::Length => 0,
                                        BoardDimension::Width => 1,
                                        BoardDimension::Thickness => 2,
                                    }];
                                    draft.value.text = format_length(value, Unit::Mm, locale, 3);
                                    draft.value.consent = false;
                                }
                                draft.error = None;
                            }
                        }
                    });
                if draft.focus_on_open {
                    axis.response.request_focus();
                }
                let valid = dimension_field(ui, &self.localizer, key, &mut draft.value, unit);
                egui::ComboBox::from_label(self.localizer.text("board-resize-anchor"))
                    .selected_text(self.localizer.text(match draft.anchor {
                        Anchor::Start => "anchor-start",
                        Anchor::Centre => "anchor-centre",
                        Anchor::End => "anchor-end",
                    }))
                    .show_ui(ui, |ui| {
                        for (anchor, key) in [
                            (Anchor::Start, "anchor-start"),
                            (Anchor::Centre, "anchor-centre"),
                            (Anchor::End, "anchor-end"),
                        ] {
                            combo_option(ui, &mut draft.anchor, anchor, self.localizer.text(key));
                        }
                    });
                if !current {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text("board-assignment-stale"),
                    );
                } else if valid {
                    match self.editor.preview_board_dimension(
                        draft.board_id,
                        draft.dimension,
                        draft.value.value(unit).expect("validated"),
                        draft.anchor,
                    ) {
                        Ok(result) => {
                            ui.label(format!(
                                "{}: {} ({})",
                                self.localizer.text("board-dimension-preview"),
                                format_length(result.value, Unit::Mm, locale, 3),
                                self.localizer.text(match result.anchor {
                                    Anchor::Start => "anchor-start",
                                    Anchor::Centre => "anchor-centre",
                                    Anchor::End => "anchor-end",
                                })
                            ));
                            for issue in &result.conflicts {
                                ui.colored_label(
                                    egui::Color32::YELLOW,
                                    format!(
                                        "{}: {}",
                                        self.localizer.text("material-prospective-conflict"),
                                        conflict_labels(&self.localizer, issue)
                                    ),
                                );
                            }
                            preview = Some(result);
                        }
                        Err(error) => draft.error = Some(error),
                    }
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        match error {
                            DimensionEditError::InvalidDimension(reason)
                            | DimensionEditError::InvalidPose(reason) => {
                                pose_error_label(&self.localizer, *reason)
                            }
                            _ => self.localizer.text("error-board-dimension"),
                        },
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    for (workspace, key) in [
                        (Workspace::Design, "navigation-design"),
                        (Workspace::Stock, "navigation-stock"),
                        (Workspace::CutPlan, "navigation-cut-plan"),
                        (Workspace::Hardware, "navigation-hardware"),
                        (Workspace::Handoff, "navigation-handoff"),
                    ] {
                        if ui.button(self.localizer.text(key)).clicked() {
                            destination = Some(NavigationRoute::Workspace(workspace));
                        }
                    }
                });
                ui.collapsing(self.localizer.text("board-list"), |ui| {
                    for target in &self.editor.project().boards {
                        if target.id != draft.board_id
                            && ui
                                .button(format!("{} ({})", target.name, target.id))
                                .clicked()
                        {
                            destination =
                                Some(NavigationRoute::Entity(Destination::Board(target.id)));
                        }
                    }
                });
                ((), preview.is_some())
            },
        );
        draft.focus_on_open = false;
        let cancel = modal.action == ModalAction::Cancel;
        let confirm = modal.action == ModalAction::Confirm;
        if actions::decision(A::CancelDialog, cancel) {
            self.board_dimension_chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, confirm)
            && let Some(preview) = preview
        {
            match self.editor.edit_board_dimension(preview) {
                Ok(conflicts) => {
                    self.material_conflicts = conflicts;
                    self.board_dimension_chrome.close(ctx);
                    return;
                }
                Err(plan_my_cabinet::commands::EditError::Command(error)) => {
                    draft.error = Some(error)
                }
                Err(_) => draft.error = Some(DimensionEditError::StalePreview),
            }
        }
        self.board_dimension = Some(draft);
        if let Some(route) = destination {
            self.request_navigation(route);
        }
    }

    fn show_board_material(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.board_material.take() else {
            if self.board_material_chrome.is_active() {
                self.board_material_chrome.close(ctx);
            }
            return;
        };
        let project = self.editor.project();
        let current = project.id == draft.project_id && project.revision == draft.revision;
        let board = project.boards.iter().find(|b| b.id == draft.board_id);
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let title = self.localizer.text("board-assign-material");
        let cancel_label = self.localizer.text("cancel");
        let confirm_label = self.localizer.text("confirm");
        let modal = self.board_material_chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &cancel_label,
                confirm: &confirm_label,
            },
            |ui| {
                if let Some(board) = board {
                    ui.label(format!("{} · {}", board.name, board.id));
                    ui.label(format!(
                        "{}: {}",
                        self.localizer.text("board-thickness"),
                        format_length(board.thickness, Unit::Mm, locale, 3)
                    ));
                }
                let selected = draft
                    .material_id
                    .and_then(|id| project.materials.iter().find(|m| m.id == id));
                let material_combo = egui::ComboBox::from_label(self.localizer.text("material"))
                    .selected_text(selected.map(|m| m.name.as_str()).unwrap_or("—"))
                    .show_ui(ui, |ui| {
                        for material in &project.materials {
                            if combo_option(
                                ui,
                                &mut draft.material_id,
                                Some(material.id),
                                &material.name,
                            )
                            .changed()
                            {
                                draft.error = None;
                            }
                        }
                    });
                if draft.focus_on_open {
                    material_combo.response.request_focus();
                }
                egui::ComboBox::from_label(self.localizer.text("material-anchor"))
                    .selected_text(self.localizer.text(match draft.anchor {
                        Anchor::Start => "anchor-start",
                        Anchor::Centre => "anchor-centre",
                        Anchor::End => "anchor-end",
                    }))
                    .show_ui(ui, |ui| {
                        for (anchor, key) in [
                            (Anchor::Start, "anchor-start"),
                            (Anchor::Centre, "anchor-centre"),
                            (Anchor::End, "anchor-end"),
                        ] {
                            if combo_option(ui, &mut draft.anchor, anchor, self.localizer.text(key))
                                .changed()
                            {
                                draft.error = None;
                            }
                        }
                    });
                let mut valid = false;
                if !current {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text("board-assignment-stale"),
                    );
                } else if let (Some(board), Some(material_id)) = (board, draft.material_id)
                    && board.material_id != material_id
                {
                    match self
                        .editor
                        .preview_board_material(board.id, material_id, draft.anchor)
                    {
                        Ok((thickness, conflict)) => {
                            valid = true;
                            ui.label(format!(
                                "{}: {}",
                                self.localizer.text("board-resulting-thickness"),
                                format_length(thickness, Unit::Mm, locale, 3)
                            ));
                            if let Some(conflict) = conflict {
                                ui.colored_label(
                                    egui::Color32::YELLOW,
                                    format!(
                                        "{}: {}",
                                        self.localizer.text("material-prospective-conflict"),
                                        conflict_labels(&self.localizer, &conflict)
                                    ),
                                );
                            }
                        }
                        Err(error) => {
                            draft.error = Some(error);
                        }
                    }
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        match error {
                            MaterialChangeError::InvalidPose { reason, .. } => {
                                pose_error_label(&self.localizer, *reason)
                            }
                            _ => self.localizer.text("error-material-edit"),
                        },
                    );
                }
                ((), valid)
            },
        );
        draft.focus_on_open = false;
        let cancel = modal.action == ModalAction::Cancel;
        let confirm = modal.action == ModalAction::Confirm;
        if actions::decision(A::CancelDialog, cancel) {
            self.board_material_chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, confirm)
            && current
            && let Some(material_id) = draft.material_id
        {
            match self
                .editor
                .assign_board_material(draft.board_id, material_id, draft.anchor)
            {
                Ok(conflicts) => {
                    self.material_conflicts = conflicts;
                    self.board_material_chrome.close(ctx);
                    return;
                }
                Err(plan_my_cabinet::commands::EditError::Command(error)) => {
                    draft.error = Some(error)
                }
                Err(_) => draft.error = Some(MaterialChangeError::StalePreview),
            }
        }
        self.board_material = Some(draft);
    }

    fn show_material_edit(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.material_edit.take() else {
            if self.material_edit_chrome.is_active() {
                self.material_edit_chrome.close(ctx);
            }
            return;
        };
        let unit = self.editor.project().display_unit;
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let mut preview: Option<MaterialChangePreview> = None;
        let title = self.localizer.text("material-edit");
        let cancel_label = self.localizer.text("cancel");
        let confirm_label = self.localizer.text("confirm");
        let modal = self.material_edit_chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &cancel_label,
                confirm: &confirm_label,
            },
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(self.localizer.text("material-name"));
                    let name = ui.text_edit_singleline(&mut draft.name);
                    if draft.focus_on_open {
                        name.request_focus();
                    }
                    if name.changed() {
                        draft.choice = None;
                        draft.error = None;
                    }
                });
                let old_text = draft.thickness.text.clone();
                let valid = dimension_field(
                    ui,
                    &self.localizer,
                    "board-thickness",
                    &mut draft.thickness,
                    unit,
                );
                if draft.thickness.text != old_text {
                    draft.choice = None;
                    draft.error = None;
                }
                let old_grain = draft.grain;
                egui::ComboBox::from_label(self.localizer.text("material-grain"))
                    .selected_text(self.localizer.text(match draft.grain {
                        BoardGrain::Length => "grain-length",
                        BoardGrain::Width => "grain-width",
                        BoardGrain::Unrestricted => "grain-unrestricted",
                    }))
                    .show_ui(ui, |ui| {
                        for (grain, key) in [
                            (BoardGrain::Length, "grain-length"),
                            (BoardGrain::Width, "grain-width"),
                            (BoardGrain::Unrestricted, "grain-unrestricted"),
                        ] {
                            combo_option(ui, &mut draft.grain, grain, self.localizer.text(key));
                        }
                    });
                if old_grain != draft.grain {
                    draft.choice = None;
                    draft.error = None;
                }
                let mut color = self.editor.project().material_colors.get(&draft.id).copied();
                if creation_color_swatches(ui, &self.localizer, &mut color) {
                    let _ = self.editor.set_material_color(draft.id, color);
                }
                ui.add_space(4.0);
                let old_anchor = draft.anchor;
                egui::ComboBox::from_label(self.localizer.text("material-anchor"))
                    .selected_text(self.localizer.text(match draft.anchor {
                        Anchor::Start => "anchor-start",
                        Anchor::Centre => "anchor-centre",
                        Anchor::End => "anchor-end",
                    }))
                    .show_ui(ui, |ui| {
                        for (anchor, key) in [
                            (Anchor::Start, "anchor-start"),
                            (Anchor::Centre, "anchor-centre"),
                            (Anchor::End, "anchor-end"),
                        ] {
                            combo_option(ui, &mut draft.anchor, anchor, self.localizer.text(key));
                        }
                    });
                if old_anchor != draft.anchor {
                    draft.choice = None;
                    draft.error = None;
                }
                if valid {
                    match self.editor.preview_material_change(
                        draft.id,
                        draft.name.clone(),
                        draft.thickness.value(unit).expect("validated"),
                        draft.grain,
                        draft.anchor,
                    ) {
                        Ok(proposal) => preview = Some(proposal),
                        Err(error) => draft.error = Some(error),
                    }
                }
                let mut apply_blocked = false;
                if let Some(proposal) = &preview {
                    ui.separator();
                    ui.label(self.localizer.text("material-affected"));
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for board in &proposal.affected {
                                ui.label(format!(
                                    "{}: {} / {}",
                                    board.name,
                                    format_length(board.thickness_before, Unit::Mm, locale, 3),
                                    format_length(board.thickness_if_applied, Unit::Mm, locale, 3)
                                ));
                                if let Some(error) = board.apply_error {
                                    ui.colored_label(
                                        egui::Color32::LIGHT_RED,
                                        format!(
                                            "{}: {}",
                                            self.localizer.text("material-apply-error"),
                                            pose_error_label(&self.localizer, error)
                                        ),
                                    );
                                }
                                if let Some(conflict) = &board.allocation_if_applied {
                                    ui.colored_label(
                                        egui::Color32::YELLOW,
                                        format!(
                                            "{}: {}",
                                            self.localizer.text("material-prospective-conflict"),
                                            conflict_labels(&self.localizer, conflict)
                                        ),
                                    );
                                }
                            }
                        });
                    ui.radio_value(
                        &mut draft.choice,
                        Some(DependantChoice::Preserve),
                        self.localizer.text("material-preserve"),
                    );
                    ui.radio_value(
                        &mut draft.choice,
                        Some(DependantChoice::ApplyAll),
                        self.localizer.text("material-apply-all"),
                    );
                    apply_blocked = matches!(draft.choice, Some(DependantChoice::ApplyAll))
                        && proposal.affected.iter().any(|b| b.apply_error.is_some());
                    if apply_blocked {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            self.localizer.text("material-apply-blocked"),
                        );
                    }
                }
                if let Some(error) = &draft.error {
                    let message = match error {
                        MaterialChangeError::InvalidPose { board_id, reason } => {
                            let name = self
                                .editor
                                .project()
                                .boards
                                .iter()
                                .find(|b| b.id == *board_id)
                                .map(|b| b.name.as_str())
                                .unwrap_or("—");
                            format!(
                                "{}: {}: {}",
                                self.localizer.text("material-apply-error"),
                                name,
                                pose_error_label(&self.localizer, *reason)
                            )
                        }
                        _ => self.localizer.text("error-material-edit"),
                    };
                    ui.colored_label(egui::Color32::LIGHT_RED, message);
                }
                (
                    (),
                    preview.is_some() && draft.choice.is_some() && !apply_blocked,
                )
            },
        );
        draft.focus_on_open = false;
        let cancel = modal.action == ModalAction::Cancel;
        let confirm = modal.action == ModalAction::Confirm;
        if actions::decision(A::CancelDialog, cancel) {
            self.material_edit_chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, confirm)
            && let (Some(preview), Some(choice)) = (preview, draft.choice.clone())
        {
            match self.editor.apply_material_change(preview, choice) {
                Ok(conflicts) => {
                    self.material_conflicts = conflicts;
                    self.material_edit_chrome.close(ctx);
                    return;
                }
                Err(plan_my_cabinet::commands::EditError::Command(error)) => {
                    draft.error = Some(error)
                }
                Err(_) => draft.error = Some(MaterialChangeError::StalePreview),
            }
        }
        self.material_edit = Some(draft);
    }

    fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.dialog.take() else {
            if self.board_creation_chrome.is_active() {
                self.board_creation_chrome.close(ctx);
            }
            if self.material_creation_chrome.is_active() {
                self.material_creation_chrome.close(ctx);
            }
            return;
        };
        let mut create_material = false;
        let project = self.editor.project();
        let unit = *draft.unit.get_or_insert(project.display_unit);
        let current_project = *draft.project_id.get_or_insert(project.id) == project.id;
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let is_board = draft.kind == DialogKind::Board;
        let chrome = if is_board {
            &mut self.board_creation_chrome
        } else {
            &mut self.material_creation_chrome
        };
        let title = self.localizer.text(if is_board {
            "board-new"
        } else {
            "material-new"
        });
        let cancel_text = self.localizer.text("cancel");
        let confirm_text = self.localizer.text(if is_board {
            "board-create-action"
        } else {
            "material-create-action"
        });
        let result = chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &cancel_text,
                confirm: &confirm_text,
            },
            |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                theme_widgets::section_header(
                    ui,
                    &self.localizer.text(if is_board {
                        "board-create-details"
                    } else {
                        "material-create-details"
                    }),
                );
                ui.label(self.localizer.text(if is_board {
                    "board-name"
                } else {
                    "material-name"
                }));
                ui.add(
                    egui::TextEdit::singleline(&mut draft.name)
                        .id(egui::Id::new(if is_board {
                            "board-creation-name"
                        } else {
                            "material-creation-name"
                        }))
                        .desired_width(f32::INFINITY),
                );
                let valid = if is_board {
                    ui.label(self.localizer.text("material"));
                    egui::ComboBox::from_id_salt("board-creation-material")
                        .selected_text(
                            project
                                .materials
                                .iter()
                                .find(|m| Some(m.id) == draft.material_id)
                                .map(|m| m.name.as_str())
                                .unwrap_or("—"),
                        )
                        .show_ui(ui, |ui| {
                            for material in &project.materials {
                                combo_option(
                                    ui,
                                    &mut draft.material_id,
                                    Some(material.id),
                                    &material.name,
                                );
                            }
                        });
                    if theme_widgets::secondary_button(ui, &self.localizer.text("material-new"))
                        .clicked()
                    {
                        create_material = true;
                    }
                    let material = draft
                        .material_id
                        .and_then(|id| project.materials.iter().find(|m| m.id == id));
                    if let Some(material) = material {
                        theme_widgets::card().show(ui, |ui| {
                            ui.label(format!(
                                "{}: {} — {}",
                                self.localizer.text("board-effective"),
                                material.name,
                                format_length(material.default_thickness, Unit::Mm, locale, 3)
                            ));
                        });
                    } else {
                        ui.colored_label(
                            theme_widgets::DANGER,
                            self.localizer.text("error-material-missing"),
                        );
                    }
                    theme_widgets::section_header(ui, &self.localizer.text("board-create-size"));
                    ui.label(self.localizer.text("board-input-hint"));
                    let length = creation_dimension_field(
                        ui,
                        &self.localizer,
                        "board-creation-length",
                        "board-length",
                        &mut draft.length,
                        unit,
                    );
                    let width = creation_dimension_field(
                        ui,
                        &self.localizer,
                        "board-creation-width",
                        "board-width",
                        &mut draft.width,
                        unit,
                    );
                    if length
                        && width
                        && material.is_some()
                        && draft.board_key(project).is_none()
                        && current_project
                    {
                        ui.colored_label(
                            theme_widgets::DANGER,
                            self.localizer.text("error-out-of-bounds"),
                        );
                    }
                    ui.label(self.localizer.text("board-effective-grain"));
                    egui::ComboBox::from_id_salt("board-creation-grain")
                        .selected_text(
                            self.localizer.text(
                                draft
                                    .grain_override
                                    .map_or("grain-follow-default", grain_key),
                            ),
                        )
                        .show_ui(ui, |ui| {
                            combo_option(
                                ui,
                                &mut draft.grain_override,
                                None,
                                self.localizer.text("grain-follow-default"),
                            );
                            for grain in [
                                BoardGrain::Length,
                                BoardGrain::Width,
                                BoardGrain::Unrestricted,
                            ] {
                                combo_option(
                                    ui,
                                    &mut draft.grain_override,
                                    Some(grain),
                                    self.localizer.text(grain_key(grain)),
                                );
                            }
                        });
                    if length
                        && width
                        && let Some(material) = material
                        && let Some(key) = draft.board_key(project)
                    {
                        let preview = draft.preview(project, key);
                        theme_widgets::section_header(
                            ui,
                            &self.localizer.text("board-create-outlook"),
                        );
                        ui.label(format!(
                            "{} × {} × {}",
                            format_length(key.length, Unit::Mm, locale, 3),
                            format_length(key.width, Unit::Mm, locale, 3),
                            format_length(material.default_thickness, Unit::Mm, locale, 3)
                        ));
                        match (preview.fit, preview.placement) {
                            (FirstFit::Allocated(stock_id), Some((origin, turn))) => {
                                ui.label(self.localizer.format(
                                    "board-preview-fit",
                                    Some(&{
                                        let mut args = FluentArgs::new();
                                        args.set("stock", stock_reference(project, stock_id));
                                        args.set(
                                            "x",
                                            format_length(origin[0], Unit::Mm, locale, 3),
                                        );
                                        args.set(
                                            "y",
                                            format_length(origin[1], Unit::Mm, locale, 3),
                                        );
                                        args.set(
                                            "turn",
                                            self.localizer.text(if turn {
                                                "board-preview-turned"
                                            } else {
                                                "board-preview-unturned"
                                            }),
                                        );
                                        args
                                    }),
                                ));
                            }
                            (FirstFit::NoFit, _) => {
                                ui.label(self.localizer.text("first-fit-no-fit"));
                            }
                            (FirstFit::SearchExhausted, _) => {
                                ui.label(self.localizer.text("first-fit-exhausted"));
                            }
                            _ => unreachable!("allocated fit includes placement"),
                        }
                        ui.small(self.localizer.text("board-preview-provisional"));
                    }
                    draft.board_key(project).is_some()
                } else {
                    let language = self.localizer.language();
                    let current = plan_my_cabinet::material_presets::BR_STANDARD
                        .iter()
                        .position(|preset| {
                            preset.name(language) == draft.name
                                && draft.thickness.text.trim() == preset.thickness_mm.to_string()
                        });
                    let mut chosen = current;
                    ui.label(
                        egui::RichText::new(self.localizer.text("material-preset"))
                            .size(12.0)
                            .color(theme_widgets::MUTED),
                    );
                    egui::ComboBox::from_id_salt("material-creation-preset")
                        .width(ui.available_width() - 8.0)
                        .selected_text(current.map_or_else(
                            || self.localizer.text("material-preset-none"),
                            |index| {
                                let preset = &plan_my_cabinet::material_presets::BR_STANDARD[index];
                                format!("{} · {} mm", preset.name(language), preset.thickness_mm)
                            },
                        ))
                        .show_ui(ui, |ui| {
                            for (index, preset) in
                                plan_my_cabinet::material_presets::BR_STANDARD.iter().enumerate()
                            {
                                combo_option(
                                    ui,
                                    &mut chosen,
                                    Some(index),
                                    format!("{} · {} mm", preset.name(language), preset.thickness_mm),
                                );
                            }
                        });
                    if chosen != current
                        && let Some(index) = chosen
                    {
                        let preset = &plan_my_cabinet::material_presets::BR_STANDARD[index];
                        draft.name = preset.name(language).to_owned();
                        draft.thickness.text = preset.thickness_mm.to_string();
                        draft.thickness.consent = false;
                        draft.grain = preset.grain;
                        draft.color = Some(preset.color);
                    }
                    ui.add_space(4.0);
                    ui.label(self.localizer.text("board-input-hint"));
                    let thickness = creation_dimension_field(
                        ui,
                        &self.localizer,
                        "material-creation-thickness",
                        "board-thickness",
                        &mut draft.thickness,
                        unit,
                    );
                    ui.label(self.localizer.text("material-grain"));
                    egui::ComboBox::from_id_salt("material-creation-grain")
                        .selected_text(self.localizer.text(grain_key(draft.grain)))
                        .show_ui(ui, |ui| {
                            for grain in [
                                BoardGrain::Length,
                                BoardGrain::Width,
                                BoardGrain::Unrestricted,
                            ] {
                                combo_option(
                                    ui,
                                    &mut draft.grain,
                                    grain,
                                    self.localizer.text(grain_key(grain)),
                                );
                            }
                        });
                    creation_color_swatches(ui, &self.localizer, &mut draft.color);
                    thickness
                };
                if draft.error {
                    ui.colored_label(theme_widgets::DANGER, self.localizer.text("error-create"));
                }
                if !current_project {
                    ui.colored_label(
                        theme_widgets::DANGER,
                        self.localizer.text("creation-project-changed"),
                    );
                }
                ((), valid && current_project)
            },
        );
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            chrome.close(ctx);
            if draft.kind == DialogKind::Material {
                self.dialog = self.suspended_board.take();
            }
            return;
        }
        if create_material {
            self.suspended_board = Some(draft);
            let _ = self.invoke(Request::new(A::NewMaterial));
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm) {
            if is_board {
                // Re-derive from current project and captured input unit: the displayed
                // preview is advisory and may have been invalidated by a stock edit.
                if let Some(key) = draft.board_key(self.editor.project()) {
                    let mut actual = None;
                    if self
                        .editor
                        .transact(|project| -> Result<(), ()> {
                            actual = Some(fit_new_board(project, draft.name.clone(), key));
                            Ok(())
                        })
                        .is_ok()
                    {
                        self.first_fit_notice = actual.map(|preview| preview.fit);
                        chrome.close(ctx);
                        return;
                    }
                }
            } else if current_project && let Ok(thickness) = draft.thickness.value(unit) {
                let id = Uuid::new_v4();
                if self
                    .editor
                    .transact(|project| -> Result<(), ()> {
                        project.materials.push(plan_my_cabinet::domain::Material {
                            id,
                            name: draft.name.clone(),
                            default_thickness: thickness,
                            default_grain: draft.grain,
                        });
                        if let Some(color) = draft.color {
                            project.material_colors.insert(id, color);
                        }
                        Ok(())
                    })
                    .is_ok()
                {
                    chrome.close(ctx);
                    if let Some(mut board) = self.suspended_board.take() {
                        board.material_id = Some(id);
                        self.dialog = Some(board);
                    }
                    return;
                }
            }
            draft.error = true;
        }
        self.dialog = Some(draft);
    }
}

impl DesktopApp {
    fn start_pdf_write(
        &mut self,
        path: PathBuf,
        packet: Arc<ReviewedPacket>,
        overwrite: Overwrite,
    ) {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.export_cancel = Some(cancel.clone());
        self.export_events = Some(rx);
        self.export_activity = Some(ExportActivity::Writing);
        std::thread::spawn(move || {
            let result = write_reviewed_pdf(&packet, Some(&path), overwrite, || {
                cancel.load(Ordering::Relaxed)
            });
            let _ = tx.send(ExportEvent::Finished(path, packet, result));
        });
    }

    fn export_settings(&self) -> ExportSettings {
        ExportSettings {
            language: self.export_language,
            units: self.export_units,
        }
    }

    fn export_key(&self) -> ExportPreparationKey {
        let project = self.editor.project();
        (
            project.id,
            project.revision,
            self.export_mode,
            self.export_language,
            self.export_units,
            plan_my_cabinet::export::fingerprint(project).packet,
            self.export_sections,
            plan_my_cabinet::document_layout::LAYOUT_VERSION,
            plan_my_cabinet::document_layout::FONT_METRICS_VERSION,
        )
    }

    fn current_reviewed_packet(&self) -> Option<Arc<ReviewedPacket>> {
        let (key, result) = self.export_preparation.as_ref()?;
        let packet = result.as_ref().ok()?;
        (key == &self.export_key()
            && packet.matches_source(
                self.editor.project(),
                self.export_mode,
                self.export_settings(),
                self.export_sections,
            ))
        .then(|| Arc::clone(packet))
    }

    fn shop_ready_available(&self) -> bool {
        self.export_candidate.as_ref().is_some_and(|(key, result)| {
            key == &self.export_key()
                && result
                    .as_ref()
                    .is_ok_and(|packet| packet.wood_issues().is_empty())
        })
    }

    fn invalidate_export_review(&mut self) {
        self.export_preparation = None;
        self.export_candidate = None;
        if let Some((_, _, cancel)) = &self.export_preparation_pending {
            cancel.store(true, Ordering::Relaxed);
        }
        self.export_preparation_pending = None;
    }

    fn poll_pdf_export(&mut self, ctx: &egui::Context) {
        let event = self
            .export_events
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        if let Some(event) = event {
            self.export_events = None;
            match event {
                ExportEvent::Selected(path) => {
                    let Some(ExportActivity::Choosing(source, settings, mode)) =
                        self.export_activity.take()
                    else {
                        return;
                    };
                    let picker_key = self.export_picker_key.take();
                    let reviewed = self.current_reviewed_packet().filter(|packet| {
                        packet.key().project_id == source.id
                            && packet.key().revision == source.revision
                            && packet.key().mode == mode
                            && packet.key().settings == settings
                            && picker_key.as_ref() == Some(packet.key())
                    });
                    match path {
                        None => self.export_message = Some(self.localizer.text("export-cancelled")),
                        Some(_) if reviewed.is_none() => {
                            self.invalidate_export_review();
                            self.export_message = Some(self.localizer.text("export-review-stale"));
                        }
                        Some(path) if path.symlink_metadata().is_ok() => {
                            self.export_activity =
                                Some(ExportActivity::Confirming(path, reviewed.expect("checked")))
                        }
                        Some(path) => self.start_pdf_write(
                            path,
                            reviewed.expect("checked"),
                            Overwrite::Decline,
                        ),
                    }
                }
                ExportEvent::Finished(path, packet, result) => {
                    self.export_activity = None;
                    self.export_cancel = None;
                    let snapshot = packet.snapshot();
                    let mut args = FluentArgs::new();
                    args.set("path", path.display().to_string());
                    args.set("revision", snapshot.revision() as i64);
                    self.export_message = Some(match result {
                        Ok(receipt) => {
                            match self.editor.record_completed_export(
                                snapshot,
                                &path,
                                receipt.completed_unix_ms,
                                &receipt.file_sha256,
                            ) {
                                Ok(()) => self.localizer.format(
                                    export_completion_key(self.editor.last_export_status()),
                                    Some(&args),
                                ),
                                Err(plan_my_cabinet::export::ExportError::WrongProject) => {
                                    self.localizer.format("export-saved-stale", Some(&args))
                                }
                                Err(_) => self.localizer.text("export-verify-failed"),
                            }
                        }
                        Err(OutputError::OverwriteRequired) => {
                            self.export_activity =
                                Some(ExportActivity::Confirming(path.clone(), packet.clone()));
                            self.localizer.text("export-overwrite")
                        }
                        Err(OutputError::Cancelled) => self.localizer.text("export-cancelled"),
                        Err(OutputError::PreparationBlocked) => {
                            self.localizer.text("export-wood-blocked")
                        }
                        Err(OutputError::Verify(_)) => self.localizer.text("export-verify-failed"),
                        Err(OutputError::Write(
                            plan_my_cabinet::persistence::SaveError::CommittedDurabilityUncertain(
                                error,
                            ),
                        )) => {
                            args.set("reason", error.to_string());
                            self.localizer.format("export-durability", Some(&args))
                        }
                        Err(error) => {
                            args.set(
                                "reason",
                                match error {
                                    OutputError::Pdf(_) => {
                                        self.localizer.text("export-render-failed")
                                    }
                                    OutputError::Write(_) => {
                                        self.localizer.text("export-write-failed")
                                    }
                                    _ => self.localizer.text("export-cancelled"),
                                },
                            );
                            self.localizer.format("export-failed", Some(&args))
                        }
                    });
                }
            }
        }
        if self.export_events.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn tick_export_preparation(&mut self, ctx: &egui::Context) {
        let key = self.export_key();
        let settings = self.export_settings();
        let source = self.editor.project().clone();
        let valid = |cache: &ExportPreparationCache| {
            cache.0 == key
                && cache.1.as_ref().map_or(true, |packet| {
                    packet.matches_source(&source, self.export_mode, settings, self.export_sections)
                })
        };
        if self
            .export_preparation
            .as_ref()
            .is_some_and(|cache| !valid(cache))
            || self
                .export_candidate
                .as_ref()
                .is_some_and(|cache| !valid(cache))
        {
            self.invalidate_export_review();
        }
        if self
            .export_preparation_pending
            .as_ref()
            .is_some_and(|(pending, _, _)| *pending != key)
        {
            self.invalidate_export_review();
        }
        if self
            .export_candidate
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
            && self
                .export_preparation_pending
                .as_ref()
                .is_none_or(|(pending, _, _)| *pending != key)
        {
            let snapshot = source.clone();
            let mode = self.export_mode;
            let sections = self.export_sections;
            let (tx, rx) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            self.export_preparation_pending = Some((key, rx, cancel));
            std::thread::spawn(move || {
                let _ = tx.send(
                    ReviewedPacket::prepare_cancellable(
                        &snapshot,
                        mode,
                        settings,
                        sections,
                        || worker_cancel.load(Ordering::Relaxed),
                    )
                    .map(Arc::new),
                );
            });
        }
        if let Some((pending, rx, _)) = &self.export_preparation_pending
            && let Ok(result) = rx.try_recv()
        {
            self.export_candidate = Some((pending.clone(), result));
            self.export_preparation_pending = None;
        }
        if self.export_preparation_pending.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn show_export_overwrite(&mut self, ctx: &egui::Context) {
        // A write can be in flight after the overwrite decision. Never take
        // (and accidentally discard) its Writing state on subsequent frames.
        if !matches!(self.export_activity, Some(ExportActivity::Confirming(..))) {
            if self.export_overwrite_chrome.is_active() {
                self.export_overwrite_chrome.close(ctx);
            }
            return;
        }
        let Some(ExportActivity::Confirming(path, packet)) = self.export_activity.take() else {
            unreachable!("confirmed activity was checked above");
        };
        let title = self.localizer.text("export-overwrite");
        let cancel_label = self.localizer.text("cancel");
        let replace_label = self.localizer.text("export-replace");
        let destination = path.display().to_string();
        let action = self
            .export_overwrite_chrome
            .show(
                ctx,
                &title,
                ModalActions {
                    cancel: &cancel_label,
                    confirm: &replace_label,
                },
                |ui| {
                    ui.label(&destination);
                    ((), true)
                },
            )
            .action;
        let replace = action == ModalAction::Confirm;
        let cancel = action == ModalAction::Cancel;
        if replace
            && !packet.matches_source(
                self.editor.project(),
                self.export_mode,
                self.export_settings(),
                self.export_sections,
            )
        {
            self.invalidate_export_review();
            self.export_message = Some(self.localizer.text("export-review-stale"));
        } else if replace && actions::contextual(Request::new(A::ReplacePdf), Ok(()), || ()).is_ok()
        {
            self.start_pdf_write(path, packet, Overwrite::Confirm);
        } else if cancel
            && actions::contextual(Request::new(A::CancelExport), Ok(()), || ()).is_ok()
        {
            self.export_message = Some(self.localizer.text("export-cancelled"));
        } else {
            self.export_activity = Some(ExportActivity::Confirming(path, packet));
        }
        if !matches!(self.export_activity, Some(ExportActivity::Confirming(..))) {
            self.export_overwrite_chrome.close(ctx);
        }
    }

    fn show_export_counts(&self, ui: &mut egui::Ui) {
        let project = self.editor.project();
        let counts = [
            ("handoff-packet-parts", project.boards.len()),
            ("handoff-packet-sheets", project.stock.len()),
            ("handoff-packet-hinges", project.hinge_installations.len()),
        ];
        let font = egui::TextStyle::Small.resolve(ui.style());
        let minimum_card_width = counts
            .iter()
            .flat_map(|(key, _)| {
                self.localizer
                    .text(key)
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .map(|word| {
                ui.painter()
                    .layout_no_wrap(word, font.clone(), theme_widgets::TEXT)
                    .size()
                    .x
                    + 16.0
            })
            .fold(0.0, f32::max);
        let columns = if ui.available_width()
            >= 3.0 * minimum_card_width + 2.0 * ui.spacing().item_spacing.x
        {
            3
        } else {
            1
        };
        // Allocate columns before rendering: horizontal_wrapped measures a
        // group's contents in the *remaining* row width, squeezing the last
        // localized label into a one-character-wide column.
        for row in counts.chunks(columns) {
            ui.columns(columns, |columns| {
                for (column, (key, count)) in columns.iter_mut().zip(row) {
                    let width = column.available_width();
                    egui::Frame::group(column.style()).show(column, |ui| {
                        // Columns justify their contents by default, which
                        // spreads letters across wrapped count-card headings.
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width((width - 16.0).max(1.0));
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(self.localizer.text(key)).small(),
                                )
                                .wrap(),
                            );
                            ui.strong(count.to_string());
                        });
                    });
                }
            });
        }
    }

    fn show_export_preparation(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.localizer.text("export-preparation"));
        self.show_export_counts(ui);
        let mut mode_choice = self.export_mode;
        let can_shop = self.shop_ready_available();
        let first_board_issue = self
            .export_candidate
            .as_ref()
            .and_then(|(key, result)| {
                (key == &self.export_key()).then(|| match result {
                    Ok(packet) => packet.wood_issues(),
                    Err(ReviewPreparationError::Blocked(blocked)) => &blocked.issues,
                    Err(_) => &[],
                })
            })
            .and_then(|issues| {
                issues.iter().find_map(|issue| match issue {
                    ExportIssue::Board {
                        id, name, reasons, ..
                    } => Some((
                        *id,
                        name.clone(),
                        reasons.first().map(|reason| reason.key()),
                    )),
                    _ => None,
                })
            });
        let mut card_fix = None;
        for (mode, title, description) in [
            (
                ExportMode::Draft,
                "export-draft",
                "handoff-draft-description",
            ),
            (
                ExportMode::ShopReady,
                "export-shop-ready",
                if can_shop {
                    "handoff-shop-available"
                } else {
                    "handoff-shop-unavailable"
                },
            ),
        ] {
            egui::Frame::new()
                .fill(theme_widgets::PANEL)
                .stroke(egui::Stroke::new(1.0, theme_widgets::BORDER))
                .corner_radius(7.0)
                .inner_margin(9.0)
                .show(ui, |ui| {
                    ui.add_enabled_ui(mode != ExportMode::ShopReady || can_shop, |ui| {
                        ui.radio_value(&mut mode_choice, mode, self.localizer.text(title));
                    });
                    ui.small(self.localizer.text(description));
                    if mode == ExportMode::ShopReady
                        && let Some((id, name, reason)) = &first_board_issue
                    {
                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(
                                theme_widgets::WARN_INK,
                                format!(
                                    "{}: {}",
                                    name,
                                    reason.map_or_else(
                                        || self.localizer.text("export-wood-blocked"),
                                        |key| self.localizer.text(key)
                                    ),
                                ),
                            );
                            if ui
                                .add_enabled(
                                    !self.modal_open(),
                                    egui::Button::new(self.localizer.text("handoff-fix")),
                                )
                                .clicked()
                            {
                                card_fix = Some(*id);
                            }
                        });
                    }
                });
        }
        if let Some(id) = card_fix {
            self.navigate_session(Destination::BoardAllocation(id));
        }
        if mode_choice != self.export_mode {
            let _ = self
                .invoke(Request::new(A::SetExportMode).argument(Argument::ExportMode(mode_choice)));
        }
        let mut language_choice = self.export_language;
        ui.horizontal(|ui| {
            ui.label(self.localizer.text("export-language"));
            ui.radio_value(
                &mut language_choice,
                Language::En,
                self.localizer.text("language-en"),
            );
            ui.radio_value(
                &mut language_choice,
                Language::PtBr,
                self.localizer.text("language-pt-br"),
            );
        });
        if language_choice != self.export_language {
            let _ = self.invoke(
                Request::new(A::SetExportLanguage).argument(Argument::Language(language_choice)),
            );
        }
        let mut units_choice = self.export_units;
        egui::ComboBox::from_label(self.localizer.text("export-output-units"))
            .selected_text(self.localizer.text(unit_key(self.export_units)))
            .show_ui(ui, |ui| {
                for unit in [Unit::Mm, Unit::Cm, Unit::M, Unit::Inch, Unit::Foot] {
                    ui.selectable_value(
                        &mut units_choice,
                        unit,
                        self.localizer.text(unit_key(unit)),
                    );
                }
            });
        if units_choice != self.export_units {
            let _ =
                self.invoke(Request::new(A::SetExportUnits).argument(Argument::Unit(units_choice)));
        }
        let mut sections = self.export_sections;
        ui.separator();
        ui.checkbox(
            &mut sections.parts_and_costs,
            self.localizer.text("export-section-parts"),
        );
        ui.checkbox(
            &mut sections.sheets_and_cut_steps,
            self.localizer.text("export-section-sheets"),
        );
        ui.checkbox(
            &mut sections.hinge_references,
            self.localizer.text("export-section-hinges"),
        );
        if sections != self.export_sections {
            self.export_sections = sections;
            self.invalidate_export_review();
        }
        ui.label(format!(
            "{}: {}",
            self.localizer.text("pdf-currency"),
            self.editor.project().currency.code()
        ));
        if self.editor.project().confirmed_shop_kerf != Some(self.editor.project().cutting_kerf) {
            ui.colored_label(
                egui::Color32::YELLOW,
                self.localizer.text("export-kerf-unconfirmed"),
            );
            if ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(self.localizer.text("export-confirm-kerf")),
                )
                .clicked()
            {
                let _ = self.invoke(Request::new(A::ConfirmKerf));
            }
        } else if let Some(label) = kerf_confirmation_label(self.editor.project(), &self.localizer)
        {
            ui.label(label);
        }
        let project = self.editor.project();
        let key = self.export_key();
        if self
            .export_candidate
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            ui.label(self.localizer.text("export-review-stale"));
            ui.ctx().request_repaint_after(Duration::from_millis(50));
            return;
        }
        let prepared = &self.export_candidate.as_ref().expect("prepared").1;
        let (issues, notices, ready) = match &prepared {
            Ok(plan) => (plan.wood_issues(), plan.notices(), true),
            Err(ReviewPreparationError::Blocked(blocked)) => (&blocked.issues[..], &[][..], false),
            Err(_) => (&[][..], &[][..], false),
        };
        if self.export_mode == ExportMode::Draft {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                self.localizer.text("export-draft-watermark"),
            );
        } else if ready {
            ui.label(self.localizer.text("export-wood-verified"));
        } else {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                self.localizer.text("export-wood-blocked"),
            );
        }
        let mut fix = None;
        for issue in issues.iter().chain(notices) {
            let route = match issue {
                ExportIssue::Board { id, .. } => {
                    Some(HandoffFix::Navigate(Destination::BoardAllocation(*id)))
                }
                ExportIssue::Sheet { id, .. } => {
                    Some(HandoffFix::Navigate(Destination::Sheet(*id)))
                }
                ExportIssue::KerfUnconfirmed(_) => Some(HandoffFix::Kerf),
                ExportIssue::UnknownPrice { stock_id: Some(id) } => Some(HandoffFix::Stock(*id)),
                ExportIssue::UnknownPrice { stock_id: None } => Some(HandoffFix::CutFee),
                ExportIssue::Hardware { id, .. } => Some(HandoffFix::Hardware(*id)),
                ExportIssue::Installation { id, .. } => {
                    Some(HandoffFix::Navigate(Destination::Installation(*id)))
                }
                ExportIssue::JointNeedsReview {
                    installation_id, ..
                } => Some(HandoffFix::Navigate(Destination::Installation(
                    *installation_id,
                ))),
                ExportIssue::InvalidWood(_) => None,
            };
            ui.group(|ui| {
                match issue {
                    ExportIssue::Board {
                        id,
                        name,
                        stock_id,
                        reasons,
                    } => {
                        ui.label(format!(
                            "{name} ({id}) · {} · {}",
                            stock_id.map_or_else(|| "—".into(), |s| stock_reference(project, s)),
                            reasons
                                .iter()
                                .map(|r| self.localizer.text(r.key()))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    ExportIssue::Sheet { id, name, reason } => {
                        let key = match reason {
                            SheetIssue::BudgetExhausted => "sheet-feasibility-unknown",
                            _ => "sheet-cut-conflict",
                        };
                        ui.label(format!(
                            "{} · {name}: {}",
                            stock_reference(project, *id),
                            self.localizer.text(key)
                        ));
                    }
                    ExportIssue::KerfUnconfirmed(_) => {
                        ui.label(self.localizer.text("export-kerf-unconfirmed"));
                    }
                    ExportIssue::UnknownPrice { stock_id } => {
                        ui.label(format!(
                            "{}: {}",
                            self.localizer.text("export-price-unknown"),
                            stock_id.map_or_else(
                                || self.localizer.text("export-cut-fee"),
                                |id| stock_reference(project, id)
                            )
                        ));
                    }
                    ExportIssue::Hardware { id, name, .. } => {
                        ui.label(format!(
                            "{name} ({id}): {}",
                            self.localizer.text("export-hardware-unverified")
                        ));
                    }
                    ExportIssue::Installation { id, name, reason } => {
                        ui.label(format!(
                            "{name} ({id}): {reason:?} — {}",
                            self.localizer.text("pdf-installation-withheld")
                        ));
                    }
                    ExportIssue::JointNeedsReview {
                        id,
                        installation_id,
                    } => {
                        ui.label(format!(
                            "{installation_id} ({id}): {}",
                            self.localizer.text("pdf-joint-review")
                        ));
                    }
                    ExportIssue::InvalidWood(_) => {
                        ui.label(self.localizer.text("pdf-invalid-wood"));
                    }
                };
                if let Some(route) = route
                    && ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("handoff-fix")),
                        )
                        .clicked()
                {
                    fix = Some(route);
                }
            });
        }
        if let Some(route) = fix {
            match route {
                HandoffFix::Navigate(target) => {
                    self.navigate_session(target);
                }
                HandoffFix::Stock(id) => {
                    if self
                        .editor
                        .project()
                        .stock
                        .iter()
                        .any(|piece| piece.id == id)
                        && matches!(
                            self.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
                            Outcome::Navigated
                        )
                    {
                        self.session.stock_piece = Some(id);
                        self.session.inspector = Some(InspectorTarget::Sheet(id));
                    }
                }
                HandoffFix::Hardware(id) => {
                    if matches!(
                        self.request_navigation(NavigationRoute::Workspace(Workspace::Hardware)),
                        Outcome::Navigated
                    ) && self
                        .editor
                        .project()
                        .hardware
                        .iter()
                        .any(|hardware| hardware.id == id)
                    {
                        self.selection.choose(Some(id), false);
                    }
                }
                HandoffFix::Kerf => {
                    let _ = self.invoke(Request::new(A::EditKerf));
                }
                HandoffFix::CutFee => {
                    let _ = self.invoke(Request::new(A::EditCutFee));
                }
            }
        }
        if ready
            && self.current_reviewed_packet().is_none()
            && ui
                .button(self.localizer.text("export-review-action"))
                .clicked()
        {
            self.export_preparation = self.export_candidate.as_ref().map(|(key, result)| {
                (
                    key.clone(),
                    result
                        .as_ref()
                        .map(Arc::clone)
                        .map_err(|_| unreachable!("ready packet")),
                )
            });
        }
        if ui
            .add_enabled(
                self.current_reviewed_packet().is_some()
                    && self.export_activity.is_none()
                    && !self.modal_open(),
                egui::Button::new(self.localizer.text("export-choose")),
            )
            .clicked()
        {
            let _ = self.invoke(Request::new(A::ExportPdf));
        }
        if let Some(activity) = &self.export_activity {
            ui.label(self.localizer.text(match activity {
                ExportActivity::Choosing(..) => "export-choosing",
                _ => "export-working",
            }));
            if matches!(activity, ExportActivity::Writing)
                && ui.button(self.localizer.text("cancel")).clicked()
                && let Some(cancel) = &self.export_cancel
                && self
                    .invoke_contextual(Request::new(A::CancelExport))
                    .is_ok()
            {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        if let Some(message) = &self.export_message {
            ui.label(message);
        }
        ui.small(
            self.localizer
                .text(if self.current_reviewed_packet().is_some() {
                    "export-review-current"
                } else {
                    "export-review-stale"
                }),
        );
    }

    fn show_allocation_issues(&mut self, ui: &mut egui::Ui) {
        let project = self
            .editor
            .preview()
            .unwrap_or(self.editor.project())
            .clone();
        // A preview can change without a committed revision. Cache only committed
        // diagnostics, and invalidate on undo/redo and project replacement via id/revision.
        let key = sheet_ui::diagnostics_key(&project, self.editor.preview().is_some());
        let cache = if self.editor.preview().is_some() {
            &mut self.allocation_preview_diagnostics
        } else {
            &mut self.allocation_diagnostics
        };
        if cache.as_ref().is_none_or(|(cached, _)| *cached != key) {
            *cache = Some((key, diagnose(&project)));
        }
        let diagnostics = cache.as_ref().unwrap().1.clone();
        let issue_count = diagnostics
            .iter()
            .filter(|d| d.status != AllocationStatus::AllocatedValid)
            .count();
        ui.heading(format!(
            "{}: {} / {}",
            self.localizer.text("global-issues"),
            issue_count,
            diagnostics.len()
        ));
        for diagnostic in diagnostics
            .into_iter()
            .filter(|d| d.status != AllocationStatus::AllocatedValid)
        {
            let Some(board) = project.boards.iter().find(|b| b.id == diagnostic.board_id) else {
                continue;
            };
            let hidden = !self.selection.visible(&project, board.id);
            ui.group(|ui| {
                ui.label(format!(
                    "{} · {}{}",
                    board.name,
                    self.localizer.text(match diagnostic.status {
                        AllocationStatus::Unallocated => "board-unallocated",
                        AllocationStatus::Conflicted => "global-conflicted",
                        AllocationStatus::UnknownSearchBudget => "sheet-feasibility-unknown",
                        AllocationStatus::AllocatedValid => "board-allocated",
                    }),
                    if hidden {
                        format!(" · {}", self.localizer.text("global-hidden"))
                    } else {
                        String::new()
                    }
                ));
                ui.small(
                    diagnostic
                        .reasons
                        .iter()
                        .map(|r| self.localizer.text(r.key()))
                        .collect::<Vec<_>>()
                        .join(" · "),
                );
                ui.horizontal_wrapped(|ui| {
                    if ui.button(self.localizer.text("global-locate")).clicked() {
                        let _ = self.invoke(Request::with(A::LocateIssue, Target::Board(board.id)));
                    }
                    if ui
                        .add_enabled(
                            !self.modal_open() || self.sheet_repair.active(),
                            egui::Button::new(self.localizer.text("global-repair")),
                        )
                        .clicked()
                    {
                        let _ = self.invoke(Request::with(A::RepairIssue, Target::Board(board.id)));
                    }
                    if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("stock-new")),
                        )
                        .clicked()
                    {
                        let _ =
                            self.invoke(Request::with(A::AddIssueStock, Target::Board(board.id)));
                    }
                });
            });
        }
    }

    /// The status bar and rail share committed global diagnostics. Repair
    /// previews use their own key in the issue list, never masquerading as a
    /// saved plan. Neither bounded witness search runs on an unchanged frame.
    fn shell_facts(
        &mut self,
    ) -> (
        workspace_shell::IssueCounts,
        Option<plan_my_cabinet::money::Money>,
        bool,
    ) {
        let project = self.editor.project();
        // Project replacement clears the shared diagnostics cache in the
        // lifecycle controller, including a reopened document at the same
        // UUID/revision with different externally saved contents.
        if self.allocation_diagnostics.is_none() {
            self.shell_estimate = None;
            self.design_stock_snapshot = None;
        }
        let key = sheet_ui::diagnostics_key(project, false);
        if self
            .allocation_diagnostics
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            self.allocation_diagnostics = Some((key, diagnose(project)));
        }
        let counts = workspace_shell::IssueCounts::from_diagnostics(
            &self.allocation_diagnostics.as_ref().expect("diagnosed").1,
            |id| !self.selection.visible(project, id),
        );
        let estimate_key = (project.id, project.revision);
        if self
            .shell_estimate
            .as_ref()
            .is_none_or(|(cached, _)| *cached != estimate_key)
        {
            self.shell_estimate = Some((
                estimate_key,
                plan_my_cabinet::cost_estimate::estimate(project),
            ));
        }
        let total = workspace_shell::complete_spending(
            project,
            counts,
            self.shell_estimate
                .as_ref()
                .and_then(|(_, estimate)| estimate.as_ref().ok()),
        )
        .copied();
        let invalid = self
            .shell_estimate
            .as_ref()
            .is_some_and(|(_, result)| result.is_err());
        (counts, total, invalid)
    }

    fn show_shell_header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("workspace-header")
            .exact_size(workspace_shell::HEADER_HEIGHT)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme_widgets::PANEL)
                    .inner_margin(egui::Margin {
                        left: 16,
                        right: 12,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ui, |ui| {
                let width = ui.available_width();
                let compact = width < 1000.0;
                let chrome_enabled = !self.palette.open
                    && !self.other_modal_open()
                    && !self.project_files.blocking()
                    && self.navigation.pending().is_none();
                let panes = workspace_shell::PaneLayout::for_width(
                    self.session.active,
                    ui.ctx().content_rect().width() - workspace_shell::RAIL_WIDTH,
                );
                let full = ui.max_rect();
                // Centered command search, painted first so the left and right
                // clusters can never overlap it at the reference width.
                let search_width = if compact { 0.0 } else { (width - 640.0).clamp(220.0, 440.0) };
                if search_width > 0.0 {
                    let rect = egui::Rect::from_center_size(
                        full.center(),
                        egui::vec2(search_width, 30.0),
                    );
                    let response = ui
                        .interact(
                            rect,
                            egui::Id::new("header-command-search"),
                            if chrome_enabled {
                                egui::Sense::click()
                            } else {
                                egui::Sense::hover()
                            },
                        )
                        .on_hover_text(self.localizer.text("palette-title"));
                    let label = self.localizer.text("palette-title");
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, chrome_enabled, &label)
                    });
                    let painter = ui.painter();
                    painter.rect(
                        rect,
                        7.0,
                        if response.hovered() {
                            theme_widgets::CARD
                        } else {
                            theme_widgets::APP
                        },
                        egui::Stroke::new(1.0, theme_widgets::BORDER_SOFT),
                        egui::StrokeKind::Inside,
                    );
                    icons::icon(icons::Icon::Search, theme_widgets::FAINT, 14.0).paint_at(
                        ui,
                        egui::Rect::from_center_size(
                            rect.left_center() + egui::vec2(18.0, 0.0),
                            egui::Vec2::splat(14.0),
                        ),
                    );
                    painter.text(
                        rect.left_center() + egui::vec2(34.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        self.localizer.text("shell-search-placeholder"),
                        egui::FontId::proportional(13.0),
                        theme_widgets::FAINT,
                    );
                    let key = if cfg!(target_os = "macos") { "⌘K" } else { "Ctrl K" };
                    let key_rect = egui::Rect::from_center_size(
                        rect.right_center() - egui::vec2(24.0, 0.0),
                        egui::vec2(if cfg!(target_os = "macos") { 28.0 } else { 40.0 }, 18.0),
                    );
                    painter.rect_stroke(
                        key_rect,
                        4.0,
                        egui::Stroke::new(1.0, theme_widgets::BORDER_STRONG),
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        key_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        key,
                        egui::FontId::monospace(11.0),
                        theme_widgets::FAINT,
                    );
                    if response.clicked() {
                        self.palette.open(ui.ctx());
                    }
                }
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui
                        .add_enabled(
                            self.action_availability(Request::new(A::OpenWelcome)).is_ok(),
                            egui::Button::new(
                                egui::RichText::new(self.localizer.text("shell-projects"))
                                    .color(theme_widgets::MUTED),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        let _ = self.invoke(Request::new(A::OpenWelcome));
                    }
                    ui.label(egui::RichText::new("/").color(theme_widgets::DISABLED));
                    let name = self.editor.project().name.clone();
                    let shown: String = if name.chars().count() > 28 {
                        name.chars().take(26).chain("…".chars()).collect()
                    } else {
                        name.clone()
                    };
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                    ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                    ui.menu_button(
                        theme_widgets::semibold(ui, shown, 13.0).color(theme_widgets::TEXT),
                        |ui| {
                            ui.set_min_width(220.0);
                            self.show_project_controls(ui);
                        },
                    )
                    .response
                    .on_hover_text(format!("{name} · {}", self.localizer.text("shell-project-menu")));
                    if self.editor.is_dirty() {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 3.5, theme_widgets::ACCENT);
                        if !compact {
                            ui.label(
                                egui::RichText::new(self.localizer.text("shell-unsaved"))
                                    .size(11.5)
                                    .color(theme_widgets::MUTED),
                            );
                        }
                    }
                    for drawer in [
                        workspace_shell::Drawer::Controls,
                        workspace_shell::Drawer::Inspector,
                    ] {
                        if panes.collapsed(drawer) {
                            let (icon, label) = if drawer == workspace_shell::Drawer::Inspector {
                                (
                                    icons::Icon::Sliders,
                                    workspace_shell::inspector_label(self.localizer.language())
                                        .to_owned(),
                                )
                            } else {
                                (
                                    icons::Icon::List,
                                    self.localizer.text(
                                        workspace_shell::ENTRIES
                                            [(self.session.active.number() - 1) as usize]
                                            .1,
                                    ),
                                )
                            };
                            if theme_widgets::ghost_icon_sized(
                                ui,
                                icon,
                                &label,
                                theme_widgets::SECONDARY,
                                16.0,
                                30.0,
                                chrome_enabled,
                                self.open_drawer == Some(drawer),
                            )
                            .clicked()
                            {
                                self.open_drawer = if self.open_drawer == Some(drawer) {
                                    None
                                } else {
                                    Some(drawer)
                                };
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let export = Request::new(A::OpenHandoff);
                        if theme_widgets::icon_text_button(
                            ui,
                            icons::Icon::Export,
                            &self.localizer.text("shell-export"),
                            true,
                            self.action_availability(export).is_ok(),
                        )
                        .clicked()
                        {
                            let _ = self.invoke(export);
                        }
                        let save = Request::new(A::SaveProject);
                        if theme_widgets::secondary_button_enabled(
                            ui,
                            &A::SaveProject.label(&self.localizer),
                            self.action_availability(save).is_ok(),
                        )
                        .on_hover_text(A::SaveProject.label(&self.localizer))
                        .clicked()
                        {
                            let _ = self.invoke(save);
                        }
                        ui.add_space(4.0);
                        for (action, icon) in [(A::Redo, icons::Icon::Redo), (A::Undo, icons::Icon::Undo)] {
                            let request = Request::new(action);
                            if theme_widgets::ghost_icon_sized(
                                ui,
                                icon,
                                &action.label(&self.localizer),
                                theme_widgets::SECONDARY,
                                17.0,
                                32.0,
                                self.action_availability(request).is_ok(),
                                false,
                            )
                            .clicked()
                            {
                                let _ = self.invoke(request);
                            }
                        }
                        if compact
                            && theme_widgets::ghost_icon_sized(
                                ui,
                                icons::Icon::Search,
                                &self.localizer.text("palette-title"),
                                theme_widgets::SECONDARY,
                                16.0,
                                32.0,
                                chrome_enabled,
                                false,
                            )
                            .clicked()
                        {
                            self.palette.open(ui.ctx());
                        }
                    });
                });
            });
    }

    fn show_shell_status(
        &mut self,
        ui: &mut egui::Ui,
        issues: workspace_shell::IssueCounts,
        total: Option<plan_my_cabinet::money::Money>,
        invalid_estimate: bool,
    ) {
        egui::Panel::bottom("workspace-status")
            .default_size(workspace_shell::STATUS_HEIGHT)
            .min_size(workspace_shell::STATUS_HEIGHT)
            .max_size(workspace_shell::STATUS_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(theme_widgets::PANEL)
                    .inner_margin(egui::Margin::symmetric(12, 3)),
            )
            .show(ui, |ui| {
                // Locale and font scale change the amount of room required;
                // a fixed window-width breakpoint cannot prevent overlapping
                // Portuguese status text at 130%.
                if ui.available_width()
                    < self.shell_status_required_width(ui, issues, total.as_ref(), invalid_estimate)
                {
                    ui.horizontal(|ui| {
                        ui.label(self.localizer.text(
                            workspace_shell::ENTRIES[(self.session.active.number() - 1) as usize].1,
                        ));
                        ui.menu_button(
                            workspace_shell::more_label(self.localizer.language()),
                            |ui| {
                                egui::ScrollArea::both()
                                    .max_width((ui.ctx().content_rect().width() - 80.0).max(180.0))
                                    .max_height(
                                        (ui.ctx().content_rect().height() - 100.0).max(100.0),
                                    )
                                    .show(ui, |ui| {
                                        self.show_shell_status_contents(
                                            ui,
                                            issues,
                                            total,
                                            invalid_estimate,
                                        );
                                    });
                            },
                        )
                        .response
                        .on_hover_text(workspace_shell::more_label(self.localizer.language()));
                    });
                } else {
                    self.show_shell_status_contents(ui, issues, total, invalid_estimate);
                }
            });
    }

    fn show_shell_status_contents(
        &mut self,
        ui: &mut egui::Ui,
        issues: workspace_shell::IssueCounts,
        total: Option<plan_my_cabinet::money::Money>,
        invalid_estimate: bool,
    ) {
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let small = |text: String| egui::RichText::new(text).size(11.5).color(theme_widgets::MUTED);
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            if let Some(error) = &self.preferences_error {
                ui.label(
                    egui::RichText::new(self.localizer.text("preferences-save-warning"))
                        .size(11.5)
                        .color(theme_widgets::WARN_INK),
                )
                .on_hover_text(error);
                ui.add_space(12.0);
            }
            let project = self.editor.project();
            let confirmed = project.confirmed_shop_kerf == Some(project.cutting_kerf);
            let kerf = assembly_ui::short_length(project.cutting_kerf, locale);
            ui.add(icons::icon(
                if confirmed {
                    icons::Icon::Check
                } else {
                    icons::Icon::Warning
                },
                if confirmed {
                    theme_widgets::OK
                } else {
                    theme_widgets::WARN
                },
                13.0,
            ));
            let kerf_label = ui.label(
                egui::RichText::new(if confirmed {
                    format!("{} {kerf} mm", self.localizer.text("shell-kerf"))
                } else {
                    format!(
                        "{} {kerf} mm · {}",
                        self.localizer.text("shell-kerf"),
                        self.localizer.text("shell-unconfirmed")
                    )
                })
                .size(11.5)
                .color(if confirmed {
                    theme_widgets::OK_INK
                } else {
                    theme_widgets::WARN_INK
                }),
            );
            if let Some(date) = kerf_confirmation_label(project, &self.localizer) {
                kerf_label.on_hover_text(date);
            }
            ui.add_space(12.0);
            match self.session.active {
                Workspace::Design | Workspace::Hardware => {
                    ui.label(small(format!(
                        "{} {} mm",
                        self.localizer.text("shell-grid"),
                        assembly_ui::short_length(project.grid_spacing, locale)
                    )));
                    ui.add_space(12.0);
                    ui.label(small(format!(
                        "{} {}",
                        project.boards.len(),
                        self.localizer.text("shell-parts")
                    )));
                }
                Workspace::Stock => {
                    ui.label(small(format!(
                        "{} {}",
                        project.stock.len(),
                        self.localizer.text("shell-stock-pieces")
                    )));
                }
                Workspace::CutPlan => {
                    ui.label(small(format!(
                        "{} {}",
                        project.allocations.len(),
                        self.localizer.text("shell-placements")
                    )));
                }
                Workspace::Handoff => {
                    ui.label(small(format!(
                        "{} {}",
                        project.export_records.len(),
                        self.localizer.text("shell-receipts")
                    )));
                }
            }
            if issues.total() > 0 {
                ui.add_space(12.0);
                ui.add(icons::icon(icons::Icon::Warning, theme_widgets::WARN, 13.0));
                let mut parts = Vec::new();
                if issues.unallocated > 0 {
                    parts.push(format!(
                        "{} {}",
                        issues.unallocated,
                        self.localizer.text("shell-unallocated")
                    ));
                }
                if issues.conflicted > 0 {
                    parts.push(format!(
                        "{} {}",
                        issues.conflicted,
                        self.localizer.text("global-conflicted")
                    ));
                }
                if issues.unknown_proof > 0 {
                    parts.push(format!(
                        "{} {}",
                        issues.unknown_proof,
                        self.localizer.text("shell-proof-unknown")
                    ));
                }
                let response = ui.add(
                    egui::Label::new(
                        egui::RichText::new(parts.join(" · "))
                            .size(11.5)
                            .color(theme_widgets::WARN_INK),
                    )
                    .sense(egui::Sense::click()),
                );
                let response = if issues.hidden > 0 {
                    response.on_hover_text(format!(
                        "{} {}",
                        issues.hidden,
                        self.localizer.text("global-hidden")
                    ))
                } else {
                    response
                };
                if response.clicked() && self.session.active != Workspace::CutPlan {
                    self.request_navigation(NavigationRoute::Workspace(Workspace::CutPlan));
                }
            }
            if self.optimizer.running() {
                ui.add_space(12.0);
                ui.label(small(self.localizer.text("shell-search-active")));
            }
            if self.export_activity.is_some() {
                ui.add_space(12.0);
                ui.label(small(self.localizer.text("shell-export-active")));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let money_locale = if self.localizer.language() == Language::En {
                    plan_my_cabinet::money::MoneyLocale::English
                } else {
                    plan_my_cabinet::money::MoneyLocale::PortugueseBrazil
                };
                let (amount, known) = if invalid_estimate {
                    (self.localizer.text("cost-invalid"), false)
                } else {
                    total.map_or_else(
                        || (self.localizer.text("cost-incomplete"), false),
                        |amount| (amount.display(money_locale), true),
                    )
                };
                ui.label(if known {
                    theme_widgets::mono(amount, 11.5).color(theme_widgets::TEXT)
                } else {
                    egui::RichText::new(amount)
                        .size(11.5)
                        .color(theme_widgets::WARN_INK)
                });
                ui.label(small(self.localizer.text("shell-est-spending")));
                let hint = match (self.session.active, self.move_tool.mode) {
                    (Workspace::Design, viewport::ToolMode::Move) => Some("viewport-move-hint"),
                    (Workspace::Design, viewport::ToolMode::Measure) => {
                        Some("viewport-measure-hint")
                    }
                    _ => None,
                };
                if self.preferences.navigation_hints
                    && let Some(key) = hint
                {
                    ui.add_space(14.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(self.localizer.text(key))
                                .size(11.5)
                                .color(theme_widgets::FAINT),
                        )
                        .truncate(),
                    );
                }
            });
        });
    }

    fn shell_status_required_width(
        &self,
        ui: &egui::Ui,
        issues: workspace_shell::IssueCounts,
        total: Option<&plan_my_cabinet::money::Money>,
        invalid_estimate: bool,
    ) -> f32 {
        let p = self.editor.project();
        let en = self.localizer.language() == Language::En;
        let (count, count_key, hint_key) = match self.session.active {
            Workspace::Design => (p.boards.len(), "board-list", "shell-hint-design"),
            Workspace::Stock => (p.stock.len(), "shell-stock-pieces", "shell-hint-stock"),
            Workspace::CutPlan => (
                p.allocations.len(),
                "shell-placements",
                "shell-hint-cut-plan",
            ),
            Workspace::Hardware => (
                p.hinge_installations.len(),
                "shell-installations",
                "shell-hint-hardware",
            ),
            Workspace::Handoff => (
                p.export_records.len(),
                "shell-receipts",
                "shell-hint-handoff",
            ),
        };
        let amount = if invalid_estimate {
            self.localizer.text("cost-invalid")
        } else {
            total.map_or_else(
                || self.localizer.text("cost-incomplete"),
                |m| {
                    m.display(if en {
                        plan_my_cabinet::money::MoneyLocale::English
                    } else {
                        plan_my_cabinet::money::MoneyLocale::PortugueseBrazil
                    })
                },
            )
        };
        let mut texts = vec![
            format!(
                "{} {} mm · {}",
                self.localizer.text("shell-kerf"),
                assembly_ui::short_length(p.cutting_kerf, if en { Locale::En } else { Locale::PtBr }),
                self.localizer
                    .text(if p.confirmed_shop_kerf == Some(p.cutting_kerf) {
                        ""
                    } else {
                        "shell-unconfirmed"
                    })
            ),
            format!("{count} {}", self.localizer.text(count_key)),
            format!("{} {amount}", self.localizer.text("shell-est-spending")),
        ];
        let _ = hint_key;
        if self.preferences_error.is_some() {
            texts.push(self.localizer.text("preferences-save-warning"));
        }
        if issues.total() > 0 {
            texts.push(format!(
                "{} {} · {} {} · {} {}",
                issues.unallocated,
                self.localizer.text("shell-unallocated"),
                issues.conflicted,
                self.localizer.text("global-conflicted"),
                issues.unknown_proof,
                self.localizer.text("shell-proof-unknown")
            ));
            if issues.hidden > 0 {
                texts.push(format!(
                    " · {} {}",
                    issues.hidden,
                    self.localizer.text("global-hidden")
                ));
            }
        }
        if self.optimizer.running() {
            texts.push(self.localizer.text("shell-search-active"));
        }
        if self.export_activity.is_some() {
            texts.push(self.localizer.text("shell-export-active"));
        }
        // Body is a conservative upper bound for the Small labels. Include
        // gaps/separators so borderline widths choose the accessible More menu.
        let font = egui::TextStyle::Body.resolve(ui.style());
        64.0 + texts
            .iter()
            .map(|text| {
                ui.painter()
                    .layout_no_wrap(text.clone(), font.clone(), theme_widgets::TEXT)
                    .size()
                    .x
                    + ui.spacing().item_spacing.x
            })
            .sum::<f32>()
    }

    fn show_workspace_inspector(&mut self, ui: &mut egui::Ui) {
        let target = self.session.inspector;
        match self.session.active {
            Workspace::Design => {
                if let Some(model) = self.design_model() {
                    self.show_design_inspector(ui, &model);
                } else {
                    ui.label(self.localizer.text("measurement-invalid"));
                }
            }
            Workspace::CutPlan => {
                if let Some(id) = self.session.allocation_issue {
                    ui.colored_label(
                        theme_widgets::WARN_INK,
                        format!("{} · {id}", self.localizer.text("shell-unallocated")),
                    );
                }
                sheet_ui::show_focused_inspector(
                    ui,
                    &self.editor,
                    &self.localizer,
                    &mut self.sheet_repair,
                );
                let blocked = self.external_modal_open();
                self.optimizer
                    .show(ui, &mut self.editor, &self.localizer, blocked);
                self.optimizer.show_inspector_comparison(
                    ui,
                    self.editor.project(),
                    &self.localizer,
                    self.external_modal_open(),
                );
            }
            Workspace::Hardware => {
                if let Some(InspectorTarget::Installation(id)) = target {
                    self.show_selected_installation_inspector(ui, id);
                } else {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(14, 16))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(self.localizer.text(
                                    if self.editor.project().hinge_installations.is_empty() {
                                        "hardware-no-installations"
                                    } else {
                                        "hardware-select-installation"
                                    },
                                ))
                                .size(12.5)
                                .color(theme_widgets::MUTED),
                            );
                        });
                }
            }
            Workspace::Stock => self.show_stock_inspector(ui),
            Workspace::Handoff => {
                receipt_ui::show_receipts(ui, self.editor.project(), &self.localizer);
            }
        }
    }

    fn show_scrolled_workspace_inspector(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        let index = (self.session.active.number() - 1) as usize;
        let width = width.min(ui.available_width());
        let scroll = egui::ScrollArea::vertical()
            .id_salt(("workspace-inspector", self.session.active.number()))
            .max_height(height)
            .auto_shrink([false, false])
            .vertical_scroll_offset(self.inspector_scroll[index].y)
            .show(ui, |ui| {
                ui.set_width(width.max(1.0));
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                self.show_workspace_inspector(ui);
            });
        self.inspector_scroll[index] = scroll.state.offset;
    }

    /// Left pane: one scroll area per workspace; content lives with its workspace.
    fn show_controls_pane(&mut self, ui: &mut egui::Ui) {
        let active = self.session.active;
        if active == Workspace::Hardware {
            egui::Panel::bottom(egui::Id::new("hardware-controls-footer"))
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .fill(theme_widgets::PANEL)
                        .inner_margin(egui::Margin::symmetric(12, 10)),
                )
                .show(ui, |ui| self.show_hardware_footer(ui));
        }
        let scroll = egui::ScrollArea::vertical()
            .id_salt((
                "workspace-controls-scroll",
                self.editor.project().id,
                active.number(),
            ))
            .auto_shrink([false, false])
            .vertical_scroll_offset(self.session.view(active).scroll)
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                match active {
                    Workspace::Design => self.show_design_controls(ui),
                    Workspace::Stock => {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(10, 4))
                            .show(ui, |ui| self.show_stock_materials(ui));
                    }
                    Workspace::CutPlan => {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(10, 4))
                            .show(ui, |ui| {
                                let blocked = self.palette.open || self.other_modal_open();
                                if let Some(request) = sheet_ui::show_sheet_list(
                                    ui,
                                    &self.editor,
                                    &self.selection,
                                    &self.localizer,
                                    &mut self.sheet_repair,
                                    blocked,
                                    sheet_ui::SheetFocus {
                                        sheet: self.session.focused_sheet,
                                        issue: self.session.allocation_issue,
                                        scroll_to_target: self.session.pending_cut_focus,
                                    },
                                ) {
                                    let _ = self.invoke(request);
                                }
                                self.show_allocation_issues(ui);
                            });
                    }
                    Workspace::Hardware => {
                        self.show_pinned_catalog(ui);
                        self.show_hinge_list(ui);
                        self.show_hardware_list(ui);
                    }
                    Workspace::Handoff => {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(14, 8))
                            .show(ui, |ui| self.show_export_preparation(ui));
                    }
                }
            });
        self.session.view_mut(active).scroll = scroll.state.offset.y;
    }

    fn show_design_controls(&mut self, ui: &mut egui::Ui) {
        self.selection.retain_objects(self.editor.project());
        self.show_hierarchy(ui);
        let mut notices = Vec::new();
        if let Some(notice) = self.first_fit_notice {
            notices.push(self.localizer.text(match notice {
                FirstFit::Allocated(_) => "first-fit-allocated",
                FirstFit::NoFit => "first-fit-no-fit",
                FirstFit::SearchExhausted => "first-fit-exhausted",
            }));
        }
        for conflict in &self.material_conflicts {
            let name = self
                .editor
                .project()
                .boards
                .iter()
                .find(|b| b.id == conflict.board_id)
                .map(|b| b.name.as_str())
                .unwrap_or("—");
            notices.push(format!(
                "{}: {}",
                name,
                conflict_labels(&self.localizer, conflict)
            ));
        }
        let template = self.template_message.clone();
        if notices.is_empty() && template.is_none() && !self.board_action_error {
            return;
        }
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                theme_widgets::warn_callout().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for notice in notices {
                        ui.label(
                            egui::RichText::new(notice)
                                .size(12.0)
                                .color(theme_widgets::WARN_INK),
                        );
                    }
                    if self.board_action_error {
                        ui.label(
                            egui::RichText::new(self.localizer.text("error-board-duplicate"))
                                .size(12.0)
                                .color(theme_widgets::DANGER),
                        );
                    }
                    if let Some(message) = template {
                        ui.label(
                            egui::RichText::new(message)
                                .size(12.0)
                                .color(theme_widgets::WARN_INK),
                        );
                        if theme_widgets::text_button(
                            ui,
                            &self.localizer.text("template-setup-open-stock"),
                            theme_widgets::ACCENT_DARK,
                            true,
                        )
                        .clicked()
                        {
                            self.request_navigation(NavigationRoute::Workspace(Workspace::Stock));
                        }
                    }
                });
            });
    }

    /// Central pane: 3D viewport, sheet canvas, stock table or PDF preview.
    fn show_canvas_pane(&mut self, ui: &mut egui::Ui, drawer_modal: bool, inspector_visible: bool) {
        match self.session.active {
            Workspace::Stock => {
                egui::ScrollArea::vertical()
                    .id_salt(("stock-content", self.editor.project().id))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(24, 20))
                            .show(ui, |ui| self.show_stock_list(ui));
                    });
            }
            Workspace::Handoff => {
                egui::ScrollArea::vertical()
                    .id_salt(("handoff-content", self.editor.project().id))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(16, 12))
                            .show(ui, |ui| {
                                let packet = self.export_candidate.as_ref().and_then(|(key, result)| {
                                    (key == &self.export_key())
                                        .then(|| result.as_ref().ok())
                                        .flatten()
                                        .cloned()
                                });
                                if let Some(packet) = packet {
                                    let labels = DocumentPreviewLabels {
                                        previous: &self.localizer.text("export-preview-previous"),
                                        next: &self.localizer.text("export-preview-next"),
                                        page: &self.localizer.text("export-preview-page"),
                                        zoom: &self.localizer.text("export-preview-zoom"),
                                        fit: &self.localizer.text("export-preview-fit"),
                                    };
                                    if show_document_preview(
                                        ui,
                                        packet.document(),
                                        &mut self.export_preview,
                                        &labels,
                                    )
                                    .is_err()
                                    {
                                        ui.label(self.localizer.text("export-render-failed"));
                                    }
                                } else {
                                    ui.label(self.localizer.text("export-review-stale"));
                                }
                            });
                    });
            }
            Workspace::CutPlan => {
                let other_modal = drawer_modal
                    || self.blocking_surface_open()
                    || self.door_motion.is_some();
                if let Some(request) = sheet_ui::show_with_layout(
                    ui,
                    &mut self.editor,
                    &mut self.selection,
                    &self.localizer,
                    other_modal,
                    &mut self.sheet_repair,
                    sheet_ui::SheetFocus {
                        sheet: self.session.focused_sheet,
                        issue: self.session.allocation_issue,
                        scroll_to_target: self.session.pending_cut_focus,
                    },
                    inspector_visible,
                ) {
                    let _ = self.invoke(request);
                }
                self.session.pending_cut_focus = false;
            }
            Workspace::Design | Workspace::Hardware => self.show_scene_pane(ui, drawer_modal),
        }
    }

    fn show_scene_pane(&mut self, ui: &mut egui::Ui, drawer_modal: bool) {
        let modal = drawer_modal || self.blocking_surface_open() || self.sheet_repair.active();
        if self.door_motion.is_some_and(|(id, angle)| {
            self.editor
                .project()
                .door_joints
                .iter()
                .find(|j| j.id == id)
                .is_none_or(|j| {
                    plan_my_cabinet::door_joint::derived_poses(self.editor.project(), j, angle)
                        .is_err()
                })
        }) {
            self.door_motion = None;
        }
        let motion_poses = self.door_motion.and_then(|(id, angle)| {
            self.editor
                .project()
                .door_joints
                .iter()
                .find(|j| j.id == id)
                .and_then(|j| {
                    plan_my_cabinet::door_joint::derived_poses(self.editor.project(), j, angle).ok()
                })
                .map(|poses| poses.into_iter().collect::<std::collections::HashMap<_, _>>())
        });
        let surface = ui.allocate_ui_with_layout(
            ui.available_size(),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let overlay_blocked = self.session.active == Workspace::Hardware
                    && self.show_hardware_motion_overlay(
                        ui.ctx(),
                        ui.available_rect_before_wrap().intersect(ui.clip_rect()),
                    );
                viewport::show_move_with_hardware(
                    ui,
                    &mut self.camera,
                    self.editor.preview().unwrap_or(self.editor.project()),
                    &mut self.selection,
                    &mut self.move_tool,
                    modal || overlay_blocked,
                    self.localizer.language(),
                    self.preferences.inverse_scroll_zoom,
                    self.preferences.material_tint,
                    self.placement.as_ref().and_then(PlacementDialog::highlighted),
                    motion_poses.as_ref(),
                    self.measurement_scope,
                    self.measurement_frame,
                    match (self.session.active, self.session.inspector) {
                        (Workspace::Hardware, Some(InspectorTarget::Installation(id))) => Some(id),
                        _ => None,
                    },
                    self.session.active == Workspace::Hardware,
                )
            },
        );
        let action = surface.inner;
        if let Some(capture) = &mut self.capture {
            capture.set_snap_evidence(self.move_tool.capture_evidence());
        }
        if self.move_tool.take_grid_edit_request() {
            let _ = self.invoke(Request::new(A::EditGrid));
        }
        if let Some(proposal) = action.selection {
            self.request_scene_selection(proposal.picked, proposal.additive);
        }
        match action.drag {
            Some(viewport::DragAction::Preview(id, pose)) => {
                if let Ok(mut session) =
                    plan_my_cabinet::placement::PlacementSession::resume(&mut self.editor, id)
                {
                    if session.preview_free(pose).is_ok() {
                        ui.ctx().request_repaint();
                    }
                    session.pause();
                }
            }
            Some(viewport::DragAction::Accept(id, pose)) => {
                if let Some(pose) = pose {
                    if let Ok(mut session) =
                        plan_my_cabinet::placement::PlacementSession::resume(&mut self.editor, id)
                        && session.preview_free(pose).is_ok()
                    {
                        let _ = session.accept();
                    }
                } else {
                    self.editor.cancel_preview();
                }
            }
            Some(viewport::DragAction::Cancel(ids, active)) => {
                self.editor.cancel_preview();
                self.selection.ids = ids;
                self.selection.active = active;
            }
            None => {}
        }
        // A collapsed pane is a foreground drawer. Keep the app-owned draft
        // alive, but do not let the HUD cover its close/actions or a modal.
        if self.session.active == Workspace::Design && self.design_hud_available() {
            self.show_design_hud(ui.ctx(), surface.response.rect);
        }
    }

    fn show_workspace(&mut self, ui: &mut egui::Ui) {
        self.tick_project_files(ui.ctx());
        self.poll_pdf_export(ui.ctx());
        self.tick_export_preparation(ui.ctx());
        // Worker completion belongs to the app session, not the Cut plan pane.
        // Its candidate is still only applied after explicit review there.
        self.optimizer.poll(ui.ctx());
        self.sync_scene_inspector();
        if let Some(action) = actions::project_shortcut(ui.ctx(), self.modal_open()) {
            let _ = self.invoke(Request::new(action));
        }
        let (issues, total, invalid_estimate) = self.shell_facts();
        if command_palette::shortcut(
            ui.ctx(),
            !self.palette.open
                && !self.optimizer.comparison_open()
                && !self.other_modal_open()
                && !self.project_files.blocking()
                && self.navigation.pending().is_none(),
        ) {
            self.palette.open(ui.ctx());
        }
        if let Some(workspace) = workspace_shell::shortcut(
            ui.ctx(),
            self.palette.open
                || self.project_files.blocking()
                || self.navigation.pending().is_some()
                || (self.other_modal_open()
                    && self.board_dimension.is_none()
                    && self.placement.is_none()),
        ) {
            self.request_navigation(NavigationRoute::Workspace(workspace));
        }
        if let Some(workspace) = workspace_shell::rail(
            ui,
            self.session.active,
            &self.localizer,
            !self.palette.open
                && !self.optimizer.comparison_open()
                && !self.project_files.blocking()
                && (!self.other_modal_open()
                    || self.board_dimension.is_some()
                    || self.placement.is_some()),
            issues,
        ) {
            match workspace {
                workspace_shell::RailAction::Workspace(workspace) => {
                    self.request_navigation(NavigationRoute::Workspace(workspace));
                }
                workspace_shell::RailAction::Language(language) => {
                    let _ = self.invoke(
                        Request::new(A::SetUiLanguage).argument(Argument::Language(language)),
                    );
                }
                workspace_shell::RailAction::Settings => {
                    let _ = self.invoke(Request::new(A::OpenSettings));
                }
            }
        }
        self.show_shell_header(ui);
        self.show_shell_status(ui, issues, total, invalid_estimate);
        let camera_workspace = self.session.active;
        std::mem::swap(
            &mut self.camera,
            &mut self.session.view_mut(camera_workspace).camera,
        );
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let available = ui.available_size();
                let active = self.session.active;
                let layout = workspace_shell::PaneLayout::for_width(active, available.x);
                if self.open_drawer.is_some_and(|drawer| !layout.collapsed(drawer)) {
                    self.open_drawer = None;
                }
                let drawer_pos = ui.min_rect().min;
                let pane_frame = egui::Frame::new()
                    .fill(theme_widgets::PANEL)
                    .stroke(egui::Stroke::new(1.0, theme_widgets::BORDER));
                if layout.controls > 0.0 {
                    egui::Panel::left(egui::Id::new(("workspace-controls", active.number())))
                        .exact_size(layout.controls)
                        .resizable(false)
                        .frame(pane_frame)
                        .show(ui, |ui| self.show_controls_pane(ui));
                } else if self.open_drawer == Some(workspace_shell::Drawer::Controls) {
                    let mut open = true;
                    let width = workspace_shell::PaneLayout::preferred(active)
                        .controls
                        .min(available.x - 20.0);
                    egui::Window::new(self.localizer.text(
                        workspace_shell::ENTRIES[(active.number() - 1) as usize].1,
                    ))
                    .id(egui::Id::new("workspace-controls-drawer"))
                    .fixed_pos(drawer_pos)
                    .fixed_size(egui::vec2(width, (available.y - 50.0).max(1.0)))
                    .resizable(false)
                    .collapsible(false)
                    .open(&mut open)
                    .show(ui.ctx(), |ui| self.show_controls_pane(ui));
                    if !open {
                        self.open_drawer = None;
                    }
                }
                if active == Workspace::CutPlan
                    && layout.controls == 0.0
                    && self.open_drawer != Some(workspace_shell::Drawer::Controls)
                    && self.optimizer.comparison_open()
                {
                    let blocked = self.external_modal_open();
                    self.optimizer.show_comparison(
                        ui.ctx(),
                        &mut self.editor,
                        &self.localizer,
                        blocked,
                    );
                }
                if layout.inspector > 0.0 {
                    egui::Panel::right(egui::Id::new(("workspace-inspector", active.number())))
                        .exact_size(layout.inspector)
                        .resizable(false)
                        .frame(pane_frame)
                        .show(ui, |ui| {
                            let height = ui.available_height();
                            self.show_scrolled_workspace_inspector(ui, layout.inspector, height);
                        });
                } else if self.open_drawer == Some(workspace_shell::Drawer::Inspector) {
                    let mut open = true;
                    let width = workspace_shell::PaneLayout::preferred(active)
                        .inspector
                        .min(available.x - 20.0);
                    egui::Window::new(workspace_shell::inspector_label(self.localizer.language()))
                        .id(egui::Id::new("workspace-inspector-drawer"))
                        .fixed_pos(egui::pos2(
                            drawer_pos.x + available.x - width - 20.0,
                            drawer_pos.y,
                        ))
                        .resizable(false)
                        .collapsible(false)
                        .open(&mut open)
                        .show(ui.ctx(), |ui| {
                            ui.set_width(width);
                            self.show_scrolled_workspace_inspector(ui, width, available.y - 44.0);
                        });
                    if !open {
                        self.open_drawer = None;
                    }
                }
                let canvas_fill = match active {
                    Workspace::Design | Workspace::Hardware | Workspace::CutPlan => {
                        theme_widgets::VIEWPORT
                    }
                    Workspace::Stock => theme_widgets::APP,
                    Workspace::Handoff => egui::Color32::from_rgb(226, 221, 212),
                };
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(canvas_fill))
                    .show(ui, |ui| {
                        let drawer_modal = self
                            .open_drawer
                            .is_some_and(|drawer| layout.collapsed(drawer));
                        self.show_canvas_pane(ui, drawer_modal, layout.inspector > 0.0);
                    });
            });
        std::mem::swap(
            &mut self.camera,
            &mut self.session.view_mut(camera_workspace).camera,
        );
        self.sync_scene_inspector();
        self.show_dialog(ui.ctx());
        self.show_material_edit(ui.ctx());
        self.show_board_material(ui.ctx());
        self.show_board_dimension(ui.ctx());
        self.show_batch_dimension(ui.ctx());
        self.show_placement(ui.ctx());
        self.show_grid_dialog(ui.ctx());
        self.show_kerf_confirmation(ui.ctx());
        self.show_stock_dialog(ui.ctx());
        self.show_cut_fee_dialog(ui.ctx());
        self.show_currency_dialog(ui.ctx());
        self.show_assembly_dialog(ui.ctx());
        self.show_hardware_dialog(ui.ctx());
        self.show_hinge_dialog(ui.ctx());
        self.show_door_dialog(ui.ctx());
        self.show_removal_dialog(ui.ctx());
        self.show_project_dialog(ui.ctx());
        self.show_export_overwrite(ui.ctx());
        self.show_navigation_prompt(ui.ctx());
        self.show_palette(ui.ctx());
        self.show_settings(ui.ctx());
        if matches!(self.export_activity, Some(ExportActivity::Choosing(..)))
            && self.export_picker_key.is_none()
        {
            self.export_picker_key = self
                .current_reviewed_packet()
                .map(|packet| packet.key().clone());
        }
    }
}

impl eframe::App for DesktopApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        if let Some(capture) = &self.capture {
            capture.filter_input(input);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_close_request(ui.ctx());
        if self.project_files.allow_close {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.capture.as_ref().is_some_and(capture::Capture::gallery) {
            widget_gallery::show(ui);
        } else if (self.capture.is_none()
            || self
                .capture
                .as_ref()
                .is_some_and(capture::Capture::welcome_empty))
            && self.project_files.welcome.visible
        {
            self.show_welcome(ui);
            self.show_project_dialog(ui.ctx());
            self.show_navigation_prompt(ui.ctx());
            self.show_settings(ui.ctx());
            self.show_template_setup(ui.ctx());
        } else {
            self.show_workspace(ui);
        }
        if let Some(capture) = &mut self.capture
            && capture.tick(ui.ctx())
        {
            self.project_files.allow_close = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl DesktopApp {
    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !self.project_files.allow_close && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_project_action(project_ui::NextAction::Close);
            if self.project_files.allow_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

fn native_backend() -> wgpu::Backends {
    #[cfg(target_os = "macos")]
    {
        wgpu::Backends::METAL
    }
    #[cfg(target_os = "linux")]
    {
        wgpu::Backends::VULKAN
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        wgpu::Backends::PRIMARY
    }
}

/// Never derive preferences from a project path or the working directory.
/// XDG_CONFIG_HOME must itself be absolute; a broken platform environment is
/// reported rather than silently switching to a portable/local file.
fn platform_config_dir() -> Result<PathBuf, &'static str> {
    #[cfg(target_os = "macos")]
    let path = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support/PlanMyCabinet"));
    #[cfg(target_os = "linux")]
    let path = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|base| base.join("plan-my-cabinet"));
    #[cfg(target_os = "windows")]
    let path = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|base| base.join("PlanMyCabinet"));
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let path: Option<PathBuf> = None;
    path.filter(|path| path.is_absolute())
        .ok_or("A valid absolute platform configuration directory is unavailable")
}

fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{}", capture::HELP);
        return std::process::ExitCode::SUCCESS;
    }
    let capture_config = match capture::Config::parse(args) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    // Isolated visual-review variants. They affect only the in-memory capture
    // session, never the persisted project or platform preferences.
    let viewport_capture_state = if capture_config.is_some() {
        match std::env::var("PMCAB_CAPTURE_VIEWPORT_STATE").as_deref() {
            Ok("neutral") => "neutral",
            Ok("hidden") => "hidden",
            Ok("neutral-hidden") => "neutral-hidden",
            Ok("") | Err(_) => "baseline",
            Ok(other) => {
                eprintln!("Unknown PMCAB_CAPTURE_VIEWPORT_STATE: {other}");
                return std::process::ExitCode::FAILURE;
            }
        }
    } else {
        "baseline"
    };
    if let Some(config) = &capture_config
        && let Err(error) = config.prepare_directory()
    {
        eprintln!("Cannot create isolated capture directory: {error}");
        return std::process::ExitCode::FAILURE;
    }
    let capturing = capture_config.is_some();
    let completion: capture::Completion = Default::default();
    let capture_completion = completion.clone();
    let size = capture_config.as_ref().map_or([1100.0, 720.0], |c| {
        c.size.map(|n| n as f32 * c.scale as f32 / 100.0)
    });
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_resizable(!capturing),
        ..Default::default()
    };
    if let WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        setup.instance_descriptor.backends = native_backend();
    }

    match eframe::run_native(
        plan_my_cabinet::APPLICATION_NAME,
        options,
        Box::new(move |context| {
            theme::install_fonts(&context.egui_ctx);
            icons::install_loaders(&context.egui_ctx);
            theme_widgets::apply_visuals(&context.egui_ctx);
            if let Some(state) = &context.wgpu_render_state {
                viewport::install(state);
            }
            let mut app = DesktopApp::default();
            if let Some(config) = capture_config {
                // Capture-only backend diagnostic: egui-wgpu 0.36.2 can read
                // screenshots only from non-sRGB RGBA/BGRA8 surface formats.
                // Record the actual native target before attempting a capture.
                let target_format = context
                    .wgpu_render_state
                    .as_ref()
                    .map(|state| format!("{:?}", state.target_format));
                let readback_supported = target_format
                    .as_deref()
                    .is_some_and(|format| matches!(format, "Rgba8Unorm" | "Bgra8Unorm"));
                std::fs::write(
                    config.directory.join("renderer-diagnostic.json"),
                    format!(
                        "{{\"target_format\":{},\"egui_wgpu_readback_supported\":{readback_supported}}}\n",
                        serde_json::to_string(&target_format).expect("format string is JSON"),
                    ),
                )
                .expect("isolated capture directory is writable");
                context
                    .egui_ctx
                    .set_zoom_factor(config.scale as f32 / 100.0);
                let mut fixture = plan_my_cabinet::reference_fixture::project();
                // Presentation metadata for the native styling review; the
                // deterministic fixture's geometry and stock remain identical.
                use plan_my_cabinet::domain::SrgbColor;
                fixture.material_colors.insert(
                    plan_my_cabinet::reference_fixture::WHITE_ID,
                    SrgbColor([233, 231, 226]),
                );
                fixture.material_colors.insert(
                    plan_my_cabinet::reference_fixture::OAK_ID,
                    SrgbColor([185, 139, 94]),
                );
                fixture.material_colors.insert(
                    plan_my_cabinet::reference_fixture::HDF_ID,
                    SrgbColor([122, 98, 70]),
                );
                app.editor = ProjectEditor::new(fixture).expect("validated reference fixture");
                app.session = WorkspaceSession::new(app.editor.project());
                app.session.active = config.workspace;
                if let Some(section) = config.settings {
                    app.settings_state.section = section;
                    app.settings_open = true;
                }
                if config.workspace == Workspace::Stock {
                    app.session.stock_piece =
                        Some(plan_my_cabinet::reference_fixture::WHITE_STOCK_ID);
                }
                if config.workspace == Workspace::Handoff {
                    let packet = Arc::new(
                        ReviewedPacket::prepare(
                            app.editor.project(),
                            app.export_mode,
                            app.export_settings(),
                            app.export_sections,
                        )
                        .expect("capture fixture draft packet"),
                    );
                    // Capture-only comparison artifact: serialize precisely
                    // the frozen pages shown by the native preview. This is
                    // not an export action and must never create a receipt.
                    std::fs::write(
                        config.directory.join("reviewed-preview.pdf"),
                        plan_my_cabinet::pdf_export::render_document_pdf(packet.document())
                            .expect("capture packet has serializable pages"),
                    )
                    .expect("isolated capture directory is writable");
                    let key = app.export_key();
                    app.export_candidate = Some((key.clone(), Ok(Arc::clone(&packet))));
                    app.export_preparation = Some((key, Ok(packet)));
                    if let Some(page) = config.page {
                        app.export_preview.page = page.saturating_sub(1);
                    }
                }
                app.localizer = Localizer::new(config.language);
                app.session.design.camera = viewport::Camera::reference_baseline();
                app.session.hardware.camera = viewport::Camera::reference_baseline();
                app.selection
                    .choose(Some(plan_my_cabinet::reference_fixture::SHELF_ID), false);
                if config.workspace != Workspace::Hardware {
                    app.selection
                        .hidden
                        .insert(plan_my_cabinet::reference_fixture::DOORS_ID);
                }
                if config.workspace == Workspace::Hardware {
                    app.selection.choose(None, false);
                    app.scene_active_seen = None;
                    app.session.inspector = Some(InspectorTarget::Installation(
                        plan_my_cabinet::reference_fixture::HINGE_IDS[0],
                    ));
                    if let Some(joint) = app.editor.project().door_joints.iter().find(|joint| {
                        joint
                            .hinge_installation_ids
                            .contains(&plan_my_cabinet::reference_fixture::HINGE_IDS[0])
                    }) {
                        let id = joint.id;
                        if let Ok(limit) =
                            plan_my_cabinet::door_joint::opening_limit(app.editor.project(), joint)
                        {
                            let _ = app.invoke(Request::with(A::StartMotion, Target::Door(id)));
                            let _ = app.invoke(
                                Request::with(A::SetDoorAngle, Target::Door(id))
                                    .argument(Argument::Angle(60.0_f64.min(limit))),
                            );
                        }
                    }
                }
                if let Some(mode) = config.snap {
                    app.move_tool.configure_capture_snap(mode);
                }
                if let Some(dialog) = config.dialog {
                    dialog.mount(&mut app);
                }
                if viewport_capture_state.contains("neutral") {
                    app.preferences.material_tint = false;
                }
                if viewport_capture_state.contains("hidden") {
                    app.selection
                        .hidden
                        .insert(plan_my_cabinet::reference_fixture::SHELF_ID);
                }
                if config.welcome_empty {
                    // An empty first-run Welcome must not retain the prepared
                    // cabinet fixture behind its overlay or claim a recent file.
                    app.editor = ProjectEditor::new(Project::new("Cabinet", Currency::Brl))
                        .expect("empty Welcome capture project");
                    app.session = WorkspaceSession::new(app.editor.project());
                    app.selection = viewport::Selection::default();
                    app.project_files.welcome.visible = true;
                    std::fs::write(
                        config.directory.join("welcome-state.json"),
                        "{\"mode\":\"empty\",\"recent_entries\":0}\n",
                    )
                    .expect("isolated Welcome capture directory is writable");
                } else {
                    // Record the effective editor fixture and view overrides.
                    use sha2::Digest;
                    let effective_hash = sha2::Sha256::digest(
                        plan_my_cabinet::persistence::serialize(app.editor.project())
                            .expect("serializable capture fixture"),
                    );
                    std::fs::write(
                        config.directory.join("viewport-state.json"),
                        format!("{{\"variant\":\"{viewport_capture_state}\",\"material_tint\":{},\"hidden_shelf\":{},\"effective_fixture_sha256\":\"{effective_hash:x}\"}}\n",
                            app.preferences.material_tint,
                            app.selection.hidden.contains(&plan_my_cabinet::reference_fixture::SHELF_ID)),
                    ).expect("isolated capture directory is writable");
                }
                app.project_files.user_data_dir = Some(config.directory.join("app-data"));
                app.capture = Some(capture::Capture::new(config, capture_completion));
            } else {
                match platform_config_dir() {
                    Ok(dir) => app.load_preferences(&context.egui_ctx, &dir),
                    Err(error) => app.preferences_error = Some(error.into()),
                }
            }
            Ok(Box::new(app))
        }),
    ) {
        Ok(()) => {
            if capturing {
                match completion.lock().expect("capture result mutex").take() {
                    Some(Ok(())) => {}
                    Some(Err(error)) => {
                        eprintln!("Native capture failed: {error}");
                        return std::process::ExitCode::FAILURE;
                    }
                    None => {
                        eprintln!("Native capture closed before completion");
                        return std::process::ExitCode::FAILURE;
                    }
                }
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!(
                "Unable to start Plan My Cabinet: {error}. Check that a compatible graphics adapter and driver are available (Metal on macOS, Vulkan on Linux)."
            );
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn template_replacement_waits_for_dirty_decision_and_commits_one_undo() {
        use plan_my_cabinet::domain::BoardGrain;
        use plan_my_cabinet::template_setup::{MaterialRole, TemplateKind};
        use plan_my_cabinet::units::{Conversion, Length};

        let mut app = DesktopApp::default();
        app.editor
            .transact(|project| -> Result<(), ()> {
                project.name = "Keep my edits".into();
                Ok(())
            })
            .unwrap();
        let old = app.editor.project().clone();
        let mut setup =
            TemplateSetupUi::new(TemplateKind::Base, "New base", Currency::Brl, Unit::Mm);
        let proposed = |mm: i64| {
            plan_my_cabinet::template_setup::ProposedLength::new(Conversion::Exact(
                Length::from_micrometres(mm * 1_000),
            ))
        };
        let carcass =
            setup
                .setup
                .add_material("Cabinet", proposed(18), BoardGrain::Unrestricted, None);
        let back = setup
            .setup
            .add_material("Back", proposed(6), BoardGrain::Unrestricted, None);
        setup.setup.roles.insert(MaterialRole::Carcass, carcass);
        setup.setup.roles.insert(MaterialRole::Back, back);
        assert!(setup.setup.review().is_ok());
        app.template_setup = Some(setup);
        app.template_guard_pending = true;
        app.request_project_action(project_ui::NextAction::Template);
        assert!(matches!(
            app.project_files.prompt,
            Some(project_ui::Prompt::Dirty(project_ui::NextAction::Template))
        ));
        assert_eq!(app.editor.project(), &old);
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("cancel"));
        assert!(!app.template_guard_pending);
        assert!(app.template_setup.is_some());
        assert_eq!(app.editor.project(), &old);

        app.template_guard_pending = true;
        app.request_project_action(project_ui::NextAction::Template);
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("project-discard"));
        assert!(!app.project_files.welcome.visible);
        assert!(app.template_setup.is_none());
        assert_eq!(app.editor.project().name, "New base");
        assert!(app.project_files.path.is_none());
        assert!(app.editor.is_dirty());
        assert_eq!(app.session.active, Workspace::Design);
        assert!(
            app.editor
                .project()
                .assemblies
                .iter()
                .any(|assembly| Some(assembly.id) == app.selection.active)
        );
        assert!(!app.editor.project().boards.is_empty());
        app.editor.undo().unwrap();
        assert!(app.editor.project().materials.is_empty());
        assert!(app.editor.project().boards.is_empty());
        assert!(app.editor.project().assemblies.is_empty());
    }

    #[test]
    fn first_run_drawers_tile_generates_editable_unsaved_project_and_stock_route() {
        use plan_my_cabinet::domain::BoardGrain;
        use plan_my_cabinet::template_setup::{MaterialRole, TemplateKind};
        use plan_my_cabinet::units::{Conversion, Length};
        let mut app = DesktopApp::default();
        let local_dir =
            std::env::temp_dir().join(format!("pmcab-template-recents-{}", Uuid::new_v4()));
        app.project_files.user_data_dir = Some(local_dir.clone());
        assert!(app.project_files.welcome.visible);
        app.handle_welcome_intent(plan_my_cabinet::welcome_ui::WelcomeIntent::Template(
            TemplateKind::Drawers,
        ));
        let setup = &mut app.template_setup.as_mut().unwrap().setup;
        setup.project_name = "Three drawers".into();
        let value = |mm: i64| {
            plan_my_cabinet::template_setup::ProposedLength::new(Conversion::Exact(
                Length::from_micrometres(mm * 1_000),
            ))
        };
        let carcass = setup.add_material("Carcass", value(18), BoardGrain::Length, None);
        let back = setup.add_material("Back / bottom", value(6), BoardGrain::Unrestricted, None);
        let box_id = setup.add_material("Box", value(12), BoardGrain::Length, None);
        let front = setup.add_material("Front", value(19), BoardGrain::Length, None);
        for (role, id) in [
            (MaterialRole::Carcass, carcass),
            (MaterialRole::Back, back),
            (MaterialRole::Box, box_id),
            (MaterialRole::BoxBottom, back),
            (MaterialRole::ExternalFront, front),
        ] {
            setup.roles.insert(role, id);
        }
        assert!(setup.review().is_ok());
        app.template_guard_pending = true;
        app.request_project_action(project_ui::NextAction::Template);
        assert!(!app.project_files.welcome.visible);
        assert_eq!(app.editor.project().boards.len(), 23);
        assert_eq!(app.editor.project().materials.len(), 4);
        assert!(app.editor.project().stock.is_empty());
        assert!(app.editor.project().allocations.is_empty());
        assert!(app.template_message.is_some());
        assert_eq!(app.session.active, Workspace::Design);
        assert!(
            app.editor
                .project()
                .assemblies
                .iter()
                .any(|a| Some(a.id) == app.selection.active)
        );
        assert!(app.project_files.path.is_none());
        assert!(app.editor.is_dirty());
        assert!(
            plan_my_cabinet::recent_projects::RecentProjects::open(&local_dir)
                .unwrap()
                .list("")
                .is_empty()
        );
        let before = app.editor.project().clone();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let size = egui::vec2(1100.0, 720.0);
        for workspace in [Workspace::Design, Workspace::Hardware, Workspace::Design] {
            app.session.switch(workspace);
            for _ in 0..3 {
                responsive_frame(&mut app, &ctx, size, vec![]);
            }
            assert!(app.session.belongs_to(app.editor.project()));
            let target = app
                .session
                .view_mut(workspace)
                .camera
                .framed_target()
                .unwrap();
            assert!((target[0] - 300.0).abs() < 0.001, "{target:?}");
            assert!((target[2] - 360.0).abs() < 0.001, "{target:?}");
            app.session
                .view_mut(workspace)
                .camera
                .assert_framed_occupancy(app.editor.project(), &app.selection);
        }
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn settings_actions_isolate_the_scene_and_preserve_unit_edit_history() {
        let mut app = DesktopApp::default();
        let ctx = egui::Context::default();
        let before = app.editor.project().clone();
        app.invoke(Request::new(A::OpenSettings)).unwrap();
        assert!(app.settings_open);
        assert_eq!(
            app.action_availability(Request::new(A::NewBoard)),
            Err(actions::Unavailable::ModalOpen)
        );
        app.apply_settings_intent(&ctx, SettingsIntent::SetDisplayUnit(Unit::Foot));
        assert_eq!(app.editor.project().display_unit, Unit::Foot);
        assert_eq!(app.editor.project().revision, before.revision);
        assert!(!app.editor.can_undo());
        app.apply_settings_intent(&ctx, SettingsIntent::SetFreeCutFee);
        assert_eq!(app.editor.project().cut_fee.unwrap().minor_units(), 0);
        assert!(app.editor.can_undo());
        app.apply_settings_intent(&ctx, SettingsIntent::EditGrid);
        assert!(!app.settings_open);
        assert!(app.settings_resume_after_dialog);
        assert!(app.grid_dialog.is_some());
        app.grid_dialog = None;
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            ..Default::default()
        });
        app.show_settings(&ctx);
        ctx.end_pass().drop_without_applying_deltas();
        assert!(app.settings_open);
        assert!(!app.settings_resume_after_dialog);
        app.apply_settings_intent(&ctx, SettingsIntent::Done);
        assert!(!app.settings_open);

        app.invoke(Request::new(A::OpenSettings)).unwrap();
        app.apply_settings_intent(&ctx, SettingsIntent::ChangeCurrency);
        assert!(!app.settings_open);
        assert!(app.currency_dialog.is_some());
        app.currency_dialog = None;
        app.apply_settings_intent(&ctx, SettingsIntent::WorkedExamples);
        assert!(app.settings_worked_examples);
        assert!(app.other_modal_open());
    }

    #[test]
    fn portuguese_status_uses_overflow_before_labels_can_collide() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::install_fonts(&ctx);
        let mut app = DesktopApp::default();
        app.localizer.set_language(Language::PtBr);
        let issues = workspace_shell::IssueCounts {
            unallocated: 22,
            ..Default::default()
        };
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(560.0, 700.0),
                )),
                ..Default::default()
            },
            |ui| {
                assert!(app.shell_status_required_width(ui, issues, None, false) > 560.0);
                app.show_shell_status(ui, issues, None, false);
            },
        );
        output.textures_delta.clear();
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        assert!(nodes.iter().any(|(_, node)| node.label() == Some("Mais")));
        assert!(!nodes.iter().any(|(_, node)| {
            node.value()
                .is_some_and(|text| text.contains("Nova despesa estimada"))
        }));
    }

    #[test]
    fn hardware_without_installations_does_not_claim_a_deleted_selection() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::install_fonts(&ctx);
        let mut app = DesktopApp::default();
        app.session.active = Workspace::Hardware;
        for language in [Language::En, Language::PtBr] {
            app.localizer.set_language(language);
            let mut output = ctx.run_ui(Default::default(), |ui| app.show_workspace_inspector(ui));
            output.textures_delta.clear();
            let nodes = &output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes;
            assert!(nodes.iter().any(|(_, node)| node.value()
                == Some(app.localizer.text("hardware-no-installations").as_str())));
            assert!(!nodes.iter().any(|(_, node)| node.value()
                == Some(app.localizer.text("hinge-selection-missing").as_str())));
        }
    }

    #[test]
    fn handoff_count_cards_keep_localized_words_readable_in_narrow_panes() {
        for language in [Language::En, Language::PtBr] {
            for width in [180.0, 240.0, 288.0, 320.0] {
                let ctx = egui::Context::default();
                theme::install_fonts(&ctx);
                let mut app = navigation_app();
                app.localizer.set_language(language);
                let before = app.editor.project().clone();
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 600.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.show_export_counts(ui),
                );
                for key in [
                    "handoff-packet-parts",
                    "handoff-packet-sheets",
                    "handoff-packet-hinges",
                ] {
                    let label = app.localizer.text(key);
                    let (clip, text) = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => {
                                Some((shape.clip_rect, text))
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("missing {label}"));
                    assert!(
                        !text.galley.job.justify,
                        "count labels must not stretch letter spacing"
                    );
                    assert!(
                        text.galley.rows.len() <= label.split_whitespace().count(),
                        "{language:?} at {width}: word broken in {label}: {:?}",
                        text.galley.rows
                    );
                    let bounds = text.galley.rect.translate(text.pos.to_vec2());
                    assert!(
                        clip.expand(1.0).contains_rect(bounds),
                        "{label}: {bounds:?} outside {clip:?}"
                    );
                    assert!(bounds.right() <= width + 1.0);
                }
                assert_eq!(app.editor.project(), &before);
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn cut_plan_inspector_wraps_optimizer_heading_inside_scroll_viewport() {
        for language in [Language::En, Language::PtBr] {
            for width in [240.0, 292.0, 320.0] {
                let ctx = egui::Context::default();
                theme::install_fonts(&ctx);
                let mut app = navigation_app();
                app.localizer.set_language(language);
                app.session.active = Workspace::CutPlan;
                app.session.focused_sheet =
                    Some(plan_my_cabinet::reference_fixture::WHITE_STOCK_ID);
                let before = app.editor.project().clone();
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.show_workspace(ui),
                )
                .drop_without_applying_deltas();
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 3000.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.show_scrolled_workspace_inspector(ui, width, 3000.0),
                );
                let label = app.localizer.text("optimize-heading");
                let (clip, text) = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some((shape.clip_rect, text))
                        }
                        _ => None,
                    })
                    .expect("optimizer heading must remain visible");
                let bounds = text.galley.rect.translate(text.pos.to_vec2());
                assert!(
                    clip.expand(1.0).contains_rect(bounds),
                    "{language:?} at {width}: {bounds:?} outside {clip:?}"
                );
                assert!(bounds.right() <= width + 1.0);
                assert_eq!(app.editor.project(), &before);
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn settings_recovery_cleanup_opens_unselected_review_without_editing_project() {
        let root = TempConfig::new();
        let mut app = DesktopApp::default();
        app.project_files.user_data_dir = Some(root.0.clone());
        let before = app.editor.project().clone();
        app.invoke(Request::new(A::OpenSettings)).unwrap();
        app.apply_settings_intent(
            &egui::Context::default(),
            SettingsIntent::ReviewRecoveryCleanup,
        );
        assert!(!app.settings_open);
        assert_eq!(
            app.settings_cleanup
                .as_ref()
                .unwrap()
                .review()
                .selected()
                .count(),
            0
        );
        assert_eq!(
            app.action_availability(Request::new(A::NewBoard)),
            Err(actions::Unavailable::ModalOpen)
        );
        assert_eq!(app.editor.project(), &before);
        app.settings_cleanup = None;
        app.settings_open = true;
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn navigation_hint_preference_controls_contextual_status_without_editing() {
        let mut app = DesktopApp::default();
        let ctx = egui::Context::default();
        let original = app.editor.project().clone();
        let render = |app: &mut DesktopApp| {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                app.show_shell_status_contents(
                    ui,
                    workspace_shell::IssueCounts::default(),
                    None,
                    false,
                );
            });
            let text = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            output.drop_without_applying_deltas();
            text
        };
        // Hints only appear for tools whose gestures need explaining.
        assert!(!render(&mut app).contains("Drag selected board"));
        app.move_tool.mode = viewport::ToolMode::Move;
        assert!(render(&mut app).contains("Drag selected board"));
        app.set_navigation_hints(false);
        assert!(!render(&mut app).contains("Drag selected board"));
        assert_eq!(app.editor.project(), &original);
        assert!(!app.editor.can_undo());
    }

    #[test]
    fn welcome_preferences_work_without_document_at_all_supported_scales() {
        use plan_my_cabinet::welcome_ui::WelcomeIntent;
        let mut app = DesktopApp::default();
        let old = app.editor.project().clone();
        app.localizer.set_language(Language::PtBr);
        app.handle_welcome_intent(WelcomeIntent::Preferences);
        assert!(app.settings_open && app.project_files.welcome.visible);
        app.settings_state.section = SettingsSection::General;
        let ctx = egui::Context::default();
        for scale in [
            InterfaceScale::Percent90,
            InterfaceScale::Percent100,
            InterfaceScale::Percent115,
            InterfaceScale::Percent130,
        ] {
            app.apply_settings_intent(&ctx, SettingsIntent::SetScale(scale));
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(780.0, 560.0) / scale.factor(),
                    )),
                    ..Default::default()
                },
                |ui| app.show_settings(ui.ctx()),
            );
            assert!(!output.shapes.is_empty());
            output.drop_without_applying_deltas();
            assert_eq!(ctx.zoom_factor(), scale.factor());
            assert_eq!(app.editor.project(), &old);
        }
        app.apply_settings_intent(&ctx, SettingsIntent::Done);
        assert!(!app.settings_open);
        assert!(app.project_files.welcome.visible);
    }

    struct TempConfig(PathBuf);

    impl TempConfig {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("pmcab-native-prefs-{}", Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }

    impl Drop for TempConfig {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn native_startup_restores_preferences_and_language_action_saves_without_project_edit() {
        let root = TempConfig::new();
        let config = root.0.join("config");
        let ctx = egui::Context::default();
        let mut app = DesktopApp::default();
        app.load_preferences(&ctx, &config);
        let before = plan_my_cabinet::persistence::serialize(app.editor.project()).unwrap();
        let revision = app.editor.project().revision;
        app.set_navigation_hints(false);
        app.set_inverse_scroll_zoom(true);
        app.set_material_tint(false);
        app.set_interface_scale(&ctx, InterfaceScale::Percent115);
        app.invoke(Request::new(A::SetUiLanguage).argument(Argument::Language(Language::PtBr)))
            .unwrap();
        assert!(app.preferences_error.is_none());
        assert_eq!(app.localizer.language(), Language::PtBr);
        assert_eq!(app.editor.project().revision, revision);
        assert!(!app.editor.is_dirty());
        assert!(!app.editor.can_undo());
        assert_eq!(
            plan_my_cabinet::persistence::serialize(app.editor.project()).unwrap(),
            before
        );

        let mut restarted = DesktopApp::default();
        let restart_ctx = egui::Context::default();
        restarted.load_preferences(&restart_ctx, &config);
        assert_eq!(restarted.preferences.language, Language::PtBr);
        assert_eq!(restarted.localizer.language(), Language::PtBr);
        assert_eq!(
            restarted.preferences.interface_scale,
            InterfaceScale::Percent115
        );
        restart_ctx
            .run_ui(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        assert!((restart_ctx.zoom_factor() - 1.15).abs() < f32::EPSILON);
        assert!(!restarted.preferences.navigation_hints);
        assert!(restarted.preferences.inverse_scroll_zoom);
        assert!(!restarted.preferences.material_tint);
        assert!(restarted.preferences_error.is_none());
        assert!(!restarted.editor.is_dirty());
    }

    #[test]
    fn malformed_config_and_write_failure_are_visible_without_project_revision() {
        let root = TempConfig::new();
        let store = PreferencesStore::new(&root.0).unwrap();
        fs::write(store.path(), b"{invalid").unwrap();
        let mut app = DesktopApp::default();
        let ctx = egui::Context::default();
        app.load_preferences(&ctx, &root.0);
        assert_eq!(app.preferences, LocalPreferences::default());
        assert!(app.preferences_error.is_some());
        assert_eq!(fs::read(store.path()).unwrap(), b"{invalid");

        fs::remove_file(store.path()).unwrap();
        fs::create_dir(store.path()).unwrap();
        let revision = app.editor.project().revision;
        app.invoke(Request::new(A::SetUiLanguage).argument(Argument::Language(Language::PtBr)))
            .unwrap();
        assert!(
            app.preferences_error
                .as_ref()
                .is_some_and(|error| error.contains("save"))
        );
        assert_eq!(app.localizer.language(), Language::PtBr);
        assert_eq!(app.editor.project().revision, revision);
        assert!(!app.editor.can_undo());
        assert!(!app.editor.is_dirty());
    }

    #[test]
    fn capture_preference_override_never_uses_the_platform_store() {
        let root = TempConfig::new();
        let mut app = DesktopApp {
            capture: Some(capture::Capture::new(
                capture::Config {
                    snap: None,
                    dialog: None,
                    directory: root.0.join("capture"),
                    gallery: false,
                    size: [1440, 900],
                    scale: 130,
                    language: Language::PtBr,
                    workspace: Workspace::Design,
                    welcome_empty: false,
                    settings: None,
                    page: None,
                },
                Default::default(),
            )),
            ..Default::default()
        };
        app.localizer.set_language(Language::PtBr);
        app.preferences.interface_scale = InterfaceScale::Percent130;
        assert!(app.preferences_store.is_none());
        assert!(!root.0.join("preferences.json").exists());
    }

    #[test]
    fn empty_welcome_capture_uses_only_isolated_recents_and_no_fixture_board() {
        let root = TempConfig::new();
        let config = capture::Config {
            directory: root.0.join("welcome-capture"),
            gallery: false,
            size: [1100, 700],
            scale: 100,
            language: Language::En,
            workspace: Workspace::Design,
            welcome_empty: true,
            settings: None,
            page: None,
            snap: None,
            dialog: None,
        };
        config.prepare_directory().unwrap();
        let mut app = DesktopApp::default();
        app.project_files.user_data_dir = Some(config.directory.join("app-data"));
        app.capture = Some(capture::Capture::new(config, Default::default()));
        assert!(app.project_files.welcome.visible);
        assert!(app.editor.project().boards.is_empty());
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 700.0),
                )),
                ..Default::default()
            },
            |ui| app.show_welcome(ui),
        );
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            labels.contains(&app.localizer.text("welcome-empty")),
            "{labels}"
        );
        assert!(
            labels.contains(&app.localizer.text("welcome-templates")),
            "{labels}"
        );
        assert!(!labels.contains("Redesign reference"));
        output.drop_without_applying_deltas();
        assert!(app.editor.project().boards.is_empty());
        assert!(app.editor.project().export_records.is_empty());
    }

    #[test]
    fn export_completion_distinguishes_edit_revision_from_output_freshness() {
        assert_eq!(export_completion_key(ExportStatus::Current), "export-saved");
        assert_eq!(
            export_completion_key(ExportStatus::WoodStale),
            "export-saved-stale"
        );
        assert_eq!(
            export_completion_key(ExportStatus::PacketStale),
            "export-saved-stale"
        );
        assert_eq!(
            export_completion_key(ExportStatus::Unknown),
            "export-saved-unknown"
        );
        for language in [Language::En, Language::PtBr] {
            let loc = Localizer::new(language);
            let mut args = FluentArgs::new();
            args.set("path", "cabinet.pdf");
            args.set("revision", 7);
            for status in [
                ExportStatus::Current,
                ExportStatus::WoodStale,
                ExportStatus::Unknown,
            ] {
                let label = loc.format(export_completion_key(status), Some(&args));
                assert!(label.contains("cabinet.pdf"), "{label}");
                assert!(!label.contains("export-saved-"), "{label}");
            }
        }
    }

    fn navigation_app() -> DesktopApp {
        let mut app = DesktopApp {
            editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
            ..Default::default()
        };
        app.sync_scene_inspector();
        app
    }

    #[test]
    fn dimension_field_navigation_rejects_invalid_apply_and_preserves_stay() {
        let mut app = navigation_app();
        let first = app.editor.project().boards[0].id;
        let second = app.editor.project().boards[1].id;
        app.invoke(Request::with(A::EditDimensions, Target::Board(first)))
            .unwrap();
        app.board_dimension.as_mut().unwrap().value.text = "invalid".into();
        let original = app.editor.project().clone();
        assert_eq!(
            app.request_navigation(NavigationRoute::Entity(Destination::Board(second))),
            Outcome::Prompt {
                kind: EditKind::Field,
                can_commit: false
            }
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Commit),
            Outcome::Blocked(pending_navigation::Blocked::InvalidCommit)
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Stay),
            Outcome::Stayed
        );
        assert_eq!(app.board_dimension.as_ref().unwrap().value.text, "invalid");
        assert_eq!(app.selection.active, None);
        assert_eq!(app.editor.project(), &original);
        assert!(matches!(
            app.request_navigation(NavigationRoute::Entity(Destination::Board(second))),
            Outcome::Prompt { .. }
        ));
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Abandon),
            Outcome::Navigated
        );
        assert!(app.board_dimension.is_none());
        assert_eq!(app.selection.active, Some(second));
        assert_eq!(app.editor.project(), &original);
    }

    #[test]
    fn dimension_field_apply_routes_once_after_commit() {
        let mut app = navigation_app();
        let first = app.editor.project().boards[0].id;
        let second = app.editor.project().boards[1].id;
        let before = app.editor.project().revision;
        let old = app.editor.project().boards[0].length.micrometres();
        app.invoke(Request::with(A::EditDimensions, Target::Board(first)))
            .unwrap();
        app.board_dimension.as_mut().unwrap().value.text = format_length(
            Length::from_micrometres(old + 10_000),
            Unit::Mm,
            Locale::En,
            3,
        );
        assert_eq!(
            app.request_navigation(NavigationRoute::Entity(Destination::Board(second))),
            Outcome::Prompt {
                kind: EditKind::Field,
                can_commit: true
            }
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Commit),
            Outcome::Navigated
        );
        assert_eq!(app.editor.project().revision, before + 1);
        assert_eq!(
            app.editor.project().boards[0].length.micrometres(),
            old + 10_000
        );
        assert_eq!(app.selection.active, Some(second));
        assert!(app.board_dimension.is_none());
    }

    #[test]
    fn shared_board_draft_guards_outliner_style_selection_and_applies_once() {
        let mut app = navigation_app();
        let first = app.editor.project().boards[0].id;
        let second = app.editor.project().boards[1].id;
        assert_eq!(
            app.request_scene_selection(Some(first), false),
            Outcome::Navigated
        );
        let original = app.editor.project().boards[0].length;
        let before = app.editor.project().revision;
        app.edit_drafts
            .board(&app.editor, first, Unit::Mm, Locale::En)
            .unwrap()
            .length
            .edit("invalid");
        assert_eq!(
            app.request_scene_selection(Some(second), false),
            Outcome::Prompt {
                kind: EditKind::Field,
                can_commit: false
            }
        );
        assert_eq!(app.selection.active, Some(first));
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Commit),
            Outcome::Blocked(pending_navigation::Blocked::InvalidCommit)
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Stay),
            Outcome::Stayed
        );
        assert_eq!(
            app.edit_drafts
                .existing_board(app.editor.project().id, first)
                .unwrap()
                .length
                .display(),
            "invalid"
        );
        app.edit_drafts
            .board(&app.editor, first, Unit::Mm, Locale::En)
            .unwrap()
            .length
            .edit("500 mm");
        assert_eq!(
            app.request_scene_selection(Some(second), false),
            Outcome::Prompt {
                kind: EditKind::Field,
                can_commit: true
            }
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Commit),
            Outcome::Navigated
        );
        assert_eq!(app.selection.active, Some(second));
        assert_eq!(app.session.inspector, Some(InspectorTarget::Board(second)));
        assert_eq!(app.editor.project().revision, before + 1);
        assert_eq!(app.editor.project().boards[0].length.micrometres(), 500_000);
        assert!(
            app.edit_drafts
                .existing_board(app.editor.project().id, first)
                .is_none()
        );
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().boards[0].length, original);
    }

    #[test]
    fn shared_draft_guards_deselection_and_discard_clears_it() {
        let mut app = navigation_app();
        let board = app.editor.project().boards[0].id;
        app.request_scene_selection(Some(board), false);
        app.edit_drafts
            .board(&app.editor, board, Unit::Inch, Locale::En)
            .unwrap()
            .length
            .edit("1/64");
        assert_eq!(
            app.request_scene_selection(None, false),
            Outcome::Prompt {
                kind: EditKind::Field,
                can_commit: false
            }
        );
        assert_eq!(app.selection.active, Some(board));
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Abandon),
            Outcome::Navigated
        );
        assert_eq!(app.selection.active, None);
        assert!(
            app.edit_drafts
                .existing_board(app.editor.project().id, board)
                .is_none()
        );
    }

    #[test]
    fn incompatible_action_waits_for_draft_and_stay_keeps_both_unchanged() {
        let mut app = navigation_app();
        let board = app.editor.project().boards[0].id;
        app.request_scene_selection(Some(board), false);
        app.edit_drafts
            .board(&app.editor, board, Unit::Mm, Locale::En)
            .unwrap()
            .length
            .edit("bad");
        let before = app.editor.project().clone();
        let request = Request::with(A::SetGrain, Target::Board(board))
            .argument(Argument::Grain(Some(BoardGrain::Width)));
        app.invoke(request).unwrap();
        assert!(app.navigation.pending().is_some());
        assert_eq!(app.editor.project(), &before);
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Stay),
            Outcome::Stayed
        );
        assert_eq!(app.editor.project(), &before);
        assert_eq!(
            app.edit_drafts
                .existing_board(before.id, board)
                .unwrap()
                .length
                .display(),
            "bad"
        );
        app.invoke(request).unwrap();
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Abandon),
            Outcome::Navigated
        );
        assert_eq!(
            app.editor.project().boards[0].grain_override,
            Some(BoardGrain::Width)
        );
        assert!(app.edit_drafts.existing_board(before.id, board).is_none());
    }

    #[test]
    fn inline_pose_draft_guards_selection_and_frame_changes() {
        let mut app = navigation_app();
        let first = app.editor.project().boards[0].id;
        let second = app.editor.project().boards[1].id;
        app.request_scene_selection(Some(first), false);
        app.edit_drafts
            .pose(
                &app.editor,
                first,
                CoordinateFrame::LocalParent,
                Unit::Mm,
                Locale::En,
            )
            .unwrap()
            .position[0]
            .edit("invalid");
        let original = app.editor.project().clone();
        assert_eq!(
            app.request_scene_selection(Some(second), false),
            Outcome::Prompt {
                kind: EditKind::Preview,
                can_commit: false
            }
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Commit),
            Outcome::Blocked(pending_navigation::Blocked::InvalidCommit)
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Stay),
            Outcome::Stayed
        );
        assert_eq!(app.selection.active, Some(first));
        app.request_pose_frame(CoordinateFrame::World);
        assert_eq!(app.pose_frame, CoordinateFrame::LocalParent);
        assert!(app.navigation.pending().is_some());
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Abandon),
            Outcome::Navigated
        );
        assert_eq!(app.pose_frame, CoordinateFrame::World);
        assert!(app.edit_drafts.existing_pose(original.id, first).is_none());
        assert_eq!(app.editor.project(), &original);
    }

    #[test]
    fn inline_pose_accepts_one_valid_edit_and_preserves_unedited_rotation() {
        let mut app = navigation_app();
        let first = app.editor.project().boards[0].id;
        let second = app.editor.project().boards[1].id;
        app.request_scene_selection(Some(first), false);
        let original = app.editor.project().boards[0].pose;
        let revision = app.editor.project().revision;
        app.edit_drafts
            .pose(
                &app.editor,
                first,
                CoordinateFrame::LocalParent,
                Unit::Mm,
                Locale::En,
            )
            .unwrap()
            .position[0]
            .edit("30 mm");
        assert_eq!(
            app.request_scene_selection(Some(second), false),
            Outcome::Prompt {
                kind: EditKind::Preview,
                can_commit: true
            }
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Commit),
            Outcome::Navigated
        );
        let pose = app.editor.project().boards[0].pose;
        assert_eq!(pose.translation_mm[0], 30.0);
        assert_eq!(pose.rotation, original.rotation);
        assert_eq!(app.editor.project().revision, revision + 1);
        assert!(
            app.edit_drafts
                .existing_pose(app.editor.project().id, first)
                .is_none()
        );
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().boards[0].pose, original);
    }

    #[test]
    fn repair_navigation_requires_a_valid_accept_or_explicit_cancel() {
        let mut app = navigation_app();
        let board = app.editor.project().allocations[0].board_id;
        let stock = app.editor.project().allocations[0].stock_id;
        app.session.switch(Workspace::CutPlan);
        app.session.focused_sheet = Some(stock);
        app.selection.choose(Some(board), false);
        let original = app.editor.project().clone();
        assert!(
            app.sheet_repair
                .begin(&mut app.editor, &app.selection, Locale::En)
        );
        app.sheet_repair.stage_placement(
            &mut app.editor,
            board,
            stock,
            [Length::from_micrometres(999_000_000); 2],
            false,
        );
        assert_eq!(
            app.request_navigation(NavigationRoute::Workspace(Workspace::Design)),
            Outcome::Prompt {
                kind: EditKind::Preview,
                can_commit: false
            }
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Commit),
            Outcome::Blocked(pending_navigation::Blocked::InvalidCommit)
        );
        assert_eq!(app.session.active, Workspace::CutPlan);
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Stay),
            Outcome::Stayed
        );
        assert_eq!(app.session.active, Workspace::CutPlan);
        assert!(app.sheet_repair.active());
        assert_eq!(app.editor.project(), &original);
        assert!(matches!(
            app.request_navigation(NavigationRoute::Workspace(Workspace::Design)),
            Outcome::Prompt { .. }
        ));
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Abandon),
            Outcome::Navigated
        );
        assert_eq!(app.editor.project(), &original);
        assert!(app.editor.preview().is_none());
    }

    #[test]
    fn navigation_prompt_footer_blocks_invalid_enter_and_escape_stays() {
        let mut app = navigation_app();
        app.localizer.set_language(Language::En);
        let board = app.editor.project().boards[0].id;
        app.request_scene_selection(Some(board), false);
        app.edit_drafts
            .board(&app.editor, board, Unit::Mm, Locale::En)
            .unwrap()
            .length
            .edit("invalid");
        let before = app.editor.project().clone();
        assert!(matches!(
            app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
            Outcome::Prompt {
                kind: EditKind::Field,
                can_commit: false
            }
        ));
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::install_fonts(&ctx);
        let frame = |app: &mut DesktopApp, events| {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.show_navigation_prompt(ui.ctx()),
            );
            let labels = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, node)| {
                    (node.role() == egui::accesskit::Role::Button)
                        .then(|| {
                            node.label()
                                .map(|label| (label.to_owned(), node.is_disabled()))
                        })
                        .flatten()
                })
                .collect::<Vec<_>>();
            output.drop_without_applying_deltas();
            labels
        };
        let labels = frame(&mut app, vec![]);
        assert!(
            labels
                .iter()
                .any(|(label, _)| label == &app.localizer.text("navigation-apply"))
        );
        assert!(
            labels
                .iter()
                .any(|(label, _)| label == &app.localizer.text("navigation-discard"))
        );
        assert!(
            labels
                .iter()
                .any(|(label, _)| label == &app.localizer.text("navigation-stay"))
        );
        frame(
            &mut app,
            vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        // Enter may activate the focused Stay button. It must never apply an
        // invalid draft or reach the requested destination.
        assert_eq!(app.session.active, Workspace::Design);
        assert_eq!(app.editor.project(), &before);
        if app.navigation.pending().is_none() {
            assert!(matches!(
                app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
                Outcome::Prompt { .. }
            ));
            frame(&mut app, vec![]);
        }
        frame(
            &mut app,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(app.navigation.pending().is_none());
        assert_eq!(app.session.active, Workspace::Design);
        assert_eq!(app.editor.project(), &before);
        assert_eq!(
            app.edit_drafts
                .existing_board(before.id, board)
                .unwrap()
                .length
                .display(),
            "invalid"
        );
        assert!(!app.navigation_chrome.is_active());
    }

    #[test]
    fn valid_repair_navigation_accepts_transfer_once_and_undo_restores_allocation() {
        let mut project = plan_my_cabinet::reference_fixture::project();
        let original_allocation = project.allocations[0].clone();
        let mut spare = project
            .stock
            .iter()
            .find(|piece| piece.id == original_allocation.stock_id)
            .unwrap()
            .clone();
        spare.id = Uuid::new_v4();
        spare.name = "Uncut spare".into();
        spare.priority = u32::try_from(project.stock.len()).unwrap();
        let spare_id = spare.id;
        match spare.source {
            plan_my_cabinet::domain::StockSource::Owned => {
                project
                    .stock_aliases
                    .insert(spare_id, format!("O{}", project.next_stock_o_alias));
                project.next_stock_o_alias += 1;
            }
            plan_my_cabinet::domain::StockSource::ToPurchase => {
                project
                    .stock_aliases
                    .insert(spare_id, format!("S{}", project.next_stock_s_alias));
                project.next_stock_s_alias += 1;
            }
        }
        project.stock.push(spare);
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        app.session = workspace_state::WorkspaceSession::new(app.editor.project());
        app.session.switch(Workspace::CutPlan);
        app.session.focused_sheet = Some(original_allocation.stock_id);
        app.selection
            .choose(Some(original_allocation.board_id), false);
        let before = app.editor.project().clone();
        assert!(
            app.sheet_repair
                .begin(&mut app.editor, &app.selection, Locale::En)
        );
        assert!(app.sheet_repair.stage_placement(
            &mut app.editor,
            original_allocation.board_id,
            spare_id,
            original_allocation.origin,
            original_allocation.quarter_turn,
        ));
        assert!(app.sheet_repair.can_accept(&mut app.editor));
        assert_eq!(app.editor.project(), &before);
        assert_eq!(
            app.request_navigation(NavigationRoute::Workspace(Workspace::Design)),
            Outcome::Prompt {
                kind: EditKind::Preview,
                can_commit: true,
            }
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Commit),
            Outcome::Navigated
        );
        assert_eq!(app.session.active, Workspace::Design);
        assert!(!app.sheet_repair.active());
        assert!(app.editor.preview().is_none());
        assert_eq!(
            app.editor
                .project()
                .allocations
                .iter()
                .find(|a| a.board_id == original_allocation.board_id)
                .unwrap()
                .stock_id,
            spare_id
        );
        assert_eq!(app.editor.project().revision, before.revision + 1);
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().allocations, before.allocations);
    }

    #[test]
    fn placement_preview_stay_and_cancel_preserve_selection_and_project() {
        let mut app = navigation_app();
        let board = app.editor.project().boards[0].id;
        let sheet = app.editor.project().stock[0].id;
        app.selection.choose(Some(board), false);
        app.placement = PlacementDialog::numeric(&app, board);
        let original = app.editor.project().clone();
        assert_eq!(
            app.request_navigation(NavigationRoute::Entity(Destination::Sheet(sheet))),
            Outcome::Prompt {
                kind: EditKind::Preview,
                can_commit: true
            }
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Stay),
            Outcome::Stayed
        );
        assert_eq!(app.session.active, Workspace::Design);
        assert!(app.placement.is_some());
        assert_eq!(app.selection.active, Some(board));
        assert!(matches!(
            app.request_navigation(NavigationRoute::Entity(Destination::Sheet(sheet))),
            Outcome::Prompt { .. }
        ));
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Abandon),
            Outcome::Navigated
        );
        assert_eq!(app.session.active, Workspace::CutPlan);
        assert_eq!(app.session.focused_sheet, Some(sheet));
        assert_eq!(app.selection.active, Some(board));
        assert_eq!(app.editor.project(), &original);
    }

    #[test]
    fn move_preview_accepts_once_and_does_not_implicitly_apply_on_stay() {
        let mut app = navigation_app();
        let board = app.editor.project().boards[0].id;
        app.selection.choose(Some(board), false);
        let before = app.editor.project().revision;
        let mut pose = plan_my_cabinet::placement::world_pose(app.editor.project(), board).unwrap();
        pose.translation_mm[0] += 10.0;
        let mut preview =
            plan_my_cabinet::placement::PlacementSession::begin(&mut app.editor, board).unwrap();
        preview.preview_free(pose).unwrap();
        preview.pause();
        assert_eq!(
            app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
            Outcome::Prompt {
                kind: EditKind::Preview,
                can_commit: true
            }
        );
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Stay),
            Outcome::Stayed
        );
        assert_eq!(app.editor.project().revision, before);
        assert_eq!(app.session.active, Workspace::Design);
        assert!(app.editor.preview().is_some());
        assert!(matches!(
            app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
            Outcome::Prompt { .. }
        ));
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Commit),
            Outcome::Navigated
        );
        assert_eq!(app.editor.project().revision, before + 1);
        assert_eq!(app.session.active, Workspace::Stock);
        assert_eq!(app.selection.active, Some(board));
        assert!(app.editor.preview().is_none());
    }

    #[test]
    fn hardware_motion_resets_only_on_successful_navigation() {
        let mut app = navigation_app();
        app.session.switch(Workspace::Hardware);
        app.door_motion = Some((Uuid::new_v4(), 20.0));
        let missing = Destination::Sheet(Uuid::new_v4());
        assert_eq!(
            app.request_navigation(NavigationRoute::Entity(missing)),
            Outcome::Blocked(pending_navigation::Blocked::MissingDestination)
        );
        assert!(app.door_motion.is_some());
        assert_eq!(
            app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
            Outcome::Navigated
        );
        assert_eq!(app.door_motion, None);
    }

    #[test]
    fn workspace_routes_retain_project_selection_filter_scroll_camera_without_edits() {
        let mut app = navigation_app();
        let before = app.editor.project().clone();
        let selected = before.boards[0].id;
        app.selection.choose(Some(selected), false);
        app.session.stock.filter = "oak".into();
        app.session.stock.scroll = 92.0;
        app.session.stock.camera = viewport::Camera::reference_baseline();
        for (workspace, _, _) in workspace_shell::ENTRIES {
            assert_eq!(
                app.request_navigation(NavigationRoute::Workspace(workspace)),
                Outcome::Navigated
            );
            assert_eq!(app.session.active, workspace);
            assert_eq!(app.selection.active, Some(selected));
        }
        assert_eq!(
            app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
            Outcome::Navigated
        );
        assert_eq!(app.session.stock.filter, "oak");
        assert_eq!(app.session.stock.scroll, 92.0);
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn collapsing_and_reopening_inspector_retains_invalid_draft_and_view_state() {
        let mut app = navigation_app();
        let board = app.editor.project().boards[0].id;
        app.selection.choose(Some(board), false);
        app.invoke(Request::with(A::EditDimensions, Target::Board(board)))
            .unwrap();
        let draft = app.board_dimension.as_mut().unwrap();
        draft.value.text = "invalid draft".into();
        draft.value.consent = true;
        app.session.design.scroll = 73.0;
        app.controls_horizontal_scroll[0] = 19.0;
        app.inspector_scroll[0] = egui::vec2(12.0, 84.0);
        let before = app.editor.project().clone();
        for width in [1440.0, 1100.0 / 1.15, 900.0 / 1.30, 1440.0] {
            let layout = workspace_shell::PaneLayout::for_width(
                app.session.active,
                width - workspace_shell::RAIL_WIDTH,
            );
            if layout.collapsed(workspace_shell::Drawer::Inspector) {
                app.open_drawer = Some(workspace_shell::Drawer::Inspector);
            }
            assert_eq!(
                app.board_dimension.as_ref().unwrap().value.text,
                "invalid draft"
            );
            assert!(app.board_dimension.as_ref().unwrap().value.consent);
            assert_eq!(app.session.design.scroll, 73.0);
            assert_eq!(app.controls_horizontal_scroll[0], 19.0);
            assert_eq!(app.inspector_scroll[0], egui::vec2(12.0, 84.0));
            assert_eq!(app.selection.active, Some(board));
            assert_eq!(app.editor.project(), &before);
        }
    }

    #[test]
    fn collapsed_inspector_keeps_inline_board_and_pose_drafts() {
        let mut app = navigation_app();
        let board = app.editor.project().boards[0].id;
        app.request_scene_selection(Some(board), false);
        let original = app.editor.project().clone();
        app.edit_drafts
            .board(&app.editor, board, Unit::Inch, Locale::PtBr)
            .unwrap()
            .length
            .edit("1/64");
        app.edit_drafts
            .existing_board_mut(original.id, board)
            .unwrap()
            .length
            .consent = true;
        for width in [1440.0, 900.0 / 1.30, 1440.0] {
            let layout = workspace_shell::PaneLayout::for_width(
                app.session.active,
                width - workspace_shell::RAIL_WIDTH,
            );
            if layout.collapsed(workspace_shell::Drawer::Inspector) {
                app.open_drawer = Some(workspace_shell::Drawer::Inspector);
            } else {
                app.open_drawer = None;
            }
            let draft = app
                .edit_drafts
                .board(&app.editor, board, Unit::Mm, Locale::En)
                .unwrap();
            assert_eq!(draft.length.display(), "1/64");
            assert!(draft.length.consent);
            assert_eq!(draft.values().unwrap()[0], Length::from_micrometres(397));
        }
        assert_eq!(app.editor.project(), &original);
        app.edit_drafts.cancel_board(original.id, board);
        app.edit_drafts
            .pose(
                &app.editor,
                board,
                CoordinateFrame::LocalParent,
                Unit::Mm,
                Locale::En,
            )
            .unwrap()
            .position[0]
            .edit("bad");
        app.open_drawer = Some(workspace_shell::Drawer::Inspector);
        assert!(matches!(
            app.navigation_edit(),
            Some(EditBlock {
                kind: EditKind::Preview,
                can_commit: false,
                ..
            })
        ));
        app.open_drawer = None;
        assert_eq!(
            app.edit_drafts
                .existing_pose(original.id, board)
                .unwrap()
                .position[0]
                .display(),
            "bad"
        );
        assert_eq!(app.editor.project(), &original);
    }

    #[test]
    fn compact_drawers_and_modals_suppress_hud_without_losing_its_draft() {
        let mut app = navigation_app();
        let board = app.editor.project().boards[0].id;
        app.request_scene_selection(Some(board), false);
        let project = app.editor.project().clone();
        app.edit_drafts
            .board(&app.editor, board, Unit::Mm, Locale::En)
            .unwrap()
            .length
            .edit("invalid");
        assert!(app.design_hud_available());
        for width in [900.0 / 1.15, 900.0 / 1.30] {
            let layout = workspace_shell::PaneLayout::for_width(
                Workspace::Design,
                width - workspace_shell::RAIL_WIDTH,
            );
            assert!(layout.collapsed(workspace_shell::Drawer::Inspector));
            for drawer in [
                workspace_shell::Drawer::Inspector,
                workspace_shell::Drawer::Controls,
            ] {
                app.open_drawer = Some(drawer);
                assert!(!app.design_hud_available());
                assert_eq!(
                    app.edit_drafts
                        .existing_board(project.id, board)
                        .unwrap()
                        .length
                        .display(),
                    "invalid"
                );
            }
        }
        app.open_drawer = None;
        app.palette.open = true;
        assert!(!app.design_hud_available());
        app.palette.open = false;
        assert!(app.design_hud_available());
        assert_eq!(app.editor.project(), &project);
    }

    // Exercise real egui pointer events against the same shell used by the
    // desktop, rather than inferring reachability from a static screenshot.
    fn responsive_frame(
        app: &mut DesktopApp,
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<Event>,
    ) -> Vec<(String, egui::Rect)> {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| app.show_workspace(ui),
        );
        let buttons = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                (node.role() == egui::accesskit::Role::Button && !node.is_disabled())
                    .then(|| {
                        let bounds = node.bounds()?;
                        Some((
                            node.label()?.to_owned(),
                            egui::Rect::from_min_max(
                                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                            ),
                        ))
                    })
                    .flatten()
            })
            .collect();
        output.drop_without_applying_deltas();
        buttons
    }

    fn responsive_click(
        app: &mut DesktopApp,
        ctx: &egui::Context,
        size: egui::Vec2,
        pos: egui::Pos2,
    ) {
        responsive_frame(
            app,
            ctx,
            size,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        responsive_frame(
            app,
            ctx,
            size,
            vec![Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
    }

    #[test]
    fn responsive_shell_drawers_menus_and_invalid_draft_survive_all_scales_and_locales() {
        for window in [
            egui::vec2(1440.0, 900.0),
            egui::vec2(1100.0, 700.0),
            egui::vec2(900.0, 650.0),
        ] {
            for scale in [0.90, 1.00, 1.15, 1.30] {
                if window.x == 1440.0 && scale != 1.0 {
                    continue;
                }
                for language in [Language::En, Language::PtBr] {
                    let size = window / scale;
                    let ctx = egui::Context::default();
                    ctx.enable_accesskit();
                    let mut app = navigation_app();
                    app.localizer.set_language(language);
                    let board = app.editor.project().boards[0].id;
                    app.request_scene_selection(Some(board), false);
                    app.edit_drafts
                        .board(&app.editor, board, Unit::Mm, Locale::En)
                        .unwrap()
                        .length
                        .edit("invalid-at-scale");
                    let original = app.editor.project().clone();
                    let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
                    let header_more = workspace_shell::more_label(language);
                    let palette_label = app.localizer.text("palette-title");
                    // Search and Save stay directly reachable in the header at every size.
                    for label in [palette_label.clone(), A::SaveProject.label(&app.localizer)] {
                        let header = buttons
                            .iter()
                            .find(|(name, rect)| {
                                name == &label && rect.center().y < workspace_shell::HEADER_HEIGHT
                            })
                            .unwrap_or_else(|| {
                                panic!("header {label} missing at {window:?}/{scale} {language:?}: {buttons:?}")
                            });
                        assert!(header.1.right() <= size.x && header.1.left() >= 0.0);
                    }
                    if let Some(status) = buttons.iter().find(|(label, rect)| {
                        label == header_more && rect.center().y > size.y - workspace_shell::STATUS_HEIGHT
                    }) {
                        responsive_click(&mut app, &ctx, size, status.1.center());
                        assert!(egui::Popup::is_any_open(&ctx), "status popup did not open");
                        responsive_frame(
                            &mut app,
                            &ctx,
                            size,
                            vec![Event::Key {
                                key: Key::Escape,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            }],
                        );
                    }
                    let layout = workspace_shell::PaneLayout::for_width(
                        Workspace::Design,
                        size.x - workspace_shell::RAIL_WIDTH,
                    );
                    if layout.collapsed(workspace_shell::Drawer::Inspector) {
                        let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
                        let inspector = buttons
                            .iter()
                            .find(|(label, _)| label == workspace_shell::inspector_label(language))
                            .unwrap_or_else(|| panic!("inspector entry missing: {buttons:?}"));
                        responsive_click(&mut app, &ctx, size, inspector.1.center());
                        assert_eq!(app.open_drawer, Some(workspace_shell::Drawer::Inspector));
                        let open = responsive_frame(&mut app, &ctx, size, vec![]);
                        assert!(
                            open.iter().any(
                                |(label, _)| label == &app.localizer.text("navigation-discard")
                            ),
                            "drawer fields not accessible: {open:?}"
                        );
                        responsive_click(&mut app, &ctx, size, inspector.1.center());
                        assert_eq!(app.open_drawer, None);
                    }
                    if layout.collapsed(workspace_shell::Drawer::Controls) {
                        let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
                        let label = app.localizer.text("navigation-design");
                        let controls = buttons
                            .iter()
                            .find(|(name, rect)| {
                                name == &label && rect.center().y < workspace_shell::HEADER_HEIGHT
                            })
                            .unwrap_or_else(|| {
                                panic!("controls drawer entry missing: {buttons:?}")
                            });
                        responsive_click(&mut app, &ctx, size, controls.1.center());
                        assert_eq!(app.open_drawer, Some(workspace_shell::Drawer::Controls));
                        let open = responsive_frame(&mut app, &ctx, size, vec![]);
                        assert!(
                            open.iter()
                                .any(|(name, _)| name == &app.localizer.text("design-add")),
                            "controls drawer has no creation route: {open:?}"
                        );
                        responsive_click(&mut app, &ctx, size, controls.1.center());
                        assert_eq!(app.open_drawer, None);
                    }
                    let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
                    let viewport_more = app.localizer.text("viewport-more");
                    let iso = app.localizer.text("viewport-iso");
                    let in_canvas = |rect: &egui::Rect| {
                        rect.center().y > workspace_shell::HEADER_HEIGHT
                            && rect.center().y < size.y - workspace_shell::STATUS_HEIGHT
                    };
                    if !buttons.iter().any(|(label, rect)| label == &iso && in_canvas(rect)) {
                        let viewport = buttons
                            .iter()
                            .find(|(label, rect)| label == &viewport_more && in_canvas(rect))
                            .unwrap_or_else(|| panic!("viewport camera controls missing: {buttons:?}"));
                        responsive_click(&mut app, &ctx, size, viewport.1.center());
                        let open = responsive_frame(&mut app, &ctx, size, vec![]);
                        assert!(
                            open.iter().any(|(label, _)| label == &iso),
                            "camera presets unreachable: {open:?}"
                        );
                        responsive_frame(
                            &mut app,
                            &ctx,
                            size,
                            vec![Event::Key {
                                key: Key::Escape,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            }],
                        );
                    }
                    let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
                    // The compact HUD now keeps four distinct named icon targets
                    // directly in its strip rather than nesting them in More.
                    for action in [
                        A::PlaceFace,
                        A::DuplicateBoard,
                        A::ToggleVisibility,
                        A::DeleteObject,
                    ] {
                        let label = action.label(&app.localizer);
                        let target = buttons
                            .iter()
                            .find(|(name, rect)| {
                                name == &label
                                    && rect.center().y > size.y * 0.55
                                    && rect.center().y < size.y - workspace_shell::STATUS_HEIGHT
                            })
                            .unwrap_or_else(|| panic!("HUD action {label} missing: {buttons:?}"));
                        assert!(
                            target.1.right() <= size.x
                                && target.1.bottom() < size.y - workspace_shell::STATUS_HEIGHT
                        );
                    }
                    assert_eq!(
                        app.edit_drafts
                            .existing_board(original.id, board)
                            .unwrap()
                            .length
                            .display(),
                        "invalid-at-scale"
                    );
                    assert_eq!(app.editor.project(), &original);
                }
            }
        }
    }

    #[test]
    fn export_preparation_finishes_outside_handoff_without_receipt_or_model_change() {
        let mut app = navigation_app();
        let project = app.editor.project().clone();
        let ctx = egui::Context::default();
        assert_eq!(app.session.active, Workspace::Design);
        for _ in 0..1000 {
            app.tick_export_preparation(&ctx);
            if app.export_candidate.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.export_candidate.is_some());
        assert!(app.export_preparation.is_none());
        assert!(app.export_preparation_pending.is_none());
        assert_eq!(app.editor.project(), &project);
        assert!(app.editor.project().export_records.is_empty());
    }

    fn await_handoff_packet(app: &mut DesktopApp) {
        let ctx = egui::Context::default();
        for _ in 0..2000 {
            app.tick_export_preparation(&ctx);
            if app
                .export_candidate
                .as_ref()
                .is_some_and(|(_, result)| result.is_ok())
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("reviewed document did not prepare");
    }

    fn acknowledge_handoff_packet(app: &mut DesktopApp) {
        await_handoff_packet(app);
        let (key, result) = app.export_candidate.as_ref().unwrap();
        app.export_preparation = Some((key.clone(), Ok(result.as_ref().unwrap().clone())));
    }

    #[test]
    fn overwrite_modal_cancel_and_stale_confirmation_never_drop_a_running_write() {
        let mut app = navigation_app();
        acknowledge_handoff_packet(&mut app);
        let packet = app.current_reviewed_packet().unwrap();
        let before = app.editor.project().clone();
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let render = |app: &mut DesktopApp, events| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.show_export_overwrite(ui.ctx()),
            )
            .drop_without_applying_deltas();
        };
        let path = PathBuf::from("overwrite-modal-test.pdf");
        app.export_activity = Some(ExportActivity::Confirming(path.clone(), packet.clone()));
        render(&mut app, vec![]);
        assert!(app.export_overwrite_chrome.is_active());
        render(
            &mut app,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(app.export_activity.is_none());
        assert!(!app.export_overwrite_chrome.is_active());
        assert_eq!(app.editor.project(), &before);

        app.export_activity = Some(ExportActivity::Confirming(path, packet));
        app.export_sections.parts_and_costs = false;
        render(&mut app, vec![]);
        render(
            &mut app,
            vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(app.export_activity.is_none());
        assert!(app.export_events.is_none());
        assert!(app.current_reviewed_packet().is_none());
        assert_eq!(app.editor.project(), &before);

        app.export_activity = Some(ExportActivity::Writing);
        render(&mut app, vec![]);
        assert!(matches!(app.export_activity, Some(ExportActivity::Writing)));
        assert!(!app.export_overwrite_chrome.is_active());
    }

    #[test]
    fn handoff_issue_fix_pointer_routes_the_actual_unallocated_board_without_editing() {
        let mut app = navigation_app();
        app.session.active = Workspace::Handoff;
        await_handoff_packet(&mut app);
        let before = app.editor.project().clone();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::install_fonts(&ctx);
        let size = egui::vec2(1440.0, 900.0);
        let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
        let button = buttons
            .iter()
            .filter(|(label, rect)| label == "Fix in workspace" && rect.center().y < 800.0)
            .min_by(|a, b| a.1.center().y.total_cmp(&b.1.center().y))
            .unwrap_or_else(|| panic!("Handoff issue fix not reachable: {buttons:?}"));
        responsive_click(&mut app, &ctx, size, button.1.center());
        assert_eq!(app.session.active, Workspace::CutPlan);
        assert_eq!(
            app.session.allocation_issue,
            Some(plan_my_cabinet::reference_fixture::BACK_ID)
        );
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn hardware_installation_route_mounts_projected_guides_only_in_hardware() {
        let mut app = navigation_app();
        let installation = plan_my_cabinet::reference_fixture::HINGE_IDS[0];
        assert!(app.navigate_session(Destination::Installation(installation)));
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let before = app.editor.project().clone();
        let render = |app: &mut DesktopApp| {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| app.show_workspace(ui),
            );
            let labels = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            output.drop_without_applying_deltas();
            labels
        };
        let hardware = render(&mut app);
        assert!(
            hardware.contains("H1") && hardware.contains("H2"),
            "{hardware}"
        );
        app.door_motion = Some((plan_my_cabinet::reference_fixture::LEFT_JOINT_ID, 45.0));
        let _ = render(&mut app); // warm the newly mounted HUD area
        let open = render(&mut app);
        assert!(
            open.contains("45°")
                && open.contains("105°")
                && open.contains("Display only — the saved pose stays closed."),
            "{open}"
        );
        assert!(matches!(
            app.request_navigation(NavigationRoute::Workspace(Workspace::Design)),
            Outcome::Navigated
        ));
        assert!(app.door_motion.is_none());
        let design = render(&mut app);
        assert!(!design.contains("H1") && !design.contains("H2"), "{design}");
        assert!(!design.contains("Display only — the saved pose stays closed."));
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn reference_cut_plan_host_places_sequence_beside_full_width_sheet() {
        let mut app = navigation_app();
        app.session.switch(Workspace::CutPlan);
        app.session.focused_sheet = Some(plan_my_cabinet::reference_fixture::WHITE_STOCK_ID);
        let before = app.editor.project().clone();
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| app.show_workspace(ui),
        );
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.pos)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let sequence = labels
            .iter()
            .find(|(label, _)| label == &app.localizer.text("sheet-cut-sequence"))
            .unwrap_or_else(|| panic!("Cut sequence not in host inspector: {labels:?}"));
        assert!(sequence.1.x > 1100.0, "{sequence:?}");
        let sheet = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill == egui::Color32::from_rgb(233, 221, 195)
                        && rect.rect.width() > 500.0 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("focused sheet surface must be painted");
        assert!(sheet.top() < 320.0, "sheet starts too low: {sheet:?}");
        assert!(
            sheet.bottom() < 860.0,
            "sheet clips under status: {sheet:?}"
        );
        assert!(sheet.height() > 300.0, "sheet is too small: {sheet:?}");
        assert!(labels.iter().any(|(label, pos)| label
            == &app.localizer.text("sheet-needs-stock")
            && pos.x < 340.0));
        assert!(
            labels
                .iter()
                .any(|(label, pos)| label == "1" && pos.x < 1100.0)
        );
        assert_eq!(app.editor.project(), &before);
        output.drop_without_applying_deltas();
    }

    #[test]
    fn handoff_refresh_requires_explicit_review_after_controls_change() {
        let mut app = navigation_app();
        acknowledge_handoff_packet(&mut app);
        let old = app.current_reviewed_packet().unwrap();
        let page_count = old.document().pages.len();
        app.export_preview.set_zoom(5.0).unwrap();
        assert_eq!(old.document().pages.len(), page_count);
        app.export_units = Unit::Foot;
        app.tick_export_preparation(&egui::Context::default());
        assert!(app.current_reviewed_packet().is_none());
        await_handoff_packet(&mut app);
        assert!(app.current_reviewed_packet().is_none());
        assert_eq!(
            app.export_candidate
                .as_ref()
                .unwrap()
                .1
                .as_ref()
                .unwrap()
                .key()
                .settings
                .units,
            Unit::Foot
        );
        acknowledge_handoff_packet(&mut app);
        assert!(app.current_reviewed_packet().is_some());
        app.export_sections.hinge_references = false;
        app.tick_export_preparation(&egui::Context::default());
        assert!(app.current_reviewed_packet().is_none());
        await_handoff_packet(&mut app);
        assert!(app.current_reviewed_packet().is_none());
    }

    #[test]
    fn palette_export_requires_the_exact_reviewed_sections_and_source() {
        let mut app = navigation_app();
        acknowledge_handoff_packet(&mut app);
        assert!(app.action_availability(Request::new(A::ExportPdf)).is_ok());
        let original_sections = app.export_sections;
        app.export_sections.parts_and_costs = false;
        assert_eq!(
            app.action_availability(Request::new(A::ExportPdf)),
            Err(actions::Unavailable::ExportNotReady)
        );
        assert_eq!(
            app.invoke(Request::new(A::ExportPdf)),
            Err(actions::Unavailable::ExportNotReady)
        );
        assert!(app.export_events.is_none());
        assert!(app.export_activity.is_none());
        app.export_sections = original_sections;
        assert!(app.action_availability(Request::new(A::ExportPdf)).is_ok());
        app.editor
            .set_cutting_kerf(Length::from_micrometres(6_000))
            .unwrap();
        assert_eq!(
            app.action_availability(Request::new(A::ExportPdf)),
            Err(actions::Unavailable::ExportNotReady)
        );
        assert!(app.editor.project().export_records.is_empty());
    }

    #[test]
    fn hidden_wood_issue_blocks_shop_card_even_with_all_optional_sections_off() {
        let mut app = navigation_app();
        let back = plan_my_cabinet::reference_fixture::BACK_ID;
        app.selection.hidden.insert(back);
        app.export_sections = ReceiptSections {
            parts_and_costs: false,
            sheets_and_cut_steps: false,
            hinge_references: false,
        };
        await_handoff_packet(&mut app);
        assert!(!app.shop_ready_available());
        assert!(
            app.export_candidate
                .as_ref()
                .unwrap()
                .1
                .as_ref()
                .unwrap()
                .wood_issues()
                .iter()
                .any(|issue| matches!(issue, ExportIssue::Board { id, .. } if *id == back))
        );
        assert_eq!(
            app.invoke(
                Request::new(A::SetExportMode)
                    .argument(Argument::ExportMode(ExportMode::ShopReady))
            ),
            Err(actions::Unavailable::ExportNotReady)
        );
        assert_eq!(app.export_mode, ExportMode::Draft);
        assert!(app.editor.project().export_records.is_empty());
    }

    #[test]
    fn verified_wood_enables_shop_card_without_changing_output_or_interface_language() {
        use plan_my_cabinet::domain::{Allocation, StockGrain};
        let mut project = plan_my_cabinet::reference_fixture::project();
        let stock_id = Uuid::new_v4();
        let mut stock = project.stock[0].clone();
        stock.id = stock_id;
        stock.name = "Backing stock".into();
        stock.material_id = plan_my_cabinet::reference_fixture::HDF_ID;
        stock.length = Length::from_micrometres(1_000_000);
        stock.width = Length::from_micrometres(1_000_000);
        stock.thickness = Length::from_micrometres(3_000);
        stock.grain = StockGrain::Nondirectional;
        stock.priority = 4;
        project.stock.push(stock);
        project.stock_aliases.insert(stock_id, "S4".into());
        project.next_stock_s_alias = 5;
        project.allocations.push(Allocation {
            id: Uuid::new_v4(),
            board_id: plan_my_cabinet::reference_fixture::BACK_ID,
            stock_id,
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: false,
        });
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        await_handoff_packet(&mut app);
        assert!(app.shop_ready_available());
        app.invoke(
            Request::new(A::SetExportMode).argument(Argument::ExportMode(ExportMode::ShopReady)),
        )
        .unwrap();
        app.export_language = Language::PtBr;
        app.export_units = Unit::Foot;
        await_handoff_packet(&mut app);
        assert!(app.shop_ready_available());
        assert_eq!(app.localizer.language(), Language::En);
        assert_eq!(app.editor.project().display_unit, Unit::Mm);
        assert_eq!(app.export_mode, ExportMode::ShopReady);
        assert_eq!(app.export_language, Language::PtBr);
        assert_eq!(app.export_units, Unit::Foot);
    }

    #[test]
    fn picker_return_with_changed_source_never_writes_or_records_receipt() {
        let mut app = navigation_app();
        acknowledge_handoff_packet(&mut app);
        let (tx, rx) = mpsc::channel();
        app.export_events = Some(rx);
        app.export_activity = Some(ExportActivity::Choosing(
            Box::new(app.editor.project().clone()),
            app.export_settings(),
            app.export_mode,
        ));
        app.export_picker_key = Some(app.current_reviewed_packet().unwrap().key().clone());
        let different_kerf =
            Length::from_micrometres(app.editor.project().cutting_kerf.micrometres() + 100);
        app.editor.set_cutting_kerf(different_kerf).unwrap();
        let path = std::env::temp_dir().join(format!("handoff-stale-{}.pdf", Uuid::new_v4()));
        tx.send(ExportEvent::Selected(Some(path.clone()))).unwrap();
        app.poll_pdf_export(&egui::Context::default());
        assert!(app.export_activity.is_none());
        assert!(app.export_preparation.is_none());
        assert!(!path.exists());
        assert!(app.editor.project().export_records.is_empty());
    }

    #[test]
    fn cancelled_handoff_picker_keeps_review_but_creates_no_receipt() {
        let mut app = navigation_app();
        acknowledge_handoff_packet(&mut app);
        let (tx, rx) = mpsc::channel();
        app.export_events = Some(rx);
        app.export_activity = Some(ExportActivity::Choosing(
            Box::new(app.editor.project().clone()),
            app.export_settings(),
            app.export_mode,
        ));
        app.export_picker_key = Some(app.current_reviewed_packet().unwrap().key().clone());
        tx.send(ExportEvent::Selected(None)).unwrap();
        app.poll_pdf_export(&egui::Context::default());
        assert!(app.export_activity.is_none());
        assert!(app.current_reviewed_packet().is_some());
        assert!(app.editor.project().export_records.is_empty());
    }

    #[test]
    fn handoff_write_records_only_the_frozen_reviewed_packet() {
        let mut app = navigation_app();
        app.export_sections = ReceiptSections {
            parts_and_costs: false,
            sheets_and_cut_steps: false,
            hinge_references: false,
        };
        acknowledge_handoff_packet(&mut app);
        let packet = app.current_reviewed_packet().unwrap();
        let path = std::env::temp_dir().join(format!("handoff-write-{}.pdf", Uuid::new_v4()));
        app.start_pdf_write(path.clone(), packet.clone(), Overwrite::Decline);
        let ctx = egui::Context::default();
        for _ in 0..5000 {
            app.poll_pdf_export(&ctx);
            if app.export_events.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.export_events.is_none(), "PDF worker timed out");
        let records = &app.editor.project().export_records;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].metadata.sections, Some(app.export_sections));
        assert_eq!(
            records[0].metadata.layout_version,
            Some(packet.key().layout_version as u16)
        );
        assert_eq!(records[0].revision, packet.key().revision);
        assert_eq!(records[0].path, path);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn handoff_overwrite_refusal_and_failed_write_leave_receipts_and_files_untouched() {
        let mut app = navigation_app();
        acknowledge_handoff_packet(&mut app);
        let packet = app.current_reviewed_packet().unwrap();
        let path = std::env::temp_dir().join(format!("handoff-refuse-{}.pdf", Uuid::new_v4()));
        std::fs::write(&path, b"existing file remains").unwrap();
        app.start_pdf_write(path.clone(), packet.clone(), Overwrite::Decline);
        for _ in 0..5000 {
            app.poll_pdf_export(&egui::Context::default());
            if app.export_events.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.export_events.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), b"existing file remains");
        assert!(app.editor.project().export_records.is_empty());
        std::fs::remove_file(path).unwrap();

        let absent_parent =
            std::env::temp_dir().join(format!("handoff-missing-{}", Uuid::new_v4()));
        let target = absent_parent.join("workshop.pdf");
        app.start_pdf_write(target.clone(), packet, Overwrite::Decline);
        for _ in 0..5000 {
            app.poll_pdf_export(&egui::Context::default());
            if app.export_events.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.export_events.is_none());
        assert!(!target.exists());
        assert!(app.editor.project().export_records.is_empty());
    }

    #[test]
    fn project_replacement_clears_session_targets_and_preserves_document_identity() {
        let mut app = DesktopApp {
            editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
            ..Default::default()
        };
        app.sync_scene_inspector();
        let before = app.editor.project().clone();
        let id = before.allocations[0].board_id;
        let sheet = before.allocations[0].stock_id;
        assert!(app.navigate_session(Destination::BoardAllocation(id)));
        assert_eq!(app.session.focused_sheet, Some(sheet));
        assert_eq!(app.selection.ids.len(), 1);
        app.session.stock.filter = "kept within project".into();

        app.proceed(project_ui::NextAction::New);
        app.sync_scene_inspector();
        assert_ne!(app.editor.project().id, before.id);
        assert!(app.selection.ids.is_empty());
        assert_eq!(app.session.inspector, None);
        assert_eq!(app.session.focused_sheet, None);
        assert_eq!(app.session.allocation_issue, None);
        assert!(app.session.stock.filter.is_empty());
        assert!(!app.navigate_session(Destination::Sheet(sheet)));
        assert_eq!(before.boards.iter().find(|b| b.id == id).unwrap().id, id);
    }

    #[test]
    fn project_replacement_and_os_close_resolve_unsaved_field_drafts_first() {
        let mut app = navigation_app();
        let project = app.editor.project().id;
        let board = app.editor.project().boards[0].id;
        app.request_scene_selection(Some(board), false);
        app.edit_drafts
            .board(&app.editor, board, Unit::Mm, Locale::En)
            .unwrap()
            .length
            .edit("invalid");
        app.request_project_action(project_ui::NextAction::New);
        assert!(app.navigation.pending().is_some());
        assert_eq!(app.editor.project().id, project);
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Stay),
            Outcome::Stayed
        );
        assert_eq!(app.editor.project().id, project);
        assert_eq!(
            app.edit_drafts
                .existing_board(project, board)
                .unwrap()
                .length
                .display(),
            "invalid"
        );
        app.request_project_action(project_ui::NextAction::Close);
        assert!(app.navigation.pending().is_some());
        assert!(!app.project_files.allow_close);
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Abandon),
            Outcome::Navigated
        );
        assert!(app.project_files.allow_close);
        assert!(app.edit_drafts.existing_board(project, board).is_none());
    }

    #[test]
    fn prepared_open_does_not_replace_document_before_board_edit_resolution() {
        let mut app = navigation_app();
        let original = app.editor.project().id;
        let id = app.editor.project().boards[0].id;
        app.request_scene_selection(Some(id), false);
        app.edit_drafts
            .board(&app.editor, id, Unit::Mm, Locale::En)
            .unwrap()
            .length
            .edit("invalid");
        let replacement = ProjectEditor::new(Project::new("Other", Currency::Brl)).unwrap();
        let next = replacement.project().id;
        app.project_files.pending_open = Some((PathBuf::from("unavailable.pmcab"), replacement));
        app.request_project_action(project_ui::NextAction::Open);
        assert_eq!(app.editor.project().id, original);
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Stay),
            Outcome::Stayed
        );
        assert!(app.project_files.pending_open.is_some());
        app.request_project_action(project_ui::NextAction::Open);
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Abandon),
            Outcome::Navigated
        );
        assert_eq!(app.editor.project().id, next);
        assert!(app.edit_drafts.existing_board(original, id).is_none());
    }

    #[test]
    fn project_action_offers_preview_cancellation_before_replacement() {
        let mut app = navigation_app();
        let original = app.editor.project().id;
        let id = app.editor.project().boards[0].id;
        app.request_scene_selection(Some(id), false);
        app.invoke(Request::with(A::PositionBoard, Target::Board(id)))
            .unwrap();
        assert!(app.placement.is_some());
        app.invoke(Request::new(A::NewProject)).unwrap();
        assert!(app.navigation.pending().is_some());
        assert_eq!(app.editor.project().id, original);
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Stay),
            Outcome::Stayed
        );
        assert!(app.placement.is_some());
        app.invoke(Request::new(A::NewProject)).unwrap();
        assert_eq!(
            app.resolve_navigation(NavigationDecision::Abandon),
            Outcome::Navigated
        );
        assert!(app.placement.is_none());
        assert_ne!(app.editor.project().id, original);
    }

    #[test]
    fn handoff_stock_references_and_kerf_dates_are_truthful_in_both_languages() {
        let mut project = ProjectEditor::new(plan_my_cabinet::reference_fixture::project())
            .unwrap()
            .project()
            .clone();
        let piece = project.stock[0].id;
        let alias = project
            .stock_alias(piece)
            .expect("reference alias")
            .to_owned();
        assert_eq!(
            stock_reference(&project, piece),
            format!("{alias} [{piece}]")
        );
        assert_eq!(
            stock_reference(&project, Uuid::nil()),
            Uuid::nil().to_string()
        );
        let unknown = Localizer::new(Language::En);
        let legacy = kerf_confirmation_label(&project, &unknown).expect("confirmed fixture");
        assert!(legacy.contains("date unavailable"), "{legacy}");
        project.confirmed_shop_kerf_unix_ms = Some(1_780_000_000_000);
        for language in [Language::En, Language::PtBr] {
            let label = kerf_confirmation_label(&project, &Localizer::new(language)).unwrap();
            assert!(label.contains("2026-05-28 UTC"), "{label}");
            assert!(!label.contains("cutting-kerf-"), "{label}");
        }
        project.confirmed_shop_kerf = None;
        assert!(kerf_confirmation_label(&project, &unknown).is_none());
    }

    mod performance_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/performance_fixture.rs"
        ));
    }

    use egui::{Event, Key, Modifiers, RawInput};
    use plan_my_cabinet::candidate_generation::SearchBudget;
    use plan_my_cabinet::candidate_ranking::Objective;
    use plan_my_cabinet::domain::{StockGrain, StockSource};
    use plan_my_cabinet::optimization_worker::OptimizationWorker;

    #[test]
    fn large_fixture_workspace_frames_during_worker() {
        use std::time::{Duration, Instant};
        let ctx = egui::Context::default();
        let mut app = DesktopApp {
            editor: ProjectEditor::new(performance_fixture::fixture()).unwrap(),
            ..Default::default()
        };
        let worker = OptimizationWorker::start(
            &app.editor,
            Objective::FewestCuts,
            SearchBudget {
                placements: 10_000,
                witness_states: 20_000,
                beam_width: 8,
            },
            Duration::from_secs(5),
        );
        let start = Instant::now();
        for _ in 0..5 {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1800.0, 1100.0),
                    )),
                    ..Default::default()
                },
                |ui| app.show_workspace(ui),
            );
            assert!(!output.shapes.is_empty());
            output.drop_without_applying_deltas();
        }
        eprintln!(
            "100-board/10-stock five full egui workspace frames during worker: {:?}",
            start.elapsed()
        );
        assert_eq!(app.editor.project().boards.len(), 100);
        worker.cancel();
        assert!(start.elapsed() < Duration::from_secs(30)); // CI hang guard, not a frame-rate SLA.
    }

    #[test]
    fn dense_sheet_first_and_warm_frames() {
        use std::time::Instant;
        let ctx = egui::Context::default();
        let mut project = performance_fixture::fixture();
        for i in 0..100 {
            project
                .allocations
                .push(plan_my_cabinet::domain::Allocation {
                    id: Uuid::from_u128(2000 + i),
                    board_id: project.boards[i as usize].id,
                    stock_id: project.stock[(i / 10) as usize].id,
                    origin: [
                        Length::from_micrometres((i % 10) as i64 * 105_000),
                        Length::ZERO,
                    ],
                    quarter_turn: false,
                    locked: false,
                });
        }
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        let frame = |app: &mut DesktopApp| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1800., 1100.),
                    )),
                    ..Default::default()
                },
                |ui| app.show_workspace(ui),
            )
            .drop_without_applying_deltas();
        };
        let first = Instant::now();
        frame(&mut app);
        eprintln!(
            "100 allocated board first workspace frame: {:?}",
            first.elapsed()
        );
        let warm = Instant::now();
        for _ in 0..5 {
            frame(&mut app);
        }
        eprintln!(
            "100 allocated board five unchanged workspace frames: {:?}",
            warm.elapsed()
        );
    }

    fn repair_frame(
        app: &mut DesktopApp,
        ctx: &egui::Context,
        events: Vec<Event>,
    ) -> Vec<(String, bool, egui::Pos2)> {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1800.0, 1800.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.show_workspace(ui),
        );
        let buttons = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                if node.role() != egui::accesskit::Role::Button {
                    return None;
                }
                let bounds = node.bounds()?;
                Some((
                    node.label()?.to_owned(),
                    node.is_disabled(),
                    egui::pos2(
                        ((bounds.x0 + bounds.x1) / 2.0) as f32,
                        ((bounds.y0 + bounds.y1) / 2.0) as f32,
                    ),
                ))
            })
            .collect();
        output.drop_without_applying_deltas();
        buttons
    }

    fn click_repair_position(app: &mut DesktopApp, ctx: &egui::Context, pos: egui::Pos2) {
        repair_frame(
            app,
            ctx,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        repair_frame(
            app,
            ctx,
            vec![Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
    }

    fn click_repair_button(app: &mut DesktopApp, ctx: &egui::Context, label: &str) {
        let buttons = repair_frame(app, ctx, vec![]);
        let (_, disabled, pos) = buttons
            .iter()
            .find(|(name, _, _)| name == label)
            .unwrap_or_else(|| panic!("missing button {label}: {buttons:?}"));
        assert!(!disabled, "{label} is disabled");
        click_repair_position(app, ctx, *pos);
    }

    #[test]
    fn project_dirty_prompt_cancel_and_discard_are_explicit() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DesktopApp::default();
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        let old = app.editor.project().id;
        app.request_project_action(project_ui::NextAction::New);
        assert!(app.editor.is_dirty());
        repair_frame(&mut app, &ctx, vec![]);
        let cancel = app.localizer.text("cancel");
        click_repair_button(&mut app, &ctx, &cancel);
        assert_eq!(app.editor.project().id, old);
        app.request_project_action(project_ui::NextAction::New);
        repair_frame(&mut app, &ctx, vec![]);
        let discard = app.localizer.text("project-discard");
        click_repair_button(&mut app, &ctx, &discard);
        assert_ne!(app.editor.project().id, old);
        assert!(!app.editor.can_undo());
    }

    #[test]
    fn desktop_save_open_and_invalid_open_preserve_editor() {
        let root = std::env::temp_dir().join(format!("pmcab-desktop-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("test.pmcab");
        let mut app = DesktopApp::default();
        app.project_files.user_data_dir = Some(root.join("user"));
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        app.save_to(&path, false, None);
        assert!(!app.editor.is_dirty());
        let saved = app.editor.project().clone();
        app.editor
            .set_grid_spacing(Length::from_micrometres(30_000))
            .unwrap();
        app.open_path(root.join("missing.pmcab"));
        assert_eq!(app.editor.project().grid_spacing.micrometres(), 30_000);
        assert!(app.editor.is_dirty());
        app.open_path(path.clone());
        assert!(app.project_files.pending_open.is_some());
        assert!(app.editor.is_dirty());
        app.proceed(project_ui::NextAction::Open);
        assert_eq!(app.editor.project(), &saved);
        assert!(!app.editor.can_undo());
        assert!(!app.editor.is_dirty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_dialog_defer_and_recover_keep_explicit_file_intact() {
        use plan_my_cabinet::recovery::{AUTOSAVE_DELAY, RecoveryStore};
        let root = std::env::temp_dir().join(format!("pmcab-ui-recover-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("test.pmcab");
        let mut app = DesktopApp::default();
        app.project_files.user_data_dir = Some(root.join("user"));
        app.save_to(&path, false, None);
        let saved = std::fs::read(&path).unwrap();
        let mut store =
            RecoveryStore::new(&root.join("user"), &path, app.editor.project().id).unwrap();
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        let now = std::time::Instant::now();
        store.note_committed_edit(&app.editor, now).unwrap();
        store.tick(&app.editor, now + AUTOSAVE_DELAY).unwrap();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        app.open_path(path.clone());
        app.proceed(project_ui::NextAction::Open);
        repair_frame(&mut app, &ctx, vec![]);
        click_repair_button(&mut app, &ctx, "Decide later");
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        assert!(store.recovery_path().exists());
        app.open_path(path.clone());
        app.proceed(project_ui::NextAction::Open);
        repair_frame(&mut app, &ctx, vec![]);
        click_repair_button(&mut app, &ctx, "Recover unsaved changes");
        assert_eq!(app.editor.project().grid_spacing.micrometres(), 20_000);
        assert!(app.editor.is_dirty());
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_desktop_new_save_as_open_and_dirty_close_preserve_disk_on_refusal() {
        let root = std::env::temp_dir().join(format!("pmcab-mac-files-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let first = root.join("first.pmcab");
        let second = root.join("second.pmcab");
        let mut app = DesktopApp::default();
        app.project_files.user_data_dir = Some(root.join("user"));
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        app.save_to(&first, false, None);
        let first_bytes = std::fs::read(&first).unwrap();
        app.editor
            .set_grid_spacing(Length::from_micrometres(30_000))
            .unwrap();
        std::fs::write(&second, b"existing document").unwrap();
        app.save_to(&second, false, None); // Save As collision: require confirmation.
        assert!(app.project_files.blocking());
        assert_eq!(std::fs::read(&second).unwrap(), b"existing document");
        assert!(app.editor.is_dirty());
        app.project_files.prompt = None; // decline overwrite
        app.save_to(&second, true, None); // explicit replace
        assert!(!app.editor.is_dirty());
        assert_eq!(std::fs::read(&first).unwrap(), first_bytes);
        let second_bytes = std::fs::read(&second).unwrap();
        app.request_project_action(project_ui::NextAction::New);
        assert_ne!(
            app.editor.project().id,
            plan_my_cabinet::persistence::prepare_reader(second_bytes.as_slice())
                .unwrap()
                .project()
                .id
        );
        app.open_path(second.clone());
        assert_eq!(app.project_files.path.as_deref(), Some(second.as_path()));
        app.editor
            .set_grid_spacing(Length::from_micrometres(40_000))
            .unwrap();
        app.request_project_action(project_ui::NextAction::Close);
        assert!(matches!(
            app.project_files.prompt,
            Some(project_ui::Prompt::Dirty(project_ui::NextAction::Close))
        ));
        assert!(!app.project_files.allow_close);
        assert_eq!(std::fs::read(&second).unwrap(), second_bytes);
        app.save_to(&second, true, Some(project_ui::NextAction::Close));
        assert!(app.project_files.allow_close);
        assert_ne!(std::fs::read(&second).unwrap(), second_bytes);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_native_close_request_cancels_until_dirty_choice() {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::default();
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        let mut input = RawInput::default();
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
        let output = ctx.run_ui(input, |ui| app.handle_close_request(ui.ctx()));
        assert!(matches!(
            app.project_files.prompt,
            Some(project_ui::Prompt::Dirty(project_ui::NextAction::Close))
        ));
        assert!(!app.project_files.allow_close);
        assert!(output.viewport_output.values().any(|v| {
            v.commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
        }));
        output.drop_without_applying_deltas();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_failed_export_worker_leaves_existing_pdf_and_receipts_intact() {
        let root = std::env::temp_dir().join(format!("pmcab-mac-export-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let existing = root.join("packet.pdf");
        std::fs::write(&existing, b"previous PDF").unwrap();
        let mut app = DesktopApp::default();
        let before = app.editor.project().clone();
        let failure = root.join("missing-parent").join("packet.pdf");
        let packet = Arc::new(
            ReviewedPacket::prepare(
                &before,
                ExportMode::Draft,
                ExportSettings {
                    language: Language::En,
                    units: Unit::Mm,
                },
                ReceiptSections::default(),
            )
            .unwrap(),
        );
        app.start_pdf_write(failure, packet, Overwrite::Decline);
        let ctx = egui::Context::default();
        for _ in 0..5000 {
            app.poll_pdf_export(&ctx);
            if app.export_events.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(app.export_events.is_none(), "export worker timed out");
        assert!(
            app.export_message
                .as_ref()
                .is_some_and(|m| m.contains("Could not export PDF"))
        );
        assert_eq!(app.editor.project(), &before);
        assert!(app.editor.project().export_records.is_empty());
        assert_eq!(std::fs::read(existing).unwrap(), b"previous PDF");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sheet_repair_excludes_other_edits_and_other_modal_excludes_repair() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DesktopApp::default();
        let material = app
            .editor
            .create_material(NewMaterial {
                name: "wood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let board = app
            .editor
            .create_board(NewBoard {
                name: "part".into(),
                material_id: material,
                length: Length::from_micrometres(100_000),
                width: Length::from_micrometres(50_000),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        app.editor
            .create_stock(
                plan_my_cabinet::stock_commands::StockInput {
                    name: "sheet".into(),
                    material_id: material,
                    length: Length::from_micrometres(300_000),
                    width: Length::from_micrometres(200_000),
                    thickness: Length::from_micrometres(18_000),
                    grain: StockGrain::Nondirectional,
                    source: StockSource::Owned,
                    price: None,
                    trim: [Length::ZERO; 4],
                },
                1,
            )
            .unwrap();
        app.editor
            .set_grid_spacing(Length::from_micrometres(11_000))
            .unwrap();
        app.editor.undo().unwrap();
        assert!(app.editor.can_undo() && app.editor.can_redo());
        app.selection.choose(Some(board), false);
        app.session.switch(Workspace::CutPlan);
        let before = app.editor.project().clone();
        app.grid_dialog = Some(GridDialog::open(app.editor.project(), Locale::En));
        let buttons = repair_frame(&mut app, &ctx, vec![]);
        let (_, disabled, position) = buttons
            .iter()
            .find(|(name, _, _)| name == &app.localizer.text("sheet-edit"))
            .unwrap();
        assert!(disabled);
        click_repair_position(&mut app, &ctx, *position);
        assert!(!app.sheet_repair.active());
        assert!(app.editor.preview().is_none());
        app.grid_dialog = None;
        // Retire the modal focus layer before interacting with the workspace.
        repair_frame(&mut app, &ctx, vec![]);
        let edit = app.localizer.text("sheet-edit");
        let cancel = app.localizer.text("sheet-cancel");
        let accept = app.localizer.text("sheet-accept");
        click_repair_button(&mut app, &ctx, &edit);
        assert!(app.sheet_repair.active());
        assert!(app.modal_open());
        assert!(!app.other_modal_open());
        let buttons = repair_frame(&mut app, &ctx, vec![]);
        for action in [A::Undo, A::Redo, A::NewStock, A::EditGrid, A::EditKerf] {
            assert!(app.action_availability(Request::new(action)).is_err());
        }
        for key in ["sheet-accept", "sheet-cancel", "sheet-stage"] {
            let label = app.localizer.text(key);
            assert!(
                buttons
                    .iter()
                    .any(|(name, disabled, _)| name == &label && !*disabled),
                "{key} should be enabled"
            );
        }
        assert!(app.invoke(Request::new(A::EditGrid)).is_err());
        assert_eq!(app.editor.project(), &before);
        assert!(
            app.dialog.is_none()
                && app.stock_dialog.is_none()
                && app.grid_dialog.is_none()
                && app.placement.is_none()
        );
        click_repair_button(&mut app, &ctx, &cancel);
        assert!(!app.sheet_repair.active());
        assert!(app.editor.preview().is_none());
        assert_eq!(app.editor.project(), &before);
        click_repair_button(&mut app, &ctx, &edit);
        click_repair_button(&mut app, &ctx, &accept);
        assert!(!app.sheet_repair.active());
        assert!(app.editor.preview().is_none());
    }

    #[test]
    fn native_backend_matches_platform() {
        #[cfg(target_os = "macos")]
        assert_eq!(native_backend(), wgpu::Backends::METAL);
        #[cfg(target_os = "linux")]
        assert_eq!(native_backend(), wgpu::Backends::VULKAN);
    }

    #[test]
    fn new_board_text_requires_explicit_rounding_consent() {
        let mut draft = DimensionDraft::new();
        draft.text = "1/64 in".into();
        assert!(draft.value(Unit::Mm).is_err());
        draft.consent = true;
        assert_eq!(draft.value(Unit::Mm), Ok(Length::from_micrometres(397)));
        draft.text = "0 mm".into();
        assert_eq!(
            draft.value(Unit::Mm),
            Err(InputError::Unit(UnitError::NonPositiveDimension))
        );
        draft.text = "NaN".into();
        assert!(draft.value(Unit::Mm).is_err());
    }

    fn creation_frame(app: &mut DesktopApp, ctx: &egui::Context, key: Option<egui::Key>) {
        ctx.run_ui(
            egui::RawInput {
                events: key
                    .into_iter()
                    .map(|key| egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .collect(),
                ..Default::default()
            },
            |ui| app.show_dialog(ui.ctx()),
        )
        .drop_without_applying_deltas();
    }

    fn creation_fixture() -> (DesktopApp, Uuid, Uuid) {
        use plan_my_cabinet::domain::{StockGrain, StockSource};
        use plan_my_cabinet::stock_commands::StockInput;
        let mut app = DesktopApp::default();
        let material = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let stock = app
            .editor
            .create_stock(
                StockInput {
                    name: "One sheet".into(),
                    material_id: material,
                    length: Length::from_micrometres(100_000),
                    width: Length::from_micrometres(50_000),
                    thickness: Length::from_micrometres(18_000),
                    grain: StockGrain::AlongX,
                    source: StockSource::Owned,
                    price: None,
                    trim: [Length::ZERO; 4],
                },
                1,
            )
            .unwrap()[0];
        (app, material, stock)
    }

    #[test]
    fn creation_preview_repeated_edits_and_cancel_do_not_touch_stock_or_history() {
        let ctx = egui::Context::default();
        let (mut app, material, stock) = creation_fixture();
        let before = app.editor.project().clone();
        let undo = app.editor.can_undo();
        app.dialog = Some(CreationDialog::board(Some(material)));
        for size in ["100 mm", "75 mm", "120 mm", "100 mm"] {
            let draft = app.dialog.as_mut().unwrap();
            draft.length.text = size.into();
            draft.width.text = "50 mm".into();
            creation_frame(&mut app, &ctx, None);
            let preview = app.dialog.as_ref().unwrap().preview.unwrap().1;
            assert_eq!(
                preview.fit,
                if size == "120 mm" {
                    FirstFit::NoFit
                } else {
                    FirstFit::Allocated(stock)
                }
            );
            assert_eq!(app.editor.project(), &before);
            assert_eq!(app.editor.can_undo(), undo);
        }
        creation_frame(&mut app, &ctx, Some(egui::Key::Escape));
        assert!(app.dialog.is_none());
        assert_eq!(app.editor.project(), &before);
        assert_eq!(app.editor.project().revision, before.revision);
        assert!(app.editor.project().allocations.is_empty());
    }

    #[test]
    fn creation_requires_fresh_rounding_consent_and_popup_keys_stay_inside() {
        let ctx = egui::Context::default();
        let (mut app, material, _) = creation_fixture();
        app.dialog = Some(CreationDialog::board(Some(material)));
        app.dialog.as_mut().unwrap().length.text = "1/64 in".into();
        app.dialog.as_mut().unwrap().width.text = "50 mm".into();
        creation_frame(&mut app, &ctx, None);
        assert!(
            app.dialog
                .as_ref()
                .unwrap()
                .board_key(app.editor.project())
                .is_none()
        );
        let before = app.editor.project().clone();
        creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
        assert_eq!(app.editor.project(), &before);
        app.dialog.as_mut().unwrap().length.consent = true;
        creation_frame(&mut app, &ctx, None);
        assert_eq!(
            app.dialog
                .as_ref()
                .unwrap()
                .board_key(app.editor.project())
                .unwrap()
                .length,
            Length::from_micrometres(397)
        );
        let popup = egui::Id::new("board-creation-material").with("popup");
        egui::Popup::open_id(&ctx, popup);
        creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
        assert!(app.dialog.is_some());
        assert_eq!(app.editor.project(), &before);
        egui::Popup::open_id(&ctx, popup);
        creation_frame(&mut app, &ctx, Some(egui::Key::Escape));
        assert!(app.dialog.is_some());
        assert_eq!(app.editor.project(), &before);
        // Editing the proposed quantity invalidates the previous consent.
        app.dialog.as_mut().unwrap().length.text = "3/64 in".into();
        app.dialog.as_mut().unwrap().length.consent = false;
        creation_frame(&mut app, &ctx, None);
        assert!(
            app.dialog
                .as_ref()
                .unwrap()
                .board_key(app.editor.project())
                .is_none()
        );
    }

    #[test]
    fn creation_confirm_uses_current_stock_and_one_undo_for_board_and_color() {
        let ctx = egui::Context::default();
        let (mut app, material, stock) = creation_fixture();
        let mut board = CreationDialog::board(Some(material));
        board.name = "Shelf".into();
        board.length.text = "100 mm".into();
        board.width.text = "50 mm".into();
        board.grain_override = Some(BoardGrain::Unrestricted);
        app.dialog = Some(board);
        creation_frame(&mut app, &ctx, None);
        assert_eq!(
            app.dialog.as_ref().unwrap().preview.unwrap().1.fit,
            FirstFit::Allocated(stock)
        );
        // An intervening edit consumes that space. The old preview is never an allocation reservation.
        app.editor
            .create_board_with_fit(NewBoard {
                name: "Earlier part".into(),
                material_id: material,
                length: Length::from_micrometres(100_000),
                width: Length::from_micrometres(50_000),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
        assert!(app.dialog.is_none());
        assert_eq!(app.first_fit_notice, Some(FirstFit::NoFit));
        assert_eq!(app.editor.project().boards.len(), 2);
        assert_eq!(app.editor.project().allocations.len(), 1);
        assert_eq!(
            app.editor.project().boards[1].grain_override,
            Some(BoardGrain::Unrestricted)
        );
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().boards.len(), 1);

        let mut material_draft = CreationDialog::material();
        material_draft.name = "Oak".into();
        material_draft.thickness.text = "18 mm".into();
        material_draft.color = Some(SrgbColor([226, 197, 156]));
        app.dialog = Some(material_draft);
        creation_frame(&mut app, &ctx, None);
        creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
        assert!(app.dialog.is_none());
        let id = app.editor.project().materials.last().unwrap().id;
        assert_eq!(
            app.editor.project().material_colors.get(&id),
            Some(&SrgbColor([226, 197, 156]))
        );
        app.editor.undo().unwrap();
        assert!(app.editor.project().materials.iter().all(|m| m.id != id));
        assert!(!app.editor.project().material_colors.contains_key(&id));
    }

    #[test]
    fn creation_rejects_a_replaced_scene_without_losing_the_draft() {
        let ctx = egui::Context::default();
        let (mut app, material, _) = creation_fixture();
        let mut board = CreationDialog::board(Some(material));
        board.name = "Pending".into();
        board.length.text = "100 mm".into();
        board.width.text = "50 mm".into();
        app.dialog = Some(board);
        creation_frame(&mut app, &ctx, None);
        let replacement = Project::new("Different project", Currency::Brl);
        app.editor = ProjectEditor::new(replacement.clone()).unwrap();
        creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
        assert_eq!(app.editor.project(), &replacement);
        assert!(app.dialog.is_some());
        assert_eq!(app.dialog.as_ref().unwrap().name, "Pending");
        creation_frame(&mut app, &ctx, Some(egui::Key::Escape));
        assert!(app.dialog.is_none());
        assert_eq!(app.editor.project(), &replacement);
    }

    #[test]
    fn creation_out_of_world_bounds_has_no_accepted_preview() {
        let ctx = egui::Context::default();
        let (mut app, material, _) = creation_fixture();
        let before = app.editor.project().clone();
        let mut board = CreationDialog::board(Some(material));
        board.length.text = "1000001 mm".into();
        board.width.text = "50 mm".into();
        app.dialog = Some(board);
        creation_frame(&mut app, &ctx, None);
        assert!(app.dialog.as_ref().unwrap().preview.is_none());
        creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
        assert!(app.dialog.is_some());
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn nested_material_success_selects_new_color_without_resetting_board_fields() {
        let ctx = egui::Context::default();
        let (mut app, original_material, _) = creation_fixture();
        let mut board = CreationDialog::board(Some(original_material));
        board.name = "Pending shelf".into();
        board.length.text = "1/64 in".into();
        board.length.consent = true;
        board.width.text = "50 mm".into();
        board.grain_override = Some(BoardGrain::Unrestricted);
        app.dialog = Some(board);
        creation_frame(&mut app, &ctx, None);
        app.suspended_board = app.dialog.take();
        let mut material = CreationDialog::material();
        material.name = "Walnut".into();
        material.thickness.text = "18 mm".into();
        material.color = Some(SrgbColor([166, 136, 101]));
        app.dialog = Some(material);
        creation_frame(&mut app, &ctx, None);
        creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
        let new_material = app.editor.project().materials.last().unwrap().id;
        assert_ne!(new_material, original_material);
        assert_eq!(
            app.editor.project().material_colors.get(&new_material),
            Some(&SrgbColor([166, 136, 101]))
        );
        assert!(app.editor.project().boards.is_empty());
        assert!(app.editor.project().allocations.is_empty());
        let restored = app.dialog.as_ref().unwrap();
        assert_eq!(restored.name, "Pending shelf");
        assert_eq!(restored.length.text, "1/64 in");
        assert!(restored.length.consent);
        assert_eq!(restored.width.text, "50 mm");
        assert_eq!(restored.grain_override, Some(BoardGrain::Unrestricted));
        assert_eq!(restored.material_id, Some(new_material));
    }

    #[test]
    fn grid_dialog_focus_invalid_input_and_escape_leave_setting_unchanged() {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::default();
        let initial = app.editor.project().clone();
        app.grid_dialog = Some(GridDialog::open(app.editor.project(), Locale::En));
        let draw = |app: &mut DesktopApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.show_grid_dialog(ui.ctx());
                },
            )
            .drop_without_applying_deltas();
        };
        draw(&mut app, vec![]);
        assert_eq!(app.editor.project(), &initial);
        app.grid_dialog.as_mut().unwrap().value.text = "0 mm".into();
        draw(&mut app, vec![]);
        assert_eq!(app.editor.project(), &initial);
        app.grid_dialog.as_mut().unwrap().value.text = "1/64 in".into();
        draw(&mut app, vec![]);
        assert!(!app.grid_dialog.as_ref().unwrap().value.consent);
        draw(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(app.grid_dialog.is_none());
        assert_eq!(app.editor.project(), &initial);
        assert!(!app.editor.can_undo());
    }

    #[test]
    fn numeric_modal_preview_cancel_and_accept_are_single_transaction() {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let board_id = app
            .editor
            .create_board(NewBoard {
                name: "Side".into(),
                material_id,
                length: Length::from_micrometres(400_000),
                width: Length::from_micrometres(200_000),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        app.selection.choose(Some(board_id), false);
        let initial = app.editor.project().clone();
        let draw = |app: &mut DesktopApp, input| {
            ctx.run_ui(input, |ui| app.show_placement(ui.ctx()))
                .drop_without_applying_deltas();
        };
        app.placement = PlacementDialog::numeric(&app, board_id);
        if let placement_ui::PlacementDraft::Numeric { position, .. } =
            &mut app.placement.as_mut().unwrap().draft
        {
            position[0].text = "25.0004 mm".into();
        }
        draw(&mut app, RawInput::default());
        assert!(app.editor.preview().is_none(), "rounding requires consent");
        if let placement_ui::PlacementDraft::Numeric { position, .. } =
            &mut app.placement.as_mut().unwrap().draft
        {
            position[0].consent = true;
        }
        draw(&mut app, RawInput::default());
        assert_eq!(app.editor.project(), &initial);
        assert_eq!(
            app.editor.preview().unwrap().boards[0].pose.translation_mm[0],
            25.0
        );
        assert_eq!(app.selection.active, Some(board_id));
        draw(
            &mut app,
            RawInput {
                events: vec![Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
        );
        assert!(app.placement.is_none());
        assert_eq!(app.editor.project(), &initial);
        assert!(app.editor.preview().is_none());
        assert_eq!(app.selection.active, Some(board_id));
        app.placement = PlacementDialog::numeric(&app, board_id);
        if let placement_ui::PlacementDraft::Numeric { position, .. } =
            &mut app.placement.as_mut().unwrap().draft
        {
            position[0].text = "25 mm".into();
        }
        draw(&mut app, RawInput::default());
        app.editor.commit_preview().unwrap();
        app.placement = None;
        assert_eq!(app.editor.project().revision, initial.revision + 1);
        assert_eq!(app.editor.project().boards[0].pose.translation_mm[0], 25.0);
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().boards, initial.boards);
    }

    #[test]
    fn numeric_dialog_unedited_frame_switch_and_restored_fields_do_not_commit() {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Wood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let board_id = app
            .editor
            .create_board(NewBoard {
                name: "Board".into(),
                material_id,
                length: Length::from_micrometres(100_000),
                width: Length::from_micrometres(50_000),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        let exact = Pose::new(
            [0.0005, -2.0005, 3.0005],
            Quaternion::normalized(0.9, 0.1, 0.2, 0.3).unwrap(),
        )
        .unwrap();
        app.editor
            .transact(|project| {
                project
                    .boards
                    .iter_mut()
                    .find(|board| board.id == board_id)
                    .unwrap()
                    .pose = exact;
                Ok::<_, ()>(())
            })
            .unwrap();
        let before = app.editor.project().clone();
        app.placement = PlacementDialog::numeric(&app, board_id);
        let draw = |app: &mut DesktopApp| {
            ctx.run_ui(RawInput::default(), |ui| app.show_placement(ui.ctx()))
                .drop_without_applying_deltas();
        };
        draw(&mut app);
        assert!(app.editor.preview().is_none());
        if let placement_ui::PlacementDraft::Numeric { frame, .. } =
            &mut app.placement.as_mut().unwrap().draft
        {
            *frame = plan_my_cabinet::placement::CoordinateFrame::World;
        }
        draw(&mut app);
        assert!(app.editor.preview().is_none());
        if let placement_ui::PlacementDraft::Numeric { position, .. } =
            &mut app.placement.as_mut().unwrap().draft
        {
            position[0].text = "5 mm".into();
        }
        draw(&mut app);
        assert!(app.editor.preview().is_some());
        if let placement_ui::PlacementDraft::Numeric { position, .. } =
            &mut app.placement.as_mut().unwrap().draft
        {
            position[0].text = format!(
                "{:.3}",
                plan_my_cabinet::placement::world_pose(&before, board_id)
                    .unwrap()
                    .translation_mm[0]
            );
        }
        draw(&mut app);
        assert!(app.editor.preview().is_none());
        assert_eq!(app.editor.project(), &before);
        assert_eq!(app.editor.project().revision, before.revision);
    }

    #[test]
    fn face_modal_previews_orientation_and_offset_without_committing_relation() {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Wood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let make_board = |app: &mut DesktopApp, name: &str, x| {
            app.editor
                .create_board(NewBoard {
                    name: name.into(),
                    material_id,
                    length: Length::from_micrometres(100_000),
                    width: Length::from_micrometres(50_000),
                    pose: Pose::new([x, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
                })
                .unwrap()
        };
        let source = make_board(&mut app, "Source", 0.0);
        let target = make_board(&mut app, "Target", 200.0);
        let before = app.editor.project().clone();
        app.placement = PlacementDialog::face(&app, source);
        if let placement_ui::PlacementDraft::Face {
            target: chosen,
            source_face,
            target_face,
            offset,
            gap,
            ..
        } = &mut app.placement.as_mut().unwrap().draft
        {
            *chosen = target;
            *source_face = plan_my_cabinet::placement::BoardFace {
                axis: 0,
                side: plan_my_cabinet::placement::Side::Negative,
            };
            *target_face = plan_my_cabinet::placement::BoardFace {
                axis: 1,
                side: plan_my_cabinet::placement::Side::Positive,
            };
            offset[0].text = "7 mm".into();
            gap.text = "2 mm".into();
        }
        ctx.run_ui(RawInput::default(), |ui| app.show_placement(ui.ctx()))
            .drop_without_applying_deltas();
        let preview = app.editor.preview().unwrap();
        assert_ne!(preview.boards[0].pose, before.boards[0].pose);
        assert_ne!(
            preview.boards[0].pose.rotation,
            before.boards[0].pose.rotation
        );
        assert_eq!(app.editor.project(), &before);
        assert_eq!(
            app.placement.as_ref().unwrap().highlighted().unwrap().2,
            target
        );
        ctx.run_ui(
            RawInput {
                events: vec![Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| app.show_placement(ui.ctx()),
        )
        .drop_without_applying_deltas();
        assert_eq!(app.editor.project(), &before);
        assert!(app.editor.preview().is_none());
    }

    #[test]
    fn delete_in_text_field_has_no_scene_delete_action_and_modal_blocks_hierarchy_edits() {
        use egui::{Event, Key, Modifiers, RawInput};
        let ctx = egui::Context::default();
        let mut app = DesktopApp::default();
        let material = app
            .editor
            .create_material(NewMaterial {
                name: "wood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let board = app
            .editor
            .create_board(NewBoard {
                name: "part".into(),
                material_id: material,
                length: Length::from_micrometres(100_000),
                width: Length::from_micrometres(50_000),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        app.selection.choose(Some(board), false);
        let before = app.editor.project().clone();
        let mut text = "edit me".to_owned();
        ctx.run_ui(RawInput::default(), |ui| {
            ui.text_edit_singleline(&mut text).request_focus();
            app.show_hierarchy(ui);
            app.show_measurement(ui);
        })
        .drop_without_applying_deltas();
        assert!(ctx.text_edit_focused());
        ctx.run_ui(
            RawInput {
                events: vec![Event::Key {
                    key: Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                ui.text_edit_singleline(&mut text);
                app.show_hierarchy(ui);
                app.show_measurement(ui);
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(app.editor.project(), &before);
        assert!(app.selection.ids.contains(&board));
        // There is currently no Delete scene action, including with viewport focus.
        app.assembly_dialog = Some(assembly_ui::AssemblyDialog::new(
            &app,
            assembly_ui::Operation::Transform,
        ));
        assert!(app.modal_open());
        ctx.run_ui(
            RawInput {
                events: vec![Event::Key {
                    key: Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                app.show_hierarchy(ui);
                app.show_measurement(ui);
                app.show_assembly_dialog(ui.ctx());
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(app.editor.project(), &before);
        assert_eq!(app.selection.active, Some(board));
        assert!(app.modal_open());
    }

    #[test]
    fn dialogs_take_and_keep_keyboard_focus_until_closed() {
        fn key(key: Key, modifiers: Modifiers) -> RawInput {
            RawInput {
                events: vec![Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            }
        }

        for kind in [
            "board",
            "material",
            "edit",
            "assign",
            "dimension",
            "batch",
            "numeric",
            "face",
            "stock",
            "currency",
            "fee",
            "grid",
            "kerf",
            "hardware",
            "hinge",
            "relationship",
            "transform",
        ] {
            let ctx = egui::Context::default();
            let mut app = DesktopApp::default();
            let mut background = String::new();
            let material_id = app
                .editor
                .create_material(NewMaterial {
                    name: "Plywood".into(),
                    thickness: Length::from_micrometres(18_000),
                    grain: BoardGrain::Length,
                })
                .unwrap();
            let board_id = app
                .editor
                .create_board(NewBoard {
                    name: "Side".into(),
                    material_id,
                    length: Length::from_micrometres(400_000),
                    width: Length::from_micrometres(200_000),
                    pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
                })
                .unwrap();
            if kind == "face" {
                app.editor
                    .create_board(NewBoard {
                        name: "Top".into(),
                        material_id,
                        length: Length::from_micrometres(400_000),
                        width: Length::from_micrometres(200_000),
                        pose: Pose::new([500.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
                    })
                    .unwrap();
            }
            let project = app.editor.project();
            match kind {
                "board" => app.dialog = Some(CreationDialog::board(Some(material_id))),
                "material" => app.dialog = Some(CreationDialog::material()),
                "edit" => {
                    app.material_edit = Some(MaterialEditDialog {
                        focus_on_open: true,
                        id: material_id,
                        name: "Plywood".into(),
                        thickness: DimensionDraft {
                            text: "18 mm".into(),
                            consent: false,
                        },
                        grain: BoardGrain::Length,
                        anchor: Anchor::Centre,
                        choice: None,
                        error: None,
                    });
                }
                "assign" => {
                    app.board_material = Some(BoardMaterialDialog {
                        focus_on_open: true,
                        board_id,
                        project_id: project.id,
                        revision: project.revision,
                        material_id: Some(material_id),
                        anchor: Anchor::Centre,
                        error: None,
                    });
                }
                "dimension" => {
                    app.board_dimension = Some(BoardDimensionDialog {
                        focus_on_open: true,
                        board_id,
                        project_id: project.id,
                        revision: project.revision,
                        dimension: BoardDimension::Length,
                        value: DimensionDraft {
                            text: "400 mm".into(),
                            consent: false,
                        },
                        anchor: Anchor::Centre,
                        error: None,
                    });
                }
                "batch" => {
                    app.batch_dimension = Some(BatchDialog {
                        focus_on_open: true,
                        project_id: project.id,
                        revision: project.revision,
                        ids: vec![board_id],
                        dimension: BoardDimension::Length,
                        value: DimensionDraft::new(),
                        anchors: vec![(board_id, Anchor::Centre)],
                        error: None,
                    });
                }
                "numeric" => app.placement = PlacementDialog::numeric(&app, board_id),
                "face" => app.placement = PlacementDialog::face(&app, board_id),
                "stock" => app.stock_dialog = Some(stock_ui::StockDialog::new(project)),
                "currency" => app.currency_dialog = Some(currency_ui::CurrencyDialog::new(project)),
                "hardware" => {
                    app.hardware_dialog = Some(hardware_ui::HardwareDialog::new(&app, None))
                }
                "hinge" => app.hinge_dialog = Some(hinge_ui::HingeDialog::new(&app, None)),
                "relationship" => {
                    app.door_dialog = Some(door_joint_ui::DoorDialog::new(&app, None))
                }
                "transform" => {
                    app.selection.choose(Some(board_id), false);
                    app.assembly_dialog = Some(assembly_ui::AssemblyDialog::new(
                        &app,
                        assembly_ui::Operation::Transform,
                    ));
                }
                "fee" | "grid" | "kerf" => {
                    app.invoke(Request::new(match kind {
                        "fee" => A::EditCutFee,
                        "grid" => A::EditGrid,
                        _ => A::EditKerf,
                    }))
                    .unwrap();
                }
                _ => unreachable!(),
            }

            let mut draw = |input| {
                let mut background_id = None;
                ctx.run_ui(input, |ui| {
                    background_id = Some(ui.text_edit_singleline(&mut background).id);
                    app.show_dialog(ui.ctx());
                    app.show_material_edit(ui.ctx());
                    app.show_board_material(ui.ctx());
                    app.show_board_dimension(ui.ctx());
                    app.show_batch_dimension(ui.ctx());
                    app.show_placement(ui.ctx());
                    app.show_stock_dialog(ui.ctx());
                    app.show_currency_dialog(ui.ctx());
                    app.show_cut_fee_dialog(ui.ctx());
                    app.show_grid_dialog(ui.ctx());
                    app.show_hardware_dialog(ui.ctx());
                    app.show_hinge_dialog(ui.ctx());
                    app.show_door_dialog(ui.ctx());
                    app.show_assembly_dialog(ui.ctx());
                })
                .drop_without_applying_deltas();
                (
                    background_id.unwrap(),
                    ctx.memory(|memory| memory.focused()),
                    app.modal_open(),
                )
            };

            let (background_id, initial, _) = draw(RawInput::default());
            assert!(
                initial.is_some() && initial != Some(background_id),
                "{kind}: initial focus"
            );
            let mut moved = false;
            for modifiers in std::iter::repeat_n(Modifiers::NONE, 40)
                .chain(std::iter::repeat_n(Modifiers::SHIFT, 40))
            {
                let (_, focused, _) = draw(key(Key::Tab, modifiers));
                assert!(
                    focused.is_some() && focused != Some(background_id),
                    "{kind}: tab escaped"
                );
                moved |= focused != initial;
            }
            assert!(moved, "{kind}: focus was reset on every Tab");
            let (_, _, open) = draw(key(Key::Escape, Modifiers::NONE));
            assert!(!open, "{kind}: Escape should close dialog");
            draw(RawInput::default()); // Modal focus layer is retired at the end of this frame.
            let (_, focus, _) = draw(key(Key::Tab, Modifiers::NONE));
            assert_eq!(
                focus,
                Some(background_id),
                "{kind}: background usable after close"
            );
        }
    }

    #[test]
    fn hardware_choosers_close_on_keyboard_selection_without_submitting_parent() {
        for relationship in [false, true] {
            let mut app = navigation_app();
            if relationship {
                app.door_dialog = Some(door_joint_ui::DoorDialog::new(&app, None));
            } else {
                app.hinge_dialog = Some(hinge_ui::HingeDialog::new(&app, None));
            }
            let before = app.editor.project().clone();
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let draw = |app: &mut DesktopApp, key: Option<egui::Key>| {
                let events = key.map_or_else(Vec::new, |key| {
                    vec![
                        egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: false,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                });
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1280.0, 875.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        app.show_hinge_dialog(ui.ctx());
                        app.show_door_dialog(ui.ctx());
                    },
                );
                let update = output.platform_output.accesskit_update.as_ref().unwrap();
                let role = update
                    .nodes
                    .iter()
                    .find(|(id, _)| *id == update.focus)
                    .map(|(_, node)| node.role());
                output.drop_without_applying_deltas();
                role
            };
            assert_eq!(draw(&mut app, None), Some(egui::accesskit::Role::ComboBox));
            let chooser = ctx.memory(|m| m.focused()).unwrap();
            for _ in 0..2 {
                ctx.memory_mut(|m| m.request_focus(chooser));
                draw(&mut app, Some(egui::Key::Enter));
                assert!(egui::Popup::is_any_open(&ctx));
                draw(&mut app, Some(egui::Key::Tab));
                assert_eq!(
                    draw(&mut app, Some(egui::Key::Tab)),
                    Some(egui::accesskit::Role::Button)
                );
                draw(&mut app, Some(egui::Key::Enter));
                assert!(
                    !egui::Popup::is_any_open(&ctx),
                    "relationship={relationship}"
                );
                assert!(app.hinge_dialog.is_some() || app.door_dialog.is_some());
                assert_eq!(app.editor.project(), &before);
            }
            draw(&mut app, Some(egui::Key::Escape));
            assert!(app.hinge_dialog.is_none() && app.door_dialog.is_none());
            assert_eq!(app.editor.project(), &before);
        }
    }

    #[test]
    fn project_prompt_blocks_camera_wheel_on_its_first_mounted_frame() {
        for workspace in [Workspace::Design, Workspace::Hardware] {
            let mut app = navigation_app();
            app.session.switch(workspace);
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let size = egui::vec2(1440.0, 900.0);
            let wheel = || {
                vec![
                    Event::PointerMoved(egui::pos2(740.0, 420.0)),
                    Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, 100.0),
                        modifiers: Modifiers::NONE,
                        phase: egui::TouchPhase::Move,
                    },
                ]
            };
            for _ in 0..3 {
                responsive_frame(&mut app, &ctx, size, vec![]);
            }
            let before = app.session.view_mut(workspace).camera.navigation_state();
            responsive_frame(&mut app, &ctx, size, wheel());
            let zoomed = app.session.view_mut(workspace).camera.navigation_state();
            assert_ne!(
                before, zoomed,
                "wheel must reach unblocked {workspace:?} canvas"
            );
            app.project_files.prompt = Some(project_ui::Prompt::Dirty(project_ui::NextAction::New));
            assert!(app.blocking_surface_open());
            let project = app.editor.project().clone();
            responsive_frame(&mut app, &ctx, size, wheel());
            assert_eq!(
                app.session.view_mut(workspace).camera.navigation_state(),
                zoomed
            );
            assert_eq!(app.editor.project(), &project);
            assert!(app.project_files.prompt.is_some());
        }
    }

    #[test]
    fn advanced_board_resize_modal_requires_consent_and_commits_one_undo() {
        for size in [egui::vec2(1440.0, 900.0), egui::vec2(900.0, 650.0)] {
            let mut app = navigation_app();
            let board = app.editor.project().boards[0].id;
            app.invoke(Request::with(A::EditDimensions, Target::Board(board)))
                .unwrap();
            let before = app.editor.project().clone();
            let draft = app.board_dimension.as_mut().unwrap();
            draft.value.text = "1/64 in".into();
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            theme::install_fonts(&ctx);
            let frame = |app: &mut DesktopApp, events| {
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        events,
                        ..Default::default()
                    },
                    |ui| app.show_board_dimension(ui.ctx()),
                );
                let buttons = output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .filter_map(|(_, node)| {
                        (node.role() == egui::accesskit::Role::Button && !node.is_disabled())
                            .then(|| {
                                let bounds = node.bounds()?;
                                Some((
                                    node.label()?.to_owned(),
                                    egui::pos2(
                                        (bounds.x0 + bounds.x1) as f32 / 2.0,
                                        (bounds.y0 + bounds.y1) as f32 / 2.0,
                                    ),
                                ))
                            })
                            .flatten()
                    })
                    .collect::<Vec<_>>();
                output.drop_without_applying_deltas();
                buttons
            };
            let invalid = frame(
                &mut app,
                vec![Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
            );
            assert!(!invalid.iter().any(|(label, _)| label == "Confirm"));
            assert_eq!(app.editor.project(), &before);
            app.board_dimension.as_mut().unwrap().value.consent = true;
            let ready = frame(&mut app, vec![]);
            let confirm = ready
                .iter()
                .find(|(label, _)| label == "Confirm")
                .unwrap_or_else(|| panic!("resize confirmation not reachable: {ready:?}"))
                .1;
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![
                        Event::PointerMoved(confirm),
                        Event::PointerButton {
                            pos: confirm,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
            }
            assert!(app.board_dimension.is_none());
            assert_eq!(app.editor.project().boards[0].length.micrometres(), 397);
            assert_eq!(app.editor.project().revision, before.revision + 1);
            assert!(app.editor.undo().unwrap());
            assert_eq!(app.editor.project().boards, before.boards);
        }
    }

    #[test]
    fn board_material_modal_rejects_noop_and_commits_valid_choice_once() {
        let mut app = navigation_app();
        let before = app.editor.project().clone();
        let board_id = before.boards[0].id;
        let alternate = before
            .materials
            .iter()
            .find(|material| material.id != before.boards[0].material_id)
            .unwrap()
            .id;
        app.board_material = Some(BoardMaterialDialog {
            focus_on_open: true,
            board_id,
            project_id: before.id,
            revision: before.revision,
            material_id: Some(before.boards[0].material_id),
            anchor: Anchor::Centre,
            error: None,
        });
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::install_fonts(&ctx);
        let frame = |app: &mut DesktopApp, events| {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.show_board_material(ui.ctx()),
            );
            let confirm = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.role() == egui::accesskit::Role::Button
                        && !node.is_disabled()
                        && node.label() == Some("Confirm"))
                    .then(|| {
                        let bounds = node.bounds()?;
                        Some(egui::pos2(
                            (bounds.x0 + bounds.x1) as f32 / 2.0,
                            (bounds.y0 + bounds.y1) as f32 / 2.0,
                        ))
                    })
                    .flatten()
                });
            output.drop_without_applying_deltas();
            confirm
        };
        assert!(frame(&mut app, vec![]).is_none());
        assert_eq!(app.editor.project(), &before);
        app.board_material.as_mut().unwrap().material_id = Some(alternate);
        let confirm = frame(&mut app, vec![]).expect("valid material confirmation");
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    Event::PointerMoved(confirm),
                    Event::PointerButton {
                        pos: confirm,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(app.board_material.is_none());
        assert!(!app.board_material_chrome.is_active());
        assert_eq!(app.editor.project().boards[0].material_id, alternate);
        assert_eq!(app.editor.project().revision, before.revision + 1);
        assert!(app.editor.undo().unwrap());
        assert_eq!(app.editor.project().boards, before.boards);
    }

    #[test]
    fn material_edit_modal_requires_explicit_preserve_or_apply_decision() {
        let mut app = navigation_app();
        let before = app.editor.project().clone();
        let material = before.materials[0].clone();
        app.material_edit = Some(MaterialEditDialog {
            focus_on_open: true,
            id: material.id,
            name: material.name.clone(),
            thickness: DimensionDraft {
                text: "21 mm".into(),
                consent: false,
            },
            grain: material.default_grain,
            anchor: Anchor::Centre,
            choice: None,
            error: None,
        });
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::install_fonts(&ctx);
        let frame = |app: &mut DesktopApp, events| {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.show_material_edit(ui.ctx()),
            );
            let confirm = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.role() == egui::accesskit::Role::Button
                        && !node.is_disabled()
                        && node.label() == Some("Confirm"))
                    .then(|| {
                        let bounds = node.bounds()?;
                        Some(egui::pos2(
                            (bounds.x0 + bounds.x1) as f32 / 2.0,
                            (bounds.y0 + bounds.y1) as f32 / 2.0,
                        ))
                    })
                    .flatten()
                });
            output.drop_without_applying_deltas();
            confirm
        };
        assert!(frame(&mut app, vec![]).is_none());
        assert_eq!(app.editor.project(), &before);
        app.material_edit.as_mut().unwrap().choice = Some(DependantChoice::Preserve);
        let confirm = frame(&mut app, vec![]).expect("choice enables confirmation");
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    Event::PointerMoved(confirm),
                    Event::PointerButton {
                        pos: confirm,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(app.material_edit.is_none());
        assert!(!app.material_edit_chrome.is_active());
        assert_eq!(
            app.editor.project().materials[0].default_thickness,
            Length::from_micrometres(21_000)
        );
        assert_eq!(app.editor.project().revision, before.revision + 1);
        assert!(app.editor.undo().unwrap());
        assert_eq!(app.editor.project().materials, before.materials);
        assert_eq!(app.editor.project().boards, before.boards);
    }

    #[test]
    fn cancelling_nested_material_restores_board_name_focus() {
        let ctx = egui::Context::default();
        let mut board = CreationDialog::board(None);
        board.name = "Unfinished shelf".into();
        board.length.text = "1/64 in".into();
        board.length.consent = true;
        board.width.text = "320 mm".into();
        board.grain_override = Some(BoardGrain::Width);
        let mut app = DesktopApp {
            suspended_board: Some(board),
            dialog: Some(CreationDialog::material()),
            ..Default::default()
        };
        let initial = app.editor.project().clone();
        let mut background = String::new();
        let mut draw = |input| {
            let mut background_id = None;
            ctx.run_ui(input, |ui| {
                background_id = Some(ui.text_edit_singleline(&mut background).id);
                app.show_dialog(ui.ctx());
            })
            .drop_without_applying_deltas();
            (
                background_id.unwrap(),
                ctx.memory(|memory| memory.focused()),
                app.dialog.as_ref().map(|dialog| dialog.kind),
            )
        };
        let (background_id, material_focus, _) = draw(RawInput::default());
        let (_, _, kind) = draw(RawInput {
            events: vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        });
        assert!(kind == Some(DialogKind::Board));
        let (_, board_focus, _) = draw(RawInput::default());
        assert!(board_focus.is_some() && board_focus != Some(background_id));
        assert_ne!(material_focus, board_focus);
        let restored = app.dialog.as_ref().unwrap();
        assert_eq!(restored.name, "Unfinished shelf");
        assert_eq!(restored.length.text, "1/64 in");
        assert!(restored.length.consent);
        assert_eq!(restored.width.text, "320 mm");
        assert_eq!(restored.grain_override, Some(BoardGrain::Width));
        assert_eq!(app.editor.project(), &initial);
    }

    #[test]
    fn keyboard_combo_selection_closes_popup_without_dismissing_dialog() {
        fn draw(
            ui: &mut egui::Ui,
            selected: &mut i32,
            option_response: &mut Option<egui::Response>,
            popup_id: &mut Option<egui::Id>,
            changed: &mut bool,
            cancel: &mut bool,
        ) {
            *cancel = dialog_escape(ui.ctx());
            egui::Window::new("Edit dimensions").show(ui.ctx(), |ui| {
                let response = egui::ComboBox::from_id_salt("axis")
                    .selected_text(format!("{selected}"))
                    .show_ui(ui, |ui| {
                        for index in 0..2 {
                            let response = combo_option(ui, selected, index, format!("{index}"));
                            if index == 1 {
                                *changed = response.changed();
                                *option_response = Some(response);
                            }
                        }
                    });
                *popup_id = Some(response.response.id.with("popup"));
            });
        }
        let ctx = egui::Context::default();
        let mut selected = 0;
        let mut option_response = None;
        let mut popup_id = None;
        let mut changed = false;
        let mut cancel = false;
        ctx.run_ui(RawInput::default(), |ui| {
            draw(
                ui,
                &mut selected,
                &mut option_response,
                &mut popup_id,
                &mut changed,
                &mut cancel,
            )
        })
        .drop_without_applying_deltas();
        egui::Popup::open_id(&ctx, popup_id.expect("combo id"));
        ctx.run_ui(RawInput::default(), |ui| {
            draw(
                ui,
                &mut selected,
                &mut option_response,
                &mut popup_id,
                &mut changed,
                &mut cancel,
            )
        })
        .drop_without_applying_deltas();
        option_response
            .take()
            .expect("popup option")
            .request_focus();

        let input = RawInput {
            events: vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            draw(
                ui,
                &mut selected,
                &mut option_response,
                &mut popup_id,
                &mut changed,
                &mut cancel,
            )
        })
        .drop_without_applying_deltas();
        assert_eq!(selected, 1);
        assert!(changed);
        assert!(!egui::Popup::is_id_open(&ctx, popup_id.unwrap()));
        assert!(!cancel);

        // Selecting the already-active item should close the menu without reporting a change.
        egui::Popup::open_id(&ctx, popup_id.unwrap());
        ctx.run_ui(RawInput::default(), |ui| {
            draw(
                ui,
                &mut selected,
                &mut option_response,
                &mut popup_id,
                &mut changed,
                &mut cancel,
            )
        })
        .drop_without_applying_deltas();
        option_response
            .take()
            .expect("popup option")
            .request_focus();
        ctx.run_ui(
            RawInput {
                events: vec![Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                draw(
                    ui,
                    &mut selected,
                    &mut option_response,
                    &mut popup_id,
                    &mut changed,
                    &mut cancel,
                )
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(selected, 1);
        assert!(!changed);
        assert!(!egui::Popup::is_id_open(&ctx, popup_id.unwrap()));

        egui::Popup::open_id(&ctx, popup_id.unwrap());
        ctx.run_ui(
            RawInput {
                events: vec![Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                draw(
                    ui,
                    &mut selected,
                    &mut option_response,
                    &mut popup_id,
                    &mut changed,
                    &mut cancel,
                )
            },
        )
        .drop_without_applying_deltas();
        assert!(!cancel);
        assert!(!egui::Popup::is_id_open(&ctx, popup_id.unwrap()));
    }
}
