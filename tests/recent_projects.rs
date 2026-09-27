use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::export::ExportStatus;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence;
use plan_my_cabinet::recent_projects::{
    MAX_RECENT_ENTRIES, MAX_RECENT_INDEX_BYTES, RecentError, RecentProjects, RecentStatus,
    THUMBNAIL_SIZE,
};
use plan_my_cabinet::reference_fixture;
use uuid::Uuid;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("pmcab-recent-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn data(&self) -> PathBuf {
        self.0.join("data")
    }
    fn project(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

#[test]
fn thumbnail_is_local_validated_and_invalidated_by_saved_state() {
    let dir = Directory::new();
    let path = dir.project("scene.pmcab");
    let mut editor = ProjectEditor::new(reference_fixture::project()).unwrap();
    persistence::save(&mut editor, &path).unwrap();
    let id = editor.project().id;
    let mut index = RecentProjects::open(&dir.data()).unwrap();
    index.register_successful(&path, id, time(1)).unwrap();
    assert!(index.list("")[0].thumbnail.is_none());
    let pixels = vec![177; THUMBNAIL_SIZE[0] * THUMBNAIL_SIZE[1] * 4];
    index
        .store_thumbnail(&path, id, editor.project(), &pixels)
        .unwrap();
    let key = index.entries()[0].thumbnail_key.clone().unwrap();
    assert_eq!(
        RecentProjects::open(&dir.data()).unwrap().list("")[0]
            .thumbnail
            .as_deref(),
        Some(pixels.as_slice())
    );
    index.register_successful(&path, id, time(2)).unwrap();
    assert_eq!(
        index.entries()[0].thumbnail_key.as_deref(),
        Some(key.as_str())
    );

    let cache = dir.data().join("thumbnails").join(format!("{key}.rgba"));
    fs::write(&cache, b"broken").unwrap();
    assert!(index.list("")[0].thumbnail.is_none());
    index
        .store_thumbnail(&path, id, editor.project(), &pixels)
        .unwrap();
    let mut changed = editor.project().clone();
    changed.name = "Changed".into();
    fs::write(&path, persistence::serialize(&changed).unwrap()).unwrap();
    assert!(index.list("")[0].thumbnail.is_none());
    index.register_successful(&path, id, time(3)).unwrap();
    assert_eq!(index.entries()[0].thumbnail_key, None);
    assert!(
        index
            .store_thumbnail(&path, id, editor.project(), &pixels)
            .is_err()
    );
    assert!(
        index
            .store_thumbnail(&path, Uuid::new_v4(), editor.project(), &pixels)
            .is_err()
    );
    assert!(
        index
            .store_thumbnail(&path, id, editor.project(), &pixels[..10])
            .is_err()
    );
    assert!(index.list("")[0].thumbnail.is_none());
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn save(path: &Path, name: &str) -> ProjectEditor {
    let mut editor = ProjectEditor::new(Project::new(name, Currency::Usd)).unwrap();
    persistence::save(&mut editor, path).unwrap();
    editor
}

fn time(n: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(n)
}

#[test]
fn explicit_success_and_failed_open_save_do_not_register_themselves() {
    let dir = Directory::new();
    let path = dir.project("first.pmcab");
    let mut index = RecentProjects::open(&dir.data()).unwrap();
    assert!(index.entries().is_empty());
    assert!(!dir.data().exists());
    let mut editor = ProjectEditor::new(Project::new("Draft", Currency::Usd)).unwrap();
    assert!(persistence::save_new(&mut editor, &dir.project("missing/failed.pmcab")).is_err());
    assert!(index.entries().is_empty());
    let editor = save(&path, "Cabinet");
    let id = editor.project().id;
    assert!(matches!(
        index.register_successful(&path, Uuid::new_v4(), time(1)),
        Err(RecentError::WrongProject { .. })
    ));
    assert!(index.entries().is_empty());
    index.register_successful(&path, id, time(10)).unwrap();
    assert_eq!(index.entries()[0].last_used_unix_ms, 10);
    let available = index.list("cAbInEt");
    assert_eq!(available.len(), 1);
    assert_eq!(
        available[0].export_status,
        Some(ExportStatus::NeverExported)
    );
    assert!(
        matches!(&available[0].status, RecentStatus::Available(summary) if summary.name == "Cabinet" && summary.boards == 0 && summary.export_records == 0)
    );
    assert_eq!(index.list("FIRST.PMCAB").len(), 1);
    assert!(index.list("unrelated").is_empty());
    let mut on_disk = editor.project().clone();
    on_disk.name = "Changed on disk".into();
    fs::write(&path, persistence::serialize(&on_disk).unwrap()).unwrap();
    assert_eq!(index.list("changed").len(), 1);
    assert!(index.list("cabinet").is_empty());

    fs::write(&path, b"{bad").unwrap();
    assert!(index.register_successful(&path, id, time(20)).is_err());
    assert_eq!(index.entries()[0].last_used_unix_ms, 10);
    assert!(matches!(
        index.list("")[0].status,
        RecentStatus::Unavailable(_)
    ));
    assert_eq!(index.list("")[0].export_status, None);
    assert!(matches!(
        RecentProjects::open(&dir.data()).unwrap().list("")[0].status,
        RecentStatus::Unavailable(_)
    ));
    assert_eq!(index.list("cabinet").len(), 1); // explicitly stale cached name
}

#[test]
fn aliases_copies_rename_and_missing_are_distinct_identities() {
    let dir = Directory::new();
    fs::create_dir(dir.project("sub")).unwrap();
    let original = dir.project("original.pmcab");
    let copy = dir.project("copy.pmcab");
    let id = save(&original, "Original").project().id;
    fs::copy(&original, &copy).unwrap();
    let mut index = RecentProjects::open(&dir.data()).unwrap();
    index.register_successful(&original, id, time(1)).unwrap();
    index
        .register_successful(&dir.project("sub/../original.pmcab"), id, time(2))
        .unwrap();
    assert_eq!(index.entries().len(), 1);
    index.register_successful(&copy, id, time(3)).unwrap();
    assert_eq!(index.entries().len(), 2);
    let old_key = index.entries()[1].path.clone();

    let renamed = dir.project("renamed.pmcab");
    fs::rename(&original, &renamed).unwrap();
    assert!(matches!(
        index.list("original.pmcab")[0].status,
        RecentStatus::Missing
    ));
    assert_eq!(index.list("original.pmcab")[0].export_status, None);
    index.locate(&old_key, id, &renamed).unwrap();
    assert_eq!(index.entries()[1].path, fs::canonicalize(renamed).unwrap());
    assert_eq!(index.entries()[1].last_used_unix_ms, 2); // Locate is not an open/save
    assert_eq!(index.entries().len(), 2);
}

#[test]
fn locate_validates_before_replacement_and_remove_touches_only_index() {
    let dir = Directory::new();
    let original = dir.project("missing.pmcab");
    let replacement = dir.project("replacement.pmcab");
    let wrong = dir.project("wrong.pmcab");
    let id = save(&original, "Original").project().id;
    let mut index = RecentProjects::open(&dir.data()).unwrap();
    index.register_successful(&original, id, time(1)).unwrap();
    let old_key = index.entries()[0].path.clone();
    fs::copy(&original, &replacement).unwrap();
    fs::rename(&original, dir.project("elsewhere.pmcab")).unwrap();
    save(&wrong, "Unrelated");
    let before = fs::read(dir.data().join("recent-projects.json")).unwrap();
    assert!(matches!(
        index.locate(&old_key, id, &wrong),
        Err(RecentError::WrongProject { .. })
    ));
    fs::write(&replacement, b"invalid").unwrap();
    assert!(index.locate(&old_key, id, &replacement).is_err());
    assert_eq!(
        fs::read(dir.data().join("recent-projects.json")).unwrap(),
        before
    );
    assert_eq!(index.entries()[0].path, old_key);
    fs::copy(dir.project("elsewhere.pmcab"), &replacement).unwrap();
    index.locate(&old_key, id, &replacement).unwrap();
    let bytes = fs::read(&replacement).unwrap();
    let recovery = dir.data().join("recovery").join("snapshot.json");
    fs::create_dir_all(recovery.parent().unwrap()).unwrap();
    fs::write(&recovery, b"snapshot").unwrap();
    let located_key = index.entries()[0].path.clone();
    index.remove(&located_key, id).unwrap();
    assert!(index.entries().is_empty());
    assert_eq!(fs::read(&replacement).unwrap(), bytes);
    assert_eq!(fs::read(&recovery).unwrap(), b"snapshot");
    assert!(matches!(
        index.remove(&located_key, id),
        Err(RecentError::NotRegistered)
    ));
}

#[test]
fn bounded_eviction_and_malformed_index_are_non_destructive() {
    let dir = Directory::new();
    let source = dir.project("source.pmcab");
    let id = save(&source, "Cabinet").project().id;
    let original_bytes = fs::read(&source).unwrap();
    let mut index = RecentProjects::open(&dir.data()).unwrap();
    for n in 0..=MAX_RECENT_ENTRIES {
        let path = dir.project(&format!("copy-{n}.pmcab"));
        fs::copy(&source, &path).unwrap();
        index
            .register_successful(&path, id, time(n as u64))
            .unwrap();
    }
    assert_eq!(index.entries().len(), MAX_RECENT_ENTRIES);
    assert_eq!(
        index.entries()[0].last_used_unix_ms,
        MAX_RECENT_ENTRIES as u64
    );
    assert!(
        !index
            .entries()
            .iter()
            .any(|entry| entry.path == fs::canonicalize(dir.project("copy-0.pmcab")).unwrap())
    );
    assert!(matches!(
        RecentProjects::open(Path::new("relative/data")),
        Err(RecentError::InvalidDirectory)
    ));
    let index_path = dir.data().join("recent-projects.json");
    for invalid in [
        b"{bad".to_vec(),
        b"{\"version\":9,\"entries\":[]}".to_vec(),
        b"{\"version\":1,\"version\":1,\"entries\":[]}".to_vec(),
        vec![b' '; MAX_RECENT_INDEX_BYTES + 1],
    ] {
        fs::write(&index_path, &invalid).unwrap();
        assert!(RecentProjects::open(&dir.data()).is_err());
        assert_eq!(fs::read(&index_path).unwrap(), invalid);
        assert_eq!(fs::read(&source).unwrap(), original_bytes);
    }
}
