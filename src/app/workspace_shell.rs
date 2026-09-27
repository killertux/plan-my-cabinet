//! Shared, project-session-only chrome for the five native workspaces.
use eframe::egui::{self, Color32};
use plan_my_cabinet::allocation_diagnostics::{BoardDiagnostic, Status};
use plan_my_cabinet::cost_estimate::ProjectEstimate;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::i18n::{Language, Localizer};

#[cfg(test)]
use crate::actions::ActionId as A;
use crate::icons::{self, Icon};
use crate::theme_widgets as colors;
use crate::workspace_state::Workspace;

pub(crate) const ENTRIES: [(Workspace, &str, Icon); 5] = [
    (Workspace::Design, "navigation-design", Icon::Cube),
    (Workspace::Stock, "navigation-stock", Icon::Material),
    (Workspace::CutPlan, "navigation-cut-plan", Icon::Sheet),
    (Workspace::Hardware, "navigation-hardware", Icon::Hinge),
    (Workspace::Handoff, "navigation-handoff", Icon::Export),
];

pub(crate) const RAIL_WIDTH: f32 = 60.0;
pub(crate) const HEADER_HEIGHT: f32 = 46.0;
pub(crate) const STATUS_HEIGHT: f32 = 26.0;
#[cfg(test)]
pub(crate) const OVERFLOW_ACTIONS: [A; 4] = [A::Undo, A::Redo, A::SaveProject, A::OpenHandoff];

pub(crate) fn more_label(language: Language) -> &'static str {
    match language {
        Language::En => "More",
        Language::PtBr => "Mais",
    }
}

pub(crate) fn inspector_label(language: Language) -> &'static str {
    match language {
        Language::En => "Inspector",
        Language::PtBr => "Inspetor",
    }
}
const PANE_GAP: f32 = 12.0;
const MIN_CANVAS: f32 = 420.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drawer {
    Controls,
    Inspector,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaneLayout {
    pub controls: f32,
    pub inspector: f32,
    pub canvas: f32,
}

impl PaneLayout {
    pub fn preferred(workspace: Workspace) -> Self {
        Self::for_width(workspace, 1440.0 - RAIL_WIDTH)
    }

    /// `width` is the remaining content width after the rail. Collapse the
    /// inspector first, then controls, rather than compressing either pane's
    /// inputs or the interactive canvas below its minimum.
    pub fn for_width(workspace: Workspace, width: f32) -> Self {
        let (controls, inspector) = match workspace {
            Workspace::Design => (256.0, 292.0),
            Workspace::Stock => (256.0, 316.0),
            Workspace::CutPlan => (256.0, 308.0),
            Workspace::Hardware => (268.0, 316.0),
            Workspace::Handoff => (300.0, 300.0),
        };
        let mut layout = Self {
            controls,
            inspector,
            canvas: 0.0,
        };
        if width < controls + inspector + MIN_CANVAS + PANE_GAP * 2.0 {
            layout.inspector = 0.0;
        }
        if width < controls + MIN_CANVAS + PANE_GAP {
            layout.controls = 0.0;
        }
        layout.canvas = (width
            - layout.controls
            - layout.inspector
            - PANE_GAP
                * (usize::from(layout.controls > 0.0) + usize::from(layout.inspector > 0.0))
                    as f32)
            .max(1.0);
        layout
    }

    pub fn collapsed(self, drawer: Drawer) -> bool {
        match drawer {
            Drawer::Controls => self.controls == 0.0,
            Drawer::Inspector => self.inspector == 0.0,
        }
    }
}

/// One row per board, regardless of visibility or number of underlying faults.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct IssueCounts {
    pub unallocated: usize,
    pub conflicted: usize,
    pub unknown_proof: usize,
    pub hidden: usize,
}

impl IssueCounts {
    pub fn from_diagnostics(
        diagnostics: &[BoardDiagnostic],
        is_hidden: impl Fn(uuid::Uuid) -> bool,
    ) -> Self {
        let mut counts = Self::default();
        for diagnostic in diagnostics {
            match diagnostic.status {
                Status::AllocatedValid => continue,
                Status::Unallocated => counts.unallocated += 1,
                Status::Conflicted => counts.conflicted += 1,
                Status::UnknownSearchBudget => counts.unknown_proof += 1,
            }
            counts.hidden += usize::from(is_hidden(diagnostic.board_id));
        }
        counts
    }

    pub fn total(self) -> usize {
        self.unallocated + self.conflicted + self.unknown_proof
    }
}

/// A numeric total is only presented when *all* board diagnostics and the
/// authoritative cost ledger agree on completeness. Unknown is never zero.
pub(crate) fn complete_spending<'a>(
    project: &Project,
    counts: IssueCounts,
    estimate: Option<&'a ProjectEstimate>,
) -> Option<&'a plan_my_cabinet::money::Money> {
    estimate
        .filter(|_| counts.total() == 0 && project.boards.len() == project.allocations.len())
        .and_then(|estimate| estimate.total.as_ref())
}

pub(crate) fn shortcut(ctx: &egui::Context, blocked: bool) -> Option<Workspace> {
    if blocked || ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx) {
        return None;
    }
    ctx.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command && !modifiers.alt && !modifiers.shift => [
                egui::Key::Num1,
                egui::Key::Num2,
                egui::Key::Num3,
                egui::Key::Num4,
                egui::Key::Num5,
            ]
            .iter()
            .position(|candidate| candidate == key)
            .map(|index| ENTRIES[index].0),
            _ => None,
        })
    })
}

/// What the user asked for from the rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RailAction {
    Workspace(Workspace),
    Language(Language),
    Settings,
}

pub(crate) fn rail(
    ui: &mut egui::Ui,
    active: Workspace,
    localizer: &Localizer,
    enabled: bool,
    issues: IssueCounts,
) -> Option<RailAction> {
    let mut next = None;
    egui::Panel::left("workspace-rail")
        .exact_size(RAIL_WIDTH)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(colors::APP)
                .stroke(egui::Stroke::new(1.0, colors::BORDER))
                .inner_margin(egui::Margin::symmetric(6, 10)),
        )
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                let (logo, _) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::hover());
                ui.painter().rect_filled(logo, 8.0, colors::TEXT);
                icons::icon(Icon::Board, Color32::from_rgb(244, 194, 122), 18.0).paint_at(
                    ui,
                    egui::Rect::from_center_size(logo.center(), egui::Vec2::splat(18.0)),
                );
                ui.add_space(12.0);
                for (workspace, key, icon) in ENTRIES {
                    let label = localizer.text(key);
                    let selected = active == workspace;
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(48.0, 50.0),
                        if enabled {
                            egui::Sense::click()
                        } else {
                            egui::Sense::hover()
                        },
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            enabled,
                            selected,
                            &label,
                        )
                    });
                    let fill = if selected {
                        colors::ACCENT_BG
                    } else if enabled && response.hovered() {
                        colors::VIEWPORT
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, 8.0, fill);
                    let ink = if selected {
                        colors::ACCENT_DARK
                    } else {
                        colors::MUTED
                    };
                    let icon_center = rect.center_top() + egui::vec2(0.0, 17.0);
                    icons::icon(icon, ink, 19.0).paint_at(
                        ui,
                        egui::Rect::from_center_size(icon_center, egui::Vec2::splat(19.0)),
                    );
                    ui.painter().text(
                        rect.center_bottom() - egui::vec2(0.0, 9.0),
                        egui::Align2::CENTER_CENTER,
                        &label,
                        if selected {
                            colors::weighted_font(ui, 9.5, crate::theme::Typeface::SansMedium)
                        } else {
                            egui::FontId::proportional(9.5)
                        },
                        ink,
                    );
                    if workspace == Workspace::CutPlan && issues.total() > 0 {
                        ui.painter().circle_filled(
                            icon_center + egui::vec2(11.0, -8.0),
                            3.5,
                            colors::ACCENT,
                        );
                    }
                    let shortcut = format!(
                        "{} · {}{}",
                        label,
                        if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" },
                        workspace.number()
                    );
                    let response = if workspace == Workspace::CutPlan && issues.total() > 0 {
                        response.on_hover_text(format!(
                            "{shortcut}\n{}: {}",
                            localizer.text("global-issues"),
                            issues.total()
                        ))
                    } else {
                        response.on_hover_text(shortcut)
                    };
                    if response.clicked() {
                        next = Some(RailAction::Workspace(workspace));
                    }
                }
            });
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                if colors::ghost_icon_sized(
                    ui,
                    Icon::Sliders,
                    &localizer.text("settings-open"),
                    colors::MUTED,
                    18.0,
                    40.0,
                    enabled,
                    false,
                )
                .clicked()
                {
                    next = Some(RailAction::Settings);
                }
                let globe = colors::ghost_icon_sized(
                    ui,
                    Icon::Globe,
                    &localizer.text("ui-language"),
                    colors::MUTED,
                    18.0,
                    40.0,
                    enabled,
                    false,
                );
                egui::Popup::menu(&globe)
                    .align(egui::RectAlign::RIGHT_END)
                    .show(|ui| {
                        for (language, key) in [
                            (Language::En, "language-en"),
                            (Language::PtBr, "language-pt-br"),
                        ] {
                            if ui
                                .selectable_label(
                                    localizer.language() == language,
                                    localizer.text(key),
                                )
                                .clicked()
                            {
                                next = Some(RailAction::Language(language));
                                ui.close();
                            }
                        }
                    });
            });
        });
    next
}

#[cfg(test)]
mod tests;
