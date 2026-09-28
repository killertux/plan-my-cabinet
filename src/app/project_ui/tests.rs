use super::*;
use crate::template_setup_ui::TemplateSetupUi;
use plan_my_cabinet::reference_fixture;
use plan_my_cabinet::template_setup::TemplateKind;
use plan_my_cabinet::units::Length;
use std::fs;

#[test]
fn saving_migrated_source_requires_notice_and_cancel_keeps_original_bytes() {
    let dir = std::env::temp_dir().join(format!("pmcab-upgrade-{}", Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let source = dir.join("legacy.pmcab");
    let bytes = include_bytes!("../../../tests/fixtures/schema-v1-cabinet.pmcab");
    fs::write(&source, bytes).unwrap();
    let prepared = persistence::prepare_bytes(bytes).unwrap();
    assert_eq!(prepared.source_version(), 1);
    let mut app = DesktopApp {
        editor: prepared.into_editor(),
        ..Default::default()
    };
    app.project_files.path = Some(source.clone());
    app.save_to(&source, true, None);
    assert!(matches!(
        app.project_files.prompt,
        Some(Prompt::Upgrade(..))
    ));
    assert_eq!(fs::read(&source).unwrap(), bytes);
    let prompt = app.project_files.prompt.take().unwrap();
    app.resolve_project_choice(prompt, Some("cancel"));
    assert_eq!(fs::read(&source).unwrap(), bytes);
    app.project_files.pending_open = Some((
        source.clone(),
        persistence::prepare_bytes(bytes).unwrap().into_editor(),
    ));
    app.save_to(&source, true, Some(NextAction::Open));
    let prompt = app.project_files.prompt.take().unwrap();
    app.resolve_project_choice(prompt, Some("cancel"));
    assert!(app.project_files.pending_open.is_none());
    assert_eq!(fs::read(&source).unwrap(), bytes);
    app.save_to(&source, true, None);
    let prompt = app.project_files.prompt.take().unwrap();
    app.resolve_project_choice(prompt, Some("project-upgrade-save"));
    assert_eq!(
        persistence::prepare_reader(File::open(&source).unwrap())
            .unwrap()
            .source_version(),
        2
    );
    fs::remove_dir_all(dir).unwrap();
}

fn untitled_snapshots(dir: &Path) -> usize {
    RecoveryIndex::open(dir)
        .unwrap()
        .discover()
        .iter()
        .filter(|row| {
            matches!(
                row.status,
                plan_my_cabinet::recovery::DiscoveryStatus::Untitled
            )
        })
        .count()
}

#[test]
fn untitled_projects_autosave_after_inactivity_and_saving_discards_the_snapshot() {
    let dir = std::env::temp_dir().join(format!("pmcab-untitled-{}", Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let mut app = DesktopApp::default();
    app.project_files.user_data_dir = Some(dir.join("user"));
    app.editor
        .transact(|p| {
            p.name = "Kitchen".into();
            Ok::<_, ()>(())
        })
        .unwrap();
    let revision = app.editor.project().revision;
    let ctx = egui::Context::default();
    app.tick_untitled_recovery(&ctx, revision);
    assert!(
        !dir.join("user").exists(),
        "nothing is written before the delay"
    );
    app.project_files.untitled_pending = Some((
        revision,
        Instant::now() - AUTOSAVE_DELAY - Duration::from_secs(1),
    ));
    app.tick_untitled_recovery(&ctx, revision);
    assert_eq!(untitled_snapshots(&dir.join("user")), 1);

    let path = dir.join("kitchen.pmcab");
    app.save_to(&path, false, None);
    assert_eq!(app.project_files.path.as_deref(), Some(path.as_path()));
    assert_eq!(untitled_snapshots(&dir.join("user")), 0);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_panic_flush_writes_recovery_without_waiting_and_discarding_removes_it() {
    let dir = std::env::temp_dir().join(format!("pmcab-panic-{}", Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let mut app = DesktopApp::default();
    app.project_files.user_data_dir = Some(dir.join("user"));
    app.flush_recovery_after_panic();
    assert!(
        !dir.join("user").exists(),
        "a clean project has nothing to recover"
    );
    app.editor
        .transact(|p| {
            p.name = "Wardrobe".into();
            Ok::<_, ()>(())
        })
        .unwrap();
    app.flush_recovery_after_panic();
    assert_eq!(untitled_snapshots(&dir.join("user")), 1);
    // Leaving the untitled project on purpose (after Discard) drops it.
    app.proceed(NextAction::New);
    assert_eq!(untitled_snapshots(&dir.join("user")), 0);
    fs::remove_dir_all(dir).unwrap();
}

fn dialog_frame(app: &mut DesktopApp, ctx: &egui::Context, input: egui::RawInput) {
    let mut output = ctx.run_ui(input, |ui| app.show_project_dialog(ui.ctx()));
    output.textures_delta.clear();
}

fn dialog_key(key: egui::Key, modifiers: egui::Modifiers) -> egui::RawInput {
    egui::RawInput {
        events: vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
        ..Default::default()
    }
}

fn click_unsaved_footer(app: &mut DesktopApp, ctx: &egui::Context, label: &str) {
    dialog_frame(app, ctx, egui::RawInput::default());
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(780.0, 560.0),
            )),
            ..Default::default()
        },
        |ui| app.show_project_dialog(ui.ctx()),
    );
    fn text_center(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_center(shape, label)),
            _ => None,
        }
    }
    let point = output
        .shapes
        .iter()
        .find_map(|shape| text_center(&shape.shape, label));
    output.textures_delta.clear();
    let point = point.unwrap_or_else(|| panic!("unsaved footer button not rendered: {label}"));
    for pressed in [true, false] {
        dialog_frame(
            app,
            ctx,
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
        );
    }
}

#[test]
fn unsaved_footer_buttons_save_discard_or_cancel_without_crossing_decisions() {
    for decision in ["project-save", "project-discard", "cancel"] {
        let directory = Directory::new();
        let path = directory.0.join("original.pmcab");
        let mut app = directory.app();
        app.save_to(&path, false, None);
        let original_id = app.editor.project().id;
        app.editor
            .set_grid_spacing(Length::from_micrometres(20_000))
            .unwrap();
        app.project_files.prompt = Some(Prompt::Dirty(NextAction::New));
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let label = app.localizer.text(decision);
        click_unsaved_footer(&mut app, &ctx, &label);
        assert!(app.project_files.prompt.is_none(), "{decision}");
        let saved = persistence::prepare_reader(File::open(path).unwrap()).unwrap();
        assert_eq!(saved.project().id, original_id, "{decision}");
        assert_eq!(
            saved.project().grid_spacing.micrometres(),
            if decision == "project-save" {
                20_000
            } else {
                10_000
            },
            "{decision}"
        );
        if decision == "cancel" {
            assert_eq!(app.editor.project().id, original_id);
            assert!(app.editor.is_dirty());
        } else {
            assert_ne!(app.editor.project().id, original_id, "{decision}");
        }
    }
}

#[test]
fn unsaved_footer_cancel_restores_focus_and_clears_prepared_open() {
    let directory = Directory::new();
    let target = directory.0.join("next.pmcab");
    let mut next = ProjectEditor::new(Project::new("Next", Currency::Usd)).unwrap();
    persistence::save(&mut next, &target).unwrap();
    let mut app = directory.app();
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    let original = app.editor.project().clone();
    app.open_path(target);
    assert!(app.project_files.pending_open.is_some());
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut invoker = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let button = ui.button("Open project");
        invoker = Some(button.id);
        button.request_focus();
        app.show_project_dialog(ui.ctx());
    });
    output.textures_delta.clear();
    assert_ne!(ctx.memory(|m| m.focused()), invoker);
    let label = app.localizer.text("cancel");
    click_unsaved_footer(&mut app, &ctx, &label);
    assert!(app.project_files.prompt.is_none());
    assert!(app.project_files.pending_open.is_none());
    assert_eq!(app.editor.project(), &original);
    assert_eq!(ctx.memory(|m| m.focused()), invoker);
}

#[test]
fn unsaved_project_dialog_traps_focus_and_popup_keys_before_saving() {
    let directory = Directory::new();
    let path = directory.0.join("original.pmcab");
    let mut app = directory.app();
    app.save_to(&path, false, None);
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    let original_id = app.editor.project().id;
    app.project_files.prompt = Some(Prompt::Dirty(NextAction::New));
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut invoker = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let button = ui.button("New project");
        invoker = Some(button.id);
        button.request_focus();
        app.show_project_dialog(ui.ctx());
    });
    output.textures_delta.clear();
    assert!(
        app.project_files
            .prompt_chrome
            .as_ref()
            .unwrap()
            .is_active()
    );
    assert_ne!(ctx.memory(|m| m.focused()), invoker);
    for modifiers in [egui::Modifiers::NONE, egui::Modifiers::SHIFT] {
        dialog_frame(&mut app, &ctx, dialog_key(egui::Key::Tab, modifiers));
        assert!(ctx.memory(|m| m.focused()).is_some());
        assert_ne!(ctx.memory(|m| m.focused()), invoker);
    }
    let popup = egui::Id::new("project-dialog-popup");
    egui::Popup::open_id(&ctx, popup);
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
    );
    assert!(matches!(app.project_files.prompt, Some(Prompt::Dirty(_))));
    assert!(app.editor.is_dirty());
    egui::Popup::open_id(&ctx, popup);
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
    );
    assert!(matches!(app.project_files.prompt, Some(Prompt::Dirty(_))));
    egui::Popup::close_id(&ctx, popup);
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
    );
    assert!(app.project_files.prompt.is_none());
    assert!(app.editor.is_dirty());
    assert_eq!(ctx.memory(|m| m.focused()), invoker);
    app.project_files.prompt = Some(Prompt::Dirty(NextAction::New));
    dialog_frame(&mut app, &ctx, egui::RawInput::default());
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
    );
    assert!(app.project_files.prompt.is_none());
    assert!(
        !app.project_files
            .prompt_chrome
            .as_ref()
            .unwrap()
            .is_active()
    );
    assert_eq!(ctx.memory(|m| m.focused()), invoker);
    assert_ne!(app.editor.project().id, original_id);
    let saved = persistence::prepare_reader(File::open(path).unwrap()).unwrap();
    assert_eq!(saved.project().id, original_id);
    assert_eq!(saved.project().grid_spacing.micrometres(), 20_000);
}

#[test]
fn overwrite_escape_preserves_project_file_and_enter_replaces_only_after_confirmation() {
    let directory = Directory::new();
    let path = directory.0.join("existing.pmcab");
    fs::write(&path, b"existing bytes").unwrap();
    let mut app = directory.app();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    app.save_to(&path, false, None);
    assert!(matches!(
        app.project_files.prompt,
        Some(Prompt::Overwrite(..))
    ));
    dialog_frame(&mut app, &ctx, egui::RawInput::default());
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
    );
    assert!(app.project_files.prompt.is_none());
    assert_eq!(fs::read(&path).unwrap(), b"existing bytes");
    assert!(app.project_files.path.is_none());

    app.save_to(&path, false, None);
    dialog_frame(&mut app, &ctx, egui::RawInput::default());
    // Initial Cancel focus must not conceal a destructive default action.
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
    );
    assert!(app.project_files.prompt.is_none());
    assert_eq!(fs::read(&path).unwrap(), b"existing bytes");
    assert!(app.project_files.path.is_none());
    app.save_to(&path, false, None);
    dialog_frame(&mut app, &ctx, egui::RawInput::default());
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Tab, egui::Modifiers::NONE),
    );
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
    );
    assert!(app.project_files.prompt.is_none());
    assert_eq!(
        app.project_files.path.as_deref(),
        Some(path.as_path()),
        "message: {:?}",
        app.project_files.message
    );
    let saved = persistence::prepare_reader(File::open(path).unwrap()).unwrap();
    assert_eq!(saved.project().id, app.editor.project().id);
}

#[test]
fn unsaved_discard_keyboard_action_does_not_save_project_file() {
    let directory = Directory::new();
    let path = directory.0.join("before.pmcab");
    let mut app = directory.app();
    app.save_to(&path, false, None);
    let original_id = app.editor.project().id;
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    app.project_files.prompt = Some(Prompt::Dirty(NextAction::New));
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    dialog_frame(&mut app, &ctx, egui::RawInput::default());
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Tab, egui::Modifiers::SHIFT),
    );
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
    );
    assert!(app.project_files.prompt.is_none());
    assert_ne!(app.editor.project().id, original_id);
    let original_file = persistence::prepare_reader(File::open(path).unwrap()).unwrap();
    assert_eq!(original_file.project().id, original_id);
    assert_eq!(original_file.project().grid_spacing.micrometres(), 10_000);
}

#[test]
fn unsaved_escape_cancels_prepared_open_without_replacing_current_project() {
    let directory = Directory::new();
    let target = directory.0.join("next.pmcab");
    let mut other = ProjectEditor::new(Project::new("Next", Currency::Usd)).unwrap();
    persistence::save(&mut other, &target).unwrap();
    let mut app = directory.app();
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    let original = app.editor.project().clone();
    app.open_path(target);
    assert!(app.project_files.pending_open.is_some());
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    dialog_frame(&mut app, &ctx, egui::RawInput::default());
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
    );
    assert!(app.project_files.prompt.is_none());
    assert!(app.project_files.pending_open.is_none());
    assert_eq!(app.editor.project(), &original);
}

#[test]
fn failed_save_from_unsaved_dialog_keeps_work_and_reopens_decision() {
    let directory = Directory::new();
    let mut app = directory.app();
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    let original = app.editor.project().clone();
    let bad_path = directory.0.join("missing").join("project.pmcab");
    app.project_files.path = Some(bad_path.clone());
    app.project_files.prompt = Some(Prompt::Dirty(NextAction::New));
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    dialog_frame(&mut app, &ctx, egui::RawInput::default());
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
    );
    assert!(matches!(
        app.project_files.prompt,
        Some(Prompt::Dirty(NextAction::New))
    ));
    assert_eq!(app.editor.project(), &original);
    assert!(app.editor.is_dirty());
    assert!(
        app.project_files
            .message
            .as_ref()
            .unwrap()
            .contains("missing")
    );
    assert!(!bad_path.exists());
    dialog_frame(&mut app, &ctx, egui::RawInput::default());
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
    );
    assert!(app.project_files.prompt.is_none());
    assert_eq!(app.editor.project(), &original);
}

#[test]
fn recovery_dialog_defers_without_deleting_and_recovers_only_on_confirmation() {
    let directory = Directory::new();
    let path = directory.0.join("cabinet.pmcab");
    let mut app = directory.app();
    app.save_to(&path, false, None);
    let saved = app.editor.project().clone();
    let mut recovering = ProjectEditor::new(saved.clone()).expect("saved project remains valid");
    recovering
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    let mut store = RecoveryStore::new(&directory.0.join("user-data"), &path, saved.id).unwrap();
    let now = Instant::now();
    store
        .note_committed_edit(&recovering, now - Duration::from_secs(31))
        .unwrap();
    assert!(store.tick(&recovering, now).unwrap());
    let snapshot = store.recovery_path().to_path_buf();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    app.project_files.prompt = Some(Prompt::Recovery(Box::new(
        store.inspect(&saved).unwrap().unwrap(),
    )));
    dialog_frame(&mut app, &ctx, egui::RawInput::default());
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Escape, egui::Modifiers::NONE),
    );
    assert!(app.project_files.prompt.is_none());
    assert_eq!(app.editor.project(), &saved);
    assert!(snapshot.exists());
    app.project_files.prompt = Some(Prompt::Recovery(Box::new(
        store.inspect(&saved).unwrap().unwrap(),
    )));
    dialog_frame(&mut app, &ctx, egui::RawInput::default());
    // Enter on the visibly focused Defer button preserves the saved work.
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
    );
    assert!(app.project_files.prompt.is_none());
    assert_eq!(app.editor.project(), &saved);
    assert!(snapshot.exists());
    app.project_files.prompt = Some(Prompt::Recovery(Box::new(
        store.inspect(&saved).unwrap().unwrap(),
    )));
    dialog_frame(&mut app, &ctx, egui::RawInput::default());
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Tab, egui::Modifiers::NONE),
    );
    dialog_frame(
        &mut app,
        &ctx,
        dialog_key(egui::Key::Enter, egui::Modifiers::NONE),
    );
    assert!(app.project_files.prompt.is_none());
    assert!(app.editor.is_dirty());
    assert_eq!(app.editor.project().grid_spacing.micrometres(), 20_000);
    assert_eq!(app.project_files.path.as_deref(), Some(path.as_path()));
    assert_eq!(
        persistence::prepare_reader(File::open(path).unwrap())
            .unwrap()
            .project(),
        &saved
    );
    assert!(snapshot.exists());
}

#[test]
fn template_save_failure_and_picker_cancel_retain_document_and_staged_setup() {
    let mut app = DesktopApp::default();
    app.template.setup = Some(TemplateSetupUi::new(
        TemplateKind::Base,
        "Next",
        Currency::Brl,
        plan_my_cabinet::units::Unit::Mm,
    ));
    app.editor
        .transact(|p| -> Result<(), ()> {
            p.name = "Original work".into();
            Ok(())
        })
        .unwrap();
    let original = app.editor.project().clone();
    app.template.guard_pending = true;
    let missing_parent = std::env::temp_dir()
        .join(Uuid::new_v4().to_string())
        .join("project.pmcab");
    app.save_to(&missing_parent, false, Some(NextAction::Template));
    assert!(matches!(
        app.project_files.prompt,
        Some(Prompt::Dirty(NextAction::Template))
    ));
    assert_eq!(app.editor.project(), &original);
    assert!(app.template.setup.is_some());
    let prompt = app.project_files.prompt.take().unwrap();
    app.resolve_project_choice(prompt, Some("cancel"));
    assert!(!app.template.guard_pending);

    let (tx, rx) = mpsc::channel();
    app.template.guard_pending = true;
    app.project_files.picker = Some(Picker {
        kind: PickerKind::Save(Some(NextAction::Template)),
        project_id: original.id,
        revision: original.revision,
        result: rx,
    });
    tx.send(None).unwrap();
    app.tick_project_files(&egui::Context::default());
    assert!(!app.template.guard_pending);
    assert!(app.project_files.picker.is_none());
    assert_eq!(app.editor.project(), &original);
    assert!(app.template.setup.is_some());
}
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("pmcab-ui-recents-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn app(&self) -> DesktopApp {
        let mut app = DesktopApp::default();
        app.project_files.user_data_dir = Some(self.0.join("user-data"));
        app
    }

    fn recents(&self) -> RecentProjects {
        RecentProjects::open(&self.0.join("user-data")).unwrap()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn successful_save_captures_committed_scene_and_ignores_preview_geometry() {
    let directory = Directory::new();
    let path = directory.0.join("cabinet.pmcab");
    let mut app = directory.app();
    app.editor = ProjectEditor::new(reference_fixture::project()).unwrap();
    app.save_to(&path, false, None);
    assert_eq!(
        app.project_files.path.as_deref(),
        Some(path.as_path()),
        "{:?}",
        app.project_files.message
    );
    assert!(!app.editor.is_dirty());
    let first = directory.recents().list("");
    let key = first[0]
        .entry
        .thumbnail_key
        .clone()
        .expect("saved thumbnail");
    let pixels = first[0].thumbnail.as_ref().expect("cached pixels").clone();
    assert!(pixels.chunks_exact(4).any(|p| p != [236, 232, 225, 255]));
    app.selection
        .choose(Some(app.editor.project().boards[0].id), false);
    app.editor.begin_preview();
    app.editor
        .update_preview(|project| -> Result<(), ()> {
            project.boards[0].pose.translation_mm[0] += 10_000.0;
            Ok(())
        })
        .unwrap();
    app.capture_saved_thumbnail(&path);
    let row = &directory.recents().list("")[0];
    assert_eq!(row.entry.thumbnail_key.as_deref(), Some(key.as_str()));
    assert_eq!(row.thumbnail.as_ref(), Some(&pixels));
    assert_eq!(
        app.selection.active,
        Some(app.editor.project().boards[0].id)
    );
    assert_ne!(
        app.editor.preview().unwrap().boards[0].pose,
        app.editor.project().boards[0].pose
    );
}

#[test]
fn local_thumbnail_write_failure_is_nonfatal_and_shows_placeholder() {
    let directory = Directory::new();
    let data = directory.0.join("user-data");
    fs::create_dir(&data).unwrap();
    fs::write(data.join("thumbnails"), b"occupied").unwrap();
    let path = directory.0.join("saved.pmcab");
    let mut app = directory.app();
    app.editor = ProjectEditor::new(reference_fixture::project()).unwrap();
    app.save_to(&path, false, None);
    assert_eq!(app.project_files.path.as_deref(), Some(path.as_path()));
    assert!(!app.editor.is_dirty());
    let saved = persistence::prepare_reader(File::open(&path).unwrap()).unwrap();
    assert_eq!(saved.project().id, app.editor.project().id);
    assert_eq!(
        saved.project().boards.len(),
        app.editor.project().boards.len()
    );
    let row = &directory.recents().list("")[0];
    assert!(row.entry.thumbnail_key.is_none());
    assert!(row.thumbnail.is_none());
}

#[test]
fn background_packet_preparation_does_not_block_project_prompt_and_is_cancelled_on_replace() {
    let mut app = DesktopApp::default();
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    let original = app.editor.project().id;
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    app.handoff.preparation_pending = Some((app.export_key(), rx, Arc::clone(&cancel)));

    assert!(app.action_availability(Request::new(A::NewProject)).is_ok());
    app.request_project_action(NextAction::New);
    let prompt = app.project_files.prompt.take().expect("dirty prompt");
    assert!(matches!(prompt, Prompt::Dirty(NextAction::New)));
    app.resolve_project_choice(prompt, Some("cancel"));
    assert_eq!(app.editor.project().id, original);
    assert!(!cancel.load(Ordering::Relaxed));

    app.request_project_action(NextAction::New);
    let prompt = app
        .project_files
        .prompt
        .take()
        .expect("repeat dirty prompt");
    app.resolve_project_choice(prompt, Some("project-discard"));
    assert_ne!(app.editor.project().id, original);
    assert!(cancel.load(Ordering::Relaxed));
    assert!(app.handoff.preparation_pending.is_none());
    drop(tx);
}

#[test]
fn only_accepted_open_and_successful_explicit_saves_register() {
    let directory = Directory::new();
    let first = directory.0.join("first.pmcab");
    let second = directory.0.join("second.pmcab");
    let mut app = directory.app();
    app.save_to(&first, false, None);
    let id = app.editor.project().id;
    assert_eq!(
        directory.recents().entries()[0].path,
        fs::canonicalize(&first).unwrap()
    );
    assert_eq!(directory.recents().entries()[0].project_id, id);
    assert_eq!(
        RecoveryIndex::open(&directory.0.join("user-data"))
            .unwrap()
            .entries()[0]
            .saved_path
            .as_deref(),
        Some(fs::canonicalize(&first).unwrap().as_path())
    );

    app.save_to(&second, false, None); // Save As is a second path for the same UUID.
    assert_eq!(directory.recents().entries().len(), 2);
    assert_eq!(
        directory.recents().entries()[0].path,
        fs::canonicalize(&second).unwrap()
    );
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    app.save_to(&second, true, None); // Explicit overwrite refreshes the existing entry.
    assert_eq!(directory.recents().entries().len(), 2);

    app.editor
        .set_grid_spacing(Length::from_micrometres(30_000))
        .unwrap();
    app.open_path(first.clone());
    assert!(matches!(
        app.project_files.prompt,
        Some(Prompt::Dirty(NextAction::Open))
    ));
    assert!(app.project_files.pending_open.is_some());
    assert_eq!(
        directory.recents().entries()[0].path,
        fs::canonicalize(&second).unwrap()
    );
    let prompt = app.project_files.prompt.take().unwrap();
    app.resolve_project_choice(prompt, Some("cancel"));
    assert!(app.project_files.pending_open.is_none());
    assert_eq!(app.editor.project().grid_spacing.micrometres(), 30_000);
    assert_eq!(
        directory.recents().entries()[0].path,
        fs::canonicalize(&second).unwrap()
    );

    app.open_path(first.clone());
    let prompt = app.project_files.prompt.take().unwrap();
    app.resolve_project_choice(prompt, Some("project-discard"));
    assert_eq!(app.project_files.path.as_deref(), Some(first.as_path()));
    assert_eq!(
        directory.recents().entries()[0].path,
        fs::canonicalize(&first).unwrap()
    );
    assert_eq!(directory.recents().entries()[0].project_id, id);
    assert!(!app.editor.is_dirty());
}

#[test]
fn prepared_open_is_not_recent_until_dirty_work_is_resolved() {
    let directory = Directory::new();
    let path = directory.0.join("external.pmcab");
    let mut external = ProjectEditor::new(Project::new("External", Currency::Usd)).unwrap();
    persistence::save(&mut external, &path).unwrap();
    let mut app = directory.app();
    let original = app.editor.project().id;
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    app.open_path(path.clone());
    assert!(app.project_files.pending_open.is_some());
    assert_eq!(app.editor.project().id, original);
    assert!(directory.recents().entries().is_empty());
    assert!(
        RecoveryIndex::open(&directory.0.join("user-data"))
            .unwrap()
            .entries()
            .is_empty()
    );
    let prompt = app.project_files.prompt.take().unwrap();
    assert!(matches!(prompt, Prompt::Dirty(NextAction::Open)));
    let (tx, rx) = mpsc::channel();
    app.project_files.picker = Some(Picker {
        kind: PickerKind::Save(Some(NextAction::Open)),
        project_id: original,
        revision: app.editor.project().revision,
        result: rx,
    });
    tx.send(None).unwrap();
    app.tick_project_files(&egui::Context::default());
    assert!(app.project_files.pending_open.is_none());
    assert!(directory.recents().entries().is_empty());

    app.open_path(path.clone());
    let prompt = app.project_files.prompt.take().unwrap();
    app.resolve_project_choice(prompt, Some("project-discard"));
    assert_eq!(app.editor.project().id, external.project().id);
    assert_eq!(directory.recents().entries().len(), 1);
    assert_eq!(
        directory.recents().entries()[0].path,
        fs::canonicalize(path).unwrap()
    );
}

#[test]
fn failed_open_save_and_cancelled_picker_do_not_register() {
    let directory = Directory::new();
    let mut app = directory.app();
    let original = app.editor.project().clone();
    let (tx, rx) = mpsc::channel();
    app.project_files.picker = Some(Picker {
        kind: PickerKind::Open,
        project_id: original.id,
        revision: original.revision,
        result: rx,
    });
    tx.send(None).unwrap();
    app.tick_project_files(&egui::Context::default());
    let (tx, rx) = mpsc::channel();
    app.project_files.picker = Some(Picker {
        kind: PickerKind::Save(None),
        project_id: original.id,
        revision: original.revision,
        result: rx,
    });
    tx.send(None).unwrap();
    app.tick_project_files(&egui::Context::default());
    app.open_path(directory.0.join("missing.pmcab"));
    fs::write(directory.0.join("invalid.pmcab"), b"invalid").unwrap();
    app.open_path(directory.0.join("invalid.pmcab"));
    app.save_to(&directory.0.join("absent/out.pmcab"), false, None);
    assert!(directory.recents().entries().is_empty());
    assert!(app.project_files.path.is_none());
    assert_eq!(app.editor.project(), &original);
    assert!(
        app.project_files
            .message
            .as_ref()
            .unwrap()
            .contains("absent")
    );

    let path = directory.0.join("existing.pmcab");
    fs::write(&path, b"existing").unwrap();
    app.save_to(&path, false, None);
    assert!(matches!(
        app.project_files.prompt,
        Some(Prompt::Overwrite(_, None))
    ));
    let prompt = app.project_files.prompt.take().unwrap();
    app.resolve_project_choice(prompt, Some("cancel"));
    assert_eq!(fs::read(path).unwrap(), b"existing");
    assert!(directory.recents().entries().is_empty());
}

#[test]
fn failed_local_index_write_reports_error_without_undoing_save_or_open() {
    let directory = Directory::new();
    let user_data = directory.0.join("user-data");
    fs::create_dir(&user_data).unwrap();
    // A directory occupying the index filename makes local indexing fail
    // deterministically even when the project file is fully writable.
    fs::create_dir(user_data.join("recent-projects.json")).unwrap();
    let path = directory.0.join("valid.pmcab");
    let mut app = directory.app();
    app.save_to(&path, false, None);
    let saved = app.editor.project().clone();
    assert!(!app.editor.is_dirty());
    assert_eq!(app.project_files.path.as_deref(), Some(path.as_path()));
    assert!(
        app.project_files
            .message
            .as_ref()
            .unwrap()
            .contains("Recent projects:")
    );
    assert_eq!(
        persistence::prepare_reader(File::open(&path).unwrap())
            .unwrap()
            .project(),
        &saved
    );

    app.proceed(NextAction::New);
    app.open_path(path.clone());
    assert_eq!(app.editor.project(), &saved);
    assert!(
        app.project_files
            .message
            .as_ref()
            .unwrap()
            .contains("Recent projects:")
    );
    assert_eq!(
        RecoveryIndex::open(&user_data).unwrap().entries()[0].project_id,
        saved.id
    );

    let pending_path = directory.0.join("other.pmcab");
    let mut other = ProjectEditor::new(Project::new("Other", Currency::Usd)).unwrap();
    persistence::save(&mut other, &pending_path).unwrap();
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    app.open_path(pending_path);
    assert!(app.project_files.pending_open.is_some());
    let prompt = app.project_files.prompt.take().unwrap();
    assert!(matches!(prompt, Prompt::Dirty(NextAction::Open)));
    app.save_to(&path, true, Some(NextAction::Open));
    assert_eq!(app.editor.project().id, other.project().id);
    assert!(
        app.project_files
            .message
            .as_ref()
            .unwrap()
            .contains("Recent projects:")
    );
}

#[test]
fn stale_picker_result_and_cancellation_preserve_current_editor() {
    let mut app = DesktopApp::default();
    let original = app.editor.project().clone();
    let (tx, rx) = mpsc::channel();
    app.project_files.picker = Some(Picker {
        kind: PickerKind::Save(None),
        project_id: original.id,
        revision: original.revision,
        result: rx,
    });
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    tx.send(Some(PathBuf::from("ignored.pmcab"))).unwrap();
    app.tick_project_files(&egui::Context::default());
    assert!(app.project_files.picker.is_none());
    assert!(app.project_files.path.is_none());
    assert!(app.editor.is_dirty());
    assert_eq!(app.editor.project().grid_spacing.micrometres(), 20_000);
    let (tx, rx) = mpsc::channel();
    app.project_files.picker = Some(Picker {
        kind: PickerKind::Open,
        project_id: original.id,
        revision: app.editor.project().revision,
        result: rx,
    });
    tx.send(None).unwrap();
    app.tick_project_files(&egui::Context::default());
    assert!(app.editor.is_dirty());
    assert!(app.project_files.pending_open.is_none());
}
