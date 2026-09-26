use super::*;
use plan_my_cabinet::placement::{
    Align, BoardFace, CoordinateFrame, FacePlacement, NumericPose, PlacementError,
    PlacementSession, Side,
};

pub(super) enum PlacementDraft {
    Numeric {
        frame: CoordinateFrame,
        position: [DimensionDraft; 3],
        rotation: [String; 3],
    },
    Face {
        target: Uuid,
        source_face: BoardFace,
        target_face: BoardFace,
        source_align: [Align; 2],
        target_align: [Align; 2],
        offset: [DimensionDraft; 2],
        gap: DimensionDraft,
    },
}

pub(super) struct PlacementDialog {
    pub board_id: Uuid,
    pub selection_ids: std::collections::HashSet<Uuid>,
    pub selection_active: Option<Uuid>,
    pub focus_on_open: bool,
    pub draft: PlacementDraft,
    pub error: Option<PlacementError>,
}

fn mm_text(value: f64) -> String {
    format!("{value:.3}")
}

fn length_draft(value: f64) -> DimensionDraft {
    DimensionDraft {
        text: mm_text(value),
        consent: false,
    }
}

fn euler(pose: Pose) -> [f64; 3] {
    let q = pose.rotation;
    let sin_pitch = 2.0 * (q.w * q.y - q.z * q.x);
    [
        (2.0 * (q.w * q.x + q.y * q.z))
            .atan2(1.0 - 2.0 * (q.x * q.x + q.y * q.y))
            .to_degrees(),
        sin_pitch.clamp(-1.0, 1.0).asin().to_degrees(),
        (2.0 * (q.w * q.z + q.x * q.y))
            .atan2(1.0 - 2.0 * (q.y * q.y + q.z * q.z))
            .to_degrees(),
    ]
}

impl PlacementDialog {
    fn numeric_draft(pose: Pose, frame: CoordinateFrame) -> PlacementDraft {
        PlacementDraft::Numeric {
            frame,
            position: pose.translation_mm.map(length_draft),
            rotation: euler(pose).map(|v| format!("{v:.6}")),
        }
    }

    pub fn numeric(app: &DesktopApp, board_id: Uuid) -> Option<Self> {
        let board = app
            .editor
            .project()
            .boards
            .iter()
            .find(|b| b.id == board_id)?;
        Some(Self {
            board_id,
            selection_ids: app.selection.ids.clone(),
            selection_active: app.selection.active,
            focus_on_open: true,
            draft: Self::numeric_draft(board.pose, CoordinateFrame::LocalParent),
            error: None,
        })
    }

    pub fn face(app: &DesktopApp, board_id: Uuid) -> Option<Self> {
        let target = app
            .editor
            .project()
            .boards
            .iter()
            .find(|b| b.id != board_id)?
            .id;
        Some(Self {
            board_id,
            selection_ids: app.selection.ids.clone(),
            selection_active: app.selection.active,
            focus_on_open: true,
            error: None,
            draft: PlacementDraft::Face {
                target,
                source_face: BoardFace {
                    axis: 2,
                    side: Side::Negative,
                },
                target_face: BoardFace {
                    axis: 2,
                    side: Side::Positive,
                },
                source_align: [Align::Centre; 2],
                target_align: [Align::Centre; 2],
                offset: [length_draft(0.0), length_draft(0.0)],
                gap: length_draft(0.0),
            },
        })
    }

    pub fn highlighted(&self) -> Option<(Uuid, BoardFace, Uuid, BoardFace)> {
        if let PlacementDraft::Face {
            target,
            source_face,
            target_face,
            ..
        } = &self.draft
        {
            Some((self.board_id, *source_face, *target, *target_face))
        } else {
            None
        }
    }
}

fn position_field(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    label: String,
    draft: &mut DimensionDraft,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.text_edit_singleline(&mut draft.text).changed() {
            draft.consent = false;
        }
    });
    match parse_length(&draft.text, Unit::Mm) {
        Ok(parsed) => {
            let value = parsed.conversion.suggested();
            if let Conversion::NeedsConfirmation(_) = parsed.conversion {
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("entered", draft.text.as_str());
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
                    &mut draft.consent,
                    localizer.format("rounding-confirmation", Some(&args)),
                );
            }
            draft.consent || parsed.conversion.exact().is_some()
        }
        Err(error) => {
            ui.colored_label(egui::Color32::LIGHT_RED, localizer.text(error_key(error)));
            false
        }
    }
}

fn coordinate(draft: &DimensionDraft) -> Option<f64> {
    let parsed = parse_length(&draft.text, Unit::Mm).ok()?.conversion;
    if parsed.exact().is_none() && !draft.consent {
        return None;
    }
    Some(parsed.suggested().micrometres() as f64 / 1000.0)
}

fn face_key(face: BoardFace) -> &'static str {
    match (face.axis, face.side) {
        (0, Side::Negative) => "face-length-minus",
        (0, Side::Positive) => "face-length-plus",
        (1, Side::Negative) => "face-width-minus",
        (1, Side::Positive) => "face-width-plus",
        (2, Side::Negative) => "face-thickness-minus",
        _ => "face-thickness-plus",
    }
}

fn face_selector(ui: &mut egui::Ui, localizer: &Localizer, id: &'static str, face: &mut BoardFace) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(localizer.text(face_key(*face)))
        .show_ui(ui, |ui| {
            for axis in 0..3 {
                for side in [Side::Negative, Side::Positive] {
                    let candidate = BoardFace { axis, side };
                    combo_option(ui, face, candidate, localizer.text(face_key(candidate)));
                }
            }
        });
}

fn align_selector(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    id: (&'static str, usize),
    align: &mut Align,
) {
    let key = |value| match value {
        Align::Start => "anchor-start",
        Align::Centre => "anchor-centre",
        Align::End => "anchor-end",
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(localizer.text(key(*align)))
        .show_ui(ui, |ui| {
            for value in [Align::Start, Align::Centre, Align::End] {
                combo_option(ui, align, value, localizer.text(key(value)));
            }
        });
}

fn placement_error_key(error: PlacementError) -> &'static str {
    match error {
        PlacementError::InvalidRotation => "error-invalid-rotation",
        PlacementError::InvalidPose(UnitError::NonFinite) => "error-non-finite",
        PlacementError::InvalidPose(UnitError::OutOfBounds) => "error-out-of-bounds",
        PlacementError::OffGrid => "placement-off-grid",
        _ => "placement-invalid",
    }
}

impl DesktopApp {
    pub(super) fn show_placement(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.placement.take() else {
            return;
        };
        let mut cancel = false;
        let mut accept = false;
        let mut request = None;
        let mut numeric_noop = false;
        let modal = egui::Modal::new(egui::Id::new("placement-dialog")).show(ctx, |ui| {
            ui.set_min_width(400.0);
            ui.heading(self.localizer.text(match dialog.draft {
                PlacementDraft::Numeric { .. } => "placement-numeric",
                PlacementDraft::Face { .. } => "placement-face",
            }));
            let board_name = self
                .editor
                .project()
                .boards
                .iter()
                .find(|b| b.id == dialog.board_id)
                .map(|b| b.name.as_str())
                .unwrap_or("—");
            ui.label(format!(
                "{}: {board_name}",
                self.localizer.text("placement-source")
            ));
            let mut valid = true;
            match &mut dialog.draft {
                PlacementDraft::Numeric {
                    frame,
                    position,
                    rotation,
                } => {
                    let old_frame = *frame;
                    let combo = egui::ComboBox::from_label(self.localizer.text("placement-frame"))
                        .selected_text(self.localizer.text(if *frame == CoordinateFrame::World {
                            "placement-world"
                        } else {
                            "placement-local"
                        }))
                        .show_ui(ui, |ui| {
                            combo_option(
                                ui,
                                frame,
                                CoordinateFrame::LocalParent,
                                self.localizer.text("placement-local"),
                            );
                            combo_option(
                                ui,
                                frame,
                                CoordinateFrame::World,
                                self.localizer.text("placement-world"),
                            );
                        });
                    if dialog.focus_on_open {
                        combo.response.request_focus();
                    }
                    if *frame != old_frame {
                        let pose = if *frame == CoordinateFrame::LocalParent {
                            self.editor
                                .project()
                                .boards
                                .iter()
                                .find(|b| b.id == dialog.board_id)
                                .unwrap()
                                .pose
                        } else {
                            plan_my_cabinet::placement::world_pose(
                                self.editor.project(),
                                dialog.board_id,
                            )
                            .unwrap()
                        };
                        if let PlacementDraft::Numeric {
                            position: new_position,
                            rotation: new_rotation,
                            ..
                        } = PlacementDialog::numeric_draft(pose, *frame)
                        {
                            *position = new_position;
                            *rotation = new_rotation;
                        }
                    }
                    let source = if *frame == CoordinateFrame::LocalParent {
                        self.editor
                            .project()
                            .boards
                            .iter()
                            .find(|b| b.id == dialog.board_id)
                            .unwrap()
                            .pose
                    } else {
                        plan_my_cabinet::placement::world_pose(
                            self.editor.project(),
                            dialog.board_id,
                        )
                        .unwrap()
                    };
                    let mut field_valid = [false; 3];
                    for i in 0..3 {
                        field_valid[i] = position_field(
                            ui,
                            &self.localizer,
                            format!(
                                "{} {} (mm)",
                                self.localizer.text("placement-position"),
                                ["X", "Y", "Z"][i]
                            ),
                            &mut position[i],
                        );
                    }
                    let position_edited = std::array::from_fn(|i| {
                        position[i].text != mm_text(source.translation_mm[i])
                    });
                    for i in 0..3 {
                        if position_edited[i] {
                            valid &= field_valid[i];
                        }
                    }
                    for i in 0..3 {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{} {} (°)",
                                self.localizer.text("placement-rotation"),
                                ["X", "Y", "Z"][i]
                            ));
                            ui.text_edit_singleline(&mut rotation[i]);
                        });
                    }
                    let rotation_edited = rotation
                        .iter()
                        .zip(euler(source))
                        .any(|(text, angle)| *text != format!("{angle:.6}"));
                    let angles: Option<Vec<f64>> = rotation
                        .iter()
                        .map(|s| {
                            s.trim()
                                .replace(',', ".")
                                .parse::<f64>()
                                .ok()
                                .filter(|v| v.is_finite() && v.abs() <= 360.0)
                        })
                        .collect();
                    if rotation_edited && angles.is_none() {
                        valid = false;
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            self.localizer.text("error-invalid-rotation"),
                        );
                    }
                    if valid {
                        let angles = angles.unwrap_or_default();
                        numeric_noop = !position_edited.contains(&true) && !rotation_edited;
                        if !numeric_noop {
                            request = Some(PlacementRequest::Numeric(
                                NumericPose {
                                    frame: *frame,
                                    position_mm: std::array::from_fn(|i| {
                                        if position_edited[i] {
                                            coordinate(&position[i]).unwrap()
                                        } else {
                                            source.translation_mm[i]
                                        }
                                    }),
                                    rotation_degrees_xyz: [angles[0], angles[1], angles[2]],
                                },
                                position_edited,
                                rotation_edited,
                            ));
                        }
                    }
                }
                PlacementDraft::Face {
                    target,
                    source_face,
                    target_face,
                    source_align,
                    target_align,
                    offset,
                    gap,
                } => {
                    let targets: Vec<_> = self
                        .editor
                        .project()
                        .boards
                        .iter()
                        .filter(|b| b.id != dialog.board_id)
                        .collect();
                    let combo = egui::ComboBox::from_label(self.localizer.text("placement-target"))
                        .selected_text(
                            targets
                                .iter()
                                .find(|b| b.id == *target)
                                .map(|b| b.name.as_str())
                                .unwrap_or("—"),
                        )
                        .show_ui(ui, |ui| {
                            for board in &targets {
                                combo_option(
                                    ui,
                                    target,
                                    board.id,
                                    format!("{} ({})", board.name, board.id),
                                );
                            }
                        });
                    if dialog.focus_on_open {
                        combo.response.request_focus();
                    }
                    ui.horizontal(|ui| {
                        ui.label(self.localizer.text("placement-source-face"));
                        face_selector(ui, &self.localizer, "source-face", source_face);
                    });
                    ui.horizontal(|ui| {
                        ui.label(self.localizer.text("placement-target-face"));
                        face_selector(ui, &self.localizer, "target-face", target_face);
                    });
                    for i in 0..2 {
                        let source_axis = (0..3).filter(|a| *a != source_face.axis).nth(i).unwrap();
                        let target_axis = (0..3).filter(|a| *a != target_face.axis).nth(i).unwrap();
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{} {} → {}",
                                self.localizer.text("placement-align"),
                                ["X", "Y", "Z"][source_axis],
                                ["X", "Y", "Z"][target_axis]
                            ));
                            align_selector(
                                ui,
                                &self.localizer,
                                ("source-align", i),
                                &mut source_align[i],
                            );
                            ui.label("→");
                            align_selector(
                                ui,
                                &self.localizer,
                                ("target-align", i),
                                &mut target_align[i],
                            );
                        });
                        valid &= position_field(
                            ui,
                            &self.localizer,
                            format!(
                                "{} {} (mm)",
                                self.localizer.text("placement-offset"),
                                ["X", "Y", "Z"][target_axis]
                            ),
                            &mut offset[i],
                        );
                    }
                    valid &= position_field(
                        ui,
                        &self.localizer,
                        format!("{} (mm)", self.localizer.text("placement-gap")),
                        gap,
                    );
                    if valid {
                        request = Some(PlacementRequest::Face(FacePlacement {
                            source_face: *source_face,
                            target_id: *target,
                            target_face: *target_face,
                            source_align: *source_align,
                            target_align: *target_align,
                            offset_mm: offset.each_ref().map(|p| coordinate(p).unwrap()),
                            gap_mm: coordinate(gap).unwrap(),
                        }));
                    }
                }
            }
            if let Some(error) = dialog.error {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    self.localizer.text(placement_error_key(error)),
                );
            }
            ui.label(self.localizer.text("placement-preview"));
            ui.horizontal(|ui| {
                cancel = ui.button(self.localizer.text("cancel")).clicked();
                accept = ui
                    .add_enabled(
                        valid && dialog.error.is_none(),
                        egui::Button::new(self.localizer.text("confirm")),
                    )
                    .clicked();
            });
        });
        dialog.focus_on_open = false;
        cancel |= modal.should_close();
        if cancel {
            self.editor.cancel_preview();
            self.selection.ids = dialog.selection_ids;
            self.selection.active = dialog.selection_active;
            return;
        }
        if numeric_noop {
            self.editor.cancel_preview();
            dialog.error = None;
            if accept {
                return;
            }
        }
        if let Some(request) = request {
            let mut session = PlacementSession::resume(&mut self.editor, dialog.board_id)
                .expect("selected board exists");
            dialog.error = match request {
                PlacementRequest::Numeric(pose, position_edited, rotation_edited) => session
                    .preview_numeric_edited(pose, position_edited, rotation_edited)
                    .err(),
                PlacementRequest::Face(face) => session.preview_face(face).err(),
            };
            session.pause();
        } else {
            dialog.error = None;
        }
        if accept && dialog.error.is_none() {
            if self.editor.commit_preview().is_ok() {
                return;
            }
            dialog.error = Some(PlacementError::InvalidPose(UnitError::OutOfBounds));
        }
        self.placement = Some(dialog);
    }
}

enum PlacementRequest {
    Numeric(NumericPose, [bool; 3], bool),
    Face(FacePlacement),
}
