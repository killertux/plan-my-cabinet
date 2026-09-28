//! Dimensioned reference hardware editor. Draft fields never mutate the project.
use crate::actions::{ActionId as A, Argument, Request, Target};
use crate::icons::Icon;
use crate::theme_widgets as tw;
use crate::*;
use plan_my_cabinet::assembly_edit::world_pose;
use plan_my_cabinet::domain::HardwareKind;

pub(crate) struct HardwareDialog {
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
    chrome: ModalChrome,
}

/// Three-decimal draft text without needless zeros ("30", "12.5").
fn trim3(value: f64) -> String {
    let text = format!("{:.3}", value + 0.0);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.to_owned()
    }
}

const AXIS_COLORS: [egui::Color32; 3] = [
    egui::Color32::from_rgb(196, 69, 58),
    egui::Color32::from_rgb(78, 154, 87),
    egui::Color32::from_rgb(62, 111, 196),
];

/// X/Y/Z mm fields in one row (axis-coloured strokes). Parse errors and the
/// rounding consent appear below the row. Returns whether all three are valid.
fn axis_fields(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    id: &'static str,
    fields: &mut [DimensionDraft; 3],
    positive: bool,
) -> bool {
    let parse = |field: &DimensionDraft| {
        parse_length(&field.text, Unit::Mm).and_then(|v| {
            if positive {
                dimension(v.conversion).map_err(InputError::Unit)
            } else {
                Ok(v.conversion)
            }
        })
    };
    let gap = 8.0;
    let width = ((ui.available_width() - 2.0 * gap) / 3.0).max(60.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (index, field) in fields.iter_mut().enumerate() {
            let invalid = !field.text.is_empty() && parse(field).is_err();
            let axis = ["X", "Y", "Z"][index];
            if tw::value_field(
                ui,
                egui::Id::new((id, index)),
                axis,
                &mut field.text,
                width,
                Some("mm"),
                Some(AXIS_COLORS[index]),
                true,
                invalid,
            )
            .changed()
            {
                field.consent = false;
            }
        }
    });
    let mut valid = true;
    for (index, field) in fields.iter_mut().enumerate() {
        let axis = ["X", "Y", "Z"][index];
        match parse(field) {
            Ok(Conversion::NeedsConfirmation(value)) => {
                let mut args = FluentArgs::new();
                args.set("entered", field.text.as_str());
                args.set(
                    "rounded",
                    format_length(
                        value,
                        Unit::Mm,
                        if localizer.language() == Language::En {
                            Locale::En
                        } else {
                            Locale::PtBr
                        },
                        3,
                    ),
                );
                ui.checkbox(
                    &mut field.consent,
                    format!(
                        "{axis}: {}",
                        localizer.format("rounding-confirmation", Some(&args))
                    ),
                );
            }
            Ok(_) => {}
            Err(error) => {
                if !field.text.is_empty() {
                    ui.label(
                        egui::RichText::new(format!(
                            "{axis}: {}",
                            localizer.text(error_key(error))
                        ))
                        .size(11.5)
                        .color(tw::DANGER),
                    );
                }
            }
        }
        valid &= if positive {
            parse(field).is_ok() && field.value(Unit::Mm).is_ok()
        } else {
            super::assembly_ui::coordinate(field).is_some()
        };
    }
    valid
}

impl HardwareDialog {
    pub(crate) fn new(app: &DesktopApp, id: Option<Uuid>) -> Self {
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
        let position =
            std::array::from_fn(|i| field(trim3(world.map_or(0.0, |p| p.translation_mm[i]))));
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
                field(dims.map_or(String::new(), |d| trim3(d[i].micrometres() as f64 / 1000.0)))
            }),
            position,
            original_position,
            original_world: world.map_or([0.0; 3], |p| p.translation_mm),
            rotation: world.map_or(Quaternion::IDENTITY, |p| p.rotation),
            error: false,
            chrome: ModalChrome::new(egui::Id::new("hardware-dialog"))
                .first_focus(egui::Id::new("hardware-dialog-name")),
        }
    }
}

impl DesktopApp {
    pub(crate) fn remove_reference_hardware(&mut self, id: Uuid) -> bool {
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

    /// Secondary Hardware section for dimensioned reference items. Creation is
    /// also in the Doors "+" menu, so the section only appears once it has rows.
    pub(crate) fn show_hardware_list(&mut self, ui: &mut egui::Ui) {
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
        if items.is_empty() {
            return;
        }
        let modal = self.modal_open();
        let mut run = None;
        tw::divider(ui);
        let open = egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 14,
                right: 10,
                top: 0,
                bottom: 0,
            })
            .show(ui, |ui| {
                let (open, ()) = tw::collapsible_section_bar(
                    ui,
                    egui::Id::new("design-hardware-section"),
                    &self.localizer.text("hardware-list"),
                    items.len(),
                    |ui| {
                        let request = Request::new(A::NewHardware);
                        if tw::ghost_icon_sized(
                            ui,
                            Icon::Plus,
                            &A::NewHardware.label(&self.localizer),
                            tw::MUTED,
                            15.0,
                            24.0,
                            self.action_availability(request).is_ok(),
                            false,
                        )
                        .clicked()
                        {
                            run = Some(request);
                        }
                    },
                );
                open
            })
            .inner;
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        if open {
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: 6,
                    right: 6,
                    top: 0,
                    bottom: 12,
                })
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for (id, name, dims, catalog_id) in items {
                        let selected = self.selection.ids.contains(&id);
                        let active = self.selection.active == Some(id);
                        let catalog = catalog_id.map(|catalog_id| {
                            self.editor
                                .project()
                                .catalog
                                .iter()
                                .find(|entry| entry.id == catalog_id)
                                .map(|entry| entry.product_id.clone())
                        });
                        let detail = match (dims, &catalog) {
                            (Some(dims), _) => dims
                                .map(|d| assembly_ui::short_length(d, locale))
                                .join(" × "),
                            (None, Some(Some(product))) => product.clone(),
                            _ => String::new(),
                        };
                        let edit = Request::with(A::EditHardware, Target::Object(id));
                        let duplicate = Request::with(A::DuplicateHardware, Target::Object(id));
                        let delete = Request::with(A::DeleteHardware, Target::Object(id));
                        let (response, ()) = tw::list_row(
                            ui,
                            egui::Id::new(("hardware-reference-row", id)),
                            28.0,
                            if active {
                                tw::RowState::Active
                            } else if selected {
                                tw::RowState::Selected
                            } else {
                                tw::RowState::Normal
                            },
                            !modal,
                            &name,
                            |ui| {
                                let hovered = ui.rect_contains_pointer(ui.max_rect());
                                ui.spacing_mut().item_spacing.x = 7.0;
                                ui.add_space(2.0);
                                ui.add(crate::icons::icon(
                                    if dims.is_some() {
                                        Icon::Cube
                                    } else {
                                        Icon::Hinge
                                    },
                                    if active { tw::ACCENT } else { tw::MUTED },
                                    14.0,
                                ));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        if matches!(catalog, Some(None)) {
                                            ui.add(crate::icons::icon(
                                                Icon::Warning,
                                                tw::WARN,
                                                13.0,
                                            ))
                                            .on_hover_text(
                                                self.localizer.text("hinge-missing-catalog"),
                                            );
                                        }
                                        if hovered && !modal && dims.is_some() {
                                            for (request, icon, color) in [
                                                (delete, Icon::Trash, tw::DANGER),
                                                (duplicate, Icon::Duplicate, tw::SECONDARY),
                                                (edit, Icon::Sliders, tw::SECONDARY),
                                            ] {
                                                if tw::ghost_icon_sized(
                                                    ui,
                                                    icon,
                                                    &request.id.label(&self.localizer),
                                                    color,
                                                    13.0,
                                                    22.0,
                                                    self.action_availability(request).is_ok(),
                                                    false,
                                                )
                                                .clicked()
                                                {
                                                    run = Some(request);
                                                }
                                            }
                                        } else if !detail.is_empty() {
                                            ui.label(tw::mono(&detail, 11.0).color(tw::FAINT));
                                        }
                                        ui.with_layout(
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(&name)
                                                            .size(13.0)
                                                            .color(if active {
                                                                tw::ACCENT_INK
                                                            } else {
                                                                tw::TEXT_2
                                                            }),
                                                    )
                                                    .truncate()
                                                    .selectable(false),
                                                );
                                            },
                                        );
                                    },
                                );
                            },
                        );
                        let response = response.on_hover_text(if detail.is_empty() {
                            name.clone()
                        } else {
                            format!("{name} · {detail}")
                        });
                        response.context_menu(|ui| {
                            let has_selection = !self.selection.ids.is_empty();
                            for (request, enabled) in [
                                (edit, dims.is_some()),
                                (duplicate, dims.is_some()),
                                (Request::new(A::Group), has_selection),
                                (Request::new(A::Reparent), has_selection),
                                (delete, dims.is_some()),
                            ] {
                                let label = match request.id {
                                    A::Group => self.localizer.text("assembly-group"),
                                    A::Reparent => self.localizer.text("assembly-parent"),
                                    _ => request.id.label(&self.localizer),
                                };
                                if ui
                                    .add_enabled(
                                        enabled && self.action_availability(request).is_ok(),
                                        egui::Button::new(label),
                                    )
                                    .clicked()
                                {
                                    run = Some(request);
                                    ui.close();
                                }
                            }
                        });
                        if run.is_none() && response.clicked() {
                            let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                            run = Some(
                                Request::with(A::SelectObject, Target::Object(id))
                                    .argument(Argument::Additive(additive)),
                            );
                        } else if run.is_none() && response.double_clicked() && dims.is_some() {
                            run = Some(edit);
                        }
                    }
                });
        }
        if let Some(request) = run {
            self.invoke_or_report(request);
        }
    }

    pub(crate) fn show_hardware_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_hardware() else {
            return;
        };
        let mut valid = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        let title = self.localizer.text(if draft.id.is_some() {
            "hardware-edit"
        } else {
            "hardware-new"
        });
        let mut chrome = draft.chrome.detach();
        let result = chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &title,
            },
            |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                let gap = 12.0;
                let half = ((ui.available_width() - gap) / 2.0).max(80.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.allocate_ui_with_layout(
                        egui::vec2(half, 52.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            hinge_ui::field_label(ui, &self.localizer.text("hardware-name"));
                            ui.add(
                                egui::TextEdit::singleline(&mut draft.name)
                                    .id(egui::Id::new("hardware-dialog-name"))
                                    .desired_width(half),
                            );
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(half, 52.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            hinge_ui::field_label(ui, &self.localizer.text("assembly-parent"));
                            egui::ComboBox::from_id_salt("hardware-dialog-parent")
                                .width(half)
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
                        },
                    );
                });
                valid &= !draft.name.trim().is_empty();
                ui.add_space(10.0);
                hinge_ui::field_label(ui, &self.localizer.text("hardware-dimensions"));
                valid &= axis_fields(
                    ui,
                    &self.localizer,
                    "hardware-dialog-dimension",
                    &mut draft.dimensions,
                    true,
                );
                ui.add_space(10.0);
                hinge_ui::field_label(ui, &self.localizer.text("hardware-position"));
                valid &= axis_fields(
                    ui,
                    &self.localizer,
                    "hardware-dialog-position",
                    &mut draft.position,
                    false,
                );
                if !valid || draft.error {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(self.localizer.text("hardware-invalid"))
                            .size(11.5)
                            .color(tw::DANGER),
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
        draft.chrome = chrome;
        self.modals.set_hardware(Some(draft));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_draft_and_escape_do_not_create_hardware() {
        let mut app = DesktopApp::default();
        let before = app.editor.project().clone();
        app.modals
            .set_hardware(Some(HardwareDialog::new(&app, None)));
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
        let draft = app.modals.hardware_mut().unwrap();
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
        assert!(app.modals.hardware().is_none());
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
            app.modals.removal().map(|dialog| &dialog.target),
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
        assert!(app.modals.removal().is_none());
        assert_eq!(app.editor.project(), &before);
    }
}
