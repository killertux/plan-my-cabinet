// Production code states its invariants with `expect("why")` or handles the
// failure; a bare `unwrap` is reserved for tests.
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

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
use plan_my_cabinet::measurements::{Frame, Scope};
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
// Shared with the library (the PDF preview uses them); compiled once there.
use plan_my_cabinet::{icons, theme, theme_widgets};
// Desktop-only modules. Re-exported here so `crate::<module>` paths stay short.
mod app;
use app::{
    actions, assembly_ui, capture, command_palette, currency_ui, door_joint_ui, handoff_ui, hardware_ui, hinge_ui, kerf_confirmation_ui, modal_chrome, modals, optimization_ui, pending_navigation, placement_ui, project_ui, receipt_ui, recovery_cleanup_ui, sheet_ui, state, stock_ui, template_setup_ui, toasts, viewport, welcome_host, widget_gallery, workspace_shell, workspace_state,
};
// Types and helpers the split-out modules share with the rest of the app.
use app::board_dialogs::*;
use app::export_flow::*;
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

#[cfg(test)]
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

type ExportPreparationJob = (
    ExportPreparationKey,
    Receiver<Result<Arc<ReviewedPacket>, ReviewPreparationError>>,
    Arc<AtomicBool>,
);

struct DesktopApp {
    capture: Option<capture::Capture>,
    preferences: LocalPreferences,
    preferences_store: Option<PreferencesStore>,
    preferences_error: Option<String>,
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
    pending_action: Option<Request>,
    pending_project_command: Option<project_ui::PendingProjectCommand>,
    palette: command_palette::Palette,
    palette_relationship_pending: Option<Uuid>,
    localizer: Localizer,
    editor: ProjectEditor,
    /// The one form dialog that may be open.
    modals: modals::Modals,
    suspended_board: Option<CreationDialog>,
    selection: viewport::Selection,
    shell_estimate: Option<(
        (Uuid, u64),
        Result<
            plan_my_cabinet::cost_estimate::ProjectEstimate,
            plan_my_cabinet::cost_estimate::EstimateError,
        >,
    )>,
    project_files: project_ui::ProjectFiles,
    toasts: toasts::Toasts,
    hardware: state::HardwareState,
    cut_plan: state::CutPlanState,
    design: state::DesignState,
    chromes: state::DialogChromes,
    template: state::TemplateHost,
    settings: state::SettingsHost,
    handoff: state::HandoffState,
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
            camera: viewport::Camera::default(),
            session: WorkspaceSession::new(editor.project()),
            open_drawer: None,
            controls_horizontal_scroll: [0.0; 5],
            inspector_scroll: [egui::Vec2::ZERO; 5],
            navigation: NavigationGuard::default(),
            navigation_chrome: ModalChrome::new(egui::Id::new("pending-navigation"))
                .width(440.0)
                .icon(icons::Icon::Warning)
                .alert(),
            navigation_error: None,
            edit_drafts: EditDrafts::default(),
            pending_action: None,
            pending_project_command: None,
            palette: command_palette::Palette::default(),
            palette_relationship_pending: None,
            localizer: Localizer::new(Language::En),
            editor,
            modals: modals::Modals::default(),
            suspended_board: None,
            selection: viewport::Selection::default(),
            shell_estimate: None,
            project_files: project_ui::ProjectFiles::default(),
            toasts: toasts::Toasts::default(),
            hardware: state::HardwareState::default(),
            cut_plan: state::CutPlanState::default(),
            design: state::DesignState::default(),
            chromes: state::DialogChromes::default(),
            template: state::TemplateHost::default(),
            settings: state::SettingsHost::default(),
            handoff: state::HandoffState::default(),
        }
    }
}

impl DesktopApp {

    fn sync_scene_inspector(&mut self) {
        if !self.session.belongs_to(self.editor.project()) {
            self.navigation.clear();
            self.edit_drafts.clear();
            self.pending_action = None;
            self.pending_project_command = None;
            self.design.pending_pose_frame = None;
            self.navigation_error = None;
            self.palette_relationship_pending = None;
            self.open_drawer = None;
            self.controls_horizontal_scroll = [0.0; 5];
            self.inspector_scroll = [egui::Vec2::ZERO; 5];
            self.design.stock_snapshot = None;
        }
        self.session.retain_existing(self.editor.project());
        self.selection.retain_objects(self.editor.project());
        if self.design.scene_active_seen != self.selection.active {
            self.design.scene_active_seen = self.selection.active;
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
            .design
            .stock_snapshot
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            self.design.stock_snapshot = StockReadModel::build(project)
                .ok()
                .map(|model| (key, model));
        }
        let stock = &self.design.stock_snapshot.as_ref()?.1;
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
    fn show_measurement(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.localizer.text("measurement-heading"));
        ui.label(self.localizer.text("measurement-selection"));
        let mut scope_choice = self.design.measurement_scope;
        let mut frame_choice = self.design.measurement_frame;
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
            let frame_label = match self.design.measurement_frame {
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
        if scope_choice != self.design.measurement_scope {
            self.invoke_or_report(
                Request::new(A::SetMeasurementScope).argument(Argument::Scope(scope_choice)),
            );
        }
        if frame_choice != self.design.measurement_frame {
            self.invoke_or_report(
                Request::new(A::SetMeasurementFrame).argument(Argument::Frame(frame_choice)),
            );
        }
        if let Frame::Object(id) = self.design.measurement_frame
            && !self.editor.project().assemblies.iter().any(|a| a.id == id)
            && !self.editor.project().boards.iter().any(|b| b.id == id)
        {
            self.design.measurement_frame = Frame::World;
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
            self.design.measurement_scope,
            self.design.measurement_frame,
            &self.localizer,
        );
        ui.label(heading);
        ui.label(content);
    }

    fn other_modal_open(&self) -> bool {
        self.settings.open
            || self.settings.worked_examples
            || self.settings.cleanup.is_some()
            || (self.template.setup.is_some() && !self.template.guard_pending)
            || self.modals.is_open()
            || matches!(self.handoff.activity, Some(ExportActivity::Confirming(..)))
    }

    fn modal_open(&self) -> bool {
        self.external_modal_open() || self.cut_plan.optimizer.comparison_open()
    }

    fn external_modal_open(&self) -> bool {
        self.palette.open
            || self.navigation.pending().is_some()
            || self.other_modal_open()
            || self.cut_plan.repair.active()
            || self.hardware.door_motion.is_some()
            || self.project_files.blocking()
    }

    /// Door motion is the only mode open: no dialog, prompt, palette or repair.
    fn door_motion_only(&self) -> bool {
        self.hardware.door_motion.is_some()
            && !self.palette.open
            && self.navigation.pending().is_none()
            && !self.other_modal_open()
            && !self.cut_plan.repair.active()
            && !self.project_files.blocking()
            && !self.cut_plan.optimizer.comparison_open()
    }

    /// Mounted overlays block raw scene input even on their opening frame,
    /// before egui has established the new modal layer. Motion and repair are
    /// scene modes, not overlays; their hosts apply their own restrictions.
    fn blocking_surface_open(&self) -> bool {
        self.palette.open
            || self.navigation.pending().is_some()
            || self.other_modal_open()
            || self.project_files.blocking()
            || self.cut_plan.optimizer.comparison_open()
    }

    fn design_hud_available(&self) -> bool {
        self.session.active == Workspace::Design && self.open_drawer.is_none() && !self.modal_open()
    }

}

impl DesktopApp {

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
        // A panic must not lose committed work: write recovery now, then let
        // the panic continue to its normal report and exit.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.show_frame(ui)));
        if let Err(payload) = result {
            self.flush_recovery_after_panic();
            std::panic::resume_unwind(payload);
        }
        let area = ui
            .ctx()
            .content_rect()
            .with_max_y(ui.ctx().content_rect().bottom() - workspace_shell::STATUS_HEIGHT);
        self.toasts
            .show(ui.ctx(), area, &self.localizer.text("toast-close"));
        if let Some(capture) = &mut self.capture
            && capture.tick(ui.ctx())
        {
            self.project_files.allow_close = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl DesktopApp {
    fn show_frame(&mut self, ui: &mut egui::Ui) {
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
    }

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
                if config.empty_project {
                    fixture = Project::new("New cabinet", Currency::Brl);
                    plan_my_cabinet::material_presets::seed_defaults(&mut fixture, config.language);
                }
                app.editor = ProjectEditor::new(fixture).expect("validated reference fixture");
                app.session = WorkspaceSession::new(app.editor.project());
                app.session.active = config.workspace;
                if let Some(section) = config.settings {
                    app.settings.state.section = section;
                    app.settings.open = true;
                }
                if config.workspace == Workspace::Stock && !config.empty_project {
                    app.session.stock_piece =
                        Some(plan_my_cabinet::reference_fixture::WHITE_STOCK_ID);
                }
                if config.workspace == Workspace::Handoff {
                    let packet = Arc::new(
                        ReviewedPacket::prepare(
                            app.editor.project(),
                            app.handoff.mode,
                            app.export_settings(),
                            app.handoff.sections,
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
                    app.handoff.candidate = Some((key.clone(), Ok(Arc::clone(&packet))));
                    app.handoff.preparation = Some((key, Ok(packet)));
                    if let Some(page) = config.page {
                        app.handoff.preview.page = page.saturating_sub(1);
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
                    app.design.scene_active_seen = None;
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
                    app.design.move_tool.configure_capture_snap(mode);
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
#[path = "app/desktop_tests.rs"]
mod tests;
