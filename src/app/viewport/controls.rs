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
        .order(egui::Order::Middle)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(canvas.right_top() + egui::vec2(-12.0, 12.0))
        .show(ui.ctx(), |ui| {
            crate::theme_widgets::floating_frame()
                .inner_margin(3)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut pending = None;
                        let mut preset = camera.preset;
                        let labels = [
                            (Preset::Isometric, localizer.text("viewport-iso")),
                            (Preset::Front, localizer.text("viewport-front")),
                            (Preset::Top, localizer.text("viewport-top")),
                        ];
                        let options: Vec<_> =
                            labels.iter().map(|(p, l)| (*p, l.as_str())).collect();
                        ui.add_enabled_ui(!modal && !tool.dragging(), |ui| {
                            if crate::theme_widgets::segmented(ui, &mut preset, &options).clicked()
                                && preset != Preset::Free
                            {
                                pending = Some(
                                    Request::new(A::ViewPreset).argument(Argument::Preset(preset)),
                                );
                            }
                        });
                        // Navigate or Move (feet and other hardware drag here).
                        for (mode, icon, action) in [
                            (
                                ToolMode::Navigate,
                                crate::icons::Icon::Orbit,
                                A::ViewNavigate,
                            ),
                            (ToolMode::Move, crate::icons::Icon::Move, A::ViewMove),
                        ] {
                            let request = Request::new(action);
                            let allowed = actions::viewport_availability(
                                request,
                                modal,
                                preview_active,
                                tool.dragging(),
                                project,
                                selection,
                            );
                            let active = tool.mode == mode;
                            if crate::theme_widgets::ghost_icon_sized(
                                ui,
                                icon,
                                &action.label(&localizer),
                                if active {
                                    crate::theme_widgets::ACCENT
                                } else {
                                    crate::theme_widgets::SECONDARY
                                },
                                16.0,
                                28.0,
                                allowed.is_ok(),
                                active,
                            )
                            .clicked()
                            {
                                pending = Some(request);
                            }
                        }
                        let frame = Request::new(A::ViewFrame);
                        let allowed = actions::viewport_availability(
                            frame,
                            modal,
                            preview_active,
                            tool.dragging(),
                            project,
                            selection,
                        );
                        if crate::theme_widgets::ghost_icon_sized(
                            ui,
                            crate::icons::Icon::Frame,
                            &frame.id.label(&localizer),
                            crate::theme_widgets::SECONDARY,
                            16.0,
                            28.0,
                            allowed.is_ok(),
                            false,
                        )
                        .clicked()
                        {
                            pending = Some(frame);
                        }
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
                    });
                });
        });
    overlay.response.contains_pointer() || egui::Popup::is_any_open(ui.ctx())
}

/// Floating viewport chrome for the Design workspace: a vertical tool strip at
/// the left edge, camera presets/projection at top centre, and the snap chip at
/// top right. Returns whether the pointer is over any overlay, so the scene does
/// not pick or orbit through it.
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
) -> bool {
    use crate::icons::Icon;
    use crate::theme_widgets as tw;
    let localizer = Localizer::new(if pt { Language::PtBr } else { Language::En });
    let canvas = ui.available_rect_before_wrap().intersect(ui.clip_rect());
    let mut pending = None;
    let mut blocked = false;
    let availability = |request: Request, tool: &MoveTool| {
        actions::viewport_availability(
            request,
            modal,
            preview_active,
            tool.dragging(),
            project,
            selection,
        )
    };

    // Tool strip.
    let strip = egui::Area::new(ui.id().with("viewport-tool-strip"))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::LEFT_CENTER)
        .fixed_pos(canvas.left_center() + egui::vec2(14.0, 0.0))
        .show(ui.ctx(), |ui| {
            tw::floating_frame().show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.vertical(|ui| {
                    for (request, selected, icon) in [
                        (
                            Request::new(A::ViewNavigate),
                            tool.mode == ToolMode::Navigate,
                            Icon::Orbit,
                        ),
                        (
                            Request::new(A::ViewMove),
                            tool.mode == ToolMode::Move,
                            Icon::Move,
                        ),
                        (
                            Request::new(A::ViewMeasure),
                            tool.mode == ToolMode::Measure,
                            Icon::Measure,
                        ),
                        (
                            Request::new(A::ViewBand),
                            tool.mode == ToolMode::Band,
                            Icon::Band,
                        ),
                    ] {
                        let allowed = availability(request, tool);
                        let label = request.id.label(&localizer);
                        let response = tw::ghost_icon_sized(
                            ui,
                            icon,
                            &label,
                            tw::SECONDARY,
                            17.0,
                            34.0,
                            allowed.is_ok(),
                            selected,
                        );
                        if let Err(reason) = allowed {
                            response.on_hover_text(reason.reason(localizer.language()));
                        } else if response.clicked() {
                            pending = Some(request);
                        }
                    }
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(34.0, 9.0), egui::Sense::hover());
                    ui.painter().hline(
                        rect.x_range().shrink(6.0),
                        rect.center().y,
                        egui::Stroke::new(1.0, tw::BORDER_SOFT),
                    );
                    let frame = Request::new(A::ViewFrame);
                    let allowed = availability(frame, tool);
                    let response = tw::ghost_icon_sized(
                        ui,
                        Icon::Frame,
                        &frame.id.label(&localizer),
                        tw::SECONDARY,
                        17.0,
                        34.0,
                        allowed.is_ok(),
                        false,
                    );
                    if response.clicked() {
                        pending = Some(frame);
                    }
                });
            });
        });
    blocked |= strip.response.contains_pointer();

    // Camera presets and projection.
    if canvas.width() >= 520.0 {
        let camera_bar = egui::Area::new(ui.id().with("viewport-camera-bar"))
            .order(egui::Order::Middle)
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(canvas.center_top() + egui::vec2(0.0, 12.0))
            .show(ui.ctx(), |ui| {
                ui.add_enabled_ui(!modal && !tool.dragging(), |ui| {
                    tw::floating_frame().inner_margin(3).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut preset = camera.preset;
                            let labels = [
                                (Preset::Isometric, localizer.text("viewport-iso")),
                                (Preset::Front, localizer.text("viewport-front")),
                                (Preset::Right, localizer.text("viewport-right")),
                                (Preset::Top, localizer.text("viewport-top")),
                            ];
                            let options: Vec<_> =
                                labels.iter().map(|(p, l)| (*p, l.as_str())).collect();
                            if tw::segmented(ui, &mut preset, &options).clicked()
                                && preset != Preset::Free
                            {
                                pending = Some(
                                    Request::new(A::ViewPreset).argument(Argument::Preset(preset)),
                                );
                            }
                            let mut projection = camera.projection;
                            let labels = [
                                (
                                    Projection::Perspective,
                                    localizer.text("viewport-persp-short"),
                                ),
                                (
                                    Projection::Orthographic,
                                    localizer.text("viewport-ortho-short"),
                                ),
                            ];
                            let options: Vec<_> =
                                labels.iter().map(|(p, l)| (*p, l.as_str())).collect();
                            if tw::segmented(ui, &mut projection, &options).clicked()
                                && projection != camera.projection
                            {
                                pending = Some(
                                    Request::new(A::ViewProjection)
                                        .argument(Argument::Projection(projection)),
                                );
                            }
                        });
                    });
                });
            });
        blocked |= camera_bar.response.contains_pointer();
    } else {
        let camera_menu = egui::Area::new(ui.id().with("viewport-camera-menu"))
            .order(egui::Order::Middle)
            .pivot(egui::Align2::LEFT_TOP)
            .fixed_pos(canvas.left_top() + egui::vec2(12.0, 12.0))
            .show(ui.ctx(), |ui| {
                ui.add_enabled_ui(!modal && !tool.dragging(), |ui| {
                    let response = tw::floating_frame()
                        .inner_margin(2)
                        .show(ui, |ui| {
                            tw::ghost_icon_sized(
                                ui,
                                Icon::Cube,
                                &localizer.text("viewport-more"),
                                tw::SECONDARY,
                                16.0,
                                30.0,
                                true,
                                false,
                            )
                        })
                        .inner;
                    egui::Popup::menu(&response).show(|ui| {
                        presets(ui, &localizer, camera.preset, &mut |r| pending = Some(r));
                        ui.separator();
                        projections(ui, &localizer, camera.projection, &mut |r| {
                            pending = Some(r)
                        });
                    });
                });
            });
        blocked |= camera_menu.response.contains_pointer();
    }

    // Snap chip.
    let locale = if pt { Locale::PtBr } else { Locale::En };
    let spacing = format_length(project.grid_spacing, Unit::Mm, locale, 0);
    let short = match (tool.face_snap, tool.grid_snap) {
        (true, true) => "viewport-snap-short-both",
        (true, false) => "viewport-snap-short-face",
        (false, true) => "viewport-snap-short-grid",
        (false, false) => "viewport-snap-short-off",
    };
    let snap = egui::Area::new(ui.id().with("viewport-snap-chip"))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(canvas.right_top() + egui::vec2(-12.0, 12.0))
        .show(ui.ctx(), |ui| {
            ui.add_enabled_ui(!modal, |ui| {
                let label = if tool.grid_snap {
                    format!("{} {spacing}", localizer.text(short))
                } else {
                    localizer.text(short)
                };
                let response = ui
                    .add(
                        egui::Button::image_and_text(
                            crate::icons::icon(Icon::Magnet, tw::ACCENT, 15.0),
                            egui::RichText::new(label).size(12.5).color(tw::TEXT),
                        )
                        .fill(tw::PANEL)
                        .stroke(egui::Stroke::new(1.0, tw::BORDER_SOFT))
                        .corner_radius(9)
                        .min_size(egui::vec2(0.0, 32.0)),
                    )
                    .on_hover_text(localizer.text(match (tool.face_snap, tool.grid_snap) {
                        (true, true) => "viewport-snap-both",
                        (true, false) => "viewport-snap-face-only",
                        (false, true) => "viewport-snap-grid-only",
                        (false, false) => "viewport-snap-off",
                    }));
                egui::Popup::menu(&response)
                    .align(egui::RectAlign::BOTTOM_END)
                    .show(|ui| {
                        ui.set_min_width(230.0);
                        ui.add_enabled_ui(!tool.dragging() && !preview_active, |ui| {
                            ui.checkbox(&mut tool.face_snap, localizer.text("viewport-snap-face"));
                            ui.checkbox(&mut tool.grid_snap, localizer.text("viewport-snap-grid"));
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(localizer.text("viewport-snap-spacing"))
                                    .color(tw::MUTED),
                            );
                            ui.label(tw::mono(spacing.clone(), 12.5));
                        });
                        let display_mm = scene_render::grid_display_interval(project, camera);
                        let configured_mm = project.grid_spacing.micrometres() as f64 / 1000.0;
                        if display_mm > configured_mm * (1.0 + 1e-9) {
                            let display = format_length(
                                Length::from_micrometres((display_mm * 1000.0).round() as i64),
                                Unit::Mm,
                                locale,
                                0,
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}: {display}",
                                    localizer.text("viewport-snap-display")
                                ))
                                .size(11.5)
                                .color(tw::FAINT),
                            );
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
                        ui.label(
                            egui::RichText::new(localizer.text("viewport-snap-alt"))
                                .size(11.5)
                                .color(tw::FAINT),
                        );
                    });
            });
        });
    blocked |= snap.response.contains_pointer() || egui::Popup::is_any_open(ui.ctx());

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
    blocked
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
