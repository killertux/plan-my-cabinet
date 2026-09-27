//! Workspace shell: header, status bar, rail wiring and the three-pane layout.
use crate::*;


impl DesktopApp {
    /// The status bar and rail share committed global diagnostics. Repair
    /// previews use their own key in the issue list, never masquerading as a
    /// saved plan. Neither bounded witness search runs on an unchanged frame.
    pub(crate) fn shell_facts(
        &mut self,
    ) -> (
        workspace_shell::IssueCounts,
        Option<plan_my_cabinet::money::Money>,
        bool,
    ) {
        let project = self.editor.project();
        // Project replacement clears the shared diagnostics cache in the
        // lifecycle controller, including a reopened document at the same
        // UUID/revision with different externally saved contents.
        if self.cut_plan.allocation_diagnostics.is_none() {
            self.shell_estimate = None;
            self.design.stock_snapshot = None;
        }
        let key = sheet_ui::diagnostics_key(project, None);
        if self
            .cut_plan
            .allocation_diagnostics
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            self.cut_plan.allocation_diagnostics = Some((key, diagnose(project)));
        }
        let counts = workspace_shell::IssueCounts::from_diagnostics(
            &self.cut_plan.allocation_diagnostics.as_ref().expect("diagnosed").1,
            |id| !self.selection.visible(project, id),
        );
        let estimate_key = (project.id, project.revision);
        if self
            .shell_estimate
            .as_ref()
            .is_none_or(|(cached, _)| *cached != estimate_key)
        {
            self.shell_estimate = Some((
                estimate_key,
                plan_my_cabinet::cost_estimate::estimate(project),
            ));
        }
        let total = workspace_shell::complete_spending(
            project,
            counts,
            self.shell_estimate
                .as_ref()
                .and_then(|(_, estimate)| estimate.as_ref().ok()),
        )
        .copied();
        let invalid = self
            .shell_estimate
            .as_ref()
            .is_some_and(|(_, result)| result.is_err());
        (counts, total, invalid)
    }

    pub(crate) fn show_shell_header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("workspace-header")
            .exact_size(workspace_shell::HEADER_HEIGHT)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme_widgets::PANEL)
                    .inner_margin(egui::Margin {
                        left: 16,
                        right: 12,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ui, |ui| {
                let width = ui.available_width();
                let compact = width < 1000.0;
                let chrome_enabled = !self.palette.open
                    && !self.other_modal_open()
                    && !self.project_files.blocking()
                    && self.navigation.pending().is_none();
                let panes = workspace_shell::PaneLayout::for_width(
                    self.session.active,
                    ui.ctx().content_rect().width() - workspace_shell::RAIL_WIDTH,
                );
                let full = ui.max_rect();
                // Centered command search, painted first so the left and right
                // clusters can never overlap it at the reference width.
                let search_width = if compact { 0.0 } else { (width - 640.0).clamp(220.0, 440.0) };
                if search_width > 0.0 {
                    let rect = egui::Rect::from_center_size(
                        full.center(),
                        egui::vec2(search_width, 30.0),
                    );
                    let response = ui
                        .interact(
                            rect,
                            egui::Id::new("header-command-search"),
                            if chrome_enabled {
                                egui::Sense::click()
                            } else {
                                egui::Sense::hover()
                            },
                        )
                        .on_hover_text(self.localizer.text("palette-title"));
                    let label = self.localizer.text("palette-title");
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, chrome_enabled, &label)
                    });
                    let painter = ui.painter();
                    painter.rect(
                        rect,
                        7.0,
                        if response.hovered() {
                            theme_widgets::CARD
                        } else {
                            theme_widgets::APP
                        },
                        egui::Stroke::new(1.0, theme_widgets::BORDER_SOFT),
                        egui::StrokeKind::Inside,
                    );
                    icons::icon(icons::Icon::Search, theme_widgets::FAINT, 14.0).paint_at(
                        ui,
                        egui::Rect::from_center_size(
                            rect.left_center() + egui::vec2(18.0, 0.0),
                            egui::Vec2::splat(14.0),
                        ),
                    );
                    painter.text(
                        rect.left_center() + egui::vec2(34.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        self.localizer.text("shell-search-placeholder"),
                        egui::FontId::proportional(13.0),
                        theme_widgets::FAINT,
                    );
                    let key = if cfg!(target_os = "macos") { "⌘K" } else { "Ctrl K" };
                    let key_rect = egui::Rect::from_center_size(
                        rect.right_center() - egui::vec2(24.0, 0.0),
                        egui::vec2(if cfg!(target_os = "macos") { 28.0 } else { 40.0 }, 18.0),
                    );
                    painter.rect_stroke(
                        key_rect,
                        4.0,
                        egui::Stroke::new(1.0, theme_widgets::BORDER_STRONG),
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        key_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        key,
                        egui::FontId::monospace(11.0),
                        theme_widgets::FAINT,
                    );
                    if response.clicked() {
                        self.palette.open(ui.ctx());
                    }
                }
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui
                        .add_enabled(
                            self.action_availability(Request::new(A::OpenWelcome)).is_ok(),
                            egui::Button::new(
                                egui::RichText::new(self.localizer.text("shell-projects"))
                                    .color(theme_widgets::MUTED),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        self.invoke_or_report(Request::new(A::OpenWelcome));
                    }
                    ui.label(egui::RichText::new("/").color(theme_widgets::DISABLED));
                    let name = self.editor.project().name.clone();
                    let shown: String = if name.chars().count() > 28 {
                        name.chars().take(26).chain("…".chars()).collect()
                    } else {
                        name.clone()
                    };
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
                    ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                    ui.menu_button(
                        theme_widgets::semibold(ui, shown, 13.0).color(theme_widgets::TEXT),
                        |ui| {
                            ui.set_min_width(220.0);
                            self.show_project_controls(ui);
                        },
                    )
                    .response
                    .on_hover_text(format!("{name} · {}", self.localizer.text("shell-project-menu")));
                    if self.editor.is_dirty() {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 3.5, theme_widgets::ACCENT);
                        if !compact {
                            ui.label(
                                egui::RichText::new(self.localizer.text("shell-unsaved"))
                                    .size(11.5)
                                    .color(theme_widgets::MUTED),
                            );
                        }
                    }
                    for drawer in [
                        workspace_shell::Drawer::Controls,
                        workspace_shell::Drawer::Inspector,
                    ] {
                        if panes.collapsed(drawer) {
                            let (icon, label) = if drawer == workspace_shell::Drawer::Inspector {
                                (
                                    icons::Icon::Sliders,
                                    workspace_shell::inspector_label(self.localizer.language())
                                        .to_owned(),
                                )
                            } else {
                                (
                                    icons::Icon::List,
                                    self.localizer.text(
                                        workspace_shell::ENTRIES
                                            [(self.session.active.number() - 1) as usize]
                                            .1,
                                    ),
                                )
                            };
                            if theme_widgets::ghost_icon_sized(
                                ui,
                                icon,
                                &label,
                                theme_widgets::SECONDARY,
                                16.0,
                                30.0,
                                chrome_enabled,
                                self.open_drawer == Some(drawer),
                            )
                            .clicked()
                            {
                                self.open_drawer = if self.open_drawer == Some(drawer) {
                                    None
                                } else {
                                    Some(drawer)
                                };
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let export = Request::new(A::OpenHandoff);
                        if theme_widgets::icon_text_button(
                            ui,
                            icons::Icon::Export,
                            &self.localizer.text("shell-export"),
                            true,
                            self.action_availability(export).is_ok(),
                        )
                        .clicked()
                        {
                            self.invoke_or_report(export);
                        }
                        let save = Request::new(A::SaveProject);
                        if theme_widgets::secondary_button_enabled(
                            ui,
                            &A::SaveProject.label(&self.localizer),
                            self.action_availability(save).is_ok(),
                        )
                        .on_hover_text(A::SaveProject.label(&self.localizer))
                        .clicked()
                        {
                            self.invoke_or_report(save);
                        }
                        ui.add_space(4.0);
                        for (action, icon) in [(A::Redo, icons::Icon::Redo), (A::Undo, icons::Icon::Undo)] {
                            let request = Request::new(action);
                            if theme_widgets::ghost_icon_sized(
                                ui,
                                icon,
                                &action.label(&self.localizer),
                                theme_widgets::SECONDARY,
                                17.0,
                                32.0,
                                self.action_availability(request).is_ok(),
                                false,
                            )
                            .clicked()
                            {
                                self.invoke_or_report(request);
                            }
                        }
                        let next = match self.session.active {
                            Workspace::Design => Some(Workspace::Stock),
                            Workspace::Stock => Some(Workspace::CutPlan),
                            Workspace::CutPlan => Some(Workspace::Hardware),
                            Workspace::Hardware => Some(Workspace::Handoff),
                            Workspace::Handoff => None,
                        };
                        if let Some(next) = next
                            && width >= 1180.0
                        {
                            ui.add_space(8.0);
                            let step = self.localizer.text(
                                workspace_shell::ENTRIES[(next.number() - 1) as usize].1,
                            );
                            let mut args = FluentArgs::new();
                            args.set("step", step);
                            if theme_widgets::text_button(
                                ui,
                                &format!("{} ›", self.localizer.format("shell-next-step", Some(&args))),
                                theme_widgets::ACCENT_DARK,
                                chrome_enabled,
                            )
                            .on_hover_text(self.localizer.text("shell-next-step-hint"))
                            .clicked()
                            {
                                self.request_navigation(NavigationRoute::Workspace(next));
                            }
                        }
                        if compact
                            && theme_widgets::ghost_icon_sized(
                                ui,
                                icons::Icon::Search,
                                &self.localizer.text("palette-title"),
                                theme_widgets::SECONDARY,
                                16.0,
                                32.0,
                                chrome_enabled,
                                false,
                            )
                            .clicked()
                        {
                            self.palette.open(ui.ctx());
                        }
                    });
                });
            });
    }

    pub(crate) fn show_shell_status(
        &mut self,
        ui: &mut egui::Ui,
        issues: workspace_shell::IssueCounts,
        total: Option<plan_my_cabinet::money::Money>,
        invalid_estimate: bool,
    ) {
        egui::Panel::bottom("workspace-status")
            .default_size(workspace_shell::STATUS_HEIGHT)
            .min_size(workspace_shell::STATUS_HEIGHT)
            .max_size(workspace_shell::STATUS_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(theme_widgets::PANEL)
                    .inner_margin(egui::Margin::symmetric(12, 3)),
            )
            .show(ui, |ui| {
                // Locale and font scale change the amount of room required;
                // a fixed window-width breakpoint cannot prevent overlapping
                // Portuguese status text at 130%.
                if ui.available_width()
                    < self.shell_status_required_width(ui, issues, total.as_ref(), invalid_estimate)
                {
                    ui.horizontal(|ui| {
                        ui.label(self.localizer.text(
                            workspace_shell::ENTRIES[(self.session.active.number() - 1) as usize].1,
                        ));
                        ui.menu_button(
                            workspace_shell::more_label(self.localizer.language()),
                            |ui| {
                                egui::ScrollArea::both()
                                    .max_width((ui.ctx().content_rect().width() - 80.0).max(180.0))
                                    .max_height(
                                        (ui.ctx().content_rect().height() - 100.0).max(100.0),
                                    )
                                    .show(ui, |ui| {
                                        self.show_shell_status_contents(
                                            ui,
                                            issues,
                                            total,
                                            invalid_estimate,
                                        );
                                    });
                            },
                        )
                        .response
                        .on_hover_text(workspace_shell::more_label(self.localizer.language()));
                    });
                } else {
                    self.show_shell_status_contents(ui, issues, total, invalid_estimate);
                }
            });
    }

    pub(crate) fn show_shell_status_contents(
        &mut self,
        ui: &mut egui::Ui,
        issues: workspace_shell::IssueCounts,
        total: Option<plan_my_cabinet::money::Money>,
        invalid_estimate: bool,
    ) {
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let small = |text: String| egui::RichText::new(text).size(11.5).color(theme_widgets::MUTED);
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            if let Some(error) = &self.preferences_error {
                ui.label(
                    egui::RichText::new(self.localizer.text("preferences-save-warning"))
                        .size(11.5)
                        .color(theme_widgets::WARN_INK),
                )
                .on_hover_text(error);
                ui.add_space(12.0);
            }
            let project = self.editor.project();
            let confirmed = project.confirmed_shop_kerf == Some(project.cutting_kerf);
            let kerf = assembly_ui::short_length(project.cutting_kerf, locale);
            ui.add(icons::icon(
                if confirmed {
                    icons::Icon::Check
                } else {
                    icons::Icon::Warning
                },
                if confirmed {
                    theme_widgets::OK
                } else {
                    theme_widgets::WARN
                },
                13.0,
            ));
            let kerf_label = ui.label(
                egui::RichText::new(if confirmed {
                    format!("{} {kerf} mm", self.localizer.text("shell-kerf"))
                } else {
                    format!(
                        "{} {kerf} mm · {}",
                        self.localizer.text("shell-kerf"),
                        self.localizer.text("shell-unconfirmed")
                    )
                })
                .size(11.5)
                .color(if confirmed {
                    theme_widgets::OK_INK
                } else {
                    theme_widgets::WARN_INK
                }),
            );
            if let Some(date) = kerf_confirmation_label(project, &self.localizer) {
                kerf_label.on_hover_text(date);
            }
            ui.add_space(12.0);
            match self.session.active {
                Workspace::Design | Workspace::Hardware => {
                    ui.label(small(format!(
                        "{} {} mm",
                        self.localizer.text("shell-grid"),
                        assembly_ui::short_length(project.grid_spacing, locale)
                    )));
                    ui.add_space(12.0);
                    ui.label(small(format!(
                        "{} {}",
                        project.boards.len(),
                        self.localizer.text("shell-parts")
                    )));
                }
                Workspace::Stock => {
                    ui.label(small(format!(
                        "{} {}",
                        project.stock.len(),
                        self.localizer.text("shell-stock-pieces")
                    )));
                }
                Workspace::CutPlan => {
                    ui.label(small(format!(
                        "{} {}",
                        project.allocations.len(),
                        self.localizer.text("shell-placements")
                    )));
                }
                Workspace::Handoff => {
                    ui.label(small(format!(
                        "{} {}",
                        project.export_records.len(),
                        self.localizer.text("shell-receipts")
                    )));
                }
            }
            if issues.total() > 0 {
                ui.add_space(12.0);
                ui.add(icons::icon(icons::Icon::Warning, theme_widgets::WARN, 13.0));
                let mut parts = Vec::new();
                if issues.unallocated > 0 {
                    parts.push(format!(
                        "{} {}",
                        issues.unallocated,
                        self.localizer.text("shell-unallocated")
                    ));
                }
                if issues.conflicted > 0 {
                    parts.push(format!(
                        "{} {}",
                        issues.conflicted,
                        self.localizer.text("global-conflicted")
                    ));
                }
                if issues.unknown_proof > 0 {
                    parts.push(format!(
                        "{} {}",
                        issues.unknown_proof,
                        self.localizer.text("shell-proof-unknown")
                    ));
                }
                let response = ui.add(
                    egui::Label::new(
                        egui::RichText::new(parts.join(" · "))
                            .size(11.5)
                            .color(theme_widgets::WARN_INK),
                    )
                    .sense(egui::Sense::click()),
                );
                let response = if issues.hidden > 0 {
                    response.on_hover_text(format!(
                        "{} {}",
                        issues.hidden,
                        self.localizer.text("global-hidden")
                    ))
                } else {
                    response
                };
                if response.clicked() && self.session.active != Workspace::CutPlan {
                    self.request_navigation(NavigationRoute::Workspace(Workspace::CutPlan));
                }
            }
            if self.cut_plan.optimizer.running() {
                ui.add_space(12.0);
                ui.label(small(self.localizer.text("shell-search-active")));
            }
            if self.handoff.activity.is_some() {
                ui.add_space(12.0);
                ui.label(small(self.localizer.text("shell-export-active")));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let money_locale = if self.localizer.language() == Language::En {
                    plan_my_cabinet::money::MoneyLocale::English
                } else {
                    plan_my_cabinet::money::MoneyLocale::PortugueseBrazil
                };
                let (amount, known) = if invalid_estimate {
                    (self.localizer.text("cost-invalid"), false)
                } else {
                    total.map_or_else(
                        || (self.localizer.text("cost-incomplete"), false),
                        |amount| (amount.display(money_locale), true),
                    )
                };
                ui.label(if known {
                    theme_widgets::mono(amount, 11.5).color(theme_widgets::TEXT)
                } else {
                    egui::RichText::new(amount)
                        .size(11.5)
                        .color(theme_widgets::WARN_INK)
                });
                ui.label(small(self.localizer.text("shell-est-spending")));
                let hint = match (self.session.active, self.design.move_tool.mode) {
                    (Workspace::Design, viewport::ToolMode::Move) => Some("viewport-move-hint"),
                    (Workspace::Design, viewport::ToolMode::Measure) => {
                        Some("viewport-measure-hint")
                    }
                    _ => None,
                };
                if self.preferences.navigation_hints
                    && let Some(key) = hint
                {
                    ui.add_space(14.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(self.localizer.text(key))
                                .size(11.5)
                                .color(theme_widgets::FAINT),
                        )
                        .truncate(),
                    );
                }
            });
        });
    }

    pub(crate) fn shell_status_required_width(
        &self,
        ui: &egui::Ui,
        issues: workspace_shell::IssueCounts,
        total: Option<&plan_my_cabinet::money::Money>,
        invalid_estimate: bool,
    ) -> f32 {
        let p = self.editor.project();
        let en = self.localizer.language() == Language::En;
        let (count, count_key, hint_key) = match self.session.active {
            Workspace::Design => (p.boards.len(), "board-list", "shell-hint-design"),
            Workspace::Stock => (p.stock.len(), "shell-stock-pieces", "shell-hint-stock"),
            Workspace::CutPlan => (
                p.allocations.len(),
                "shell-placements",
                "shell-hint-cut-plan",
            ),
            Workspace::Hardware => (
                p.hinge_installations.len(),
                "shell-installations",
                "shell-hint-hardware",
            ),
            Workspace::Handoff => (
                p.export_records.len(),
                "shell-receipts",
                "shell-hint-handoff",
            ),
        };
        let amount = if invalid_estimate {
            self.localizer.text("cost-invalid")
        } else {
            total.map_or_else(
                || self.localizer.text("cost-incomplete"),
                |m| {
                    m.display(if en {
                        plan_my_cabinet::money::MoneyLocale::English
                    } else {
                        plan_my_cabinet::money::MoneyLocale::PortugueseBrazil
                    })
                },
            )
        };
        let mut texts = vec![
            format!(
                "{} {} mm · {}",
                self.localizer.text("shell-kerf"),
                assembly_ui::short_length(p.cutting_kerf, if en { Locale::En } else { Locale::PtBr }),
                self.localizer
                    .text(if p.confirmed_shop_kerf == Some(p.cutting_kerf) {
                        ""
                    } else {
                        "shell-unconfirmed"
                    })
            ),
            format!("{count} {}", self.localizer.text(count_key)),
            format!("{} {amount}", self.localizer.text("shell-est-spending")),
        ];
        let _ = hint_key;
        if self.preferences_error.is_some() {
            texts.push(self.localizer.text("preferences-save-warning"));
        }
        if issues.total() > 0 {
            texts.push(format!(
                "{} {} · {} {} · {} {}",
                issues.unallocated,
                self.localizer.text("shell-unallocated"),
                issues.conflicted,
                self.localizer.text("global-conflicted"),
                issues.unknown_proof,
                self.localizer.text("shell-proof-unknown")
            ));
            if issues.hidden > 0 {
                texts.push(format!(
                    " · {} {}",
                    issues.hidden,
                    self.localizer.text("global-hidden")
                ));
            }
        }
        if self.cut_plan.optimizer.running() {
            texts.push(self.localizer.text("shell-search-active"));
        }
        if self.handoff.activity.is_some() {
            texts.push(self.localizer.text("shell-export-active"));
        }
        // Body is a conservative upper bound for the Small labels. Include
        // gaps/separators so borderline widths choose the accessible More menu.
        let font = egui::TextStyle::Body.resolve(ui.style());
        64.0 + texts
            .iter()
            .map(|text| {
                ui.painter()
                    .layout_no_wrap(text.clone(), font.clone(), theme_widgets::TEXT)
                    .size()
                    .x
                    + ui.spacing().item_spacing.x
            })
            .sum::<f32>()
    }

    pub(crate) fn show_workspace_inspector(&mut self, ui: &mut egui::Ui) {
        let target = self.session.inspector;
        match self.session.active {
            Workspace::Design => {
                if let Some(model) = self.design_model() {
                    self.show_design_inspector(ui, &model);
                } else {
                    ui.label(self.localizer.text("measurement-invalid"));
                }
            }
            Workspace::CutPlan => {
                // The optimizer stays pinned at the bottom: the sequence scrolls
                // on its own, and short content is padded.
                let height_id = egui::Id::new("cut-plan-optimizer-height");
                let needed: f32 = ui.data(|d| d.get_temp(height_id)).unwrap_or(170.0);
                sheet_ui::show_focused_inspector_with_reserve(
                    ui,
                    &self.editor,
                    &self.localizer,
                    &mut self.cut_plan.repair,
                    (!self.cut_plan.optimizer.has_result()).then_some(needed + 4.0),
                );
                let remaining = ui.clip_rect().bottom() - ui.cursor().top();
                if !self.cut_plan.optimizer.has_result() && remaining > needed + 1.0 {
                    ui.add_space(remaining - needed - 1.0);
                }
                let blocked = self.external_modal_open();
                let top = ui.cursor().top();
                self.cut_plan.optimizer
                    .show(ui, &mut self.editor, &self.localizer, blocked);
                let used = ui.min_rect().bottom() - top;
                ui.data_mut(|d| d.insert_temp(height_id, used));
                self.cut_plan.optimizer.show_inspector_comparison(
                    ui,
                    self.editor.project(),
                    &self.localizer,
                    self.external_modal_open(),
                );
            }
            Workspace::Hardware => {
                if let Some(InspectorTarget::Installation(id)) = target {
                    self.show_selected_installation_inspector(ui, id);
                } else {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(14, 16))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(self.localizer.text(
                                    if self.editor.project().hinge_installations.is_empty() {
                                        "hardware-no-installations"
                                    } else {
                                        "hardware-select-installation"
                                    },
                                ))
                                .size(12.5)
                                .color(theme_widgets::MUTED),
                            );
                        });
                }
            }
            Workspace::Stock => self.show_stock_inspector(ui),
            Workspace::Handoff => {
                receipt_ui::show_receipts(ui, self.editor.project(), &self.localizer);
            }
        }
    }

    pub(crate) fn show_scrolled_workspace_inspector(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        let index = (self.session.active.number() - 1) as usize;
        let width = width.min(ui.available_width());
        if self.session.active == Workspace::Handoff {
            // History scrolls; the save reminder stays pinned to the bottom.
            receipt_ui::show_panel(ui, self.editor.project(), &self.localizer, width, height);
            return;
        }
        let scroll = egui::ScrollArea::vertical()
            .id_salt(("workspace-inspector", self.session.active.number()))
            .max_height(height)
            .auto_shrink([false, false])
            .vertical_scroll_offset(self.inspector_scroll[index].y)
            .show(ui, |ui| {
                ui.set_width(width.max(1.0));
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                self.show_workspace_inspector(ui);
            });
        self.inspector_scroll[index] = scroll.state.offset;
    }

    /// Left pane: one scroll area per workspace; content lives with its workspace.
    pub(crate) fn show_controls_pane(&mut self, ui: &mut egui::Ui) {
        let active = self.session.active;
        if active == Workspace::Handoff {
            // Options scroll above a pinned export footer.
            self.show_export_preparation(ui);
            return;
        }
        if active == Workspace::Hardware {
            egui::Panel::bottom(egui::Id::new("hardware-controls-footer"))
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .fill(theme_widgets::PANEL)
                        .inner_margin(egui::Margin::symmetric(12, 10)),
                )
                .show(ui, |ui| self.show_hardware_footer(ui));
        }
        if active == Workspace::CutPlan {
            // "+ Sheet or offcut" stays pinned under the scrolling sheet list.
            let blocked = self.palette.open || self.other_modal_open();
            let footer = egui::Panel::bottom(egui::Id::new("cut-plan-controls-footer"))
                .resizable(false)
                .show_separator_line(true)
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 10)))
                .show(ui, |ui| {
                    sheet_ui::show_sheet_footer(
                        ui,
                        &self.localizer,
                        &self.cut_plan.repair,
                        blocked,
                    )
                })
                .inner;
            if let Some(request) = footer {
                self.invoke_or_report(request);
            }
        }
        let scroll = egui::ScrollArea::vertical()
            .id_salt((
                "workspace-controls-scroll",
                self.editor.project().id,
                active.number(),
            ))
            .auto_shrink([false, false])
            .vertical_scroll_offset(self.session.view(active).scroll)
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                match active {
                    Workspace::Design => self.show_design_controls(ui),
                    Workspace::Stock => {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(8, 0))
                            .show(ui, |ui| self.show_stock_materials(ui));
                    }
                    Workspace::CutPlan => {
                        let blocked = self.palette.open || self.other_modal_open();
                        if let Some(request) = sheet_ui::show_sheet_list(
                            ui,
                            &self.editor,
                            &self.selection,
                            &self.localizer,
                            &mut self.cut_plan.repair,
                            blocked,
                            sheet_ui::SheetFocus {
                                sheet: self.session.focused_sheet,
                                issue: self.session.allocation_issue,
                                scroll_to_target: self.session.pending_cut_focus,
                            },
                        ) {
                            self.invoke_or_report(request);
                        }
                    }
                    Workspace::Hardware => {
                        self.show_pinned_catalog(ui);
                        self.show_hinge_list(ui);
                        self.show_hardware_list(ui);
                    }
                    // Laid out by `show_export_preparation` above.
                    Workspace::Handoff => {}
                }
            });
        self.session.view_mut(active).scroll = scroll.state.offset.y;
    }

    pub(crate) fn show_design_controls(&mut self, ui: &mut egui::Ui) {
        self.selection.retain_objects(self.editor.project());
        self.show_hierarchy(ui);
        let mut notices = Vec::new();
        if let Some(notice) = self.design.first_fit_notice {
            notices.push(self.localizer.text(match notice {
                FirstFit::Allocated(_) => "first-fit-allocated",
                FirstFit::NoFit => "first-fit-no-fit",
                FirstFit::SearchExhausted => "first-fit-exhausted",
            }));
        }
        for conflict in &self.cut_plan.material_conflicts {
            let name = self
                .editor
                .project()
                .boards
                .iter()
                .find(|b| b.id == conflict.board_id)
                .map(|b| b.name.as_str())
                .unwrap_or("—");
            notices.push(format!(
                "{}: {}",
                name,
                conflict_labels(&self.localizer, conflict)
            ));
        }
        let template = self.template.message.clone();
        if notices.is_empty() && template.is_none() && !self.design.board_action_error {
            return;
        }
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                theme_widgets::warn_callout().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for notice in notices {
                        ui.label(
                            egui::RichText::new(notice)
                                .size(12.0)
                                .color(theme_widgets::WARN_INK),
                        );
                    }
                    if self.design.board_action_error {
                        ui.label(
                            egui::RichText::new(self.localizer.text("error-board-duplicate"))
                                .size(12.0)
                                .color(theme_widgets::DANGER),
                        );
                    }
                    if let Some(message) = template {
                        ui.label(
                            egui::RichText::new(message)
                                .size(12.0)
                                .color(theme_widgets::WARN_INK),
                        );
                        if theme_widgets::text_button(
                            ui,
                            &self.localizer.text("template-setup-open-stock"),
                            theme_widgets::ACCENT_DARK,
                            true,
                        )
                        .clicked()
                        {
                            self.request_navigation(NavigationRoute::Workspace(Workspace::Stock));
                        }
                    }
                });
            });
    }

    /// Central pane: 3D viewport, sheet canvas, stock table or PDF preview.
    pub(crate) fn show_canvas_pane(&mut self, ui: &mut egui::Ui, drawer_modal: bool, inspector_visible: bool) {
        match self.session.active {
            Workspace::Stock => {
                egui::ScrollArea::vertical()
                    .id_salt(("stock-content", self.editor.project().id))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(24, 20))
                            .show(ui, |ui| self.show_stock_list(ui));
                    });
            }
            Workspace::Handoff => {
                let packet = self.handoff.candidate.as_ref().and_then(|(key, result)| {
                    (key == &self.export_key())
                        .then(|| result.as_ref().ok())
                        .flatten()
                        .cloned()
                });
                let centered_note = |ui: &mut egui::Ui, text: String, spinner: bool| {
                    ui.centered_and_justified(|ui| {
                        ui.horizontal_centered(|ui| {
                            if spinner {
                                ui.add(egui::Spinner::new().size(14.0).color(theme_widgets::FAINT));
                            }
                            ui.label(
                                egui::RichText::new(text)
                                    .size(12.5)
                                    .color(theme_widgets::MUTED),
                            );
                        });
                    });
                };
                if let Some(packet) = packet {
                    let pages = self.localizer.count(
                        "handoff-preview-pages",
                        packet.document().pages.len() as u64,
                    );
                    let labels = DocumentPreviewLabels {
                        title: &self.localizer.text("handoff-preview"),
                        pages: &pages,
                        page: &self.localizer.text("export-preview-page"),
                        zoom_in: &self.localizer.text("handoff-zoom-in"),
                        zoom_out: &self.localizer.text("handoff-zoom-out"),
                        fit: &self.localizer.text("export-preview-fit"),
                    };
                    if show_document_preview(
                        ui,
                        packet.document(),
                        &mut self.handoff.preview,
                        &labels,
                    )
                    .is_err()
                    {
                        centered_note(ui, self.localizer.text("export-render-failed"), false);
                    }
                } else {
                    centered_note(ui, self.localizer.text("handoff-preparing"), true);
                }
            }
            Workspace::CutPlan => {
                let other_modal = drawer_modal
                    || self.blocking_surface_open()
                    || self.hardware.door_motion.is_some();
                if let Some(request) = sheet_ui::show_with_layout(
                    ui,
                    &mut self.editor,
                    &mut self.selection,
                    &self.localizer,
                    other_modal,
                    &mut self.cut_plan.repair,
                    sheet_ui::SheetFocus {
                        sheet: self.session.focused_sheet,
                        issue: self.session.allocation_issue,
                        scroll_to_target: self.session.pending_cut_focus,
                    },
                    inspector_visible,
                ) {
                    self.invoke_or_report(request);
                }
                self.session.pending_cut_focus = false;
            }
            Workspace::Design | Workspace::Hardware => self.show_scene_pane(ui, drawer_modal),
        }
    }

    pub(crate) fn show_scene_pane(&mut self, ui: &mut egui::Ui, drawer_modal: bool) {
        let modal = drawer_modal || self.blocking_surface_open() || self.cut_plan.repair.active();
        if self.hardware.door_motion.is_some_and(|(id, angle)| {
            self.editor
                .project()
                .door_joints
                .iter()
                .find(|j| j.id == id)
                .is_none_or(|j| {
                    plan_my_cabinet::door_joint::derived_poses(self.editor.project(), j, angle)
                        .is_err()
                })
        }) {
            self.hardware.door_motion = None;
        }
        let motion_poses = self.hardware.door_motion.and_then(|(id, angle)| {
            self.editor
                .project()
                .door_joints
                .iter()
                .find(|j| j.id == id)
                .and_then(|j| {
                    plan_my_cabinet::door_joint::derived_poses(self.editor.project(), j, angle).ok()
                })
                .map(|poses| poses.into_iter().collect::<std::collections::HashMap<_, _>>())
        });
        let surface = ui.allocate_ui_with_layout(
            ui.available_size(),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let overlay_blocked = self.session.active == Workspace::Hardware
                    && self.show_hardware_motion_overlay(
                        ui.ctx(),
                        ui.available_rect_before_wrap().intersect(ui.clip_rect()),
                    );
                viewport::show_move_with_hardware(
                    ui,
                    &mut self.camera,
                    self.editor.preview().unwrap_or(self.editor.project()),
                    &mut self.selection,
                    &mut self.design.move_tool,
                    modal || overlay_blocked,
                    self.localizer.language(),
                    self.preferences.inverse_scroll_zoom,
                    self.preferences.material_tint,
                    self.modals.placement().and_then(PlacementDialog::highlighted),
                    motion_poses.as_ref(),
                    self.design.measurement_scope,
                    self.design.measurement_frame,
                    match (self.session.active, self.session.inspector) {
                        (Workspace::Hardware, Some(InspectorTarget::Installation(id))) => Some(id),
                        _ => None,
                    },
                    self.session.active == Workspace::Hardware,
                )
            },
        );
        let action = surface.inner;
        if let Some(capture) = &mut self.capture {
            capture.set_snap_evidence(self.design.move_tool.capture_evidence());
        }
        if self.design.move_tool.take_grid_edit_request() {
            self.invoke_or_report(Request::new(A::EditGrid));
        }
        if let Some(proposal) = action.selection {
            self.request_scene_selection(proposal.picked, proposal.additive);
        }
        match action.drag {
            Some(viewport::DragAction::Preview(id, pose)) => {
                if let Ok(mut session) =
                    plan_my_cabinet::placement::PlacementSession::resume(&mut self.editor, id)
                {
                    if session.preview_free(pose).is_ok() {
                        ui.ctx().request_repaint();
                    }
                    session.pause();
                }
            }
            Some(viewport::DragAction::Accept(id, pose)) => {
                if let Some(pose) = pose {
                    if let Ok(mut session) =
                        plan_my_cabinet::placement::PlacementSession::resume(&mut self.editor, id)
                        && session.preview_free(pose).is_ok()
                    {
                        let _ = session.accept();
                    }
                } else {
                    self.editor.cancel_preview();
                }
            }
            Some(viewport::DragAction::Cancel(ids, active)) => {
                self.editor.cancel_preview();
                self.selection.ids = ids;
                self.selection.active = active;
            }
            None => {}
        }
        // A collapsed pane is a foreground drawer. Keep the app-owned draft
        // alive, but do not let the HUD cover its close/actions or a modal.
        if self.session.active == Workspace::Design && self.design_hud_available() {
            self.show_design_hud(ui.ctx(), surface.response.rect);
        }
    }

    pub(crate) fn show_workspace(&mut self, ui: &mut egui::Ui) {
        self.tick_project_files(ui.ctx());
        self.poll_pdf_export(ui.ctx());
        self.tick_export_preparation(ui.ctx());
        // Worker completion belongs to the app session, not the Cut plan pane.
        // Its candidate is still only applied after explicit review there.
        self.cut_plan.optimizer.poll(ui.ctx());
        self.sync_scene_inspector();
        if let Some(action) = actions::project_shortcut(ui.ctx(), self.modal_open()) {
            self.invoke_or_report(Request::new(action));
        }
        let (issues, total, invalid_estimate) = self.shell_facts();
        if command_palette::shortcut(
            ui.ctx(),
            !self.palette.open
                && !self.cut_plan.optimizer.comparison_open()
                && !self.other_modal_open()
                && !self.project_files.blocking()
                && self.navigation.pending().is_none(),
        ) {
            self.palette.open(ui.ctx());
        }
        if let Some(workspace) = workspace_shell::shortcut(
            ui.ctx(),
            self.palette.open
                || self.project_files.blocking()
                || self.navigation.pending().is_some()
                || (self.other_modal_open()
                    && self.modals.board_dimension().is_none()
                    && self.modals.placement().is_none()),
        ) {
            self.request_navigation(NavigationRoute::Workspace(workspace));
        }
        if let Some(workspace) = workspace_shell::rail(
            ui,
            self.session.active,
            &self.localizer,
            !self.palette.open
                && !self.cut_plan.optimizer.comparison_open()
                && !self.project_files.blocking()
                && (!self.other_modal_open()
                    || self.modals.board_dimension().is_some()
                    || self.modals.placement().is_some()),
            issues,
        ) {
            match workspace {
                workspace_shell::RailAction::Workspace(workspace) => {
                    self.request_navigation(NavigationRoute::Workspace(workspace));
                }
                workspace_shell::RailAction::Language(language) => {
                    self.invoke_or_report(
                        Request::new(A::SetUiLanguage).argument(Argument::Language(language)),
                    );
                }
                workspace_shell::RailAction::Settings => {
                    self.invoke_or_report(Request::new(A::OpenSettings));
                }
            }
        }
        self.show_shell_header(ui);
        self.show_shell_status(ui, issues, total, invalid_estimate);
        let camera_workspace = self.session.active;
        std::mem::swap(
            &mut self.camera,
            &mut self.session.view_mut(camera_workspace).camera,
        );
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let available = ui.available_size();
                let active = self.session.active;
                let layout = workspace_shell::PaneLayout::for_width(active, available.x);
                if self.open_drawer.is_some_and(|drawer| !layout.collapsed(drawer)) {
                    self.open_drawer = None;
                }
                let drawer_pos = ui.min_rect().min;
                let pane_frame = egui::Frame::new()
                    .fill(theme_widgets::PANEL)
                    .stroke(egui::Stroke::new(1.0, theme_widgets::BORDER));
                if layout.controls > 0.0 {
                    egui::Panel::left(egui::Id::new(("workspace-controls", active.number())))
                        .exact_size(layout.controls)
                        .resizable(false)
                        .frame(pane_frame)
                        .show(ui, |ui| self.show_controls_pane(ui));
                } else if self.open_drawer == Some(workspace_shell::Drawer::Controls) {
                    let mut open = true;
                    let width = workspace_shell::PaneLayout::preferred(active)
                        .controls
                        .min(available.x - 20.0);
                    egui::Window::new(self.localizer.text(
                        workspace_shell::ENTRIES[(active.number() - 1) as usize].1,
                    ))
                    .id(egui::Id::new("workspace-controls-drawer"))
                    .fixed_pos(drawer_pos)
                    .fixed_size(egui::vec2(width, (available.y - 50.0).max(1.0)))
                    .resizable(false)
                    .collapsible(false)
                    .open(&mut open)
                    .show(ui.ctx(), |ui| self.show_controls_pane(ui));
                    if !open {
                        self.open_drawer = None;
                    }
                }
                if active == Workspace::CutPlan
                    && layout.controls == 0.0
                    && self.open_drawer != Some(workspace_shell::Drawer::Controls)
                    && self.cut_plan.optimizer.comparison_open()
                {
                    let blocked = self.external_modal_open();
                    self.cut_plan.optimizer.show_comparison(
                        ui.ctx(),
                        &mut self.editor,
                        &self.localizer,
                        blocked,
                    );
                }
                if layout.inspector > 0.0 {
                    egui::Panel::right(egui::Id::new(("workspace-inspector", active.number())))
                        .exact_size(layout.inspector)
                        .resizable(false)
                        .frame(pane_frame)
                        .show(ui, |ui| {
                            let height = ui.available_height();
                            self.show_scrolled_workspace_inspector(ui, layout.inspector, height);
                        });
                } else if self.open_drawer == Some(workspace_shell::Drawer::Inspector) {
                    let mut open = true;
                    let width = workspace_shell::PaneLayout::preferred(active)
                        .inspector
                        .min(available.x - 20.0);
                    egui::Window::new(workspace_shell::inspector_label(self.localizer.language()))
                        .id(egui::Id::new("workspace-inspector-drawer"))
                        .fixed_pos(egui::pos2(
                            drawer_pos.x + available.x - width - 20.0,
                            drawer_pos.y,
                        ))
                        .resizable(false)
                        .collapsible(false)
                        .open(&mut open)
                        .show(ui.ctx(), |ui| {
                            ui.set_width(width);
                            self.show_scrolled_workspace_inspector(ui, width, available.y - 44.0);
                        });
                    if !open {
                        self.open_drawer = None;
                    }
                }
                let canvas_fill = match active {
                    Workspace::Design | Workspace::Hardware | Workspace::CutPlan => {
                        theme_widgets::VIEWPORT
                    }
                    Workspace::Stock => theme_widgets::APP,
                    Workspace::Handoff => egui::Color32::from_rgb(226, 221, 212),
                };
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(canvas_fill))
                    .show(ui, |ui| {
                        let drawer_modal = self
                            .open_drawer
                            .is_some_and(|drawer| layout.collapsed(drawer));
                        self.show_canvas_pane(ui, drawer_modal, layout.inspector > 0.0);
                    });
            });
        std::mem::swap(
            &mut self.camera,
            &mut self.session.view_mut(camera_workspace).camera,
        );
        self.sync_scene_inspector();
        self.show_dialog(ui.ctx());
        self.show_material_edit(ui.ctx());
        self.show_board_material(ui.ctx());
        self.show_board_dimension(ui.ctx());
        self.show_batch_dimension(ui.ctx());
        self.show_placement(ui.ctx());
        self.show_grid_dialog(ui.ctx());
        self.show_kerf_confirmation(ui.ctx());
        self.show_stock_dialog(ui.ctx());
        self.show_cut_fee_dialog(ui.ctx());
        self.show_currency_dialog(ui.ctx());
        self.show_assembly_dialog(ui.ctx());
        self.show_hardware_dialog(ui.ctx());
        self.show_hinge_dialog(ui.ctx());
        self.show_door_dialog(ui.ctx());
        self.show_removal_dialog(ui.ctx());
        self.show_project_dialog(ui.ctx());
        self.show_export_overwrite(ui.ctx());
        self.show_navigation_prompt(ui.ctx());
        self.show_palette(ui.ctx());
        self.show_settings(ui.ctx());
        if matches!(self.handoff.activity, Some(ExportActivity::Choosing(..)))
            && self.handoff.picker_key.is_none()
        {
            self.handoff.picker_key = self
                .current_reviewed_packet()
                .map(|packet| packet.key().clone());
        }
    }
}
