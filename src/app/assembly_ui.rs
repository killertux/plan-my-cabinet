use crate::*;
use crate::actions::{ActionId as A, Argument, Request, Target};
use crate::modal_chrome::{ModalAction, ModalActions, ModalChrome};
use plan_my_cabinet::assembly_edit::{AssemblyEditError, world_pose};
use plan_my_cabinet::design_read_models::{
    DesignInspector, DesignReadModel, ObjectKind, OutlinerRow, ThicknessProvenance,
};
use plan_my_cabinet::edit_drafts::DraftError;
use plan_my_cabinet::stock_read_models::StockPieceReadModel;
#[cfg(test)]
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub(crate) enum Operation {
    Group,
    Reparent,
    Ungroup,
    Transform,
    Duplicate,
}

pub(crate) struct AssemblyDialog {
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

pub(crate) fn coordinate(field: &DimensionDraft) -> Option<f64> {
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
    let used: f64 = piece
        .parts
        .iter()
        .map(|p| p.length.micrometres() as f64 * p.width.micrometres() as f64)
        .sum();
    let utilization = if piece.measured_area() > 0 {
        (used / piece.measured_area() as f64 * 100.0).round()
    } else {
        0.0
    };
    let frame = egui::Frame::new()
        .fill(theme_widgets::CARD)
        .stroke(egui::Stroke::new(1.0, theme_widgets::BORDER_SOFT))
        .corner_radius(9)
        .inner_margin(10);
    let inner = frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                theme_widgets::medium(ui, format!("{} · {}", piece.alias, piece.material_name), 12.5)
                    .color(theme_widgets::TEXT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    theme_widgets::mono(format!("{utilization:.0}%"), 11.5)
                        .color(theme_widgets::MUTED),
                );
            });
        });
        ui.add_space(4.0);
        let width = ui.available_width();
        let height = (width * wide / long).clamp(48.0, 170.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        let painter = ui.painter().with_clip_rect(rect.expand(1.0));
        painter.rect(
            rect,
            2.0,
            egui::Color32::from_rgb(247, 245, 240),
            egui::Stroke::new(1.0, egui::Color32::from_rgb(191, 181, 165)),
            egui::StrokeKind::Inside,
        );
        let scale_x = rect.width() / long;
        let scale_y = rect.height() / wide;
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
                let selected = part.board_id == board;
                painter.rect(
                    footprint.shrink(0.5),
                    1.0,
                    if selected {
                        egui::Color32::from_rgb(244, 194, 122)
                    } else {
                        egui::Color32::from_rgb(226, 219, 207)
                    },
                    egui::Stroke::new(
                        if selected { 1.5 } else { 1.0 },
                        if selected {
                            egui::Color32::from_rgb(154, 91, 18)
                        } else {
                            egui::Color32::from_rgb(191, 181, 165)
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
            }
        }
    });
    let response = ui.interact(
        inner.response.rect,
        ui.id().with(("sheet-miniature", piece.id)),
        egui::Sense::click(),
    );
    let label = format!(
        "{} · {} · {}",
        localizer.text("sheet-heading"),
        piece.alias,
        piece.name
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &label)
    });
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("{label} · {}", localizer.text("design-open-cut-plan")))
}

fn hud_rect(canvas: egui::Rect) -> Option<egui::Rect> {
    if canvas.width() < 260.0 || canvas.height() < 150.0 {
        return None;
    }
    let width = (canvas.width() - 24.0).min(560.0);
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
    pub(crate) fn new(app: &DesktopApp, operation: Operation) -> Self {
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

pub(crate) fn short_length(length: Length, locale: Locale) -> String {
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

pub(crate) fn unit_suffix(unit: Unit) -> &'static str {
    match unit {
        Unit::Mm => "mm",
        Unit::Cm => "cm",
        Unit::M => "m",
        Unit::Inch => "in",
        Unit::Foot => "ft",
    }
}

/// Field text without a unit suffix or needless trailing zeros.
fn field_text(field: &plan_my_cabinet::edit_drafts::LengthField) -> String {
    field.text.clone().unwrap_or_else(|| {
        let formatted = format_length(field.committed, field.unit, field.locale, 3);
        let number = formatted.split_whitespace().next().unwrap_or("").to_owned();
        if number.contains(['.', ',']) {
            number
                .trim_end_matches('0')
                .trim_end_matches(['.', ','])
                .to_owned()
        } else {
            number
        }
    })
}

/// A short, human-scale identifier for headers ("b-7f3a").
pub(crate) fn short_id(prefix: char, id: Uuid) -> String {
    let hex = id.simple().to_string();
    format!("{prefix}-{}", &hex[hex.len() - 4..])
}

const AXIS_COLORS: [egui::Color32; 3] = [
    egui::Color32::from_rgb(196, 69, 58),
    egui::Color32::from_rgb(78, 154, 87),
    egui::Color32::from_rgb(62, 111, 196),
];

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
        let thickness = self
            .editor
            .project()
            .boards
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.thickness);
        let mut apply = false;
        let mut discard = false;
        let mut error = None;
        ui.push_id((surface, id), |ui| {
            match self.edit_drafts.board(&self.editor, id, unit, locale) {
                Ok(draft) => {
                    let invalid = |axis: usize, error: &Option<DraftError>| {
                        matches!(error, Some(DraftError::InvalidField { axis: a, .. }) if *a == axis)
                    };
                    let pending_error = if draft.dirty() {
                        draft.preview(&self.editor).err()
                    } else {
                        None
                    };
                    let mut field_ui = |ui: &mut egui::Ui, axis: usize, width: f32| {
                        let field = if axis == 0 {
                            &mut draft.length
                        } else {
                            &mut draft.width
                        };
                        let mut text = field_text(field);
                        let label = self.localizer.text(if axis == 0 {
                            "board-length"
                        } else {
                            "board-width"
                        });
                        let response = theme_widgets::value_field(
                            ui,
                            ui.id().with(("dimension", axis)),
                            &label,
                            &mut text,
                            width,
                            if compact { None } else { Some(unit_suffix(unit)) },
                            None,
                            editable,
                            invalid(axis, &pending_error),
                        )
                        .on_hover_text(format!("{label}: {}", field.display()));
                        if response.changed() {
                            field.edit(text);
                        }
                        if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            apply = true;
                        }
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            apply = true;
                        }
                        if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            discard = true;
                        }
                    };
                    let thickness_text = thickness
                        .map(|t| short_length(t, locale))
                        .unwrap_or_default();
                    if compact {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        field_ui(ui, 0, 58.0);
                        ui.label(egui::RichText::new("×").color(theme_widgets::FAINT));
                        field_ui(ui, 1, 58.0);
                        ui.label(egui::RichText::new("×").color(theme_widgets::FAINT));
                        theme_widgets::derived_field(ui, &thickness_text, "", 40.0)
                            .on_hover_text(self.localizer.text("board-thickness"));
                        ui.label(
                            egui::RichText::new(unit_suffix(unit))
                                .size(12.0)
                                .color(theme_widgets::FAINT),
                        );
                    } else {
                        let width = ui.available_width() - 96.0;
                        theme_widgets::prop_row(ui, &self.localizer.text("board-length"), 88.0, |ui| {
                            field_ui(ui, 0, width)
                        });
                        theme_widgets::prop_row(ui, &self.localizer.text("board-width"), 88.0, |ui| {
                            field_ui(ui, 1, width)
                        });
                        theme_widgets::prop_row(
                            ui,
                            &self.localizer.text("board-thickness"),
                            88.0,
                            |ui| {
                                theme_widgets::derived_field(
                                    ui,
                                    &thickness_text,
                                    &self.localizer.text("design-from-material"),
                                    width,
                                )
                            },
                        );
                        theme_widgets::prop_row(ui, &self.localizer.text("design-anchor"), 88.0, |ui| {
                            ui.add_enabled_ui(editable, |ui| {
                                theme_widgets::segmented(
                                    ui,
                                    &mut draft.anchor,
                                    &[
                                        (Anchor::Start, &self.localizer.text("anchor-start-short")),
                                        (Anchor::Centre, &self.localizer.text("anchor-centre-short")),
                                        (Anchor::End, &self.localizer.text("anchor-end-short")),
                                    ],
                                )
                            })
                            .inner
                            .on_hover_text(self.localizer.text("design-anchor-hint"));
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
                            theme_widgets::warn_callout().show(ui, |ui| {
                                ui.add_enabled(
                                    editable,
                                    egui::Checkbox::new(&mut field.consent, label),
                                );
                            });
                            error = draft.preview(&self.editor).err();
                        }
                    }
                    if !compact && let Some(ref problem) = error {
                        let key = match problem {
                            DraftError::InvalidField { error, .. } => error_key(*error),
                            _ => "error-board-dimension",
                        };
                        if !matches!(problem, DraftError::RoundingConsent { .. }) {
                            ui.label(
                                egui::RichText::new(self.localizer.text(key))
                                    .size(11.5)
                                    .color(theme_widgets::DANGER),
                            );
                        }
                    }
                    if !compact && draft.dirty() {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.add_space(88.0);
                            if theme_widgets::primary_button(
                                ui,
                                &self.localizer.text("navigation-apply"),
                                editable && error.is_none(),
                            )
                            .clicked()
                            {
                                apply = true;
                            }
                            if theme_widgets::secondary_button_enabled(
                                ui,
                                &self.localizer.text("navigation-discard"),
                                editable,
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
            && draft.dirty()
        {
            if draft.accept(&mut self.editor).is_err() {
                ui.colored_label(
                    theme_widgets::WARN_INK,
                    self.localizer.text("error-board-dimension"),
                );
            } else {
                self.edit_drafts.cancel_board(self.editor.project().id, id);
                ui.ctx().request_repaint();
            }
        }
    }

    pub(crate) fn show_design_hud(&mut self, ctx: &egui::Context, canvas: egui::Rect) {
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
        egui::Area::new(egui::Id::new("design-board-hud"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(strip.center_bottom())
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme_widgets::PANEL)
                    .stroke(egui::Stroke::new(1.0, theme_widgets::BORDER_STRONG))
                    .inner_margin(egui::Margin {
                        left: 14,
                        right: 6,
                        top: 5,
                        bottom: 5,
                    })
                    .corner_radius(11)
                    .shadow(egui::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: egui::Color32::from_rgba_unmultiplied(60, 45, 25, 40),
                    })
                    .show(ui, |ui| {
                        ui.set_max_width(strip.width());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.add(
                                egui::Label::new(
                                    theme_widgets::semibold(ui, &name, 13.0)
                                        .color(theme_widgets::ACCENT_INK),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&name);
                            ui.add_space(4.0);
                            self.show_shared_board_dimensions(ui, id, "hud");
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(9.0, 24.0), egui::Sense::hover());
                            ui.painter().vline(
                                rect.center().x,
                                rect.y_range(),
                                egui::Stroke::new(1.0, theme_widgets::BORDER_SOFT),
                            );
                            for (action, target, icon, color) in [
                                (A::PlaceFace, Target::Board(id), icons::Icon::Place, theme_widgets::SECONDARY),
                                (A::DuplicateBoard, Target::Board(id), icons::Icon::Duplicate, theme_widgets::SECONDARY),
                                (A::ToggleVisibility, Target::Object(id), icons::Icon::EyeOff, theme_widgets::SECONDARY),
                                (A::DeleteObject, Target::None, icons::Icon::Trash, theme_widgets::DANGER),
                            ] {
                                let request = Request::with(action, target);
                                let label = action.label(&self.localizer);
                                let response = theme_widgets::ghost_icon_sized(
                                    ui,
                                    icon,
                                    &label,
                                    color,
                                    16.0,
                                    30.0,
                                    self.action_availability(request).is_ok(),
                                    false,
                                );
                                #[cfg(test)]
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::new(("hud-action", action)),
                                        response.rect,
                                    )
                                });
                                if response.clicked() {
                                    self.invoke_or_report(request);
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
        let mut accept = false;
        let mut cancel = false;
        let mut error = None;
        ui.push_id(("inspector-pose", id), |ui| {
            match self
                .edit_drafts
                .pose(&self.editor, id, self.design.pose_frame, unit, locale)
            {
                Ok(draft) => {
                    let invalid_axis = if draft.dirty() {
                        match draft.preview(&self.editor).err() {
                            Some(DraftError::InvalidField { axis, .. }) => Some(axis),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    let width = ((ui.available_width() - 96.0 - 8.0) / 3.0).max(40.0);
                    theme_widgets::prop_row(
                        ui,
                        &self.localizer.text("placement-position"),
                        88.0,
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for (axis, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                                let field = &mut draft.position[axis];
                                let mut text = field_text(field);
                                let response = theme_widgets::value_field(
                                    ui,
                                    ui.id().with(("position", axis)),
                                    label,
                                    &mut text,
                                    width,
                                    None,
                                    Some(AXIS_COLORS[axis]),
                                    editable,
                                    invalid_axis == Some(axis),
                                )
                                .on_hover_text(format!("{label} · {}", field.display()));
                                if response.changed() {
                                    field.edit(text);
                                }
                                if (response.has_focus() || response.lost_focus())
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    accept = true;
                                }
                                if response.has_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Escape))
                                {
                                    cancel = true;
                                }
                            }
                        },
                    );
                    theme_widgets::prop_row(
                        ui,
                        &self.localizer.text("design-rotation"),
                        88.0,
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for (axis, label) in ["X°", "Y°", "Z°"].into_iter().enumerate() {
                                let mut text = draft.rotation[axis].clone().unwrap_or_else(|| {
                                    let value = draft.rotation_degrees[axis];
                                    let value = if value.abs() < 0.005 { 0.0 } else { value };
                                    let text = format!("{value:.2}");
                                    text.trim_end_matches('0').trim_end_matches('.').to_owned()
                                });
                                let response = theme_widgets::value_field(
                                    ui,
                                    ui.id().with(("rotation", axis)),
                                    label,
                                    &mut text,
                                    width,
                                    Some("°"),
                                    None,
                                    editable,
                                    false,
                                );
                                if response.changed() {
                                    draft.rotation[axis] = Some(text);
                                }
                                if (response.has_focus() || response.lost_focus())
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    accept = true;
                                }
                                if response.has_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Escape))
                                {
                                    cancel = true;
                                }
                            }
                        },
                    );
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
                            theme_widgets::warn_callout().show(ui, |ui| {
                                ui.add_enabled(
                                    editable,
                                    egui::Checkbox::new(&mut draft.position[*axis].consent, label),
                                );
                            });
                            error = draft.preview(&self.editor).err();
                        }
                    }
                    if let Some(ref problem) = error {
                        let key = match problem {
                            DraftError::InvalidField { error, .. } => error_key(*error),
                            DraftError::InvalidRotation(_) => "error-invalid-rotation",
                            DraftError::Stale => "navigation-stale",
                            _ => "placement-invalid",
                        };
                        if !matches!(problem, DraftError::RoundingConsent { .. }) {
                            ui.label(
                                egui::RichText::new(self.localizer.text(key))
                                    .size(11.5)
                                    .color(theme_widgets::DANGER),
                            );
                        }
                    }
                    if draft.dirty() {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.add_space(88.0);
                            if theme_widgets::primary_button(
                                ui,
                                &self.localizer.text("navigation-accept"),
                                editable && error.is_none(),
                            )
                            .clicked()
                            {
                                accept = true;
                            }
                            if theme_widgets::secondary_button_enabled(
                                ui,
                                &self.localizer.text("navigation-cancel"),
                                editable,
                            )
                            .clicked()
                            {
                                cancel = true;
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
            && draft.dirty()
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

    /// Actions that operate on the current outliner selection, offered from the
    /// outliner's `+` menu and each row's context menu.
    fn hierarchy_menu(&mut self, ui: &mut egui::Ui, include_create: bool) {
        let active_assembly = self
            .selection
            .active
            .is_some_and(|id| self.editor.project().assemblies.iter().any(|a| a.id == id));
        let has_selection = !self.selection.ids.is_empty();
        let modal = self.modal_open();
        let mut run = None;
        if include_create {
            for action in [A::NewBoard, A::NewMaterial] {
                let request = Request::new(action);
                if ui
                    .add_enabled(
                        self.action_availability(request).is_ok(),
                        egui::Button::new(action.label(&self.localizer)),
                    )
                    .clicked()
                {
                    run = Some(request);
                }
            }
            ui.separator();
        }
        for (action, key, enabled) in [
            (A::Group, "assembly-group", has_selection),
            (A::Reparent, "assembly-reparent", has_selection),
            (A::Ungroup, "assembly-ungroup", active_assembly),
            (A::DuplicateAssembly, "assembly-duplicate", active_assembly),
            (A::Transform, "assembly-transform", has_selection),
            (A::BatchDimensions, "", self.selection.ids.len() > 1),
        ] {
            let label = if key.is_empty() {
                action.label(&self.localizer)
            } else {
                self.localizer.text(key)
            };
            if ui
                .add_enabled(!modal && enabled, egui::Button::new(label))
                .clicked()
            {
                run = Some(Request::new(action));
            }
        }
        if has_selection {
            ui.separator();
            let delete = Request::new(A::DeleteObject);
            if ui
                .add_enabled(
                    self.action_availability(delete).is_ok(),
                    egui::Button::new(
                        egui::RichText::new(A::DeleteObject.label(&self.localizer))
                            .color(theme_widgets::DANGER),
                    ),
                )
                .clicked()
            {
                run = Some(delete);
            }
        }
        if let Some(request) = run {
            ui.close();
            self.invoke_or_report(request);
        }
    }

    pub(crate) fn show_hierarchy(&mut self, ui: &mut egui::Ui) {
        let Some(model) = self.design_model() else {
            ui.label(self.localizer.text("measurement-invalid"));
            return;
        };
        let modal = self.modal_open();
        let pad = egui::Margin {
            left: 14,
            right: 10,
            top: 0,
            bottom: 8,
        };
        egui::Frame::new().inner_margin(pad).show(ui, |ui| {
            theme_widgets::section_bar(ui, &self.localizer.text("design-outliner-title"), |ui| {
                let plus = theme_widgets::ghost_icon_sized(
                    ui,
                    icons::Icon::Plus,
                    &self.localizer.text("design-add"),
                    theme_widgets::MUTED,
                    15.0,
                    24.0,
                    !modal,
                    false,
                );
                egui::Popup::menu(&plus)
                    .align(egui::RectAlign::BOTTOM_END)
                    .show(|ui| {
                        ui.set_min_width(200.0);
                        self.hierarchy_menu(ui, true);
                    });
                if theme_widgets::ghost_icon_sized(
                    ui,
                    icons::Icon::Search,
                    &self.localizer.text("palette-title"),
                    theme_widgets::MUTED,
                    14.0,
                    24.0,
                    !modal,
                    false,
                )
                .clicked()
                {
                    self.palette.open(ui.ctx());
                }
            });
            if model.outliner.is_empty() {
                ui.label(
                    egui::RichText::new(self.localizer.text("design-empty"))
                        .size(12.0)
                        .color(theme_widgets::FAINT),
                );
                ui.add_space(6.0);
                if theme_widgets::icon_text_button(
                    ui,
                    icons::Icon::Plus,
                    &A::NewBoard.label(&self.localizer),
                    true,
                    self.action_availability(Request::new(A::NewBoard)).is_ok(),
                )
                .clicked()
                {
                    self.invoke_or_report(Request::new(A::NewBoard));
                }
            }
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in visible_rows(&model.outliner) {
                let state = if row.active {
                    theme_widgets::RowState::Active
                } else if row.selected {
                    theme_widgets::RowState::Selected
                } else {
                    theme_widgets::RowState::Normal
                };
                let kind = match row.kind {
                    ObjectKind::Assembly => "assembly-kind",
                    ObjectKind::Board => "board-kind",
                    ObjectKind::Hardware => "hardware-kind",
                };
                let mut toggle_expand = None;
                let mut toggle_visibility = false;
                let (response, ()) = theme_widgets::list_row(
                    ui,
                    egui::Id::new(("outliner-row", row.id)),
                    26.0,
                    state,
                    !modal,
                    &row.name,
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.add_space(row.depth as f32 * 16.0);
                        if let Some(expanded) = row.expanded {
                            let label = self.localizer.text(if expanded {
                                "design-collapse"
                            } else {
                                "design-expand"
                            });
                            if theme_widgets::ghost_icon_sized(
                                ui,
                                if expanded {
                                    icons::Icon::ChevDown
                                } else {
                                    icons::Icon::ChevRight
                                },
                                &label,
                                theme_widgets::FAINT,
                                11.0,
                                16.0,
                                !modal,
                                false,
                            )
                            .clicked()
                            {
                                toggle_expand = Some(expanded);
                            }
                        } else {
                            ui.add_space(16.0);
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
                            if row.kind == ObjectKind::Assembly { 14.0 } else { 13.0 },
                        ));
                        let ink = if row.active {
                            theme_widgets::ACCENT_INK
                        } else if !row.visible {
                            theme_widgets::FAINT
                        } else {
                            theme_widgets::TEXT
                        };
                        let text = if row.active {
                            theme_widgets::medium(ui, &row.name, 13.0).color(ink)
                        } else {
                            egui::RichText::new(&row.name).size(13.0).color(ink)
                        };
                        let name_width = (ui.available_width() - 30.0).max(20.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(name_width, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(egui::Label::new(text).truncate().selectable(false));
                            },
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if row.issue_count > 0 && row.visible {
                                ui.add(icons::icon(icons::Icon::Warning, theme_widgets::WARN, 13.0))
                                    .on_hover_text(format!(
                                        "{}: {}",
                                        self.localizer.text("design-issue-count"),
                                        row.issue_count
                                    ));
                            } else {
                                let eye_label =
                                    self.localizer.text(if row.hidden_directly || !row.visible {
                                        "assembly-reveal"
                                    } else {
                                        "assembly-hide"
                                    });
                                toggle_visibility = theme_widgets::ghost_icon_sized(
                                    ui,
                                    if row.visible {
                                        icons::Icon::Eye
                                    } else {
                                        icons::Icon::EyeOff
                                    },
                                    &eye_label,
                                    if row.visible {
                                        egui::Color32::from_rgb(201, 193, 180)
                                    } else {
                                        theme_widgets::MUTED
                                    },
                                    13.0,
                                    22.0,
                                    !modal,
                                    false,
                                )
                                .clicked();
                            }
                        });
                    },
                );
                if response.hovered() {
                    viewport::set_hover(ui.ctx(), Some(row.id));
                }
                let response =
                    response.on_hover_text(format!("{} · {}", self.localizer.text(kind), row.name));
                if let Some(expanded) = toggle_expand {
                    if expanded {
                        self.session.design_expanded.remove(&row.id);
                    } else {
                        self.session.design_expanded.insert(row.id);
                    }
                } else if toggle_visibility {
                    self.invoke_or_report(Request::with(A::ToggleVisibility, Target::Object(row.id)));
                } else if response.clicked() {
                    let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                    let request = if row.kind == ObjectKind::Board {
                        Request::with(A::SelectBoard, Target::Board(row.id))
                    } else {
                        Request::with(A::SelectObject, Target::Object(row.id))
                    };
                    self.invoke_or_report(request.argument(Argument::Additive(additive)));
                } else if response.double_clicked() {
                    self.invoke_or_report(Request::new(A::ViewFrame));
                }
                if response.secondary_clicked() && !self.selection.ids.contains(&row.id) {
                    self.invoke_or_report(
                        Request::with(A::SelectObject, Target::Object(row.id))
                            .argument(Argument::Additive(false)),
                    );
                }
                response.context_menu(|ui| {
                    ui.set_min_width(200.0);
                    if row.kind == ObjectKind::Board {
                        for action in [
                            A::EditDimensions,
                            A::PositionBoard,
                            A::PlaceFace,
                            A::DuplicateBoard,
                            A::AssignMaterial,
                        ] {
                            let request = Request::with(action, Target::Board(row.id));
                            if ui
                                .add_enabled(
                                    self.action_availability(request).is_ok(),
                                    egui::Button::new(action.label(&self.localizer)),
                                )
                                .clicked()
                            {
                                ui.close();
                                self.invoke_or_report(request);
                            }
                        }
                        ui.separator();
                    }
                    self.hierarchy_menu(ui, false);
                });
            }
        });
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        theme_widgets::divider(ui);
        egui::Frame::new().inner_margin(pad).show(ui, |ui| {
            theme_widgets::section_bar(ui, &self.localizer.text("design-materials-title"), |ui| {
                if theme_widgets::ghost_icon_sized(
                    ui,
                    icons::Icon::Plus,
                    &A::NewMaterial.label(&self.localizer),
                    theme_widgets::MUTED,
                    15.0,
                    24.0,
                    self.action_availability(Request::new(A::NewMaterial)).is_ok(),
                    false,
                )
                .clicked()
                {
                    self.invoke_or_report(Request::new(A::NewMaterial));
                }
            });
            ui.spacing_mut().item_spacing.y = 0.0;
            for material in &model.materials {
                let warning = material.unallocated_board_count;
                let (response, ()) = theme_widgets::list_row(
                    ui,
                    egui::Id::new(("design-material-row", material.id)),
                    28.0,
                    theme_widgets::RowState::Normal,
                    !modal,
                    &material.name,
                    |ui| {
                        material_swatch(ui, material.color);
                        let name_width = (ui.available_width() - 64.0).max(20.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(name_width, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&material.name).color(theme_widgets::TEXT),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                            },
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                theme_widgets::mono(
                                    format!(
                                        "{} · {}",
                                        short_length(material.default_thickness, locale),
                                        material.board_count
                                    ),
                                    11.5,
                                )
                                .color(theme_widgets::FAINT),
                            );
                            if warning > 0 {
                                ui.add(icons::icon(icons::Icon::Warning, theme_widgets::WARN, 13.0));
                            }
                        });
                    },
                );
                let response = response.on_hover_text(format!(
                    "{} {} mm · {} {} · {} {}{}",
                    material.name,
                    short_length(material.default_thickness, locale),
                    material.board_count,
                    self.localizer.text("shell-parts"),
                    material.stock_piece_count,
                    self.localizer.text("shell-stock-pieces"),
                    if warning > 0 {
                        format!(" · {warning} {}", self.localizer.text("shell-unallocated"))
                    } else {
                        String::new()
                    }
                ));
                if response.clicked() {
                    self.invoke_or_report(Request::with(
                        A::EditMaterial,
                        Target::Material(material.id),
                    ));
                }
            }
        });
        theme_widgets::divider(ui);
        let holding: std::collections::HashSet<Uuid> = self
            .editor
            .project()
            .allocations
            .iter()
            .filter(|a| self.selection.ids.contains(&a.board_id))
            .map(|a| a.stock_id)
            .collect();
        egui::Frame::new().inner_margin(pad).show(ui, |ui| {
            theme_widgets::section_bar(ui, &self.localizer.text("design-stock-title"), |ui| {
                if theme_widgets::ghost_icon_sized(
                    ui,
                    icons::Icon::Plus,
                    &self.localizer.text("stock-new"),
                    theme_widgets::MUTED,
                    15.0,
                    24.0,
                    !modal,
                    false,
                )
                .clicked()
                {
                    self.invoke_or_report(Request::new(A::NewStock));
                }
            });
            ui.spacing_mut().item_spacing.y = 0.0;
            if model.stock.is_empty() {
                ui.label(
                    egui::RichText::new(self.localizer.text("design-no-stock"))
                        .size(12.0)
                        .color(theme_widgets::FAINT),
                );
            }
            for stock in &model.stock {
                let owned = stock.source == plan_my_cabinet::domain::StockSource::Owned;
                let source = self.localizer.text(if owned {
                    "design-stock-owned"
                } else {
                    "design-stock-to-buy"
                });
                let size = format!(
                    "{}×{}",
                    short_length(stock.size[0], locale),
                    short_length(stock.size[1], locale),
                );
                let (response, ()) = theme_widgets::list_row(
                    ui,
                    egui::Id::new(("design-stock-row", stock.id)),
                    28.0,
                    theme_widgets::RowState::Normal,
                    !modal,
                    &format!("{} {}", stock.alias, stock.name),
                    |ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(26.0, 24.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(theme_widgets::mono(&stock.alias, 11.5).color(
                                    if holding.contains(&stock.id) {
                                        theme_widgets::ACCENT
                                    } else {
                                        theme_widgets::FAINT
                                    },
                                ));
                            },
                        );
                        ui.label(theme_widgets::mono(size, 12.0).color(theme_widgets::TEXT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(source)
                                    .size(11.5)
                                    .color(theme_widgets::FAINT),
                            );
                        });
                    },
                );
                let response = response.on_hover_text(format!(
                    "{} · {} · {}×{}×{} mm · {} {}",
                    stock.alias,
                    stock.name,
                    short_length(stock.size[0], locale),
                    short_length(stock.size[1], locale),
                    short_length(stock.size[2], locale),
                    stock.part_count,
                    self.localizer.text("shell-parts"),
                ));
                if response.clicked() && !modal {
                    self.navigate_session(Destination::Sheet(stock.id));
                }
            }
        });
    }

    fn inspector_header(
        &self,
        ui: &mut egui::Ui,
        icon: icons::Icon,
        title: &str,
        id_text: &str,
        subline: &str,
    ) {
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 14,
                bottom: 12,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    ui.add(icons::icon(icon, theme_widgets::ACCENT, 15.0));
                    let title_width = (ui.available_width() - 60.0).max(40.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(title_width, 22.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add(
                                egui::Label::new(
                                    theme_widgets::semibold(ui, title, 15.0).color(theme_widgets::TEXT),
                                )
                                .truncate(),
                            );
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(theme_widgets::mono(id_text, 11.0).color(theme_widgets::FAINT));
                    });
                });
                if !subline.is_empty() {
                    ui.label(
                        egui::RichText::new(subline)
                            .size(12.0)
                            .color(theme_widgets::MUTED),
                    );
                }
            });
        theme_widgets::divider(ui);
    }

    fn inspector_actions(&mut self, ui: &mut egui::Ui, actions: &[(A, Target)]) {
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for &(action, target) in actions {
                let request = Request::with(action, target);
                if theme_widgets::secondary_button_enabled(
                    ui,
                    &action.label(&self.localizer),
                    self.action_availability(request).is_ok(),
                )
                .clicked()
                {
                    self.invoke_or_report(request);
                }
            }
        });
    }

    pub(crate) fn show_design_inspector(&mut self, ui: &mut egui::Ui, model: &DesignReadModel) {
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let dims = |values: [f64; 3]| {
            values
                .map(|v| short_length(Length::from_micrometres((v * 1000.0).round() as i64), locale))
                .join(" × ")
        };
        let body = egui::Frame::new().inner_margin(egui::Margin {
            left: 14,
            right: 14,
            top: 2,
            bottom: 16,
        });
        match &model.inspector {
            DesignInspector::None => {
                body.show(ui, |ui| {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.add(icons::icon(icons::Icon::Cube, theme_widgets::DISABLED, 28.0));
                        ui.add_space(8.0);
                        ui.label(
                            theme_widgets::medium(ui, self.localizer.text("design-no-selection"), 13.0)
                                .color(theme_widgets::MUTED),
                        );
                        ui.label(
                            egui::RichText::new(self.localizer.text("design-no-selection-hint"))
                                .size(12.0)
                                .color(theme_widgets::FAINT),
                        );
                        ui.add_space(14.0);
                        if theme_widgets::icon_text_button(
                            ui,
                            icons::Icon::Plus,
                            &A::NewBoard.label(&self.localizer),
                            false,
                            self.action_availability(Request::new(A::NewBoard)).is_ok(),
                        )
                        .clicked()
                        {
                            self.invoke_or_report(Request::new(A::NewBoard));
                        }
                    });
                });
            }
            DesignInspector::Board(board) => {
                let parent = board
                    .parent_name
                    .clone()
                    .unwrap_or_else(|| self.localizer.text("assembly-root"));
                let allocation = match board.allocation.status {
                    AllocationStatus::AllocatedValid => board
                        .stock
                        .first()
                        .and_then(|a| a.stock_alias.clone())
                        .map(|alias| format!("{} {alias}", self.localizer.text("design-allocated-on")))
                        .unwrap_or_else(|| self.localizer.text("board-allocated")),
                    AllocationStatus::Unallocated => self.localizer.text("board-unallocated"),
                    AllocationStatus::Conflicted => self.localizer.text("global-conflicted"),
                    AllocationStatus::UnknownSearchBudget => {
                        self.localizer.text("sheet-feasibility-unknown")
                    }
                };
                let mut args = FluentArgs::new();
                args.set("parent", parent.as_str());
                let subline = format!(
                    "{} · {allocation}",
                    self.localizer.format("design-board-in", Some(&args))
                );
                self.inspector_header(
                    ui,
                    icons::Icon::Board,
                    &board.name,
                    &short_id('b', board.id),
                    &subline,
                );
                let source = self
                    .editor
                    .project()
                    .boards
                    .iter()
                    .find(|b| b.id == board.id)
                    .expect("inspected board")
                    .clone();
                body.show(ui, |ui| {
                    theme_widgets::inspector_heading(ui, &self.localizer.text("board-kind"), |_| {});
                    let width = ui.available_width() - 96.0;
                    theme_widgets::prop_row(ui, &self.localizer.text("design-material-title"), 88.0, |ui| {
                        let request = Request::with(A::AssignMaterial, Target::Board(board.id));
                        let color = board.material_color.0;
                        let response = ui.add_enabled(
                            self.action_availability(request).is_ok(),
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "{} {}",
                                    board.material_name,
                                    short_length(board.material_default_thickness, locale)
                                ))
                                .color(theme_widgets::TEXT),
                            )
                            .right_text(egui::RichText::new("▾").color(theme_widgets::FAINT))
                            .fill(theme_widgets::APP)
                            .stroke(egui::Stroke::new(1.0, theme_widgets::BORDER_SOFT))
                            .corner_radius(6)
                            .min_size(egui::vec2(width, 28.0)),
                        );
                        let swatch = egui::Rect::from_center_size(
                            response.rect.left_center() + egui::vec2(-6.0, 0.0),
                            egui::vec2(0.0, 0.0),
                        );
                        let _ = swatch;
                        ui.painter().rect(
                            egui::Rect::from_min_size(
                                response.rect.right_center() + egui::vec2(-38.0, -6.0),
                                egui::vec2(12.0, 12.0),
                            ),
                            3.0,
                            egui::Color32::from_rgb(color[0], color[1], color[2]),
                            egui::Stroke::new(1.0, egui::Color32::from_black_alpha(28)),
                            egui::StrokeKind::Inside,
                        );
                        if response
                            .on_hover_text(self.localizer.text("board-assign-material"))
                            .clicked()
                        {
                            self.invoke_or_report(request);
                        }
                    });
                    let mut grain = source.grain_override;
                    theme_widgets::prop_row(ui, &self.localizer.text("design-grain-title"), 88.0, |ui| {
                        ui.add_enabled_ui(!self.modal_open(), |ui| {
                            egui::ComboBox::from_id_salt(("inspector-grain", board.id))
                                .width(width - 8.0)
                                .selected_text(match grain {
                                    None => format!(
                                        "{} · {}",
                                        self.localizer.text("design-grain-material"),
                                        self.localizer.text(match board.grain {
                                            BoardGrain::Length => "grain-short-length",
                                            BoardGrain::Width => "grain-short-width",
                                            BoardGrain::Unrestricted => "grain-short-any",
                                        })
                                    ),
                                    Some(_) => self.localizer.text(board_grain_key(grain)),
                                })
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
                    });
                    if grain != source.grain_override {
                        self.invoke_or_report(
                            Request::with(A::SetGrain, Target::Board(board.id))
                                .argument(Argument::Grain(grain)),
                        );
                    }
                    theme_widgets::inspector_heading(
                        ui,
                        &self.localizer.text("design-dimensions-title"),
                        |_| {},
                    );
                    self.show_shared_board_dimensions(ui, board.id, "inspector");
                    if board.thickness_provenance == ThicknessProvenance::StoredBoardValueDiffers {
                        let mut args = fluent_bundle::FluentArgs::new();
                        args.set(
                            "value",
                            format_length(board.material_default_thickness, Unit::Mm, locale, 1),
                        );
                        ui.label(
                            egui::RichText::new(
                                self.localizer.format("design-thickness-diff-note", Some(&args)),
                            )
                            .size(11.5)
                            .color(theme_widgets::WARN_INK),
                        );
                    }
                    let mut requested_frame = self.design.pose_frame;
                    theme_widgets::inspector_heading(
                        ui,
                        &self.localizer.text("design-transform-title"),
                        |ui| {
                            let world = self.localizer.text("placement-world");
                            let local = format!(
                                "{} · {}",
                                self.localizer.text("design-local"),
                                board.parent_name.as_deref().unwrap_or(&world)
                            );
                            let current = if requested_frame == CoordinateFrame::World {
                                world.clone()
                            } else {
                                local.clone()
                            };
                            ui.add_enabled_ui(!self.modal_open(), |ui| {
                                ui.menu_button(
                                    egui::RichText::new(format!("{current} ▾"))
                                        .size(11.5)
                                        .color(theme_widgets::SECONDARY),
                                    |ui| {
                                        if ui
                                            .selectable_label(
                                                requested_frame == CoordinateFrame::World,
                                                &world,
                                            )
                                            .clicked()
                                        {
                                            requested_frame = CoordinateFrame::World;
                                            ui.close();
                                        }
                                        if ui
                                            .selectable_label(
                                                requested_frame == CoordinateFrame::LocalParent,
                                                &local,
                                            )
                                            .clicked()
                                        {
                                            requested_frame = CoordinateFrame::LocalParent;
                                            ui.close();
                                        }
                                    },
                                );
                            });
                        },
                    );
                    if requested_frame != self.design.pose_frame {
                        self.request_pose_frame(requested_frame);
                    } else {
                        self.show_shared_pose_fields(ui, board.id);
                    }
                    theme_widgets::inspector_heading(ui, &self.localizer.text("design-stock-title"), |_| {});
                    if board.stock.is_empty() {
                        theme_widgets::warn_callout().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.add(icons::icon(icons::Icon::Warning, theme_widgets::WARN, 14.0));
                                ui.label(
                                    egui::RichText::new(self.localizer.text(match board.allocation.status {
                                        AllocationStatus::Unallocated => "design-needs-stock",
                                        AllocationStatus::Conflicted => "global-conflicted",
                                        AllocationStatus::UnknownSearchBudget => "sheet-feasibility-unknown",
                                        AllocationStatus::AllocatedValid => "board-allocated",
                                    }))
                                    .size(12.0)
                                    .color(theme_widgets::WARN_INK),
                                );
                            });
                            if theme_widgets::text_button(
                                ui,
                                &format!("{} →", self.localizer.text("design-open-cut-plan")),
                                theme_widgets::ACCENT_DARK,
                                !self.modal_open(),
                            )
                            .clicked()
                            {
                                self.navigate_session(Destination::BoardAllocation(board.id));
                            }
                        });
                    }
                    for allocation in &board.stock {
                        if let Some(piece) = self
                            .design
                            .stock_snapshot
                            .as_ref()
                            .and_then(|(_, stock)| stock.miniature(allocation.stock_id))
                            && sheet_miniature(ui, piece, board.id, &self.localizer).clicked()
                            && !self.modal_open()
                        {
                            self.navigate_session(Destination::BoardAllocation(board.id));
                        }
                        ui.add_space(4.0);
                        theme_widgets::prop_row(ui, &self.localizer.text("design-sheet"), 88.0, |ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.label(
                                theme_widgets::mono(
                                    allocation.stock_alias.as_deref().unwrap_or("—"),
                                    12.5,
                                )
                                .color(theme_widgets::TEXT),
                            );
                            ui.label(
                                egui::RichText::new(self.localizer.text("design-at"))
                                    .size(12.0)
                                    .color(theme_widgets::FAINT),
                            );
                            ui.label(
                                theme_widgets::mono(
                                    format!(
                                        "{}, {}{}",
                                        short_length(allocation.origin[0], locale),
                                        short_length(allocation.origin[1], locale),
                                        if allocation.quarter_turn { " · 90°" } else { "" }
                                    ),
                                    12.5,
                                )
                                .color(theme_widgets::SECONDARY),
                            );
                        });
                        theme_widgets::prop_row(ui, &self.localizer.text("design-placement"), 88.0, |ui| {
                            ui.add(icons::icon(icons::Icon::Lock, theme_widgets::MUTED, 13.0));
                            ui.label(self.localizer.text(if allocation.locked {
                                "design-locked"
                            } else {
                                "design-unlocked"
                            }));
                        });
                    }
                    theme_widgets::inspector_heading(ui, &self.localizer.text("design-bounding"), |_| {});
                    theme_widgets::prop_row(ui, &self.localizer.text("design-bounding-body"), 88.0, |ui| {
                        match &board.bounds_world.body {
                            Ok(m) => ui.label(
                                theme_widgets::mono(dims(m.dimensions_mm), 12.5)
                                    .color(theme_widgets::TEXT),
                            ),
                            Err(_) => ui.label(self.localizer.text("measurement-invalid")),
                        };
                    });
                });
            }
            DesignInspector::Assembly(assembly) => {
                let mut args = FluentArgs::new();
                args.set(
                    "count",
                    assembly.descendant_board_count as i64,
                );
                let parent = assembly
                    .parent_name
                    .clone()
                    .unwrap_or_else(|| self.localizer.text("assembly-root"));
                let mut in_args = FluentArgs::new();
                in_args.set("parent", parent.as_str());
                self.inspector_header(
                    ui,
                    icons::Icon::Assembly,
                    &assembly.name,
                    &short_id('a', assembly.id),
                    &format!(
                        "{} · {}",
                        self.localizer.format("design-group-in", Some(&in_args)),
                        self.localizer.format("design-part-count", Some(&args))
                    ),
                );
                body.show(ui, |ui| {
                    theme_widgets::inspector_heading(ui, &self.localizer.text("design-bounding"), |_| {});
                    for (label, result) in [
                        ("design-bounding-body", &assembly.bounds_world.body),
                        ("design-bounding-overall", &assembly.bounds_world.overall),
                    ] {
                        theme_widgets::prop_row(ui, &self.localizer.text(label), 88.0, |ui| match result {
                            Ok(m) => {
                                ui.label(
                                    theme_widgets::mono(dims(m.dimensions_mm), 12.5)
                                        .color(theme_widgets::TEXT),
                                );
                            }
                            Err(_) => {
                                ui.label(self.localizer.text("measurement-invalid"));
                            }
                        });
                    }
                    if assembly.issue_count > 0 {
                        ui.add_space(6.0);
                        theme_widgets::warn_callout().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}: {}",
                                    self.localizer.text("design-issue-count"),
                                    assembly.issue_count
                                ))
                                .size(12.0)
                                .color(theme_widgets::WARN_INK),
                            );
                        });
                    }
                    self.inspector_actions(
                        ui,
                        &[
                            (A::Transform, Target::None),
                            (A::DuplicateAssembly, Target::None),
                            (A::Ungroup, Target::None),
                        ],
                    );
                });
            }
            DesignInspector::Multi(multi) => {
                let mut args = FluentArgs::new();
                args.set("count", (multi.board_count + multi.assembly_count + multi.hardware_count) as i64);
                self.inspector_header(
                    ui,
                    icons::Icon::Layers,
                    &self.localizer.format("design-multi-count", Some(&args)),
                    "",
                    "",
                );
                body.show(ui, |ui| {
                    theme_widgets::inspector_heading(ui, &self.localizer.text("design-bounding"), |_| {});
                    for (label, result) in [
                        ("design-bounding-body", &multi.bounds_world.body),
                        ("design-bounding-overall", &multi.bounds_world.overall),
                    ] {
                        theme_widgets::prop_row(ui, &self.localizer.text(label), 88.0, |ui| match result {
                            Ok(m) => {
                                ui.label(
                                    theme_widgets::mono(dims(m.dimensions_mm), 12.5)
                                        .color(theme_widgets::TEXT),
                                );
                            }
                            Err(_) => {
                                ui.label(self.localizer.text("measurement-invalid"));
                            }
                        });
                    }
                    self.inspector_actions(
                        ui,
                        &[
                            (A::BatchDimensions, Target::None),
                            (A::Group, Target::None),
                            (A::Reparent, Target::None),
                            (A::Transform, Target::None),
                        ],
                    );
                });
            }
            DesignInspector::Hardware(hardware) => {
                self.inspector_header(
                    ui,
                    icons::Icon::Hinge,
                    &hardware.name,
                    &short_id('h', hardware.id),
                    &self.localizer.text("hardware-kind"),
                );
                body.show(ui, |ui| {
                    if let Some(size) = hardware.dimensions {
                        theme_widgets::inspector_heading(
                            ui,
                            &self.localizer.text("design-dimensions-title"),
                            |_| {},
                        );
                        theme_widgets::prop_row(ui, "X × Y × Z", 88.0, |ui| {
                            ui.label(
                                theme_widgets::mono(
                                    size.map(|v| short_length(v, locale)).join(" × "),
                                    12.5,
                                )
                                .color(theme_widgets::TEXT),
                            );
                        });
                    }
                    theme_widgets::inspector_heading(ui, &self.localizer.text("design-bounding"), |_| {});
                    theme_widgets::prop_row(
                        ui,
                        &self.localizer.text("design-bounding-overall"),
                        88.0,
                        |ui| match &hardware.bounds_world.overall {
                            Ok(m) => {
                                ui.label(
                                    theme_widgets::mono(dims(m.dimensions_mm), 12.5)
                                        .color(theme_widgets::TEXT),
                                );
                            }
                            Err(_) => {
                                ui.label(self.localizer.text("measurement-unknown-hardware"));
                            }
                        },
                    );
                    let mut actions = vec![(A::Transform, Target::None)];
                    if hardware.dimensions.is_some() {
                        actions.push((A::EditHardware, Target::Object(hardware.id)));
                        actions.push((A::DuplicateHardware, Target::Object(hardware.id)));
                    }
                    self.inspector_actions(ui, &actions);
                });
            }
        }
        if self.design.move_tool.mode == viewport::ToolMode::Measure {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(14, 4))
                .show(ui, |ui| self.show_measurement(ui));
        }
    }

    pub(crate) fn show_assembly_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_assembly() else {
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
                    match rotation(angles) {
                        Some(rotation) => self
                            .editor
                            .transform_selection(&draft.ids, delta, rotation, pivot)
                            .map(|_| ()),
                        None => Err(plan_my_cabinet::commands::EditError::Command(
                            AssemblyEditError::InvalidPose(UnitError::InvalidNumber),
                        )),
                    }
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
        self.modals.set_assembly(Some(draft));
    }
}

pub(crate) fn assembly_coordinate_field(
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
mod tests;
