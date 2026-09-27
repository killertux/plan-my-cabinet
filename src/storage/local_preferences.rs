//! Machine-local display preferences. The desktop host supplies its platform
//! configuration directory; this module never discovers a path or uses cwd.
//! No project/editor value is accepted or changed by this store.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Language;
use crate::persistence::{self, SaveError};

const FILE_NAME: &str = "preferences.json";
const VERSION: u32 = 1;
pub const MAX_PREFERENCES_BYTES: usize = 16 * 1024;

/// The supported egui interface zoom levels, stored as whole percentages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub enum InterfaceScale {
    Percent90,
    #[default]
    Percent100,
    Percent115,
    Percent130,
}

impl TryFrom<u16> for InterfaceScale {
    type Error = &'static str;

    fn try_from(percent: u16) -> Result<Self, Self::Error> {
        match percent {
            90 => Ok(Self::Percent90),
            100 => Ok(Self::Percent100),
            115 => Ok(Self::Percent115),
            130 => Ok(Self::Percent130),
            _ => Err("interface scale must be 90, 100, 115 or 130 percent"),
        }
    }
}

impl From<InterfaceScale> for u16 {
    fn from(scale: InterfaceScale) -> Self {
        scale.percent()
    }
}

impl InterfaceScale {
    pub const fn percent(self) -> u16 {
        match self {
            Self::Percent90 => 90,
            Self::Percent100 => 100,
            Self::Percent115 => 115,
            Self::Percent130 => 130,
        }
    }

    pub const fn factor(self) -> f32 {
        self.percent() as f32 / 100.0
    }
}

/// App-wide preferences only. These have no relationship to portable material
/// colors, cutting settings, export language, project revision or undo history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalPreferences {
    #[serde(with = "language_tag")]
    pub language: Language,
    pub navigation_hints: bool,
    pub inverse_scroll_zoom: bool,
    pub material_tint: bool,
    pub interface_scale: InterfaceScale,
}

mod language_tag {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    use crate::i18n::Language;

    pub fn serialize<S: Serializer>(language: &Language, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(language.tag())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Language, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "en" => Ok(Language::En),
            "pt-BR" => Ok(Language::PtBr),
            _ => Err(D::Error::custom("language must be en or pt-BR")),
        }
    }
}

impl Default for LocalPreferences {
    fn default() -> Self {
        Self {
            language: Language::En,
            navigation_hints: true,
            inverse_scroll_zoom: false,
            material_tint: true,
            interface_scale: InterfaceScale::Percent100,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPreferences {
    schema_version: u32,
    preferences: LocalPreferences,
}

#[derive(Debug)]
pub enum PreferencesError {
    InvalidDirectory,
    TooLarge,
    UnsupportedVersion(u32),
    Io(io::Error),
    Json(serde_json::Error),
    Write(SaveError),
}

impl fmt::Display for PreferencesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDirectory => {
                f.write_str("Preferences require an absolute platform configuration directory")
            }
            Self::TooLarge => f.write_str("Preferences exceed the 16 KiB limit"),
            Self::UnsupportedVersion(version) => {
                write!(f, "Unsupported preferences version {version}")
            }
            Self::Io(error) => write!(f, "Cannot read or create preferences directory: {error}"),
            Self::Json(error) => write!(f, "Invalid preferences JSON: {error}"),
            Self::Write(error) => write!(f, "Cannot save preferences: {error}"),
        }
    }
}

impl std::error::Error for PreferencesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Write(error) => Some(error),
            _ => None,
        }
    }
}

/// Missing files use defaults without a warning. Malformed/unreadable files
/// also use defaults, but carry a diagnostic for the UI to report; they are
/// never silently overwritten on load.
#[derive(Debug)]
pub struct PreferencesLoad {
    pub preferences: LocalPreferences,
    pub warning: Option<PreferencesError>,
}

pub struct PreferencesStore {
    path: PathBuf,
}

impl PreferencesStore {
    /// Pass the app's platform-provided configuration directory, not a project
    /// path or the current directory. No directory is created until save.
    pub fn new(config_dir: &Path) -> Result<Self, PreferencesError> {
        if !config_dir.is_absolute() {
            return Err(PreferencesError::InvalidDirectory);
        }
        Ok(Self {
            path: config_dir.join(FILE_NAME),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> PreferencesLoad {
        match self.read() {
            Ok(Some(preferences)) => PreferencesLoad {
                preferences,
                warning: None,
            },
            Ok(None) => PreferencesLoad {
                preferences: LocalPreferences::default(),
                warning: None,
            },
            Err(error) => PreferencesLoad {
                preferences: LocalPreferences::default(),
                warning: Some(error),
            },
        }
    }

    fn read(&self) -> Result<Option<LocalPreferences>, PreferencesError> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(PreferencesError::Io(error)),
        };
        let mut bytes = Vec::new();
        file.take(MAX_PREFERENCES_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(PreferencesError::Io)?;
        if bytes.len() > MAX_PREFERENCES_BYTES {
            return Err(PreferencesError::TooLarge);
        }
        let value = persistence::parse_unique_json(&bytes).map_err(PreferencesError::Json)?;
        let stored: StoredPreferences =
            serde_json::from_value(value).map_err(PreferencesError::Json)?;
        if stored.schema_version != VERSION {
            return Err(PreferencesError::UnsupportedVersion(stored.schema_version));
        }
        Ok(Some(stored.preferences))
    }

    /// Persist a complete snapshot after a General preference change. Errors
    /// are returned to the caller, including post-rename durability uncertainty.
    /// A pre-rename failure leaves the old file untouched.
    pub fn save(&self, preferences: &LocalPreferences) -> Result<(), PreferencesError> {
        let bytes = serde_json::to_vec_pretty(&StoredPreferences {
            schema_version: VERSION,
            preferences: preferences.clone(),
        })
        .map_err(PreferencesError::Json)?;
        if bytes.len() > MAX_PREFERENCES_BYTES {
            return Err(PreferencesError::TooLarge);
        }
        let parent = self.path.parent().expect("preferences have a parent");
        fs::create_dir_all(parent).map_err(PreferencesError::Io)?;
        persistence::atomic_write(&self.path, &bytes).map_err(PreferencesError::Write)
    }
}
