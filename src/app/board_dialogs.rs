//! Board, material, dimension and grid dialogs: drafts, form helpers and their show methods.
use crate::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogKind {
    Board,
    Material,
}

pub(crate) struct DimensionDraft {
    pub(crate) text: String,
    pub(crate) consent: bool,
}

pub(crate) struct GridDialog {
    pub(crate) kerf: bool,
    pub(crate) project_id: Uuid,
    pub(crate) revision: u64,
    pub(crate) original: Length,
    pub(crate) unit: Unit,
    pub(crate) value: DimensionDraft,
    pub(crate) focus_on_open: bool,
    pub(crate) error: bool,
}

impl GridDialog {
    pub(crate) fn open(project: &Project, locale: Locale) -> Self {
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

    pub(crate) fn cutting_kerf(project: &Project, locale: Locale) -> Self {
        let mut draft = Self::open(project, locale);
        draft.kerf = true;
        draft.original = project.cutting_kerf;
        draft.value.text = format_length(project.cutting_kerf, Unit::Mm, locale, 3);
        draft
    }
}

impl DimensionDraft {
    pub(crate) fn new() -> Self {
        Self {
            text: String::new(),
            consent: false,
        }
    }

    pub(crate) fn value(&self, unit: Unit) -> Result<Length, InputError> {
        let parsed = parse_length(&self.text, unit)?;
        let converted = dimension(parsed.conversion).map_err(InputError::Unit)?;
        match converted {
            Conversion::Exact(value) => Ok(value),
            Conversion::NeedsConfirmation(value) if self.consent => Ok(value),
            Conversion::NeedsConfirmation(_) => Err(InputError::Unit(UnitError::InvalidNumber)),
        }
    }
}

pub(crate) struct CreationDialog {
    pub(crate) kind: DialogKind,
    pub(crate) name: String,
    pub(crate) length: DimensionDraft,
    pub(crate) width: DimensionDraft,
    pub(crate) thickness: DimensionDraft,
    pub(crate) unit: Option<Unit>,
    pub(crate) project_id: Option<Uuid>,
    pub(crate) material_id: Option<Uuid>,
    pub(crate) grain: BoardGrain,
    pub(crate) grain_override: Option<BoardGrain>,
    pub(crate) color: Option<SrgbColor>,
    pub(crate) preview: Option<(BoardPreviewKey, BoardPreview)>,
    pub(crate) error: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoardPreviewKey {
    pub(crate) project_id: Uuid,
    pub(crate) revision: u64,
    pub(crate) material_id: Uuid,
    pub(crate) length: Length,
    pub(crate) width: Length,
    pub(crate) grain_override: Option<BoardGrain>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BoardPreview {
    pub(crate) fit: FirstFit,
    pub(crate) placement: Option<([Length; 2], bool)>,
}

pub(crate) fn fit_new_board(
    project: &mut Project,
    name: String,
    key: BoardPreviewKey,
) -> BoardPreview {
    let material = project
        .materials
        .iter()
        .find(|material| material.id == key.material_id)
        .expect("validated material");
    let id = Uuid::new_v4();
    project.boards.push(Board {
        banding: Default::default(),
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
    pub(crate) fn board_key(&self, project: &Project) -> Option<BoardPreviewKey> {
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

    pub(crate) fn preview(&mut self, project: &Project, key: BoardPreviewKey) -> BoardPreview {
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

impl CreationDialog {
    pub(crate) fn board(material_id: Option<Uuid>) -> Self {
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
    pub(crate) fn material() -> Self {
        Self {
            kind: DialogKind::Material,
            ..Self::board(None)
        }
    }
}

pub(crate) struct MaterialEditDialog {
    pub(crate) focus_on_open: bool,
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) thickness: DimensionDraft,
    pub(crate) grain: BoardGrain,
    pub(crate) anchor: Anchor,
    pub(crate) choice: Option<DependantChoice>,
    pub(crate) error: Option<MaterialChangeError>,
}

pub(crate) struct BoardMaterialDialog {
    pub(crate) focus_on_open: bool,
    pub(crate) board_id: Uuid,
    pub(crate) project_id: Uuid,
    pub(crate) revision: u64,
    pub(crate) material_id: Option<Uuid>,
    pub(crate) anchor: Anchor,
    pub(crate) error: Option<MaterialChangeError>,
}

pub(crate) struct BoardDimensionDialog {
    pub(crate) focus_on_open: bool,
    pub(crate) board_id: Uuid,
    pub(crate) project_id: Uuid,
    pub(crate) revision: u64,
    pub(crate) dimension: BoardDimension,
    pub(crate) value: DimensionDraft,
    pub(crate) anchor: Anchor,
    pub(crate) error: Option<DimensionEditError>,
}

pub(crate) struct BatchDialog {
    pub(crate) focus_on_open: bool,
    pub(crate) project_id: Uuid,
    pub(crate) revision: u64,
    pub(crate) ids: Vec<Uuid>,
    pub(crate) dimension: BoardDimension,
    pub(crate) value: DimensionDraft,
    pub(crate) anchors: Vec<(Uuid, Anchor)>,
    pub(crate) error: Option<BatchDimensionError>,
}

pub(crate) fn error_key(error: InputError) -> &'static str {
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

/// `539.75`, `18`, `12,7`: a millimetre value without the unit or needless zeros.
pub(crate) fn short_mm(value: Length, locale: Locale) -> String {
    let text = format_length(value, Unit::Mm, locale, 3);
    let mut number = text.trim_end_matches(" mm").to_owned();
    if number.contains(['.', ',']) {
        while number.ends_with('0') {
            number.pop();
        }
        if number.ends_with(['.', ',']) {
            number.pop();
        }
    }
    number
}

pub(crate) fn dialog_locale(localizer: &Localizer) -> Locale {
    match localizer.language() {
        Language::En => Locale::En,
        Language::PtBr => Locale::PtBr,
    }
}

/// Whether the text is a unit expression (suffix, fraction) rather than a
/// plain number in the display unit; only those get a live parse line.
pub(crate) fn is_unit_expression(text: &str) -> bool {
    text.chars()
        .any(|c| c.is_alphabetic() || c == '/' || c == '"' || c == '\'')
}

/// A dialog value field: optional 12pt label, 34-high mono input with the
/// display-unit suffix, then one line: the live parse (`= 539.750 mm exact`),
/// an explicit rounding consent, or the validation message. Returns whether
/// the field currently holds an accepted value.
#[allow(clippy::too_many_arguments)]
pub(crate) fn unit_value_field(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    id: egui::Id,
    label: &str,
    field: &mut DimensionDraft,
    unit: Unit,
    width: f32,
    axis: Option<usize>,
) -> bool {
    use modal_chrome::form;
    let locale = dialog_locale(localizer);
    let parsed = parse_length(&field.text, unit)
        .and_then(|v| dimension(v.conversion).map_err(InputError::Unit));
    let invalid = parsed.is_err() && !field.text.trim().is_empty();
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 6.0;
        if !label.is_empty() {
            form::label(ui, label);
        }
        let suffix = localizer.text(unit_key(unit));
        let mut input = form::Input::new(id, label, width).invalid(invalid);
        if !is_unit_expression(&field.text) {
            input = input.suffix(&suffix);
        }
        if let Some(axis) = axis {
            input = input.axis(axis);
        }
        if input.show(ui, &mut field.text).changed() {
            field.consent = false;
        }
        match parse_length(&field.text, unit)
            .and_then(|v| dimension(v.conversion).map_err(InputError::Unit))
        {
            Ok(Conversion::NeedsConfirmation(value)) => {
                let mut args = FluentArgs::new();
                args.set("entered", field.text.as_str());
                args.set("rounded", format_length(value, Unit::Mm, locale, 3));
                ui.checkbox(
                    &mut field.consent,
                    egui::RichText::new(localizer.format("rounding-confirmation", Some(&args)))
                        .size(11.5)
                        .color(theme_widgets::WARN_INK),
                );
            }
            Ok(parsed) => {
                if is_unit_expression(&field.text) {
                    form::parse_ok(
                        ui,
                        &format!(
                            "= {} {}",
                            format_length(parsed.suggested(), Unit::Mm, locale, 3),
                            localizer.text("parse-exact")
                        ),
                    );
                }
            }
            Err(error) => {
                if invalid {
                    form::error(ui, &localizer.text(error_key(error)));
                }
            }
        }
        field.value(unit).is_ok()
    })
    .inner
}

/// Material picker content: swatch, name and default thickness.
pub(crate) fn material_option_content(
    ui: &mut egui::Ui,
    project: &Project,
    material: &plan_my_cabinet::domain::Material,
    locale: Locale,
) {
    let color = project
        .material_colors
        .get(&material.id)
        .map_or(egui::Color32::from_rgb(241, 240, 236), |c| {
            egui::Color32::from_rgb(c.0[0], c.0[1], c.0[2])
        });
    theme_widgets::swatch(ui, color, egui::vec2(14.0, 14.0));
    ui.add(
        egui::Label::new(
            egui::RichText::new(&material.name)
                .size(13.0)
                .color(theme_widgets::TEXT),
        )
        .truncate()
        .selectable(false),
    );
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add(
            egui::Label::new(
                theme_widgets::mono(
                    format!("{} mm", short_mm(material.default_thickness, locale)),
                    12.0,
                )
                .color(theme_widgets::FAINT),
            )
            .selectable(false),
        );
    });
}

/// Select box listing the project materials. Returns true when changed.
pub(crate) fn material_picker(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    project: &Project,
    popup: egui::Id,
    selected: &mut Option<Uuid>,
    width: f32,
) -> egui::Response {
    use modal_chrome::form;
    let locale = dialog_locale(localizer);
    let current = selected.and_then(|id| project.materials.iter().find(|m| m.id == id));
    let response = form::select_box(
        ui,
        popup,
        &current.map_or_else(|| localizer.text("material"), |m| m.name.clone()),
        width,
        |ui| match current {
            Some(material) => material_option_content(ui, project, material, locale),
            None => {
                ui.label(egui::RichText::new("—").color(theme_widgets::FAINT));
            }
        },
    );
    egui::Popup::menu(&response)
        .id(popup)
        .width(width)
        .show(|ui| {
            ui.set_min_width(width - 12.0);
            for material in &project.materials {
                if form::option(ui, *selected == Some(material.id), &material.name, |ui| {
                    material_option_content(ui, project, material, locale)
                })
                .clicked()
                {
                    *selected = Some(material.id);
                }
            }
        });
    response
}

/// Rounding consent in a `warn_bg` callout: what will be stored and the
/// required "Use the rounded value" checkbox (editing clears it).
pub(crate) fn rounding_callout(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    entered: &str,
    _unit: Unit,
    rounded: Length,
    consent: &mut bool,
) {
    let locale = dialog_locale(localizer);
    egui::Frame::new()
        .fill(theme_widgets::WARN_BG)
        .stroke(egui::Stroke::new(1.0, theme_widgets::WARN_STROKE))
        .corner_radius(8)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 9.0;
                ui.add(icons::icon(icons::Icon::Warning, theme_widgets::WARN, 15.0));
                let mut args = FluentArgs::new();
                args.set("entered", entered.trim());
                args.set("rounded", format_length(rounded, Unit::Mm, locale, 3));
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(localizer.format("rounding-callout", Some(&args)))
                            .size(13.0)
                            .color(egui::Color32::from_rgb(92, 62, 16)),
                    )
                    .wrap()
                    .selectable(false),
                );
            });
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.checkbox(consent, localizer.text("rounding-use"));
            });
        });
}

/// Swatch grid plus a free colour picker. Returns whether the value changed.
pub(crate) fn creation_color_swatches(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    color: &mut Option<SrgbColor>,
) -> bool {
    let before = *color;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            modal_chrome::form::label(ui, &localizer.text("material-color"));
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("· {}", localizer.text("material-color-scope")))
                        .size(12.0)
                        .color(theme_widgets::FAINT),
                )
                .selectable(false),
            )
            .on_hover_text(localizer.text("material-color-hint"));
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            let mut swatch = |ui: &mut egui::Ui, value: Option<SrgbColor>, label: String| {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
                let selected = *color == value;
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::RadioButton,
                        true,
                        selected,
                        &label,
                    )
                });
                let painter = ui.painter();
                if selected {
                    painter.rect_stroke(
                        rect.expand(3.0),
                        8.0,
                        egui::Stroke::new(2.0, theme_widgets::TEXT),
                        egui::StrokeKind::Inside,
                    );
                } else if response.hovered() || response.has_focus() {
                    painter.rect_stroke(
                        rect.expand(3.0),
                        8.0,
                        egui::Stroke::new(1.0, theme_widgets::BORDER_STRONG),
                        egui::StrokeKind::Inside,
                    );
                }
                match value {
                    Some(SrgbColor([r, g, b])) => {
                        painter.rect(
                            rect,
                            6.0,
                            egui::Color32::from_rgb(r, g, b),
                            egui::Stroke::new(1.0, egui::Color32::from_black_alpha(30)),
                            egui::StrokeKind::Inside,
                        );
                    }
                    None => {
                        painter.rect(
                            rect,
                            6.0,
                            theme_widgets::CARD,
                            egui::Stroke::new(1.0, theme_widgets::BORDER_STRONG),
                            egui::StrokeKind::Inside,
                        );
                        painter.line_segment(
                            [
                                rect.left_bottom() + egui::vec2(6.0, -6.0),
                                rect.right_top() + egui::vec2(-6.0, 6.0),
                            ],
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
            ui.scope(|ui| {
                ui.spacing_mut().interact_size = egui::vec2(26.0, 26.0);
                let custom = egui::color_picker::color_edit_button_srgb(ui, &mut rgb)
                    .on_hover_text(localizer.text("color-custom"));
                if custom.changed() {
                    *color = Some(SrgbColor(rgb));
                }
            });
        });
    });
    *color != before
}

pub(crate) fn conflict_labels(localizer: &Localizer, conflict: &AllocationConflict) -> String {
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

pub(crate) fn grain_key(grain: BoardGrain) -> &'static str {
    match grain {
        BoardGrain::Length => "grain-length",
        BoardGrain::Width => "grain-width",
        BoardGrain::Unrestricted => "grain-unrestricted",
    }
}

pub(crate) fn board_grain_key(grain: Option<BoardGrain>) -> &'static str {
    grain.map_or("grain-follow-default", grain_key)
}

pub(crate) fn combo_option<Value: PartialEq>(
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
pub(crate) fn dialog_escape(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !egui::Popup::is_any_open(ctx)
}

pub(crate) fn pose_error_label(localizer: &Localizer, error: UnitError) -> String {
    localizer.text(match error {
        UnitError::OutOfBounds => "error-out-of-bounds",
        UnitError::NonFinite => "error-non-finite",
        UnitError::Overflow => "error-overflow",
        _ => "error-invalid-number",
    })
}

impl DesktopApp {
    pub(crate) fn show_grid_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_grid() else {
            // A draft can be dismissed by project replacement or a caller that
            // clears dialogs directly. Do not leave a modal layer or focus trap
            // alive after its owning draft is gone.
            if self.chromes.grid.is_active() {
                self.chromes.grid.close(ctx);
            }
            return;
        };
        let locale = dialog_locale(&self.localizer);
        let current = self.editor.project().id == draft.project_id
            && self.editor.project().revision == draft.revision;
        let mut proposal = None;
        let title = self.localizer.text(if draft.kerf {
            "cutting-kerf-edit"
        } else {
            "grid-edit"
        });
        let cancel_text = self.localizer.text("cancel");
        let confirm_text = self.localizer.text(if draft.kerf {
            "cutting-kerf-set-action"
        } else {
            "grid-set-action"
        });
        self.chromes.grid.set_icon(if draft.kerf {
            icons::Icon::Cut
        } else {
            icons::Icon::Grid
        });
        let mut args = FluentArgs::new();
        args.set("value", format!("{} mm", short_mm(draft.original, locale)));
        self.chromes.grid.set_context(Some(
            self.localizer.format("dialog-current-value", Some(&args)),
        ));
        self.chromes
            .grid
            .set_hint(Some(self.localizer.text(if draft.kerf {
                "cutting-kerf-dialog-hint"
            } else {
                "grid-dialog-hint"
            })));
        let result = self.chromes.grid.show(
            ctx,
            &title,
            ModalActions {
                cancel: &cancel_text,
                confirm: &confirm_text,
            },
            |ui| {
                use modal_chrome::form;
                let width = ui.available_width();
                ui.horizontal(|ui| {
                    form::label(
                        ui,
                        &self.localizer.text(if draft.kerf {
                            "cutting-kerf-short"
                        } else {
                            "grid-spacing-short"
                        }),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        form::segmented_sized(
                            ui,
                            "grid-input-unit",
                            &mut draft.unit,
                            &[
                                (Unit::Mm, &self.localizer.text("unit-mm")),
                                (Unit::Inch, &self.localizer.text("unit-in")),
                            ],
                            96.0,
                            26.0,
                            true,
                        )
                        .on_hover_text(self.localizer.text("grid-input-unit"));
                    });
                });
                ui.add_space(4.0);
                let invalid_text = parse_length(&draft.value.text, draft.unit).is_err()
                    && !draft.value.text.trim().is_empty();
                if form::Input::new(
                    egui::Id::new("grid-spacing-value"),
                    &self.localizer.text(if draft.kerf {
                        "cutting-kerf"
                    } else {
                        "grid-spacing"
                    }),
                    width,
                )
                .suffix(&self.localizer.text(unit_key(draft.unit)))
                .invalid(invalid_text)
                .show(ui, &mut draft.value.text)
                .changed()
                {
                    draft.value.consent = false;
                    draft.error = false;
                }
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
                            form::error(
                                ui,
                                &self.localizer.text(match error {
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
                                    egui::RichText::new(
                                        self.localizer.format("rounding-confirmation", Some(&args)),
                                    )
                                    .size(11.5)
                                    .color(theme_widgets::WARN_INK),
                                );
                            } else if is_unit_expression(&draft.value.text) || unchanged {
                                form::parse_ok(
                                    ui,
                                    &format!(
                                        "= {} {}",
                                        format_length(value, Unit::Mm, locale, 3),
                                        self.localizer.text("parse-exact")
                                    ),
                                );
                            }
                            if unchanged || parsed.exact().is_some() || draft.value.consent {
                                proposal = Some(if unchanged { draft.original } else { value });
                            }
                        }
                    }
                    Err(error) => {
                        form::error(ui, &self.localizer.text(error_key(error)));
                    }
                }
                if draft.kerf {
                    form::gap(ui);
                    form::hint(ui, &self.localizer.text("cutting-kerf-clears-confirmation"));
                }
                if !current {
                    form::error(ui, &self.localizer.text("grid-stale"));
                }
                if draft.error {
                    form::error(
                        ui,
                        &self.localizer.text(if draft.kerf {
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
            self.chromes.grid.close(ctx);
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
                self.chromes.grid.close(ctx);
                return;
            }
            draft.error = true;
        }
        self.modals.set_grid(Some(draft));
    }

    pub(crate) fn show_batch_dimension(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_batch_dimension() else {
            return;
        };
        let mut accepted_preview = None;
        let project = self.editor.project();
        let current = project.id == draft.project_id && project.revision == draft.revision;
        let unit = project.display_unit;
        let locale = dialog_locale(&self.localizer);
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
        let names = draft
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
            .join(", ");
        self.chromes.batch_dimension.set_context(Some(names));
        self.chromes
            .batch_dimension
            .set_hint(Some(self.localizer.text("board-resize-atomic")));
        let result = self.chromes.batch_dimension.show(
            ctx,
            &title,
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &confirm_label,
            },
            |ui| {
                use modal_chrome::form;
                let width = ui.available_width();
                let mut summary = None;
                if current {
                    summary = self.editor.selected_boards(&selection).ok();
                }
                let half = ((width - 12.0) / 2.0).max(80.0);
                let old_axis = draft.dimension;
                let common = draft.anchors.first().and_then(|(_, first)| {
                    draft
                        .anchors
                        .iter()
                        .all(|(_, anchor)| anchor == first)
                        .then_some(*first)
                });
                let mut chosen = common;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    ui.vertical(|ui| {
                        ui.set_width(half);
                        ui.spacing_mut().item_spacing.y = 6.0;
                        form::label(ui, &self.localizer.text("board-axis"))
                            .on_hover_text(self.localizer.text("board-local-dimension"));
                        form::segmented(
                            ui,
                            "batch-axis",
                            &mut draft.dimension,
                            &[
                                (
                                    BoardDimension::Length,
                                    &self.localizer.text("grain-seg-length"),
                                ),
                                (
                                    BoardDimension::Width,
                                    &self.localizer.text("grain-seg-width"),
                                ),
                                (
                                    BoardDimension::Thickness,
                                    &self.localizer.text("board-thickness-short"),
                                ),
                            ],
                            half,
                        );
                    });
                    ui.vertical(|ui| {
                        ui.set_width(half);
                        ui.spacing_mut().item_spacing.y = 6.0;
                        form::label(ui, &self.localizer.text("board-keep-fixed"))
                            .on_hover_text(self.localizer.text("board-resize-anchor"));
                        form::segmented(
                            ui,
                            "batch-anchor",
                            &mut chosen,
                            &[
                                (
                                    Some(Anchor::Start),
                                    &self.localizer.text("anchor-start-short"),
                                ),
                                (
                                    Some(Anchor::Centre),
                                    &self.localizer.text("anchor-centre-short"),
                                ),
                                (Some(Anchor::End), &self.localizer.text("anchor-end-short")),
                            ],
                            half,
                        );
                    });
                });
                if draft.dimension != old_axis {
                    draft.value = DimensionDraft::new();
                    draft.error = None;
                }
                if chosen != common
                    && let Some(anchor) = chosen
                {
                    for (_, value) in &mut draft.anchors {
                        *value = anchor;
                    }
                    draft.error = None;
                }
                form::gap(ui);
                let axis_key = match draft.dimension {
                    BoardDimension::Length => "board-new-length",
                    BoardDimension::Width => "board-new-width",
                    BoardDimension::Thickness => "board-new-thickness",
                };
                let current_note = summary.as_ref().map(|summary| {
                    let text = match summary.dimensions[draft.dimension.axis()] {
                        SelectionValue::Uniform(value) => short_mm(value, locale),
                        SelectionValue::Mixed => format!(
                            "{} ({})",
                            self.localizer.text("board-mixed-short"),
                            draft
                                .ids
                                .iter()
                                .filter_map(|id| project.boards.iter().find(|b| b.id == *id))
                                .map(|board| short_mm(
                                    board.blank_dimensions()[draft.dimension.axis()],
                                    locale
                                ))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    };
                    let mut args = FluentArgs::new();
                    args.set("value", text);
                    self.localizer.format("board-current-note", Some(&args))
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    form::label_with_note(
                        ui,
                        &self.localizer.text(axis_key),
                        current_note.as_deref().unwrap_or(""),
                    );
                    let parsed = parse_length(&draft.value.text, unit)
                        .and_then(|value| dimension(value.conversion).map_err(InputError::Unit));
                    let suffix = self.localizer.text(unit_key(unit));
                    let accessible = self.localizer.text("board-dimension-preview");
                    let mut input = form::Input::new(
                        egui::Id::new("batch-dimension-value"),
                        &accessible,
                        width,
                    )
                    .invalid(parsed.is_err() && !draft.value.text.trim().is_empty());
                    if !is_unit_expression(&draft.value.text) {
                        input = input.suffix(&suffix);
                    }
                    if input.show(ui, &mut draft.value.text).changed() {
                        draft.value.consent = false;
                        draft.error = None;
                    }
                    match &parsed {
                        Ok(Conversion::NeedsConfirmation(_)) => {}
                        Ok(parsed) if is_unit_expression(&draft.value.text) => {
                            form::parse_ok(
                                ui,
                                &format!(
                                    "= {} {}",
                                    format_length(parsed.suggested(), Unit::Mm, locale, 3),
                                    self.localizer.text("parse-exact")
                                ),
                            );
                        }
                        Ok(_) => {}
                        Err(error) => {
                            if !draft.value.text.trim().is_empty() {
                                form::error(ui, &self.localizer.text(error_key(*error)));
                            }
                        }
                    }
                });
                let parsed = parse_length(&draft.value.text, unit)
                    .and_then(|value| dimension(value.conversion).map_err(InputError::Unit));
                if let Ok(Conversion::NeedsConfirmation(value)) = &parsed {
                    form::gap(ui);
                    rounding_callout(
                        ui,
                        &self.localizer,
                        &draft.value.text,
                        unit,
                        *value,
                        &mut draft.value.consent,
                    );
                }
                let valid = draft.value.value(unit).is_ok();
                form::gap(ui);
                let rows = draft.anchors.len();
                egui::ScrollArea::vertical()
                    .id_salt("batch-affected")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (index, (id, anchor)) in draft.anchors.iter_mut().enumerate() {
                            let Some(board) = project.boards.iter().find(|b| b.id == *id) else {
                                continue;
                            };
                            let before = board.blank_dimensions()[draft.dimension.axis()];
                            let after = if valid {
                                short_mm(draft.value.value(unit).expect("validated"), locale)
                            } else {
                                "—".into()
                            };
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 30.0),
                                egui::Sense::hover(),
                            );
                            let mut row = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(rect)
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            );
                            row.add(
                                egui::Label::new(
                                    egui::RichText::new(&board.name)
                                        .size(12.0)
                                        .color(theme_widgets::TEXT),
                                )
                                .truncate()
                                .selectable(false),
                            );
                            row.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    ui.add(
                                        egui::Label::new(
                                            theme_widgets::mono(
                                                format!("{} → {after}", short_mm(before, locale)),
                                                12.0,
                                            )
                                            .color(theme_widgets::MUTED),
                                        )
                                        .selectable(false),
                                    );
                                    {
                                        let mut local = *anchor;
                                        let initial = |key: &str| {
                                            self.localizer
                                                .text(key)
                                                .chars()
                                                .next()
                                                .map(String::from)
                                                .unwrap_or_default()
                                        };
                                        let [start, centre, end] = [
                                            initial("anchor-start-short"),
                                            initial("anchor-centre-short"),
                                            initial("anchor-end-short"),
                                        ];
                                        form::segmented_sized(
                                            ui,
                                            ("batch-board-anchor", index),
                                            &mut local,
                                            &[
                                                (Anchor::Start, start.as_str()),
                                                (Anchor::Centre, centre.as_str()),
                                                (Anchor::End, end.as_str()),
                                            ],
                                            84.0,
                                            24.0,
                                            true,
                                        )
                                        .on_hover_text(self.localizer.text("board-resize-anchor"));
                                        *anchor = local;
                                    }
                                },
                            );
                            if index + 1 < rows {
                                ui.painter().hline(
                                    rect.x_range(),
                                    rect.bottom() - 0.5,
                                    egui::Stroke::new(1.0, theme_widgets::RULE),
                                );
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
                    form::error(ui, &self.localizer.text("board-assignment-stale"));
                }
                if let Some(error) = &draft.error {
                    let label = match error {
                        BatchDimensionError::Target { board_id, reason } => format!(
                            "{}: {} ({reason:?})",
                            self.localizer.text("board-batch-invalid"),
                            project
                                .boards
                                .iter()
                                .find(|b| b.id == *board_id)
                                .map_or_else(
                                    || assembly_ui::short_id('b', *board_id),
                                    |b| { b.name.clone() }
                                )
                        ),
                        _ => self.localizer.text("error-board-dimension"),
                    };
                    form::error(ui, &label);
                }
                if let Some(proposal) = &preview {
                    for issue in &proposal.conflicts {
                        form::warning(
                            ui,
                            &format!(
                                "{}: {}",
                                project
                                    .boards
                                    .iter()
                                    .find(|b| b.id == issue.board_id)
                                    .map_or_else(
                                        || assembly_ui::short_id('b', issue.board_id),
                                        |b| b.name.clone()
                                    ),
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
            self.chromes.batch_dimension.close(ctx);
            return;
        }
        let confirm = actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm);
        if confirm {
            accepted_preview = result.body;
        }
        if let Some(preview) = accepted_preview {
            match self.editor.edit_batch_board_dimension(preview) {
                Ok(conflicts) => {
                    self.cut_plan.material_conflicts = conflicts;
                    self.chromes.batch_dimension.close(ctx);
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
        self.modals.set_batch_dimension(Some(draft));
    }

    pub(crate) fn show_board_dimension(&mut self, ctx: &egui::Context) {
        if self.navigation.pending().is_some() {
            return;
        }
        let Some(mut draft) = self.modals.take_board_dimension() else {
            if self.chromes.board_dimension.is_active() {
                self.chromes.board_dimension.close(ctx);
            }
            return;
        };
        let project = self.editor.project();
        let current = project.id == draft.project_id && project.revision == draft.revision;
        let board = project
            .boards
            .iter()
            .find(|board| board.id == draft.board_id);
        let unit = project.display_unit;
        let locale = dialog_locale(&self.localizer);
        let mut preview: Option<DimensionPreview> = None;
        let title = self.localizer.text("board-edit-dimension");
        let cancel_label = self.localizer.text("cancel");
        let confirm_label = self.localizer.text("board-resize-one-action");
        self.chromes
            .board_dimension
            .set_context(board.map(|board| board.name.clone()));
        self.chromes
            .board_dimension
            .set_hint(Some(self.localizer.text("board-input-hint-short")));
        let modal = self.chromes.board_dimension.show(
            ctx,
            &title,
            ModalActions {
                cancel: &cancel_label,
                confirm: &confirm_label,
            },
            |ui| {
                use modal_chrome::form;
                let width = ui.available_width();
                let half = ((width - 12.0) / 2.0).max(80.0);
                let old_dimension = draft.dimension;
                let axis = ui
                    .horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let axis = ui
                            .vertical(|ui| {
                                ui.set_width(half);
                                ui.spacing_mut().item_spacing.y = 6.0;
                                form::label(ui, &self.localizer.text("board-axis"))
                                    .on_hover_text(self.localizer.text("board-local-dimension"));
                                form::segmented(
                                    ui,
                                    "board-dimension-axis",
                                    &mut draft.dimension,
                                    &[
                                        (
                                            BoardDimension::Length,
                                            &self.localizer.text("grain-seg-length"),
                                        ),
                                        (
                                            BoardDimension::Width,
                                            &self.localizer.text("grain-seg-width"),
                                        ),
                                        (
                                            BoardDimension::Thickness,
                                            &self.localizer.text("board-thickness-short"),
                                        ),
                                    ],
                                    half,
                                )
                            })
                            .inner;
                        ui.vertical(|ui| {
                            ui.set_width(half);
                            ui.spacing_mut().item_spacing.y = 6.0;
                            form::label(ui, &self.localizer.text("board-keep-fixed"))
                                .on_hover_text(self.localizer.text("board-resize-anchor"));
                            form::segmented(
                                ui,
                                "board-dimension-anchor",
                                &mut draft.anchor,
                                &[
                                    (Anchor::Start, &self.localizer.text("anchor-start-short")),
                                    (Anchor::Centre, &self.localizer.text("anchor-centre-short")),
                                    (Anchor::End, &self.localizer.text("anchor-end-short")),
                                ],
                                half,
                            );
                        });
                        axis
                    })
                    .inner;
                if draft.focus_on_open {
                    axis.request_focus();
                }
                if draft.dimension != old_dimension {
                    if let Some(board) = board {
                        let value = board.blank_dimensions()[draft.dimension.axis()];
                        draft.value.text = format_length(value, Unit::Mm, locale, 3);
                        draft.value.consent = false;
                    }
                    draft.error = None;
                }
                form::gap(ui);
                let axis_key = match draft.dimension {
                    BoardDimension::Length => "board-new-length",
                    BoardDimension::Width => "board-new-width",
                    BoardDimension::Thickness => "board-new-thickness",
                };
                let note = board.map(|board| {
                    let mut args = FluentArgs::new();
                    args.set(
                        "value",
                        short_mm(board.blank_dimensions()[draft.dimension.axis()], locale),
                    );
                    self.localizer.format("board-current-note", Some(&args))
                });
                form::label_with_note(
                    ui,
                    &self.localizer.text(axis_key),
                    note.as_deref().unwrap_or(""),
                );
                ui.add_space(2.0);
                let parsed = parse_length(&draft.value.text, unit)
                    .and_then(|value| dimension(value.conversion).map_err(InputError::Unit));
                let suffix = self.localizer.text(unit_key(unit));
                let accessible = self.localizer.text(axis_key);
                let mut input =
                    form::Input::new(egui::Id::new("board-dimension-value"), &accessible, width)
                        .invalid(parsed.is_err() && !draft.value.text.trim().is_empty());
                if !is_unit_expression(&draft.value.text) {
                    input = input.suffix(&suffix);
                }
                if input.show(ui, &mut draft.value.text).changed() {
                    draft.value.consent = false;
                    draft.error = None;
                }
                match parse_length(&draft.value.text, unit)
                    .and_then(|value| dimension(value.conversion).map_err(InputError::Unit))
                {
                    Ok(Conversion::NeedsConfirmation(value)) => {
                        form::gap(ui);
                        rounding_callout(
                            ui,
                            &self.localizer,
                            &draft.value.text,
                            unit,
                            value,
                            &mut draft.value.consent,
                        );
                    }
                    Ok(parsed) if is_unit_expression(&draft.value.text) => {
                        form::parse_ok(
                            ui,
                            &format!(
                                "= {} {}",
                                format_length(parsed.suggested(), Unit::Mm, locale, 3),
                                self.localizer.text("parse-exact")
                            ),
                        );
                    }
                    Ok(_) => {}
                    Err(error) => {
                        if !draft.value.text.trim().is_empty() {
                            form::error(ui, &self.localizer.text(error_key(error)));
                        }
                    }
                }
                let valid = draft.value.value(unit).is_ok();
                if !current {
                    form::error(ui, &self.localizer.text("board-assignment-stale"));
                } else if valid {
                    match self.editor.preview_board_dimension(
                        draft.board_id,
                        draft.dimension,
                        draft.value.value(unit).expect("validated"),
                        draft.anchor,
                    ) {
                        Ok(result) => {
                            if let Some(board) = board {
                                form::gap(ui);
                                form::strip(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&board.name)
                                                    .size(12.0)
                                                    .color(theme_widgets::TEXT),
                                            )
                                            .selectable(false),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        theme_widgets::mono(
                                                            format!(
                                                                "{} → {}",
                                                                short_mm(
                                                                    board.blank_dimensions()
                                                                        [draft.dimension.axis()],
                                                                    locale
                                                                ),
                                                                short_mm(result.value, locale)
                                                            ),
                                                            12.0,
                                                        )
                                                        .color(theme_widgets::MUTED),
                                                    )
                                                    .selectable(false),
                                                );
                                            },
                                        );
                                    });
                                });
                            }
                            for issue in &result.conflicts {
                                form::warning(
                                    ui,
                                    &format!(
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
                    form::error(
                        ui,
                        &match error {
                            DimensionEditError::InvalidDimension(reason)
                            | DimensionEditError::InvalidPose(reason) => {
                                pose_error_label(&self.localizer, *reason)
                            }
                            _ => self.localizer.text("error-board-dimension"),
                        },
                    );
                }
                ((), preview.is_some())
            },
        );
        draft.focus_on_open = false;
        let cancel = modal.action == ModalAction::Cancel;
        let confirm = modal.action == ModalAction::Confirm;
        if actions::decision(A::CancelDialog, cancel) {
            self.chromes.board_dimension.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, confirm)
            && let Some(preview) = preview
        {
            match self.editor.edit_board_dimension(preview) {
                Ok(conflicts) => {
                    self.cut_plan.material_conflicts = conflicts;
                    self.chromes.board_dimension.close(ctx);
                    return;
                }
                Err(plan_my_cabinet::commands::EditError::Command(error)) => {
                    draft.error = Some(error)
                }
                Err(_) => draft.error = Some(DimensionEditError::StalePreview),
            }
        }
        self.modals.set_board_dimension(Some(draft));
    }

    pub(crate) fn show_board_material(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_board_material() else {
            if self.chromes.board_material.is_active() {
                self.chromes.board_material.close(ctx);
            }
            return;
        };
        let project = self.editor.project();
        let current = project.id == draft.project_id && project.revision == draft.revision;
        let board = project.boards.iter().find(|b| b.id == draft.board_id);
        let locale = dialog_locale(&self.localizer);
        let title = self.localizer.text("board-assign-material");
        let cancel_label = self.localizer.text("cancel");
        let confirm_label = self.localizer.text("board-assign-action");
        self.chromes.board_material.set_context(board.map(|board| {
            let mut args = FluentArgs::new();
            args.set("name", board.name.as_str());
            args.set("thickness", short_mm(board.thickness, locale));
            self.localizer.format("board-material-context", Some(&args))
        }));
        let modal = self.chromes.board_material.show(
            ctx,
            &title,
            ModalActions {
                cancel: &cancel_label,
                confirm: &confirm_label,
            },
            |ui| {
                use modal_chrome::form;
                let width = ui.available_width();
                let before = draft.material_id;
                let picker = ui
                    .vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        form::label(ui, &self.localizer.text("material"));
                        material_picker(
                            ui,
                            &self.localizer,
                            project,
                            egui::Id::new("board-material-picker").with("popup"),
                            &mut draft.material_id,
                            width,
                        )
                    })
                    .inner;
                if draft.material_id != before {
                    draft.error = None;
                }
                if draft.focus_on_open {
                    picker.request_focus();
                }
                form::gap(ui);
                let before = draft.anchor;
                form::field(ui, &self.localizer.text("material-anchor"), |ui| {
                    form::segmented(
                        ui,
                        "board-material-anchor",
                        &mut draft.anchor,
                        &[
                            (Anchor::Start, &self.localizer.text("anchor-start-short")),
                            (Anchor::Centre, &self.localizer.text("anchor-centre-short")),
                            (Anchor::End, &self.localizer.text("anchor-end-short")),
                        ],
                        width,
                    );
                });
                if draft.anchor != before {
                    draft.error = None;
                }
                let mut valid = false;
                if !current {
                    form::error(ui, &self.localizer.text("board-assignment-stale"));
                } else if let (Some(board), Some(material_id)) = (board, draft.material_id)
                    && board.material_id != material_id
                {
                    match self
                        .editor
                        .preview_board_material(board.id, material_id, draft.anchor)
                    {
                        Ok((thickness, conflict)) => {
                            valid = true;
                            form::gap(ui);
                            form::strip(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(
                                                self.localizer.text("board-resulting-thickness"),
                                            )
                                            .size(12.0)
                                            .color(theme_widgets::MUTED),
                                        )
                                        .selectable(false),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    theme_widgets::mono(
                                                        format!(
                                                            "{} → {} mm",
                                                            short_mm(board.thickness, locale),
                                                            short_mm(thickness, locale)
                                                        ),
                                                        12.0,
                                                    )
                                                    .color(theme_widgets::TEXT),
                                                )
                                                .selectable(false),
                                            );
                                        },
                                    );
                                });
                            });
                            if let Some(conflict) = conflict {
                                form::warning(
                                    ui,
                                    &format!(
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
                    form::error(
                        ui,
                        &match error {
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
            self.chromes.board_material.close(ctx);
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
                    self.cut_plan.material_conflicts = conflicts;
                    self.chromes.board_material.close(ctx);
                    return;
                }
                Err(plan_my_cabinet::commands::EditError::Command(error)) => {
                    draft.error = Some(error)
                }
                Err(_) => draft.error = Some(MaterialChangeError::StalePreview),
            }
        }
        self.modals.set_board_material(Some(draft));
    }

    pub(crate) fn show_material_edit(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_material_edit() else {
            if self.chromes.material_edit.is_active() {
                self.chromes.material_edit.close(ctx);
            }
            return;
        };
        let unit = self.editor.project().display_unit;
        let locale = dialog_locale(&self.localizer);
        let mut preview: Option<MaterialChangePreview> = None;
        let title = self.localizer.text("material-edit");
        let cancel_label = self.localizer.text("cancel");
        let confirm_label = self.localizer.text("material-save-action");
        let used_by = self
            .editor
            .project()
            .boards
            .iter()
            .filter(|board| board.material_id == draft.id)
            .count();
        let mut args = FluentArgs::new();
        args.set("count", used_by as i64);
        self.chromes.material_edit.set_context(Some(
            self.localizer.format("material-edit-context", Some(&args)),
        ));
        let modal = self.chromes.material_edit.show(
            ctx,
            &title,
            ModalActions {
                cancel: &cancel_label,
                confirm: &confirm_label,
            },
            |ui| {
                use modal_chrome::form;
                let width = ui.available_width();
                let thickness_width = 110.0;
                let name_width = (width - thickness_width - 10.0).max(80.0);
                let old_text = draft.thickness.text.clone();
                let valid = ui
                    .horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.vertical(|ui| {
                            ui.set_width(name_width);
                            ui.spacing_mut().item_spacing.y = 6.0;
                            form::label(ui, &self.localizer.text("material-name-short"));
                            let name = form::Input::new(
                                egui::Id::new("material-edit-name"),
                                &self.localizer.text("material-name"),
                                name_width,
                            )
                            .text()
                            .show(ui, &mut draft.name);
                            if draft.focus_on_open {
                                name.request_focus();
                            }
                            if name.changed() {
                                draft.choice = None;
                                draft.error = None;
                            }
                        });
                        unit_value_field(
                            ui,
                            &self.localizer,
                            egui::Id::new("material-edit-thickness"),
                            &self.localizer.text("board-thickness"),
                            &mut draft.thickness,
                            unit,
                            thickness_width,
                            None,
                        )
                    })
                    .inner;
                if draft.thickness.text != old_text {
                    draft.choice = None;
                    draft.error = None;
                }
                form::gap(ui);
                let old_grain = draft.grain;
                form::field(ui, &self.localizer.text("material-grain"), |ui| {
                    form::segmented(
                        ui,
                        "material-edit-grain",
                        &mut draft.grain,
                        &[
                            (BoardGrain::Length, &self.localizer.text("grain-length")),
                            (BoardGrain::Width, &self.localizer.text("grain-width")),
                            (BoardGrain::Unrestricted, &self.localizer.text("grain-none")),
                        ],
                        width,
                    );
                });
                if old_grain != draft.grain {
                    draft.choice = None;
                    draft.error = None;
                }
                form::gap(ui);
                let mut color = self
                    .editor
                    .project()
                    .material_colors
                    .get(&draft.id)
                    .copied();
                if creation_color_swatches(ui, &self.localizer, &mut color) {
                    let _ = self.editor.set_material_color(draft.id, color);
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
                    form::gap(ui);
                    form::label(ui, &self.localizer.text("material-affected-short"));
                    ui.add_space(2.0);
                    egui::ScrollArea::vertical()
                        .id_salt("material-edit-affected")
                        .max_height(150.0)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            let rows = proposal.affected.len();
                            for (index, board) in proposal.affected.iter().enumerate() {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 28.0),
                                    egui::Sense::hover(),
                                );
                                let mut row = ui.new_child(
                                    egui::UiBuilder::new()
                                        .max_rect(rect)
                                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                );
                                row.add(
                                    egui::Label::new(
                                        egui::RichText::new(&board.name)
                                            .size(12.0)
                                            .color(theme_widgets::TEXT),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                                row.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                theme_widgets::mono(
                                                    format!(
                                                        "{} → {}",
                                                        short_mm(board.thickness_before, locale),
                                                        short_mm(
                                                            board.thickness_if_applied,
                                                            locale
                                                        )
                                                    ),
                                                    12.0,
                                                )
                                                .color(theme_widgets::MUTED),
                                            )
                                            .selectable(false),
                                        );
                                        if board.apply_error.is_some()
                                            || board.allocation_if_applied.is_some()
                                        {
                                            let detail = board.apply_error.map_or_else(
                                                || {
                                                    board
                                                        .allocation_if_applied
                                                        .as_ref()
                                                        .map_or_else(String::new, |conflict| {
                                                            format!(
                                                                "{}: {}",
                                                                self.localizer.text(
                                                                    "material-prospective-conflict"
                                                                ),
                                                                conflict_labels(
                                                                    &self.localizer,
                                                                    conflict
                                                                )
                                                            )
                                                        })
                                                },
                                                |error| {
                                                    format!(
                                                        "{}: {}",
                                                        self.localizer.text("material-apply-error"),
                                                        pose_error_label(&self.localizer, error)
                                                    )
                                                },
                                            );
                                            ui.add(icons::icon(
                                                icons::Icon::Warning,
                                                if board.apply_error.is_some() {
                                                    theme_widgets::DANGER
                                                } else {
                                                    theme_widgets::WARN
                                                },
                                                13.0,
                                            ))
                                            .on_hover_text(detail);
                                        }
                                    },
                                );
                                if index + 1 < rows {
                                    ui.painter().hline(
                                        rect.x_range(),
                                        rect.bottom() - 0.5,
                                        egui::Stroke::new(1.0, theme_widgets::RULE),
                                    );
                                }
                            }
                        });
                    form::gap(ui);
                    let old_anchor = draft.anchor;
                    form::field(ui, &self.localizer.text("material-anchor"), |ui| {
                        form::segmented(
                            ui,
                            "material-edit-anchor",
                            &mut draft.anchor,
                            &[
                                (Anchor::Start, &self.localizer.text("anchor-start-short")),
                                (Anchor::Centre, &self.localizer.text("anchor-centre-short")),
                                (Anchor::End, &self.localizer.text("anchor-end-short")),
                            ],
                            width,
                        );
                    });
                    if old_anchor != draft.anchor {
                        draft.choice = None;
                        draft.error = None;
                    }
                    form::gap(ui);
                    ui.spacing_mut().item_spacing.y = 8.0;
                    if form::radio_card(
                        ui,
                        "material-preserve",
                        draft.choice == Some(DependantChoice::Preserve),
                        &self.localizer.text("material-preserve"),
                        Some(&self.localizer.text("material-preserve-detail")),
                    )
                    .clicked()
                    {
                        draft.choice = Some(DependantChoice::Preserve);
                    }
                    if form::radio_card(
                        ui,
                        "material-apply-all",
                        draft.choice == Some(DependantChoice::ApplyAll),
                        &self.localizer.text("material-apply-all"),
                        Some(&self.localizer.text("material-apply-all-detail")),
                    )
                    .clicked()
                    {
                        draft.choice = Some(DependantChoice::ApplyAll);
                    }
                    apply_blocked = matches!(draft.choice, Some(DependantChoice::ApplyAll))
                        && proposal.affected.iter().any(|b| b.apply_error.is_some());
                    if apply_blocked {
                        form::error(ui, &self.localizer.text("material-apply-blocked"));
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
                    form::error(ui, &message);
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
            self.chromes.material_edit.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, confirm)
            && let (Some(preview), Some(choice)) = (preview, draft.choice.clone())
        {
            match self.editor.apply_material_change(preview, choice) {
                Ok(conflicts) => {
                    self.cut_plan.material_conflicts = conflicts;
                    self.chromes.material_edit.close(ctx);
                    return;
                }
                Err(plan_my_cabinet::commands::EditError::Command(error)) => {
                    draft.error = Some(error)
                }
                Err(_) => draft.error = Some(MaterialChangeError::StalePreview),
            }
        }
        self.modals.set_material_edit(Some(draft));
    }

    pub(crate) fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_creation() else {
            if self.chromes.board_creation.is_active() {
                self.chromes.board_creation.close(ctx);
            }
            if self.chromes.material_creation.is_active() {
                self.chromes.material_creation.close(ctx);
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
            &mut self.chromes.board_creation
        } else {
            &mut self.chromes.material_creation
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
        chrome.set_context(Some(self.localizer.text(if is_board {
            "board-new-context"
        } else {
            "material-new-context"
        })));
        chrome.set_hint(is_board.then(|| self.localizer.text("board-input-hint-short")));
        let result = chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &cancel_text,
                confirm: &confirm_text,
            },
            |ui| {
                use modal_chrome::form;
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                let width = ui.available_width();
                let valid = if is_board {
                    form::field(ui, &self.localizer.text("board-name-short"), |ui| {
                        form::Input::new(
                            egui::Id::new("board-creation-name"),
                            &self.localizer.text("board-name"),
                            width,
                        )
                        .text()
                        .show(ui, &mut draft.name);
                    });
                    form::gap(ui);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.horizontal(|ui| {
                            form::label(ui, &self.localizer.text("material"));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let new_material = ui
                                        .add(
                                            egui::Button::image_and_text(
                                                icons::icon(
                                                    icons::Icon::Plus,
                                                    modal_chrome::ICON_INK,
                                                    12.0,
                                                ),
                                                egui::RichText::new(
                                                    self.localizer.text("material-new"),
                                                )
                                                .size(12.0)
                                                .color(modal_chrome::ICON_INK),
                                            )
                                            .frame_when_inactive(false)
                                            .min_size(egui::vec2(0.0, 20.0)),
                                        )
                                        .on_hover_text(self.localizer.text("material-new"));
                                    if new_material.clicked() {
                                        create_material = true;
                                    }
                                },
                            );
                        });
                        material_picker(
                            ui,
                            &self.localizer,
                            project,
                            egui::Id::new("board-creation-material").with("popup"),
                            &mut draft.material_id,
                            width,
                        );
                    });
                    let material = draft
                        .material_id
                        .and_then(|id| project.materials.iter().find(|m| m.id == id));
                    if material.is_none() {
                        form::error(ui, &self.localizer.text("error-material-missing"));
                    }
                    form::gap(ui);
                    let column = ((width - 96.0 - 20.0) / 2.0).max(60.0);
                    let (length, width_valid) = ui
                        .horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            let length = unit_value_field(
                                ui,
                                &self.localizer,
                                egui::Id::new("board-creation-length"),
                                &self.localizer.text("board-length-x"),
                                &mut draft.length,
                                unit,
                                column,
                                None,
                            );
                            let width = unit_value_field(
                                ui,
                                &self.localizer,
                                egui::Id::new("board-creation-width"),
                                &self.localizer.text("board-width-y"),
                                &mut draft.width,
                                unit,
                                column,
                                None,
                            );
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 6.0;
                                form::label(ui, &self.localizer.text("board-thickness"));
                                form::derived(
                                    ui,
                                    &material.map_or_else(
                                        || "—".to_owned(),
                                        |m| short_mm(m.default_thickness, locale),
                                    ),
                                    &self.localizer.text("board-thickness-from-material"),
                                    96.0,
                                )
                                .on_hover_text(self.localizer.text("board-effective"));
                            });
                            (length, width)
                        })
                        .inner;
                    if length
                        && width_valid
                        && material.is_some()
                        && draft.board_key(project).is_none()
                        && current_project
                    {
                        form::error(ui, &self.localizer.text("error-out-of-bounds"));
                    }
                    form::gap(ui);
                    form::field(ui, &self.localizer.text("board-grain-short"), |ui| {
                        let material_grain =
                            material.map_or("grain-seg-any", |m| match m.default_grain {
                                BoardGrain::Length => "grain-short-length",
                                BoardGrain::Width => "grain-short-width",
                                BoardGrain::Unrestricted => "grain-short-any",
                            });
                        let mut args = FluentArgs::new();
                        args.set("grain", self.localizer.text(material_grain));
                        let follow = self.localizer.format("grain-seg-material", Some(&args));
                        form::segmented(
                            ui,
                            "board-creation-grain",
                            &mut draft.grain_override,
                            &[
                                (None, follow.as_str()),
                                (
                                    Some(BoardGrain::Length),
                                    &self.localizer.text("grain-seg-length"),
                                ),
                                (
                                    Some(BoardGrain::Width),
                                    &self.localizer.text("grain-seg-width"),
                                ),
                                (
                                    Some(BoardGrain::Unrestricted),
                                    &self.localizer.text("grain-seg-any"),
                                ),
                            ],
                            width,
                        );
                    });
                    if length
                        && width_valid
                        && let Some(material) = material
                        && let Some(key) = draft.board_key(project)
                    {
                        let preview = draft.preview(project, key);
                        form::gap(ui);
                        let strip = egui::Frame::new()
                            .fill(theme_widgets::APP)
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(12, 10))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 12.0;
                                    let (area, _) = ui.allocate_exact_size(
                                        egui::vec2(54.0, 42.0),
                                        egui::Sense::hover(),
                                    );
                                    let [l, w] = [key.length, key.width]
                                        .map(|v| v.micrometres().max(1) as f32);
                                    let scale = (48.0 / l).min(38.0 / w);
                                    let size =
                                        egui::vec2((l * scale).max(4.0), (w * scale).max(4.0));
                                    let fill = project
                                        .material_colors
                                        .get(&material.id)
                                        .map_or(egui::Color32::from_rgb(233, 227, 215), |c| {
                                            egui::Color32::from_rgb(c.0[0], c.0[1], c.0[2])
                                        });
                                    ui.painter().rect(
                                        egui::Rect::from_center_size(area.center(), size),
                                        0.0,
                                        fill,
                                        egui::Stroke::new(
                                            1.5,
                                            egui::Color32::from_rgb(156, 144, 126),
                                        ),
                                        egui::StrokeKind::Inside,
                                    );
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 2.0;
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format!(
                                                    "{} × {} × {} mm",
                                                    short_mm(key.length, locale),
                                                    short_mm(key.width, locale),
                                                    short_mm(material.default_thickness, locale)
                                                ))
                                                .font(theme_widgets::weighted_font(
                                                    ui,
                                                    13.0,
                                                    theme::Typeface::MonoMedium,
                                                ))
                                                .color(theme_widgets::TEXT),
                                            )
                                            .selectable(false),
                                        );
                                        let outlook = match (preview.fit, preview.placement) {
                                            (
                                                FirstFit::Allocated(stock_id),
                                                Some((origin, turn)),
                                            ) => {
                                                let mut args = FluentArgs::new();
                                                args.set(
                                                    "stock",
                                                    project
                                                        .stock_alias(stock_id)
                                                        .map(|alias| alias.to_string())
                                                        .unwrap_or_else(|| {
                                                            assembly_ui::short_id('s', stock_id)
                                                        }),
                                                );
                                                args.set("x", short_mm(origin[0], locale));
                                                args.set("y", short_mm(origin[1], locale));
                                                let fits = self
                                                    .localizer
                                                    .format("board-preview-fits", Some(&args));
                                                if turn {
                                                    format!(
                                                        "{fits} · {}",
                                                        self.localizer.text("board-preview-turned")
                                                    )
                                                } else {
                                                    fits
                                                }
                                            }
                                            (FirstFit::NoFit, _) => {
                                                self.localizer.text("board-preview-no-fit")
                                            }
                                            (FirstFit::SearchExhausted, _) => {
                                                self.localizer.text("first-fit-exhausted")
                                            }
                                            _ => unreachable!("allocated fit includes placement"),
                                        };
                                        form::note(ui, &outlook);
                                    });
                                });
                            });
                        strip
                            .response
                            .on_hover_text(self.localizer.text("board-preview-provisional"));
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
                    form::field(ui, &self.localizer.text("material-preset"), |ui| {
                        let popup = egui::Id::new("material-creation-preset").with("popup");
                        let preset_label = current.map_or_else(
                            || self.localizer.text("material-preset-none"),
                            |index| {
                                let preset = &plan_my_cabinet::material_presets::BR_STANDARD[index];
                                format!("{} · {} mm", preset.name(language), preset.thickness_mm)
                            },
                        );
                        let response = form::select_box(ui, popup, &preset_label, width, |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&preset_label).size(13.0).color(
                                        if current.is_some() {
                                            theme_widgets::TEXT
                                        } else {
                                            theme_widgets::MUTED
                                        },
                                    ),
                                )
                                .truncate()
                                .selectable(false),
                            );
                        });
                        egui::Popup::menu(&response)
                            .id(popup)
                            .width(width)
                            .show(|ui| {
                                ui.set_min_width(width - 12.0);
                                egui::ScrollArea::vertical()
                                    .max_height(260.0)
                                    .show(ui, |ui| {
                                        for (index, preset) in
                                            plan_my_cabinet::material_presets::BR_STANDARD
                                                .iter()
                                                .enumerate()
                                        {
                                            let name = preset.name(language);
                                            if form::option(
                                                ui,
                                                current == Some(index),
                                                name,
                                                |ui| {
                                                    theme_widgets::swatch(
                                                        ui,
                                                        egui::Color32::from_rgb(
                                                            preset.color.0[0],
                                                            preset.color.0[1],
                                                            preset.color.0[2],
                                                        ),
                                                        egui::vec2(14.0, 14.0),
                                                    );
                                                    ui.label(egui::RichText::new(name).size(13.0));
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.label(
                                                                theme_widgets::mono(
                                                                    format!(
                                                                        "{} mm",
                                                                        preset.thickness_mm
                                                                    ),
                                                                    12.0,
                                                                )
                                                                .color(theme_widgets::FAINT),
                                                            );
                                                        },
                                                    );
                                                },
                                            )
                                            .clicked()
                                            {
                                                chosen = Some(index);
                                            }
                                        }
                                    });
                            });
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
                    form::gap(ui);
                    let thickness_width = 110.0;
                    let name_width = (width - thickness_width - 10.0).max(80.0);
                    let thickness = ui
                        .horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            ui.vertical(|ui| {
                                ui.set_width(name_width);
                                ui.spacing_mut().item_spacing.y = 6.0;
                                form::label(ui, &self.localizer.text("material-name-short"));
                                form::Input::new(
                                    egui::Id::new("material-creation-name"),
                                    &self.localizer.text("material-name"),
                                    name_width,
                                )
                                .text()
                                .show(ui, &mut draft.name);
                            });
                            unit_value_field(
                                ui,
                                &self.localizer,
                                egui::Id::new("material-creation-thickness"),
                                &self.localizer.text("board-thickness"),
                                &mut draft.thickness,
                                unit,
                                thickness_width,
                                None,
                            )
                        })
                        .inner;
                    form::gap(ui);
                    form::field(ui, &self.localizer.text("material-grain"), |ui| {
                        form::segmented(
                            ui,
                            "material-creation-grain",
                            &mut draft.grain,
                            &[
                                (BoardGrain::Length, &self.localizer.text("grain-length")),
                                (BoardGrain::Width, &self.localizer.text("grain-width")),
                                (BoardGrain::Unrestricted, &self.localizer.text("grain-none")),
                            ],
                            width,
                        );
                    });
                    form::gap(ui);
                    creation_color_swatches(ui, &self.localizer, &mut draft.color);
                    thickness
                };
                if draft.error {
                    form::error(ui, &self.localizer.text("error-create"));
                }
                if !current_project {
                    form::error(ui, &self.localizer.text("creation-project-changed"));
                }
                ((), valid && current_project)
            },
        );
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            chrome.close(ctx);
            if draft.kind == DialogKind::Material {
                self.modals.set_creation(self.suspended_board.take());
            }
            return;
        }
        if create_material {
            self.suspended_board = Some(draft);
            self.invoke_or_report(Request::new(A::NewMaterial));
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
                        self.design.first_fit_notice = actual.map(|preview| preview.fit);
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
                            default_band: None,
                            kind: Default::default(),
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
                        self.modals.set_creation(Some(board));
                    }
                    return;
                }
            }
            draft.error = true;
        }
        self.modals.set_creation(Some(draft));
    }
}
