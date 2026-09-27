use super::*;
use crate::actions::{ActionId as A, Argument, Request, Target};
use crate::modal_chrome::{ModalAction, ModalActions, ModalChrome};
use plan_my_cabinet::assembly_edit::{AssemblyEditError, world_pose};
use plan_my_cabinet::design_read_models::{
    DesignInspector, DesignReadModel, GrainProvenance, ObjectKind, OutlinerRow, ThicknessProvenance,
};
use plan_my_cabinet::edit_drafts::DraftError;
use plan_my_cabinet::stock_read_models::StockPieceReadModel;
#[cfg(test)]
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub(super) enum Operation {
    Group,
    Reparent,
    Ungroup,
    Transform,
    Duplicate,
}

pub(super) struct AssemblyDialog {
    operation: Operation,
    ids: Vec<Uuid>,
    active: Option<Uuid>,
    project_id: Uuid,
    revision: u64,
    focus: bool,
    chrome: ModalChrome,
    name: String,
    target: Option<Uuid>,
    pivot: [DimensionDraft; 3],
    translation: [DimensionDraft; 3],
    rotation: [String; 3],
    error: Option<String>,
}

pub(super) fn coordinate(field: &DimensionDraft) -> Option<f64> {
    let parsed = parse_length(&field.text, Unit::Mm).ok()?.conversion;
    (parsed.exact().is_some() || field.consent)
        .then(|| parsed.suggested().micrometres() as f64 / 1000.0)
}

fn angle(text: &str) -> Option<f64> {
    if text.contains('.') && text.contains(',') {
        return None;
    }
    let value = text.replace(',', ".").parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

/// A measured-stock sketch, not an independent feasibility proof. The same
/// cached read model supplies Cut plan labels and witness-aware summaries.
fn sheet_miniature(
    ui: &mut egui::Ui,
    piece: &StockPieceReadModel,
    board: Uuid,
    localizer: &Localizer,
) -> egui::Response {
    let long = piece.length.micrometres() as f32;
    let wide = piece.width.micrometres() as f32;
    let width = ui.available_width().clamp(120.0, 250.0);
    let height = (width * wide / long).clamp(64.0, 150.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(233, 221, 195));
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, theme_widgets::MUTED),
        egui::StrokeKind::Inside,
    );
    let scale_x = rect.width() / long;
    let scale_y = rect.height() / wide;
    let [left, right, bottom, top] = piece.trim.map(|v| v.micrometres() as f32);
    let usable = egui::Rect::from_min_max(
        rect.min + egui::vec2(left * scale_x, bottom * scale_y),
        rect.max - egui::vec2(right * scale_x, top * scale_y),
    );
    painter.rect_stroke(
        usable,
        0.0,
        egui::Stroke::new(1.0, theme_widgets::MUTED),
        egui::StrokeKind::Inside,
    );
    for part in &piece.parts {
        let (length, width) = if part.quarter_turn {
            (part.width, part.length)
        } else {
            (part.length, part.width)
        };
        let pos = rect.min
            + egui::vec2(
                part.origin[0].micrometres() as f32 * scale_x,
                part.origin[1].micrometres() as f32 * scale_y,
            );
        let footprint = egui::Rect::from_min_size(
            pos,
            egui::vec2(
                length.micrometres() as f32 * scale_x,
                width.micrometres() as f32 * scale_y,
            ),
        );
        if footprint.intersects(rect) {
            painter.rect_filled(
                footprint,
                1.0,
                if part.board_id == board {
                    theme_widgets::ACCENT
                } else {
                    egui::Color32::from_rgb(189, 169, 130)
                },
            );
            painter.rect_stroke(
                footprint,
                1.0,
                egui::Stroke::new(1.0, theme_widgets::PANEL),
                egui::StrokeKind::Inside,
            );
        }
    }
    let label = format!(
        "{} · {} · {}",
        localizer.text("sheet-heading"),
        piece.alias,
        piece.name
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &label)
    });
    response.on_hover_text(label)
}

fn hud_rect(canvas: egui::Rect) -> Option<egui::Rect> {
    if canvas.width() < 260.0 || canvas.height() < 150.0 {
        return None;
    }
    let width = (canvas.width() - 24.0).min(460.0);
    Some(egui::Rect::from_min_size(
        egui::pos2(canvas.center().x - width / 2.0, canvas.bottom() - 60.0),
        egui::vec2(width, 44.0),
    ))
}

fn rotation(degrees: [f64; 3]) -> Option<Quaternion> {
    let axis = |axis: usize, degrees: f64| {
        let half = degrees.to_radians() / 2.0;
        let mut vector = [0.0; 3];
        vector[axis] = half.sin();
        Quaternion::normalized(half.cos(), vector[0], vector[1], vector[2]).ok()
    };
    axis(2, degrees[2])?
        .compose(axis(1, degrees[1])?)
        .ok()?
        .compose(axis(0, degrees[0])?)
        .ok()
}

#[cfg(test)]
fn hierarchy(project: &Project) -> Vec<(Uuid, usize, bool, String)> {
    fn visit(
        project: &Project,
        parent: Option<Uuid>,
        depth: usize,
        out: &mut Vec<(Uuid, usize, bool, String)>,
        seen: &mut HashSet<Uuid>,
    ) {
        for a in project.assemblies.iter().filter(|a| a.parent_id == parent) {
            if seen.insert(a.id) {
                out.push((a.id, depth, true, a.name.clone()));
                visit(project, Some(a.id), depth + 1, out, seen);
            }
        }
        for b in project.boards.iter().filter(|b| b.parent_id == parent) {
            out.push((b.id, depth, false, b.name.clone()));
        }
        for h in project.hardware.iter().filter(|h| h.parent_id == parent) {
            out.push((h.id, depth, false, h.name.clone()));
        }
    }
    let mut out = Vec::new();
    visit(project, None, 0, &mut out, &mut HashSet::new());
    out
}

impl AssemblyDialog {
    pub(super) fn new(app: &DesktopApp, operation: Operation) -> Self {
        let ids: Vec<_> = app.selection.ids.iter().copied().collect();
        let origin = app
            .selection
            .active
            .and_then(|id| world_pose(app.editor.project(), id).ok())
            .map_or([0.0; 3], |pose| pose.translation_mm);
        let field = |value: f64| DimensionDraft {
            text: format!("{value:.3}"),
            consent: false,
        };
        Self {
            operation,
            ids,
            active: app.selection.active,
            project_id: app.editor.project().id,
            revision: app.editor.project().revision,
            focus: true,
            chrome: match operation {
                Operation::Group => ModalChrome::new(egui::Id::new("assembly-hierarchy-dialog"))
                    .first_focus(egui::Id::new("assembly-hierarchy-name")),
                Operation::Duplicate => {
                    ModalChrome::new(egui::Id::new("assembly-hierarchy-dialog"))
                        .first_focus(egui::Id::new("assembly-duplicate-x"))
                }
                Operation::Transform => {
                    ModalChrome::new(egui::Id::new("assembly-transform-dialog"))
                        .first_focus(egui::Id::new("assembly-transform-pivot-x"))
                }
                _ => ModalChrome::new(egui::Id::new("assembly-hierarchy-dialog")),
            }
            .width(520.0),
            name: String::new(),
            target: None,
            pivot: origin.map(field),
            translation: [field(0.0), field(0.0), field(0.0)],
            rotation: ["0".into(), "0".into(), "0".into()],
            error: None,
        }
    }
}

fn object_parent(project: &Project, id: Uuid) -> Option<Option<Uuid>> {
    project
        .assemblies
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.parent_id)
        .or_else(|| {
            project
                .boards
                .iter()
                .find(|b| b.id == id)
                .map(|b| b.parent_id)
        })
        .or_else(|| {
            project
                .hardware
                .iter()
                .find(|h| h.id == id)
                .map(|h| h.parent_id)
        })
}

fn object_label(project: &Project, localizer: &Localizer, id: Uuid) -> Option<String> {
    project
        .assemblies
        .iter()
        .find(|a| a.id == id)
        .map(|a| format!("{} · {}", localizer.text("assembly-kind"), a.name))
        .or_else(|| {
            project
                .boards
                .iter()
                .find(|b| b.id == id)
                .map(|b| format!("{} · {}", localizer.text("board-kind"), b.name))
        })
        .or_else(|| {
            project
                .hardware
                .iter()
                .find(|h| h.id == id)
                .map(|h| format!("{} · {}", localizer.text("hardware-kind"), h.name))
        })
}

fn within(project: &Project, id: Uuid, ancestor: Uuid) -> bool {
    let mut current = Some(id);
    while let Some(node) = current {
        if node == ancestor {
            return true;
        }
        current = object_parent(project, node).flatten();
    }
    false
}

fn affected_objects(project: &Project, draft: &AssemblyDialog) -> Vec<(Uuid, usize)> {
    let mut roots: Vec<_> = if matches!(draft.operation, Operation::Ungroup | Operation::Duplicate)
    {
        draft.active.into_iter().collect()
    } else {
        draft
            .ids
            .iter()
            .copied()
            .filter(|id| {
                !draft
                    .ids
                    .iter()
                    .any(|other| other != id && within(project, *id, *other))
            })
            .collect()
    };
    roots.sort_unstable();
    fn visit(project: &Project, id: Uuid, depth: usize, rows: &mut Vec<(Uuid, usize)>) {
        rows.push((id, depth));
        for child in project
            .assemblies
            .iter()
            .filter(|a| a.parent_id == Some(id))
            .map(|a| a.id)
            .chain(
                project
                    .boards
                    .iter()
                    .filter(|b| b.parent_id == Some(id))
                    .map(|b| b.id),
            )
            .chain(
                project
                    .hardware
                    .iter()
                    .filter(|h| h.parent_id == Some(id))
                    .map(|h| h.id),
            )
        {
            visit(project, child, depth + 1, rows);
        }
    }
    let mut rows = Vec::new();
    for id in roots {
        visit(project, id, 0, &mut rows);
    }
    rows
}

fn hierarchy_target_valid(project: &Project, draft: &AssemblyDialog) -> bool {
    let ids = affected_objects(project, draft);
    !ids.is_empty()
        && ids
            .iter()
            .all(|(id, _)| object_parent(project, *id).is_some())
        && draft.target.is_none_or(|target| {
            project.assemblies.iter().any(|a| a.id == target)
                && !draft.ids.iter().any(|id| within(project, target, *id))
        })
}

/// Collapse only the display: diagnostics and revealable descendants remain in
/// the projection, independently of visibility and selection.
fn visible_rows(rows: &[OutlinerRow]) -> Vec<&OutlinerRow> {
    let mut expanded = std::collections::HashMap::new();
    rows.iter()
        .filter(|row| {
            let shown = row
                .parent_id
                .is_none_or(|parent| expanded.get(&parent).copied().unwrap_or(false));
            expanded.insert(row.id, shown && row.expanded.unwrap_or(true));
            shown
        })
        .collect()
}

fn design_section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(7.0);
    ui.separator();
    ui.add_space(3.0);
    ui.label(
        egui::RichText::new(label.to_uppercase())
            .font(design_font(
                ui,
                "Section",
                crate::theme::SECTION.size,
                false,
            ))
            .color(theme_widgets::FAINT),
    );
}

fn design_font(ui: &egui::Ui, style: &str, size: f32, mono: bool) -> egui::FontId {
    ui.style()
        .text_styles
        .get(&egui::TextStyle::Name(style.into()))
        .cloned()
        .unwrap_or_else(|| {
            if mono {
                egui::FontId::monospace(size)
            } else {
                egui::FontId::proportional(size)
            }
        })
}

fn design_icon_button(
    ui: &mut egui::Ui,
    icon: icons::Icon,
    label: &str,
    color: egui::Color32,
    enabled: bool,
) -> egui::Response {
    let response = ui
        .add_enabled(
            enabled,
            egui::Button::image(icons::icon(icon, color, 14.0).alt_text(label))
                .frame(false)
                .min_size(egui::vec2(22.0, 24.0)),
        )
        .on_hover_text(label);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    response
}

fn material_swatch(ui: &mut egui::Ui, color: plan_my_cabinet::domain::SrgbColor) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        3.0,
        egui::Color32::from_rgb(color.0[0], color.0[1], color.0[2]),
        egui::Stroke::new(1.0, theme_widgets::BORDER_SOFT),
        egui::StrokeKind::Inside,
    );
}

fn short_length(length: Length, locale: Locale) -> String {
    let mm = length.micrometres() as f64 / 1000.0;
    let text = if mm.fract() == 0.0 {
        format!("{mm:.0}")
    } else {
        format!("{mm:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    };
    if locale == Locale::PtBr {
        text.replace('.', ",")
    } else {
        text
    }
}

fn inspector_section(ui: &mut egui::Ui, label: &str) {
    design_section(ui, label);
    ui.add_space(2.0);
}

fn inspector_value(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.set_min_height(24.0);
        ui.add_sized(
            [84.0, 20.0],
            egui::Label::new(egui::RichText::new(label).color(theme_widgets::MUTED)),
        );
        ui.label(egui::RichText::new(value.into()).font(design_font(
            ui,
            "MonoSmall",
            crate::theme::MONO_SMALL.size,
            true,
        )));
    });
}

fn show_design_pose(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    frame: &plan_my_cabinet::design_read_models::ObjectFrame,
    locale: Locale,
) {
    ui.separator();
    ui.label(format!(
        "{}: {}",
        localizer.text("design-local-frame"),
        frame
            .local_to
            .map_or_else(|| localizer.text("assembly-root"), |id| id.to_string())
    ));
    let position = frame.local_pose.translation_mm.map(|v| {
        if locale == Locale::PtBr {
            format!("{v:.3}").replace('.', ",")
        } else {
            format!("{v:.3}")
        }
    });
    ui.small(format!(
        "X / Y / Z: {} / {} / {} mm",
        position[0], position[1], position[2]
    ));
    let q = frame.local_pose.rotation;
    ui.small(format!(
        "{}: ({:.4}, {:.4}, {:.4}, {:.4})",
        localizer.text("design-local-rotation"),
        q.w,
        q.x,
        q.y,
        q.z
    ));
}

fn show_design_measurement(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    result: &Result<plan_my_cabinet::measurements::Measurement, MeasurementError>,
    label: &str,
) {
    match result {
        Ok(m) => {
            ui.small(format!(
                "{label}: {:.3} × {:.3} × {:.3} mm",
                m.dimensions_mm[0], m.dimensions_mm[1], m.dimensions_mm[2]
            ));
        }
        Err(MeasurementError::UndimensionedHardware(_)) => {
            ui.small(format!(
                "{label}: {}",
                localizer.text("measurement-unknown-hardware")
            ));
        }
        Err(_) => {
            ui.small(format!(
                "{label}: {}",
                localizer.text("measurement-invalid")
            ));
        }
    }
}

fn show_design_bounds(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    world: &plan_my_cabinet::design_read_models::ScopedBounds,
    object: &plan_my_cabinet::design_read_models::ScopedBounds,
) {
    ui.separator();
    ui.label(localizer.text("design-bounding"));
    for (bounds, frame) in [(world, "design-world"), (object, "design-object-frame")] {
        show_design_measurement(
            ui,
            localizer,
            &bounds.body,
            &format!(
                "{} · {}",
                localizer.text("design-body"),
                localizer.text(frame)
            ),
        );
        show_design_measurement(
            ui,
            localizer,
            &bounds.overall,
            &format!(
                "{} · {}",
                localizer.text("design-overall"),
                localizer.text(frame)
            ),
        );
    }
}

impl DesktopApp {
    /// Inspector and HUD borrow the same app-session board draft; neither owns
    /// or silently commits text on focus changes.
    fn show_shared_board_dimensions(&mut self, ui: &mut egui::Ui, id: Uuid, surface: &'static str) {
        let compact = surface == "hud";
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let unit = self.editor.project().display_unit;
        let editable = !self.modal_open()
            && !self
                .edit_drafts
                .existing_pose(self.editor.project().id, id)
                .is_some_and(|draft| draft.dirty());
        let mut apply = false;
        let mut discard = false;
        let mut error = None;
        ui.push_id((surface, id), |ui| {
            match self.edit_drafts.board(&self.editor, id, unit, locale) {
                Ok(draft) => {
                    let mut fields = |ui: &mut egui::Ui| {
                        for (axis, key) in ["board-length", "board-width"].into_iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(if compact {
                                    if axis == 0 { "L" } else { "W" }.to_owned()
                                } else {
                                    self.localizer.text(key)
                                });
                                let field = if axis == 0 {
                                    &mut draft.length
                                } else {
                                    &mut draft.width
                                };
                                let mut text =
                                    if compact && field.text.is_none() && field.unit == Unit::Mm {
                                        format_length(field.committed, Unit::Mm, field.locale, 2)
                                            .trim_end_matches(" mm")
                                            .to_owned()
                                    } else {
                                        field.display()
                                    };
                                let response = ui
                                    .add_enabled(
                                        editable,
                                        egui::TextEdit::singleline(&mut text)
                                            .desired_width(if compact { 72.0 } else { 104.0 }),
                                    )
                                    .on_hover_text(field.display());
                                if response.changed() {
                                    field.edit(text);
                                }
                                if response.has_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    apply = true;
                                }
                                if response.has_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Escape))
                                {
                                    discard = true;
                                }
                            });
                        }
                    };
                    if compact {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            fields(ui);
                        });
                    } else {
                        fields(ui);
                    }
                    if !compact {
                        ui.horizontal(|ui| {
                            ui.label(self.localizer.text("design-anchor"));
                            ui.add_enabled_ui(editable, |ui| {
                                egui::ComboBox::from_id_salt("draft-anchor")
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
                                            ui.selectable_value(
                                                &mut draft.anchor,
                                                anchor,
                                                self.localizer.text(key),
                                            );
                                        }
                                    })
                            });
                        });
                    }
                    if draft.dirty() {
                        error = draft.preview(&self.editor).err();
                        if !compact
                            && let Some(DraftError::RoundingConsent {
                                axis,
                                entered,
                                rounded_mm,
                            }) = &error
                        {
                            let mut args = FluentArgs::new();
                            args.set("entered", entered.as_str());
                            args.set("rounded", rounded_mm.as_str());
                            let label = self.localizer.format("rounding-confirmation", Some(&args));
                            let field = if *axis == 0 {
                                &mut draft.length
                            } else {
                                &mut draft.width
                            };
                            ui.add_enabled(
                                editable,
                                egui::Checkbox::new(&mut field.consent, label),
                            );
                            error = draft.preview(&self.editor).err();
                        }
                    }
                    if !compact && let Some(ref problem) = error {
                        let key = match problem {
                            DraftError::InvalidField { error, .. } => error_key(*error),
                            _ => "error-board-dimension",
                        };
                        if !matches!(problem, DraftError::RoundingConsent { .. }) {
                            ui.colored_label(theme_widgets::WARN_INK, self.localizer.text(key));
                        }
                    }
                    if !compact {
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    editable && draft.dirty() && error.is_none(),
                                    egui::Button::new(self.localizer.text("navigation-apply")),
                                )
                                .clicked()
                            {
                                apply = true;
                            }
                            if ui
                                .add_enabled(
                                    editable && draft.dirty(),
                                    egui::Button::new(self.localizer.text("navigation-discard")),
                                )
                                .clicked()
                            {
                                discard = true;
                            }
                        });
                    }
                }
                Err(_) => {
                    ui.colored_label(
                        theme_widgets::WARN_INK,
                        self.localizer.text("navigation-stale"),
                    );
                    if ui
                        .button(self.localizer.text("navigation-discard"))
                        .clicked()
                    {
                        discard = true;
                    }
                }
            }
        });
        if discard {
            self.edit_drafts.cancel_board(self.editor.project().id, id);
            ui.ctx().request_repaint();
        } else if apply
            && error.is_none()
            && let Some(draft) = self
                .edit_drafts
                .existing_board_mut(self.editor.project().id, id)
        {
            if draft.accept(&mut self.editor).is_err() {
                ui.colored_label(
                    theme_widgets::WARN_INK,
                    self.localizer.text("error-board-dimension"),
                );
            } else {
                ui.ctx().request_repaint();
            }
        }
    }

    pub(super) fn show_design_hud(&mut self, ctx: &egui::Context, canvas: egui::Rect) {
        let Some(model) = self.design_model() else {
            return;
        };
        let DesignInspector::Board(board) = &model.inspector else {
            return;
        };
        if self.selection.active != Some(board.id) || self.selection.ids.len() != 1 {
            return;
        }
        let Some(strip) = hud_rect(canvas) else {
            return;
        };
        let id = board.id;
        let name = board.name.clone();
        let thickness = board.dimensions[2];
        egui::Area::new(egui::Id::new("design-board-hud"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(strip.center_bottom())
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme_widgets::PANEL)
                    .stroke(egui::Stroke::new(1.0, theme_widgets::MUTED))
                    .inner_margin(egui::Margin::symmetric(8, 5))
                    .corner_radius(8.0)
                    .show(ui, |ui| {
                        ui.set_width(strip.width() - 16.0);
                        ui.set_max_width(strip.width() - 16.0);
                        ui.set_min_height(32.0);
                        if self
                            .edit_drafts
                            .existing_board(self.editor.project().id, id)
                            .is_some_and(|draft| draft.dirty())
                        {
                            egui::ScrollArea::vertical()
                                .max_height((canvas.height() - 130.0).max(80.0))
                                .show(ui, |ui| {
                                    self.show_shared_board_dimensions(ui, id, "hud-details")
                                });
                            ui.separator();
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            ui.add_sized(
                                [43.0, 24.0],
                                egui::Label::new(egui::RichText::new(&name).strong()).truncate(),
                            )
                            .on_hover_text(&name);
                            self.show_shared_board_dimensions(ui, id, "hud");
                            ui.label("×");
                            ui.label(format_length(
                                thickness,
                                self.editor.project().display_unit,
                                if self.localizer.language() == Language::En {
                                    Locale::En
                                } else {
                                    Locale::PtBr
                                },
                                2,
                            ))
                            .on_hover_text(self.localizer.text("board-thickness"));
                            ui.separator();
                            for (action, target, icon) in [
                                (A::PlaceFace, Target::Board(id), icons::Icon::Place),
                                (A::DuplicateBoard, Target::Board(id), icons::Icon::Duplicate),
                                (A::ToggleVisibility, Target::Object(id), icons::Icon::EyeOff),
                                (A::DeleteObject, Target::None, icons::Icon::Trash),
                            ] {
                                let request = Request::with(action, target);
                                let label = action.label(&self.localizer);
                                let response = design_icon_button(
                                    ui,
                                    icon,
                                    &label,
                                    theme_widgets::TEXT,
                                    self.action_availability(request).is_ok(),
                                );
                                #[cfg(test)]
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::new(("hud-action", action)),
                                        response.rect,
                                    )
                                });
                                if response.clicked() {
                                    let _ = self.invoke(request);
                                }
                            }
                        });
                    });
            });
    }

    fn show_shared_pose_fields(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let unit = self.editor.project().display_unit;
        let editable = !self.modal_open()
            && !self
                .edit_drafts
                .existing_board(self.editor.project().id, id)
                .is_some_and(|draft| draft.dirty());
        let mut requested_frame = self.pose_frame;
        ui.horizontal(|ui| {
            ui.label(self.localizer.text("placement-frame"));
            ui.selectable_value(
                &mut requested_frame,
                CoordinateFrame::LocalParent,
                self.localizer.text("placement-local"),
            );
            ui.selectable_value(
                &mut requested_frame,
                CoordinateFrame::World,
                self.localizer.text("placement-world"),
            );
        });
        if requested_frame != self.pose_frame {
            self.request_pose_frame(requested_frame);
            return;
        }
        let mut accept = false;
        let mut cancel = false;
        let mut error = None;
        ui.push_id(("inspector-pose", id), |ui| {
            match self
                .edit_drafts
                .pose(&self.editor, id, self.pose_frame, unit, locale)
            {
                Ok(draft) => {
                    ui.label(self.localizer.text("placement-position"));
                    ui.horizontal_wrapped(|ui| {
                        for (axis, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                            ui.label(label);
                            let field = &mut draft.position[axis];
                            let mut text = field.display();
                            let response = ui.add_enabled(
                                editable,
                                egui::TextEdit::singleline(&mut text).desired_width(84.0),
                            );
                            if response.changed() {
                                field.edit(text);
                            }
                            if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                accept = true;
                            }
                            if response.has_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                cancel = true;
                            }
                        }
                    });
                    ui.label(self.localizer.text("placement-rotation"));
                    ui.horizontal_wrapped(|ui| {
                        for (axis, label) in ["X°", "Y°", "Z°"].into_iter().enumerate() {
                            ui.label(label);
                            let mut text = draft.rotation[axis]
                                .clone()
                                .unwrap_or_else(|| format!("{:.2}", draft.rotation_degrees[axis]));
                            let response = ui.add_enabled(
                                editable,
                                egui::TextEdit::singleline(&mut text).desired_width(72.0),
                            );
                            if response.changed() {
                                draft.rotation[axis] = Some(text);
                            }
                            if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                accept = true;
                            }
                            if response.has_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Escape))
                            {
                                cancel = true;
                            }
                        }
                    });
                    if draft.dirty() {
                        error = draft.preview(&self.editor).err();
                        if let Some(DraftError::RoundingConsent {
                            axis,
                            entered,
                            rounded_mm,
                        }) = &error
                        {
                            let mut args = FluentArgs::new();
                            args.set("entered", entered.as_str());
                            args.set("rounded", rounded_mm.as_str());
                            let label = self.localizer.format("rounding-confirmation", Some(&args));
                            ui.add_enabled(
                                editable,
                                egui::Checkbox::new(&mut draft.position[*axis].consent, label),
                            );
                            error = draft.preview(&self.editor).err();
                        }
                        ui.small(self.localizer.text("placement-preview"));
                    }
                    if let Some(ref problem) = error {
                        let key = match problem {
                            DraftError::InvalidField { error, .. } => error_key(*error),
                            DraftError::InvalidRotation(_) => "error-invalid-rotation",
                            DraftError::Stale => "navigation-stale",
                            _ => "placement-invalid",
                        };
                        if !matches!(problem, DraftError::RoundingConsent { .. }) {
                            ui.colored_label(theme_widgets::WARN_INK, self.localizer.text(key));
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                editable && draft.dirty() && error.is_none(),
                                egui::Button::new(self.localizer.text("navigation-accept")),
                            )
                            .clicked()
                        {
                            accept = true;
                        }
                        if ui
                            .add_enabled(
                                editable && draft.dirty(),
                                egui::Button::new(self.localizer.text("navigation-cancel")),
                            )
                            .clicked()
                        {
                            cancel = true;
                        }
                    });
                }
                Err(_) => {
                    ui.colored_label(
                        theme_widgets::WARN_INK,
                        self.localizer.text("navigation-stale"),
                    );
                    if ui
                        .button(self.localizer.text("navigation-cancel"))
                        .clicked()
                    {
                        cancel = true;
                    }
                }
            }
        });
        if cancel {
            self.edit_drafts.cancel_pose(self.editor.project().id, id);
            ui.ctx().request_repaint();
        } else if accept
            && error.is_none()
            && let Some(draft) = self
                .edit_drafts
                .existing_pose_mut(self.editor.project().id, id)
        {
            if draft.accept(&mut self.editor).is_err() {
                ui.colored_label(
                    theme_widgets::WARN_INK,
                    self.localizer.text("placement-invalid"),
                );
            } else {
                self.edit_drafts.cancel_pose(self.editor.project().id, id);
                ui.ctx().request_repaint();
            }
        }
    }

    pub(super) fn show_hierarchy(&mut self, ui: &mut egui::Ui) {
        let Some(model) = self.design_model() else {
            ui.label(self.localizer.text("measurement-invalid"));
            return;
        };
        design_section(ui, &self.localizer.text("design-outliner-title"));
        let modal = self.modal_open();
        if model.outliner.is_empty() {
            ui.small(self.localizer.text("design-empty"));
        }
        for row in visible_rows(&model.outliner) {
            let fill = if row.active {
                theme_widgets::ACCENT_BG
            } else if row.selected {
                egui::Color32::from_rgb(251, 241, 227)
            } else {
                egui::Color32::TRANSPARENT
            };
            egui::Frame::new()
                .fill(fill)
                .corner_radius(5)
                .show(ui, |ui| {
                    ui.set_min_height(26.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.add_space(row.depth as f32 * 12.0);
                        if let Some(expanded) = row.expanded {
                            let label = self.localizer.text(if expanded {
                                "design-collapse"
                            } else {
                                "design-expand"
                            });
                            if design_icon_button(
                                ui,
                                if expanded {
                                    icons::Icon::ChevDown
                                } else {
                                    icons::Icon::ChevRight
                                },
                                &label,
                                theme_widgets::MUTED,
                                !modal,
                            )
                            .clicked()
                            {
                                if expanded {
                                    self.session.design_expanded.remove(&row.id);
                                } else {
                                    self.session.design_expanded.insert(row.id);
                                }
                            }
                        } else {
                            ui.add_space(22.0);
                        }
                        let symbol = match row.kind {
                            ObjectKind::Assembly => icons::Icon::Assembly,
                            ObjectKind::Board => icons::Icon::Board,
                            ObjectKind::Hardware => icons::Icon::Hinge,
                        };
                        ui.add(icons::icon(
                            symbol,
                            if row.active {
                                theme_widgets::ACCENT
                            } else {
                                theme_widgets::MUTED
                            },
                            13.0,
                        ));
                        let kind = match row.kind {
                            ObjectKind::Assembly => "assembly-kind",
                            ObjectKind::Board => "board-kind",
                            ObjectKind::Hardware => "hardware-kind",
                        };
                        let right = 22.0 + if row.issue_count > 0 { 24.0 } else { 0.0 };
                        let name_width = (ui.available_width() - right - 6.0).max(24.0);
                        let name = ui
                            .add_enabled(
                                !modal,
                                egui::Button::new(egui::RichText::new(&row.name).color(
                                    if row.active {
                                        theme_widgets::ACCENT_INK
                                    } else if !row.visible {
                                        theme_widgets::FAINT
                                    } else {
                                        theme_widgets::TEXT
                                    },
                                ))
                                .frame(false)
                                .min_size(egui::vec2(0.0, 24.0)),
                            )
                            .on_hover_text(format!("{} · {}", self.localizer.text(kind), row.id));
                        if name.clicked() {
                            let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                            let _ = self.invoke(
                                Request::with(A::SelectObject, Target::Object(row.id))
                                    .argument(Argument::Additive(additive)),
                            );
                        }
                        ui.add_space((name_width - name.rect.width()).max(0.0));
                        if row.issue_count > 0 {
                            ui.add(icons::icon(
                                icons::Icon::Warning,
                                theme_widgets::WARN_INK,
                                14.0,
                            ))
                            .on_hover_text(format!(
                                "{}: {}",
                                self.localizer.text("design-issue-count"),
                                row.issue_count
                            ));
                        }
                        let eye_label =
                            self.localizer.text(if row.hidden_directly || !row.visible {
                                "assembly-reveal"
                            } else {
                                "assembly-hide"
                            });
                        if design_icon_button(
                            ui,
                            if row.visible {
                                icons::Icon::Eye
                            } else {
                                icons::Icon::EyeOff
                            },
                            &eye_label,
                            theme_widgets::MUTED,
                            !modal,
                        )
                        .clicked()
                        {
                            let _ = self
                                .invoke(Request::with(A::ToggleVisibility, Target::Object(row.id)));
                        }
                    });
                });
        }
        let active_assembly = self
            .selection
            .active
            .is_some_and(|id| self.editor.project().assemblies.iter().any(|a| a.id == id));
        ui.collapsing(self.localizer.text("design-hierarchy-actions"), |ui| {
            ui.horizontal_wrapped(|ui| {
                for (operation, key, enabled) in [
                    (
                        Operation::Group,
                        "assembly-group",
                        !self.selection.ids.is_empty(),
                    ),
                    (
                        Operation::Reparent,
                        "assembly-reparent",
                        !self.selection.ids.is_empty(),
                    ),
                    (Operation::Ungroup, "assembly-ungroup", active_assembly),
                    (Operation::Duplicate, "assembly-duplicate", active_assembly),
                    (
                        Operation::Transform,
                        "assembly-transform",
                        !self.selection.ids.is_empty(),
                    ),
                ] {
                    if ui
                        .add_enabled(
                            !modal && enabled,
                            egui::Button::new(self.localizer.text(key)),
                        )
                        .clicked()
                    {
                        let action = match operation {
                            Operation::Group => A::Group,
                            Operation::Reparent => A::Reparent,
                            Operation::Ungroup => A::Ungroup,
                            Operation::Duplicate => A::DuplicateAssembly,
                            Operation::Transform => A::Transform,
                        };
                        let _ = self.invoke(Request::new(action));
                    }
                }
            })
        });
        design_section(ui, &self.localizer.text("design-materials-title"));
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        for material in &model.materials {
            ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                material_swatch(ui, material.color);
                let detail = format!(
                    "{} · {} {} · {} {}",
                    short_length(material.default_thickness, locale),
                    material.board_count,
                    self.localizer.text("board-kind"),
                    material.stock_piece_count,
                    self.localizer.text("shell-stock-pieces")
                );
                let warning = material.unallocated_board_count;
                let trailing = 70.0 + if warning > 0 { 20.0 } else { 0.0 };
                let name_width = (ui.available_width() - trailing).max(30.0);
                let name = ui
                    .add_enabled(
                        !modal,
                        egui::Button::new(&material.name)
                            .frame(false)
                            .min_size(egui::vec2(0.0, 24.0)),
                    )
                    .on_hover_text(format!(
                        "{} · {detail} · {}: {warning} · {}",
                        material.id,
                        self.localizer.text("board-unallocated"),
                        self.localizer.text("material-edit")
                    ));
                if name.clicked() {
                    let _ = self.invoke(Request::with(
                        A::EditMaterial,
                        Target::Material(material.id),
                    ));
                }
                ui.add_space((name_width - name.rect.width()).max(0.0));
                if warning > 0 {
                    ui.add(icons::icon(
                        icons::Icon::Warning,
                        theme_widgets::WARN_INK,
                        14.0,
                    ))
                    .on_hover_text(format!(
                        "{}: {warning}",
                        self.localizer.text("board-unallocated")
                    ));
                }
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {}",
                        short_length(material.default_thickness, locale),
                        material.board_count
                    ))
                    .font(design_font(ui, "MonoId", crate::theme::MONO_ID.size, true))
                    .color(theme_widgets::MUTED),
                )
                .on_hover_text(detail);
            });
        }
        design_section(ui, &self.localizer.text("design-stock-title"));
        for stock in &model.stock {
            let source = self.localizer.text(match stock.source {
                plan_my_cabinet::domain::StockSource::Owned => "stock-owned",
                plan_my_cabinet::domain::StockSource::ToPurchase => "stock-purchase",
            });
            let size = format!(
                "{}×{}×{}",
                short_length(stock.size[0], locale),
                short_length(stock.size[1], locale),
                short_length(stock.size[2], locale)
            );
            ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(
                    egui::RichText::new(&stock.alias)
                        .font(design_font(ui, "MonoId", crate::theme::MONO_ID.size, true))
                        .color(if stock.part_count > 0 {
                            theme_widgets::ACCENT
                        } else {
                            theme_widgets::TEXT
                        }),
                );
                ui.label(
                    egui::RichText::new(format!("#{}", stock.global_rank))
                        .font(design_font(ui, "MonoId", crate::theme::MONO_ID.size, true))
                        .color(theme_widgets::FAINT),
                );
                ui.label(egui::RichText::new(&size).font(design_font(
                    ui,
                    "MonoId",
                    crate::theme::MONO_ID.size,
                    true,
                )))
                .on_hover_text(format!(
                    "{} · {} · {}: {} · {}",
                    stock.id,
                    stock.name,
                    self.localizer.text("board-kind"),
                    stock.part_count,
                    source
                ));
                ui.label(
                    egui::RichText::new(&source)
                        .color(theme_widgets::FAINT)
                        .size(11.0),
                )
                .on_hover_text(&stock.name);
            });
        }
        if ui
            .add_enabled(
                !self.modal_open(),
                egui::Button::new(self.localizer.text("stock-new")),
            )
            .clicked()
        {
            let _ = self.invoke(Request::new(A::NewStock));
        }
    }

    pub(super) fn show_design_inspector(&mut self, ui: &mut egui::Ui, model: &DesignReadModel) {
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let fmt = |value| format_length(value, Unit::Mm, locale, 3);
        match &model.inspector {
            DesignInspector::None => {
                ui.heading(self.localizer.text("design-no-selection"));
                ui.small(self.localizer.text("measurement-empty"));
            }
            DesignInspector::Board(board) => {
                ui.label(egui::RichText::new(&board.name).font(design_font(
                    ui,
                    "Title",
                    crate::theme::TITLE.size,
                    false,
                )));
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {}",
                        self.localizer.text("board-kind"),
                        board
                            .parent_name
                            .as_deref()
                            .unwrap_or(&self.localizer.text("assembly-root"))
                    ))
                    .color(theme_widgets::MUTED),
                );
                ui.small(board.id.to_string())
                    .on_hover_text(board.id.to_string());
                inspector_section(ui, &self.localizer.text("board-kind"));
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [84.0, 20.0],
                        egui::Label::new(
                            egui::RichText::new(self.localizer.text("design-material-title"))
                                .color(theme_widgets::MUTED),
                        ),
                    );
                    material_swatch(ui, board.material_color);
                    ui.label(&board.material_name);
                });
                let grain_origin = self.localizer.text(match board.grain_provenance {
                    GrainProvenance::MaterialDefault => "design-grain-default",
                    GrainProvenance::BoardOverride => "design-grain-override",
                });
                inspector_value(
                    ui,
                    &self.localizer.text("design-grain-title"),
                    self.localizer.text(grain_key(board.grain)),
                );
                ui.small(grain_origin);
                let source = self
                    .editor
                    .project()
                    .boards
                    .iter()
                    .find(|b| b.id == board.id)
                    .expect("inspected board");
                let mut grain = source.grain_override;
                ui.add_enabled_ui(!self.modal_open(), |ui| {
                    egui::ComboBox::from_id_salt(("inspector-grain", board.id))
                        .selected_text(self.localizer.text(board_grain_key(grain)))
                        .show_ui(ui, |ui| {
                            for (value, key) in [
                                (None, "grain-follow-default"),
                                (Some(BoardGrain::Length), "grain-length"),
                                (Some(BoardGrain::Width), "grain-width"),
                                (Some(BoardGrain::Unrestricted), "grain-unrestricted"),
                            ] {
                                combo_option(ui, &mut grain, value, self.localizer.text(key));
                            }
                        });
                });
                if grain != source.grain_override {
                    let _ = self.invoke(
                        Request::with(A::SetGrain, Target::Board(board.id))
                            .argument(Argument::Grain(grain)),
                    );
                }
                inspector_section(ui, &self.localizer.text("design-dimensions-title"));
                self.show_shared_board_dimensions(ui, board.id, "inspector");
                inspector_value(
                    ui,
                    &self.localizer.text("board-thickness"),
                    fmt(board.dimensions[2]),
                );
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("value", fmt(board.material_default_thickness));
                ui.small(self.localizer.format(
                    match board.thickness_provenance {
                        ThicknessProvenance::MatchesCurrentMaterialDefault => {
                            "design-thickness-stored-note"
                        }
                        ThicknessProvenance::StoredBoardValueDiffers => {
                            "design-thickness-diff-note"
                        }
                    },
                    Some(&args),
                ));
                inspector_section(ui, &self.localizer.text("design-transform-title"));
                let world = self.localizer.text("design-world");
                let frame_name = board.parent_name.as_deref().unwrap_or(&world);
                inspector_value(ui, &self.localizer.text("design-frame-title"), frame_name);
                inspector_value(
                    ui,
                    "X / Y / Z",
                    format!(
                        "{} / {} / {} mm",
                        board.frame.local_pose.translation_mm[0],
                        board.frame.local_pose.translation_mm[1],
                        board.frame.local_pose.translation_mm[2]
                    ),
                );
                ui.collapsing(self.localizer.text("placement-numeric"), |ui| {
                    self.show_shared_pose_fields(ui, board.id);
                });
                inspector_section(ui, &self.localizer.text("design-stock-title"));
                ui.label(self.localizer.text(match board.allocation.status {
                    AllocationStatus::Unallocated => "board-unallocated",
                    AllocationStatus::Conflicted => "global-conflicted",
                    AllocationStatus::UnknownSearchBudget => "sheet-feasibility-unknown",
                    AllocationStatus::AllocatedValid => "board-allocated",
                }));
                for allocation in &board.stock {
                    ui.label(format!(
                        "{} · ({}, {}) · {} · {}",
                        allocation.stock_alias.as_deref().unwrap_or("—"),
                        short_length(allocation.origin[0], locale),
                        short_length(allocation.origin[1], locale),
                        self.localizer.text(if allocation.locked {
                            "design-locked"
                        } else {
                            "design-unlocked"
                        }),
                        if allocation.quarter_turn {
                            "90°"
                        } else {
                            "0°"
                        }
                    ))
                    .on_hover_text(allocation.stock_id.to_string());
                    if let Some(piece) = self
                        .design_stock_snapshot
                        .as_ref()
                        .and_then(|(_, stock)| stock.miniature(allocation.stock_id))
                        && sheet_miniature(ui, piece, board.id, &self.localizer).clicked()
                        && !self.modal_open()
                    {
                        self.navigate_session(Destination::BoardAllocation(board.id));
                    }
                }
                if ui
                    .add_enabled(
                        !self.modal_open(),
                        egui::Button::new(self.localizer.text("sheet-heading")),
                    )
                    .clicked()
                {
                    self.navigate_session(Destination::BoardAllocation(board.id));
                }
                inspector_section(ui, &self.localizer.text("design-bounding"));
                show_design_measurement(
                    ui,
                    &self.localizer,
                    &board.bounds_world.body,
                    &format!(
                        "{} · {}",
                        self.localizer.text("design-body"),
                        self.localizer.text("design-world")
                    ),
                );
                ui.push_id("design-inspector-advanced", |ui| {
                    ui.collapsing(self.localizer.text("shell-advanced"), |ui| {
                        show_design_pose(ui, &self.localizer, &board.frame, locale);
                        show_design_bounds(
                            ui,
                            &self.localizer,
                            &board.bounds_world,
                            &board.bounds_object,
                        );
                        for action in [
                            A::EditDimensions,
                            A::AssignMaterial,
                            A::PositionBoard,
                            A::PlaceFace,
                            A::DuplicateBoard,
                        ] {
                            let request = Request::with(action, Target::Board(board.id));
                            if actions::button(
                                ui,
                                &self.localizer,
                                request,
                                self.action_availability(request),
                            )
                            .clicked()
                            {
                                let _ = self.invoke(request);
                            }
                        }
                        let request = Request::new(A::DeleteObject);
                        if actions::button(
                            ui,
                            &self.localizer,
                            request,
                            self.action_availability(request),
                        )
                        .clicked()
                        {
                            let _ = self.invoke(request);
                        }
                    })
                });
            }
            DesignInspector::Assembly(assembly) => {
                ui.heading(&assembly.name);
                ui.small(assembly.id.to_string());
                ui.label(format!(
                    "{}: {}",
                    self.localizer.text("assembly-parent"),
                    assembly
                        .parent_name
                        .as_deref()
                        .unwrap_or(&self.localizer.text("assembly-root"))
                ));
                ui.label(format!(
                    "{}: {} · {}: {}",
                    self.localizer.text("board-kind"),
                    assembly.descendant_board_count,
                    self.localizer.text("design-issue-count"),
                    assembly.issue_count
                ));
                show_design_pose(ui, &self.localizer, &assembly.frame, locale);
                show_design_bounds(
                    ui,
                    &self.localizer,
                    &assembly.bounds_world,
                    &assembly.bounds_object,
                );
                for action in [
                    A::Ungroup,
                    A::DuplicateAssembly,
                    A::Transform,
                    A::DeleteObject,
                ] {
                    let request = Request::new(action);
                    if actions::button(
                        ui,
                        &self.localizer,
                        request,
                        self.action_availability(request),
                    )
                    .clicked()
                    {
                        let _ = self.invoke(request);
                    }
                }
            }
            DesignInspector::Multi(multi) => {
                ui.heading(self.localizer.text("design-multi-selection"));
                ui.label(format!(
                    "{}: {} · {}: {} · {}: {}",
                    self.localizer.text("board-kind"),
                    multi.board_count,
                    self.localizer.text("assembly-kind"),
                    multi.assembly_count,
                    self.localizer.text("hardware-kind"),
                    multi.hardware_count
                ));
                show_design_measurement(
                    ui,
                    &self.localizer,
                    &multi.bounds_world.body,
                    &format!(
                        "{} · {}",
                        self.localizer.text("design-body"),
                        self.localizer.text("design-world")
                    ),
                );
                show_design_measurement(
                    ui,
                    &self.localizer,
                    &multi.bounds_world.overall,
                    &format!(
                        "{} · {}",
                        self.localizer.text("design-overall"),
                        self.localizer.text("design-world")
                    ),
                );
                for action in [A::BatchDimensions, A::Group, A::Reparent, A::Transform] {
                    let request = Request::new(action);
                    if actions::button(
                        ui,
                        &self.localizer,
                        request,
                        self.action_availability(request),
                    )
                    .clicked()
                    {
                        let _ = self.invoke(request);
                    }
                }
            }
            DesignInspector::Hardware(hardware) => {
                ui.heading(&hardware.name);
                ui.small(hardware.id.to_string());
                show_design_pose(ui, &self.localizer, &hardware.frame, locale);
                if let Some(size) = hardware.dimensions {
                    ui.label(format!(
                        "X · {}   Y · {}   Z · {}",
                        fmt(size[0]),
                        fmt(size[1]),
                        fmt(size[2])
                    ));
                }
                show_design_measurement(
                    ui,
                    &self.localizer,
                    &hardware.bounds_world.overall,
                    &format!(
                        "{} · {}",
                        self.localizer.text("design-overall"),
                        self.localizer.text("design-world")
                    ),
                );
                let request = Request::new(A::Transform);
                if actions::button(
                    ui,
                    &self.localizer,
                    request,
                    self.action_availability(request),
                )
                .clicked()
                {
                    let _ = self.invoke(request);
                }
                if hardware.dimensions.is_some() {
                    for action in [A::EditHardware, A::DuplicateHardware] {
                        let request = Request::with(action, Target::Object(hardware.id));
                        if actions::button(
                            ui,
                            &self.localizer,
                            request,
                            self.action_availability(request),
                        )
                        .clicked()
                        {
                            let _ = self.invoke(request);
                        }
                    }
                }
            }
        }
        ui.separator();
        self.show_measurement(ui);
    }

    pub(super) fn show_assembly_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.assembly_dialog.take() else {
            return;
        };
        let cancel;
        let accept;
        let mut valid = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        if !matches!(draft.operation, Operation::Transform) {
            let project = self.editor.project();
            valid &= match draft.operation {
                Operation::Group | Operation::Reparent => true,
                Operation::Ungroup | Operation::Duplicate => draft
                    .active
                    .is_some_and(|id| project.assemblies.iter().any(|assembly| assembly.id == id)),
                Operation::Transform => unreachable!(),
            };
            let title = self.localizer.text(match draft.operation {
                Operation::Group => "assembly-group",
                Operation::Reparent => "assembly-reparent",
                Operation::Ungroup => "assembly-ungroup",
                Operation::Duplicate => "assembly-duplicate",
                Operation::Transform => unreachable!(),
            });
            let first_focus = draft.focus;
            let mut parent_focus = None;
            let mut chrome = std::mem::replace(
                &mut draft.chrome,
                ModalChrome::new(egui::Id::new("assembly-hierarchy-dialog")),
            );
            let action = chrome
                .show(
                    ctx,
                    &title,
                    ModalActions {
                        cancel: &self.localizer.text("cancel"),
                        confirm: &title,
                    },
                    |ui| {
                        if matches!(draft.operation, Operation::Group) {
                            ui.label(self.localizer.text("assembly-name"));
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut draft.name)
                                    .id(egui::Id::new("assembly-hierarchy-name")),
                            );
                            if first_focus {
                                response.request_focus();
                            }
                            valid &= !draft.name.trim().is_empty();
                        }
                        if matches!(draft.operation, Operation::Group | Operation::Reparent) {
                            let combo =
                                egui::ComboBox::from_label(self.localizer.text("assembly-parent"))
                                    .selected_text(
                                        draft
                                            .target
                                            .and_then(|id| {
                                                project
                                                    .assemblies
                                                    .iter()
                                                    .find(|a| a.id == id)
                                                    .map(|a| a.name.as_str())
                                            })
                                            .unwrap_or(&self.localizer.text("assembly-root")),
                                    )
                                    .show_ui(ui, |ui| {
                                        combo_option(
                                            ui,
                                            &mut draft.target,
                                            None,
                                            self.localizer.text("assembly-root"),
                                        );
                                        for assembly in &project.assemblies {
                                            combo_option(
                                                ui,
                                                &mut draft.target,
                                                Some(assembly.id),
                                                &assembly.name,
                                            );
                                        }
                                    });
                            if first_focus && matches!(draft.operation, Operation::Reparent) {
                                parent_focus = Some(combo.response.id);
                            }
                            // Recompute after a target change in this frame, before the footer
                            // can become active by click or Enter.
                            valid &= hierarchy_target_valid(project, &draft);
                        }
                        if matches!(draft.operation, Operation::Group) {
                            ui.label(self.localizer.text("assembly-pivot"));
                            for (axis, field) in ["X", "Y", "Z"].into_iter().zip(&mut draft.pivot) {
                                valid &= assembly_coordinate_field(
                                    ui,
                                    &self.localizer,
                                    axis,
                                    field,
                                    false,
                                );
                            }
                        }
                        if matches!(draft.operation, Operation::Duplicate) {
                            ui.label(self.localizer.text("assembly-translation"));
                            for (axis, field) in
                                ["X", "Y", "Z"].into_iter().zip(&mut draft.translation)
                            {
                                if axis == "X" {
                                    valid &= assembly_coordinate_field_with_id(
                                        ui,
                                        &self.localizer,
                                        axis,
                                        field,
                                        false,
                                        Some(egui::Id::new("assembly-duplicate-x")),
                                    );
                                } else {
                                    valid &= assembly_coordinate_field(
                                        ui,
                                        &self.localizer,
                                        axis,
                                        field,
                                        false,
                                    );
                                }
                            }
                        }
                        ui.add_space(8.0);
                        theme_widgets::section_header(
                            ui,
                            &self.localizer.text("assembly-hierarchy"),
                        );
                        if matches!(draft.operation, Operation::Duplicate) {
                            ui.small(self.localizer.text("assembly-duplicate-stock-note"));
                            ui.small(self.localizer.text("assembly-duplicate-relationship-note"));
                        }
                        let rows = affected_objects(project, &draft);
                        for (id, depth) in &rows {
                            if let Some(label) = object_label(project, &self.localizer, *id) {
                                ui.label(format!("{}{} · {}", "  ".repeat(*depth), label, id));
                            }
                        }
                        if matches!(draft.operation, Operation::Ungroup | Operation::Duplicate)
                            && let Some(root) = draft.active
                            && let Some(parent) = object_parent(project, root)
                        {
                            ui.label(format!(
                                "{}: {}",
                                self.localizer.text("assembly-parent"),
                                parent
                                    .and_then(|id| project
                                        .assemblies
                                        .iter()
                                        .find(|a| a.id == id)
                                        .map(|a| a.name.clone()))
                                    .unwrap_or_else(|| self.localizer.text("assembly-root"))
                            ));
                        }
                        let board_ids: std::collections::HashSet<_> = rows
                            .iter()
                            .filter_map(|(id, _)| {
                                project.boards.iter().any(|b| b.id == *id).then_some(*id)
                            })
                            .collect();
                        let allocation_count = project
                            .allocations
                            .iter()
                            .filter(|allocation| board_ids.contains(&allocation.board_id))
                            .count();
                        let hinge_count = project
                            .hinge_installations
                            .iter()
                            .filter(|hinge| {
                                board_ids.contains(&hinge.door_board_id)
                                    || board_ids.contains(&hinge.mounting_board_id)
                            })
                            .count();
                        let joint_count = project
                            .door_joints
                            .iter()
                            .filter(|joint| {
                                rows.iter().any(|(id, _)| *id == joint.moving_root_id)
                                    || board_ids.contains(&joint.mounting_board_id)
                            })
                            .count();
                        if allocation_count > 0 {
                            ui.collapsing(
                                format!(
                                    "{}: {allocation_count}",
                                    self.localizer.text("door-allocations")
                                ),
                                |ui| {
                                    for allocation in project
                                        .allocations
                                        .iter()
                                        .filter(|a| board_ids.contains(&a.board_id))
                                    {
                                        let board = object_label(
                                            project,
                                            &self.localizer,
                                            allocation.board_id,
                                        )
                                        .unwrap_or_else(|| allocation.board_id.to_string());
                                        let stock = project
                                            .stock
                                            .iter()
                                            .find(|s| s.id == allocation.stock_id)
                                            .map_or_else(
                                                || allocation.stock_id.to_string(),
                                                |s| s.name.clone(),
                                            );
                                        ui.label(format!("{board} → {stock}"));
                                    }
                                },
                            );
                        }
                        if hinge_count > 0 {
                            ui.collapsing(
                                format!("{}: {hinge_count}", self.localizer.text("door-hinges")),
                                |ui| {
                                    for hinge in project.hinge_installations.iter().filter(|h| {
                                        board_ids.contains(&h.door_board_id)
                                            || board_ids.contains(&h.mounting_board_id)
                                    }) {
                                        ui.label(format!(
                                            "{} · {} / {}",
                                            hinge.id,
                                            object_label(
                                                project,
                                                &self.localizer,
                                                hinge.door_board_id
                                            )
                                            .unwrap_or_default(),
                                            object_label(
                                                project,
                                                &self.localizer,
                                                hinge.mounting_board_id
                                            )
                                            .unwrap_or_default()
                                        ));
                                    }
                                },
                            );
                        }
                        if joint_count > 0 {
                            ui.collapsing(
                                format!("{}: {joint_count}", self.localizer.text("door-joints")),
                                |ui| {
                                    for joint in project.door_joints.iter().filter(|j| {
                                        rows.iter().any(|(id, _)| *id == j.moving_root_id)
                                            || board_ids.contains(&j.mounting_board_id)
                                    }) {
                                        ui.label(format!(
                                            "{} · {} / {}",
                                            joint.id,
                                            object_label(
                                                project,
                                                &self.localizer,
                                                joint.moving_root_id
                                            )
                                            .unwrap_or_default(),
                                            object_label(
                                                project,
                                                &self.localizer,
                                                joint.mounting_board_id
                                            )
                                            .unwrap_or_default()
                                        ));
                                    }
                                },
                            );
                        }
                        if !valid || draft.error.is_some() {
                            ui.colored_label(
                                theme_widgets::DANGER,
                                draft
                                    .error
                                    .as_deref()
                                    .unwrap_or(&self.localizer.text("assembly-invalid")),
                            );
                        }
                        ((), valid)
                    },
                )
                .action;
            if let Some(id) = parent_focus {
                ctx.memory_mut(|m| m.request_focus(id));
            }
            draft.chrome = chrome;
            cancel = action == ModalAction::Cancel;
            accept = action == ModalAction::Confirm;
        } else {
            let title = self.localizer.text("assembly-transform");
            let mut chrome = std::mem::replace(
                &mut draft.chrome,
                ModalChrome::new(egui::Id::new("assembly-transform-dialog")),
            );
            let result = chrome.show(
                ctx,
                &title,
                ModalActions {
                    cancel: &self.localizer.text("cancel"),
                    confirm: &self.localizer.text("confirm"),
                },
                |ui| {
                    if matches!(draft.operation, Operation::Group | Operation::Transform) {
                        ui.label(self.localizer.text("assembly-pivot"));
                        for (axis, field) in ["X", "Y", "Z"].into_iter().zip(&mut draft.pivot) {
                            valid &= assembly_coordinate_field_with_id(
                                ui,
                                &self.localizer,
                                axis,
                                field,
                                false,
                                (axis == "X").then(|| egui::Id::new("assembly-transform-pivot-x")),
                            );
                        }
                    }
                    if matches!(draft.operation, Operation::Transform | Operation::Duplicate) {
                        ui.label(self.localizer.text("assembly-translation"));
                        for (axis, field) in ["X", "Y", "Z"].into_iter().zip(&mut draft.translation)
                        {
                            valid &=
                                assembly_coordinate_field(ui, &self.localizer, axis, field, false);
                        }
                    }
                    if matches!(draft.operation, Operation::Transform) {
                        ui.label(self.localizer.text("assembly-rotation"));
                        for (axis, text) in ["X", "Y", "Z"].into_iter().zip(&mut draft.rotation) {
                            ui.horizontal(|ui| {
                                ui.label(format!("{axis} (°)"));
                                ui.text_edit_singleline(text);
                            });
                            valid &= angle(text).is_some();
                        }
                        valid &= draft.rotation.iter().all(|v| angle(v).is_some())
                            && rotation(draft.rotation.each_ref().map(|v| angle(v).unwrap_or(0.0)))
                                .is_some();
                    }
                    if !valid || draft.error.is_some() {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            draft
                                .error
                                .as_deref()
                                .unwrap_or(&self.localizer.text("assembly-invalid")),
                        );
                    }
                    ((), valid)
                },
            );
            draft.chrome = chrome;
            cancel = result.action == ModalAction::Cancel;
            accept = result.action == ModalAction::Confirm;
        }
        draft.focus = false;
        if actions::decision(A::CancelDialog, cancel) {
            draft.chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, accept) {
            let pivot = draft.pivot.each_ref().map(|v| coordinate(v).unwrap_or(0.0));
            let result = match draft.operation {
                Operation::Group => self
                    .editor
                    .group_objects(&draft.ids, draft.target, draft.name.clone(), pivot)
                    .map(|id| {
                        self.selection.choose(Some(id), false);
                    }),
                Operation::Reparent => self
                    .editor
                    .reparent_objects(&draft.ids, draft.target)
                    .map(|_| ()),
                Operation::Ungroup => self
                    .editor
                    .ungroup_assembly(draft.active.expect("active assembly at dialog opening"))
                    .map(|_| {
                        self.selection.choose(None, false);
                    }),
                Operation::Transform => {
                    let delta = draft
                        .translation
                        .each_ref()
                        .map(|v| coordinate(v).unwrap_or(0.0));
                    let angles = draft.rotation.each_ref().map(|v| angle(v).unwrap_or(0.0));
                    self.editor
                        .transform_selection(&draft.ids, delta, rotation(angles).unwrap(), pivot)
                        .map(|_| ())
                }
                Operation::Duplicate => {
                    let offset = draft
                        .translation
                        .each_ref()
                        .map(|v| coordinate(v).unwrap_or(0.0));
                    self.editor
                        .duplicate_assembly(
                            draft.active.expect("active assembly at dialog opening"),
                            offset,
                        )
                        .map(|id| self.selection.choose(Some(id), false))
                }
            };
            match result {
                Ok(()) => {
                    draft.chrome.close(ctx);
                    return;
                }
                Err(plan_my_cabinet::commands::EditError::Command(AssemblyEditError::Cycle(_))) => {
                    draft.error = Some(self.localizer.text("assembly-cycle"))
                }
                Err(_) => draft.error = Some(self.localizer.text("assembly-invalid")),
            }
        }
        self.assembly_dialog = Some(draft);
    }
}

pub(super) fn assembly_coordinate_field(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    axis: &str,
    field: &mut DimensionDraft,
    focus: bool,
) -> bool {
    assembly_coordinate_field_with_id(ui, localizer, axis, field, focus, None)
}

fn assembly_coordinate_field_with_id(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    axis: &str,
    field: &mut DimensionDraft,
    focus: bool,
    id: Option<egui::Id>,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(format!("{axis} (mm)"));
        let edit = egui::TextEdit::singleline(&mut field.text);
        let response = ui.add(if let Some(id) = id { edit.id(id) } else { edit });
        if focus {
            response.request_focus();
        }
        if response.changed() {
            field.consent = false;
        }
    });
    if let Ok(parsed) = parse_length(&field.text, Unit::Mm)
        && matches!(parsed.conversion, Conversion::NeedsConfirmation(_))
    {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("entered", field.text.as_str());
        args.set(
            "rounded",
            format_length(
                parsed.conversion.suggested(),
                Unit::Mm,
                if localizer.language() == Language::En {
                    Locale::En
                } else {
                    Locale::PtBr
                },
                3,
            ),
        );
        ui.checkbox(
            &mut field.consent,
            localizer.format("rounding-confirmation", Some(&args)),
        );
    }
    coordinate(field).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};

    fn app_with_board() -> (DesktopApp, Uuid) {
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Wood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let board = app
            .editor
            .create_board(NewBoard {
                name: "Side".into(),
                material_id,
                length: Length::from_micrometres(100_000),
                width: Length::from_micrometres(50_000),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        (app, board)
    }

    #[test]
    fn inspector_and_hud_render_one_pending_edit_without_committing() {
        let (mut app, board) = app_with_board();
        app.request_scene_selection(Some(board), false);
        assert_eq!(app.selection.active, Some(board));
        assert!(matches!(
            app.design_model().unwrap().inspector,
            DesignInspector::Board(_)
        ));
        let revision = app.editor.project().revision;
        app.edit_drafts
            .board(&app.editor, board, Unit::Mm, Locale::En)
            .unwrap()
            .length
            .edit("bad");
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_shared_board_dimensions(ui, board, "inspector");
            app.show_shared_board_dimensions(ui, board, "hud");
        });
        output.drop_without_applying_deltas();
        let draft = app
            .edit_drafts
            .existing_board(app.editor.project().id, board)
            .unwrap();
        assert_eq!(draft.length.display(), "bad");
        assert_eq!(app.editor.project().revision, revision);
        assert!(matches!(
            draft.values(),
            Err(DraftError::InvalidField { axis: 0, .. })
        ));
    }

    #[test]
    fn inspector_pose_fields_are_session_only_and_keep_face_placement_available() {
        let (mut app, board) = app_with_board();
        let material_id = app.editor.project().materials[0].id;
        app.editor
            .create_board(NewBoard {
                name: "Target".into(),
                material_id,
                length: Length::from_micrometres(100_000),
                width: Length::from_micrometres(50_000),
                pose: Pose::new([200.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        app.request_scene_selection(Some(board), false);
        let original = app.editor.project().clone();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_shared_pose_fields(ui, board)
        })
        .drop_without_applying_deltas();
        assert_eq!(app.editor.project(), &original);
        assert!(
            !app.edit_drafts
                .existing_pose(original.id, board)
                .unwrap()
                .dirty()
        );
        assert!(
            app.action_availability(Request::with(A::PlaceFace, Target::Board(board)))
                .is_ok()
        );
        app.edit_drafts
            .existing_pose_mut(original.id, board)
            .unwrap()
            .position[0]
            .edit("bad");
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_shared_pose_fields(ui, board)
        })
        .drop_without_applying_deltas();
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
    fn inspector_miniature_uses_same_stock_identity_and_exact_part_records() {
        let project = plan_my_cabinet::reference_fixture::project();
        let unchanged = project.clone();
        let stock = StockReadModel::build(&project).unwrap();
        let allocation = &project.allocations[0];
        let piece = stock.miniature(allocation.stock_id).unwrap();
        assert!(piece.parts.iter().any(
            |part| part.board_id == allocation.board_id && part.allocation_id == allocation.id
        ));
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = sheet_miniature(
                ui,
                piece,
                allocation.board_id,
                &Localizer::new(Language::En),
            );
            assert!(response.rect.width() >= 120.0);
            assert!(response.rect.height() >= 64.0);
        });
        assert!(output.shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Rect(rect) => rect.fill == theme_widgets::ACCENT,
            _ => false,
        }));
        output.drop_without_applying_deltas();
        assert_eq!(project, unchanged);
    }

    #[test]
    fn miniature_pointer_activation_routes_one_board_to_its_real_cut_sheet() {
        let project = plan_my_cabinet::reference_fixture::project();
        let stock = StockReadModel::build(&project).unwrap();
        let allocation = &project.allocations[0];
        let piece = stock.miniature(allocation.stock_id).unwrap();
        let ctx = egui::Context::default();
        let mut point = egui::Pos2::ZERO;
        let first = ctx.run_ui(egui::RawInput::default(), |ui| {
            point = sheet_miniature(
                ui,
                piece,
                allocation.board_id,
                &Localizer::new(Language::En),
            )
            .rect
            .center();
        });
        first.drop_without_applying_deltas();
        let pointer = |pressed| egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        ctx.run_ui(pointer(true), |ui| {
            assert!(
                !sheet_miniature(
                    ui,
                    piece,
                    allocation.board_id,
                    &Localizer::new(Language::En)
                )
                .clicked()
            );
        })
        .drop_without_applying_deltas();
        let mut activated = false;
        let click = ctx.run_ui(pointer(false), |ui| {
            activated = sheet_miniature(
                ui,
                piece,
                allocation.board_id,
                &Localizer::new(Language::En),
            )
            .clicked();
        });
        assert!(activated);
        click.drop_without_applying_deltas();
        let mut session = WorkspaceSession::new(&project);
        let mut selection = viewport::Selection::default();
        assert!(session.navigate(
            &project,
            &mut selection,
            Destination::BoardAllocation(allocation.board_id)
        ));
        assert_eq!(session.focused_sheet, Some(piece.id));
        assert_eq!(selection.ids, [allocation.board_id].into_iter().collect());
        assert_eq!(project, plan_my_cabinet::reference_fixture::project());
    }

    #[test]
    fn hud_geometry_centers_on_canvas_and_keeps_secondary_windows_clear() {
        for canvas in [
            egui::Rect::from_min_max(egui::pos2(316.0, 46.0), egui::pos2(1148.0, 875.0)),
            egui::Rect::from_min_max(egui::pos2(60.0, 46.0), egui::pos2(700.0, 625.0)),
            egui::Rect::from_min_max(egui::pos2(55.0, 40.0), egui::pos2(540.0, 565.0)),
        ] {
            let strip = hud_rect(canvas).unwrap();
            assert_eq!(strip.center().x, canvas.center().x);
            assert!(canvas.contains_rect(strip));
            assert_eq!(strip.height(), 44.0);
            assert!(strip.width() <= 460.0);
        }
        assert!(
            hud_rect(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(180.0, 400.0)
            ))
            .is_none()
        );
    }

    #[test]
    fn board_hud_only_renders_for_one_active_board_and_preserves_invalid_draft() {
        let (mut app, board) = app_with_board();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let canvas = egui::Rect::from_min_max(egui::pos2(316.0, 46.0), egui::pos2(1148.0, 875.0));
        let hud_id = egui::Id::new("design-board-hud");
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_design_hud(ui.ctx(), canvas)
        })
        .drop_without_applying_deltas();
        assert!(ctx.memory(|memory| memory.area_rect(hud_id)).is_none());
        app.request_scene_selection(Some(board), false);
        let revision = app.editor.project().revision;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_design_hud(ui.ctx(), canvas)
        })
        .drop_without_applying_deltas();
        let pristine = ctx.memory(|memory| memory.area_rect(hud_id)).unwrap();
        assert!(pristine.width() <= 462.0, "strip expanded: {pristine:?}");
        assert!(
            pristine.height() <= 50.0,
            "HUD should be one row: {pristine:?}"
        );
        app.edit_drafts
            .board(&app.editor, board, Unit::Mm, Locale::En)
            .unwrap()
            .length
            .edit("bad");
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_design_hud(ui.ctx(), canvas)
        })
        .drop_without_applying_deltas();
        let rect = ctx.memory(|memory| memory.area_rect(hud_id)).unwrap();
        assert!(canvas.contains_rect(rect));
        assert_eq!(app.editor.project().revision, revision);
        assert_eq!(
            app.edit_drafts
                .existing_board(app.editor.project().id, board)
                .unwrap()
                .length
                .display(),
            "bad"
        );
        app.selection.choose(None, false);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_design_hud(ui.ctx(), canvas)
        });
        assert!(!output.shapes.iter().any(|shape| shape.clip_rect.intersects(rect) && matches!(&shape.shape, egui::Shape::Rect(r) if r.fill == theme_widgets::PANEL)));
        output.drop_without_applying_deltas();
    }

    #[test]
    fn hud_actions_have_separate_pointer_targets_and_hide_without_editing_project() {
        for (language, canvas) in [
            (
                Language::En,
                egui::Rect::from_min_max(egui::pos2(316.0, 46.0), egui::pos2(1148.0, 875.0)),
            ),
            (
                Language::PtBr,
                egui::Rect::from_min_max(egui::pos2(60.0, 46.0), egui::pos2(700.0, 625.0)),
            ),
        ] {
            let (mut app, board) = app_with_board();
            app.localizer.set_language(language);
            app.request_scene_selection(Some(board), false);
            let before = app.editor.project().clone();
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            let screen =
                egui::Rect::from_min_max(egui::Pos2::ZERO, canvas.max + egui::vec2(100.0, 25.0));
            let mut frame = |events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        events,
                        ..Default::default()
                    },
                    |ui| app.show_design_hud(ui.ctx(), canvas),
                )
                .drop_without_applying_deltas();
            };
            frame(vec![]);
            frame(vec![]);
            let strip = ctx
                .memory(|memory| memory.area_rect(egui::Id::new("design-board-hud")))
                .unwrap();
            let actions = [
                A::PlaceFace,
                A::DuplicateBoard,
                A::ToggleVisibility,
                A::DeleteObject,
            ];
            let rects: Vec<_> = actions
                .iter()
                .map(|action| {
                    ctx.data(|data| {
                        data.get_temp::<egui::Rect>(egui::Id::new(("hud-action", *action)))
                            .unwrap()
                    })
                })
                .collect();
            for (index, rect) in rects.iter().enumerate() {
                assert!(
                    strip.contains_rect(*rect),
                    "{language:?}: {rect:?} outside {strip:?}"
                );
                assert!(rect.width() >= 22.0 && rect.height() >= 24.0);
                if index > 0 {
                    assert!(rects[index - 1].right() < rect.left());
                }
            }
            let point = rects[2].center();
            for pressed in [true, false] {
                frame(vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            assert!(app.selection.hidden.contains(&board));
            assert_eq!(app.editor.project(), &before);
        }
    }

    #[test]
    fn hierarchy_selection_and_group_transform_reparent_ungroup_are_atomic() {
        let (mut app, board) = app_with_board();
        app.selection.choose(Some(board), false);
        let group = app
            .editor
            .group_objects(&[board], None, "Cabinet", [0.0; 3])
            .unwrap();
        let rows = hierarchy(app.editor.project());
        assert_eq!(
            rows.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(),
            vec![(group, 0), (board, 1)]
        );
        app.selection.choose(Some(group), false);
        app.selection.choose(Some(board), true);
        let before = world_pose(app.editor.project(), board).unwrap();
        app.editor
            .transform_selection(
                &app.selection.ids.iter().copied().collect::<Vec<_>>(),
                [100.0, 0.0, 0.0],
                Quaternion::IDENTITY,
                [0.0; 3],
            )
            .unwrap();
        assert!(
            (world_pose(app.editor.project(), board)
                .unwrap()
                .translation_mm[0]
                - before.translation_mm[0]
                - 100.0)
                .abs()
                < 1e-6
        );
        app.editor.undo().unwrap();
        assert_eq!(world_pose(app.editor.project(), board).unwrap(), before);
        app.editor.redo().unwrap();
        let unchanged = app.editor.project().clone();
        assert!(app.editor.reparent_objects(&[group], Some(group)).is_err());
        assert_eq!(app.editor.project(), &unchanged);
        app.editor.reparent_objects(&[board], None).unwrap();
        app.editor.reparent_objects(&[board], Some(group)).unwrap();
        app.editor.ungroup_assembly(group).unwrap();
        assert_eq!(app.editor.project().boards[0].parent_id, None);
        assert!(
            (world_pose(app.editor.project(), board)
                .unwrap()
                .translation_mm[0]
                - 100.0)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn modal_focus_invalid_and_escape_leave_project_untouched() {
        let (mut app, board) = app_with_board();
        app.selection.choose(Some(board), false);
        let initial = app.editor.project().clone();
        app.assembly_dialog = Some(AssemblyDialog::new(&app, Operation::Transform));
        let ctx = egui::Context::default();
        let draw = |app: &mut DesktopApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| app.show_assembly_dialog(ui.ctx()),
            )
            .drop_without_applying_deltas();
        };
        draw(&mut app, vec![]);
        assert!(ctx.memory(|m| m.focused()).is_some());
        app.assembly_dialog.as_mut().unwrap().translation[0].text = "invalid".into();
        draw(&mut app, vec![]);
        assert_eq!(app.editor.project(), &initial);
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
        assert!(app.assembly_dialog.is_none());
        assert_eq!(app.editor.project(), &initial);
    }

    fn dialog_frame(app: &mut DesktopApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                app.show_assembly_dialog(ui.ctx());
            },
        )
        .drop_without_applying_deltas();
    }

    fn dialog_key(key: egui::Key) -> Vec<egui::Event> {
        vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]
    }

    #[test]
    fn hierarchy_modal_keys_validate_cancel_and_commit_one_undo() {
        let (mut app, board) = app_with_board();
        app.selection.choose(Some(board), false);
        let initial = app.editor.project().clone();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        app.assembly_dialog = Some(AssemblyDialog::new(&app, Operation::Group));
        dialog_frame(&mut app, &ctx, vec![]);
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(egui::Id::new("assembly-hierarchy-name"))
        );
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
        assert!(app.assembly_dialog.is_some()); // empty name
        assert_eq!(app.editor.project(), &initial);
        app.assembly_dialog.as_mut().unwrap().name = "Cabinet".into();
        app.assembly_dialog.as_mut().unwrap().pivot[0].text = "invalid".into();
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
        assert_eq!(app.editor.project(), &initial);
        app.assembly_dialog.as_mut().unwrap().pivot[0].text = "0".into();
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
        assert!(app.assembly_dialog.is_none());
        assert_eq!(app.editor.project().assemblies.len(), 1);
        assert_eq!(
            app.editor.project().boards[0].parent_id,
            app.selection.active
        );
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().assemblies, initial.assemblies);
        assert_eq!(app.editor.project().boards, initial.boards);
    }

    #[test]
    fn hierarchy_modal_previews_nested_members_and_blocks_cycles() {
        let (mut app, board) = app_with_board();
        let root = app
            .editor
            .group_objects(&[board], None, "Cabinet", [0.0; 3])
            .unwrap();
        let child = app
            .editor
            .group_objects(&[board], Some(root), "Drawer", [0.0; 3])
            .unwrap();
        app.selection.choose(Some(root), false);
        let mut draft = AssemblyDialog::new(&app, Operation::Duplicate);
        assert_eq!(
            affected_objects(app.editor.project(), &draft),
            vec![(root, 0), (child, 1), (board, 2)]
        );
        assert_eq!(
            object_label(app.editor.project(), &app.localizer, board).unwrap(),
            format!("{} · Side", app.localizer.text("board-kind"))
        );
        draft.operation = Operation::Reparent;
        draft.target = Some(child);
        assert!(!hierarchy_target_valid(app.editor.project(), &draft));
        app.assembly_dialog = Some(draft);
        let initial = app.editor.project().clone();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        dialog_frame(&mut app, &ctx, vec![]);
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
        assert_eq!(app.editor.project(), &initial);
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Escape));
        if app.assembly_dialog.is_some() {
            // The focused parent chooser consumes the first Escape.
            dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Escape));
        }
        assert!(app.assembly_dialog.is_none());
        assert_eq!(app.editor.project(), &initial);
    }

    #[test]
    fn reparent_modal_popup_key_stays_in_draft_then_commits_with_undo() {
        let (mut app, board) = app_with_board();
        let group = app
            .editor
            .group_objects(&[board], None, "Cabinet", [0.0; 3])
            .unwrap();
        app.selection.choose(Some(board), false);
        app.assembly_dialog = Some(AssemblyDialog::new(&app, Operation::Reparent));
        let original = app.editor.project().clone();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        dialog_frame(&mut app, &ctx, vec![]);
        let popup = egui::Id::new("hierarchy-test-popup");
        egui::Popup::open_id(&ctx, popup);
        ctx.run_ui(
            egui::RawInput {
                events: dialog_key(egui::Key::Enter),
                ..Default::default()
            },
            |ui| {
                app.show_assembly_dialog(ui.ctx());
                egui::Popup::close_id(ui.ctx(), popup);
            },
        )
        .drop_without_applying_deltas();
        assert!(app.assembly_dialog.is_some());
        assert_eq!(app.editor.project(), &original);
        // Move through the chooser and secondary action to the named confirmation.
        for _ in 0..3 {
            dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Tab));
        }
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
        assert!(app.assembly_dialog.is_some()); // popup handles the first Enter
        assert_eq!(app.editor.project(), &original);
        app.assembly_dialog.as_mut().unwrap().target = None;
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
        assert!(app.assembly_dialog.is_none());
        assert_eq!(app.editor.project().boards[0].parent_id, None);
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().boards[0].parent_id, Some(group));
    }

    #[test]
    fn duplicate_and_ungroup_modals_cancel_or_accept_atomically() {
        let (mut app, board) = app_with_board();
        let group = app
            .editor
            .group_objects(&[board], None, "Cabinet", [0.0; 3])
            .unwrap();
        app.selection.choose(Some(group), false);
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let initial = app.editor.project().clone();
        app.assembly_dialog = Some(AssemblyDialog::new(&app, Operation::Duplicate));
        dialog_frame(&mut app, &ctx, vec![]);
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(egui::Id::new("assembly-duplicate-x"))
        );
        app.assembly_dialog.as_mut().unwrap().translation[0].text = "1/64 in".into();
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
        assert!(app.assembly_dialog.is_some()); // rounding needs explicit consent
        assert_eq!(app.editor.project(), &initial);
        app.assembly_dialog.as_mut().unwrap().translation[0].text = "bad".into();
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
        assert_eq!(app.editor.project(), &initial);
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Escape));
        assert_eq!(app.editor.project(), &initial);
        app.assembly_dialog = Some(AssemblyDialog::new(&app, Operation::Duplicate));
        dialog_frame(&mut app, &ctx, vec![]);
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
        assert_eq!(app.editor.project().assemblies.len(), 2);
        assert_eq!(app.editor.project().boards.len(), 2);
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().boards, initial.boards);
        app.editor.redo().unwrap();
        app.selection.choose(Some(group), false);
        app.assembly_dialog = Some(AssemblyDialog::new(&app, Operation::Ungroup));
        assert_eq!(app.assembly_dialog.as_ref().unwrap().active, Some(group));
        dialog_frame(&mut app, &ctx, vec![]);
        dialog_frame(&mut app, &ctx, vec![]);
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Tab));
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Enter));
        assert!(
            app.editor
                .project()
                .assemblies
                .iter()
                .all(|a| a.id != group)
        );
        assert_eq!(
            app.editor
                .project()
                .boards
                .iter()
                .find(|b| b.id == board)
                .unwrap()
                .parent_id,
            None
        );
        app.editor.undo().unwrap();
        assert_eq!(
            app.editor
                .project()
                .boards
                .iter()
                .find(|b| b.id == board)
                .unwrap()
                .parent_id,
            Some(group)
        );
    }

    #[test]
    fn hierarchy_modal_blocks_background_pointer_and_cancel_preserves_selection() {
        let (mut app, board) = app_with_board();
        app.selection.choose(Some(board), false);
        app.assembly_dialog = Some(AssemblyDialog::new(&app, Operation::Group));
        let initial = app.editor.project().clone();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let point = std::cell::Cell::new(egui::Pos2::ZERO);
        let primed = std::cell::Cell::new(false);
        let mut draw = |events: Vec<egui::Event>| {
            let mut clicked = false;
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let button = ui.button("background scene selection");
                    point.set(button.rect.center());
                    clicked = button.clicked();
                    let background = ui.layer_id();
                    app.show_assembly_dialog(ui.ctx());
                    if primed.get() {
                        assert!(!ctx.memory(|m| m.allows_interaction(background)));
                    }
                },
            )
            .drop_without_applying_deltas();
            clicked
        };
        assert!(!draw(vec![]));
        primed.set(true);
        assert!(!draw(vec![]));
        for pressed in [true, false] {
            assert!(!draw(vec![
                egui::Event::PointerMoved(point.get()),
                egui::Event::PointerButton {
                    pos: point.get(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }
            ]));
        }
        assert_eq!(app.selection.active, Some(board));
        assert_eq!(app.editor.project(), &initial);
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Escape));
        assert!(app.assembly_dialog.is_none());
        assert_eq!(app.editor.project(), &initial);
    }

    #[test]
    fn hidden_group_keeps_unallocated_board_demand_and_list_selection() {
        let (mut app, board) = app_with_board();
        let group = app
            .editor
            .group_objects(&[board], None, "Body", [0.0; 3])
            .unwrap();
        let revision = app.editor.project().revision;
        let snapshot = app.editor.project().clone();
        app.selection.hidden.insert(group);
        let rows = hierarchy(app.editor.project());
        assert!(rows.iter().any(|row| row.0 == board));
        assert!(!app.selection.visible(app.editor.project(), board));
        assert_eq!(app.editor.project().boards.len(), 1);
        assert!(app.editor.project().allocations.is_empty());
        app.selection.choose(Some(board), false); // Object-list row remains selectable.
        assert_eq!(app.selection.active, Some(board));
        app.selection.reveal(app.editor.project(), board);
        assert!(app.selection.visible(app.editor.project(), board));
        assert_eq!(app.editor.project().revision, revision);
        assert_eq!(app.editor.project(), &snapshot);
    }

    #[test]
    fn collapsed_hidden_issues_and_eye_are_independent_view_state() {
        let (mut app, board) = app_with_board();
        let assembly = app
            .editor
            .group_objects(&[board], None, "Body", [0.0; 3])
            .unwrap();
        app.session.design_expanded.insert(assembly);
        let revision = app.editor.project().revision;
        let original = app.editor.project().clone();
        app.selection.hidden.insert(assembly);
        app.session.design_expanded.remove(&assembly);
        let model = app.design_model().unwrap();
        let parent = model.outliner.iter().find(|r| r.id == assembly).unwrap();
        assert_eq!(parent.issue_count, 1);
        assert_eq!(parent.expanded, Some(false));
        assert!(!parent.visible);
        assert_eq!(visible_rows(&model.outliner).len(), 1);
        assert!(
            model
                .outliner
                .iter()
                .any(|r| r.id == board && !r.visible && r.issue_count == 1)
        );
        app.invoke(Request::with(A::ToggleVisibility, Target::Object(assembly)))
            .unwrap();
        let revealed = app.design_model().unwrap();
        assert!(
            revealed
                .outliner
                .iter()
                .find(|r| r.id == board)
                .unwrap()
                .visible
        );
        assert_eq!(revealed.outliner[0].expanded, Some(false));
        app.session.design_expanded.insert(assembly);
        assert_eq!(visible_rows(&app.design_model().unwrap().outliner).len(), 2);
        assert_eq!(app.editor.project().revision, revision);
        assert_eq!(app.editor.project(), &original);
    }

    #[test]
    fn equal_names_and_selection_cardinality_never_infer_a_single_board() {
        let (mut app, board) = app_with_board();
        let second = app
            .editor
            .duplicate_board(
                board,
                Pose::new([140.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
            )
            .unwrap();
        let group = app
            .editor
            .group_objects(&[board], None, "Body", [0.0; 3])
            .unwrap();
        assert!(matches!(
            app.design_model().unwrap().inspector,
            DesignInspector::None
        ));
        app.selection.choose(Some(group), false);
        assert!(matches!(
            app.design_model().unwrap().inspector,
            DesignInspector::Assembly(_)
        ));
        app.selection.choose(Some(board), false);
        assert!(
            matches!(app.design_model().unwrap().inspector, DesignInspector::Board(ref b) if b.id == board)
        );
        app.selection.choose(Some(second), true);
        let model = app.design_model().unwrap();
        assert!(
            matches!(model.inspector, DesignInspector::Multi(ref m) if m.board_count == 2 && m.active == Some(second))
        );
        assert_eq!(
            model.outliner.iter().filter(|r| r.name == "Side").count(),
            2
        );
        assert_ne!(
            model
                .outliner
                .iter()
                .find(|r| r.id == board)
                .unwrap()
                .active,
            model
                .outliner
                .iter()
                .find(|r| r.id == second)
                .unwrap()
                .active
        );
    }

    #[test]
    fn revision_invalidates_stock_snapshot_but_view_gestures_reuse_it() {
        let (mut app, board) = app_with_board();
        app.design_model().unwrap();
        let first = &app.design_stock_snapshot.as_ref().unwrap().1 as *const _;
        let revision = app.editor.project().revision;
        app.selection.choose(Some(board), false);
        app.selection.hidden.insert(board);
        app.design_model().unwrap();
        assert_eq!(
            first,
            &app.design_stock_snapshot.as_ref().unwrap().1 as *const _
        );
        assert_eq!(app.editor.project().revision, revision);
        app.editor
            .duplicate_board(
                board,
                Pose::new([200.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
            )
            .unwrap();
        app.design_model().unwrap();
        assert_eq!(
            app.design_stock_snapshot.as_ref().unwrap().0.1,
            app.editor.project().revision
        );
        assert_eq!(
            app.design_stock_snapshot.as_ref().unwrap().1.boards.len(),
            2
        );
    }

    #[test]
    fn inspector_routes_obey_selection_and_modal_guards() {
        let (mut app, board) = app_with_board();
        assert!(app.action_availability(Request::new(A::Transform)).is_err());
        assert!(
            app.action_availability(Request::new(A::DeleteObject))
                .is_err()
        );
        app.selection.choose(Some(board), false);
        assert!(app.action_availability(Request::new(A::Transform)).is_ok());
        assert!(
            app.action_availability(Request::new(A::DeleteObject))
                .is_ok()
        );
        let group = app
            .editor
            .group_objects(&[board], None, "Body", [0.0; 3])
            .unwrap();
        app.selection.choose(Some(group), false);
        assert!(app.action_availability(Request::new(A::Ungroup)).is_ok());
        assert!(
            app.action_availability(Request::new(A::DuplicateAssembly))
                .is_ok()
        );
        app.assembly_dialog = Some(AssemblyDialog::new(&app, Operation::Transform));
        assert!(
            app.action_availability(Request::new(A::DeleteObject))
                .is_err()
        );
        assert!(app.action_availability(Request::new(A::Ungroup)).is_err());
    }

    #[test]
    fn fixture_compact_sidebar_fits_stock_and_preserves_warning_selection_and_source() {
        use plan_my_cabinet::reference_fixture as fixture;
        let mut project = fixture::project();
        project.material_colors.insert(
            fixture::WHITE_ID,
            plan_my_cabinet::domain::SrgbColor([233, 231, 226]),
        );
        project.material_colors.insert(
            fixture::OAK_ID,
            plan_my_cabinet::domain::SrgbColor([185, 139, 94]),
        );
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..DesktopApp::default()
        };
        app.session = WorkspaceSession::new(app.editor.project());
        app.selection.choose(Some(fixture::SHELF_ID), false);
        let model = app.design_model().unwrap();
        let back = model
            .outliner
            .iter()
            .find(|r| r.id == fixture::BACK_ID)
            .unwrap();
        assert_eq!(back.issue_count, 1);
        assert!(!back.selected);
        assert!(back.visible);
        let shelf = model
            .outliner
            .iter()
            .find(|r| r.id == fixture::SHELF_ID)
            .unwrap();
        assert!(shelf.active);
        assert_eq!(
            model.materials.iter().map(|m| m.board_count).sum::<usize>(),
            9
        );
        assert_eq!(
            model
                .materials
                .iter()
                .map(|m| m.unallocated_board_count)
                .sum::<usize>(),
            1
        );
        assert_ne!(model.materials[0].color, model.materials[1].color);
        assert_eq!(model.stock.len(), 4);
        assert!(model.stock.iter().any(|s| s.alias == "O1" && s.source == plan_my_cabinet::domain::StockSource::Owned));
        assert!(
            model
                .stock
                .iter()
                .any(|s| s.alias == "S1" && s.part_count > 0)
        );

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::theme::install_fonts(&ctx);
        icons::install_loaders(&ctx);
        let mut height = 0.0;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_width(256.0);
            app.show_hierarchy(ui);
            height = ui.min_rect().height();
        });
        assert!(
            height < 800.0,
            "stock controls must fit at the reference height: {}",
            height
        );
        let update = output.platform_output.accesskit_update.as_ref().unwrap();
        for rank in 1..=4 {
            let expected = format!("#{rank}");
            assert!(
                update
                    .nodes
                    .iter()
                    .any(|(_, node)| node.value() == Some(expected.as_str()))
            );
        }
        assert!(
            !update
                .nodes
                .iter()
                .any(|(_, node)| node.value() == Some("#5"))
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn hardware_placeholder_actions_require_matching_target_and_respect_modal_guard() {
        let (mut app, _) = app_with_board();
        let hardware = app
            .editor
            .create_placeholder(
                "Foot".into(),
                [Length::from_micrometres(20_000); 3],
                None,
                Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            )
            .unwrap();
        app.selection.choose(Some(hardware), false);
        for action in [A::EditHardware, A::DuplicateHardware] {
            assert!(
                app.action_availability(Request::with(action, Target::Object(hardware)))
                    .is_ok()
            );
            assert!(
                app.action_availability(Request::with(
                    action,
                    Target::Object(app.editor.project().boards[0].id)
                ))
                .is_err()
            );
        }
        app.assembly_dialog = Some(AssemblyDialog::new(&app, Operation::Transform));
        assert!(
            app.action_availability(Request::with(A::EditHardware, Target::Object(hardware)))
                .is_err()
        );
    }
}
