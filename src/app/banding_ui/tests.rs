use super::*;
use crate::actions::Unavailable;
use crate::workspace_state::{Workspace, WorkspaceSession};
use plan_my_cabinet::template_setup::{ProposedLength, TemplateField, TemplateKind, TemplateSetup};
use plan_my_cabinet::units::{Conversion, Length};

fn cabinet() -> DesktopApp {
    let mut stage = TemplateSetup::new(
        TemplateKind::Base,
        "Base",
        plan_my_cabinet::money::Currency::Brl,
        Unit::Mm,
    );
    stage.seed_standard_materials(Language::En);
    for (field, value) in [
        (TemplateField::Width, 600),
        (TemplateField::Depth, 560),
        (TemplateField::Height, 720),
        (TemplateField::RailWidth, 80),
    ] {
        stage.dimensions.insert(
            field,
            ProposedLength::new(Conversion::Exact(Length::from_micrometres(value * 1000))),
        );
    }
    let mut app = DesktopApp {
        editor: stage.generate().unwrap().editor,
        ..Default::default()
    };
    app.session = WorkspaceSession::new(app.editor.project());
    app.session.active = Workspace::Design;
    app
}

fn board(app: &DesktopApp, name: &str) -> Uuid {
    app.editor
        .project()
        .boards
        .iter()
        .find(|b| b.name == name)
        .unwrap()
        .id
}

fn select(app: &mut DesktopApp, ids: &[Uuid]) {
    app.selection.choose(Some(ids[0]), false);
    for id in &ids[1..] {
        app.selection.choose(Some(*id), true);
    }
}

fn render(app: &mut DesktopApp) -> String {
    let ctx = egui::Context::default();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_workspace_inspector(ui);
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

fn bands(app: &DesktopApp, id: Uuid) -> [Option<Uuid>; 4] {
    plan_my_cabinet::banding_rules::bands(
        &plan_my_cabinet::banding_rules::board_states(app.editor.project(), id).unwrap(),
    )
}

#[test]
fn the_banding_section_follows_the_material_in_both_languages() {
    for language in [Language::En, Language::PtBr] {
        let mut app = cabinet();
        app.localizer = Localizer::new(language);
        let side = board(&app, "Left side");
        select(&mut app, &[side]);
        let text = render(&mut app);
        let (title, preset, band) = match language {
            Language::En => ("EDGE BANDING", "Automatic", "White band 1x22"),
            Language::PtBr => ("FITA DE BORDA", "Automática", "White band 1x22"),
        };
        assert!(text.contains(title), "{text}");
        assert!(text.contains(preset), "{text}");
        assert!(text.contains(band), "{text}");
        // The HDF back takes no banding: one line saying so.
        let back = board(&app, "Overlay back");
        select(&mut app, &[back]);
        let text = render(&mut app);
        let refusal = match language {
            Language::En => "no edge banding",
            Language::PtBr => "sem fita de borda",
        };
        assert!(text.contains(refusal), "{text}");
        assert!(!text.contains(preset), "{text}");
    }
}

#[test]
fn toggling_an_edge_is_one_undo_step_and_needs_a_bandable_board() {
    let mut app = cabinet();
    let side = board(&app, "Left side");
    let revision = app.editor.project().revision;
    app.invoke(
        Request::with(A::ToggleBanding, Target::Board(side))
            .argument(Argument::Edge(BoardEdge::MinY)),
    )
    .unwrap();
    assert_eq!(app.editor.project().revision, revision + 1);
    assert_eq!(bands(&app, side)[2], None);
    app.invoke(Request::new(A::Undo)).unwrap();
    assert!(bands(&app, side)[2].is_some());
    let back = board(&app, "Overlay back");
    assert_eq!(
        app.action_availability(
            Request::with(A::ToggleBanding, Target::Board(back))
                .argument(Argument::Edge(BoardEdge::MinY))
        ),
        Err(Unavailable::NoBanding)
    );
}

#[test]
fn presets_apply_to_the_whole_selection_and_report_skipped_boards() {
    let mut app = cabinet();
    let boards = [
        board(&app, "Left side"),
        board(&app, "Bottom"),
        board(&app, "Overlay back"),
    ];
    select(&mut app, &boards);
    let text = render(&mut app);
    assert!(
        text.contains("of 2"),
        "counts over the bandable boards: {text}"
    );
    let revision = app.editor.project().revision;
    app.invoke(
        Request::with(A::ApplyBandingPreset, Target::None)
            .argument(Argument::BandingPreset(BandingPreset::Front)),
    )
    .unwrap();
    assert_eq!(app.editor.project().revision, revision + 1);
    let band = app.editor.project().edge_bands[0].id;
    for id in &boards[..2] {
        assert_eq!(bands(&app, *id), [None, None, Some(band), None]);
    }
    // One board was left alone, and the user is told.
    assert!(
        app.toasts
            .texts()
            .iter()
            .any(|m| m.contains("left as it is"))
    );
    // Back to automatic on one edge of every selected board.
    app.invoke(
        Request::with(A::SetBanding, Target::None).argument(Argument::EdgeSetting {
            edge: BoardEdge::MinX,
            value: EdgeBanding::Auto,
        }),
    )
    .unwrap();
    assert!(
        bands(&app, boards[0])[0].is_some(),
        "the side's bottom edge is free"
    );
    assert_eq!(
        bands(&app, boards[1])[0],
        None,
        "the bottom's end is joined"
    );
}

#[test]
fn bands_open_their_dialog_and_cannot_be_removed_while_used() {
    let mut app = cabinet();
    let band = app.editor.project().edge_bands[0].id;
    assert_eq!(
        app.action_availability(Request::with(A::RemoveEdgeBand, Target::Band(band))),
        Err(Unavailable::BandInUse)
    );
    app.invoke(Request::with(A::EditEdgeBand, Target::Band(band)))
        .unwrap();
    let dialog = app.modals.edge_band().unwrap();
    assert_eq!(dialog.name, "White band 1x22");
    assert_eq!(dialog.height.text, "22");
    app.modals.set_edge_band(None);
    app.invoke(Request::new(A::NewEdgeBand)).unwrap();
    assert!(app.modals.edge_band().is_some_and(|d| d.id.is_none()));
}

#[test]
fn the_band_tool_picks_the_edge_under_the_pointer() {
    let app = cabinet();
    let project = app.editor.project();
    let side = board(&app, "Left side");
    let frame = plan_my_cabinet::board_frame::BoardFrame::new(project, side).unwrap();
    let mut camera = crate::viewport::Camera::reference_baseline();
    camera.set_preset(crate::viewport::Preset::Front);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    camera.viewport_aspect = 800.0 / 600.0;
    camera.viewport_height = 600.0;
    let selection = crate::viewport::Selection::default();
    // Seen from the front, the side's front edge face is what the pointer hits.
    let point = frame.edge_mid(BoardEdge::MinY);
    let screen = camera.project(point, rect).unwrap();
    assert_eq!(
        crate::viewport::pick_edge(project, &camera, screen, rect, &selection),
        Some((side, BoardEdge::MinY))
    );
}
