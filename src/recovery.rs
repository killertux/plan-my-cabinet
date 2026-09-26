//! Headless, opt-in recovery snapshots. The desktop integration supplies its
//! platform user-data directory and calls `note_committed_edit`/`tick` on its
//! event loop; no working-directory fallback or background thread is used.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::commands::ProjectEditor;
use crate::domain::Project;
use crate::persistence::{self, MAX_DOCUMENT_BYTES, PersistenceError, SaveError};

pub const AUTOSAVE_DELAY: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub enum RecoveryError {
    Io(io::Error),
    Save(SaveError),
    Invalid(PersistenceError),
    Json(serde_json::Error),
    WrongProject,
}

impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "Cannot access recovery: {e}"),
            Self::Save(e) => write!(f, "Cannot write recovery: {e}"),
            Self::Invalid(e) => write!(f, "Invalid recovery project: {e}"),
            Self::Json(e) => write!(f, "Invalid recovery record: {e}"),
            Self::WrongProject => write!(f, "Recovery does not belong to this project and path"),
        }
    }
}

impl std::error::Error for RecoveryError {}

#[derive(Serialize, Deserialize)]
struct Record {
    project_id: Uuid,
    path_key: String,
    project: Project,
}

/// One saved document at one location. Moving/copying a document to another
/// path does not accidentally adopt recovery from the original path.
pub struct RecoveryStore {
    file: PathBuf,
    saved_path: PathBuf,
    project_id: Uuid,
    key: String,
    pending: Option<(u64, Instant)>,
    observed_revision: Option<u64>,
}

impl RecoveryStore {
    /// `user_data_dir` must be the platform application's user-data directory.
    /// `saved_path` must exist, or its parent directory must exist. Resolving
    /// the parent also collapses `..` and symlinks before computing the key.
    pub fn new(user_data_dir: &Path, saved_path: &Path, project_id: Uuid) -> io::Result<Self> {
        let saved_path = match fs::canonicalize(saved_path) {
            Ok(path) => path,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let parent = saved_path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
                let name = saved_path.file_name().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "project path needs a filename")
                })?;
                fs::canonicalize(parent)?.join(name)
            }
            Err(e) => return Err(e),
        };
        let mut hash = Sha256::new();
        hash.update(project_id.as_bytes());
        // Hash the platform's actual path bytes, not its potentially lossy display.
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            hash.update(saved_path.as_os_str().as_bytes());
        }
        #[cfg(not(unix))]
        hash.update(saved_path.to_string_lossy().as_bytes());
        let key = format!("{:x}", hash.finalize());
        let file = user_data_dir.join("recovery").join(format!("{key}.json"));
        if file == saved_path {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "recovery and saved paths coincide",
            ));
        }
        Ok(Self {
            file,
            saved_path,
            project_id,
            key,
            pending: None,
            observed_revision: None,
        })
    }

    pub fn recovery_path(&self) -> &Path {
        &self.file
    }

    /// Call after a successful committed edit, including undo/redo; previews and
    /// no-op edits must not call this. The timestamp is supplied by the caller.
    pub fn note_committed_edit(
        &mut self,
        editor: &ProjectEditor,
        now: Instant,
    ) -> Result<(), RecoveryError> {
        self.check_editor(editor)?;
        self.observed_revision = Some(editor.project().revision);
        self.pending = editor
            .is_dirty()
            .then_some((editor.project().revision, now));
        Ok(())
    }

    /// On the first tick after an unreported edit, start the inactivity clock.
    /// Writes at most once per revision and retries a failed write on later ticks.
    pub fn tick(&mut self, editor: &ProjectEditor, now: Instant) -> Result<bool, RecoveryError> {
        self.check_editor(editor)?;
        let revision = editor.project().revision;
        if self.observed_revision != Some(revision) {
            self.note_committed_edit(editor, now)?;
        }
        if !editor.is_dirty() {
            self.pending = None;
            return Ok(false);
        }
        let Some((pending_revision, since)) = self.pending else {
            return Ok(false);
        };
        if pending_revision != revision || now.saturating_duration_since(since) < AUTOSAVE_DELAY {
            return Ok(false);
        }
        // Serialize only the committed model; a preview is never inspected.
        let project = editor.project().clone();
        let record = Record {
            project_id: self.project_id,
            path_key: self.key.clone(),
            project,
        };
        persistence::serialize(&record.project).map_err(RecoveryError::Invalid)?;
        let bytes = serde_json::to_vec(&record).map_err(RecoveryError::Json)?;
        fs::create_dir_all(self.file.parent().expect("recovery has a parent"))
            .map_err(RecoveryError::Io)?;
        // The recovery destination is derived exclusively from a digest, never
        // from caller-controlled filename components.
        persistence::atomic_write(&self.file, &bytes).map_err(RecoveryError::Save)?;
        self.pending = None;
        Ok(true)
    }

    /// Inspect a recovery without changing either file or the active editor.
    /// Corrupt records return an error and are retained for diagnostics/choice.
    pub fn inspect(&self, saved: &Project) -> Result<Option<RecoveryCandidate>, RecoveryError> {
        if saved.id != self.project_id {
            return Err(RecoveryError::WrongProject);
        }
        let file = match File::open(&self.file) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(RecoveryError::Io(e)),
        };
        let mut bytes = Vec::new();
        file.take(MAX_DOCUMENT_BYTES as u64 + 4097)
            .read_to_end(&mut bytes)
            .map_err(RecoveryError::Io)?;
        if bytes.len() > MAX_DOCUMENT_BYTES + 4096 {
            return Err(RecoveryError::Invalid(PersistenceError::TooLarge));
        }
        let value = persistence::parse_unique_json(&bytes).map_err(RecoveryError::Json)?;
        let record: Record = serde_json::from_value(value).map_err(RecoveryError::Json)?;
        if record.project_id != self.project_id
            || record.path_key != self.key
            || record.project.id != saved.id
        {
            return Err(RecoveryError::WrongProject);
        }
        let validated = persistence::prepare_bytes(
            &persistence::serialize(&record.project).map_err(RecoveryError::Invalid)?,
        )
        .map_err(RecoveryError::Invalid)?;
        if validated.project().revision <= saved.revision {
            return Ok(None);
        }
        let mut editor = validated.into_editor();
        editor.mark_saved_snapshot(saved.clone());
        if !editor.is_dirty() {
            return Ok(None);
        }
        Ok(Some(RecoveryCandidate {
            project_id: saved.id,
            project_name: saved.name.clone(),
            saved_revision: saved.revision,
            recovery_revision: editor.project().revision,
            saved_path: self.saved_path.clone(),
            editor,
        }))
    }

    fn check_editor(&self, editor: &ProjectEditor) -> Result<(), RecoveryError> {
        if editor.project().id != self.project_id {
            Err(RecoveryError::WrongProject)
        } else {
            Ok(())
        }
    }
}

/// Validated snapshot plus metadata for presenting an explicit choice.
pub struct RecoveryCandidate {
    pub project_id: Uuid,
    pub project_name: String,
    pub saved_revision: u64,
    pub recovery_revision: u64,
    pub saved_path: PathBuf,
    editor: ProjectEditor,
}

pub enum RecoveryChoice {
    Recover,
    Discard,
    Defer,
}

impl RecoveryCandidate {
    /// Recover returns a dirty editor; discard removes only the recovery file;
    /// defer retains both. The caller replaces its active editor only on Recover.
    pub fn resolve(
        self,
        store: &RecoveryStore,
        choice: RecoveryChoice,
    ) -> Result<Option<ProjectEditor>, RecoveryError> {
        if self.project_id != store.project_id || self.saved_path != store.saved_path {
            return Err(RecoveryError::WrongProject);
        }
        match choice {
            RecoveryChoice::Recover => Ok(Some(self.editor)),
            RecoveryChoice::Discard => {
                fs::remove_file(&store.file).map_err(RecoveryError::Io)?;
                Ok(None)
            }
            RecoveryChoice::Defer => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("pmcab-recovery-test-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn edit(editor: &mut ProjectEditor, name: &str) {
        editor
            .transact(|project| -> Result<(), ()> {
                project.name = name.into();
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn interrupted_session_recovers_only_committed_edits_without_touching_saved_file() {
        let directory = Directory::new();
        let path = directory.0.join("cabinet.pmcab");
        let mut editor = ProjectEditor::new(Project::new("Original", Currency::Usd)).unwrap();
        persistence::save(&mut editor, &path).unwrap();
        let saved_bytes = fs::read(&path).unwrap();
        let saved = editor.project().clone();
        let mut store =
            RecoveryStore::new(&directory.0.join("user-data"), &path, saved.id).unwrap();
        let now = Instant::now();
        edit(&mut editor, "Committed");
        store.note_committed_edit(&editor, now).unwrap();
        editor.begin_preview();
        editor
            .update_preview(|p| -> Result<(), ()> {
                p.name = "Preview".into();
                Ok(())
            })
            .unwrap();
        assert!(!store.tick(&editor, now + Duration::from_secs(29)).unwrap());
        assert!(store.tick(&editor, now + AUTOSAVE_DELAY).unwrap());
        assert!(!store.tick(&editor, now + Duration::from_secs(90)).unwrap());
        assert_eq!(fs::read(&path).unwrap(), saved_bytes);
        drop(editor);
        drop(store);

        let store = RecoveryStore::new(&directory.0.join("user-data"), &path, saved.id).unwrap();
        let candidate = store.inspect(&saved).unwrap().unwrap();
        assert_eq!(candidate.project_name, "Original");
        assert_eq!(
            (candidate.saved_revision, candidate.recovery_revision),
            (0, 1)
        );
        let recovered = candidate
            .resolve(&store, RecoveryChoice::Recover)
            .unwrap()
            .unwrap();
        assert_eq!(recovered.project().name, "Committed");
        assert!(recovered.preview().is_none());
        assert!(recovered.is_dirty());
        assert!(!recovered.can_undo());
        assert_eq!(fs::read(&path).unwrap(), saved_bytes);

        let candidate = store.inspect(&saved).unwrap().unwrap();
        assert!(
            candidate
                .resolve(&store, RecoveryChoice::Defer)
                .unwrap()
                .is_none()
        );
        assert!(store.recovery_path().exists());
        let candidate = store.inspect(&saved).unwrap().unwrap();
        assert!(
            candidate
                .resolve(&store, RecoveryChoice::Discard)
                .unwrap()
                .is_none()
        );
        assert!(store.inspect(&saved).unwrap().is_none());
        assert_eq!(fs::read(path).unwrap(), saved_bytes);
    }

    #[test]
    fn delayed_edit_and_stale_or_corrupt_recovery() {
        let directory = Directory::new();
        let path = directory.0.join("cabinet.pmcab");
        let mut editor = ProjectEditor::new(Project::new("Original", Currency::Usd)).unwrap();
        persistence::save(&mut editor, &path).unwrap();
        let mut store = RecoveryStore::new(&directory.0, &path, editor.project().id).unwrap();
        let now = Instant::now();
        editor.begin_preview();
        editor
            .update_preview(|p| -> Result<(), ()> {
                p.name = "Uncommitted".into();
                Ok(())
            })
            .unwrap();
        assert!(!store.tick(&editor, now + AUTOSAVE_DELAY).unwrap());
        assert!(!store.recovery_path().exists());
        editor.cancel_preview();
        edit(&mut editor, "First");
        assert!(!store.tick(&editor, now).unwrap());
        edit(&mut editor, "Second");
        store
            .note_committed_edit(&editor, now + Duration::from_secs(20))
            .unwrap();
        assert!(!store.tick(&editor, now + Duration::from_secs(40)).unwrap());
        assert!(store.tick(&editor, now + Duration::from_secs(50)).unwrap());
        persistence::save(&mut editor, &path).unwrap();
        assert!(store.inspect(editor.project()).unwrap().is_none());
        let mut ahead = editor.project().clone();
        ahead.revision += 1;
        assert!(store.inspect(&ahead).unwrap().is_none());
        fs::write(store.recovery_path(), b"{broken").unwrap();
        assert!(matches!(
            store.inspect(editor.project()),
            Err(RecoveryError::Json(_))
        ));
        assert!(store.recovery_path().exists());
        fs::write(
            store.recovery_path(),
            b"{\"project_id\":1,\"project_id\":2}",
        )
        .unwrap();
        assert!(matches!(
            store.inspect(editor.project()),
            Err(RecoveryError::Json(_))
        ));
    }

    #[test]
    fn same_project_different_paths_and_canonical_aliases() {
        let directory = Directory::new();
        fs::create_dir(directory.0.join("sub")).unwrap();
        let first = directory.0.join("first.pmcab");
        let second = directory.0.join("second.pmcab");
        let mut editor = ProjectEditor::new(Project::new("Original", Currency::Usd)).unwrap();
        persistence::save(&mut editor, &first).unwrap();
        fs::copy(&first, &second).unwrap();
        let a = RecoveryStore::new(&directory.0, &first, editor.project().id).unwrap();
        let alias = RecoveryStore::new(
            &directory.0,
            &directory.0.join("sub/../first.pmcab"),
            editor.project().id,
        )
        .unwrap();
        let b = RecoveryStore::new(&directory.0, &second, editor.project().id).unwrap();
        assert_eq!(a.recovery_path(), alias.recovery_path());
        assert_ne!(a.recovery_path(), b.recovery_path());
        let saved = editor.project().clone();
        edit(&mut editor, "Only first");
        let mut a = a;
        let now = Instant::now();
        a.note_committed_edit(&editor, now).unwrap();
        a.tick(&editor, now + AUTOSAVE_DELAY).unwrap();
        assert!(b.inspect(&saved).unwrap().is_none());
        assert!(a.inspect(&saved).unwrap().is_some());
    }
}
