//! Screen-space drag annotations, projected through the same camera and canvas rect as picking.
use super::*;
use plan_my_cabinet::dimension_input::{Locale, format_length};
use plan_my_cabinet::measurements::{Frame, Scope};

fn selected_dimensions(
    camera: &Camera,
    project: &Project,
    selection: &Selection,
    rect: egui::Rect,
) -> Option<[(egui::Pos2, egui::Pos2, plan_my_cabinet::units::Length); 2]> {
    let id = selection.active?;
    if selection.ids.len() != 1 || !selection.visible(project, id) {
        return None;
    }
    let board = project.boards.iter().find(|b| b.id == id)?;
    let corners = board_corners(project, board)?;
    let origin = camera.project(corners[0], rect)?;
    let length = camera.project(corners[1], rect)?;
    let width = camera.project(corners[2], rect)?;
    Some([(origin, length, board.length), (origin, width, board.width)])
}

#[cfg_attr(not(test), allow(dead_code))]
fn dimension_badge_origin(
    rect: egui::Rect,
    mid: egui::Pos2,
    size: egui::Vec2,
    index: usize,
) -> egui::Pos2 {
    let mut origin = mid + egui::vec2(10.0, if index == 0 { -24.0 } else { 8.0 });
    // At compact widths the selected-board HUD is docked to the bottom left.
    // Keep its full scrollable card and these paint-only labels separate.
    if origin.x < rect.left() + 430.0 && origin.y + size.y > rect.bottom() - 300.0 {
        origin.y = rect.bottom() - if index == 0 { 320.0 } else { 280.0 };
    }
    origin.x = origin.x.clamp(
        rect.left() + 4.0,
        (rect.right() - size.x - 4.0).max(rect.left() + 4.0),
    );
    origin.y = origin.y.clamp(
        rect.top() + 4.0,
        (rect.bottom() - size.y - 4.0).max(rect.top() + 4.0),
    );
    origin
}

fn trim_mm(value: plan_my_cabinet::units::Length, unit: plan_my_cabinet::units::Unit, locale: Locale) -> String {
    let text = format_length(value, unit, locale, 2);
    let number = text.split_whitespace().next().unwrap_or("").to_owned();
    if number.contains(['.', ',']) {
        number
            .trim_end_matches('0')
            .trim_end_matches(['.', ','])
            .to_owned()
    } else {
        number
    }
}

/// One dark pill ("764 × 537 × 18") beside the selected board with a short
/// leader to its right-most projected corner.
fn paint_selected_dimensions(
    ui: &egui::Ui,
    camera: &Camera,
    project: &Project,
    selection: &Selection,
    rect: egui::Rect,
    pt: bool,
) {
    if selected_dimensions(camera, project, selection, rect).is_none() {
        return;
    }
    let Some(board) = selection
        .active
        .and_then(|id| project.boards.iter().find(|b| b.id == id))
    else {
        return;
    };
    let Some(corners) = board_corners(project, board) else {
        return;
    };
    let projected: Vec<_> = corners
        .iter()
        .filter_map(|c| camera.project(*c, rect))
        .filter(|p| rect.contains(*p))
        .collect();
    if projected.is_empty() {
        return;
    }
    // Anchor between the board's projected centre and its lowest corner, so
    // the leader lands on the board itself rather than a panel behind it.
    let centre = projected
        .iter()
        .fold(egui::Vec2::ZERO, |sum, p| sum + p.to_vec2())
        / projected.len() as f32;
    let lowest = projected
        .iter()
        .copied()
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .unwrap_or(projected[0]);
    let anchor = centre.to_pos2() + (lowest - centre.to_pos2()) * 0.6;
    let locale = if pt { Locale::PtBr } else { Locale::En };
    let unit = project.display_unit;
    let text = format!(
        "{} × {} × {}",
        trim_mm(board.length, unit, locale),
        trim_mm(board.width, unit, locale),
        trim_mm(board.thickness, unit, locale)
    );
    let painter = ui.painter().with_clip_rect(rect);
    let galley = painter.layout_no_wrap(
        text,
        egui::FontId::monospace(11.5),
        crate::theme_widgets::PANEL,
    );
    let size = galley.size() + egui::vec2(16.0, 9.0);
    let mut origin = anchor + egui::vec2(46.0, 18.0);
    origin.x = origin
        .x
        .clamp(rect.left() + 60.0, (rect.right() - size.x - 8.0).max(rect.left() + 60.0));
    origin.y = origin
        .y
        .clamp(rect.top() + 56.0, (rect.bottom() - size.y - 80.0).max(rect.top() + 56.0));
    let pill = egui::Rect::from_min_size(origin, size);
    painter.line_segment(
        [anchor, pill.left_center()],
        egui::Stroke::new(1.0, crate::theme_widgets::MUTED),
    );
    painter.rect_filled(pill, 5.0, crate::theme_widgets::TEXT);
    painter.galley(
        pill.min + egui::vec2(8.0, 4.5),
        galley,
        crate::theme_widgets::PANEL,
    );
}

#[allow(clippy::too_many_arguments)] // Camera, canvas, transient tool, and measurement choices are independent view state.
pub(super) fn paint(
    ui: &egui::Ui,
    camera: &Camera,
    project: &Project,
    selection: &Selection,
    tool: &MoveTool,
    rect: egui::Rect,
    pt: bool,
    scope: Scope,
    frame: Frame,
) {
    paint_selected_dimensions(ui, camera, project, selection, rect, pt);
    if tool.mode == ToolMode::Measure && rect.width() > 120.0 && rect.height() > 90.0 {
        let localizer = plan_my_cabinet::i18n::Localizer::new(if pt {
            plan_my_cabinet::i18n::Language::PtBr
        } else {
            plan_my_cabinet::i18n::Language::En
        });
        let (heading, value) =
            super::measurement_readout(project, selection, scope, frame, &localizer);
        let painter = ui.painter().with_clip_rect(rect);
        let font = egui::FontId::proportional(13.0);
        let color = egui::Color32::WHITE;
        let max_width = (rect.width() - 32.0).max(40.0);
        let title = painter.layout(heading, font.clone(), color, max_width);
        let value = painter.layout(value, font, color, max_width);
        let size = egui::vec2(
            title.size().x.max(value.size().x) + 20.0,
            title.size().y + value.size().y + 18.0,
        );
        let origin = rect.right_top() - egui::vec2(size.x + 8.0, -8.0);
        painter.rect_filled(
            egui::Rect::from_min_size(origin, size).intersect(rect),
            6.0,
            egui::Color32::from_rgba_unmultiplied(42, 37, 32, 230),
        );
        let second_row = 8.0 + title.size().y;
        painter.galley(origin + egui::vec2(10.0, 6.0), title, color);
        painter.galley(origin + egui::vec2(10.0, second_row), value, color);
    }
    if let Some(drag) = &tool.drag {
        if let Some(DragSnap::Face(snap)) = drag.snap
            && let Some(pose) = drag.last_pose
            && let Some(source) = project.boards.iter().find(|b| b.id == drag.board_id)
            && let Some(target) = project.boards.iter().find(|b| b.id == snap.target_id)
            && let Some(target_pose) = world_pose(project, target)
        {
            // The snapped faces coincide in world space. Anchor both labels to
            // their actual face centres, then separate their badges on screen so
            // cyan source and magenta target remain distinguishable at a glance.
            let center = |board: &Board, pose: plan_my_cabinet::units::Pose, face: BoardFace| {
                let mut local = board
                    .blank_dimensions()
                    .map(|d| d.micrometres() as f64 / 2000.0);
                if face.side == Side::Negative {
                    local[face.axis] = 0.0;
                } else {
                    local[face.axis] *= 2.0;
                }
                pose.transform_point(local)
                    .ok()
                    .and_then(|world| camera.project(world, rect))
            };
            if let (Some(from), Some(to)) = (
                center(source, pose, snap.source_face),
                center(target, target_pose, snap.target_face),
            ) && rect.contains(from)
                && rect.contains(to)
            {
                let painter = ui.painter().with_clip_rect(rect);
                for (anchor, offset, color, label) in [
                    (
                        from,
                        egui::vec2(-86.0, -28.0),
                        egui::Color32::from_rgb(38, 170, 180),
                        if pt { "Origem" } else { "Source" },
                    ),
                    (
                        to,
                        egui::vec2(20.0, 18.0),
                        egui::Color32::from_rgb(240, 72, 170),
                        if pt { "Destino" } else { "Target" },
                    ),
                ] {
                    let badge = anchor + offset;
                    painter.line_segment([anchor, badge], egui::Stroke::new(2.0, color));
                    painter.circle_filled(anchor, 4.0, color);
                    painter.text(
                        badge,
                        egui::Align2::LEFT_CENTER,
                        label,
                        egui::FontId::proportional(14.0),
                        color,
                    );
                }
            }
        }
        if matches!(drag.snap, Some(DragSnap::Grid))
            && let Some(pose) = drag.last_pose
            && let Some(point) =
                camera.project([pose.translation_mm[0], pose.translation_mm[1], 0.0], rect)
            && rect.contains(point)
        {
            // At distant zoom levels the rendered grid omits fine lines. Mark
            // the exact candidate intersection so the target remains visible.
            let painter = ui.painter().with_clip_rect(rect);
            let color = egui::Color32::from_rgb(240, 72, 170);
            painter.circle_stroke(point, 7.0, egui::Stroke::new(2.0, color));
            painter.line_segment(
                [
                    point + egui::vec2(-11.0, 0.0),
                    point + egui::vec2(11.0, 0.0),
                ],
                egui::Stroke::new(2.0, color),
            );
            painter.line_segment(
                [
                    point + egui::vec2(0.0, -11.0),
                    point + egui::vec2(0.0, 11.0),
                ],
                egui::Stroke::new(2.0, color),
            );
        }
        let label = if let Some(DragSnap::Face(snap)) = drag.snap {
            let name = project
                .boards
                .iter()
                .find(|b| b.id == snap.target_id)
                .map_or("—", |b| b.name.as_str());
            if pt {
                format!("Encaixe: {name} · solte para aceitar")
            } else {
                format!("Snap: {name} · release to accept")
            }
        } else if let Some(DragSnap::Grid) = drag.snap {
            if pt {
                "Grade XY · solte para aceitar".into()
            } else {
                "XY grid · release to accept".into()
            }
        } else if ui.input(|i| i.modifiers.alt) {
            if pt {
                "Encaixe ignorado · solte para aceitar".into()
            } else {
                "Snap bypassed · release to accept".into()
            }
        } else if pt {
            "Posição livre · solte para aceitar".into()
        } else {
            "Free position · release to accept".into()
        };
        let painter = ui.painter().with_clip_rect(rect);
        let text = painter.layout_no_wrap(
            label,
            egui::FontId::proportional(15.0),
            egui::Color32::WHITE,
        );
        let origin = rect.left_top() + egui::vec2(12.0, 12.0);
        let chip = egui::Rect::from_min_size(origin, text.size() + egui::vec2(16.0, 10.0));
        painter.rect_filled(
            chip.intersect(rect),
            6.0,
            egui::Color32::from_rgba_unmultiplied(42, 37, 32, 230),
        );
        painter.galley(origin + egui::vec2(8.0, 5.0), text, egui::Color32::WHITE);
    }
}

#[cfg(test)]
mod dimension_tests {
    use super::*;

    #[test]
    fn dimensions_follow_board_corners_at_both_projections_and_canvas_positions() {
        let project = plan_my_cabinet::reference_fixture::project();
        let board = &project.boards[0];
        let mut selection = Selection::default();
        selection.choose(Some(board.id), false);
        let mut camera = Camera::default();
        for projection in [Projection::Orthographic, Projection::Perspective] {
            camera.projection = projection;
            for rect in [
                egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0)),
                egui::Rect::from_min_size(egui::pos2(270.0, 64.0), egui::vec2(500.0, 430.0)),
            ] {
                let edges = selected_dimensions(&camera, &project, &selection, rect).unwrap();
                let corners = board_corners(&project, board).unwrap();
                assert_eq!(edges[0].0, camera.project(corners[0], rect).unwrap());
                assert_eq!(edges[0].1, camera.project(corners[1], rect).unwrap());
                assert_eq!(edges[1].1, camera.project(corners[2], rect).unwrap());
                assert_eq!(edges[0].2, board.length);
                assert_eq!(edges[1].2, board.width);
            }
        }
        selection.hidden.insert(board.id);
        assert!(
            selected_dimensions(&camera, &project, &selection, egui::Rect::EVERYTHING).is_none()
        );
        selection.hidden.clear();
        selection.choose(Some(project.boards[1].id), true);
        assert!(
            selected_dimensions(&camera, &project, &selection, egui::Rect::EVERYTHING).is_none()
        );
    }

    #[test]
    fn compact_projected_labels_stay_clear_of_bottom_left_hud() {
        for (width, height) in [(550.0, 490.0), (810.0, 700.0)] {
            let rect =
                egui::Rect::from_min_size(egui::pos2(60.0, 130.0), egui::vec2(width, height));
            let size = egui::vec2(94.0, 26.0);
            let mid = rect.left_bottom() + egui::vec2(180.0, -100.0);
            let first = dimension_badge_origin(rect, mid, size, 0);
            let second = dimension_badge_origin(rect, mid, size, 1);
            for point in [first, second] {
                assert!(point.y + size.y < rect.bottom() - 240.0);
                assert!(rect.contains(point));
            }
            assert!(second.y - first.y >= size.y);
        }
    }
}
