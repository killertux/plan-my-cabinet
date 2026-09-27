//! Canvas allocation and pointer/keyboard interaction. Camera projection, rays and drag math live in the parent.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn interact_with_selection(
    ui: &mut egui::Ui,
    camera: &mut Camera,
    project: &Project,
    selection: &mut Selection,
    tool: &mut MoveTool,
    modal: bool,
    inverse_scroll_zoom: bool,
    preview_active: bool,
) -> (egui::Rect, ViewportInteraction) {
    let mut interaction = ViewportInteraction::default();
    let mut action = None;
    let available = ui
        .available_rect_before_wrap()
        .intersect(ui.clip_rect())
        .intersect(ui.ctx().content_rect());
    let size = egui::vec2(available.width().max(1.0), available.height().max(1.0));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    camera.viewport_aspect = (rect.width() as f64 / rect.height().max(1.0) as f64).max(0.01);
    camera.viewport_height = rect.height() as f64;
    if camera.pending_frame {
        if let Some(bounds) = bounds_visible(project, &selection.ids, selection) {
            camera.frame(bounds, camera.viewport_aspect);
        }
        camera.pending_frame = false;
    }
    if modal {
        action = tool.cancel();
    } else {
        if tool.drag.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = tool.cancel();
        }
        if action.is_none()
            && tool.mode == ToolMode::Move
            && !preview_active
            && response.drag_started_by(egui::PointerButton::Primary)
            && !ui.input(|i| i.modifiers.shift || i.modifiers.command)
            && !ui.ctx().text_edit_focused()
            && let Some(start) = ui.input(|i| i.pointer.press_origin())
            && let Some(id) = selection.active
            && pick_visible(project, camera, start, rect, selection) == Some(id)
            && let Ok(world) = plan_my_cabinet::placement::world_pose(project, id)
        {
            tool.drag = Some(MoveDrag {
                board_id: id,
                start,
                world,
                selection_ids: selection.ids.clone(),
                selection_active: selection.active,
                snap: None,
                last_pose: None,
            });
        }
        if action.is_none()
            && let Some(drag) = &mut tool.drag
        {
            if response.drag_stopped_by(egui::PointerButton::Primary) {
                let final_pose = response
                    .interact_pointer_pos()
                    .and_then(|pointer| camera.drag_pose(drag.start, pointer, rect, drag.world))
                    .map(|free| {
                        drag_target_with_modes_visible(
                            project,
                            camera,
                            rect,
                            drag.board_id,
                            free,
                            drag.world,
                            ui.input(|i| i.modifiers.alt),
                            tool.face_snap,
                            tool.grid_snap,
                            selection,
                        )
                        .0
                    })
                    .or(drag.last_pose);
                action = Some(DragAction::Accept(drag.board_id, final_pose));
                tool.drag = None;
            } else if let Some(pointer) = response.interact_pointer_pos()
                && let Some(free) = camera.drag_pose(drag.start, pointer, rect, drag.world)
            {
                let (pose, snap) = drag_target_with_modes_visible(
                    project,
                    camera,
                    rect,
                    drag.board_id,
                    free,
                    drag.world,
                    ui.input(|i| i.modifiers.alt),
                    tool.face_snap,
                    tool.grid_snap,
                    selection,
                );
                drag.snap = snap;
                drag.last_pose = Some(pose);
                action = Some(DragAction::Preview(drag.board_id, pose));
            }
        }
        if tool.drag.is_none()
            && !ui.input(|i| i.pointer.any_down())
            && let Some(pointer) = response.hover_pos()
        {
            let picked = pick_visible(project, camera, pointer, rect, selection);
            super::set_hover(ui.ctx(), picked);
            if let Some(id) = picked {
                ui.ctx().set_cursor_icon(
                    if tool.mode == ToolMode::Move && selection.active == Some(id) {
                        egui::CursorIcon::Grab
                    } else {
                        egui::CursorIcon::PointingHand
                    },
                );
            }
        }
        if !preview_active
            && response.clicked_by(egui::PointerButton::Primary)
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            interaction.selection = Some(SelectionProposal {
                picked: pick_visible(project, camera, pointer, rect, selection),
                additive,
            });
        }
        if response.clicked() || response.drag_started() {
            response.request_focus();
            // egui only accepts a focus lock after the widget was focused in a
            // previous pass. Schedule that pass even if there is no other input.
            ui.ctx().request_repaint();
        }
        let delta = ui.input(|i| i.pointer.delta());
        if response.dragged_by(egui::PointerButton::Secondary)
            || (response.dragged_by(egui::PointerButton::Primary)
                && ui.input(|i| i.modifiers.shift))
        {
            camera.pan(delta.x as f64, delta.y as f64, rect.height() as f64);
        } else if response.dragged_by(egui::PointerButton::Primary)
            && (tool.mode != ToolMode::Move || preview_active)
        {
            camera.orbit(delta.x as f64, delta.y as f64);
        }
        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let direction = if inverse_scroll_zoom { -1.0 } else { 1.0 };
            camera.zoom((scroll as f64 * 0.002 * direction).exp() * pinch as f64);
        }
        if response.has_focus() && !egui::Popup::is_any_open(ui.ctx()) {
            if ui.input(|i| i.key_pressed(egui::Key::F) && !i.modifiers.any())
                && !ui.ctx().text_edit_focused()
            {
                let _ = crate::actions::viewport_control(
                    crate::actions::Request::new(crate::actions::ActionId::ViewFrame),
                    camera,
                    tool,
                    project,
                    selection,
                    false,
                    preview_active,
                );
            }
            let plain_arrow_pressed = ui.input(|i| {
                !i.modifiers.any()
                    && [
                        egui::Key::ArrowLeft,
                        egui::Key::ArrowRight,
                        egui::Key::ArrowUp,
                        egui::Key::ArrowDown,
                    ]
                    .into_iter()
                    .any(|key| i.key_pressed(key))
            });
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        tab: false,
                        ..Default::default()
                    },
                );
                // On the first pass after focus is requested the filter is not
                // yet active at begin_pass. Prevent its arrow direction from
                // moving focus at end_pass, even if the user typed immediately.
                if plain_arrow_pressed {
                    memory.move_focus(egui::FocusDirection::None);
                }
            });
            ui.input(|i| {
                let step = if i.modifiers.shift { 28.0 } else { 20.0 };
                let x = (i.key_pressed(egui::Key::ArrowRight) as i32
                    - i.key_pressed(egui::Key::ArrowLeft) as i32) as f64
                    * step;
                let y = (i.key_pressed(egui::Key::ArrowDown) as i32
                    - i.key_pressed(egui::Key::ArrowUp) as i32) as f64
                    * step;
                if i.modifiers.shift {
                    camera.pan(x, y, rect.height() as f64);
                } else {
                    camera.orbit(x, y);
                }
                if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                    camera.zoom(1.2);
                }
                if i.key_pressed(egui::Key::Minus) {
                    camera.zoom(1.0 / 1.2);
                }
            });
        }
    }
    interaction.drag = action;
    (rect, interaction)
}

// Existing viewport unit fixtures exercise drag and overlay capture through this
// private seam. Production callers use the full interaction result above.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn interact(
    ui: &mut egui::Ui,
    camera: &mut Camera,
    project: &Project,
    selection: &mut Selection,
    tool: &mut MoveTool,
    modal: bool,
    inverse_scroll_zoom: bool,
    preview_active: bool,
) -> (egui::Rect, Option<DragAction>) {
    let (rect, result) = interact_with_selection(
        ui,
        camera,
        project,
        selection,
        tool,
        modal,
        inverse_scroll_zoom,
        preview_active,
    );
    (rect, result.drag)
}

#[cfg(test)]
mod preference_tests {
    use super::*;

    #[test]
    fn inverse_scroll_preference_reverses_real_hovered_canvas_zoom_without_editing_project() {
        let project = plan_my_cabinet::reference_fixture::project();
        let original = project.clone();
        let starting = Camera::reference_baseline();
        let pointer = egui::pos2(150.0, 120.0);
        let distance_after_scroll = |inverse| {
            let ctx = egui::Context::default();
            let mut camera = Camera::reference_baseline();
            let mut selection = Selection::default();
            let mut tool = MoveTool::default();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360.0, 260.0),
                )),
                events: vec![
                    egui::Event::PointerMoved(pointer),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, 100.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                interact_with_selection(
                    ui,
                    &mut camera,
                    &project,
                    &mut selection,
                    &mut tool,
                    false,
                    inverse,
                    false,
                );
            });
            output.drop_without_applying_deltas();
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(360.0, 260.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    interact_with_selection(
                        ui,
                        &mut camera,
                        &project,
                        &mut selection,
                        &mut tool,
                        false,
                        inverse,
                        false,
                    );
                },
            );
            output.drop_without_applying_deltas();
            camera.distance
        };
        assert!(distance_after_scroll(false) < starting.distance);
        assert!(distance_after_scroll(true) > starting.distance);
        assert_eq!(project, original);
    }
}
