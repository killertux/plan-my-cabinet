use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence;
use plan_my_cabinet::recovery::{
    AUTOSAVE_DELAY, DiscoveryStatus, RecoveryIndex, RecoveryStore, write_untitled_snapshot,
};
use uuid::Uuid;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("pmcab-cleanup-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn saved_snapshot(index: &mut RecoveryIndex, local: &Path, saved: &Path) -> (Uuid, PathBuf) {
    let mut editor = ProjectEditor::new(Project::new("Original", Currency::Usd)).unwrap();
    persistence::save(&mut editor, saved).unwrap();
    let id = editor.project().id;
    index.register_saved(saved, id).unwrap();
    editor
        .transact(|project| -> Result<(), ()> {
            project.name = "Recovered".into();
            Ok(())
        })
        .unwrap();
    let now = Instant::now();
    let mut store = RecoveryStore::new(local, saved, id).unwrap();
    store.note_committed_edit(&editor, now).unwrap();
    store.tick(&editor, now + AUTOSAVE_DELAY).unwrap();
    (id, store.recovery_path().to_path_buf())
}

#[test]
fn review_starts_empty_and_cancellation_preserves_everything() {
    let dir = Directory::new();
    let local = dir.0.join("local");
    let saved = dir.0.join("cabinet.pmcab");
    let mut index = RecoveryIndex::open(&local).unwrap();
    let (_, snapshot) = saved_snapshot(&mut index, &local, &saved);
    let project_bytes = fs::read(&saved).unwrap();
    let snapshot_bytes = fs::read(&snapshot).unwrap();
    assert_eq!(index.recovery_folder(), local.join("recovery"));

    let review = index.cleanup_review();
    assert_eq!(review.rows().len(), 1);
    assert_eq!(review.selected().count(), 0);
    assert!(review.prepare_confirmation().is_none());
    let mut review = index.cleanup_review();
    let row = &review.rows()[0];
    let identity = row.identity.clone();
    let path = row.snapshot_path.clone();
    review.select(&identity, &path).unwrap();
    let pending = review.prepare_confirmation().unwrap();
    assert_eq!(pending.selected().count(), 1);
    drop(pending); // Cancel in the UI: there is no deletion call.
    assert_eq!(fs::read(saved).unwrap(), project_bytes);
    assert_eq!(fs::read(snapshot).unwrap(), snapshot_bytes);
}

#[test]
fn partial_failure_keeps_changed_and_unselected_snapshots_and_all_saved_files() {
    let dir = Directory::new();
    let local = dir.0.join("local");
    let mut index = RecoveryIndex::open(&local).unwrap();
    let first = dir.0.join("first.pmcab");
    let second = dir.0.join("second.pmcab");
    let untouched = dir.0.join("untouched.pmcab");
    let (_, first_snapshot) = saved_snapshot(&mut index, &local, &first);
    let (_, second_snapshot) = saved_snapshot(&mut index, &local, &second);
    let (_, untouched_snapshot) = saved_snapshot(&mut index, &local, &untouched);
    let saved_bytes: Vec<_> = [&first, &second, &untouched]
        .iter()
        .map(|path| fs::read(path).unwrap())
        .collect();
    let mut review = index.cleanup_review();
    for row in review.rows() {
        assert!(matches!(row.status, DiscoveryStatus::Newer));
    }
    let selections: Vec<_> = review
        .rows()
        .iter()
        .filter(|row| row.snapshot_path != untouched_snapshot)
        .map(|row| (row.identity.clone(), row.snapshot_path.clone()))
        .collect();
    for (identity, path) in selections {
        review.select(&identity, &path).unwrap();
    }
    let pending = review.prepare_confirmation().unwrap();
    // The bytes may be invalid now, but cleanup must not silently delete a
    // file that changed after selection.
    fs::write(&first_snapshot, b"changed while reviewing").unwrap();
    let token = pending.confirmation_token();
    let outcome = pending.confirm(&index, token).unwrap();
    assert_eq!(
        outcome.deleted.as_slice(),
        std::slice::from_ref(&second_snapshot)
    );
    assert_eq!(outcome.failed.len(), 1);
    assert_eq!(outcome.failed[0].snapshot_path, first_snapshot);
    assert!(first_snapshot.exists());
    assert!(untouched_snapshot.exists());
    assert!(!second_snapshot.exists());
    for (path, bytes) in [&first, &second, &untouched].iter().zip(saved_bytes) {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn copied_identity_cannot_be_selected_as_original_and_invalid_requires_selection() {
    let dir = Directory::new();
    let local = dir.0.join("local");
    let mut index = RecoveryIndex::open(&local).unwrap();
    let saved = dir.0.join("original.pmcab");
    let copied = dir.0.join("copied.pmcab");
    let (id, original_snapshot) = saved_snapshot(&mut index, &local, &saved);
    fs::copy(&saved, &copied).unwrap();
    index.register_saved(&copied, id).unwrap();
    let mut review = index.cleanup_review();
    assert_eq!(review.rows().len(), 1);
    let original_identity = review.rows()[0].identity.clone();
    let copied_identity = index
        .entries()
        .iter()
        .find(|identity| {
            identity.saved_path.as_deref() == Some(fs::canonicalize(&copied).unwrap().as_path())
        })
        .unwrap();
    assert!(review.select(copied_identity, &original_snapshot).is_err());
    assert!(original_snapshot.exists());

    fs::write(&original_snapshot, b"invalid recovery").unwrap();
    let review = index.cleanup_review();
    assert!(matches!(
        review.rows()[0].status,
        DiscoveryStatus::Invalid(_)
    ));
    assert!(review.prepare_confirmation().is_none());
    let mut review = index.cleanup_review();
    review
        .select(&original_identity, &original_snapshot)
        .unwrap();
    let pending = review.prepare_confirmation().unwrap();
    let token = pending.confirmation_token();
    assert_eq!(
        pending.confirm(&index, token).unwrap().deleted,
        [original_snapshot]
    );
    assert!(saved.exists());
    assert!(copied.exists());
}

#[test]
fn wrong_token_and_unregistered_snapshot_fail_without_removal() {
    let dir = Directory::new();
    let local = dir.0.join("local");
    let mut index = RecoveryIndex::open(&local).unwrap();
    let mut editor = ProjectEditor::new(Project::new("Untitled", Currency::Usd)).unwrap();
    editor
        .transact(|project| -> Result<(), ()> {
            project.name = "Edited".into();
            Ok(())
        })
        .unwrap();
    index.register_untitled(editor.project().id).unwrap();
    write_untitled_snapshot(&index, &editor).unwrap();
    let mut review = index.cleanup_review();
    let row = &review.rows()[0];
    let identity = row.identity.clone();
    let path = row.snapshot_path.clone();
    review.select(&identity, &path).unwrap();
    let pending = review.prepare_confirmation().unwrap();
    let mut other_review = index.cleanup_review();
    other_review.select(&identity, &path).unwrap();
    let wrong = other_review
        .prepare_confirmation()
        .unwrap()
        .confirmation_token();
    assert!(pending.confirm(&index, wrong).is_err());
    assert!(path.exists());
    let mut review = index.cleanup_review();
    review.select(&identity, &path).unwrap();
    let pending = review.prepare_confirmation().unwrap();
    let token = pending.confirmation_token();
    fs::remove_file(local.join("recovery-index.json")).unwrap();
    let outcome = pending.confirm(&index, token).unwrap();
    assert!(outcome.deleted.is_empty());
    assert_eq!(outcome.failed.len(), 1);
    assert!(path.exists());
}
