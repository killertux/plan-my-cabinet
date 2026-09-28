use super::*;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::Project;

fn frame(
    ctx: &egui::Context,
    state: &mut TemplateSetupUi,
    l: &Localizer,
    input: egui::RawInput,
) -> Option<TemplateSetupIntent> {
    let mut intent = None;
    let mut output = ctx.run_ui(input, |_ui| intent = state.show(ctx, l));
    output.textures_delta.clear();
    intent
}

fn screen() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(780.0, 560.0),
        )),
        ..Default::default()
    }
}

fn key(key: egui::Key) -> egui::RawInput {
    let mut input = screen();
    input.events.push(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    input
}

#[test]
fn wizard_body_change_does_not_publish_a_removed_accessibility_focus() {
    let ctx = egui::Context::default();
    let l = Localizer::new(Language::En);
    let mut state = TemplateSetupUi::new(TemplateKind::Drawers, "Cabinet", Currency::Brl, Unit::Mm);
    frame(&ctx, &mut state, &l, screen());
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(egui::Id::new("template-project-name"))
    );
    // Accepting this step removes the focused name field from the next
    // accessibility tree. Native macOS AccessKit otherwise panics.
    frame(&ctx, &mut state, &l, key(egui::Key::Enter));
    assert_eq!(state.stage, Stage::Materials);
    assert_ne!(
        ctx.memory(|m| m.focused()),
        Some(egui::Id::new("template-project-name"))
    );
    let mut output = ctx.run_ui(screen(), |_ui| {
        state.show(&ctx, &l);
    });
    output.textures_delta.clear();
    assert_ne!(
        ctx.memory(|m| m.focused()),
        Some(egui::Id::new("template-project-name"))
    );
}

#[test]
fn first_run_role_assignment_review_and_cancel_leave_existing_editor_untouched() {
    let old = ProjectEditor::new(Project::new("Existing", Currency::Usd)).unwrap();
    let before = old.project().clone();
    let ctx = egui::Context::default();
    let l = Localizer::new(Language::En);
    let mut state = TemplateSetupUi::new(TemplateKind::Drawers, "Kitchen", Currency::Brl, Unit::Mm);
    frame(&ctx, &mut state, &l, screen());
    assert!(state.is_active());
    assert!(
        state
            .review()
            .unwrap_err()
            .contains(&SetupError::MissingRole(MaterialRole::Carcass))
    );
    state.stage = Stage::Materials;
    state.material = Some(MaterialEntry::new(Unit::Mm));
    state.material.as_mut().unwrap().name = "Carcass".into();
    frame(&ctx, &mut state, &l, screen());
    // Cancelling the child drops only its separate draft.
    state.child.close(&ctx);
    state.material = None;
    assert!(state.setup.materials.is_empty());
    let id = state.setup.add_material(
        "Carcass",
        ProposedLength::new(Conversion::Exact(Length::from_micrometres(18_000))),
        BoardGrain::Unrestricted,
        None,
    );
    for role in TemplateKind::Drawers.roles() {
        state.setup.roles.insert(*role, id);
    }
    state.stage = Stage::Review;
    let review = state.review().unwrap();
    assert_eq!(review.candidate.boards.len(), 23);
    assert!(
        review
            .fits
            .iter()
            .all(|(fit, allocation)| *fit == FirstFit::NoFit && allocation.is_none())
    );
    frame(&ctx, &mut state, &l, screen());
    assert_eq!(old.project(), &before);
    assert!(!old.can_undo());
    assert!(state.setup.materials.len() == 1);
    state.close(&ctx);
    assert!(!state.is_active());
    assert_eq!(old.project(), &before);
}

#[test]
fn edited_rounding_resets_consent_and_invalid_geometry_disables_review() {
    let mut state = TemplateSetupUi::new(TemplateKind::Base, "Base", Currency::Brl, Unit::Mm);
    let id = state.setup.add_material(
        "Wood",
        ProposedLength::new(Conversion::Exact(Length::from_micrometres(18_000))),
        BoardGrain::Length,
        None,
    );
    for role in TemplateKind::Base.roles() {
        state.setup.roles.insert(*role, id);
    }
    let entry = state.fields.get_mut(&TemplateField::RailWidth).unwrap();
    entry.text = "1/64 in".into();
    state
        .setup
        .dimensions
        .insert(TemplateField::RailWidth, entry.parse().unwrap());
    assert!(
        state
            .review()
            .unwrap_err()
            .contains(&SetupError::Rounding(TemplateField::RailWidth))
    );
    state
        .setup
        .dimensions
        .get_mut(&TemplateField::RailWidth)
        .unwrap()
        .confirm_rounding();
    assert!(state.review().is_ok());
    let entry = state.fields.get_mut(&TemplateField::RailWidth).unwrap();
    entry.text = "1/32 in".into();
    state
        .setup
        .dimensions
        .insert(TemplateField::RailWidth, entry.parse().unwrap());
    assert!(
        state
            .review()
            .unwrap_err()
            .contains(&SetupError::Rounding(TemplateField::RailWidth))
    );
    state.setup.dimensions.insert(
        TemplateField::Width,
        ProposedLength::new(Conversion::Exact(Length::from_micrometres(10_000))),
    );
    state
        .setup
        .dimensions
        .get_mut(&TemplateField::RailWidth)
        .unwrap()
        .confirm_rounding();
    assert!(
        matches!(state.review(), Err(errors) if errors.iter().any(|e| matches!(e, SetupError::Geometry(_))))
    );
}

#[test]
fn nested_cancel_escape_does_not_cancel_parent() {
    let ctx = egui::Context::default();
    let l = Localizer::new(Language::PtBr);
    let mut state =
        TemplateSetupUi::new(TemplateKind::Wall, "Minha parede", Currency::Brl, Unit::Cm);
    state.stage = Stage::Materials;
    state.material = Some(MaterialEntry::new(Unit::Cm));
    frame(&ctx, &mut state, &l, screen());
    let mut input = screen();
    input.events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    assert!(frame(&ctx, &mut state, &l, input).is_none());
    assert!(state.material.is_none());
    assert!(state.parent.is_active());
    assert_eq!(state.setup.project_name, "Minha parede");
    assert!(state.setup.materials.is_empty());
    // Once the child is gone, a later Escape cancels the setup itself.
    frame(&ctx, &mut state, &l, screen());
    assert!(ctx.memory(|m| m.focused()).is_some());
    assert_ne!(
        ctx.memory(|m| m.focused()),
        Some(egui::Id::new("template-material-name"))
    );
    assert!(matches!(
        frame(&ctx, &mut state, &l, key(egui::Key::Escape)),
        Some(TemplateSetupIntent::Cancel)
    ));
    assert!(!state.is_active());
}

#[test]
fn review_enter_emits_snapshot_without_generating_and_invalid_form_does_not() {
    let ctx = egui::Context::default();
    let l = Localizer::new(Language::En);
    let old = ProjectEditor::new(Project::new("Unsaved", Currency::Usd)).unwrap();
    let before = old.project().clone();
    let mut state = TemplateSetupUi::new(TemplateKind::Base, "New", Currency::Brl, Unit::Mm);
    let id = state.setup.add_material(
        "Panel",
        ProposedLength::new(Conversion::Exact(Length::from_micrometres(18_000))),
        BoardGrain::Length,
        None,
    );
    for role in TemplateKind::Base.roles() {
        state.setup.roles.insert(*role, id);
    }
    state.stage = Stage::Review;
    frame(&ctx, &mut state, &l, screen());
    let Some(TemplateSetupIntent::Accept(snapshot)) =
        frame(&ctx, &mut state, &l, key(egui::Key::Enter))
    else {
        panic!("valid review should emit an acceptance intent");
    };
    assert_eq!(snapshot.project_name, "New");
    assert!(state.is_active()); // host save/replacement can still fail or be cancelled
    assert_eq!(old.project(), &before);
    state.setup.dimensions.insert(
        TemplateField::Width,
        ProposedLength::new(Conversion::Exact(Length::from_micrometres(1_000))),
    );
    frame(&ctx, &mut state, &l, screen());
    assert!(frame(&ctx, &mut state, &l, key(egui::Key::Enter)).is_none());
    assert_eq!(old.project(), &before);
}

#[test]
fn template_setup_fluent_keys_match_in_both_bundles() {
    let keys = |content: &str| -> std::collections::BTreeSet<String> {
        content
            .lines()
            .filter_map(|line| line.split_once(" = ").map(|(key, _)| key))
            .filter(|key| key.starts_with("template-setup-"))
            .map(str::to_owned)
            .collect()
    };
    let en = keys(include_str!("../../../i18n/en.ftl"));
    let pt = keys(include_str!("../../../i18n/pt-BR.ftl"));
    assert_eq!(en, pt);
    assert!(en.len() >= 75);
}
