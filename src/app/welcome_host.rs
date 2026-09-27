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
                self.template_setup = Some(TemplateSetupUi::new(
                    kind,
                    self.localizer.text("project-default-name"),
                    self.editor.project().currency,
                    self.editor.project().display_unit,
                ));
                self.template_message = None;
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
            WelcomeIntent::Preferences => self.settings_open = true,
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
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc;

    use plan_my_cabinet::commands::ProjectEditor;
    use plan_my_cabinet::domain::Project;
    use plan_my_cabinet::money::Currency;
    use plan_my_cabinet::persistence;
    use plan_my_cabinet::recovery::{AUTOSAVE_DELAY, write_untitled_snapshot};
    use plan_my_cabinet::units::Length;
    use std::time::Instant;

    #[test]
    fn saved_recovery_deferral_and_guarded_recover_preserve_original_bytes() {
        let dir = Directory::new();
        let (path, id) = dir.save("Saved");
        let original_bytes = fs::read(&path).unwrap();
        let local = dir.0.join("app-data");
        let mut index = RecoveryIndex::open(&local).unwrap();
        index.register_saved(&path, id).unwrap();
        let mut saved = persistence::prepare_bytes(&original_bytes)
            .unwrap()
            .into_editor();
        saved
            .transact(|project| -> Result<(), ()> {
                project.name = "Recovered edits".into();
                Ok(())
            })
            .unwrap();
        let mut store = RecoveryStore::new(&local, &path, id).unwrap();
        let now = Instant::now();
        store.note_committed_edit(&saved, now).unwrap();
        store.tick(&saved, now + AUTOSAVE_DELAY).unwrap();
        let row = index.discover().pop().unwrap();
        assert!(matches!(row.status, DiscoveryStatus::Newer));
        let snapshot_bytes = fs::read(&row.snapshot_path).unwrap();
        let choice = |action| WelcomeIntent::Recovery {
            identity: row.identity.clone(),
            snapshot_path: row.snapshot_path.clone(),
            action,
        };
        let mut app = dir.app();
        app.handle_welcome_intent(choice(RecoveryAction::DecideLater));
        assert_eq!(fs::read(&row.snapshot_path).unwrap(), snapshot_bytes);
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        let prior = app.editor.project().clone();
        app.handle_welcome_intent(choice(RecoveryAction::Recover));
        assert!(matches!(
            app.project_files.prompt,
            Some(project_ui::Prompt::Dirty(
                project_ui::NextAction::RecoverFromWelcome
            ))
        ));
        assert_eq!(app.editor.project(), &prior);
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("cancel"));
        assert!(app.project_files.pending_recovery.is_none());
        assert_eq!(fs::read(&path).unwrap(), original_bytes);
        app.handle_welcome_intent(choice(RecoveryAction::Recover));
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("project-discard"));
        assert_eq!(app.editor.project().name, "Recovered edits");
        assert!(app.editor.is_dirty());
        assert_eq!(app.project_files.path.as_deref(), Some(path.as_path()));
        assert!(!app.project_files.welcome.visible);
        assert!(app.project_files.prompt.is_none());
        assert_eq!(fs::read(&path).unwrap(), original_bytes);
        assert_eq!(fs::read(&row.snapshot_path).unwrap(), snapshot_bytes);
    }

    #[test]
    fn untitled_recovery_requires_save_as_and_discard_touches_no_project() {
        let dir = Directory::new();
        let local = dir.0.join("app-data");
        let mut index = RecoveryIndex::open(&local).unwrap();
        let mut editor = ProjectEditor::new(Project::new("Untitled", Currency::Usd)).unwrap();
        let id = editor.project().id;
        index.register_untitled(id).unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.name = "Unsaved recovered work".into();
                Ok(())
            })
            .unwrap();
        write_untitled_snapshot(&index, &editor).unwrap();
        let row = index.discover().pop().unwrap();
        assert!(matches!(row.status, DiscoveryStatus::Untitled));
        let mut app = dir.app();
        app.handle_welcome_intent(WelcomeIntent::Recovery {
            identity: row.identity.clone(),
            snapshot_path: row.snapshot_path.clone(),
            action: RecoveryAction::Recover,
        });
        assert_eq!(app.editor.project().id, id);
        assert_eq!(app.editor.project().name, "Unsaved recovered work");
        assert!(app.editor.is_dirty());
        assert!(app.project_files.path.is_none());
        assert!(row.snapshot_path.exists());

        // A separate Welcome session explicitly discards only the snapshot.
        let mut other = dir.app();
        let old = other.editor.project().clone();
        other.handle_welcome_intent(WelcomeIntent::Recovery {
            identity: row.identity,
            snapshot_path: row.snapshot_path.clone(),
            action: RecoveryAction::Discard,
        });
        assert!(!row.snapshot_path.exists());
        assert_eq!(other.editor.project(), &old);
    }

    struct Directory(PathBuf);

    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("pmcab-welcome-host-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn app(&self) -> DesktopApp {
            let mut app = DesktopApp::default();
            app.project_files.user_data_dir = Some(self.0.join("app-data"));
            app
        }

        fn save(&self, name: &str) -> (PathBuf, Uuid) {
            let path = self.0.join(format!("{name}.pmcab"));
            let mut editor = ProjectEditor::new(Project::new(name, Currency::Usd)).unwrap();
            let id = editor.project().id;
            persistence::save(&mut editor, &path).unwrap();
            (fs::canonicalize(path).unwrap(), id)
        }

        fn recents(&self) -> RecentProjects {
            RecentProjects::open(&self.0.join("app-data")).unwrap()
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn welcome_caches_validated_metadata_and_filter_until_refresh() {
        let dir = Directory::new();
        let (path, id) = dir.save("Actual");
        let mut index = dir.recents();
        index
            .register_successful(&path, id, std::time::SystemTime::now())
            .unwrap();
        let mut host = WelcomeHost::default();
        host.refresh(Some(&dir.0.join("app-data")));
        assert!(
            matches!(&host.rows[0].status, RecentStatus::Available(summary) if summary.name == "Actual")
        );
        host.state.filter = "actual".into();
        assert_eq!(host.state.filtered(&host.rows).len(), 1);
        fs::remove_file(&path).unwrap();
        // No per-frame reads; the next entry/explicit invalidation refreshes.
        assert!(matches!(host.rows[0].status, RecentStatus::Available(_)));
        host.enter();
        host.refresh(Some(&dir.0.join("app-data")));
        assert!(matches!(host.rows[0].status, RecentStatus::Missing));
        assert_eq!(host.state.filtered(&host.rows).len(), 1); // last-known name
    }

    #[test]
    fn recent_open_preserves_dirty_work_until_prepared_load_is_accepted() {
        let dir = Directory::new();
        let (path, id) = dir.save("External");
        dir.recents()
            .register_successful(&path, id, std::time::SystemTime::now())
            .unwrap();
        let mut app = dir.app();
        let old_id = app.editor.project().id;
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        app.handle_welcome_intent(WelcomeIntent::OpenRecent {
            path: path.clone(),
            project_id: id,
        });
        assert_eq!(app.editor.project().id, old_id);
        assert!(app.project_files.pending_open.is_some());
        assert!(matches!(
            app.project_files.prompt,
            Some(project_ui::Prompt::Dirty(project_ui::NextAction::Open))
        ));
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("cancel"));
        assert_eq!(app.editor.project().id, old_id);
        assert!(app.editor.is_dirty());
        assert!(app.project_files.pending_open.is_none());

        // A row changed since the last render cannot open a different file.
        fs::write(&path, b"invalid").unwrap();
        app.handle_welcome_intent(WelcomeIntent::OpenRecent {
            path,
            project_id: id,
        });
        assert!(app.project_files.pending_open.is_none());
        assert_eq!(app.editor.project().id, old_id);
    }

    #[test]
    fn locate_rejects_wrong_identity_and_remove_never_deletes_project_or_recovery() {
        let dir = Directory::new();
        let (path, id) = dir.save("First");
        let (wrong, _) = dir.save("Other");
        let recovery = dir.0.join("snapshot.keep");
        fs::write(&recovery, b"recovery").unwrap();
        dir.recents()
            .register_successful(&path, id, std::time::SystemTime::now())
            .unwrap();
        let mut app = dir.app();
        let original = app.editor.project().id;
        let (tx, rx) = mpsc::channel();
        app.project_files.welcome.picker = Some(LocatePicker {
            path: path.clone(),
            id,
            source_id: original,
            source_revision: app.editor.project().revision,
            result: rx,
        });
        tx.send(Some(wrong.clone())).unwrap();
        app.poll_welcome_locate(&egui::Context::default());
        assert!(
            app.project_files
                .welcome
                .error
                .as_ref()
                .unwrap()
                .contains("not")
        );
        assert_eq!(dir.recents().entries()[0].path, path);
        assert_eq!(app.editor.project().id, original);

        app.handle_welcome_intent(WelcomeIntent::Remove {
            path: path.clone(),
            project_id: id,
        });
        assert!(dir.recents().entries().is_empty());
        assert!(path.exists());
        assert!(wrong.exists());
        assert_eq!(fs::read(&recovery).unwrap(), b"recovery");
    }

    #[test]
    fn locate_valid_copy_updates_only_the_index_and_stale_picker_cannot_update_it() {
        let dir = Directory::new();
        let (path, id) = dir.save("Moved");
        dir.recents()
            .register_successful(&path, id, std::time::SystemTime::now())
            .unwrap();
        let candidate = dir.0.join("relocated.pmcab");
        fs::copy(&path, &candidate).unwrap();
        fs::remove_file(&path).unwrap();
        let project_bytes = fs::read(&candidate).unwrap();
        let mut app = dir.app();
        let original = app.editor.project().id;
        let revision = app.editor.project().revision;
        let (tx, rx) = mpsc::channel();
        app.project_files.welcome.picker = Some(LocatePicker {
            path: path.clone(),
            id,
            source_id: original,
            source_revision: revision,
            result: rx,
        });
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        tx.send(Some(candidate.clone())).unwrap();
        app.poll_welcome_locate(&egui::Context::default());
        assert_eq!(dir.recents().entries()[0].path, path);

        let (tx, rx) = mpsc::channel();
        app.project_files.welcome.picker = Some(LocatePicker {
            path,
            id,
            source_id: original,
            source_revision: app.editor.project().revision,
            result: rx,
        });
        tx.send(Some(candidate.clone())).unwrap();
        app.poll_welcome_locate(&egui::Context::default());
        assert_eq!(
            dir.recents().entries()[0].path,
            fs::canonicalize(&candidate).unwrap()
        );
        assert!(app.project_files.welcome.picker.is_none());
        assert_eq!(app.editor.project().id, original);
        assert_eq!(fs::read(candidate).unwrap(), project_bytes);
    }

    #[test]
    fn returning_to_welcome_retains_unsaved_document_and_new_still_prompts() {
        let mut app = DesktopApp::default();
        app.proceed(project_ui::NextAction::New);
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        let id = app.editor.project().id;
        app.request_project_action(project_ui::NextAction::Welcome);
        assert!(app.project_files.welcome.visible);
        assert_eq!(app.editor.project().id, id);
        assert!(app.editor.is_dirty());
        app.handle_welcome_intent(WelcomeIntent::NewProject);
        assert!(matches!(
            app.project_files.prompt,
            Some(project_ui::Prompt::Dirty(project_ui::NextAction::New))
        ));
        assert_eq!(app.editor.project().id, id);
    }
}
