//! Native Settings presentation. The host owns every project transaction,
//! preference write, help destination and recovery operation. Rendered controls
//! emit intents rather than changing a portable document in this module.
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

use crate::APPLICATION_NAME;
use crate::cost_estimate::ProjectEstimate;
use crate::domain::Project;
use crate::i18n::{Language, Localizer};
use crate::icons::{Icon, icon};
use crate::kerf_date::confirmation_date_utc;
use crate::local_preferences::{InterfaceScale, LocalPreferences};
use crate::money::{Money, MoneyLocale};
use crate::theme::Typeface;
use crate::theme_widgets as tw;
use crate::units::{Length, Unit};

const FOOTER_HEIGHT: f32 = 58.0;
const LINK: Color32 = Color32::from_rgb(154, 91, 18);

fn sidebar_width(width: f32) -> f32 {
    if width < 580.0 { 150.0 } else { 210.0 }
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

    fn icon(self) -> Icon {
        match self {
            Self::Cutting => Icon::Cut,
            Self::GridUnits => Icon::Grid,
            Self::Costs => Icon::Sheet,
            Self::General => Icon::Globe,
            Self::Shortcuts => Icon::Command,
            Self::About => Icon::List,
        }
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
        let height = 558.0_f32.min((available.y - 2.0).max(160.0));
        let modal = egui::Modal::new(egui::Id::new("settings-modal"))
            .backdrop_color(Color32::from_rgba_unmultiplied(42, 37, 32, 64))
            .frame(
                egui::Frame::new()
                    .fill(tw::PANEL)
                    .stroke(Stroke::new(1.0, tw::BORDER))
                    .corner_radius(12)
                    .inner_margin(0)
                    .shadow(egui::Shadow {
                        offset: [0, 18],
                        blur: 44,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(60, 45, 25, 46),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_min_size(egui::vec2(width, height));
                ui.set_max_size(egui::vec2(width, height));
                ui.spacing_mut().item_spacing.y = 0.0;
                let body_height = (height - FOOTER_HEIGHT).max(80.0);
                let sidebar_width = sidebar_width(width);
                let content_inset = if width < 580.0 { 14.0 } else { 26.0 };
                let (first_focus, sidebar_rect, content_rect) = ui
                    .horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let sidebar = egui::Frame::new()
                            .fill(tw::APP)
                            .corner_radius(CornerRadius {
                                nw: 11,
                                ..Default::default()
                            })
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
                                        ui.vertical(|ui| self.sidebar(ui, language, project, sidebar_width - 20.0))
                                            .inner
                                    })
                                    .inner
                            });
                        let sidebar_rect = sidebar.response.rect;
                        ui.painter().vline(
                            sidebar_rect.right() - 0.5,
                            sidebar_rect.y_range(),
                            Stroke::new(1.0, tw::BORDER_SOFT),
                        );
                        let inner_width = (width - sidebar_width - 2.0 * content_inset).max(105.0);
                        let content = egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(content_inset as i8, 0))
                            .show(ui, |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt(("settings-content", self.section))
                                    .max_width(inner_width)
                                    .max_height(body_height)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.set_width(inner_width);
                                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                                        ui.spacing_mut().item_spacing.y = 4.0;
                                        ui.vertical(|ui| {
                                            ui.add_space(22.0);
                                            if let Some(notice) = notice {
                                                warning(ui, notice);
                                                ui.add_space(8.0);
                                            }
                                            self.content(
                                                ui,
                                                localizer,
                                                project,
                                                preferences,
                                                estimate,
                                                preference_error,
                                                &mut intents,
                                            );
                                            ui.add_space(16.0);
                                        });
                                    });
                            });
                        (sidebar.inner, sidebar_rect, content.response.rect)
                    })
                    .inner;
                let footer = egui::Frame::new()
                    .fill(tw::APP)
                    .corner_radius(CornerRadius {
                        sw: 11,
                        se: 11,
                        ..Default::default()
                    })
                    .inner_margin(egui::Margin::symmetric(18, 12))
                    .show(ui, |ui| {
                        ui.set_width(width - 36.0);
                        ui.set_min_height(FOOTER_HEIGHT - 24.0);
                        ui.spacing_mut().button_padding = egui::vec2(16.0, 4.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let done =
                                done_button(ui, tr(language, "Done", "Concluído"));
                            let hint = match (project.is_some(), self.section) {
                                (true, Section::GridUnits) => tr(
                                    language,
                                    "Grid edits can be undone; display units do not edit the project",
                                    "Alterações da grade podem ser desfeitas; unidades não editam o projeto",
                                ),
                                (true, Section::Cutting | Section::Costs) => tr(
                                    language,
                                    "Project changes apply immediately and can be undone",
                                    "Alterações do projeto são aplicadas imediatamente e podem ser desfeitas",
                                ),
                                _ => tr(
                                    language,
                                    "App settings save automatically",
                                    "Preferências do aplicativo são salvas automaticamente",
                                ),
                            };
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(hint).size(11.5).color(tw::FAINT),
                                    )
                                    .wrap()
                                    .selectable(false),
                                );
                            });
                            done
                        })
                        .inner
                    });
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    footer.response.rect.top() + 0.5,
                    Stroke::new(1.0, tw::BORDER_SOFT),
                );
                (
                    first_focus,
                    footer.inner,
                    sidebar_rect,
                    content_rect,
                    footer.response.rect,
                )
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
        ui.spacing_mut().item_spacing.y = 2.0;
        group_label(ui, tr(language, "PROJECT", "PROJETO"));
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.add(
                egui::Label::new(
                    RichText::new(project.map_or(
                        tr(language, "No project open", "Nenhum projeto aberto"),
                        |p| p.name.as_str(),
                    ))
                    .size(11.5)
                    .color(tw::MUTED),
                )
                .truncate()
                .selectable(false),
            );
        });
        ui.add_space(6.0);
        for section in Section::ALL {
            if section == Section::General {
                ui.add_space(16.0);
                group_label(ui, tr(language, "APP", "APLICATIVO"));
            }
            let project_section = matches!(
                section,
                Section::Cutting | Section::GridUnits | Section::Costs
            );
            let enabled = !project_section || project.is_some();
            let selected = self.section == section;
            let issue = project.is_some_and(|project| match section {
                Section::Cutting => project.confirmed_shop_kerf != Some(project.cutting_kerf),
                Section::Costs => project.cut_fee.is_none(),
                _ => false,
            });
            let response = section_row(
                ui,
                section,
                section.title(language),
                selected,
                issue,
                enabled,
                width,
            );
            if response.clicked() {
                self.section = section;
            }
            if enabled {
                if first_focus.is_none() {
                    first_focus = Some(response.id);
                }
                if selected {
                    selected_focus = Some(response.id);
                }
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
        ui.add(
            egui::Label::new(tw::semibold(ui, section.title(language), 17.0).color(tw::TEXT))
                .selectable(false),
        );
        ui.add_space(-1.0);
        ui.add(
            egui::Label::new(
                RichText::new(match section {
                    Section::Cutting => tr(
                        language,
                        "Used by first-fit, the optimizer, cut sequences and the shop packet.",
                        "Usado no primeiro encaixe, na otimização, na sequência de cortes e no pacote da oficina.",
                    ),
                    Section::GridUnits => tr(
                        language,
                        "Project grid and measurement presentation.",
                        "Grade do projeto e apresentação das medidas.",
                    ),
                    Section::Costs => tr(
                        language,
                        "Estimates only. Taxes, delivery, setup fees and stacking discounts are excluded.",
                        "Apenas estimativas. Impostos, entrega, preparação e descontos por empilhamento não entram.",
                    ),
                    Section::General => tr(
                        language,
                        "Stored on this computer, not in project files.",
                        "Salvo neste computador, não nos arquivos do projeto.",
                    ),
                    Section::Shortcuts => tr(
                        language,
                        "Keyboard and trackpad controls; available offline.",
                        "Controles de teclado e trackpad; disponíveis sem internet.",
                    ),
                    Section::About => tr(
                        language,
                        "Application and source information.",
                        "Informações do aplicativo e código-fonte.",
                    ),
                })
                .size(12.5)
                .color(tw::MUTED),
            )
            .wrap()
            .selectable(false),
        );
        ui.add_space(20.0);
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
                    warning(
                        ui,
                        &format!(
                            "{}: {error}",
                            tr(
                                language,
                                "Preferences could not be saved",
                                "Não foi possível salvar as preferências"
                            )
                        ),
                    );
                    ui.add_space(8.0);
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

fn warning(ui: &mut egui::Ui, text: &str) {
    paragraph(ui, text, 12.0, tw::WARN_INK);
}

/// Primary "Done  Esc" footer button; the accessible name is "Done".
fn done_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let font = tw::weighted_font(ui, 13.0, Typeface::SansMedium);
    let response = ui.add(
        egui::Button::new(RichText::new(text).font(font).color(tw::PANEL))
            .right_text(
                RichText::new("Esc")
                    .font(egui::FontId::monospace(11.0))
                    .color(tw::PANEL.gamma_multiply(0.6)),
            )
            .fill(tw::TEXT)
            .stroke(Stroke::NONE)
            .corner_radius(7)
            .min_size(egui::vec2(0.0, 32.0)),
    );
    let name = text.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
    response
}

/// Full-width segmented control with near-equal, focusable segments.
fn segmented<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut T,
    options: &[(T, &str)],
    width: f32,
    height: f32,
    mono: bool,
) {
    let width = width.min(ui.available_width()).max(40.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter().rect_filled(rect, 7.0, tw::VIEWPORT);
    let font = |ui: &egui::Ui, selected: bool| {
        if mono {
            egui::FontId::monospace(12.0)
        } else if selected {
            tw::weighted_font(ui, 12.5, Typeface::SansMedium)
        } else {
            egui::FontId::proportional(12.5)
        }
    };
    let gap = 2.0;
    let segment_width =
        (width - 4.0 - gap * options.len().saturating_sub(1) as f32) / options.len().max(1) as f32;
    let base = egui::Id::new(("settings-segmented", id));
    for (index, (candidate, label)) in options.iter().enumerate() {
        let segment = egui::Rect::from_min_size(
            egui::pos2(
                rect.left() + 2.0 + index as f32 * (segment_width + gap),
                rect.top() + 2.0,
            ),
            egui::vec2(segment_width, height - 4.0),
        );
        let selected = *value == *candidate;
        let response = ui.interact(segment, base.with(index), egui::Sense::click());
        let name = (*label).to_owned();
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, &name)
        });
        if response.clicked() {
            *value = *candidate;
        }
        let painter = ui.painter();
        if selected {
            painter.rect_filled(
                segment.translate(egui::vec2(0.0, 1.0)),
                5.0,
                Color32::from_rgba_unmultiplied(60, 45, 25, 26),
            );
            painter.rect_filled(segment, 5.0, tw::PANEL);
        }
        if response.has_focus() {
            painter.rect_stroke(
                segment,
                5.0,
                Stroke::new(1.0, tw::FOCUS),
                egui::StrokeKind::Inside,
            );
        }
        let color = if selected { tw::TEXT } else { tw::SECONDARY };
        let galley = painter.layout((*label).to_owned(), font(ui, selected), color, segment_width);
        painter.galley(segment.center() - galley.size() / 2.0, galley, color);
    }
}

/// Uppercase tracked group label ("PROJECT", "APP").
fn group_label(ui: &mut egui::Ui, text: &str) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        8.0,
        egui::TextFormat {
            font_id: tw::weighted_font(ui, 10.5, Typeface::SansSemibold),
            color: tw::FAINT,
            extra_letter_spacing: 10.5 * 0.09,
            ..Default::default()
        },
    );
    ui.add(egui::Label::new(job).selectable(false));
    ui.add_space(4.0);
}

/// One 32-high section row: 15pt icon, label, optional issue dot.
fn section_row(
    ui: &mut egui::Ui,
    section: Section,
    title: &str,
    selected: bool,
    issue: bool,
    enabled: bool,
    width: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, 32.0),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, title)
    });
    let painter = ui.painter();
    let fill = if selected {
        tw::ACCENT_BG
    } else if enabled && (response.hovered() || response.has_focus()) {
        tw::VIEWPORT
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, 7.0, fill);
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            7.0,
            Stroke::new(1.0, tw::FOCUS),
            egui::StrokeKind::Inside,
        );
    }
    let alpha = if enabled { 1.0 } else { 0.45 };
    let icon_color = if selected { tw::ACCENT_DARK } else { tw::MUTED };
    icon(section.icon(), icon_color.gamma_multiply(alpha), 15.0).paint_at(
        ui,
        egui::Rect::from_center_size(
            egui::pos2(rect.left() + 17.5, rect.center().y),
            egui::Vec2::splat(15.0),
        ),
    );
    let (font, color) = if selected {
        (
            tw::weighted_font(ui, 13.0, Typeface::SansMedium),
            tw::ACCENT_INK,
        )
    } else {
        (egui::FontId::proportional(13.0), tw::TEXT_2)
    };
    let text_left = rect.left() + 34.0;
    let text_right = rect.right() - if issue { 22.0 } else { 8.0 };
    let galley = ui.painter().layout(
        title.to_owned(),
        font,
        color.gamma_multiply(alpha),
        (text_right - text_left).max(10.0),
    );
    ui.painter().galley(
        egui::pos2(text_left, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    if issue {
        ui.painter().circle_filled(
            egui::pos2(rect.right() - 14.0, rect.center().y),
            3.5,
            tw::ACCENT,
        );
    }
    response
}

/// 1px `#EDE8E0` rule between groups.
fn rule(ui: &mut egui::Ui) {
    ui.add_space(10.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, Stroke::new(1.0, tw::RULE));
    ui.add_space(10.0);
}

/// Two-column settings row: label column 150 (13 medium + 11.5 faint hint),
/// then the controls. `inset` aligns the label with a 34-high control.
fn row(
    ui: &mut egui::Ui,
    title: &str,
    hint: &str,
    inset: f32,
    content: impl FnOnce(&mut egui::Ui),
) {
    let label = |ui: &mut egui::Ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.add(
            egui::Label::new(tw::medium(ui, title, 13.0).color(tw::TEXT))
                .wrap()
                .selectable(false),
        );
        if !hint.is_empty() {
            ui.add(
                egui::Label::new(RichText::new(hint).size(11.5).color(tw::FAINT))
                    .wrap()
                    .selectable(false),
            );
        }
    };
    // Below 390 points, stack the label above the control instead of clipping.
    if ui.available_width() < 390.0 {
        ui.vertical(label);
        ui.add_space(6.0);
        ui.vertical(content);
    } else {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            ui.vertical(|ui| {
                ui.set_width(150.0);
                ui.add_space(inset);
                label(ui);
            });
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                content(ui);
            });
        });
    }
}

fn paragraph(ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
    ui.add(
        egui::Label::new(RichText::new(text).size(size).color(color))
            .wrap()
            .selectable(false),
    );
}

fn link(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    let response = ui.add(
        egui::Label::new(RichText::new(text).size(12.0).color(color))
            .sense(egui::Sense::click())
            .selectable(false),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() || response.has_focus() {
        let rect = response.rect;
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 1.0,
            Stroke::new(1.0, color),
        );
    }
    let name = text.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, &name));
    response
}

/// Read-only value that opens the host's validated editor (kerf, grid, fee):
/// looks like a 34-high input with a unit suffix.
fn value_button(
    ui: &mut egui::Ui,
    accessible_name: &str,
    value: &str,
    placeholder: bool,
    suffix: &str,
    width: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::click());
    let response = response
        .on_hover_text(accessible_name)
        .on_hover_cursor(egui::CursorIcon::Text);
    let active = response.hovered() || response.has_focus();
    let painter = ui.painter();
    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(1.5),
            9.0,
            Stroke::new(3.0, tw::ACCENT_BG),
            egui::StrokeKind::Outside,
        );
    }
    painter.rect(
        rect,
        7.0,
        if active { tw::CARD } else { tw::APP },
        Stroke::new(1.0, if active { tw::FOCUS } else { tw::BORDER_SOFT }),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        value,
        egui::FontId::monospace(13.0),
        if placeholder { tw::DISABLED } else { tw::TEXT },
    );
    painter.text(
        rect.right_center() - egui::vec2(10.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        suffix,
        egui::FontId::proportional(12.0),
        tw::FAINT,
    );
    let name = accessible_name.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
    response
}

/// Small `bg_viewport` chip button ("Free (0)").
fn chip_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(12.0).color(tw::TEXT))
            .fill(tw::VIEWPORT)
            .stroke(Stroke::NONE)
            .corner_radius(6)
            .min_size(egui::vec2(0.0, 24.0)),
    )
}

/// 32×18 pill switch with a label; on = `text` fill, off = `border`.
fn toggle(ui: &mut egui::Ui, value: bool, label: &str) -> egui::Response {
    let galley = ui.painter().layout(
        label.to_owned(),
        egui::FontId::proportional(13.0),
        tw::TEXT,
        (ui.available_width() - 42.0).max(40.0),
    );
    let height = galley.size().y.max(20.0);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(42.0 + galley.size().x, height),
        egui::Sense::click(),
    );
    let pill = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.top() + (height.min(22.0) - 18.0) / 2.0),
        egui::vec2(32.0, 18.0),
    );
    let painter = ui.painter();
    painter.rect_filled(pill, 9.0, if value { tw::TEXT } else { tw::BORDER });
    let knob_x = if value {
        pill.right() - 9.0
    } else {
        pill.left() + 9.0
    };
    painter.circle_filled(egui::pos2(knob_x, pill.center().y), 7.0, tw::PANEL);
    if response.has_focus() {
        painter.rect_stroke(
            pill.expand(2.0),
            11.0,
            Stroke::new(1.0, tw::FOCUS),
            egui::StrokeKind::Outside,
        );
    }
    painter.galley(egui::pos2(rect.left() + 42.0, rect.top()), galley, tw::TEXT);
    let name = label.to_owned();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, value, &name)
    });
    response
}

/// Painted checkbox used by the kerf confirmation card.
fn check_box(ui: &mut egui::Ui, checked: bool, enabled: bool, label: &str) -> egui::Response {
    let width = ui.available_width();
    let galley = ui.painter().layout(
        label.to_owned(),
        tw::weighted_font(ui, 13.0, Typeface::SansMedium),
        tw::TEXT,
        (width - 25.0).max(40.0),
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, galley.size().y.max(18.0)),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let boxed = egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 2.0), egui::vec2(16.0, 16.0));
    let painter = ui.painter();
    if checked {
        painter.rect_filled(boxed, 4.0, tw::TEXT);
        icon(Icon::Check, tw::PANEL, 11.0).paint_at(
            ui,
            egui::Rect::from_center_size(boxed.center(), egui::Vec2::splat(11.0)),
        );
    } else {
        painter.rect(
            boxed,
            4.0,
            tw::CARD,
            Stroke::new(
                1.0,
                if response.hovered() || response.has_focus() {
                    tw::FOCUS
                } else {
                    tw::BORDER_STRONG
                },
            ),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter()
        .galley(egui::pos2(rect.left() + 25.0, rect.top()), galley, tw::TEXT);
    let name = label.to_owned();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, checked, &name)
    });
    response
}

/// `5`, `0.5`, `12,7` — a millimetre value without needless zeros.
fn trimmed_mm(length: Length, language: Language) -> String {
    let micrometres = length.micrometres();
    let mut text = format!("{:.3}", micrometres as f64 / 1000.0);
    while text.contains('.') && (text.ends_with('0') || text.ends_with('.')) {
        text.pop();
    }
    if language == Language::PtBr {
        text = text.replace('.', ",");
    }
    text
}

fn cutting(ui: &mut egui::Ui, l: &Localizer, project: &Project, out: &mut Vec<SettingsIntent>) {
    let language = l.language();
    let confirmed = project.confirmed_shop_kerf == Some(project.cutting_kerf);
    row(
        ui,
        tr(language, "Blade kerf", "Espessura do corte"),
        tr(
            language,
            "Width of one saw pass",
            "Largura de uma passada da serra",
        ),
        7.0,
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if value_button(
                    ui,
                    &l.text("cutting-kerf-edit"),
                    &trimmed_mm(project.cutting_kerf, language),
                    false,
                    "mm",
                    140.0,
                )
                .clicked()
                {
                    out.push(SettingsIntent::EditKerf);
                }
                if confirmed {
                    let date = confirmation_date_utc(project.confirmed_shop_kerf_unix_ms);
                    let text = date.as_ref().map_or_else(
                        || l.text("settings-kerf-confirmed-undated"),
                        |date| {
                            let mut args = fluent_bundle::FluentArgs::new();
                            args.set("date", date.trim_end_matches(" UTC").to_owned());
                            l.format("settings-kerf-confirmed", Some(&args))
                        },
                    );
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.add(icon(Icon::Check, tw::OK, 14.0));
                    let label = ui.add(
                        egui::Label::new(tw::medium(ui, text, 12.0).color(tw::OK))
                            .wrap()
                            .selectable(false),
                    );
                    if let Some(date) = date {
                        label.on_hover_text(date);
                    }
                } else {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.add(icon(Icon::Warning, tw::WARN, 14.0));
                    ui.add(
                        egui::Label::new(
                            tw::medium(ui, l.text("settings-kerf-unconfirmed"), 12.0)
                                .color(tw::WARN_INK),
                        )
                        .wrap()
                        .selectable(false),
                    );
                }
            });
            egui::Frame::new()
                .fill(tw::CARD)
                .stroke(Stroke::new(1.0, tw::BORDER_SOFT))
                .corner_radius(9)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 8.0;
                    if check_box(
                        ui,
                        confirmed,
                        !confirmed,
                        tr(
                            language,
                            "I confirmed this kerf and cutting practice with the shop",
                            "Confirmei esta espessura de corte e a prática de trabalho com a oficina",
                        ),
                    )
                    .clicked()
                    {
                        out.push(SettingsIntent::ConfirmKerf);
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(25.0);
                        paragraph(
                            ui,
                            tr(
                                language,
                                "Required for Shop-ready packets. Changing the kerf clears this and re-checks every placement.",
                                "Obrigatório para o pacote pronto para a oficina. Alterar a espessura remove a confirmação e reavalia as alocações.",
                            ),
                            12.0,
                            tw::MUTED,
                        );
                    });
                });
        },
    );
    rule(ui);
    row(
        ui,
        tr(language, "Cutting model", "Modelo de corte"),
        tr(language, "What plans assume", "Premissas do plano"),
        0.0,
        |ui| {
            ui.spacing_mut().item_spacing.y = 7.0;
            for line in [
                tr(
                    language,
                    "Straight, full-span cuts parallel to an edge",
                    "Cortes retos de ponta a ponta, paralelos a uma borda",
                ),
                tr(
                    language,
                    "One physical pass per split, trims included",
                    "Uma passada física por divisão, refilos incluídos",
                ),
                tr(
                    language,
                    "No stacking, stopped, angled or interior cuts",
                    "Sem empilhamento nem cortes interrompidos, angulados ou internos",
                ),
            ] {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(RichText::new("•").size(12.5).color(tw::FAINT));
                    paragraph(ui, line, 12.5, tw::TEXT_2);
                });
            }
            if link(
                ui,
                tr(language, "Worked examples →", "Exemplos práticos →"),
                LINK,
            )
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
        tr(language, "Grid spacing", "Espaçamento da grade"),
        tr(language, "XY snapping step", "Passo de encaixe XY"),
        7.0,
        |ui| {
            if value_button(
                ui,
                &l.text("grid-edit"),
                &trimmed_mm(project.grid_spacing, language),
                false,
                "mm",
                140.0,
            )
            .clicked()
            {
                out.push(SettingsIntent::EditGrid);
            }
            paragraph(
                ui,
                tr(
                    language,
                    "An undoable project edit; existing positions are not snapped again.",
                    "Edição do projeto que pode ser desfeita; posições existentes não são reajustadas.",
                ),
                12.0,
                tw::MUTED,
            );
        },
    );
    rule(ui);
    row(
        ui,
        tr(language, "Display units", "Unidades de exibição"),
        tr(language, "Measurements only", "Somente medidas"),
        4.0,
        |ui| {
            let mut unit = project.display_unit;
            segmented(
                ui,
                "settings-display-unit",
                &mut unit,
                &[
                    (Unit::Mm, "mm"),
                    (Unit::Cm, "cm"),
                    (Unit::M, "m"),
                    (Unit::Inch, "in"),
                    (Unit::Foot, "ft"),
                ],
                280.0,
                30.0,
                true,
            );
            if unit != project.display_unit {
                out.push(SettingsIntent::SetDisplayUnit(unit));
            }
            paragraph(
                ui,
                tr(
                    language,
                    "Presentation only: nothing is converted or added to undo. PDF units are chosen at export.",
                    "Somente apresentação: nada é convertido nem entra no histórico. As unidades do PDF são escolhidas na exportação.",
                ),
                12.0,
                tw::MUTED,
            );
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
    let code = project.currency.code();
    row(
        ui,
        tr(language, "Fee per cut", "Custo por corte"),
        tr(language, "Charged per saw pass", "Cobrado por passada"),
        7.0,
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (value, placeholder) = project.cut_fee.map_or_else(
                    || (tr(language, "unknown", "desconhecido").to_owned(), true),
                    |fee| {
                        let text = fee.display(money_locale);
                        (
                            text.trim_start_matches(code).trim().to_owned(),
                            false,
                        )
                    },
                );
                if value_button(
                    ui,
                    &l.text("settings-edit-cut-fee"),
                    &value,
                    placeholder,
                    code,
                    160.0,
                )
                .clicked()
                {
                    out.push(SettingsIntent::EditCutFee);
                }
                if project.cut_fee.is_none_or(|fee| fee.minor_units() != 0)
                    && chip_button(ui, &l.text("cut-fee-free")).clicked()
                {
                    out.push(SettingsIntent::SetFreeCutFee);
                }
            });
            paragraph(
                ui,
                tr(
                    language,
                    "Leave blank if you don't know it yet: the estimate shows as incomplete, never as zero. Trims count as cuts.",
                    "Deixe em branco se ainda não souber: a estimativa aparece como incompleta, nunca como zero. Refilos contam como cortes.",
                ),
                12.0,
                tw::MUTED,
            );
        },
    );
    rule(ui);
    row(
        ui,
        tr(language, "Currency", "Moeda"),
        tr(language, "One per project", "Uma por projeto"),
        7.0,
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(160.0, 34.0), egui::Sense::hover());
                ui.painter().rect(
                    rect,
                    7.0,
                    tw::APP,
                    Stroke::new(1.0, tw::BORDER_SOFT),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    rect.left_center() + egui::vec2(10.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    code,
                    egui::FontId::monospace(13.0),
                    tw::TEXT,
                );
                if tw::secondary_button(ui, tr(language, "Change…", "Alterar…")).clicked() {
                    out.push(SettingsIntent::ChangeCurrency);
                }
            });
            paragraph(ui, &l.text("currency-no-conversion"), 12.0, tw::MUTED);
        },
    );
    rule(ui);
    row(
        ui,
        tr(language, "Current estimate", "Estimativa atual"),
        "",
        12.0,
        |ui| {
            let incomplete = estimate.is_none_or(|estimate| estimate.total.is_none());
            let (fill, stroke, ink) = if incomplete {
                (tw::WARN_BG, tw::WARN_STROKE, Color32::from_rgb(92, 62, 16))
            } else {
                (tw::CARD, tw::BORDER_SOFT, tw::TEXT_2)
            };
            egui::Frame::new()
                .fill(fill)
                .stroke(Stroke::new(1.0, stroke))
                .inner_margin(12)
                .corner_radius(9)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let Some(estimate) = estimate else {
                        paragraph(
                            ui,
                            tr(
                                language,
                                "Estimate unavailable until calculated",
                                "Estimativa indisponível até o cálculo",
                            ),
                            12.5,
                            ink,
                        );
                        return;
                    };
                    let unknown = tr(language, "unknown", "desconhecido");
                    let money = |value: Option<Money>| {
                        value.map_or_else(|| unknown.to_owned(), |m| m.display(money_locale))
                    };
                    let cuts = estimate
                        .used_stock
                        .iter()
                        .try_fold(0_u64, |sum, stock| sum.checked_add(stock.cuts?));
                    let cuts_label = cuts.map_or_else(
                        || tr(language, "Cuts × fee", "Cortes × custo").to_owned(),
                        |n| {
                            format!(
                                "{n} {}",
                                tr(language, "cuts × fee", "cortes × custo")
                            )
                        },
                    );
                    let lines = [
                        (
                            tr(language, "Material to purchase", "Material a comprar").to_owned(),
                            money(estimate.material),
                            estimate.material.is_none(),
                            false,
                        ),
                        (cuts_label, money(estimate.cutting), estimate.cutting.is_none(), false),
                        (
                            tr(language, "New spending", "Novo gasto").to_owned(),
                            estimate.total.map_or_else(
                                || tr(language, "incomplete", "incompleto").to_owned(),
                                |m| m.display(money_locale),
                            ),
                            false,
                            true,
                        ),
                    ];
                    ui.spacing_mut().item_spacing.y = 5.0;
                    for (label, value, missing, strong) in lines {
                        ui.horizontal(|ui| {
                            let text = if strong {
                                tw::semibold(ui, label, 12.5)
                            } else {
                                RichText::new(label).size(12.5)
                            };
                            ui.add(egui::Label::new(text.color(ink)).selectable(false));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let text = if missing {
                                        RichText::new(value).size(12.5).color(tw::WARN_INK)
                                    } else if strong {
                                        RichText::new(value)
                                            .font(tw::weighted_font(
                                                ui,
                                                12.5,
                                                Typeface::MonoSemibold,
                                            ))
                                            .color(tw::TEXT)
                                    } else {
                                        tw::mono(value, 12.5).color(tw::TEXT)
                                    };
                                    ui.add(egui::Label::new(text).selectable(false));
                                },
                            );
                        });
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
    row(ui, l.text("ui-language").as_str(), "", 5.0, |ui| {
        let mut chosen = prefs.language;
        segmented(
            ui,
            "settings-language",
            &mut chosen,
            &[
                (Language::En, "English"),
                (Language::PtBr, "Português (BR)"),
            ],
            280.0,
            30.0,
            false,
        );
        if chosen != prefs.language {
            out.push(SettingsIntent::SetLanguage(chosen));
        }
        paragraph(
            ui,
            tr(
                language,
                "Never converts prices or units. PDF language is chosen at export.",
                "Nunca converte preços nem unidades. O idioma do PDF é escolhido na exportação.",
            ),
            12.0,
            tw::MUTED,
        );
    });
    rule(ui);
    row(
        ui,
        tr(language, "Recovery", "Recuperação"),
        tr(language, "Autosave", "Cópia automática"),
        0.0,
        |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            paragraph(
                ui,
                tr(
                    language,
                    "A recovery snapshot is written 30 s after your last edit. It never replaces your saved file.",
                    "Uma cópia de recuperação é gravada 30 s após a última edição. Ela nunca substitui o arquivo salvo.",
                ),
                12.5,
                tw::TEXT_2,
            );
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if link(
                    ui,
                    tr(
                        language,
                        "Show recovery folder",
                        "Mostrar pasta de recuperação",
                    ),
                    LINK,
                )
                .clicked()
                {
                    out.push(SettingsIntent::ShowRecoveryFolder);
                }
                if link(
                    ui,
                    tr(language, "Clear old snapshots…", "Limpar cópias antigas…"),
                    tw::FAINT,
                )
                .clicked()
                {
                    out.push(SettingsIntent::ReviewRecoveryCleanup);
                }
            });
        },
    );
    rule(ui);
    row(ui, tr(language, "Viewport", "Área 3D"), "", 0.0, |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        for (value, label, intent) in [
            (
                prefs.navigation_hints,
                tr(
                    language,
                    "Show navigation hints in the status bar",
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
            if toggle(ui, value, label).clicked() {
                out.push(intent);
            }
        }
    });
    rule(ui);
    row(
        ui,
        tr(language, "Interface scale", "Escala da interface"),
        "",
        5.0,
        |ui| {
            let mut scale = prefs.interface_scale;
            let labels = [
                InterfaceScale::Percent90,
                InterfaceScale::Percent100,
                InterfaceScale::Percent115,
                InterfaceScale::Percent130,
            ]
            .map(|scale| (scale, format!("{}%", scale.percent())));
            let options: Vec<(InterfaceScale, &str)> = labels
                .iter()
                .map(|(scale, label)| (*scale, label.as_str()))
                .collect();
            segmented(ui, "settings-scale", &mut scale, &options, 280.0, 30.0, true);
            if scale != prefs.interface_scale {
                out.push(SettingsIntent::SetScale(scale));
            }
        },
    );
}

fn shortcuts(ui: &mut egui::Ui, language: Language, out: &mut Vec<SettingsIntent>) {
    let command = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    };
    let rows = [
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
            format!("{command}Z / {command}⇧Z"),
            "Undo / Redo",
            "Desfazer / Refazer",
        ),
        (
            "← → ↑ ↓ / ⇧".into(),
            "Orbit / pan the focused viewport",
            "Orbitar / deslocar a vista em foco",
        ),
        (
            "+ / − / F".into(),
            "Zoom / frame selection in the focused viewport",
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
    ];
    let count = rows.len();
    for (index, (keys, en, pt)) in rows.into_iter().enumerate() {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            ui.vertical(|ui| {
                ui.set_width(150.0);
                tw::keycap(ui, &keys);
            });
            paragraph(ui, tr(language, en, pt), 12.5, tw::TEXT_2);
        });
        if index + 1 < count {
            ui.add_space(4.0);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
            ui.painter()
                .hline(rect.x_range(), rect.center().y, Stroke::new(1.0, tw::RULE));
            ui.add_space(4.0);
        }
    }
    ui.add_space(10.0);
    if link(
        ui,
        tr(
            language,
            "Open shortcut help →",
            "Abrir ajuda de atalhos →",
        ),
        LINK,
    )
    .clicked()
    {
        out.push(SettingsIntent::HelpShortcuts);
    }
}

fn about(ui: &mut egui::Ui, language: Language, out: &mut Vec<SettingsIntent>) {
    row(ui, APPLICATION_NAME, "", 0.0, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.add(
            egui::Label::new(
                tw::mono(
                    format!(
                        "{} {}",
                        tr(language, "Version", "Versão"),
                        env!("CARGO_PKG_VERSION")
                    ),
                    12.5,
                )
                .color(tw::TEXT),
            )
            .selectable(false),
        );
        paragraph(
            ui,
            &format!(
                "{}: {} / {}",
                tr(language, "Platform", "Plataforma"),
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
            12.5,
            tw::TEXT_2,
        );
        paragraph(
            ui,
            tr(
                language,
                "Native desktop application; project files remain portable.",
                "Aplicativo nativo; os arquivos de projeto permanecem portáteis.",
            ),
            12.5,
            tw::TEXT_2,
        );
    });
    rule(ui);
    row(ui, tr(language, "Source", "Código-fonte"), "", 0.0, |ui| {
        ui.add(egui::Label::new(tw::mono(SOURCE_URL, 12.0).color(tw::TEXT_2)).selectable(true));
        if link(
            ui,
            tr(
                language,
                "Open source page →",
                "Abrir página do código-fonte →",
            ),
            LINK,
        )
        .clicked()
        {
            out.push(SettingsIntent::Source);
        }
    });
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
