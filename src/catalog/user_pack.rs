//! The one catalog pack file the app writes: slide and foot models created in
//! the app or by an agent, kept in `<catalog dir>/user-models.toml`. Other
//! pack files are never written. The file is an ordinary pack (status
//! "draft"), so it loads, validates and shows like any user pack.
use std::path::{Path, PathBuf};

use crate::catalog_pack::{
    FootFamily, IssueKind, LoadedPack, Pack, PackIssue, PackOrigin, RawFoot, RawPack, RawReview,
    RawSlide, ReviewStatus, Severity, SlideFamily, load,
};

pub const FILE_NAME: &str = "user-models.toml";
pub const PACK_ID: &str = "user-models";

const HEADER: &str = "# Hardware models created in Plan My Cabinet (drawer slides and feet).\n\
# The app rewrites this file when you save a new model: hand edits to the\n\
# values are kept, comments are not. Units are millimetres.\n\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserPackError {
    /// The file on disk is not a valid pack; it is left untouched.
    ExistingInvalid(Vec<PackIssue>),
    /// The new model has errors.
    Invalid(Vec<PackIssue>),
    Io(String),
}

impl std::fmt::Display for UserPackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |issues: &[PackIssue]| {
            issues
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        };
        match self {
            Self::ExistingInvalid(issues) => write!(
                f,
                "{FILE_NAME} is not a valid pack and was left unchanged: {}",
                list(issues)
            ),
            Self::Invalid(issues) => write!(f, "the model is invalid: {}", list(issues)),
            Self::Io(message) => write!(f, "cannot write {FILE_NAME}: {message}"),
        }
    }
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

fn empty() -> RawPack {
    RawPack {
        schema: crate::catalog_pack::PACK_SCHEMA,
        id: PACK_ID.into(),
        manufacturer: "User".into(),
        country: None,
        version: "user".into(),
        review: RawReview {
            status: ReviewStatus::Draft,
            by: None,
            date: None,
            notes: Some("Models created in Plan My Cabinet.".into()),
        },
        sources: Vec::new(),
        hinges: Vec::new(),
        drawer_slides: Vec::new(),
        feet: Vec::new(),
    }
}

fn errors(loaded: &LoadedPack) -> Vec<PackIssue> {
    loaded
        .issues
        .iter()
        .filter(|i| i.kind.severity() == Severity::Error)
        .cloned()
        .collect()
}

fn to_text(raw: &RawPack) -> Result<String, UserPackError> {
    toml::to_string_pretty(raw)
        .map(|body| format!("{HEADER}{body}"))
        .map_err(|e| UserPackError::Io(e.to_string()))
}

/// Validate a raw pack exactly as a file would be loaded.
fn check(raw: &RawPack) -> Result<(Pack, Vec<PackIssue>), UserPackError> {
    let text = to_text(raw)?;
    let loaded = load(PackOrigin::User(FILE_NAME.into()), text.as_bytes());
    let errors = errors(&loaded);
    match loaded.pack {
        Some(pack) if errors.is_empty() => Ok((
            pack,
            loaded
                .issues
                .into_iter()
                .filter(|i| i.kind != IssueKind::Draft)
                .collect(),
        )),
        _ => Err(UserPackError::Invalid(errors)),
    }
}

/// A slide model checked with the pack loader's rules, as the user pack
/// would hold it, plus any warnings.
pub fn check_slide(raw: &RawSlide) -> Result<(Pack, SlideFamily, Vec<PackIssue>), UserPackError> {
    let mut pack = empty();
    pack.drawer_slides.push(raw.clone());
    let (pack, warnings) = check(&pack)?;
    let family = pack
        .slides
        .first()
        .cloned()
        .ok_or(UserPackError::Invalid(Vec::new()))?;
    Ok((pack, family, warnings))
}

pub fn check_foot(raw: &RawFoot) -> Result<(Pack, FootFamily, Vec<PackIssue>), UserPackError> {
    let mut pack = empty();
    pack.feet.push(raw.clone());
    let (pack, warnings) = check(&pack)?;
    let family = pack
        .feet
        .first()
        .cloned()
        .ok_or(UserPackError::Invalid(Vec::new()))?;
    Ok((pack, family, warnings))
}

/// Add or replace (by id) these models in `<dir>/user-models.toml`.
pub fn save_models(
    dir: &Path,
    slides: Vec<RawSlide>,
    feet: Vec<RawFoot>,
) -> Result<PathBuf, UserPackError> {
    let file = path(dir);
    let mut raw = match std::fs::read(&file) {
        Ok(bytes) => {
            let loaded = load(PackOrigin::User(file.clone()), &bytes);
            let problems = errors(&loaded);
            let parsed = std::str::from_utf8(&bytes)
                .ok()
                .and_then(|text| toml::from_str::<RawPack>(text).ok());
            match parsed {
                Some(parsed) if problems.is_empty() => parsed,
                _ => return Err(UserPackError::ExistingInvalid(problems)),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => empty(),
        Err(e) => return Err(UserPackError::Io(e.to_string())),
    };
    for slide in slides {
        match raw.drawer_slides.iter_mut().find(|s| s.id == slide.id) {
            Some(slot) => *slot = slide,
            None => raw.drawer_slides.push(slide),
        }
    }
    for foot in feet {
        match raw.feet.iter_mut().find(|f| f.id == foot.id) {
            Some(slot) => *slot = foot,
            None => raw.feet.push(foot),
        }
    }
    check(&raw)?;
    let text = to_text(&raw)?;
    std::fs::create_dir_all(dir).map_err(|e| UserPackError::Io(e.to_string()))?;
    crate::persistence::atomic_write(&file, text.as_bytes())
        .map_err(|e| UserPackError::Io(e.to_string()))?;
    Ok(file)
}

/// A pack id from a display name: lowercase letters, digits and '-'.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'ê' | 'è' => 'e',
            'í' | 'ì' => 'i',
            'ó' | 'ô' | 'õ' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() { "model".into() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog_pack::{
        RawClearance, RawFootVariant, RawHole, RawSection, RawShape, RawSlideVariant,
    };
    use std::collections::BTreeMap;

    fn foot(id: &str, height: f64) -> RawFoot {
        RawFoot {
            id: id.into(),
            name: BTreeMap::from([("en".into(), "Cone".into())]),
            shape: RawShape {
                kind: "tapered".into(),
                height: Some(height),
                top: Some(RawSection {
                    diameter: Some(50.0),
                    ..Default::default()
                }),
                bottom: Some(RawSection {
                    diameter: Some(30.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
            color: "#101010".into(),
            variants: vec![RawFootVariant {
                code: format!("{id}-1"),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn slide() -> RawSlide {
        RawSlide {
            id: "my-slide".into(),
            name: BTreeMap::from([("en".into(), "My slide".into())]),
            height: 45.0,
            clearance: RawClearance {
                nominal: 12.7,
                minus: 0.0,
                plus: 0.5,
            },
            front_setback: 2.0,
            variants: vec![RawSlideVariant {
                code: "MS-450".into(),
                length: 450.0,
                travel: 450.0,
                cabinet_holes: vec![RawHole::Along(35.0), RawHole::Along(99.5)],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn models_round_trip_and_upsert() {
        let dir = std::env::temp_dir().join(format!("user-pack-{}", uuid::Uuid::new_v4()));
        let file = save_models(&dir, vec![slide()], vec![foot("cone", 40.0)]).unwrap();
        save_models(
            &dir,
            Vec::new(),
            vec![foot("cone", 45.0), foot("other", 30.0)],
        )
        .unwrap();
        let loaded = crate::catalog_pack::load_file(&file);
        assert_eq!(
            loaded
                .issues
                .iter()
                .map(|i| i.kind.code())
                .collect::<Vec<_>>(),
            ["draft"]
        );
        let pack = loaded.usable().unwrap();
        assert_eq!(pack.id, PACK_ID);
        assert_eq!(pack.slides.len(), 1);
        assert_eq!(
            pack.slides[0].variants[0].cabinet_holes[1]
                .along
                .micrometres(),
            99_500
        );
        assert_eq!(pack.feet.len(), 2);
        assert_eq!(pack.feet[0].shape.height().micrometres(), 45_000);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn invalid_files_and_models_are_refused() {
        let dir = std::env::temp_dir().join(format!("user-pack-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(path(&dir), "hand edited [").unwrap();
        assert!(matches!(
            save_models(&dir, Vec::new(), vec![foot("cone", 40.0)]),
            Err(UserPackError::ExistingInvalid(_))
        ));
        assert_eq!(
            std::fs::read_to_string(path(&dir)).unwrap(),
            "hand edited ["
        );
        std::fs::remove_file(path(&dir)).unwrap();
        let mut bad = foot("cone", 40.0);
        bad.shape.kind = "cube".into();
        assert!(matches!(
            save_models(&dir, Vec::new(), vec![bad]),
            Err(UserPackError::Invalid(_))
        ));
        assert!(!path(&dir).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn slugs_are_pack_ids() {
        assert_eq!(slug("Pé Cônico 4 cm!"), "pe-conico-4-cm");
        assert_eq!(slug("  "), "model");
    }
}
