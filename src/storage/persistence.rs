//! Portable `.pmcab` documents: read versions 1/2, write version 2.
//! Migration is in memory only. Parsing prepares a separate editor;
//! the active editor changes only when the caller explicitly commits the result.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::commands::ProjectEditor;
use crate::domain::{DomainError, Project, SCHEMA_VERSION};

pub const EXTENSION: &str = "pmcab";
pub const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

/// A rename has committed the new bytes, but syncing the directory failed.
/// The caller must not report this as an ordinary pre-commit failure.
#[derive(Debug)]
pub enum SaveError {
    Prepare(PersistenceError),
    Io(io::Error),
    CommittedDurabilityUncertain(io::Error),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Prepare(error) => write!(f, "Cannot prepare project for saving: {error}"),
            Self::Io(error) => write!(f, "Cannot save project: {error}"),
            Self::CommittedDurabilityUncertain(error) => write!(
                f,
                "Project was replaced, but directory durability is uncertain: {error}"
            ),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Prepare(error) => Some(error),
            Self::Io(error) | Self::CommittedDurabilityUncertain(error) => Some(error),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Cancelled,
    Saved,
}

/// Save As picker cancellation does not touch the path, project, or dirty state.
pub fn save_as(
    editor: &mut ProjectEditor,
    current_path: &mut Option<PathBuf>,
    selected_path: Option<PathBuf>,
) -> Result<SaveOutcome, SaveError> {
    let Some(path) = selected_path else {
        return Ok(SaveOutcome::Cancelled);
    };
    save(editor, &path)?;
    *current_path = Some(path);
    Ok(SaveOutcome::Saved)
}

/// Serialize committed state and atomically replace an existing document.
/// The destination must be a normal file path on a filesystem supporting
/// atomic same-directory rename; no cross-device copy fallback is attempted.
pub fn save(editor: &mut ProjectEditor, path: &Path) -> Result<(), SaveError> {
    save_with_stages(editor, path, &NoFailure)
}

/// Atomically create a Save As destination without replacing a file that
/// appeared after the user chose it. Existing destinations require consent.
pub fn save_new(editor: &mut ProjectEditor, path: &Path) -> Result<(), SaveError> {
    let snapshot = editor.project().clone();
    let bytes = serialize(&snapshot).map_err(SaveError::Prepare)?;
    atomic_write_new_with_stages(path, &bytes, &NoFailure, || {})?;
    editor.mark_saved_snapshot(snapshot);
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveStage {
    Create,
    Write,
    Flush,
    SyncFile,
    Rename,
    SyncDirectory,
}

pub(crate) trait SaveStages {
    fn before(&self, _stage: SaveStage) -> io::Result<()> {
        Ok(())
    }
    fn after_snapshot(&self, _editor: &mut ProjectEditor) {}
    fn write(&self, file: &mut File, bytes: &[u8]) -> io::Result<()> {
        file.write_all(bytes)
    }
}

pub(crate) struct NoFailure;
impl SaveStages for NoFailure {}

struct TemporaryFile(PathBuf);
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn save_with_stages(
    editor: &mut ProjectEditor,
    path: &Path,
    stages: &impl SaveStages,
) -> Result<(), SaveError> {
    let snapshot = editor.project().clone();
    let bytes = serialize(&snapshot).map_err(SaveError::Prepare)?;
    atomic_write_with_stages(path, &bytes, stages, || stages.after_snapshot(editor))?;
    editor.mark_saved_snapshot(snapshot);
    Ok(())
}

/// Same-directory atomic replacement for other private project files (recovery).
/// A post-rename sync failure means the new bytes may already be present.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    atomic_write_with_stages(path, bytes, &NoFailure, || {})
}

pub(crate) fn atomic_write_with_stages(
    path: &Path,
    bytes: &[u8],
    stages: &impl SaveStages,
    after_snapshot: impl FnOnce(),
) -> Result<(), SaveError> {
    atomic_write_mode(path, bytes, stages, after_snapshot, true)
}

/// Install a new file without replacing one that appeared after a picker check.
/// A same-directory hard link is an atomic create-if-absent operation on the
/// supported local filesystems (unlike a rename with a preflight existence check).
pub(crate) fn atomic_write_new_with_stages(
    path: &Path,
    bytes: &[u8],
    stages: &impl SaveStages,
    after_snapshot: impl FnOnce(),
) -> Result<(), SaveError> {
    atomic_write_mode(path, bytes, stages, after_snapshot, false)
}

fn atomic_write_mode(
    path: &Path,
    bytes: &[u8],
    stages: &impl SaveStages,
    after_snapshot: impl FnOnce(),
    replace: bool,
) -> Result<(), SaveError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // A generated, exclusive same-directory name prevents both collisions and
    // cross-device replacement. Restrict permissions even before writing data.
    stages.before(SaveStage::Create).map_err(SaveError::Io)?;
    let (mut file, temporary) = loop {
        let candidate = parent.join(format!(".pmcab-{}.tmp", uuid::Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&candidate) {
            Ok(file) => break (file, TemporaryFile(candidate)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(SaveError::Io(error)),
        }
    };
    stages.before(SaveStage::Write).map_err(SaveError::Io)?;
    stages.write(&mut file, bytes).map_err(SaveError::Io)?;
    stages.before(SaveStage::Flush).map_err(SaveError::Io)?;
    file.flush().map_err(SaveError::Io)?;
    stages.before(SaveStage::SyncFile).map_err(SaveError::Io)?;
    file.sync_all().map_err(SaveError::Io)?;
    drop(file);
    after_snapshot();
    stages.before(SaveStage::Rename).map_err(SaveError::Io)?;
    if replace {
        fs::rename(&temporary.0, path).map_err(SaveError::Io)?;
    } else {
        fs::hard_link(&temporary.0, path).map_err(SaveError::Io)?;
        // The final name is already committed. A failure to remove the staging
        // name is post-commit uncertainty, never a claim that the old file won.
        fs::remove_file(&temporary.0).map_err(SaveError::CommittedDurabilityUncertain)?;
    }
    // The guard's old path no longer exists after rename. A directory fsync
    // makes the replacement durable on supported Unix filesystems.
    stages
        .before(SaveStage::SyncDirectory)
        .and_then(|()| File::open(parent)?.sync_all())
        .map_err(SaveError::CommittedDurabilityUncertain)?;
    Ok(())
}

#[derive(Debug)]
pub enum PersistenceError {
    TooLarge,
    Io(std::io::Error),
    Json(serde_json::Error),
    MissingVersion,
    UnsupportedVersion(u64),
    InvalidProject(DomainError),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(f, "Project exceeds the 16 MiB document limit"),
            Self::Io(error) => write!(f, "Cannot read project: {error}"),
            Self::Json(error) => write!(f, "Invalid or incomplete project JSON: {error}"),
            Self::MissingVersion => write!(f, "Missing or invalid schema_version in project file"),
            Self::UnsupportedVersion(version) => write!(
                f,
                "Project format version {version} is unsupported (this application supports version {SCHEMA_VERSION}); open it with a compatible application"
            ),
            Self::InvalidProject(error) => write!(f, "Invalid project data: {error:?}"),
        }
    }
}

impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

/// Serialize only committed, validated state. No UI locale affects stored units,
/// prices or catalog snapshots. The caller owns any subsequent disk write.
pub fn serialize(project: &Project) -> Result<Vec<u8>, PersistenceError> {
    validate(project)?;
    let mut document = project.clone();
    document
        .assign_missing_stock_aliases()
        .map_err(PersistenceError::InvalidProject)?;
    validate(&document)?;
    let bytes = serde_json::to_vec_pretty(&document).map_err(PersistenceError::Json)?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(PersistenceError::TooLarge);
    }
    Ok(bytes)
}

/// A fully parsed and validated replacement, including fresh session-local history.
pub struct PreparedProject(ProjectEditor, u64);

impl PreparedProject {
    pub fn project(&self) -> &Project {
        self.0.project()
    }

    /// On-disk schema before the validated in-memory migration.
    pub fn source_version(&self) -> u64 {
        self.1
    }

    /// Explicitly accept a prepared project. Undo history and previews belong to
    /// the previous session and are not carried over to the opened document.
    pub fn replace(self, editor: &mut ProjectEditor) {
        *editor = self.0;
    }

    pub fn into_editor(self) -> ProjectEditor {
        self.0
    }
}

/// Bounded read even for streams of unknown size; rejects extra bytes before JSON parsing.
pub fn prepare_reader(reader: impl Read) -> Result<PreparedProject, PersistenceError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(PersistenceError::Io)?;
    prepare_bytes(&bytes)
}

pub fn prepare_bytes(bytes: &[u8]) -> Result<PreparedProject, PersistenceError> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(PersistenceError::TooLarge);
    }
    // Parse through a duplicate-detecting value so unknown fields and nested
    // catalog dimension maps cannot silently overwrite earlier JSON keys.
    let value = parse_unique_json(bytes).map_err(PersistenceError::Json)?;
    let version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or(PersistenceError::MissingVersion)?;
    if !(1..=u64::from(SCHEMA_VERSION)).contains(&version) {
        return Err(PersistenceError::UnsupportedVersion(version));
    }
    let mut project: Project = serde_json::from_value(value).map_err(PersistenceError::Json)?;
    if version == 1 {
        // Validate the original typed payload before migration, retaining all
        // legacy defaults and exact stored values. Do not rebuild entities from
        // material defaults, the installed catalog, or manufacturing grids.
        project
            .validate_version(1)
            .map_err(PersistenceError::InvalidProject)?;
        project.stock_aliases.clear();
        project.next_stock_s_alias = 1;
        project.next_stock_o_alias = 1;
        project.schema_version = SCHEMA_VERSION;
    } else {
        // Version 2 predates catalog packs: its hinge snapshots have no origin,
        // arm (all were full overlay) or inset depth, which default exactly.
        // Version 3 predates slides and feet: no catalog items and no slide
        // installations, which also default exactly.
        project.schema_version = SCHEMA_VERSION;
        validate(&project)?;
    }
    project
        .assign_missing_stock_aliases()
        .map_err(PersistenceError::InvalidProject)?;
    validate(&project)?;
    Ok(PreparedProject(
        ProjectEditor::new(project).map_err(PersistenceError::InvalidProject)?,
        version,
    ))
}

pub(crate) fn parse_unique_json(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let UniqueValue(value) = UniqueValue::deserialize(&mut decoder)?;
    decoder.end()?;
    Ok(value)
}

fn validate(project: &Project) -> Result<(), PersistenceError> {
    project
        .validate()
        .map_err(PersistenceError::InvalidProject)?;
    Ok(())
}

struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON without duplicate object keys")
            }

            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::from(v)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::from(v)))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::from(v)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(v.into())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(v)))
            }
            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
                UniqueValue::deserialize(d)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueValue(value)) = seq.next_element::<UniqueValue>()? {
                    values.push(value);
                }
                Ok(UniqueValue(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some((key, UniqueValue(value))) =
                    map.next_entry::<String, UniqueValue>()?
                {
                    if values.insert(key.clone(), value).is_some() {
                        return Err(de::Error::custom(format!("duplicate JSON key: {key}")));
                    }
                }
                Ok(UniqueValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use uuid::Uuid;

    use crate::domain::{
        Allocation, Assembly, Board, BoardGrain, CatalogReference, Hardware, HardwareKind,
        Material, Stock, StockGrain, StockSource,
    };
    use crate::money::{Currency, Money, MoneyLocale};
    use crate::units::{Length, Pose, Quaternion, Unit};

    fn mm(v: i64) -> Length {
        Length::from_micrometres(v * 1000)
    }
    fn pose() -> Pose {
        Pose::new([1.0005, -2.0, 0.0], Quaternion::IDENTITY).unwrap()
    }

    fn fixture() -> Project {
        let mut p = Project::new("Armário", Currency::Brl);
        let material = Uuid::new_v4();
        let parent = Uuid::new_v4();
        let stock = Uuid::new_v4();
        let catalog = Uuid::new_v4();
        p.materials.push(Material {
            id: material,
            name: "Plywood".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Length,
        });
        p.assemblies.push(Assembly {
            id: parent,
            name: "Body".into(),
            parent_id: None,
            pose: pose(),
        });
        for _ in 0..2 {
            let board = Uuid::new_v4();
            p.boards.push(Board {
                id: board,
                name: "Side".into(),
                material_id: material,
                length: mm(100),
                width: mm(50),
                thickness: mm(18),
                grain_override: Some(BoardGrain::Width),
                parent_id: Some(parent),
                pose: pose(),
            });
            p.allocations.push(Allocation {
                id: Uuid::new_v4(),
                board_id: board,
                stock_id: stock,
                origin: [mm(10), mm(20)],
                quarter_turn: true,
                locked: true,
            });
        }
        p.stock.push(Stock {
            id: stock,
            name: "Sheet".into(),
            material_id: material,
            length: mm(300),
            width: mm(200),
            thickness: mm(18),
            grain: StockGrain::AlongY,
            source: StockSource::ToPurchase,
            price: Some(Money::new(Currency::Brl, 20_005).unwrap()),
            priority: 3,
            trim: [mm(1); 4],
        });
        p.stock_aliases.insert(stock, "S1".into());
        p.next_stock_s_alias = 2;
        p.catalog.push(CatalogReference {
            id: catalog,
            name: "Pinned".into(),
            product_id: "H-1".into(),
            plate_id: Some("P-1".into()),
            source: "reviewed".into(),
            revision: "2026".into(),
            installation_dimensions: HashMap::from([("cup_depth".into(), mm(11))]),
            verified_hinge: None,
            origin: None,
            item: None,
        });
        p.hardware.push(Hardware {
            id: Uuid::new_v4(),
            name: "Hinge".into(),
            parent_id: Some(parent),
            pose: pose(),
            kind: HardwareKind::Catalog {
                catalog_id: catalog,
            },
        });
        p.validate().unwrap();
        p
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("pmcab-save-test-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> PathBuf {
            self.0.join("project.pmcab")
        }

        fn entries(&self) -> Vec<PathBuf> {
            fs::read_dir(&self.0)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect()
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    struct FailAt(SaveStage);
    impl SaveStages for FailAt {
        fn before(&self, stage: SaveStage) -> io::Result<()> {
            if self.0 == stage && stage != SaveStage::Write {
                Err(io::Error::other(format!("injected {stage:?} failure")))
            } else {
                Ok(())
            }
        }

        fn write(&self, file: &mut File, bytes: &[u8]) -> io::Result<()> {
            if self.0 == SaveStage::Write {
                file.write_all(&bytes[..bytes.len() / 2])?;
                Err(io::Error::other(
                    "injected storage exhaustion after partial write",
                ))
            } else {
                file.write_all(bytes)
            }
        }
    }

    #[test]
    fn save_replaces_complete_file_and_marks_only_successful_revision() {
        let directory = TestDirectory::new();
        let path = directory.path();
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        save(&mut editor, &path).unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.name = "New version".into();
                Ok(())
            })
            .unwrap();
        assert!(editor.is_dirty());
        save(&mut editor, &path).unwrap();
        assert!(!editor.is_dirty());
        assert_eq!(editor.saved_revision(), Some(1));
        assert_eq!(
            prepare_reader(File::open(&path).unwrap())
                .unwrap()
                .project(),
            editor.project()
        );
        assert_eq!(directory.entries(), vec![path]);
    }

    #[test]
    fn save_new_refuses_existing_destination_without_marking_editor_saved() {
        let directory = TestDirectory::new();
        let path = directory.path();
        fs::write(&path, b"original").unwrap();
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        editor.set_grid_spacing(mm(20)).unwrap();
        assert!(
            matches!(save_new(&mut editor, &path), Err(SaveError::Io(e)) if e.kind() == io::ErrorKind::AlreadyExists)
        );
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert!(editor.is_dirty());
        assert_eq!(directory.entries(), vec![path]);
    }

    #[test]
    fn pre_rename_failures_keep_old_bytes_dirty_revision_and_clean_temporary_file() {
        for stage in [
            SaveStage::Create,
            SaveStage::Write,
            SaveStage::Flush,
            SaveStage::SyncFile,
            SaveStage::Rename,
        ] {
            let directory = TestDirectory::new();
            let path = directory.path();
            let mut editor = ProjectEditor::new(fixture()).unwrap();
            save(&mut editor, &path).unwrap();
            let previous = fs::read(&path).unwrap();
            editor
                .transact(|p| -> Result<(), ()> {
                    p.name = "Unsaved".into();
                    Ok(())
                })
                .unwrap();
            assert!(
                matches!(
                    save_with_stages(&mut editor, &path, &FailAt(stage)),
                    Err(SaveError::Io(_))
                ),
                "{stage:?}"
            );
            assert_eq!(fs::read(&path).unwrap(), previous, "{stage:?}");
            assert_eq!(directory.entries(), vec![path], "{stage:?}");
            assert_eq!(editor.saved_revision(), Some(0));
            assert_eq!(editor.project().revision, 1);
            assert!(editor.is_dirty());
        }
    }

    #[test]
    fn directory_sync_failure_reports_committed_but_uncertain_and_keeps_dirty() {
        let directory = TestDirectory::new();
        let path = directory.path();
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        save(&mut editor, &path).unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.name = "Written".into();
                Ok(())
            })
            .unwrap();
        assert!(matches!(
            save_with_stages(&mut editor, &path, &FailAt(SaveStage::SyncDirectory)),
            Err(SaveError::CommittedDurabilityUncertain(_))
        ));
        assert_eq!(
            prepare_reader(File::open(&path).unwrap())
                .unwrap()
                .project()
                .name,
            "Written"
        );
        assert_eq!(directory.entries(), vec![path]);
        assert_eq!(editor.saved_revision(), Some(0));
        assert!(editor.is_dirty());
    }

    #[test]
    fn cancelled_save_as_leaves_path_and_unsaved_editor_untouched() {
        let directory = TestDirectory::new();
        let original = directory.path();
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        save(&mut editor, &original).unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.name = "Unsaved".into();
                Ok(())
            })
            .unwrap();
        editor.begin_preview();
        let mut current_path = Some(original.clone());
        let project = editor.project().clone();
        assert_eq!(
            save_as(&mut editor, &mut current_path, None).unwrap(),
            SaveOutcome::Cancelled
        );
        assert_eq!(current_path, Some(original.clone()));
        assert_eq!(editor.project(), &project);
        assert!(editor.preview().is_some());
        assert!(editor.is_dirty());
        let new_path = directory.0.join("other.pmcab");
        assert_eq!(
            save_as(&mut editor, &mut current_path, Some(new_path.clone())).unwrap(),
            SaveOutcome::Saved
        );
        assert_eq!(current_path, Some(new_path.clone()));
        assert!(!editor.is_dirty());
        assert_eq!(fs::read(new_path).unwrap(), serialize(&project).unwrap());
    }

    struct EditAfterSnapshot;
    impl SaveStages for EditAfterSnapshot {
        fn after_snapshot(&self, editor: &mut ProjectEditor) {
            editor
                .transact(|p| -> Result<(), ()> {
                    p.name = "Newer edit".into();
                    Ok(())
                })
                .unwrap();
        }
    }

    #[test]
    fn edit_during_save_does_not_mark_newer_revision_as_saved() {
        let directory = TestDirectory::new();
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        let snapshot = editor.project().clone();
        save_with_stages(&mut editor, &directory.path(), &EditAfterSnapshot).unwrap();
        assert_eq!(
            prepare_reader(File::open(directory.path()).unwrap())
                .unwrap()
                .project(),
            &snapshot
        );
        assert_eq!(editor.saved_revision(), Some(snapshot.revision));
        assert_eq!(editor.project().name, "Newer edit");
        assert!(editor.is_dirty());
    }

    #[test]
    fn round_trip_preserves_physical_values_and_pinned_catalog_across_locales() {
        let mut p = fixture();
        let bytes = serialize(&p).unwrap();
        p.display_unit = Unit::Inch;
        let other = prepare_bytes(&serialize(&p).unwrap())
            .unwrap()
            .into_editor();
        assert_eq!(other.project(), &p);
        assert_eq!(
            other.project().stock[0]
                .price
                .unwrap()
                .display(MoneyLocale::English),
            "BRL 200.05"
        );
        assert_eq!(
            other.project().stock[0]
                .price
                .unwrap()
                .display(MoneyLocale::PortugueseBrazil),
            "BRL 200,05"
        );
        assert_eq!(other.project().boards[0].length.micrometres(), 100_000);
        assert_ne!(other.project().boards[0].id, other.project().boards[1].id);
        assert_eq!(
            other.project().allocations[0].board_id,
            other.project().boards[0].id
        );
        assert_eq!(other.project().catalog[0].revision, "2026");
        assert_eq!(
            prepare_bytes(&bytes).unwrap().project().display_unit,
            Unit::Mm
        );
    }

    #[test]
    fn invalid_or_newer_load_preserves_unsaved_work_and_history() {
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.name = "Unsaved".into();
                Ok(())
            })
            .unwrap();
        editor.begin_preview();
        let before = editor.project().clone();
        let valid = serde_json::to_value(fixture()).unwrap();
        let mut cases = Vec::new();
        for version in [SCHEMA_VERSION + 1, 0] {
            let mut v = valid.clone();
            v["schema_version"] = Value::from(version);
            cases.push(serde_json::to_vec(&v).unwrap());
        }
        for key in ["materials", "currency"] {
            let mut v = valid.clone();
            v.as_object_mut().unwrap().remove(key);
            cases.push(serde_json::to_vec(&v).unwrap());
        }
        let mut dangling = valid.clone();
        dangling["boards"][0]["material_id"] = Value::from(Uuid::new_v4().to_string());
        cases.push(serde_json::to_vec(&dangling).unwrap());
        let mut cycle = valid.clone();
        let assembly_id = cycle["assemblies"][0]["id"].clone();
        cycle["assemblies"][0]["parent_id"] = assembly_id;
        cases.push(serde_json::to_vec(&cycle).unwrap());
        let mut bad_dimension = valid.clone();
        bad_dimension["boards"][0]["length"] = Value::from(0);
        cases.push(serde_json::to_vec(&bad_dimension).unwrap());
        let mut bad_price = valid.clone();
        bad_price["stock"][0]["price"]["minor_units"] = Value::from(-1);
        cases.push(serde_json::to_vec(&bad_price).unwrap());
        let mut bad_catalog = valid.clone();
        bad_catalog["catalog"][0]["installation_dimensions"]["cup_depth"] = Value::from(-1);
        cases.push(serde_json::to_vec(&bad_catalog).unwrap());
        let mut bad_pose = valid;
        bad_pose["boards"][0]["pose"]["rotation"]["w"] = Value::from(2);
        cases.push(serde_json::to_vec(&bad_pose).unwrap());
        cases.push(b"{\"schema_version\":1,\"schema_version\":2}".to_vec());
        cases.push(
            b"{\"schema_version\":1,\"catalog\": [{\"revision\":\"a\",\"revision\":\"b\"}]}"
                .to_vec(),
        );
        cases.push(b"{\"schema_version\":1,\"pose\":1e999}".to_vec());
        cases.push(vec![0xff, 0xfe]);
        cases.push(vec![b' '; MAX_DOCUMENT_BYTES + 1]);
        for bytes in cases {
            assert!(prepare_bytes(&bytes).is_err());
            assert_eq!(editor.project(), &before);
            assert!(editor.is_dirty());
            assert!(editor.can_undo());
            assert!(editor.preview().is_some());
        }
        assert!(matches!(
            prepare_bytes(b"{\"schema_version\":5}"),
            Err(PersistenceError::UnsupportedVersion(5))
        ));
        assert!(matches!(
            prepare_bytes(b"{\"schema_version\":4294967296}"),
            Err(PersistenceError::UnsupportedVersion(4294967296))
        ));
        assert!(matches!(
            prepare_reader(std::io::repeat(b' ')),
            Err(PersistenceError::TooLarge)
        ));
    }

    #[test]
    fn replacement_is_explicit_and_resets_session_history() {
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.name = "Unsaved".into();
                Ok(())
            })
            .unwrap();
        let target = fixture();
        let prepared = prepare_reader(serialize(&target).unwrap().as_slice()).unwrap();
        assert_eq!(editor.project().name, "Unsaved");
        prepared.replace(&mut editor);
        assert_eq!(editor.project(), &target);
        assert!(!editor.is_dirty());
        assert!(!editor.can_undo());
        assert!(editor.preview().is_none());
    }

    #[test]
    fn legacy_grid_default_does_not_rewrite_file_until_save() {
        let directory = TestDirectory::new();
        let path = directory.path();
        let mut project = fixture();
        project.boards[0].pose.translation_mm[0] = 0.0005;
        let original_pose = project.boards[0].pose;
        let mut json = serde_json::to_value(&project).unwrap();
        json.as_object_mut().unwrap().remove("grid_spacing");
        let old_bytes = serde_json::to_vec_pretty(&json).unwrap();
        fs::write(&path, &old_bytes).unwrap();
        let mut editor = prepare_reader(File::open(&path).unwrap())
            .unwrap()
            .into_editor();
        assert_eq!(
            editor.project().grid_spacing,
            crate::domain::DEFAULT_GRID_SPACING
        );
        assert_eq!(editor.project().boards[0].pose, original_pose);
        assert_eq!(editor.project().revision, project.revision);
        assert!(!editor.is_dirty());
        assert_eq!(fs::read(&path).unwrap(), old_bytes);
        save(&mut editor, &path).unwrap();
        assert_eq!(editor.project().boards[0].pose, original_pose);
        assert_eq!(
            prepare_reader(File::open(&path).unwrap())
                .unwrap()
                .project(),
            editor.project()
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap()["grid_spacing"],
            10_000
        );
    }

    #[test]
    fn half_inch_grid_survives_save_reopen_without_pose_quantization() {
        let directory = TestDirectory::new();
        let mut project = fixture();
        project.boards[0].pose.translation_mm = [0.0005, -1.0005, 3.0005];
        let original_pose = project.boards[0].pose;
        let mut editor = ProjectEditor::new(project).unwrap();
        let spacing = crate::units::Length::from_inch_fraction("1/2")
            .unwrap()
            .exact()
            .unwrap();
        editor.set_grid_spacing(spacing).unwrap();
        save(&mut editor, &directory.path()).unwrap();
        let reopened = prepare_reader(File::open(directory.path()).unwrap())
            .unwrap()
            .into_editor();
        assert_eq!(
            reopened.project().grid_spacing,
            Length::from_micrometres(12_700)
        );
        assert_eq!(reopened.project().boards[0].pose, original_pose);
        assert_eq!(reopened.project().revision, 1);
        assert!(!reopened.is_dirty());
    }

    #[test]
    fn cutting_kerf_is_positive_and_survives_save_reopen_with_legacy_default() {
        let directory = TestDirectory::new();
        let mut json = serde_json::to_value(fixture()).unwrap();
        json.as_object_mut().unwrap().remove("cutting_kerf");
        let original = serde_json::to_vec(&json).unwrap();
        fs::write(directory.path(), &original).unwrap();
        let mut editor = prepare_reader(File::open(directory.path()).unwrap())
            .unwrap()
            .into_editor();
        assert_eq!(
            editor.project().cutting_kerf,
            crate::domain::DEFAULT_CUTTING_KERF
        );
        assert_eq!(fs::read(directory.path()).unwrap(), original);
        assert!(!editor.is_dirty());
        editor.set_cutting_kerf(mm(3)).unwrap();
        save(&mut editor, &directory.path()).unwrap();
        let reopened = prepare_reader(File::open(directory.path()).unwrap())
            .unwrap()
            .into_editor();
        assert_eq!(reopened.project().cutting_kerf, mm(3));
        assert!(!reopened.is_dirty());
        for invalid in [0, -1] {
            json["cutting_kerf"] = Value::from(invalid);
            assert!(matches!(
                prepare_bytes(&serde_json::to_vec(&json).unwrap()),
                Err(PersistenceError::InvalidProject(
                    DomainError::InvalidCuttingKerf(_)
                ))
            ));
        }
    }

    #[test]
    fn invalid_grid_in_file_is_rejected_without_replacing_editor() {
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        editor.set_grid_spacing(mm(20)).unwrap();
        let before = editor.project().clone();
        for spacing in [0, -1, 1_000_000_001] {
            let mut json = serde_json::to_value(fixture()).unwrap();
            json["grid_spacing"] = Value::from(spacing);
            assert!(matches!(
                prepare_bytes(&serde_json::to_vec(&json).unwrap()),
                Err(PersistenceError::InvalidProject(
                    DomainError::InvalidGridSpacing(_)
                ))
            ));
            assert_eq!(editor.project(), &before);
            assert!(editor.can_undo());
        }
    }
}
