use super::*;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence;
use plan_my_cabinet::recovery::{AUTOSAVE_DELAY, RecoveryStore};
use std::{fs, time::Instant};
use uuid::Uuid;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("pmcab-cleanup-ui-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn saved(index: &mut RecoveryIndex, data: &Path, file: &Path) -> PathBuf {
    let mut editor = ProjectEditor::new(Project::new("Cabinet", Currency::Usd)).unwrap();
    persistence::save(&mut editor, file).unwrap();
    index.register_saved(file, editor.project().id).unwrap();
    editor
        .transact(|p| -> Result<(), ()> {
            p.name = "Draft".into();
            Ok(())
        })
        .unwrap();
    let mut store = RecoveryStore::new(data, file, editor.project().id).unwrap();
    let now = Instant::now();
    store.note_committed_edit(&editor, now).unwrap();
    store.tick(&editor, now + AUTOSAVE_DELAY).unwrap();
    store.recovery_path().to_path_buf()
}

#[test]
fn cancellation_and_partial_failure_preserve_unselected_and_saved_bytes() {
    let temp = Directory::new();
    let data = temp.0.join("data");
    let mut index = RecoveryIndex::open(&data).unwrap();
    let files: Vec<_> = (0..3).map(|n| temp.0.join(format!("{n}.pmcab"))).collect();
    let snapshots: Vec<_> = files
        .iter()
        .map(|file| saved(&mut index, &data, file))
        .collect();
    let saved_bytes: Vec<_> = files.iter().map(|file| fs::read(file).unwrap()).collect();
    let mut ui = CleanupUi::open(&data).unwrap();
    assert_eq!(ui.review().selected().count(), 0);
    assert!(!ui.begin_confirmation());
    let rows: Vec<_> = ui
        .review()
        .rows()
        .iter()
        .take(2)
        .map(|row| (row.identity.clone(), row.snapshot_path.clone()))
        .collect();
    assert!(ui.set_selected(&rows[0].0, &files[0], true).is_err());
    for (identity, path) in &rows {
        ui.set_selected(identity, path, true).unwrap();
    }
    ui.set_selected(&rows[1].0, &rows[1].1, false).unwrap();
    assert_eq!(ui.review().selected().count(), 1);
    ui.set_selected(&rows[1].0, &rows[1].1, true).unwrap();
    assert!(ui.begin_confirmation());
    assert_eq!(ui.pending().unwrap().selected().count(), 2);
    ui.cancel_confirmation();
    assert_eq!(ui.review().selected().count(), 0);
    assert!(snapshots.iter().all(|p| p.exists()));
    for (identity, path) in &rows {
        ui.set_selected(identity, path, true).unwrap();
    }
    assert!(ui.begin_confirmation());
    fs::write(&snapshots[0], b"changed since selection").unwrap();
    ui.confirm_deletion();
    let outcome = ui.outcome.as_ref().unwrap();
    assert_eq!(outcome.deleted, [snapshots[1].clone()]);
    assert_eq!(outcome.failed.len(), 1);
    assert_eq!(outcome.failed[0].snapshot_path, snapshots[0]);
    assert!(snapshots[0].exists() && snapshots[2].exists());
    assert!(!snapshots[1].exists());
    for (file, bytes) in files.iter().zip(saved_bytes) {
        assert_eq!(fs::read(file).unwrap(), bytes);
    }
    assert_eq!(ui.review().selected().count(), 0);
}

#[test]
fn headless_modal_displays_invalid_status_identity_and_paths_in_both_languages() {
    let temp = Directory::new();
    let data = temp.0.join("data");
    let file = temp.0.join("saved.pmcab");
    let mut index = RecoveryIndex::open(&data).unwrap();
    let snapshot = saved(&mut index, &data, &file);
    fs::write(&snapshot, b"invalid snapshot").unwrap();
    for language in [Language::En, Language::PtBr] {
        let mut ui = CleanupUi::open(&data).unwrap();
        assert!(matches!(
            ui.review().rows()[0].status,
            DiscoveryStatus::Invalid(_)
        ));
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.enable_accesskit();
        let mut output = ctx.run_ui(egui::RawInput::default(), |screen| {
            assert_eq!(
                ui.show(screen.ctx(), &Localizer::new(language)),
                CleanupIntent::None
            );
        });
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
            .collect();
        for expected in [
            file.display().to_string(),
            snapshot.display().to_string(),
            ui.review().rows()[0].identity.project_id.to_string(),
            Localizer::new(language).text("recovery-cleanup-invalid"),
        ] {
            assert!(
                labels.iter().any(|label| label.contains(&expected)),
                "missing {expected}: {labels:?}"
            );
        }
        output.textures_delta.clear();
    }
    assert!(snapshot.exists() && file.exists());
}

#[test]
fn focused_cancel_never_deletes_and_confirm_requires_deliberate_activation() {
    let temp = Directory::new();
    let data = temp.0.join("data");
    let file = temp.0.join("saved.pmcab");
    let mut index = RecoveryIndex::open(&data).unwrap();
    let snapshot = saved(&mut index, &data, &file);
    let saved_bytes = fs::read(&file).unwrap();
    let mut ui = CleanupUi::open(&data).unwrap();
    let (identity, path) = {
        let row = &ui.review().rows()[0];
        (row.identity.clone(), row.snapshot_path.clone())
    };
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let l = Localizer::new(Language::En);
    let draw = |ctx: &egui::Context, ui: &mut CleanupUi, input: egui::RawInput| {
        let mut intent = CleanupIntent::None;
        let mut output = ctx.run_ui(input, |screen| {
            intent = ui.show(screen.ctx(), &l);
        });
        output.textures_delta.clear();
        intent
    };
    let key = |key| egui::RawInput {
        events: vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    let release = |key| egui::RawInput {
        events: vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        ..Default::default()
    };
    assert_eq!(
        draw(&ctx, &mut ui, egui::RawInput::default()),
        CleanupIntent::None
    );
    // Enter activates the initially focused Cancel, even without selection.
    assert_eq!(
        draw(&ctx, &mut ui, key(egui::Key::Enter)),
        CleanupIntent::Closed
    );
    assert!(ui.pending().is_none() && snapshot.exists());
    draw(&ctx, &mut ui, release(egui::Key::Enter));
    ui.set_selected(&identity, &path, true).unwrap();
    // Selecting a snapshot must not turn keyboard Cancel into confirmation.
    assert_eq!(
        draw(&ctx, &mut ui, key(egui::Key::Enter)),
        CleanupIntent::Closed
    );
    assert!(ui.pending().is_none() && snapshot.exists());
    draw(&ctx, &mut ui, release(egui::Key::Enter));
    // Move deliberately from Cancel to the enabled review action.
    draw(&ctx, &mut ui, key(egui::Key::Tab));
    draw(&ctx, &mut ui, release(egui::Key::Tab));
    draw(&ctx, &mut ui, key(egui::Key::Enter));
    assert!(ui.pending().is_some());
    draw(&ctx, &mut ui, egui::RawInput::default());
    draw(&ctx, &mut ui, release(egui::Key::Enter));
    // The confirmation also starts on Cancel: Enter must preserve bytes.
    draw(&ctx, &mut ui, key(egui::Key::Enter));
    assert!(ui.pending().is_none() && snapshot.exists());
    assert_eq!(fs::read(&file).unwrap(), saved_bytes);
    assert!(ui.review().selected().next().is_none());
    draw(&ctx, &mut ui, release(egui::Key::Enter));
    ui.set_selected(&identity, &path, true).unwrap();
    assert!(ui.begin_confirmation());
    draw(&ctx, &mut ui, egui::RawInput::default());
    draw(&ctx, &mut ui, key(egui::Key::Escape));
    assert!(ui.pending().is_none() && snapshot.exists());
    draw(&ctx, &mut ui, release(egui::Key::Escape));
    ui.set_selected(&identity, &path, true).unwrap();
    assert!(ui.begin_confirmation());
    assert!(ui.pending().is_some());
    draw(&ctx, &mut ui, egui::RawInput::default());
    draw(&ctx, &mut ui, key(egui::Key::Tab));
    draw(&ctx, &mut ui, release(egui::Key::Tab));
    draw(&ctx, &mut ui, key(egui::Key::Enter));
    assert!(!snapshot.exists());
    assert_eq!(ui.outcome.as_ref().unwrap().deleted, [path]);
    assert_eq!(fs::read(&file).unwrap(), saved_bytes);
}

#[test]
fn recovery_cleanup_translations_have_bilingual_key_parity() {
    let keys = |source: &'static str| -> std::collections::BTreeSet<&'static str> {
        source
            .lines()
            .filter_map(|line| line.split_once(" = "))
            .map(|(key, _)| key)
            .filter(|key| key.starts_with("recovery-cleanup-"))
            .collect()
    };
    assert_eq!(
        keys(include_str!("../../../i18n/en.ftl")),
        keys(include_str!("../../../i18n/pt-BR.ftl"))
    );
}
