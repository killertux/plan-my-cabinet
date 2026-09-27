//! Dimensioned reference hardware editor. Draft fields never mutate the project.
use super::*;
use crate::actions::{ActionId as A, Argument, Request, Target};
use plan_my_cabinet::assembly_edit::world_pose;
use plan_my_cabinet::domain::HardwareKind;

pub(super) struct HardwareDialog {
    id: Option<Uuid>,
    project_id: Uuid,
    revision: u64,
    name: String,
    parent: Option<Uuid>,
    dimensions: [DimensionDraft; 3],
    position: [DimensionDraft; 3],
    original_position: [String; 3],
    original_world: [f64; 3],
    rotation: Quaternion,
    error: bool,
    chrome: Option<ModalChrome>,
}

impl HardwareDialog {
    pub(super) fn new(app: &DesktopApp, id: Option<Uuid>) -> Self {
        let item = id.and_then(|id| app.editor.project().hardware.iter().find(|h| h.id == id));
        let world = item.and_then(|h| world_pose(app.editor.project(), h.id).ok());
        let dims = item.and_then(|h| match h.kind {
            HardwareKind::Placeholder { dimensions } => Some(dimensions),
            _ => None,
        });
        let field = |value: String| DimensionDraft {
            text: value,
            consent: false,
        };
        let position = std::array::from_fn(|i| {
            field(format!("{:.3}", world.map_or(0.0, |p| p.translation_mm[i])))
        });
        let original_position = position.each_ref().map(|field| field.text.clone());
        Self {
            id,
            project_id: app.editor.project().id,
            revision: app.editor.project().revision,
            name: item.map_or(String::new(), |h| h.name.clone()),
            parent: item.and_then(|h| h.parent_id).or_else(|| {
                app.selection
                    .active
                    .filter(|id| app.editor.project().assemblies.iter().any(|a| a.id == *id))
            }),
            dimensions: std::array::from_fn(|i| {
                field(dims.map_or(String::new(), |d| {
                    format!("{:.3}", d[i].micrometres() as f64 / 1000.0)
                }))
            }),
            position,
            original_position,
            original_world: world.map_or([0.0; 3], |p| p.translation_mm),
            rotation: world.map_or(Quaternion::IDENTITY, |p| p.rotation),
            error: false,
            chrome: Some(
                ModalChrome::new(egui::Id::new("hardware-dialog"))
                    .first_focus(egui::Id::new("hardware-dialog-name")),
            ),
        }
    }
}

impl DesktopApp {
    pub(super) fn remove_reference_hardware(&mut self, id: Uuid) -> bool {
        let removed =
            self.editor
                .transact(|project| -> Result<(), ()> {
                    let Some(index) = project.hardware.iter().position(|h| {
                        h.id == id && matches!(h.kind, HardwareKind::Placeholder { .. })
                    }) else {
                        return Err(());
                    };
                    project.hardware.remove(index);
                    Ok(())
                })
                .is_ok();
        if removed {
            self.selection.choose(None, false);
        }
        removed
    }

    pub(super) fn show_hardware_list(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label(self.localizer.text("hardware-list"));
        let modal = self.modal_open();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !modal && !self.selection.ids.is_empty(),
                    egui::Button::new(self.localizer.text("assembly-group")),
                )
                .clicked()
            {
                let _ = self.invoke(Request::new(A::Group));
            }
            if ui
                .add_enabled(
                    !modal && !self.selection.ids.is_empty(),
                    egui::Button::new(self.localizer.text("assembly-parent")),
                )
                .clicked()
            {
                let _ = self.invoke(Request::new(A::Reparent));
            }
        });
        if ui
            .add_enabled(
                !modal,
                egui::Button::new(self.localizer.text("hardware-new")),
            )
            .clicked()
        {
            let _ = self.invoke(Request::new(A::NewHardware));
        }
        let items: Vec<_> = self
            .editor
            .project()
            .hardware
            .iter()
            .map(|h| match h.kind {
                HardwareKind::Placeholder { dimensions } => {
                    (h.id, h.name.clone(), Some(dimensions), None)
                }
                HardwareKind::Catalog { catalog_id } => {
                    (h.id, h.name.clone(), None, Some(catalog_id))
                }
            })
            .collect();
        for (id, name, dims, catalog_id) in items {
            ui.push_id(id, |ui| {
                let text = dims.map_or_else(
                    || name.clone(),
                    |dims| {
                        format!(
                            "{name} — {} × {} × {} mm",
                            dims[0].micrometres() as f64 / 1000.0,
                            dims[1].micrometres() as f64 / 1000.0,
                            dims[2].micrometres() as f64 / 1000.0,
                        )
                    },
                );
                if ui
                    .add_enabled(
                        !modal,
                        egui::Button::new(text.clone())
                            .wrap_mode(egui::TextWrapMode::Wrap)
                            .selected(self.selection.ids.contains(&id)),
                    )
                    .on_hover_text(&text)
                    .clicked()
                {
                    let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                    let _ = self.invoke(
                        Request::with(A::SelectObject, Target::Object(id))
                            .argument(Argument::Additive(additive)),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            !modal && dims.is_some(),
                            egui::Button::new(self.localizer.text("hardware-edit")),
                        )
                        .clicked()
                    {
                        let _ = self.invoke(Request::with(A::EditHardware, Target::Object(id)));
                    }
                    if ui
                        .add_enabled(
                            !modal && dims.is_some(),
                            egui::Button::new(self.localizer.text("hardware-duplicate")),
                        )
                        .clicked()
                    {
                        let _ =
                            self.invoke(Request::with(A::DuplicateHardware, Target::Object(id)));
                    }
                    if dims.is_some() {
                        let request = Request::with(A::DeleteHardware, Target::Object(id));
                        if crate::actions::button(
                            ui,
                            &self.localizer,
                            request,
                            self.action_availability(request),
                        )
                        .clicked()
                        {
                            let _ = self.invoke(request);
                        }
                    }
                });
                if let Some(catalog_id) = catalog_id {
                    if let Some(entry) = self
                        .editor
                        .project()
                        .catalog
                        .iter()
                        .find(|entry| entry.id == catalog_id)
                    {
                        ui.small(format!(
                            "{} · {} / {} · {}",
                            self.localizer.text("hinge-kit-identifiers"),
                            entry.product_id,
                            entry.plate_id.as_deref().unwrap_or("—"),
                            entry.revision
                        ));
                    } else {
                        ui.colored_label(
                            crate::theme_widgets::WARN_INK,
                            self.localizer.text("hinge-missing-catalog"),
                        );
                    }
                }
            });
        }
    }

    pub(super) fn show_hardware_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.hardware_dialog.take() else {
            return;
        };
        let mut valid = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        let title = self.localizer.text(if draft.id.is_some() {
            "hardware-edit"
        } else {
            "hardware-new"
        });
        let mut chrome = draft.chrome.take().expect("hardware modal controller");
        let result = chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &title,
            },
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(self.localizer.text("hardware-name"));
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.name)
                            .id(egui::Id::new("hardware-dialog-name")),
                    );
                });
                valid &= !draft.name.trim().is_empty();
                egui::ComboBox::from_label(self.localizer.text("assembly-parent"))
                    .selected_text(
                        draft
                            .parent
                            .and_then(|id| {
                                self.editor
                                    .project()
                                    .assemblies
                                    .iter()
                                    .find(|a| a.id == id)
                                    .map(|a| a.name.as_str())
                            })
                            .unwrap_or(&self.localizer.text("assembly-root")),
                    )
                    .show_ui(ui, |ui| {
                        combo_option(
                            ui,
                            &mut draft.parent,
                            None,
                            self.localizer.text("assembly-root"),
                        );
                        for a in &self.editor.project().assemblies {
                            combo_option(ui, &mut draft.parent, Some(a.id), &a.name);
                        }
                    });
                ui.label(self.localizer.text("hardware-dimensions"));
                for (axis, field) in ["X", "Y", "Z"].into_iter().zip(&mut draft.dimensions) {
                    valid &= dimension_field(ui, &self.localizer, axis, field, Unit::Mm);
                }
                ui.label(self.localizer.text("hardware-position"));
                for (axis, field) in ["X", "Y", "Z"].into_iter().zip(&mut draft.position) {
                    valid &= super::assembly_ui::assembly_coordinate_field(
                        ui,
                        &self.localizer,
                        axis,
                        field,
                        false,
                    );
                }
                if !valid || draft.error {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text("hardware-invalid"),
                    );
                }
                ((), valid)
            },
        );
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm) {
            let dims = draft
                .dimensions
                .each_ref()
                .map(|d| d.value(Unit::Mm).expect("validated dimension"));
            let position = std::array::from_fn(|i| {
                if draft.position[i].text == draft.original_position[i] {
                    draft.original_world[i]
                } else {
                    super::assembly_ui::coordinate(&draft.position[i])
                        .expect("validated coordinate")
                }
            });
            let pose = Pose::new(position, draft.rotation);
            let result = pose.map_err(|_| ()).and_then(|pose| match draft.id {
                Some(id) => self
                    .editor
                    .edit_placeholder(id, draft.name.clone(), dims, draft.parent, pose)
                    .map(|_| id)
                    .map_err(|_| ()),
                None => self
                    .editor
                    .create_placeholder(draft.name.clone(), dims, draft.parent, pose)
                    .map_err(|_| ()),
            });
            match result {
                Ok(id) => {
                    self.selection.choose(Some(id), false);
                    chrome.close(ctx);
                    return;
                }
                Err(()) => draft.error = true,
            }
        }
        draft.chrome = Some(chrome);
        self.hardware_dialog = Some(draft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_draft_and_escape_do_not_create_hardware() {
        let mut app = DesktopApp::default();
        let before = app.editor.project().clone();
        app.hardware_dialog = Some(HardwareDialog::new(&app, None));
        let ctx = egui::Context::default();
        let draw = |app: &mut DesktopApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| app.show_hardware_dialog(ui.ctx()),
            )
            .drop_without_applying_deltas();
        };
        draw(&mut app, vec![]);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(egui::Id::new("hardware-dialog-name"))
        );
        assert_eq!(app.editor.project(), &before);
        let draft = app.hardware_dialog.as_mut().unwrap();
        draft.name = "Foot".into();
        draft.dimensions[0].text = "0".into();
        draft.dimensions[1].text = "30".into();
        draft.dimensions[2].text = "100".into();
        draw(&mut app, vec![]);
        assert_eq!(app.editor.project(), &before);
        draw(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(app.hardware_dialog.is_none());
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn reference_hardware_removal_is_undoable_and_clears_selection() {
        let mut app = DesktopApp::default();
        let id = app
            .editor
            .create_placeholder(
                "Foot".into(),
                [Length::from_micrometres(30_000); 3],
                None,
                Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            )
            .unwrap();
        app.selection.choose(Some(id), false);
        assert!(app.remove_reference_hardware(id));
        assert!(app.editor.project().hardware.is_empty());
        assert_eq!(app.selection.active, None);
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().hardware[0].id, id);
    }

    #[test]
    fn reference_removal_route_opens_explicit_confirmation_before_edit() {
        let mut app = DesktopApp::default();
        let id = app
            .editor
            .create_placeholder(
                "Foot".into(),
                [Length::from_micrometres(30_000); 3],
                None,
                Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            )
            .unwrap();
        let before = app.editor.project().clone();
        let request = Request::with(A::DeleteHardware, Target::Object(id));
        assert!(app.action_availability(request).is_ok());
        app.invoke(request).unwrap();
        assert!(matches!(
            app.removal_dialog.as_ref().map(|dialog| &dialog.target),
            Some(door_joint_ui::DoorRemoval::Hardware(target)) if *target == id
        ));
        assert_eq!(app.editor.project(), &before);
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_removal_dialog(ui.ctx())
        })
        .drop_without_applying_deltas();
        assert_eq!(app.editor.project(), &before);
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
        assert_eq!(app.editor.project(), &before);
    }
}
