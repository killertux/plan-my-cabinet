//! Edge banding in the Design workspace: the inspector's banding section
//! (an edge diagram, the band to use and quick presets), the Edge bands list
//! in the outliner, and the dialog that creates or edits a band.
use crate::actions::{ActionId as A, Argument, Request, Target};
use crate::hinge_ui::field_label;
use crate::modal_chrome::{ModalAction, ModalActions, ModalChrome};
use crate::theme_widgets as tw;
use crate::*;
use plan_my_cabinet::banding::{BandingPreset, band_lengths, band_usage};
use plan_my_cabinet::banding_rules::{self, Contact, EdgeState, FRONT};
use plan_my_cabinet::board_frame::{BoardFrame, edge_length};
use plan_my_cabinet::domain::{BoardEdge, EdgeBand, EdgeBanding, MaterialKind, SrgbColor};

/// Band colours offered for a new band.
const BAND_SWATCHES: &[(&str, SrgbColor)] = &[
    ("color-white", SrgbColor([244, 242, 238])),
    ("color-off-white", SrgbColor([234, 228, 214])),
    ("color-light-grey", SrgbColor([200, 200, 196])),
    ("color-graphite", SrgbColor([92, 92, 96])),
    ("color-black", SrgbColor([44, 42, 42])),
    ("color-raw-mdf", SrgbColor([196, 166, 126])),
    ("color-oak", SrgbColor([214, 180, 132])),
    ("color-freijo", SrgbColor([184, 142, 92])),
    ("color-walnut", SrgbColor([124, 88, 60])),
];

pub(crate) fn kind_key(kind: MaterialKind) -> &'static str {
    match kind {
        MaterialKind::Mdf => "material-kind-mdf",
        MaterialKind::Mdp => "material-kind-mdp",
        MaterialKind::Hdf => "material-kind-hdf",
        MaterialKind::Plywood => "material-kind-plywood",
        MaterialKind::SolidWood => "material-kind-solid",
        MaterialKind::Other => "material-kind-other",
    }
}

fn color32(color: SrgbColor) -> egui::Color32 {
    let [r, g, b] = color.0;
    egui::Color32::from_rgb(r, g, b)
}

/// A length in millimetres without trailing zeros or unit: "22", "0,45".
pub(crate) fn mm_text(value: plan_my_cabinet::units::Length, locale: Locale) -> String {
    let text = format_length(value, Unit::Mm, locale, 3);
    let number = text.trim_end_matches(" mm");
    if number.contains(['.', ',']) {
        number
            .trim_end_matches('0')
            .trim_end_matches(['.', ','])
            .to_owned()
    } else {
        number.to_owned()
    }
}

/// "1 × 22 mm" for a band's size.
pub(crate) fn band_size(band: &EdgeBand, locale: Locale) -> String {
    format!(
        "{} × {} mm",
        mm_text(band.thickness, locale),
        mm_text(band.height, locale)
    )
}

impl DesktopApp {
    fn banding_locale(&self) -> Locale {
        if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        }
    }

    /// The band a click or preset puts on `board`: the one picked in the
    /// inspector, else the material's default band, else the first band.
    pub(crate) fn click_band(&self, board: Uuid) -> Option<Uuid> {
        let project = self.editor.project();
        self.design
            .banding_band
            .filter(|id| project.edge_band(*id).is_some())
            .or_else(|| {
                project
                    .board(board)
                    .and_then(|b| project.material(b.material_id))
                    .and_then(|m| m.default_band)
            })
            .or_else(|| project.edge_bands.first().map(|b| b.id))
    }

    /// The boards a banding action works on: its board, or the selected boards.
    pub(crate) fn banding_targets(&self, target: Target) -> Vec<Uuid> {
        let project = self.editor.project();
        match target {
            Target::Board(id) => vec![id],
            _ => project
                .boards
                .iter()
                .filter(|b| self.selection.ids.contains(&b.id))
                .map(|b| b.id)
                .collect(),
        }
    }

    /// Tell the user about boards a banding edit left alone.
    pub(crate) fn report_banding(
        &mut self,
        result: Result<
            plan_my_cabinet::banding::BandingOutcome,
            plan_my_cabinet::commands::EditError<plan_my_cabinet::banding::BandingError>,
        >,
    ) {
        match result {
            Ok(outcome) if !outcome.skipped.is_empty() => {
                let text = self
                    .localizer
                    .count("banding-skipped", outcome.skipped.len() as u64);
                self.toasts.info(text);
            }
            Ok(_) => {}
            Err(plan_my_cabinet::commands::EditError::Command(
                plan_my_cabinet::banding::BandingError::NotAccepted,
            )) => {
                let text = self.localizer.text("banding-none-accepted");
                self.toasts.error(text);
            }
            Err(error) => self.report_edit(Err::<(), _>(error)),
        }
    }

    /// Run a change that may drop banding (a material or kind change) and
    /// say how many banded edges it removed.
    pub(crate) fn note_removed_banding(&mut self, before: usize) {
        let after = plan_my_cabinet::banding::banded_edges(self.editor.project());
        if after < before {
            let text = self
                .localizer
                .count("banding-removed", (before - after) as u64);
            self.toasts.info(text);
        }
    }

    /// The banding section of the board inspector, for one or several boards.
    pub(crate) fn show_banding_section(&mut self, ui: &mut egui::Ui, boards: &[Uuid]) {
        let project = self.editor.project();
        let bandable: Vec<Uuid> = boards
            .iter()
            .copied()
            .filter(|id| {
                project
                    .board(*id)
                    .and_then(|b| project.material(b.material_id))
                    .is_some_and(|m| m.kind.accepts_banding())
            })
            .collect();
        tw::inspector_heading(ui, &self.localizer.text("banding-title"), |_| {});
        if bandable.is_empty() {
            let material = boards
                .first()
                .and_then(|id| project.board(*id))
                .and_then(|b| project.material(b.material_id));
            let text = match (boards.len(), material) {
                (1, Some(material)) => {
                    let mut args = fluent_bundle::FluentArgs::new();
                    args.set("material", material.name.clone());
                    args.set("kind", self.localizer.text(kind_key(material.kind)));
                    self.localizer.format("banding-not-accepted", Some(&args))
                }
                _ => self.localizer.text("banding-none-accepted"),
            };
            ui.label(egui::RichText::new(text).size(11.5).color(tw::FAINT));
            return;
        }
        let states: Vec<(Uuid, [EdgeState; 4])> = bandable
            .iter()
            .filter_map(|id| banding_rules::board_states(project, *id).map(|s| (*id, s)))
            .collect();
        let modal = self.modal_open();
        let mut pending: Option<Request> = None;
        let target = if boards.len() == 1 {
            Target::Board(boards[0])
        } else {
            Target::None
        };
        let band = self.click_band(bandable[0]);
        if boards.len() == 1 {
            pending = pending.or(self.show_edge_diagram(ui, bandable[0], &states[0].1, band));
        } else {
            pending = pending.or(self.show_edge_counts(ui, &states, band));
        }
        // The band new edges get.
        let mut chosen = band;
        tw::prop_row(ui, &self.localizer.text("banding-band"), 88.0, |ui| {
            ui.add_enabled_ui(!modal, |ui| {
                let project = self.editor.project();
                let selected = chosen
                    .and_then(|id| project.edge_band(id))
                    .map_or_else(|| "—".to_owned(), |b| b.name.clone());
                egui::ComboBox::from_id_salt(("banding-band", boards.first().copied()))
                    .width(ui.available_width() - 8.0)
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for band in &project.edge_bands {
                            ui.horizontal(|ui| {
                                tw::swatch(ui, color32(band.color), egui::vec2(10.0, 10.0));
                                if ui
                                    .selectable_label(chosen == Some(band.id), &band.name)
                                    .clicked()
                                {
                                    chosen = Some(band.id);
                                }
                            });
                        }
                        ui.separator();
                        if ui
                            .selectable_label(false, self.localizer.text("banding-new-band"))
                            .clicked()
                        {
                            pending = Some(Request::new(A::NewEdgeBand));
                        }
                    });
            });
        });
        if chosen != band {
            self.design.banding_band = chosen;
        }
        // Presets.
        let current = preset_of(&states);
        let mut preset = current;
        let options = [
            (
                Some(BandingPreset::Auto),
                self.localizer.text("banding-preset-auto"),
            ),
            (
                Some(BandingPreset::None),
                self.localizer.text("banding-preset-none"),
            ),
            (
                Some(BandingPreset::Front),
                self.localizer.text("banding-preset-front"),
            ),
            (
                Some(BandingPreset::AllFour),
                self.localizer.text("banding-preset-all"),
            ),
        ];
        let options: Vec<_> = options.iter().map(|(p, l)| (*p, l.as_str())).collect();
        ui.add_enabled_ui(!modal, |ui| {
            tw::segmented(ui, &mut preset, &options);
        });
        if preset != current
            && let Some(preset) = preset
        {
            pending = Some(
                Request::with(A::ApplyBandingPreset, target)
                    .argument(Argument::BandingPreset(preset)),
            );
        }
        self.show_banding_notes(ui, &states);
        if let Some(request) = pending {
            self.invoke_or_report(request);
        }
    }

    /// Warnings under the banding controls: a band lower than its board, and
    /// a material with no band for its automatic edges.
    fn show_banding_notes(&mut self, ui: &mut egui::Ui, states: &[(Uuid, [EdgeState; 4])]) {
        let project = self.editor.project();
        let locale = self.banding_locale();
        let mut short = None;
        let mut no_default = None;
        for (id, edges) in states {
            let Some(board) = project.board(*id) else {
                continue;
            };
            for state in edges {
                if let Some(band) = state.band.and_then(|b| project.edge_band(b))
                    && band.height < board.thickness
                {
                    short.get_or_insert((band.name.clone(), board.thickness));
                }
            }
            if let Some(material) = project.material(board.material_id)
                && material.default_band.is_none()
                && edges.iter().any(|s| s.setting == EdgeBanding::Auto)
            {
                no_default.get_or_insert((material.id, material.name.clone()));
            }
        }
        if let Some((band, thickness)) = short {
            let mut args = fluent_bundle::FluentArgs::new();
            args.set("band", band);
            args.set("thickness", format!("{} mm", mm_text(thickness, locale)));
            ui.label(
                egui::RichText::new(self.localizer.format("banding-short-band", Some(&args)))
                    .size(11.5)
                    .color(tw::WARN_INK),
            );
        }
        if let Some((material, name)) = no_default {
            let mut args = fluent_bundle::FluentArgs::new();
            args.set("material", name);
            ui.label(
                egui::RichText::new(self.localizer.format("banding-no-default", Some(&args)))
                    .size(11.5)
                    .color(tw::WARN_INK),
            );
            if tw::text_button(
                ui,
                &self.localizer.text("banding-set-default"),
                tw::ACCENT_DARK,
                !self.modal_open(),
            )
            .clicked()
            {
                self.invoke_or_report(Request::with(A::EditMaterial, Target::Material(material)));
            }
        }
    }

    /// A single board's four edges around a rectangle in its proportions.
    /// Returns the action a click asked for.
    fn show_edge_diagram(
        &mut self,
        ui: &mut egui::Ui,
        id: Uuid,
        states: &[EdgeState; 4],
        band: Option<Uuid>,
    ) -> Option<Request> {
        let project = self.editor.project();
        let board = project.board(id)?;
        let locale = self.banding_locale();
        let front = BoardFrame::new(project, id)
            .map(|frame| banding_rules::edges_facing(&frame, FRONT))
            .unwrap_or_default();
        let (length, width) = (board.length, board.width);
        let available = ui.available_width().max(120.0);
        let aspect =
            (width.micrometres() as f32 / length.micrometres().max(1) as f32).clamp(0.12, 1.6);
        let face_width = (available - 32.0).max(60.0);
        let face_height = (face_width * aspect).clamp(48.0, 120.0);
        let (outer, _) = ui.allocate_exact_size(
            egui::vec2(available, face_height + 30.0),
            egui::Sense::hover(),
        );
        let face =
            egui::Rect::from_center_size(outer.center(), egui::vec2(face_width, face_height));
        let painter = ui.painter_at(outer);
        painter.rect_filled(
            face,
            2.0,
            color32(project.material_color(board.material_id)),
        );
        painter.rect_stroke(
            face,
            2.0,
            egui::Stroke::new(1.0, tw::BORDER),
            egui::StrokeKind::Inside,
        );
        let bar = 8.0;
        let gap = 3.0;
        let mut request = None;
        for edge in BoardEdge::ALL {
            let state = states[edge.index()];
            // X runs to the right, Y upward: MinY is the bottom bar.
            let rect = match edge {
                BoardEdge::MinX => egui::Rect::from_min_max(
                    egui::pos2(face.left() - gap - bar, face.top()),
                    egui::pos2(face.left() - gap, face.bottom()),
                ),
                BoardEdge::MaxX => egui::Rect::from_min_max(
                    egui::pos2(face.right() + gap, face.top()),
                    egui::pos2(face.right() + gap + bar, face.bottom()),
                ),
                BoardEdge::MinY => egui::Rect::from_min_max(
                    egui::pos2(face.left(), face.bottom() + gap),
                    egui::pos2(face.right(), face.bottom() + gap + bar),
                ),
                BoardEdge::MaxY => egui::Rect::from_min_max(
                    egui::pos2(face.left(), face.top() - gap - bar),
                    egui::pos2(face.right(), face.top() - gap),
                ),
            };
            let response = ui.interact(
                rect.expand(3.0),
                egui::Id::new(("banding-edge", id, edge.index())),
                if self.modal_open() {
                    egui::Sense::hover()
                } else {
                    egui::Sense::click()
                },
            );
            let band_record = state.band.and_then(|b| project.edge_band(b));
            let fill = band_record.map_or(tw::PANEL, |b| color32(b.color));
            painter.rect_filled(rect, 2.0, fill);
            let stroke_color = if response.hovered() {
                tw::ACCENT
            } else if band_record.is_some() {
                egui::Color32::from_rgb(26, 148, 128)
            } else {
                tw::BORDER_STRONG
            };
            let stroke =
                egui::Stroke::new(if band_record.is_some() { 1.6 } else { 1.0 }, stroke_color);
            if state.is_manual() {
                painter.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Outside);
            } else {
                let corners = [
                    rect.left_top(),
                    rect.right_top(),
                    rect.right_bottom(),
                    rect.left_bottom(),
                    rect.left_top(),
                ];
                for pair in corners.windows(2) {
                    painter.extend(egui::Shape::dashed_line(
                        &[pair[0], pair[1]],
                        stroke,
                        4.0,
                        3.0,
                    ));
                }
            }
            // Edge length, and the automatic badge and front tick.
            let edge_mm = mm_text(edge_length(edge, length, width), locale);
            let mut label = edge_mm.clone();
            if front.contains(&edge) {
                label = format!("{label} · {}", self.localizer.text("banding-front"));
            }
            if !state.is_manual() {
                label = format!("{label} · A");
            }
            let font = egui::FontId::proportional(10.0);
            // Labels sit inside the face, next to their bar.
            let (pos, align) = match edge {
                BoardEdge::MinY => (
                    face.center_bottom() - egui::vec2(0.0, 4.0),
                    egui::Align2::CENTER_BOTTOM,
                ),
                BoardEdge::MaxY => (
                    face.center_top() + egui::vec2(0.0, 4.0),
                    egui::Align2::CENTER_TOP,
                ),
                BoardEdge::MinX => (
                    face.left_center() + egui::vec2(5.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                ),
                BoardEdge::MaxX => (
                    face.right_center() - egui::vec2(5.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                ),
            };
            let short = if matches!(edge, BoardEdge::MinX | BoardEdge::MaxX) {
                // Side labels have little room: length only, with the badge.
                if state.is_manual() {
                    edge_mm
                } else {
                    format!("{edge_mm} A")
                }
            } else {
                label
            };
            painter.text(pos, align, short, font, tw::MUTED);
            let hover = self.edge_description(board.length, board.width, edge, &state);
            let response = response.on_hover_text(hover);
            if response.clicked() {
                request = Some(match band {
                    Some(_) => Request::with(A::ToggleBanding, Target::Board(id))
                        .argument(Argument::Edge(edge)),
                    None => Request::new(A::NewEdgeBand),
                });
            }
            response.context_menu(|ui| {
                if ui
                    .add_enabled(
                        state.is_manual(),
                        egui::Button::new(self.localizer.text("banding-back-to-auto")),
                    )
                    .clicked()
                {
                    request = Some(Request::with(A::SetBanding, Target::Board(id)).argument(
                        Argument::EdgeSetting {
                            edge,
                            value: EdgeBanding::Auto,
                        },
                    ));
                    ui.close();
                }
            });
        }
        ui.label(
            egui::RichText::new(self.localizer.text("banding-hint"))
                .size(11.0)
                .color(tw::FAINT),
        );
        request
    }

    /// Several boards: per edge, how many of them are banded. A click bands
    /// that edge on all of them, or takes it off when all are banded.
    fn show_edge_counts(
        &mut self,
        ui: &mut egui::Ui,
        states: &[(Uuid, [EdgeState; 4])],
        band: Option<Uuid>,
    ) -> Option<Request> {
        let mut request = None;
        let total = states.len() as u64;
        for (edge, key) in [
            (BoardEdge::MinY, "banding-edge-min-y"),
            (BoardEdge::MaxY, "banding-edge-max-y"),
            (BoardEdge::MinX, "banding-edge-min-x"),
            (BoardEdge::MaxX, "banding-edge-max-x"),
        ] {
            let count = states
                .iter()
                .filter(|(_, s)| s[edge.index()].band.is_some())
                .count() as u64;
            let mut args = fluent_bundle::FluentArgs::new();
            args.set("count", count);
            args.set("total", total);
            let value = self.localizer.format("banding-mixed", Some(&args));
            let label = self.localizer.text(key);
            let all = count == total;
            tw::prop_row(ui, &label, 88.0, |ui| {
                let response = ui.add_enabled(
                    !self.modal_open(),
                    egui::Button::new(egui::RichText::new(value).size(12.0))
                        .selected(all)
                        .min_size(egui::vec2(ui.available_width() - 8.0, 24.0)),
                );
                if response.clicked() {
                    let value = if all {
                        EdgeBanding::Off
                    } else {
                        match band {
                            Some(band) => EdgeBanding::On(band),
                            None => {
                                request = Some(Request::new(A::NewEdgeBand));
                                return;
                            }
                        }
                    };
                    request = Some(
                        Request::with(A::SetBanding, Target::None)
                            .argument(Argument::EdgeSetting { edge, value }),
                    );
                }
                response.context_menu(|ui| {
                    if ui
                        .button(self.localizer.text("banding-back-to-auto"))
                        .clicked()
                    {
                        request = Some(Request::with(A::SetBanding, Target::None).argument(
                            Argument::EdgeSetting {
                                edge,
                                value: EdgeBanding::Auto,
                            },
                        ));
                        ui.close();
                    }
                });
            });
        }
        request
    }

    /// Hover text for one edge: its length and why it is (not) banded.
    fn edge_description(
        &self,
        length: plan_my_cabinet::units::Length,
        width: plan_my_cabinet::units::Length,
        edge: BoardEdge,
        state: &EdgeState,
    ) -> String {
        let project = self.editor.project();
        let locale = self.banding_locale();
        let band_name = |id: Option<Uuid>| {
            id.and_then(|b| project.edge_band(b))
                .map_or_else(|| "—".to_owned(), |b| b.name.clone())
        };
        let board_name = |id: Uuid| {
            project
                .board(id)
                .map_or("—", |b| b.name.as_str())
                .to_owned()
        };
        let mut args = fluent_bundle::FluentArgs::new();
        args.set(
            "length",
            format!("{} mm", mm_text(edge_length(edge, length, width), locale)),
        );
        args.set("band", band_name(state.band));
        let key = match (state.setting, state.contact) {
            (EdgeBanding::On(_), _) => "banding-manual-on",
            (EdgeBanding::Off, _) => "banding-manual-off",
            (EdgeBanding::Auto, Contact::Joined { board, .. }) => {
                args.set("board", board_name(board));
                "banding-auto-joined"
            }
            (EdgeBanding::Auto, _) if state.band.is_none() => "banding-auto-no-default",
            (EdgeBanding::Auto, Contact::Partly { board, .. }) => {
                args.set("board", board_name(board));
                "banding-auto-partly"
            }
            (EdgeBanding::Auto, Contact::Free) => "banding-auto-free",
        };
        self.localizer.format(key, Some(&args))
    }

    /// The Edge bands list in the Design outliner.
    pub(crate) fn show_edge_band_list(&mut self, ui: &mut egui::Ui) {
        let project = self.editor.project();
        let locale = self.banding_locale();
        let lengths = band_lengths(project);
        let rows: Vec<(Uuid, String, String, egui::Color32, usize, i128)> = project
            .edge_bands
            .iter()
            .map(|band| {
                let (boards, _) = band_usage(project, band.id);
                let metres = lengths
                    .iter()
                    .find(|(id, _)| *id == band.id)
                    .map_or(0, |(_, um)| *um);
                (
                    band.id,
                    band.name.clone(),
                    band_size(band, locale),
                    color32(band.color),
                    boards.len(),
                    metres,
                )
            })
            .collect();
        let modal = self.modal_open();
        let mut run = None;
        let (open, ()) = tw::collapsible_section_bar(
            ui,
            egui::Id::new("design-edge-bands-section"),
            &self.localizer.text("edge-bands-title"),
            rows.len(),
            |ui| {
                let request = Request::new(A::NewEdgeBand);
                if tw::ghost_icon_sized(
                    ui,
                    crate::icons::Icon::Plus,
                    &request.id.label(&self.localizer),
                    tw::MUTED,
                    15.0,
                    24.0,
                    !modal,
                    false,
                )
                .clicked()
                {
                    run = Some(request);
                }
            },
        );
        if open {
            ui.spacing_mut().item_spacing.y = 1.0;
            if rows.is_empty() {
                ui.label(
                    egui::RichText::new(self.localizer.text("edge-bands-empty"))
                        .size(11.5)
                        .color(tw::FAINT),
                );
            }
            for (id, name, size, color, _, micrometres) in rows {
                let metres = micrometres as f64 / 1_000_000.0;
                let usage = if micrometres == 0 {
                    self.localizer.text("edge-band-unused")
                } else {
                    format!("{metres:.1} m")
                };
                let (response, ()) = tw::list_row(
                    ui,
                    egui::Id::new(("edge-band-row", id)),
                    30.0,
                    tw::RowState::Normal,
                    !modal,
                    &name,
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 7.0;
                        ui.add_space(4.0);
                        tw::swatch(ui, color, egui::vec2(12.0, 12.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(tw::mono(&usage, 11.0).color(tw::FAINT));
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&name).size(12.5).color(tw::TEXT_2),
                                )
                                .truncate()
                                .selectable(false),
                            );
                        });
                    },
                );
                let response = response.on_hover_text(size);
                if response.clicked() || response.double_clicked() {
                    run = Some(Request::with(A::EditEdgeBand, Target::Band(id)));
                }
                response.context_menu(|ui| {
                    if ui.button(A::EditEdgeBand.label(&self.localizer)).clicked() {
                        run = Some(Request::with(A::EditEdgeBand, Target::Band(id)));
                        ui.close();
                    }
                    let remove = Request::with(A::RemoveEdgeBand, Target::Band(id));
                    let allowed = self.action_availability(remove);
                    let response = ui.add_enabled(
                        allowed.is_ok(),
                        egui::Button::new(A::RemoveEdgeBand.label(&self.localizer)),
                    );
                    if let Err(reason) = allowed {
                        response.on_disabled_hover_text(reason.reason(self.localizer.language()));
                    } else if response.clicked() {
                        run = Some(remove);
                        ui.close();
                    }
                });
            }
        }
        if let Some(request) = run {
            self.invoke_or_report(request);
        }
    }

    pub(crate) fn show_edge_band_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.modals.take_edge_band() else {
            return;
        };
        let title = self.localizer.text(if dialog.id.is_some() {
            "edge-band-edit"
        } else {
            "edge-band-new"
        });
        let mut chrome = dialog.chrome.detach();
        let result = chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &title,
            },
            |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                field_label(ui, &self.localizer.text("edge-band-name"));
                ui.add(
                    egui::TextEdit::singleline(&mut dialog.name)
                        .id(egui::Id::new("edge-band-name"))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);
                let thickness = dialog.thickness.value(Unit::Mm);
                let height = dialog.height.value(Unit::Mm);
                let invalid = |value: &Result<plan_my_cabinet::units::Length, _>| {
                    value.as_ref().map_or(true, |v| v.micrometres() <= 0)
                };
                let thickness_error = (!dialog.thickness.text.trim().is_empty()
                    && invalid(&thickness))
                .then(|| self.localizer.text("error-non-positive-dimension"));
                let height_error = (!dialog.height.text.trim().is_empty() && invalid(&height))
                    .then(|| self.localizer.text("error-non-positive-dimension"));
                let thickness_label = self.localizer.text("edge-band-thickness");
                field_label(ui, &thickness_label);
                tw::unit_field(
                    ui,
                    egui::Id::new("edge-band-thickness"),
                    &thickness_label,
                    &mut dialog.thickness.text,
                    "mm",
                    thickness_error.as_deref(),
                );
                let height_label = self.localizer.text("edge-band-height");
                field_label(ui, &height_label);
                tw::unit_field(
                    ui,
                    egui::Id::new("edge-band-height"),
                    &height_label,
                    &mut dialog.height.text,
                    "mm",
                    height_error.as_deref(),
                );
                ui.add_space(8.0);
                field_label(ui, &self.localizer.text("edge-band-color"));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                    for (key, color) in BAND_SWATCHES {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                        let selected = dialog.color == *color;
                        let painter = ui.painter();
                        if selected {
                            painter.rect_stroke(
                                rect.expand(3.0),
                                7.0,
                                egui::Stroke::new(2.0, tw::TEXT),
                                egui::StrokeKind::Outside,
                            );
                        }
                        painter.rect_filled(rect, 6.0, color32(*color));
                        painter.rect_stroke(
                            rect,
                            6.0,
                            egui::Stroke::new(1.0, tw::BORDER),
                            egui::StrokeKind::Inside,
                        );
                        if response.on_hover_text(self.localizer.text(key)).clicked() {
                            dialog.color = *color;
                        }
                    }
                });
                ui.label(
                    egui::RichText::new(self.localizer.text("edge-band-hint"))
                        .size(11.5)
                        .color(tw::FAINT),
                );
                let valid =
                    !dialog.name.trim().is_empty() && !invalid(&thickness) && !invalid(&height);
                ((), valid)
            },
        );
        dialog.chrome = chrome;
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            dialog.chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm)
            && let (Ok(thickness), Ok(height)) = (
                dialog.thickness.value(Unit::Mm),
                dialog.height.value(Unit::Mm),
            )
        {
            let saved = match dialog.id {
                Some(id) => self
                    .editor
                    .update_edge_band(EdgeBand {
                        id,
                        name: dialog.name.clone(),
                        thickness,
                        height,
                        color: dialog.color,
                    })
                    .map(|_| id),
                None => self
                    .editor
                    .create_edge_band(&dialog.name, thickness, height, dialog.color),
            };
            match saved {
                Ok(id) => {
                    if dialog.id.is_none() {
                        self.design.banding_band = Some(id);
                    }
                    dialog.chrome.close(ctx);
                    return;
                }
                Err(error) => self.report_edit(Err::<(), _>(error)),
            }
        }
        self.modals.set_edge_band(Some(dialog));
    }
}

/// The material's type and default band, for the material dialogs.
/// Returns whether either changed.
pub(crate) fn material_banding_fields(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    project: &plan_my_cabinet::domain::Project,
    salt: &str,
    kind: &mut MaterialKind,
    band: &mut Option<Uuid>,
) -> bool {
    use crate::modal_chrome::form;
    let before = (*kind, *band);
    let width = ui.available_width();
    form::field(ui, &localizer.text("material-kind"), |ui| {
        egui::ComboBox::from_id_salt((salt, "kind"))
            .width(width)
            .selected_text(localizer.text(kind_key(*kind)))
            .show_ui(ui, |ui| {
                for option in MaterialKind::ALL {
                    ui.selectable_value(kind, option, localizer.text(kind_key(option)));
                }
            });
    });
    if kind.accepts_banding() {
        form::gap(ui);
        form::field(ui, &localizer.text("material-default-band"), |ui| {
            egui::ComboBox::from_id_salt((salt, "band"))
                .width(width)
                .selected_text(
                    band.and_then(|id| project.edge_band(id))
                        .map_or_else(|| localizer.text("material-no-band"), |b| b.name.clone()),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(band, None, localizer.text("material-no-band"));
                    for option in &project.edge_bands {
                        ui.selectable_value(band, Some(option.id), &option.name);
                    }
                });
        });
        ui.label(
            egui::RichText::new(localizer.text("material-default-band-hint"))
                .size(11.5)
                .color(tw::FAINT),
        );
    } else {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("kind", localizer.text(kind_key(*kind)));
        ui.label(
            egui::RichText::new(localizer.format("material-kind-no-banding", Some(&args)))
                .size(11.5)
                .color(tw::FAINT),
        );
    }
    before != (*kind, *band)
}

/// The preset every board already matches, if any.
fn preset_of(states: &[(Uuid, [EdgeState; 4])]) -> Option<BandingPreset> {
    let all = |f: &dyn Fn(&EdgeState) -> bool| states.iter().all(|(_, s)| s.iter().all(f));
    if all(&|s| s.setting == EdgeBanding::Auto) {
        Some(BandingPreset::Auto)
    } else if all(&|s| s.setting == EdgeBanding::Off) {
        Some(BandingPreset::None)
    } else if all(&|s| matches!(s.setting, EdgeBanding::On(_))) {
        Some(BandingPreset::AllFour)
    } else {
        None
    }
}

/// Creates or edits one edge band.
pub(crate) struct EdgeBandDialog {
    pub(crate) id: Option<Uuid>,
    pub(crate) name: String,
    pub(crate) thickness: DimensionDraft,
    pub(crate) height: DimensionDraft,
    pub(crate) color: SrgbColor,
    chrome: ModalChrome,
}

impl EdgeBandDialog {
    fn chrome() -> ModalChrome {
        ModalChrome::new(egui::Id::new("edge-band-dialog"))
            .first_focus(egui::Id::new("edge-band-name"))
    }

    pub(crate) fn new(locale: Locale) -> Self {
        let mm = |value| DimensionDraft {
            text: mm_text(
                plan_my_cabinet::units::Length::from_micrometres(value),
                locale,
            ),
            consent: false,
        };
        Self {
            id: None,
            name: String::new(),
            thickness: mm(1_000),
            height: mm(22_000),
            color: BAND_SWATCHES[0].1,
            chrome: Self::chrome(),
        }
    }

    pub(crate) fn edit(band: &EdgeBand, locale: Locale) -> Self {
        let draft = |value| DimensionDraft {
            text: mm_text(value, locale),
            consent: false,
        };
        Self {
            id: Some(band.id),
            name: band.name.clone(),
            thickness: draft(band.thickness),
            height: draft(band.height),
            color: band.color,
            chrome: Self::chrome(),
        }
    }
}

#[cfg(test)]
mod tests;
