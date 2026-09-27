//! Presentation-only Welcome page. The host supplies a cached, validated recent
//! list and optional recovery discovery; rendering never reads or writes files.
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Color32, RichText};
use uuid::Uuid;

use crate::APPLICATION_NAME;
use crate::export::ExportStatus;
use crate::i18n::{Language, Localizer};
use crate::recent_projects::{RecentProjectView, RecentStatus, THUMBNAIL_SIZE};
use crate::recovery::{DiscoveryStatus, RecoveryDiscovery, RecoveryIdentity};
use crate::template_setup::TemplateKind;

const PANEL: Color32 = Color32::from_rgb(251, 250, 247);
const APP: Color32 = Color32::from_rgb(244, 241, 236);
const TEXT: Color32 = Color32::from_rgb(42, 37, 32);
const MUTED: Color32 = Color32::from_rgb(110, 101, 90);
const BORDER: Color32 = Color32::from_rgb(221, 215, 205);
const WARN: Color32 = Color32::from_rgb(138, 90, 18);
const SIDEBAR_WIDTH: f32 = 340.0;
const COMPACT_MIN_CONTENT: f32 = 390.0;
const PANE_PADDING: f32 = 24.0;

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
        egui::Frame::new().fill(APP).show(ui, |ui| {
            let width = ui.available_width();
            let height = ui.available_height();
            if wide_columns(width).is_none() {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_width(width);
                    egui::Frame::new()
                        .fill(PANEL)
                        .inner_margin(egui::Margin::symmetric(24, 20))
                        .show(ui, |ui| {
                            self.sidebar_actions(ui, localizer, &mut intents);
                            ui.add_space(16.0);
                            self.sidebar_footer(ui, localizer, &mut intents, false);
                        });
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(20, 16))
                        .show(ui, |ui| {
                            self.content(ui, localizer, recents, discoveries, &mut intents);
                        });
                });
            } else {
                let (sidebar_width, content_width) = wide_columns(width).expect("wide layout");
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(sidebar_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::Frame::new()
                                .fill(PANEL)
                                .stroke(egui::Stroke::new(1.0, BORDER))
                                .inner_margin(egui::Margin::symmetric(24, 20))
                                .show(ui, |ui| {
                                    ui.set_width(sidebar_width - 2.0 * PANE_PADDING);
                                    ui.set_min_height(height - 40.0);
                                    self.sidebar_actions(ui, localizer, &mut intents);
                                    ui.with_layout(
                                        egui::Layout::bottom_up(egui::Align::Min),
                                        |ui| {
                                            self.sidebar_footer(ui, localizer, &mut intents, true);
                                        },
                                    );
                                });
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(content_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(28, 20))
                                .show(ui, |ui| {
                                    ui.set_width(content_width - 56.0);
                                    self.content(ui, localizer, recents, discoveries, &mut intents);
                                });
                        },
                    );
                });
            }
        });
        intents
    }

    fn sidebar_actions(&self, ui: &mut egui::Ui, l: &Localizer, intents: &mut Vec<WelcomeIntent>) {
        ui.heading(APPLICATION_NAME);
        ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).color(MUTED));
        ui.add_space(18.0);
        if ui
            .add_sized(
                [ui.available_width(), 40.0],
                egui::Button::new(
                    RichText::new(format!("{}  {}", l.text("welcome-new"), shortcut("N")))
                        .color(PANEL),
                )
                .fill(TEXT),
            )
            .on_hover_text(l.text("welcome-new"))
            .clicked()
        {
            intents.push(WelcomeIntent::NewProject);
        }
        if ui
            .add_sized(
                [ui.available_width(), 40.0],
                egui::Button::new(format!("{}  {}", l.text("welcome-open"), shortcut("O"))),
            )
            .on_hover_text(l.text("welcome-open"))
            .clicked()
        {
            intents.push(WelcomeIntent::OpenProject);
        }
        ui.add_space(18.0);
        ui.label(
            RichText::new(l.text("welcome-templates"))
                .strong()
                .color(MUTED),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let tile_width = (ui.available_width() - 12.0) / 3.0;
            for (kind, key) in [
                (TemplateKind::Base, "template-setup-base"),
                (TemplateKind::Wall, "template-setup-wall"),
                (TemplateKind::Drawers, "template-setup-drawers"),
            ] {
                if ui
                    .add_sized([tile_width, 74.0], egui::Button::new(l.text(key)))
                    .clicked()
                {
                    intents.push(WelcomeIntent::Template(kind));
                }
            }
        });
    }

    fn sidebar_footer(
        &self,
        ui: &mut egui::Ui,
        l: &Localizer,
        intents: &mut Vec<WelcomeIntent>,
        bottom_up: bool,
    ) {
        if bottom_up {
            self.preferences_control(ui, l, intents);
            self.language_control(ui, l, intents);
        } else {
            self.language_control(ui, l, intents);
            self.preferences_control(ui, l, intents);
        }
    }

    fn language_control(&self, ui: &mut egui::Ui, l: &Localizer, intents: &mut Vec<WelcomeIntent>) {
        // Put the selector behind a menu instead of squeezing three long pt-BR
        // labels into the sidebar's narrow horizontal row.
        ui.horizontal(|ui| {
            ui.label(l.text("ui-language"));
            ui.menu_button(
                l.text(match l.language() {
                    Language::En => "language-en",
                    Language::PtBr => "language-pt-br",
                }),
                |ui| {
                    for language in [Language::En, Language::PtBr] {
                        let key = match language {
                            Language::En => "language-en",
                            Language::PtBr => "language-pt-br",
                        };
                        if ui
                            .selectable_label(l.language() == language, l.text(key))
                            .clicked()
                        {
                            intents.push(WelcomeIntent::SetLanguage(language));
                            ui.close();
                        }
                    }
                },
            );
        });
    }

    fn preferences_control(
        &self,
        ui: &mut egui::Ui,
        l: &Localizer,
        intents: &mut Vec<WelcomeIntent>,
    ) {
        if ui
            .button(format!(
                "{}  {}",
                l.text("welcome-preferences"),
                shortcut(",")
            ))
            .clicked()
        {
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
        self.recovery_cards(ui, l, discoveries, intents);
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.heading(l.text("welcome-recents"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text(l.text("welcome-filter"))
                        .desired_width(220.0),
                );
            });
        });
        ui.add_space(8.0);
        let rows = self.filtered(recents);
        egui::ScrollArea::vertical()
            .id_salt("welcome-recents")
            .max_height((ui.available_height() - 12.0).max(150.0))
            .show(ui, |ui| {
                if rows.is_empty() {
                    ui.label(l.text(if recents.is_empty() {
                        "welcome-empty"
                    } else {
                        "welcome-no-results"
                    }));
                }
                for row in rows {
                    egui::Frame::new()
                        .fill(PANEL)
                        .stroke(egui::Stroke::new(1.0, BORDER))
                        .corner_radius(7)
                        .inner_margin(10)
                        .show(ui, |ui| self.row(ui, l, row, discoveries, intents));
                    ui.add_space(5.0);
                }
            });
    }

    fn recovery_cards(
        &self,
        ui: &mut egui::Ui,
        l: &Localizer,
        discoveries: Option<&[RecoveryDiscovery]>,
        intents: &mut Vec<WelcomeIntent>,
    ) {
        ui.label(RichText::new(l.text("welcome-recovery-heading")).strong());
        let Some(discoveries) = discoveries else {
            ui.label(l.text("welcome-recovery-unknown"));
            return;
        };
        if discoveries.is_empty() {
            ui.label(l.text("welcome-recovery-none"));
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("welcome-recovery-cards")
            .max_height(260.0)
            .show(ui, |ui| {
                for discovery in discoveries {
                    egui::Frame::new()
                        .fill(PANEL)
                        .stroke(egui::Stroke::new(1.0, BORDER))
                        .corner_radius(10)
                        .inner_margin(14)
                        .show(ui, |ui| {
                            ui.set_min_width((ui.available_width() - 28.0).max(180.0));
                            ui.label(
                                RichText::new(l.text(recovery_status_key(&discovery.status)))
                                    .strong(),
                            );
                            ui.label(format!(
                                "{}: {}",
                                l.text("welcome-recovery-project-id"),
                                discovery.identity.project_id
                            ));
                            match &discovery.identity.saved_path {
                                Some(path) => {
                                    ui.add(
                                        egui::Label::new(format!(
                                            "{}: {}",
                                            l.text("welcome-recovery-saved-path"),
                                            path.display()
                                        ))
                                        .wrap(),
                                    );
                                    ui.label(format!(
                                        "{}: {}",
                                        l.text("welcome-recovery-saved-revision"),
                                        revision_label(discovery.saved_revision, l)
                                    ));
                                }
                                None => {
                                    ui.label(l.text("welcome-recovery-untitled-path"));
                                }
                            }
                            ui.label(format!(
                                "{}: {}",
                                l.text("welcome-recovery-snapshot-revision"),
                                revision_label(discovery.recovery_revision, l)
                            ));
                            ui.label(format!(
                                "{}: {}",
                                l.text("welcome-recovery-snapshot-date"),
                                modified_label(discovery.snapshot_modified, l)
                            ));
                            match &discovery.status {
                                DiscoveryStatus::SavedUnavailable(reason)
                                | DiscoveryStatus::Invalid(reason) => {
                                    ui.label(RichText::new(reason).color(WARN));
                                }
                                DiscoveryStatus::Newer | DiscoveryStatus::Untitled => {
                                    ui.label(l.text(if discovery.identity.saved_path.is_none() {
                                        "welcome-recovery-save-as"
                                    } else {
                                        "welcome-recovery-unsaved"
                                    }));
                                    ui.horizontal_wrapped(|ui| {
                                        for (action, key) in [
                                            (RecoveryAction::Recover, "welcome-recovery-recover"),
                                            (RecoveryAction::DecideLater, "welcome-recovery-defer"),
                                            (RecoveryAction::Discard, "welcome-recovery-discard"),
                                        ] {
                                            if ui.button(l.text(key)).clicked()
                                                && let Some(intent) =
                                                    recovery_intent(discovery, action)
                                            {
                                                intents.push(intent);
                                            }
                                        }
                                    });
                                }
                                DiscoveryStatus::NotNewer => {}
                            }
                        });
                    ui.add_space(6.0);
                }
            });
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
        ui.horizontal(|ui| {
            if let (Some(key), Some(rgba)) = (&row.entry.thumbnail_key, &row.thumbnail) {
                if self.thumbnails.len() >= 128 && !self.thumbnails.contains_key(key) {
                    self.thumbnails.clear();
                }
                let image = self.thumbnails.entry(key.clone()).or_insert_with(|| {
                    ui.ctx().load_texture(
                        format!("recent-{key}"),
                        egui::ColorImage::from_rgba_unmultiplied(THUMBNAIL_SIZE, rgba),
                        egui::TextureOptions::LINEAR,
                    )
                });
                ui.image((image.id(), egui::vec2(96.0, 60.0)));
            } else {
                egui::Frame::new()
                    .fill(APP)
                    .stroke(egui::Stroke::new(1.0, BORDER))
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(96.0, 60.0),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.label(RichText::new("3D").monospace().color(MUTED));
                            },
                        );
                    });
            }
            ui.vertical(|ui| {
                ui.set_max_width((ui.available_width() - 12.0).max(100.0));
                if let Some(intent) = recent_intent(row, RecentAction::Open) {
                    if ui
                        .button(RichText::new(name).strong())
                        .on_hover_text(l.text("welcome-open-recent"))
                        .clicked()
                    {
                        intents.push(intent);
                    }
                } else {
                    ui.label(RichText::new(name).strong().color(MUTED));
                    ui.small(l.text("welcome-cached-name"));
                }
                ui.label(
                    RichText::new(row.entry.path.display().to_string())
                        .monospace()
                        .size(11.0)
                        .color(MUTED),
                )
                .on_hover_text(row.entry.path.display().to_string());
                if let RecentStatus::Available(summary) = &row.status {
                    ui.label(format!(
                        "{} · {} · {} · {}",
                        l.count("boards-count", summary.boards as u64),
                        l.count("welcome-assemblies-count", summary.assemblies as u64),
                        l.count("welcome-materials-count", summary.materials as u64),
                        l.count("welcome-stock-count", summary.stock as u64)
                    ));
                }
                ui.label(
                    RichText::new(format!(
                        "{}: {}",
                        l.text("welcome-last-used"),
                        last_used_label(row.entry.last_used_unix_ms, l)
                    ))
                    .small()
                    .color(MUTED),
                );
                match &row.status {
                    RecentStatus::Available(_) => {
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                l.text(export_label(row.export_status)),
                                l.text(recovery_label(recovery_badge(row, discoveries)))
                            ))
                            .small(),
                        );
                    }
                    RecentStatus::Missing => {
                        ui.label(RichText::new(l.text("welcome-missing")).color(WARN));
                        ui.small(l.text(recovery_label(recovery_badge(row, discoveries))));
                    }
                    RecentStatus::Unavailable(reason) => {
                        ui.label(
                            RichText::new(format!("{}: {reason}", l.text("welcome-unavailable")))
                                .color(WARN),
                        );
                        ui.small(l.text(recovery_label(recovery_badge(row, discoveries))));
                    }
                }
                self.row_actions(ui, l, row, intents);
            });
        });
    }

    fn row_actions(
        &self,
        ui: &mut egui::Ui,
        l: &Localizer,
        row: &RecentProjectView,
        intents: &mut Vec<WelcomeIntent>,
    ) {
        if let Some(intent) = recent_intent(row, RecentAction::Locate)
            && ui.button(l.text("welcome-locate")).clicked()
        {
            intents.push(intent);
        }
        if let Some(intent) = recent_intent(row, RecentAction::Remove)
            && ui.button(l.text("welcome-remove")).clicked()
        {
            intents.push(intent);
        }
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

fn recovery_status_key(status: &DiscoveryStatus) -> &'static str {
    match status {
        DiscoveryStatus::Newer => "welcome-recovery-newer",
        DiscoveryStatus::Untitled => "welcome-recovery-untitled",
        DiscoveryStatus::NotNewer => "welcome-recovery-not-newer",
        DiscoveryStatus::SavedUnavailable(_) => "welcome-recovery-saved-unavailable",
        DiscoveryStatus::Invalid(_) => "welcome-recovery-invalid",
    }
}

fn revision_label(revision: Option<u64>, l: &Localizer) -> String {
    revision.map_or_else(
        || l.text("welcome-recovery-value-unknown"),
        |revision| revision.to_string(),
    )
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
    time::OffsetDateTime::from_unix_timestamp(ms / 1000)
        .map(|date| date.date().to_string())
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
                    "ui-language",
                    "welcome-preferences",
                ] {
                    let label = Localizer::new(language).text(key);
                    assert!(
                        texts.iter().any(|(text, _, _)| text.contains(&label)),
                        "missing {key} at {width}x{height}: {texts:?}"
                    );
                }
                if width == 1100.0 {
                    let label = Localizer::new(language).text("ui-language");
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
