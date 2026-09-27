use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence;
use plan_my_cabinet::recovery::{
    AUTOSAVE_DELAY, DiscoveryStatus, RecoveryChoice, RecoveryIndex, RecoveryStore,
    write_untitled_snapshot,
};
use uuid::Uuid;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("pmcab-index-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn saved_editor(path: &Path) -> ProjectEditor {
    let mut editor = ProjectEditor::new(Project::new("Original", Currency::Usd)).unwrap();
    persistence::save(&mut editor, path).unwrap();
    editor
}

#[test]
fn welcome_decisions_defer_across_reopen_recover_dirty_and_discard_snapshot_only() {
    let dir = Directory::new();
    let local = dir.0.join("local");
    let original = dir.0.join("original.pmcab");
    let copy = dir.0.join("copy.pmcab");
    let mut editor = saved_editor(&original);
    let id = editor.project().id;
    fs::copy(&original, &copy).unwrap();
    let saved_bytes = fs::read(&original).unwrap();
    let mut index = RecoveryIndex::open(&local).unwrap();
    index.register_saved(&original, id).unwrap();
    index.register_saved(&copy, id).unwrap();
    editor
        .transact(|project| -> Result<(), ()> {
            project.name = "Recovered edits".into();
            Ok(())
        })
        .unwrap();
    let mut store = RecoveryStore::new(&local, &original, id).unwrap();
    let now = Instant::now();
    store.note_committed_edit(&editor, now).unwrap();
    store.tick(&editor, now + AUTOSAVE_DELAY).unwrap();
    let snapshot_bytes = fs::read(store.recovery_path()).unwrap();
    let candidate = index.inspect_saved(&original, id).unwrap().unwrap();
    assert!(
        candidate
            .resolve(&store, RecoveryChoice::Defer)
            .unwrap()
            .is_none()
    );
    let reopened = RecoveryIndex::open(&local).unwrap();
    let discovered = reopened.discover();
    assert_eq!(discovered.len(), 1);
    assert!(matches!(discovered[0].status, DiscoveryStatus::Newer));
    assert_eq!(
        discovered[0].identity.saved_path.as_deref(),
        Some(fs::canonicalize(&original).unwrap().as_path())
    );
    assert!(reopened.inspect_saved(&copy, id).unwrap().is_none());
    assert_eq!(fs::read(store.recovery_path()).unwrap(), snapshot_bytes);

    let recovered = reopened
        .inspect_saved(&original, id)
        .unwrap()
        .unwrap()
        .resolve(&store, RecoveryChoice::Recover)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.project().name, "Recovered edits");
    assert!(recovered.is_dirty());
    assert_eq!(fs::read(&original).unwrap(), saved_bytes);
    assert_eq!(fs::read(store.recovery_path()).unwrap(), snapshot_bytes);

    reopened
        .inspect_saved(&original, id)
        .unwrap()
        .unwrap()
        .resolve(&store, RecoveryChoice::Discard)
        .unwrap();
    assert!(reopened.discover().is_empty());
    assert_eq!(fs::read(&original).unwrap(), saved_bytes);
    assert_eq!(fs::read(&copy).unwrap(), saved_bytes);
}

#[test]
fn registered_aliases_match_but_copies_and_unregistered_paths_do_not() {
    let dir = Directory::new();
    fs::create_dir(dir.0.join("sub")).unwrap();
    let original = dir.0.join("original.pmcab");
    let copied = dir.0.join("copy.pmcab");
    let unregistered = dir.0.join("unregistered.pmcab");
    let mut editor = saved_editor(&original);
    fs::copy(&original, &copied).unwrap();
    fs::copy(&original, &unregistered).unwrap();
    let id = editor.project().id;
    let mut index = RecoveryIndex::open(&dir.0.join("local")).unwrap();
    index.register_saved(&original, id).unwrap();
    index
        .register_saved(&dir.0.join("sub/../original.pmcab"), id)
        .unwrap();
    index.register_saved(&copied, id).unwrap();
    assert_eq!(index.entries().len(), 2);

    let mut store = RecoveryStore::new(&dir.0.join("local"), &original, id).unwrap();
    editor
        .transact(|p| -> Result<(), ()> {
            p.name = "Edited".into();
            Ok(())
        })
        .unwrap();
    let now = Instant::now();
    store.note_committed_edit(&editor, now).unwrap();
    store.tick(&editor, now + AUTOSAVE_DELAY).unwrap();
    let bytes = fs::read(store.recovery_path()).unwrap();
    let entries = RecoveryIndex::open(&dir.0.join("local"))
        .unwrap()
        .discover();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].identity.saved_path.as_deref(),
        Some(fs::canonicalize(&original).unwrap().as_path())
    );
    assert!(matches!(entries[0].status, DiscoveryStatus::Newer));
    assert_eq!(entries[0].saved_revision, Some(0));
    assert_eq!(entries[0].recovery_revision, Some(1));
    assert_eq!(fs::read(store.recovery_path()).unwrap(), bytes);

    // A path now pointing to another canonical location cannot adopt the
    // original recovery, even if the portable document has the same UUID.
    fs::remove_file(&original).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&copied, &original).unwrap();
    #[cfg(unix)]
    {
        let entries = index.discover();
        assert!(matches!(
            entries[0].status,
            DiscoveryStatus::SavedUnavailable(_)
        ));
    }
}

#[test]
fn untitled_stays_pathless_even_after_save_as_and_unregistered_write_is_rejected() {
    let dir = Directory::new();
    let local = dir.0.join("local");
    let mut index = RecoveryIndex::open(&local).unwrap();
    let mut editor = ProjectEditor::new(Project::new("Never saved", Currency::Usd)).unwrap();
    let project_id = editor.project().id;
    assert!(write_untitled_snapshot(&index, &editor).is_err());
    index.register_untitled(project_id).unwrap();
    write_untitled_snapshot(&index, &editor).unwrap();
    assert!(index.discover().is_empty());
    editor
        .transact(|p| -> Result<(), ()> {
            p.name = "Untitled edits".into();
            Ok(())
        })
        .unwrap();
    write_untitled_snapshot(&index, &editor).unwrap();
    let entries = RecoveryIndex::open(&local).unwrap().discover();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].identity.saved_path, None);
    assert_eq!(entries[0].saved_revision, None);
    assert_eq!(entries[0].recovery_revision, Some(1));
    assert!(matches!(entries[0].status, DiscoveryStatus::Untitled));
    assert_eq!(index.load_untitled(project_id).unwrap().id, project_id);

    let save_as = dir.0.join("new.pmcab");
    persistence::save(&mut editor, &save_as).unwrap();
    index.register_saved(&save_as, project_id).unwrap();
    let entries = index.discover();
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].status, DiscoveryStatus::Untitled));
    assert_eq!(entries[0].saved_revision, None);
    assert_eq!(
        persistence::prepare_bytes(&fs::read(&save_as).unwrap())
            .unwrap()
            .project()
            .id,
        project_id
    );
}

#[test]
fn unknown_metadata_legacy_payload_and_invalid_snapshots_remain_diagnosable() {
    let dir = Directory::new();
    let path = dir.0.join("legacy.pmcab");
    let golden = include_bytes!("fixtures/schema-v1-cabinet.pmcab");
    fs::write(&path, golden).unwrap();
    let saved = persistence::prepare_bytes(golden)
        .unwrap()
        .project()
        .clone();
    let mut index = RecoveryIndex::open(&dir.0.join("local")).unwrap();
    index.register_saved(&path, saved.id).unwrap();
    let store = RecoveryStore::new(&dir.0.join("local"), &path, saved.id).unwrap();
    fs::create_dir_all(store.recovery_path().parent().unwrap()).unwrap();
    let mut payload: serde_json::Value = serde_json::from_slice(golden).unwrap();
    payload["revision"] = (saved.revision + 1).into();
    payload["name"] = "Recovered legacy edit".into();
    let key = store.recovery_path().file_stem().unwrap().to_str().unwrap();
    let mut record = serde_json::json!({
        "project_id": saved.id, "path_key": key,
        "unexpected_metadata": {"future": true}, "project": payload
    });
    let bytes = serde_json::to_vec(&record).unwrap();
    fs::write(store.recovery_path(), &bytes).unwrap();
    let entry = index.discover().pop().unwrap();
    assert!(matches!(entry.status, DiscoveryStatus::Newer));
    assert_eq!(entry.recovery_revision, Some(saved.revision + 1));
    assert_eq!(fs::read(store.recovery_path()).unwrap(), bytes);
    assert!(index.inspect_saved(&path, saved.id).unwrap().is_some());
    assert_eq!(
        store.inspect(&saved).unwrap().unwrap().recovery_revision,
        saved.revision + 1
    );

    for version in [1, 3] {
        record["project"]["schema_version"] = version.into();
        if version == 1 {
            record["project"]["boards"][0]["length"] = 0.into();
        }
        let bytes = serde_json::to_vec(&record).unwrap();
        fs::write(store.recovery_path(), &bytes).unwrap();
        let entry = index.discover().pop().unwrap();
        assert!(matches!(entry.status, DiscoveryStatus::Invalid(_)));
        assert_eq!(entry.saved_revision, None);
        assert_eq!(fs::read(store.recovery_path()).unwrap(), bytes);
        assert_eq!(fs::read(&path).unwrap(), golden);
    }
    // Unknown / missing saved data is never reported as a known revision.
    fs::remove_file(&path).unwrap();
    record["project"] = serde_json::from_slice(golden).unwrap();
    record["project"]["revision"] = (saved.revision + 1).into();
    fs::write(store.recovery_path(), serde_json::to_vec(&record).unwrap()).unwrap();
    let entry = index.discover().pop().unwrap();
    assert!(matches!(entry.status, DiscoveryStatus::SavedUnavailable(_)));
    assert_eq!(entry.saved_revision, None);
    assert!(index.inspect_saved(&path, saved.id).is_err());
}

#[test]
fn malformed_index_is_retained_and_failed_registration_does_not_erase_it() {
    let dir = Directory::new();
    let local = dir.0.join("local");
    fs::create_dir(&local).unwrap();
    let path = local.join("recovery-index.json");
    let bad = b"{\"version\":1,\"version\":2}";
    fs::write(&path, bad).unwrap();
    assert!(RecoveryIndex::open(&local).is_err());
    assert_eq!(fs::read(path).unwrap(), bad);

    let mut index = RecoveryIndex::open(&dir.0.join("other")).unwrap();
    assert!(
        index
            .register_saved(&dir.0.join("missing/unknown.pmcab"), Uuid::new_v4())
            .is_err()
    );
    assert!(index.entries().is_empty());
}

#[test]
fn untitled_snapshot_requires_explicit_null_and_index_is_bounded() {
    let dir = Directory::new();
    let local = dir.0.join("local");
    let mut index = RecoveryIndex::open(&local).unwrap();
    let mut editor = ProjectEditor::new(Project::new("New", Currency::Usd)).unwrap();
    index.register_untitled(editor.project().id).unwrap();
    editor
        .transact(|p| -> Result<(), ()> {
            p.name = "Edit".into();
            Ok(())
        })
        .unwrap();
    write_untitled_snapshot(&index, &editor).unwrap();
    let snapshot = &index.discover()[0].snapshot_path;
    let mut record: serde_json::Value =
        serde_json::from_slice(&fs::read(snapshot).unwrap()).unwrap();
    record.as_object_mut().unwrap().remove("saved_path");
    let bytes = serde_json::to_vec(&record).unwrap();
    fs::write(snapshot, &bytes).unwrap();
    assert!(matches!(
        index.discover()[0].status,
        DiscoveryStatus::Invalid(_)
    ));
    assert!(index.load_untitled(editor.project().id).is_err());
    assert_eq!(fs::read(snapshot).unwrap(), bytes);

    let path = local.join("recovery-index.json");
    let prior = fs::read(&path).unwrap();
    for _ in 0..255 {
        index.register_untitled(Uuid::new_v4()).unwrap();
    }
    assert!(index.register_untitled(Uuid::new_v4()).is_err());
    assert_eq!(index.entries().len(), 256);
    assert_ne!(fs::read(&path).unwrap(), prior);
    assert_eq!(RecoveryIndex::open(&local).unwrap().entries().len(), 256);
}
