//! Face handles on the selected board: drag one to change that dimension while
//! the opposite face stays put. The viewport only proposes values; the host
//! previews them and commits one undoable edit on release.
use super::*;
use plan_my_cabinet::board_dimensions::BoardDimension;
use plan_my_cabinet::units::{Anchor, Length};

/// Hit radius around a handle, in points.
const GRAB_RADIUS: f32 = 9.0;
/// A face seen almost edge-on cannot be dragged meaningfully.
const MIN_POINTS_PER_100_MM: f32 = 12.0;

/// A face handle of the selected board, in screen space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResizeHandle {
    /// Board-local axis: 0 = length, 1 = width.
    pub axis: usize,
    /// The face at the far end of the axis (the local origin face otherwise).
    pub positive: bool,
    pub at: egui::Pos2,
    /// Screen movement per millimetre along the outward face normal.
    pub per_mm: egui::Vec2,
}

impl ResizeHandle {
    fn dimension(&self) -> BoardDimension {
        if self.axis == 0 {
            BoardDimension::Length
        } else {
            BoardDimension::Width
        }
    }

    /// The opposite face stays fixed.
    fn anchor(&self) -> Anchor {
        if self.positive {
            Anchor::Start
        } else {
            Anchor::End
        }
    }

    fn cursor(&self) -> egui::CursorIcon {
        let angle = self
            .per_mm
            .y
            .atan2(self.per_mm.x)
            .to_degrees()
            .rem_euclid(180.0);
        match angle {
            a if !(22.5..157.5).contains(&a) => egui::CursorIcon::ResizeHorizontal,
            a if (67.5..112.5).contains(&a) => egui::CursorIcon::ResizeVertical,
            a if a < 90.0 => egui::CursorIcon::ResizeNwSe,
            _ => egui::CursorIcon::ResizeNeSw,
        }
    }
}

pub(crate) struct ResizeDrag {
    board_id: Uuid,
    handle: ResizeHandle,
    start: egui::Pos2,
    original_mm: f64,
    last: Option<Length>,
}

/// One proposed dimension for the host to preview or commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResizeRequest {
    pub board_id: Uuid,
    pub dimension: BoardDimension,
    pub value: Length,
    pub anchor: Anchor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeAction {
    Preview(ResizeRequest),
    Accept(ResizeRequest),
    Cancel,
}

/// Handles for the single selected, visible board, when the host allows it.
pub(super) fn handles(
    project: &Project,
    camera: &Camera,
    rect: egui::Rect,
    selection: &Selection,
    tool: &MoveTool,
) -> Vec<ResizeHandle> {
    let Some(id) = selection.active else {
        return Vec::new();
    };
    if !tool.resize_enabled
        || tool.mode == ToolMode::Measure
        || tool.drag.is_some()
        || selection.ids.len() != 1
        || !selection.visible(project, id)
    {
        return Vec::new();
    }
    let Some(board) = project.boards.iter().find(|b| b.id == id) else {
        return Vec::new();
    };
    let Some(pose) = world_pose(project, board) else {
        return Vec::new();
    };
    let size = board
        .blank_dimensions()
        .map(|v| v.micrometres() as f64 / 1000.0);
    let mut result = Vec::new();
    for axis in 0..2 {
        for positive in [false, true] {
            let mut centre = size.map(|v| v / 2.0);
            centre[axis] = if positive { size[axis] } else { 0.0 };
            let mut outward = centre;
            outward[axis] += if positive { 100.0 } else { -100.0 };
            let (Ok(a), Ok(b)) = (pose.transform_point(centre), pose.transform_point(outward))
            else {
                continue;
            };
            let (Some(at), Some(far)) = (camera.project(a, rect), camera.project(b, rect)) else {
                continue;
            };
            let per_100 = far - at;
            if per_100.length() < MIN_POINTS_PER_100_MM || !rect.contains(at) {
                continue;
            }
            result.push(ResizeHandle {
                axis,
                positive,
                at,
                per_mm: per_100 / 100.0,
            });
        }
    }
    result
}

fn near(handles: &[ResizeHandle], pointer: egui::Pos2) -> Option<ResizeHandle> {
    handles
        .iter()
        .filter(|h| h.at.distance(pointer) <= GRAB_RADIUS)
        .min_by(|a, b| a.at.distance(pointer).total_cmp(&b.at.distance(pointer)))
        .copied()
}

/// The dimension implied by moving the pointer from `start` to `pointer`.
/// Snaps to the grid spacing (or whole millimetres without grid snap); Alt
/// gives 0.1 mm steps. Never below one step.
pub(super) fn proposed_value(
    handle: &ResizeHandle,
    original_mm: f64,
    start: egui::Pos2,
    pointer: egui::Pos2,
    step_mm: f64,
) -> Length {
    let along = (pointer - start).dot(handle.per_mm) / handle.per_mm.length_sq();
    let raw = original_mm + along as f64;
    let snapped = ((raw / step_mm).round() * step_mm).max(step_mm);
    Length::from_micrometres((snapped * 1000.0).round() as i64)
}

/// Pointer handling for handles. Returns true while a handle owns the drag,
/// so the canvas does not also orbit or start a move.
pub(super) fn interact(
    ui: &egui::Ui,
    response: &egui::Response,
    project: &Project,
    selected: Option<Uuid>,
    tool: &mut MoveTool,
    handles: &[ResizeHandle],
    interaction: &mut ViewportInteraction,
) -> bool {
    if tool.resize.is_none()
        && response.drag_started_by(egui::PointerButton::Primary)
        && !ui.input(|i| i.modifiers.shift || i.modifiers.command)
        && let Some(start) = ui.input(|i| i.pointer.press_origin())
        && let Some(handle) = near(handles, start)
        && let Some(board_id) = selected
        && let Some(board) = project.boards.iter().find(|b| b.id == board_id)
    {
        tool.resize = Some(ResizeDrag {
            board_id,
            handle,
            start,
            original_mm: board.blank_dimensions()[handle.axis].micrometres() as f64 / 1000.0,
            last: None,
        });
    }
    tool.resize_hover = None;
    let Some(drag) = &mut tool.resize else {
        if !ui.input(|i| i.pointer.any_down())
            && let Some(pointer) = response.hover_pos()
            && let Some(handle) = near(handles, pointer)
        {
            ui.ctx().set_cursor_icon(handle.cursor());
            tool.resize_hover = Some((handle.axis, handle.positive));
        }
        return false;
    };
    ui.ctx().set_cursor_icon(drag.handle.cursor());
    let request = |value| ResizeRequest {
        board_id: drag.board_id,
        dimension: drag.handle.dimension(),
        value,
        anchor: drag.handle.anchor(),
    };
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        tool.resize = None;
        interaction.resize = Some(ResizeAction::Cancel);
    } else if response.drag_stopped_by(egui::PointerButton::Primary) {
        interaction.resize = Some(match drag.last {
            Some(value) => ResizeAction::Accept(request(value)),
            None => ResizeAction::Cancel,
        });
        tool.resize = None;
    } else if let Some(pointer) = response.interact_pointer_pos() {
        let step = if ui.input(|i| i.modifiers.alt) {
            0.1
        } else if tool.grid_snap {
            project.grid_spacing.micrometres() as f64 / 1000.0
        } else {
            1.0
        };
        let value = proposed_value(&drag.handle, drag.original_mm, drag.start, pointer, step);
        let original = Length::from_micrometres((drag.original_mm * 1000.0).round() as i64);
        if drag.last != Some(value) && (drag.last.is_some() || value != original) {
            drag.last = Some(value);
            interaction.resize = Some(ResizeAction::Preview(request(value)));
        }
    }
    true
}

/// Handles are drawn above the scene: white with an accent ring, filled
/// while hovered or dragged.
pub(super) fn paint(ui: &egui::Ui, tool: &MoveTool, handles: &[ResizeHandle]) {
    let accent = crate::theme_widgets::ACCENT;
    let active = tool
        .resize
        .as_ref()
        .map(|drag| (drag.handle.axis, drag.handle.positive))
        .or(tool.resize_hover);
    for handle in handles {
        let hot = active == Some((handle.axis, handle.positive));
        let painter = ui.painter();
        painter.circle_filled(
            handle.at,
            if hot { 6.5 } else { 5.5 },
            if hot { accent } else { egui::Color32::WHITE },
        );
        painter.circle_stroke(
            handle.at,
            if hot { 6.5 } else { 5.5 },
            egui::Stroke::new(1.5, accent),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::money::Currency;
    use plan_my_cabinet::units::{Pose, Quaternion};

    fn mm(value: i64) -> Length {
        Length::from_micrometres(value * 1000)
    }

    fn panel() -> (Project, Uuid) {
        let id = Uuid::from_u128(7);
        let mut project = Project::new("Resize", Currency::Brl);
        project.boards.push(Board {
            coated_face: Default::default(),
            banding: Default::default(),
            id,
            name: "Panel".into(),
            material_id: Uuid::from_u128(8),
            length: mm(100),
            width: mm(100),
            thickness: mm(20),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        });
        (project, id)
    }

    #[test]
    fn top_view_offers_length_and_width_handles_only_when_allowed() {
        let (project, id) = panel();
        let mut camera = Camera::default();
        camera.set_preset(Preset::Top);
        camera.target = [50.0, 50.0, 10.0];
        camera.distance = 400.0;
        camera.viewport_aspect = 1.0;
        camera.viewport_height = 500.0;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, 500.0));
        let mut selection = Selection::default();
        selection.choose(Some(id), false);
        let mut tool = MoveTool::default();
        assert!(handles(&project, &camera, rect, &selection, &tool).is_empty());
        tool.resize_enabled = true;
        let found = handles(&project, &camera, rect, &selection, &tool);
        assert_eq!(
            found.len(),
            4,
            "length and width faces; thickness stays with the material"
        );
        for axis in 0..2 {
            let plus = found.iter().find(|h| h.axis == axis && h.positive).unwrap();
            let minus = found
                .iter()
                .find(|h| h.axis == axis && !h.positive)
                .unwrap();
            assert!(
                plus.per_mm.dot(plus.at - minus.at) > 0.0,
                "outward points away"
            );
            assert!(
                minus.per_mm.dot(minus.at - plus.at) > 0.0,
                "outward points away"
            );
        }
        tool.mode = ToolMode::Measure;
        assert!(handles(&project, &camera, rect, &selection, &tool).is_empty());
        tool.mode = ToolMode::Navigate;
        selection.choose(Some(Uuid::from_u128(99)), true);
        assert!(handles(&project, &camera, rect, &selection, &tool).is_empty());
    }

    #[test]
    fn dragging_outward_grows_snaps_and_never_collapses() {
        let handle = ResizeHandle {
            axis: 0,
            positive: true,
            at: egui::Pos2::ZERO,
            per_mm: egui::vec2(2.0, 0.0),
        };
        let start = egui::pos2(10.0, 10.0);
        // Only movement along the face normal counts: 20 points = 10 mm.
        assert_eq!(
            proposed_value(&handle, 764.0, start, start + egui::vec2(20.0, 7.0), 1.0),
            mm(774)
        );
        assert_eq!(
            proposed_value(&handle, 764.0, start, start + egui::vec2(20.0, 0.0), 10.0),
            mm(770)
        );
        assert_eq!(
            proposed_value(&handle, 764.0, start, start + egui::vec2(1.0, 0.0), 0.1),
            Length::from_micrometres(764_500)
        );
        assert_eq!(
            proposed_value(&handle, 764.0, start, start - egui::vec2(4000.0, 0.0), 1.0),
            mm(1)
        );
        assert_eq!(handle.anchor(), Anchor::Start);
        let opposite = ResizeHandle {
            positive: false,
            ..handle
        };
        assert_eq!(opposite.anchor(), Anchor::End);
    }

    #[test]
    fn a_press_on_a_handle_resizes_instead_of_orbiting_and_release_accepts() {
        let (project, id) = panel();
        let ctx = egui::Context::default();
        let mut camera = Camera::default();
        camera.set_preset(Preset::Top);
        camera.target = [50.0, 50.0, 10.0];
        camera.distance = 400.0;
        let mut selection = Selection::default();
        selection.choose(Some(id), false);
        let mut tool = MoveTool {
            resize_enabled: true,
            grid_snap: false,
            ..MoveTool::default()
        };
        let mut run = |events: Vec<egui::Event>, camera: &mut Camera, tool: &mut MoveTool| {
            let mut out = (egui::Rect::NOTHING, ViewportInteraction::default());
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(700.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    out = canvas::interact_with_selection(
                        ui,
                        camera,
                        &project,
                        &mut selection,
                        tool,
                        false,
                        false,
                        false,
                    );
                },
            )
            .drop_without_applying_deltas();
            out
        };
        run(vec![], &mut camera, &mut tool);
        let (rect, _) = run(vec![], &mut camera, &mut tool);
        let selection_now = {
            let mut s = Selection::default();
            s.choose(Some(id), false);
            s
        };
        let handle = handles(&project, &camera, rect, &selection_now, &tool)
            .into_iter()
            .find(|h| h.axis == 0 && h.positive)
            .expect("length handle");
        let target = handle.at + handle.per_mm * 30.0;
        let view = (camera.yaw.rem_euclid(std::f64::consts::TAU), camera.pitch);
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(
            vec![egui::Event::PointerMoved(handle.at), press(handle.at, true)],
            &mut camera,
            &mut tool,
        );
        let mut preview = None;
        for step in 1..=6 {
            let at = handle.at + (target - handle.at) * (step as f32 / 6.0);
            let (_, event) = run(vec![egui::Event::PointerMoved(at)], &mut camera, &mut tool);
            if let Some(ResizeAction::Preview(request)) = event.resize {
                preview = Some(request);
            }
        }
        let preview = preview.expect("dragging a handle previews a size");
        assert_eq!(preview.board_id, id);
        assert_eq!(preview.dimension, BoardDimension::Length);
        assert_eq!(preview.value, mm(130));
        let after = (camera.yaw.rem_euclid(std::f64::consts::TAU), camera.pitch);
        assert!(
            (after.0 - view.0).abs() < 1e-9 && (after.1 - view.1).abs() < 1e-9,
            "the drag does not orbit the view: {view:?} -> {after:?}"
        );
        let (_, event) = run(vec![press(target, false)], &mut camera, &mut tool);
        assert_eq!(event.resize, Some(ResizeAction::Accept(preview)));
        assert!(!tool.resizing());
    }
}
