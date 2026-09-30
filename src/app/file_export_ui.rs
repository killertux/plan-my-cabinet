//! Handoff for part-list formats (CorteCloud today): the format choice, a
//! summary of what the file will say and what it leaves out, a parts
//! preview, and the save flow with its receipt. The workshop PDF keeps its
//! own reviewed flow in `export_flow`.
use crate::theme_widgets as tw;
use crate::*;
use plan_my_cabinet::formats::{self, ExportFormat, FileExportRecord, FormatOptions};
use plan_my_cabinet::machining::{MachiningOptions, OmissionReason, PilotHole};
use plan_my_cabinet::part_list::{PartList, PartListBlocked};

/// A part-list export in progress.
pub(crate) enum FileFlow {
    /// The save dialog is open.
    Choosing(ExportFormat, Receiver<Option<PathBuf>>),
    /// The file exists; asking before replacing it.
    Confirming(ExportFormat, PathBuf),
}

/// Screw pilot holes the user asks the shop to drill, for screws the
/// catalog does not size.
pub(crate) struct PilotDraft {
    pub(crate) enabled: bool,
    pub(crate) diameter: DimensionDraft,
    pub(crate) depth: DimensionDraft,
}

impl Default for PilotDraft {
    fn default() -> Self {
        Self {
            enabled: false,
            diameter: DimensionDraft {
                text: "2.5".into(),
                consent: false,
            },
            depth: DimensionDraft {
                text: "10".into(),
                consent: false,
            },
        }
    }
}

type PartListCache = (
    (Uuid, u64, FormatOptions),
    Result<PartList, PartListBlocked>,
);

#[derive(Default)]
pub(crate) struct FileExportState {
    pub(crate) format: Option<ExportFormat>,
    pub(crate) flow: Option<FileFlow>,
    pub(crate) pilot: PilotDraft,
    cache: Option<PartListCache>,
    pub(crate) overwrite_chrome: Option<ModalChrome>,
}

impl FileExportState {
    /// The format chosen in Handoff; the workshop PDF until another is picked.
    pub(crate) fn format(&self) -> ExportFormat {
        self.format.unwrap_or(ExportFormat::WorkshopPdf)
    }
}

impl DesktopApp {
    pub(crate) fn file_export_options(&self) -> FormatOptions {
        let pilot = &self.file_export.pilot;
        let screw_pilot = pilot
            .enabled
            .then(|| {
                let diameter = pilot.diameter.value(Unit::Mm).ok()?;
                let depth = pilot.depth.value(Unit::Mm).ok()?;
                (diameter.micrometres() > 0 && depth.micrometres() > 0)
                    .then_some(PilotHole { diameter, depth })
            })
            .flatten();
        FormatOptions {
            machining: MachiningOptions { screw_pilot },
        }
    }

    /// The part list for the current design and options, built once per edit.
    pub(crate) fn file_part_list(&mut self) -> Result<&PartList, &PartListBlocked> {
        let project = self.editor.project();
        let key = (project.id, project.revision, self.file_export_options());
        if self
            .file_export
            .cache
            .as_ref()
            .is_none_or(|(k, _)| *k != key)
        {
            let built = plan_my_cabinet::part_list::build(project, &key.2.machining);
            self.file_export.cache = Some((key, built));
        }
        self.file_export
            .cache
            .as_ref()
            .map(|(_, r)| r.as_ref())
            .expect("just built")
    }

    /// The two format cards at the top of the Handoff options.
    pub(crate) fn show_format_choice(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.vertical(|ui| {
                tw::inspector_heading(ui, &self.localizer.text("export-format"), |_| ())
            });
        });
        ui.spacing_mut().item_spacing.y = 6.0;
        let current = self.file_export.format();
        let mut chosen = None;
        for (format, description) in [
            (ExportFormat::WorkshopPdf, "export-format-pdf-description"),
            (
                ExportFormat::CorteCloudJson,
                "export-format-cortecloud-description",
            ),
        ] {
            let card = handoff_ui::radio_card(
                ui,
                egui::Id::new(("export-format", format.id())),
                current == format,
                None,
                &self.localizer.text(format.label_key()),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(self.localizer.text(description))
                                .size(12.0)
                                .color(tw::MUTED),
                        )
                        .wrap(),
                    );
                },
            );
            if card.is_some_and(|r| r.clicked()) && format != current {
                chosen = Some(format);
            }
        }
        if let Some(format) = chosen {
            self.invoke_or_report(
                Request::new(A::SetExportFormat).argument(Argument::Format(format)),
            );
        }
        ui.add_space(6.0);
    }

    /// Options and summary for a part-list format.
    pub(crate) fn show_file_export_options(&mut self, ui: &mut egui::Ui) {
        let heading = |ui: &mut egui::Ui, text: String| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.vertical(|ui| tw::inspector_heading(ui, &text, |_| ()));
            });
        };
        heading(ui, self.localizer.text("cortecloud-summary"));
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let mut fix = None;
        let summary = match self.file_part_list() {
            Err(blocked) => Err(blocked.clone()),
            Ok(list) => Ok(list.clone()),
        };
        match summary {
            Err(blocked) => {
                let text = match blocked {
                    PartListBlocked::NoBoards => self.localizer.text("cortecloud-no-boards"),
                    PartListBlocked::InvalidDesign(_) => {
                        self.localizer.text("cortecloud-invalid-design")
                    }
                };
                tw::warn_callout().show(ui, |ui| {
                    ui.label(egui::RichText::new(text).size(12.0).color(tw::WARN_INK));
                });
            }
            Ok(list) => {
                let project = self.editor.project();
                let banded = list
                    .groups
                    .iter()
                    .filter(|g| g.banding.iter().any(Option::is_some))
                    .map(|g| g.quantity())
                    .sum::<usize>();
                let drilled = list
                    .groups
                    .iter()
                    .filter(|g| !g.machining.is_empty())
                    .map(|g| g.quantity())
                    .sum::<usize>();
                let holes = list
                    .groups
                    .iter()
                    .map(|g| g.machining.face_drills.len() * g.quantity())
                    .sum::<usize>();
                let metres = plan_my_cabinet::banding::band_lengths(project)
                    .iter()
                    .map(|(_, um)| *um)
                    .sum::<i128>() as f64
                    / 1_000_000.0;
                let mut args = FluentArgs::new();
                args.set("parts", list.part_count() as u64);
                args.set("groups", list.groups.len() as u64);
                args.set("banded", banded as u64);
                args.set("metres", format!("{metres:.1}"));
                args.set("drilled", drilled as u64);
                args.set("holes", holes as u64);
                tw::card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for key in [
                        "cortecloud-parts",
                        "cortecloud-banded",
                        "cortecloud-drilled",
                    ] {
                        ui.label(
                            egui::RichText::new(self.localizer.format(key, Some(&args)))
                                .size(12.5)
                                .color(tw::TEXT_2),
                        );
                    }
                    // Per material: what the shop links to its stock.
                    let mut materials: Vec<(String, usize)> = Vec::new();
                    for group in &list.groups {
                        let label = format!(
                            "{} · {} mm",
                            group.material,
                            crate::banding_ui::mm_text(group.thickness, locale)
                        );
                        match materials.iter_mut().find(|(m, _)| *m == label) {
                            Some((_, n)) => *n += group.quantity(),
                            None => materials.push((label, group.quantity())),
                        }
                    }
                    for (material, count) in materials {
                        ui.label(tw::mono(format!("{count} × {material}"), 11.5).color(tw::MUTED));
                    }
                });
                if !list.omissions.is_empty() {
                    ui.add_space(4.0);
                    heading(ui, self.localizer.text("cortecloud-left-out"));
                    // One line per door or drawer and reason, holes added up.
                    let mut rows: Vec<(String, OmissionReason, usize, Option<HandoffFix>)> =
                        Vec::new();
                    for omission in &list.omissions {
                        let (name, route) = omission_target(project, omission.installation);
                        match rows
                            .iter_mut()
                            .find(|(n, r, _, _)| *n == name && *r == omission.reason)
                        {
                            Some(row) => row.2 += omission.holes,
                            None => rows.push((name, omission.reason, omission.holes, route)),
                        }
                    }
                    for (name, reason, holes, route) in rows {
                        let omission = plan_my_cabinet::machining::Omission {
                            installation: Uuid::nil(),
                            reason,
                            holes,
                        };
                        let mut args = FluentArgs::new();
                        args.set("name", name);
                        args.set("holes", omission.holes as u64);
                        let key = match omission.reason {
                            OmissionReason::HasIssues => "cortecloud-omit-issues",
                            OmissionReason::DoorNeedsReview => "cortecloud-omit-review",
                            OmissionReason::PilotSizeUnknown => "cortecloud-omit-pilot",
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.add(crate::icons::icon(
                                crate::icons::Icon::Warning,
                                tw::WARN,
                                12.0,
                            ));
                            ui.label(
                                egui::RichText::new(self.localizer.format(key, Some(&args)))
                                    .size(11.5)
                                    .color(tw::TEXT_2),
                            );
                            if let Some(route) = route
                                && omission.reason != OmissionReason::PilotSizeUnknown
                                && tw::text_button(
                                    ui,
                                    &self.localizer.text("handoff-fix-short"),
                                    tw::ACCENT_DARK,
                                    !self.modal_open(),
                                )
                                .clicked()
                            {
                                fix = Some(route);
                            }
                        });
                    }
                }
            }
        }
        // Screw pilots, for shops that drill them.
        ui.add_space(6.0);
        heading(ui, self.localizer.text("cortecloud-pilots"));
        let pilot = &mut self.file_export.pilot;
        handoff_ui::checkbox(
            ui,
            &mut pilot.enabled,
            &self.localizer.text("cortecloud-pilots-enable"),
        );
        if pilot.enabled {
            ui.horizontal(|ui| {
                for (draft, key, id) in [
                    (
                        &mut pilot.diameter,
                        "cortecloud-pilot-diameter",
                        "pilot-diameter",
                    ),
                    (&mut pilot.depth, "cortecloud-pilot-depth", "pilot-depth"),
                ] {
                    ui.vertical(|ui| {
                        ui.set_width(110.0);
                        let label = self.localizer.text(key);
                        ui.label(egui::RichText::new(&label).size(11.5).color(tw::MUTED));
                        let error = draft
                            .value(Unit::Mm)
                            .map_or(true, |v| v.micrometres() <= 0)
                            .then(|| self.localizer.text("error-non-positive-dimension"));
                        tw::unit_field(
                            ui,
                            egui::Id::new(id),
                            &label,
                            &mut draft.text,
                            "mm",
                            error.as_deref(),
                        );
                    });
                }
            });
        }
        ui.label(
            egui::RichText::new(self.localizer.text("cortecloud-pilots-hint"))
                .size(11.5)
                .color(tw::FAINT),
        );
        // How to import, and the last file written.
        ui.add_space(6.0);
        heading(ui, self.localizer.text("cortecloud-import"));
        ui.add(
            egui::Label::new(
                egui::RichText::new(self.localizer.text("cortecloud-import-steps"))
                    .size(12.0)
                    .color(tw::TEXT_2),
            )
            .wrap(),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(self.localizer.text("cortecloud-verify-note"))
                    .size(11.5)
                    .color(tw::FAINT),
            )
            .wrap(),
        );
        if let Some(record) = self.last_file_export(ExportFormat::CorteCloudJson).cloned() {
            ui.add_space(6.0);
            let current = record.is_current(self.editor.project(), &self.file_export_options());
            let mut args = FluentArgs::new();
            args.set(
                "file",
                record
                    .path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            );
            ui.label(
                egui::RichText::new(self.localizer.format(
                    if current {
                        "cortecloud-last-current"
                    } else {
                        "cortecloud-last-outdated"
                    },
                    Some(&args),
                ))
                .size(11.5)
                .color(if current { tw::OK_INK } else { tw::WARN_INK }),
            );
        }
        if let Some(route) = fix {
            self.apply_handoff_fix(route);
        }
    }

    pub(crate) fn last_file_export(&self, format: ExportFormat) -> Option<&FileExportRecord> {
        self.editor
            .project()
            .file_exports
            .iter()
            .rev()
            .find(|r| r.format == format)
    }

    /// The pinned footer: the export button, or what is happening.
    pub(crate) fn show_file_export_footer(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 8.0;
        if let Some(message) = &self.handoff.message {
            ui.add(
                egui::Label::new(egui::RichText::new(message).size(11.5).color(tw::MUTED)).wrap(),
            );
        }
        let format = self.file_export.format();
        let request = Request::new(A::ExportFile).argument(Argument::Format(format));
        let allowed = self.action_availability(request);
        if matches!(self.file_export.flow, Some(FileFlow::Choosing(..))) {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(12.0).color(tw::FAINT));
                ui.label(
                    egui::RichText::new(self.localizer.text("export-choosing"))
                        .size(12.0)
                        .color(tw::MUTED),
                );
            });
        }
        let response = ui.add_enabled(
            allowed.is_ok(),
            egui::Button::image_and_text(
                crate::icons::icon(crate::icons::Icon::Export, tw::PANEL, 14.0),
                tw::medium(ui, self.localizer.text("cortecloud-export-button"), 13.0)
                    .color(tw::PANEL),
            )
            .fill(tw::TEXT)
            .corner_radius(7)
            .min_size(egui::vec2(ui.available_width(), 32.0)),
        );
        if let Err(reason) = allowed {
            response.on_disabled_hover_text(reason.reason(self.localizer.language()));
        } else if response.clicked() {
            self.invoke_or_report(request);
        }
    }

    /// The parts the file will list, in the Handoff preview area.
    pub(crate) fn show_file_export_preview(&mut self, ui: &mut egui::Ui) {
        let language = self.localizer.language();
        let locale = if language == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let list = match self.file_part_list() {
            Ok(list) => list.clone(),
            Err(_) => {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(self.localizer.text("cortecloud-nothing"))
                            .size(12.5)
                            .color(tw::MUTED),
                    );
                });
                return;
            }
        };
        let project = self.editor.project();
        egui::ScrollArea::both()
            .id_salt("cortecloud-preview")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(18))
                    .show(ui, |ui| {
                        ui.label(tw::semibold(
                            ui,
                            self.localizer.text("cortecloud-preview-title"),
                            15.0,
                        ));
                        ui.add_space(8.0);
                        egui::Grid::new("cortecloud-parts")
                            .striped(true)
                            .spacing(egui::vec2(18.0, 6.0))
                            .show(ui, |ui| {
                                for key in [
                                    "cortecloud-col-quantity",
                                    "cortecloud-col-part",
                                    "cortecloud-col-cabinet",
                                    "cortecloud-col-size",
                                    "cortecloud-col-material",
                                    "cortecloud-col-banding",
                                    "cortecloud-col-holes",
                                ] {
                                    ui.label(
                                        egui::RichText::new(self.localizer.text(key))
                                            .size(11.5)
                                            .color(tw::FAINT),
                                    );
                                }
                                ui.end_row();
                                for group in &list.groups {
                                    let part = formats::cortecloud::part(group);
                                    ui.label(tw::mono(part.quantity.to_string(), 12.0));
                                    ui.label(egui::RichText::new(&part.function).size(12.5));
                                    ui.label(
                                        egui::RichText::new(
                                            part.complement.as_deref().unwrap_or("—"),
                                        )
                                        .size(12.0)
                                        .color(tw::MUTED),
                                    );
                                    ui.label(tw::mono(
                                        format!(
                                            "{} × {} × {}",
                                            crate::banding_ui::mm_text(
                                                plan_my_cabinet::units::Length::from_micrometres(
                                                    (part.c * 1000.0).round() as i64
                                                ),
                                                locale
                                            ),
                                            crate::banding_ui::mm_text(
                                                plan_my_cabinet::units::Length::from_micrometres(
                                                    (part.l * 1000.0).round() as i64
                                                ),
                                                locale
                                            ),
                                            crate::banding_ui::mm_text(group.thickness, locale)
                                        ),
                                        12.0,
                                    ));
                                    ui.label(egui::RichText::new(&part.material).size(12.0));
                                    let sides: Vec<&str> = [
                                        ("C1", &part.c1),
                                        ("C2", &part.c2),
                                        ("L1", &part.l1),
                                        ("L2", &part.l2),
                                    ]
                                    .into_iter()
                                    .filter(|(_, b)| b.is_some())
                                    .map(|(code, _)| code)
                                    .collect();
                                    ui.label(
                                        egui::RichText::new(if sides.is_empty() {
                                            "—".to_owned()
                                        } else {
                                            sides.join(" ")
                                        })
                                        .size(12.0),
                                    );
                                    let holes = group.machining.face_drills.len();
                                    ui.label(tw::mono(
                                        if holes == 0 {
                                            "—".to_owned()
                                        } else {
                                            holes.to_string()
                                        },
                                        12.0,
                                    ));
                                    ui.end_row();
                                }
                            });
                        let _ = project;
                    });
            });
    }

    /// Open the save dialog for a part-list format.
    pub(crate) fn start_file_export(&mut self, format: ExportFormat) {
        let (tx, rx) = mpsc::channel();
        self.file_export.flow = Some(FileFlow::Choosing(format, rx));
        self.handoff.message = None;
        let picker = rfd::AsyncFileDialog::new()
            .add_filter(
                self.localizer.text(format.label_key()),
                &[format.extension()],
            )
            .set_file_name(format.file_name(&self.editor.project().name))
            .save_file();
        std::thread::spawn(move || {
            let path = pollster::block_on(picker).map(|handle| handle.path().to_path_buf());
            let _ = tx.send(path);
        });
    }

    /// Write the file once a destination is known. JSON is small, so it is
    /// written on this thread.
    pub(crate) fn write_file_export(
        &mut self,
        format: ExportFormat,
        path: PathBuf,
        overwrite: Overwrite,
    ) {
        let options = self.file_export_options();
        match formats::write(format, self.editor.project(), &options, &path, overwrite) {
            Ok(record) => {
                let name = path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                let _ = self.editor.record_file_export(record);
                let mut args = FluentArgs::new();
                args.set("file", name);
                let text = self.localizer.format("cortecloud-saved", Some(&args));
                self.handoff.message = Some(text.clone());
                self.toasts.info(text);
            }
            Err(OutputError::OverwriteRequired) => {
                self.file_export.flow = Some(FileFlow::Confirming(format, path));
            }
            Err(_) => {
                let text = self.localizer.text("cortecloud-failed");
                self.handoff.message = Some(text.clone());
                self.toasts.error(text);
            }
        }
    }

    pub(crate) fn poll_file_export(&mut self, ctx: &egui::Context) {
        let chosen = match &self.file_export.flow {
            Some(FileFlow::Choosing(format, rx)) => match rx.try_recv() {
                Ok(path) => Some((*format, path)),
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100));
                    None
                }
                Err(mpsc::TryRecvError::Disconnected) => Some((*format, None)),
            },
            _ => None,
        };
        if let Some((format, path)) = chosen {
            self.file_export.flow = None;
            match path {
                Some(path) => self.write_file_export(format, path, Overwrite::Decline),
                None => self.handoff.message = Some(self.localizer.text("export-cancelled")),
            }
        }
        self.show_file_overwrite(ctx);
    }

    fn show_file_overwrite(&mut self, ctx: &egui::Context) {
        let Some(FileFlow::Confirming(format, path)) = &self.file_export.flow else {
            if let Some(mut chrome) = self.file_export.overwrite_chrome.take()
                && chrome.is_active()
            {
                chrome.close(ctx);
            }
            return;
        };
        let (format, path) = (*format, path.clone());
        let mut chrome = self.file_export.overwrite_chrome.take().unwrap_or_else(|| {
            ModalChrome::new(egui::Id::new("file-export-overwrite")).width(520.0)
        });
        let title = self.localizer.text("export-overwrite");
        let cancel = self.localizer.text("cancel");
        let replace = self.localizer.text("export-replace");
        let destination = path.display().to_string();
        let action = chrome
            .show(
                ctx,
                &title,
                ModalActions {
                    cancel: &cancel,
                    confirm: &replace,
                },
                |ui| {
                    ui.label(&destination);
                    ((), true)
                },
            )
            .action;
        match action {
            ModalAction::Confirm => {
                chrome.close(ctx);
                self.file_export.flow = None;
                self.write_file_export(format, path, Overwrite::Confirm);
            }
            ModalAction::Cancel => {
                chrome.close(ctx);
                self.file_export.flow = None;
                self.handoff.message = Some(self.localizer.text("export-cancelled"));
            }
            _ => self.file_export.overwrite_chrome = Some(chrome),
        }
    }
}

/// A name for the hinge or slides an omission is about, and where to fix it.
fn omission_target(project: &Project, installation: Uuid) -> (String, Option<HandoffFix>) {
    use crate::workspace_state::{Destination, InspectorTarget};
    if let Some(hinge) = project
        .hinge_installations
        .iter()
        .find(|h| h.id == installation)
    {
        let door = project
            .board(hinge.door_board_id)
            .map_or("—", |b| b.name.as_str());
        return (
            door.to_owned(),
            Some(HandoffFix::Navigate(Destination::Installation(
                installation,
            ))),
        );
    }
    if let Some(slide) = project
        .slide_installations
        .iter()
        .find(|s| s.id == installation)
    {
        let drawer = project
            .assemblies
            .iter()
            .find(|a| a.id == slide.drawer_root_id)
            .map(|a| a.name.clone())
            .or_else(|| project.board(slide.drawer_root_id).map(|b| b.name.clone()))
            .unwrap_or_default();
        return (
            drawer,
            Some(HandoffFix::Navigate(Destination::Fitting(
                InspectorTarget::Slide(installation),
            ))),
        );
    }
    (String::new(), None)
}

#[cfg(test)]
mod tests;
