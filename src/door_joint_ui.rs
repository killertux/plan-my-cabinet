//! Explicit relationship drafts and dependency-aware object removal.
use super::*;
use crate::actions::{ActionId as A, Argument, Request, Target};
use plan_my_cabinet::door_joint::{self, JointPreview};

pub(super) struct DoorDialog {
    id: Uuid,
    editing: bool,
    project_id: Uuid,
    revision: u64,
    root: Option<Uuid>,
    mount: Option<Uuid>,
    hinges: Vec<Uuid>,
    error: bool,
    chrome: Option<ModalChrome>,
}

impl DoorDialog {
    pub(super) fn new(app: &DesktopApp, id: Option<Uuid>) -> Self {
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
            chrome: Some(ModalChrome::new(egui::Id::new("door-joint-dialog")).width(540.0)),
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

pub(super) enum DoorRemoval {
    Joint(Uuid),
    Object(Uuid),
    Hardware(Uuid),
}

pub(super) struct RemovalDialog {
    pub target: DoorRemoval,
    project_id: Uuid,
    revision: u64,
    error: bool,
    chrome: Option<ModalChrome>,
}

impl RemovalDialog {
    pub(super) fn new(app: &DesktopApp, target: DoorRemoval) -> Self {
        Self {
            target,
            project_id: app.editor.project().id,
            revision: app.editor.project().revision,
            error: false,
            chrome: Some(ModalChrome::new(egui::Id::new("door-removal-dialog"))),
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
    pub(super) fn show_hardware_motion_overlay(
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
                    + egui::vec2(10.0, if canvas.width() < 600.0 { 58.0 } else { 10.0 }),
            )
            .show(ctx, |ui| {
                crate::theme_widgets::card().inner_margin(4).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.action_availability(start).is_ok(),
                                egui::Button::selectable(
                                    active,
                                    self.localizer.text("door-motion-start"),
                                ),
                            )
                            .clicked()
                        {
                            let _ = self.invoke(start);
                        }
                        if ui
                            .add_enabled(
                                active && self.action_availability(close).is_ok(),
                                egui::Button::selectable(
                                    !active,
                                    self.localizer.text("hardware-closed"),
                                ),
                            )
                            .clicked()
                        {
                            let _ = self.invoke(close);
                        }
                    });
                });
            });
        let mut blocked = top.response.contains_pointer();
        if let Some((id, mut angle)) = self.door_motion
            && let Ok(limit) = door_joint::opening_limit(self.editor.project(), &joint)
        {
            let name = object_name(self.editor.project(), joint.moving_root_id).to_owned();
            let hud = egui::Area::new(egui::Id::new("hardware-motion-hud"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::CENTER_BOTTOM)
                .fixed_pos(canvas.center_bottom() - egui::vec2(0.0, 16.0))
                .show(ctx, |ui| {
                    crate::theme_widgets::card().show(ui, |ui| {
                        ui.set_width((canvas.width() - 64.0).clamp(160.0, 436.0));
                        ui.horizontal(|ui| {
                            ui.strong(name);
                            ui.label(format!("{angle:.0}° / {limit:.0}°"));
                        });
                        ui.spacing_mut().slider_width = ui.available_width() - 12.0;
                        let slider =
                            ui.add(egui::Slider::new(&mut angle, 0.0..=limit).show_value(false));
                        if slider.changed() {
                            let _ = self.invoke(
                                Request::with(A::SetDoorAngle, Target::Door(id))
                                    .argument(Argument::Angle(angle)),
                            );
                        }
                        show_motion_ticks(ui, slider.rect, limit);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(self.localizer.text("door-motion-disclosure"))
                                    .small(),
                            )
                            .wrap(),
                        );
                    });
                });
            blocked |= hud.response.contains_pointer();
        }
        blocked
    }

    pub(super) fn show_door_motion_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading(self.localizer.text("door-motion-angle"));
        if self.door_motion.is_some() {
            ui.group(|ui| {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    self.localizer.text("door-motion-disclosure"),
                );
                if ui.button(self.localizer.text("door-motion-exit")).clicked() {
                    let _ = self.invoke(Request::new(A::CloseMotion));
                }
            });
        }
        for joint in self.editor.project().door_joints.clone() {
            let limit = door_joint::opening_limit(self.editor.project(), &joint);
            ui.group(|ui| {
                ui.label(format!(
                    "{} → {}",
                    object_name(self.editor.project(), joint.moving_root_id),
                    object_name(self.editor.project(), joint.mounting_board_id)
                ));
                if door_joint::needs_review(self.editor.project(), &joint) {
                    ui.colored_label(egui::Color32::YELLOW, self.localizer.text("door-review"));
                }
                if let Ok(limit) = limit {
                    if self.door_motion.map(|(id, _)| id) == Some(joint.id) {
                        let mut angle = self.door_motion.unwrap().1;
                        ui.strong(format!("{angle:.0}° / {limit:.0}°"));
                        let slider = ui.add(
                            egui::Slider::new(&mut angle, 0.0..=limit)
                                .text(self.localizer.text("door-motion-angle")),
                        );
                        if slider.changed() {
                            let _ = self.invoke(
                                Request::with(A::SetDoorAngle, Target::Door(joint.id))
                                    .argument(Argument::Angle(angle)),
                            );
                        }
                        // This response also contains the numeric editor and
                        // caption. Only its leading slider-width region is the rail.
                        let rail = egui::Rect::from_min_size(
                            slider.rect.min,
                            egui::vec2(
                                ui.spacing().slider_width,
                                ui.text_style_height(&egui::TextStyle::Body)
                                    .max(ui.spacing().interact_size.y),
                            ),
                        );
                        show_motion_ticks(ui, rail, limit);
                    } else if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("door-motion-start")),
                        )
                        .clicked()
                    {
                        let _ = self.invoke(Request::with(A::StartMotion, Target::Door(joint.id)));
                    }
                } else {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        self.localizer.text("door-motion-unavailable"),
                    );
                }
            });
        }
        let target = self.selection.active.filter(|id| {
            self.editor.project().boards.iter().any(|b| b.id == *id)
                || self.editor.project().assemblies.iter().any(|a| a.id == *id)
        });
        if ui
            .add_enabled(
                !self.modal_open() && target.is_some(),
                egui::Button::new(self.localizer.text("door-delete-object")),
            )
            .clicked()
        {
            let _ = self.invoke(Request::new(A::DeleteObject));
        }
    }

    pub(super) fn show_door_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.door_dialog.take() else {
            return;
        };
        let title = self.localizer.text(if draft.editing {
            "door-edit"
        } else {
            "door-add"
        });
        let mut chrome = draft.chrome.take().expect("relationship modal controller");
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
                let moving = egui::ComboBox::from_label(self.localizer.text("door-moving"))
                    .selected_text(draft.root.map_or("—", |id| object_name(p, id)))
                    .show_ui(ui, |ui| {
                        for a in &p.assemblies {
                            crate::combo_option(ui, &mut draft.root, Some(a.id), &a.name);
                        }
                        for b in &p.boards {
                            crate::combo_option(ui, &mut draft.root, Some(b.id), &b.name);
                        }
                    });
                first_control = Some(moving.response.id);
                egui::ComboBox::from_label(self.localizer.text("door-stationary"))
                    .selected_text(draft.mount.map_or("—", |id| object_name(p, id)))
                    .show_ui(ui, |ui| {
                        for b in &p.boards {
                            crate::combo_option(ui, &mut draft.mount, Some(b.id), &b.name);
                        }
                    });
                ui.label(self.localizer.text("door-hinges"));
                for h in &p.hinge_installations {
                    if draft.mount != Some(h.mounting_board_id)
                        || !draft.root.is_some_and(|root| {
                            door_joint::moving_members(p, root).contains(&h.door_board_id)
                        })
                    {
                        continue;
                    }
                    let mut checked = draft.hinges.contains(&h.id);
                    if ui
                        .checkbox(
                            &mut checked,
                            format!(
                                "{} → {} · {:.3} mm",
                                object_name(p, h.door_board_id),
                                object_name(p, h.mounting_board_id),
                                h.door_y.micrometres() as f64 / 1000.0
                            ),
                        )
                        .changed()
                    {
                        if checked {
                            draft.hinges.push(h.id);
                        } else {
                            draft.hinges.retain(|id| *id != h.id);
                        }
                    }
                }
                let proposal = draft.proposal(self);
                if let Ok(ref preview) = proposal {
                    ui.label(self.localizer.text("door-members"));
                    for id in &preview.moving_members {
                        ui.label(object_name(p, *id));
                    }
                    ui.label(format!(
                        "{}: {}",
                        self.localizer.text("door-fixed"),
                        object_name(p, preview.joint.mounting_board_id)
                    ));
                    ui.label(format!(
                        "{}: {:?} / {:?} mm",
                        self.localizer.text("door-axis"),
                        preview.joint.axis_direction,
                        preview.joint.axis_origin_mm
                    ));
                    for status in &preview.installation_statuses {
                        for issue in &status.issues {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                self.localizer.text(hinge_ui::issue_key(issue)),
                            );
                        }
                    }
                } else {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        format!(
                            "{}: {:?}",
                            self.localizer.text("door-invalid"),
                            proposal.as_ref().err()
                        ),
                    );
                }
                if draft.error {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text("door-invalid"),
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
        draft.chrome = Some(chrome);
        self.door_dialog = Some(draft);
    }

    pub(super) fn show_removal_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.removal_dialog.take() else {
            return;
        };
        let current = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        let mut chrome = draft.chrome.take().expect("removal modal controller");
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
                                "{} → {} · {:.3} mm",
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
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text("door-invalid"),
                    );
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
        draft.chrome = Some(chrome);
        self.removal_dialog = Some(draft);
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

/// Align the scale to the actual linear slider, not a spaced row of labels.
/// Endpoints stay inside the card; crowded intermediate labels use another row.
fn show_motion_ticks(ui: &mut egui::Ui, slider: egui::Rect, limit: f64) {
    // egui 0.36's horizontal Slider::position_range reserves this half-handle
    // width at each end. Keep ticks aligned with handle centers, not rail edges.
    let radius = slider.height() / 2.5;
    let inset = match ui.visuals().handle_shape {
        egui::style::HandleShape::Circle => radius,
        egui::style::HandleShape::Rect { aspect_ratio } => radius * aspect_ratio,
    };
    let travel = slider.x_range().shrink(inset);
    let font = egui::TextStyle::Small.resolve(ui.style());
    let row_height = ui.text_style_height(&egui::TextStyle::Small) + 4.0;
    let top = ui.cursor().min.y;
    let ticks = motion_ticks(limit);
    let mut labels: Vec<(f32, egui::Rect, std::sync::Arc<egui::Galley>)> = Vec::new();
    let mut rows = 1;
    // Reserve both endpoint labels on the first row before intermediates.
    for angle in [0.0, limit]
        .into_iter()
        .chain(ticks[1..ticks.len() - 1].iter().copied())
    {
        let x = egui::lerp(travel, (angle / limit) as f32);
        let galley = ui.painter().layout_no_wrap(
            format!("{angle:.0}°"),
            font.clone(),
            ui.visuals().text_color(),
        );
        let left = (x - galley.size().x * 0.5).clamp(
            slider.left(),
            (slider.right() - galley.size().x).max(slider.left()),
        );
        let mut row = 0;
        let mut rect = egui::Rect::from_min_size(egui::pos2(left, top + 4.0), galley.size());
        while labels
            .iter()
            .any(|(_, placed, _)| placed.expand(2.0).intersects(rect))
        {
            row += 1;
            rect = rect.translate(egui::vec2(0.0, row_height));
        }
        rows = rows.max(row + 1);
        labels.push((x, rect, galley));
    }
    ui.allocate_exact_size(
        egui::vec2(slider.width(), 4.0 + rows as f32 * row_height),
        egui::Sense::hover(),
    );
    for (x, rect, galley) in labels {
        ui.painter().line_segment(
            [egui::pos2(x, top), egui::pos2(x, top + 3.0)],
            egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
        );
        ui.put(rect, egui::Label::new(galley).selectable(false));
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
    fn angle_scale_tracks_slider_handle_and_keeps_labels_separate() {
        for width in [150.0, 300.0, 436.0] {
            for limit in [60.0, 90.0, 105.0, 110.0, 180.0] {
                for handle in [
                    egui::style::HandleShape::Circle,
                    egui::style::HandleShape::Rect { aspect_ratio: 0.6 },
                ] {
                    let ctx = egui::Context::default();
                    crate::theme::install_fonts(&ctx);
                    let mut angle = 45.0;
                    let mut rail = egui::Rect::NOTHING;
                    let mut top = 0.0;
                    let output = ctx.run_ui(Default::default(), |ui| {
                        ui.visuals_mut().handle_shape = handle;
                        ui.spacing_mut().slider_width = width;
                        rail = ui
                            .add(egui::Slider::new(&mut angle, 0.0..=limit).show_value(false))
                            .rect;
                        top = ui.cursor().min.y;
                        show_motion_ticks(ui, rail, limit);
                    });
                    let mut markers = Vec::new();
                    let mut labels = Vec::new();
                    let mut knob = None;
                    for shape in &output.shapes {
                        match &shape.shape {
                            egui::Shape::LineSegment { points, .. }
                                if (points[0].y - top).abs() < 0.01 =>
                            {
                                markers.push(points[0].x)
                            }
                            egui::Shape::Text(text) if text.galley.text().ends_with('°') => {
                                labels
                                    .push(egui::Rect::from_min_size(text.pos, text.galley.size()));
                            }
                            egui::Shape::Circle(circle)
                                if (circle.center.y - rail.center().y).abs() < 0.01
                                    && circle.radius > 4.0 =>
                            {
                                knob = Some(circle.center.x)
                            }
                            egui::Shape::Rect(rect)
                                if (rect.rect.center().y - rail.center().y).abs() < 0.01
                                    && rect.rect.height() > 10.0
                                    && rect.rect.width() < 40.0 =>
                            {
                                knob = Some(rect.rect.center().x)
                            }
                            _ => {}
                        }
                    }
                    markers.sort_by(f32::total_cmp);
                    let ticks = motion_ticks(limit);
                    assert_eq!(markers.len(), ticks.len());
                    assert_eq!(labels.len(), ticks.len());
                    let travel = markers[0]..=*markers.last().unwrap();
                    for (marker, value) in markers.iter().zip(ticks) {
                        assert!(
                            (*marker - egui::lerp(travel.clone(), (value / limit) as f32)).abs()
                                < 0.1
                        );
                    }
                    assert!(
                        (knob.expect("actual slider thumb")
                            - egui::lerp(travel, (45.0 / limit) as f32))
                        .abs()
                            < 0.1,
                        "scale must share the real thumb's range at width {width}, limit {limit}"
                    );
                    for (index, label) in labels.iter().enumerate() {
                        assert!(
                            label.left() >= rail.left() - 0.1
                                && label.right() <= rail.right() + 0.1
                        );
                        assert!(
                            labels[index + 1..]
                                .iter()
                                .all(|other| !label.intersects(*other)),
                            "angle labels overlap at width {width}, limit {limit}: {labels:?}"
                        );
                    }
                    assert_eq!(angle, 45.0, "laying out the scale must not move the door");
                    output.drop_without_applying_deltas();
                }
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
                    .any(|s| s == &app.localizer.text("door-motion-disclosure"))
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
                        if text.galley.text() == app.localizer.text("door-motion-disclosure") {
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
        app.door_dialog = Some(draft);
        assert!(
            app.invoke(Request::with(A::StartMotion, Target::Door(joint)))
                .is_err()
        );
        let retained = app.door_dialog.as_ref().unwrap();
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
        app.door_dialog = Some(draft);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_door_dialog(ui.ctx())
        })
        .drop_without_applying_deltas();
        assert_eq!(app.editor.project(), &initial);
        let draft = app.door_dialog.take().unwrap();
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
        app.removal_dialog = Some(RemovalDialog::new(&app, DoorRemoval::Object(door)));
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
        assert!(app.removal_dialog.is_none());
        assert_eq!(app.editor.project().boards.len(), 2);
        door_joint::delete_object(&mut app.editor, door).unwrap();
        assert!(app.editor.project().door_joints.is_empty());
        assert!(app.editor.project().hinge_installations.is_empty());
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().door_joints.len(), 1);
        assert_eq!(app.editor.project().hinge_installations.len(), 1);
    }
}
