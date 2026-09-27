//! Desktop document lifecycle. Picker results are tagged with the originating
//! project and revision; no worker can replace an editor after it has moved on.
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use fluent_bundle::FluentArgs;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence::{self, SaveError};
use plan_my_cabinet::recent_projects::RecentProjects;
use plan_my_cabinet::recovery::{
    AUTOSAVE_DELAY, RecoveryCandidate, RecoveryChoice, RecoveryError, RecoveryIndex, RecoveryStore,
    discard_untitled_snapshot, write_untitled_snapshot,
};
use uuid::Uuid;

use crate::{
    DesktopApp,
    actions::{self, ActionId as A, Request},
    icons, modal_chrome,
    modal_chrome::{ModalAction, ModalActions, ModalChrome, ModalThreeAction, ModalThreeActions},
    theme_widgets, viewport,
    welcome_host::WelcomeHost,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NextAction {
    New,
    Open,
    Close,
    Welcome,
    Template,
    RecoverFromWelcome,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PendingProjectCommand {
    Action(NextAction),
    Save(bool),
}

pub(crate) enum Prompt {
    Dirty(NextAction),
    Overwrite(PathBuf, Option<NextAction>),
    Upgrade(PathBuf, bool, Option<NextAction>),
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
pub(crate) struct ProjectFiles {
    pub(crate) path: Option<PathBuf>,
    pub(crate) user_data_dir: Option<PathBuf>,
    store: Option<RecoveryStore>,
    pub(crate) prompt: Option<Prompt>,
    prompt_chrome: Option<ModalChrome>,
    picker: Option<Picker>,
    pub(crate) pending_open: Option<(PathBuf, ProjectEditor)>,
    pub(crate) pending_recovery: Option<(Option<PathBuf>, ProjectEditor)>,
    pub(crate) message: Option<String>,
    observed: Option<(Uuid, u64)>,
    /// Untitled autosave: the revision waiting for the inactivity delay, and
    /// the last revision written.
    untitled_pending: Option<(u64, Instant)>,
    untitled_written: Option<u64>,
    pub(crate) allow_close: bool,
    pub(crate) welcome: WelcomeHost,
}

impl ProjectFiles {
    pub(crate) fn blocking(&self) -> bool {
        self.prompt.is_some() || self.picker.is_some() || self.welcome.locating()
    }
}

pub(crate) fn user_data_dir() -> Option<PathBuf> {
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
    fn resolve_draft_before_project_command(&mut self, command: PendingProjectCommand) -> bool {
        if self.navigation.pending().is_some() {
            return true;
        }
        if self.navigation_edit().is_none() {
            return false;
        }
        match self.request_navigation(crate::pending_navigation::Route::Workspace(
            self.session.active,
        )) {
            crate::pending_navigation::Outcome::Prompt { .. } => {
                self.pending_project_command = Some(command);
                true
            }
            crate::pending_navigation::Outcome::Blocked(_) => true,
            crate::pending_navigation::Outcome::Navigated
            | crate::pending_navigation::Outcome::Stayed => false,
        }
    }

    pub(crate) fn busy_for_project(&self) -> bool {
        self.modal_open()
            || self.export_activity.is_some()
            || self.optimizer.running()
            || self.editor.preview().is_some()
    }

    pub(crate) fn request_project_action(&mut self, action: NextAction) {
        if self.project_files.allow_close {
            return;
        }
        if self.resolve_draft_before_project_command(PendingProjectCommand::Action(action)) {
            return;
        }
        if self.busy_for_project() {
            return;
        }
        if action == NextAction::Welcome {
            // Welcome retains the document. Its later New/Open requests still
            // run the existing dirty-document and prepared-load protections.
            self.project_files.welcome.enter();
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

    pub(crate) fn request_save(&mut self, save_as: bool) {
        if self.resolve_draft_before_project_command(PendingProjectCommand::Save(save_as)) {
            return;
        }
        if self.busy_for_project() {
            return;
        }
        if !save_as && let Some(path) = self.project_files.path.clone() {
            self.save_to(&path, true, None);
        } else {
            self.choose_file(PickerKind::Save(None));
        }
    }

    pub(crate) fn proceed(&mut self, action: NextAction) {
        match action {
            NextAction::New => {
                let mut project =
                    Project::new(self.localizer.text("project-default-name"), Currency::Brl);
                plan_my_cabinet::material_presets::seed_defaults(
                    &mut project,
                    self.localizer.language(),
                );
                self.replace_project(
                    ProjectEditor::new(project).expect("empty project"),
                    None,
                );
                self.project_files.message = None;
                self.project_files.welcome.leave();
            }
            NextAction::Open => {
                if let Some((path, editor)) = self.project_files.pending_open.take() {
                    self.replace_project(editor, Some(path));
                    self.project_files.message = None;
                    self.project_files.welcome.leave();
                    self.attach_recovery(true);
                    for error in self.register_successful_project() {
                        self.append_project_message(error);
                    }
                    self.project_files.welcome.invalidate();
                }
            }
            NextAction::Close => {
                if self.project_files.path.is_none() {
                    self.discard_untitled_recovery(self.editor.project().id);
                }
                self.project_files.allow_close = true;
                // A window close may originate from the OS rather than a button.
                // Send a fresh close after resolving the prompt.
            }
            NextAction::Welcome => self.project_files.welcome.enter(),
            NextAction::Template => {
                let Some(setup) = &self.template_setup else {
                    self.template_guard_pending = false;
                    return;
                };
                match setup.setup.generate() {
                    Ok(generated) => {
                        let assembly = generated.assembly_id;
                        let needs_stock = generated
                            .fits
                            .iter()
                            .any(|(_, fit)| !matches!(fit, crate::FirstFit::Allocated(_)));
                        self.replace_project(generated.editor, None);
                        self.sync_scene_inspector();
                        self.session.active = crate::workspace_state::Workspace::Design;
                        self.selection.choose(Some(assembly), false);
                        // A generated assembly can be much taller than the
                        // empty-project default view. Fit its actual visible
                        // bounds instead of opening Design or Hardware with
                        // most of the cabinet outside a compact viewport.
                        for workspace in [
                            crate::workspace_state::Workspace::Design,
                            crate::workspace_state::Workspace::Hardware,
                        ] {
                            self.session.view_mut(workspace).camera.request_frame();
                        }
                        self.project_files.welcome.leave();
                        self.template_setup = None;
                        self.template_guard_pending = false;
                        self.template_message =
                            needs_stock.then(|| self.localizer.text("template-setup-needs-stock"));
                    }
                    Err(error) => {
                        self.template_message = Some(format!(
                            "{}: {error:?}",
                            self.localizer.text("template-setup-error-generate")
                        ));
                        self.template_guard_pending = false;
                    }
                }
            }
            NextAction::RecoverFromWelcome => {
                if let Some((path, editor)) = self.project_files.pending_recovery.take() {
                    self.replace_project(editor, path.clone());
                    self.project_files.welcome.leave();
                    if path.is_some() {
                        // Keep the snapshot until an explicit save or discard and avoid
                        // immediately prompting for the same candidate again.
                        self.attach_recovery(false);
                        for error in self.register_successful_project() {
                            self.append_project_message(error);
                        }
                    }
                    self.project_files.welcome.invalidate();
                }
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

    pub(crate) fn save_to(&mut self, path: &Path, replacing: bool, after: Option<NextAction>) {
        // The migrated editor is schema 2 even though its source bytes remain
        // schema 1. Require a conscious decision before the first write,
        // including Save As and Save while resolving an unsaved-work prompt.
        if self.project_files.path.as_ref().is_some_and(|source| {
            File::open(source)
                .ok()
                .and_then(|file| persistence::prepare_reader(file).ok())
                .is_some_and(|prepared| prepared.source_version() == 1)
        }) {
            self.project_files.prompt = Some(Prompt::Upgrade(path.to_path_buf(), replacing, after));
            return;
        }
        self.save_to_confirmed(path, replacing, after);
    }

    fn save_to_confirmed(&mut self, path: &Path, replacing: bool, after: Option<NextAction>) {
        let was_untitled = self.project_files.path.is_none();
        let result = if replacing {
            persistence::save(&mut self.editor, path)
        } else {
            persistence::save_new(&mut self.editor, path)
        };
        match result {
            Ok(()) => {
                if was_untitled {
                    self.discard_untitled_recovery(self.editor.project().id);
                }
                self.project_files.path = Some(path.to_path_buf());
                self.project_files.store = None;
                self.project_files.observed = None;
                self.project_files.message = Some(self.localizer.text("project-saved"));
                self.attach_recovery(false);
                let index_errors = self.register_successful_project();
                // Derived capture uses only the committed project and its own
                // neutral camera. Save success does not depend on local cache IO.
                self.capture_saved_thumbnail(path);
                self.project_files.welcome.invalidate();
                if let Some(action) = after {
                    self.proceed(action);
                }
                for error in index_errors {
                    self.append_project_message(error);
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

    pub(crate) fn open_path(&mut self, path: PathBuf) {
        self.open_path_checked(path, None);
    }

    /// Recent entries are untrusted cached navigation targets. Recheck their
    /// canonical path and UUID before preparing a replacement editor.
    pub(crate) fn open_path_checked(&mut self, path: PathBuf, expected: Option<Uuid>) {
        // The picker has already been removed by tick_project_files. An open
        // arriving during a different modal must not leave an unguarded
        // prepared editor waiting to be installed by a later action.
        if self.busy_for_project() || self.project_files.picker.is_some() {
            return;
        }
        match File::open(&path)
            .map_err(persistence::PersistenceError::Io)
            .and_then(persistence::prepare_reader)
        {
            Ok(prepared) => {
                if let Some(id) = expected {
                    let canonical = std::fs::canonicalize(&path);
                    if !canonical.is_ok_and(|actual| actual == path) || prepared.project().id != id
                    {
                        self.append_project_message(format!(
                            "Recent project changed at {}. Refresh or locate it before opening.",
                            path.display()
                        ));
                        return;
                    }
                }
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
        // Replacing is only reached after a save or an explicit discard, so an
        // untitled snapshot of the outgoing project is obsolete.
        if self.project_files.path.is_none() {
            self.discard_untitled_recovery(self.editor.project().id);
        }
        // Packet preparation is read-only and tied to the old document. Cancel
        // its worker instead of letting a preview block New/Open indefinitely.
        self.invalidate_export_review();
        self.edit_drafts.clear();
        self.pose_frame = plan_my_cabinet::placement::CoordinateFrame::LocalParent;
        self.pending_pose_frame = None;
        self.navigation.clear();
        self.pending_action = None;
        self.pending_project_command = None;
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
            self.append_project_message(self.localizer.text("project-recovery-unavailable"));
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
                        Err(error) => self.append_project_message(format!(
                            "{}: {error}",
                            self.localizer.text("project-recovery-unavailable")
                        )),
                    }
                } else {
                    self.project_files.store = Some(store);
                }
            }
            Err(error) => self.append_project_message(format!(
                "{}: {error}",
                self.localizer.text("project-recovery-unavailable")
            )),
        }
    }

    /// Index only a document that has already been accepted by the editor or
    /// written successfully. Local indexing errors never roll back that action.
    fn register_successful_project(&self) -> Vec<String> {
        let Some(path) = self.project_files.path.as_deref() else {
            return Vec::new();
        };
        let Some(dir) = self
            .project_files
            .user_data_dir
            .clone()
            .or_else(user_data_dir)
        else {
            return vec!["Recent projects: application user-data directory unavailable".into()];
        };
        if !dir.is_absolute() {
            return vec![
                "Recent projects: application user-data directory must be absolute".into(),
            ];
        }
        let id = self.editor.project().id;
        let errors = [
            RecentProjects::open(&dir)
                .and_then(|mut recents| recents.register_successful(path, id, SystemTime::now()))
                .err()
                .map(|error| format!("Recent projects: {error}")),
            RecoveryIndex::open(&dir)
                .and_then(|mut index| index.register_saved(path, id))
                .err()
                .map(|error| format!("Recovery index: {error}")),
        ];
        errors.into_iter().flatten().collect()
    }

    fn capture_saved_thumbnail(&self, path: &Path) {
        let Some(dir) = self
            .project_files
            .user_data_dir
            .clone()
            .or_else(user_data_dir)
        else {
            return;
        };
        let Ok(mut recents) = RecentProjects::open(&dir) else {
            return;
        };
        let Ok(saved) = File::open(path)
            .map_err(persistence::PersistenceError::Io)
            .and_then(persistence::prepare_reader)
        else {
            return;
        };
        let snapshot = saved.project();
        if snapshot.id != self.editor.project().id
            || snapshot.revision != self.editor.project().revision
        {
            return;
        }
        let Ok(pixels) = viewport::saved_thumbnail(snapshot) else {
            return;
        };
        let _ = recents.store_thumbnail(path, snapshot.id, snapshot, &pixels);
    }

    fn append_project_message(&mut self, detail: String) {
        let message = self.project_files.message.get_or_insert_with(String::new);
        if message.split(" · ").any(|part| part == detail) {
            return;
        }
        if !message.is_empty() {
            message.push_str(" · ");
        }
        message.push_str(&detail);
    }

    pub(crate) fn tick_project_files(&mut self, ctx: &egui::Context) {
        if let Some(picker) = &self.project_files.picker {
            let result = picker.result.try_recv();
            if let Ok(path) = result {
                let picker = self
                    .project_files
                    .picker
                    .take()
                    .expect("the enclosing if-let saw the picker");
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
                        if matches!(picker.kind, PickerKind::Save(Some(NextAction::Template))) {
                            self.template_guard_pending = false;
                        }
                        if matches!(
                            picker.kind,
                            PickerKind::Save(Some(NextAction::RecoverFromWelcome))
                        ) {
                            self.project_files.pending_recovery = None;
                        }
                        self.project_files.message = Some(self.localizer.text("project-cancelled"));
                    }
                }
            } else if result == Err(mpsc::TryRecvError::Disconnected) {
                if let Some(picker) = self.project_files.picker.take()
                    && matches!(picker.kind, PickerKind::Save(Some(NextAction::Open)))
                {
                    self.project_files.pending_open = None;
                }
                if self.template_guard_pending {
                    self.template_guard_pending = false;
                }
                self.project_files.pending_recovery = None;
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
        self.tick_untitled_recovery(ctx, key.1);
    }

    /// Never-saved projects get the same inactivity autosave as saved ones,
    /// through the recovery index that Welcome reads.
    fn tick_untitled_recovery(&mut self, ctx: &egui::Context, revision: u64) {
        if self.project_files.path.is_some() || !self.editor.is_dirty() {
            self.project_files.untitled_pending = None;
            return;
        }
        if self.project_files.untitled_written == Some(revision) {
            return;
        }
        let now = Instant::now();
        let since = match self.project_files.untitled_pending {
            Some((pending, since)) if pending == revision => since,
            _ => {
                self.project_files.untitled_pending = Some((revision, now));
                now
            }
        };
        if now.saturating_duration_since(since) < AUTOSAVE_DELAY {
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }
        // One attempt per revision: a failing disk must not retry every frame.
        self.project_files.untitled_written = Some(revision);
        self.project_files.untitled_pending = None;
        if let Err(error) = self.write_untitled_recovery() {
            self.append_project_message(format!(
                "{}: {error}",
                self.localizer.text("project-recovery-unavailable")
            ));
        }
    }

    /// The recovery location, or none in tests and captures that did not
    /// provide one: those must never write into the real user profile.
    fn recovery_dir(&self) -> Option<PathBuf> {
        self.project_files.user_data_dir.clone().or_else(|| {
            (!cfg!(test) && self.capture.is_none())
                .then(user_data_dir)
                .flatten()
        })
    }

    fn write_untitled_recovery(&mut self) -> Result<(), RecoveryError> {
        let Some(dir) = self.recovery_dir() else {
            return Ok(());
        };
        let mut index = RecoveryIndex::open(&dir)?;
        index.register_untitled(self.editor.project().id)?;
        write_untitled_snapshot(&index, &self.editor)
    }

    /// Called when the untitled project is saved or deliberately left.
    fn discard_untitled_recovery(&mut self, project_id: Uuid) {
        self.project_files.untitled_pending = None;
        self.project_files.untitled_written = None;
        let Some(dir) = self.recovery_dir() else {
            return;
        };
        if let Err(error) = discard_untitled_snapshot(&dir, project_id) {
            self.append_project_message(format!(
                "{}: {error}",
                self.localizer.text("project-recovery-unavailable")
            ));
        }
    }

    /// Last chance before an abnormal exit: write the committed project now
    /// instead of waiting for the inactivity delay.
    pub(crate) fn flush_recovery_after_panic(&mut self) {
        let result = match &mut self.project_files.store {
            Some(store) => store.write_now(&self.editor),
            None if self.project_files.path.is_none() && self.editor.is_dirty() => {
                self.write_untitled_recovery()
            }
            None => Ok(()),
        };
        if let Err(error) = result {
            eprintln!("could not write recovery before exiting: {error}");
        }
    }

    pub(crate) fn show_project_controls(&mut self, ui: &mut egui::Ui) {
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
        if ui.button(self.localizer.text("shell-projects")).clicked() {
            self.request_project_action(NextAction::Welcome);
            ui.close();
        }
        ui.horizontal_wrapped(|ui| {
            for action in [A::NewProject, A::OpenProject] {
                if actions::button(
                    ui,
                    &self.localizer,
                    Request::new(action),
                    self.action_availability(Request::new(action)),
                )
                .clicked()
                {
                    self.invoke_or_report(Request::new(action));
                }
            }
            if actions::button(
                ui,
                &self.localizer,
                Request::new(A::SaveProject),
                self.action_availability(Request::new(A::SaveProject)),
            )
            .clicked()
            {
                self.invoke_or_report(Request::new(A::SaveProject));
            }
            if actions::button(
                ui,
                &self.localizer,
                Request::new(A::SaveProjectAs),
                self.action_availability(Request::new(A::SaveProjectAs)),
            )
            .clicked()
            {
                self.invoke_or_report(Request::new(A::SaveProjectAs));
            }
        });
        ui.separator();
        if actions::button(
            ui,
            &self.localizer,
            Request::new(A::OpenSettings),
            self.action_availability(Request::new(A::OpenSettings)),
        )
        .clicked()
        {
            self.invoke_or_report(Request::new(A::OpenSettings));
        }
        if let Some(message) = &self.project_files.message {
            ui.label(message);
        }
        if self.project_files.picker.is_some() {
            ui.label(self.localizer.text("project-choosing"));
        }
    }

    pub(crate) fn show_project_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.project_files.prompt.take() else {
            if let Some(chrome) = &mut self.project_files.prompt_chrome {
                chrome.close(ctx);
            }
            return;
        };
        let mut chrome = self
            .project_files
            .prompt_chrome
            .take()
            .unwrap_or_else(|| ModalChrome::new(egui::Id::new("project-file-dialog")));
        let (title, cancel, confirm) = match &prompt {
            Prompt::Dirty(_) => ("project-unsaved-title", "cancel", "project-save"),
            Prompt::Overwrite(_, _) => ("project-overwrite", "cancel", "project-replace"),
            Prompt::Upgrade(..) => ("project-upgrade-title", "cancel", "project-upgrade-save"),
            Prompt::Recovery(_) => ("project-recovery-title", "project-defer", "project-recover"),
        };
        let dirty = matches!(prompt, Prompt::Dirty(_));
        chrome.set_alert(dirty);
        chrome.set_icon(match &prompt {
            Prompt::Dirty(_) => icons::Icon::Save,
            Prompt::Overwrite(..) | Prompt::Upgrade(..) => icons::Icon::Warning,
            Prompt::Recovery(_) => icons::Icon::Undo,
        });
        chrome.set_primary_hint(dirty.then(|| {
            if cfg!(target_os = "macos") {
                "⌘S".to_owned()
            } else {
                "Ctrl+S".to_owned()
            }
        }));
        chrome.set_context(match &prompt {
            Prompt::Overwrite(path, _) | Prompt::Upgrade(path, _, _) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            Prompt::Recovery(candidate) => Some(candidate.project_name.clone()),
            Prompt::Dirty(_) => None,
        });
        let title = if dirty {
            let mut args = fluent_bundle::FluentArgs::new();
            args.set("name", self.editor.project().name.as_str());
            self.localizer.format("project-unsaved-question", Some(&args))
        } else {
            self.localizer.text(title)
        };
        let body = |ui: &mut egui::Ui| {
            let paragraph = |ui: &mut egui::Ui, text: &str| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text)
                            .size(13.0)
                            .color(theme_widgets::SECONDARY),
                    )
                    .wrap()
                    .selectable(false),
                );
            };
            let path_line = |ui: &mut egui::Ui, path: &std::path::Path| {
                ui.add(
                    egui::Label::new(
                        theme_widgets::mono(path.display().to_string(), 11.5)
                            .color(theme_widgets::MUTED),
                    )
                    .wrap(),
                );
            };
            let extra = match &prompt {
                Prompt::Dirty(action) => {
                    paragraph(ui, &self.localizer.text("project-unsaved-body"));
                    let mut args = fluent_bundle::FluentArgs::new();
                    args.set(
                        "action",
                        self.localizer.text(match action {
                            NextAction::New => "project-new",
                            NextAction::Open => "project-open",
                            NextAction::Close => "project-close",
                            NextAction::Welcome => "shell-projects",
                            NextAction::Template => "template-setup-title",
                            NextAction::RecoverFromWelcome => "welcome-recovery-heading",
                        }),
                    );
                    modal_chrome::form::hint(
                        ui,
                        &self.localizer.format("project-unsaved-next", Some(&args)),
                    );
                    None
                }
                Prompt::Overwrite(path, _) => {
                    path_line(ui, path);
                    None
                }
                Prompt::Upgrade(path, _, _) => {
                    paragraph(ui, &self.localizer.text("project-upgrade-detail"));
                    path_line(ui, path);
                    None
                }
                Prompt::Recovery(candidate) => {
                    path_line(ui, &candidate.saved_path);
                    ui.add(
                        egui::Label::new(
                            theme_widgets::mono(
                                format!(
                                    "{}: {} · {}: {}",
                                    self.localizer.text("project-saved-revision"),
                                    candidate.saved_revision,
                                    self.localizer.text("project-recovery-revision"),
                                    candidate.recovery_revision
                                ),
                                12.0,
                            )
                            .color(theme_widgets::SECONDARY),
                        )
                        .wrap(),
                    );
                    ui.add_space(4.0);
                    modal_chrome::footer_danger(
                        ui,
                        &self.localizer.text("project-recovery-discard"),
                    )
                    .clicked()
                    .then_some("project-recovery-discard")
                }
            };
            (extra, true)
        };
        let mut choice = if matches!(prompt, Prompt::Dirty(_)) {
            let result = chrome.show_three(
                ctx,
                &title,
                ModalThreeActions {
                    primary: &self.localizer.text("project-save"),
                    secondary: &self.localizer.text("project-discard"),
                    stay: &self.localizer.text(cancel),
                },
                body,
            );
            match result.action {
                ModalThreeAction::Primary => Some("project-save"),
                ModalThreeAction::Secondary => Some("project-discard"),
                ModalThreeAction::Stay => Some("cancel"),
                ModalThreeAction::None => None,
            }
        } else {
            let result = chrome.show(
                ctx,
                &title,
                ModalActions {
                    cancel: &self.localizer.text(cancel),
                    confirm: &self.localizer.text(confirm),
                },
                body,
            );
            result.body.or(match result.action {
                ModalAction::Confirm => Some(confirm),
                ModalAction::Cancel => Some(cancel),
                ModalAction::None => None,
            })
        };
        if let Some(key) = choice {
            let action = match key {
                "project-save" => A::DirtySave,
                "project-discard" => A::DirtyDiscard,
                "project-replace" => A::OverwriteProject,
                "project-upgrade-save" => A::OverwriteProject,
                "project-recover" => A::Recover,
                "project-recovery-discard" => A::DiscardRecovery,
                "project-defer" => A::DeferRecovery,
                _ => A::CancelDialog,
            };
            if actions::contextual(Request::new(action), Ok(()), || ()).is_err() {
                choice = None;
            }
        }
        if choice.is_some() {
            chrome.close(ctx);
        }
        self.project_files.prompt_chrome = Some(chrome);
        self.resolve_project_choice(prompt, choice);
    }

    pub(crate) fn resolve_project_choice(&mut self, prompt: Prompt, choice: Option<&str>) {
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
                // This follows save_new's AlreadyExists result. The upgrade
                // confirmation, if needed, preceded that attempt.
                self.save_to_confirmed(&path, true, after)
            }
            (Prompt::Upgrade(path, replacing, after), Some("project-upgrade-save")) => {
                self.save_to_confirmed(&path, replacing, after)
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
            (
                Prompt::Overwrite(_, Some(NextAction::Open))
                | Prompt::Upgrade(_, _, Some(NextAction::Open)),
                _,
            ) => self.project_files.pending_open = None,
            (
                Prompt::Dirty(NextAction::Template)
                | Prompt::Overwrite(_, Some(NextAction::Template))
                | Prompt::Upgrade(_, _, Some(NextAction::Template)),
                _,
            ) => {
                self.template_guard_pending = false;
            }
            (
                Prompt::Dirty(NextAction::RecoverFromWelcome)
                | Prompt::Overwrite(_, Some(NextAction::RecoverFromWelcome))
                | Prompt::Upgrade(_, _, Some(NextAction::RecoverFromWelcome)),
                _,
            ) => {
                self.project_files.pending_recovery = None;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template_setup_ui::TemplateSetupUi;
    use plan_my_cabinet::reference_fixture;
    use plan_my_cabinet::template_setup::TemplateKind;
    use plan_my_cabinet::units::Length;
    use std::fs;

    #[test]
    fn saving_migrated_source_requires_notice_and_cancel_keeps_original_bytes() {
        let dir = std::env::temp_dir().join(format!("pmcab-upgrade-{}", Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let source = dir.join("legacy.pmcab");
        let bytes = include_bytes!("../../tests/fixtures/schema-v1-cabinet.pmcab");
        fs::write(&source, bytes).unwrap();
        let prepared = persistence::prepare_bytes(bytes).unwrap();
        assert_eq!(prepared.source_version(), 1);
        let mut app = DesktopApp {
            editor: prepared.into_editor(),
            ..Default::default()
        };
        app.project_files.path = Some(source.clone());
        app.save_to(&source, true, None);
        assert!(matches!(
            app.project_files.prompt,
            Some(Prompt::Upgrade(..))
        ));
        assert_eq!(fs::read(&source).unwrap(), bytes);
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("cancel"));
        assert_eq!(fs::read(&source).unwrap(), bytes);
        app.project_files.pending_open = Some((
            source.clone(),
            persistence::prepare_bytes(bytes).unwrap().into_editor(),
        ));
        app.save_to(&source, true, Some(NextAction::Open));
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("cancel"));
        assert!(app.project_files.pending_open.is_none());
        assert_eq!(fs::read(&source).unwrap(), bytes);
        app.save_to(&source, true, None);
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("project-upgrade-save"));
        assert_eq!(
            persistence::prepare_reader(File::open(&source).unwrap())
                .unwrap()
                .source_version(),
            2
        );
        fs::remove_dir_all(dir).unwrap();
    }

    fn untitled_snapshots(dir: &Path) -> usize {
        RecoveryIndex::open(dir)
            .unwrap()
            .discover()
            .iter()
            .filter(|row| matches!(row.status, plan_my_cabinet::recovery::DiscoveryStatus::Untitled))
            .count()
    }

    #[test]
    fn untitled_projects_autosave_after_inactivity_and_saving_discards_the_snapshot() {
        let dir = std::env::temp_dir().join(format!("pmcab-untitled-{}", Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let mut app = DesktopApp::default();
        app.project_files.user_data_dir = Some(dir.join("user"));
        app.editor
            .transact(|p| {
                p.name = "Kitchen".into();
                Ok::<_, ()>(())
            })
            .unwrap();
        let revision = app.editor.project().revision;
        let ctx = egui::Context::default();
        app.tick_untitled_recovery(&ctx, revision);
        assert!(!dir.join("user").exists(), "nothing is written before the delay");
        app.project_files.untitled_pending =
            Some((revision, Instant::now() - AUTOSAVE_DELAY - Duration::from_secs(1)));
        app.tick_untitled_recovery(&ctx, revision);
        assert_eq!(untitled_snapshots(&dir.join("user")), 1);

        let path = dir.join("kitchen.pmcab");
        app.save_to(&path, false, None);
        assert_eq!(app.project_files.path.as_deref(), Some(path.as_path()));
        assert_eq!(untitled_snapshots(&dir.join("user")), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_panic_flush_writes_recovery_without_waiting_and_discarding_removes_it() {
        let dir = std::env::temp_dir().join(format!("pmcab-panic-{}", Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let mut app = DesktopApp::default();
        app.project_files.user_data_dir = Some(dir.join("user"));
        app.flush_recovery_after_panic();
        assert!(!dir.join("user").exists(), "a clean project has nothing to recover");
        app.editor
            .transact(|p| {
                p.name = "Wardrobe".into();
                Ok::<_, ()>(())
            })
            .unwrap();
        app.flush_recovery_after_panic();
        assert_eq!(untitled_snapshots(&dir.join("user")), 1);
        // Leaving the untitled project on purpose (after Discard) drops it.
        app.proceed(NextAction::New);
        assert_eq!(untitled_snapshots(&dir.join("user")), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    fn dialog_frame(app: &mut DesktopApp, ctx: &egui::Context, input: egui::RawInput) {
        let mut output = ctx.run_ui(input, |ui| app.show_project_dialog(ui.ctx()));
        output.textures_delta.clear();
    }

    fn dialog_key(key: egui::Key, modifiers: egui::Modifiers) -> egui::RawInput {
        egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        }
    }

    fn click_unsaved_footer(app: &mut DesktopApp, ctx: &egui::Context, label: &str) {
        dialog_frame(app, ctx, egui::RawInput::default());
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(780.0, 560.0),
                )),
                ..Default::default()
            },
            |ui| app.show_project_dialog(ui.ctx()),
        );
        fn text_center(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                egui::Shape::Vec(shapes) => {
                    shapes.iter().find_map(|shape| text_center(shape, label))
                }
                _ => None,
            }
        }
        let point = output
            .shapes
            .iter()
            .find_map(|shape| text_center(&shape.shape, label));
        output.textures_delta.clear();
        let point = point.unwrap_or_else(|| panic!("unsaved footer button not rendered: {label}"));
        for pressed in [true, false] {
            dialog_frame(
                app,
                ctx,
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
            );
        }
    }

    #[test]
    fn unsaved_footer_buttons_save_discard_or_cancel_without_crossing_decisions() {
        for decision in ["project-save", "project-discard", "cancel"] {
            let directory = Directory::new();
            let path = directory.0.join("original.pmcab");
            let mut app = directory.app();
            app.save_to(&path, false, None);
            let original_id = app.editor.project().id;
            app.editor
                .set_grid_spacing(Length::from_micrometres(20_000))
                .unwrap();
            app.project_files.prompt = Some(Prompt::Dirty(NextAction::New));
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            let label = app.localizer.text(decision);
            click_unsaved_footer(&mut app, &ctx, &label);
            assert!(app.project_files.prompt.is_none(), "{decision}");
            let saved = persistence::prepare_reader(File::open(path).unwrap()).unwrap();
            assert_eq!(saved.project().id, original_id, "{decision}");
            assert_eq!(
                saved.project().grid_spacing.micrometres(),
                if decision == "project-save" {
                    20_000
                } else {
                    10_000
                },
                "{decision}"
            );
            if decision == "cancel" {
                assert_eq!(app.editor.project().id, original_id);
                assert!(app.editor.is_dirty());
            } else {
                assert_ne!(app.editor.project().id, original_id, "{decision}");
            }
        }
    }

    #[test]
    fn unsaved_footer_cancel_restores_focus_and_clears_prepared_open() {
        let directory = Directory::new();
        let target = directory.0.join("next.pmcab");
        let mut next = ProjectEditor::new(Project::new("Next", Currency::Usd)).unwrap();
        persistence::save(&mut next, &target).unwrap();
        let mut app = directory.app();
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        let original = app.editor.project().clone();
        app.open_path(target);
        assert!(app.project_files.pending_open.is_some());
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut invoker = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let button = ui.button("Open project");
            invoker = Some(button.id);
            button.request_focus();
            app.show_project_dialog(ui.ctx());
        });
        output.textures_delta.clear();
        assert_ne!(ctx.memory(|m| m.focused()), invoker);
        let label = app.localizer.text("cancel");
        click_unsaved_footer(&mut app, &ctx, &label);
        assert!(app.project_files.prompt.is_none());
        assert!(app.project_files.pending_open.is_none());
        assert_eq!(app.editor.project(), &original);
        assert_eq!(ctx.memory(|m| m.focused()), invoker);
    }

    #[test]
    fn unsaved_project_dialog_traps_focus_and_popup_keys_before_saving() {
        let directory = Directory::new();
        let path = directory.0.join("original.pmcab");
        let mut app = directory.app();
        app.save_to(&path, false, None);
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        let original_id = app.editor.project().id;
        app.project_files.prompt = Some(Prompt::Dirty(NextAction::New));
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut invoker = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let button = ui.button("New project");
            invoker = Some(button.id);
            button.request_focus();
            app.show_project_dialog(ui.ctx());
        });
        output.textures_delta.clear();
        assert!(
            app.project_files
                .prompt_chrome
                .as_ref()
                .unwrap()
                .is_active()
        );
        assert_ne!(ctx.memory(|m| m.focused()), invoker);
        for modifiers in [egui::Modifiers::NONE, egui::Modifiers::SHIFT] {
            dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Tab, modifiers));
            assert!(ctx.memory(|m| m.focused()).is_some());
            assert_ne!(ctx.memory(|m| m.focused()), invoker);
        }
        let popup = egui::Id::new("project-dialog-popup");
        egui::Popup::open_id(&ctx, popup);
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(matches!(app.project_files.prompt, Some(Prompt::Dirty(_))));
        assert!(app.editor.is_dirty());
        egui::Popup::open_id(&ctx, popup);
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
        );
        assert!(matches!(app.project_files.prompt, Some(Prompt::Dirty(_))));
        egui::Popup::close_id(&ctx, popup);
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
        );
        assert!(app.project_files.prompt.is_none());
        assert!(app.editor.is_dirty());
        assert_eq!(ctx.memory(|m| m.focused()), invoker);
        app.project_files.prompt = Some(Prompt::Dirty(NextAction::New));
        dialog_frame(&mut app, &ctx, egui::RawInput::default());
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(app.project_files.prompt.is_none());
        assert!(
            !app.project_files
                .prompt_chrome
                .as_ref()
                .unwrap()
                .is_active()
        );
        assert_eq!(ctx.memory(|m| m.focused()), invoker);
        assert_ne!(app.editor.project().id, original_id);
        let saved = persistence::prepare_reader(File::open(path).unwrap()).unwrap();
        assert_eq!(saved.project().id, original_id);
        assert_eq!(saved.project().grid_spacing.micrometres(), 20_000);
    }

    #[test]
    fn overwrite_escape_preserves_project_file_and_enter_replaces_only_after_confirmation() {
        let directory = Directory::new();
        let path = directory.0.join("existing.pmcab");
        fs::write(&path, b"existing bytes").unwrap();
        let mut app = directory.app();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        app.save_to(&path, false, None);
        assert!(matches!(
            app.project_files.prompt,
            Some(Prompt::Overwrite(..))
        ));
        dialog_frame(&mut app, &ctx, egui::RawInput::default());
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
        );
        assert!(app.project_files.prompt.is_none());
        assert_eq!(fs::read(&path).unwrap(), b"existing bytes");
        assert!(app.project_files.path.is_none());

        app.save_to(&path, false, None);
        dialog_frame(&mut app, &ctx, egui::RawInput::default());
        // Initial Cancel focus must not conceal a destructive default action.
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(app.project_files.prompt.is_none());
        assert_eq!(fs::read(&path).unwrap(), b"existing bytes");
        assert!(app.project_files.path.is_none());
        app.save_to(&path, false, None);
        dialog_frame(&mut app, &ctx, egui::RawInput::default());
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Tab, egui::Modifiers::NONE),
        );
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(app.project_files.prompt.is_none());
        assert_eq!(
            app.project_files.path.as_deref(),
            Some(path.as_path()),
            "message: {:?}",
            app.project_files.message
        );
        let saved = persistence::prepare_reader(File::open(path).unwrap()).unwrap();
        assert_eq!(saved.project().id, app.editor.project().id);
    }

    #[test]
    fn unsaved_discard_keyboard_action_does_not_save_project_file() {
        let directory = Directory::new();
        let path = directory.0.join("before.pmcab");
        let mut app = directory.app();
        app.save_to(&path, false, None);
        let original_id = app.editor.project().id;
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        app.project_files.prompt = Some(Prompt::Dirty(NextAction::New));
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        dialog_frame(&mut app, &ctx, egui::RawInput::default());
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Tab, egui::Modifiers::SHIFT),
        );
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(app.project_files.prompt.is_none());
        assert_ne!(app.editor.project().id, original_id);
        let original_file = persistence::prepare_reader(File::open(path).unwrap()).unwrap();
        assert_eq!(original_file.project().id, original_id);
        assert_eq!(original_file.project().grid_spacing.micrometres(), 10_000);
    }

    #[test]
    fn unsaved_escape_cancels_prepared_open_without_replacing_current_project() {
        let directory = Directory::new();
        let target = directory.0.join("next.pmcab");
        let mut other = ProjectEditor::new(Project::new("Next", Currency::Usd)).unwrap();
        persistence::save(&mut other, &target).unwrap();
        let mut app = directory.app();
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        let original = app.editor.project().clone();
        app.open_path(target);
        assert!(app.project_files.pending_open.is_some());
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        dialog_frame(&mut app, &ctx, egui::RawInput::default());
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
        );
        assert!(app.project_files.prompt.is_none());
        assert!(app.project_files.pending_open.is_none());
        assert_eq!(app.editor.project(), &original);
    }

    #[test]
    fn failed_save_from_unsaved_dialog_keeps_work_and_reopens_decision() {
        let directory = Directory::new();
        let mut app = directory.app();
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        let original = app.editor.project().clone();
        let bad_path = directory.0.join("missing").join("project.pmcab");
        app.project_files.path = Some(bad_path.clone());
        app.project_files.prompt = Some(Prompt::Dirty(NextAction::New));
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        dialog_frame(&mut app, &ctx, egui::RawInput::default());
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(matches!(
            app.project_files.prompt,
            Some(Prompt::Dirty(NextAction::New))
        ));
        assert_eq!(app.editor.project(), &original);
        assert!(app.editor.is_dirty());
        assert!(
            app.project_files
                .message
                .as_ref()
                .unwrap()
                .contains("missing")
        );
        assert!(!bad_path.exists());
        dialog_frame(&mut app, &ctx, egui::RawInput::default());
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
        );
        assert!(app.project_files.prompt.is_none());
        assert_eq!(app.editor.project(), &original);
    }

    #[test]
    fn recovery_dialog_defers_without_deleting_and_recovers_only_on_confirmation() {
        let directory = Directory::new();
        let path = directory.0.join("cabinet.pmcab");
        let mut app = directory.app();
        app.save_to(&path, false, None);
        let saved = app.editor.project().clone();
        let mut recovering =
            ProjectEditor::new(saved.clone()).expect("saved project remains valid");
        recovering
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        let mut store =
            RecoveryStore::new(&directory.0.join("user-data"), &path, saved.id).unwrap();
        let now = Instant::now();
        store
            .note_committed_edit(&recovering, now - Duration::from_secs(31))
            .unwrap();
        assert!(store.tick(&recovering, now).unwrap());
        let snapshot = store.recovery_path().to_path_buf();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        app.project_files.prompt = Some(Prompt::Recovery(Box::new(
            store.inspect(&saved).unwrap().unwrap(),
        )));
        dialog_frame(&mut app, &ctx, egui::RawInput::default());
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
        );
        assert!(app.project_files.prompt.is_none());
        assert_eq!(app.editor.project(), &saved);
        assert!(snapshot.exists());
        app.project_files.prompt = Some(Prompt::Recovery(Box::new(
            store.inspect(&saved).unwrap().unwrap(),
        )));
        dialog_frame(&mut app, &ctx, egui::RawInput::default());
        // Enter on the visibly focused Defer button preserves the saved work.
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(app.project_files.prompt.is_none());
        assert_eq!(app.editor.project(), &saved);
        assert!(snapshot.exists());
        app.project_files.prompt = Some(Prompt::Recovery(Box::new(
            store.inspect(&saved).unwrap().unwrap(),
        )));
        dialog_frame(&mut app, &ctx, egui::RawInput::default());
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Tab, egui::Modifiers::NONE),
        );
        dialog_frame(
            &mut app,
            &ctx,
            dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(app.project_files.prompt.is_none());
        assert!(app.editor.is_dirty());
        assert_eq!(app.editor.project().grid_spacing.micrometres(), 20_000);
        assert_eq!(app.project_files.path.as_deref(), Some(path.as_path()));
        assert_eq!(
            persistence::prepare_reader(File::open(path).unwrap())
                .unwrap()
                .project(),
            &saved
        );
        assert!(snapshot.exists());
    }

    #[test]
    fn template_save_failure_and_picker_cancel_retain_document_and_staged_setup() {
        let mut app = DesktopApp {
            template_setup: Some(TemplateSetupUi::new(
                TemplateKind::Base,
                "Next",
                Currency::Brl,
                plan_my_cabinet::units::Unit::Mm,
            )),
            ..DesktopApp::default()
        };
        app.editor
            .transact(|p| -> Result<(), ()> {
                p.name = "Original work".into();
                Ok(())
            })
            .unwrap();
        let original = app.editor.project().clone();
        app.template_guard_pending = true;
        let missing_parent = std::env::temp_dir()
            .join(Uuid::new_v4().to_string())
            .join("project.pmcab");
        app.save_to(&missing_parent, false, Some(NextAction::Template));
        assert!(matches!(
            app.project_files.prompt,
            Some(Prompt::Dirty(NextAction::Template))
        ));
        assert_eq!(app.editor.project(), &original);
        assert!(app.template_setup.is_some());
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("cancel"));
        assert!(!app.template_guard_pending);

        let (tx, rx) = mpsc::channel();
        app.template_guard_pending = true;
        app.project_files.picker = Some(Picker {
            kind: PickerKind::Save(Some(NextAction::Template)),
            project_id: original.id,
            revision: original.revision,
            result: rx,
        });
        tx.send(None).unwrap();
        app.tick_project_files(&egui::Context::default());
        assert!(!app.template_guard_pending);
        assert!(app.project_files.picker.is_none());
        assert_eq!(app.editor.project(), &original);
        assert!(app.template_setup.is_some());
    }
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    struct Directory(PathBuf);

    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("pmcab-ui-recents-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn app(&self) -> DesktopApp {
            let mut app = DesktopApp::default();
            app.project_files.user_data_dir = Some(self.0.join("user-data"));
            app
        }

        fn recents(&self) -> RecentProjects {
            RecentProjects::open(&self.0.join("user-data")).unwrap()
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn successful_save_captures_committed_scene_and_ignores_preview_geometry() {
        let directory = Directory::new();
        let path = directory.0.join("cabinet.pmcab");
        let mut app = directory.app();
        app.editor = ProjectEditor::new(reference_fixture::project()).unwrap();
        app.save_to(&path, false, None);
        assert_eq!(
            app.project_files.path.as_deref(),
            Some(path.as_path()),
            "{:?}",
            app.project_files.message
        );
        assert!(!app.editor.is_dirty());
        let first = directory.recents().list("");
        let key = first[0]
            .entry
            .thumbnail_key
            .clone()
            .expect("saved thumbnail");
        let pixels = first[0].thumbnail.as_ref().expect("cached pixels").clone();
        assert!(pixels.chunks_exact(4).any(|p| p != [236, 232, 225, 255]));
        app.selection
            .choose(Some(app.editor.project().boards[0].id), false);
        app.editor.begin_preview();
        app.editor
            .update_preview(|project| -> Result<(), ()> {
                project.boards[0].pose.translation_mm[0] += 10_000.0;
                Ok(())
            })
            .unwrap();
        app.capture_saved_thumbnail(&path);
        let row = &directory.recents().list("")[0];
        assert_eq!(row.entry.thumbnail_key.as_deref(), Some(key.as_str()));
        assert_eq!(row.thumbnail.as_ref(), Some(&pixels));
        assert_eq!(
            app.selection.active,
            Some(app.editor.project().boards[0].id)
        );
        assert_ne!(
            app.editor.preview().unwrap().boards[0].pose,
            app.editor.project().boards[0].pose
        );
    }

    #[test]
    fn local_thumbnail_write_failure_is_nonfatal_and_shows_placeholder() {
        let directory = Directory::new();
        let data = directory.0.join("user-data");
        fs::create_dir(&data).unwrap();
        fs::write(data.join("thumbnails"), b"occupied").unwrap();
        let path = directory.0.join("saved.pmcab");
        let mut app = directory.app();
        app.editor = ProjectEditor::new(reference_fixture::project()).unwrap();
        app.save_to(&path, false, None);
        assert_eq!(app.project_files.path.as_deref(), Some(path.as_path()));
        assert!(!app.editor.is_dirty());
        let saved = persistence::prepare_reader(File::open(&path).unwrap()).unwrap();
        assert_eq!(saved.project().id, app.editor.project().id);
        assert_eq!(
            saved.project().boards.len(),
            app.editor.project().boards.len()
        );
        let row = &directory.recents().list("")[0];
        assert!(row.entry.thumbnail_key.is_none());
        assert!(row.thumbnail.is_none());
    }

    #[test]
    fn background_packet_preparation_does_not_block_project_prompt_and_is_cancelled_on_replace() {
        let mut app = DesktopApp::default();
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        let original = app.editor.project().id;
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        app.export_preparation_pending = Some((app.export_key(), rx, Arc::clone(&cancel)));

        assert!(app.action_availability(Request::new(A::NewProject)).is_ok());
        app.request_project_action(NextAction::New);
        let prompt = app.project_files.prompt.take().expect("dirty prompt");
        assert!(matches!(prompt, Prompt::Dirty(NextAction::New)));
        app.resolve_project_choice(prompt, Some("cancel"));
        assert_eq!(app.editor.project().id, original);
        assert!(!cancel.load(Ordering::Relaxed));

        app.request_project_action(NextAction::New);
        let prompt = app
            .project_files
            .prompt
            .take()
            .expect("repeat dirty prompt");
        app.resolve_project_choice(prompt, Some("project-discard"));
        assert_ne!(app.editor.project().id, original);
        assert!(cancel.load(Ordering::Relaxed));
        assert!(app.export_preparation_pending.is_none());
        drop(tx);
    }

    #[test]
    fn only_accepted_open_and_successful_explicit_saves_register() {
        let directory = Directory::new();
        let first = directory.0.join("first.pmcab");
        let second = directory.0.join("second.pmcab");
        let mut app = directory.app();
        app.save_to(&first, false, None);
        let id = app.editor.project().id;
        assert_eq!(
            directory.recents().entries()[0].path,
            fs::canonicalize(&first).unwrap()
        );
        assert_eq!(directory.recents().entries()[0].project_id, id);
        assert_eq!(
            RecoveryIndex::open(&directory.0.join("user-data"))
                .unwrap()
                .entries()[0]
                .saved_path
                .as_deref(),
            Some(fs::canonicalize(&first).unwrap().as_path())
        );

        app.save_to(&second, false, None); // Save As is a second path for the same UUID.
        assert_eq!(directory.recents().entries().len(), 2);
        assert_eq!(
            directory.recents().entries()[0].path,
            fs::canonicalize(&second).unwrap()
        );
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        app.save_to(&second, true, None); // Explicit overwrite refreshes the existing entry.
        assert_eq!(directory.recents().entries().len(), 2);

        app.editor
            .set_grid_spacing(Length::from_micrometres(30_000))
            .unwrap();
        app.open_path(first.clone());
        assert!(matches!(
            app.project_files.prompt,
            Some(Prompt::Dirty(NextAction::Open))
        ));
        assert!(app.project_files.pending_open.is_some());
        assert_eq!(
            directory.recents().entries()[0].path,
            fs::canonicalize(&second).unwrap()
        );
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("cancel"));
        assert!(app.project_files.pending_open.is_none());
        assert_eq!(app.editor.project().grid_spacing.micrometres(), 30_000);
        assert_eq!(
            directory.recents().entries()[0].path,
            fs::canonicalize(&second).unwrap()
        );

        app.open_path(first.clone());
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("project-discard"));
        assert_eq!(app.project_files.path.as_deref(), Some(first.as_path()));
        assert_eq!(
            directory.recents().entries()[0].path,
            fs::canonicalize(&first).unwrap()
        );
        assert_eq!(directory.recents().entries()[0].project_id, id);
        assert!(!app.editor.is_dirty());
    }

    #[test]
    fn prepared_open_is_not_recent_until_dirty_work_is_resolved() {
        let directory = Directory::new();
        let path = directory.0.join("external.pmcab");
        let mut external = ProjectEditor::new(Project::new("External", Currency::Usd)).unwrap();
        persistence::save(&mut external, &path).unwrap();
        let mut app = directory.app();
        let original = app.editor.project().id;
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        app.open_path(path.clone());
        assert!(app.project_files.pending_open.is_some());
        assert_eq!(app.editor.project().id, original);
        assert!(directory.recents().entries().is_empty());
        assert!(
            RecoveryIndex::open(&directory.0.join("user-data"))
                .unwrap()
                .entries()
                .is_empty()
        );
        let prompt = app.project_files.prompt.take().unwrap();
        assert!(matches!(prompt, Prompt::Dirty(NextAction::Open)));
        let (tx, rx) = mpsc::channel();
        app.project_files.picker = Some(Picker {
            kind: PickerKind::Save(Some(NextAction::Open)),
            project_id: original,
            revision: app.editor.project().revision,
            result: rx,
        });
        tx.send(None).unwrap();
        app.tick_project_files(&egui::Context::default());
        assert!(app.project_files.pending_open.is_none());
        assert!(directory.recents().entries().is_empty());

        app.open_path(path.clone());
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("project-discard"));
        assert_eq!(app.editor.project().id, external.project().id);
        assert_eq!(directory.recents().entries().len(), 1);
        assert_eq!(
            directory.recents().entries()[0].path,
            fs::canonicalize(path).unwrap()
        );
    }

    #[test]
    fn failed_open_save_and_cancelled_picker_do_not_register() {
        let directory = Directory::new();
        let mut app = directory.app();
        let original = app.editor.project().clone();
        let (tx, rx) = mpsc::channel();
        app.project_files.picker = Some(Picker {
            kind: PickerKind::Open,
            project_id: original.id,
            revision: original.revision,
            result: rx,
        });
        tx.send(None).unwrap();
        app.tick_project_files(&egui::Context::default());
        let (tx, rx) = mpsc::channel();
        app.project_files.picker = Some(Picker {
            kind: PickerKind::Save(None),
            project_id: original.id,
            revision: original.revision,
            result: rx,
        });
        tx.send(None).unwrap();
        app.tick_project_files(&egui::Context::default());
        app.open_path(directory.0.join("missing.pmcab"));
        fs::write(directory.0.join("invalid.pmcab"), b"invalid").unwrap();
        app.open_path(directory.0.join("invalid.pmcab"));
        app.save_to(&directory.0.join("absent/out.pmcab"), false, None);
        assert!(directory.recents().entries().is_empty());
        assert!(app.project_files.path.is_none());
        assert_eq!(app.editor.project(), &original);
        assert!(
            app.project_files
                .message
                .as_ref()
                .unwrap()
                .contains("absent")
        );

        let path = directory.0.join("existing.pmcab");
        fs::write(&path, b"existing").unwrap();
        app.save_to(&path, false, None);
        assert!(matches!(
            app.project_files.prompt,
            Some(Prompt::Overwrite(_, None))
        ));
        let prompt = app.project_files.prompt.take().unwrap();
        app.resolve_project_choice(prompt, Some("cancel"));
        assert_eq!(fs::read(path).unwrap(), b"existing");
        assert!(directory.recents().entries().is_empty());
    }

    #[test]
    fn failed_local_index_write_reports_error_without_undoing_save_or_open() {
        let directory = Directory::new();
        let user_data = directory.0.join("user-data");
        fs::create_dir(&user_data).unwrap();
        // A directory occupying the index filename makes local indexing fail
        // deterministically even when the project file is fully writable.
        fs::create_dir(user_data.join("recent-projects.json")).unwrap();
        let path = directory.0.join("valid.pmcab");
        let mut app = directory.app();
        app.save_to(&path, false, None);
        let saved = app.editor.project().clone();
        assert!(!app.editor.is_dirty());
        assert_eq!(app.project_files.path.as_deref(), Some(path.as_path()));
        assert!(
            app.project_files
                .message
                .as_ref()
                .unwrap()
                .contains("Recent projects:")
        );
        assert_eq!(
            persistence::prepare_reader(File::open(&path).unwrap())
                .unwrap()
                .project(),
            &saved
        );

        app.proceed(NextAction::New);
        app.open_path(path.clone());
        assert_eq!(app.editor.project(), &saved);
        assert!(
            app.project_files
                .message
                .as_ref()
                .unwrap()
                .contains("Recent projects:")
        );
        assert_eq!(
            RecoveryIndex::open(&user_data).unwrap().entries()[0].project_id,
            saved.id
        );

        let pending_path = directory.0.join("other.pmcab");
        let mut other = ProjectEditor::new(Project::new("Other", Currency::Usd)).unwrap();
        persistence::save(&mut other, &pending_path).unwrap();
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        app.open_path(pending_path);
        assert!(app.project_files.pending_open.is_some());
        let prompt = app.project_files.prompt.take().unwrap();
        assert!(matches!(prompt, Prompt::Dirty(NextAction::Open)));
        app.save_to(&path, true, Some(NextAction::Open));
        assert_eq!(app.editor.project().id, other.project().id);
        assert!(
            app.project_files
                .message
                .as_ref()
                .unwrap()
                .contains("Recent projects:")
        );
    }

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
