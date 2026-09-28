//! Handoff logic: export review preparation, PDF writing and issue routing.
use crate::*;

pub(crate) fn export_completion_key(status: ExportStatus) -> &'static str {
    match status {
        ExportStatus::Current => "export-saved",
        ExportStatus::PacketStale | ExportStatus::WoodStale => "export-saved-stale",
        ExportStatus::Unknown | ExportStatus::NeverExported => "export-saved-unknown",
    }
}

pub(crate) enum ExportEvent {
    Selected(Option<PathBuf>),
    Finished(
        PathBuf,
        Arc<ReviewedPacket>,
        Result<plan_my_cabinet::export::ExportRecord, OutputError>,
    ),
}

pub(crate) enum ExportActivity {
    Choosing(Box<Project>, ExportSettings, ExportMode),
    Confirming(PathBuf, Arc<ReviewedPacket>),
    Writing,
}

#[derive(Clone, Copy)]
pub(crate) enum HandoffFix {
    Navigate(Destination),
    Stock(Uuid),
    Hardware(Uuid),
    Kerf,
    CutFee,
}

impl DesktopApp {
    pub(crate) fn start_pdf_write(
        &mut self,
        path: PathBuf,
        packet: Arc<ReviewedPacket>,
        overwrite: Overwrite,
    ) {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.handoff.cancel = Some(cancel.clone());
        self.handoff.events = Some(rx);
        self.handoff.activity = Some(ExportActivity::Writing);
        std::thread::spawn(move || {
            let result = write_reviewed_pdf(&packet, Some(&path), overwrite, || {
                cancel.load(Ordering::Relaxed)
            });
            let _ = tx.send(ExportEvent::Finished(path, packet, result));
        });
    }

    pub(crate) fn export_settings(&self) -> ExportSettings {
        ExportSettings {
            language: self.handoff.language,
            units: self.handoff.units,
        }
    }

    pub(crate) fn export_key(&self) -> ExportPreparationKey {
        let project = self.editor.project();
        (
            project.id,
            project.revision,
            self.handoff.mode,
            self.handoff.language,
            self.handoff.units,
            plan_my_cabinet::export::fingerprint(project).packet,
            self.handoff.sections,
            plan_my_cabinet::document_layout::LAYOUT_VERSION,
            plan_my_cabinet::document_layout::FONT_METRICS_VERSION,
        )
    }

    pub(crate) fn current_reviewed_packet(&self) -> Option<Arc<ReviewedPacket>> {
        let (key, result) = self.handoff.preparation.as_ref()?;
        let packet = result.as_ref().ok()?;
        (key == &self.export_key()
            && packet.matches_source(
                self.editor.project(),
                self.handoff.mode,
                self.export_settings(),
                self.handoff.sections,
            ))
        .then(|| Arc::clone(packet))
    }

    pub(crate) fn shop_ready_available(&self) -> bool {
        self.handoff
            .candidate
            .as_ref()
            .is_some_and(|(key, result)| {
                key == &self.export_key()
                    && result
                        .as_ref()
                        .is_ok_and(|packet| packet.wood_issues().is_empty())
            })
    }

    pub(crate) fn invalidate_export_review(&mut self) {
        self.handoff.preparation = None;
        self.handoff.candidate = None;
        if let Some((_, _, cancel)) = &self.handoff.preparation_pending {
            cancel.store(true, Ordering::Relaxed);
        }
        self.handoff.preparation_pending = None;
    }

    pub(crate) fn poll_pdf_export(&mut self, ctx: &egui::Context) {
        let event = self
            .handoff
            .events
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        if let Some(event) = event {
            self.handoff.events = None;
            match event {
                ExportEvent::Selected(path) => {
                    let Some(ExportActivity::Choosing(source, settings, mode)) =
                        self.handoff.activity.take()
                    else {
                        return;
                    };
                    let picker_key = self.handoff.picker_key.take();
                    let reviewed = self.current_reviewed_packet().filter(|packet| {
                        packet.key().project_id == source.id
                            && packet.key().revision == source.revision
                            && packet.key().mode == mode
                            && packet.key().settings == settings
                            && picker_key.as_ref() == Some(packet.key())
                    });
                    match path {
                        None => {
                            self.handoff.message = Some(self.localizer.text("export-cancelled"))
                        }
                        Some(_) if reviewed.is_none() => {
                            self.invalidate_export_review();
                            self.handoff.message = Some(self.localizer.text("export-review-stale"));
                        }
                        Some(path) if path.symlink_metadata().is_ok() => {
                            self.handoff.activity =
                                Some(ExportActivity::Confirming(path, reviewed.expect("checked")))
                        }
                        Some(path) => self.start_pdf_write(
                            path,
                            reviewed.expect("checked"),
                            Overwrite::Decline,
                        ),
                    }
                }
                ExportEvent::Finished(path, packet, result) => {
                    self.handoff.activity = None;
                    self.handoff.cancel = None;
                    let snapshot = packet.snapshot();
                    let mut args = FluentArgs::new();
                    args.set("path", path.display().to_string());
                    args.set("revision", snapshot.revision() as i64);
                    self.handoff.message = Some(match result {
                        Ok(receipt) => {
                            match self.editor.record_completed_export(
                                snapshot,
                                &path,
                                receipt.completed_unix_ms,
                                &receipt.file_sha256,
                            ) {
                                Ok(()) => self.localizer.format(
                                    export_completion_key(self.editor.last_export_status()),
                                    Some(&args),
                                ),
                                Err(plan_my_cabinet::export::ExportError::WrongProject) => {
                                    self.localizer.format("export-saved-stale", Some(&args))
                                }
                                Err(_) => self.localizer.text("export-verify-failed"),
                            }
                        }
                        Err(OutputError::OverwriteRequired) => {
                            self.handoff.activity =
                                Some(ExportActivity::Confirming(path.clone(), packet.clone()));
                            self.localizer.text("export-overwrite")
                        }
                        Err(OutputError::Cancelled) => self.localizer.text("export-cancelled"),
                        Err(OutputError::PreparationBlocked) => {
                            self.localizer.text("export-wood-blocked")
                        }
                        Err(OutputError::Verify(_)) => self.localizer.text("export-verify-failed"),
                        Err(OutputError::Write(
                            plan_my_cabinet::persistence::SaveError::CommittedDurabilityUncertain(
                                error,
                            ),
                        )) => {
                            args.set("reason", error.to_string());
                            self.localizer.format("export-durability", Some(&args))
                        }
                        Err(error) => {
                            args.set(
                                "reason",
                                match error {
                                    OutputError::Pdf(_) => {
                                        self.localizer.text("export-render-failed")
                                    }
                                    OutputError::Write(_) => {
                                        self.localizer.text("export-write-failed")
                                    }
                                    _ => self.localizer.text("export-cancelled"),
                                },
                            );
                            self.localizer.format("export-failed", Some(&args))
                        }
                    });
                }
            }
        }
        if self.handoff.events.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    pub(crate) fn tick_export_preparation(&mut self, ctx: &egui::Context) {
        let key = self.export_key();
        let settings = self.export_settings();
        let source = self.editor.project().clone();
        let valid = |cache: &ExportPreparationCache| {
            cache.0 == key
                && cache.1.as_ref().map_or(true, |packet| {
                    packet.matches_source(
                        &source,
                        self.handoff.mode,
                        settings,
                        self.handoff.sections,
                    )
                })
        };
        if self
            .handoff
            .preparation
            .as_ref()
            .is_some_and(|cache| !valid(cache))
            || self
                .handoff
                .candidate
                .as_ref()
                .is_some_and(|cache| !valid(cache))
        {
            self.invalidate_export_review();
        }
        if self
            .handoff
            .preparation_pending
            .as_ref()
            .is_some_and(|(pending, _, _)| *pending != key)
        {
            self.invalidate_export_review();
        }
        if self
            .handoff
            .candidate
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
            && self
                .handoff
                .preparation_pending
                .as_ref()
                .is_none_or(|(pending, _, _)| *pending != key)
        {
            let snapshot = source.clone();
            let mode = self.handoff.mode;
            let sections = self.handoff.sections;
            let (tx, rx) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            self.handoff.preparation_pending = Some((key, rx, cancel));
            std::thread::spawn(move || {
                let _ = tx.send(
                    ReviewedPacket::prepare_cancellable(
                        &snapshot,
                        mode,
                        settings,
                        sections,
                        || worker_cancel.load(Ordering::Relaxed),
                    )
                    .map(Arc::new),
                );
            });
        }
        if let Some((pending, rx, _)) = &self.handoff.preparation_pending
            && let Ok(result) = rx.try_recv()
        {
            self.handoff.candidate = Some((pending.clone(), result));
            self.handoff.preparation_pending = None;
        }
        if self.handoff.preparation_pending.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    pub(crate) fn show_export_overwrite(&mut self, ctx: &egui::Context) {
        // A write can be in flight after the overwrite decision. Never take
        // (and accidentally discard) its Writing state on subsequent frames.
        if !matches!(self.handoff.activity, Some(ExportActivity::Confirming(..))) {
            if self.handoff.overwrite_chrome.is_active() {
                self.handoff.overwrite_chrome.close(ctx);
            }
            return;
        }
        let Some(ExportActivity::Confirming(path, packet)) = self.handoff.activity.take() else {
            unreachable!("confirmed activity was checked above");
        };
        let title = self.localizer.text("export-overwrite");
        let cancel_label = self.localizer.text("cancel");
        let replace_label = self.localizer.text("export-replace");
        let destination = path.display().to_string();
        let action = self
            .handoff
            .overwrite_chrome
            .show(
                ctx,
                &title,
                ModalActions {
                    cancel: &cancel_label,
                    confirm: &replace_label,
                },
                |ui| {
                    ui.label(&destination);
                    ((), true)
                },
            )
            .action;
        let replace = action == ModalAction::Confirm;
        let cancel = action == ModalAction::Cancel;
        if replace
            && !packet.matches_source(
                self.editor.project(),
                self.handoff.mode,
                self.export_settings(),
                self.handoff.sections,
            )
        {
            self.invalidate_export_review();
            self.handoff.message = Some(self.localizer.text("export-review-stale"));
        } else if replace && actions::contextual(Request::new(A::ReplacePdf), Ok(()), || ()).is_ok()
        {
            self.start_pdf_write(path, packet, Overwrite::Confirm);
        } else if cancel
            && actions::contextual(Request::new(A::CancelExport), Ok(()), || ()).is_ok()
        {
            self.handoff.message = Some(self.localizer.text("export-cancelled"));
        } else {
            self.handoff.activity = Some(ExportActivity::Confirming(path, packet));
        }
        if !matches!(self.handoff.activity, Some(ExportActivity::Confirming(..))) {
            self.handoff.overwrite_chrome.close(ctx);
        }
    }

    /// Handoff left pane: packet type and PDF options scroll above a pinned
    /// export footer.
    pub(crate) fn show_export_preparation(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom(egui::Id::new((
            "handoff-export-footer",
            self.editor.project().id,
        )))
        .resizable(false)
        .show_separator_line(true)
        .frame(
            egui::Frame::new()
                .fill(theme_widgets::PANEL)
                .inner_margin(egui::Margin {
                    left: 16,
                    right: 16,
                    top: 14,
                    bottom: 14,
                }),
        )
        .show(ui, |ui| self.show_export_footer(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("handoff-controls", self.editor.project().id))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::Frame::new()
                            .inner_margin(egui::Margin {
                                left: 12,
                                right: 12,
                                top: 4,
                                bottom: 14,
                            })
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                self.show_export_options(ui);
                            });
                    });
            });
    }

    /// Short, ID-free text for one export issue plus its full description.
    pub(crate) fn handoff_issue_text(&self, issue: &ExportIssue) -> (String, String) {
        let project = self.editor.project();
        let l = &self.localizer;
        let alias = |id: Uuid| {
            project
                .stock_alias(id)
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    project
                        .stock
                        .iter()
                        .find(|piece| piece.id == id)
                        .map_or_else(|| l.text("receipt-kind-stock"), |piece| piece.name.clone())
                })
        };
        match issue {
            ExportIssue::Board {
                name,
                stock_id,
                reasons,
                ..
            } => {
                let reasons = reasons
                    .iter()
                    .map(|r| l.text(r.key()))
                    .collect::<Vec<_>>()
                    .join(", ");
                let short = format!("{name} · {reasons}");
                let full = match stock_id {
                    Some(stock) => format!("{name} · {} · {reasons}", alias(*stock)),
                    None => short.clone(),
                };
                (short, full)
            }
            ExportIssue::Sheet { id, name, reason } => {
                let key = match reason {
                    SheetIssue::BudgetExhausted => "sheet-feasibility-unknown",
                    _ => "sheet-cut-conflict",
                };
                let short = format!("{} · {}", alias(*id), l.text(key));
                (
                    short.clone(),
                    format!("{} ({name}) · {}", alias(*id), l.text(key)),
                )
            }
            ExportIssue::KerfUnconfirmed(_) => {
                let text = l.text("export-kerf-unconfirmed");
                (text.clone(), text)
            }
            ExportIssue::UnknownPrice { stock_id } => {
                let short = match stock_id {
                    Some(id) => {
                        let mut args = FluentArgs::new();
                        args.set("item", alias(*id));
                        l.format("handoff-price-unknown", Some(&args))
                    }
                    None => l.text("handoff-cut-fee-unknown"),
                };
                (short, l.text("export-price-unknown"))
            }
            ExportIssue::Hardware { name, .. } => (
                format!("{name} · {}", l.text("handoff-hardware-issue")),
                format!("{name}: {}", l.text("export-hardware-unverified")),
            ),
            ExportIssue::Installation { name, .. } => (
                format!("{name} · {}", l.text("pdf-installation-withheld")),
                format!("{name}: {}", l.text("pdf-installation-withheld")),
            ),
            ExportIssue::JointNeedsReview { .. } => {
                let text = l.text("pdf-joint-review");
                (text.clone(), text)
            }
            ExportIssue::InvalidWood(_) => {
                let text = l.text("pdf-invalid-wood");
                (text.clone(), text)
            }
        }
    }

    pub(crate) fn handoff_issue_route(issue: &ExportIssue) -> Option<HandoffFix> {
        match issue {
            ExportIssue::Board { id, .. } => {
                Some(HandoffFix::Navigate(Destination::BoardAllocation(*id)))
            }
            ExportIssue::Sheet { id, .. } => Some(HandoffFix::Navigate(Destination::Sheet(*id))),
            ExportIssue::KerfUnconfirmed(_) => Some(HandoffFix::Kerf),
            ExportIssue::UnknownPrice { stock_id: Some(id) } => Some(HandoffFix::Stock(*id)),
            ExportIssue::UnknownPrice { stock_id: None } => Some(HandoffFix::CutFee),
            ExportIssue::Hardware { id, .. } => Some(HandoffFix::Hardware(*id)),
            ExportIssue::Installation { id, .. } => {
                Some(HandoffFix::Navigate(Destination::Installation(*id)))
            }
            ExportIssue::JointNeedsReview {
                installation_id, ..
            } => Some(HandoffFix::Navigate(Destination::Installation(
                *installation_id,
            ))),
            ExportIssue::InvalidWood(_) => None,
        }
    }

    pub(crate) fn apply_handoff_fix(&mut self, route: HandoffFix) {
        match route {
            HandoffFix::Navigate(target) => {
                self.navigate_session(target);
            }
            HandoffFix::Stock(id) => {
                if self
                    .editor
                    .project()
                    .stock
                    .iter()
                    .any(|piece| piece.id == id)
                    && matches!(
                        self.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
                        Outcome::Navigated
                    )
                {
                    self.session.stock_piece = Some(id);
                    self.session.inspector = Some(InspectorTarget::Sheet(id));
                }
            }
            HandoffFix::Hardware(id) => {
                if matches!(
                    self.request_navigation(NavigationRoute::Workspace(Workspace::Hardware)),
                    Outcome::Navigated
                ) && self
                    .editor
                    .project()
                    .hardware
                    .iter()
                    .any(|hardware| hardware.id == id)
                {
                    self.selection.choose(Some(id), false);
                }
            }
            HandoffFix::Kerf => {
                self.invoke_or_report(Request::new(A::EditKerf));
            }
            HandoffFix::CutFee => {
                self.invoke_or_report(Request::new(A::EditCutFee));
            }
        }
    }

    pub(crate) fn show_export_options(&mut self, ui: &mut egui::Ui) {
        let heading = |ui: &mut egui::Ui, text: String| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.vertical(|ui| theme_widgets::inspector_heading(ui, &text, |_| ()));
            });
        };
        heading(ui, self.localizer.text("handoff-packet-type"));
        let key = self.export_key();
        let prepared = self
            .handoff
            .candidate
            .as_ref()
            .filter(|(cached, _)| *cached == key)
            .map(|(_, result)| result);
        let (wood, notices, prepared_ok, failed): (Vec<ExportIssue>, Vec<ExportIssue>, bool, bool) =
            match prepared {
                Some(Ok(packet)) => (
                    packet.wood_issues().to_vec(),
                    packet.notices().to_vec(),
                    true,
                    false,
                ),
                Some(Err(ReviewPreparationError::Blocked(blocked))) => {
                    (blocked.issues.clone(), Vec::new(), false, false)
                }
                Some(Err(_)) => (Vec::new(), Vec::new(), false, true),
                None => (Vec::new(), Vec::new(), false, false),
            };
        let checked = prepared.is_some();
        let can_shop = self.shop_ready_available();
        let modal = self.modal_open();
        let mut mode_choice = self.handoff.mode;
        let mut fix = None;

        ui.spacing_mut().item_spacing.y = 6.0;
        let draft = handoff_ui::radio_card(
            ui,
            egui::Id::new("handoff-packet-draft"),
            self.handoff.mode == ExportMode::Draft,
            None,
            &self.localizer.text("export-draft"),
            |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(self.localizer.text("handoff-draft-description"))
                            .size(12.0)
                            .color(theme_widgets::MUTED),
                    )
                    .wrap(),
                );
            },
        );
        if draft.is_some_and(|r| r.clicked()) {
            mode_choice = ExportMode::Draft;
        }

        let project = self.editor.project();
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let kerf_ok = project.confirmed_shop_kerf == Some(project.cutting_kerf);
        let mut kerf_args = FluentArgs::new();
        kerf_args.set(
            "kerf",
            assembly_ui::short_length(project.cutting_kerf, locale),
        );
        let kerf_text = self.localizer.format(
            if kerf_ok {
                "handoff-kerf-ok"
            } else {
                "handoff-kerf-unconfirmed"
            },
            Some(&kerf_args),
        );
        let kerf_detail = if kerf_ok {
            kerf_confirmation_label(project, &self.localizer)
        } else {
            Some(self.localizer.text("export-kerf-unconfirmed"))
        };
        let sheets = project
            .allocations
            .iter()
            .map(|allocation| allocation.stock_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let issues: Vec<(ExportIssue, bool)> = wood
            .iter()
            .filter(|issue| !matches!(issue, ExportIssue::KerfUnconfirmed(_)))
            .map(|issue| (issue.clone(), true))
            .chain(notices.iter().map(|issue| (issue.clone(), false)))
            .collect();
        let wood_clear = prepared_ok
            && !wood
                .iter()
                .any(|issue| !matches!(issue, ExportIssue::KerfUnconfirmed(_)));
        let expanded_id = egui::Id::new("handoff-issues-expanded");
        let mut expanded: bool = ui.ctx().data(|d| d.get_temp(expanded_id)).unwrap_or(false);
        let shop_unavailable = self.localizer.text("handoff-shop-unavailable");
        let fix_short = self.localizer.text("handoff-fix-short");
        let fix_label = self.localizer.text("handoff-fix");
        let shop = handoff_ui::radio_card(
            ui,
            egui::Id::new("handoff-packet-shop"),
            self.handoff.mode == ExportMode::ShopReady,
            (!can_shop).then_some(shop_unavailable.as_str()),
            &self.localizer.text("export-shop-ready"),
            |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let kerf_fix =
                    (!kerf_ok).then_some((fix_short.as_str(), fix_label.as_str(), !modal));
                if handoff_ui::check_line(
                    ui,
                    if kerf_ok {
                        handoff_ui::Check::Ok
                    } else {
                        handoff_ui::Check::Warn
                    },
                    &kerf_text,
                    kerf_detail.as_deref(),
                    kerf_fix,
                ) {
                    fix = Some(HandoffFix::Kerf);
                }
                if !checked {
                    handoff_ui::check_line(
                        ui,
                        handoff_ui::Check::Pending,
                        &self.localizer.text("handoff-checking"),
                        None,
                        None,
                    );
                    return;
                }
                if failed {
                    handoff_ui::check_line(
                        ui,
                        handoff_ui::Check::Warn,
                        &self.localizer.text("export-wood-blocked"),
                        None,
                        None,
                    );
                }
                if wood_clear {
                    handoff_ui::check_line(
                        ui,
                        handoff_ui::Check::Ok,
                        &self.localizer.count("handoff-cuts-ok", sheets as u64),
                        Some(&self.localizer.text("export-wood-verified")),
                        None,
                    );
                }
                let limit = if expanded { issues.len() } else { 3 };
                for (issue, blocking) in issues.iter().take(limit) {
                    let (short, full) = self.handoff_issue_text(issue);
                    let route = Self::handoff_issue_route(issue);
                    let link = route
                        .as_ref()
                        .map(|_| (fix_short.as_str(), fix_label.as_str(), !modal));
                    if handoff_ui::check_line(
                        ui,
                        if *blocking {
                            handoff_ui::Check::Warn
                        } else {
                            handoff_ui::Check::Note
                        },
                        &short,
                        Some(&full),
                        link,
                    ) {
                        fix = route;
                    }
                }
                if issues.len() > 3 {
                    ui.horizontal(|ui| {
                        ui.add_space(18.0);
                        let text = if expanded {
                            self.localizer.text("handoff-fewer-issues")
                        } else {
                            self.localizer
                                .count("handoff-more-issues", (issues.len() - 3) as u64)
                        };
                        let rest = issues
                            .iter()
                            .skip(3)
                            .map(|(issue, _)| self.handoff_issue_text(issue).0)
                            .collect::<Vec<_>>()
                            .join("\n");
                        let response =
                            handoff_ui::link(ui, &text, &text, theme_widgets::MUTED, true);
                        let response = if expanded {
                            response
                        } else {
                            response.on_hover_text(rest)
                        };
                        if response.clicked() {
                            expanded = !expanded;
                        }
                    });
                }
            },
        );
        ui.ctx().data_mut(|d| d.insert_temp(expanded_id, expanded));
        if shop.is_some_and(|r| r.clicked()) {
            mode_choice = ExportMode::ShopReady;
        }
        if let Some(route) = fix {
            self.apply_handoff_fix(route);
        }
        if mode_choice != self.handoff.mode {
            let _ = self
                .invoke(Request::new(A::SetExportMode).argument(Argument::ExportMode(mode_choice)));
        }

        ui.add_space(8.0);
        heading(ui, self.localizer.text("handoff-pdf-output"));
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(4, 0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                let label_width = 78.0;
                let mut language_choice = self.handoff.language;
                let language_text = |language: Language| {
                    self.localizer.text(match language {
                        Language::En => "handoff-lang-en",
                        Language::PtBr => "handoff-lang-pt",
                    })
                };
                handoff_ui::field_row(
                    ui,
                    &self.localizer.text("handoff-language"),
                    label_width,
                    false,
                    |ui| {
                        egui::ComboBox::from_id_salt("handoff-export-language")
                            .width(ui.available_width())
                            .height(200.0)
                            .icon(|ui, rect, _, _| {
                                icons::icon(icons::Icon::ChevDown, theme_widgets::MUTED, 11.0)
                                    .paint_at(
                                        ui,
                                        egui::Rect::from_center_size(
                                            rect.center(),
                                            egui::vec2(11.0, 11.0),
                                        ),
                                    );
                            })
                            .selected_text(
                                egui::RichText::new(language_text(self.handoff.language))
                                    .size(13.0),
                            )
                            .show_ui(ui, |ui| {
                                for language in [Language::En, Language::PtBr] {
                                    ui.selectable_value(
                                        &mut language_choice,
                                        language,
                                        language_text(language),
                                    );
                                }
                            })
                            .response
                            .on_hover_text(self.localizer.text("export-language"));
                    },
                );
                if language_choice != self.handoff.language {
                    self.invoke_or_report(
                        Request::new(A::SetExportLanguage)
                            .argument(Argument::Language(language_choice)),
                    );
                }
                let mut units_choice = self.handoff.units;
                let unit_labels = [Unit::Mm, Unit::Cm, Unit::M, Unit::Inch, Unit::Foot]
                    .map(|unit| (unit, self.localizer.text(unit_key(unit))));
                handoff_ui::field_row(
                    ui,
                    &self.localizer.text("handoff-units"),
                    label_width,
                    false,
                    |ui| {
                        let options = unit_labels
                            .iter()
                            .map(|(unit, label)| (*unit, label.as_str()))
                            .collect::<Vec<_>>();
                        theme_widgets::segmented(ui, &mut units_choice, &options)
                            .on_hover_text(self.localizer.text("export-output-units"));
                    },
                );
                if units_choice != self.handoff.units {
                    self.invoke_or_report(
                        Request::new(A::SetExportUnits).argument(Argument::Unit(units_choice)),
                    );
                }
                let mut sections = self.handoff.sections;
                handoff_ui::field_row(
                    ui,
                    &self.localizer.text("handoff-include"),
                    label_width,
                    true,
                    |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        for (value, key) in [
                            (&mut sections.parts_and_costs, "export-section-parts"),
                            (&mut sections.sheets_and_cut_steps, "export-section-sheets"),
                            (&mut sections.hinge_references, "export-section-hinges"),
                        ] {
                            handoff_ui::checkbox(ui, value, &self.localizer.text(key));
                        }
                    },
                );
                if sections != self.handoff.sections {
                    self.handoff.sections = sections;
                    self.invalidate_export_review();
                }
                ui.add_space(6.0);
                let mut args = FluentArgs::new();
                args.set("currency", self.editor.project().currency.code());
                egui::Frame::new()
                    .fill(theme_widgets::APP)
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(
                                    self.localizer.format("handoff-output-note", Some(&args)),
                                )
                                .size(11.5)
                                .color(theme_widgets::MUTED),
                            )
                            .wrap(),
                        );
                    });
            });
    }

    /// Pinned footer: explicit review step, the export action and its status.
    pub(crate) fn show_export_footer(&mut self, ui: &mut egui::Ui) {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 8.0;
        let width = ui.available_width();
        if let Some(message) = &self.handoff.message {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(message)
                        .size(11.5)
                        .color(theme_widgets::MUTED),
                )
                .wrap(),
            );
        }
        if let Some(activity) = &self.handoff.activity {
            let writing = matches!(activity, ExportActivity::Writing);
            let text = self.localizer.text(match activity {
                ExportActivity::Choosing(..) => "export-choosing",
                _ => "export-working",
            });
            let mut cancel = false;
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(12.0).color(theme_widgets::FAINT));
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text)
                            .size(12.0)
                            .color(theme_widgets::MUTED),
                    )
                    .wrap(),
                );
                if writing {
                    cancel = theme_widgets::text_button(
                        ui,
                        &self.localizer.text("cancel"),
                        theme_widgets::DANGER,
                        true,
                    )
                    .clicked();
                }
            });
            if cancel
                && let Some(flag) = &self.handoff.cancel
                && self
                    .invoke_contextual(Request::new(A::CancelExport))
                    .is_ok()
            {
                flag.store(true, Ordering::Relaxed);
            }
        }
        let ready = self
            .handoff
            .candidate
            .as_ref()
            .is_some_and(|(key, result)| key == &self.export_key() && result.is_ok());
        let reviewed = self.current_reviewed_packet().is_some();
        if ready && !reviewed {
            let response = ui
                .add_sized(
                    [width, 30.0],
                    egui::Button::image_and_text(
                        icons::icon(icons::Icon::Check, theme_widgets::OK, 14.0),
                        theme_widgets::medium(
                            ui,
                            self.localizer.text("handoff-mark-reviewed"),
                            13.0,
                        )
                        .color(theme_widgets::TEXT),
                    )
                    .fill(theme_widgets::VIEWPORT)
                    .stroke(egui::Stroke::new(1.0, theme_widgets::BORDER_SOFT))
                    .corner_radius(7),
                )
                .on_hover_text(self.localizer.text("export-review-stale"));
            if response.clicked() {
                self.handoff.preparation = self.handoff.candidate.as_ref().map(|(key, result)| {
                    (
                        key.clone(),
                        result
                            .as_ref()
                            .map(Arc::clone)
                            .map_err(|_| unreachable!("ready packet")),
                    )
                });
            }
        }
        let label = self.localizer.text(match self.handoff.mode {
            ExportMode::Draft => "handoff-export-draft",
            ExportMode::ShopReady => "handoff-export-shop",
        });
        let enabled = reviewed && self.handoff.activity.is_none() && !self.modal_open();
        let response = ui.add_enabled(
            enabled,
            egui::Button::image_and_text(
                icons::icon(icons::Icon::Export, theme_widgets::PANEL, 16.0),
                theme_widgets::semibold(ui, &label, 13.0).color(theme_widgets::PANEL),
            )
            .fill(theme_widgets::TEXT)
            .stroke(egui::Stroke::NONE)
            .corner_radius(8)
            .min_size(egui::vec2(width, 38.0)),
        );
        let response = if reviewed {
            response.on_hover_text(self.localizer.text("export-review-current"))
        } else {
            response.on_disabled_hover_text(self.localizer.text("handoff-review-needed"))
        };
        if response.clicked() {
            self.invoke_or_report(Request::new(A::ExportPdf));
        }
        ui.vertical_centered(|ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(self.localizer.text("handoff-export-hint"))
                        .size(11.0)
                        .color(theme_widgets::FAINT),
                )
                .wrap(),
            );
        });
    }
}
