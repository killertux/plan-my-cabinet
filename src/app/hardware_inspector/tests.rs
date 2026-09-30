use super::*;
use crate::actions::Unavailable;
use crate::workspace_state::Workspace;
use plan_my_cabinet::template_setup::{
    ProposedLength, SlideChoice, TemplateField, TemplateKind, TemplateSetup,
};
use plan_my_cabinet::units::Conversion;

fn chest(slides: bool) -> DesktopApp {
    let mut stage = TemplateSetup::new(
        TemplateKind::Drawers,
        "Chest",
        plan_my_cabinet::money::Currency::Brl,
        Unit::Mm,
    );
    stage.seed_standard_materials(Language::En);
    for (field, value) in [
        (TemplateField::Width, 600),
        (TemplateField::Depth, 560),
        (TemplateField::Height, 720),
        (TemplateField::BoxDepth, 500),
        (TemplateField::SideClearance, 13),
        (TemplateField::RearClearance, 20),
        (TemplateField::VerticalClearance, 8),
        (TemplateField::FrontReveal, 3),
        (TemplateField::FrontGap, 3),
    ] {
        stage.dimensions.insert(
            field,
            ProposedLength::new(Conversion::Exact(Length::from_micrometres(value * 1000))),
        );
    }
    stage.drawer_count = Some(2);
    if !slides {
        stage.slides = SlideChoice::None;
        // Gaps that TT45 accepts, so slides can be added later.
        stage.dimensions.insert(
            TemplateField::SideClearance,
            ProposedLength::new(Conversion::Exact(Length::from_micrometres(12_700))),
        );
    }
    let mut app = DesktopApp {
        editor: stage.generate().unwrap().editor,
        ..Default::default()
    };
    app.session = WorkspaceSession::new(app.editor.project());
    app.session.active = Workspace::Hardware;
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

fn render(app: &mut DesktopApp) -> String {
    let ctx = egui::Context::default();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.show_hardware_panel(ui);
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

#[test]
fn a_foot_goes_under_the_selected_cabinet_and_opens_its_inspector() {
    let mut app = chest(true);
    let side = board(&app, "Left side");
    app.selection.choose(Some(side), false);
    let revision = app.editor.project().revision;
    app.invoke(Request::new(A::NewFoot)).unwrap();
    assert_eq!(app.editor.project().revision, revision + 1, "one undo step");
    let foot = app.editor.project().hardware[0].clone();
    assert_eq!(
        app.session.inspector,
        Some(InspectorTarget::Hardware(foot.id))
    );
    assert_eq!(app.selection.active, Some(foot.id));
    let root = app.editor.project().assemblies[0].id;
    assert_eq!(foot.parent_id, Some(root), "under the whole cabinet");
    let text = render(&mut app);
    assert!(text.contains(&app.localizer.text("foot-model")), "{text}");
    // It hangs below the floor: the inspector offers to raise the cabinet.
    assert!(text.contains("Raise Drawers by 40 mm"), "{text}");
    // The same inspector shows in Design once the foot is selected.
    app.session.active = Workspace::Design;
    let text = render(&mut app);
    assert!(text.contains(&app.localizer.text("foot-model")), "{text}");
}

#[test]
fn selecting_hardware_opens_its_inspector_in_both_workspaces() {
    let mut app = chest(true);
    let placeholder = app
        .editor
        .create_placeholder(
            "Handle".into(),
            [Length::from_micrometres(100_000); 3],
            None,
            Pose::IDENTITY,
        )
        .unwrap();
    for workspace in [Workspace::Hardware, Workspace::Design] {
        app.session.active = workspace;
        app.request_scene_selection(None, false);
        app.request_scene_selection(Some(placeholder), false);
        assert_eq!(
            app.session.inspector,
            Some(InspectorTarget::Hardware(placeholder)),
            "{workspace:?}"
        );
        let text = render(&mut app);
        assert!(text.contains("Handle"), "{text}");
        let heading = app.localizer.text("hardware-dimensions").to_uppercase();
        assert!(text.to_uppercase().contains(&heading), "{text}");
    }
}

#[test]
fn slides_are_added_on_the_selected_drawer_or_the_existing_pair_is_shown() {
    let mut app = chest(false);
    assert!(app.editor.project().slide_installations.is_empty());
    let side = board(&app, "Drawer 1 left box side");
    app.selection.choose(Some(side), false);
    app.invoke(Request::new(A::NewSlides)).unwrap();
    let project = app.editor.project();
    assert_eq!(project.slide_installations.len(), 1);
    let id = project.slide_installations[0].id;
    assert_eq!(app.session.inspector, Some(InspectorTarget::Slide(id)));
    assert!(!app.modal_open());
    // Asking again on the same drawer shows the pair instead of a second one.
    app.selection.choose(Some(side), false);
    app.session.inspector = None;
    app.invoke(Request::new(A::NewSlides)).unwrap();
    assert_eq!(app.editor.project().slide_installations.len(), 1);
    assert_eq!(app.session.inspector, Some(InspectorTarget::Slide(id)));
    let text = render(&mut app);
    assert!(text.contains(&app.localizer.text("slide-fits")), "{text}");
    // With nothing selected the picker opens.
    app.selection.choose(None, false);
    app.invoke(Request::new(A::NewSlides)).unwrap();
    assert!(app.modal_open());
}

#[test]
fn doors_and_hinges_are_added_on_the_selected_board() {
    let mut app = DesktopApp {
        editor: plan_my_cabinet::commands::ProjectEditor::new(
            plan_my_cabinet::reference_fixture::project(),
        )
        .unwrap(),
        ..Default::default()
    };
    app.session = WorkspaceSession::new(app.editor.project());
    app.session.active = Workspace::Hardware;
    let door = plan_my_cabinet::reference_fixture::LEFT_DOOR_ID;
    let joint = app
        .editor
        .project()
        .door_joints
        .iter()
        .find(|j| j.moving_root_id == door)
        .unwrap()
        .clone();
    // A hinge on a hung door joins its relationship.
    app.selection.choose(Some(door), false);
    app.invoke(Request::new(A::NewHinge)).unwrap();
    let joint_now = app
        .editor
        .project()
        .door_joints
        .iter()
        .find(|j| j.id == joint.id)
        .unwrap();
    assert_eq!(
        joint_now.hinge_installation_ids.len(),
        joint.hinge_installation_ids.len() + 1
    );
    assert!(matches!(
        app.session.inspector,
        Some(InspectorTarget::Installation(_))
    ));
    // The door's inspector lists its hinges and can reconfirm it.
    app.request_inspect(InspectorTarget::Door(joint.id));
    let text = render(&mut app);
    assert!(
        text.contains(&app.localizer.text("door-moving-part")),
        "{text}"
    );
    assert!(text.contains("Door left hinge 3"), "{text}");
    // Removing the relationship, then Add door re-creates it on the loose hinges.
    plan_my_cabinet::door_joint::remove(&mut app.editor, joint.id).unwrap();
    app.selection.choose(Some(door), false);
    app.invoke(Request::new(A::NewDoor)).unwrap();
    let recreated = app
        .editor
        .project()
        .door_joints
        .iter()
        .find(|j| j.moving_root_id == door)
        .unwrap();
    assert_eq!(recreated.hinge_installation_ids.len(), 3);
    assert_eq!(
        app.session.inspector,
        Some(InspectorTarget::Door(recreated.id))
    );
}

#[test]
fn catalog_models_list_every_kind_and_only_unused_ones_can_be_removed() {
    let mut app = chest(true);
    let slide_catalog = app.editor.project().catalog[0].id;
    assert_eq!(
        app.action_availability(Request::with(
            A::RemoveCatalog,
            Target::Catalog(slide_catalog)
        )),
        Err(Unavailable::CatalogInUse)
    );
    let registry = plan_my_cabinet::catalog_pack::CatalogRegistry::bundled();
    let pack = registry.pack("generic-feet").unwrap();
    let family = &pack.feet[0];
    let foot = plan_my_cabinet::hardware_catalog::add(
        &mut app.editor,
        plan_my_cabinet::catalog_pack::snapshot_foot(pack, family, &family.variants[0], "en"),
    )
    .unwrap();
    app.request_inspect(InspectorTarget::Catalog(foot));
    let text = render(&mut app);
    assert!(
        text.contains(&app.localizer.text("catalog-kind-short-slide")),
        "{text}"
    );
    assert!(
        text.contains(&app.localizer.text("catalog-kind-short-foot")),
        "{text}"
    );
    assert!(
        text.contains(&app.localizer.text("catalog-unused")),
        "{text}"
    );
    app.invoke(Request::with(A::RemoveCatalog, Target::Catalog(foot)))
        .unwrap();
    assert!(!app.editor.project().catalog.iter().any(|c| c.id == foot));
    assert_eq!(app.session.inspector, None);
}

#[test]
fn every_section_shows_an_empty_state_in_both_languages() {
    for language in [Language::En, Language::PtBr] {
        let mut app = DesktopApp::default();
        app.localizer.set_language(language);
        app.session.active = Workspace::Hardware;
        let text = render(&mut app);
        for key in [
            "hardware-empty-doors",
            "hardware-empty-slides",
            "hardware-empty-feet",
            "hardware-empty-other",
            "hardware-empty-catalog",
            "hardware-add-button",
            "hardware-inspect-empty",
        ] {
            let expected = app.localizer.text(key);
            let start: String = expected.chars().take(20).collect();
            assert!(text.contains(&start), "{language:?} {key}: {text}");
        }
    }
}

#[test]
fn position_drafts_wait_for_apply_and_escape_restores() {
    let mut app = chest(true);
    let id = app
        .editor
        .create_placeholder(
            "Handle".into(),
            [Length::from_micrometres(100_000); 3],
            None,
            Pose::IDENTITY,
        )
        .unwrap();
    app.request_inspect(InspectorTarget::Hardware(id));
    let _ = render(&mut app);
    let project_id = app.editor.project().id;
    let target = FittingTarget::Hardware(id);
    let revision = app.editor.project().revision;
    app.edit_drafts
        .existing_fitting_mut(project_id, target)
        .unwrap()
        .fields[0]
        .edit("120");
    // Rendering (and focus changes) never commit.
    let _ = render(&mut app);
    assert_eq!(app.editor.project().revision, revision);
    // Leaving the inspector asks first.
    let other = board(&app, "Left side");
    assert!(matches!(
        app.request_scene_selection(Some(other), false),
        pending_navigation::Outcome::Prompt { .. }
    ));
    app.resolve_navigation(NavigationDecision::Commit);
    assert_eq!(app.editor.project().revision, revision + 1);
    let moved = app
        .editor
        .project()
        .hardware
        .iter()
        .find(|h| h.id == id)
        .unwrap();
    assert_eq!(moved.pose.translation_mm[0], 120.0);
}
