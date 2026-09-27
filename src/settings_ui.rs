//! Native Settings presentation. The host owns every project transaction,
//! preference write, help destination and recovery operation. Rendered controls
//! emit intents rather than changing a portable document in this module.
use eframe::egui::{self, Color32, RichText, Stroke};

use crate::APPLICATION_NAME;
use crate::cost_estimate::ProjectEstimate;
use crate::dimension_input::{Locale, format_length};
use crate::domain::Project;
use crate::i18n::{Language, Localizer};
use crate::kerf_date::confirmation_date_utc;
use crate::local_preferences::{InterfaceScale, LocalPreferences};
use crate::money::{Money, MoneyLocale};
use crate::units::Unit;

const PANEL: Color32 = Color32::from_rgb(251, 250, 247);
const APP: Color32 = Color32::from_rgb(244, 241, 236);
const TEXT: Color32 = Color32::from_rgb(42, 37, 32);
const MUTED: Color32 = Color32::from_rgb(110, 101, 90);
const BORDER: Color32 = Color32::from_rgb(230, 224, 214);
const ACCENT: Color32 = Color32::from_rgb(246, 227, 203);
const WARN: Color32 = Color32::from_rgb(138, 90, 18);
const OK: Color32 = Color32::from_rgb(63, 107, 69);
const FOOTER_HEIGHT: f32 = 58.0;

fn sidebar_width(width: f32) -> f32 {
    if width < 580.0 { 150.0 } else { 208.0 }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Section {
    #[default]
    Cutting,
    GridUnits,
    Costs,
    General,
    Shortcuts,
    About,
}

impl Section {
    pub const ALL: [Self; 6] = [
        Self::Cutting,
        Self::GridUnits,
        Self::Costs,
        Self::General,
        Self::Shortcuts,
        Self::About,
    ];

    fn title(self, language: Language) -> &'static str {
        match self {
            Self::Cutting => tr(language, "Cutting", "Corte"),
            Self::GridUnits => tr(language, "Grid & units", "Grade e unidades"),
            Self::Costs => tr(language, "Costs & currency", "Custos e moeda"),
            Self::General => tr(language, "General", "Geral"),
            Self::Shortcuts => tr(language, "Shortcuts", "Atalhos"),
            Self::About => tr(language, "About", "Sobre"),
        }
    }

    fn icon(self) -> egui::Image<'static> {
        let source = match self {
            Self::Cutting => egui::include_image!("../assets/icons/cut.svg"),
            Self::GridUnits => egui::include_image!("../assets/icons/grid.svg"),
            Self::Costs => egui::include_image!("../assets/icons/layers.svg"),
            Self::General => egui::include_image!("../assets/icons/globe.svg"),
            Self::Shortcuts => egui::include_image!("../assets/icons/command.svg"),
            Self::About => egui::include_image!("../assets/icons/list.svg"),
        };
        egui::Image::new(source)
            .fit_to_exact_size(egui::vec2(14.0, 14.0))
            .tint(MUTED)
    }
}

/// Every command is handled by the host through its existing action and
/// availability guards. Display units are presentation-only and require the
/// host's no-revision/no-undo unit setter; export units remain independent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsIntent {
    Done,
    EditKerf,
    ConfirmKerf,
    EditGrid,
    SetDisplayUnit(Unit),
    EditCutFee,
    SetFreeCutFee,
    ChangeCurrency,
    SetLanguage(Language),
    SetNavigationHints(bool),
    SetInverseScrollZoom(bool),
    SetMaterialTint(bool),
    SetScale(InterfaceScale),
    ShowRecoveryFolder,
    ReviewRecoveryCleanup,
    WorkedExamples,
    HelpShortcuts,
    Source,
}

pub const SOURCE_URL: &str = "https://github.com/killertux/plan-my-cabinet";

#[derive(Default)]
pub struct SettingsState {
    pub section: Section,
    visible: bool,
    invoking_focus: Option<egui::Id>,
    last_focus: Option<egui::Id>,
    #[cfg(test)]
    done_id: Option<egui::Id>,
    #[cfg(test)]
    layout: Option<SettingsLayout>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct SettingsLayout {
    sidebar: egui::Rect,
    content: egui::Rect,
    footer: egui::Rect,
}

impl SettingsState {
    /// The caller must suppress background project/scene shortcuts while open,
    /// dispatch the returned intent after drawing, and call this each frame.
    /// `None` project keeps app sections usable on Welcome/no-document states.
    #[allow(clippy::too_many_arguments)] // Read-only localized inputs, estimate and two independent warning channels.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        localizer: &Localizer,
        project: Option<&Project>,
        preferences: &LocalPreferences,
        estimate: Option<&ProjectEstimate>,
        preference_error: Option<&str>,
        notice: Option<&str>,
    ) -> Vec<SettingsIntent> {
        let language = localizer.language();
        let mut intents = Vec::new();
        let opening = !self.visible;
        if opening {
            self.invoking_focus = ctx.memory(|m| m.focused());
            self.visible = true;
        }
        let popup_before = egui::Popup::is_any_open(ctx);
        let available = ctx.content_rect().size();
        // The modal frame adds four logical points around the requested inner size.
        let width = 776.0_f32.min((available.x - 4.0).max(180.0));
        let height = 553.0_f32.min((available.y - 7.0).max(160.0));
        let modal = egui::Modal::new(egui::Id::new("settings-modal"))
            .frame(egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, BORDER)).corner_radius(12).inner_margin(0))
            .show(ctx, |ui| {
            ui.set_min_size(egui::vec2(width, height));
            ui.set_max_size(egui::vec2(width, height));
            egui::Frame::new().fill(PANEL).show(ui, |ui| {
                 let body_height = (height - FOOTER_HEIGHT).max(80.0);
                 let sidebar_width = sidebar_width(width);
                 let content_inset = if width < 580.0 { 14.0 } else { 26.0 };
                 let first_focus = ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                     let sidebar = egui::Frame::new()
                        .fill(APP)
                        .stroke(Stroke::new(1.0, BORDER))
                         .inner_margin(egui::Margin::symmetric(10, 16))
                        .show(ui, |ui| {
                            ui.set_width(sidebar_width - 20.0);
                            ui.set_max_width(sidebar_width - 20.0);
                            ui.set_min_height(body_height - 32.0);
                            egui::ScrollArea::vertical()
                                .id_salt("settings-sections")
                                .max_height(body_height - 32.0)
                                .show(ui, |ui| {
                                    ui.set_width(sidebar_width - 20.0);
                                    ui.vertical(|ui| self.sidebar(ui, language, project, sidebar_width - 20.0)).inner
                                }).inner
                         });
                     let content = egui::Frame::new().inner_margin(egui::Margin::symmetric(content_inset as i8, 0)).show(ui, |ui| {
                     egui::ScrollArea::vertical()
                         .id_salt(("settings-content", self.section))
                         .max_width((width - sidebar_width - 2.0 * content_inset).max(110.0))
                         .max_height(body_height)
                         .auto_shrink([false, false])
                         .show(ui, |ui| {
                             ui.set_width((width - sidebar_width - 2.0 * content_inset).max(105.0));
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                            ui.vertical(|ui| {
                                 ui.add_space(23.0);
                                if let Some(notice) = notice {
                                    ui.colored_label(WARN, notice);
                                    ui.add_space(8.0);
                                }
                                self.content(ui, localizer, project, preferences, estimate, preference_error, &mut intents);
                                ui.add_space(16.0);
                         });
                     });
                     });
                     (sidebar.inner, sidebar.response.rect, content.response.rect)
                 }).inner;
                 let footer = egui::Frame::new().fill(APP).stroke(Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::symmetric(18, 7)).show(ui, |ui| {
                     ui.set_min_height(FOOTER_HEIGHT - 16.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let done = ui.add(egui::Button::new(RichText::new(format!("{}  Esc", tr(language, "Done", "Concluído"))).color(PANEL)).fill(TEXT).stroke(Stroke::NONE).min_size(egui::vec2(92.0, 32.0)));
                        let hint = match (project.is_some(), self.section) {
                            (true, Section::GridUnits) => tr(language,
                                "Grid edits can be undone; display units do not edit the project",
                                "Alterações da grade podem ser desfeitas; unidades não editam o projeto"),
                            (true, Section::Cutting | Section::Costs) => tr(language,
                                "Project changes apply immediately and can be undone",
                                "Alterações do projeto são aplicadas imediatamente e podem ser desfeitas"),
                            _ => tr(language, "App settings save automatically",
                                "Preferências do aplicativo são salvas automaticamente"),
                        };
                         ui.add_sized([ui.available_width().max(80.0), 42.0],
                            egui::Label::new(RichText::new(hint).size(11.5).color(MUTED)).wrap());
                         done
                    }).inner
                 });
                 (first_focus.0, footer.inner, first_focus.1, first_focus.2, footer.response.rect)
             }).inner
         });
        #[cfg(test)]
        {
            self.done_id = Some(modal.inner.1.id);
            self.layout = Some(SettingsLayout {
                sidebar: modal.inner.2,
                content: modal.inner.3,
                footer: modal.inner.4,
            });
        }
        if opening {
            if let Some(id) = modal.inner.0 {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        } else if !popup_before
            && ctx.memory(|m| m.focused()).is_none()
            && let Some(id) = self.last_focus
        {
            // A temporarily suspended Settings layer can lose focus while
            // a child dialog is shown; restore its last live control.
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if !popup_before {
            self.last_focus = ctx.memory(|m| m.focused());
        }
        let popup_active = popup_before || modal.any_popup_open || egui::Popup::is_any_open(ctx);
        let top = modal.is_top_modal || opening && ctx.memory(|m| m.top_modal_layer().is_none());
        if top && !popup_active && (modal.inner.1.clicked() || modal.should_close()) {
            intents.push(SettingsIntent::Done);
        }
        // Other intents may temporarily suspend Settings for a child dialog;
        // keep the parent's focus record until it resumes. Only Done ends it.
        if intents.contains(&SettingsIntent::Done) {
            self.close(ctx);
        }
        intents
    }

    // Settings keeps its larger two-column section list and Done-only footer,
    // but uses the same focus lifecycle as ModalChrome on final dismissal.
    fn close(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.invoking_focus.take() {
            ctx.memory_mut(|m| m.request_focus(id));
        } else {
            ctx.memory_mut(|m| m.stop_text_input());
        }
        self.visible = false;
        self.last_focus = None;
    }

    fn sidebar(
        &mut self,
        ui: &mut egui::Ui,
        language: Language,
        project: Option<&Project>,
        width: f32,
    ) -> Option<egui::Id> {
        let mut first_focus = None;
        let mut selected_focus = None;
        ui.add_space(2.0);
        ui.label(
            RichText::new(tr(language, "PROJECT", "PROJETO"))
                .small()
                .strong()
                .color(MUTED),
        );
        ui.add(
            egui::Label::new(
                RichText::new(project.map_or(
                    tr(language, "No project open", "Nenhum projeto aberto"),
                    |p| p.name.as_str(),
                ))
                .small()
                .color(MUTED),
            )
            .truncate(),
        );
        ui.add_space(8.0);
        for section in Section::ALL.into_iter().take(3) {
            let selected = self.section == section;
            let issue = project.is_some_and(|project| match section {
                Section::Cutting => project.confirmed_shop_kerf != Some(project.cutting_kerf),
                Section::Costs => project.cut_fee.is_none(),
                _ => false,
            });
            let title = if issue {
                format!("{}  •", section.title(language))
            } else {
                section.title(language).to_owned()
            };
            let response = ui.add_enabled(
                project.is_some(),
                egui::Button::image_and_text(section.icon(), title)
                    .selected(selected)
                    .fill(if selected { ACCENT } else { APP })
                    .stroke(Stroke::NONE)
                    .min_size(egui::vec2(width, 32.0)),
            );
            if response.clicked() {
                self.section = section;
            }
            if first_focus.is_none() && project.is_some() {
                first_focus = Some(response.id);
            }
            if selected && project.is_some() {
                selected_focus = Some(response.id);
            }
        }
        ui.add_space(15.0);
        ui.label(
            RichText::new(tr(language, "APP", "APLICATIVO"))
                .small()
                .strong()
                .color(MUTED),
        );
        for section in Section::ALL.into_iter().skip(3) {
            let selected = self.section == section;
            let response = ui.add(
                egui::Button::image_and_text(section.icon(), section.title(language))
                    .selected(selected)
                    .fill(if selected { ACCENT } else { APP })
                    .stroke(Stroke::NONE)
                    .min_size(egui::vec2(width, 32.0)),
            );
            if response.clicked() {
                self.section = section;
            }
            if first_focus.is_none() {
                first_focus = Some(response.id);
            }
            if selected {
                selected_focus = Some(response.id);
            }
        }
        selected_focus.or(first_focus)
    }

    #[allow(clippy::too_many_arguments)] // localized read-only inputs and intent sink
    fn content(
        &self,
        ui: &mut egui::Ui,
        l: &Localizer,
        project: Option<&Project>,
        prefs: &LocalPreferences,
        estimate: Option<&ProjectEstimate>,
        preference_error: Option<&str>,
        out: &mut Vec<SettingsIntent>,
    ) {
        let language = l.language();
        let section = if project.is_none()
            && matches!(
                self.section,
                Section::Cutting | Section::GridUnits | Section::Costs
            ) {
            Section::General
        } else {
            self.section
        };
        ui.label(
            RichText::new(section.title(language))
                .size(17.0)
                .strong()
                .color(TEXT),
        );
        ui.label(RichText::new(match section {
            Section::Cutting => tr(language, "Used by first-fit, optimization, cut sequences and the shop packet.", "Usado no primeiro encaixe, otimização, sequência de cortes e pacote da oficina."),
            Section::GridUnits => tr(language, "Project grid and measurement presentation.", "Grade do projeto e apresentação das medidas."),
            Section::Costs => tr(language, "Estimates exclude taxes, delivery, setup and stacking discounts.", "Estimativas não incluem impostos, entrega, preparação nem descontos por empilhamento."),
            Section::General => tr(language, "Stored on this computer, not in project files.", "Salvo neste computador, não nos arquivos do projeto."),
            Section::Shortcuts => tr(language, "Keyboard and trackpad controls; available offline.", "Controles de teclado e trackpad; disponíveis sem internet."),
            Section::About => tr(language, "Application and source information.", "Informações do aplicativo e código-fonte."),
        }).size(12.5).color(MUTED));
        ui.add_space(22.0);
        match section {
            Section::Cutting => {
                if let Some(project) = project {
                    cutting(ui, l, project, out);
                }
            }
            Section::GridUnits => {
                if let Some(project) = project {
                    grid_units(ui, l, project, out);
                }
            }
            Section::Costs => {
                if let Some(project) = project {
                    costs(ui, l, project, estimate, out);
                }
            }
            Section::General => {
                if let Some(error) = preference_error {
                    ui.colored_label(
                        WARN,
                        format!(
                            "{}: {error}",
                            tr(
                                language,
                                "Preferences could not be saved",
                                "Não foi possível salvar as preferências"
                            )
                        ),
                    );
                }
                general(ui, l, prefs, out);
            }
            Section::Shortcuts => shortcuts(ui, language, out),
            Section::About => about(ui, language, out),
        }
    }
}

fn tr(language: Language, en: &'static str, pt: &'static str) -> &'static str {
    match language {
        Language::En => en,
        Language::PtBr => pt,
    }
}

fn rule(ui: &mut egui::Ui) {
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(10.0);
}

fn row(ui: &mut egui::Ui, title: &str, hint: &str, content: impl FnOnce(&mut egui::Ui)) {
    // Below 490 points, stack the label above the control instead of clipping.
    if ui.available_width() < 390.0 {
        ui.label(RichText::new(title).strong());
        if !hint.is_empty() {
            ui.small(RichText::new(hint).color(MUTED));
        }
        content(ui);
    } else {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(156.0);
                ui.label(RichText::new(title).strong());
                if !hint.is_empty() {
                    ui.small(RichText::new(hint).color(MUTED));
                }
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.set_max_width(ui.available_width());
                content(ui);
            });
        });
    }
}

fn cutting(ui: &mut egui::Ui, l: &Localizer, project: &Project, out: &mut Vec<SettingsIntent>) {
    let language = l.language();
    let locale = locale(language);
    row(
        ui,
        tr(language, "Blade kerf", "Espessura do corte"),
        tr(
            language,
            "Width of one saw pass",
            "Largura de uma passada da serra",
        ),
        |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(format!(
                        "{}  ✎",
                        format_length(project.cutting_kerf, Unit::Mm, locale, 3)
                    ))
                    .on_hover_text(l.text("cutting-kerf-edit"))
                    .clicked()
                {
                    out.push(SettingsIntent::EditKerf);
                }
                if project.confirmed_shop_kerf == Some(project.cutting_kerf) {
                    let status = confirmation_date_utc(project.confirmed_shop_kerf_unix_ms)
                        .map_or_else(
                            || l.text("cutting-kerf-confirmed-unknown-date"),
                            |date| {
                                let mut args = fluent_bundle::FluentArgs::new();
                                args.set("date", date);
                                l.format("cutting-kerf-confirmed-on", Some(&args))
                            },
                        );
                    ui.colored_label(OK, status);
                } else {
                    ui.colored_label(WARN, l.text("shell-unconfirmed"));
                }
            });
            egui::Frame::new().fill(Color32::WHITE).stroke(Stroke::new(1.0, BORDER)).corner_radius(9).inner_margin(12).show(ui, |ui| {
            let confirmed = project.confirmed_shop_kerf == Some(project.cutting_kerf);
            if ui.add_enabled(!confirmed, egui::Button::new(tr(language,
                "Confirm kerf and cutting practice with shop", "Confirmar corte e prática de trabalho com a oficina")))
                .clicked() { out.push(SettingsIntent::ConfirmKerf); }
            ui.small(tr(language,
                "Required for Shop-ready. Changing kerf clears confirmation and rechecks placements.",
                "Obrigatório para o pacote pronto para a oficina. Alterar o corte remove a confirmação e reavalia as alocações."));
        });
        },
    );
    rule(ui);
    row(
        ui,
        tr(language, "Cutting model", "Modelo de corte"),
        tr(language, "Plan assumptions", "Premissas do plano"),
        |ui| {
            for line in [
                tr(
                    language,
                    "Straight full-span cuts parallel to an edge",
                    "Cortes retos de ponta a ponta, paralelos a uma borda",
                ),
                tr(
                    language,
                    "One physical pass per split; trims included",
                    "Uma passada física por divisão; inclui refilos",
                ),
                tr(
                    language,
                    "No stacking, stopped, angled or interior cuts",
                    "Sem empilhamento nem cortes interrompidos, angulados ou internos",
                ),
            ] {
                ui.label(format!("• {line}"));
            }
            if ui
                .link(tr(language, "Worked examples →", "Exemplos práticos →"))
                .clicked()
            {
                out.push(SettingsIntent::WorkedExamples);
            }
        },
    );
}

fn grid_units(ui: &mut egui::Ui, l: &Localizer, project: &Project, out: &mut Vec<SettingsIntent>) {
    let language = l.language();
    row(
        ui,
        l.text("grid-spacing").as_str(),
        tr(language, "XY project spacing", "Espaçamento XY do projeto"),
        |ui| {
            if ui
                .button(format!(
                    "{}  ✎",
                    format_length(project.grid_spacing, Unit::Mm, locale(language), 3)
                ))
                .on_hover_text(l.text("grid-edit"))
                .clicked()
            {
                out.push(SettingsIntent::EditGrid);
            }
            ui.small(tr(language, "Changing spacing is an undoable project edit; existing poses are not snapped again.",
            "Alterar o espaçamento é uma edição desfeita pelo histórico; posições existentes não são ajustadas."));
        },
    );
    rule(ui);
    row(
        ui,
        tr(language, "Display units", "Unidades de exibição"),
        tr(language, "Measurements only", "Somente medidas"),
        |ui| {
            ui.horizontal_wrapped(|ui| {
                for (unit, name) in [
                    (Unit::Mm, "mm"),
                    (Unit::Cm, "cm"),
                    (Unit::M, "m"),
                    (Unit::Inch, "in"),
                    (Unit::Foot, "ft"),
                ] {
                    if ui
                        .selectable_label(project.display_unit == unit, name)
                        .clicked()
                    {
                        out.push(SettingsIntent::SetDisplayUnit(unit));
                    }
                }
            });
            ui.small(tr(language, "Presentation only: no revision, undo entry or conversion of physical dimensions. PDF units are chosen at export.",
            "Somente apresentação: sem revisão, entrada no histórico ou conversão física. As unidades do PDF são escolhidas na exportação."));
        },
    );
}

fn costs(
    ui: &mut egui::Ui,
    l: &Localizer,
    project: &Project,
    estimate: Option<&ProjectEstimate>,
    out: &mut Vec<SettingsIntent>,
) {
    let language = l.language();
    let money_locale = if language == Language::En {
        MoneyLocale::English
    } else {
        MoneyLocale::PortugueseBrazil
    };
    row(
        ui,
        tr(language, "Fee per cut", "Custo por corte"),
        tr(language, "Per physical pass", "Por passada física"),
        |ui| {
            ui.horizontal_wrapped(|ui| {
                let value = project.cut_fee.map_or_else(
                    || tr(language, "unknown", "desconhecido").to_owned(),
                    |fee| fee.display(money_locale),
                );
                if ui.button(format!("{}  ✎", value)).clicked() {
                    out.push(SettingsIntent::EditCutFee);
                }
                if project.cut_fee.is_none_or(|fee| fee.minor_units() != 0)
                    && ui.button(l.text("cut-fee-free")).clicked()
                {
                    out.push(SettingsIntent::SetFreeCutFee);
                }
            });
            ui.small(tr(
                language,
                "Blank is unknown, not zero. Trims count as cuts.",
                "Em branco é desconhecido, não zero. Refilos contam como cortes.",
            ));
        },
    );
    rule(ui);
    row(
        ui,
        tr(language, "Currency", "Moeda"),
        tr(language, "One per project", "Uma por projeto"),
        |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(project.currency.code()).monospace());
                if ui.button(tr(language, "Change…", "Alterar…")).clicked() {
                    out.push(SettingsIntent::ChangeCurrency);
                }
            });
            ui.small(l.text("currency-no-conversion"));
        },
    );
    rule(ui);
    row(
        ui,
        tr(language, "Current estimate", "Estimativa atual"),
        "",
        |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(252, 244, 231))
                .stroke(Stroke::new(1.0, Color32::from_rgb(235, 210, 176)))
                .inner_margin(12)
                .corner_radius(9)
                .show(ui, |ui| {
                    if let Some(estimate) = estimate {
                        let display = |value: Option<Money>| {
                            value.map_or_else(
                                || tr(language, "unknown", "desconhecido").to_owned(),
                                |m| m.display(money_locale),
                            )
                        };
                        ui.label(format!(
                            "{}: {}",
                            tr(language, "Material to purchase", "Material a comprar"),
                            display(estimate.material)
                        ));
                        let cuts = estimate
                            .used_stock
                            .iter()
                            .try_fold(0_u64, |sum, stock| sum.checked_add(stock.cuts?));
                        ui.label(format!(
                            "{}: {} · {}",
                            tr(language, "Physical cuts", "Cortes físicos"),
                            cuts.map_or_else(
                                || tr(language, "unverified", "não verificado").to_owned(),
                                |n| n.to_string()
                            ),
                            display(estimate.cutting)
                        ));
                        ui.label(format!(
                            "{}: {}",
                            tr(language, "New spending", "Novo gasto"),
                            estimate.total.map_or_else(
                                || tr(language, "incomplete", "incompleto").to_owned(),
                                |m| m.display(money_locale)
                            )
                        ));
                    } else {
                        ui.label(tr(
                            language,
                            "Estimate unavailable until calculated",
                            "Estimativa indisponível até o cálculo",
                        ));
                    }
                });
        },
    );
}

fn general(
    ui: &mut egui::Ui,
    l: &Localizer,
    prefs: &LocalPreferences,
    out: &mut Vec<SettingsIntent>,
) {
    let language = l.language();
    row(ui, l.text("ui-language").as_str(), "", |ui| {
        ui.horizontal_wrapped(|ui| {
            for (value, label) in [
                (Language::En, l.text("language-en")),
                (Language::PtBr, l.text("language-pt-br")),
            ] {
                if ui
                    .selectable_label(prefs.language == value, label)
                    .clicked()
                {
                    out.push(SettingsIntent::SetLanguage(value));
                }
            }
        });
        ui.small(tr(
            language,
            "Does not change prices, units or PDF language.",
            "Não altera preços, unidades nem o idioma do PDF.",
        ));
    });
    rule(ui);
    row(
        ui,
        tr(language, "Recovery", "Recuperação"),
        tr(language, "Autosave", "Cópia automática"),
        |ui| {
            ui.label(tr(language,"A snapshot is written 30 seconds after the last committed edit; it never replaces the saved project.",
            "Uma cópia é criada 30 segundos após a última edição confirmada; ela nunca substitui o projeto salvo."));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                if ui
                    .link(tr(
                        language,
                        "Show recovery folder",
                        "Mostrar pasta de recuperação",
                    ))
                    .clicked()
                {
                    out.push(SettingsIntent::ShowRecoveryFolder);
                }
                if ui
                    .link(tr(language, "Review snapshots…", "Revisar cópias…"))
                    .clicked()
                {
                    out.push(SettingsIntent::ReviewRecoveryCleanup);
                }
            });
        },
    );
    rule(ui);
    row(
        ui,
        tr(language, "Viewport", "Área de visualização"),
        "",
        |ui| {
            for (value, label, intent) in [
                (
                    prefs.navigation_hints,
                    tr(
                        language,
                        "Show navigation hints in status bar",
                        "Mostrar dicas de navegação na barra de status",
                    ),
                    SettingsIntent::SetNavigationHints(!prefs.navigation_hints),
                ),
                (
                    prefs.inverse_scroll_zoom,
                    tr(
                        language,
                        "Invert scroll-to-zoom",
                        "Inverter rolagem para zoom",
                    ),
                    SettingsIntent::SetInverseScrollZoom(!prefs.inverse_scroll_zoom),
                ),
                (
                    prefs.material_tint,
                    tr(
                        language,
                        "Tint boards by material colour",
                        "Colorir peças pela cor do material",
                    ),
                    SettingsIntent::SetMaterialTint(!prefs.material_tint),
                ),
            ] {
                let mut selected = value;
                if ui.checkbox(&mut selected, label).changed() {
                    out.push(intent);
                }
            }
        },
    );
    rule(ui);
    row(
        ui,
        tr(language, "Interface scale", "Escala da interface"),
        "",
        |ui| {
            ui.horizontal_wrapped(|ui| {
                for scale in [
                    InterfaceScale::Percent90,
                    InterfaceScale::Percent100,
                    InterfaceScale::Percent115,
                    InterfaceScale::Percent130,
                ] {
                    if ui
                        .selectable_label(
                            prefs.interface_scale == scale,
                            format!("{}%", scale.percent()),
                        )
                        .clicked()
                    {
                        out.push(SettingsIntent::SetScale(scale));
                    }
                }
            });
        },
    );
}

fn shortcuts(ui: &mut egui::Ui, language: Language, out: &mut Vec<SettingsIntent>) {
    let command = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    };
    for (keys, en, pt) in [
        (
            format!("{command}1–5"),
            "Switch Design / Stock / Cut plan / Hardware / Handoff",
            "Alternar Projeto 3D / Estoque / Plano de corte / Ferragens / Entrega",
        ),
        (
            format!("{command}K"),
            "Search commands · arrows choose · Enter runs · Esc closes",
            "Buscar comandos · setas escolhem · Enter executa · Esc fecha",
        ),
        (format!("{command}S"), "Save project", "Salvar projeto"),
        (
            format!("{command}Z / {command}Shift+Z"),
            "Undo / Redo",
            "Desfazer / Refazer",
        ),
        (
            "Arrows / Shift+arrows".into(),
            "Orbit / pan focused viewport",
            "Orbitar / deslocar a vista em foco",
        ),
        (
            "+ / − / F".into(),
            "Zoom / frame selection in focused viewport",
            "Zoom / enquadrar seleção na vista em foco",
        ),
        (
            "Scroll / pinch".into(),
            "Zoom on trackpad; Shift+drag pans, drag orbits",
            "Zoom no trackpad; Shift+arrastar desloca, arrastar orbita",
        ),
        (
            "Alt / Esc".into(),
            "Bypass snaps while moving / cancel preview or active edit",
            "Ignorar encaixes ao mover / cancelar prévia ou edição ativa",
        ),
        (
            format!("{command},"),
            "Open Settings",
            "Abrir configurações",
        ),
    ] {
        ui.horizontal_top(|ui| {
            ui.label(RichText::new(keys).monospace().color(TEXT));
            ui.label(tr(language, en, pt));
        });
        ui.add_space(6.0);
    }
    if ui
        .link(tr(
            language,
            "Open shortcut help →",
            "Abrir ajuda de atalhos →",
        ))
        .clicked()
    {
        out.push(SettingsIntent::HelpShortcuts);
    }
}

fn about(ui: &mut egui::Ui, language: Language, out: &mut Vec<SettingsIntent>) {
    ui.heading(APPLICATION_NAME);
    ui.label(format!(
        "{} {}",
        tr(language, "Version", "Versão"),
        env!("CARGO_PKG_VERSION")
    ));
    ui.label(format!(
        "{}: {} / {}",
        tr(language, "Platform", "Plataforma"),
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    ui.label(tr(
        language,
        "Native desktop application; project files remain portable.",
        "Aplicativo nativo; os arquivos de projeto permanecem portáteis.",
    ));
    ui.label(format!(
        "{}: {SOURCE_URL}",
        tr(language, "Source", "Código-fonte")
    ));
    if ui
        .link(tr(
            language,
            "Open source page →",
            "Abrir página do código-fonte →",
        ))
        .clicked()
    {
        out.push(SettingsIntent::Source);
    }
}

fn locale(language: Language) -> Locale {
    match language {
        Language::En => Locale::En,
        Language::PtBr => Locale::PtBr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;

    fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(780.0, 560.0),
            )),
            events,
            ..Default::default()
        }
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn frame(ctx: &egui::Context, events: Vec<egui::Event>, f: impl FnMut(&mut egui::Ui)) {
        let mut output = ctx.run_ui(input(events), f);
        output.textures_delta.clear();
    }

    #[test]
    fn compact_portuguese_settings_keep_modal_bounded_and_done_reachable() {
        let ctx = egui::Context::default();
        ctx.set_zoom_factor(1.3);
        let l = Localizer::new(Language::PtBr);
        let project = Project::new("Projeto de cozinha com nome bastante longo", Currency::Brl);
        let prefs = LocalPreferences {
            language: Language::PtBr,
            interface_scale: InterfaceScale::Percent130,
            ..Default::default()
        };
        let mut state = SettingsState::default();
        for section in Section::ALL {
            state.section = section;
            let input = egui::RawInput {
                // Deliberately stress the effective available area as well as
                // the reference 780x560 logical window.
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(780.0, 560.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |_ui| {
                assert!(
                    state
                        .show(&ctx, &l, Some(&project), &prefs, None, None, None)
                        .is_empty()
                );
            });
            output.textures_delta.clear();
            let rect = ctx
                .memory(|m| m.area_rect(egui::Id::new("settings-modal")))
                .unwrap();
            assert!(
                rect.width() <= ctx.content_rect().width() + 1.0,
                "{section:?}: {rect:?}"
            );
            assert!(
                rect.height() <= ctx.content_rect().height() + 1.0,
                "{section:?}: {rect:?}"
            );
            let layout = state.layout.unwrap();
            assert!(
                layout.footer.bottom() <= rect.bottom() + 1.0,
                "{section:?}: {layout:?}"
            );
            assert!(
                layout.content.right() <= rect.right() + 1.0,
                "{section:?}: {layout:?}"
            );
            assert!(
                layout.sidebar.bottom() <= layout.footer.top() + 1.0,
                "{section:?}: {layout:?}"
            );
        }
    }

    #[test]
    fn reference_settings_geometry_and_active_section_focus() {
        let l = Localizer::new(Language::En);
        let project = Project::new("Kitchen base 800", Currency::Brl);
        let mut focus_ids = Vec::new();
        for section in [Section::Cutting, Section::Costs, Section::General] {
            let ctx = egui::Context::default();
            frame(&ctx, Vec::new(), |_ui| {});
            frame(&ctx, Vec::new(), |_ui| {});
            let mut state = SettingsState {
                section,
                ..Default::default()
            };
            frame(&ctx, Vec::new(), |_ui| {
                state.show(
                    &ctx,
                    &l,
                    Some(&project),
                    &LocalPreferences::default(),
                    None,
                    None,
                    None,
                );
            });
            let area = ctx
                .memory(|m| m.area_rect(egui::Id::new("settings-modal")))
                .unwrap();
            let layout = state.layout.unwrap();
            assert!((area.width() - 780.0).abs() <= 2.0, "{section:?}: {area:?}");
            assert!(
                (area.height() - 560.0).abs() <= 2.0,
                "{section:?}: {area:?}"
            );
            assert!(
                (layout.sidebar.right() - area.left() - 210.0).abs() <= 3.0,
                "{section:?}: {layout:?}"
            );
            assert!(
                (layout.footer.top() - area.top() - 502.0).abs() <= 3.0,
                "{section:?}: {layout:?}"
            );
            assert!(
                (layout.content.left() + 26.0 - area.left() - 236.0).abs() <= 3.0,
                "{section:?}: {layout:?}"
            );
            focus_ids.push(ctx.memory(|m| m.focused()).unwrap());
        }
        assert_ne!(
            focus_ids[0], focus_ids[1],
            "inactive Cutting must not retain initial focus on Costs"
        );
        assert_ne!(
            focus_ids[0], focus_ids[2],
            "inactive Cutting must not retain initial focus on General"
        );
    }

    #[test]
    fn compact_portuguese_footer_is_reachable_at_enlarged_scale() {
        let ctx = egui::Context::default();
        ctx.set_zoom_factor(1.3);
        let l = Localizer::new(Language::PtBr);
        let project = Project::new(
            "Projeto muito comprido para a lista de seções",
            Currency::Brl,
        );
        let prefs = LocalPreferences {
            language: Language::PtBr,
            interface_scale: InterfaceScale::Percent130,
            ..Default::default()
        };
        let mut state = SettingsState {
            section: Section::General,
            ..Default::default()
        };
        let compact = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(520.0, 380.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(compact(Vec::new()), |_ui| {});
        output.textures_delta.clear();
        for section in [Section::Cutting, Section::Costs, Section::General] {
            state.section = section;
            let mut output = ctx.run_ui(compact(Vec::new()), |_ui| {
                state.show(&ctx, &l, Some(&project), &prefs, None, None, None);
            });
            output.textures_delta.clear();
            let area = ctx
                .memory(|m| m.area_rect(egui::Id::new("settings-modal")))
                .unwrap();
            let layout = state.layout.unwrap();
            assert!(
                area.width() <= ctx.content_rect().width() + 1.0,
                "{section:?}: {area:?}"
            );
            assert!(
                area.height() <= ctx.content_rect().height() + 1.0,
                "{section:?}: {area:?}"
            );
            assert!(
                layout.footer.bottom() <= area.bottom() + 1.0,
                "{section:?}: {layout:?}"
            );
            assert!(
                layout.sidebar.right() <= layout.content.left() + 1.0,
                "{section:?}: {layout:?}"
            );
        }
        ctx.memory_mut(|m| m.request_focus(state.done_id.unwrap()));
        let mut output = ctx.run_ui(compact(vec![key(egui::Key::Enter)]), |_ui| {
            assert_eq!(
                state.show(&ctx, &l, Some(&project), &prefs, None, None, None),
                vec![SettingsIntent::Done]
            );
        });
        output.textures_delta.clear();
    }

    #[test]
    fn app_sections_render_without_document_and_escape_closes() {
        let ctx = egui::Context::default();
        let l = Localizer::new(Language::En);
        let mut state = SettingsState::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(780.0, 560.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |_ui| {
            state.show(
                &ctx,
                &l,
                None,
                &LocalPreferences::default(),
                None,
                None,
                None,
            );
        });
        output.textures_delta.clear();
        state.section = Section::Shortcuts;
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(780.0, 560.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |_ui| {
            assert!(
                state
                    .show(
                        &ctx,
                        &l,
                        None,
                        &LocalPreferences::default(),
                        None,
                        None,
                        None
                    )
                    .contains(&SettingsIntent::Done)
            );
        });
        output.textures_delta.clear();
    }

    #[test]
    fn tab_stays_on_settings_modal_layer() {
        let ctx = egui::Context::default();
        let l = Localizer::new(Language::En);
        let mut state = SettingsState {
            section: Section::General,
            ..Default::default()
        };
        let background_id = std::cell::Cell::new(None);
        let check_layer = std::cell::Cell::new(false);
        let mut frame = |events| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(780.0, 560.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let background = ui.button("Background");
                background_id.set(Some(background.id));
                state.show(
                    &ctx,
                    &l,
                    None,
                    &LocalPreferences::default(),
                    None,
                    None,
                    None,
                );
                if check_layer.get() {
                    assert!(!ctx.memory(|m| m.allows_interaction(background.layer_id)));
                }
            });
            output.textures_delta.clear();
        };
        frame(Vec::new());
        check_layer.set(true);
        for modifiers in [egui::Modifiers::NONE, egui::Modifiers::SHIFT] {
            frame(vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }]);
            assert_ne!(ctx.memory(|m| m.focused()), background_id.get());
            assert!(ctx.memory(|m| m.focused()).is_some());
        }
    }

    #[test]
    fn first_action_receives_focus_and_escape_restores_invoker() {
        let ctx = egui::Context::default();
        let l = Localizer::new(Language::En);
        let mut state = SettingsState::default();
        let mut invoker = None;
        frame(&ctx, Vec::new(), |ui| {
            let button = ui.button("Settings");
            invoker = Some(button.id);
            button.request_focus();
            assert!(
                state
                    .show(
                        &ctx,
                        &l,
                        None,
                        &LocalPreferences::default(),
                        None,
                        None,
                        None
                    )
                    .is_empty()
            );
            assert_eq!(ctx.memory(|m| m.focused()), state.last_focus);
            assert_ne!(state.last_focus, invoker);
            assert!(state.last_focus.is_some());
        });
        frame(&ctx, vec![key(egui::Key::Escape)], |_ui| {
            assert_eq!(
                state.show(
                    &ctx,
                    &l,
                    None,
                    &LocalPreferences::default(),
                    None,
                    None,
                    None
                ),
                vec![SettingsIntent::Done]
            );
        });
        assert_eq!(ctx.memory(|m| m.focused()), invoker);
        assert!(!state.visible);
        frame(&ctx, Vec::new(), |_ui| {
            state.show(
                &ctx,
                &l,
                None,
                &LocalPreferences::default(),
                None,
                None,
                None,
            );
        });
        assert!(state.visible);
        assert_ne!(ctx.memory(|m| m.focused()), invoker);
    }

    #[test]
    fn focused_done_footer_restores_invoker() {
        let ctx = egui::Context::default();
        let l = Localizer::new(Language::En);
        let mut state = SettingsState::default();
        let mut invoker = None;
        frame(&ctx, Vec::new(), |ui| {
            let button = ui.button("Settings");
            invoker = Some(button.id);
            button.request_focus();
            state.show(
                &ctx,
                &l,
                None,
                &LocalPreferences::default(),
                None,
                None,
                None,
            );
        });
        ctx.memory_mut(|m| m.request_focus(state.done_id.unwrap()));
        frame(&ctx, vec![key(egui::Key::Enter)], |_ui| {
            assert_eq!(
                state.show(
                    &ctx,
                    &l,
                    None,
                    &LocalPreferences::default(),
                    None,
                    None,
                    None
                ),
                vec![SettingsIntent::Done]
            );
        });
        assert_eq!(ctx.memory(|m| m.focused()), invoker);
    }

    #[test]
    fn popup_enter_and_escape_do_not_close_settings() {
        let ctx = egui::Context::default();
        let l = Localizer::new(Language::En);
        let mut state = SettingsState::default();
        frame(&ctx, Vec::new(), |_ui| {
            state.show(
                &ctx,
                &l,
                None,
                &LocalPreferences::default(),
                None,
                None,
                None,
            );
        });
        let popup = egui::Id::new("settings-child-popup");
        for key_event in [egui::Key::Enter, egui::Key::Escape] {
            ctx.memory_mut(|m| m.request_focus(state.done_id.unwrap()));
            egui::Popup::open_id(&ctx, popup);
            frame(&ctx, vec![key(key_event)], |_ui| {
                assert!(
                    state
                        .show(
                            &ctx,
                            &l,
                            None,
                            &LocalPreferences::default(),
                            None,
                            None,
                            None
                        )
                        .is_empty()
                );
                assert!(state.visible);
            });
            egui::Popup::close_id(&ctx, popup);
        }
        frame(&ctx, vec![key(egui::Key::Escape)], |_ui| {
            assert_eq!(
                state.show(
                    &ctx,
                    &l,
                    None,
                    &LocalPreferences::default(),
                    None,
                    None,
                    None
                ),
                vec![SettingsIntent::Done]
            );
        });
    }

    #[test]
    fn child_dialog_returns_focus_to_settings_before_done_restores_invoker() {
        let ctx = egui::Context::default();
        let l = Localizer::new(Language::En);
        let mut state = SettingsState::default();
        let mut invoker = None;
        frame(&ctx, Vec::new(), |ui| {
            let button = ui.button("Settings");
            invoker = Some(button.id);
            button.request_focus();
            state.show(
                &ctx,
                &l,
                None,
                &LocalPreferences::default(),
                None,
                None,
                None,
            );
        });
        let parent_focus = ctx.memory(|m| m.focused());
        let mut child_focus = None;
        frame(&ctx, Vec::new(), |_ui| {
            egui::Modal::new(egui::Id::new("settings-child")).show(&ctx, |ui| {
                let button = ui.button("Child action");
                child_focus = Some(button.id);
                button.request_focus();
            });
        });
        assert_ne!(ctx.memory(|m| m.focused()), parent_focus);
        ctx.memory_mut(|m| m.surrender_focus(child_focus.unwrap()));
        frame(&ctx, Vec::new(), |_ui| {
            state.show(
                &ctx,
                &l,
                None,
                &LocalPreferences::default(),
                None,
                None,
                None,
            );
        });
        assert_eq!(ctx.memory(|m| m.focused()), parent_focus);
        frame(&ctx, vec![key(egui::Key::Escape)], |_ui| {
            assert_eq!(
                state.show(
                    &ctx,
                    &l,
                    None,
                    &LocalPreferences::default(),
                    None,
                    None,
                    None
                ),
                vec![SettingsIntent::Done]
            );
        });
        assert_eq!(ctx.memory(|m| m.focused()), invoker);
    }

    #[test]
    fn background_widget_and_scroll_remain_isolated() {
        let ctx = egui::Context::default();
        let l = Localizer::new(Language::En);
        let mut state = SettingsState::default();
        let draw_background = |ui: &mut egui::Ui| {
            let button = ui.button("Scene action");
            let scroll = egui::ScrollArea::vertical()
                .max_height(100.0)
                .show(ui, |ui| {
                    for n in 0..30 {
                        ui.label(format!("Scene row {n}"));
                    }
                });
            (
                button.clicked(),
                button.rect,
                scroll.state.offset.y,
                scroll.inner_rect,
            )
        };
        frame(&ctx, Vec::new(), |ui| {
            draw_background(ui);
            state.show(
                &ctx,
                &l,
                None,
                &LocalPreferences::default(),
                None,
                None,
                None,
            );
        });
        let mut scene = None;
        frame(&ctx, Vec::new(), |ui| {
            let layer = ui.layer_id();
            scene = Some(draw_background(ui));
            state.show(
                &ctx,
                &l,
                None,
                &LocalPreferences::default(),
                None,
                None,
                None,
            );
            assert!(!ctx.memory(|m| m.allows_interaction(layer)));
        });
        let (_, button, offset, scroll) = scene.unwrap();
        let point = button.center();
        frame(
            &ctx,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            |ui| {
                let (clicked, _, _, _) = draw_background(ui);
                assert!(!clicked);
                state.show(
                    &ctx,
                    &l,
                    None,
                    &LocalPreferences::default(),
                    None,
                    None,
                    None,
                );
            },
        );
        frame(
            &ctx,
            vec![
                egui::Event::PointerMoved(scroll.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -80.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            |ui| {
                assert_eq!(draw_background(ui).2, offset);
                state.show(
                    &ctx,
                    &l,
                    None,
                    &LocalPreferences::default(),
                    None,
                    None,
                    None,
                );
            },
        );
    }
}
