//! The Hardware panel: one "Add hardware" menu, then a section per kind —
//! doors & hinges, drawers & slides, feet & legs, other hardware, and the
//! catalog models pinned to the project. Every row opens its inspector.
use crate::actions::{ActionId as A, Request};
use crate::icons::Icon;
use crate::theme_widgets as tw;
use crate::*;
use plan_my_cabinet::hardware_catalog::{self, CatalogKind};

impl DesktopApp {
    pub(crate) fn show_hardware_panel(&mut self, ui: &mut egui::Ui) {
        self.show_add_hardware_button(ui);
        self.show_hinge_list(ui);
        tw::divider(ui);
        self.show_slide_list(ui);
        self.show_hardware_list(ui, true);
        self.show_hardware_list(ui, false);
        self.show_catalog_models(ui);
    }

    fn show_add_hardware_button(&mut self, ui: &mut egui::Ui) {
        let modal = self.modal_open();
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                let response = ui.add_enabled(
                    !modal,
                    egui::Button::image_and_text(
                        crate::icons::icon(Icon::Plus, tw::TEXT, 14.0),
                        tw::medium(ui, self.localizer.text("hardware-add-button"), 13.0)
                            .color(tw::TEXT),
                    )
                    .fill(tw::VIEWPORT)
                    .stroke(egui::Stroke::new(1.0, tw::BORDER_SOFT))
                    .corner_radius(7)
                    .min_size(egui::vec2(ui.available_width(), 32.0)),
                );
                egui::Popup::menu(&response)
                    .align(egui::RectAlign::BOTTOM_START)
                    .show(|ui| {
                        ui.set_min_width(240.0);
                        self.hardware_add_items(
                            ui,
                            &[
                                A::NewDoor,
                                A::NewHinge,
                                A::NewSlides,
                                A::NewFoot,
                                A::NewHardware,
                            ],
                        );
                        ui.separator();
                        self.hardware_add_items(ui, &[A::AddCatalog]);
                    });
            });
    }

    fn show_catalog_models(&mut self, ui: &mut egui::Ui) {
        let project = self.editor.project();
        let rows: Vec<(Uuid, String, String, CatalogKind, usize)> = project
            .catalog
            .iter()
            .map(|c| {
                (
                    c.id,
                    c.name.clone(),
                    c.product_id.clone(),
                    hardware_catalog::kind(c),
                    hardware_catalog::usage(project, c.id).count(),
                )
            })
            .collect();
        let modal = self.modal_open();
        let mut run = None;
        let mut inspect = None;
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
                    egui::Id::new("hardware-catalog-section"),
                    &self.localizer.text("hardware-section-catalog"),
                    rows.len(),
                    |ui| {
                        let request = Request::new(A::AddCatalog);
                        if tw::ghost_icon_sized(
                            ui,
                            Icon::Plus,
                            &self.localizer.text("hardware-browse-catalog-item"),
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
                    if rows.is_empty() {
                        slide_ui::empty_hint(ui, &self.localizer.text("hardware-empty-catalog"));
                    }
                    for (id, name, code, kind, used) in &rows {
                        let active = self.session.inspector == Some(InspectorTarget::Catalog(*id));
                        let (icon, kind_key) = match kind {
                            CatalogKind::Hinge => (Icon::Hinge, "catalog-kind-short-hinge"),
                            CatalogKind::Slide => (Icon::Layers, "catalog-kind-short-slide"),
                            CatalogKind::Foot => (Icon::Cube, "catalog-kind-short-foot"),
                            CatalogKind::Other => (Icon::Hinge, "catalog-kind-short-other"),
                        };
                        let usage = if *used == 0 {
                            self.localizer.text("catalog-unused-short")
                        } else {
                            format!("×{used}")
                        };
                        let (response, ()) = tw::list_row(
                            ui,
                            egui::Id::new(("hardware-catalog-row", *id)),
                            40.0,
                            if active {
                                tw::RowState::Active
                            } else {
                                tw::RowState::Normal
                            },
                            !modal,
                            name,
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 7.0;
                                ui.add_space(2.0);
                                ui.add(crate::icons::icon(
                                    icon,
                                    if active { tw::ACCENT } else { tw::MUTED },
                                    14.0,
                                ));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(tw::mono(&usage, 11.0).color(tw::FAINT));
                                        ui.with_layout(
                                            egui::Layout::top_down(egui::Align::Min),
                                            |ui| {
                                                ui.spacing_mut().item_spacing.y = 0.0;
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(name).size(12.5).color(
                                                            if active {
                                                                tw::ACCENT_INK
                                                            } else {
                                                                tw::TEXT_2
                                                            },
                                                        ),
                                                    )
                                                    .truncate()
                                                    .selectable(false),
                                                );
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(format!(
                                                            "{} · {code}",
                                                            self.localizer.text(kind_key)
                                                        ))
                                                        .size(10.5)
                                                        .color(tw::FAINT),
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
                        if response.clicked() || response.double_clicked() {
                            inspect = Some(*id);
                        }
                    }
                });
        }
        if let Some(id) = inspect {
            self.request_inspect(InspectorTarget::Catalog(id));
        }
        if let Some(request) = run {
            self.invoke_or_report(request);
        }
    }
}
