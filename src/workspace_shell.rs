//! Shared, project-session-only chrome for the five native workspaces.
use eframe::egui::{self, Color32};
use plan_my_cabinet::allocation_diagnostics::{BoardDiagnostic, Status};
use plan_my_cabinet::cost_estimate::ProjectEstimate;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::i18n::{Language, Localizer};

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
mod tests {
    use super::*;
    use plan_my_cabinet::allocation_diagnostics::diagnose;
    use plan_my_cabinet::cost_estimate::estimate;
    use plan_my_cabinet::reference_fixture::{BACK_ID, LEFT_SIDE_ID, RIGHT_SIDE_ID};

    #[test]
    fn global_issues_include_hidden_unallocated_and_conflicting_placements() {
        let mut project = plan_my_cabinet::reference_fixture::project();
        let overlapping = project
            .allocations
            .iter_mut()
            .find(|a| a.board_id == RIGHT_SIDE_ID)
            .unwrap();
        overlapping.origin = [plan_my_cabinet::units::Length::ZERO; 2];
        let diagnostics = diagnose(&project);
        let counts =
            IssueCounts::from_diagnostics(&diagnostics, |id| id == BACK_ID || id == LEFT_SIDE_ID);
        assert_eq!(counts.unallocated, 1);
        assert!(counts.conflicted >= 2);
        assert_eq!(counts.hidden, 2);
        assert_eq!(
            complete_spending(&project, counts, estimate(&project).ok().as_ref()),
            None
        );
    }

    #[test]
    fn unknown_price_or_fee_cannot_be_shown_as_a_total() {
        let project = Project::new("Empty", plan_my_cabinet::money::Currency::Brl);
        let counts = IssueCounts::from_diagnostics(&diagnose(&project), |_| false);
        assert_eq!(
            complete_spending(&project, counts, estimate(&project).ok().as_ref())
                .map(|v| v.minor_units()),
            Some(0)
        );
        // A used sheet with a missing fee or price is incomplete even if all
        // allocations are otherwise valid; unknown is distinct from free.
        let mut project = plan_my_cabinet::reference_fixture::project();
        project.boards.retain(|board| board.id != BACK_ID);
        let counts = IssueCounts::from_diagnostics(&diagnose(&project), |_| false);
        assert_eq!(counts.total(), 0);
        project.cut_fee = None;
        assert!(complete_spending(&project, counts, estimate(&project).ok().as_ref()).is_none());
        project.cut_fee = Some(plan_my_cabinet::money::Money::new(project.currency, 0).unwrap());
        assert!(complete_spending(&project, counts, estimate(&project).ok().as_ref()).is_some());
        project
            .stock
            .iter_mut()
            .find(|s| s.id == project.allocations[0].stock_id)
            .unwrap()
            .price = None;
        assert!(complete_spending(&project, counts, estimate(&project).ok().as_ref()).is_none());
    }

    #[test]
    fn shell_labels_exist_in_both_languages() {
        for language in [
            plan_my_cabinet::i18n::Language::En,
            plan_my_cabinet::i18n::Language::PtBr,
        ] {
            let locale = Localizer::new(language);
            assert!(!more_label(language).is_empty());
            assert!(!inspector_label(language).is_empty());
            for action in OVERFLOW_ACTIONS {
                let label = action.label(&locale);
                assert!(!label.trim().is_empty(), "{language:?} {action:?}");
            }
            for key in [
                "shell-projects",
                "shell-export",
                "shell-unsaved",
                "shell-proof-unknown",
                "shell-search-active",
                "shell-export-active",
                "cost-incomplete",
                "global-conflicted",
                "global-hidden",
            ] {
                let label = locale.text(key);
                assert!(
                    !label.trim().is_empty() && label != key,
                    "{language:?}: {key}"
                );
            }
        }
    }

    #[test]
    fn logical_widths_collapse_at_minimum_canvas_without_shrinking_panes() {
        let baseline = PaneLayout::for_width(Workspace::Design, 1440.0 - RAIL_WIDTH);
        assert_eq!((baseline.controls, baseline.inspector), (256.0, 292.0));
        for window in [(1440.0, 900.0), (1100.0, 700.0), (900.0, 650.0)] {
            for scale in [0.90, 1.0, 1.15, 1.30] {
                let width = window.0 / scale - RAIL_WIDTH;
                for (workspace, _, _) in ENTRIES {
                    let layout = PaneLayout::for_width(workspace, width);
                    assert!(
                        layout.canvas >= MIN_CANVAS,
                        "{workspace:?} {window:?} {scale}"
                    );
                    assert!(layout.controls == 0.0 || layout.controls >= 256.0);
                    assert!(layout.inspector == 0.0 || layout.inspector >= 292.0);
                    assert!(layout.controls + layout.inspector + layout.canvas <= width);
                }
            }
        }
        let narrow = PaneLayout::for_width(Workspace::Design, 900.0 / 1.30 - RAIL_WIDTH);
        assert!(narrow.collapsed(Drawer::Controls));
        assert!(narrow.collapsed(Drawer::Inspector));
        let medium = PaneLayout::for_width(Workspace::Design, 1100.0 / 1.15 - RAIL_WIDTH);
        assert!(!medium.collapsed(Drawer::Controls));
        assert!(medium.collapsed(Drawer::Inspector));
    }

    #[test]
    fn five_unique_entries_and_command_shortcuts_obey_modal_and_text_focus() {
        assert_eq!(
            ENTRIES.map(|entry| entry.0),
            [
                Workspace::Design,
                Workspace::Stock,
                Workspace::CutPlan,
                Workspace::Hardware,
                Workspace::Handoff
            ]
        );
        let ctx = egui::Context::default();
        let modifiers = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        let key_event = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        for (index, key) in [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
        ]
        .into_iter()
        .enumerate()
        {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: vec![key_event(key, modifiers)],
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(shortcut(ui.ctx(), false), Some(ENTRIES[index].0));
                    assert_eq!(shortcut(ui.ctx(), true), None);
                },
            );
            output.textures_delta.clear();
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![key_event(egui::Key::Num1, egui::Modifiers::default())],
                ..Default::default()
            },
            |ui| assert_eq!(shortcut(ui.ctx(), false), None),
        );
        output.textures_delta.clear();

        let id = egui::Id::new("shell-shortcut-text-focus");
        let mut query = String::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.add(egui::TextEdit::singleline(&mut query).id(id));
            ui.memory_mut(|memory| memory.request_focus(id));
        });
        output.textures_delta.clear();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![key_event(egui::Key::Num4, modifiers)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(shortcut(ui.ctx(), false), None);
                ui.add(egui::TextEdit::singleline(&mut query).id(id));
            },
        );
        output.textures_delta.clear();
    }
}
