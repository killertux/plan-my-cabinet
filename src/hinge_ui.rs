//! Board-local hinge references. The modal owns an uncommitted draft.
use super::*;
use plan_my_cabinet::domain::{BoardEdge, BoardFace, HingeInstallation, HingeMountingSide};
use plan_my_cabinet::hinge_installation::{self, InstallationIssue, InstallationStatus};

pub(super) struct HingeDialog {
    id: Option<Uuid>,
    project_id: Uuid,
    revision: u64,
    door: Option<Uuid>,
    mount: Option<Uuid>,
    catalog: Option<Uuid>,
    side: HingeMountingSide,
    values: [String; 4],
    focus: bool,
    error: bool,
}

fn mm(value: Length) -> String {
    format!("{:.3}", value.micrometres() as f64 / 1000.0)
}

fn distance(text: &str) -> Option<Length> {
    let value = parse_length(text, Unit::Mm).ok()?.conversion.exact()?;
    (value.micrometres() >= 0).then_some(value)
}

impl HingeDialog {
    fn new(app: &DesktopApp, id: Option<Uuid>) -> Self {
        let project = app.editor.project();
        let old = id.and_then(|id| project.hinge_installations.iter().find(|i| i.id == id));
        let boards = &project.boards;
        Self {
            id,
            project_id: project.id,
            revision: project.revision,
            door: old
                .map(|i| i.door_board_id)
                .or_else(|| boards.first().map(|b| b.id)),
            mount: old
                .map(|i| i.mounting_board_id)
                .or_else(|| boards.get(1).map(|b| b.id)),
            catalog: old.map(|i| i.catalog_id).or_else(|| {
                project
                    .catalog
                    .iter()
                    .find(|c| hardware_catalog::is_verified(c))
                    .map(|c| c.id)
            }),
            side: old.map_or(
                HingeMountingSide {
                    door_edge: BoardEdge::MinX,
                    door_face: BoardFace::MinZ,
                    mount_front_edge: BoardEdge::MinX,
                    mount_face: BoardFace::MinZ,
                },
                |i| i.side,
            ),
            values: old.map_or(["50".into(), "50".into(), "3".into(), "15".into()], |i| {
                [
                    mm(i.door_y),
                    mm(i.mount_y),
                    mm(i.cup_edge_setback),
                    mm(i.overlay),
                ]
            }),
            focus: true,
            error: false,
        }
    }

    fn proposed(&self, app: &DesktopApp) -> Option<HingeInstallation> {
        let project = app.editor.project();
        let (door, mount, catalog) = (self.door?, self.mount?, self.catalog?);
        if door == mount
            || !project.boards.iter().any(|b| b.id == door)
            || !project.boards.iter().any(|b| b.id == mount)
            || !project
                .catalog
                .iter()
                .any(|c| c.id == catalog && hardware_catalog::is_verified(c))
        {
            return None;
        }
        let [door_y, mount_y, k, overlay] = self.values.each_ref().map(|v| distance(v));
        Some(HingeInstallation {
            id: self.id.unwrap_or_else(Uuid::new_v4),
            door_board_id: door,
            mounting_board_id: mount,
            catalog_id: catalog,
            side: self.side,
            door_y: door_y?,
            mount_y: mount_y?,
            cup_edge_setback: k?,
            overlay: overlay?,
        })
    }
}

pub(super) fn issue_key(issue: &InstallationIssue) -> &'static str {
    match issue {
        InstallationIssue::MissingPart(_) => "hinge-missing-part",
        InstallationIssue::MissingCatalog(_) | InstallationIssue::MissingVerifiedCatalog => {
            "hinge-missing-catalog"
        }
        InstallationIssue::UnsupportedThickness => "hinge-thickness-warning",
        InstallationIssue::UnsupportedOverlay => "hinge-overlay-warning",
        InstallationIssue::CupOutsideDoor => "hinge-cup-warning",
        InstallationIssue::PlateOutsideMount => "hinge-plate-warning",
    }
}

fn status_ui(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    status: &InstallationStatus,
    proposed: &HingeInstallation,
) {
    for issue in &status.issues {
        ui.colored_label(egui::Color32::YELLOW, localizer.text(issue_key(issue)));
    }
    if let Some(r) = &status.references {
        let to_mm = |v: i128| v as f64 / 1000.0;
        ui.label(format!(
            "{}: {} / {}",
            localizer.text("hinge-kit"),
            r.product_id,
            r.plate_id
        ));
        ui.label(format!(
            "{}: Ø35 × 11.3 mm; K={} mm; X={:.3}, Y={:.3} mm",
            localizer.text("hinge-cup-reference"),
            mm(proposed.cup_edge_setback),
            to_mm(r.cup_center_um[0]),
            to_mm(r.cup_center_um[1])
        ));
        ui.label(format!(
            "{}: H=0; 32 mm / 37 mm; X={:.3}, Y={:.3} / {:.3} mm",
            localizer.text("hinge-plate-reference"),
            to_mm(r.plate_hole_centers_um[0][0]),
            to_mm(r.plate_hole_centers_um[0][1]),
            to_mm(r.plate_hole_centers_um[1][1])
        ));
        ui.label(format!(
            "{}: {} — {} ({} {}, PDF {})",
            localizer.text("hinge-source"),
            r.attribution,
            r.source,
            localizer.text("hinge-printed-page"),
            r.printed_page,
            r.pdf_page
        ));
        ui.hyperlink_to(localizer.text("hinge-source"), hardware_catalog::SOURCE_URL);
    }
    ui.colored_label(
        egui::Color32::YELLOW,
        localizer.text("hinge-fasteners-unavailable"),
    );
    ui.small(localizer.text("hinge-provisional"));
}

impl DesktopApp {
    pub(super) fn show_hinge_list(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading(self.localizer.text("hinge-list"));
        if ui
            .add_enabled(
                !self.modal_open(),
                egui::Button::new(self.localizer.text("hinge-new")),
            )
            .clicked()
        {
            self.hinge_dialog = Some(HingeDialog::new(self, None));
        }
        let installations = self.editor.project().hinge_installations.clone();
        for installation in installations {
            let project = self.editor.project();
            let name = |id| {
                project
                    .boards
                    .iter()
                    .find(|b| b.id == id)
                    .map_or("?", |b| b.name.as_str())
            };
            let label = format!(
                "{} → {} · Y {} / {} mm",
                name(installation.door_board_id),
                name(installation.mounting_board_id),
                mm(installation.door_y),
                mm(installation.mount_y)
            );
            ui.group(|ui| {
                ui.label(&label);
                status_ui(
                    ui,
                    &self.localizer,
                    &hinge_installation::diagnose(self.editor.project(), &installation),
                    &installation,
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("hinge-edit")),
                        )
                        .clicked()
                    {
                        self.hinge_dialog = Some(HingeDialog::new(self, Some(installation.id)));
                    }
                    if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("hinge-delete")),
                        )
                        .clicked()
                    {
                        let _ = hinge_installation::remove(&mut self.editor, installation.id);
                    }
                });
            });
        }
    }

    pub(super) fn show_hinge_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.hinge_dialog.take() else {
            return;
        };
        let mut accept = false;
        let mut cancel = false;
        let current = draft.project_id == self.editor.project().id
            && draft.revision == self.editor.project().revision;
        let modal = egui::Modal::new(egui::Id::new("hinge-dialog")).show(ctx, |ui| {
            ui.heading(self.localizer.text(if draft.id.is_some() {
                "hinge-edit"
            } else {
                "hinge-new"
            }));
            for (key, selected) in [
                ("hinge-door", &mut draft.door),
                ("hinge-mount", &mut draft.mount),
            ] {
                let name = selected
                    .and_then(|id| self.editor.project().boards.iter().find(|b| b.id == id))
                    .map_or("—", |b| b.name.as_str());
                egui::ComboBox::from_label(self.localizer.text(key))
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        for board in &self.editor.project().boards {
                            ui.selectable_value(selected, Some(board.id), &board.name);
                        }
                    });
            }
            let name = draft
                .catalog
                .and_then(|id| self.editor.project().catalog.iter().find(|c| c.id == id))
                .map_or("—", |c| c.name.as_str());
            egui::ComboBox::from_label(self.localizer.text("hinge-kit"))
                .selected_text(name)
                .show_ui(ui, |ui| {
                    for entry in &self.editor.project().catalog {
                        if hardware_catalog::is_verified(entry) {
                            ui.selectable_value(&mut draft.catalog, Some(entry.id), &entry.name);
                        }
                    }
                });
            for (key, edge) in [
                ("hinge-door-edge", &mut draft.side.door_edge),
                ("hinge-front-edge", &mut draft.side.mount_front_edge),
            ] {
                egui::ComboBox::from_label(self.localizer.text(key))
                    .selected_text(self.localizer.text(if *edge == BoardEdge::MinX {
                        "hinge-min-x"
                    } else {
                        "hinge-max-x"
                    }))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            edge,
                            BoardEdge::MinX,
                            self.localizer.text("hinge-min-x"),
                        );
                        ui.selectable_value(
                            edge,
                            BoardEdge::MaxX,
                            self.localizer.text("hinge-max-x"),
                        );
                    });
            }
            for (key, face) in [
                ("hinge-door-face", &mut draft.side.door_face),
                ("hinge-mount-face", &mut draft.side.mount_face),
            ] {
                egui::ComboBox::from_label(self.localizer.text(key))
                    .selected_text(self.localizer.text(if *face == BoardFace::MinZ {
                        "hinge-min-z"
                    } else {
                        "hinge-max-z"
                    }))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            face,
                            BoardFace::MinZ,
                            self.localizer.text("hinge-min-z"),
                        );
                        ui.selectable_value(
                            face,
                            BoardFace::MaxZ,
                            self.localizer.text("hinge-max-z"),
                        );
                    });
            }
            for (index, key) in ["hinge-door-y", "hinge-mount-y", "hinge-k", "hinge-overlay"]
                .iter()
                .enumerate()
            {
                ui.horizontal(|ui| {
                    ui.label(self.localizer.text(key));
                    let response = ui.text_edit_singleline(&mut draft.values[index]);
                    if draft.focus && index == 0 {
                        response.request_focus();
                    }
                });
            }
            let proposed = current.then(|| draft.proposed(self)).flatten();
            if let Some(ref proposed) = proposed {
                if let Ok(status) = hinge_installation::preview(self.editor.project(), proposed) {
                    status_ui(ui, &self.localizer, &status, proposed);
                }
            } else {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    self.localizer.text("hinge-invalid"),
                );
            }
            if draft.error {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    self.localizer.text("hinge-invalid"),
                );
            }
            ui.horizontal(|ui| {
                cancel = ui.button(self.localizer.text("cancel")).clicked();
                accept = ui
                    .add_enabled(
                        proposed.is_some(),
                        egui::Button::new(self.localizer.text("confirm")),
                    )
                    .clicked();
            });
        });
        draft.focus = false;
        if cancel || modal.should_close() {
            return;
        }
        if accept {
            if let Some(proposed) = draft.proposed(self) {
                let result = if draft.id.is_some() {
                    hinge_installation::update(&mut self.editor, proposed)
                } else {
                    hinge_installation::create(&mut self.editor, proposed)
                };
                if result.is_ok() {
                    return;
                }
            }
            draft.error = true;
        }
        self.hinge_dialog = Some(draft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
    use plan_my_cabinet::domain::BoardGrain;

    fn fixture() -> DesktopApp {
        let mut app = DesktopApp::default();
        let material = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Unrestricted,
            })
            .unwrap();
        for name in ["Door", "Side"] {
            app.editor
                .create_board(NewBoard {
                    name: name.into(),
                    material_id: material,
                    length: Length::from_micrometres(100_000),
                    width: Length::from_micrometres(100_000),
                    pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
                })
                .unwrap();
        }
        hardware_catalog::add_builtin(&mut app.editor).unwrap();
        app
    }

    #[test]
    fn invalid_draft_and_escape_leave_project_untouched() {
        let mut app = fixture();
        let before = app.editor.project().clone();
        let mut draft = HingeDialog::new(&app, None);
        assert!(draft.proposed(&app).is_some());
        draft.values[0] = "invalid".into();
        assert!(draft.proposed(&app).is_none());
        draft.values[0] = "-1".into();
        assert!(draft.proposed(&app).is_none());
        draft.values[0] = "50".into();
        draft.mount = draft.door;
        assert!(draft.proposed(&app).is_none());
        app.hinge_dialog = Some(draft);
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_hinge_dialog(ui.ctx())
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
            |ui| app.show_hinge_dialog(ui.ctx()),
        )
        .drop_without_applying_deltas();
        assert!(app.hinge_dialog.is_none());
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn valid_and_unsupported_preview_and_refresh() {
        let mut app = fixture();
        let mut draft = HingeDialog::new(&app, None);
        let proposed = draft.proposed(&app).unwrap();
        assert!(
            hinge_installation::preview(app.editor.project(), &proposed)
                .unwrap()
                .issues
                .is_empty()
        );
        draft.values[3] = "18".into();
        let unsupported =
            hinge_installation::preview(app.editor.project(), &draft.proposed(&app).unwrap())
                .unwrap();
        assert!(
            unsupported
                .issues
                .contains(&InstallationIssue::UnsupportedOverlay)
        );
        assert!(unsupported.references.is_none());
        let id = proposed.catalog_id;
        hinge_installation::create(&mut app.editor, proposed).unwrap();
        let (_, statuses) =
            hardware_catalog::update_from_builtin_with_status(&mut app.editor, id).unwrap();
        assert_eq!(statuses.len(), 1);
        assert!(statuses[0].issues.is_empty());
        app.editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].width = Length::from_micrometres(20_000);
                Ok(())
            })
            .unwrap();
        assert!(
            hinge_installation::diagnose_all(app.editor.project())[0]
                .issues
                .contains(&InstallationIssue::CupOutsideDoor)
        );
        let (_, statuses) =
            hardware_catalog::update_from_builtin_with_status(&mut app.editor, id).unwrap();
        assert!(
            statuses[0]
                .issues
                .contains(&InstallationIssue::CupOutsideDoor)
        );
        assert_eq!(
            app.editor.project().hinge_installations[0].door_y,
            Length::from_micrometres(50_000)
        );
    }
}
