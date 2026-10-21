//! Point-to-point Measure tool: snapped points, axis locks and kept
//! measurements. Session-only presentation state; nothing is persisted.
use super::*;
use plan_my_cabinet::dimension_input::{Locale, format_length};
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::render::snap::{self, Constraint, MeasurePoint, Snap, SnapKind};
use plan_my_cabinet::units::{Length, Unit};

/// World axis colours, matching the scene's axis lines.
const AXIS_COLORS: [egui::Color32; 3] = [
    egui::Color32::from_rgb(196, 69, 59),
    egui::Color32::from_rgb(79, 153, 87),
    egui::Color32::from_rgb(61, 112, 196),
];
const AXIS_NAMES: [&str; 3] = ["X", "Y", "Z"];

/// A finished measurement between two snapped points.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Kept {
    pub(crate) a: MeasurePoint,
    pub(crate) b: MeasurePoint,
    pub(crate) kinds: [SnapKind; 2],
}

#[derive(Default)]
pub(crate) struct MeasureTool {
    start: Option<(MeasurePoint, SnapKind)>,
    hover: Option<Snap>,
    /// X/Y/Z: a world axis the second point is held to, until the next click.
    axis_lock: Option<usize>,
    /// Shift: the inferred direction captured when Shift went down.
    held: Option<[f64; 3]>,
    kept: Vec<Kept>,
}

impl MeasureTool {
    #[cfg(test)]
    pub(crate) fn kept(&self) -> &[Kept] {
        &self.kept
    }

    pub(crate) fn in_progress(&self) -> bool {
        self.start.is_some()
    }

    #[cfg(test)]
    pub(crate) fn axis_lock(&self) -> Option<usize> {
        self.axis_lock
    }

    fn lock(&self) -> Option<[f64; 3]> {
        self.axis_lock.map(axis_direction).or(self.held)
    }

    pub(super) fn clear_hover(&mut self) {
        self.hover = None;
    }
}

fn axis_direction(axis: usize) -> [f64; 3] {
    std::array::from_fn(|i| f64::from(u8::from(i == axis)))
}

/// Clicks place points; X/Y/Z toggle an axis lock, Shift holds the shown
/// inference, Alt places free points, Esc cancels then clears, Delete removes
/// the last measurement. Drags still orbit and pan, so the camera can turn
/// between the two clicks.
pub(super) fn interact(
    ui: &egui::Ui,
    response: &egui::Response,
    project: &Project,
    camera: &Camera,
    rect: egui::Rect,
    selection: &Selection,
    measure: &mut MeasureTool,
) {
    measure
        .kept
        .retain(|k| k.a.resolve(project).is_some() && k.b.resolve(project).is_some());
    let mut from = measure.start.and_then(|(point, _)| point.resolve(project));
    if from.is_none() {
        measure.start = None;
    }
    if (response.has_focus() || response.hovered())
        && !ui.ctx().text_edit_focused()
        && !egui::Popup::is_any_open(ui.ctx())
    {
        ui.input(|i| {
            let plain = !(i.modifiers.command || i.modifiers.ctrl || i.modifiers.alt);
            for (key, axis) in [(egui::Key::X, 0), (egui::Key::Y, 1), (egui::Key::Z, 2)] {
                if plain && i.key_pressed(key) {
                    measure.axis_lock = (measure.axis_lock != Some(axis)).then_some(axis);
                }
            }
            if i.key_pressed(egui::Key::Escape) {
                if measure.start.take().is_none() {
                    measure.kept.clear();
                }
                measure.axis_lock = None;
            }
            if plain && (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                measure.kept.pop();
            }
        });
        if measure.start.is_none() {
            from = None;
        }
    }
    let (shift, alt) = ui.input(|i| (i.modifiers.shift, i.modifiers.alt));
    if !shift {
        measure.held = None;
    } else if measure.held.is_none()
        && measure.axis_lock.is_none()
        && from.is_some()
        && let Some(hover) = measure.hover
    {
        measure.held = hover
            .axis
            .map(axis_direction)
            .or(hover.edge.filter(|_| hover.kind == SnapKind::OnEdge));
    }
    let constraint = Constraint {
        from,
        lock: from.and(measure.lock()),
        free: alt,
    };
    let boxes = snap::scene_boxes(project, selection);
    if !ui.input(|i| i.pointer.any_down()) {
        measure.hover = response
            .hover_pos()
            .map(|pointer| snap::measure_target(&boxes, camera, rect, pointer, constraint));
    }
    if response.clicked_by(egui::PointerButton::Primary)
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let target = snap::measure_target(&boxes, camera, rect, pointer, constraint);
        match (measure.start, from) {
            (Some((a, kind)), Some(from)) => {
                // A second click on the first point measures nothing; keep waiting.
                if distance(from, target.point) > 0.0 {
                    measure.kept.push(Kept {
                        a,
                        b: target.measure_point(),
                        kinds: [kind, target.kind],
                    });
                    measure.start = None;
                    measure.axis_lock = None;
                }
            }
            _ => measure.start = Some((target.measure_point(), target.kind)),
        }
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        super::set_hover(ui.ctx(), measure.hover.and_then(|h| h.anchor).map(|a| a.id));
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt()
}

/// Display length with trailing zeros trimmed: "642.5 mm", "600 mm", "25,4 cm".
pub(super) fn length_text(mm: f64, unit: Unit, locale: Locale) -> String {
    let decimals = match unit {
        Unit::Mm => 2,
        Unit::Cm | Unit::Inch => 3,
        Unit::Foot => 4,
        Unit::M => 5,
    };
    let text = format_length(
        Length::from_micrometres((mm * 1000.0).round() as i64),
        unit,
        locale,
        decimals,
    );
    let (number, suffix) = text.split_once(' ').unwrap_or((&text, ""));
    let number = if number.contains(['.', ',']) {
        number.trim_end_matches('0').trim_end_matches(['.', ','])
    } else {
        number
    };
    format!("{number} {suffix}")
}

/// The axis colour when a segment runs along a world axis, else the accent.
fn segment_color(a: [f64; 3], b: [f64; 3]) -> egui::Color32 {
    let delta: [f64; 3] = std::array::from_fn(|i| (b[i] - a[i]).abs());
    (0..3)
        .find(|&axis| delta[axis] > 0.0 && (0..3).all(|i| i == axis || delta[i] <= 0.01))
        .map_or(crate::theme_widgets::ACCENT, |axis| AXIS_COLORS[axis])
}

fn snap_label(snap: &Snap, localizer: &Localizer) -> String {
    let kind = if snap.kind == SnapKind::OnAxis && snap.axis.is_none() {
        localizer.text("measure-snap-parallel")
    } else {
        localizer.text(snap.kind.key())
    };
    match snap.axis {
        Some(axis) if snap.kind != SnapKind::OnAxis => format!("{kind} · {}", AXIS_NAMES[axis]),
        Some(axis) => format!("{kind} {}", AXIS_NAMES[axis]),
        None => kind,
    }
}

fn dashed(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2, stroke: egui::Stroke) {
    let length = from.distance(to);
    if !length.is_finite() || length < 1.0 {
        return;
    }
    let step = (to - from) / length;
    let mut at = 0.0;
    while at < length {
        let end = (at + 6.0).min(length);
        painter.line_segment([from + step * at, from + step * end], stroke);
        at += 10.0;
    }
}

/// The marker shape names the snap: filled dot for a corner, ring for a
/// midpoint, ring with dot for a face centre, square on an edge, diamond on a face.
fn paint_marker(painter: &egui::Painter, at: egui::Pos2, kind: SnapKind, color: egui::Color32) {
    let outline = egui::Stroke::new(1.5, crate::theme_widgets::PANEL);
    let stroke = egui::Stroke::new(2.0, color);
    match kind {
        SnapKind::Endpoint => {
            painter.circle(at, 5.5, color, outline);
        }
        SnapKind::Midpoint => {
            painter.circle_stroke(at, 6.0, egui::Stroke::new(3.5, crate::theme_widgets::PANEL));
            painter.circle_stroke(at, 6.0, stroke);
        }
        SnapKind::FaceCenter => {
            painter.circle_stroke(at, 6.5, stroke);
            painter.circle_filled(at, 2.5, color);
        }
        SnapKind::OnEdge => {
            painter.rect(
                egui::Rect::from_center_size(at, egui::vec2(9.0, 9.0)),
                1.0,
                color,
                outline,
                egui::StrokeKind::Outside,
            );
        }
        SnapKind::OnFace | SnapKind::OnAxis => {
            let r = 6.0;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    at + egui::vec2(0.0, -r),
                    at + egui::vec2(r, 0.0),
                    at + egui::vec2(0.0, r),
                    at + egui::vec2(-r, 0.0),
                ],
                color,
                outline,
            ));
        }
        SnapKind::Ground | SnapKind::Free => {
            painter.line_segment(
                [at - egui::vec2(5.0, 0.0), at + egui::vec2(5.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [at - egui::vec2(0.0, 5.0), at + egui::vec2(0.0, 5.0)],
                stroke,
            );
        }
    }
}

fn pill(painter: &egui::Painter, rect: egui::Rect, at: egui::Pos2, text: String) {
    let galley = painter.layout_no_wrap(
        text,
        egui::FontId::monospace(11.5),
        crate::theme_widgets::PANEL,
    );
    let size = galley.size() + egui::vec2(14.0, 8.0);
    let mut origin = at - size / 2.0;
    origin.x = origin.x.clamp(
        rect.left() + 4.0,
        (rect.right() - size.x - 4.0).max(rect.left() + 4.0),
    );
    origin.y = origin.y.clamp(
        rect.top() + 4.0,
        (rect.bottom() - size.y - 4.0).max(rect.top() + 4.0),
    );
    let pill = egui::Rect::from_min_size(origin, size);
    painter.rect_filled(pill, 5.0, crate::theme_widgets::TEXT);
    painter.galley(
        pill.min + egui::vec2(7.0, 4.0),
        galley,
        crate::theme_widgets::PANEL,
    );
}

fn paint_segment(
    painter: &egui::Painter,
    camera: &Camera,
    rect: egui::Rect,
    a: [f64; 3],
    b: [f64; 3],
    label: String,
) {
    let (Some(from), Some(to)) = (camera.project(a, rect), camera.project(b, rect)) else {
        return;
    };
    let color = segment_color(a, b);
    painter.line_segment(
        [from, to],
        egui::Stroke::new(4.0, crate::theme_widgets::PANEL),
    );
    painter.line_segment([from, to], egui::Stroke::new(2.0, color));
    for end in [from, to] {
        painter.circle(
            end,
            3.5,
            color,
            egui::Stroke::new(1.0, crate::theme_widgets::PANEL),
        );
    }
    pill(painter, rect, from + (to - from) / 2.0, label);
}

/// A dashed line through `from` along `direction`, across the visible canvas.
fn paint_guide(
    painter: &egui::Painter,
    camera: &Camera,
    rect: egui::Rect,
    from: [f64; 3],
    direction: [f64; 3],
    color: egui::Color32,
) {
    let Some(origin) = camera.project(from, rect) else {
        return;
    };
    let reach = camera.distance.max(1000.0) * 0.5;
    let ends = [-1.0, 1.0].map(|sign| {
        camera.project(
            std::array::from_fn(|i| from[i] + direction[i] * reach * sign),
            rect,
        )
    });
    for end in ends.into_iter().flatten() {
        let along = end - origin;
        if along.length() < 1.0 {
            continue;
        }
        let far = origin + along.normalized() * (rect.width() + rect.height());
        dashed(
            painter,
            origin,
            far,
            egui::Stroke::new(1.2, color.gamma_multiply(0.8)),
        );
    }
}

pub(super) fn paint(
    ui: &egui::Ui,
    camera: &Camera,
    project: &Project,
    rect: egui::Rect,
    measure: &MeasureTool,
    pt: bool,
) {
    let localizer = Localizer::new(if pt { Language::PtBr } else { Language::En });
    let locale = if pt { Locale::PtBr } else { Locale::En };
    let unit = project.display_unit;
    let painter = ui.painter().with_clip_rect(rect);
    let mut last = None;
    for kept in &measure.kept {
        let (Some(a), Some(b)) = (kept.a.resolve(project), kept.b.resolve(project)) else {
            continue;
        };
        paint_segment(
            &painter,
            camera,
            rect,
            a,
            b,
            length_text(distance(a, b), unit, locale),
        );
        last = Some((a, b, kept.kinds));
    }
    let from = measure.start.and_then(|(point, _)| point.resolve(project));
    if let Some(from) = from {
        let direction = measure
            .lock()
            .or(measure.hover.and_then(|h| h.axis).map(axis_direction));
        if let Some(direction) = direction {
            let color =
                snap::axis_of(direction).map_or(crate::theme_widgets::ACCENT, |a| AXIS_COLORS[a]);
            paint_guide(&painter, camera, rect, from, direction, color);
        }
        if let Some(hover) = measure.hover {
            if let Some(source) = hover.source
                && let (Some(s), Some(p)) = (
                    camera.project(source, rect),
                    camera.project(hover.point, rect),
                )
            {
                dashed(
                    &painter,
                    s,
                    p,
                    egui::Stroke::new(1.0, crate::theme_widgets::MUTED),
                );
                paint_marker(&painter, s, SnapKind::Endpoint, crate::theme_widgets::MUTED);
            }
            paint_segment(
                &painter,
                camera,
                rect,
                from,
                hover.point,
                length_text(distance(from, hover.point), unit, locale),
            );
            last = Some((
                from,
                hover.point,
                [
                    measure.start.map_or(hover.kind, |(_, kind)| kind),
                    hover.kind,
                ],
            ));
        } else if let Some(at) = camera.project(from, rect) {
            paint_marker(
                &painter,
                at,
                SnapKind::Endpoint,
                crate::theme_widgets::ACCENT,
            );
        }
    }
    if let Some(hover) = measure.hover
        && let Some(at) = camera.project(hover.point, rect)
    {
        let color = hover
            .axis
            .map_or(crate::theme_widgets::ACCENT, |a| AXIS_COLORS[a]);
        paint_marker(&painter, at, hover.kind, color);
        let galley = painter.layout_no_wrap(
            snap_label(&hover, &localizer),
            egui::FontId::proportional(11.5),
            crate::theme_widgets::TEXT,
        );
        let tip = egui::Rect::from_min_size(
            at + egui::vec2(12.0, 10.0),
            galley.size() + egui::vec2(10.0, 6.0),
        );
        painter.rect(
            tip,
            4.0,
            crate::theme_widgets::PANEL,
            egui::Stroke::new(1.0, crate::theme_widgets::BORDER_STRONG),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            tip.min + egui::vec2(5.0, 3.0),
            galley,
            crate::theme_widgets::TEXT,
        );
    }
    paint_panel(ui, rect, measure, last, &localizer, unit, locale);
}

/// The readout card, docked bottom-left clear of the tool strip, the camera
/// bar, the snap chip and the selected-board strip.
pub(super) fn panel_origin(rect: egui::Rect) -> egui::Pos2 {
    rect.left_bottom() + egui::vec2(66.0, -72.0)
}

fn paint_panel(
    ui: &egui::Ui,
    rect: egui::Rect,
    measure: &MeasureTool,
    last: Option<([f64; 3], [f64; 3], [SnapKind; 2])>,
    localizer: &Localizer,
    unit: Unit,
    locale: Locale,
) {
    use crate::theme_widgets as tw;
    if rect.width() < 320.0 || rect.height() < 240.0 {
        return;
    }
    egui::Area::new(ui.id().with("viewport-measure-panel"))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(panel_origin(rect))
        .interactable(false)
        .show(ui.ctx(), |ui| {
            tw::floating_frame()
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_max_width((rect.width() - 90.0).clamp(160.0, 280.0));
                    ui.spacing_mut().item_spacing.y = 3.0;
                    let locked = measure
                        .axis_lock
                        .map(|axis| (AXIS_NAMES[axis].to_owned(), AXIS_COLORS[axis]))
                        .or(measure.held.map(|direction| {
                            snap::axis_of(direction).map_or(
                                (localizer.text("measure-snap-parallel"), tw::ACCENT),
                                |axis| (AXIS_NAMES[axis].to_owned(), AXIS_COLORS[axis]),
                            )
                        }));
                    if let Some((name, color)) = locked {
                        ui.label(
                            tw::semibold(
                                ui,
                                format!("{name} · {}", localizer.text("measure-locked")),
                                11.5,
                            )
                            .color(color),
                        );
                    }
                    let Some((a, b, kinds)) = last else {
                        ui.label(
                            egui::RichText::new(localizer.text(if measure.in_progress() {
                                "measure-next"
                            } else {
                                "measure-start"
                            }))
                            .size(12.0)
                            .color(tw::SECONDARY),
                        );
                        return;
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(localizer.text("measure-distance"))
                                .size(12.0)
                                .color(tw::SECONDARY),
                        );
                        ui.label(
                            tw::mono(length_text(distance(a, b), unit, locale), 15.0)
                                .color(tw::TEXT),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        for axis in 0..3 {
                            ui.label(
                                tw::mono(
                                    format!(
                                        "Δ{} {}",
                                        AXIS_NAMES[axis],
                                        length_text((b[axis] - a[axis]).abs(), unit, locale)
                                    ),
                                    11.0,
                                )
                                .color(AXIS_COLORS[axis]),
                            );
                        }
                    });
                    ui.label(
                        egui::RichText::new(format!(
                            "{} → {}",
                            localizer.text(kinds[0].key()),
                            localizer.text(kinds[1].key())
                        ))
                        .size(11.0)
                        .color(tw::FAINT),
                    );
                });
        });
}

#[cfg(test)]
mod tests;
