//! Presentation-only Welcome page. The host supplies a cached, validated recent
//! list and optional recovery discovery; rendering never reads or writes files.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Color32, RichText, Stroke};
use fluent_bundle::FluentArgs;
use uuid::Uuid;

use crate::APPLICATION_NAME;
use crate::export::ExportStatus;
use crate::i18n::{Language, Localizer};
use crate::icons::{Icon, icon};
use crate::recent_projects::{RecentProjectView, RecentStatus, THUMBNAIL_SIZE};
use crate::recovery::{DiscoveryStatus, RecoveryDiscovery, RecoveryIdentity};
use crate::template_setup::TemplateKind;
use crate::theme_widgets as tw;

const SIDEBAR_WIDTH: f32 = 340.0;
const COMPACT_MIN_CONTENT: f32 = 390.0;
/// Logo glyph tint (`#F4C27A`).
const LOGO_TINT: Color32 = Color32::from_rgb(244, 194, 122);
/// Save-icon tint on the recovery tile (`#9A5B12`).
const RECOVERY_ICON: Color32 = Color32::from_rgb(154, 91, 18);
/// "Recovery snapshot" caption (`#8A5418`).
const RECOVERY_CAPTION: Color32 = Color32::from_rgb(138, 84, 24);
/// Chip ink for "Recovery available" (`#7A4410`).
const RECOVERY_CHIP_INK: Color32 = Color32::from_rgb(122, 68, 16);
/// Chip ink for a stale export (`#8A520A`).
const STALE_CHIP_INK: Color32 = Color32::from_rgb(138, 82, 10);
/// Hatched thumbnail placeholder (`#EFEBE3` with `#E4DED3` lines).
const HATCH_FILL: Color32 = Color32::from_rgb(239, 235, 227);
const HATCH_LINE: Color32 = Color32::from_rgb(228, 222, 211);
/// Dashed outline of a missing file's thumbnail (`#CFC7B9`).
const MISSING_OUTLINE: Color32 = Color32::from_rgb(207, 199, 185);
const ROW_HEIGHT: f32 = 66.0;

fn wide_columns(width: f32) -> Option<(f32, f32)> {
    (width >= SIDEBAR_WIDTH + COMPACT_MIN_CONTENT).then_some((SIDEBAR_WIDTH, width - SIDEBAR_WIDTH))
}

/// The host must revalidate every identity at activation time, run navigation
/// and unsaved-document guards, and own the native picker. No intent edits a
/// project, a recent index, or recovery bytes in this module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WelcomeIntent {
    NewProject,
    Template(TemplateKind),
    OpenProject,
    OpenRecent {
        path: PathBuf,
        project_id: Uuid,
    },
    Locate {
        path: PathBuf,
        project_id: Uuid,
    },
    Remove {
        path: PathBuf,
        project_id: Uuid,
    },
    Recovery {
        identity: RecoveryIdentity,
        snapshot_path: PathBuf,
        action: RecoveryAction,
    },
    Preferences,
    SetLanguage(Language),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecentAction {
    Open,
    Locate,
    Remove,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryAction {
    Recover,
    DecideLater,
    Discard,
}

/// Only validated actionable discoveries can produce decisions. The host must
/// recheck registration, snapshot and saved-file identity before acting.
pub fn recovery_intent(
    discovery: &RecoveryDiscovery,
    action: RecoveryAction,
) -> Option<WelcomeIntent> {
    if !matches!(
        discovery.status,
        DiscoveryStatus::Newer | DiscoveryStatus::Untitled
    ) {
        return None;
    }
    Some(WelcomeIntent::Recovery {
        identity: discovery.identity.clone(),
        snapshot_path: discovery.snapshot_path.clone(),
        action,
    })
}

/// The identity includes both the canonical indexed path and UUID: a copy of
/// the same project at a different path is a different recent row.
pub fn recent_intent(row: &RecentProjectView, action: RecentAction) -> Option<WelcomeIntent> {
    let path = row.entry.path.clone();
    let project_id = row.entry.project_id;
    match (action, &row.status) {
        (RecentAction::Open, RecentStatus::Available(_)) => {
            Some(WelcomeIntent::OpenRecent { path, project_id })
        }
        (RecentAction::Locate, RecentStatus::Missing | RecentStatus::Unavailable(_)) => {
            Some(WelcomeIntent::Locate { path, project_id })
        }
        (RecentAction::Remove, _) => Some(WelcomeIntent::Remove { path, project_id }),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryBadge {
    Unknown,
    NoneKnown,
    Available,
    Unavailable,
}

/// Match the registered path and UUID, never just the project name or UUID.
pub fn recovery_badge(
    row: &RecentProjectView,
    discoveries: Option<&[RecoveryDiscovery]>,
) -> RecoveryBadge {
    let Some(discoveries) = discoveries else {
        return RecoveryBadge::Unknown;
    };
    let mut result = RecoveryBadge::NoneKnown;
    for item in discoveries.iter().filter(|item| {
        item.identity.project_id == row.entry.project_id
            && item.identity.saved_path.as_deref() == Some(row.entry.path.as_path())
    }) {
        match item.status {
            DiscoveryStatus::Newer => return RecoveryBadge::Available,
            DiscoveryStatus::SavedUnavailable(_) | DiscoveryStatus::Invalid(_) => {
                result = RecoveryBadge::Unavailable;
            }
            DiscoveryStatus::NotNewer | DiscoveryStatus::Untitled => {}
        }
    }
    result
}

#[derive(Default)]
pub struct WelcomeState {
    /// Presentation-only filter. Refresh the cached recents in the host when it
    /// opens Welcome or after a successful file operation; do not read per frame.
    pub filter: String,
    thumbnails: HashMap<String, egui::TextureHandle>,
}

impl WelcomeState {
    pub fn filtered<'a>(&self, rows: &'a [RecentProjectView]) -> Vec<&'a RecentProjectView> {
        let needle = self.filter.trim().to_lowercase();
        rows.iter()
            .filter(|row| {
                let name = match &row.status {
                    RecentStatus::Available(summary) => &summary.name,
                    _ => &row.entry.cached_summary.name,
                };
                needle.is_empty()
                    || name.to_lowercase().contains(&needle)
                    || row
                        .entry
                        .path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&needle)
            })
            .collect()
    }

    /// Draw into the host's available native egui panel. `recents` must be a
    /// previously cached `RecentProjects::list("")` result; `None` discovery
    /// means recovery was not checked, not that no recovery exists.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        localizer: &Localizer,
        recents: &[RecentProjectView],
        discoveries: Option<&[RecoveryDiscovery]>,
    ) -> Vec<WelcomeIntent> {
        let mut intents = Vec::new();
        let full = ui.available_rect_before_wrap();
        ui.painter().rect_filled(full, 0.0, tw::APP);
        if wide_columns(full.width()).is_none() {
            egui::ScrollArea::vertical()
                .id_salt("welcome-compact")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(full.width());
                    egui::Frame::new()
                        .fill(tw::PANEL)
                        .inner_margin(egui::Margin::symmetric(24, 20))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.sidebar_actions(ui, localizer, &mut intents);
                            ui.add_space(16.0);
                            self.language_control(ui, localizer, &mut intents);
                            self.preferences_control(ui, localizer, &mut intents);
                        });
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(20, 16))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.content(ui, localizer, recents, discoveries, &mut intents);
                        });
                });
            return intents;
        }
        let (sidebar_width, _) = wide_columns(full.width()).expect("wide layout");
        let height = full.height().max(1.0);
        let side = egui::Rect::from_min_size(full.min, egui::vec2(sidebar_width, height));
        ui.painter().rect_filled(side, 0.0, tw::PANEL);
        ui.painter().vline(
            side.right() - 0.5,
            side.y_range(),
            Stroke::new(1.0, tw::BORDER),
        );
        let side_inner = egui::Rect::from_min_max(
            side.min + egui::vec2(24.0, 28.0),
            side.max - egui::vec2(24.0, 20.0),
        );
        let mut top = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(side_inner)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        self.sidebar_actions(&mut top, localizer, &mut intents);
        let mut bottom = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(side_inner)
                .layout(egui::Layout::bottom_up(egui::Align::Min)),
        );
        bottom.spacing_mut().item_spacing.y = 4.0;
        self.preferences_control(&mut bottom, localizer, &mut intents);
        self.language_control(&mut bottom, localizer, &mut intents);
        let content = egui::Rect::from_min_max(
            egui::pos2(side.right() + 28.0, full.top() + 24.0),
            egui::pos2(full.right() - 28.0, full.top() + height - 24.0),
        );
        let mut right = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        right.set_width(content.width());
        self.content(&mut right, localizer, recents, discoveries, &mut intents);
        ui.allocate_rect(full, egui::Sense::hover());
        intents
    }

    fn sidebar_actions(&self, ui: &mut egui::Ui, l: &Localizer, intents: &mut Vec<WelcomeIntent>) {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let (tile, _) = ui.allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
            ui.painter().rect_filled(tile, 10.0, tw::TEXT);
            icon(Icon::Board, LOGO_TINT, 24.0).paint_at(
                ui,
                egui::Rect::from_center_size(tile.center(), egui::vec2(24.0, 24.0)),
            );
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.add_space(3.0);
                ui.add(
                    egui::Label::new(tw::semibold(ui, APPLICATION_NAME, 17.0).color(tw::TEXT))
                        .selectable(false),
                );
                ui.add(egui::Label::new(
                    tw::mono(version_line(), 11.0).color(tw::FAINT),
                ));
            });
        });
        ui.add_space(20.0);
        if launch_button(ui, Icon::Plus, &l.text("welcome-new"), &shortcut("N"), true).clicked() {
            intents.push(WelcomeIntent::NewProject);
        }
        if launch_button(
            ui,
            Icon::Folder,
            &l.text("welcome-open"),
            &shortcut("O"),
            false,
        )
        .clicked()
        {
            intents.push(WelcomeIntent::OpenProject);
        }
        ui.add_space(20.0);
        tracked_label(ui, &l.text("welcome-templates"));
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let tile_width = ((ui.available_width() - 12.0) / 3.0).max(40.0);
            for (kind, key) in [
                (TemplateKind::Base, "template-setup-base"),
                (TemplateKind::Wall, "template-setup-wall"),
                (TemplateKind::Drawers, "template-setup-drawers"),
            ] {
                if template_tile(ui, kind, &l.text(key), tile_width).clicked() {
                    intents.push(WelcomeIntent::Template(kind));
                }
            }
        });
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(
                RichText::new(l.text("welcome-template-hint"))
                    .size(11.5)
                    .color(tw::FAINT),
            )
            .wrap(),
        );
    }

    fn language_control(&self, ui: &mut egui::Ui, l: &Localizer, intents: &mut Vec<WelcomeIntent>) {
        let language_key = |language: Language| match language {
            Language::En => "language-en",
            Language::PtBr => "language-pt-br",
        };
        footer_row(ui, Icon::Globe, &l.text("welcome-language"), |ui| {
            // Right-to-left: chevron first, then the menu to its left.
            ui.add(icon(Icon::ChevDown, tw::MUTED, 11.0));
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            widgets.inactive.bg_stroke = Stroke::NONE;
            ui.spacing_mut().button_padding = egui::vec2(4.0, 2.0);
            ui.menu_button(
                RichText::new(l.text(language_key(l.language())))
                    .size(12.5)
                    .color(tw::TEXT),
                |ui| {
                    for language in [Language::En, Language::PtBr] {
                        if ui
                            .selectable_label(
                                l.language() == language,
                                l.text(language_key(language)),
                            )
                            .clicked()
                        {
                            intents.push(WelcomeIntent::SetLanguage(language));
                            ui.close();
                        }
                    }
                },
            )
            .response
            .on_hover_text(l.text("ui-language"));
        });
    }

    fn preferences_control(
        &self,
        ui: &mut egui::Ui,
        l: &Localizer,
        intents: &mut Vec<WelcomeIntent>,
    ) {
        let label = l.text("welcome-preferences");
        let clicked = footer_row(ui, Icon::Sliders, &label, |ui| {
            ui.label(tw::mono(shortcut(","), 11.0).color(tw::FAINT));
        });
        if clicked {
            intents.push(WelcomeIntent::Preferences);
        }
    }

    fn content(
        &mut self,
        ui: &mut egui::Ui,
        l: &Localizer,
        recents: &[RecentProjectView],
        discoveries: Option<&[RecoveryDiscovery]>,
        intents: &mut Vec<WelcomeIntent>,
    ) {
        ui.spacing_mut().item_spacing.y = 0.0;
        if self.recovery_cards(ui, l, recents, discoveries, intents) {
            ui.add_space(18.0);
        }
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 32.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add(
                    egui::Label::new(
                        tw::semibold(ui, l.text("welcome-recents"), 15.0).color(tw::TEXT),
                    )
                    .selectable(false),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    filter_field(ui, &mut self.filter, &l.text("welcome-filter"));
                });
            },
        );
        ui.add_space(12.0);
        let rows = self.filtered(recents);
        let list = egui::Frame::new()
            .fill(tw::PANEL)
            .stroke(Stroke::new(1.0, tw::BORDER_SOFT))
            .corner_radius(10);
        if rows.is_empty() {
            list.inner_margin(egui::Margin::symmetric(16, 22))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        if recents.is_empty() {
                            ui.add(icon(Icon::Folder, tw::DISABLED, 22.0));
                            ui.label(
                                tw::medium(ui, l.text("welcome-empty-title"), 13.0)
                                    .color(tw::TEXT_2),
                            );
                            ui.label(
                                RichText::new(l.text("welcome-empty-hint"))
                                    .size(12.0)
                                    .color(tw::FAINT),
                            );
                        } else {
                            ui.label(
                                RichText::new(l.text("welcome-no-results"))
                                    .size(12.5)
                                    .color(tw::MUTED),
                            );
                        }
                    });
                });
            return;
        }
        let max_height = if ui.available_height().is_finite() {
            (ui.available_height() - 4.0).max(150.0)
        } else {
            f32::INFINITY
        };
        list.show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_salt("welcome-recents")
                .max_height(max_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let count = rows.len();
                    for (index, row) in rows.into_iter().enumerate() {
                        self.row(ui, l, row, discoveries, intents);
                        if index + 1 < count {
                            let (line, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 1.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().hline(
                                line.x_range(),
                                line.center().y,
                                Stroke::new(1.0, tw::RULE),
                            );
                        }
                    }
                });
        });
    }

    /// Returns whether anything was drawn.
    fn recovery_cards(
        &self,
        ui: &mut egui::Ui,
        l: &Localizer,
        recents: &[RecentProjectView],
        discoveries: Option<&[RecoveryDiscovery]>,
        intents: &mut Vec<WelcomeIntent>,
    ) -> bool {
        let Some(discoveries) = discoveries else {
            return false;
        };
        let mut drawn = false;
        let actionable: Vec<_> = discoveries
            .iter()
            .filter(|d| matches!(d.status, DiscoveryStatus::Newer | DiscoveryStatus::Untitled))
            .collect();
        egui::ScrollArea::vertical()
            .id_salt("welcome-recovery-cards")
            .max_height(if actionable.len() > 1 {
                420.0
            } else {
                f32::INFINITY
            })
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (index, discovery) in actionable.iter().enumerate() {
                    if index > 0 {
                        ui.add_space(10.0);
                    }
                    recovery_card(ui, l, recents, discovery, intents);
                    drawn = true;
                }
            });
        let skipped: Vec<_> = discoveries
            .iter()
            .filter_map(|d| match &d.status {
                DiscoveryStatus::SavedUnavailable(reason) | DiscoveryStatus::Invalid(reason) => {
                    Some(reason.as_str())
                }
                _ => None,
            })
            .collect();
        if !skipped.is_empty() {
            if drawn {
                ui.add_space(8.0);
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.add(icon(Icon::Warning, tw::WARN, 12.0));
                ui.label(
                    RichText::new(l.count("welcome-recovery-skipped", skipped.len() as u64))
                        .size(12.0)
                        .color(tw::MUTED),
                )
                .on_hover_text(skipped.join("\n"));
            });
            drawn = true;
        }
        drawn
    }

    fn row(
        &mut self,
        ui: &mut egui::Ui,
        l: &Localizer,
        row: &RecentProjectView,
        discoveries: Option<&[RecoveryDiscovery]>,
        intents: &mut Vec<WelcomeIntent>,
    ) {
        let name = match &row.status {
            RecentStatus::Available(summary) => &summary.name,
            _ => &row.entry.cached_summary.name,
        };
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), ROW_HEIGHT),
            egui::Sense::hover(),
        );
        let open = recent_intent(row, RecentAction::Open);
        let id = ui
            .id()
            .with(("welcome-recent", &row.entry.path, row.entry.project_id));
        // Register the row first so its inner links and menu stay on top.
        let response = ui.interact(
            rect,
            id,
            if open.is_some() {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, open.is_some(), name.as_str())
        });
        if open.is_some() && (response.hovered() || response.has_focus()) {
            ui.painter().rect_filled(rect, 0.0, tw::HOVER_ROW);
        }
        let inner = rect.shrink2(egui::vec2(14.0, 10.0));
        let thumb = egui::Rect::from_min_size(inner.min, egui::vec2(64.0, 46.0));
        self.paint_thumbnail(ui, row, thumb);
        let path = display_path(&row.entry.path);
        let wide = inner.width() >= 520.0;
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    egui::pos2(thumb.right() + 14.0, inner.top()),
                    inner.max,
                ))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        child.spacing_mut().item_spacing.x = 14.0;
        let ui = &mut child;
        match &row.status {
            RecentStatus::Available(summary) => {
                // Right to left: menu, date, chip, stats, then name/path.
                ui.spacing_mut().item_spacing.x = 8.0;
                let widgets = &mut ui.visuals_mut().widgets;
                widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                widgets.inactive.bg_stroke = Stroke::NONE;
                let menu = ui.menu_image_button(icon(Icon::Dots, tw::MUTED, 14.0), |ui| {
                    if let Some(intent) = open.clone()
                        && ui.button(l.text("welcome-open-recent")).clicked()
                    {
                        intents.push(intent);
                        ui.close();
                    }
                    if let Some(intent) = recent_intent(row, RecentAction::Remove)
                        && ui.button(l.text("welcome-remove")).clicked()
                    {
                        intents.push(intent);
                        ui.close();
                    }
                });
                menu.response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        l.text("welcome-more"),
                    )
                });
                ui.spacing_mut().item_spacing.x = 14.0;
                fixed_cell(ui, 84.0, egui::Align::Max, |ui| {
                    ui.label(
                        RichText::new(last_used_label(row.entry.last_used_unix_ms, l))
                            .size(12.0)
                            .color(tw::MUTED),
                    )
                    .on_hover_text(format!(
                        "{}: {}",
                        l.text("welcome-last-used"),
                        full_date_label(row.entry.last_used_unix_ms, l)
                    ));
                });
                let details = format!(
                    "{}\n{}\n{} · {} · {} · {}",
                    l.text(export_label(row.export_status)),
                    l.text(recovery_label(recovery_badge(row, discoveries))),
                    l.count("boards-count", summary.boards as u64),
                    l.count("welcome-assemblies-count", summary.assemblies as u64),
                    l.count("welcome-materials-count", summary.materials as u64),
                    l.count("welcome-stock-count", summary.stock as u64)
                );
                if wide {
                    fixed_cell(ui, 140.0, egui::Align::Min, |ui| {
                        if let Some((key, fill, ink)) =
                            status_chip(row.export_status, recovery_badge(row, discoveries))
                        {
                            tw::chip(ui, &l.text(key), fill, ink).on_hover_text(&details);
                        }
                    });
                    fixed_cell(ui, 130.0, egui::Align::Min, |ui| {
                        let stock = if summary.stock == 0 {
                            l.text("welcome-no-stock")
                        } else {
                            l.count("welcome-sheets-count", summary.stock as u64)
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!(
                                    "{} · {stock}",
                                    l.count("boards-count", summary.boards as u64)
                                ))
                                .size(12.0)
                                .color(tw::MUTED),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&details);
                    });
                }
                name_cell(ui, name, &path, false, &row.entry.path);
            }
            RecentStatus::Missing | RecentStatus::Unavailable(_) => {
                if let Some(intent) = recent_intent(row, RecentAction::Remove)
                    && link(
                        ui,
                        &l.text("welcome-remove-short"),
                        &l.text("welcome-remove"),
                        tw::FAINT,
                    )
                    .clicked()
                {
                    intents.push(intent);
                }
                if let Some(intent) = recent_intent(row, RecentAction::Locate)
                    && link(
                        ui,
                        &l.text("welcome-locate"),
                        &l.text("welcome-locate"),
                        tw::ACCENT_DARK,
                    )
                    .clicked()
                {
                    intents.push(intent);
                }
                let (reason, detail) = match &row.status {
                    RecentStatus::Unavailable(reason) => (
                        l.text("welcome-file-unreadable"),
                        format!("{}: {reason}", l.text("welcome-unavailable")),
                    ),
                    _ => (l.text("welcome-file-missing"), l.text("welcome-missing")),
                };
                let response = name_cell(
                    ui,
                    name,
                    &format!("{reason} · {path}"),
                    true,
                    &row.entry.path,
                );
                response.on_hover_text(format!("{detail}\n{}", l.text("welcome-cached-name")));
            }
        }
        if let Some(intent) = open
            && response.clicked()
        {
            intents.push(intent);
        }
        response.on_hover_text(l.text("welcome-open-recent"));
    }

    fn paint_thumbnail(&mut self, ui: &mut egui::Ui, row: &RecentProjectView, rect: egui::Rect) {
        let painter = ui.painter();
        if !matches!(row.status, RecentStatus::Available(_)) {
            dashed_rect(painter, rect, MISSING_OUTLINE);
            return;
        }
        if let (Some(key), Some(rgba)) = (&row.entry.thumbnail_key, &row.thumbnail) {
            if self.thumbnails.len() >= 128 && !self.thumbnails.contains_key(key) {
                self.thumbnails.clear();
            }
            let texture = self.thumbnails.entry(key.clone()).or_insert_with(|| {
                ui.ctx().load_texture(
                    format!("recent-{key}"),
                    egui::ColorImage::from_rgba_unmultiplied(THUMBNAIL_SIZE, rgba),
                    egui::TextureOptions::LINEAR,
                )
            });
            egui::Image::new((texture.id(), rect.size()))
                .corner_radius(6)
                .paint_at(ui, rect);
            ui.painter().rect_stroke(
                rect,
                6.0,
                Stroke::new(1.0, tw::BORDER),
                egui::StrokeKind::Inside,
            );
            return;
        }
        painter.rect_filled(rect, 6.0, HATCH_FILL);
        let clipped = painter.with_clip_rect(rect.shrink(1.0));
        let mut offset = -rect.height();
        while offset < rect.width() {
            let a = egui::pos2(rect.left() + offset, rect.bottom());
            let b = a + egui::vec2(rect.height(), -rect.height());
            clipped.line_segment([a, b], Stroke::new(1.0, HATCH_LINE));
            offset += 7.0;
        }
        painter.rect_stroke(
            rect,
            6.0,
            Stroke::new(1.0, tw::BORDER),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.right_bottom() - egui::vec2(4.0, 3.0),
            egui::Align2::RIGHT_BOTTOM,
            "3D",
            egui::FontId::monospace(8.0),
            tw::FAINT,
        );
    }
}

fn recovery_card(
    ui: &mut egui::Ui,
    l: &Localizer,
    recents: &[RecentProjectView],
    discovery: &RecoveryDiscovery,
    intents: &mut Vec<WelcomeIntent>,
) {
    let untitled = discovery.identity.saved_path.is_none();
    let name = discovery.identity.saved_path.as_ref().map(|path| {
        recents
            .iter()
            .find(|row| {
                row.entry.path == *path && row.entry.project_id == discovery.identity.project_id
            })
            .map(|row| match &row.status {
                RecentStatus::Available(summary) => summary.name.clone(),
                _ => row.entry.cached_summary.name.clone(),
            })
            .unwrap_or_else(|| {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            })
    });
    let title = match &name {
        Some(name) => {
            let mut args = FluentArgs::new();
            args.set("name", name.clone());
            l.format("welcome-recovery-title", Some(&args))
        }
        None => l.text("welcome-recovery-untitled"),
    };
    egui::Frame::new()
        .fill(tw::CARD)
        .stroke(Stroke::new(1.0, tw::WARN_STROKE))
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(18, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 12.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (tile, _) =
                    ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                ui.painter().rect_filled(tile, 8.0, tw::ACCENT_BG);
                icon(Icon::Save, RECOVERY_ICON, 18.0).paint_at(
                    ui,
                    egui::Rect::from_center_size(tile.center(), egui::vec2(18.0, 18.0)),
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.add(
                        egui::Label::new(tw::semibold(ui, &title, 14.5).color(tw::TEXT)).truncate(),
                    )
                    .on_hover_text(format!(
                        "{}: {}",
                        l.text("welcome-recovery-project-id"),
                        discovery.identity.project_id
                    ));
                    ui.add(
                        egui::Label::new(
                            RichText::new(l.text(if untitled {
                                "welcome-recovery-body-untitled"
                            } else {
                                "welcome-recovery-body"
                            }))
                            .size(12.5)
                            .color(tw::SECONDARY),
                        )
                        .wrap(),
                    );
                });
            });
            ui.horizontal(|ui| {
                ui.add_space(46.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                let width = ((ui.available_width() - 8.0) / 2.0).max(80.0);
                let saved_value = if untitled {
                    l.text("welcome-recovery-no-file")
                } else {
                    discovery.saved_revision.map_or_else(
                        || l.text("welcome-recovery-value-unknown"),
                        |r| format!("rev {r}"),
                    )
                };
                revision_tile(
                    ui,
                    width,
                    &l.text("welcome-recovery-saved-path"),
                    &saved_value,
                    false,
                )
                .on_hover_text(discovery.identity.saved_path.as_ref().map_or_else(
                    || l.text("welcome-recovery-untitled-path"),
                    |p| p.display().to_string(),
                ));
                let mut snapshot = discovery.recovery_revision.map_or_else(
                    || l.text("welcome-recovery-value-unknown"),
                    |r| format!("rev {r}"),
                );
                if let Some(date) = short_modified(discovery.snapshot_modified) {
                    snapshot = format!("{snapshot} · {date}");
                }
                revision_tile(
                    ui,
                    width,
                    &l.text("welcome-recovery-snapshot-tile"),
                    &snapshot,
                    true,
                )
                .on_hover_text(format!(
                    "{}: {}",
                    l.text("welcome-recovery-snapshot-date"),
                    modified_label(discovery.snapshot_modified, l)
                ));
            });
            ui.horizontal(|ui| {
                ui.add_space(46.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                let mut chosen = None;
                if ui
                    .add(
                        egui::Button::new(
                            tw::medium(ui, l.text("welcome-recovery-recover"), 13.0)
                                .color(tw::PANEL),
                        )
                        .fill(tw::TEXT)
                        .stroke(Stroke::NONE)
                        .corner_radius(7)
                        .min_size(egui::vec2(0.0, 32.0)),
                    )
                    .clicked()
                {
                    chosen = Some(RecoveryAction::Recover);
                }
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(l.text("welcome-recovery-defer"))
                                .size(13.0)
                                .color(tw::TEXT),
                        )
                        .fill(tw::VIEWPORT)
                        .stroke(Stroke::new(1.0, tw::BORDER))
                        .corner_radius(7)
                        .min_size(egui::vec2(0.0, 32.0)),
                    )
                    .clicked()
                {
                    chosen = Some(RecoveryAction::DecideLater);
                }
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(l.text("welcome-recovery-discard"))
                                .size(13.0)
                                .color(tw::DANGER),
                        )
                        .frame(false)
                        .min_size(egui::vec2(0.0, 32.0)),
                    )
                    .clicked()
                {
                    chosen = Some(RecoveryAction::Discard);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(l.text("welcome-recovery-note"))
                                .size(11.5)
                                .color(tw::FAINT),
                        )
                        .truncate(),
                    )
                    .on_hover_text(l.text(if untitled {
                        "welcome-recovery-save-as"
                    } else {
                        "welcome-recovery-unsaved"
                    }));
                });
                if let Some(action) = chosen
                    && let Some(intent) = recovery_intent(discovery, action)
                {
                    intents.push(intent);
                }
            });
        });
}

fn revision_tile(
    ui: &mut egui::Ui,
    width: f32,
    caption: &str,
    value: &str,
    recovery: bool,
) -> egui::Response {
    let frame = if recovery {
        egui::Frame::new()
            .fill(tw::WARN_BG)
            .stroke(Stroke::new(1.0, tw::WARN_STROKE))
    } else {
        egui::Frame::new().fill(tw::APP)
    };
    frame
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width((width - 24.0).max(40.0));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(caption).size(11.0).color(if recovery {
                    RECOVERY_CAPTION
                } else {
                    tw::MUTED
                }));
                ui.add(
                    egui::Label::new(tw::mono(value, 12.5).color(if recovery {
                        tw::ACCENT_INK
                    } else {
                        tw::TEXT
                    }))
                    .truncate(),
                );
            });
        })
        .response
}

/// Sidebar primary/secondary launch action: icon, label, trailing shortcut.
fn launch_button(
    ui: &mut egui::Ui,
    symbol: Icon,
    label: &str,
    keys: &str,
    primary: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let hovered = response.hovered() || response.has_focus();
    let (fill, ink, keys_ink) = if primary {
        (
            if hovered {
                Color32::from_rgb(58, 51, 44)
            } else {
                tw::TEXT
            },
            tw::PANEL,
            tw::PANEL.gamma_multiply(0.6),
        )
    } else {
        (
            if hovered {
                Color32::from_rgb(228, 223, 214)
            } else {
                tw::VIEWPORT
            },
            tw::TEXT,
            tw::FAINT,
        )
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 8.0, fill);
    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(2.0),
            10.0,
            Stroke::new(2.0, tw::FOCUS),
            egui::StrokeKind::Outside,
        );
    }
    icon(symbol, ink, 16.0).paint_at(
        ui,
        egui::Rect::from_center_size(
            rect.left_center() + egui::vec2(22.0, 0.0),
            egui::vec2(16.0, 16.0),
        ),
    );
    painter.text(
        rect.left_center() + egui::vec2(40.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        tw::weighted_font(ui, 13.0, crate::theme::Typeface::SansMedium),
        ink,
    );
    painter.text(
        rect.right_center() - egui::vec2(14.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        keys,
        egui::FontId::monospace(11.0),
        keys_ink,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Template tile with a small painted cabinet front.
fn template_tile(ui: &mut egui::Ui, kind: TemplateKind, label: &str, width: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 72.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let hovered = response.hovered() || response.has_focus();
    let painter = ui.painter();
    painter.rect(
        rect,
        8.0,
        if hovered { tw::HOVER_ROW } else { tw::CARD },
        Stroke::new(
            1.0,
            if hovered {
                tw::BORDER_STRONG
            } else {
                tw::BORDER_SOFT
            },
        ),
        egui::StrokeKind::Inside,
    );
    let stroke = Stroke::new(1.5, tw::MUTED);
    let thin = Stroke::new(1.0, tw::MUTED);
    let glyph_center = egui::pos2(rect.center().x, rect.top() + 25.0);
    let body = match kind {
        TemplateKind::Wall => egui::Rect::from_center_size(glyph_center, egui::vec2(34.0, 22.0)),
        _ => egui::Rect::from_center_size(glyph_center, egui::vec2(34.0, 30.0)),
    };
    painter.rect_stroke(body, 2.0, stroke, egui::StrokeKind::Middle);
    match kind {
        TemplateKind::Base => {
            painter.vline(body.center().x, body.y_range(), thin);
        }
        TemplateKind::Drawers => {
            for i in 1..3 {
                let y = body.top() + body.height() * i as f32 / 3.0;
                painter.hline(body.x_range(), y, thin);
            }
        }
        TemplateKind::Wall => {}
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 10.0),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(11.5),
        tw::TEXT,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Sidebar footer row: icon + label, trailing content laid out right to left.
/// Returns whether the row itself was clicked.
fn footer_row(
    ui: &mut egui::Ui,
    symbol: Icon,
    label: &str,
    trailing: impl FnOnce(&mut egui::Ui),
) -> bool {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::hover());
    let response = ui.interact(
        rect,
        ui.id().with(("welcome-footer", label)),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(rect.expand2(egui::vec2(6.0, 0.0)), 6.0, tw::HOVER_ROW);
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 10.0;
    child.add(icon(symbol, tw::SECONDARY, 15.0));
    child.add(
        egui::Label::new(RichText::new(label).size(12.5).color(tw::SECONDARY))
            .selectable(false)
            .truncate(),
    );
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        trailing(ui);
    });
    response.clicked()
}

fn filter_field(ui: &mut egui::Ui, text: &mut String, hint: &str) {
    let width = 220.0_f32.min(ui.available_width() * 0.5).max(120.0);
    let id = ui.id().with("welcome-filter");
    let focused = ui.memory(|m| m.has_focus(id));
    egui::Frame::new()
        .fill(if focused { tw::CARD } else { tw::PANEL })
        .stroke(Stroke::new(
            1.0,
            if focused { tw::FOCUS } else { tw::BORDER_SOFT },
        ))
        .corner_radius(7)
        .inner_margin(egui::Margin::symmetric(10, 0))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(width - 20.0, 30.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_size(egui::vec2(width - 20.0, 30.0));
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.add(icon(Icon::Search, tw::FAINT, 14.0));
                    ui.add(
                        egui::TextEdit::singleline(text)
                            .id(id)
                            .frame(egui::Frame::NONE)
                            .hint_text(RichText::new(hint).color(tw::FAINT))
                            .desired_width(ui.available_width()),
                    );
                },
            );
        });
}

/// Name (medium) over a mono ellipsized path; fills the remaining row width.
fn name_cell(
    ui: &mut egui::Ui,
    name: &str,
    path: &str,
    muted: bool,
    full_path: &Path,
) -> egui::Response {
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.add_space(((ui.available_height() - 36.0) / 2.0).max(0.0));
        let name = tw::medium(ui, name, 13.0).color(if muted { tw::FAINT } else { tw::TEXT });
        let response = ui.add(egui::Label::new(name).truncate().selectable(false));
        ui.add(
            egui::Label::new(tw::mono(path, 11.0).color(tw::FAINT))
                .truncate()
                .selectable(false),
        )
        .on_hover_text(full_path.display().to_string());
        response
    })
    .inner
}

fn fixed_cell(ui: &mut egui::Ui, width: f32, align: egui::Align, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 24.0),
        if align == egui::Align::Max {
            egui::Layout::right_to_left(egui::Align::Center)
        } else {
            egui::Layout::left_to_right(egui::Align::Center)
        },
        |ui| {
            ui.set_width(width);
            add(ui);
        },
    );
}

fn link(ui: &mut egui::Ui, text: &str, accessible: &str, color: Color32) -> egui::Response {
    let response = ui.add(
        egui::Button::new(RichText::new(text).size(12.0).color(color))
            .frame(false)
            .min_size(egui::vec2(0.0, 24.0)),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, accessible));
    if accessible != text {
        response.on_hover_text(accessible)
    } else {
        response
    }
}

fn dashed_rect(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let r = rect.shrink(0.5);
    let stroke = Stroke::new(1.0, color);
    for [a, b] in [
        [r.left_top(), r.right_top()],
        [r.right_top(), r.right_bottom()],
        [r.right_bottom(), r.left_bottom()],
        [r.left_bottom(), r.left_top()],
    ] {
        painter.extend(egui::Shape::dashed_line(&[a, b], stroke, 4.0, 3.0));
    }
}

fn tracked_label(ui: &mut egui::Ui, text: &str) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: tw::weighted_font(ui, 10.5, crate::theme::Typeface::SansSemibold),
            color: tw::FAINT,
            extra_letter_spacing: 10.5 * 0.09,
            ..Default::default()
        },
    );
    ui.add(egui::Label::new(job).selectable(false));
}

fn status_chip(
    export: Option<ExportStatus>,
    recovery: RecoveryBadge,
) -> Option<(&'static str, Color32, Color32)> {
    if recovery == RecoveryBadge::Available {
        return Some(("welcome-chip-recovery", tw::ACCENT_BG, RECOVERY_CHIP_INK));
    }
    match export? {
        ExportStatus::Current => Some(("welcome-chip-current", tw::OK_BG, tw::OK_INK)),
        ExportStatus::PacketStale | ExportStatus::WoodStale => {
            Some(("welcome-chip-stale", tw::WARN_BG, STALE_CHIP_INK))
        }
        ExportStatus::NeverExported => Some(("welcome-chip-never", tw::VIEWPORT, tw::SECONDARY)),
        ExportStatus::Unknown => None,
    }
}

fn version_line() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    format!("v{} · {os} {arch}", env!("CARGO_PKG_VERSION"))
}

/// `~`-relative path for display only.
fn display_path(path: &Path) -> String {
    let text = path.display().to_string();
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => {
            let home = PathBuf::from(home).display().to_string();
            text.strip_prefix(&home)
                .filter(|rest| rest.starts_with('/') || rest.starts_with('\\'))
                .map_or(text.clone(), |rest| format!("~{rest}"))
        }
        _ => text,
    }
}

fn export_label(status: Option<ExportStatus>) -> &'static str {
    match status {
        Some(ExportStatus::NeverExported) => "export-never",
        Some(ExportStatus::Current) => "export-current",
        Some(ExportStatus::PacketStale | ExportStatus::WoodStale) => "export-stale",
        Some(ExportStatus::Unknown) | None => "export-unknown",
    }
}

fn shortcut(key: &str) -> String {
    format!(
        "{}{key}",
        if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl+"
        }
    )
}

fn recovery_label(status: RecoveryBadge) -> &'static str {
    match status {
        RecoveryBadge::Available => "welcome-recovery-available",
        RecoveryBadge::Unknown => "welcome-recovery-unknown",
        RecoveryBadge::NoneKnown => "welcome-recovery-none",
        RecoveryBadge::Unavailable => "welcome-recovery-invalid",
    }
}

fn modified_label(modified: Option<SystemTime>, l: &Localizer) -> String {
    modified
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| {
            time::OffsetDateTime::from_unix_timestamp(duration.as_secs().try_into().ok()?).ok()
        })
        .map(|date| {
            format!(
                "{} {:02}:{:02} UTC",
                date.date(),
                date.hour(),
                date.minute()
            )
        })
        .unwrap_or_else(|| l.text("welcome-recovery-value-unknown"))
}

/// "25/09 14:32" (UTC) for the compact recovery tile.
fn short_modified(modified: Option<SystemTime>) -> Option<String> {
    modified
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| {
            time::OffsetDateTime::from_unix_timestamp(duration.as_secs().try_into().ok()?).ok()
        })
        .map(|date| {
            format!(
                "{:02}/{:02} {:02}:{:02}",
                date.day(),
                u8::from(date.month()),
                date.hour(),
                date.minute()
            )
        })
}

fn full_date_label(unix_ms: u64, l: &Localizer) -> String {
    i64::try_from(unix_ms / 1000)
        .ok()
        .and_then(|seconds| time::OffsetDateTime::from_unix_timestamp(seconds).ok())
        .map(|date| date.date().to_string())
        .unwrap_or_else(|| l.text("welcome-date-unknown"))
}

fn last_used_label(unix_ms: u64, l: &Localizer) -> String {
    let Some(used) = UNIX_EPOCH.checked_add(Duration::from_millis(unix_ms)) else {
        return l.text("welcome-date-unknown");
    };
    let now = SystemTime::now();
    if let Ok(elapsed) = now.duration_since(used) {
        if elapsed < Duration::from_secs(24 * 60 * 60) {
            return l.text("welcome-today");
        }
        if elapsed < Duration::from_secs(48 * 60 * 60) {
            return l.text("welcome-yesterday");
        }
    }
    let Ok(ms) = i64::try_from(unix_ms) else {
        return l.text("welcome-date-unknown");
    };
    const EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const PT: [&str; 12] = [
        "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
    ];
    time::OffsetDateTime::from_unix_timestamp(ms / 1000)
        .map(|date| {
            let month = usize::from(u8::from(date.month())) - 1;
            let name = match l.language() {
                Language::En => EN[month],
                Language::PtBr => PT[month],
            };
            let this_year = time::OffsetDateTime::now_utc().year();
            if date.year() == this_year {
                format!("{} {name}", date.day())
            } else {
                format!("{} {name} {}", date.day(), date.year())
            }
        })
        .unwrap_or_else(|_| l.text("welcome-date-unknown"))
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn reference_and_compact_columns_are_bounded() {
        assert_eq!(wide_columns(1100.0), Some((340.0, 760.0)));
        assert_eq!(wide_columns(729.0), None);
        assert_eq!(wide_columns(620.0), None);
    }

    #[test]
    fn empty_reference_and_compact_welcome_render_in_both_languages() {
        for language in [Language::En, Language::PtBr] {
            for (width, height) in [(1100.0, 700.0), (620.0, 500.0)] {
                let ctx = egui::Context::default();
                let mut state = WelcomeState::default();
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, height),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        ui.set_min_size(egui::vec2(width, height));
                        assert!(
                            state
                                .show(ui, &Localizer::new(language), &[], Some(&[]))
                                .is_empty()
                        );
                    },
                );
                let texts: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => {
                            Some((text.galley.text(), text.pos, text.galley.rows.len()))
                        }
                        _ => None,
                    })
                    .collect();
                for key in [
                    "welcome-new",
                    "welcome-open",
                    "welcome-templates",
                    "welcome-recents",
                    "welcome-language",
                    "welcome-preferences",
                ] {
                    let label = Localizer::new(language).text(key);
                    assert!(
                        texts.iter().any(|(text, _, _)| text.contains(&label)),
                        "missing {key} at {width}x{height}: {texts:?}"
                    );
                }
                if width == 1100.0 {
                    let label = Localizer::new(language).text("welcome-language");
                    let (_, position, lines) =
                        texts.iter().find(|(text, _, _)| text == &label).unwrap();
                    assert!(
                        position.x < SIDEBAR_WIDTH,
                        "footer escaped sidebar: {position:?}"
                    );
                    assert!(
                        position.y > 600.0,
                        "footer not bottom anchored: {position:?}"
                    );
                    assert!(*lines <= 2, "language label wrapped into {lines} lines");
                    let preferences = Localizer::new(language).text("welcome-preferences");
                    let (_, preferences_position, _) = texts
                        .iter()
                        .find(|(text, _, _)| text.contains(&preferences))
                        .unwrap();
                    assert!(position.y < preferences_position.y, "footer order reversed");
                    let recents = Localizer::new(language).text("welcome-recents");
                    let (_, position, _) =
                        texts.iter().find(|(text, _, _)| text == &recents).unwrap();
                    assert!(
                        position.x >= SIDEBAR_WIDTH && position.x < SIDEBAR_WIDTH + 60.0,
                        "recents escaped content: {position:?}"
                    );
                    let tiles: Vec<_> = [
                        "template-setup-base",
                        "template-setup-wall",
                        "template-setup-drawers",
                    ]
                    .iter()
                    .map(|key| {
                        let label = Localizer::new(language).text(key);
                        texts.iter().find(|(text, _, _)| text == &label).unwrap().1
                    })
                    .collect();
                    assert!(
                        tiles.windows(2).all(
                            |pair| pair[0].x < pair[1].x && (pair[0].y - pair[1].y).abs() < 2.0
                        ),
                        "template tiles not in a row: {tiles:?}"
                    );
                }
                output.textures_delta.clear();
            }
        }
    }
}
