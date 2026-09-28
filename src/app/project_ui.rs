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

    /// A background search never blocks: saving writes the committed project
    /// and replacing it resets (and so cancels) the optimizer.
    pub(crate) fn busy_for_project(&self) -> bool {
        self.modal_open() || self.handoff.activity.is_some() || self.editor.preview().is_some()
    }

    /// Door motion only changes the displayed angle, so Save stays available.
    pub(crate) fn busy_for_save(&self) -> bool {
        if self.door_motion_only() {
            self.handoff.activity.is_some() || self.editor.preview().is_some()
        } else {
            self.busy_for_project()
        }
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
        if self.busy_for_save() {
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
                self.replace_project(ProjectEditor::new(project).expect("empty project"), None);
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
                let Some(setup) = &self.template.setup else {
                    self.template.guard_pending = false;
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
                        self.template.setup = None;
                        self.template.guard_pending = false;
                        self.template.message =
                            needs_stock.then(|| self.localizer.text("template-setup-needs-stock"));
                    }
                    Err(error) => {
                        self.template.message = Some(format!(
                            "{}: {error:?}",
                            self.localizer.text("template-setup-error-generate")
                        ));
                        self.template.guard_pending = false;
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
        // The migrated editor is the current schema even though its source
        // bytes are older, which older releases then cannot open. Require a
        // conscious decision before the first write, including Save As and
        // Save while resolving an unsaved-work prompt.
        if self.project_files.path.as_ref().is_some_and(|source| {
            File::open(source)
                .ok()
                .and_then(|file| persistence::prepare_reader(file).ok())
                .is_some_and(|prepared| {
                    prepared.source_version() < u64::from(plan_my_cabinet::domain::SCHEMA_VERSION)
                })
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
        self.design.pose_frame = plan_my_cabinet::placement::CoordinateFrame::LocalParent;
        self.design.pending_pose_frame = None;
        self.navigation.clear();
        self.pending_action = None;
        self.pending_project_command = None;
        self.editor = editor;
        self.project_files.path = path;
        self.project_files.store = None;
        self.project_files.observed = None;
        self.selection = viewport::Selection::default();
        self.design.move_tool = viewport::MoveTool::default();
        self.camera = viewport::Camera::default();
        self.design.measurement_frame = plan_my_cabinet::measurements::Frame::World;
        self.hardware.door_motion = None;
        self.design.first_fit_notice = None;
        self.cut_plan.material_conflicts.clear();
        self.hardware.catalog_update_notice = None;
        self.cut_plan.allocation_diagnostics = None;
        self.handoff.message = None;
        self.cut_plan.optimizer = Default::default();
        self.cut_plan.repair = Default::default();
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
                            self.template.guard_pending = false;
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
                if self.template.guard_pending {
                    self.template.guard_pending = false;
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
            self.localizer
                .format("project-unsaved-question", Some(&args))
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
                self.template.guard_pending = false;
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
mod tests;
