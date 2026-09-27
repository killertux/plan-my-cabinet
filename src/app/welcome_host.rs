//! Desktop host for the presentation-only Welcome page. Keep validated list
//! reads out of the frame loop and route project replacement through project_ui.
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use crate::template_setup_ui::TemplateSetupUi;
use eframe::egui;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::recent_projects::{RecentProjectView, RecentProjects, RecentStatus};
use plan_my_cabinet::recovery::{
    DiscoveryStatus, RecoveryChoice, RecoveryDiscovery, RecoveryIdentity, RecoveryIndex,
    RecoveryStore,
};
use plan_my_cabinet::welcome_ui::{RecoveryAction, WelcomeIntent, WelcomeState};
use uuid::Uuid;

use crate::{DesktopApp, project_ui};

struct LocatePicker {
    path: PathBuf,
    id: Uuid,
    source_id: Uuid,
    source_revision: u64,
    result: Receiver<Option<PathBuf>>,
}

pub(crate) struct WelcomeHost {
    pub(crate) visible: bool,
    state: WelcomeState,
    rows: Vec<RecentProjectView>,
    discoveries: Option<Vec<RecoveryDiscovery>>,
    loaded: bool,
    error: Option<String>,
    picker: Option<LocatePicker>,
}

impl Default for WelcomeHost {
    fn default() -> Self {
        Self {
            visible: true,
            state: WelcomeState::default(),
            rows: Vec::new(),
            discoveries: None,
            loaded: false,
            error: None,
            picker: None,
        }
    }
}

impl WelcomeHost {
    pub(crate) fn locating(&self) -> bool {
        self.picker.is_some()
    }

    pub(crate) fn enter(&mut self) {
        self.visible = true;
        self.invalidate();
    }

    pub(crate) fn leave(&mut self) {
        self.visible = false;
        self.picker = None;
    }

    pub(crate) fn invalidate(&mut self) {
        self.loaded = false;
    }

    fn refresh(&mut self, dir: Option<&Path>) {
        self.loaded = true;
        self.rows.clear();
        self.discoveries = None;
        self.error = None;
        let Some(dir) = dir else {
            self.error = Some("Application user-data directory unavailable".into());
            return;
        };
        match RecentProjects::open(dir) {
            Ok(recents) => self.rows = recents.list(""),
            Err(error) => self.error = Some(format!("Recent projects: {error}")),
        }
        match RecoveryIndex::open(dir) {
            Ok(index) => self.discoveries = Some(index.discover()),
            Err(error) => {
                let detail = format!("Recovery index: {error}");
                self.error = Some(
                    self.error
                        .take()
                        .map_or(detail.clone(), |previous| format!("{previous} · {detail}")),
                );
            }
        }
    }
}

impl DesktopApp {
    /// Call instead of the workspace surface while Welcome is visible. The
    /// normal project dialog, navigation prompt and Settings layers must still
    /// be drawn by the main host after this call.
    pub(crate) fn show_welcome(&mut self, ui: &mut egui::Ui) {
        self.tick_project_files(ui.ctx());
        self.poll_welcome_locate(ui.ctx());
        if !self.project_files.welcome.loaded {
            let dir = self
                .project_files
                .user_data_dir
                .clone()
                .or_else(project_ui::user_data_dir);
            self.project_files.welcome.refresh(dir.as_deref());
        }
        let mut intents = Vec::new();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(crate::theme_widgets::APP))
            .show(ui, |ui| {
                let welcome = &mut self.project_files.welcome;
                intents = welcome.state.show(
                    ui,
                    &self.localizer,
                    &welcome.rows,
                    welcome.discoveries.as_deref(),
                );
            });
        // Transient status floats over the recent list instead of pushing the
        // layout down: errors in a warning callout, progress/info plainly.
        let welcome = &self.project_files.welcome;
        let mut notes: Vec<(String, bool)> = Vec::new();
        if let Some(error) = &welcome.error {
            notes.push((error.clone(), true));
        }
        if welcome.locating() {
            notes.push((self.localizer.text("project-choosing"), false));
        }
        if let Some(message) = &self.project_files.message {
            notes.push((message.clone(), false));
        }
        if !notes.is_empty() {
            let offset_x = if ui.ctx().content_rect().width() >= 730.0 {
                170.0
            } else {
                0.0
            };
            egui::Area::new(egui::Id::new("welcome-status-notes"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(offset_x, -20.0))
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    ui.set_max_width(560.0);
                    for (text, error) in notes {
                        let frame = if error {
                            crate::theme_widgets::warn_callout()
                        } else {
                            crate::theme_widgets::floating_frame()
                                .inner_margin(egui::Margin::symmetric(12, 8))
                        };
                        frame.show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(text).size(12.0).color(
                                    if error {
                                        crate::theme_widgets::WARN_INK
                                    } else {
                                        crate::theme_widgets::SECONDARY
                                    },
                                ))
                                .wrap(),
                            );
                        });
                        ui.add_space(6.0);
                    }
                });
        }
        if !self.project_files.blocking() && !self.modal_open() {
            for intent in intents {
                self.handle_welcome_intent(intent);
            }
        }
    }

    pub(crate) fn handle_welcome_intent(&mut self, intent: WelcomeIntent) {
        match intent {
            WelcomeIntent::NewProject => self.request_project_action(project_ui::NextAction::New),
            WelcomeIntent::Template(kind) => {
                self.template.setup = Some(TemplateSetupUi::new(
                    kind,
                    self.localizer.text("project-default-name"),
                    self.editor.project().currency,
                    self.editor.project().display_unit,
                ));
                self.template.message = None;
            }
            WelcomeIntent::OpenProject => self.request_project_action(project_ui::NextAction::Open),
            WelcomeIntent::OpenRecent { path, project_id } => {
                let dir = self
                    .project_files
                    .user_data_dir
                    .clone()
                    .or_else(project_ui::user_data_dir);
                let valid = dir
                    .as_deref()
                    .ok_or_else(|| "Application user-data directory unavailable".to_owned())
                    .and_then(|dir| RecentProjects::open(dir).map_err(|error| error.to_string()))
                    .map(|recents| {
                        recents.list("").iter().any(|row| {
                            row.entry.path == path
                                && row.entry.project_id == project_id
                                && matches!(row.status, RecentStatus::Available(_))
                        })
                    });
                match valid {
                    Ok(true) => {
                        self.project_files.welcome.error = None;
                        self.open_path_checked(path, Some(project_id));
                    }
                    Ok(false) => {
                        self.project_files.welcome.error =
                            Some("Recent project changed; refresh or locate its file".into());
                        self.project_files.welcome.invalidate();
                    }
                    Err(error) => self.project_files.welcome.error = Some(error),
                }
            }
            WelcomeIntent::Locate { path, project_id } => {
                self.choose_welcome_locate(path, project_id)
            }
            WelcomeIntent::Remove { path, project_id } => {
                let dir = self
                    .project_files
                    .user_data_dir
                    .clone()
                    .or_else(project_ui::user_data_dir);
                let result = dir
                    .as_deref()
                    .ok_or_else(|| "Application user-data directory unavailable".to_owned())
                    .and_then(|dir| {
                        RecentProjects::open(dir)
                            .and_then(|mut recents| recents.remove(&path, project_id))
                            .map_err(|error| error.to_string())
                    });
                self.project_files.welcome.error = result.err();
                if self.project_files.welcome.error.is_none() {
                    self.project_files.welcome.invalidate();
                }
            }
            WelcomeIntent::Preferences => self.settings.open = true,
            WelcomeIntent::SetLanguage(language) => self.set_ui_language(language),
            WelcomeIntent::Recovery {
                identity,
                snapshot_path,
                action,
            } => {
                if let Err(error) = self.resolve_welcome_recovery(&identity, &snapshot_path, action)
                {
                    self.project_files.welcome.error = Some(error);
                    self.project_files.welcome.invalidate();
                }
            }
        }
    }

    fn resolve_welcome_recovery(
        &mut self,
        identity: &RecoveryIdentity,
        snapshot_path: &Path,
        action: RecoveryAction,
    ) -> Result<(), String> {
        let dir = self
            .project_files
            .user_data_dir
            .clone()
            .or_else(project_ui::user_data_dir)
            .ok_or_else(|| "Application user-data directory unavailable".to_owned())?;
        let index = RecoveryIndex::open(&dir).map_err(|error| error.to_string())?;
        let current = index
            .discover()
            .into_iter()
            .find(|row| {
                &row.identity == identity
                    && row.snapshot_path == snapshot_path
                    && matches!(
                        row.status,
                        DiscoveryStatus::Newer | DiscoveryStatus::Untitled
                    )
            })
            .ok_or_else(|| "Recovery candidate changed; refresh before deciding".to_owned())?;
        if action == RecoveryAction::DecideLater {
            self.project_files.welcome.error = None;
            return Ok(());
        }
        match (&identity.saved_path, action) {
            (Some(path), RecoveryAction::Recover | RecoveryAction::Discard) => {
                let candidate = index
                    .inspect_saved(path, identity.project_id)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "Recovery snapshot no longer exists".to_owned())?;
                if candidate.saved_path != *path || current.snapshot_path != snapshot_path {
                    return Err("Recovery identity changed".into());
                }
                let store = RecoveryStore::new(&dir, path, identity.project_id)
                    .map_err(|error| error.to_string())?;
                let choice = if action == RecoveryAction::Recover {
                    RecoveryChoice::Recover
                } else {
                    RecoveryChoice::Discard
                };
                let editor = candidate
                    .resolve(&store, choice)
                    .map_err(|error| error.to_string())?;
                if let Some(editor) = editor {
                    self.project_files.pending_recovery = Some((Some(path.clone()), editor));
                    self.request_project_action(project_ui::NextAction::RecoverFromWelcome);
                } else {
                    self.project_files.welcome.invalidate();
                }
            }
            (None, RecoveryAction::Recover) => {
                let project = index
                    .load_untitled(identity.project_id)
                    .map_err(|error| error.to_string())?;
                let editor = ProjectEditor::from_untitled_recovery(project)
                    .map_err(|error| format!("{error:?}"))?;
                self.project_files.pending_recovery = Some((None, editor));
                self.request_project_action(project_ui::NextAction::RecoverFromWelcome);
            }
            (None, RecoveryAction::Discard) => {
                let mut review = index.cleanup_review();
                review
                    .select(identity, snapshot_path)
                    .map_err(|error| error.to_string())?;
                let pending = review
                    .prepare_confirmation()
                    .ok_or_else(|| "No snapshot selected".to_owned())?;
                let token = pending.confirmation_token();
                let outcome = pending
                    .confirm(&index, token)
                    .map_err(|error| error.to_string())?;
                if let Some(failure) = outcome.failed.first() {
                    return Err(failure.error.clone());
                }
                self.project_files.welcome.invalidate();
            }
            (_, RecoveryAction::DecideLater) => unreachable!(),
        }
        self.project_files.welcome.error = None;
        Ok(())
    }

    fn choose_welcome_locate(&mut self, path: PathBuf, id: Uuid) {
        if self.project_files.blocking() || self.modal_open() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let dialog = rfd::AsyncFileDialog::new().add_filter(
            "Plan My Cabinet",
            &[plan_my_cabinet::persistence::EXTENSION],
        );
        self.project_files.welcome.picker = Some(LocatePicker {
            path,
            id,
            source_id: self.editor.project().id,
            source_revision: self.editor.project().revision,
            result: rx,
        });
        std::thread::spawn(move || {
            let path = pollster::block_on(dialog.pick_file()).map(|file| file.path().to_path_buf());
            let _ = tx.send(path);
        });
    }

    fn poll_welcome_locate(&mut self, ctx: &egui::Context) {
        let Some(picker) = &self.project_files.welcome.picker else {
            return;
        };
        match picker.result.try_recv() {
            Ok(candidate) => {
                let picker = self
                    .project_files
                    .welcome
                    .picker
                    .take()
                    .expect("pending locate");
                if !self.project_files.welcome.visible
                    || picker.source_id != self.editor.project().id
                    || picker.source_revision != self.editor.project().revision
                {
                    return;
                }
                if let Some(candidate) = candidate {
                    let dir = self
                        .project_files
                        .user_data_dir
                        .clone()
                        .or_else(project_ui::user_data_dir);
                    let result = dir
                        .as_deref()
                        .ok_or_else(|| "Application user-data directory unavailable".to_owned())
                        .and_then(|dir| {
                            RecentProjects::open(dir)
                                .and_then(|mut recents| {
                                    recents.locate(&picker.path, picker.id, &candidate)
                                })
                                .map_err(|error| error.to_string())
                        });
                    self.project_files.welcome.error = result.err();
                    if self.project_files.welcome.error.is_none() {
                        self.project_files.welcome.invalidate();
                    }
                }
            }
            Err(TryRecvError::Disconnected) => {
                self.project_files.welcome.picker = None;
                self.project_files.welcome.error = Some("Locate picker failed".into());
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(50)),
        }
    }
}

#[cfg(test)]
mod tests;
