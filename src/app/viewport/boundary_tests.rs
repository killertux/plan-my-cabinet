//! Exercises the binary viewport's private canvas and projection seams.
use super::*;
use plan_my_cabinet::domain::Board;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::units::{Length, Pose, Quaternion};

fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[allow(clippy::too_many_arguments)] // The fixture passes its independent viewport state into each egui frame.
fn frame(
    ctx: &egui::Context,
    camera: &mut Camera,
    project: &Project,
    selection: &mut Selection,
    tool: &mut MoveTool,
    origin: egui::Pos2,
    extent: egui::Vec2,
    events: Vec<egui::Event>,
) -> (egui::Rect, bool, ViewportInteraction) {
    let mut canvas_rect = egui::Rect::NOTHING;
    let mut overlay_clicked = false;
    let mut action = ViewportInteraction::default();
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            events,
            ..Default::default()
        },
        |_ui| {
            egui::Area::new(egui::Id::new("test-canvas"))
                .fixed_pos(origin)
                .show(ctx, |ui| {
                    ui.set_min_size(extent);
                    ui.set_max_size(extent);
                    controls::show(ui, camera, project, selection, tool, false, false, false);
                    let (rect, result) = canvas::interact_with_selection(
                        ui, camera, project, selection, tool, false, false, false,
                    );
                    canvas_rect = rect;
                    action = result;
                });
            let board_center = camera.project([50.0, 50.0, 10.0], canvas_rect).unwrap();
            egui::Area::new(egui::Id::new("test-overlay"))
                .order(egui::Order::Foreground)
                .fixed_pos(board_center - egui::vec2(35.0, 15.0))
                .show(ctx, |ui| {
                    overlay_clicked = ui
                        .add_sized([70.0, 30.0], egui::Button::new("Overlay"))
                        .clicked();
                });
        },
    )
    .drop_without_applying_deltas();
    (canvas_rect, overlay_clicked, action)
}

#[test]
fn moving_and_resizing_canvas_keeps_projection_pick_drag_and_overlay_capture_aligned() {
    let ctx = egui::Context::default();
    let id = Uuid::from_u128(42);
    let mut project = Project::new("Viewport", Currency::Brl);
    project.boards.push(Board {
        id,
        name: "Panel".into(),
        material_id: Uuid::from_u128(43),
        length: Length::from_micrometres(100_000),
        width: Length::from_micrometres(100_000),
        thickness: Length::from_micrometres(20_000),
        grain_override: None,
        parent_id: None,
        pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
    });
    let mut camera = Camera::default();
    camera.set_preset(Preset::Top);
    camera.target = [50.0, 50.0, 10.0];
    camera.distance = 400.0;
    let mut selection = Selection::default();
    let mut tool = MoveTool::default();

    macro_rules! draw {
        ($origin:expr, $extent:expr, $events:expr $(,)?) => {
            frame(
                &ctx,
                &mut camera,
                &project,
                &mut selection,
                &mut tool,
                $origin,
                $extent,
                $events,
            )
        };
    }

    for (origin, extent) in [
        (egui::pos2(20.0, 20.0), egui::vec2(650.0, 480.0)),
        (egui::pos2(130.0, 65.0), egui::vec2(450.0, 390.0)),
    ] {
        draw!(origin, extent, vec![]);
        let (rect, _, _) = draw!(origin, extent, vec![]); // settle hit testing after reflow
        assert!(rect.min.x >= origin.x && rect.min.y >= origin.y);
        assert!(rect.width() <= extent.x && rect.height() <= extent.y);
        let center = camera.project([50.0, 50.0, 10.0], rect).unwrap();
        assert_eq!(
            pick_visible(&project, &camera, center, rect, &selection),
            Some(id)
        );

        draw!(
            origin,
            extent,
            vec![egui::Event::PointerMoved(center), pointer(center, true)],
        );
        let (_, clicked, action) = draw!(origin, extent, vec![pointer(center, false)]);
        assert!(clicked, "overlay receives the click after canvas reflow");
        assert!(action.drag.is_none() && action.selection.is_none());
        assert!(selection.ids.is_empty(), "covered canvas cannot select");
        assert!(tool.drag.is_none(), "covered canvas cannot start a drag");

        // Outside the overlay, a projected board point is still pickable. A
        // drag on the camera-facing plane must project to the pointer delta.
        let exposed = center + egui::vec2(50.0, 0.0);
        assert_eq!(
            pick_visible(&project, &camera, exposed, rect, &selection),
            Some(id)
        );
        draw!(
            origin,
            extent,
            vec![egui::Event::PointerMoved(exposed), pointer(exposed, true)],
        );
        let (_, _, event) = draw!(origin, extent, vec![pointer(exposed, false)]);
        let proposal = event
            .selection
            .expect("exposed board click is a proposed selection");
        selection.choose(proposal.picked, proposal.additive);
        assert_eq!(selection.active, Some(id));
        tool.mode = ToolMode::Move;
        draw!(origin, extent, vec![]);
        let (rect, _, _) = draw!(origin, extent, vec![]);
        let center = camera.project([50.0, 50.0, 10.0], rect).unwrap();
        draw!(origin, extent, vec![pointer(center, true)]);
        let (_, clicked, action) = draw!(origin, extent, vec![pointer(center, false)]);
        assert!(clicked);
        assert!(action.drag.is_none() && action.selection.is_none());
        assert!(tool.drag.is_none(), "overlay captures before the move tool");
        let exposed = center + egui::vec2(50.0, 0.0);
        let moved = exposed + egui::vec2(24.0, -17.0);
        let start = project.boards[0].pose;
        let free = camera.drag_pose(exposed, moved, rect, start).unwrap();
        let before = camera.project(start.translation_mm, rect).unwrap();
        let after = camera.project(free.translation_mm, rect).unwrap();
        assert!(
            (after - before - (moved - exposed)).length() < 0.3,
            "rect {rect:?}, projected {:?}, pointer {:?}",
            after - before,
            moved - exposed
        );
        draw!(origin, extent, vec![pointer(exposed, true)]);
        let (_, _, preview) = draw!(origin, extent, vec![egui::Event::PointerMoved(moved)]);
        let Some(DragAction::Preview(board, preview)) = preview.drag else {
            panic!("drag starts at the projected pick after moving the canvas");
        };
        assert_eq!(board, id);
        assert_eq!(
            preview,
            drag_target_visible(&project, &camera, rect, id, free, start, false, &selection).0
        );
        draw!(origin, extent, vec![pointer(moved, false)]);
        tool.mode = ToolMode::Navigate;
        selection.choose(None, false);
    }
}
