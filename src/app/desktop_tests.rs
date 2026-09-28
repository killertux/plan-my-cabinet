use super::*;
use std::fs;

#[test]
fn template_replacement_waits_for_dirty_decision_and_commits_one_undo() {
    use plan_my_cabinet::domain::BoardGrain;
    use plan_my_cabinet::template_setup::{MaterialRole, TemplateKind};
    use plan_my_cabinet::units::{Conversion, Length};

    let mut app = DesktopApp::default();
    app.editor
        .transact(|project| -> Result<(), ()> {
            project.name = "Keep my edits".into();
            Ok(())
        })
        .unwrap();
    let old = app.editor.project().clone();
    let mut setup =
        TemplateSetupUi::new(TemplateKind::Base, "New base", Currency::Brl, Unit::Mm);
    let proposed = |mm: i64| {
        plan_my_cabinet::template_setup::ProposedLength::new(Conversion::Exact(
            Length::from_micrometres(mm * 1_000),
        ))
    };
    let carcass =
        setup
            .setup
            .add_material("Cabinet", proposed(18), BoardGrain::Unrestricted, None);
    let back = setup
        .setup
        .add_material("Back", proposed(6), BoardGrain::Unrestricted, None);
    setup.setup.roles.insert(MaterialRole::Carcass, carcass);
    setup.setup.roles.insert(MaterialRole::Back, back);
    assert!(setup.setup.review().is_ok());
    app.template.setup = Some(setup);
    app.template.guard_pending = true;
    app.request_project_action(project_ui::NextAction::Template);
    assert!(matches!(
        app.project_files.prompt,
        Some(project_ui::Prompt::Dirty(project_ui::NextAction::Template))
    ));
    assert_eq!(app.editor.project(), &old);
    let prompt = app.project_files.prompt.take().unwrap();
    app.resolve_project_choice(prompt, Some("cancel"));
    assert!(!app.template.guard_pending);
    assert!(app.template.setup.is_some());
    assert_eq!(app.editor.project(), &old);

    app.template.guard_pending = true;
    app.request_project_action(project_ui::NextAction::Template);
    let prompt = app.project_files.prompt.take().unwrap();
    app.resolve_project_choice(prompt, Some("project-discard"));
    assert!(!app.project_files.welcome.visible);
    assert!(app.template.setup.is_none());
    assert_eq!(app.editor.project().name, "New base");
    assert!(app.project_files.path.is_none());
    assert!(app.editor.is_dirty());
    assert_eq!(app.session.active, Workspace::Design);
    assert!(
        app.editor
            .project()
            .assemblies
            .iter()
            .any(|assembly| Some(assembly.id) == app.selection.active)
    );
    assert!(!app.editor.project().boards.is_empty());
    app.editor.undo().unwrap();
    assert!(app.editor.project().materials.is_empty());
    assert!(app.editor.project().boards.is_empty());
    assert!(app.editor.project().assemblies.is_empty());
}

#[test]
fn first_run_drawers_tile_generates_editable_unsaved_project_and_stock_route() {
    use plan_my_cabinet::domain::BoardGrain;
    use plan_my_cabinet::template_setup::{MaterialRole, TemplateKind};
    use plan_my_cabinet::units::{Conversion, Length};
    let mut app = DesktopApp::default();
    let local_dir =
        std::env::temp_dir().join(format!("pmcab-template-recents-{}", Uuid::new_v4()));
    app.project_files.user_data_dir = Some(local_dir.clone());
    assert!(app.project_files.welcome.visible);
    app.handle_welcome_intent(plan_my_cabinet::welcome_ui::WelcomeIntent::Template(
        TemplateKind::Drawers,
    ));
    let setup = &mut app.template.setup.as_mut().unwrap().setup;
    setup.project_name = "Three drawers".into();
    let value = |mm: i64| {
        plan_my_cabinet::template_setup::ProposedLength::new(Conversion::Exact(
            Length::from_micrometres(mm * 1_000),
        ))
    };
    let carcass = setup.add_material("Carcass", value(18), BoardGrain::Length, None);
    let back = setup.add_material("Back / bottom", value(6), BoardGrain::Unrestricted, None);
    let box_id = setup.add_material("Box", value(12), BoardGrain::Length, None);
    let front = setup.add_material("Front", value(19), BoardGrain::Length, None);
    for (role, id) in [
        (MaterialRole::Carcass, carcass),
        (MaterialRole::Back, back),
        (MaterialRole::Box, box_id),
        (MaterialRole::BoxBottom, back),
        (MaterialRole::ExternalFront, front),
    ] {
        setup.roles.insert(role, id);
    }
    assert!(setup.review().is_ok());
    app.template.guard_pending = true;
    app.request_project_action(project_ui::NextAction::Template);
    assert!(!app.project_files.welcome.visible);
    assert_eq!(app.editor.project().boards.len(), 23);
    assert_eq!(app.editor.project().materials.len(), 4);
    assert!(app.editor.project().stock.is_empty());
    assert!(app.editor.project().allocations.is_empty());
    assert!(app.template.message.is_some());
    assert_eq!(app.session.active, Workspace::Design);
    assert!(
        app.editor
            .project()
            .assemblies
            .iter()
            .any(|a| Some(a.id) == app.selection.active)
    );
    assert!(app.project_files.path.is_none());
    assert!(app.editor.is_dirty());
    assert!(
        plan_my_cabinet::recent_projects::RecentProjects::open(&local_dir)
            .unwrap()
            .list("")
            .is_empty()
    );
    let before = app.editor.project().clone();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let size = egui::vec2(1100.0, 720.0);
    for workspace in [Workspace::Design, Workspace::Hardware, Workspace::Design] {
        app.session.switch(workspace);
        for _ in 0..3 {
            responsive_frame(&mut app, &ctx, size, vec![]);
        }
        assert!(app.session.belongs_to(app.editor.project()));
        let target = app
            .session
            .view_mut(workspace)
            .camera
            .framed_target()
            .unwrap();
        assert!((target[0] - 300.0).abs() < 0.001, "{target:?}");
        assert!((target[2] - 360.0).abs() < 0.001, "{target:?}");
        app.session
            .view_mut(workspace)
            .camera
            .assert_framed_occupancy(app.editor.project(), &app.selection);
    }
    assert_eq!(app.editor.project(), &before);
}

#[test]
fn settings_actions_isolate_the_scene_and_preserve_unit_edit_history() {
    let mut app = DesktopApp::default();
    let ctx = egui::Context::default();
    let before = app.editor.project().clone();
    app.invoke(Request::new(A::OpenSettings)).unwrap();
    assert!(app.settings.open);
    assert_eq!(
        app.action_availability(Request::new(A::NewBoard)),
        Err(actions::Unavailable::ModalOpen)
    );
    app.apply_settings_intent(&ctx, SettingsIntent::SetDisplayUnit(Unit::Foot));
    assert_eq!(app.editor.project().display_unit, Unit::Foot);
    assert_eq!(app.editor.project().revision, before.revision);
    assert!(!app.editor.can_undo());
    app.apply_settings_intent(&ctx, SettingsIntent::SetFreeCutFee);
    assert_eq!(app.editor.project().cut_fee.unwrap().minor_units(), 0);
    assert!(app.editor.can_undo());
    app.apply_settings_intent(&ctx, SettingsIntent::EditGrid);
    assert!(!app.settings.open);
    assert!(app.settings.resume_after_dialog);
    assert!(app.modals.grid().is_some());
    app.modals.set_grid(None);
    ctx.begin_pass(egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 700.0),
        )),
        ..Default::default()
    });
    app.show_settings(&ctx);
    ctx.end_pass().drop_without_applying_deltas();
    assert!(app.settings.open);
    assert!(!app.settings.resume_after_dialog);
    app.apply_settings_intent(&ctx, SettingsIntent::Done);
    assert!(!app.settings.open);

    app.invoke(Request::new(A::OpenSettings)).unwrap();
    app.apply_settings_intent(&ctx, SettingsIntent::ChangeCurrency);
    assert!(!app.settings.open);
    assert!(app.modals.currency().is_some());
    app.modals.set_currency(None);
    app.apply_settings_intent(&ctx, SettingsIntent::WorkedExamples);
    assert!(app.settings.worked_examples);
    assert!(app.other_modal_open());
}

#[test]
fn portuguese_status_uses_overflow_before_labels_can_collide() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::install_fonts(&ctx);
    let mut app = DesktopApp::default();
    app.localizer.set_language(Language::PtBr);
    let issues = workspace_shell::IssueCounts {
        unallocated: 22,
        ..Default::default()
    };
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(560.0, 700.0),
            )),
            ..Default::default()
        },
        |ui| {
            assert!(app.shell_status_required_width(ui, issues, None, false) > 560.0);
            app.show_shell_status(ui, issues, None, false);
        },
    );
    output.textures_delta.clear();
    let nodes = &output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes;
    assert!(nodes.iter().any(|(_, node)| node.label() == Some("Mais")));
    assert!(!nodes.iter().any(|(_, node)| {
        node.value()
            .is_some_and(|text| text.contains("Nova despesa estimada"))
    }));
}

#[test]
fn hardware_without_installations_does_not_claim_a_deleted_selection() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::install_fonts(&ctx);
    let mut app = DesktopApp::default();
    app.session.active = Workspace::Hardware;
    for language in [Language::En, Language::PtBr] {
        app.localizer.set_language(language);
        let mut output = ctx.run_ui(Default::default(), |ui| app.show_workspace_inspector(ui));
        output.textures_delta.clear();
        let nodes = &output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        assert!(nodes.iter().any(|(_, node)| node.value()
            == Some(app.localizer.text("hardware-no-installations").as_str())));
        assert!(!nodes.iter().any(|(_, node)| node.value()
            == Some(app.localizer.text("hinge-selection-missing").as_str())));
    }
}

#[test]
fn cut_plan_inspector_wraps_optimizer_heading_inside_scroll_viewport() {
    for language in [Language::En, Language::PtBr] {
        for width in [240.0, 292.0, 320.0] {
            let ctx = egui::Context::default();
            theme::install_fonts(&ctx);
            let mut app = navigation_app();
            app.localizer.set_language(language);
            app.session.active = Workspace::CutPlan;
            app.session.focused_sheet =
                Some(plan_my_cabinet::reference_fixture::WHITE_STOCK_ID);
            let before = app.editor.project().clone();
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| app.show_workspace(ui),
            )
            .drop_without_applying_deltas();
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 3000.0),
                    )),
                    ..Default::default()
                },
                |ui| app.show_scrolled_workspace_inspector(ui, width, 3000.0),
            );
            let label = app.localizer.text("optimize-all-sheets").to_uppercase();
            let (clip, text) = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some((shape.clip_rect, text))
                    }
                    _ => None,
                })
                .expect("optimizer heading must remain visible");
            let bounds = text.galley.rect.translate(text.pos.to_vec2());
            assert!(
                clip.expand(1.0).contains_rect(bounds),
                "{language:?} at {width}: {bounds:?} outside {clip:?}"
            );
            assert!(bounds.right() <= width + 1.0);
            assert_eq!(app.editor.project(), &before);
            output.drop_without_applying_deltas();
        }
    }
}

#[test]
fn settings_recovery_cleanup_opens_unselected_review_without_editing_project() {
    let root = TempConfig::new();
    let mut app = DesktopApp::default();
    app.project_files.user_data_dir = Some(root.0.clone());
    let before = app.editor.project().clone();
    app.invoke(Request::new(A::OpenSettings)).unwrap();
    app.apply_settings_intent(
        &egui::Context::default(),
        SettingsIntent::ReviewRecoveryCleanup,
    );
    assert!(!app.settings.open);
    assert_eq!(
        app.settings.cleanup
            .as_ref()
            .unwrap()
            .review()
            .selected()
            .count(),
        0
    );
    assert_eq!(
        app.action_availability(Request::new(A::NewBoard)),
        Err(actions::Unavailable::ModalOpen)
    );
    assert_eq!(app.editor.project(), &before);
    app.settings.cleanup = None;
    app.settings.open = true;
    assert_eq!(app.editor.project(), &before);
}

#[test]
fn navigation_hint_preference_controls_contextual_status_without_editing() {
    let mut app = DesktopApp::default();
    let ctx = egui::Context::default();
    let original = app.editor.project().clone();
    let render = |app: &mut DesktopApp| {
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_shell_status_contents(
                ui,
                workspace_shell::IssueCounts::default(),
                None,
                false,
            );
        });
        let text = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        output.drop_without_applying_deltas();
        text
    };
    // Hints only appear for tools whose gestures need explaining.
    assert!(!render(&mut app).contains("Drag selected board"));
    app.design.move_tool.mode = viewport::ToolMode::Move;
    assert!(render(&mut app).contains("Drag selected board"));
    app.set_navigation_hints(false);
    assert!(!render(&mut app).contains("Drag selected board"));
    assert_eq!(app.editor.project(), &original);
    assert!(!app.editor.can_undo());
}

#[test]
fn welcome_preferences_work_without_document_at_all_supported_scales() {
    use plan_my_cabinet::welcome_ui::WelcomeIntent;
    let mut app = DesktopApp::default();
    let old = app.editor.project().clone();
    app.localizer.set_language(Language::PtBr);
    app.handle_welcome_intent(WelcomeIntent::Preferences);
    assert!(app.settings.open && app.project_files.welcome.visible);
    app.settings.state.section = SettingsSection::General;
    let ctx = egui::Context::default();
    for scale in [
        InterfaceScale::Percent90,
        InterfaceScale::Percent100,
        InterfaceScale::Percent115,
        InterfaceScale::Percent130,
    ] {
        app.apply_settings_intent(&ctx, SettingsIntent::SetScale(scale));
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(780.0, 560.0) / scale.factor(),
                )),
                ..Default::default()
            },
            |ui| app.show_settings(ui.ctx()),
        );
        assert!(!output.shapes.is_empty());
        output.drop_without_applying_deltas();
        assert_eq!(ctx.zoom_factor(), scale.factor());
        assert_eq!(app.editor.project(), &old);
    }
    app.apply_settings_intent(&ctx, SettingsIntent::Done);
    assert!(!app.settings.open);
    assert!(app.project_files.welcome.visible);
}

struct TempConfig(PathBuf);

impl TempConfig {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("pmcab-native-prefs-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for TempConfig {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn native_startup_restores_preferences_and_language_action_saves_without_project_edit() {
    let root = TempConfig::new();
    let config = root.0.join("config");
    let ctx = egui::Context::default();
    let mut app = DesktopApp::default();
    app.load_preferences(&ctx, &config);
    let before = plan_my_cabinet::persistence::serialize(app.editor.project()).unwrap();
    let revision = app.editor.project().revision;
    app.set_navigation_hints(false);
    app.set_inverse_scroll_zoom(true);
    app.set_material_tint(false);
    app.set_interface_scale(&ctx, InterfaceScale::Percent115);
    app.invoke(Request::new(A::SetUiLanguage).argument(Argument::Language(Language::PtBr)))
        .unwrap();
    assert!(app.preferences_error.is_none());
    assert_eq!(app.localizer.language(), Language::PtBr);
    assert_eq!(app.editor.project().revision, revision);
    assert!(!app.editor.is_dirty());
    assert!(!app.editor.can_undo());
    assert_eq!(
        plan_my_cabinet::persistence::serialize(app.editor.project()).unwrap(),
        before
    );

    let mut restarted = DesktopApp::default();
    let restart_ctx = egui::Context::default();
    restarted.load_preferences(&restart_ctx, &config);
    assert_eq!(restarted.preferences.language, Language::PtBr);
    assert_eq!(restarted.localizer.language(), Language::PtBr);
    assert_eq!(
        restarted.preferences.interface_scale,
        InterfaceScale::Percent115
    );
    restart_ctx
        .run_ui(egui::RawInput::default(), |_| {})
        .drop_without_applying_deltas();
    assert!((restart_ctx.zoom_factor() - 1.15).abs() < f32::EPSILON);
    assert!(!restarted.preferences.navigation_hints);
    assert!(restarted.preferences.inverse_scroll_zoom);
    assert!(!restarted.preferences.material_tint);
    assert!(restarted.preferences_error.is_none());
    assert!(!restarted.editor.is_dirty());
}

#[test]
fn malformed_config_and_write_failure_are_visible_without_project_revision() {
    let root = TempConfig::new();
    let store = PreferencesStore::new(&root.0).unwrap();
    fs::write(store.path(), b"{invalid").unwrap();
    let mut app = DesktopApp::default();
    let ctx = egui::Context::default();
    app.load_preferences(&ctx, &root.0);
    assert_eq!(app.preferences, LocalPreferences::default());
    assert!(app.preferences_error.is_some());
    assert_eq!(fs::read(store.path()).unwrap(), b"{invalid");

    fs::remove_file(store.path()).unwrap();
    fs::create_dir(store.path()).unwrap();
    let revision = app.editor.project().revision;
    app.invoke(Request::new(A::SetUiLanguage).argument(Argument::Language(Language::PtBr)))
        .unwrap();
    assert!(
        app.preferences_error
            .as_ref()
            .is_some_and(|error| error.contains("save"))
    );
    assert_eq!(app.localizer.language(), Language::PtBr);
    assert_eq!(app.editor.project().revision, revision);
    assert!(!app.editor.can_undo());
    assert!(!app.editor.is_dirty());
}

#[test]
fn capture_preference_override_never_uses_the_platform_store() {
    let root = TempConfig::new();
    let mut app = DesktopApp {
        capture: Some(capture::Capture::new(
            capture::Config {
                snap: None,
                dialog: None,
                empty_project: false,
                directory: root.0.join("capture"),
                gallery: false,
                size: [1440, 900],
                scale: 130,
                language: Language::PtBr,
                workspace: Workspace::Design,
                welcome_empty: false,
                settings: None,
                page: None,
            },
            Default::default(),
        )),
        ..Default::default()
    };
    app.localizer.set_language(Language::PtBr);
    app.preferences.interface_scale = InterfaceScale::Percent130;
    assert!(app.preferences_store.is_none());
    assert!(!root.0.join("preferences.json").exists());
}

#[test]
fn empty_welcome_capture_uses_only_isolated_recents_and_no_fixture_board() {
    let root = TempConfig::new();
    let config = capture::Config {
        directory: root.0.join("welcome-capture"),
        gallery: false,
        size: [1100, 700],
        scale: 100,
        language: Language::En,
        workspace: Workspace::Design,
        welcome_empty: true,
        settings: None,
        page: None,
        snap: None,
        dialog: None,
        empty_project: false,
    };
    config.prepare_directory().unwrap();
    let mut app = DesktopApp::default();
    app.project_files.user_data_dir = Some(config.directory.join("app-data"));
    app.capture = Some(capture::Capture::new(config, Default::default()));
    assert!(app.project_files.welcome.visible);
    assert!(app.editor.project().boards.is_empty());
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    let output = ctx.run_ui(
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            ..Default::default()
        },
        |ui| app.show_welcome(ui),
    );
    let labels = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        labels.contains(&app.localizer.text("welcome-empty-title")),
        "{labels}"
    );
    assert!(
        labels.contains(&app.localizer.text("welcome-templates")),
        "{labels}"
    );
    assert!(!labels.contains("Redesign reference"));
    output.drop_without_applying_deltas();
    assert!(app.editor.project().boards.is_empty());
    assert!(app.editor.project().export_records.is_empty());
}

#[test]
fn export_completion_distinguishes_edit_revision_from_output_freshness() {
    assert_eq!(export_completion_key(ExportStatus::Current), "export-saved");
    assert_eq!(
        export_completion_key(ExportStatus::WoodStale),
        "export-saved-stale"
    );
    assert_eq!(
        export_completion_key(ExportStatus::PacketStale),
        "export-saved-stale"
    );
    assert_eq!(
        export_completion_key(ExportStatus::Unknown),
        "export-saved-unknown"
    );
    for language in [Language::En, Language::PtBr] {
        let loc = Localizer::new(language);
        let mut args = FluentArgs::new();
        args.set("path", "cabinet.pdf");
        args.set("revision", 7);
        for status in [
            ExportStatus::Current,
            ExportStatus::WoodStale,
            ExportStatus::Unknown,
        ] {
            let label = loc.format(export_completion_key(status), Some(&args));
            assert!(label.contains("cabinet.pdf"), "{label}");
            assert!(!label.contains("export-saved-"), "{label}");
        }
    }
}

fn navigation_app() -> DesktopApp {
    let mut app = DesktopApp {
        editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
        ..Default::default()
    };
    app.sync_scene_inspector();
    app
}

#[test]
fn dimension_field_navigation_rejects_invalid_apply_and_preserves_stay() {
    let mut app = navigation_app();
    let first = app.editor.project().boards[0].id;
    let second = app.editor.project().boards[1].id;
    app.invoke(Request::with(A::EditDimensions, Target::Board(first)))
        .unwrap();
    app.modals.board_dimension_mut().unwrap().value.text = "invalid".into();
    let original = app.editor.project().clone();
    assert_eq!(
        app.request_navigation(NavigationRoute::Entity(Destination::Board(second))),
        Outcome::Prompt {
            kind: EditKind::Field,
            can_commit: false
        }
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Commit),
        Outcome::Blocked(pending_navigation::Blocked::InvalidCommit)
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert_eq!(app.modals.board_dimension().unwrap().value.text, "invalid");
    assert_eq!(app.selection.active, None);
    assert_eq!(app.editor.project(), &original);
    assert!(matches!(
        app.request_navigation(NavigationRoute::Entity(Destination::Board(second))),
        Outcome::Prompt { .. }
    ));
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Abandon),
        Outcome::Navigated
    );
    assert!(app.modals.board_dimension().is_none());
    assert_eq!(app.selection.active, Some(second));
    assert_eq!(app.editor.project(), &original);
}

#[test]
fn dimension_field_apply_routes_once_after_commit() {
    let mut app = navigation_app();
    let first = app.editor.project().boards[0].id;
    let second = app.editor.project().boards[1].id;
    let before = app.editor.project().revision;
    let old = app.editor.project().boards[0].length.micrometres();
    app.invoke(Request::with(A::EditDimensions, Target::Board(first)))
        .unwrap();
    app.modals.board_dimension_mut().unwrap().value.text = format_length(
        Length::from_micrometres(old + 10_000),
        Unit::Mm,
        Locale::En,
        3,
    );
    assert_eq!(
        app.request_navigation(NavigationRoute::Entity(Destination::Board(second))),
        Outcome::Prompt {
            kind: EditKind::Field,
            can_commit: true
        }
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Commit),
        Outcome::Navigated
    );
    assert_eq!(app.editor.project().revision, before + 1);
    assert_eq!(
        app.editor.project().boards[0].length.micrometres(),
        old + 10_000
    );
    assert_eq!(app.selection.active, Some(second));
    assert!(app.modals.board_dimension().is_none());
}

#[test]
fn shared_board_draft_guards_outliner_style_selection_and_applies_once() {
    let mut app = navigation_app();
    let first = app.editor.project().boards[0].id;
    let second = app.editor.project().boards[1].id;
    assert_eq!(
        app.request_scene_selection(Some(first), false),
        Outcome::Navigated
    );
    let original = app.editor.project().boards[0].length;
    let before = app.editor.project().revision;
    app.edit_drafts
        .board(&app.editor, first, Unit::Mm, Locale::En)
        .unwrap()
        .length
        .edit("invalid");
    assert_eq!(
        app.request_scene_selection(Some(second), false),
        Outcome::Prompt {
            kind: EditKind::Field,
            can_commit: false
        }
    );
    assert_eq!(app.selection.active, Some(first));
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Commit),
        Outcome::Blocked(pending_navigation::Blocked::InvalidCommit)
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert_eq!(
        app.edit_drafts
            .existing_board(app.editor.project().id, first)
            .unwrap()
            .length
            .display(),
        "invalid"
    );
    app.edit_drafts
        .board(&app.editor, first, Unit::Mm, Locale::En)
        .unwrap()
        .length
        .edit("500 mm");
    assert_eq!(
        app.request_scene_selection(Some(second), false),
        Outcome::Prompt {
            kind: EditKind::Field,
            can_commit: true
        }
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Commit),
        Outcome::Navigated
    );
    assert_eq!(app.selection.active, Some(second));
    assert_eq!(app.session.inspector, Some(InspectorTarget::Board(second)));
    assert_eq!(app.editor.project().revision, before + 1);
    assert_eq!(app.editor.project().boards[0].length.micrometres(), 500_000);
    assert!(
        app.edit_drafts
            .existing_board(app.editor.project().id, first)
            .is_none()
    );
    app.editor.undo().unwrap();
    assert_eq!(app.editor.project().boards[0].length, original);
}

#[test]
fn shared_draft_guards_deselection_and_discard_clears_it() {
    let mut app = navigation_app();
    let board = app.editor.project().boards[0].id;
    app.request_scene_selection(Some(board), false);
    app.edit_drafts
        .board(&app.editor, board, Unit::Inch, Locale::En)
        .unwrap()
        .length
        .edit("1/64");
    assert_eq!(
        app.request_scene_selection(None, false),
        Outcome::Prompt {
            kind: EditKind::Field,
            can_commit: false
        }
    );
    assert_eq!(app.selection.active, Some(board));
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Abandon),
        Outcome::Navigated
    );
    assert_eq!(app.selection.active, None);
    assert!(
        app.edit_drafts
            .existing_board(app.editor.project().id, board)
            .is_none()
    );
}

#[test]
fn incompatible_action_waits_for_draft_and_stay_keeps_both_unchanged() {
    let mut app = navigation_app();
    let board = app.editor.project().boards[0].id;
    app.request_scene_selection(Some(board), false);
    app.edit_drafts
        .board(&app.editor, board, Unit::Mm, Locale::En)
        .unwrap()
        .length
        .edit("bad");
    let before = app.editor.project().clone();
    let request = Request::with(A::SetGrain, Target::Board(board))
        .argument(Argument::Grain(Some(BoardGrain::Width)));
    app.invoke(request).unwrap();
    assert!(app.navigation.pending().is_some());
    assert_eq!(app.editor.project(), &before);
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert_eq!(app.editor.project(), &before);
    assert_eq!(
        app.edit_drafts
            .existing_board(before.id, board)
            .unwrap()
            .length
            .display(),
        "bad"
    );
    app.invoke(request).unwrap();
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Abandon),
        Outcome::Navigated
    );
    assert_eq!(
        app.editor.project().boards[0].grain_override,
        Some(BoardGrain::Width)
    );
    assert!(app.edit_drafts.existing_board(before.id, board).is_none());
}

#[test]
fn inline_pose_draft_guards_selection_and_frame_changes() {
    let mut app = navigation_app();
    let first = app.editor.project().boards[0].id;
    let second = app.editor.project().boards[1].id;
    app.request_scene_selection(Some(first), false);
    app.edit_drafts
        .pose(
            &app.editor,
            first,
            CoordinateFrame::LocalParent,
            Unit::Mm,
            Locale::En,
        )
        .unwrap()
        .position[0]
        .edit("invalid");
    let original = app.editor.project().clone();
    assert_eq!(
        app.request_scene_selection(Some(second), false),
        Outcome::Prompt {
            kind: EditKind::Preview,
            can_commit: false
        }
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Commit),
        Outcome::Blocked(pending_navigation::Blocked::InvalidCommit)
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert_eq!(app.selection.active, Some(first));
    app.request_pose_frame(CoordinateFrame::World);
    assert_eq!(app.design.pose_frame, CoordinateFrame::LocalParent);
    assert!(app.navigation.pending().is_some());
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Abandon),
        Outcome::Navigated
    );
    assert_eq!(app.design.pose_frame, CoordinateFrame::World);
    assert!(app.edit_drafts.existing_pose(original.id, first).is_none());
    assert_eq!(app.editor.project(), &original);
}

#[test]
fn inline_pose_accepts_one_valid_edit_and_preserves_unedited_rotation() {
    let mut app = navigation_app();
    let first = app.editor.project().boards[0].id;
    let second = app.editor.project().boards[1].id;
    app.request_scene_selection(Some(first), false);
    let original = app.editor.project().boards[0].pose;
    let revision = app.editor.project().revision;
    app.edit_drafts
        .pose(
            &app.editor,
            first,
            CoordinateFrame::LocalParent,
            Unit::Mm,
            Locale::En,
        )
        .unwrap()
        .position[0]
        .edit("30 mm");
    assert_eq!(
        app.request_scene_selection(Some(second), false),
        Outcome::Prompt {
            kind: EditKind::Preview,
            can_commit: true
        }
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Commit),
        Outcome::Navigated
    );
    let pose = app.editor.project().boards[0].pose;
    assert_eq!(pose.translation_mm[0], 30.0);
    assert_eq!(pose.rotation, original.rotation);
    assert_eq!(app.editor.project().revision, revision + 1);
    assert!(
        app.edit_drafts
            .existing_pose(app.editor.project().id, first)
            .is_none()
    );
    app.editor.undo().unwrap();
    assert_eq!(app.editor.project().boards[0].pose, original);
}

#[test]
fn repair_navigation_requires_a_valid_accept_or_explicit_cancel() {
    let mut app = navigation_app();
    let board = app.editor.project().allocations[0].board_id;
    let stock = app.editor.project().allocations[0].stock_id;
    app.session.switch(Workspace::CutPlan);
    app.session.focused_sheet = Some(stock);
    app.selection.choose(Some(board), false);
    let original = app.editor.project().clone();
    assert!(
        app.cut_plan.repair
            .begin(&mut app.editor, &app.selection, Locale::En)
    );
    app.cut_plan.repair.stage_placement(
        &mut app.editor,
        board,
        stock,
        [Length::from_micrometres(999_000_000); 2],
        false,
    );
    assert_eq!(
        app.request_navigation(NavigationRoute::Workspace(Workspace::Design)),
        Outcome::Prompt {
            kind: EditKind::Preview,
            can_commit: false
        }
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Commit),
        Outcome::Blocked(pending_navigation::Blocked::InvalidCommit)
    );
    assert_eq!(app.session.active, Workspace::CutPlan);
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert_eq!(app.session.active, Workspace::CutPlan);
    assert!(app.cut_plan.repair.active());
    assert_eq!(app.editor.project(), &original);
    assert!(matches!(
        app.request_navigation(NavigationRoute::Workspace(Workspace::Design)),
        Outcome::Prompt { .. }
    ));
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Abandon),
        Outcome::Navigated
    );
    assert_eq!(app.editor.project(), &original);
    assert!(app.editor.preview().is_none());
}

#[test]
fn navigation_prompt_footer_blocks_invalid_enter_and_escape_stays() {
    let mut app = navigation_app();
    app.localizer.set_language(Language::En);
    let board = app.editor.project().boards[0].id;
    app.request_scene_selection(Some(board), false);
    app.edit_drafts
        .board(&app.editor, board, Unit::Mm, Locale::En)
        .unwrap()
        .length
        .edit("invalid");
    let before = app.editor.project().clone();
    assert!(matches!(
        app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
        Outcome::Prompt {
            kind: EditKind::Field,
            can_commit: false
        }
    ));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::install_fonts(&ctx);
    let frame = |app: &mut DesktopApp, events| {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.show_navigation_prompt(ui.ctx()),
        );
        let labels = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                (node.role() == egui::accesskit::Role::Button)
                    .then(|| {
                        node.label()
                            .map(|label| (label.to_owned(), node.is_disabled()))
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        output.drop_without_applying_deltas();
        labels
    };
    let labels = frame(&mut app, vec![]);
    assert!(
        labels
            .iter()
            .any(|(label, _)| label == &app.localizer.text("navigation-apply"))
    );
    assert!(
        labels
            .iter()
            .any(|(label, _)| label == &app.localizer.text("navigation-discard"))
    );
    assert!(
        labels
            .iter()
            .any(|(label, _)| label == &app.localizer.text("navigation-stay"))
    );
    frame(
        &mut app,
        vec![Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    // Enter may activate the focused Stay button. It must never apply an
    // invalid draft or reach the requested destination.
    assert_eq!(app.session.active, Workspace::Design);
    assert_eq!(app.editor.project(), &before);
    if app.navigation.pending().is_none() {
        assert!(matches!(
            app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
            Outcome::Prompt { .. }
        ));
        frame(&mut app, vec![]);
    }
    frame(
        &mut app,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(app.navigation.pending().is_none());
    assert_eq!(app.session.active, Workspace::Design);
    assert_eq!(app.editor.project(), &before);
    assert_eq!(
        app.edit_drafts
            .existing_board(before.id, board)
            .unwrap()
            .length
            .display(),
        "invalid"
    );
    assert!(!app.navigation_chrome.is_active());
}

#[test]
fn valid_repair_navigation_accepts_transfer_once_and_undo_restores_allocation() {
    let mut project = plan_my_cabinet::reference_fixture::project();
    let original_allocation = project.allocations[0].clone();
    let mut spare = project
        .stock
        .iter()
        .find(|piece| piece.id == original_allocation.stock_id)
        .unwrap()
        .clone();
    spare.id = Uuid::new_v4();
    spare.name = "Uncut spare".into();
    spare.priority = u32::try_from(project.stock.len()).unwrap();
    let spare_id = spare.id;
    match spare.source {
        plan_my_cabinet::domain::StockSource::Owned => {
            project
                .stock_aliases
                .insert(spare_id, format!("O{}", project.next_stock_o_alias));
            project.next_stock_o_alias += 1;
        }
        plan_my_cabinet::domain::StockSource::ToPurchase => {
            project
                .stock_aliases
                .insert(spare_id, format!("S{}", project.next_stock_s_alias));
            project.next_stock_s_alias += 1;
        }
    }
    project.stock.push(spare);
    let mut app = DesktopApp {
        editor: ProjectEditor::new(project).unwrap(),
        ..Default::default()
    };
    app.session = workspace_state::WorkspaceSession::new(app.editor.project());
    app.session.switch(Workspace::CutPlan);
    app.session.focused_sheet = Some(original_allocation.stock_id);
    app.selection
        .choose(Some(original_allocation.board_id), false);
    let before = app.editor.project().clone();
    assert!(
        app.cut_plan.repair
            .begin(&mut app.editor, &app.selection, Locale::En)
    );
    assert!(app.cut_plan.repair.stage_placement(
        &mut app.editor,
        original_allocation.board_id,
        spare_id,
        original_allocation.origin,
        original_allocation.quarter_turn,
    ));
    assert!(app.cut_plan.repair.can_accept(&mut app.editor));
    assert_eq!(app.editor.project(), &before);
    assert_eq!(
        app.request_navigation(NavigationRoute::Workspace(Workspace::Design)),
        Outcome::Prompt {
            kind: EditKind::Preview,
            can_commit: true,
        }
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Commit),
        Outcome::Navigated
    );
    assert_eq!(app.session.active, Workspace::Design);
    assert!(!app.cut_plan.repair.active());
    assert!(app.editor.preview().is_none());
    assert_eq!(
        app.editor
            .project()
            .allocations
            .iter()
            .find(|a| a.board_id == original_allocation.board_id)
            .unwrap()
            .stock_id,
        spare_id
    );
    assert_eq!(app.editor.project().revision, before.revision + 1);
    app.editor.undo().unwrap();
    assert_eq!(app.editor.project().allocations, before.allocations);
}

#[test]
fn placement_preview_stay_and_cancel_preserve_selection_and_project() {
    let mut app = navigation_app();
    let board = app.editor.project().boards[0].id;
    let sheet = app.editor.project().stock[0].id;
    app.selection.choose(Some(board), false);
    app.modals.set_placement(PlacementDialog::numeric(&app, board));
    let original = app.editor.project().clone();
    assert_eq!(
        app.request_navigation(NavigationRoute::Entity(Destination::Sheet(sheet))),
        Outcome::Prompt {
            kind: EditKind::Preview,
            can_commit: true
        }
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert_eq!(app.session.active, Workspace::Design);
    assert!(app.modals.placement().is_some());
    assert_eq!(app.selection.active, Some(board));
    assert!(matches!(
        app.request_navigation(NavigationRoute::Entity(Destination::Sheet(sheet))),
        Outcome::Prompt { .. }
    ));
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Abandon),
        Outcome::Navigated
    );
    assert_eq!(app.session.active, Workspace::CutPlan);
    assert_eq!(app.session.focused_sheet, Some(sheet));
    assert_eq!(app.selection.active, Some(board));
    assert_eq!(app.editor.project(), &original);
}

#[test]
fn move_preview_accepts_once_and_does_not_implicitly_apply_on_stay() {
    let mut app = navigation_app();
    let board = app.editor.project().boards[0].id;
    app.selection.choose(Some(board), false);
    let before = app.editor.project().revision;
    let mut pose = plan_my_cabinet::placement::world_pose(app.editor.project(), board).unwrap();
    pose.translation_mm[0] += 10.0;
    let mut preview =
        plan_my_cabinet::placement::PlacementSession::begin(&mut app.editor, board).unwrap();
    preview.preview_free(pose).unwrap();
    preview.pause();
    assert_eq!(
        app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
        Outcome::Prompt {
            kind: EditKind::Preview,
            can_commit: true
        }
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert_eq!(app.editor.project().revision, before);
    assert_eq!(app.session.active, Workspace::Design);
    assert!(app.editor.preview().is_some());
    assert!(matches!(
        app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
        Outcome::Prompt { .. }
    ));
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Commit),
        Outcome::Navigated
    );
    assert_eq!(app.editor.project().revision, before + 1);
    assert_eq!(app.session.active, Workspace::Stock);
    assert_eq!(app.selection.active, Some(board));
    assert!(app.editor.preview().is_none());
}

#[test]
fn save_stays_available_during_door_motion_but_project_replacement_waits() {
    let mut app = navigation_app();
    app.hardware.door_motion = Some((Uuid::new_v4(), 30.0));
    assert!(app.door_motion_only());
    assert_eq!(app.action_availability(Request::new(A::SaveProject)), Ok(()));
    assert_eq!(app.action_availability(Request::new(A::SaveProjectAs)), Ok(()));
    assert_eq!(
        app.action_availability(Request::new(A::NewProject)),
        Err(actions::Unavailable::Busy)
    );
    assert!(!app.busy_for_save());
    assert!(app.busy_for_project());
    app.palette.open = true;
    assert!(!app.door_motion_only(), "another surface on top still blocks");
}

#[test]
fn hardware_motion_resets_only_on_successful_navigation() {
    let mut app = navigation_app();
    app.session.switch(Workspace::Hardware);
    app.hardware.door_motion = Some((Uuid::new_v4(), 20.0));
    let missing = Destination::Sheet(Uuid::new_v4());
    assert_eq!(
        app.request_navigation(NavigationRoute::Entity(missing)),
        Outcome::Blocked(pending_navigation::Blocked::MissingDestination)
    );
    assert!(app.hardware.door_motion.is_some());
    assert_eq!(
        app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
        Outcome::Navigated
    );
    assert_eq!(app.hardware.door_motion, None);
}

#[test]
fn workspace_routes_retain_project_selection_filter_scroll_camera_without_edits() {
    let mut app = navigation_app();
    let before = app.editor.project().clone();
    let selected = before.boards[0].id;
    app.selection.choose(Some(selected), false);
    app.session.stock.filter = "oak".into();
    app.session.stock.scroll = 92.0;
    app.session.stock.camera = viewport::Camera::reference_baseline();
    for (workspace, _, _) in workspace_shell::ENTRIES {
        assert_eq!(
            app.request_navigation(NavigationRoute::Workspace(workspace)),
            Outcome::Navigated
        );
        assert_eq!(app.session.active, workspace);
        assert_eq!(app.selection.active, Some(selected));
    }
    assert_eq!(
        app.request_navigation(NavigationRoute::Workspace(Workspace::Stock)),
        Outcome::Navigated
    );
    assert_eq!(app.session.stock.filter, "oak");
    assert_eq!(app.session.stock.scroll, 92.0);
    assert_eq!(app.editor.project(), &before);
}

#[test]
fn collapsing_and_reopening_inspector_retains_invalid_draft_and_view_state() {
    let mut app = navigation_app();
    let board = app.editor.project().boards[0].id;
    app.selection.choose(Some(board), false);
    app.invoke(Request::with(A::EditDimensions, Target::Board(board)))
        .unwrap();
    let draft = app.modals.board_dimension_mut().unwrap();
    draft.value.text = "invalid draft".into();
    draft.value.consent = true;
    app.session.design.scroll = 73.0;
    app.controls_horizontal_scroll[0] = 19.0;
    app.inspector_scroll[0] = egui::vec2(12.0, 84.0);
    let before = app.editor.project().clone();
    for width in [1440.0, 1100.0 / 1.15, 900.0 / 1.30, 1440.0] {
        let layout = workspace_shell::PaneLayout::for_width(
            app.session.active,
            width - workspace_shell::RAIL_WIDTH,
        );
        if layout.collapsed(workspace_shell::Drawer::Inspector) {
            app.open_drawer = Some(workspace_shell::Drawer::Inspector);
        }
        assert_eq!(
            app.modals.board_dimension().unwrap().value.text,
            "invalid draft"
        );
        assert!(app.modals.board_dimension().unwrap().value.consent);
        assert_eq!(app.session.design.scroll, 73.0);
        assert_eq!(app.controls_horizontal_scroll[0], 19.0);
        assert_eq!(app.inspector_scroll[0], egui::vec2(12.0, 84.0));
        assert_eq!(app.selection.active, Some(board));
        assert_eq!(app.editor.project(), &before);
    }
}

#[test]
fn collapsed_inspector_keeps_inline_board_and_pose_drafts() {
    let mut app = navigation_app();
    let board = app.editor.project().boards[0].id;
    app.request_scene_selection(Some(board), false);
    let original = app.editor.project().clone();
    app.edit_drafts
        .board(&app.editor, board, Unit::Inch, Locale::PtBr)
        .unwrap()
        .length
        .edit("1/64");
    app.edit_drafts
        .existing_board_mut(original.id, board)
        .unwrap()
        .length
        .consent = true;
    for width in [1440.0, 900.0 / 1.30, 1440.0] {
        let layout = workspace_shell::PaneLayout::for_width(
            app.session.active,
            width - workspace_shell::RAIL_WIDTH,
        );
        if layout.collapsed(workspace_shell::Drawer::Inspector) {
            app.open_drawer = Some(workspace_shell::Drawer::Inspector);
        } else {
            app.open_drawer = None;
        }
        let draft = app
            .edit_drafts
            .board(&app.editor, board, Unit::Mm, Locale::En)
            .unwrap();
        assert_eq!(draft.length.display(), "1/64");
        assert!(draft.length.consent);
        assert_eq!(draft.values().unwrap()[0], Length::from_micrometres(397));
    }
    assert_eq!(app.editor.project(), &original);
    app.edit_drafts.cancel_board(original.id, board);
    app.edit_drafts
        .pose(
            &app.editor,
            board,
            CoordinateFrame::LocalParent,
            Unit::Mm,
            Locale::En,
        )
        .unwrap()
        .position[0]
        .edit("bad");
    app.open_drawer = Some(workspace_shell::Drawer::Inspector);
    assert!(matches!(
        app.navigation_edit(),
        Some(EditBlock {
            kind: EditKind::Preview,
            can_commit: false,
            ..
        })
    ));
    app.open_drawer = None;
    assert_eq!(
        app.edit_drafts
            .existing_pose(original.id, board)
            .unwrap()
            .position[0]
            .display(),
        "bad"
    );
    assert_eq!(app.editor.project(), &original);
}

#[test]
fn compact_drawers_and_modals_suppress_hud_without_losing_its_draft() {
    let mut app = navigation_app();
    let board = app.editor.project().boards[0].id;
    app.request_scene_selection(Some(board), false);
    let project = app.editor.project().clone();
    app.edit_drafts
        .board(&app.editor, board, Unit::Mm, Locale::En)
        .unwrap()
        .length
        .edit("invalid");
    assert!(app.design_hud_available());
    for width in [900.0 / 1.15, 900.0 / 1.30] {
        let layout = workspace_shell::PaneLayout::for_width(
            Workspace::Design,
            width - workspace_shell::RAIL_WIDTH,
        );
        assert!(layout.collapsed(workspace_shell::Drawer::Inspector));
        for drawer in [
            workspace_shell::Drawer::Inspector,
            workspace_shell::Drawer::Controls,
        ] {
            app.open_drawer = Some(drawer);
            assert!(!app.design_hud_available());
            assert_eq!(
                app.edit_drafts
                    .existing_board(project.id, board)
                    .unwrap()
                    .length
                    .display(),
                "invalid"
            );
        }
    }
    app.open_drawer = None;
    app.palette.open = true;
    assert!(!app.design_hud_available());
    app.palette.open = false;
    assert!(app.design_hud_available());
    assert_eq!(app.editor.project(), &project);
}

// Exercise real egui pointer events against the same shell used by the
// desktop, rather than inferring reachability from a static screenshot.
fn responsive_frame(
    app: &mut DesktopApp,
    ctx: &egui::Context,
    size: egui::Vec2,
    events: Vec<Event>,
) -> Vec<(String, egui::Rect)> {
    let output = ctx.run_ui(
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| app.show_workspace(ui),
    );
    let buttons = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            (node.role() == egui::accesskit::Role::Button && !node.is_disabled())
                .then(|| {
                    let bounds = node.bounds()?;
                    Some((
                        node.label()?.to_owned(),
                        egui::Rect::from_min_max(
                            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                        ),
                    ))
                })
                .flatten()
        })
        .collect();
    output.drop_without_applying_deltas();
    buttons
}

fn responsive_click(
    app: &mut DesktopApp,
    ctx: &egui::Context,
    size: egui::Vec2,
    pos: egui::Pos2,
) {
    responsive_frame(
        app,
        ctx,
        size,
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    responsive_frame(
        app,
        ctx,
        size,
        vec![Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
}

#[test]
fn responsive_shell_drawers_menus_and_invalid_draft_survive_all_scales_and_locales() {
    for window in [
        egui::vec2(1440.0, 900.0),
        egui::vec2(1100.0, 700.0),
        egui::vec2(900.0, 650.0),
    ] {
        for scale in [0.90, 1.00, 1.15, 1.30] {
            if window.x == 1440.0 && scale != 1.0 {
                continue;
            }
            for language in [Language::En, Language::PtBr] {
                let size = window / scale;
                let ctx = egui::Context::default();
                ctx.enable_accesskit();
                let mut app = navigation_app();
                app.localizer.set_language(language);
                let board = app.editor.project().boards[0].id;
                app.request_scene_selection(Some(board), false);
                app.edit_drafts
                    .board(&app.editor, board, Unit::Mm, Locale::En)
                    .unwrap()
                    .length
                    .edit("invalid-at-scale");
                let original = app.editor.project().clone();
                let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
                let header_more = workspace_shell::more_label(language);
                let palette_label = app.localizer.text("palette-title");
                // Search and Save stay directly reachable in the header at every size.
                for label in [palette_label.clone(), A::SaveProject.label(&app.localizer)] {
                    let header = buttons
                        .iter()
                        .find(|(name, rect)| {
                            name == &label && rect.center().y < workspace_shell::HEADER_HEIGHT
                        })
                        .unwrap_or_else(|| {
                            panic!("header {label} missing at {window:?}/{scale} {language:?}: {buttons:?}")
                        });
                    assert!(header.1.right() <= size.x && header.1.left() >= 0.0);
                }
                if let Some(status) = buttons.iter().find(|(label, rect)| {
                    label == header_more && rect.center().y > size.y - workspace_shell::STATUS_HEIGHT
                }) {
                    responsive_click(&mut app, &ctx, size, status.1.center());
                    assert!(egui::Popup::is_any_open(&ctx), "status popup did not open");
                    responsive_frame(
                        &mut app,
                        &ctx,
                        size,
                        vec![Event::Key {
                            key: Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Modifiers::NONE,
                        }],
                    );
                }
                let layout = workspace_shell::PaneLayout::for_width(
                    Workspace::Design,
                    size.x - workspace_shell::RAIL_WIDTH,
                );
                if layout.collapsed(workspace_shell::Drawer::Inspector) {
                    let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
                    let inspector = buttons
                        .iter()
                        .find(|(label, _)| label == workspace_shell::inspector_label(language))
                        .unwrap_or_else(|| panic!("inspector entry missing: {buttons:?}"));
                    responsive_click(&mut app, &ctx, size, inspector.1.center());
                    assert_eq!(app.open_drawer, Some(workspace_shell::Drawer::Inspector));
                    let open = responsive_frame(&mut app, &ctx, size, vec![]);
                    assert!(
                        open.iter().any(
                            |(label, _)| label == &app.localizer.text("navigation-discard")
                        ),
                        "drawer fields not accessible: {open:?}"
                    );
                    responsive_click(&mut app, &ctx, size, inspector.1.center());
                    assert_eq!(app.open_drawer, None);
                }
                if layout.collapsed(workspace_shell::Drawer::Controls) {
                    let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
                    let label = app.localizer.text("navigation-design");
                    let controls = buttons
                        .iter()
                        .find(|(name, rect)| {
                            name == &label && rect.center().y < workspace_shell::HEADER_HEIGHT
                        })
                        .unwrap_or_else(|| {
                            panic!("controls drawer entry missing: {buttons:?}")
                        });
                    responsive_click(&mut app, &ctx, size, controls.1.center());
                    assert_eq!(app.open_drawer, Some(workspace_shell::Drawer::Controls));
                    let open = responsive_frame(&mut app, &ctx, size, vec![]);
                    assert!(
                        open.iter()
                            .any(|(name, _)| name == &app.localizer.text("design-add")),
                        "controls drawer has no creation route: {open:?}"
                    );
                    responsive_click(&mut app, &ctx, size, controls.1.center());
                    assert_eq!(app.open_drawer, None);
                }
                let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
                let viewport_more = app.localizer.text("viewport-more");
                let iso = app.localizer.text("viewport-iso");
                let in_canvas = |rect: &egui::Rect| {
                    rect.center().y > workspace_shell::HEADER_HEIGHT
                        && rect.center().y < size.y - workspace_shell::STATUS_HEIGHT
                };
                if !buttons.iter().any(|(label, rect)| label == &iso && in_canvas(rect)) {
                    let viewport = buttons
                        .iter()
                        .find(|(label, rect)| label == &viewport_more && in_canvas(rect))
                        .unwrap_or_else(|| panic!("viewport camera controls missing: {buttons:?}"));
                    responsive_click(&mut app, &ctx, size, viewport.1.center());
                    let open = responsive_frame(&mut app, &ctx, size, vec![]);
                    assert!(
                        open.iter().any(|(label, _)| label == &iso),
                        "camera presets unreachable: {open:?}"
                    );
                    responsive_frame(
                        &mut app,
                        &ctx,
                        size,
                        vec![Event::Key {
                            key: Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Modifiers::NONE,
                        }],
                    );
                }
                let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
                // The compact HUD now keeps four distinct named icon targets
                // directly in its strip rather than nesting them in More.
                for action in [
                    A::PlaceFace,
                    A::DuplicateBoard,
                    A::ToggleVisibility,
                    A::DeleteObject,
                ] {
                    let label = action.label(&app.localizer);
                    let target = buttons
                        .iter()
                        .find(|(name, rect)| {
                            name == &label
                                && rect.center().y > size.y * 0.55
                                && rect.center().y < size.y - workspace_shell::STATUS_HEIGHT
                        })
                        .unwrap_or_else(|| panic!("HUD action {label} missing: {buttons:?}"));
                    assert!(
                        target.1.right() <= size.x
                            && target.1.bottom() < size.y - workspace_shell::STATUS_HEIGHT
                    );
                }
                assert_eq!(
                    app.edit_drafts
                        .existing_board(original.id, board)
                        .unwrap()
                        .length
                        .display(),
                    "invalid-at-scale"
                );
                assert_eq!(app.editor.project(), &original);
            }
        }
    }
}

#[test]
fn export_preparation_finishes_outside_handoff_without_receipt_or_model_change() {
    let mut app = navigation_app();
    let project = app.editor.project().clone();
    let ctx = egui::Context::default();
    assert_eq!(app.session.active, Workspace::Design);
    for _ in 0..1000 {
        app.tick_export_preparation(&ctx);
        if app.handoff.candidate.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.handoff.candidate.is_some());
    assert!(app.handoff.preparation.is_none());
    assert!(app.handoff.preparation_pending.is_none());
    assert_eq!(app.editor.project(), &project);
    assert!(app.editor.project().export_records.is_empty());
}

fn await_handoff_packet(app: &mut DesktopApp) {
    let ctx = egui::Context::default();
    for _ in 0..2000 {
        app.tick_export_preparation(&ctx);
        if app
            .handoff
            .candidate
            .as_ref()
            .is_some_and(|(_, result)| result.is_ok())
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("reviewed document did not prepare");
}

fn acknowledge_handoff_packet(app: &mut DesktopApp) {
    await_handoff_packet(app);
    let (key, result) = app.handoff.candidate.as_ref().unwrap();
    app.handoff.preparation = Some((key.clone(), Ok(result.as_ref().unwrap().clone())));
}

#[test]
fn overwrite_modal_cancel_and_stale_confirmation_never_drop_a_running_write() {
    let mut app = navigation_app();
    acknowledge_handoff_packet(&mut app);
    let packet = app.current_reviewed_packet().unwrap();
    let before = app.editor.project().clone();
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    let render = |app: &mut DesktopApp, events| {
        ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.show_export_overwrite(ui.ctx()),
        )
        .drop_without_applying_deltas();
    };
    let path = PathBuf::from("overwrite-modal-test.pdf");
    app.handoff.activity = Some(ExportActivity::Confirming(path.clone(), packet.clone()));
    render(&mut app, vec![]);
    assert!(app.handoff.overwrite_chrome.is_active());
    render(
        &mut app,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(app.handoff.activity.is_none());
    assert!(!app.handoff.overwrite_chrome.is_active());
    assert_eq!(app.editor.project(), &before);

    app.handoff.activity = Some(ExportActivity::Confirming(path, packet));
    app.handoff.sections.parts_and_costs = false;
    render(&mut app, vec![]);
    render(
        &mut app,
        vec![Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(app.handoff.activity.is_none());
    assert!(app.handoff.events.is_none());
    assert!(app.current_reviewed_packet().is_none());
    assert_eq!(app.editor.project(), &before);

    app.handoff.activity = Some(ExportActivity::Writing);
    render(&mut app, vec![]);
    assert!(matches!(app.handoff.activity, Some(ExportActivity::Writing)));
    assert!(!app.handoff.overwrite_chrome.is_active());
}

#[test]
fn handoff_issue_fix_pointer_routes_the_actual_unallocated_board_without_editing() {
    let mut app = navigation_app();
    app.session.active = Workspace::Handoff;
    await_handoff_packet(&mut app);
    let before = app.editor.project().clone();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::install_fonts(&ctx);
    let size = egui::vec2(1440.0, 900.0);
    let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
    let button = buttons
        .iter()
        .filter(|(label, rect)| label == "Fix in workspace" && rect.center().y < 800.0)
        .min_by(|a, b| a.1.center().y.total_cmp(&b.1.center().y))
        .unwrap_or_else(|| panic!("Handoff issue fix not reachable: {buttons:?}"));
    responsive_click(&mut app, &ctx, size, button.1.center());
    assert_eq!(app.session.active, Workspace::CutPlan);
    assert_eq!(
        app.session.allocation_issue,
        Some(plan_my_cabinet::reference_fixture::BACK_ID)
    );
    assert_eq!(app.editor.project(), &before);
}

#[test]
fn hardware_installation_route_mounts_projected_guides_only_in_hardware() {
    let mut app = navigation_app();
    let installation = plan_my_cabinet::reference_fixture::HINGE_IDS[0];
    assert!(app.navigate_session(Destination::Installation(installation)));
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    let before = app.editor.project().clone();
    let render = |app: &mut DesktopApp| {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| app.show_workspace(ui),
        );
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        output.drop_without_applying_deltas();
        labels
    };
    let hardware = render(&mut app);
    assert!(
        hardware.contains("H1") && hardware.contains("H2"),
        "{hardware}"
    );
    app.hardware.door_motion = Some((plan_my_cabinet::reference_fixture::LEFT_JOINT_ID, 45.0));
    let _ = render(&mut app); // warm the newly mounted HUD area
    let open = render(&mut app);
    assert!(
        open.contains("45°")
            && open.contains("105°")
            && open.contains("Display only — the saved pose stays closed."),
        "{open}"
    );
    assert!(matches!(
        app.request_navigation(NavigationRoute::Workspace(Workspace::Design)),
        Outcome::Navigated
    ));
    assert!(app.hardware.door_motion.is_none());
    let design = render(&mut app);
    assert!(!design.contains("H1") && !design.contains("H2"), "{design}");
    assert!(!design.contains("Display only — the saved pose stays closed."));
    assert_eq!(app.editor.project(), &before);
}

#[test]
fn reference_cut_plan_host_places_sequence_beside_full_width_sheet() {
    let mut app = navigation_app();
    app.session.switch(Workspace::CutPlan);
    app.session.focused_sheet = Some(plan_my_cabinet::reference_fixture::WHITE_STOCK_ID);
    let before = app.editor.project().clone();
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    let output = ctx.run_ui(
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| app.show_workspace(ui),
    );
    let labels = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.pos)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let sequence = labels
        .iter()
        .find(|(label, _)| {
            label == &app.localizer.text("sheet-sequence-heading").to_uppercase()
        })
        .unwrap_or_else(|| panic!("Cut sequence not in host inspector: {labels:?}"));
    assert!(sequence.1.x > 1100.0, "{sequence:?}");
    let sheet = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.fill == egui::Color32::from_rgb(247, 245, 240)
                    && rect.rect.width() > 500.0 =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .expect("focused sheet surface must be painted");
    assert!(sheet.top() < 320.0, "sheet starts too low: {sheet:?}");
    assert!(
        sheet.bottom() < 860.0,
        "sheet clips under status: {sheet:?}"
    );
    assert!(sheet.height() > 300.0, "sheet is too small: {sheet:?}");
    assert!(labels.iter().any(|(label, pos)| label
        == &app.localizer.text("sheet-needs-stock")
        && pos.x < 340.0));
    assert!(
        labels
            .iter()
            .any(|(label, pos)| label == "C1" && pos.x < 1100.0)
    );
    assert_eq!(app.editor.project(), &before);
    output.drop_without_applying_deltas();
}

#[test]
fn handoff_refresh_requires_explicit_review_after_controls_change() {
    let mut app = navigation_app();
    acknowledge_handoff_packet(&mut app);
    let old = app.current_reviewed_packet().unwrap();
    let page_count = old.document().pages.len();
    app.handoff.preview.set_zoom(5.0).unwrap();
    assert_eq!(old.document().pages.len(), page_count);
    app.handoff.units = Unit::Foot;
    app.tick_export_preparation(&egui::Context::default());
    assert!(app.current_reviewed_packet().is_none());
    await_handoff_packet(&mut app);
    assert!(app.current_reviewed_packet().is_none());
    assert_eq!(
        app.handoff.candidate
            .as_ref()
            .unwrap()
            .1
            .as_ref()
            .unwrap()
            .key()
            .settings
            .units,
        Unit::Foot
    );
    acknowledge_handoff_packet(&mut app);
    assert!(app.current_reviewed_packet().is_some());
    app.handoff.sections.hinge_references = false;
    app.tick_export_preparation(&egui::Context::default());
    assert!(app.current_reviewed_packet().is_none());
    await_handoff_packet(&mut app);
    assert!(app.current_reviewed_packet().is_none());
}

#[test]
fn palette_export_requires_the_exact_reviewed_sections_and_source() {
    let mut app = navigation_app();
    acknowledge_handoff_packet(&mut app);
    assert!(app.action_availability(Request::new(A::ExportPdf)).is_ok());
    let original_sections = app.handoff.sections;
    app.handoff.sections.parts_and_costs = false;
    assert_eq!(
        app.action_availability(Request::new(A::ExportPdf)),
        Err(actions::Unavailable::ExportNotReady)
    );
    assert_eq!(
        app.invoke(Request::new(A::ExportPdf)),
        Err(actions::Unavailable::ExportNotReady)
    );
    assert!(app.handoff.events.is_none());
    assert!(app.handoff.activity.is_none());
    app.handoff.sections = original_sections;
    assert!(app.action_availability(Request::new(A::ExportPdf)).is_ok());
    app.editor
        .set_cutting_kerf(Length::from_micrometres(6_000))
        .unwrap();
    assert_eq!(
        app.action_availability(Request::new(A::ExportPdf)),
        Err(actions::Unavailable::ExportNotReady)
    );
    assert!(app.editor.project().export_records.is_empty());
}

#[test]
fn hidden_wood_issue_blocks_shop_card_even_with_all_optional_sections_off() {
    let mut app = navigation_app();
    let back = plan_my_cabinet::reference_fixture::BACK_ID;
    app.selection.hidden.insert(back);
    app.handoff.sections = ReceiptSections {
        parts_and_costs: false,
        sheets_and_cut_steps: false,
        hinge_references: false,
    };
    await_handoff_packet(&mut app);
    assert!(!app.shop_ready_available());
    assert!(
        app.handoff.candidate
            .as_ref()
            .unwrap()
            .1
            .as_ref()
            .unwrap()
            .wood_issues()
            .iter()
            .any(|issue| matches!(issue, ExportIssue::Board { id, .. } if *id == back))
    );
    assert_eq!(
        app.invoke(
            Request::new(A::SetExportMode)
                .argument(Argument::ExportMode(ExportMode::ShopReady))
        ),
        Err(actions::Unavailable::ExportNotReady)
    );
    assert_eq!(app.handoff.mode, ExportMode::Draft);
    assert!(app.editor.project().export_records.is_empty());
}

#[test]
fn verified_wood_enables_shop_card_without_changing_output_or_interface_language() {
    use plan_my_cabinet::domain::{Allocation, StockGrain};
    let mut project = plan_my_cabinet::reference_fixture::project();
    let stock_id = Uuid::new_v4();
    let mut stock = project.stock[0].clone();
    stock.id = stock_id;
    stock.name = "Backing stock".into();
    stock.material_id = plan_my_cabinet::reference_fixture::HDF_ID;
    stock.length = Length::from_micrometres(1_000_000);
    stock.width = Length::from_micrometres(1_000_000);
    stock.thickness = Length::from_micrometres(3_000);
    stock.grain = StockGrain::Nondirectional;
    stock.priority = 4;
    project.stock.push(stock);
    project.stock_aliases.insert(stock_id, "S4".into());
    project.next_stock_s_alias = 5;
    project.allocations.push(Allocation {
        id: Uuid::new_v4(),
        board_id: plan_my_cabinet::reference_fixture::BACK_ID,
        stock_id,
        origin: [Length::ZERO; 2],
        quarter_turn: false,
        locked: false,
    });
    let mut app = DesktopApp {
        editor: ProjectEditor::new(project).unwrap(),
        ..Default::default()
    };
    await_handoff_packet(&mut app);
    assert!(app.shop_ready_available());
    app.invoke(
        Request::new(A::SetExportMode).argument(Argument::ExportMode(ExportMode::ShopReady)),
    )
    .unwrap();
    app.handoff.language = Language::PtBr;
    app.handoff.units = Unit::Foot;
    await_handoff_packet(&mut app);
    assert!(app.shop_ready_available());
    assert_eq!(app.localizer.language(), Language::En);
    assert_eq!(app.editor.project().display_unit, Unit::Mm);
    assert_eq!(app.handoff.mode, ExportMode::ShopReady);
    assert_eq!(app.handoff.language, Language::PtBr);
    assert_eq!(app.handoff.units, Unit::Foot);
}

#[test]
fn picker_return_with_changed_source_never_writes_or_records_receipt() {
    let mut app = navigation_app();
    acknowledge_handoff_packet(&mut app);
    let (tx, rx) = mpsc::channel();
    app.handoff.events = Some(rx);
    app.handoff.activity = Some(ExportActivity::Choosing(
        Box::new(app.editor.project().clone()),
        app.export_settings(),
        app.handoff.mode,
    ));
    app.handoff.picker_key = Some(app.current_reviewed_packet().unwrap().key().clone());
    let different_kerf =
        Length::from_micrometres(app.editor.project().cutting_kerf.micrometres() + 100);
    app.editor.set_cutting_kerf(different_kerf).unwrap();
    let path = std::env::temp_dir().join(format!("handoff-stale-{}.pdf", Uuid::new_v4()));
    tx.send(ExportEvent::Selected(Some(path.clone()))).unwrap();
    app.poll_pdf_export(&egui::Context::default());
    assert!(app.handoff.activity.is_none());
    assert!(app.handoff.preparation.is_none());
    assert!(!path.exists());
    assert!(app.editor.project().export_records.is_empty());
}

#[test]
fn handoff_export_stays_disabled_until_the_preview_is_marked_reviewed() {
    let mut app = navigation_app();
    app.session.active = Workspace::Handoff;
    await_handoff_packet(&mut app);
    assert!(app.current_reviewed_packet().is_none());
    let before = app.editor.project().clone();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::install_fonts(&ctx);
    let size = egui::vec2(1440.0, 900.0);
    let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
    let export = app.localizer.text("handoff-export-draft");
    assert!(
        !buttons.iter().any(|(label, _)| label == &export),
        "export enabled before review: {buttons:?}"
    );
    let review = app.localizer.text("handoff-mark-reviewed");
    let (_, rect) = buttons
        .iter()
        .find(|(label, _)| label == &review)
        .unwrap_or_else(|| panic!("review step missing: {buttons:?}"))
        .clone();
    responsive_click(&mut app, &ctx, size, rect.center());
    assert!(app.current_reviewed_packet().is_some());
    let buttons = responsive_frame(&mut app, &ctx, size, vec![]);
    assert!(
        buttons.iter().any(|(label, _)| label == &export),
        "export not enabled after review: {buttons:?}"
    );
    assert!(!buttons.iter().any(|(label, _)| label == &review));
    assert_eq!(app.editor.project(), &before);
    assert!(app.handoff.activity.is_none());
}

#[test]
fn cancelled_handoff_picker_keeps_review_but_creates_no_receipt() {
    let mut app = navigation_app();
    acknowledge_handoff_packet(&mut app);
    let (tx, rx) = mpsc::channel();
    app.handoff.events = Some(rx);
    app.handoff.activity = Some(ExportActivity::Choosing(
        Box::new(app.editor.project().clone()),
        app.export_settings(),
        app.handoff.mode,
    ));
    app.handoff.picker_key = Some(app.current_reviewed_packet().unwrap().key().clone());
    tx.send(ExportEvent::Selected(None)).unwrap();
    app.poll_pdf_export(&egui::Context::default());
    assert!(app.handoff.activity.is_none());
    assert!(app.current_reviewed_packet().is_some());
    assert!(app.editor.project().export_records.is_empty());
}

#[test]
fn handoff_write_records_only_the_frozen_reviewed_packet() {
    let mut app = navigation_app();
    app.handoff.sections = ReceiptSections {
        parts_and_costs: false,
        sheets_and_cut_steps: false,
        hinge_references: false,
    };
    acknowledge_handoff_packet(&mut app);
    let packet = app.current_reviewed_packet().unwrap();
    let path = std::env::temp_dir().join(format!("handoff-write-{}.pdf", Uuid::new_v4()));
    app.start_pdf_write(path.clone(), packet.clone(), Overwrite::Decline);
    let ctx = egui::Context::default();
    for _ in 0..5000 {
        app.poll_pdf_export(&ctx);
        if app.handoff.events.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.handoff.events.is_none(), "PDF worker timed out");
    let records = &app.editor.project().export_records;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].metadata.sections, Some(app.handoff.sections));
    assert_eq!(
        records[0].metadata.layout_version,
        Some(packet.key().layout_version as u16)
    );
    assert_eq!(records[0].revision, packet.key().revision);
    assert_eq!(records[0].path, path);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn handoff_overwrite_refusal_and_failed_write_leave_receipts_and_files_untouched() {
    let mut app = navigation_app();
    acknowledge_handoff_packet(&mut app);
    let packet = app.current_reviewed_packet().unwrap();
    let path = std::env::temp_dir().join(format!("handoff-refuse-{}.pdf", Uuid::new_v4()));
    std::fs::write(&path, b"existing file remains").unwrap();
    app.start_pdf_write(path.clone(), packet.clone(), Overwrite::Decline);
    for _ in 0..5000 {
        app.poll_pdf_export(&egui::Context::default());
        if app.handoff.events.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.handoff.events.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), b"existing file remains");
    assert!(app.editor.project().export_records.is_empty());
    std::fs::remove_file(path).unwrap();

    let absent_parent =
        std::env::temp_dir().join(format!("handoff-missing-{}", Uuid::new_v4()));
    let target = absent_parent.join("workshop.pdf");
    app.start_pdf_write(target.clone(), packet, Overwrite::Decline);
    for _ in 0..5000 {
        app.poll_pdf_export(&egui::Context::default());
        if app.handoff.events.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.handoff.events.is_none());
    assert!(!target.exists());
    assert!(app.editor.project().export_records.is_empty());
}

#[test]
fn project_replacement_clears_session_targets_and_preserves_document_identity() {
    let mut app = DesktopApp {
        editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
        ..Default::default()
    };
    app.sync_scene_inspector();
    let before = app.editor.project().clone();
    let id = before.allocations[0].board_id;
    let sheet = before.allocations[0].stock_id;
    assert!(app.navigate_session(Destination::BoardAllocation(id)));
    assert_eq!(app.session.focused_sheet, Some(sheet));
    assert_eq!(app.selection.ids.len(), 1);
    app.session.stock.filter = "kept within project".into();

    app.proceed(project_ui::NextAction::New);
    app.sync_scene_inspector();
    assert_ne!(app.editor.project().id, before.id);
    assert!(app.selection.ids.is_empty());
    assert_eq!(app.session.inspector, None);
    assert_eq!(app.session.focused_sheet, None);
    assert_eq!(app.session.allocation_issue, None);
    assert!(app.session.stock.filter.is_empty());
    assert!(!app.navigate_session(Destination::Sheet(sheet)));
    assert_eq!(before.boards.iter().find(|b| b.id == id).unwrap().id, id);
}

#[test]
fn project_replacement_and_os_close_resolve_unsaved_field_drafts_first() {
    let mut app = navigation_app();
    let project = app.editor.project().id;
    let board = app.editor.project().boards[0].id;
    app.request_scene_selection(Some(board), false);
    app.edit_drafts
        .board(&app.editor, board, Unit::Mm, Locale::En)
        .unwrap()
        .length
        .edit("invalid");
    app.request_project_action(project_ui::NextAction::New);
    assert!(app.navigation.pending().is_some());
    assert_eq!(app.editor.project().id, project);
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert_eq!(app.editor.project().id, project);
    assert_eq!(
        app.edit_drafts
            .existing_board(project, board)
            .unwrap()
            .length
            .display(),
        "invalid"
    );
    app.request_project_action(project_ui::NextAction::Close);
    assert!(app.navigation.pending().is_some());
    assert!(!app.project_files.allow_close);
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Abandon),
        Outcome::Navigated
    );
    assert!(app.project_files.allow_close);
    assert!(app.edit_drafts.existing_board(project, board).is_none());
}

#[test]
fn prepared_open_does_not_replace_document_before_board_edit_resolution() {
    let mut app = navigation_app();
    let original = app.editor.project().id;
    let id = app.editor.project().boards[0].id;
    app.request_scene_selection(Some(id), false);
    app.edit_drafts
        .board(&app.editor, id, Unit::Mm, Locale::En)
        .unwrap()
        .length
        .edit("invalid");
    let replacement = ProjectEditor::new(Project::new("Other", Currency::Brl)).unwrap();
    let next = replacement.project().id;
    app.project_files.pending_open = Some((PathBuf::from("unavailable.pmcab"), replacement));
    app.request_project_action(project_ui::NextAction::Open);
    assert_eq!(app.editor.project().id, original);
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert!(app.project_files.pending_open.is_some());
    app.request_project_action(project_ui::NextAction::Open);
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Abandon),
        Outcome::Navigated
    );
    assert_eq!(app.editor.project().id, next);
    assert!(app.edit_drafts.existing_board(original, id).is_none());
}

#[test]
fn project_action_offers_preview_cancellation_before_replacement() {
    let mut app = navigation_app();
    let original = app.editor.project().id;
    let id = app.editor.project().boards[0].id;
    app.request_scene_selection(Some(id), false);
    app.invoke(Request::with(A::PositionBoard, Target::Board(id)))
        .unwrap();
    assert!(app.modals.placement().is_some());
    app.invoke(Request::new(A::NewProject)).unwrap();
    assert!(app.navigation.pending().is_some());
    assert_eq!(app.editor.project().id, original);
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert!(app.modals.placement().is_some());
    app.invoke(Request::new(A::NewProject)).unwrap();
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Abandon),
        Outcome::Navigated
    );
    assert!(app.modals.placement().is_none());
    assert_ne!(app.editor.project().id, original);
}

#[test]
fn handoff_stock_references_and_kerf_dates_are_truthful_in_both_languages() {
    let mut project = ProjectEditor::new(plan_my_cabinet::reference_fixture::project())
        .unwrap()
        .project()
        .clone();
    let piece = project.stock[0].id;
    let alias = project
        .stock_alias(piece)
        .expect("reference alias")
        .to_owned();
    assert_eq!(
        stock_reference(&project, piece),
        format!("{alias} [{piece}]")
    );
    assert_eq!(
        stock_reference(&project, Uuid::nil()),
        Uuid::nil().to_string()
    );
    let unknown = Localizer::new(Language::En);
    let legacy = kerf_confirmation_label(&project, &unknown).expect("confirmed fixture");
    assert!(legacy.contains("date unavailable"), "{legacy}");
    project.confirmed_shop_kerf_unix_ms = Some(1_780_000_000_000);
    for language in [Language::En, Language::PtBr] {
        let label = kerf_confirmation_label(&project, &Localizer::new(language)).unwrap();
        assert!(label.contains("2026-05-28 UTC"), "{label}");
        assert!(!label.contains("cutting-kerf-"), "{label}");
    }
    project.confirmed_shop_kerf = None;
    assert!(kerf_confirmation_label(&project, &unknown).is_none());
}

mod performance_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/performance_fixture.rs"
    ));
}

use egui::{Event, Key, Modifiers, RawInput};
use plan_my_cabinet::candidate_generation::SearchBudget;
use plan_my_cabinet::candidate_ranking::Objective;
use plan_my_cabinet::domain::{StockGrain, StockSource};
use plan_my_cabinet::optimization_worker::OptimizationWorker;

#[test]
fn large_fixture_workspace_frames_during_worker() {
    use std::time::{Duration, Instant};
    let ctx = egui::Context::default();
    let mut app = DesktopApp {
        editor: ProjectEditor::new(performance_fixture::fixture()).unwrap(),
        ..Default::default()
    };
    let worker = OptimizationWorker::start(
        &app.editor,
        Objective::FewestCuts,
        SearchBudget {
            placements: 10_000,
            witness_states: 20_000,
            beam_width: 8,
        },
        Duration::from_secs(5),
    );
    let start = Instant::now();
    for _ in 0..5 {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1800.0, 1100.0),
                )),
                ..Default::default()
            },
            |ui| app.show_workspace(ui),
        );
        assert!(!output.shapes.is_empty());
        output.drop_without_applying_deltas();
    }
    eprintln!(
        "100-board/10-stock five full egui workspace frames during worker: {:?}",
        start.elapsed()
    );
    assert_eq!(app.editor.project().boards.len(), 100);
    worker.cancel();
    assert!(start.elapsed() < Duration::from_secs(30)); // CI hang guard, not a frame-rate SLA.
}

#[test]
fn dense_sheet_first_and_warm_frames() {
    use std::time::Instant;
    let ctx = egui::Context::default();
    let mut project = performance_fixture::fixture();
    for i in 0..100 {
        project
            .allocations
            .push(plan_my_cabinet::domain::Allocation {
                id: Uuid::from_u128(2000 + i),
                board_id: project.boards[i as usize].id,
                stock_id: project.stock[(i / 10) as usize].id,
                origin: [
                    Length::from_micrometres((i % 10) as i64 * 105_000),
                    Length::ZERO,
                ],
                quarter_turn: false,
                locked: false,
            });
    }
    let mut app = DesktopApp {
        editor: ProjectEditor::new(project).unwrap(),
        ..Default::default()
    };
    let frame = |app: &mut DesktopApp| {
        ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1800., 1100.),
                )),
                ..Default::default()
            },
            |ui| app.show_workspace(ui),
        )
        .drop_without_applying_deltas();
    };
    let first = Instant::now();
    frame(&mut app);
    eprintln!(
        "100 allocated board first workspace frame: {:?}",
        first.elapsed()
    );
    let warm = Instant::now();
    for _ in 0..5 {
        frame(&mut app);
    }
    eprintln!(
        "100 allocated board five unchanged workspace frames: {:?}",
        warm.elapsed()
    );
}

fn repair_frame(
    app: &mut DesktopApp,
    ctx: &egui::Context,
    events: Vec<Event>,
) -> Vec<(String, bool, egui::Pos2)> {
    let output = ctx.run_ui(
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1800.0, 1800.0),
            )),
            events,
            ..Default::default()
        },
        |ui| app.show_workspace(ui),
    );
    let buttons = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            if node.role() != egui::accesskit::Role::Button {
                return None;
            }
            let bounds = node.bounds()?;
            Some((
                node.label()?.to_owned(),
                node.is_disabled(),
                egui::pos2(
                    ((bounds.x0 + bounds.x1) / 2.0) as f32,
                    ((bounds.y0 + bounds.y1) / 2.0) as f32,
                ),
            ))
        })
        .collect();
    output.drop_without_applying_deltas();
    buttons
}

fn click_repair_position(app: &mut DesktopApp, ctx: &egui::Context, pos: egui::Pos2) {
    repair_frame(
        app,
        ctx,
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    repair_frame(
        app,
        ctx,
        vec![Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
}

fn click_repair_button(app: &mut DesktopApp, ctx: &egui::Context, label: &str) {
    let buttons = repair_frame(app, ctx, vec![]);
    let (_, disabled, pos) = buttons
        .iter()
        .find(|(name, _, _)| name == label)
        .unwrap_or_else(|| panic!("missing button {label}: {buttons:?}"));
    assert!(!disabled, "{label} is disabled");
    click_repair_position(app, ctx, *pos);
}

#[test]
fn project_dirty_prompt_cancel_and_discard_are_explicit() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = DesktopApp::default();
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    let old = app.editor.project().id;
    app.request_project_action(project_ui::NextAction::New);
    assert!(app.editor.is_dirty());
    repair_frame(&mut app, &ctx, vec![]);
    let cancel = app.localizer.text("cancel");
    click_repair_button(&mut app, &ctx, &cancel);
    assert_eq!(app.editor.project().id, old);
    app.request_project_action(project_ui::NextAction::New);
    repair_frame(&mut app, &ctx, vec![]);
    let discard = app.localizer.text("project-discard");
    click_repair_button(&mut app, &ctx, &discard);
    assert_ne!(app.editor.project().id, old);
    assert!(!app.editor.can_undo());
}

#[test]
fn desktop_save_open_and_invalid_open_preserve_editor() {
    let root = std::env::temp_dir().join(format!("pmcab-desktop-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("test.pmcab");
    let mut app = DesktopApp::default();
    app.project_files.user_data_dir = Some(root.join("user"));
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    app.save_to(&path, false, None);
    assert!(!app.editor.is_dirty());
    let saved = app.editor.project().clone();
    app.editor
        .set_grid_spacing(Length::from_micrometres(30_000))
        .unwrap();
    app.open_path(root.join("missing.pmcab"));
    assert_eq!(app.editor.project().grid_spacing.micrometres(), 30_000);
    assert!(app.editor.is_dirty());
    app.open_path(path.clone());
    assert!(app.project_files.pending_open.is_some());
    assert!(app.editor.is_dirty());
    app.proceed(project_ui::NextAction::Open);
    assert_eq!(app.editor.project(), &saved);
    assert!(!app.editor.can_undo());
    assert!(!app.editor.is_dirty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_dialog_defer_and_recover_keep_explicit_file_intact() {
    use plan_my_cabinet::recovery::{AUTOSAVE_DELAY, RecoveryStore};
    let root = std::env::temp_dir().join(format!("pmcab-ui-recover-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("test.pmcab");
    let mut app = DesktopApp::default();
    app.project_files.user_data_dir = Some(root.join("user"));
    app.save_to(&path, false, None);
    let saved = std::fs::read(&path).unwrap();
    let mut store =
        RecoveryStore::new(&root.join("user"), &path, app.editor.project().id).unwrap();
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    let now = std::time::Instant::now();
    store.note_committed_edit(&app.editor, now).unwrap();
    store.tick(&app.editor, now + AUTOSAVE_DELAY).unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.open_path(path.clone());
    app.proceed(project_ui::NextAction::Open);
    repair_frame(&mut app, &ctx, vec![]);
    click_repair_button(&mut app, &ctx, "Decide later");
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert!(store.recovery_path().exists());
    app.open_path(path.clone());
    app.proceed(project_ui::NextAction::Open);
    repair_frame(&mut app, &ctx, vec![]);
    click_repair_button(&mut app, &ctx, "Recover unsaved changes");
    assert_eq!(app.editor.project().grid_spacing.micrometres(), 20_000);
    assert!(app.editor.is_dirty());
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn mac_desktop_new_save_as_open_and_dirty_close_preserve_disk_on_refusal() {
    let root = std::env::temp_dir().join(format!("pmcab-mac-files-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let first = root.join("first.pmcab");
    let second = root.join("second.pmcab");
    let mut app = DesktopApp::default();
    app.project_files.user_data_dir = Some(root.join("user"));
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    app.save_to(&first, false, None);
    let first_bytes = std::fs::read(&first).unwrap();
    app.editor
        .set_grid_spacing(Length::from_micrometres(30_000))
        .unwrap();
    std::fs::write(&second, b"existing document").unwrap();
    app.save_to(&second, false, None); // Save As collision: require confirmation.
    assert!(app.project_files.blocking());
    assert_eq!(std::fs::read(&second).unwrap(), b"existing document");
    assert!(app.editor.is_dirty());
    app.project_files.prompt = None; // decline overwrite
    app.save_to(&second, true, None); // explicit replace
    assert!(!app.editor.is_dirty());
    assert_eq!(std::fs::read(&first).unwrap(), first_bytes);
    let second_bytes = std::fs::read(&second).unwrap();
    app.request_project_action(project_ui::NextAction::New);
    assert_ne!(
        app.editor.project().id,
        plan_my_cabinet::persistence::prepare_reader(second_bytes.as_slice())
            .unwrap()
            .project()
            .id
    );
    app.open_path(second.clone());
    assert_eq!(app.project_files.path.as_deref(), Some(second.as_path()));
    app.editor
        .set_grid_spacing(Length::from_micrometres(40_000))
        .unwrap();
    app.request_project_action(project_ui::NextAction::Close);
    assert!(matches!(
        app.project_files.prompt,
        Some(project_ui::Prompt::Dirty(project_ui::NextAction::Close))
    ));
    assert!(!app.project_files.allow_close);
    assert_eq!(std::fs::read(&second).unwrap(), second_bytes);
    app.save_to(&second, true, Some(project_ui::NextAction::Close));
    assert!(app.project_files.allow_close);
    assert_ne!(std::fs::read(&second).unwrap(), second_bytes);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn mac_native_close_request_cancels_until_dirty_choice() {
    let ctx = egui::Context::default();
    let mut app = DesktopApp::default();
    app.editor
        .set_grid_spacing(Length::from_micrometres(20_000))
        .unwrap();
    let mut input = RawInput::default();
    input
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    let output = ctx.run_ui(input, |ui| app.handle_close_request(ui.ctx()));
    assert!(matches!(
        app.project_files.prompt,
        Some(project_ui::Prompt::Dirty(project_ui::NextAction::Close))
    ));
    assert!(!app.project_files.allow_close);
    assert!(output.viewport_output.values().any(|v| {
        v.commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
    }));
    output.drop_without_applying_deltas();
}

#[cfg(target_os = "macos")]
#[test]
fn mac_failed_export_worker_leaves_existing_pdf_and_receipts_intact() {
    let root = std::env::temp_dir().join(format!("pmcab-mac-export-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let existing = root.join("packet.pdf");
    std::fs::write(&existing, b"previous PDF").unwrap();
    let mut app = DesktopApp::default();
    let before = app.editor.project().clone();
    let failure = root.join("missing-parent").join("packet.pdf");
    let packet = Arc::new(
        ReviewedPacket::prepare(
            &before,
            ExportMode::Draft,
            ExportSettings {
                language: Language::En,
                units: Unit::Mm,
            },
            ReceiptSections::default(),
        )
        .unwrap(),
    );
    app.start_pdf_write(failure, packet, Overwrite::Decline);
    let ctx = egui::Context::default();
    for _ in 0..5000 {
        app.poll_pdf_export(&ctx);
        if app.handoff.events.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.handoff.events.is_none(), "export worker timed out");
    assert!(
        app.handoff.message
            .as_ref()
            .is_some_and(|m| m.contains("Could not export PDF"))
    );
    assert_eq!(app.editor.project(), &before);
    assert!(app.editor.project().export_records.is_empty());
    assert_eq!(std::fs::read(existing).unwrap(), b"previous PDF");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn sheet_repair_excludes_other_edits_and_other_modal_excludes_repair() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = DesktopApp::default();
    let material = app
        .editor
        .create_material(NewMaterial {
            name: "wood".into(),
            thickness: Length::from_micrometres(18_000),
            grain: BoardGrain::Length,
        })
        .unwrap();
    let board = app
        .editor
        .create_board(NewBoard {
            name: "part".into(),
            material_id: material,
            length: Length::from_micrometres(100_000),
            width: Length::from_micrometres(50_000),
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        })
        .unwrap();
    app.editor
        .create_stock(
            plan_my_cabinet::stock_commands::StockInput {
                name: "sheet".into(),
                material_id: material,
                length: Length::from_micrometres(300_000),
                width: Length::from_micrometres(200_000),
                thickness: Length::from_micrometres(18_000),
                grain: StockGrain::Nondirectional,
                source: StockSource::Owned,
                price: None,
                trim: [Length::ZERO; 4],
            },
            1,
        )
        .unwrap();
    app.editor
        .set_grid_spacing(Length::from_micrometres(11_000))
        .unwrap();
    app.editor.undo().unwrap();
    assert!(app.editor.can_undo() && app.editor.can_redo());
    app.selection.choose(Some(board), false);
    app.session.switch(Workspace::CutPlan);
    let before = app.editor.project().clone();
    app.modals.set_grid(Some(GridDialog::open(app.editor.project(), Locale::En)));
    let buttons = repair_frame(&mut app, &ctx, vec![]);
    let (_, disabled, position) = buttons
        .iter()
        .find(|(name, _, _)| name == &app.localizer.text("sheet-edit"))
        .unwrap();
    assert!(disabled);
    click_repair_position(&mut app, &ctx, *position);
    assert!(!app.cut_plan.repair.active());
    assert!(app.editor.preview().is_none());
    app.modals.set_grid(None);
    // Retire the modal focus layer before interacting with the workspace.
    repair_frame(&mut app, &ctx, vec![]);
    let edit = app.localizer.text("sheet-edit");
    let cancel = app.localizer.text("sheet-cancel");
    let accept = app.localizer.text("sheet-accept");
    click_repair_button(&mut app, &ctx, &edit);
    assert!(app.cut_plan.repair.active());
    assert!(app.modal_open());
    assert!(!app.other_modal_open());
    let buttons = repair_frame(&mut app, &ctx, vec![]);
    for action in [A::Undo, A::Redo, A::NewStock, A::EditGrid, A::EditKerf] {
        assert!(app.action_availability(Request::new(action)).is_err());
    }
    for key in ["sheet-accept", "sheet-cancel", "sheet-stage"] {
        let label = app.localizer.text(key);
        assert!(
            buttons
                .iter()
                .any(|(name, disabled, _)| name == &label && !*disabled),
            "{key} should be enabled"
        );
    }
    assert!(app.invoke(Request::new(A::EditGrid)).is_err());
    assert_eq!(app.editor.project(), &before);
    assert!(
        app.modals.creation().is_none()
            && app.modals.stock().is_none()
            && app.modals.grid().is_none()
            && app.modals.placement().is_none()
    );
    click_repair_button(&mut app, &ctx, &cancel);
    assert!(!app.cut_plan.repair.active());
    assert!(app.editor.preview().is_none());
    assert_eq!(app.editor.project(), &before);
    click_repair_button(&mut app, &ctx, &edit);
    click_repair_button(&mut app, &ctx, &accept);
    assert!(!app.cut_plan.repair.active());
    assert!(app.editor.preview().is_none());
}

#[test]
fn native_backend_matches_platform() {
    #[cfg(target_os = "macos")]
    assert_eq!(native_backend(), wgpu::Backends::METAL);
    #[cfg(target_os = "linux")]
    assert_eq!(native_backend(), wgpu::Backends::VULKAN);
}

#[test]
fn new_board_text_requires_explicit_rounding_consent() {
    let mut draft = DimensionDraft::new();
    draft.text = "1/64 in".into();
    assert!(draft.value(Unit::Mm).is_err());
    draft.consent = true;
    assert_eq!(draft.value(Unit::Mm), Ok(Length::from_micrometres(397)));
    draft.text = "0 mm".into();
    assert_eq!(
        draft.value(Unit::Mm),
        Err(InputError::Unit(UnitError::NonPositiveDimension))
    );
    draft.text = "NaN".into();
    assert!(draft.value(Unit::Mm).is_err());
}

fn creation_frame(app: &mut DesktopApp, ctx: &egui::Context, key: Option<egui::Key>) {
    ctx.run_ui(
        egui::RawInput {
            events: key
                .into_iter()
                .map(|key| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .collect(),
            ..Default::default()
        },
        |ui| app.show_dialog(ui.ctx()),
    )
    .drop_without_applying_deltas();
}

fn creation_fixture() -> (DesktopApp, Uuid, Uuid) {
    use plan_my_cabinet::domain::{StockGrain, StockSource};
    use plan_my_cabinet::stock_commands::StockInput;
    let mut app = DesktopApp::default();
    let material = app
        .editor
        .create_material(NewMaterial {
            name: "Plywood".into(),
            thickness: Length::from_micrometres(18_000),
            grain: BoardGrain::Length,
        })
        .unwrap();
    let stock = app
        .editor
        .create_stock(
            StockInput {
                name: "One sheet".into(),
                material_id: material,
                length: Length::from_micrometres(100_000),
                width: Length::from_micrometres(50_000),
                thickness: Length::from_micrometres(18_000),
                grain: StockGrain::AlongX,
                source: StockSource::Owned,
                price: None,
                trim: [Length::ZERO; 4],
            },
            1,
        )
        .unwrap()[0];
    (app, material, stock)
}

#[test]
fn creation_preview_repeated_edits_and_cancel_do_not_touch_stock_or_history() {
    let ctx = egui::Context::default();
    let (mut app, material, stock) = creation_fixture();
    let before = app.editor.project().clone();
    let undo = app.editor.can_undo();
    app.modals.set_creation(Some(CreationDialog::board(Some(material))));
    for size in ["100 mm", "75 mm", "120 mm", "100 mm"] {
        let draft = app.modals.creation_mut().unwrap();
        draft.length.text = size.into();
        draft.width.text = "50 mm".into();
        creation_frame(&mut app, &ctx, None);
        let preview = app.modals.creation().unwrap().preview.unwrap().1;
        assert_eq!(
            preview.fit,
            if size == "120 mm" {
                FirstFit::NoFit
            } else {
                FirstFit::Allocated(stock)
            }
        );
        assert_eq!(app.editor.project(), &before);
        assert_eq!(app.editor.can_undo(), undo);
    }
    creation_frame(&mut app, &ctx, Some(egui::Key::Escape));
    assert!(app.modals.creation().is_none());
    assert_eq!(app.editor.project(), &before);
    assert_eq!(app.editor.project().revision, before.revision);
    assert!(app.editor.project().allocations.is_empty());
}

#[test]
fn creation_requires_fresh_rounding_consent_and_popup_keys_stay_inside() {
    let ctx = egui::Context::default();
    let (mut app, material, _) = creation_fixture();
    app.modals.set_creation(Some(CreationDialog::board(Some(material))));
    app.modals.creation_mut().unwrap().length.text = "1/64 in".into();
    app.modals.creation_mut().unwrap().width.text = "50 mm".into();
    creation_frame(&mut app, &ctx, None);
    assert!(
        app.modals
            .creation()
            .unwrap()
            .board_key(app.editor.project())
            .is_none()
    );
    let before = app.editor.project().clone();
    creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
    assert_eq!(app.editor.project(), &before);
    app.modals.creation_mut().unwrap().length.consent = true;
    creation_frame(&mut app, &ctx, None);
    assert_eq!(
        app.modals
            .creation()
            .unwrap()
            .board_key(app.editor.project())
            .unwrap()
            .length,
        Length::from_micrometres(397)
    );
    let popup = egui::Id::new("board-creation-material").with("popup");
    egui::Popup::open_id(&ctx, popup);
    creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
    assert!(app.modals.creation().is_some());
    assert_eq!(app.editor.project(), &before);
    egui::Popup::open_id(&ctx, popup);
    creation_frame(&mut app, &ctx, Some(egui::Key::Escape));
    assert!(app.modals.creation().is_some());
    assert_eq!(app.editor.project(), &before);
    // Editing the proposed quantity invalidates the previous consent.
    app.modals.creation_mut().unwrap().length.text = "3/64 in".into();
    app.modals.creation_mut().unwrap().length.consent = false;
    creation_frame(&mut app, &ctx, None);
    assert!(
        app.modals
            .creation()
            .unwrap()
            .board_key(app.editor.project())
            .is_none()
    );
}

#[test]
fn creation_confirm_uses_current_stock_and_one_undo_for_board_and_color() {
    let ctx = egui::Context::default();
    let (mut app, material, stock) = creation_fixture();
    let mut board = CreationDialog::board(Some(material));
    board.name = "Shelf".into();
    board.length.text = "100 mm".into();
    board.width.text = "50 mm".into();
    board.grain_override = Some(BoardGrain::Unrestricted);
    app.modals.set_creation(Some(board));
    creation_frame(&mut app, &ctx, None);
    assert_eq!(
        app.modals.creation().unwrap().preview.unwrap().1.fit,
        FirstFit::Allocated(stock)
    );
    // An intervening edit consumes that space. The old preview is never an allocation reservation.
    app.editor
        .create_board_with_fit(NewBoard {
            name: "Earlier part".into(),
            material_id: material,
            length: Length::from_micrometres(100_000),
            width: Length::from_micrometres(50_000),
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        })
        .unwrap();
    creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
    assert!(app.modals.creation().is_none());
    assert_eq!(app.design.first_fit_notice, Some(FirstFit::NoFit));
    assert_eq!(app.editor.project().boards.len(), 2);
    assert_eq!(app.editor.project().allocations.len(), 1);
    assert_eq!(
        app.editor.project().boards[1].grain_override,
        Some(BoardGrain::Unrestricted)
    );
    app.editor.undo().unwrap();
    assert_eq!(app.editor.project().boards.len(), 1);

    let mut material_draft = CreationDialog::material();
    material_draft.name = "Oak".into();
    material_draft.thickness.text = "18 mm".into();
    material_draft.color = Some(SrgbColor([226, 197, 156]));
    app.modals.set_creation(Some(material_draft));
    creation_frame(&mut app, &ctx, None);
    creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
    assert!(app.modals.creation().is_none());
    let id = app.editor.project().materials.last().unwrap().id;
    assert_eq!(
        app.editor.project().material_colors.get(&id),
        Some(&SrgbColor([226, 197, 156]))
    );
    app.editor.undo().unwrap();
    assert!(app.editor.project().materials.iter().all(|m| m.id != id));
    assert!(!app.editor.project().material_colors.contains_key(&id));
}

#[test]
fn creation_rejects_a_replaced_scene_without_losing_the_draft() {
    let ctx = egui::Context::default();
    let (mut app, material, _) = creation_fixture();
    let mut board = CreationDialog::board(Some(material));
    board.name = "Pending".into();
    board.length.text = "100 mm".into();
    board.width.text = "50 mm".into();
    app.modals.set_creation(Some(board));
    creation_frame(&mut app, &ctx, None);
    let replacement = Project::new("Different project", Currency::Brl);
    app.editor = ProjectEditor::new(replacement.clone()).unwrap();
    creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
    assert_eq!(app.editor.project(), &replacement);
    assert!(app.modals.creation().is_some());
    assert_eq!(app.modals.creation().unwrap().name, "Pending");
    creation_frame(&mut app, &ctx, Some(egui::Key::Escape));
    assert!(app.modals.creation().is_none());
    assert_eq!(app.editor.project(), &replacement);
}

#[test]
fn creation_out_of_world_bounds_has_no_accepted_preview() {
    let ctx = egui::Context::default();
    let (mut app, material, _) = creation_fixture();
    let before = app.editor.project().clone();
    let mut board = CreationDialog::board(Some(material));
    board.length.text = "1000001 mm".into();
    board.width.text = "50 mm".into();
    app.modals.set_creation(Some(board));
    creation_frame(&mut app, &ctx, None);
    assert!(app.modals.creation().unwrap().preview.is_none());
    creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
    assert!(app.modals.creation().is_some());
    assert_eq!(app.editor.project(), &before);
}

#[test]
fn nested_material_success_selects_new_color_without_resetting_board_fields() {
    let ctx = egui::Context::default();
    let (mut app, original_material, _) = creation_fixture();
    let mut board = CreationDialog::board(Some(original_material));
    board.name = "Pending shelf".into();
    board.length.text = "1/64 in".into();
    board.length.consent = true;
    board.width.text = "50 mm".into();
    board.grain_override = Some(BoardGrain::Unrestricted);
    app.modals.set_creation(Some(board));
    creation_frame(&mut app, &ctx, None);
    app.suspended_board = app.modals.take_creation();
    let mut material = CreationDialog::material();
    material.name = "Walnut".into();
    material.thickness.text = "18 mm".into();
    material.color = Some(SrgbColor([166, 136, 101]));
    app.modals.set_creation(Some(material));
    creation_frame(&mut app, &ctx, None);
    creation_frame(&mut app, &ctx, Some(egui::Key::Enter));
    let new_material = app.editor.project().materials.last().unwrap().id;
    assert_ne!(new_material, original_material);
    assert_eq!(
        app.editor.project().material_colors.get(&new_material),
        Some(&SrgbColor([166, 136, 101]))
    );
    assert!(app.editor.project().boards.is_empty());
    assert!(app.editor.project().allocations.is_empty());
    let restored = app.modals.creation().unwrap();
    assert_eq!(restored.name, "Pending shelf");
    assert_eq!(restored.length.text, "1/64 in");
    assert!(restored.length.consent);
    assert_eq!(restored.width.text, "50 mm");
    assert_eq!(restored.grain_override, Some(BoardGrain::Unrestricted));
    assert_eq!(restored.material_id, Some(new_material));
}

#[test]
fn grid_dialog_focus_invalid_input_and_escape_leave_setting_unchanged() {
    let ctx = egui::Context::default();
    let mut app = DesktopApp::default();
    let initial = app.editor.project().clone();
    app.modals.set_grid(Some(GridDialog::open(app.editor.project(), Locale::En)));
    let draw = |app: &mut DesktopApp, events| {
        ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                app.show_grid_dialog(ui.ctx());
            },
        )
        .drop_without_applying_deltas();
    };
    draw(&mut app, vec![]);
    assert_eq!(app.editor.project(), &initial);
    app.modals.grid_mut().unwrap().value.text = "0 mm".into();
    draw(&mut app, vec![]);
    assert_eq!(app.editor.project(), &initial);
    app.modals.grid_mut().unwrap().value.text = "1/64 in".into();
    draw(&mut app, vec![]);
    assert!(!app.modals.grid().unwrap().value.consent);
    draw(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(app.modals.grid().is_none());
    assert_eq!(app.editor.project(), &initial);
    assert!(!app.editor.can_undo());
}

#[test]
fn numeric_modal_preview_cancel_and_accept_are_single_transaction() {
    let ctx = egui::Context::default();
    let mut app = DesktopApp::default();
    let material_id = app
        .editor
        .create_material(NewMaterial {
            name: "Plywood".into(),
            thickness: Length::from_micrometres(18_000),
            grain: BoardGrain::Length,
        })
        .unwrap();
    let board_id = app
        .editor
        .create_board(NewBoard {
            name: "Side".into(),
            material_id,
            length: Length::from_micrometres(400_000),
            width: Length::from_micrometres(200_000),
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        })
        .unwrap();
    app.selection.choose(Some(board_id), false);
    let initial = app.editor.project().clone();
    let draw = |app: &mut DesktopApp, input| {
        ctx.run_ui(input, |ui| app.show_placement(ui.ctx()))
            .drop_without_applying_deltas();
    };
    app.modals.set_placement(PlacementDialog::numeric(&app, board_id));
    if let placement_ui::PlacementDraft::Numeric { position, .. } =
        &mut app.modals.placement_mut().unwrap().draft
    {
        position[0].text = "25.0004 mm".into();
    }
    draw(&mut app, RawInput::default());
    assert!(app.editor.preview().is_none(), "rounding requires consent");
    if let placement_ui::PlacementDraft::Numeric { position, .. } =
        &mut app.modals.placement_mut().unwrap().draft
    {
        position[0].consent = true;
    }
    draw(&mut app, RawInput::default());
    assert_eq!(app.editor.project(), &initial);
    assert_eq!(
        app.editor.preview().unwrap().boards[0].pose.translation_mm[0],
        25.0
    );
    assert_eq!(app.selection.active, Some(board_id));
    draw(
        &mut app,
        RawInput {
            events: vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        },
    );
    assert!(app.modals.placement().is_none());
    assert_eq!(app.editor.project(), &initial);
    assert!(app.editor.preview().is_none());
    assert_eq!(app.selection.active, Some(board_id));
    app.modals.set_placement(PlacementDialog::numeric(&app, board_id));
    if let placement_ui::PlacementDraft::Numeric { position, .. } =
        &mut app.modals.placement_mut().unwrap().draft
    {
        position[0].text = "25 mm".into();
    }
    draw(&mut app, RawInput::default());
    app.editor.commit_preview().unwrap();
    app.modals.set_placement(None);
    assert_eq!(app.editor.project().revision, initial.revision + 1);
    assert_eq!(app.editor.project().boards[0].pose.translation_mm[0], 25.0);
    app.editor.undo().unwrap();
    assert_eq!(app.editor.project().boards, initial.boards);
}

#[test]
fn numeric_dialog_unedited_frame_switch_and_restored_fields_do_not_commit() {
    let ctx = egui::Context::default();
    let mut app = DesktopApp::default();
    let material_id = app
        .editor
        .create_material(NewMaterial {
            name: "Wood".into(),
            thickness: Length::from_micrometres(18_000),
            grain: BoardGrain::Length,
        })
        .unwrap();
    let board_id = app
        .editor
        .create_board(NewBoard {
            name: "Board".into(),
            material_id,
            length: Length::from_micrometres(100_000),
            width: Length::from_micrometres(50_000),
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        })
        .unwrap();
    let exact = Pose::new(
        [0.0005, -2.0005, 3.0005],
        Quaternion::normalized(0.9, 0.1, 0.2, 0.3).unwrap(),
    )
    .unwrap();
    app.editor
        .transact(|project| {
            project
                .boards
                .iter_mut()
                .find(|board| board.id == board_id)
                .unwrap()
                .pose = exact;
            Ok::<_, ()>(())
        })
        .unwrap();
    let before = app.editor.project().clone();
    app.modals.set_placement(PlacementDialog::numeric(&app, board_id));
    let draw = |app: &mut DesktopApp| {
        ctx.run_ui(RawInput::default(), |ui| app.show_placement(ui.ctx()))
            .drop_without_applying_deltas();
    };
    draw(&mut app);
    assert!(app.editor.preview().is_none());
    if let placement_ui::PlacementDraft::Numeric { frame, .. } =
        &mut app.modals.placement_mut().unwrap().draft
    {
        *frame = plan_my_cabinet::placement::CoordinateFrame::World;
    }
    draw(&mut app);
    assert!(app.editor.preview().is_none());
    if let placement_ui::PlacementDraft::Numeric { position, .. } =
        &mut app.modals.placement_mut().unwrap().draft
    {
        position[0].text = "5 mm".into();
    }
    draw(&mut app);
    assert!(app.editor.preview().is_some());
    if let placement_ui::PlacementDraft::Numeric { position, .. } =
        &mut app.modals.placement_mut().unwrap().draft
    {
        position[0].text = format!(
            "{:.3}",
            plan_my_cabinet::placement::world_pose(&before, board_id)
                .unwrap()
                .translation_mm[0]
        );
    }
    draw(&mut app);
    assert!(app.editor.preview().is_none());
    assert_eq!(app.editor.project(), &before);
    assert_eq!(app.editor.project().revision, before.revision);
}

#[test]
fn face_modal_previews_orientation_and_offset_without_committing_relation() {
    let ctx = egui::Context::default();
    let mut app = DesktopApp::default();
    let material_id = app
        .editor
        .create_material(NewMaterial {
            name: "Wood".into(),
            thickness: Length::from_micrometres(18_000),
            grain: BoardGrain::Length,
        })
        .unwrap();
    let make_board = |app: &mut DesktopApp, name: &str, x| {
        app.editor
            .create_board(NewBoard {
                name: name.into(),
                material_id,
                length: Length::from_micrometres(100_000),
                width: Length::from_micrometres(50_000),
                pose: Pose::new([x, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap()
    };
    let source = make_board(&mut app, "Source", 0.0);
    let target = make_board(&mut app, "Target", 200.0);
    let before = app.editor.project().clone();
    app.modals.set_placement(PlacementDialog::face(&app, source));
    if let placement_ui::PlacementDraft::Face {
        target: chosen,
        source_face,
        target_face,
        offset,
        gap,
        ..
    } = &mut app.modals.placement_mut().unwrap().draft
    {
        *chosen = target;
        *source_face = plan_my_cabinet::placement::BoardFace {
            axis: 0,
            side: plan_my_cabinet::placement::Side::Negative,
        };
        *target_face = plan_my_cabinet::placement::BoardFace {
            axis: 1,
            side: plan_my_cabinet::placement::Side::Positive,
        };
        offset[0].text = "7 mm".into();
        gap.text = "2 mm".into();
    }
    ctx.run_ui(RawInput::default(), |ui| app.show_placement(ui.ctx()))
        .drop_without_applying_deltas();
    let preview = app.editor.preview().unwrap();
    assert_ne!(preview.boards[0].pose, before.boards[0].pose);
    assert_ne!(
        preview.boards[0].pose.rotation,
        before.boards[0].pose.rotation
    );
    assert_eq!(app.editor.project(), &before);
    assert_eq!(
        app.modals.placement().unwrap().highlighted().unwrap().2,
        target
    );
    ctx.run_ui(
        RawInput {
            events: vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| app.show_placement(ui.ctx()),
    )
    .drop_without_applying_deltas();
    assert_eq!(app.editor.project(), &before);
    assert!(app.editor.preview().is_none());
}

#[test]
fn delete_in_text_field_has_no_scene_delete_action_and_modal_blocks_hierarchy_edits() {
    use egui::{Event, Key, Modifiers, RawInput};
    let ctx = egui::Context::default();
    let mut app = DesktopApp::default();
    let material = app
        .editor
        .create_material(NewMaterial {
            name: "wood".into(),
            thickness: Length::from_micrometres(18_000),
            grain: BoardGrain::Length,
        })
        .unwrap();
    let board = app
        .editor
        .create_board(NewBoard {
            name: "part".into(),
            material_id: material,
            length: Length::from_micrometres(100_000),
            width: Length::from_micrometres(50_000),
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        })
        .unwrap();
    app.selection.choose(Some(board), false);
    let before = app.editor.project().clone();
    let mut text = "edit me".to_owned();
    ctx.run_ui(RawInput::default(), |ui| {
        ui.text_edit_singleline(&mut text).request_focus();
        app.show_hierarchy(ui);
        app.show_measurement(ui);
    })
    .drop_without_applying_deltas();
    assert!(ctx.text_edit_focused());
    ctx.run_ui(
        RawInput {
            events: vec![Event::Key {
                key: Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| {
            ui.text_edit_singleline(&mut text);
            app.show_hierarchy(ui);
            app.show_measurement(ui);
        },
    )
    .drop_without_applying_deltas();
    assert_eq!(app.editor.project(), &before);
    assert!(app.selection.ids.contains(&board));
    // There is currently no Delete scene action, including with viewport focus.
    app.modals.set_assembly(Some(assembly_ui::AssemblyDialog::new(
        &app,
        assembly_ui::Operation::Transform,
    )));
    assert!(app.modal_open());
    ctx.run_ui(
        RawInput {
            events: vec![Event::Key {
                key: Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| {
            app.show_hierarchy(ui);
            app.show_measurement(ui);
            app.show_assembly_dialog(ui.ctx());
        },
    )
    .drop_without_applying_deltas();
    assert_eq!(app.editor.project(), &before);
    assert_eq!(app.selection.active, Some(board));
    assert!(app.modal_open());
}

#[test]
fn dialogs_take_and_keep_keyboard_focus_until_closed() {
    fn key(key: Key, modifiers: Modifiers) -> RawInput {
        RawInput {
            events: vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        }
    }

    for kind in [
        "board",
        "material",
        "edit",
        "assign",
        "dimension",
        "batch",
        "numeric",
        "face",
        "stock",
        "currency",
        "fee",
        "grid",
        "kerf",
        "hardware",
        "hinge",
        "relationship",
        "transform",
    ] {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::default();
        let mut background = String::new();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let board_id = app
            .editor
            .create_board(NewBoard {
                name: "Side".into(),
                material_id,
                length: Length::from_micrometres(400_000),
                width: Length::from_micrometres(200_000),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
        if kind == "face" {
            app.editor
                .create_board(NewBoard {
                    name: "Top".into(),
                    material_id,
                    length: Length::from_micrometres(400_000),
                    width: Length::from_micrometres(200_000),
                    pose: Pose::new([500.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
                })
                .unwrap();
        }
        let project = app.editor.project();
        match kind {
            "board" => app.modals.set_creation(Some(CreationDialog::board(Some(material_id)))),
            "material" => app.modals.set_creation(Some(CreationDialog::material())),
            "edit" => {
                app.modals.set_material_edit(Some(MaterialEditDialog {
                    focus_on_open: true,
                    id: material_id,
                    name: "Plywood".into(),
                    thickness: DimensionDraft {
                        text: "18 mm".into(),
                        consent: false,
                    },
                    grain: BoardGrain::Length,
                    anchor: Anchor::Centre,
                    choice: None,
                    error: None,
                }));
            }
            "assign" => {
                app.modals.set_board_material(Some(BoardMaterialDialog {
                    focus_on_open: true,
                    board_id,
                    project_id: project.id,
                    revision: project.revision,
                    material_id: Some(material_id),
                    anchor: Anchor::Centre,
                    error: None,
                }));
            }
            "dimension" => {
                app.modals.set_board_dimension(Some(BoardDimensionDialog {
                    focus_on_open: true,
                    board_id,
                    project_id: project.id,
                    revision: project.revision,
                    dimension: BoardDimension::Length,
                    value: DimensionDraft {
                        text: "400 mm".into(),
                        consent: false,
                    },
                    anchor: Anchor::Centre,
                    error: None,
                }));
            }
            "batch" => {
                app.modals.set_batch_dimension(Some(BatchDialog {
                    focus_on_open: true,
                    project_id: project.id,
                    revision: project.revision,
                    ids: vec![board_id],
                    dimension: BoardDimension::Length,
                    value: DimensionDraft::new(),
                    anchors: vec![(board_id, Anchor::Centre)],
                    error: None,
                }));
            }
            "numeric" => app.modals.set_placement(PlacementDialog::numeric(&app, board_id)),
            "face" => app.modals.set_placement(PlacementDialog::face(&app, board_id)),
            "stock" => app.modals.set_stock(Some(stock_ui::StockDialog::new(project))),
            "currency" => app.modals.set_currency(Some(currency_ui::CurrencyDialog::new(project))),
            "hardware" => {
                app.modals.set_hardware(Some(hardware_ui::HardwareDialog::new(&app, None)))
            }
            "hinge" => app.modals.set_hinge(Some(hinge_ui::HingeDialog::new(&app, None))),
            "relationship" => {
                app.modals.set_door(Some(door_joint_ui::DoorDialog::new(&app, None)))
            }
            "transform" => {
                app.selection.choose(Some(board_id), false);
                app.modals.set_assembly(Some(assembly_ui::AssemblyDialog::new(
                    &app,
                    assembly_ui::Operation::Transform,
                )));
            }
            "fee" | "grid" | "kerf" => {
                app.invoke(Request::new(match kind {
                    "fee" => A::EditCutFee,
                    "grid" => A::EditGrid,
                    _ => A::EditKerf,
                }))
                .unwrap();
            }
            _ => unreachable!(),
        }

        let mut draw = |input| {
            let mut background_id = None;
            ctx.run_ui(input, |ui| {
                background_id = Some(ui.text_edit_singleline(&mut background).id);
                app.show_dialog(ui.ctx());
                app.show_material_edit(ui.ctx());
                app.show_board_material(ui.ctx());
                app.show_board_dimension(ui.ctx());
                app.show_batch_dimension(ui.ctx());
                app.show_placement(ui.ctx());
                app.show_stock_dialog(ui.ctx());
                app.show_currency_dialog(ui.ctx());
                app.show_cut_fee_dialog(ui.ctx());
                app.show_grid_dialog(ui.ctx());
                app.show_hardware_dialog(ui.ctx());
                app.show_hinge_dialog(ui.ctx());
                app.show_door_dialog(ui.ctx());
                app.show_assembly_dialog(ui.ctx());
            })
            .drop_without_applying_deltas();
            (
                background_id.unwrap(),
                ctx.memory(|memory| memory.focused()),
                app.modal_open(),
            )
        };

        let (background_id, initial, _) = draw(RawInput::default());
        assert!(
            initial.is_some() && initial != Some(background_id),
            "{kind}: initial focus"
        );
        let mut moved = false;
        for modifiers in std::iter::repeat_n(Modifiers::NONE, 40)
            .chain(std::iter::repeat_n(Modifiers::SHIFT, 40))
        {
            let (_, focused, _) = draw(key(Key::Tab, modifiers));
            assert!(
                focused.is_some() && focused != Some(background_id),
                "{kind}: tab escaped"
            );
            moved |= focused != initial;
        }
        assert!(moved, "{kind}: focus was reset on every Tab");
        let (_, _, open) = draw(key(Key::Escape, Modifiers::NONE));
        assert!(!open, "{kind}: Escape should close dialog");
        draw(RawInput::default()); // Modal focus layer is retired at the end of this frame.
        let (_, focus, _) = draw(key(Key::Tab, Modifiers::NONE));
        assert_eq!(
            focus,
            Some(background_id),
            "{kind}: background usable after close"
        );
    }
}

#[test]
fn hardware_choosers_close_on_keyboard_selection_without_submitting_parent() {
    for relationship in [false, true] {
        let mut app = navigation_app();
        if relationship {
            app.modals.set_door(Some(door_joint_ui::DoorDialog::new(&app, None)));
        } else {
            app.modals.set_hinge(Some(hinge_ui::HingeDialog::new(&app, None)));
        }
        let before = app.editor.project().clone();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let draw = |app: &mut DesktopApp, key: Option<egui::Key>| {
            let events = key.map_or_else(Vec::new, |key| {
                vec![
                    egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            });
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 875.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.show_hinge_dialog(ui.ctx());
                    app.show_door_dialog(ui.ctx());
                },
            );
            let update = output.platform_output.accesskit_update.as_ref().unwrap();
            let role = update
                .nodes
                .iter()
                .find(|(id, _)| *id == update.focus)
                .map(|(_, node)| node.role());
            output.drop_without_applying_deltas();
            role
        };
        assert_eq!(draw(&mut app, None), Some(egui::accesskit::Role::ComboBox));
        let chooser = ctx.memory(|m| m.focused()).unwrap();
        for _ in 0..2 {
            ctx.memory_mut(|m| m.request_focus(chooser));
            draw(&mut app, Some(egui::Key::Enter));
            assert!(egui::Popup::is_any_open(&ctx));
            draw(&mut app, Some(egui::Key::Tab));
            assert_eq!(
                draw(&mut app, Some(egui::Key::Tab)),
                Some(egui::accesskit::Role::Button)
            );
            draw(&mut app, Some(egui::Key::Enter));
            assert!(
                !egui::Popup::is_any_open(&ctx),
                "relationship={relationship}"
            );
            assert!(app.modals.hinge().is_some() || app.modals.door().is_some());
            assert_eq!(app.editor.project(), &before);
        }
        draw(&mut app, Some(egui::Key::Escape));
        assert!(app.modals.hinge().is_none() && app.modals.door().is_none());
        assert_eq!(app.editor.project(), &before);
    }
}

#[test]
fn project_prompt_blocks_camera_wheel_on_its_first_mounted_frame() {
    for workspace in [Workspace::Design, Workspace::Hardware] {
        let mut app = navigation_app();
        app.session.switch(workspace);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let size = egui::vec2(1440.0, 900.0);
        let wheel = || {
            vec![
                Event::PointerMoved(egui::pos2(740.0, 420.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 100.0),
                    modifiers: Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                },
            ]
        };
        for _ in 0..3 {
            responsive_frame(&mut app, &ctx, size, vec![]);
        }
        let before = app.session.view_mut(workspace).camera.navigation_state();
        responsive_frame(&mut app, &ctx, size, wheel());
        let zoomed = app.session.view_mut(workspace).camera.navigation_state();
        assert_ne!(
            before, zoomed,
            "wheel must reach unblocked {workspace:?} canvas"
        );
        app.project_files.prompt = Some(project_ui::Prompt::Dirty(project_ui::NextAction::New));
        assert!(app.blocking_surface_open());
        let project = app.editor.project().clone();
        responsive_frame(&mut app, &ctx, size, wheel());
        assert_eq!(
            app.session.view_mut(workspace).camera.navigation_state(),
            zoomed
        );
        assert_eq!(app.editor.project(), &project);
        assert!(app.project_files.prompt.is_some());
    }
}

#[test]
fn advanced_board_resize_modal_requires_consent_and_commits_one_undo() {
    for size in [egui::vec2(1440.0, 900.0), egui::vec2(900.0, 650.0)] {
        let mut app = navigation_app();
        let board = app.editor.project().boards[0].id;
        app.invoke(Request::with(A::EditDimensions, Target::Board(board)))
            .unwrap();
        let before = app.editor.project().clone();
        let draft = app.modals.board_dimension_mut().unwrap();
        draft.value.text = "1/64 in".into();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::install_fonts(&ctx);
        let frame = |app: &mut DesktopApp, events| {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..Default::default()
                },
                |ui| app.show_board_dimension(ui.ctx()),
            );
            let buttons = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, node)| {
                    (node.role() == egui::accesskit::Role::Button && !node.is_disabled())
                        .then(|| {
                            let bounds = node.bounds()?;
                            Some((
                                node.label()?.to_owned(),
                                egui::pos2(
                                    (bounds.x0 + bounds.x1) as f32 / 2.0,
                                    (bounds.y0 + bounds.y1) as f32 / 2.0,
                                ),
                            ))
                        })
                        .flatten()
                })
                .collect::<Vec<_>>();
            output.drop_without_applying_deltas();
            buttons
        };
        let invalid = frame(
            &mut app,
            vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(!invalid.iter().any(|(label, _)| label == "Resize board"));
        assert_eq!(app.editor.project(), &before);
        app.modals.board_dimension_mut().unwrap().value.consent = true;
        let ready = frame(&mut app, vec![]);
        let confirm = ready
            .iter()
            .find(|(label, _)| label == "Resize board")
            .unwrap_or_else(|| panic!("resize confirmation not reachable: {ready:?}"))
            .1;
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    Event::PointerMoved(confirm),
                    Event::PointerButton {
                        pos: confirm,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(app.modals.board_dimension().is_none());
        assert_eq!(app.editor.project().boards[0].length.micrometres(), 397);
        assert_eq!(app.editor.project().revision, before.revision + 1);
        assert!(app.editor.undo().unwrap());
        assert_eq!(app.editor.project().boards, before.boards);
    }
}

#[test]
fn board_material_modal_rejects_noop_and_commits_valid_choice_once() {
    let mut app = navigation_app();
    let before = app.editor.project().clone();
    let board_id = before.boards[0].id;
    let alternate = before
        .materials
        .iter()
        .find(|material| material.id != before.boards[0].material_id)
        .unwrap()
        .id;
    app.modals.set_board_material(Some(BoardMaterialDialog {
        focus_on_open: true,
        board_id,
        project_id: before.id,
        revision: before.revision,
        material_id: Some(before.boards[0].material_id),
        anchor: Anchor::Centre,
        error: None,
    }));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::install_fonts(&ctx);
    let frame = |app: &mut DesktopApp, events| {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.show_board_material(ui.ctx()),
        );
        let confirm = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.role() == egui::accesskit::Role::Button
                    && !node.is_disabled()
                    && node.label() == Some("Assign material"))
                .then(|| {
                    let bounds = node.bounds()?;
                    Some(egui::pos2(
                        (bounds.x0 + bounds.x1) as f32 / 2.0,
                        (bounds.y0 + bounds.y1) as f32 / 2.0,
                    ))
                })
                .flatten()
            });
        output.drop_without_applying_deltas();
        confirm
    };
    assert!(frame(&mut app, vec![]).is_none());
    assert_eq!(app.editor.project(), &before);
    app.modals.board_material_mut().unwrap().material_id = Some(alternate);
    let confirm = frame(&mut app, vec![]).expect("valid material confirmation");
    for pressed in [true, false] {
        frame(
            &mut app,
            vec![
                Event::PointerMoved(confirm),
                Event::PointerButton {
                    pos: confirm,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    assert!(app.modals.board_material().is_none());
    assert!(!app.chromes.board_material.is_active());
    assert_eq!(app.editor.project().boards[0].material_id, alternate);
    assert_eq!(app.editor.project().revision, before.revision + 1);
    assert!(app.editor.undo().unwrap());
    assert_eq!(app.editor.project().boards, before.boards);
}

#[test]
fn material_edit_modal_requires_explicit_preserve_or_apply_decision() {
    let mut app = navigation_app();
    let before = app.editor.project().clone();
    let material = before.materials[0].clone();
    app.modals.set_material_edit(Some(MaterialEditDialog {
        focus_on_open: true,
        id: material.id,
        name: material.name.clone(),
        thickness: DimensionDraft {
            text: "21 mm".into(),
            consent: false,
        },
        grain: material.default_grain,
        anchor: Anchor::Centre,
        choice: None,
        error: None,
    }));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::install_fonts(&ctx);
    let frame = |app: &mut DesktopApp, events| {
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.show_material_edit(ui.ctx()),
        );
        let confirm = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.role() == egui::accesskit::Role::Button
                    && !node.is_disabled()
                    && node.label() == Some("Save material"))
                .then(|| {
                    let bounds = node.bounds()?;
                    Some(egui::pos2(
                        (bounds.x0 + bounds.x1) as f32 / 2.0,
                        (bounds.y0 + bounds.y1) as f32 / 2.0,
                    ))
                })
                .flatten()
            });
        output.drop_without_applying_deltas();
        confirm
    };
    assert!(frame(&mut app, vec![]).is_none());
    assert_eq!(app.editor.project(), &before);
    app.modals.material_edit_mut().unwrap().choice = Some(DependantChoice::Preserve);
    let confirm = frame(&mut app, vec![]).expect("choice enables confirmation");
    for pressed in [true, false] {
        frame(
            &mut app,
            vec![
                Event::PointerMoved(confirm),
                Event::PointerButton {
                    pos: confirm,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    assert!(app.modals.material_edit().is_none());
    assert!(!app.chromes.material_edit.is_active());
    assert_eq!(
        app.editor.project().materials[0].default_thickness,
        Length::from_micrometres(21_000)
    );
    assert_eq!(app.editor.project().revision, before.revision + 1);
    assert!(app.editor.undo().unwrap());
    assert_eq!(app.editor.project().materials, before.materials);
    assert_eq!(app.editor.project().boards, before.boards);
}

#[test]
fn cancelling_nested_material_restores_board_name_focus() {
    let ctx = egui::Context::default();
    let mut board = CreationDialog::board(None);
    board.name = "Unfinished shelf".into();
    board.length.text = "1/64 in".into();
    board.length.consent = true;
    board.width.text = "320 mm".into();
    board.grain_override = Some(BoardGrain::Width);
    let mut app = DesktopApp {
        suspended_board: Some(board),
        ..Default::default()
    };
    app.modals.set_creation(Some(CreationDialog::material()));
    let initial = app.editor.project().clone();
    let mut background = String::new();
    let mut draw = |input| {
        let mut background_id = None;
        ctx.run_ui(input, |ui| {
            background_id = Some(ui.text_edit_singleline(&mut background).id);
            app.show_dialog(ui.ctx());
        })
        .drop_without_applying_deltas();
        (
            background_id.unwrap(),
            ctx.memory(|memory| memory.focused()),
            app.modals.creation().map(|dialog| dialog.kind),
        )
    };
    let (background_id, material_focus, _) = draw(RawInput::default());
    let (_, _, kind) = draw(RawInput {
        events: vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..Default::default()
    });
    assert!(kind == Some(DialogKind::Board));
    let (_, board_focus, _) = draw(RawInput::default());
    assert!(board_focus.is_some() && board_focus != Some(background_id));
    assert_ne!(material_focus, board_focus);
    let restored = app.modals.creation().unwrap();
    assert_eq!(restored.name, "Unfinished shelf");
    assert_eq!(restored.length.text, "1/64 in");
    assert!(restored.length.consent);
    assert_eq!(restored.width.text, "320 mm");
    assert_eq!(restored.grain_override, Some(BoardGrain::Width));
    assert_eq!(app.editor.project(), &initial);
}

#[test]
fn keyboard_combo_selection_closes_popup_without_dismissing_dialog() {
    fn draw(
        ui: &mut egui::Ui,
        selected: &mut i32,
        option_response: &mut Option<egui::Response>,
        popup_id: &mut Option<egui::Id>,
        changed: &mut bool,
        cancel: &mut bool,
    ) {
        *cancel = dialog_escape(ui.ctx());
        egui::Window::new("Edit dimensions").show(ui.ctx(), |ui| {
            let response = egui::ComboBox::from_id_salt("axis")
                .selected_text(format!("{selected}"))
                .show_ui(ui, |ui| {
                    for index in 0..2 {
                        let response = combo_option(ui, selected, index, format!("{index}"));
                        if index == 1 {
                            *changed = response.changed();
                            *option_response = Some(response);
                        }
                    }
                });
            *popup_id = Some(response.response.id.with("popup"));
        });
    }
    let ctx = egui::Context::default();
    let mut selected = 0;
    let mut option_response = None;
    let mut popup_id = None;
    let mut changed = false;
    let mut cancel = false;
    ctx.run_ui(RawInput::default(), |ui| {
        draw(
            ui,
            &mut selected,
            &mut option_response,
            &mut popup_id,
            &mut changed,
            &mut cancel,
        )
    })
    .drop_without_applying_deltas();
    egui::Popup::open_id(&ctx, popup_id.expect("combo id"));
    ctx.run_ui(RawInput::default(), |ui| {
        draw(
            ui,
            &mut selected,
            &mut option_response,
            &mut popup_id,
            &mut changed,
            &mut cancel,
        )
    })
    .drop_without_applying_deltas();
    option_response
        .take()
        .expect("popup option")
        .request_focus();

    let input = RawInput {
        events: vec![Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..Default::default()
    };
    ctx.run_ui(input, |ui| {
        draw(
            ui,
            &mut selected,
            &mut option_response,
            &mut popup_id,
            &mut changed,
            &mut cancel,
        )
    })
    .drop_without_applying_deltas();
    assert_eq!(selected, 1);
    assert!(changed);
    assert!(!egui::Popup::is_id_open(&ctx, popup_id.unwrap()));
    assert!(!cancel);

    // Selecting the already-active item should close the menu without reporting a change.
    egui::Popup::open_id(&ctx, popup_id.unwrap());
    ctx.run_ui(RawInput::default(), |ui| {
        draw(
            ui,
            &mut selected,
            &mut option_response,
            &mut popup_id,
            &mut changed,
            &mut cancel,
        )
    })
    .drop_without_applying_deltas();
    option_response
        .take()
        .expect("popup option")
        .request_focus();
    ctx.run_ui(
        RawInput {
            events: vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| {
            draw(
                ui,
                &mut selected,
                &mut option_response,
                &mut popup_id,
                &mut changed,
                &mut cancel,
            )
        },
    )
    .drop_without_applying_deltas();
    assert_eq!(selected, 1);
    assert!(!changed);
    assert!(!egui::Popup::is_id_open(&ctx, popup_id.unwrap()));

    egui::Popup::open_id(&ctx, popup_id.unwrap());
    ctx.run_ui(
        RawInput {
            events: vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| {
            draw(
                ui,
                &mut selected,
                &mut option_response,
                &mut popup_id,
                &mut changed,
                &mut cancel,
            )
        },
    )
    .drop_without_applying_deltas();
    assert!(!cancel);
    assert!(!egui::Popup::is_id_open(&ctx, popup_id.unwrap()));
}
