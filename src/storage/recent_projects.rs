//! Explicit, machine-local recent document index. The desktop host supplies an
//! absolute application user-data directory and calls registration only after
//! its open/save lifecycle has actually succeeded.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::Project;
use crate::export::ExportStatus;
use crate::persistence::{self, PersistenceError, SaveError};

const FILE_NAME: &str = "recent-projects.json";
const VERSION: u8 = 1;
pub const THUMBNAIL_SIZE: [usize; 2] = [192, 120];
const THUMBNAIL_BYTES: usize = THUMBNAIL_SIZE[0] * THUMBNAIL_SIZE[1] * 4;
pub const MAX_RECENT_ENTRIES: usize = 128;
pub const MAX_RECENT_INDEX_BYTES: usize = 128 * 1024;

#[derive(Debug)]
pub enum RecentError {
    InvalidDirectory,
    InvalidIndex,
    TooLarge,
    NotRegistered,
    WrongProject { expected: Uuid, actual: Uuid },
    Io(io::Error),
    Json(serde_json::Error),
    Project(PersistenceError),
    Write(SaveError),
}

impl fmt::Display for RecentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDirectory => {
                f.write_str("Recents require an absolute application user-data directory")
            }
            Self::InvalidIndex => {
                f.write_str("Invalid recent-project index or unsupported version")
            }
            Self::TooLarge => f.write_str("Recent-project index exceeds its storage limit"),
            Self::NotRegistered => f.write_str("Recent project is no longer registered"),
            Self::WrongProject { expected, actual } => write!(
                f,
                "Selected file belongs to project {actual}, not {expected}"
            ),
            Self::Io(error) => write!(f, "Cannot access recent projects: {error}"),
            Self::Json(error) => write!(f, "Invalid recent-project index: {error}"),
            Self::Project(error) => write!(f, "Cannot validate project: {error}"),
            Self::Write(error) => write!(f, "Cannot write recent-project index: {error}"),
        }
    }
}

impl std::error::Error for RecentError {}

/// A cached summary is historical; only `RecentStatus::Available` contains
/// metadata validated from the file at the time of the read.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecentSummary {
    pub name: String,
    pub boards: usize,
    pub assemblies: usize,
    pub materials: usize,
    pub stock: usize,
    pub export_records: usize,
}

impl RecentSummary {
    fn from_project(project: &Project) -> Self {
        Self {
            name: project.name.clone(),
            boards: project.boards.len(),
            assemblies: project.assemblies.len(),
            materials: project.materials.len(),
            stock: project.stock.len(),
            export_records: project.export_records.len(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecentEntry {
    pub path: PathBuf,
    pub project_id: Uuid,
    pub last_used_unix_ms: u64,
    pub cached_summary: RecentSummary,
    /// Content-addressed local thumbnail; never part of a project file.
    pub thumbnail_key: Option<String>,
}

#[derive(Debug)]
pub enum RecentStatus {
    Available(RecentSummary),
    Missing,
    Unavailable(String),
}

#[derive(Debug)]
pub struct RecentProjectView {
    pub entry: RecentEntry,
    pub status: RecentStatus,
    /// Qualified against validated current file bytes; absent for a missing,
    /// unreadable, or identity-mismatched path, never inferred from receipt count.
    pub export_status: Option<ExportStatus>,
    /// Fixed-size RGBA pixels, validated against the available saved document.
    pub thumbnail: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u8,
    entries: Vec<RecentEntry>,
}

pub struct RecentProjects {
    index_path: PathBuf,
    entries: Vec<RecentEntry>,
}

impl RecentProjects {
    /// An absent index is empty. Malformed/oversized indexes return an error
    /// and are left on disk for diagnosis, never reset or overwritten.
    pub fn open(user_data_dir: &Path) -> Result<Self, RecentError> {
        if !user_data_dir.is_absolute() {
            return Err(RecentError::InvalidDirectory);
        }
        let index_path = user_data_dir.join(FILE_NAME);
        let file = match File::open(&index_path) {
            Ok(file) => Some(file),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(RecentError::Io(e)),
        };
        let entries = if let Some(file) = file {
            let mut bytes = Vec::new();
            file.take(MAX_RECENT_INDEX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(RecentError::Io)?;
            if bytes.len() > MAX_RECENT_INDEX_BYTES {
                return Err(RecentError::TooLarge);
            }
            let value = persistence::parse_unique_json(&bytes).map_err(RecentError::Json)?;
            let index: Index = serde_json::from_value(value).map_err(RecentError::Json)?;
            if index.version != VERSION || index.entries.len() > MAX_RECENT_ENTRIES {
                return Err(RecentError::InvalidIndex);
            }
            for (position, entry) in index.entries.iter().enumerate() {
                if !valid_entry(entry)
                    || index.entries[..position]
                        .iter()
                        .any(|other| other.path == entry.path)
                {
                    return Err(RecentError::InvalidIndex);
                }
            }
            index.entries
        } else {
            Vec::new()
        };
        Ok(Self {
            index_path,
            entries,
        })
    }

    pub fn entries(&self) -> &[RecentEntry] {
        &self.entries
    }

    /// Call only after the host accepted a successful validated open/save.
    /// Re-read the bounded on-disk document to avoid caching an unsaved preview
    /// or a different file that replaced the destination in the meantime.
    pub fn register_successful(
        &mut self,
        path: &Path,
        expected_id: Uuid,
        used_at: SystemTime,
    ) -> Result<(), RecentError> {
        let (path, project) = validated_file(path)?;
        check_id(expected_id, project.id)?;
        let last_used_unix_ms = timestamp(used_at)?;
        let mut next = self.entries.clone();
        // A new project at an old location supersedes the old path identity.
        // Another path containing the same UUID is a distinct copy.
        let key = thumbnail_key(&project)?;
        let previous_key = next
            .iter()
            .find(|entry| entry.path == path)
            .and_then(|entry| entry.thumbnail_key.as_ref())
            .filter(|existing| *existing == &key)
            .cloned();
        next.retain(|entry| entry.path != path);
        next.insert(
            0,
            RecentEntry {
                path,
                project_id: expected_id,
                last_used_unix_ms,
                cached_summary: RecentSummary::from_project(&project),
                thumbnail_key: previous_key,
            },
        );
        next.truncate(MAX_RECENT_ENTRIES);
        self.persist(next)
    }

    /// Called after the successful explicit save and recent registration. Recheck
    /// the file's identity and serialized state before associating derived pixels.
    /// Any failure leaves the successful project save intact.
    pub fn store_thumbnail(
        &mut self,
        path: &Path,
        expected_id: Uuid,
        captured_project: &Project,
        rgba: &[u8],
    ) -> Result<(), RecentError> {
        if rgba.len() != THUMBNAIL_BYTES {
            return Err(RecentError::InvalidIndex);
        }
        let (path, project) = validated_file(path)?;
        check_id(expected_id, project.id)?;
        check_id(expected_id, captured_project.id)?;
        let position = self.position(&path, expected_id)?;
        let key = thumbnail_key(&project)?;
        if thumbnail_key(captured_project)? != key {
            return Err(RecentError::InvalidIndex);
        }
        let directory = self
            .index_path
            .parent()
            .expect("absolute index path")
            .join("thumbnails");
        fs::create_dir_all(&directory).map_err(RecentError::Io)?;
        let mut bytes = b"PMCT1".to_vec();
        bytes.extend(Sha256::digest(rgba));
        bytes.extend(rgba);
        persistence::atomic_write(&directory.join(format!("{key}.rgba")), &bytes)
            .map_err(RecentError::Write)?;
        let mut next = self.entries.clone();
        next[position].thumbnail_key = Some(key);
        self.persist(next)
    }

    /// Resolve a missing entry to a validated file of the *same* project.
    /// A different UUID is an error, never silently substituted. No file is
    /// moved or modified, and a failure leaves the index unchanged.
    pub fn locate(
        &mut self,
        old_path: &Path,
        expected_id: Uuid,
        candidate: &Path,
    ) -> Result<(), RecentError> {
        let position = self.position(old_path, expected_id)?;
        let (path, project) = validated_file(candidate)?;
        check_id(expected_id, project.id)?;
        let mut next = self.entries.clone();
        let mut replacement = next.remove(position);
        replacement.path = path.clone();
        replacement.cached_summary = RecentSummary::from_project(&project);
        replacement.thumbnail_key = None;
        // If this copy was registered earlier, coalesce its path identity.
        next.retain(|entry| entry.path != path);
        next.insert(position.min(next.len()), replacement);
        self.persist(next)
    }

    /// Delete only the local index entry. Project, export and recovery bytes
    /// are never opened for deletion by this operation.
    pub fn remove(&mut self, path: &Path, project_id: Uuid) -> Result<(), RecentError> {
        let position = self.position(path, project_id)?;
        let mut next = self.entries.clone();
        next.remove(position);
        self.persist(next)
    }

    /// Filter against the current validated name when available, or the
    /// explicitly stale cached name for missing/unavailable documents.
    pub fn list(&self, filter: &str) -> Vec<RecentProjectView> {
        let needle = filter.to_lowercase();
        self.entries
            .iter()
            .filter_map(|entry| {
                let (status, export_status, thumbnail) = match fs::canonicalize(&entry.path) {
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        (RecentStatus::Missing, None, None)
                    }
                    Err(e) => (RecentStatus::Unavailable(e.to_string()), None, None),
                    Ok(actual) if actual != entry.path => (
                        RecentStatus::Unavailable(
                            "Project path now resolves to a different location".into(),
                        ),
                        None,
                        None,
                    ),
                    Ok(_) => match read_project(&entry.path) {
                        Ok(project) if project.id == entry.project_id => (
                            RecentStatus::Available(RecentSummary::from_project(&project)),
                            Some(ExportStatus::for_project(&project)),
                            entry.thumbnail_key.as_ref().and_then(|key| {
                                (thumbnail_key(&project).ok().as_ref() == Some(key))
                                    .then(|| self.read_thumbnail(key))
                                    .flatten()
                            }),
                        ),
                        Ok(_) => (
                            RecentStatus::Unavailable(
                                "Project identity at this path has changed".into(),
                            ),
                            None,
                            None,
                        ),
                        Err(e) => (RecentStatus::Unavailable(e.to_string()), None, None),
                    },
                };
                let name = match &status {
                    RecentStatus::Available(summary) => &summary.name,
                    _ => &entry.cached_summary.name,
                };
                let matched = needle.is_empty()
                    || name.to_lowercase().contains(&needle)
                    || entry
                        .path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&needle);
                matched.then(|| RecentProjectView {
                    entry: entry.clone(),
                    status,
                    export_status,
                    thumbnail,
                })
            })
            .collect()
    }

    fn read_thumbnail(&self, key: &str) -> Option<Vec<u8>> {
        let bytes = fs::read(
            self.index_path
                .parent()?
                .join("thumbnails")
                .join(format!("{key}.rgba")),
        )
        .ok()?;
        if bytes.len() != 5 + 32 + THUMBNAIL_BYTES
            || &bytes[..5] != b"PMCT1"
            || Sha256::digest(&bytes[37..]).as_slice() != &bytes[5..37]
        {
            return None;
        }
        Some(bytes[37..].to_vec())
    }

    fn position(&self, path: &Path, project_id: Uuid) -> Result<usize, RecentError> {
        self.entries
            .iter()
            .position(|entry| entry.path == path && entry.project_id == project_id)
            .ok_or(RecentError::NotRegistered)
    }

    fn persist(&mut self, next: Vec<RecentEntry>) -> Result<(), RecentError> {
        if next.len() > MAX_RECENT_ENTRIES || next.iter().any(|entry| !valid_entry(entry)) {
            return Err(RecentError::InvalidIndex);
        }
        let bytes = serde_json::to_vec(&Index {
            version: VERSION,
            entries: next.clone(),
        })
        .map_err(RecentError::Json)?;
        if bytes.len() > MAX_RECENT_INDEX_BYTES {
            return Err(RecentError::TooLarge);
        }
        fs::create_dir_all(self.index_path.parent().expect("absolute index path"))
            .map_err(RecentError::Io)?;
        persistence::atomic_write(&self.index_path, &bytes).map_err(RecentError::Write)?;
        self.entries = next;
        Ok(())
    }
}

fn thumbnail_key(project: &Project) -> Result<String, RecentError> {
    let bytes = persistence::serialize(project).map_err(RecentError::Project)?;
    // Canonicalize object-key order: catalog/maps may serialize in a different
    // iteration order after a saved document is reloaded.
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(RecentError::Json)?;
    let canonical = serde_json::to_vec(&value).map_err(RecentError::Json)?;
    Ok(format!("v1-{:x}", Sha256::digest(&canonical)))
}

fn valid_entry(entry: &RecentEntry) -> bool {
    entry.path.is_absolute()
        && entry.path.to_str().is_some()
        && entry.path.as_os_str().len() <= 4096
        && !entry
            .path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        && entry.cached_summary.name.len() <= 4096
        && entry.thumbnail_key.as_ref().is_none_or(|key| {
            key.len() <= 128
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

fn timestamp(used_at: SystemTime) -> Result<u64, RecentError> {
    used_at
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .ok_or(RecentError::InvalidIndex)
}

fn check_id(expected: Uuid, actual: Uuid) -> Result<(), RecentError> {
    if expected == actual {
        Ok(())
    } else {
        Err(RecentError::WrongProject { expected, actual })
    }
}

fn validated_file(path: &Path) -> Result<(PathBuf, Project), RecentError> {
    let canonical = fs::canonicalize(path).map_err(RecentError::Io)?;
    if !canonical.is_absolute()
        || canonical.to_str().is_none()
        || canonical.as_os_str().len() > 4096
    {
        return Err(RecentError::InvalidIndex);
    }
    let project = read_project(&canonical)?;
    Ok((canonical, project))
}

fn read_project(path: &Path) -> Result<Project, RecentError> {
    let file = File::open(path).map_err(RecentError::Io)?;
    persistence::prepare_reader(file)
        .map(|prepared| prepared.project().clone())
        .map_err(RecentError::Project)
}
