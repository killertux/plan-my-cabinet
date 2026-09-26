use super::*;
use plan_my_cabinet::assembly_edit::{AssemblyEditError, world_pose};
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub(super) enum Operation {
    Group,
    Reparent,
    Ungroup,
    Transform,
    Duplicate,
}

pub(super) struct AssemblyDialog {
    operation: Operation,
    ids: Vec<Uuid>,
    active: Option<Uuid>,
    project_id: Uuid,
    revision: u64,
    focus: bool,
    name: String,
    target: Option<Uuid>,
    pivot: [DimensionDraft; 3],
    translation: [DimensionDraft; 3],
    rotation: [String; 3],
    error: Option<String>,
}

pub(super) fn coordinate(field: &DimensionDraft) -> Option<f64> {
    let parsed = parse_length(&field.text, Unit::Mm).ok()?.conversion;
    (parsed.exact().is_some() || field.consent)
        .then(|| parsed.suggested().micrometres() as f64 / 1000.0)
}

fn angle(text: &str) -> Option<f64> {
    if text.contains('.') && text.contains(',') {
        return None;
    }
    let value = text.replace(',', ".").parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

fn rotation(degrees: [f64; 3]) -> Option<Quaternion> {
    let axis = |axis: usize, degrees: f64| {
        let half = degrees.to_radians() / 2.0;
        let mut vector = [0.0; 3];
        vector[axis] = half.sin();
        Quaternion::normalized(half.cos(), vector[0], vector[1], vector[2]).ok()
    };
    axis(2, degrees[2])?
        .compose(axis(1, degrees[1])?)
        .ok()?
        .compose(axis(0, degrees[0])?)
        .ok()
}

fn hierarchy(project: &Project) -> Vec<(Uuid, usize, bool, String)> {
    fn visit(
        project: &Project,
        parent: Option<Uuid>,
        depth: usize,
        out: &mut Vec<(Uuid, usize, bool, String)>,
        seen: &mut HashSet<Uuid>,
    ) {
        for a in project.assemblies.iter().filter(|a| a.parent_id == parent) {
            if seen.insert(a.id) {
                out.push((a.id, depth, true, a.name.clone()));
                visit(project, Some(a.id), depth + 1, out, seen);
            }
        }
        for b in project.boards.iter().filter(|b| b.parent_id == parent) {
            out.push((b.id, depth, false, b.name.clone()));
        }
        for h in project.hardware.iter().filter(|h| h.parent_id == parent) {
            out.push((h.id, depth, false, h.name.clone()));
        }
    }
    let mut out = Vec::new();
    visit(project, None, 0, &mut out, &mut HashSet::new());
    out
}

impl AssemblyDialog {
    pub(super) fn new(app: &DesktopApp, operation: Operation) -> Self {
        let ids: Vec<_> = app.selection.ids.iter().copied().collect();
        let origin = app
            .selection
            .active
            .and_then(|id| world_pose(app.editor.project(), id).ok())
            .map_or([0.0; 3], |pose| pose.translation_mm);
        let field = |value: f64| DimensionDraft {
            text: format!("{value:.3}"),
            consent: false,
        };
        Self {
            operation,
            ids,
            active: app.selection.active,
            project_id: app.editor.project().id,
            revision: app.editor.project().revision,
            focus: true,
            name: String::new(),
            target: None,
            pivot: origin.map(field),
            translation: [field(0.0), field(0.0), field(0.0)],
            rotation: ["0".into(), "0".into(), "0".into()],
            error: None,
        }
    }
}

impl DesktopApp {
    pub(super) fn show_hierarchy(&mut self, ui: &mut egui::Ui) {
        ui.label(self.localizer.text("assembly-hierarchy"));
        let modal = self.modal_open();
        for (id, depth, assembly, name) in hierarchy(self.editor.project()) {
            let visible = self.selection.visible(self.editor.project(), id);
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 15.0);
                let label = format!(
                    "{} {name}{}",
                    self.localizer.text(if assembly {
                        "assembly-kind"
                    } else if self.editor.project().hardware.iter().any(|h| h.id == id) {
                        "hardware-kind"
                    } else {
                        "board-kind"
                    }),
                    if visible {
                        String::new()
                    } else {
                        format!(" ({})", self.localizer.text("assembly-hidden"))
                    }
                );
                if ui
                    .add_enabled(
                        !modal,
                        egui::Button::new(label).selected(self.selection.ids.contains(&id)),
                    )
                    .clicked()
                {
                    let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                    self.selection.choose(Some(id), additive);
                }
                if ui
                    .add_enabled(
                        !modal,
                        egui::Button::new(self.localizer.text(if visible {
                            "assembly-hide"
                        } else {
                            "assembly-reveal"
                        })),
                    )
                    .clicked()
                {
                    if visible {
                        self.selection.hidden.insert(id);
                    } else {
                        self.selection.reveal(self.editor.project(), id);
                    }
                }
            });
        }
        let active_assembly = self
            .selection
            .active
            .is_some_and(|id| self.editor.project().assemblies.iter().any(|a| a.id == id));
        ui.horizontal_wrapped(|ui| {
            for (operation, key, enabled) in [
                (
                    Operation::Group,
                    "assembly-group",
                    !self.selection.ids.is_empty(),
                ),
                (
                    Operation::Reparent,
                    "assembly-reparent",
                    !self.selection.ids.is_empty(),
                ),
                (Operation::Ungroup, "assembly-ungroup", active_assembly),
                (Operation::Duplicate, "assembly-duplicate", active_assembly),
                (
                    Operation::Transform,
                    "assembly-transform",
                    !self.selection.ids.is_empty(),
                ),
            ] {
                if ui
                    .add_enabled(
                        !modal && enabled,
                        egui::Button::new(self.localizer.text(key)),
                    )
                    .clicked()
                {
                    self.assembly_dialog = Some(AssemblyDialog::new(self, operation));
                }
            }
        });
    }

    pub(super) fn show_assembly_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.assembly_dialog.take() else {
            return;
        };
        let mut cancel = false;
        let mut accept = false;
        let mut valid = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        let modal = egui::Modal::new(egui::Id::new("assembly-dialog")).show(ctx, |ui| {
            let title = match draft.operation {
                Operation::Group => "assembly-group",
                Operation::Reparent => "assembly-reparent",
                Operation::Ungroup => "assembly-ungroup",
                Operation::Transform => "assembly-transform",
                Operation::Duplicate => "assembly-duplicate",
            };
            ui.heading(self.localizer.text(title));
            if matches!(draft.operation, Operation::Group) {
                ui.horizontal(|ui| {
                    ui.label(self.localizer.text("assembly-name"));
                    let response = ui.text_edit_singleline(&mut draft.name);
                    if draft.focus {
                        response.request_focus();
                    }
                });
                valid &= !draft.name.trim().is_empty();
            }
            if matches!(draft.operation, Operation::Group | Operation::Reparent) {
                let combo = egui::ComboBox::from_label(self.localizer.text("assembly-parent"))
                    .selected_text(
                        draft
                            .target
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
                            &mut draft.target,
                            None,
                            self.localizer.text("assembly-root"),
                        );
                        for a in &self.editor.project().assemblies {
                            combo_option(ui, &mut draft.target, Some(a.id), &a.name);
                        }
                    });
                if draft.focus && matches!(draft.operation, Operation::Reparent) {
                    combo.response.request_focus();
                }
            }
            if matches!(draft.operation, Operation::Group | Operation::Transform) {
                ui.label(self.localizer.text("assembly-pivot"));
                for (axis, field) in ["X", "Y", "Z"].into_iter().zip(&mut draft.pivot) {
                    valid &= assembly_coordinate_field(
                        ui,
                        &self.localizer,
                        axis,
                        field,
                        draft.focus
                            && matches!(draft.operation, Operation::Transform)
                            && axis == "X",
                    );
                }
            }
            if matches!(draft.operation, Operation::Transform | Operation::Duplicate) {
                ui.label(self.localizer.text("assembly-translation"));
                for (axis, field) in ["X", "Y", "Z"].into_iter().zip(&mut draft.translation) {
                    valid &= assembly_coordinate_field(ui, &self.localizer, axis, field, false);
                }
            }
            if matches!(draft.operation, Operation::Transform) {
                ui.label(self.localizer.text("assembly-rotation"));
                for (axis, text) in ["X", "Y", "Z"].into_iter().zip(&mut draft.rotation) {
                    ui.horizontal(|ui| {
                        ui.label(format!("{axis} (°)"));
                        ui.text_edit_singleline(text);
                    });
                    valid &= angle(text).is_some();
                }
                valid &= draft.rotation.iter().all(|v| angle(v).is_some())
                    && rotation(draft.rotation.each_ref().map(|v| angle(v).unwrap_or(0.0)))
                        .is_some();
            }
            if !valid || draft.error.is_some() {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    draft
                        .error
                        .as_deref()
                        .unwrap_or(&self.localizer.text("assembly-invalid")),
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
            let pivot = draft.pivot.each_ref().map(|v| coordinate(v).unwrap_or(0.0));
            let result = match draft.operation {
                Operation::Group => self
                    .editor
                    .group_objects(&draft.ids, draft.target, draft.name.clone(), pivot)
                    .map(|id| {
                        self.selection.choose(Some(id), false);
                    }),
                Operation::Reparent => self
                    .editor
                    .reparent_objects(&draft.ids, draft.target)
                    .map(|_| ()),
                Operation::Ungroup => self
                    .editor
                    .ungroup_assembly(draft.active.expect("active assembly at dialog opening"))
                    .map(|_| {
                        self.selection.choose(None, false);
                    }),
                Operation::Transform => {
                    let delta = draft
                        .translation
                        .each_ref()
                        .map(|v| coordinate(v).unwrap_or(0.0));
                    let angles = draft.rotation.each_ref().map(|v| angle(v).unwrap_or(0.0));
                    self.editor
                        .transform_selection(&draft.ids, delta, rotation(angles).unwrap(), pivot)
                        .map(|_| ())
                }
                Operation::Duplicate => {
                    let offset = draft
                        .translation
                        .each_ref()
                        .map(|v| coordinate(v).unwrap_or(0.0));
                    self.editor
                        .duplicate_assembly(
                            draft.active.expect("active assembly at dialog opening"),
                            offset,
                        )
                        .map(|id| self.selection.choose(Some(id), false))
                }
            };
            match result {
                Ok(()) => return,
                Err(plan_my_cabinet::commands::EditError::Command(AssemblyEditError::Cycle(_))) => {
                    draft.error = Some(self.localizer.text("assembly-cycle"))
                }
                Err(_) => draft.error = Some(self.localizer.text("assembly-invalid")),
            }
        }
        self.assembly_dialog = Some(draft);
    }
}

pub(super) fn assembly_coordinate_field(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    axis: &str,
    field: &mut DimensionDraft,
    focus: bool,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(format!("{axis} (mm)"));
        let response = ui.text_edit_singleline(&mut field.text);
        if focus {
            response.request_focus();
        }
        if response.changed() {
            field.consent = false;
        }
    });
    if let Ok(parsed) = parse_length(&field.text, Unit::Mm)
        && matches!(parsed.conversion, Conversion::NeedsConfirmation(_))
    {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("entered", field.text.as_str());
        args.set(
            "rounded",
            format_length(
                parsed.conversion.suggested(),
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
            localizer.format("rounding-confirmation", Some(&args)),
        );
    }
    coordinate(field).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};

    fn app_with_board() -> (DesktopApp, Uuid) {
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Wood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let board = app
            .editor
            .create_board(NewBoard {
                name: "Side".into(),
                material_id,
                length: Length::from_micrometres(100_000),
                width: Length::from_micrometres(50_000),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        (app, board)
    }

    #[test]
    fn hierarchy_selection_and_group_transform_reparent_ungroup_are_atomic() {
        let (mut app, board) = app_with_board();
        app.selection.choose(Some(board), false);
        let group = app
            .editor
            .group_objects(&[board], None, "Cabinet", [0.0; 3])
            .unwrap();
        let rows = hierarchy(app.editor.project());
        assert_eq!(
            rows.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(),
            vec![(group, 0), (board, 1)]
        );
        app.selection.choose(Some(group), false);
        app.selection.choose(Some(board), true);
        let before = world_pose(app.editor.project(), board).unwrap();
        app.editor
            .transform_selection(
                &app.selection.ids.iter().copied().collect::<Vec<_>>(),
                [100.0, 0.0, 0.0],
                Quaternion::IDENTITY,
                [0.0; 3],
            )
            .unwrap();
        assert!(
            (world_pose(app.editor.project(), board)
                .unwrap()
                .translation_mm[0]
                - before.translation_mm[0]
                - 100.0)
                .abs()
                < 1e-6
        );
        app.editor.undo().unwrap();
        assert_eq!(world_pose(app.editor.project(), board).unwrap(), before);
        app.editor.redo().unwrap();
        let unchanged = app.editor.project().clone();
        assert!(app.editor.reparent_objects(&[group], Some(group)).is_err());
        assert_eq!(app.editor.project(), &unchanged);
        app.editor.reparent_objects(&[board], None).unwrap();
        app.editor.reparent_objects(&[board], Some(group)).unwrap();
        app.editor.ungroup_assembly(group).unwrap();
        assert_eq!(app.editor.project().boards[0].parent_id, None);
        assert!(
            (world_pose(app.editor.project(), board)
                .unwrap()
                .translation_mm[0]
                - 100.0)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn modal_focus_invalid_and_escape_leave_project_untouched() {
        let (mut app, board) = app_with_board();
        app.selection.choose(Some(board), false);
        let initial = app.editor.project().clone();
        app.assembly_dialog = Some(AssemblyDialog::new(&app, Operation::Transform));
        let ctx = egui::Context::default();
        let draw = |app: &mut DesktopApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| app.show_assembly_dialog(ui.ctx()),
            )
            .drop_without_applying_deltas();
        };
        draw(&mut app, vec![]);
        assert!(ctx.memory(|m| m.focused()).is_some());
        app.assembly_dialog.as_mut().unwrap().translation[0].text = "invalid".into();
        draw(&mut app, vec![]);
        assert_eq!(app.editor.project(), &initial);
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
        assert!(app.assembly_dialog.is_none());
        assert_eq!(app.editor.project(), &initial);
    }

    #[test]
    fn hidden_group_keeps_unallocated_board_demand_and_list_selection() {
        let (mut app, board) = app_with_board();
        let group = app
            .editor
            .group_objects(&[board], None, "Body", [0.0; 3])
            .unwrap();
        let revision = app.editor.project().revision;
        let snapshot = app.editor.project().clone();
        app.selection.hidden.insert(group);
        let rows = hierarchy(app.editor.project());
        assert!(rows.iter().any(|row| row.0 == board));
        assert!(!app.selection.visible(app.editor.project(), board));
        assert_eq!(app.editor.project().boards.len(), 1);
        assert!(app.editor.project().allocations.is_empty());
        app.selection.choose(Some(board), false); // Object-list row remains selectable.
        assert_eq!(app.selection.active, Some(board));
        app.selection.reveal(app.editor.project(), board);
        assert!(app.selection.visible(app.editor.project(), board));
        assert_eq!(app.editor.project().revision, revision);
        assert_eq!(app.editor.project(), &snapshot);
    }
}
