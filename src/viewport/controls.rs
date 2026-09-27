//! Localized camera/tool controls. Every choice passes the shared action guard.
use super::*;
use crate::actions::{self, ActionId as A, Argument, Request};
use plan_my_cabinet::dimension_input::{Locale, format_length};
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::units::{Length, Unit};

/// Hardware keeps the full-height scene as the primary surface. Less common
/// camera/tools/snap controls remain in the same guarded menu, not a second
/// three-row toolbar taking space from the cabinet and motion HUD.
#[allow(clippy::too_many_arguments)]
pub(super) fn hardware_overlay(
    ui: &mut egui::Ui,
    camera: &mut Camera,
    project: &Project,
    selection: &Selection,
    tool: &mut MoveTool,
    modal: bool,
    pt: bool,
    preview_active: bool,
) -> bool {
    let localizer = Localizer::new(if pt { Language::PtBr } else { Language::En });
    let canvas = ui.available_rect_before_wrap().intersect(ui.clip_rect());
    let overlay = egui::Area::new(ui.id().with("hardware-camera-controls"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(canvas.right_top() + egui::vec2(-10.0, 10.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(crate::theme_widgets::PANEL)
                .corner_radius(6)
                .inner_margin(4)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if canvas.width() >= 400.0 {
                            for preset in [Preset::Isometric, Preset::Front, Preset::Top] {
                                let request =
                                    Request::new(A::ViewPreset).argument(Argument::Preset(preset));
                                let allowed = actions::viewport_availability(
                                    request,
                                    modal,
                                    preview_active,
                                    tool.dragging(),
                                    project,
                                    selection,
                                );
                                if ui
                                    .add_enabled(
                                        allowed.is_ok(),
                                        egui::Button::selectable(
                                            camera.preset == preset,
                                            localizer.text(preset_key(preset)),
                                        ),
                                    )
                                    .clicked()
                                {
                                    let _ = actions::viewport_control(
                                        request,
                                        camera,
                                        tool,
                                        project,
                                        selection,
                                        modal,
                                        preview_active,
                                    );
                                }
                            }
                        }
                        ui.menu_button(localizer.text("viewport-more"), |ui| {
                            show(
                                ui,
                                camera,
                                project,
                                selection,
                                tool,
                                modal,
                                pt,
                                preview_active,
                            );
                        });
                    });
                });
        });
    overlay.response.contains_pointer() || egui::Popup::is_any_open(ui.ctx())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn show(
    ui: &mut egui::Ui,
    camera: &mut Camera,
    project: &Project,
    selection: &Selection,
    tool: &mut MoveTool,
    modal: bool,
    pt: bool,
    preview_active: bool,
) {
    let localizer = Localizer::new(if pt { Language::PtBr } else { Language::En });
    let mut pending = None;
    let mut invoke = |request| pending = Some(request);
    let navigate = Request::new(A::ViewNavigate);
    let move_board = Request::new(A::ViewMove);
    let measure = Request::new(A::ViewMeasure);
    let frame = Request::new(A::ViewFrame);
    // Include the longer pt-BR projection/preset labels and egui button
    // padding before attempting the non-wrapping full toolbar. The 808 pt
    // reference canvas retains the complete controls; narrow canvases use
    // the same actions in More rather than clipping the final menu.
    let compact = ui.available_width() < 880.0;
    let tiny = ui.available_width() < 290.0;
    ui.add_enabled_ui(!modal, |ui| {
        ui.horizontal(|ui| {
            if !tiny {
                for (request, selected) in [
                    (navigate, tool.mode == ToolMode::Navigate),
                    (move_board, tool.mode == ToolMode::Move),
                    (measure, tool.mode == ToolMode::Measure),
                ] {
                    let enabled = actions::viewport_availability(
                        request,
                        modal,
                        preview_active,
                        tool.dragging(),
                        project,
                        selection,
                    );
                    let response = ui.add_enabled(
                        enabled.is_ok(),
                        egui::Button::selectable(selected, request.id.label(&localizer)),
                    );
                    let response = if let Err(reason) = enabled {
                        response.on_disabled_hover_text(reason.reason(localizer.language()))
                    } else {
                        response
                    };
                    if response.clicked() {
                        invoke(request);
                    }
                }
            }
            if compact {
                ui.menu_button(localizer.text("viewport-more"), |ui| {
                    if tiny {
                        for (request, selected) in [
                            (navigate, tool.mode == ToolMode::Navigate),
                            (move_board, tool.mode == ToolMode::Move),
                            (measure, tool.mode == ToolMode::Measure),
                        ] {
                            let availability = actions::viewport_availability(
                                request,
                                modal,
                                preview_active,
                                tool.dragging(),
                                project,
                                selection,
                            );
                            if ui
                                .add_enabled(
                                    availability.is_ok(),
                                    egui::Button::selectable(
                                        selected,
                                        request.id.label(&localizer),
                                    ),
                                )
                                .clicked()
                            {
                                invoke(request);
                                ui.close();
                            }
                        }
                    }
                    menu_contents(
                        ui,
                        &localizer,
                        camera,
                        tool,
                        project,
                        selection,
                        modal,
                        preview_active,
                        &mut invoke,
                    );
                });
            } else {
                let availability = actions::viewport_availability(
                    frame,
                    modal,
                    preview_active,
                    tool.dragging(),
                    project,
                    selection,
                );
                let response = ui.add_enabled(
                    availability.is_ok(),
                    egui::Button::new(frame.id.label(&localizer)),
                );
                if response.clicked() {
                    invoke(frame);
                }
                if let Err(reason) = availability {
                    response.on_disabled_hover_text(reason.reason(localizer.language()));
                }
                ui.add_enabled_ui(!tool.dragging(), |ui| {
                    ui.menu_button(
                        format!("{} ▾", localizer.text(preset_key(camera.preset))),
                        |ui| {
                            presets(ui, &localizer, camera.preset, &mut invoke);
                        },
                    )
                    .response
                    .on_hover_text(localizer.text("viewport-preset"));
                    ui.menu_button(
                        format!("{} ▾", localizer.text(projection_key(camera.projection))),
                        |ui| {
                            projections(ui, &localizer, camera.projection, &mut invoke);
                        },
                    )
                    .response
                    .on_hover_text(localizer.text("viewport-projection"));
                });
            }
        })
    });
    let locale = if pt { Locale::PtBr } else { Locale::En };
    let spacing = format_length(project.grid_spacing, Unit::Mm, locale, 3);
    let status = match (tool.face_snap, tool.grid_snap) {
        (true, true) => "viewport-snap-both",
        (true, false) => "viewport-snap-face-only",
        (false, true) => "viewport-snap-grid-only",
        (false, false) => "viewport-snap-off",
    };
    let summary = if ui.available_width() < 290.0 {
        let short = match (tool.face_snap, tool.grid_snap) {
            (true, true) => "viewport-snap-short-both",
            (true, false) => "viewport-snap-short-face",
            (false, true) => "viewport-snap-short-grid",
            (false, false) => "viewport-snap-short-off",
        };
        format!("{} · {spacing}", localizer.text(short))
    } else {
        format!("{} · {spacing}", localizer.text(status))
    };
    ui.add_enabled_ui(!modal, |ui| {
        ui.menu_button(format!("{summary} ▾"), |ui| {
            ui.set_min_width(210.0);
            ui.add_enabled_ui(!tool.dragging() && !preview_active, |ui| {
                ui.checkbox(&mut tool.face_snap, localizer.text("viewport-snap-face"));
                ui.checkbox(&mut tool.grid_snap, localizer.text("viewport-snap-grid"));
            });
            ui.label(format!(
                "{}: {spacing}",
                localizer.text("viewport-snap-spacing")
            ));
            let display_mm = scene_render::grid_display_interval(project, camera);
            let configured_mm = project.grid_spacing.micrometres() as f64 / 1000.0;
            if display_mm > configured_mm * (1.0 + 1e-9) {
                let display = format_length(
                    Length::from_micrometres((display_mm * 1000.0).round() as i64),
                    Unit::Mm,
                    locale,
                    3,
                );
                ui.small(format!(
                    "{}: {display}",
                    localizer.text("viewport-snap-display")
                ));
            }
            if ui
                .add_enabled(
                    !tool.dragging() && !preview_active,
                    egui::Button::new(localizer.text("grid-edit")),
                )
                .clicked()
            {
                tool.grid_edit_requested = true;
                ui.close();
            }
            ui.small(localizer.text("viewport-snap-alt"));
        })
        .response
        .on_hover_text(format!("{} · {spacing}", localizer.text(status)));
    });
    if let Some(request) = pending {
        let _ = actions::viewport_control(
            request,
            camera,
            tool,
            project,
            selection,
            modal,
            preview_active,
        );
    }
    let hint = localizer.text(match tool.mode {
        ToolMode::Move if !preview_active => "viewport-move-hint",
        ToolMode::Measure => "viewport-measure-hint",
        _ => "viewport-navigate-hint",
    });
    // This is guidance, not a control: keep it within the canvas at compact
    // widths and expose the complete wording on hover instead of painting it
    // across the inspector or beyond the window edge.
    ui.add(egui::Label::new(egui::RichText::new(&hint).small()).truncate())
        .on_hover_text(hint);
}

fn preset_key(preset: Preset) -> &'static str {
    match preset {
        Preset::Isometric => "viewport-iso",
        Preset::Front => "viewport-front",
        Preset::Right => "viewport-right",
        Preset::Top => "viewport-top",
        Preset::Free => "viewport-free",
    }
}

fn projection_key(projection: Projection) -> &'static str {
    match projection {
        Projection::Perspective => "viewport-perspective",
        Projection::Orthographic => "viewport-orthographic",
    }
}

#[allow(clippy::too_many_arguments)]
fn menu_contents(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    camera: &Camera,
    tool: &MoveTool,
    project: &Project,
    selection: &Selection,
    modal: bool,
    preview_active: bool,
    invoke: &mut impl FnMut(Request),
) {
    let frame = Request::new(A::ViewFrame);
    let availability = actions::viewport_availability(
        frame,
        modal,
        preview_active,
        tool.dragging(),
        project,
        selection,
    );
    let response = ui.add_enabled(
        availability.is_ok(),
        egui::Button::new(frame.id.label(localizer)),
    );
    if response.clicked() {
        invoke(frame);
        ui.close();
    }
    if let Err(reason) = availability {
        response.on_disabled_hover_text(reason.reason(localizer.language()));
    }
    ui.separator();
    ui.add_enabled_ui(!tool.dragging(), |ui| {
        ui.label(format!(
            "{}: {}",
            localizer.text("viewport-preset"),
            localizer.text(preset_key(camera.preset))
        ));
        presets(ui, localizer, camera.preset, invoke);
        ui.separator();
        ui.label(format!(
            "{}: {}",
            localizer.text("viewport-projection"),
            localizer.text(projection_key(camera.projection))
        ));
        projections(ui, localizer, camera.projection, invoke);
    });
}

fn presets(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    current: Preset,
    invoke: &mut impl FnMut(Request),
) {
    for (preset, key) in [
        (Preset::Isometric, "viewport-iso"),
        (Preset::Front, "viewport-front"),
        (Preset::Right, "viewport-right"),
        (Preset::Top, "viewport-top"),
    ] {
        if ui
            .selectable_label(current == preset, localizer.text(key))
            .clicked()
        {
            invoke(Request::new(A::ViewPreset).argument(Argument::Preset(preset)));
            ui.close();
        }
    }
}

fn projections(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    current: Projection,
    invoke: &mut impl FnMut(Request),
) {
    for (projection, key) in [
        (Projection::Perspective, "viewport-perspective"),
        (Projection::Orthographic, "viewport-orthographic"),
    ] {
        if ui
            .selectable_label(current == projection, localizer.text(key))
            .clicked()
        {
            invoke(Request::new(A::ViewProjection).argument(Argument::Projection(projection)));
            ui.close();
        }
    }
}
