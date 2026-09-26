//! Desktop document lifecycle. Picker results are tagged with the originating
//! project and revision; no worker can replace an editor after it has moved on.
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui;
use fluent_bundle::FluentArgs;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence::{self, SaveError};
use plan_my_cabinet::recovery::{RecoveryCandidate, RecoveryChoice, RecoveryStore};
use uuid::Uuid;

use crate::{DesktopApp, viewport};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum NextAction {
    New,
    Open,
    Close,
}

pub(super) enum Prompt {
    Dirty(NextAction),
    Overwrite(PathBuf, Option<NextAction>),
    Recovery(Box<RecoveryCandidate>),
}

#[derive(Clone, Copy)]
enum PickerKind {
    Open,
    Save(Option<NextAction>),
}

struct Picker {
    kind: PickerKind,
    project_id: Uuid,
    revision: u64,
    result: Receiver<Option<PathBuf>>,
}

#[derive(Default)]
pub(super) struct ProjectFiles {
    pub(super) path: Option<PathBuf>,
    pub(super) user_data_dir: Option<PathBuf>,
    store: Option<RecoveryStore>,
    pub(super) prompt: Option<Prompt>,
    picker: Option<Picker>,
    pub(super) pending_open: Option<(PathBuf, ProjectEditor)>,
    message: Option<String>,
    observed: Option<(Uuid, u64)>,
    pub(super) allow_close: bool,
}

impl ProjectFiles {
    pub(super) fn blocking(&self) -> bool {
        self.prompt.is_some() || self.picker.is_some()
    }
}

fn user_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support/Plan My Cabinet"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .map(|base| base.join("plan-my-cabinet"))
    }
}

impl DesktopApp {
    fn busy_for_project(&self) -> bool {
        self.modal_open()
            || self.export_activity.is_some()
            || self.optimizer.running()
            || self.export_preparation_pending.is_some()
            || self.editor.preview().is_some()
    }

    pub(super) fn request_project_action(&mut self, action: NextAction) {
        if self.project_files.allow_close || self.busy_for_project() {
            return;
        }
        if action == NextAction::Open && self.project_files.pending_open.is_none() {
            self.choose_file(PickerKind::Open);
            return;
        }
        if self.editor.is_dirty() {
            self.project_files.prompt = Some(Prompt::Dirty(action));
        } else {
            self.proceed(action);
        }
    }

    pub(super) fn proceed(&mut self, action: NextAction) {
        match action {
            NextAction::New => {
                self.replace_project(
                    ProjectEditor::new(Project::new(
                        self.localizer.text("project-default-name"),
                        Currency::Brl,
                    ))
                    .expect("empty project"),
                    None,
                );
                self.project_files.message = None;
            }
            NextAction::Open => {
                if let Some((path, editor)) = self.project_files.pending_open.take() {
                    self.replace_project(editor, Some(path));
                    self.project_files.message = None;
                    self.attach_recovery(true);
                }
            }
            NextAction::Close => {
                self.project_files.allow_close = true;
                // A window close may originate from the OS rather than a button.
                // Send a fresh close after resolving the prompt.
            }
        }
    }

    fn choose_file(&mut self, kind: PickerKind) {
        let (tx, rx) = mpsc::channel();
        // Construct on the UI thread for the native window association. Await
        // the platform dialog on a worker, never blocking an egui frame.
        let dialog =
            rfd::AsyncFileDialog::new().add_filter("Plan My Cabinet", &[persistence::EXTENSION]);
        let save_name = format!(
            "{}.pmcab",
            self.editor.project().name.replace(['/', '\\'], "_")
        );
        self.project_files.picker = Some(Picker {
            kind,
            project_id: self.editor.project().id,
            revision: self.editor.project().revision,
            result: rx,
        });
        std::thread::spawn(move || {
            let path = match kind {
                PickerKind::Open => pollster::block_on(dialog.pick_file()),
                PickerKind::Save(_) => {
                    pollster::block_on(dialog.set_file_name(&save_name).save_file())
                }
            }
            .map(|handle| handle.path().to_path_buf());
            let _ = tx.send(path);
        });
    }

    pub(super) fn save_to(&mut self, path: &Path, replacing: bool, after: Option<NextAction>) {
        let result = if replacing {
            persistence::save(&mut self.editor, path)
        } else {
            persistence::save_new(&mut self.editor, path)
        };
        match result {
            Ok(()) => {
                self.project_files.path = Some(path.to_path_buf());
                self.project_files.store = None;
                self.project_files.observed = None;
                self.attach_recovery(false);
                self.project_files.message = Some(self.localizer.text("project-saved"));
                if let Some(action) = after {
                    self.proceed(action);
                }
            }
            Err(SaveError::Io(ref error))
                if error.kind() == std::io::ErrorKind::AlreadyExists && !replacing =>
            {
                self.project_files.prompt = Some(Prompt::Overwrite(path.to_path_buf(), after));
            }
            Err(error) => {
                let mut args = FluentArgs::new();
                args.set("path", path.display().to_string());
                args.set("reason", error.to_string());
                self.project_files.message = Some(self.localizer.format("error-save", Some(&args)));
                if let Some(action) = after {
                    self.project_files.prompt = Some(Prompt::Dirty(action));
                }
            }
        }
    }

    pub(super) fn open_path(&mut self, path: PathBuf) {
        match File::open(&path)
            .map_err(persistence::PersistenceError::Io)
            .and_then(persistence::prepare_reader)
        {
            Ok(prepared) => {
                self.project_files.pending_open = Some((path, prepared.into_editor()));
                self.request_project_action(NextAction::Open);
            }
            Err(error) => {
                let mut args = FluentArgs::new();
                args.set("path", path.display().to_string());
                args.set("reason", error.to_string());
                self.project_files.message = Some(self.localizer.format("error-open", Some(&args)));
            }
        }
    }

    fn replace_project(&mut self, editor: ProjectEditor, path: Option<PathBuf>) {
        self.editor = editor;
        self.project_files.path = path;
        self.project_files.store = None;
        self.project_files.observed = None;
        self.selection = viewport::Selection::default();
        self.move_tool = viewport::MoveTool::default();
        self.camera = viewport::Camera::default();
        self.measurement_frame = plan_my_cabinet::measurements::Frame::World;
        self.door_motion = None;
        self.first_fit_notice = None;
        self.material_conflicts.clear();
        self.catalog_update_notice = None;
        self.allocation_diagnostics = None;
        self.export_preparation = None;
        self.export_preparation_pending = None;
        self.export_message = None;
        self.optimizer = Default::default();
        self.sheet_repair = Default::default();
    }

    fn attach_recovery(&mut self, inspect: bool) {
        let Some(path) = &self.project_files.path else {
            return;
        };
        let Some(dir) = self
            .project_files
            .user_data_dir
            .clone()
            .or_else(user_data_dir)
        else {
            self.project_files.message = Some(self.localizer.text("project-recovery-unavailable"));
            return;
        };
        match RecoveryStore::new(&dir, path, self.editor.project().id) {
            Ok(store) => {
                if inspect {
                    match store.inspect(self.editor.project()) {
                        Ok(Some(candidate)) => {
                            self.project_files.prompt = Some(Prompt::Recovery(Box::new(candidate)));
                            // Do not overwrite a deferred candidate in this session.
                        }
                        Ok(None) => self.project_files.store = Some(store),
                        Err(error) => {
                            self.project_files.message = Some(format!(
                                "{}: {error}",
                                self.localizer.text("project-recovery-unavailable")
                            ))
                        }
                    }
                } else {
                    self.project_files.store = Some(store);
                }
            }
            Err(error) => {
                self.project_files.message = Some(format!(
                    "{}: {error}",
                    self.localizer.text("project-recovery-unavailable")
                ))
            }
        }
    }

    pub(super) fn tick_project_files(&mut self, ctx: &egui::Context) {
        if let Some(picker) = &self.project_files.picker {
            let result = picker.result.try_recv();
            if let Ok(path) = result {
                let picker = self.project_files.picker.take().unwrap();
                if picker.project_id == self.editor.project().id
                    && picker.revision == self.editor.project().revision
                {
                    if let Some(path) = path {
                        match picker.kind {
                            PickerKind::Open => self.open_path(path),
                            PickerKind::Save(after) => {
                                if self.project_files.path.as_deref() == Some(path.as_path()) {
                                    self.save_to(&path, true, after);
                                } else if path.symlink_metadata().is_ok() {
                                    self.project_files.prompt =
                                        Some(Prompt::Overwrite(path, after));
                                } else {
                                    self.save_to(&path, false, after);
                                }
                            }
                        }
                    } else {
                        if matches!(picker.kind, PickerKind::Save(Some(NextAction::Open))) {
                            self.project_files.pending_open = None;
                        }
                        self.project_files.message = Some(self.localizer.text("project-cancelled"));
                    }
                }
            } else if result == Err(mpsc::TryRecvError::Disconnected) {
                self.project_files.picker = None;
                self.project_files.message = Some(self.localizer.text("project-cancelled"));
            } else {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }
        let key = (self.editor.project().id, self.editor.project().revision);
        if let Some(store) = &mut self.project_files.store {
            let now = Instant::now();
            if self.project_files.observed != Some(key) {
                self.project_files.observed = Some(key);
                if let Err(error) = store.note_committed_edit(&self.editor, now) {
                    self.project_files.message = Some(error.to_string());
                }
            }
            if self.editor.is_dirty() {
                if let Err(error) = store.tick(&self.editor, now) {
                    self.project_files.message = Some(error.to_string());
                }
                ctx.request_repaint_after(Duration::from_secs(1));
            }
        }
    }

    pub(super) fn show_project_controls(&mut self, ui: &mut egui::Ui) {
        ui.label(format!(
            "{}{}{}",
            self.editor.project().name,
            if self.editor.is_dirty() { " *" } else { "" },
            self.project_files
                .path
                .as_ref()
                .map(|p| format!(" — {}", p.display()))
                .unwrap_or_default()
        ));
        ui.horizontal_wrapped(|ui| {
            for (action, key) in [
                (NextAction::New, "project-new"),
                (NextAction::Open, "project-open"),
            ] {
                if ui
                    .add_enabled(
                        !self.busy_for_project(),
                        egui::Button::new(self.localizer.text(key)),
                    )
                    .clicked()
                {
                    self.request_project_action(action);
                }
            }
            if ui
                .add_enabled(
                    !self.busy_for_project(),
                    egui::Button::new(self.localizer.text("project-save")),
                )
                .clicked()
            {
                if let Some(path) = self.project_files.path.clone() {
                    self.save_to(&path, true, None);
                } else {
                    self.choose_file(PickerKind::Save(None));
                }
            }
            if ui
                .add_enabled(
                    !self.busy_for_project(),
                    egui::Button::new(self.localizer.text("project-save-as")),
                )
                .clicked()
            {
                self.choose_file(PickerKind::Save(None));
            }
        });
        if let Some(message) = &self.project_files.message {
            ui.label(message);
        }
        if self.project_files.picker.is_some() {
            ui.label(self.localizer.text("project-choosing"));
        }
    }

    pub(super) fn show_project_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.project_files.prompt.take() else {
            return;
        };
        let mut choice = None;
        let modal =
            egui::Modal::new(egui::Id::new("project-file-dialog")).show(ctx, |ui| match &prompt {
                Prompt::Dirty(action) => {
                    ui.heading(self.localizer.text("project-unsaved-title"));
                    ui.label(format!(
                        "{}: {}",
                        self.localizer.text("project-unsaved-detail"),
                        self.editor.project().name
                    ));
                    ui.label(self.localizer.text(match action {
                        NextAction::New => "project-new",
                        NextAction::Open => "project-open",
                        NextAction::Close => "project-close",
                    }));
                    for key in ["project-save", "project-discard", "cancel"] {
                        if ui.button(self.localizer.text(key)).clicked() {
                            choice = Some(key);
                        }
                    }
                }
                Prompt::Overwrite(path, _) => {
                    ui.heading(self.localizer.text("project-overwrite"));
                    ui.label(path.display().to_string());
                    for key in ["project-replace", "cancel"] {
                        if ui.button(self.localizer.text(key)).clicked() {
                            choice = Some(key);
                        }
                    }
                }
                Prompt::Recovery(candidate) => {
                    ui.heading(self.localizer.text("project-recovery-title"));
                    ui.label(format!(
                        "{} — {}",
                        candidate.project_name,
                        candidate.saved_path.display()
                    ));
                    ui.label(format!(
                        "{}: {} · {}: {}",
                        self.localizer.text("project-saved-revision"),
                        candidate.saved_revision,
                        self.localizer.text("project-recovery-revision"),
                        candidate.recovery_revision
                    ));
                    for key in [
                        "project-recover",
                        "project-recovery-discard",
                        "project-defer",
                    ] {
                        if ui.button(self.localizer.text(key)).clicked() {
                            choice = Some(key);
                        }
                    }
                }
            });
        if modal.should_close() && choice.is_none() {
            choice = Some("cancel");
        }
        match (prompt, choice) {
            (prompt, None) => self.project_files.prompt = Some(prompt),
            (Prompt::Dirty(action), Some("project-save")) => {
                if let Some(path) = self.project_files.path.clone() {
                    self.save_to(&path, true, Some(action));
                } else {
                    self.choose_file(PickerKind::Save(Some(action)));
                }
            }
            (Prompt::Dirty(action), Some("project-discard")) => self.proceed(action),
            (Prompt::Overwrite(path, after), Some("project-replace")) => {
                self.save_to(&path, true, after)
            }
            (Prompt::Recovery(candidate), Some(key)) if key != "cancel" => {
                let choice = match key {
                    "project-recover" => RecoveryChoice::Recover,
                    "project-recovery-discard" => RecoveryChoice::Discard,
                    _ => RecoveryChoice::Defer,
                };
                if let Some(path) = self.project_files.path.clone()
                    && let Some(dir) = self
                        .project_files
                        .user_data_dir
                        .clone()
                        .or_else(user_data_dir)
                {
                    let defer = matches!(choice, RecoveryChoice::Defer);
                    match RecoveryStore::new(&dir, &path, self.editor.project().id)
                        .map_err(|e| e.to_string())
                        .and_then(|store| {
                            candidate
                                .resolve(&store, choice)
                                .map_err(|e| e.to_string())
                                .map(|editor| (store, editor))
                        }) {
                        Ok((store, editor)) => {
                            if let Some(editor) = editor {
                                self.replace_project(editor, Some(path));
                            }
                            if !defer {
                                self.project_files.store = Some(store);
                            }
                        }
                        Err(error) => self.project_files.message = Some(error),
                    }
                }
            }
            (Prompt::Recovery(candidate), _) => {
                self.project_files.prompt = Some(Prompt::Recovery(candidate))
            }
            (Prompt::Dirty(NextAction::Open), _) => self.project_files.pending_open = None,
            (Prompt::Overwrite(_, Some(NextAction::Open)), _) => {
                self.project_files.pending_open = None
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::units::Length;

    #[test]
    fn stale_picker_result_and_cancellation_preserve_current_editor() {
        let mut app = DesktopApp::default();
        let original = app.editor.project().clone();
        let (tx, rx) = mpsc::channel();
        app.project_files.picker = Some(Picker {
            kind: PickerKind::Save(None),
            project_id: original.id,
            revision: original.revision,
            result: rx,
        });
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        tx.send(Some(PathBuf::from("ignored.pmcab"))).unwrap();
        app.tick_project_files(&egui::Context::default());
        assert!(app.project_files.picker.is_none());
        assert!(app.project_files.path.is_none());
        assert!(app.editor.is_dirty());
        assert_eq!(app.editor.project().grid_spacing.micrometres(), 20_000);
        let (tx, rx) = mpsc::channel();
        app.project_files.picker = Some(Picker {
            kind: PickerKind::Open,
            project_id: original.id,
            revision: app.editor.project().revision,
            result: rx,
        });
        tx.send(None).unwrap();
        app.tick_project_files(&egui::Context::default());
        assert!(app.editor.is_dirty());
        assert!(app.project_files.pending_open.is_none());
    }
}
