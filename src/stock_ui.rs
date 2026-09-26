//! Keyboard-operable stock form and priority controls; drafts never enter the project.
use super::*;
use plan_my_cabinet::cost_estimate::{Feasibility, estimate};
use plan_my_cabinet::domain::{Stock, StockGrain, StockSource};
use plan_my_cabinet::money::{Money, MoneyLocale};
use plan_my_cabinet::stock_commands::{MAX_STOCK_QUANTITY, StockError, StockField, StockInput};

pub(super) struct StockDialog {
    project_id: Uuid,
    revision: u64,
    edit_id: Option<Uuid>,
    focus: bool,
    name: String,
    material_id: Option<Uuid>,
    dimensions: [DimensionDraft; 3],
    trim: [DimensionDraft; 4],
    grain: StockGrain,
    source: StockSource,
    price: String,
    quantity: String,
    error: Option<StockError>,
}

fn locale(app: &DesktopApp) -> Locale {
    if app.localizer.language() == Language::En {
        Locale::En
    } else {
        Locale::PtBr
    }
}

impl StockDialog {
    pub(super) fn new(project: &Project) -> Self {
        Self {
            project_id: project.id,
            revision: project.revision,
            edit_id: None,
            focus: true,
            name: String::new(),
            material_id: project.materials.first().map(|m| m.id),
            dimensions: std::array::from_fn(|_| DimensionDraft::new()),
            trim: std::array::from_fn(|_| DimensionDraft {
                text: String::from("0"),
                consent: false,
            }),
            grain: StockGrain::Unknown,
            source: StockSource::Owned,
            price: String::new(),
            quantity: "1".into(),
            error: None,
        }
    }

    fn edit(project: &Project, piece: &Stock, locale: Locale) -> Self {
        Self {
            project_id: project.id,
            revision: project.revision,
            edit_id: Some(piece.id),
            focus: true,
            name: piece.name.clone(),
            material_id: Some(piece.material_id),
            dimensions: [piece.length, piece.width, piece.thickness].map(|value| DimensionDraft {
                text: format_length(value, Unit::Mm, locale, 3),
                consent: false,
            }),
            trim: piece.trim.map(|value| DimensionDraft {
                text: format_length(value, Unit::Mm, locale, 3),
                consent: false,
            }),
            grain: piece.grain,
            source: piece.source,
            price: piece.price.map_or(String::new(), |p| {
                format!("{}.{:02}", p.minor_units() / 100, p.minor_units() % 100)
            }),
            quantity: "1".into(),
            error: None,
        }
    }

    fn input(&self, unit: Unit, currency: Currency) -> Result<StockInput, StockError> {
        let fields = [StockField::Length, StockField::Width, StockField::Thickness];
        let values = std::array::from_fn::<_, 3, _>(|index| self.dimensions[index].value(unit));
        let dimensions = std::array::from_fn::<_, 3, _>(|index| {
            values[index].as_ref().copied().unwrap_or(Length::ZERO)
        });
        for (index, value) in values.iter().enumerate() {
            if value.is_err() {
                return Err(StockError::Invalid(fields[index]));
            }
        }
        let mut trim = [Length::ZERO; 4];
        for (index, field) in self.trim.iter().enumerate() {
            trim[index] = trim_value(field).map_err(|_| StockError::Invalid(StockField::Trim))?;
        }
        let price = if self.price.trim().is_empty() {
            None
        } else {
            Some(
                Money::parse(currency, &self.price)
                    .map_err(|_| StockError::Invalid(StockField::Price))?,
            )
        };
        Ok(StockInput {
            name: self.name.clone(),
            material_id: self
                .material_id
                .ok_or(StockError::Invalid(StockField::Material))?,
            length: dimensions[0],
            width: dimensions[1],
            thickness: dimensions[2],
            grain: self.grain,
            source: self.source,
            price,
            trim,
        })
    }
}

// Trims allow zero, unlike board dimensions, but still require consent for grid rounding.
fn trim_value(field: &DimensionDraft) -> Result<Length, InputError> {
    let parsed = parse_length(&field.text, Unit::Mm)?;
    let value = match parsed.conversion {
        Conversion::Exact(value) => value,
        Conversion::NeedsConfirmation(value) if field.consent => value,
        Conversion::NeedsConfirmation(_) => return Err(InputError::Unit(UnitError::InvalidNumber)),
    };
    if value.micrometres() < 0 {
        return Err(InputError::Unit(UnitError::InvalidNumber));
    }
    Ok(value)
}

fn trim_field(ui: &mut egui::Ui, localizer: &Localizer, label: &str, field: &mut DimensionDraft) {
    ui.horizontal(|ui| {
        ui.label(localizer.text(label));
        if ui.text_edit_singleline(&mut field.text).changed() {
            field.consent = false;
        }
    });
    match parse_length(&field.text, Unit::Mm) {
        Ok(parsed) if parsed.conversion.suggested().micrometres() >= 0 => {
            let value = parsed.conversion.suggested();
            let locale = if localizer.language() == Language::En {
                Locale::En
            } else {
                Locale::PtBr
            };
            if matches!(parsed.conversion, Conversion::NeedsConfirmation(_)) {
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("entered", field.text.as_str());
                args.set("rounded", format_length(value, Unit::Mm, locale, 3));
                ui.checkbox(
                    &mut field.consent,
                    localizer.format("rounding-confirmation", Some(&args)),
                );
            } else {
                ui.small(format_length(value, Unit::Mm, locale, 3));
            }
        }
        Ok(_) | Err(_) => {
            if !field.text.is_empty() {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    localizer.text("stock-invalid-trim"),
                );
            }
        }
    }
}

fn stock_grain_key(grain: StockGrain) -> &'static str {
    match grain {
        StockGrain::AlongX => "stock-grain-x",
        StockGrain::AlongY => "stock-grain-y",
        StockGrain::Nondirectional => "stock-grain-none",
        StockGrain::Unknown => "stock-grain-unknown",
    }
}

impl DesktopApp {
    pub(super) fn show_stock_list(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.localizer.text("stock-list"));
        let money_locale = if locale(self) == Locale::En {
            MoneyLocale::English
        } else {
            MoneyLocale::PortugueseBrazil
        };
        ui.horizontal(|ui| {
            ui.label(format!(
                "{}: {}",
                self.localizer.text("cut-fee"),
                self.editor.project().cut_fee.map_or_else(
                    || self.localizer.text("stock-price-unknown"),
                    |fee| fee.display(money_locale)
                )
            ));
            if ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(self.localizer.text("cut-fee-edit")),
                )
                .clicked()
            {
                self.cut_fee_dialog =
                    Some(self.editor.project().cut_fee.map_or(String::new(), |fee| {
                        format!("{}.{:02}", fee.minor_units() / 100, fee.minor_units() % 100)
                    }));
            }
        });
        ui.small(self.localizer.text("cut-fee-hint"));
        match estimate(self.editor.project()) {
            Ok(result) => {
                ui.label(format!(
                    "{}: {}",
                    self.localizer.text("cost-owned-consumed"),
                    result
                        .used_stock
                        .iter()
                        .filter(|s| s.source == StockSource::Owned)
                        .count()
                ));
                let amount = |value: Option<Money>| {
                    value.map_or_else(
                        || self.localizer.text("stock-price-unknown"),
                        |v| v.display(money_locale),
                    )
                };
                ui.label(format!(
                    "{}: {}",
                    self.localizer.text("cost-material"),
                    amount(result.material)
                ));
                ui.label(format!(
                    "{}: {}",
                    self.localizer.text("cost-cutting"),
                    amount(result.cutting)
                ));
                ui.label(format!(
                    "{}: {}",
                    self.localizer.text("cost-total"),
                    result.total.map_or_else(
                        || self.localizer.text("cost-incomplete"),
                        |v| v.display(money_locale)
                    )
                ));
                if result.feasibility != Feasibility::Verified {
                    ui.small(self.localizer.text("cost-feasibility"));
                }
            }
            Err(_) => {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    self.localizer.text("cost-invalid"),
                );
            }
        }
        ui.small(self.localizer.text("cost-exclusions"));
        if ui
            .add_enabled(
                !self.modal_open(),
                egui::Button::new(self.localizer.text("stock-new")),
            )
            .clicked()
        {
            self.stock_dialog = Some(StockDialog::new(self.editor.project()));
        }
        let ordered: Vec<_> = self
            .editor
            .project()
            .ordered_stock()
            .iter()
            .map(|s| (*s).clone())
            .collect();
        let mut action = None;
        for (index, piece) in ordered.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{} ({})", piece.name, &piece.id.to_string()[..8]))
                    .on_hover_text(piece.id.to_string());
                if ui
                    .add_enabled(
                        !self.modal_open(),
                        egui::Button::new(self.localizer.text("stock-edit")),
                    )
                    .clicked()
                {
                    action = Some((piece.id, index, 0));
                }
                if ui
                    .add_enabled(
                        !self.modal_open() && index > 0,
                        egui::Button::new(self.localizer.text("stock-up")),
                    )
                    .clicked()
                {
                    action = Some((piece.id, index - 1, 1));
                }
                if ui
                    .add_enabled(
                        !self.modal_open() && index + 1 < ordered.len(),
                        egui::Button::new(self.localizer.text("stock-down")),
                    )
                    .clicked()
                {
                    action = Some((piece.id, index + 1, 1));
                }
            });
            ui.small(format!(
                "{} × {} × {} · {} · {} · {}",
                format_length(piece.length, Unit::Mm, locale(self), 3),
                format_length(piece.width, Unit::Mm, locale(self), 3),
                format_length(piece.thickness, Unit::Mm, locale(self), 3),
                self.localizer.text(stock_grain_key(piece.grain)),
                self.localizer.text(if piece.source == StockSource::Owned {
                    "stock-owned"
                } else {
                    "stock-purchase"
                }),
                piece.price.map_or_else(
                    || self.localizer.text("stock-price-unknown"),
                    |p| p.display(if locale(self) == Locale::En {
                        MoneyLocale::English
                    } else {
                        MoneyLocale::PortugueseBrazil
                    })
                )
            ));
        }
        if let Some((id, target, kind)) = action {
            if kind == 0 {
                let piece = ordered.iter().find(|s| s.id == id).unwrap();
                self.stock_dialog = Some(StockDialog::edit(
                    self.editor.project(),
                    piece,
                    locale(self),
                ));
            } else {
                let _ = self.editor.reorder_stock(id, target);
            }
        }
    }

    pub(super) fn show_cut_fee_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.cut_fee_dialog.take() else {
            return;
        };
        let mut cancel = false;
        let mut save = false;
        let modal = egui::Modal::new(egui::Id::new("cut-fee-dialog")).show(ctx, |ui| {
            ui.heading(self.localizer.text("cut-fee-edit"));
            ui.label(format!(
                "{} ({})",
                self.localizer.text("cut-fee"),
                self.editor.project().currency.code()
            ));
            ui.text_edit_singleline(&mut text);
            ui.small(self.localizer.text("cut-fee-hint"));
            if !text.trim().is_empty()
                && Money::parse(self.editor.project().currency, &text).is_err()
            {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    self.localizer.text("error-invalid-amount"),
                );
            }
            ui.horizontal(|ui| {
                cancel = ui.button(self.localizer.text("cancel")).clicked();
                save = ui
                    .add_enabled(
                        text.trim().is_empty()
                            || Money::parse(self.editor.project().currency, &text).is_ok(),
                        egui::Button::new(self.localizer.text("confirm")),
                    )
                    .clicked();
            });
        });
        if cancel || modal.should_close() {
            return;
        }
        if save {
            let fee = if text.trim().is_empty() {
                Ok(None)
            } else {
                Money::parse(self.editor.project().currency, &text).map(Some)
            };
            if let Ok(fee) = fee
                && self.editor.set_cut_fee(fee).is_ok()
            {
                return;
            }
        }
        self.cut_fee_dialog = Some(text);
    }

    pub(super) fn show_stock_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.stock_dialog.take() else {
            return;
        };
        let project = self.editor.project();
        let current = draft.project_id == project.id && draft.revision == project.revision;
        let mut cancel = false;
        let mut confirm = false;
        let modal = egui::Modal::new(egui::Id::new("stock-dialog")).show(ctx, |ui| {
            ui.heading(self.localizer.text(if draft.edit_id.is_some() {
                "stock-edit"
            } else {
                "stock-new"
            }));
            ui.horizontal(|ui| {
                ui.label(self.localizer.text("stock-name"));
                let response = ui.text_edit_singleline(&mut draft.name);
                if draft.focus {
                    response.request_focus();
                }
            });
            egui::ComboBox::from_label(self.localizer.text("material"))
                .selected_text(
                    project
                        .materials
                        .iter()
                        .find(|m| Some(m.id) == draft.material_id)
                        .map_or("—", |m| m.name.as_str()),
                )
                .show_ui(ui, |ui| {
                    for material in &project.materials {
                        combo_option(
                            ui,
                            &mut draft.material_id,
                            Some(material.id),
                            &material.name,
                        );
                    }
                });
            for (index, key) in ["board-length", "board-width", "board-thickness"]
                .iter()
                .enumerate()
            {
                dimension_field(
                    ui,
                    &self.localizer,
                    key,
                    &mut draft.dimensions[index],
                    project.display_unit,
                );
            }
            egui::ComboBox::from_label(self.localizer.text("stock-grain"))
                .selected_text(self.localizer.text(stock_grain_key(draft.grain)))
                .show_ui(ui, |ui| {
                    for grain in [
                        StockGrain::AlongX,
                        StockGrain::AlongY,
                        StockGrain::Nondirectional,
                        StockGrain::Unknown,
                    ] {
                        combo_option(
                            ui,
                            &mut draft.grain,
                            grain,
                            self.localizer.text(stock_grain_key(grain)),
                        );
                    }
                });
            egui::ComboBox::from_label(self.localizer.text("stock-source"))
                .selected_text(self.localizer.text(if draft.source == StockSource::Owned {
                    "stock-owned"
                } else {
                    "stock-purchase"
                }))
                .show_ui(ui, |ui| {
                    for (source, key) in [
                        (StockSource::Owned, "stock-owned"),
                        (StockSource::ToPurchase, "stock-purchase"),
                    ] {
                        combo_option(ui, &mut draft.source, source, self.localizer.text(key));
                    }
                });
            for (index, key) in [
                "stock-trim-left",
                "stock-trim-right",
                "stock-trim-bottom",
                "stock-trim-top",
            ]
            .iter()
            .enumerate()
            {
                trim_field(ui, &self.localizer, key, &mut draft.trim[index]);
            }
            ui.label(self.localizer.text("stock-trim-hint"));
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} ({})",
                    self.localizer.text("stock-price"),
                    project.currency.code()
                ));
                ui.text_edit_singleline(&mut draft.price);
            });
            if draft.edit_id.is_none() {
                ui.horizontal(|ui| {
                    ui.label(self.localizer.text("stock-quantity"));
                    ui.text_edit_singleline(&mut draft.quantity);
                });
            }
            let quantity = draft
                .quantity
                .parse::<u32>()
                .ok()
                .filter(|&n| n > 0 && n <= MAX_STOCK_QUANTITY);
            let parsed = draft.input(project.display_unit, project.currency);
            if !current {
                ui.colored_label(egui::Color32::LIGHT_RED, self.localizer.text("stock-stale"));
            }
            if let Some(error) = draft.error.or_else(|| parsed.as_ref().err().copied()) {
                let key = match error {
                    StockError::MissingMaterial(_) | StockError::Invalid(StockField::Material) => {
                        "error-material-missing"
                    }
                    StockError::Invalid(StockField::Price) => "error-invalid-amount",
                    StockError::Invalid(StockField::Quantity) => "stock-invalid-quantity",
                    StockError::Invalid(StockField::Trim) => "stock-invalid-trim",
                    _ => "stock-invalid",
                };
                ui.colored_label(egui::Color32::LIGHT_RED, self.localizer.text(key));
            }
            if draft.edit_id.is_none() && quantity.is_none() {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    self.localizer.text("stock-invalid-quantity"),
                );
            }
            ui.horizontal(|ui| {
                cancel = ui.button(self.localizer.text("cancel")).clicked();
                confirm = ui
                    .add_enabled(
                        current
                            && parsed.is_ok()
                            && (draft.edit_id.is_some() || quantity.is_some()),
                        egui::Button::new(self.localizer.text("confirm")),
                    )
                    .clicked();
            });
        });
        draft.focus = false;
        if cancel || modal.should_close() {
            return;
        }
        if confirm {
            let input = draft
                .input(
                    self.editor.project().display_unit,
                    self.editor.project().currency,
                )
                .expect("validated");
            let result = if let Some(id) = draft.edit_id {
                self.editor.edit_stock(id, input).map(|_| ())
            } else {
                self.editor
                    .create_stock(input, draft.quantity.parse().expect("validated"))
                    .map(|_| ())
            };
            match result {
                Ok(()) => {
                    self.material_conflicts = allocation_conflicts(self.editor.project());
                    return;
                }
                Err(plan_my_cabinet::commands::EditError::Command(error)) => {
                    draft.error = Some(error)
                }
                Err(_) => draft.error = Some(StockError::Invalid(StockField::Material)),
            }
        }
        self.stock_dialog = Some(draft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::board_commands::NewMaterial;

    #[test]
    fn modal_cancel_and_invalid_quantity_do_not_commit() {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let initial = app.editor.project().clone();
        let mut draft = StockDialog::new(app.editor.project());
        draft.material_id = Some(material_id);
        draft.dimensions = ["900", "240", "18"].map(|text| DimensionDraft {
            text: text.into(),
            consent: false,
        });
        draft.quantity = "0".into();
        app.stock_dialog = Some(draft);
        let draw = |app: &mut DesktopApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.show_stock_dialog(ui.ctx());
                },
            )
            .drop_without_applying_deltas();
        };
        draw(&mut app, vec![]);
        assert!(app.stock_dialog.is_some());
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
        assert!(app.stock_dialog.is_none());
        assert_eq!(app.editor.project(), &initial);
        assert_eq!(app.editor.project().revision, initial.revision);
    }

    #[test]
    fn rounded_trims_each_require_consent_and_edit_preserves_exact_values() {
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let mut draft = StockDialog::new(app.editor.project());
        draft.material_id = Some(material_id);
        draft.dimensions = ["900", "240", "18"].map(|text| DimensionDraft {
            text: text.into(),
            consent: false,
        });
        draft.trim[0].text = "1/64 in".into();
        draft.trim[1].text = "1/64 in".into();
        assert_eq!(
            draft
                .input(Unit::Mm, app.editor.project().currency)
                .unwrap_err(),
            StockError::Invalid(StockField::Trim)
        );
        draft.trim[0].consent = true;
        assert_eq!(
            draft
                .input(Unit::Mm, app.editor.project().currency)
                .unwrap_err(),
            StockError::Invalid(StockField::Trim)
        );
        draft.trim[1].consent = true;
        let input = draft
            .input(Unit::Mm, app.editor.project().currency)
            .unwrap();
        assert_eq!(
            input.trim,
            [
                Length::from_micrometres(397),
                Length::from_micrometres(397),
                Length::ZERO,
                Length::ZERO
            ]
        );
        let id = app.editor.create_stock(input, 1).unwrap()[0];
        let piece = app
            .editor
            .project()
            .stock
            .iter()
            .find(|piece| piece.id == id)
            .unwrap();
        let edited = StockDialog::edit(app.editor.project(), piece, Locale::En);
        assert_eq!(edited.trim[0].text, "0.397 mm");
        assert!(edited.trim.iter().all(|field| !field.consent));
        assert_eq!(
            edited
                .input(Unit::Mm, app.editor.project().currency)
                .unwrap()
                .trim,
            piece.trim
        );
    }

    #[test]
    fn invalid_trim_does_not_modify_project() {
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let initial = app.editor.project().clone();
        let mut draft = StockDialog::new(app.editor.project());
        draft.material_id = Some(material_id);
        draft.dimensions = ["900", "240", "18"].map(|text| DimensionDraft {
            text: text.into(),
            consent: false,
        });
        draft.trim[0].text = "-1 mm".into();
        assert_eq!(
            draft.input(Unit::Mm, initial.currency).unwrap_err(),
            StockError::Invalid(StockField::Trim)
        );
        app.stock_dialog = Some(draft);
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_stock_dialog(ui.ctx())
        })
        .drop_without_applying_deltas();
        assert!(app.stock_dialog.is_some());
        assert_eq!(app.editor.project(), &initial);
    }
}
