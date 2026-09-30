use super::*;
use crate::actions::Unavailable;
use crate::workspace_state::{Workspace, WorkspaceSession};

fn app() -> DesktopApp {
    let mut app = DesktopApp {
        editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
        ..Default::default()
    };
    app.session = WorkspaceSession::new(app.editor.project());
    app.session.active = Workspace::Handoff;
    app
}

fn render(app: &mut DesktopApp) -> String {
    let ctx = egui::Context::default();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_export_options(ui);
        app.show_export_footer(ui);
        app.show_file_export_preview(ui);
    });
    let text = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    output.drop_without_applying_deltas();
    text
}

#[test]
fn choosing_cortecloud_shows_its_summary_in_both_languages() {
    for (language, summary, button, left_out) in [
        (
            Language::En,
            "WHAT THE FILE LISTS",
            "Export for CorteCloud…",
            "LEFT OUT",
        ),
        (
            Language::PtBr,
            "O QUE O ARQUIVO LISTA",
            "Exportar para o CorteCloud…",
            "FICOU DE FORA",
        ),
    ] {
        let mut app = app();
        app.localizer = Localizer::new(language);
        let revision = app.editor.project().revision;
        app.invoke(
            Request::new(A::SetExportFormat)
                .argument(Argument::Format(ExportFormat::CorteCloudJson)),
        )
        .unwrap();
        assert_eq!(
            app.editor.project().revision,
            revision,
            "a view choice, not an edit"
        );
        let text = render(&mut app);
        assert!(text.contains(summary), "{text}");
        assert!(text.contains(button), "{text}");
        // The reference plate screws have no pilot size, and one door's
        // hinges have issues: both are listed.
        assert!(text.contains(left_out), "{text}");
        assert!(text.contains("Door left"), "{text}");
        // The PDF's packet options are not shown.
        assert!(
            !text.contains(&app.localizer.text("export-draft")),
            "{text}"
        );
    }
}

#[test]
fn exporting_needs_boards_and_writes_a_receipt_without_an_edit() {
    let mut app = app();
    app.file_export.format = Some(ExportFormat::CorteCloudJson);
    let request =
        Request::new(A::ExportFile).argument(Argument::Format(ExportFormat::CorteCloudJson));
    assert_eq!(app.action_availability(request), Ok(()));
    assert_eq!(
        app.action_availability(
            Request::new(A::ExportFile).argument(Argument::Format(ExportFormat::WorkshopPdf))
        ),
        Err(Unavailable::MissingTarget)
    );
    let dir = std::env::temp_dir().join(format!("pmcab-file-export-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cabinet-cortecloud.json");
    let revision = app.editor.project().revision;
    app.write_file_export(
        ExportFormat::CorteCloudJson,
        path.clone(),
        Overwrite::Decline,
    );
    assert!(path.exists());
    assert_eq!(app.editor.project().revision, revision);
    assert_eq!(app.editor.project().file_exports.len(), 1);
    assert!(
        app.handoff
            .message
            .as_deref()
            .unwrap()
            .contains("cabinet-cortecloud.json")
    );
    let text = render(&mut app);
    assert!(text.contains("matches the design"), "{text}");
    // A second write to the same file asks first.
    app.write_file_export(
        ExportFormat::CorteCloudJson,
        path.clone(),
        Overwrite::Decline,
    );
    assert!(matches!(
        app.file_export.flow,
        Some(FileFlow::Confirming(..))
    ));
    assert!(app.modal_open());
    app.file_export.flow = None;
    std::fs::remove_dir_all(dir).unwrap();

    // An empty design cannot be exported.
    let mut empty = DesktopApp::default();
    empty.file_export.format = Some(ExportFormat::CorteCloudJson);
    assert_eq!(
        empty.action_availability(request),
        Err(Unavailable::ExportNotReady)
    );
}

#[test]
fn a_pilot_size_drills_the_screw_holes_and_clears_the_notice() {
    let mut app = app();
    app.file_export.format = Some(ExportFormat::CorteCloudJson);
    let before = app.file_part_list().unwrap().omissions.len();
    app.file_export.pilot.enabled = true;
    let list = app.file_part_list().unwrap().clone();
    assert!(list.omissions.len() < before);
    assert!(
        list.omissions
            .iter()
            .all(|o| o.reason != OmissionReason::PilotSizeUnknown)
    );
    // An invalid size is not used.
    app.file_export.pilot.diameter.text = "0".into();
    assert!(app.file_export_options().machining.screw_pilot.is_none());
}
