//! Explicit relationship drafts and dependency-aware object removal.
use super::*;
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
}

impl DoorDialog {
    fn new(app: &DesktopApp, id: Option<Uuid>) -> Self {
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
}

pub(super) struct RemovalDialog {
    pub target: DoorRemoval,
    project_id: Uuid,
    revision: u64,
    error: bool,
}

impl RemovalDialog {
    fn new(app: &DesktopApp, target: DoorRemoval) -> Self {
        Self {
            target,
            project_id: app.editor.project().id,
            revision: app.editor.project().revision,
            error: false,
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
    pub(super) fn show_door_list(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading(self.localizer.text("door-joints"));
        if self.door_motion.is_some() {
            ui.group(|ui| {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    self.localizer.text("door-motion-disclosure"),
                );
                if ui.button(self.localizer.text("door-motion-exit")).clicked() {
                    self.door_motion = None;
                }
            });
        }
        if ui
            .add_enabled(
                !self.modal_open(),
                egui::Button::new(self.localizer.text("door-add")),
            )
            .clicked()
        {
            self.door_dialog = Some(DoorDialog::new(self, None));
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
                        if ui
                            .add(
                                egui::Slider::new(&mut angle, 0.0..=limit)
                                    .text(self.localizer.text("door-motion-angle")),
                            )
                            .changed()
                        {
                            self.door_motion = Some((joint.id, angle));
                        }
                    } else if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("door-motion-start")),
                        )
                        .clicked()
                    {
                        self.editor.cancel_preview();
                        self.move_tool.cancel();
                        self.door_motion = Some((joint.id, 0.0));
                    }
                } else {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        self.localizer.text("door-motion-unavailable"),
                    );
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("hinge-edit")),
                        )
                        .clicked()
                    {
                        self.door_dialog = Some(DoorDialog::new(self, Some(joint.id)));
                    }
                    if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("hinge-delete")),
                        )
                        .clicked()
                    {
                        self.removal_dialog =
                            Some(RemovalDialog::new(self, DoorRemoval::Joint(joint.id)));
                    }
                });
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
            self.removal_dialog = Some(RemovalDialog::new(
                self,
                DoorRemoval::Object(target.unwrap()),
            ));
        }
    }

    pub(super) fn show_door_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.door_dialog.take() else {
            return;
        };
        let mut accept = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("door-joint-dialog")).show(ctx, |ui| {
            ui.heading(self.localizer.text(if draft.editing {
                "door-edit"
            } else {
                "door-add"
            }));
            let p = self.editor.project();
            egui::ComboBox::from_label(self.localizer.text("door-moving"))
                .selected_text(draft.root.map_or("—", |id| object_name(p, id)))
                .show_ui(ui, |ui| {
                    for a in &p.assemblies {
                        ui.selectable_value(&mut draft.root, Some(a.id), &a.name);
                    }
                    for b in &p.boards {
                        ui.selectable_value(&mut draft.root, Some(b.id), &b.name);
                    }
                });
            egui::ComboBox::from_label(self.localizer.text("door-stationary"))
                .selected_text(draft.mount.map_or("—", |id| object_name(p, id)))
                .show_ui(ui, |ui| {
                    for b in &p.boards {
                        ui.selectable_value(&mut draft.mount, Some(b.id), &b.name);
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
            ui.horizontal(|ui| {
                cancel = ui.button(self.localizer.text("cancel")).clicked();
                accept = ui
                    .add_enabled(
                        proposal.is_ok(),
                        egui::Button::new(self.localizer.text("confirm")),
                    )
                    .clicked();
            });
        });
        if cancel || modal.should_close() {
            return;
        }
        if accept {
            if draft
                .proposal(self)
                .is_ok_and(|p| door_joint::confirm(&mut self.editor, p).is_ok())
            {
                return;
            }
            draft.error = true;
        }
        self.door_dialog = Some(draft);
    }

    pub(super) fn show_removal_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.removal_dialog.take() else {
            return;
        };
        let current = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        let mut accept = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("door-removal-dialog")).show(ctx, |ui| {
            ui.heading(self.localizer.text("door-delete-confirm"));
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
            }
            if !current || draft.error {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    self.localizer.text("door-invalid"),
                );
            }
            ui.horizontal(|ui| {
                cancel = ui.button(self.localizer.text("cancel")).clicked();
                accept = ui
                    .add_enabled(current, egui::Button::new(self.localizer.text("confirm")))
                    .clicked();
            });
        });
        if cancel || modal.should_close() {
            return;
        }
        if accept {
            let result = match draft.target {
                DoorRemoval::Object(id) => door_joint::delete_object(&mut self.editor, id),
                DoorRemoval::Joint(id) => door_joint::remove(&mut self.editor, id),
            };
            if result.is_ok() {
                self.selection.retain_objects(self.editor.project());
                return;
            }
            draft.error = true;
        }
        self.removal_dialog = Some(draft);
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
    fn motion_controls_disclose_limit_and_exit_without_editing_selection_or_document() {
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
            let output = ctx.run_ui(egui::RawInput::default(), |ui| app.show_door_list(ui));
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
