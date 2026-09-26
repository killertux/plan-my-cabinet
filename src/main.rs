use eframe::{egui, egui_wgpu::WgpuSetup, wgpu};
use fluent_bundle::FluentArgs;
use plan_my_cabinet::allocation_diagnostics::{
    BoardDiagnostic, Status as AllocationStatus, diagnose,
};
use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
use plan_my_cabinet::board_dimensions::{
    BatchDimensionError, BoardDimension, BoardSelection, DimensionEditError, DimensionPreview,
    SelectionValue,
};
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::dimension_input::{InputError, Locale, format_length, parse_length};
use plan_my_cabinet::domain::{BoardGrain, Project, validate_grid_spacing};
use plan_my_cabinet::export::{
    ExportIssue, ExportMode, ExportSettings, ExportSnapshot, OutputError, Overwrite,
    PreparedExport, SheetIssue, ShopReadyBlocked, prepare_export, write_pdf_cancellable,
};
use plan_my_cabinet::first_fit::FirstFit;
use plan_my_cabinet::hardware_catalog::{self, SOURCE_URL};
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::material_changes::{
    AllocationConflict, ConflictReason, DependantChoice, MaterialChangeError,
    MaterialChangePreview, allocation_conflicts,
};
use plan_my_cabinet::measurements::{Frame, MeasurementError, Scope, measure};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::units::{
    Anchor, Conversion, Length, Pose, Quaternion, Unit, UnitError, dimension,
};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use uuid::Uuid;
mod assembly_ui;
mod door_joint_ui;
mod hardware_ui;
mod hinge_ui;
mod optimization_ui;
mod placement_ui;
mod project_ui;
mod sheet_ui;
mod stock_ui;
mod viewport;
use placement_ui::PlacementDialog;

fn unit_key(unit: Unit) -> &'static str {
    match unit {
        Unit::Mm => "unit-mm",
        Unit::Cm => "unit-cm",
        Unit::M => "unit-m",
        Unit::Inch => "unit-in",
        Unit::Foot => "unit-ft",
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

type ExportPreparationKey = (Uuid, u64, ExportMode, Language, Unit, String);
type ExportPreparationCache = (
    ExportPreparationKey,
    Result<PreparedExport, ShopReadyBlocked>,
);

enum ExportEvent {
    Selected(Option<PathBuf>),
    Finished(
        PathBuf,
        Box<ExportSnapshot>,
        ExportMode,
        Result<plan_my_cabinet::export::ExportRecord, OutputError>,
    ),
}

enum ExportActivity {
    Choosing(Project, ExportSettings, ExportMode),
    Confirming(PathBuf, Project, ExportSettings, ExportMode),
    Writing,
}

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
    focus_on_open: bool,
    kind: DialogKind,
    name: String,
    length: DimensionDraft,
    width: DimensionDraft,
    thickness: DimensionDraft,
    material_id: Option<Uuid>,
    grain: BoardGrain,
    error: bool,
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
            focus_on_open: true,
            kind: DialogKind::Board,
            name: String::new(),
            length: DimensionDraft::new(),
            width: DimensionDraft::new(),
            thickness: DimensionDraft::new(),
            material_id,
            grain: BoardGrain::Length,
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
    camera: viewport::Camera,
    move_tool: viewport::MoveTool,
    localizer: Localizer,
    editor: ProjectEditor,
    dialog: Option<CreationDialog>,
    suspended_board: Option<CreationDialog>,
    material_edit: Option<MaterialEditDialog>,
    board_material: Option<BoardMaterialDialog>,
    board_dimension: Option<BoardDimensionDialog>,
    batch_dimension: Option<BatchDialog>,
    placement: Option<PlacementDialog>,
    grid_dialog: Option<GridDialog>,
    stock_dialog: Option<stock_ui::StockDialog>,
    cut_fee_dialog: Option<String>,
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
    measurement_scope: Scope,
    measurement_frame: Frame,
    board_action_error: bool,
    first_fit_notice: Option<FirstFit>,
    material_conflicts: Vec<AllocationConflict>,
    allocation_diagnostics: Option<(sheet_ui::DiagnosticsKey, Vec<BoardDiagnostic>)>,
    export_mode: ExportMode,
    export_preparation: Option<ExportPreparationCache>,
    export_preparation_pending: Option<(
        ExportPreparationKey,
        Receiver<Result<PreparedExport, ShopReadyBlocked>>,
    )>,
    export_language: Language,
    export_units: Unit,
    export_activity: Option<ExportActivity>,
    export_events: Option<Receiver<ExportEvent>>,
    export_cancel: Option<Arc<AtomicBool>>,
    export_message: Option<String>,
    project_files: project_ui::ProjectFiles,
}

impl Default for DesktopApp {
    fn default() -> Self {
        Self {
            camera: viewport::Camera::default(),
            move_tool: viewport::MoveTool::default(),
            localizer: Localizer::new(Language::En),
            editor: ProjectEditor::new(Project::new("Cabinet", Currency::Brl))
                .expect("empty project"),
            dialog: None,
            suspended_board: None,
            material_edit: None,
            board_material: None,
            board_dimension: None,
            batch_dimension: None,
            placement: None,
            grid_dialog: None,
            stock_dialog: None,
            cut_fee_dialog: None,
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
            measurement_scope: Scope::Body,
            measurement_frame: Frame::World,
            board_action_error: false,
            first_fit_notice: None,
            material_conflicts: Vec::new(),
            allocation_diagnostics: None,
            export_mode: ExportMode::Draft,
            export_preparation: None,
            export_preparation_pending: None,
            export_language: Language::En,
            export_units: Unit::Mm,
            export_activity: None,
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
    fn show_measurement(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.localizer.text("measurement-heading"));
        ui.label(self.localizer.text("measurement-selection"));
        ui.add_enabled_ui(!self.modal_open(), |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.measurement_scope,
                    Scope::Body,
                    self.localizer.text("measurement-body"),
                );
                ui.selectable_value(
                    &mut self.measurement_scope,
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
                        &mut self.measurement_frame,
                        Frame::World,
                        self.localizer.text("placement-world"),
                    );
                    for a in &self.editor.project().assemblies {
                        combo_option(
                            ui,
                            &mut self.measurement_frame,
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
                            &mut self.measurement_frame,
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
        let scope = self.localizer.text(match self.measurement_scope {
            Scope::Body => "measurement-body",
            Scope::Overall => "measurement-overall",
        });
        let frame = match self.measurement_frame {
            Frame::World => self.localizer.text("placement-world"),
            Frame::Object(id) => self
                .editor
                .project()
                .assemblies
                .iter()
                .map(|a| (a.id, a.name.as_str()))
                .chain(
                    self.editor
                        .project()
                        .boards
                        .iter()
                        .map(|b| (b.id, b.name.as_str())),
                )
                .find(|(object, _)| *object == id)
                .map(|(_, name)| format!("{name} ({})", &id.to_string()[..8]))
                .unwrap_or_default(),
        };
        ui.label(format!(
            "{scope} · {}: {frame}",
            self.localizer.text("measurement-frame")
        ));
        match measure(
            self.editor.project(),
            &ids,
            self.measurement_scope,
            self.measurement_frame,
        ) {
            Ok(result) => {
                let locale = if self.localizer.language() == Language::En {
                    Locale::En
                } else {
                    Locale::PtBr
                };
                let numbers = result.dimensions_mm.map(|v| {
                    // Spatial bounds are f64; formatting is presentation only, never manufacturing quantization.
                    if locale == Locale::PtBr {
                        format!("{v:.3}").replace('.', ",")
                    } else {
                        format!("{v:.3}")
                    }
                });
                ui.label(format!(
                    "X × Y × Z: {} × {} × {} mm",
                    numbers[0], numbers[1], numbers[2]
                ));
                ui.small(format!(
                    "{}: {} · {}: {}",
                    self.localizer.text("board-kind"),
                    result.board_count,
                    self.localizer.text("hardware-kind"),
                    result.hardware_count
                ));
            }
            Err(MeasurementError::EmptySelection) => {
                ui.label(self.localizer.text("measurement-empty"));
            }
            Err(MeasurementError::UndimensionedHardware(_)) => {
                ui.label(self.localizer.text("measurement-unknown-hardware"));
            }
            Err(_) => {
                ui.label(self.localizer.text("measurement-invalid"));
            }
        }
    }

    fn other_modal_open(&self) -> bool {
        self.dialog.is_some()
            || self.material_edit.is_some()
            || self.board_material.is_some()
            || self.board_dimension.is_some()
            || self.batch_dimension.is_some()
            || self.placement.is_some()
            || self.grid_dialog.is_some()
            || self.stock_dialog.is_some()
            || self.cut_fee_dialog.is_some()
            || self.assembly_dialog.is_some()
            || self.hardware_dialog.is_some()
            || self.hinge_dialog.is_some()
            || self.door_dialog.is_some()
            || self.removal_dialog.is_some()
    }

    fn modal_open(&self) -> bool {
        self.other_modal_open()
            || self.sheet_repair.active()
            || self.door_motion.is_some()
            || self.project_files.blocking()
    }

    fn show_grid_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.grid_dialog.take() else {
            return;
        };
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let current = self.editor.project().id == draft.project_id
            && self.editor.project().revision == draft.revision;
        let mut cancel = false;
        let mut confirm = false;
        let mut proposal = None;
        let modal = egui::Modal::new(egui::Id::new("grid-spacing-dialog")).show(ctx, |ui| {
            ui.heading(self.localizer.text(if draft.kerf {
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
            if draft.focus_on_open {
                combo.response.request_focus();
            }
            ui.horizontal(|ui| {
                ui.label(self.localizer.text(if draft.kerf {
                    "cutting-kerf"
                } else {
                    "grid-spacing"
                }));
                if ui.text_edit_singleline(&mut draft.value.text).changed() {
                    draft.value.consent = false;
                    draft.error = false;
                }
            });
            let unchanged = draft.value.text == format_length(draft.original, Unit::Mm, locale, 3);
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
            ui.horizontal(|ui| {
                cancel = ui.button(self.localizer.text("cancel")).clicked();
                confirm = ui
                    .add_enabled(
                        current && proposal.is_some(),
                        egui::Button::new(self.localizer.text("confirm")),
                    )
                    .clicked();
            });
        });
        draft.focus_on_open = false;
        if cancel || modal.should_close() {
            return;
        }
        if confirm && let Some(value) = proposal {
            if (if draft.kerf {
                self.editor.set_cutting_kerf(value)
            } else {
                self.editor.set_grid_spacing(value)
            })
            .is_ok()
            {
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
        let mut cancel = false;
        let mut confirm = false;
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
        let modal = egui::Modal::new(egui::Id::new("batch-dimension-dialog")).show(ctx, |ui| {
            ui.heading(self.localizer.text("board-batch-edit"));
            let mut summary = None;
            if current {
                summary = self.editor.selected_boards(&selection).ok();
            }
            let axis = egui::ComboBox::from_label(self.localizer.text("board-local-dimension"))
                .selected_text(self.localizer.text(match draft.dimension {
                    BoardDimension::Length => "board-length",
                    BoardDimension::Width => "board-width",
                    BoardDimension::Thickness => "board-thickness",
                }))
                .show_ui(ui, |ui| {
                    for (axis, key) in [
                        (BoardDimension::Length, "board-length"),
                        (BoardDimension::Width, "board-width"),
                        (BoardDimension::Thickness, "board-thickness"),
                    ] {
                        if combo_option(ui, &mut draft.dimension, axis, self.localizer.text(key))
                            .changed()
                        {
                            draft.value = DimensionDraft::new();
                            draft.error = None;
                        }
                    }
                });
            if draft.focus_on_open {
                axis.response.request_focus();
            }
            if let Some(summary) = &summary {
                let text = match summary.dimensions[draft.dimension.axis()] {
                    SelectionValue::Uniform(value) => format_length(value, Unit::Mm, locale, 3),
                    SelectionValue::Mixed => self.localizer.text("board-mixed"),
                };
                ui.label(format!(
                    "{}: {text}",
                    self.localizer.text("board-current-value")
                ));
            }
            let valid = dimension_field(
                ui,
                &self.localizer,
                "board-dimension-preview",
                &mut draft.value,
                unit,
            );
            ui.label(self.localizer.text("board-affected"));
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for (id, anchor) in &mut draft.anchors {
                        if let Some(board) = project.boards.iter().find(|b| b.id == *id) {
                            ui.label(format!("{} · {}", board.name, id));
                            egui::ComboBox::from_id_salt(("batch-anchor", id))
                                .selected_text(self.localizer.text(match anchor {
                                    Anchor::Start => "anchor-start",
                                    Anchor::Centre => "anchor-centre",
                                    Anchor::End => "anchor-end",
                                }))
                                .show_ui(ui, |ui| {
                                    for (value, key) in [
                                        (Anchor::Start, "anchor-start"),
                                        (Anchor::Centre, "anchor-centre"),
                                        (Anchor::End, "anchor-end"),
                                    ] {
                                        combo_option(ui, anchor, value, self.localizer.text(key));
                                    }
                                });
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
                    Ok(result) => preview = Some(result),
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
            ui.horizontal(|ui| {
                if ui.button(self.localizer.text("cancel")).clicked() {
                    cancel = true;
                }
                if ui
                    .add_enabled(
                        preview.is_some(),
                        egui::Button::new(self.localizer.text("confirm")),
                    )
                    .clicked()
                {
                    confirm = true;
                }
            });
            if confirm && let Some(preview) = preview {
                accepted_preview = Some(preview);
            }
        });
        draft.focus_on_open = false;
        cancel |= modal.should_close();
        if cancel {
            return;
        }
        if let Some(preview) = accepted_preview {
            match self.editor.edit_batch_board_dimension(preview) {
                Ok(conflicts) => self.material_conflicts = conflicts,
                Err(plan_my_cabinet::commands::EditError::Command(error)) => {
                    draft.error = Some(error);
                    confirm = false;
                }
                Err(_) => {
                    draft.error = Some(BatchDimensionError::StalePreview);
                    confirm = false;
                }
            }
        }
        if confirm {
            return;
        }
        self.batch_dimension = Some(draft);
    }

    fn show_board_dimension(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.board_dimension.take() else {
            return;
        };
        let mut cancel = false;
        let mut confirm = false;
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
        let modal = egui::Modal::new(egui::Id::new("board-dimension-dialog")).show(ctx, |ui| {
            ui.heading(self.localizer.text("board-edit-dimension"));
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
            ui.horizontal(|ui| {
                if ui.button(self.localizer.text("cancel")).clicked() {
                    cancel = true;
                }
                if ui
                    .add_enabled(
                        preview.is_some(),
                        egui::Button::new(self.localizer.text("confirm")),
                    )
                    .clicked()
                {
                    confirm = true;
                }
            });
        });
        draft.focus_on_open = false;
        cancel |= modal.should_close();
        if cancel {
            return;
        }
        if confirm && let Some(preview) = preview {
            match self.editor.edit_board_dimension(preview) {
                Ok(conflicts) => {
                    self.material_conflicts = conflicts;
                    return;
                }
                Err(plan_my_cabinet::commands::EditError::Command(error)) => {
                    draft.error = Some(error)
                }
                Err(_) => draft.error = Some(DimensionEditError::StalePreview),
            }
        }
        self.board_dimension = Some(draft);
    }

    fn show_board_material(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.board_material.take() else {
            return;
        };
        let mut cancel = false;
        let mut confirm = false;
        let project = self.editor.project();
        let current = project.id == draft.project_id && project.revision == draft.revision;
        let board = project.boards.iter().find(|b| b.id == draft.board_id);
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let modal = egui::Modal::new(egui::Id::new("board-material-dialog")).show(ctx, |ui| {
            ui.heading(self.localizer.text("board-assign-material"));
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
            ui.horizontal(|ui| {
                if ui.button(self.localizer.text("cancel")).clicked() {
                    cancel = true;
                }
                if ui
                    .add_enabled(valid, egui::Button::new(self.localizer.text("confirm")))
                    .clicked()
                {
                    confirm = true;
                }
            });
        });
        draft.focus_on_open = false;
        cancel |= modal.should_close();
        if cancel {
            return;
        }
        if confirm
            && current
            && let Some(material_id) = draft.material_id
        {
            match self
                .editor
                .assign_board_material(draft.board_id, material_id, draft.anchor)
            {
                Ok(conflicts) => {
                    self.material_conflicts = conflicts;
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
            return;
        };
        let mut cancel = false;
        let mut confirm = false;
        let unit = self.editor.project().display_unit;
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let mut preview: Option<MaterialChangePreview> = None;
        let modal = egui::Modal::new(egui::Id::new("material-edit-dialog")).show(ctx, |ui| {
            ui.heading(self.localizer.text("material-edit"));
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
                if matches!(draft.choice, Some(DependantChoice::ApplyAll))
                    && proposal.affected.iter().any(|b| b.apply_error.is_some())
                {
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
            ui.horizontal(|ui| {
                if ui.button(self.localizer.text("cancel")).clicked() {
                    cancel = true;
                }
                if ui
                    .add_enabled(
                        preview.is_some() && draft.choice.is_some(),
                        egui::Button::new(self.localizer.text("confirm")),
                    )
                    .clicked()
                {
                    confirm = true;
                }
            });
        });
        draft.focus_on_open = false;
        cancel |= modal.should_close();
        if cancel {
            return;
        }
        if confirm && let (Some(preview), Some(choice)) = (preview, draft.choice.clone()) {
            match self.editor.apply_material_change(preview, choice) {
                Ok(conflicts) => {
                    self.material_conflicts = conflicts;
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
            return;
        };
        // Enter activates a focused popup option before it can submit the form.
        let enter =
            ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !egui::Popup::is_any_open(ctx);
        let mut cancel = false;
        let mut confirm = false;
        let mut create_material = false;
        let title = self.localizer.text(match draft.kind {
            DialogKind::Board => "board-new",
            DialogKind::Material => "material-new",
        });
        let modal_id = match draft.kind {
            DialogKind::Board => "board-creation-dialog",
            DialogKind::Material => "material-creation-dialog",
        };
        let modal = egui::Modal::new(egui::Id::new(modal_id)).show(ctx, |ui| {
            ui.heading(title);
            ui.horizontal(|ui| {
                ui.label(self.localizer.text(if draft.kind == DialogKind::Board {
                    "board-name"
                } else {
                    "material-name"
                }));
                let name = ui.text_edit_singleline(&mut draft.name);
                if draft.focus_on_open {
                    name.request_focus();
                }
            });
            let unit = self.editor.project().display_unit;
            let valid = if draft.kind == DialogKind::Board {
                ui.label(self.localizer.text("board-input-hint"));
                let mut material_id = draft.material_id;
                egui::ComboBox::from_label(self.localizer.text("material"))
                    .selected_text(
                        self.editor
                            .project()
                            .materials
                            .iter()
                            .find(|m| Some(m.id) == material_id)
                            .map(|m| m.name.as_str())
                            .unwrap_or("—"),
                    )
                    .show_ui(ui, |ui| {
                        for material in &self.editor.project().materials {
                            combo_option(ui, &mut material_id, Some(material.id), &material.name);
                        }
                    });
                draft.material_id = material_id;
                if ui.button(self.localizer.text("material-new")).clicked() {
                    create_material = true;
                }
                let material = material_id
                    .and_then(|id| self.editor.project().materials.iter().find(|m| m.id == id));
                if let Some(m) = material {
                    let locale = if self.localizer.language() == Language::En {
                        Locale::En
                    } else {
                        Locale::PtBr
                    };
                    ui.label(format!(
                        "{}: {} — {}",
                        self.localizer.text("board-effective"),
                        m.name,
                        format_length(m.default_thickness, Unit::Mm, locale, 3)
                    ));
                    ui.label(format!(
                        "{}: {}",
                        self.localizer.text("board-effective-grain"),
                        self.localizer.text(grain_key(m.default_grain))
                    ));
                } else {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text("error-material-missing"),
                    );
                }
                let length =
                    dimension_field(ui, &self.localizer, "board-length", &mut draft.length, unit);
                let width =
                    dimension_field(ui, &self.localizer, "board-width", &mut draft.width, unit);
                material.is_some() && length && width
            } else {
                ui.label(self.localizer.text("board-input-hint"));
                let thickness = dimension_field(
                    ui,
                    &self.localizer,
                    "board-thickness",
                    &mut draft.thickness,
                    unit,
                );
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
                thickness
            };
            if draft.error {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    self.localizer.text("error-create"),
                );
            }
            ui.horizontal(|ui| {
                if ui.button(self.localizer.text("cancel")).clicked() {
                    cancel = true;
                }
                if ui
                    .add_enabled(valid, egui::Button::new(self.localizer.text("confirm")))
                    .clicked()
                    || (enter && valid && !egui::Popup::is_any_open(ctx))
                {
                    confirm = true;
                }
            });
        });
        draft.focus_on_open = false;
        cancel |= modal.should_close();
        if cancel {
            if draft.kind == DialogKind::Material {
                self.dialog = self.suspended_board.take();
                if let Some(board) = &mut self.dialog {
                    board.focus_on_open = true;
                }
            }
            return;
        }
        if create_material {
            self.suspended_board = Some(draft);
            self.dialog = Some(CreationDialog::material());
            return;
        }
        if confirm {
            let result = match draft.kind {
                DialogKind::Material => self
                    .editor
                    .create_material(NewMaterial {
                        name: draft.name.clone(),
                        thickness: draft
                            .thickness
                            .value(self.editor.project().display_unit)
                            .expect("validated"),
                        grain: draft.grain,
                    })
                    .inspect(|&id| {
                        draft.material_id = Some(id);
                    })
                    .map(|id| (id, None)),
                DialogKind::Board => self
                    .editor
                    .create_board_with_fit(NewBoard {
                        name: draft.name.clone(),
                        material_id: draft.material_id.expect("validated"),
                        length: draft
                            .length
                            .value(self.editor.project().display_unit)
                            .expect("validated"),
                        width: draft
                            .width
                            .value(self.editor.project().display_unit)
                            .expect("validated"),
                        pose: Pose::new([0.0; 3], Quaternion::IDENTITY).expect("identity pose"),
                    })
                    .map(|(id, fit)| (id, Some(fit))),
            };
            if let Ok((_, fit)) = result {
                self.first_fit_notice = fit;
                if draft.kind == DialogKind::Material
                    && let Some(mut board) = self.suspended_board.take()
                {
                    board.material_id = draft.material_id;
                    board.focus_on_open = true;
                    self.dialog = Some(board);
                }
                return;
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
        project: Project,
        settings: ExportSettings,
        mode: ExportMode,
        overwrite: Overwrite,
    ) {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.export_cancel = Some(cancel.clone());
        self.export_events = Some(rx);
        self.export_activity = Some(ExportActivity::Writing);
        std::thread::spawn(move || {
            // Revalidation, witness reconstruction, rendering and disk I/O are
            // entirely off the egui thread and use only committed snapshot data.
            let result = prepare_export(&project, settings, mode);
            let (snapshot, result) = match result {
                Ok(prepared) => {
                    let snapshot = prepared.snapshot.clone();
                    let result = write_pdf_cancellable(&prepared, Some(&path), overwrite, || {
                        cancel.load(Ordering::Relaxed)
                    });
                    (snapshot, result)
                }
                Err(_) => (
                    plan_my_cabinet::export::ExportSnapshot::new(&project, settings)
                        .expect("validated project"),
                    Err(OutputError::PreparationBlocked),
                ),
            };
            let _ = tx.send(ExportEvent::Finished(
                path,
                Box::new(snapshot),
                mode,
                result,
            ));
        });
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
                    let Some(ExportActivity::Choosing(project, settings, mode)) =
                        self.export_activity.take()
                    else {
                        return;
                    };
                    match path {
                        None => self.export_message = Some(self.localizer.text("export-cancelled")),
                        Some(path) if path.symlink_metadata().is_ok() => {
                            self.export_activity =
                                Some(ExportActivity::Confirming(path, project, settings, mode))
                        }
                        Some(path) => {
                            self.start_pdf_write(path, project, settings, mode, Overwrite::Decline)
                        }
                    }
                }
                ExportEvent::Finished(path, snapshot, mode, result) => {
                    self.export_activity = None;
                    self.export_cancel = None;
                    let mut args = FluentArgs::new();
                    args.set("path", path.display().to_string());
                    args.set("revision", snapshot.revision() as i64);
                    self.export_message = Some(match result {
                        Ok(receipt) => {
                            match self.editor.record_completed_export(
                                &snapshot,
                                &path,
                                receipt.completed_unix_ms,
                                &receipt.file_sha256,
                            ) {
                                Ok(()) => {
                                    let stale = self.editor.project().revision
                                        != snapshot.revision()
                                        || plan_my_cabinet::export::fingerprint(
                                            self.editor.project(),
                                        ) != *snapshot.fingerprint();
                                    self.localizer.format(
                                        if stale {
                                            "export-saved-stale"
                                        } else {
                                            "export-saved"
                                        },
                                        Some(&args),
                                    )
                                }
                                Err(plan_my_cabinet::export::ExportError::WrongProject) => {
                                    self.localizer.format("export-saved-stale", Some(&args))
                                }
                                Err(_) => self.localizer.text("export-verify-failed"),
                            }
                        }
                        Err(OutputError::OverwriteRequired) => {
                            self.export_activity = Some(ExportActivity::Confirming(
                                path.clone(),
                                snapshot.project().clone(),
                                snapshot.settings(),
                                mode,
                            ));
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

    fn show_export_preparation(&mut self, ui: &mut egui::Ui) {
        self.poll_pdf_export(ui.ctx());
        ui.heading(self.localizer.text("export-preparation"));
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut self.export_mode,
                ExportMode::Draft,
                self.localizer.text("export-draft"),
            );
            ui.radio_value(
                &mut self.export_mode,
                ExportMode::ShopReady,
                self.localizer.text("export-shop-ready"),
            );
        });
        ui.horizontal(|ui| {
            ui.label(self.localizer.text("export-language"));
            ui.radio_value(
                &mut self.export_language,
                Language::En,
                self.localizer.text("language-en"),
            );
            ui.radio_value(
                &mut self.export_language,
                Language::PtBr,
                self.localizer.text("language-pt-br"),
            );
        });
        egui::ComboBox::from_label(self.localizer.text("export-output-units"))
            .selected_text(self.localizer.text(unit_key(self.export_units)))
            .show_ui(ui, |ui| {
                for unit in [Unit::Mm, Unit::Cm, Unit::M, Unit::Inch, Unit::Foot] {
                    ui.selectable_value(
                        &mut self.export_units,
                        unit,
                        self.localizer.text(unit_key(unit)),
                    );
                }
            });
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
                let _ = self.editor.confirm_shop_kerf();
            }
        }
        let project = self.editor.project();
        let key = (
            project.id,
            project.revision,
            self.export_mode,
            self.export_language,
            self.export_units,
            plan_my_cabinet::export::fingerprint(project).packet,
        );
        if self
            .export_preparation
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
            && self
                .export_preparation_pending
                .as_ref()
                .is_none_or(|(pending, _)| *pending != key)
        {
            let snapshot = project.clone();
            let settings = ExportSettings {
                language: self.export_language,
                units: self.export_units,
            };
            let mode = self.export_mode;
            let (tx, rx) = mpsc::channel();
            self.export_preparation_pending = Some((key.clone(), rx));
            std::thread::spawn(move || {
                let _ = tx.send(prepare_export(&snapshot, settings, mode));
            });
        }
        if let Some((pending, rx)) = &self.export_preparation_pending
            && let Ok(result) = rx.try_recv()
        {
            self.export_preparation = Some((pending.clone(), result));
            self.export_preparation_pending = None;
        }
        if self
            .export_preparation
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            ui.label(self.localizer.text("export-working"));
            ui.ctx().request_repaint_after(Duration::from_millis(50));
            return;
        }
        let prepared = &self.export_preparation.as_ref().expect("prepared").1;
        let (issues, notices, ready) = match &prepared {
            Ok(plan) => (&plan.wood_issues[..], &plan.notices[..], true),
            Err(blocked) => (&blocked.issues[..], &[][..], false),
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
        for issue in issues.iter().chain(notices) {
            match issue {
                ExportIssue::Board {
                    id,
                    name,
                    stock_id,
                    reasons,
                } => {
                    ui.label(format!(
                        "{name} ({id}) · {} · {}",
                        stock_id.map_or_else(|| "—".into(), |s| s.to_string()),
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
                    ui.label(format!("{name} ({id}): {}", self.localizer.text(key)));
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
                            |id| id.to_string()
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
        }
        if ui
            .add_enabled(
                ready && self.export_activity.is_none() && !self.modal_open(),
                egui::Button::new(self.localizer.text("export-choose")),
            )
            .clicked()
        {
            let (tx, rx) = mpsc::channel();
            self.export_events = Some(rx);
            self.export_activity = Some(ExportActivity::Choosing(
                self.editor.project().clone(),
                ExportSettings {
                    language: self.export_language,
                    units: self.export_units,
                },
                self.export_mode,
            ));
            // Construct on the UI thread so AppKit identifies the owning
            // window; only polling/waiting happens on the worker thread.
            let picker = rfd::AsyncFileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name("workshop.pdf")
                .save_file();
            std::thread::spawn(move || {
                let path = pollster::block_on(picker).map(|handle| handle.path().to_path_buf());
                let _ = tx.send(ExportEvent::Selected(path));
            });
        }
        match self.export_activity.take() {
            Some(ExportActivity::Confirming(path, project, settings, mode)) => {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!(
                        "{} {}",
                        self.localizer.text("export-overwrite"),
                        path.display()
                    ),
                );
                if ui.button(self.localizer.text("export-replace")).clicked() {
                    self.start_pdf_write(path, project, settings, mode, Overwrite::Confirm);
                } else if ui.button(self.localizer.text("cancel")).clicked() {
                    self.export_message = Some(self.localizer.text("export-cancelled"));
                } else {
                    self.export_activity =
                        Some(ExportActivity::Confirming(path, project, settings, mode));
                }
            }
            activity => self.export_activity = activity,
        }
        if let Some(activity) = &self.export_activity {
            ui.label(self.localizer.text(match activity {
                ExportActivity::Choosing(..) => "export-choosing",
                _ => "export-working",
            }));
            if matches!(activity, ExportActivity::Writing)
                && ui.button(self.localizer.text("cancel")).clicked()
                && let Some(cancel) = &self.export_cancel
            {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        if let Some(message) = &self.export_message {
            ui.label(message);
        }
        ui.small(self.localizer.text("export-preview-only"));
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
        if self
            .allocation_diagnostics
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            self.allocation_diagnostics = Some((key, diagnose(&project)));
        }
        let diagnostics = self.allocation_diagnostics.as_ref().unwrap().1.clone();
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
                ui.horizontal(|ui| {
                    if ui.button(self.localizer.text("global-locate")).clicked() {
                        self.selection.choose(Some(board.id), false);
                        self.selection.reveal(&project, board.id);
                    }
                    if ui
                        .add_enabled(
                            !self.modal_open() || self.sheet_repair.active(),
                            egui::Button::new(self.localizer.text("global-repair")),
                        )
                        .clicked()
                    {
                        self.selection.choose(Some(board.id), false);
                        self.selection.reveal(&project, board.id);
                        self.sheet_repair.begin(
                            &mut self.editor,
                            &self.selection,
                            if self.localizer.language() == Language::En {
                                Locale::En
                            } else {
                                Locale::PtBr
                            },
                        );
                    }
                    if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("stock-new")),
                        )
                        .clicked()
                    {
                        self.stock_dialog = Some(stock_ui::StockDialog::new(self.editor.project()));
                    }
                });
            });
        }
    }

    fn show_workspace(&mut self, ui: &mut egui::Ui) {
        self.tick_project_files(ui.ctx());
        egui::CentralPanel::default().show(ui, |ui| {
            let available = ui.available_size();
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(340.0, available.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("project-controls-scroll")
                            .show(ui, |ui| {
                                ui.heading(self.localizer.text("app-title"));
                                ui.label(self.localizer.text("viewport-heading"));
                                let current = self.localizer.language();
                                let mut selected = current;
                                egui::ComboBox::from_label(self.localizer.text("ui-language"))
                                    .selected_text(self.localizer.text(match current {
                                        Language::En => "language-en",
                                        Language::PtBr => "language-pt-br",
                                    }))
                                    .show_ui(ui, |ui| {
                                        combo_option(
                                            ui,
                                            &mut selected,
                                            Language::En,
                                            self.localizer.text("language-en"),
                                        );
                                        combo_option(
                                            ui,
                                            &mut selected,
                                            Language::PtBr,
                                            self.localizer.text("language-pt-br"),
                                        );
                                    });
                                self.localizer.set_language(selected);
                                self.show_project_controls(ui);
                                ui.label(
                                    self.localizer
                                        .text(self.editor.last_export_status().label_key()),
                                );
                                ui.separator();
                                ui.label(format!(
                                    "{}: {}",
                                    self.localizer.text("grid-spacing"),
                                    format_length(
                                        self.editor.project().grid_spacing,
                                        Unit::Mm,
                                        if selected == Language::En {
                                            Locale::En
                                        } else {
                                            Locale::PtBr
                                        },
                                        3
                                    )
                                ));
                                if ui
                                    .add_enabled(
                                        !self.modal_open(),
                                        egui::Button::new(self.localizer.text("grid-edit")),
                                    )
                                    .clicked()
                                {
                                    self.grid_dialog = Some(GridDialog::open(
                                        self.editor.project(),
                                        if selected == Language::En {
                                            Locale::En
                                        } else {
                                            Locale::PtBr
                                        },
                                    ));
                                }
                                ui.label(format!(
                                    "{}: {}",
                                    self.localizer.text("cutting-kerf"),
                                    format_length(
                                        self.editor.project().cutting_kerf,
                                        Unit::Mm,
                                        if selected == Language::En {
                                            Locale::En
                                        } else {
                                            Locale::PtBr
                                        },
                                        3
                                    )
                                ));
                                ui.small(self.localizer.text("cutting-kerf-hint"));
                                if ui
                                    .add_enabled(
                                        !self.modal_open(),
                                        egui::Button::new(self.localizer.text("cutting-kerf-edit")),
                                    )
                                    .clicked()
                                {
                                    self.grid_dialog = Some(GridDialog::cutting_kerf(
                                        self.editor.project(),
                                        if selected == Language::En {
                                            Locale::En
                                        } else {
                                            Locale::PtBr
                                        },
                                    ));
                                }
                                if let Some(notice) = self.first_fit_notice {
                                    ui.label(self.localizer.text(match notice {
                                        FirstFit::Allocated(_) => "first-fit-allocated",
                                        FirstFit::NoFit => "first-fit-no-fit",
                                        FirstFit::SearchExhausted => "first-fit-exhausted",
                                    }));
                                }
                                ui.collapsing(self.localizer.text("catalog-heading"), |ui| {
                                    ui.label(self.localizer.text("catalog-builtin"));
                                    ui.small(self.localizer.text("catalog-limits"));
                                    ui.hyperlink_to(
                                        self.localizer.text("catalog-source"),
                                        SOURCE_URL,
                                    );
                                    if ui
                                        .add_enabled(
                                            !self.modal_open(),
                                            egui::Button::new(self.localizer.text("catalog-add")),
                                        )
                                        .clicked()
                                    {
                                        let _ = hardware_catalog::add_builtin(&mut self.editor);
                                    }
                                    let entries: Vec<_> = self
                                        .editor
                                        .project()
                                        .catalog
                                        .iter()
                                        .map(|entry| {
                                            (
                                                entry.id,
                                                entry.name.clone(),
                                                hardware_catalog::is_verified(entry),
                                            )
                                        })
                                        .collect();
                                    for (id, name, verified) in entries {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(format!("{name} ({id})"));
                                            ui.label(self.localizer.text(if verified {
                                                "catalog-verified"
                                            } else {
                                                "catalog-unverified"
                                            }));
                                            if ui
                                                .add_enabled(
                                                    !self.modal_open(),
                                                    egui::Button::new(
                                                        self.localizer.text("catalog-update"),
                                                    ),
                                                )
                                                .clicked()
                                            {
                                                self.catalog_update_notice = Some(match hardware_catalog::update_from_builtin_with_status(&mut self.editor, id) {
                                                    Ok((_, statuses)) => {
                                                        let details = statuses.iter().map(|status| {
                                                            let warnings = if status.issues.is_empty() {
                                                                self.localizer.text("catalog-verified")
                                                            } else {
                                                                status.issues.iter().map(|issue| self.localizer.text(hinge_ui::issue_key(issue))).collect::<Vec<_>>().join(", ")
                                                            };
                                                            format!("{}: {warnings}", status.id)
                                                        }).collect::<Vec<_>>().join("; ");
                                                        format!("{}: {} / {}. {details}", self.localizer.text("hinge-update-reviewed"), statuses.iter().filter(|s| s.issues.is_empty()).count(), statuses.len())
                                                    },
                                                    Err(_) => self.localizer.text("hinge-update-error"),
                                                });
                                            }
                                        });
                                    }
                                    if let Some(notice) = &self.catalog_update_notice {
                                        ui.label(notice);
                                    }
                                });
                                ui.separator();
                                if ui
                                    .add_enabled(
                                        !self.modal_open(),
                                        egui::Button::new(self.localizer.text("board-new")),
                                    )
                                    .clicked()
                                {
                                    self.dialog = Some(CreationDialog::board(
                                        self.editor.project().materials.first().map(|m| m.id),
                                    ));
                                }
                                if ui
                                    .add_enabled(
                                        !self.modal_open(),
                                        egui::Button::new(self.localizer.text("material-new")),
                                    )
                                    .clicked()
                                {
                                    self.dialog = Some(CreationDialog::material());
                                }
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(
                                            !self.modal_open() && self.editor.can_undo(),
                                            egui::Button::new(self.localizer.text("undo")),
                                        )
                                        .clicked()
                                    {
                                        let _ = self.editor.undo();
                                        self.material_conflicts =
                                            allocation_conflicts(self.editor.project());
                                    }
                                    if ui
                                        .add_enabled(
                                            !self.modal_open() && self.editor.can_redo(),
                                            egui::Button::new(self.localizer.text("redo")),
                                        )
                                        .clicked()
                                    {
                                        let _ = self.editor.redo();
                                        self.material_conflicts =
                                            allocation_conflicts(self.editor.project());
                                    }
                                });
                                ui.heading(self.localizer.text("material-list"));
                                let mut modal_open = self.modal_open();
                                for material in &self.editor.project().materials {
                                    ui.horizontal(|ui| {
                                        ui.label(&material.name);
                                        if ui
                                            .add_enabled(
                                                !modal_open,
                                                egui::Button::new(
                                                    self.localizer.text("material-edit"),
                                                ),
                                            )
                                            .clicked()
                                        {
                                            self.material_edit = Some(MaterialEditDialog {
                                                focus_on_open: true,
                                                id: material.id,
                                                name: material.name.clone(),
                                                thickness: DimensionDraft {
                                                    text: format_length(
                                                        material.default_thickness,
                                                        Unit::Mm,
                                                        if self.localizer.language() == Language::En
                                                        {
                                                            Locale::En
                                                        } else {
                                                            Locale::PtBr
                                                        },
                                                        3,
                                                    ),
                                                    consent: false,
                                                },
                                                grain: material.default_grain,
                                                anchor: Anchor::Centre,
                                                choice: None,
                                                error: None,
                                            });
                                            modal_open = true;
                                        }
                                    });
                                }
                                self.show_stock_list(ui);
                                let blocked = self.modal_open();
                                self.optimizer
                                    .show(ui, &mut self.editor, &self.localizer, blocked);
                                self.show_allocation_issues(ui);
                                self.show_export_preparation(ui);
                                if !self.material_conflicts.is_empty() {
                                    ui.heading(self.localizer.text("material-post-conflicts"));
                                    for conflict in &self.material_conflicts {
                                        let name = self
                                            .editor
                                            .project()
                                            .boards
                                            .iter()
                                            .find(|b| b.id == conflict.board_id)
                                            .map(|b| b.name.as_str())
                                            .unwrap_or("—");
                                        ui.colored_label(
                                            egui::Color32::YELLOW,
                                            format!(
                                                "{}: {}",
                                                name,
                                                conflict_labels(&self.localizer, conflict)
                                            ),
                                        );
                                    }
                                }
                                ui.heading(self.localizer.text("board-list"));
                                self.selection.retain_objects(self.editor.project());
                                self.show_hierarchy(ui);
                                self.show_hardware_list(ui);
                                self.show_hinge_list(ui);
                                self.show_door_list(ui);
                                self.show_measurement(ui);
                                if ui
                                    .add_enabled(
                                        !self.modal_open() && !self.selection.ids.is_empty(),
                                        egui::Button::new(self.localizer.text("board-batch-edit")),
                                    )
                                    .clicked()
                                {
                                    let selection: Vec<_> = self
                                        .selection
                                        .ids
                                        .iter()
                                        .copied()
                                        .filter(|id| {
                                            self.editor.project().boards.iter().any(|b| b.id == *id)
                                        })
                                        .map(BoardSelection::Board)
                                        .collect();
                                    if let Ok(summary) = self.editor.selected_boards(&selection) {
                                        self.batch_dimension = Some(BatchDialog {
                                            focus_on_open: true,
                                            project_id: self.editor.project().id,
                                            revision: self.editor.project().revision,
                                            anchors: summary
                                                .board_ids
                                                .iter()
                                                .map(|id| (*id, Anchor::Centre))
                                                .collect(),
                                            ids: summary.board_ids,
                                            dimension: BoardDimension::Length,
                                            value: DimensionDraft::new(),
                                            error: None,
                                        });
                                    }
                                }
                                if self.board_action_error {
                                    ui.colored_label(
                                        egui::Color32::LIGHT_RED,
                                        self.localizer.text("error-board-duplicate"),
                                    );
                                }
                                let locale = if self.localizer.language() == Language::En {
                                    Locale::En
                                } else {
                                    Locale::PtBr
                                };
                                let mut grain_edit = None;
                                let mut duplicate = None;
                                let mut place_numeric = None;
                                let mut place_face = None;
                                for board in &self.editor.project().boards {
                                    ui.horizontal(|ui| {
                                        let mut selected = self.selection.ids.contains(&board.id);
                                        if ui
                                            .add_enabled(
                                                !modal_open,
                                                egui::Checkbox::new(&mut selected, ""),
                                            )
                                            .changed()
                                        {
                                            if selected {
                                                self.selection.ids.insert(board.id);
                                                self.selection.active = Some(board.id);
                                            } else {
                                                self.selection.ids.remove(&board.id);
                                                if self.selection.active == Some(board.id) {
                                                    self.selection.active =
                                                        self.selection.ids.iter().copied().min();
                                                }
                                            }
                                        }
                                        let label = if self.selection.active == Some(board.id) {
                                            format!("* {}", board.name)
                                        } else {
                                            board.name.clone()
                                        };
                                        if ui
                                            .add_enabled(
                                                !modal_open,
                                                egui::Button::new(label).selected(
                                                    self.selection.active == Some(board.id),
                                                ),
                                            )
                                            .clicked()
                                        {
                                            let additive = ui.input(|i| {
                                                i.modifiers.command || i.modifiers.shift
                                            });
                                            self.selection.choose(Some(board.id), additive);
                                        }
                                        if ui
                                            .add_enabled(
                                                !modal_open
                                                    && !self.editor.project().materials.is_empty(),
                                                egui::Button::new(
                                                    self.localizer.text("board-assign-material"),
                                                ),
                                            )
                                            .clicked()
                                        {
                                            self.board_material = Some(BoardMaterialDialog {
                                                focus_on_open: true,
                                                board_id: board.id,
                                                project_id: self.editor.project().id,
                                                revision: self.editor.project().revision,
                                                material_id: Some(board.material_id),
                                                anchor: Anchor::Centre,
                                                error: None,
                                            });
                                            modal_open = true;
                                        }
                                    });
                                    ui.small(board.id.to_string());
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add_enabled(
                                                !modal_open,
                                                egui::Button::new(
                                                    self.localizer.text("placement-numeric"),
                                                ),
                                            )
                                            .clicked()
                                        {
                                            place_numeric = Some(board.id);
                                            modal_open = true;
                                        }
                                        if ui
                                            .add_enabled(
                                                !modal_open
                                                    && self.editor.project().boards.len() > 1,
                                                egui::Button::new(
                                                    self.localizer.text("placement-face"),
                                                ),
                                            )
                                            .clicked()
                                        {
                                            place_face = Some(board.id);
                                            modal_open = true;
                                        }
                                    });
                                    if ui
                                        .add_enabled(
                                            !modal_open,
                                            egui::Button::new(
                                                self.localizer.text("board-duplicate"),
                                            ),
                                        )
                                        .clicked()
                                    {
                                        // Offset the copy so the two independent parts remain visible.
                                        let mut pose = board.pose;
                                        pose.translation_mm[0] += 25.0;
                                        duplicate = Some((board.id, pose));
                                        modal_open = true;
                                    }
                                    if ui
                                        .add_enabled(
                                            !modal_open,
                                            egui::Button::new(
                                                self.localizer.text("board-edit-dimension"),
                                            ),
                                        )
                                        .clicked()
                                    {
                                        self.board_dimension = Some(BoardDimensionDialog {
                                            focus_on_open: true,
                                            board_id: board.id,
                                            project_id: self.editor.project().id,
                                            revision: self.editor.project().revision,
                                            dimension: BoardDimension::Length,
                                            value: DimensionDraft {
                                                text: format_length(
                                                    board.length,
                                                    Unit::Mm,
                                                    locale,
                                                    3,
                                                ),
                                                consent: false,
                                            },
                                            anchor: Anchor::Centre,
                                            error: None,
                                        });
                                        modal_open = true;
                                    }
                                    ui.label(format!(
                                        "{} × {} × {} · {}",
                                        format_length(board.length, Unit::Mm, locale, 3),
                                        format_length(board.width, Unit::Mm, locale, 3),
                                        format_length(board.thickness, Unit::Mm, locale, 3),
                                        self.localizer.text(
                                            if self
                                                .editor
                                                .project()
                                                .allocations
                                                .iter()
                                                .any(|a| a.board_id == board.id)
                                            {
                                                "board-allocated"
                                            } else {
                                                "board-unallocated"
                                            }
                                        )
                                    ));
                                    if let Some(material) = self
                                        .editor
                                        .project()
                                        .materials
                                        .iter()
                                        .find(|material| material.id == board.material_id)
                                    {
                                        let mut selected = board.grain_override;
                                        ui.add_enabled_ui(!self.modal_open(), |ui| {
                                            ui.label(self.localizer.text("board-grain"));
                                            egui::ComboBox::from_id_salt(("board-grain", board.id))
                                                .selected_text(
                                                    self.localizer.text(board_grain_key(selected)),
                                                )
                                                .show_ui(ui, |ui| {
                                                    for (value, key) in [
                                                        (None, "grain-follow-default"),
                                                        (Some(BoardGrain::Length), "grain-length"),
                                                        (Some(BoardGrain::Width), "grain-width"),
                                                        (
                                                            Some(BoardGrain::Unrestricted),
                                                            "grain-unrestricted",
                                                        ),
                                                    ] {
                                                        combo_option(
                                                            ui,
                                                            &mut selected,
                                                            value,
                                                            self.localizer.text(key),
                                                        );
                                                    }
                                                });
                                        });
                                        if selected != board.grain_override {
                                            grain_edit = Some((board.id, selected));
                                        }
                                        ui.small(format!(
                                            "{}: {}",
                                            self.localizer.text("board-effective-grain"),
                                            self.localizer
                                                .text(grain_key(board.effective_grain(material)))
                                        ));
                                    }
                                }
                                if let Some((id, pose)) = duplicate {
                                    match self.editor.duplicate_board_with_fit(id, pose) {
                                        Ok((_, fit)) => {
                                            self.board_action_error = false;
                                            self.first_fit_notice = Some(fit);
                                        }
                                        Err(_) => self.board_action_error = true,
                                    }
                                }
                                if let Some(id) = place_numeric {
                                    self.placement = PlacementDialog::numeric(self, id);
                                }
                                if let Some(id) = place_face {
                                    self.placement = PlacementDialog::face(self, id);
                                }
                                if let Some((id, selected)) = grain_edit
                                    && self.editor.set_board_grain_override(id, selected).is_ok()
                                {
                                    self.material_conflicts =
                                        allocation_conflicts(self.editor.project());
                                }
                                ui.separator();
                                ui.label(self.localizer.text("viewport-axes"));
                            });
                    },
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    egui::vec2((available.x - 360.0).max(1.0), available.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let modal = self.other_modal_open() || self.sheet_repair.active();
                        if self.door_motion.is_some_and(|(id, angle)| {
                            self.editor.project().door_joints.iter().find(|j| j.id == id)
                                .is_none_or(|j| plan_my_cabinet::door_joint::derived_poses(self.editor.project(), j, angle).is_err())
                        }) {
                            self.door_motion = None;
                        }
                        let motion_poses = self.door_motion.and_then(|(id, angle)| {
                            self.editor.project().door_joints.iter().find(|j| j.id == id)
                                .and_then(|j| plan_my_cabinet::door_joint::derived_poses(self.editor.project(), j, angle).ok())
                                .map(|poses| poses.into_iter().collect::<std::collections::HashMap<_, _>>())
                        });
                        let action = ui
                            .allocate_ui_with_layout(
                                egui::vec2(
                                    ui.available_width(),
                                    (ui.available_height() * 0.55).max(180.0),
                                ),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    if self.door_motion.is_some() {
                                        ui.colored_label(egui::Color32::YELLOW,
                                            self.localizer.text("door-motion-disclosure"));
                                    }
                                    viewport::show_move(
                                        ui,
                                        &mut self.camera,
                                        self.editor.preview().unwrap_or(self.editor.project()),
                                        &mut self.selection,
                                        &mut self.move_tool,
                                        modal,
                                        self.localizer.language(),
                                        self.placement
                                            .as_ref()
                                            .and_then(PlacementDialog::highlighted),
                                        motion_poses.as_ref(),
                                    )
                                },
                            )
                            .inner;
                        match action {
                            Some(viewport::DragAction::Preview(id, pose)) => {
                                if let Ok(mut session) =
                                    plan_my_cabinet::placement::PlacementSession::resume(
                                        &mut self.editor,
                                        id,
                                    )
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
                                        plan_my_cabinet::placement::PlacementSession::resume(
                                            &mut self.editor,
                                            id,
                                        )
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
                        let other_modal = self.other_modal_open() || self.door_motion.is_some();
                        sheet_ui::show(
                            ui,
                            &mut self.editor,
                            &mut self.selection,
                            &self.localizer,
                            other_modal,
                            &mut self.sheet_repair,
                        );
                    },
                );
            });
        });
        self.show_dialog(ui.ctx());
        self.show_material_edit(ui.ctx());
        self.show_board_material(ui.ctx());
        self.show_board_dimension(ui.ctx());
        self.show_batch_dimension(ui.ctx());
        self.show_placement(ui.ctx());
        self.show_grid_dialog(ui.ctx());
        self.show_stock_dialog(ui.ctx());
        self.show_cut_fee_dialog(ui.ctx());
        self.show_assembly_dialog(ui.ctx());
        self.show_hardware_dialog(ui.ctx());
        self.show_hinge_dialog(ui.ctx());
        self.show_door_dialog(ui.ctx());
        self.show_removal_dialog(ui.ctx());
        self.show_project_dialog(ui.ctx());
    }
}

impl eframe::App for DesktopApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_close_request(ui.ctx());
        if self.project_files.allow_close {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.show_workspace(ui);
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

fn main() -> std::process::ExitCode {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 720.0]),
        ..Default::default()
    };
    if let WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        setup.instance_descriptor.backends = native_backend();
    }

    match eframe::run_native(
        plan_my_cabinet::APPLICATION_NAME,
        options,
        Box::new(|context| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "noto-sans".into(),
                egui::FontData::from_static(include_bytes!("../assets/fonts/NotoSans-Regular.ttf"))
                    .into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "noto-sans".into());
            context.egui_ctx.set_fonts(fonts);
            if let Some(state) = &context.wgpu_render_state {
                viewport::install(state);
            }
            Ok(Box::<DesktopApp>::default())
        }),
    ) {
        Ok(()) => std::process::ExitCode::SUCCESS,
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
        app.start_pdf_write(
            failure,
            before.clone(),
            ExportSettings {
                language: Language::En,
                units: Unit::Mm,
            },
            ExportMode::Draft,
            Overwrite::Decline,
        );
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
        for key in [
            "undo",
            "redo",
            "grid-edit",
            "cutting-kerf-edit",
            "stock-new",
            "stock-edit",
            "material-edit",
            "placement-numeric",
            "assembly-group",
            "board-edit-dimension",
        ] {
            let label = app.localizer.text(key);
            assert!(
                buttons
                    .iter()
                    .any(|(name, disabled, _)| name == &label && *disabled),
                "{key} should be disabled: {buttons:?}"
            );
        }
        assert!(
            buttons
                .iter()
                .any(|(name, disabled, _)| { name == "Frame selection/scene" && *disabled })
        );
        for key in ["sheet-accept", "sheet-cancel", "sheet-stage"] {
            let label = app.localizer.text(key);
            assert!(
                buttons
                    .iter()
                    .any(|(name, disabled, _)| name == &label && !*disabled),
                "{key} should be enabled"
            );
        }
        for key in [
            "undo",
            "stock-edit",
            "placement-numeric",
            "assembly-group",
            "grid-edit",
        ] {
            let label = app.localizer.text(key);
            let (_, _, pos) = buttons.iter().find(|(name, _, _)| name == &label).unwrap();
            click_repair_position(&mut app, &ctx, *pos);
        }
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
            for modifiers in [
                Modifiers::NONE,
                Modifiers::NONE,
                Modifiers::SHIFT,
                Modifiers::NONE,
            ] {
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
    fn cancelling_nested_material_restores_board_name_focus() {
        let ctx = egui::Context::default();
        let mut app = DesktopApp {
            suspended_board: Some(CreationDialog::board(None)),
            dialog: Some(CreationDialog::material()),
            ..Default::default()
        };
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
