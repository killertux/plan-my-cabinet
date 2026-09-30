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
const MAX_INDEX_BYTES: usize = 128 * 1024;
const MAX_INDEX_ENTRIES: usize = 256;

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
        self.write_now(editor)?;
        Ok(true)
    }

    /// Write the committed project immediately, skipping the inactivity delay.
    /// Used when the app is about to terminate abnormally. A clean editor has
    /// nothing to recover and writes nothing.
    pub fn write_now(&mut self, editor: &ProjectEditor) -> Result<(), RecoveryError> {
        self.check_editor(editor)?;
        if !editor.is_dirty() {
            return Ok(());
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
        Ok(())
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
        // Keep the payload untyped until it passes the same version dispatch,
        // migration and validation as a portable file (including future versions).
        #[derive(Deserialize)]
        struct ReadRecord {
            project_id: Uuid,
            path_key: String,
            project: serde_json::Value,
        }
        let record: ReadRecord = serde_json::from_value(value).map_err(RecoveryError::Json)?;
        if record.project_id != self.project_id || record.path_key != self.key {
            return Err(RecoveryError::WrongProject);
        }
        let validated = persistence::prepare_bytes(
            &serde_json::to_vec(&record.project).map_err(RecoveryError::Json)?,
        )
        .map_err(RecoveryError::Invalid)?;
        if validated.project().id != saved.id {
            return Err(RecoveryError::WrongProject);
        }
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

/// A local registration, never part of a portable `.pmcab` document. An
/// untitled identity has an explicit null path, not a guessed future Save As
/// destination. Paths in this index are canonical and UTF-8 so the index can
/// be serialized losslessly; non-UTF-8 project paths still work with the
/// original `RecoveryStore` API.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryIdentity {
    pub project_id: Uuid,
    pub saved_path: Option<PathBuf>,
}

#[derive(Debug)]
pub enum DiscoveryStatus {
    /// Valid snapshot ahead of the validated saved document.
    Newer,
    /// Valid snapshot, but no newer committed work than the saved document.
    NotNewer,
    /// Valid untitled snapshot; there is no saved revision to compare.
    Untitled,
    /// A saved document cannot currently be validated (including a missing
    /// file). Do not offer recovery against an unverified saved document.
    SavedUnavailable(String),
    /// The snapshot or its identity is invalid. It remains on disk.
    Invalid(String),
}

#[derive(Debug)]
pub struct RecoveryDiscovery {
    pub identity: RecoveryIdentity,
    pub snapshot_path: PathBuf,
    pub snapshot_modified: Option<std::time::SystemTime>,
    pub saved_revision: Option<u64>,
    pub recovery_revision: Option<u64>,
    pub status: DiscoveryStatus,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiscoveryIndex {
    version: u8,
    entries: Vec<RecoveryIdentity>,
}

/// Explicit, bounded discovery for registered projects only. Constructing or
/// reading the index never scans a project folder or writes a snapshot.
pub struct RecoveryIndex {
    user_data_dir: PathBuf,
    entries: Vec<RecoveryIdentity>,
}

impl RecoveryIndex {
    pub fn open(user_data_dir: &Path) -> Result<Self, RecoveryError> {
        let path = user_data_dir.join("recovery-index.json");
        let entries = match File::open(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(RecoveryError::Io(e)),
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_INDEX_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(RecoveryError::Io)?;
                if bytes.len() > MAX_INDEX_BYTES {
                    return Err(RecoveryError::Invalid(PersistenceError::TooLarge));
                }
                let value = persistence::parse_unique_json(&bytes).map_err(RecoveryError::Json)?;
                let index: DiscoveryIndex =
                    serde_json::from_value(value).map_err(RecoveryError::Json)?;
                if index.version != 1 || index.entries.len() > MAX_INDEX_ENTRIES {
                    return Err(invalid_index());
                }
                for (position, identity) in index.entries.iter().enumerate() {
                    if index.entries[..position].contains(identity) || !valid_identity(identity)? {
                        return Err(invalid_index());
                    }
                }
                index.entries
            }
        };
        Ok(Self {
            user_data_dir: user_data_dir.to_path_buf(),
            entries,
        })
    }

    pub fn entries(&self) -> &[RecoveryIdentity] {
        &self.entries
    }

    /// The platform file manager may open this directory on request. This
    /// accessor does not create it or scan other folders.
    pub fn recovery_folder(&self) -> PathBuf {
        self.user_data_dir.join("recovery")
    }

    /// A fresh review has no selected snapshots. Rows include invalid records
    /// so they can be diagnosed or explicitly selected for removal.
    pub fn cleanup_review(&self) -> RecoveryCleanupReview {
        RecoveryCleanupReview {
            rows: self.discover(),
            selected: Vec::new(),
        }
    }

    /// Call only after a successful open or save. Canonical aliases register
    /// one identity; a copy at another path remains a different identity.
    pub fn register_saved(&mut self, path: &Path, project_id: Uuid) -> Result<(), RecoveryError> {
        let path = canonical_project_path(path).map_err(RecoveryError::Io)?;
        let identity = RecoveryIdentity {
            project_id,
            saved_path: Some(path),
        };
        self.register(identity)
    }

    /// Register an explicitly unsaved project, never a synthetic saved path.
    pub fn register_untitled(&mut self, project_id: Uuid) -> Result<(), RecoveryError> {
        self.register(RecoveryIdentity {
            project_id,
            saved_path: None,
        })
    }

    fn register(&mut self, identity: RecoveryIdentity) -> Result<(), RecoveryError> {
        if !valid_identity(&identity)? {
            return Err(invalid_index());
        }
        if self.entries.contains(&identity) {
            return Ok(());
        }
        if self.entries.len() == MAX_INDEX_ENTRIES {
            return Err(invalid_index());
        }
        let mut entries = self.entries.clone();
        entries.push(identity);
        let bytes = serde_json::to_vec(&DiscoveryIndex {
            version: 1,
            entries: entries.clone(),
        })
        .map_err(RecoveryError::Json)?;
        if bytes.len() > MAX_INDEX_BYTES {
            return Err(RecoveryError::Invalid(PersistenceError::TooLarge));
        }
        fs::create_dir_all(&self.user_data_dir).map_err(RecoveryError::Io)?;
        persistence::atomic_write(&self.user_data_dir.join("recovery-index.json"), &bytes)
            .map_err(RecoveryError::Save)?;
        self.entries = entries;
        Ok(())
    }

    /// Examine only hash-derived files for registered identities. Every result
    /// is revalidated against the current index and snapshot payload; no
    /// cached metadata is treated as evidence that recovery is safe.
    pub fn discover(&self) -> Vec<RecoveryDiscovery> {
        self.entries
            .iter()
            .filter_map(|identity| {
                let key = match &identity.saved_path {
                    Some(path) => recovery_key(identity.project_id, path),
                    None => untitled_key(identity.project_id),
                };
                let snapshot_path = self
                    .user_data_dir
                    .join("recovery")
                    .join(format!("{key}.json"));
                let metadata = match fs::metadata(&snapshot_path) {
                    Ok(metadata) => metadata,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
                    Err(e) => {
                        return Some(RecoveryDiscovery {
                            identity: identity.clone(),
                            snapshot_path,
                            snapshot_modified: None,
                            saved_revision: None,
                            recovery_revision: None,
                            status: DiscoveryStatus::Invalid(e.to_string()),
                        });
                    }
                };
                let mut item = RecoveryDiscovery {
                    identity: identity.clone(),
                    snapshot_path: snapshot_path.clone(),
                    snapshot_modified: metadata.modified().ok(),
                    saved_revision: None,
                    recovery_revision: None,
                    status: DiscoveryStatus::Invalid("snapshot not yet validated".into()),
                };
                let snapshot = match read_snapshot(&snapshot_path, identity, &key) {
                    Ok(project) => project,
                    Err(e) => {
                        item.status = DiscoveryStatus::Invalid(e.to_string());
                        return Some(item);
                    }
                };
                item.recovery_revision = Some(snapshot.revision);
                item.status = match &identity.saved_path {
                    None => DiscoveryStatus::Untitled,
                    Some(path) => match fs::canonicalize(path)
                        .map_err(RecoveryError::Io)
                        .and_then(|actual| {
                            if &actual == path {
                                Ok(actual)
                            } else {
                                Err(RecoveryError::WrongProject)
                            }
                        })
                        .and_then(|actual| read_bounded(&actual, MAX_DOCUMENT_BYTES))
                        .and_then(|bytes| {
                            persistence::prepare_bytes(&bytes).map_err(RecoveryError::Invalid)
                        }) {
                        Err(e) => DiscoveryStatus::SavedUnavailable(e.to_string()),
                        Ok(saved) if saved.project().id != identity.project_id => {
                            DiscoveryStatus::SavedUnavailable(
                                "saved project identity changed".into(),
                            )
                        }
                        Ok(saved) => {
                            item.saved_revision = Some(saved.project().revision);
                            if snapshot.revision > saved.project().revision {
                                match RecoveryStore::new(
                                    &self.user_data_dir,
                                    path,
                                    identity.project_id,
                                )
                                .map_err(RecoveryError::Io)
                                .and_then(|store| store.inspect(saved.project()))
                                {
                                    Ok(Some(_)) => DiscoveryStatus::Newer,
                                    Ok(None) => DiscoveryStatus::NotNewer,
                                    Err(e) => DiscoveryStatus::Invalid(e.to_string()),
                                }
                            } else {
                                DiscoveryStatus::NotNewer
                            }
                        }
                    },
                };
                Some(item)
            })
            .collect()
    }

    /// Revalidate at use time. The returned project has no saved-file baseline;
    /// the UI must present it as unsaved and use Save As.
    pub fn load_untitled(&self, project_id: Uuid) -> Result<Project, RecoveryError> {
        let identity = RecoveryIdentity {
            project_id,
            saved_path: None,
        };
        if !self.entries.contains(&identity) {
            return Err(RecoveryError::WrongProject);
        }
        let key = untitled_key(project_id);
        read_snapshot(
            &self
                .user_data_dir
                .join("recovery")
                .join(format!("{key}.json")),
            &identity,
            &key,
        )
    }

    /// Recheck registration, canonical path and saved bytes at selection time;
    /// a discovery card is advisory and may have gone stale since rendering.
    pub fn inspect_saved(
        &self,
        path: &Path,
        project_id: Uuid,
    ) -> Result<Option<RecoveryCandidate>, RecoveryError> {
        let canonical = canonical_project_path(path).map_err(RecoveryError::Io)?;
        let identity = RecoveryIdentity {
            project_id,
            saved_path: Some(canonical.clone()),
        };
        if !self.entries.contains(&identity)
            || fs::canonicalize(&canonical).map_err(RecoveryError::Io)? != canonical
        {
            return Err(RecoveryError::WrongProject);
        }
        let saved_bytes = read_bounded(&canonical, MAX_DOCUMENT_BYTES)?;
        let saved = persistence::prepare_bytes(&saved_bytes).map_err(RecoveryError::Invalid)?;
        if saved.project().id != project_id {
            return Err(RecoveryError::WrongProject);
        }
        let key = recovery_key(project_id, &canonical);
        match read_snapshot(
            &self
                .user_data_dir
                .join("recovery")
                .join(format!("{key}.json")),
            &identity,
            &key,
        ) {
            Err(RecoveryError::Io(e)) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
            Ok(_) => {}
        }
        RecoveryStore::new(&self.user_data_dir, &canonical, project_id)
            .map_err(RecoveryError::Io)?
            .inspect(saved.project())
    }
}

/// A cleanup review contains discovery rows, including invalid snapshots, but
/// starts with an empty selection. Selection always names both the registered
/// project identity and the exact snapshot path shown to the user.
pub struct RecoveryCleanupReview {
    rows: Vec<RecoveryDiscovery>,
    selected: Vec<CleanupSelection>,
}

struct CleanupSelection {
    identity: RecoveryIdentity,
    path: PathBuf,
    fingerprint: SnapshotFingerprint,
}

#[derive(Eq, PartialEq)]
struct SnapshotFingerprint {
    digest: [u8; 32],
    len: u64,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

/// Opaque confirmation tied to one review. Show `selected()` in a destructive
/// confirmation prompt, then pass this token only on the affirmative action.
#[derive(Debug, Eq, PartialEq)]
pub struct CleanupConfirmation(Uuid);

pub struct PendingRecoveryCleanup {
    selected: Vec<CleanupSelection>,
    confirmation: CleanupConfirmation,
}

#[derive(Debug)]
pub struct CleanupFailure {
    pub identity: RecoveryIdentity,
    pub snapshot_path: PathBuf,
    pub error: String,
}

/// A failure for one selected snapshot does not prevent the others from being
/// processed. Unselected snapshots and saved projects are never touched.
#[derive(Default, Debug)]
pub struct CleanupOutcome {
    pub deleted: Vec<PathBuf>,
    pub failed: Vec<CleanupFailure>,
}

impl RecoveryCleanupReview {
    pub fn rows(&self) -> &[RecoveryDiscovery] {
        &self.rows
    }

    pub fn selected(&self) -> impl Iterator<Item = (&RecoveryIdentity, &Path)> {
        self.selected
            .iter()
            .map(|entry| (&entry.identity, entry.path.as_path()))
    }

    /// No age or status selects a row implicitly. Invalid rows can be selected
    /// by this same explicit action, but symlinks and directories cannot.
    pub fn select(
        &mut self,
        identity: &RecoveryIdentity,
        snapshot_path: &Path,
    ) -> Result<(), RecoveryError> {
        if !self
            .rows
            .iter()
            .any(|row| &row.identity == identity && row.snapshot_path == snapshot_path)
        {
            return Err(RecoveryError::WrongProject);
        }
        if self
            .selected
            .iter()
            .any(|entry| &entry.identity == identity && entry.path == snapshot_path)
        {
            return Ok(());
        }
        let fingerprint = snapshot_fingerprint(snapshot_path).map_err(RecoveryError::Io)?;
        self.selected.push(CleanupSelection {
            identity: identity.clone(),
            path: snapshot_path.to_path_buf(),
            fingerprint,
        });
        Ok(())
    }

    pub fn deselect(&mut self, identity: &RecoveryIdentity, snapshot_path: &Path) {
        self.selected
            .retain(|entry| &entry.identity != identity || entry.path != snapshot_path);
    }

    /// Dropping either this review or the pending confirmation cancels cleanup.
    pub fn prepare_confirmation(self) -> Option<PendingRecoveryCleanup> {
        (!self.selected.is_empty()).then(|| PendingRecoveryCleanup {
            selected: self.selected,
            confirmation: CleanupConfirmation(Uuid::new_v4()),
        })
    }
}

impl PendingRecoveryCleanup {
    pub fn selected(&self) -> impl Iterator<Item = (&RecoveryIdentity, &Path)> {
        self.selected
            .iter()
            .map(|entry| (&entry.identity, entry.path.as_path()))
    }

    pub fn confirmation_token(&self) -> CleanupConfirmation {
        CleanupConfirmation(self.confirmation.0)
    }

    /// Consumes the confirmation. Reloads registration from disk and checks
    /// every exact path and file fingerprint again immediately before removal.
    pub fn confirm(
        self,
        index: &RecoveryIndex,
        token: CleanupConfirmation,
    ) -> Result<CleanupOutcome, RecoveryError> {
        if self.confirmation != token {
            return Err(RecoveryError::WrongProject);
        }
        let mut outcome = CleanupOutcome::default();
        for entry in self.selected {
            let result = validate_cleanup_target(index, &entry)
                .and_then(|()| fs::remove_file(&entry.path).map_err(RecoveryError::Io));
            match result {
                Ok(()) => outcome.deleted.push(entry.path),
                Err(error) => outcome.failed.push(CleanupFailure {
                    identity: entry.identity,
                    snapshot_path: entry.path,
                    error: error.to_string(),
                }),
            }
        }
        Ok(outcome)
    }
}

fn validate_cleanup_target(
    index: &RecoveryIndex,
    entry: &CleanupSelection,
) -> Result<(), RecoveryError> {
    let current = RecoveryIndex::open(&index.user_data_dir)?;
    if !current.entries.contains(&entry.identity) {
        return Err(RecoveryError::WrongProject);
    }
    let key = match &entry.identity.saved_path {
        Some(path) => recovery_key(entry.identity.project_id, path),
        None => untitled_key(entry.identity.project_id),
    };
    let folder = current.recovery_folder();
    if entry.path != folder.join(format!("{key}.json")) {
        return Err(RecoveryError::WrongProject);
    }
    // A replaced recovery directory must not redirect deletion elsewhere.
    if !fs::symlink_metadata(&folder)
        .map_err(RecoveryError::Io)?
        .file_type()
        .is_dir()
    {
        return Err(RecoveryError::WrongProject);
    }
    // The saved path can itself be within the recovery folder. Never unlink a
    // registered portable project, including one reached through a symlink.
    let target = fs::canonicalize(&entry.path).map_err(RecoveryError::Io)?;
    for identity in current.entries() {
        if let Some(path) = &identity.saved_path
            && (path == &entry.path || fs::canonicalize(path).is_ok_and(|saved| saved == target))
        {
            return Err(RecoveryError::WrongProject);
        }
    }
    if snapshot_fingerprint(&entry.path).map_err(RecoveryError::Io)? != entry.fingerprint {
        return Err(RecoveryError::WrongProject);
    }
    Ok(())
}

fn snapshot_fingerprint(path: &Path) -> io::Result<SnapshotFingerprint> {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;

    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "snapshot must be a regular file, not a symlink",
        ));
    }
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    // A concurrent replacement while reading invalidates the review.
    let after = fs::symlink_metadata(path)?;
    if !after.file_type().is_file() || after.len() != metadata.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "snapshot changed",
        ));
    }
    #[cfg(unix)]
    {
        if (after.dev(), after.ino()) != (metadata.dev(), metadata.ino()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "snapshot changed",
            ));
        }
    }
    Ok(SnapshotFingerprint {
        digest: hash.finalize().into(),
        len: metadata.len(),
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
    })
}

/// Write an untitled committed snapshot only for an explicitly registered
/// identity. The caller controls the same inactivity timer as saved recovery.
pub fn write_untitled_snapshot(
    index: &RecoveryIndex,
    editor: &ProjectEditor,
) -> Result<(), RecoveryError> {
    let project = editor.project();
    let identity = RecoveryIdentity {
        project_id: project.id,
        saved_path: None,
    };
    if !index.entries.contains(&identity) {
        return Err(RecoveryError::WrongProject);
    }
    if !editor.is_dirty() {
        return Ok(());
    }
    let project_bytes = persistence::serialize(project).map_err(RecoveryError::Invalid)?;
    let record = serde_json::json!({
        "project_id": project.id,
        "path_key": untitled_key(project.id),
        "saved_path": null,
        "project": serde_json::from_slice::<serde_json::Value>(&project_bytes)
            .map_err(RecoveryError::Json)?,
    });
    let bytes = serde_json::to_vec(&record).map_err(RecoveryError::Json)?;
    if bytes.len() > MAX_DOCUMENT_BYTES + 4096 {
        return Err(RecoveryError::Invalid(PersistenceError::TooLarge));
    }
    let file = index
        .user_data_dir
        .join("recovery")
        .join(format!("{}.json", untitled_key(project.id)));
    fs::create_dir_all(file.parent().expect("recovery has a parent")).map_err(RecoveryError::Io)?;
    persistence::atomic_write(&file, &bytes).map_err(RecoveryError::Save)
}

/// Remove an untitled snapshot once its project was saved or knowingly
/// discarded, so Welcome stops offering it. A missing snapshot is not an error.
pub fn discard_untitled_snapshot(
    user_data_dir: &Path,
    project_id: Uuid,
) -> Result<(), RecoveryError> {
    let file = user_data_dir
        .join("recovery")
        .join(format!("{}.json", untitled_key(project_id)));
    match fs::remove_file(file) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(RecoveryError::Io(e)),
        _ => Ok(()),
    }
}

fn read_snapshot(
    path: &Path,
    identity: &RecoveryIdentity,
    key: &str,
) -> Result<Project, RecoveryError> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(RecoveryError::Io)?
        .take(MAX_DOCUMENT_BYTES as u64 + 4097)
        .read_to_end(&mut bytes)
        .map_err(RecoveryError::Io)?;
    if bytes.len() > MAX_DOCUMENT_BYTES + 4096 {
        return Err(RecoveryError::Invalid(PersistenceError::TooLarge));
    }
    let value = persistence::parse_unique_json(&bytes).map_err(RecoveryError::Json)?;
    match &identity.saved_path {
        None if value.get("saved_path") != Some(&serde_json::Value::Null) => {
            return Err(RecoveryError::WrongProject);
        }
        Some(path)
            if value
                .get("saved_path")
                .is_some_and(|v| v.as_str() != path.to_str()) =>
        {
            return Err(RecoveryError::WrongProject);
        }
        _ => {}
    }
    let record: ReadSnapshot = serde_json::from_value(value).map_err(RecoveryError::Json)?;
    if record.project_id != identity.project_id || record.path_key != key {
        return Err(RecoveryError::WrongProject);
    }
    let validated = persistence::prepare_bytes(
        &serde_json::to_vec(&record.project).map_err(RecoveryError::Json)?,
    )
    .map_err(RecoveryError::Invalid)?;
    if validated.project().id != identity.project_id {
        return Err(RecoveryError::WrongProject);
    }
    Ok(validated.project().clone())
}

#[derive(Deserialize)]
struct ReadSnapshot {
    project_id: Uuid,
    path_key: String,
    project: serde_json::Value,
}

fn invalid_index() -> RecoveryError {
    RecoveryError::Io(io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid recovery index",
    ))
}

fn valid_identity(identity: &RecoveryIdentity) -> Result<bool, RecoveryError> {
    let Some(path) = &identity.saved_path else {
        return Ok(true);
    };
    if path.to_str().is_none()
        || !path.is_absolute()
        || path.as_os_str().len() > 4096
        || path.components().any(|c| {
            matches!(
                c,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return Ok(false);
    }
    // Parents may disappear between sessions. Keep that identity available
    // for diagnosis; a different canonical target is rejected at discovery.
    Ok(true)
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, RecoveryError> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(RecoveryError::Io)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(RecoveryError::Io)?;
    if bytes.len() > limit {
        return Err(RecoveryError::Invalid(PersistenceError::TooLarge));
    }
    Ok(bytes)
}

fn canonical_project_path(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let name = path.file_name().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "project path needs a filename")
            })?;
            Ok(fs::canonicalize(parent)?.join(name))
        }
        Err(e) => Err(e),
    }
}

fn recovery_key(project_id: Uuid, path: &Path) -> String {
    let mut hash = Sha256::new();
    hash.update(project_id.as_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        hash.update(path.as_os_str().as_bytes());
    }
    #[cfg(not(unix))]
    hash.update(path.to_string_lossy().as_bytes());
    format!("{:x}", hash.finalize())
}

fn untitled_key(project_id: Uuid) -> String {
    let mut hash = Sha256::new();
    hash.update(b"pmcab-untitled-recovery-v1\0");
    hash.update(project_id.as_bytes());
    format!("{:x}", hash.finalize())
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

    #[test]
    fn legacy_payload_migrates_without_writes_and_future_or_invalid_payloads_are_retained() {
        let directory = Directory::new();
        let path = directory.0.join("legacy.pmcab");
        let golden = include_bytes!("../../tests/fixtures/schema-v1-cabinet.pmcab");
        fs::write(&path, golden).unwrap();
        let saved = persistence::prepare_bytes(golden)
            .unwrap()
            .project()
            .clone();
        let store = RecoveryStore::new(&directory.0, &path, saved.id).unwrap();
        fs::create_dir_all(store.file.parent().unwrap()).unwrap();
        let mut payload: serde_json::Value = serde_json::from_slice(golden).unwrap();
        payload["revision"] = (saved.revision + 1).into();
        payload["name"] = "Recovered legacy edits".into();
        let record = serde_json::json!({
            "project_id": saved.id, "path_key": store.key, "project": payload
        });
        let bytes = serde_json::to_vec(&record).unwrap();
        fs::write(&store.file, &bytes).unwrap();
        let candidate = store.inspect(&saved).unwrap().unwrap();
        assert_eq!(candidate.recovery_revision, 18);
        assert_eq!(fs::read(&store.file).unwrap(), bytes);
        let recovered = candidate
            .resolve(&store, RecoveryChoice::Recover)
            .unwrap()
            .unwrap();
        assert_eq!(
            recovered.project().schema_version,
            crate::domain::SCHEMA_VERSION
        );
        assert_eq!(recovered.project().export_records, saved.export_records);
        assert_eq!(recovered.project().boards, saved.boards);
        assert!(recovered.is_dirty());
        assert!(!recovered.can_undo());
        assert_eq!(fs::read(&path).unwrap(), golden);

        for version in [1, crate::domain::SCHEMA_VERSION + 1] {
            let mut invalid = record.clone();
            invalid["project"]["schema_version"] = version.into();
            if version == 1 {
                invalid["project"]["boards"][0]["length"] = 0.into();
            }
            let bytes = serde_json::to_vec(&invalid).unwrap();
            fs::write(&store.file, &bytes).unwrap();
            let result = store.inspect(&saved);
            if version > 1 {
                assert!(matches!(
                    result,
                    Err(RecoveryError::Invalid(
                        PersistenceError::UnsupportedVersion(v)
                    )) if v == u64::from(version)
                ));
            } else {
                assert!(matches!(
                    result,
                    Err(RecoveryError::Invalid(PersistenceError::InvalidProject(_)))
                ));
            }
            assert_eq!(fs::read(&store.file).unwrap(), bytes);
            assert_eq!(fs::read(&path).unwrap(), golden);
        }
    }
}
