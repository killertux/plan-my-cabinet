use std::fs;
use std::path::{Path, PathBuf};

use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::local_preferences::{
    InterfaceScale, LocalPreferences, MAX_PREFERENCES_BYTES, PreferencesError, PreferencesStore,
};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence::serialize;
use uuid::Uuid;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("pmcab-local-preferences-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn preferences_survive_store_restart_without_touching_project_or_history() {
    let root = TempDir::new();
    let config = root.path().join("platform-config");
    let store = PreferencesStore::new(&config).unwrap();
    assert_eq!(store.load().preferences, LocalPreferences::default());
    assert!(store.load().warning.is_none());
    assert!(!config.exists());

    let editor = ProjectEditor::new(Project::new("Portable", Currency::Brl)).unwrap();
    let before = serialize(editor.project()).unwrap();
    let revision = editor.project().revision;
    let prefs = LocalPreferences {
        language: Language::PtBr,
        navigation_hints: false,
        inverse_scroll_zoom: true,
        material_tint: false,
        interface_scale: InterfaceScale::Percent115,
    };
    store.save(&prefs).unwrap();

    let restarted = PreferencesStore::new(&config).unwrap().load();
    assert!(restarted.warning.is_none());
    assert_eq!(restarted.preferences, prefs);
    assert_eq!(restarted.preferences.interface_scale.factor(), 1.15);
    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
    assert_eq!(stored["preferences"]["language"], "pt-BR");
    assert_eq!(editor.project().revision, revision);
    assert!(!editor.is_dirty());
    assert!(!editor.can_undo());
    assert_eq!(serialize(editor.project()).unwrap(), before);
    let portable: serde_json::Value = serde_json::from_slice(&before).unwrap();
    for field in [
        "language",
        "navigation_hints",
        "inverse_scroll_zoom",
        "material_tint",
        "interface_scale",
    ] {
        assert!(
            portable.get(field).is_none(),
            "portable project contains {field}"
        );
    }
    assert_eq!(fs::read_dir(config).unwrap().count(), 1);
}

#[test]
fn every_supported_scale_roundtrips_as_a_numeric_percentage() {
    let root = TempDir::new();
    let store = PreferencesStore::new(root.path()).unwrap();
    for (scale, percent) in [
        (InterfaceScale::Percent90, 90),
        (InterfaceScale::Percent100, 100),
        (InterfaceScale::Percent115, 115),
        (InterfaceScale::Percent130, 130),
    ] {
        let prefs = LocalPreferences {
            interface_scale: scale,
            ..LocalPreferences::default()
        };
        store.save(&prefs).unwrap();
        assert_eq!(store.load().preferences, prefs);
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
        assert_eq!(json["preferences"]["interface_scale"], percent);
    }
}

#[test]
fn malformed_and_oversized_configs_fall_back_and_report_without_rewriting() {
    let root = TempDir::new();
    let store = PreferencesStore::new(root.path()).unwrap();
    store.save(&LocalPreferences::default()).unwrap();
    let mut cases = vec![
        b"{".to_vec(),
        b"{}".to_vec(),
        br#"{"schema_version":9,"preferences":{}}"#.to_vec(),
        br#"{"schema_version":1,"schema_version":1,"preferences":{}}"#.to_vec(),
        br#"{"schema_version":1,"preferences":{"language":"en","navigation_hints":true,"inverse_scroll_zoom":false,"material_tint":true,"interface_scale":125}}"#.to_vec(),
        br#"{"schema_version":1,"preferences":{"language":"en","navigation_hints":true,"inverse_scroll_zoom":false,"material_tint":true,"interface_scale":100,"project":{}}}"#.to_vec(),
        vec![b' '; MAX_PREFERENCES_BYTES + 1],
    ];
    cases.push(
        br#"{"schema_version":1,"preferences":{"language":"unsupported","navigation_hints":true,"inverse_scroll_zoom":false,"material_tint":true,"interface_scale":100}}"#.to_vec(),
    );
    for bytes in cases {
        fs::write(store.path(), &bytes).unwrap();
        let result = store.load();
        assert_eq!(result.preferences, LocalPreferences::default());
        assert!(
            result.warning.is_some(),
            "malformed config had no diagnostic"
        );
        assert_eq!(fs::read(store.path()).unwrap(), bytes);
    }
    fs::write(store.path(), vec![b' '; MAX_PREFERENCES_BYTES + 1]).unwrap();
    assert!(matches!(
        store.load().warning,
        Some(PreferencesError::TooLarge)
    ));
}

#[test]
fn invalid_platform_path_and_write_failure_are_explicit() {
    assert!(matches!(
        PreferencesStore::new(Path::new("relative/config")),
        Err(PreferencesError::InvalidDirectory)
    ));
    let root = TempDir::new();
    let store = PreferencesStore::new(root.path()).unwrap();
    fs::create_dir(store.path()).unwrap();
    assert!(matches!(
        store.save(&LocalPreferences::default()),
        Err(PreferencesError::Write(_))
    ));
    assert!(store.path().is_dir());
    // The failed rename must not leave a temporary file beside the destination.
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}
