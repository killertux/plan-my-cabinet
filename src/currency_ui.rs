//! Explicit, project-wide currency decisions. No exchange-rate conversion is implied.
use super::*;
use plan_my_cabinet::money::{Currency, Money};
use plan_my_cabinet::stock_commands::ProjectCurrencyChange;

pub(super) struct CurrencyDialog {
    project_id: Uuid,
    revision: u64,
    target: Currency,
    replace: bool,
    relabel_consent: bool,
    cut_fee: String,
    stock_prices: Vec<(Uuid, String)>,
    error: Option<String>,
    chrome: Option<ModalChrome>,
}

fn amount_text(value: Option<Money>) -> String {
    value.map_or_else(String::new, |money| {
        format!(
            "{}.{:02}",
            money.minor_units() / 100,
            money.minor_units() % 100
        )
    })
}

impl CurrencyDialog {
    pub(super) fn new(project: &Project) -> Self {
        Self {
            project_id: project.id,
            revision: project.revision,
            target: if project.currency == Currency::Brl {
                Currency::Usd
            } else {
                Currency::Brl
            },
            replace: false,
            relabel_consent: false,
            cut_fee: String::new(),
            stock_prices: project
                .stock
                .iter()
                .map(|piece| (piece.id, String::new()))
                .collect(),
            error: None,
            chrome: Some(ModalChrome::new(egui::Id::new("currency-change-dialog")).width(500.0)),
        }
    }

    fn decision(&self, project: &Project) -> Option<ProjectCurrencyChange> {
        if self.project_id != project.id
            || self.revision != project.revision
            || self.target == project.currency
        {
            return None;
        }
        if !self.replace {
            return self
                .relabel_consent
                .then_some(ProjectCurrencyChange::ConfirmRelabelWithoutConversion);
        }
        let parse = |text: &str, was_known: bool| -> Option<Option<Money>> {
            if text.trim().is_empty() {
                (!was_known).then_some(None)
            } else {
                Money::parse(self.target, text).ok().map(Some)
            }
        };
        let cut_fee = parse(&self.cut_fee, project.cut_fee.is_some())?;
        let stock_prices = self
            .stock_prices
            .iter()
            .map(|(id, text)| {
                let piece = project.stock.iter().find(|piece| piece.id == *id)?;
                Some((*id, parse(text, piece.price.is_some())?))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(ProjectCurrencyChange::Replace {
            cut_fee,
            stock_prices,
        })
    }
}

impl DesktopApp {
    pub(super) fn show_currency_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.currency_dialog.take() else {
            return;
        };
        let project = self.editor.project();
        let current = draft.project_id == project.id && draft.revision == project.revision;
        let mut chrome = draft.chrome.take().expect("currency modal controller");
        let opening = !chrome.is_active();
        let mut first_control = None;
        let result = chrome.show(
            ctx,
            &self.localizer.text("currency-change-heading"),
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &self.localizer.text("currency-change-heading"),
            },
            |ui| {
                ui.label(format!(
                    "{}: {}",
                    self.localizer.text("currency-current"),
                    project.currency.code()
                ));
                ui.horizontal(|ui| {
                    ui.label(self.localizer.text("currency-new"));
                    first_control = Some(
                        ui.selectable_value(&mut draft.target, Currency::Brl, "BRL")
                            .id,
                    );
                    ui.selectable_value(&mut draft.target, Currency::Usd, "USD");
                });
                ui.small(self.localizer.text("currency-no-conversion"));
                ui.radio_value(
                    &mut draft.replace,
                    false,
                    self.localizer.text("currency-relabel"),
                );
                ui.radio_value(
                    &mut draft.replace,
                    true,
                    self.localizer.text("currency-replace"),
                );
                if !draft.replace {
                    ui.checkbox(
                        &mut draft.relabel_consent,
                        self.localizer.text("currency-relabel-consent"),
                    );
                    ui.small(self.localizer.text("currency-relabel-example"));
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("currency-replacements")
                        .max_height(340.0)
                        .show(ui, |ui| {
                            ui.label(format!(
                                "{} ({})",
                                self.localizer.text("cut-fee"),
                                draft.target.code()
                            ));
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut draft.cut_fee);
                                ui.small(format!(
                                    "{}: {}",
                                    self.localizer.text("currency-previous"),
                                    project.cut_fee.map_or_else(
                                        || self.localizer.text("stock-price-unknown"),
                                        |value| amount_text(Some(value))
                                    )
                                ));
                            });
                            for (id, text) in &mut draft.stock_prices {
                                if let Some(piece) =
                                    project.stock.iter().find(|piece| piece.id == *id)
                                {
                                    ui.label(format!(
                                        "{} · {} ({})",
                                        project.stock_alias(*id).unwrap_or("?"),
                                        piece.name,
                                        draft.target.code()
                                    ));
                                    ui.horizontal(|ui| {
                                        ui.text_edit_singleline(text);
                                        ui.small(format!(
                                            "{}: {}",
                                            self.localizer.text("currency-previous"),
                                            piece.price.map_or_else(
                                                || self.localizer.text("stock-price-unknown"),
                                                |value| amount_text(Some(value))
                                            )
                                        ));
                                    });
                                }
                            }
                        });
                    ui.small(self.localizer.text("currency-replace-hint"));
                }
                if !current {
                    ui.colored_label(egui::Color32::DARK_RED, self.localizer.text("stock-stale"));
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(egui::Color32::DARK_RED, error);
                }
                ((), current && draft.decision(project).is_some())
            },
        );
        if opening && let Some(id) = first_control {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if actions::decision(
            actions::ActionId::CancelDialog,
            result.action == ModalAction::Cancel,
        ) {
            chrome.close(ctx);
            return;
        }
        if actions::decision(
            actions::ActionId::ConfirmDialog,
            result.action == ModalAction::Confirm,
        ) && let Some(decision) = draft.decision(self.editor.project())
        {
            match self.editor.change_project_currency(draft.target, decision) {
                Ok(_) => {
                    chrome.close(ctx);
                    return;
                }
                Err(error) => draft.error = Some(format!("{error:?}")),
            }
        }
        draft.chrome = Some(chrome);
        self.currency_dialog = Some(draft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_requires_all_known_values_and_keeps_unknown_distinct_from_free() {
        let mut project = plan_my_cabinet::reference_fixture::project();
        project.cut_fee = Some(Money::new(project.currency, 0).unwrap());
        project.stock[1].price = None;
        let mut draft = CurrencyDialog::new(&project);
        assert!(draft.decision(&project).is_none());
        draft.relabel_consent = true;
        assert!(matches!(
            draft.decision(&project),
            Some(ProjectCurrencyChange::ConfirmRelabelWithoutConversion)
        ));
        draft.replace = true;
        assert!(draft.decision(&project).is_none());
        draft.cut_fee = "0".into();
        for (id, text) in &mut draft.stock_prices {
            if let Some(piece) = project.stock.iter().find(|piece| piece.id == *id)
                && piece.price.is_some()
            {
                *text = "0".into();
            }
        }
        let Some(ProjectCurrencyChange::Replace {
            cut_fee,
            stock_prices,
        }) = draft.decision(&project)
        else {
            panic!("valid replacement")
        };
        assert_eq!(cut_fee.unwrap().minor_units(), 0);
        assert!(
            stock_prices
                .iter()
                .any(|(id, price)| *id == project.stock[1].id && price.is_none())
        );
        assert!(
            stock_prices
                .iter()
                .any(|(_, price)| price.is_some_and(|value| value.minor_units() == 0))
        );
        draft.stock_prices[0].1 = "invalid".into();
        assert!(draft.decision(&project).is_none());
    }

    #[test]
    fn currency_action_opens_explicit_choice_without_changing_project() {
        let mut app = DesktopApp::default();
        let before = app.editor.project().clone();
        app.invoke(Request::new(actions::ActionId::EditCurrency))
            .unwrap();
        assert!(app.currency_dialog.is_some());
        assert_eq!(app.editor.project(), &before);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            ..Default::default()
        });
        app.show_currency_dialog(&ctx);
        let output = ctx.end_pass();
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().map(str::to_owned))
            .collect();
        output.drop_without_applying_deltas();
        assert!(
            labels
                .iter()
                .any(|label| label.contains("Relabel existing amounts")),
            "{labels:?}"
        );
        assert!(
            labels
                .iter()
                .any(|label| label.contains("Replace known prices")),
            "{labels:?}"
        );
        assert!(app.currency_dialog.is_some());
        assert_eq!(app.editor.project(), &before);
    }
}
