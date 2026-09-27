use super::*;
use plan_my_cabinet::measurements::measure;
use plan_my_cabinet::placement::{
    Align, BoardFace, CoordinateFrame, FacePlacement, NumericPose, PlacementError,
    PlacementSession, PosePreset, Side, preset_pose,
};

pub(super) struct PresetState {
    /// Exact proposal in the selected frame, never reconstructed from a
    /// six-decimal Euler readout unless the user edits that readout.
    pose: Pose,
    position_text: [String; 3],
    rotation_text: [String; 3],
    kind: Option<PosePreset>,
    pending_change: bool,
}

pub(super) enum PlacementDraft {
    Numeric {
        frame: CoordinateFrame,
        position: [DimensionDraft; 3],
        rotation: [String; 3],
        preset: Option<Box<PresetState>>,
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
    presented_frame: CoordinateFrame,
}

/// Fixed-precision readout without needless zeros (`18`, `0.5`, `-90`).
/// Lossless relative to the fixed-precision text, so equality checks that
/// detect edits keep working on the trimmed form.
fn trim_fixed(mut text: String) -> String {
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" { "0".into() } else { text }
}

fn mm_text(value: f64) -> String {
    trim_fixed(format!("{value:.3}"))
}

fn angle_text(value: f64) -> String {
    trim_fixed(format!("{value:.6}"))
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

fn framed_pose(project: &Project, board_id: Uuid, frame: CoordinateFrame) -> Option<Pose> {
    match frame {
        CoordinateFrame::LocalParent => project
            .boards
            .iter()
            .find(|b| b.id == board_id)
            .map(|b| b.pose),
        CoordinateFrame::World => plan_my_cabinet::placement::world_pose(project, board_id).ok(),
    }
}

fn rotation_text(pose: Pose) -> [String; 3] {
    euler(pose).map(angle_text)
}

fn apply_numeric_preset(
    current: Pose,
    choice: PosePreset,
    position: &mut [DimensionDraft; 3],
    rotation: &mut [String; 3],
    preset: &mut Option<Box<PresetState>>,
) -> Result<(), PlacementError> {
    let pose = preset_pose(current, choice)?;
    *position = pose.translation_mm.map(length_draft);
    *rotation = rotation_text(pose);
    *preset = Some(Box::new(PresetState {
        pose,
        position_text: position.each_ref().map(|field| field.text.clone()),
        rotation_text: rotation.clone(),
        kind: Some(choice),
        pending_change: true,
    }));
    Ok(())
}

fn numeric_candidate_world(
    editor: &ProjectEditor,
    board_id: Uuid,
    frame: CoordinateFrame,
    position: &[DimensionDraft; 3],
    rotation: &[String; 3],
    preset: Option<&PresetState>,
) -> Result<Pose, PlacementError> {
    let source = framed_pose(editor.project(), board_id, frame)
        .ok_or(PlacementError::MissingBoard(board_id))?;
    let exact_preset = preset.filter(|state| *rotation == state.rotation_text);
    let position_edited = std::array::from_fn(|axis| {
        position[axis].text
            != exact_preset.map_or_else(
                || mm_text(source.translation_mm[axis]),
                |state| state.position_text[axis].clone(),
            )
    });
    let mut coordinates =
        exact_preset.map_or(source.translation_mm, |state| state.pose.translation_mm);
    for axis in 0..3 {
        if position_edited[axis] {
            coordinates[axis] = coordinate(&position[axis]).ok_or(PlacementError::OffGrid)?;
        }
    }
    let rotation_edited = *rotation != rotation_text(source);
    let degrees: Vec<f64> = rotation
        .iter()
        .map(|text| {
            text.trim()
                .replace(',', ".")
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && v.abs() <= 360.0)
                .ok_or(PlacementError::InvalidRotation)
        })
        .collect::<Result<_, _>>()?;
    let mut shadow =
        ProjectEditor::new(editor.project().clone()).expect("validated committed project");
    let mut session = PlacementSession::begin(&mut shadow, board_id)?;
    if let Some(state) = exact_preset {
        let pose =
            Pose::new(coordinates, state.pose.rotation).map_err(PlacementError::InvalidPose)?;
        session.preview_exact_framed(frame, pose, position_edited)?;
    } else {
        session.preview_numeric_edited(
            NumericPose {
                frame,
                position_mm: coordinates,
                rotation_degrees_xyz: [degrees[0], degrees[1], degrees[2]],
            },
            position_edited,
            rotation_edited,
        )?;
    }
    plan_my_cabinet::placement::world_pose(session.project(), board_id)
}

impl PlacementDialog {
    pub fn can_accept_navigation(&self, project: &Project) -> bool {
        if self.error.is_some() || !project.boards.iter().any(|b| b.id == self.board_id) {
            return false;
        }
        match &self.draft {
            PlacementDraft::Numeric {
                position, rotation, ..
            } => {
                position.iter().all(|value| coordinate(value).is_some())
                    && rotation.iter().all(|text| {
                        text.trim()
                            .replace(',', ".")
                            .parse::<f64>()
                            .is_ok_and(|angle| angle.is_finite() && angle.abs() <= 360.0)
                    })
            }
            PlacementDraft::Face {
                target,
                offset,
                gap,
                ..
            } => {
                project
                    .boards
                    .iter()
                    .any(|b| b.id == *target && b.id != self.board_id)
                    && offset.iter().all(|value| coordinate(value).is_some())
                    && coordinate(gap).is_some()
            }
        }
    }
    fn numeric_draft(pose: Pose, frame: CoordinateFrame) -> PlacementDraft {
        PlacementDraft::Numeric {
            frame,
            position: pose.translation_mm.map(length_draft),
            rotation: euler(pose).map(angle_text),
            preset: None,
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
            presented_frame: CoordinateFrame::LocalParent,
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
            presented_frame: CoordinateFrame::LocalParent,
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

const SOURCE_COLOR: egui::Color32 = egui::Color32::from_rgb(79, 184, 214);
const TARGET_COLOR: egui::Color32 = egui::Color32::from_rgb(224, 85, 159);

/// `18`, `-0.5`, `539.75`: millimetres without needless zeros.
fn trim_mm_value(value: f64) -> String {
    mm_text(value)
}

/// A compact pose input (mm) built by the caller (axis letter, prefix,
/// suffix). Returns (accepted, rounding needed) without drawing messages.
fn pose_input(
    ui: &mut egui::Ui,
    input: modal_chrome::form::Input<'_>,
    draft: &mut DimensionDraft,
) -> (bool, bool) {
    let parsed = parse_length(&draft.text, Unit::Mm);
    let input = input.invalid(parsed.is_err() && !draft.text.trim().is_empty());
    if input.show(ui, &mut draft.text).changed() {
        draft.consent = false;
    }
    match parse_length(&draft.text, Unit::Mm) {
        Ok(parsed) => {
            let needs = matches!(parsed.conversion, Conversion::NeedsConfirmation(_));
            (draft.consent || parsed.conversion.exact().is_some(), needs)
        }
        Err(_) => (false, false),
    }
}

/// Explicit rounding consent (and validation messages) under a group of
/// pose inputs. Editing a field clears its consent.
fn pose_messages(ui: &mut egui::Ui, localizer: &Localizer, fields: &mut [(&str, &mut DimensionDraft)]) {
    let locale = if localizer.language() == Language::En {
        Locale::En
    } else {
        Locale::PtBr
    };
    for (axis, draft) in fields.iter_mut() {
        match parse_length(&draft.text, Unit::Mm) {
            Ok(parsed) => {
                if let Conversion::NeedsConfirmation(value) = parsed.conversion {
                    let mut args = fluent_bundle::FluentArgs::new();
                    args.set("entered", format!("{axis} {}", draft.text.trim()));
                    args.set("rounded", format_length(value, Unit::Mm, locale, 3));
                    ui.checkbox(
                        &mut draft.consent,
                        egui::RichText::new(localizer.format("rounding-confirmation", Some(&args)))
                            .size(11.5)
                            .color(theme_widgets::WARN_INK),
                    );
                }
            }
            Err(error) => {
                if !draft.text.trim().is_empty() {
                    modal_chrome::form::error(
                        ui,
                        &format!("{axis}: {}", localizer.text(error_key(error))),
                    );
                }
            }
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

fn face_selector(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    id: &'static str,
    face: &mut BoardFace,
    width: f32,
) -> egui::Response {
    use modal_chrome::form;
    let popup = egui::Id::new(id).with("popup");
    let current = localizer.text(face_key(*face));
    let response = form::select_box(ui, popup, &current, width, |ui| {
        ui.add(
            egui::Label::new(egui::RichText::new(&current).size(13.0).color(theme_widgets::TEXT))
                .truncate()
                .selectable(false),
        );
    });
    egui::Popup::menu(&response)
        .id(popup)
        .width(width)
        .show(|ui| {
            ui.set_min_width(width - 12.0);
            for axis in 0..3 {
                for side in [Side::Negative, Side::Positive] {
                    let candidate = BoardFace { axis, side };
                    let label = localizer.text(face_key(candidate));
                    if form::option(ui, *face == candidate, &label, |ui| {
                        ui.label(egui::RichText::new(&label).size(13.0));
                    })
                    .clicked()
                    {
                        *face = candidate;
                    }
                }
            }
        });
    response
}

fn align_key(value: Align) -> &'static str {
    match value {
        Align::Start => "anchor-start-short",
        Align::Centre => "anchor-centre-short",
        Align::End => "anchor-end-short",
    }
}

/// One in-plane axis: a shared Start | Centre | End choice (source and
/// target aligned alike), an optional split into source → target, and the
/// target offset field.
#[allow(clippy::too_many_arguments)]
fn align_axis(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    index: usize,
    target_axis: usize,
    source_align: &mut Align,
    target_align: &mut Align,
    offset: &mut DimensionDraft,
    width: f32,
) -> (bool, bool) {
    use modal_chrome::form;
    let split_id = egui::Id::new(("placement-align-split", index));
    let mut split = ui.data(|d| d.get_temp::<bool>(split_id)).unwrap_or(false) || source_align != target_align;
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            form::label(
                ui,
                &format!(
                    "{} {}",
                    localizer.text("placement-align-along"),
                    ["X", "Y", "Z"][target_axis]
                ),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let toggle = theme_widgets::ghost_icon_sized(
                    ui,
                    icons::Icon::Sliders,
                    &localizer.text("placement-align-separately"),
                    if split { theme_widgets::ACCENT_DARK } else { theme_widgets::FAINT },
                    12.0,
                    18.0,
                    true,
                    false,
                );
                if toggle.clicked() {
                    split = !split;
                    if !split {
                        *source_align = *target_align;
                    }
                }
            });
        });
        ui.data_mut(|d| d.insert_temp(split_id, split));
        let options = [Align::Start, Align::Centre, Align::End]
            .map(|value| (value, localizer.text(align_key(value))));
        let options: Vec<(Align, &str)> =
            options.iter().map(|(value, text)| (*value, text.as_str())).collect();
        if split {
            let half = ((width - 18.0) / 2.0).max(40.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                form::segmented_sized(ui, ("source-align", index), source_align, &options, half, 30.0, false)
                    .on_hover_text(localizer.text("placement-source-face"));
                ui.label(egui::RichText::new("→").color(theme_widgets::FAINT));
                form::segmented_sized(ui, ("target-align", index), target_align, &options, half, 30.0, false)
                    .on_hover_text(localizer.text("placement-target-face"));
            });
        } else {
            let before = *target_align;
            form::segmented(ui, ("align", index), target_align, &options, width);
            if *target_align != before || *source_align != *target_align {
                *source_align = *target_align;
            }
        }
        let offset_label = localizer.text("placement-offset-short");
        pose_input(
            ui,
            form::Input::new(egui::Id::new(("placement-offset", index)), &offset_label, width)
                .prefix(&offset_label, theme_widgets::FAINT)
                .suffix("mm"),
            offset,
        )
    })
    .inner
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
        if self.navigation.pending().is_some() {
            return;
        }
        let Some(mut dialog) = self.modals.take_placement() else {
            return;
        };
        let mut request = None;
        let mut numeric_noop = false;
        let mut first_focus = None;
        let numeric = matches!(dialog.draft, PlacementDraft::Numeric { .. });
        let project = self.editor.project();
        let board_name = project
            .boards
            .iter()
            .find(|b| b.id == dialog.board_id)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| "—".into());
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("name", board_name.as_str());
        let title = if numeric {
            self.localizer.format("placement-position-title", Some(&args))
        } else {
            self.localizer.text("placement-face")
        };
        let confirm_label = self.localizer.text(if numeric {
            "navigation-apply"
        } else {
            "placement-place-action"
        });
        self.placement_chrome.set_icon(if numeric {
            icons::Icon::Axes
        } else {
            icons::Icon::Place
        });
        self.placement_chrome.set_context(Some(if numeric {
            self.localizer.text("placement-live")
        } else {
            self.localizer.format("placement-face-context", Some(&args))
        }));
        self.placement_chrome.set_hint(Some(self.localizer.text(if numeric {
            "placement-numeric-hint"
        } else {
            "placement-face-hint"
        })));
        let parent_name = project
            .boards
            .iter()
            .find(|b| b.id == dialog.board_id)
            .and_then(|b| b.parent_id)
            .and_then(|parent| project.assemblies.iter().find(|a| a.id == parent))
            .map(|a| a.name.clone());
        let result = self.placement_chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &confirm_label,
            },
            |ui| {
                use modal_chrome::form;
                let width = ui.available_width();
                let mut valid = true;
                match &mut dialog.draft {
                    PlacementDraft::Numeric {
                        frame,
                        position,
                        rotation,
                        preset,
                    } => {
                        let old_frame = dialog.presented_frame;
                        let local_label = parent_name.as_ref().map_or_else(
                            || self.localizer.text("placement-local"),
                            |name| {
                                let mut args = fluent_bundle::FluentArgs::new();
                                args.set("name", name.as_str());
                                self.localizer.format("placement-local-named", Some(&args))
                            },
                        );
                        ui.horizontal(|ui| {
                            form::label(ui, &self.localizer.text("placement-frame"));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    form::segmented(
                                        ui,
                                        "placement-frame",
                                        frame,
                                        &[
                                            (
                                                CoordinateFrame::World,
                                                &self.localizer.text("placement-world"),
                                            ),
                                            (CoordinateFrame::LocalParent, &local_label),
                                        ],
                                        (width * 0.52).max(150.0),
                                    );
                                },
                            );
                        });
                        if *frame != old_frame {
                            let old_source =
                                framed_pose(self.editor.project(), dialog.board_id, old_frame)
                                    .expect("the modal's board cannot be removed while it is open");
                            let unedited = if let Some(state) = preset.as_ref() {
                                !state.pending_change
                                    && position
                                        .iter()
                                        .zip(&state.position_text)
                                        .all(|(field, text)| field.text == *text)
                                    && *rotation == state.rotation_text
                            } else {
                                position
                                    .iter()
                                    .zip(old_source.translation_mm)
                                    .all(|(field, value)| field.text == mm_text(value))
                                    && *rotation == rotation_text(old_source)
                            };
                            let proposed = if unedited {
                                framed_pose(self.editor.project(), dialog.board_id, *frame)
                                    .ok_or(PlacementError::MissingBoard(dialog.board_id))
                            } else {
                                numeric_candidate_world(
                                    &self.editor,
                                    dialog.board_id,
                                    old_frame,
                                    position,
                                    rotation,
                                    preset.as_deref(),
                                )
                                .and_then(|world| {
                                    plan_my_cabinet::placement::pose_in_frame(
                                        self.editor.project(),
                                        dialog.board_id,
                                        world,
                                        *frame,
                                    )
                                })
                            };
                            match proposed {
                                Ok(pose) => {
                                    *position = pose.translation_mm.map(length_draft);
                                    *rotation = rotation_text(pose);
                                    *preset = Some(Box::new(PresetState {
                                        pose,
                                        position_text: position
                                            .each_ref()
                                            .map(|field| field.text.clone()),
                                        rotation_text: rotation.clone(),
                                        kind: None,
                                        pending_change: !unedited,
                                    }));
                                    dialog.presented_frame = *frame;
                                    dialog.error = None;
                                }
                                Err(error) => {
                                    *frame = old_frame;
                                    dialog.error = Some(error);
                                    form::error(
                                        ui,
                                        &self.localizer.text("placement-frame-pending"),
                                    );
                                }
                            }
                        }
                        let source =
                            framed_pose(self.editor.project(), dialog.board_id, *frame)
                            .expect("the modal's board cannot be removed while it is open");
                        form::gap(ui);
                        let third = ((width - 16.0) / 3.0).max(50.0);
                        form::label(
                            ui,
                            &format!("{} (mm)", self.localizer.text("placement-position")),
                        )
                        .on_hover_text(format!(
                            "{}: {}",
                            self.localizer.text("placement-pivot"),
                            self.localizer.text("placement-origin")
                        ));
                        ui.add_space(2.0);
                        let mut field_valid = [false; 3];
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            for i in 0..3 {
                                let label = format!(
                                    "{} {}",
                                    self.localizer.text("placement-position"),
                                    ["X", "Y", "Z"][i]
                                );
                                field_valid[i] = pose_input(
                                    ui,
                                    form::Input::new(
                                        egui::Id::new(("placement-position", i)),
                                        &label,
                                        third,
                                    )
                                    .axis(i),
                                    &mut position[i],
                                )
                                .0;
                            }
                        });
                        {
                            let [x, y, z] = position;
                            pose_messages(
                                ui,
                                &self.localizer,
                                &mut [("X", x), ("Y", y), ("Z", z)],
                            );
                        }
                        first_focus = Some(egui::Id::new(("placement-position", 0)));
                        let position_edited = std::array::from_fn(|i| {
                            position[i].text
                                != preset.as_ref().map_or_else(
                                    || mm_text(source.translation_mm[i]),
                                    |state| state.position_text[i].clone(),
                                )
                        });
                        for i in 0..3 {
                            if position_edited[i] {
                                valid &= field_valid[i];
                            }
                        }
                        form::gap(ui);
                        form::label(ui, &self.localizer.text("placement-rotation-order"));
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            for i in 0..3 {
                                let label = format!(
                                    "{} {}",
                                    self.localizer.text("placement-rotation"),
                                    ["X", "Y", "Z"][i]
                                );
                                let invalid = rotation[i]
                                    .trim()
                                    .replace(',', ".")
                                    .parse::<f64>()
                                    .ok()
                                    .filter(|v| v.is_finite() && v.abs() <= 360.0)
                                    .is_none();
                                form::Input::new(
                                    egui::Id::new(("placement-rotation", i)),
                                    &label,
                                    third,
                                )
                                .axis(i)
                                .suffix("°")
                                .invalid(invalid)
                                .show(ui, &mut rotation[i]);
                            }
                        });
                        ui.add_space(2.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            for (choice, key) in [
                                (PosePreset::StandUp, "placement-stand-up"),
                                (PosePreset::LayFlat, "placement-lay-flat"),
                                (PosePreset::Turn90Z, "placement-turn-z"),
                            ] {
                                let disclosure = match choice {
                                    PosePreset::StandUp => "placement-stand-disclosure",
                                    PosePreset::LayFlat => "placement-flat-disclosure",
                                    PosePreset::Turn90Z => "placement-turn-disclosure",
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(self.localizer.text(key))
                                                .size(12.0)
                                                .color(theme_widgets::SECONDARY),
                                        )
                                        .fill(theme_widgets::VIEWPORT)
                                        .stroke(egui::Stroke::NONE)
                                        .corner_radius(6)
                                        .min_size(egui::vec2(0.0, 24.0)),
                                    )
                                    .on_hover_text(self.localizer.text(disclosure))
                                    .clicked()
                                {
                                    let current = framed_pose(
                                        self.editor.preview().unwrap_or(self.editor.project()),
                                        dialog.board_id,
                                        *frame,
                                    )
                                    .expect("the modal's board cannot be removed while it is open");
                                    dialog.error = apply_numeric_preset(
                                        current, choice, position, rotation, preset,
                                    )
                                    .err();
                                }
                            }
                        });
                        if let Some(choice) = preset.as_ref().and_then(|state| state.kind) {
                            let key = match choice {
                                PosePreset::StandUp => "placement-stand-disclosure",
                                PosePreset::LayFlat => "placement-flat-disclosure",
                                PosePreset::Turn90Z => "placement-turn-disclosure",
                            };
                            form::hint(ui, &self.localizer.text(key));
                        }
                        let rotation_edited = rotation
                            .iter()
                            .zip(euler(source))
                            .any(|(text, angle)| *text != angle_text(angle));
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
                            form::error(ui, &self.localizer.text("error-invalid-rotation"));
                        }
                        if preset
                            .as_ref()
                            .is_some_and(|state| *rotation != state.rotation_text)
                        {
                            *preset = None;
                        }
                        let proposed_position: [Option<f64>; 3] = std::array::from_fn(|i| {
                            if position_edited[i] {
                                coordinate(&position[i])
                            } else {
                                Some(preset.as_ref().map_or(source.translation_mm[i], |state| {
                                    state.pose.translation_mm[i]
                                }))
                            }
                        });
                        if valid && let [Some(x), Some(y), Some(z)] = proposed_position {
                            let angles = angles.unwrap_or_default();
                            let proposed_position = [x, y, z];
                            if let Some(state) = preset.as_ref() {
                                match Pose::new(proposed_position, state.pose.rotation) {
                                    Ok(exact) => {
                                        numeric_noop = exact == source
                                            || (state.kind.is_none()
                                                && !state.pending_change
                                                && !position_edited.contains(&true));
                                        if !numeric_noop {
                                            request = Some(PlacementRequest::Exact(
                                                *frame,
                                                exact,
                                                position_edited,
                                            ));
                                        }
                                    }
                                    Err(error) => {
                                        valid = false;
                                        dialog.error = Some(PlacementError::InvalidPose(error));
                                    }
                                }
                            } else {
                                numeric_noop = !position_edited.contains(&true) && !rotation_edited;
                            }
                            if !numeric_noop && preset.is_none() {
                                request = Some(PlacementRequest::Numeric(
                                    NumericPose {
                                        frame: *frame,
                                        position_mm: proposed_position,
                                        rotation_degrees_xyz: [angles[0], angles[1], angles[2]],
                                    },
                                    position_edited,
                                    rotation_edited,
                                ));
                            }
                        }
                        if valid && dialog.error.is_none() {
                            let preview = self.editor.preview().unwrap_or(self.editor.project());
                            let parent = preview
                                .boards
                                .iter()
                                .find(|b| b.id == dialog.board_id)
                                .and_then(|b| b.parent_id);
                            let measure_frame = if *frame == CoordinateFrame::World {
                                Frame::World
                            } else {
                                parent.map_or(Frame::World, Frame::Object)
                            };
                            form::gap(ui);
                            let strip = form::strip(ui, |ui| {
                                if let Ok(bounds) = measure(
                                    preview,
                                    &[dialog.board_id],
                                    Scope::Body,
                                    measure_frame,
                                ) {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing.x = 14.0;
                                        for axis in 0..3 {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 6.0;
                                                ui.label(
                                                    egui::RichText::new(["X", "Y", "Z"][axis])
                                                        .size(11.0)
                                                        .color(form::axis_color(axis)),
                                                );
                                                ui.label(
                                                    theme_widgets::mono(
                                                        format!(
                                                            "{} → {}",
                                                            trim_mm_value(bounds.minimum_mm[axis]),
                                                            trim_mm_value(bounds.maximum_mm[axis])
                                                        ),
                                                        12.0,
                                                    )
                                                    .color(theme_widgets::TEXT),
                                                );
                                            });
                                        }
                                    });
                                } else {
                                    form::note(ui, &self.localizer.text("measurement-invalid"));
                                }
                            });
                            strip
                                .response.on_hover_text(self.localizer.text("placement-derived-extents"));
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
                        let project = self.editor.project();
                        let targets: Vec<_> = project
                            .boards
                            .iter()
                            .filter(|b| b.id != dialog.board_id)
                            .collect();
                        let target_name = targets
                            .iter()
                            .find(|b| b.id == *target)
                            .map(|b| b.name.clone())
                            .unwrap_or_else(|| "—".into());
                        let popup = egui::Id::new("placement-target").with("popup");
                        let combo = ui
                            .vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 6.0;
                                form::label(ui, &self.localizer.text("placement-target"));
                                let response = form::select_box(
                                    ui,
                                    popup,
                                    &self.localizer.text("placement-target"),
                                    width,
                                    |ui| {
                                        form::legend_swatch(ui, TARGET_COLOR);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&target_name)
                                                    .size(13.0)
                                                    .color(theme_widgets::TEXT),
                                            )
                                            .truncate()
                                            .selectable(false),
                                        );
                                    },
                                );
                                egui::Popup::menu(&response).id(popup).width(width).show(|ui| {
                                    ui.set_min_width(width - 12.0);
                                    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                                        for board in &targets {
                                            if form::option(
                                                ui,
                                                board.id == *target,
                                                &board.name,
                                                |ui| {
                                                    ui.label(
                                                        egui::RichText::new(&board.name).size(13.0),
                                                    );
                                                },
                                            )
                                            .clicked()
                                            {
                                                *target = board.id;
                                            }
                                        }
                                    });
                                });
                                response
                            })
                            .inner;
                        first_focus = Some(combo.id);
                        form::gap(ui);
                        let column = ((width - 34.0) / 2.0).max(80.0);
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.vertical(|ui| {
                                ui.set_width(column);
                                ui.spacing_mut().item_spacing.y = 6.0;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    form::legend_swatch(ui, SOURCE_COLOR);
                                    form::label(
                                        ui,
                                        &format!(
                                            "{} · {board_name}",
                                            self.localizer.text("placement-source-short")
                                        ),
                                    );
                                });
                                face_selector(
                                    ui,
                                    &self.localizer,
                                    "source-face",
                                    source_face,
                                    column,
                                )
                                .on_hover_text(self.localizer.text("placement-source-face"));
                            });
                            ui.vertical(|ui| {
                                ui.set_width(18.0);
                                ui.add_space(24.0);
                                ui.label(egui::RichText::new("→").color(theme_widgets::FAINT));
                            });
                            ui.vertical(|ui| {
                                ui.set_width(column);
                                ui.spacing_mut().item_spacing.y = 6.0;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    form::legend_swatch(ui, TARGET_COLOR);
                                    form::label(
                                        ui,
                                        &format!(
                                            "{} · {target_name}",
                                            self.localizer.text("placement-target-short")
                                        ),
                                    );
                                });
                                face_selector(
                                    ui,
                                    &self.localizer,
                                    "target-face",
                                    target_face,
                                    column,
                                )
                                .on_hover_text(self.localizer.text("placement-target-face"));
                            });
                        });
                        form::gap(ui);
                        let half = ((width - 14.0) / 2.0).max(80.0);
                        let mut rounding = false;
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 14.0;
                            for i in 0..2 {
                                let target_axis =
                                    (0..3).filter(|a| *a != target_face.axis).nth(i).expect("two axes lie in a face");
                                let (ok, needs) = align_axis(
                                    ui,
                                    &self.localizer,
                                    i,
                                    target_axis,
                                    &mut source_align[i],
                                    &mut target_align[i],
                                    &mut offset[i],
                                    half,
                                );
                                valid &= ok;
                                rounding |= needs;
                            }
                        });
                        {
                            let [first, second] = offset;
                            let axes: Vec<usize> =
                                (0..3).filter(|a| *a != target_face.axis).collect();
                            pose_messages(
                                ui,
                                &self.localizer,
                                &mut [
                                    (["X", "Y", "Z"][axes[0]], first),
                                    (["X", "Y", "Z"][axes[1]], second),
                                ],
                            );
                        }
                        let _ = rounding;
                        form::gap(ui);
                        form::label(ui, &self.localizer.text("placement-gap-short"));
                        ui.add_space(2.0);
                        let gap_label = self.localizer.text("placement-gap");
                        valid &= pose_input(
                            ui,
                            form::Input::new(egui::Id::new("placement-gap"), &gap_label, width)
                                .suffix("mm"),
                            gap,
                        )
                        .0;
                        pose_messages(ui, &self.localizer, &mut [("", gap)]);
                        // Parse once: the proposal exists only when every field does.
                        let parsed = valid
                            .then(|| {
                                let [first, second] = offset.each_ref().map(coordinate);
                                Some(([first?, second?], coordinate(gap)?))
                            })
                            .flatten();
                        if let Some((offset_mm, gap_mm)) = parsed {
                            let face = FacePlacement {
                                source_face: *source_face,
                                target_id: *target,
                                target_face: *target_face,
                                source_align: *source_align,
                                target_align: *target_align,
                                offset_mm,
                                gap_mm,
                            };
                            let mut shadow = ProjectEditor::new(self.editor.project().clone())
                                .expect("validated committed project");
                            if let Ok(mut session) =
                                PlacementSession::begin(&mut shadow, dialog.board_id)
                                && session.preview_face(face).is_ok()
                                && let Ok(pose) = plan_my_cabinet::placement::world_pose(
                                    session.project(),
                                    dialog.board_id,
                                )
                            {
                                let mut args = fluent_bundle::FluentArgs::new();
                                args.set("name", board_name.as_str());
                                args.set("x", trim_mm_value(pose.translation_mm[0]));
                                args.set("y", trim_mm_value(pose.translation_mm[1]));
                                args.set("z", trim_mm_value(pose.translation_mm[2]));
                                form::note(
                                    ui,
                                    &self.localizer.format("placement-face-result", Some(&args)),
                                );
                            }
                            request = Some(PlacementRequest::Face(face));
                        }
                    }
                }
                if let Some(error) = dialog.error {
                    form::error(ui, &self.localizer.text(placement_error_key(error)));
                }
                ((), valid && dialog.error.is_none())
            },
        );
        if dialog.focus_on_open
            && let Some(id) = first_focus
        {
            ctx.memory_mut(|memory| memory.request_focus(id));
        }
        dialog.focus_on_open = false;
        if crate::actions::decision(
            crate::actions::ActionId::CancelDialog,
            result.action == ModalAction::Cancel,
        ) {
            self.editor.cancel_preview();
            self.selection.ids = dialog.selection_ids;
            self.selection.active = dialog.selection_active;
            self.placement_chrome.close(ctx);
            return;
        }
        if numeric_noop {
            self.editor.cancel_preview();
            dialog.error = None;
            if crate::actions::decision(
                crate::actions::ActionId::ConfirmDialog,
                result.action == ModalAction::Confirm,
            ) {
                self.placement_chrome.close(ctx);
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
                PlacementRequest::Exact(frame, pose, position_edited) => session
                    .preview_exact_framed(frame, pose, position_edited)
                    .err(),
                PlacementRequest::Face(face) => session.preview_face(face).err(),
            };
            session.pause();
        } else {
            dialog.error = None;
        }
        if crate::actions::decision(
            crate::actions::ActionId::ConfirmDialog,
            result.action == ModalAction::Confirm,
        ) && dialog.error.is_none()
        {
            if self.editor.commit_preview().is_ok() {
                self.placement_chrome.close(ctx);
                return;
            }
            dialog.error = Some(PlacementError::InvalidPose(UnitError::OutOfBounds));
        }
        self.modals.set_placement(Some(dialog));
    }
}

enum PlacementRequest {
    Numeric(NumericPose, [bool; 3], bool),
    Exact(CoordinateFrame, Pose, [bool; 3]),
    Face(FacePlacement),
}

#[cfg(test)]
mod preset_dialog_tests {
    use super::*;

    use plan_my_cabinet::i18n::Language;

    #[test]
    fn preset_disclosures_name_frame_axes_origin_pivot_and_extents_in_both_languages() {
        for language in [Language::En, Language::PtBr] {
            let localizer = Localizer::new(language);
            for key in [
                "placement-frame",
                "placement-pivot",
                "placement-origin",
                "placement-derived-extents",
            ] {
                let label = localizer.text(key);
                assert!(!label.is_empty() && label != key, "{key}: {label}");
            }
            for key in [
                "placement-stand-disclosure",
                "placement-flat-disclosure",
                "placement-turn-disclosure",
            ] {
                let text = localizer.text(key);
                assert!(text.contains('X') || text.contains('Z'), "{key}: {text}");
                assert!(text.contains('Y') || text.contains('Z'), "{key}: {text}");
            }
        }
    }

    #[test]
    fn numeric_preset_fields_preview_exactly_and_escape_restores_selection() {
        let project = plan_my_cabinet::reference_fixture::project();
        let board_id = project.boards[0].id;
        let other_id = project.boards[1].id;
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        app.session = WorkspaceSession::new(app.editor.project());
        app.selection.choose(Some(board_id), false);
        app.selection.choose(Some(other_id), true);
        let original_selection = app.selection.ids.clone();
        let original = app.editor.project().clone();
        app.modals.set_placement(PlacementDialog::numeric(&app, board_id));
        let dialog = app.modals.placement_mut().unwrap();
        let PlacementDraft::Numeric {
            position,
            rotation,
            preset,
            ..
        } = &mut dialog.draft
        else {
            unreachable!()
        };
        let standing = preset_pose(original.boards[0].pose, PosePreset::StandUp).unwrap();
        apply_numeric_preset(
            original.boards[0].pose,
            PosePreset::StandUp,
            position,
            rotation,
            preset,
        )
        .unwrap();
        assert_eq!(preset.as_ref().unwrap().pose, standing);
        assert_eq!(position[0].text, mm_text(standing.translation_mm[0]));
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| app.show_placement(ui.ctx()))
            .drop_without_applying_deltas();
        assert_eq!(app.editor.preview().unwrap().boards[0].pose, standing);
        assert_eq!(app.editor.project(), &original);
        assert_eq!(app.selection.ids, original_selection);
        let current = app.editor.preview().unwrap().boards[0].pose;
        let dialog = app.modals.placement_mut().unwrap();
        let PlacementDraft::Numeric {
            position,
            rotation,
            preset,
            ..
        } = &mut dialog.draft
        else {
            unreachable!()
        };
        apply_numeric_preset(current, PosePreset::Turn90Z, position, rotation, preset).unwrap();
        let turned = preset.as_ref().unwrap().pose;
        ctx.run_ui(egui::RawInput::default(), |ui| app.show_placement(ui.ctx()))
            .drop_without_applying_deltas();
        assert_eq!(app.editor.preview().unwrap().boards[0].pose, turned);
        let PlacementDraft::Numeric { position, .. } = &mut app.modals.placement_mut().unwrap().draft
        else {
            unreachable!()
        };
        position[1].text = "23 mm".into();
        ctx.run_ui(egui::RawInput::default(), |ui| app.show_placement(ui.ctx()))
            .drop_without_applying_deltas();
        assert_eq!(
            app.editor.preview().unwrap().boards[0].pose.rotation,
            turned.rotation
        );
        assert_eq!(
            app.editor.preview().unwrap().boards[0].pose.translation_mm[1],
            23.0
        );
        let mut escape = egui::RawInput::default();
        escape.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        ctx.run_ui(escape, |ui| app.show_placement(ui.ctx()))
            .drop_without_applying_deltas();
        assert!(app.modals.placement().is_none());
        assert!(app.editor.preview().is_none());
        assert_eq!(app.editor.project(), &original);
        assert_eq!(app.selection.ids, original_selection);
        assert_eq!(app.selection.active, Some(other_id));
    }

    #[test]
    fn invalid_pending_position_cannot_be_reinterpreted_by_frame_switch() {
        let project = plan_my_cabinet::reference_fixture::project();
        let board_id = project.boards[0].id;
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        app.modals.set_placement(PlacementDialog::numeric(&app, board_id));
        let before = app.editor.project().clone();
        let PlacementDraft::Numeric {
            position, frame, ..
        } = &mut app.modals.placement_mut().unwrap().draft
        else {
            unreachable!()
        };
        position[0].text = "not a position".into();
        *frame = CoordinateFrame::World;
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| app.show_placement(ui.ctx()))
            .drop_without_applying_deltas();
        let PlacementDraft::Numeric {
            position, frame, ..
        } = &app.modals.placement().unwrap().draft
        else {
            unreachable!()
        };
        assert_eq!(*frame, CoordinateFrame::LocalParent);
        assert_eq!(position[0].text, "not a position");
        assert_eq!(app.editor.project(), &before);
        assert!(app.editor.preview().is_none());
    }

    #[test]
    fn valid_pending_position_converts_frames_without_losing_its_proposal() {
        let project = plan_my_cabinet::reference_fixture::project();
        let board_id = project.boards[0].id;
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        app.modals.set_placement(PlacementDialog::numeric(&app, board_id));
        let before = app.editor.project().clone();
        let PlacementDraft::Numeric {
            position, frame, ..
        } = &mut app.modals.placement_mut().unwrap().draft
        else {
            unreachable!()
        };
        position[0].text = "29 mm".into();
        *frame = CoordinateFrame::World;
        let PlacementDraft::Numeric {
            position, rotation, ..
        } = &app.modals.placement().unwrap().draft
        else {
            unreachable!()
        };
        let expected_world = numeric_candidate_world(
            &app.editor,
            board_id,
            CoordinateFrame::LocalParent,
            position,
            rotation,
            None,
        )
        .unwrap();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| app.show_placement(ui.ctx()))
            .drop_without_applying_deltas();
        let PlacementDraft::Numeric {
            position, frame, ..
        } = &app.modals.placement().unwrap().draft
        else {
            unreachable!()
        };
        assert_eq!(*frame, CoordinateFrame::World);
        assert_eq!(position[0].text, mm_text(expected_world.translation_mm[0]));
        assert_eq!(
            plan_my_cabinet::placement::world_pose(app.editor.preview().unwrap(), board_id)
                .unwrap(),
            expected_world
        );
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn untouched_half_grid_origin_survives_preset_and_frame_switch() {
        let project = plan_my_cabinet::reference_fixture::project();
        let board_id = project.boards[0].id;
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        app.editor
            .transact(|project| {
                project.boards[0].pose.translation_mm = [0.0005, -1.0005, 3.0005];
                Ok::<_, ()>(())
            })
            .unwrap();
        let original = app.editor.project().clone();
        app.modals.set_placement(PlacementDialog::numeric(&app, board_id));
        let PlacementDraft::Numeric {
            position,
            rotation,
            preset,
            ..
        } = &mut app.modals.placement_mut().unwrap().draft
        else {
            unreachable!()
        };
        apply_numeric_preset(
            original.boards[0].pose,
            PosePreset::StandUp,
            position,
            rotation,
            preset,
        )
        .unwrap();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| app.show_placement(ui.ctx()))
            .drop_without_applying_deltas();
        let preview =
            plan_my_cabinet::placement::world_pose(app.editor.preview().unwrap(), board_id)
                .unwrap();
        assert_eq!(
            app.editor.preview().unwrap().boards[0].pose.translation_mm,
            original.boards[0].pose.translation_mm
        );
        let PlacementDraft::Numeric { frame, .. } = &mut app.modals.placement_mut().unwrap().draft
        else {
            unreachable!()
        };
        *frame = CoordinateFrame::World;
        ctx.run_ui(egui::RawInput::default(), |ui| app.show_placement(ui.ctx()))
            .drop_without_applying_deltas();
        assert_eq!(
            plan_my_cabinet::placement::world_pose(app.editor.preview().unwrap(), board_id)
                .unwrap(),
            preview
        );
        assert_eq!(app.editor.project(), &original);
    }

    #[test]
    fn switching_frames_twice_in_rotated_parent_does_not_quantize_or_edit() {
        let project = plan_my_cabinet::reference_fixture::project();
        let board_id = project.boards[0].id;
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        app.editor
            .transact(|project| {
                project.assemblies[0].pose.rotation =
                    Quaternion::normalized(0.9, 0.1, 0.2, 0.3).unwrap();
                project.boards[0].pose = Pose::new(
                    [0.0005, -1.0005, 3.0005],
                    Quaternion::normalized(0.91, 0.12, 0.2, 0.31).unwrap(),
                )
                .unwrap();
                Ok::<_, ()>(())
            })
            .unwrap();
        let original = app.editor.project().clone();
        app.modals.set_placement(PlacementDialog::numeric(&app, board_id));
        let ctx = egui::Context::default();
        for frame_choice in [CoordinateFrame::World, CoordinateFrame::LocalParent] {
            let PlacementDraft::Numeric { frame, .. } = &mut app.modals.placement_mut().unwrap().draft
            else {
                unreachable!()
            };
            *frame = frame_choice;
            ctx.run_ui(egui::RawInput::default(), |ui| app.show_placement(ui.ctx()))
                .drop_without_applying_deltas();
        }
        assert_eq!(app.editor.project(), &original);
        assert!(
            app.editor.preview().is_none(),
            "a frame-only round trip must not propose a model edit"
        );
    }
}

#[cfg(test)]
mod reference_dialog_tests {
    use super::*;
    use egui::{Event, Key, Modifiers, RawInput};

    fn app() -> DesktopApp {
        let editor = ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap();
        DesktopApp {
            editor,
            ..Default::default()
        }
    }

    fn render(
        app: &mut DesktopApp,
        ctx: &egui::Context,
        size: egui::Vec2,
        events: Vec<Event>,
    ) -> Vec<(String, egui::Rect)> {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| {
                app.show_batch_dimension(ui.ctx());
                app.show_placement(ui.ctx());
            },
        );
        let buttons = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                (node.role() == egui::accesskit::Role::Button && !node.is_disabled())
                    .then(|| {
                        let bounds = node.bounds()?;
                        Some((
                            node.label()?.to_owned(),
                            egui::Rect::from_min_max(
                                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                            ),
                        ))
                    })
                    .flatten()
            })
            .collect();
        output.drop_without_applying_deltas();
        buttons
    }

    fn click(app: &mut DesktopApp, ctx: &egui::Context, size: egui::Vec2, label: &str) {
        let buttons = render(app, ctx, size, vec![]);
        let pos = buttons
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing enabled button {label}: {buttons:?}"))
            .1
            .center();
        render(
            app,
            ctx,
            size,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        render(
            app,
            ctx,
            size,
            vec![Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
    }

    fn key(key: Key) -> Vec<Event> {
        vec![Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]
    }

    #[test]
    fn position_pointer_apply_and_escape_at_reference_and_compact_sizes() {
        for size in [egui::vec2(1440.0, 900.0), egui::vec2(900.0, 650.0)] {
            let mut app = app();
            let id = app.editor.project().boards[0].id;
            let before = app.editor.project().clone();
            app.modals.set_placement(PlacementDialog::numeric(&app, id));
            let PlacementDraft::Numeric { position, .. } =
                &mut app.modals.placement_mut().unwrap().draft
            else {
                unreachable!()
            };
            position[0].text = "bad".into();
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let buttons = render(&mut app, &ctx, size, key(Key::Enter));
            assert!(!buttons.iter().any(|(label, _)| label == "Apply"));
            assert_eq!(app.editor.project(), &before);
            let PlacementDraft::Numeric { position, .. } =
                &mut app.modals.placement_mut().unwrap().draft
            else {
                unreachable!()
            };
            position[0].text = "29 mm".into();
            render(&mut app, &ctx, size, vec![]);
            assert!(app.editor.preview().is_some());
            assert_eq!(app.editor.project(), &before);
            click(&mut app, &ctx, size, "Apply");
            assert!(app.modals.placement().is_none());
            assert_eq!(app.editor.project().boards[0].pose.translation_mm[0], 29.0);
            assert!(app.editor.undo().unwrap());
            assert_eq!(app.editor.project().boards[0].pose, before.boards[0].pose);
            app.modals.set_placement(PlacementDialog::numeric(&app, id));
            let PlacementDraft::Numeric { position, .. } =
                &mut app.modals.placement_mut().unwrap().draft
            else {
                unreachable!()
            };
            position[0].text = "39 mm".into();
            render(&mut app, &ctx, size, vec![]);
            render(&mut app, &ctx, size, key(Key::Escape));
            assert!(app.modals.placement().is_none());
            assert!(app.editor.preview().is_none());
            assert_eq!(app.editor.project().boards[0].pose, before.boards[0].pose);
        }
    }

    #[test]
    fn face_pointer_place_and_cancel_preserve_atomic_preview() {
        for size in [egui::vec2(1440.0, 900.0), egui::vec2(900.0, 650.0)] {
            let mut app = app();
            let id = app.editor.project().boards[0].id;
            let before = app.editor.project().clone();
            app.modals.set_placement(PlacementDialog::face(&app, id));
            let PlacementDraft::Face { gap, .. } = &mut app.modals.placement_mut().unwrap().draft
            else {
                unreachable!()
            };
            gap.text = "not a length".into();
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let buttons = render(&mut app, &ctx, size, key(Key::Enter));
            assert!(!buttons.iter().any(|(label, _)| label == "Place"));
            assert_eq!(app.editor.project(), &before);
            let PlacementDraft::Face { gap, .. } = &mut app.modals.placement_mut().unwrap().draft
            else {
                unreachable!()
            };
            gap.text = "2 mm".into();
            render(&mut app, &ctx, size, vec![]);
            assert!(app.editor.preview().is_some());
            assert_eq!(app.editor.project(), &before);
            click(&mut app, &ctx, size, "Place");
            assert!(app.modals.placement().is_none());
            assert_ne!(app.editor.project().boards[0].pose, before.boards[0].pose);
            assert!(app.editor.undo().unwrap());
            assert_eq!(app.editor.project().boards[0].pose, before.boards[0].pose);
            app.modals.set_placement(PlacementDialog::face(&app, id));
            render(&mut app, &ctx, size, vec![]);
            render(&mut app, &ctx, size, key(Key::Escape));
            assert!(app.modals.placement().is_none());
            assert!(app.editor.preview().is_none());
        }
    }

    #[test]
    fn batch_mixed_rounding_requires_consent_and_commits_once() {
        for size in [egui::vec2(1440.0, 900.0), egui::vec2(900.0, 650.0)] {
            let mut app = app();
            let project = app.editor.project().clone();
            let ids: Vec<_> = [0, 2].map(|i| project.boards[i].id).to_vec();
            app.modals.set_batch_dimension(Some(BatchDialog {
                focus_on_open: true,
                project_id: project.id,
                revision: project.revision,
                ids: ids.clone(),
                dimension: BoardDimension::Length,
                value: DimensionDraft {
                    text: "1/64 in".into(),
                    consent: false,
                },
                anchors: ids.iter().map(|id| (*id, Anchor::Centre)).collect(),
                error: None,
            }));
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let buttons = render(&mut app, &ctx, size, key(Key::Enter));
            assert!(!buttons.iter().any(|(label, _)| label == "Resize 2 boards"));
            assert_eq!(app.editor.project(), &project);
            app.modals.batch_dimension_mut().unwrap().value.consent = true;
            render(&mut app, &ctx, size, vec![Event::Text("2".into())]);
            assert!(
                !app.modals.batch_dimension().unwrap().value.consent,
                "editing after rounding consent must require fresh consent"
            );
            app.modals.batch_dimension_mut().unwrap().value.text = "1/64 in".into();
            app.modals.batch_dimension_mut().unwrap().value.consent = true;
            let buttons = render(&mut app, &ctx, size, vec![]);
            assert!(buttons.iter().any(|(label, _)| label == "Resize 2 boards"));
            click(&mut app, &ctx, size, "Resize 2 boards");
            assert!(app.modals.batch_dimension().is_none());
            assert_eq!(app.editor.project().boards[0].length.micrometres(), 397);
            assert_eq!(app.editor.project().boards[2].length.micrometres(), 397);
            assert!(app.editor.undo().unwrap());
            assert_eq!(app.editor.project().boards, project.boards);
            app.modals.set_batch_dimension(Some(BatchDialog {
                focus_on_open: true,
                project_id: project.id,
                revision: app.editor.project().revision,
                ids: ids.clone(),
                dimension: BoardDimension::Length,
                value: DimensionDraft {
                    text: "600 mm".into(),
                    consent: false,
                },
                anchors: ids.iter().map(|id| (*id, Anchor::Centre)).collect(),
                error: None,
            }));
            render(&mut app, &ctx, size, vec![]);
            render(&mut app, &ctx, size, key(Key::Escape));
            assert!(app.modals.batch_dimension().is_none());
            assert_eq!(app.editor.project().boards, project.boards);
        }
    }
}
