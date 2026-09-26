//! Dimensioned reference hardware editor. Draft fields never mutate the project.
use super::*;
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
    focus: bool,
    error: bool,
}

impl HardwareDialog {
    fn new(app: &DesktopApp, id: Option<Uuid>) -> Self {
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
            focus: true,
            error: false,
        }
    }
}

impl DesktopApp {
    pub(super) fn show_hardware_list(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label(self.localizer.text("hardware-list"));
        let modal = self.modal_open();
        if ui
            .add_enabled(
                !modal,
                egui::Button::new(self.localizer.text("hardware-new")),
            )
            .clicked()
        {
            self.hardware_dialog = Some(HardwareDialog::new(self, None));
        }
        let items: Vec<_> = self
            .editor
            .project()
            .hardware
            .iter()
            .map(|h| match h.kind {
                HardwareKind::Placeholder { dimensions } => {
                    (h.id, h.name.clone(), Some(dimensions))
                }
                HardwareKind::Catalog { .. } => (h.id, h.name.clone(), None),
            })
            .collect();
        for (id, name, dims) in items {
            ui.horizontal(|ui| {
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
                        egui::Button::new(text).selected(self.selection.ids.contains(&id)),
                    )
                    .clicked()
                {
                    self.selection.choose(
                        Some(id),
                        ui.input(|i| i.modifiers.command || i.modifiers.shift),
                    );
                }
                if ui
                    .add_enabled(
                        !modal && dims.is_some(),
                        egui::Button::new(self.localizer.text("hardware-edit")),
                    )
                    .clicked()
                {
                    self.hardware_dialog = Some(HardwareDialog::new(self, Some(id)));
                }
                if ui
                    .add_enabled(
                        !modal && dims.is_some(),
                        egui::Button::new(self.localizer.text("hardware-duplicate")),
                    )
                    .clicked()
                    && let Ok(copy) = self.editor.duplicate_placeholder(id)
                {
                    self.selection.choose(Some(copy), false);
                }
            });
        }
    }

    pub(super) fn show_hardware_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.hardware_dialog.take() else {
            return;
        };
        let mut cancel = false;
        let mut accept = false;
        let mut valid = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        let modal = egui::Modal::new(egui::Id::new("hardware-dialog")).show(ctx, |ui| {
            ui.heading(self.localizer.text(if draft.id.is_some() {
                "hardware-edit"
            } else {
                "hardware-new"
            }));
            ui.horizontal(|ui| {
                ui.label(self.localizer.text("hardware-name"));
                let response = ui.text_edit_singleline(&mut draft.name);
                if draft.focus {
                    response.request_focus();
                }
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
            ui.horizontal(|ui| {
                cancel = ui.button(self.localizer.text("cancel")).clicked();
                accept = ui
                    .add_enabled(valid, egui::Button::new(self.localizer.text("confirm")))
                    .clicked();
            });
        });
        draft.focus = false;
        if cancel || modal.should_close() {
            return;
        }
        if accept {
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
                    return;
                }
                Err(()) => draft.error = true,
            }
        }
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
}
