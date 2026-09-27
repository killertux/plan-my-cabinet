//! Explicit relationship drafts and dependency-aware object removal.
use crate::*;
use crate::actions::{ActionId as A, Argument, Request, Target};
use crate::icons::Icon;
use crate::theme_widgets as tw;
use plan_my_cabinet::door_joint::{self, JointPreview};

pub(crate) struct DoorDialog {
    id: Uuid,
    editing: bool,
    project_id: Uuid,
    revision: u64,
    root: Option<Uuid>,
    mount: Option<Uuid>,
    hinges: Vec<Uuid>,
    error: bool,
    chrome: ModalChrome,
}

impl DoorDialog {
    pub(crate) fn new(app: &DesktopApp, id: Option<Uuid>) -> Self {
        let p = app.editor.project();
        let old = id.and_then(|id| p.door_joints.iter().find(|j| j.id == id));
        Self {
            id: id.unwrap_or_else(Uuid::new_v4),
            editing: id.is_some(),
            project_id: p.id,
            revision: p.revision,
            root: old.map(|j| j.moving_root_id).or_else(|| {
                app.selection.active.filter(|id| {
                    p.boards.iter().any(|b| b.id == *id) || p.assemblies.iter().any(|a| a.id == *id)
                })
            }),
            mount: old.map(|j| j.mounting_board_id),
            hinges: old.map_or_else(Vec::new, |j| j.hinge_installation_ids.clone()),
            error: false,
            chrome: ModalChrome::new(egui::Id::new("door-joint-dialog")).width(540.0),
        }
    }

    fn proposal(&self, app: &DesktopApp) -> Result<JointPreview, door_joint::JointError> {
        if self.project_id != app.editor.project().id
            || self.revision != app.editor.project().revision
        {
            return Err(door_joint::JointError::StalePreview);
        }
        door_joint::preview(
            app.editor.project(),
            self.id,
            self.root.ok_or(door_joint::JointError::MissingRoot)?,
            self.mount.ok_or(door_joint::JointError::MissingMount)?,
            self.hinges.clone(),
        )
    }
}

pub(crate) enum DoorRemoval {
    Joint(Uuid),
    Object(Uuid),
    Hardware(Uuid),
}

pub(crate) struct RemovalDialog {
    pub target: DoorRemoval,
    project_id: Uuid,
    revision: u64,
    error: bool,
    chrome: ModalChrome,
}

impl RemovalDialog {
    pub(crate) fn new(app: &DesktopApp, target: DoorRemoval) -> Self {
        Self {
            target,
            project_id: app.editor.project().id,
            revision: app.editor.project().revision,
            error: false,
            chrome: ModalChrome::new(egui::Id::new("door-removal-dialog")),
        }
    }
}

fn object_name(project: &Project, id: Uuid) -> &str {
    project
        .boards
        .iter()
        .find(|b| b.id == id)
        .map(|b| b.name.as_str())
        .or_else(|| {
            project
                .assemblies
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.name.as_str())
        })
        .or_else(|| {
            project
                .hardware
                .iter()
                .find(|h| h.id == id)
                .map(|h| h.name.as_str())
        })
        .unwrap_or("?")
}

impl DesktopApp {
    /// Only the active (or inspected) relationship belongs on the canvas.
    /// All other relationships and creation/edit routes stay in the tree.
    pub(crate) fn show_hardware_motion_overlay(
        &mut self,
        ctx: &egui::Context,
        canvas: egui::Rect,
    ) -> bool {
        if self.other_modal_open()
            || self.palette.open
            || self.project_files.blocking()
            || self.navigation.pending().is_some()
            || self.open_drawer.is_some()
        {
            return false;
        }
        let selected = match self.session.inspector {
            Some(InspectorTarget::Installation(id)) => Some(id),
            _ => None,
        };
        let joint = self
            .editor
            .project()
            .door_joints
            .iter()
            .find(|joint| self.door_motion.map(|(id, _)| id) == Some(joint.id))
            .or_else(|| {
                self.editor.project().door_joints.iter().find(|joint| {
                    selected.is_some_and(|id| joint.hinge_installation_ids.contains(&id))
                })
            })
            .cloned();
        let Some(joint) = joint else {
            return false;
        };
        let start = Request::with(A::StartMotion, Target::Door(joint.id));
        let close = Request::new(A::CloseMotion);
        let active = self.door_motion.map(|(id, _)| id) == Some(joint.id);
        let top = egui::Area::new(egui::Id::new("hardware-motion-mode"))
            .order(egui::Order::Foreground)
            .fixed_pos(
                canvas.left_top()
                    + egui::vec2(12.0, if canvas.width() < 600.0 { 58.0 } else { 12.0 }),
            )
            .show(ctx, |ui| {
                tw::floating_frame().inner_margin(3).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (selected, key, request) in [
                            (active, "hardware-motion-preview", start),
                            (!active, "hardware-closed", close),
                        ] {
                            let enabled = !selected && self.action_availability(request).is_ok();
                            let text = if selected {
                                tw::medium(ui, self.localizer.text(key), 12.5).color(tw::PANEL)
                            } else {
                                egui::RichText::new(self.localizer.text(key))
                                    .size(12.5)
                                    .color(if enabled { tw::SECONDARY } else { tw::FAINT })
                            };
                            let response = ui
                                .add_enabled(
                                    enabled || selected,
                                    egui::Button::new(text)
                                        .selected(selected)
                                        .fill(if selected {
                                            tw::TEXT
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        })
                                        .stroke(egui::Stroke::NONE)
                                        .corner_radius(6)
                                        .min_size(egui::vec2(0.0, 26.0)),
                                )
                                .on_hover_text(if key == "hardware-closed" {
                                    A::CloseMotion.label(&self.localizer)
                                } else {
                                    self.localizer.text("door-motion-disclosure")
                                });
                            if response.clicked() && !selected {
                                self.invoke_or_report(request);
                            }
                        }
                    });
                });
            });
        let mut blocked = top.response.contains_pointer();
        if self.door_motion.is_some() {
            let width = (canvas.width() - 32.0).clamp(160.0, 460.0);
            let hud = egui::Area::new(egui::Id::new("hardware-motion-hud"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::CENTER_BOTTOM)
                .fixed_pos(canvas.center_bottom() - egui::vec2(0.0, 16.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(tw::PANEL)
                        .stroke(egui::Stroke::new(1.0, tw::BORDER_STRONG))
                        .corner_radius(11)
                        .inner_margin(egui::Margin::symmetric(16, 12))
                        .shadow(egui::Shadow {
                            offset: [0, 8],
                            blur: 24,
                            spread: 0,
                            color: egui::Color32::from_rgba_unmultiplied(60, 45, 25, 40),
                        })
                        .show(ui, |ui| {
                            ui.set_width(width - 32.0);
                            self.show_door_motion_controls(ui);
                        });
                });
            blocked |= hud.response.contains_pointer();
        }
        blocked
    }

    /// Body of the motion HUD: door name, angle, slider, scale and one note.
    /// The angle is session display state; the saved pose never changes.
    pub(crate) fn show_door_motion_controls(&mut self, ui: &mut egui::Ui) {
        let Some((id, mut angle)) = self.door_motion else {
            return;
        };
        let Some(joint) = self
            .editor
            .project()
            .door_joints
            .iter()
            .find(|j| j.id == id)
            .cloned()
        else {
            return;
        };
        let name = object_name(self.editor.project(), joint.moving_root_id).to_owned();
        let limit = door_joint::opening_limit(self.editor.project(), &joint);
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.add(crate::icons::icon(Icon::Door, tw::MUTED, 15.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{angle:.0}°"))
                        .font(if tw::weights_available(ui) {
                            egui::FontId::new(14.0, crate::theme::Typeface::MonoSemibold.family())
                        } else {
                            egui::FontId::monospace(14.0)
                        })
                        .color(tw::TEXT),
                );
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(tw::semibold(ui, &name, 13.0).color(tw::TEXT))
                            .truncate()
                            .selectable(false),
                    );
                });
            });
        });
        match limit {
            Ok(limit) => {
                ui.spacing_mut().item_spacing.y = 6.0;
                let label = A::SetDoorAngle.label(&self.localizer);
                let (slider, travel) = angle_slider(ui, &mut angle, limit, &label, true);
                if slider.changed() {
                    self.invoke_or_report(
                        Request::with(A::SetDoorAngle, Target::Door(id))
                            .argument(Argument::Angle(angle)),
                    );
                }
                show_motion_ticks(ui, travel, slider.rect.x_range(), limit);
                ui.add_space(2.0);
            }
            Err(_) => {
                ui.label(
                    egui::RichText::new(self.localizer.text("door-motion-unavailable"))
                        .size(11.5)
                        .color(tw::WARN_INK),
                );
            }
        }
        ui.add(
            egui::Label::new(
                egui::RichText::new(self.localizer.text("hardware-motion-note"))
                    .size(11.5)
                    .color(tw::MUTED),
            )
            .wrap(),
        )
        .on_hover_text(self.localizer.text("door-motion-disclosure"));
    }

    pub(crate) fn show_door_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_door() else {
            return;
        };
        let title = self.localizer.text(if draft.editing {
            "door-edit"
        } else {
            "door-add"
        });
        let mut chrome = draft.chrome.detach();
        let opening = !chrome.is_active();
        let mut first_control = None;
        let result = chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &title,
            },
            |ui| {
                let p = self.editor.project();
                let localizer = &self.localizer;
                ui.spacing_mut().item_spacing.y = 4.0;
                let gap = 12.0;
                let half = ((ui.available_width() - gap) / 2.0).max(80.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.allocate_ui_with_layout(
                        egui::vec2(half, 52.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            hinge_ui::field_label(ui, &localizer.text("door-moving"));
                            let moving = egui::ComboBox::from_id_salt("door-moving")
                                .width(half)
                                .selected_text(draft.root.map_or("—", |id| object_name(p, id)))
                                .show_ui(ui, |ui| {
                                    for a in &p.assemblies {
                                        crate::combo_option(
                                            ui,
                                            &mut draft.root,
                                            Some(a.id),
                                            &a.name,
                                        );
                                    }
                                    for b in &p.boards {
                                        crate::combo_option(
                                            ui,
                                            &mut draft.root,
                                            Some(b.id),
                                            &b.name,
                                        );
                                    }
                                });
                            first_control = Some(moving.response.id);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(half, 52.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            hinge_ui::field_label(ui, &localizer.text("door-stationary"));
                            egui::ComboBox::from_id_salt("door-stationary")
                                .width(half)
                                .selected_text(draft.mount.map_or("—", |id| object_name(p, id)))
                                .show_ui(ui, |ui| {
                                    for b in &p.boards {
                                        crate::combo_option(
                                            ui,
                                            &mut draft.mount,
                                            Some(b.id),
                                            &b.name,
                                        );
                                    }
                                });
                        },
                    );
                });
                ui.add_space(8.0);
                hinge_ui::field_label(ui, &localizer.text("door-hinges"));
                let mut any = false;
                egui::Frame::new()
                    .fill(tw::APP)
                    .stroke(egui::Stroke::new(1.0, tw::BORDER_SOFT))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for h in &p.hinge_installations {
                            if draft.mount != Some(h.mounting_board_id)
                                || !draft.root.is_some_and(|root| {
                                    door_joint::moving_members(p, root).contains(&h.door_board_id)
                                })
                            {
                                continue;
                            }
                            any = true;
                            let mut checked = draft.hinges.contains(&h.id);
                            let label = format!(
                                "{} {} · {} → {}",
                                localizer.text("hardware-hinge"),
                                hinge_ui::hinge_ordinal(p, h.id),
                                object_name(p, h.door_board_id),
                                object_name(p, h.mounting_board_id),
                            );
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut checked, label).changed() {
                                    if checked {
                                        draft.hinges.push(h.id);
                                    } else {
                                        draft.hinges.retain(|id| *id != h.id);
                                    }
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            tw::mono(
                                                format!(
                                                    "Y {}",
                                                    hinge_ui::short_mm(localizer, h.door_y)
                                                ),
                                                11.5,
                                            )
                                            .color(tw::FAINT),
                                        );
                                    },
                                );
                            });
                        }
                        if !any {
                            ui.label(
                                egui::RichText::new(localizer.text("hardware-no-hinges"))
                                    .size(12.0)
                                    .color(tw::FAINT),
                            );
                        }
                    });
                ui.add_space(8.0);
                let proposal = draft.proposal(self);
                if let Ok(ref preview) = proposal {
                    tw::card().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::Grid::new("door-dialog-preview")
                            .num_columns(2)
                            .spacing(egui::vec2(10.0, 5.0))
                            .show(ui, |ui| {
                                let muted = |ui: &mut egui::Ui, key: &str| {
                                    ui.label(
                                        egui::RichText::new(localizer.text(key))
                                            .size(12.0)
                                            .color(tw::MUTED),
                                    );
                                };
                                muted(ui, "door-members");
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(
                                            preview
                                                .moving_members
                                                .iter()
                                                .map(|id| object_name(p, *id))
                                                .collect::<Vec<_>>()
                                                .join(", "),
                                        )
                                        .size(12.5),
                                    )
                                    .wrap(),
                                );
                                ui.end_row();
                                muted(ui, "door-fixed");
                                ui.label(
                                    egui::RichText::new(object_name(
                                        p,
                                        preview.joint.mounting_board_id,
                                    ))
                                    .size(12.5),
                                );
                                ui.end_row();
                                muted(ui, "door-axis");
                                let v = |values: [f64; 3]| {
                                    values
                                        .map(|value| {
                                            let text = format!("{:.3}", value + 0.0);
                                            text.trim_end_matches('0')
                                                .trim_end_matches('.')
                                                .to_owned()
                                        })
                                        .join(", ")
                                };
                                ui.label(
                                    tw::mono(
                                        format!(
                                            "({}) · ({})",
                                            v(preview.joint.axis_direction),
                                            v(preview.joint.axis_origin_mm)
                                        ),
                                        11.5,
                                    )
                                    .color(tw::TEXT),
                                );
                                ui.end_row();
                            });
                    });
                    let issues: Vec<_> = preview
                        .installation_statuses
                        .iter()
                        .flat_map(|status| status.issues.iter())
                        .collect();
                    if !issues.is_empty() {
                        ui.add_space(6.0);
                        hinge_ui::warning_callout(ui, false, |ui| {
                            for issue in issues {
                                ui.label(
                                    egui::RichText::new(localizer.text(hinge_ui::issue_key(issue)))
                                        .size(12.0)
                                        .color(tw::WARN_INK),
                                );
                            }
                        });
                    }
                } else {
                    ui.label(
                        egui::RichText::new(localizer.text("door-invalid"))
                            .size(11.5)
                            .color(tw::DANGER),
                    );
                }
                if draft.error && proposal.is_ok() {
                    ui.label(
                        egui::RichText::new(localizer.text("door-invalid"))
                            .size(11.5)
                            .color(tw::DANGER),
                    );
                }
                ((), proposal.is_ok())
            },
        );
        if opening && let Some(id) = first_control {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm) {
            if draft
                .proposal(self)
                .is_ok_and(|p| door_joint::confirm(&mut self.editor, p).is_ok())
            {
                chrome.close(ctx);
                return;
            }
            draft.error = true;
        }
        draft.chrome = chrome;
        self.modals.set_door(Some(draft));
    }

    pub(crate) fn show_removal_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_removal() else {
            return;
        };
        let current = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        let mut chrome = draft.chrome.detach();
        let result = chrome.show(
            ctx,
            &self.localizer.text("door-delete-confirm"),
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &self.localizer.text("door-delete-confirm"),
            },
            |ui| {
                let p = self.editor.project();
                match draft.target {
                    DoorRemoval::Object(id) => {
                        let members = door_joint::moving_members(p, id);
                        let boards: Vec<_> = p
                            .boards
                            .iter()
                            .filter(|b| members.contains(&b.id))
                            .map(|b| b.id)
                            .collect();
                        let installations: Vec<_> = p
                            .hinge_installations
                            .iter()
                            .filter(|h| {
                                boards.contains(&h.door_board_id)
                                    || boards.contains(&h.mounting_board_id)
                            })
                            .map(|h| h.id)
                            .collect();
                        let joints: Vec<_> = p
                            .door_joints
                            .iter()
                            .filter(|j| {
                                members.contains(&j.moving_root_id)
                                    || boards.contains(&j.mounting_board_id)
                                    || j.hinge_installation_ids
                                        .iter()
                                        .any(|h| installations.contains(h))
                            })
                            .collect();
                        ui.label(format!(
                            "{}: {}",
                            self.localizer.text("door-members"),
                            members
                                .iter()
                                .map(|id| object_name(p, *id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                        ui.label(self.localizer.text("door-allocations"));
                        for allocation in p
                            .allocations
                            .iter()
                            .filter(|a| boards.contains(&a.board_id))
                        {
                            let stock = p
                                .stock
                                .iter()
                                .find(|s| s.id == allocation.stock_id)
                                .map_or("?", |s| s.name.as_str());
                            ui.label(format!(
                                "{} → {}",
                                object_name(p, allocation.board_id),
                                stock
                            ));
                        }
                        ui.label(self.localizer.text("door-hinges"));
                        for h in p
                            .hinge_installations
                            .iter()
                            .filter(|h| installations.contains(&h.id))
                        {
                            ui.label(format!(
                                "{} → {} · Y {}",
                                object_name(p, h.door_board_id),
                                object_name(p, h.mounting_board_id),
                                h.door_y.micrometres() as f64 / 1000.0
                            ));
                        }
                        ui.label(self.localizer.text("door-joints"));
                        for j in joints {
                            ui.label(format!(
                                "{} → {}",
                                object_name(p, j.moving_root_id),
                                object_name(p, j.mounting_board_id)
                            ));
                        }
                    }
                    DoorRemoval::Joint(id) => {
                        if let Some(j) = p.door_joints.iter().find(|j| j.id == id) {
                            ui.label(format!(
                                "{} → {}",
                                object_name(p, j.moving_root_id),
                                object_name(p, j.mounting_board_id)
                            ));
                        }
                    }
                    DoorRemoval::Hardware(id) => {
                        if let Some(hardware) = p.hardware.iter().find(|h| h.id == id) {
                            ui.label(&hardware.name);
                            ui.small(self.localizer.text("hardware-remove-description"));
                        }
                    }
                }
                if !current || draft.error {
                    ui.colored_label(tw::DANGER, self.localizer.text("door-invalid"));
                }
                ((), current)
            },
        );
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm) {
            let removed = match draft.target {
                DoorRemoval::Object(id) => door_joint::delete_object(&mut self.editor, id).is_ok(),
                DoorRemoval::Joint(id) => door_joint::remove(&mut self.editor, id).is_ok(),
                DoorRemoval::Hardware(id) => self.remove_reference_hardware(id),
            };
            if removed {
                self.selection.retain_objects(self.editor.project());
                chrome.close(ctx);
                return;
            }
            draft.error = true;
        }
        draft.chrome = chrome;
        self.modals.set_removal(Some(draft));
    }
}

fn motion_ticks(limit: f64) -> Vec<f64> {
    let mut values = vec![0.0];
    let mut intermediate = 45.0;
    while intermediate < limit {
        values.push(intermediate);
        intermediate += 45.0;
    }
    values.push(limit);
    values
}

const KNOB_RADIUS: f32 = 9.0;

/// Display-only angle slider: `border_soft` track, `accent` fill and an 18px
/// knob with a 2px `accent` stroke. Returns the response and the knob travel
/// (knob centres), which the scale below shares.
fn angle_slider(
    ui: &mut egui::Ui,
    angle: &mut f64,
    limit: f64,
    label: &str,
    enabled: bool,
) -> (egui::Response, egui::Rangef) {
    let width = ui.available_width().max(2.0 * KNOB_RADIUS + 1.0);
    let (rect, mut response) = ui.allocate_exact_size(
        egui::vec2(width, 2.0 * KNOB_RADIUS),
        if enabled {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::hover()
        },
    );
    let travel = egui::Rangef::new(rect.left() + KNOB_RADIUS, rect.right() - KNOB_RADIUS);
    let limit = limit.max(f64::EPSILON);
    let before = *angle;
    if enabled {
        if (response.dragged() || response.clicked() || response.is_pointer_button_down_on())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let t = ((pointer.x - travel.min) / travel.span().max(1.0)).clamp(0.0, 1.0);
            *angle = (f64::from(t) * limit).round().clamp(0.0, limit);
        }
        if response.has_focus() {
            let step = if ui.input(|i| i.modifiers.shift) {
                15.0
            } else {
                1.0
            };
            ui.input(|i| {
                if i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowUp) {
                    *angle = (*angle + step).min(limit);
                }
                if i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowDown) {
                    *angle = (*angle - step).max(0.0);
                }
                if i.key_pressed(egui::Key::Home) {
                    *angle = 0.0;
                }
                if i.key_pressed(egui::Key::End) {
                    *angle = limit;
                }
            });
        }
    }
    if *angle != before {
        response.mark_changed();
    }
    let value = *angle;
    response.widget_info(|| egui::WidgetInfo::slider(enabled, value, label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let y = rect.center().y;
        let x = egui::lerp(travel, (value / limit).clamp(0.0, 1.0) as f32);
        let track = egui::Rect::from_x_y_ranges(rect.x_range(), (y - 2.0)..=(y + 2.0));
        painter.rect_filled(track, 2.0, tw::BORDER_SOFT);
        painter.rect_filled(
            egui::Rect::from_x_y_ranges(rect.left()..=x, (y - 2.0)..=(y + 2.0)),
            2.0,
            tw::ACCENT,
        );
        let knob = egui::pos2(x, y);
        painter.circle_filled(
            knob + egui::vec2(0.0, 1.0),
            KNOB_RADIUS + 0.5,
            egui::Color32::from_rgba_unmultiplied(60, 45, 25, 40),
        );
        painter.circle(
            knob,
            KNOB_RADIUS - 1.0,
            tw::PANEL,
            egui::Stroke::new(2.0, tw::ACCENT),
        );
        if response.has_focus() {
            painter.circle_stroke(
                knob,
                KNOB_RADIUS + 3.0,
                egui::Stroke::new(2.0, tw::ACCENT_BG),
            );
        }
    }
    (response, travel)
}

/// Mono tick labels (0/45/90/limit) aligned with the knob travel. Endpoints stay
/// inside `bounds`; crowded intermediate labels move to another row.
fn show_motion_ticks(ui: &mut egui::Ui, travel: egui::Rangef, bounds: egui::Rangef, limit: f64) {
    let font = egui::FontId::monospace(10.5);
    let row_height = 14.0;
    let top = ui.cursor().min.y;
    let ticks = motion_ticks(limit);
    let mut labels: Vec<(egui::Rect, std::sync::Arc<egui::Galley>)> = Vec::new();
    let mut rows = 1;
    // Reserve both endpoint labels on the first row before intermediates.
    for angle in [0.0, limit]
        .into_iter()
        .chain(ticks[1..ticks.len() - 1].iter().copied())
    {
        let x = egui::lerp(travel, (angle / limit.max(f64::EPSILON)) as f32);
        let galley = ui
            .painter()
            .layout_no_wrap(format!("{angle:.0}°"), font.clone(), tw::FAINT);
        let left = (x - galley.size().x * 0.5)
            .clamp(bounds.min, (bounds.max - galley.size().x).max(bounds.min));
        let mut row = 0;
        let mut rect = egui::Rect::from_min_size(egui::pos2(left, top), galley.size());
        while labels
            .iter()
            .any(|(placed, _)| placed.expand(2.0).intersects(rect))
        {
            row += 1;
            rect = rect.translate(egui::vec2(0.0, row_height));
        }
        rows = rows.max(row + 1);
        labels.push((rect, galley));
    }
    ui.allocate_exact_size(
        egui::vec2(bounds.span(), rows as f32 * row_height),
        egui::Sense::hover(),
    );
    for (rect, galley) in labels {
        ui.painter().galley(rect.min, galley, tw::FAINT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
    use plan_my_cabinet::domain::{
        BoardEdge, BoardFace, BoardGrain, HingeInstallation, HingeMountingSide,
    };

    fn fixture() -> (DesktopApp, Uuid, Uuid) {
        let mut app = DesktopApp::default();
        let material = app
            .editor
            .create_material(NewMaterial {
                name: "Wood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Unrestricted,
            })
            .unwrap();
        let mut ids = Vec::new();
        for name in ["Door", "Mount"] {
            ids.push(
                app.editor
                    .create_board(NewBoard {
                        name: name.into(),
                        material_id: material,
                        length: Length::from_micrometres(100_000),
                        width: Length::from_micrometres(100_000),
                        pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
                    })
                    .unwrap(),
            );
        }
        hardware_catalog::add_builtin(&mut app.editor).unwrap();
        let hinge = HingeInstallation {
            id: Uuid::new_v4(),
            door_board_id: ids[0],
            mounting_board_id: ids[1],
            catalog_id: app.editor.project().catalog[0].id,
            side: HingeMountingSide {
                door_edge: BoardEdge::MinX,
                door_face: BoardFace::MinZ,
                mount_front_edge: BoardEdge::MinX,
                mount_face: BoardFace::MaxZ,
            },
            door_y: Length::from_micrometres(50_000),
            mount_y: Length::from_micrometres(50_000),
            cup_edge_setback: Length::from_micrometres(3_000),
            overlay: Length::from_micrometres(15_000),
        };
        plan_my_cabinet::hinge_installation::create(&mut app.editor, hinge).unwrap();
        (app, ids[0], ids[1])
    }

    #[test]
    fn angle_scale_tracks_slider_knob_and_keeps_labels_separate() {
        for width in [150.0, 300.0, 428.0] {
            for limit in [60.0, 90.0, 105.0, 110.0, 180.0] {
                let ctx = egui::Context::default();
                crate::theme::install_fonts(&ctx);
                let mut angle = 45.0;
                let mut rail = egui::Rect::NOTHING;
                let mut travel = egui::Rangef::EVERYTHING;
                let output = ctx.run_ui(Default::default(), |ui| {
                    ui.allocate_ui(egui::vec2(width, 200.0), |ui| {
                        let (response, range) = angle_slider(ui, &mut angle, limit, "Angle", true);
                        rail = response.rect;
                        travel = range;
                        show_motion_ticks(ui, travel, rail.x_range(), limit);
                    });
                });
                let mut labels = Vec::new();
                let mut knob = None;
                for shape in &output.shapes {
                    match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text().ends_with('°') => {
                            labels.push((
                                text.galley.text().to_owned(),
                                egui::Rect::from_min_size(text.pos, text.galley.size()),
                            ));
                        }
                        egui::Shape::Circle(circle)
                            if (circle.center.y - rail.center().y).abs() < 0.01
                                && (circle.radius - (KNOB_RADIUS - 1.0)).abs() < 0.01 =>
                        {
                            knob = Some(circle.center.x)
                        }
                        _ => {}
                    }
                }
                let ticks = motion_ticks(limit);
                assert_eq!(labels.len(), ticks.len());
                assert!(
                    (knob.expect("slider knob") - egui::lerp(travel, (45.0 / limit) as f32)).abs()
                        < 0.1,
                    "knob must share the scale's range at width {width}, limit {limit}"
                );
                for (index, (text, label)) in labels.iter().enumerate() {
                    assert!(
                        label.left() >= rail.left() - 0.1 && label.right() <= rail.right() + 0.1,
                        "{text} leaves the rail at width {width}"
                    );
                    assert!(
                        labels[index + 1..]
                            .iter()
                            .all(|(_, other)| !label.intersects(*other)),
                        "angle labels overlap at width {width}, limit {limit}: {labels:?}"
                    );
                    let value: f64 = text.trim_end_matches('°').parse().unwrap();
                    let x = egui::lerp(travel, (value / limit) as f32);
                    let clamped =
                        label.left() <= rail.left() + 0.1 || label.right() >= rail.right() - 0.1;
                    assert!(
                        clamped || (label.center().x - x).abs() < 0.6,
                        "{text} is centred on its knob position"
                    );
                }
                assert_eq!(angle, 45.0, "laying out the scale must not move the door");
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn motion_controls_disclose_limit_and_exit_without_editing_selection_or_document() {
        assert_eq!(motion_ticks(90.0), [0.0, 45.0, 90.0]);
        assert_eq!(motion_ticks(105.0), [0.0, 45.0, 90.0, 105.0]);
        let (mut app, door, mount) = fixture();
        let proposed = door_joint::preview(
            app.editor.project(),
            Uuid::new_v4(),
            door,
            mount,
            vec![app.editor.project().hinge_installations[0].id],
        )
        .unwrap();
        door_joint::confirm(&mut app.editor, proposed).unwrap();
        app.selection.choose(Some(door), false);
        let original = app.editor.project().clone();
        let joint_id = original.door_joints[0].id;
        app.door_motion = Some((joint_id, 105.0));
        let ctx = egui::Context::default();
        for language in [Language::En, Language::PtBr] {
            app.localizer.set_language(language);
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                app.show_door_motion_controls(ui)
            });
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            assert!(
                texts
                    .iter()
                    .any(|s| s == &app.localizer.text("hardware-motion-note"))
            );
            assert!(texts.iter().any(|s| s.contains("105")));
            output.drop_without_applying_deltas();
        }
        assert!(app.modal_open());
        assert_eq!(app.selection.active, Some(door));
        assert_eq!(app.editor.project(), &original);
        app.door_motion = None;
        assert!(!app.modal_open());
        assert_eq!(app.editor.project(), &original);
        assert_eq!(
            door_joint::derived_poses(app.editor.project(), &original.door_joints[0], 0.0)
                .unwrap()
                .into_iter()
                .find(|(id, _)| *id == door)
                .unwrap()
                .1,
            plan_my_cabinet::assembly_edit::world_pose(app.editor.project(), door).unwrap()
        );
    }

    #[test]
    fn canvas_motion_card_is_bilingual_bounded_and_closed_restores_without_edit() {
        for language in [Language::En, Language::PtBr] {
            let mut app = DesktopApp {
                editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
                ..Default::default()
            };
            app.localizer.set_language(language);
            app.session.switch(Workspace::Hardware);
            app.session.inspector = Some(InspectorTarget::Installation(
                plan_my_cabinet::reference_fixture::HINGE_IDS[0],
            ));
            let original = app.editor.project().clone();
            let joint = original.door_joints[0].id;
            app.invoke(Request::with(A::StartMotion, Target::Door(joint)))
                .unwrap();
            app.invoke(
                Request::with(A::SetDoorAngle, Target::Door(joint)).argument(Argument::Angle(60.0)),
            )
            .unwrap();
            let ctx = egui::Context::default();
            let canvas =
                egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(580.0, 620.0));
            let mut point = None;
            for _ in 0..3 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(640.0, 680.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        app.show_hardware_motion_overlay(ui.ctx(), canvas);
                    },
                );
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape {
                        if text.galley.text() == app.localizer.text("hardware-closed") {
                            point = Some(text.pos + text.galley.size() * 0.5);
                        }
                        if text.galley.text() == app.localizer.text("hardware-motion-note") {
                            assert!(canvas.contains_rect(egui::Rect::from_min_size(
                                text.pos,
                                text.galley.size()
                            )));
                        }
                    }
                }
                output.drop_without_applying_deltas();
            }
            let point = point.expect("Closed is reachable on the canvas");
            for pressed in [true, false] {
                ctx.run_ui(
                    egui::RawInput {
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
                    },
                    |ui| {
                        app.show_hardware_motion_overlay(ui.ctx(), canvas);
                    },
                )
                .drop_without_applying_deltas();
            }
            assert!(app.door_motion.is_none());
            assert_eq!(app.editor.project(), &original);
        }
    }

    #[test]
    fn relationship_draft_blocks_motion_without_losing_its_choices() {
        let mut app = DesktopApp {
            editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
            ..Default::default()
        };
        app.session.switch(Workspace::Hardware);
        let joint = app.editor.project().door_joints[0].id;
        let original = app.editor.project().clone();
        let draft = DoorDialog::new(&app, Some(joint));
        let selected = (draft.root, draft.mount, draft.hinges.clone());
        app.modals.set_door(Some(draft));
        assert!(
            app.invoke(Request::with(A::StartMotion, Target::Door(joint)))
                .is_err()
        );
        let retained = app.modals.door().unwrap();
        assert_eq!(
            (retained.root, retained.mount, retained.hinges.clone()),
            selected
        );
        assert!(app.door_motion.is_none());
        assert_eq!(app.editor.project(), &original);
    }

    #[test]
    fn select_confirm_duplicate_cancel_and_delete_are_atomic() {
        let (mut app, door, mount) = fixture();
        let mut draft = DoorDialog::new(&app, None);
        draft.root = Some(door);
        draft.mount = Some(mount);
        draft.hinges = vec![app.editor.project().hinge_installations[0].id];
        let initial = app.editor.project().clone();
        let ctx = egui::Context::default();
        app.modals.set_door(Some(draft));
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_door_dialog(ui.ctx())
        })
        .drop_without_applying_deltas();
        assert_eq!(app.editor.project(), &initial);
        let draft = app.modals.take_door().unwrap();
        let proposal = draft.proposal(&app).unwrap();
        door_joint::confirm(&mut app.editor, proposal).unwrap();
        assert_eq!(app.editor.project().door_joints.len(), 1);
        let mut duplicate = DoorDialog::new(&app, None);
        duplicate.root = Some(door);
        duplicate.mount = Some(mount);
        duplicate.hinges = vec![app.editor.project().hinge_installations[0].id];
        assert!(duplicate.proposal(&app).is_err());
        let mut cycle = DoorDialog::new(&app, None);
        cycle.root = Some(door);
        cycle.mount = Some(door);
        cycle.hinges = duplicate.hinges;
        assert!(cycle.proposal(&app).is_err());
        let saved = serde_json::to_vec(app.editor.project()).unwrap();
        assert_eq!(
            plan_my_cabinet::persistence::prepare_bytes(&saved)
                .unwrap()
                .project()
                .door_joints,
            app.editor.project().door_joints
        );
        app.modals.set_removal(Some(RemovalDialog::new(&app, DoorRemoval::Object(door))));
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_removal_dialog(ui.ctx())
        })
        .drop_without_applying_deltas();
        ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| app.show_removal_dialog(ui.ctx()),
        )
        .drop_without_applying_deltas();
        assert!(app.modals.removal().is_none());
        assert_eq!(app.editor.project().boards.len(), 2);
        door_joint::delete_object(&mut app.editor, door).unwrap();
        assert!(app.editor.project().door_joints.is_empty());
        assert!(app.editor.project().hinge_installations.is_empty());
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().door_joints.len(), 1);
        assert_eq!(app.editor.project().hinge_installations.len(), 1);
    }
}
