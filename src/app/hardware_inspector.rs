//! Editable inspectors for every kind of hardware, shared by the Design and
//! Hardware workspaces: feet and other hardware, drawer slides, door
//! relationships and catalog models (hinges keep their own inspector).
//!
//! Text fields edit a session draft: Enter or Apply commits one undo step,
//! Escape or Discard reverts, and losing focus never commits. Choices
//! (combos, checkboxes) commit at once, one undo step each.
use crate::actions::{ActionId as A, Request, Target};
use crate::assembly_ui::{field_text, unit_suffix};
use crate::hardware_ui::{FootModel, foot_models, foot_size, resolve_foot};
use crate::icons::Icon;
use crate::theme_widgets as tw;
use crate::*;
use plan_my_cabinet::domain::{CatalogReference, FootShape, HardwareKind};
use plan_my_cabinet::door_joint;
use plan_my_cabinet::edit_drafts::DraftError;
use plan_my_cabinet::hardware_catalog::{self, CatalogKind};

const LABEL: f32 = 88.0;

fn mm_text(length: Length) -> String {
    let um = length.micrometres();
    if um % 1000 == 0 {
        (um / 1000).to_string()
    } else {
        format!("{}", um as f64 / 1000.0)
    }
}

fn short_id(id: Uuid) -> String {
    id.to_string()[..6].to_owned()
}

fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add(egui::Label::new(egui::RichText::new(text.into()).size(12.0).color(tw::MUTED)).wrap());
}

fn warn(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text.into())
                .size(12.0)
                .color(tw::WARN_INK),
        )
        .wrap(),
    );
}

/// One field of a draft: (label key, field index).
type FieldRow<'a> = (&'a str, usize);

impl DesktopApp {
    pub(crate) fn show_fitting_inspector(&mut self, ui: &mut egui::Ui, target: InspectorTarget) {
        match target {
            InspectorTarget::Installation(id) => {
                self.show_selected_installation_inspector(ui, id);
                self.show_hinge_door_link(ui, id);
            }
            InspectorTarget::Slide(id) => self.show_slide_inspector(ui, id),
            InspectorTarget::Hardware(id) => self.show_hardware_item_inspector(ui, id),
            InspectorTarget::Door(id) => self.show_door_inspector(ui, id),
            InspectorTarget::Catalog(id) => self.show_catalog_inspector(ui, id),
            _ => {}
        }
    }

    fn locale(&self) -> Locale {
        if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        }
    }

    /// Draft-backed length fields with Apply/Discard, plus rotation fields
    /// for hardware. Commits (or discards) at the end of the frame.
    pub(crate) fn fitting_fields(
        &mut self,
        ui: &mut egui::Ui,
        target: FittingTarget,
        rows: &[FieldRow],
        rotation: bool,
    ) {
        let unit = self.editor.project().display_unit;
        let locale = self.locale();
        let editable = !self.modal_open();
        let mut apply = false;
        let mut discard = false;
        let localizer = &self.localizer;
        let project_id = self.editor.project().id;
        match self.edit_drafts.fitting(&self.editor, target, unit, locale) {
            Ok(draft) => {
                let pending = if draft.dirty() {
                    draft.preview(&self.editor).err()
                } else {
                    None
                };
                let invalid = |axis: usize| matches!(&pending, Some(DraftError::InvalidField { axis: a, .. }) if *a == axis);
                let key_events = |response: &egui::Response, ui: &egui::Ui| {
                    let (enter, escape) = ui.input(|i| {
                        (
                            i.key_pressed(egui::Key::Enter),
                            i.key_pressed(egui::Key::Escape),
                        )
                    });
                    let focused = response.has_focus() || response.lost_focus();
                    (focused && enter, focused && escape)
                };
                for &(key, index) in rows {
                    let label = localizer.text(key);
                    tw::prop_row(ui, &label, LABEL, |ui| {
                        let width = ui.available_width();
                        let field = &mut draft.fields[index];
                        let mut text = field_text(field);
                        let response = tw::value_field(
                            ui,
                            egui::Id::new(("fitting-field", target, index)),
                            &label,
                            &mut text,
                            width,
                            Some(unit_suffix(unit)),
                            None,
                            editable,
                            invalid(index),
                        );
                        if response.changed() {
                            field.edit(text);
                        }
                        let (enter, escape) = key_events(&response, ui);
                        apply |= enter;
                        discard |= escape;
                    });
                }
                if rotation {
                    let label = localizer.text("hardware-rotation");
                    tw::prop_row(ui, &label, LABEL, |ui| {
                        let width = ((ui.available_width() - 12.0) / 3.0).max(40.0);
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for axis in 0..3 {
                            let mut text = draft.rotation[axis].clone().unwrap_or_else(|| {
                                let v = draft.rotation_degrees[axis];
                                let r = (v * 100.0).round() / 100.0;
                                if r == 0.0 { "0".into() } else { format!("{r}") }
                            });
                            let response = tw::value_field(
                                ui,
                                egui::Id::new(("fitting-rotation", target, axis)),
                                &format!("{label} {}", ["X", "Y", "Z"][axis]),
                                &mut text,
                                width,
                                Some("°"),
                                Some(
                                    [
                                        egui::Color32::from_rgb(196, 69, 58),
                                        egui::Color32::from_rgb(78, 154, 87),
                                        egui::Color32::from_rgb(62, 111, 196),
                                    ][axis],
                                ),
                                editable,
                                matches!(&pending, Some(DraftError::InvalidRotation(a)) if *a == axis),
                            );
                            if response.changed() {
                                draft.rotation[axis] = Some(text);
                            }
                            let (enter, escape) = key_events(&response, ui);
                            apply |= enter;
                            discard |= escape;
                        }
                    });
                }
                if let Some(DraftError::RoundingConsent {
                    axis,
                    entered,
                    rounded_mm,
                }) = &pending
                {
                    let mut args = FluentArgs::new();
                    args.set("entered", entered.as_str());
                    args.set("rounded", rounded_mm.as_str());
                    let label = localizer.format("rounding-confirmation", Some(&args));
                    let axis = *axis;
                    tw::warn_callout().show(ui, |ui| {
                        ui.add_enabled(
                            editable,
                            egui::Checkbox::new(&mut draft.fields[axis].consent, label),
                        );
                    });
                } else if let Some(problem) = &pending {
                    let key = match problem {
                        DraftError::InvalidField { error, .. } => error_key(*error),
                        DraftError::InvalidRotation(_) => "hardware-invalid-rotation",
                        _ => "hardware-invalid",
                    };
                    ui.label(
                        egui::RichText::new(localizer.text(key))
                            .size(11.5)
                            .color(tw::DANGER),
                    );
                }
                if draft.dirty() {
                    ui.add_space(4.0);
                    let valid = draft.preview(&self.editor).is_ok();
                    ui.horizontal(|ui| {
                        ui.add_space(LABEL);
                        if tw::primary_button(
                            ui,
                            &localizer.text("navigation-apply"),
                            editable && valid,
                        )
                        .clicked()
                        {
                            apply = true;
                        }
                        if tw::secondary_button_enabled(
                            ui,
                            &localizer.text("navigation-discard"),
                            editable,
                        )
                        .clicked()
                        {
                            discard = true;
                        }
                    });
                }
            }
            Err(_) => {
                ui.colored_label(tw::WARN_INK, localizer.text("navigation-stale"));
                if ui.button(localizer.text("navigation-discard")).clicked() {
                    discard = true;
                }
            }
        }
        if discard {
            self.edit_drafts.cancel_fitting(project_id, target);
            ui.ctx().request_repaint();
        } else if apply
            && let Some(draft) = self.edit_drafts.existing_fitting_mut(project_id, target)
            && draft.dirty()
        {
            if draft.accept(&mut self.editor).is_err() {
                ui.colored_label(tw::WARN_INK, self.localizer.text("hardware-invalid"));
            }
            ui.ctx().request_repaint();
        }
    }

    /// A row of action buttons (disabled when unavailable).
    fn fitting_actions(&mut self, ui: &mut egui::Ui, requests: &[Request]) {
        ui.add_space(10.0);
        let mut run = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for request in requests {
                if tw::secondary_button_enabled(
                    ui,
                    &request.id.label(&self.localizer),
                    self.action_availability(*request).is_ok(),
                )
                .clicked()
                {
                    run = Some(*request);
                }
            }
        });
        if let Some(request) = run {
            self.invoke_or_report(request);
        }
    }

    fn parent_choice(&mut self, ui: &mut egui::Ui, id: Uuid, current: Option<Uuid>) {
        let project = self.editor.project();
        let root = self.localizer.text("assembly-root");
        let options: Vec<(Option<Uuid>, String)> = std::iter::once((None, root.clone()))
            .chain(
                project
                    .assemblies
                    .iter()
                    .map(|a| (Some(a.id), a.name.clone())),
            )
            .collect();
        let mut chosen = current;
        let editable = !self.modal_open();
        tw::prop_row(ui, &self.localizer.text("assembly-parent"), LABEL, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                egui::ComboBox::from_id_salt(("fitting-parent", id))
                    .width(ui.available_width())
                    .selected_text(
                        options
                            .iter()
                            .find(|(k, _)| *k == current)
                            .map_or(root.clone(), |(_, n)| n.clone()),
                    )
                    .show_ui(ui, |ui| {
                        for (key, name) in &options {
                            combo_option(ui, &mut chosen, *key, name);
                        }
                    });
            });
        });
        if chosen != current {
            let result = self.editor.reparent_objects(&[id], chosen);
            self.report_edit(result);
        }
    }

    // ------------------------------------------------------------ hardware

    fn show_hardware_item_inspector(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let project = self.editor.project();
        let Some(item) = project.hardware.iter().find(|h| h.id == id).cloned() else {
            return;
        };
        let foot = project.foot_spec(&item).cloned();
        let entry = match item.kind {
            HardwareKind::Catalog { catalog_id } => {
                project.catalog.iter().find(|c| c.id == catalog_id).cloned()
            }
            HardwareKind::Placeholder { .. } => None,
        };
        let subline = match (&foot, &entry) {
            (Some(_), Some(entry)) => format!(
                "{} · {}",
                self.localizer.text("hardware-kind-foot"),
                entry.product_id
            ),
            (None, Some(_)) => self.localizer.text("hardware-kind-catalog"),
            _ => self.localizer.text("hardware-kind-other"),
        };
        self.inspector_header(
            ui,
            Icon::Cube,
            &item.name,
            Some(id),
            &format!("h-{}", short_id(id)),
            &subline,
        );
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 6))
            .show(ui, |ui| {
                if foot.is_some() {
                    self.foot_model_choice(ui, id, &item);
                }
                self.parent_choice(ui, id, item.parent_id);
                tw::inspector_heading(ui, &self.localizer.text("hardware-position"), |_| {});
                let target = FittingTarget::Hardware(id);
                self.fitting_fields(
                    ui,
                    target,
                    &[("axis-x", 0), ("axis-y", 1), ("axis-z", 2)],
                    true,
                );
                match &item.kind {
                    HardwareKind::Placeholder { .. } => {
                        tw::inspector_heading(
                            ui,
                            &self.localizer.text("hardware-dimensions"),
                            |_| {},
                        );
                        self.fitting_fields(
                            ui,
                            target,
                            &[("axis-x", 3), ("axis-y", 4), ("axis-z", 5)],
                            false,
                        );
                    }
                    HardwareKind::Catalog { .. } => {
                        if let Some(size) = self.editor.project().hardware_dimensions(&item) {
                            tw::prop_row(
                                ui,
                                &self.localizer.text("hardware-dimensions"),
                                LABEL,
                                |ui| {
                                    let width = ui.available_width();
                                    tw::derived_field(
                                        ui,
                                        &format!(
                                            "{} × {} × {}",
                                            mm_text(size[0]),
                                            mm_text(size[1]),
                                            mm_text(size[2])
                                        ),
                                        "mm",
                                        width,
                                    )
                                },
                            );
                        }
                    }
                }
                if let Some(spec) = &foot {
                    self.foot_details(ui, spec, entry.as_ref());
                    self.below_floor_notice(ui, id, &item);
                }
                self.fitting_actions(
                    ui,
                    &[
                        Request::with(A::DuplicateHardware, Target::Object(id)),
                        Request::with(A::DeleteHardware, Target::Object(id)),
                    ],
                );
            });
    }

    fn foot_model_choice(
        &mut self,
        ui: &mut egui::Ui,
        id: Uuid,
        item: &plan_my_cabinet::domain::Hardware,
    ) {
        let HardwareKind::Catalog { catalog_id } = item.kind else {
            return;
        };
        let models = foot_models(self);
        let current = FootModel::Pinned(catalog_id);
        let mut chosen = current.clone();
        let editable = !self.modal_open();
        let name_of = |m: &FootModel| {
            models
                .iter()
                .find(|(k, _)| k == m)
                .map(|(_, n)| n.clone())
                .unwrap_or_default()
        };
        tw::prop_row(ui, &self.localizer.text("foot-model"), LABEL, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                egui::ComboBox::from_id_salt(("foot-model", id))
                    .width(ui.available_width())
                    .selected_text(name_of(&current))
                    .show_ui(ui, |ui| {
                        for (key, name) in &models {
                            combo_option(ui, &mut chosen, key.clone(), name);
                        }
                    });
            });
        });
        if chosen != current
            && let Some((new_catalog, pin)) = resolve_foot(self, &chosen)
        {
            // Keep the mounting face where it was when the height changes.
            let old = foot_size(self, &current);
            let new = foot_size(self, &chosen);
            let world = plan_my_cabinet::assembly_edit::world_pose(self.editor.project(), id)
                .ok()
                .map(|pose| match (old, new) {
                    (Some(old), Some(new)) => {
                        let lift = (old[2].micrometres() - new[2].micrometres()) as f64 / 1000.0;
                        let shift = pose.rotation.rotate([
                            (old[0].micrometres() - new[0].micrometres()) as f64 / 2000.0,
                            (old[1].micrometres() - new[1].micrometres()) as f64 / 2000.0,
                            lift,
                        ]);
                        Pose::new(
                            std::array::from_fn(|i| pose.translation_mm[i] + shift[i]),
                            pose.rotation,
                        )
                        .unwrap_or(pose)
                    }
                    _ => pose,
                });
            if let Some(world) = world {
                let result = self.editor.edit_foot(
                    id,
                    item.name.clone(),
                    new_catalog,
                    pin,
                    item.parent_id,
                    world,
                );
                self.report_edit(result);
            }
        }
    }

    fn foot_details(
        &self,
        ui: &mut egui::Ui,
        spec: &plan_my_cabinet::domain::FootSpec,
        entry: Option<&CatalogReference>,
    ) {
        let l = &self.localizer;
        let shape = l.text(match spec.shape {
            FootShape::Tapered { .. } => "pdf-foot-tapered",
            FootShape::Post { .. } => "pdf-foot-post",
            FootShape::Frame { .. } => "pdf-foot-frame",
        });
        let mut line = shape;
        if let Some(finish) = &spec.finish {
            line.push_str(&format!(" · {finish}"));
        }
        if spec.adjustment.micrometres() > 0 {
            line.push_str(&format!(
                " · {} {} mm",
                l.text("pdf-foot-adjustment"),
                mm_text(spec.adjustment)
            ));
        }
        ui.add_space(4.0);
        muted(ui, line);
        if let Some(entry) = entry {
            ui.horizontal(|ui| {
                catalog_ui::trust_chip(ui, l, hardware_catalog::trust(entry));
            });
        }
    }

    /// A new foot under a cabinet that sits on the floor ends below it:
    /// offer to raise the cabinet by that much.
    fn below_floor_notice(
        &mut self,
        ui: &mut egui::Ui,
        id: Uuid,
        item: &plan_my_cabinet::domain::Hardware,
    ) {
        let project = self.editor.project();
        let Some(pose) = plan_my_cabinet::assembly_edit::world_pose(project, id).ok() else {
            return;
        };
        let Some(size) = project.hardware_dimensions(item) else {
            return;
        };
        let size = size.map(|v| v.micrometres() as f64 / 1000.0);
        let lowest = (0..8)
            .filter_map(|i| {
                pose.transform_point([
                    if i & 1 == 0 { 0.0 } else { size[0] },
                    if i & 2 == 0 { 0.0 } else { size[1] },
                    if i & 4 == 0 { 0.0 } else { size[2] },
                ])
                .ok()
            })
            .map(|p| p[2])
            .fold(f64::INFINITY, f64::min);
        if lowest > -0.5 {
            return;
        }
        // The top-level assembly the foot belongs to.
        let mut cabinet = item.parent_id;
        while let Some(parent) = cabinet
            .and_then(|c| project.assemblies.iter().find(|a| a.id == c))
            .and_then(|a| a.parent_id)
        {
            cabinet = Some(parent);
        }
        let lift = (-lowest * 1000.0).round() / 1000.0;
        let cabinet_name = cabinet
            .and_then(|c| project.assemblies.iter().find(|a| a.id == c))
            .map_or_else(String::new, |a| a.name.clone());
        let mut args = FluentArgs::new();
        args.set("mm", format!("{lift}"));
        ui.add_space(6.0);
        tw::warn_callout().show(ui, |ui| {
            warn(ui, self.localizer.format("foot-below-floor", Some(&args)));
            if let Some(cabinet) = cabinet {
                args.set("name", cabinet_name.clone());
                if tw::secondary_button_enabled(
                    ui,
                    &self.localizer.format("foot-raise", Some(&args)),
                    !self.modal_open(),
                )
                .clicked()
                {
                    let result = self.editor.transform_selection(
                        &[cabinet],
                        [0.0, 0.0, lift],
                        plan_my_cabinet::units::Quaternion::IDENTITY,
                        [0.0; 3],
                    );
                    self.report_edit(result);
                }
            }
        });
    }

    // --------------------------------------------------------------- doors

    fn show_door_inspector(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let project = self.editor.project();
        let Some(joint) = project.door_joints.iter().find(|j| j.id == id).cloned() else {
            return;
        };
        let name = project
            .board(joint.moving_root_id)
            .map(|b| b.name.clone())
            .or_else(|| {
                project
                    .assemblies
                    .iter()
                    .find(|a| a.id == joint.moving_root_id)
                    .map(|a| a.name.clone())
            })
            .unwrap_or_default();
        let mount_name = project
            .board(joint.mounting_board_id)
            .map_or_else(String::new, |b| b.name.clone());
        let mut args = FluentArgs::new();
        args.set("board", mount_name.clone());
        let subline = self.localizer.format("hardware-door-on", Some(&args));
        let members = door_joint::moving_members(project, joint.moving_root_id);
        // Moving part: the door board or any assembly around it.
        let mut roots: Vec<(Uuid, String)> = Vec::new();
        let door_board = project
            .hinge_installations
            .iter()
            .find(|h| joint.hinge_installation_ids.contains(&h.id))
            .map(|h| h.door_board_id);
        if let Some(board) = door_board.and_then(|b| project.board(b)) {
            roots.push((board.id, board.name.clone()));
            let mut parent = board.parent_id;
            while let Some(p) = parent.and_then(|p| project.assemblies.iter().find(|a| a.id == p)) {
                roots.push((p.id, p.name.clone()));
                parent = p.parent_id;
            }
        }
        let mounts: Vec<(Uuid, String)> = project
            .boards
            .iter()
            .filter(|b| !members.contains(&b.id))
            .map(|b| (b.id, b.name.clone()))
            .collect();
        let hinges: Vec<(Uuid, String, bool)> = project
            .hinge_installations
            .iter()
            .filter(|h| members.contains(&h.door_board_id))
            .map(|h| {
                (
                    h.id,
                    plan_my_cabinet::render::picture::hinge_name(project, h),
                    joint.hinge_installation_ids.contains(&h.id),
                )
            })
            .collect();
        let review = door_joint::needs_review(project, &joint);
        let limit = door_joint::opening_limit(project, &joint).ok();
        self.inspector_header(
            ui,
            Icon::Door,
            &name,
            Some(joint.moving_root_id),
            &format!("d-{}", short_id(id)),
            &subline,
        );
        let editable = !self.modal_open();
        let mut new_root = joint.moving_root_id;
        let mut new_mount = joint.mounting_board_id;
        let mut new_hinges = joint.hinge_installation_ids.clone();
        let mut inspect = None;
        let mut reconfirm = false;
        let mut add_hinge = false;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 6))
            .show(ui, |ui| {
                let l = &self.localizer;
                if review {
                    tw::warn_callout().show(ui, |ui| {
                        warn(ui, l.text("door-review"));
                        if tw::secondary_button_enabled(ui, &l.text("door-reconfirm"), editable)
                            .clicked()
                        {
                            reconfirm = true;
                        }
                    });
                }
                for (key, options, value, salt) in [
                    ("door-moving-part", &roots, &mut new_root, "door-root"),
                    ("door-mount", &mounts, &mut new_mount, "door-mount"),
                ] {
                    tw::prop_row(ui, &l.text(key), LABEL, |ui| {
                        ui.add_enabled_ui(editable, |ui| {
                            egui::ComboBox::from_id_salt((salt, id))
                                .width(ui.available_width())
                                .selected_text(
                                    options
                                        .iter()
                                        .find(|(k, _)| *k == *value)
                                        .map_or_else(String::new, |(_, n)| n.clone()),
                                )
                                .show_ui(ui, |ui| {
                                    for (key, name) in options {
                                        combo_option(ui, value, *key, name);
                                    }
                                });
                        });
                    });
                }
                if let Some(limit) = limit {
                    tw::prop_row(ui, &l.text("door-opening-limit"), LABEL, |ui| {
                        let width = ui.available_width();
                        tw::derived_field(ui, &format!("{limit:.0}"), "°", width)
                    });
                }
                tw::inspector_heading(ui, &l.text("door-hinge-list"), |ui| {
                    if tw::ghost_icon_sized(
                        ui,
                        Icon::Plus,
                        &l.text("door-add-hinge"),
                        tw::MUTED,
                        14.0,
                        22.0,
                        editable,
                        false,
                    )
                    .clicked()
                    {
                        add_hinge = true;
                    }
                });
                for (hinge, name, member) in &hinges {
                    ui.horizontal(|ui| {
                        let mut on = *member;
                        if ui
                            .add_enabled(
                                editable && !(on && new_hinges.len() == 1),
                                egui::Checkbox::without_text(&mut on),
                            )
                            .changed()
                        {
                            if on {
                                new_hinges.push(*hinge);
                            } else {
                                new_hinges.retain(|h| h != hinge);
                            }
                        }
                        if ui
                            .link(egui::RichText::new(name).size(12.5))
                            .on_hover_text(l.text("hardware-open-inspector"))
                            .clicked()
                        {
                            inspect = Some(InspectorTarget::Installation(*hinge));
                        }
                    });
                }
            });
        if reconfirm
            || new_root != joint.moving_root_id
            || new_mount != joint.mounting_board_id
            || new_hinges != joint.hinge_installation_ids
        {
            let result =
                door_joint::reconfigure(&mut self.editor, id, new_root, new_mount, new_hinges);
            self.report_edit(result);
        }
        if add_hinge
            && let Some(board) = door_board
            && let Some(hinge) = self.next_hinge(board)
        {
            let result = door_joint::add_hinge(&mut self.editor, id, hinge);
            self.report_edit(result);
        }
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 0))
            .show(ui, |ui| {
                self.fitting_actions(
                    ui,
                    &[
                        Request::with(A::StartMotion, Target::Door(id)),
                        Request::with(A::DeleteDoor, Target::Door(id)),
                    ],
                );
            });
        if let Some(target) = inspect {
            self.request_inspect(target);
        }
    }

    /// Under a hinge's inspector: the door relationship it belongs to.
    fn show_hinge_door_link(&mut self, ui: &mut egui::Ui, hinge: Uuid) {
        let project = self.editor.project();
        let Some(joint) = project
            .door_joints
            .iter()
            .find(|j| j.hinge_installation_ids.contains(&hinge))
        else {
            return;
        };
        let id = joint.id;
        let name = project
            .board(joint.moving_root_id)
            .map(|b| b.name.clone())
            .or_else(|| {
                project
                    .assemblies
                    .iter()
                    .find(|a| a.id == joint.moving_root_id)
                    .map(|a| a.name.clone())
            })
            .unwrap_or_default();
        let mut open = false;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(self.localizer.text("hardware-door-relationship"))
                            .size(12.0)
                            .color(tw::MUTED),
                    );
                    open = ui.link(format!("{name} ›")).clicked();
                });
            });
        if open {
            self.request_inspect(InspectorTarget::Door(id));
        }
    }

    // ------------------------------------------------------------- catalog

    fn show_catalog_inspector(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let project = self.editor.project();
        let Some(entry) = project.catalog.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        let kind = hardware_catalog::kind(&entry);
        let kind_key = match kind {
            CatalogKind::Hinge => "catalog-kind-hinge",
            CatalogKind::Slide => "catalog-kind-slide",
            CatalogKind::Foot => "catalog-kind-foot",
            CatalogKind::Other => "catalog-kind-other",
        };
        let usage = hardware_catalog::usage(project, id);
        let used_by: Vec<(InspectorTarget, String)> = usage
            .hinges
            .iter()
            .filter_map(|h| project.hinge_installations.iter().find(|i| i.id == *h))
            .map(|h| {
                (
                    InspectorTarget::Installation(h.id),
                    plan_my_cabinet::render::picture::hinge_name(project, h),
                )
            })
            .chain(
                usage
                    .slides
                    .iter()
                    .filter_map(|s| project.slide_installations.iter().find(|i| i.id == *s))
                    .map(|s| {
                        (
                            InspectorTarget::Slide(s.id),
                            plan_my_cabinet::render::picture::slide_name(project, s),
                        )
                    }),
            )
            .chain(
                usage
                    .hardware
                    .iter()
                    .filter_map(|h| project.hardware.iter().find(|i| i.id == *h))
                    .map(|h| (InspectorTarget::Hardware(h.id), h.name.clone())),
            )
            .collect();
        self.inspector_header(
            ui,
            match kind {
                CatalogKind::Hinge => Icon::Hinge,
                CatalogKind::Slide => Icon::Layers,
                _ => Icon::Cube,
            },
            &entry.name,
            None,
            &entry.product_id,
            &self.localizer.text(kind_key),
        );
        let mut inspect = None;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 6))
            .show(ui, |ui| {
                let l = &self.localizer;
                ui.horizontal(|ui| {
                    catalog_ui::trust_chip(ui, l, hardware_catalog::trust(&entry));
                    if let Some(origin) = &entry.origin {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {}",
                                origin.manufacturer, origin.pack_version
                            ))
                            .size(11.5)
                            .color(tw::FAINT),
                        );
                    }
                });
                ui.add_space(6.0);
                match kind {
                    CatalogKind::Hinge | CatalogKind::Other => {
                        hinge_ui::catalog_card(ui, l, &entry);
                    }
                    CatalogKind::Slide => {
                        if let Some(s) = entry.slide() {
                            for (key, value) in [
                                ("slide-length", format!("{} mm", mm_text(s.length))),
                                ("slide-travel", format!("{} mm", mm_text(s.travel))),
                                (
                                    "slide-clearance",
                                    format!(
                                        "{} mm (+{} / -{})",
                                        mm_text(s.clearance),
                                        mm_text(s.clearance_plus),
                                        mm_text(s.clearance_minus)
                                    ),
                                ),
                                ("slide-profile-height", format!("{} mm", mm_text(s.height))),
                            ] {
                                tw::prop_row(ui, &l.text(key), LABEL, |ui| {
                                    ui.label(tw::mono(&value, 12.0).color(tw::TEXT))
                                });
                            }
                            if let Some(rear) = &s.rear_fixing {
                                muted(ui, format!("{}: {rear}", l.text("pdf-slide-rear-fixing")));
                            }
                        }
                    }
                    CatalogKind::Foot => {
                        if let Some(f) = entry.foot() {
                            let size = f.local_size();
                            tw::prop_row(ui, &l.text("hardware-dimensions"), LABEL, |ui| {
                                ui.label(
                                    tw::mono(
                                        format!(
                                            "{} × {} × {} mm",
                                            mm_text(size[0]),
                                            mm_text(size[1]),
                                            mm_text(size[2])
                                        ),
                                        12.0,
                                    )
                                    .color(tw::TEXT),
                                )
                            });
                            self.foot_details(ui, f, None);
                        }
                    }
                }
                if !entry.source.is_empty() && entry.source.starts_with("http") {
                    ui.hyperlink_to(
                        egui::RichText::new(l.text("hinge-source-review"))
                            .size(11.5)
                            .color(tw::ACCENT_DARK),
                        &entry.source,
                    );
                }
                tw::inspector_heading(ui, &l.text("catalog-used-by"), |ui| {
                    ui.label(
                        egui::RichText::new(used_by.len().to_string())
                            .size(11.0)
                            .color(tw::MUTED),
                    );
                });
                if used_by.is_empty() {
                    muted(ui, l.text("catalog-unused"));
                }
                for (target, name) in &used_by {
                    if ui.link(egui::RichText::new(name).size(12.5)).clicked() {
                        inspect = Some(*target);
                    }
                }
            });
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 0))
            .show(ui, |ui| {
                self.fitting_actions(
                    ui,
                    &[
                        Request::with(A::UpdateCatalog, Target::Catalog(id)),
                        Request::with(A::RemoveCatalog, Target::Catalog(id)),
                    ],
                );
            });
        if let Some(target) = inspect {
            self.request_inspect(target);
        }
    }

    /// Under a board's or assembly's Design inspector: the hardware on it,
    /// each opening its editable inspector.
    pub(crate) fn show_hardware_links(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let project = self.editor.project();
        let members = door_joint::moving_members(project, id);
        let mut links: Vec<(InspectorTarget, String)> = Vec::new();
        for joint in &project.door_joints {
            if members.contains(&joint.moving_root_id) || joint.mounting_board_id == id {
                let name = project
                    .board(joint.moving_root_id)
                    .map(|b| b.name.clone())
                    .or_else(|| {
                        project
                            .assemblies
                            .iter()
                            .find(|a| a.id == joint.moving_root_id)
                            .map(|a| a.name.clone())
                    })
                    .unwrap_or_default();
                links.push((
                    InspectorTarget::Door(joint.id),
                    format!("{} · {name}", self.localizer.text("hardware-kind-door")),
                ));
            }
        }
        for hinge in &project.hinge_installations {
            if members.contains(&hinge.door_board_id) || hinge.mounting_board_id == id {
                links.push((
                    InspectorTarget::Installation(hinge.id),
                    plan_my_cabinet::render::picture::hinge_name(project, hinge),
                ));
            }
        }
        for slide in &project.slide_installations {
            let on = members.contains(&slide.drawer_root_id)
                || slide.drawer_root_id == id
                || slide.cabinet_sides.contains(&id)
                || members.iter().any(|m| slide.cabinet_sides.contains(m));
            if on {
                links.push((
                    InspectorTarget::Slide(slide.id),
                    plan_my_cabinet::render::picture::slide_name(project, slide),
                ));
            }
        }
        for item in &project.hardware {
            if item
                .parent_id
                .is_some_and(|p| members.contains(&p) || p == id)
            {
                links.push((InspectorTarget::Hardware(item.id), item.name.clone()));
            }
        }
        if links.is_empty() {
            return;
        }
        let mut inspect = None;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 6))
            .show(ui, |ui| {
                tw::inspector_heading(ui, &self.localizer.text("design-hardware-links"), |ui| {
                    ui.label(
                        egui::RichText::new(links.len().to_string())
                            .size(11.0)
                            .color(tw::MUTED),
                    );
                });
                for (target, name) in &links {
                    if ui
                        .link(egui::RichText::new(format!("{name} ›")).size(12.5))
                        .clicked()
                    {
                        inspect = Some(*target);
                    }
                }
            });
        if let Some(target) = inspect {
            self.request_inspect(target);
        }
    }
}

#[cfg(test)]
mod tests;
