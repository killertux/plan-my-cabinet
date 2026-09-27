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
