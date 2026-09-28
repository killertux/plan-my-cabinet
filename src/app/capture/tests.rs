use super::*;

fn parse(args: &[&str]) -> Result<Option<Config>, String> {
    Config::parse(args.iter().map(OsString::from))
}

#[test]
fn opt_in_and_explicit_configuration() {
    assert_eq!(parse(&[]).unwrap(), None);
    let c = parse(&[
        "--capture-baseline",
        "new-output",
        "--capture-size",
        "1100x700",
        "--capture-scale",
        "115",
        "--capture-language",
        "pt-BR",
        "--capture-workspace",
        "stock",
    ])
    .unwrap()
    .unwrap();
    assert_eq!(c.size, [1100, 700]);
    assert_eq!(c.scale, 115);
    assert_eq!(c.language, Language::PtBr);
    assert_eq!(c.workspace, Workspace::Stock);
    assert!(!c.gallery);
    let settings = parse(&[
        "--capture-baseline",
        "new-settings",
        "--capture-size",
        "780x560",
        "--capture-scale",
        "130",
        "--capture-language",
        "pt-BR",
        "--capture-settings",
        "costs",
    ])
    .unwrap()
    .unwrap();
    assert_eq!(settings.settings, Some(SettingsSection::Costs));
    assert_eq!(
        parse(&["--capture-baseline", "out", "--capture-snap", "face"])
            .unwrap()
            .unwrap()
            .snap,
        Some(crate::viewport::CaptureSnap::Face)
    );
    assert_eq!(
        parse(&["--capture-baseline", "out", "--capture-snap", "grid"])
            .unwrap()
            .unwrap()
            .snap,
        Some(crate::viewport::CaptureSnap::Grid)
    );
    assert!(
        parse(&["--capture-gallery", "gallery"])
            .unwrap()
            .unwrap()
            .gallery
    );
    for args in [
        vec!["--capture-baseline"],
        vec!["--capture-baseline", "out", "--capture-size", "0x900"],
        vec!["--capture-baseline", "out", "--capture-scale", "0"],
        vec!["--capture-baseline", "out", "--capture-settings", "unknown"],
        vec!["--capture-baseline", "out", "--capture-page", "0"],
        vec!["--capture-baseline", "out", "--capture-language", "unknown"],
        vec![
            "--capture-baseline",
            "out",
            "--capture-workspace",
            "unknown",
        ],
        vec![
            "--capture-baseline",
            "out",
            "--capture-snap",
            "face",
            "--capture-workspace",
            "stock",
        ],
        vec!["--capture-baseline", "out", "--capture-screen", "welcome"],
        vec!["--capture-gallery", "out", "--capture-snap", "face"],
        vec!["--capture-baseline", "out", "--capture-snap", "unknown"],
        vec![
            "--capture-baseline",
            "out",
            "--capture-snap",
            "grid",
            "--capture-size",
            "900x650",
        ],
        vec![
            "--capture-baseline",
            "out",
            "--capture-scale",
            "100",
            "--capture-scale",
            "115",
        ],
    ] {
        assert!(parse(&args).is_err(), "{args:?}");
    }
}

#[test]
fn empty_welcome_selector_is_isolated_from_fixture_and_other_surfaces() {
    let welcome = parse(&[
        "--capture-baseline",
        "new-welcome-root",
        "--capture-welcome",
        "empty",
        "--capture-size",
        "1100x700",
        "--capture-language",
        "pt-BR",
    ])
    .unwrap()
    .unwrap();
    assert!(welcome.welcome_empty);
    assert_eq!(welcome.size, [1100, 700]);
    assert_eq!(welcome.language, Language::PtBr);
    for extra in [
        vec!["--capture-settings", "general"],
        vec!["--capture-workspace", "design"],
        vec!["--capture-page", "1"],
        vec!["--capture-snap", "face"],
    ] {
        let mut args = vec![
            "--capture-baseline",
            "new-welcome-root",
            "--capture-welcome",
            "empty",
        ];
        args.extend(extra);
        assert!(parse(&args).is_err(), "{args:?}");
    }
    assert!(parse(&["--capture-gallery", "gallery", "--capture-welcome", "empty"]).is_err());
    assert!(parse(&["--capture-baseline", "out", "--capture-welcome", "missing"]).is_err());
}

#[test]
fn reference_dialogs_are_exclusive_nonmutating_real_drafts() {
    for dialog in Dialog::ALL {
        let config = parse(&[
            "--capture-baseline",
            "out",
            "--capture-dialog",
            dialog.name(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(config.dialog, Some(dialog));
        for extra in [
            ["--capture-workspace", "design"],
            ["--capture-settings", "general"],
            ["--capture-welcome", "empty"],
            ["--capture-snap", "face"],
            ["--capture-page", "1"],
        ] {
            let mut args = vec![
                "--capture-baseline",
                "out",
                "--capture-dialog",
                dialog.name(),
            ];
            args.extend(extra);
            assert!(parse(&args).is_err());
        }
        assert!(
            parse(&[
                "--capture-gallery",
                "out",
                "--capture-dialog",
                dialog.name()
            ])
            .is_err()
        );
        let mut app = crate::DesktopApp::default();
        app.editor = ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap();
        app.session = crate::WorkspaceSession::new(app.editor.project());
        let before = app.editor.project().clone();
        dialog.mount(&mut app);
        assert!(dialog == Dialog::Palette || app.modal_open());
        assert_eq!(app.editor.project(), &before);
        assert!(match dialog {
            Dialog::Board | Dialog::Material => app.modals.creation().is_some(),
            Dialog::Position | Dialog::Face => app.modals.placement().is_some(),
            Dialog::Resize => app.modals.batch_dimension().is_some(),
            Dialog::Unsaved => app.project_files.prompt.is_some(),
            Dialog::Palette => app.palette.open,
            Dialog::Catalog => app.modals.catalog().is_some(),
            Dialog::NewProject => app.modals.project_name().is_some(),
        });
    }
    assert!(parse(&["--capture-baseline", "out", "--capture-dialog", "unknown"]).is_err());
}

#[test]
fn screenshot_output_is_exact_and_never_overwrites() {
    let root = std::env::temp_dir().join(format!("pmcab-capture-{}", uuid::Uuid::new_v4()));
    let config = Config {
        directory: root.clone(),
        gallery: false,
        size: [1440, 900],
        scale: 100,
        language: Language::En,
        workspace: Workspace::Design,
        welcome_empty: false,
        settings: None,
        page: None,
        snap: None,
        dialog: None,
        empty_project: false,
    };
    config.prepare_directory().unwrap();
    assert!(config.prepare_directory().is_err());
    let path = root.join("capture.ppm");
    let image = egui::ColorImage::new([2, 1], vec![egui::Color32::RED, egui::Color32::GREEN]);
    write_ppm(&path, &image).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"P6\n2 1\n255\n\xff\0\0\0\xff\0");
    assert!(write_ppm(&path, &image).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn input_cannot_trigger_project_actions() {
    #[derive(Debug)]
    struct UnreadableDrop;
    impl egui::DroppedFile for UnreadableDrop {
        fn path(&self) -> &Path {
            Path::new("/never-open-this-project.pmcab")
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            panic!("capture must never read a dropped file")
        }
    }
    let c = Capture::new(
        parse(&["--capture-baseline", "out"]).unwrap().unwrap(),
        Default::default(),
    );
    let mut input = egui::RawInput::default();
    input
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    input.events.push(egui::Event::Text("save".into()));
    input.dropped_files.push(Arc::new(UnreadableDrop));
    c.filter_input(&mut input);
    assert!(matches!(
        input.events.as_slice(),
        [egui::Event::ModifiersChanged(egui::Modifiers::NONE)]
    ));
    assert!(input.dropped_files.is_empty());
    assert_eq!(input.time, Some(0.0));
}

#[test]
fn manifest_hash_identifies_the_effective_editor_fixture() {
    let capture = Capture::new(
        parse(&["--capture-baseline", "out"]).unwrap().unwrap(),
        Default::default(),
    );
    let fixture = ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap();
    let expected = format!(
        "{:x}",
        Sha256::digest(plan_my_cabinet::persistence::serialize(fixture.project()).unwrap())
    );
    assert_eq!(capture.fixture_sha256, expected);
    assert!(
        fixture
            .project()
            .stock
            .iter()
            .all(|piece| fixture.project().stock_alias(piece.id).is_some())
    );
}

#[test]
fn screenshot_retries_are_bounded_and_never_start_before_settling() {
    assert!((0..12).all(|frame| !screenshot_request_frame(frame)));
    assert_eq!(
        (12..=300)
            .filter(|frame| screenshot_request_frame(*frame))
            .collect::<Vec<_>>(),
        [12, 72, 132, 192, 252]
    );
}
