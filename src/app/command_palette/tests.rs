use super::*;
use crate::workspace_state::InspectorTarget;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::reference_fixture;

fn fixture() -> DesktopApp {
    let mut app = DesktopApp::default();
    app.editor = ProjectEditor::new(reference_fixture::project()).unwrap();
    app.session = crate::workspace_state::WorkspaceSession::new(app.editor.project());
    app
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::RawInput {
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

#[test]
fn groups_localized_case_insensitive_search_and_empty_state() {
    let mut app = fixture();
    let project = app.editor.project();
    for language in [Language::En, Language::PtBr] {
        let localizer = Localizer::new(language);
        assert!(
            results(project, &localizer, "")
                .iter()
                .all(|r| r.group == Group::Actions)
        );
        assert!(results(project, &localizer, "impossible-ZZ987").is_empty());
        let keyword = if language == Language::En {
            "NEW BOARD"
        } else {
            "NOVA CHAPA"
        };
        assert!(
            results(project, &localizer, keyword)
                .iter()
                .any(|r| r.route == ResultRoute::Action(Request::new(A::NewBoard)))
        );
        assert!(
            results(
                project,
                &localizer,
                &project.boards[0].id.to_string().to_uppercase()
            )
            .iter()
            .any(|r| r.route == ResultRoute::Entity(Destination::Board(project.boards[0].id)))
        );
        let stock = &project.stock[0];
        assert!(
            results(project, &localizer, project.stock_alias(stock.id).unwrap())
                .iter()
                .any(|r| r.route == ResultRoute::Entity(Destination::Sheet(stock.id)))
        );
        let installation = &project.hinge_installations[0];
        assert!(
            results(project, &localizer, &installation.id.to_string())
                .iter()
                .any(|r| r.route
                    == ResultRoute::Entity(Destination::Installation(installation.id)))
        );
        let joint = &project.door_joints[0];
        assert!(
            results(project, &localizer, &joint.id.to_string())
                .iter()
                .any(|r| r.route == ResultRoute::Relationship(joint.id))
        );
    }
    app.palette.query.clear();
    let empty = app.palette_results();
    assert!(!empty.is_empty());
    assert!(
        empty
            .iter()
            .all(|r| app.palette_availability(r.route).is_ok())
    );
    app.palette.query = "no-matching-command-487".into();
    assert!(app.palette_results().is_empty());
}

#[test]
fn stock_pieces_offer_duplicate_and_delete_with_in_use_guard() {
    let mut app = fixture();
    let project = app.editor.project();
    let used = project.allocations[0].stock_id;
    let alias = project.stock_alias(used).unwrap().to_owned();
    let unused = project
        .stock
        .iter()
        .find(|s| !project.allocations.iter().any(|a| a.stock_id == s.id))
        .unwrap()
        .id;
    let rows = results(project, &app.localizer, &alias);
    for action in [A::DuplicateStock, A::DeleteStock] {
        assert!(rows.iter().any(|r| r.route
            == ResultRoute::Action(Request::with(action, Target::Stock(used)))));
    }
    let by_command = results(project, &app.localizer, "delete");
    assert!(by_command.iter().any(|r| r.route
        == ResultRoute::Action(Request::with(A::DeleteStock, Target::Stock(unused)))));
    // Deleting a piece that still holds parts is refused with a reason.
    let before = app.editor.project().clone();
    let delete_used = rows
        .iter()
        .find(|r| r.route == ResultRoute::Action(Request::with(A::DeleteStock, Target::Stock(used))))
        .unwrap()
        .clone();
    assert!(app.invoke_palette(&delete_used).is_err());
    assert_eq!(app.editor.project(), &before);
    let count = before.stock.len();
    let duplicate = ResultRow {
        group: Group::Actions,
        label: String::new(),
        detail: String::new(),
        route: ResultRoute::Action(Request::with(A::DuplicateStock, Target::Stock(used))),
    };
    app.invoke_palette(&duplicate).unwrap();
    assert_eq!(app.editor.project().stock.len(), count + 1);
    let delete = ResultRow {
        route: ResultRoute::Action(Request::with(A::DeleteStock, Target::Stock(unused))),
        ..duplicate
    };
    app.invoke_palette(&delete).unwrap();
    assert_eq!(app.editor.project().stock.len(), count);
    assert!(!app.editor.project().stock.iter().any(|s| s.id == unused));
    app.editor.undo().unwrap();
    assert!(app.editor.project().stock.iter().any(|s| s.id == unused));
}

#[test]
fn equal_names_route_only_the_selected_uuid_and_removed_targets_fail() {
    let mut app = fixture();
    let first = app.editor.project().boards[0].id;
    let second = app.editor.project().boards[1].id;
    let name = app.editor.project().boards[0].name.clone();
    let mut project = app.editor.project().clone();
    project.boards[1].name = name.clone();
    app.editor = ProjectEditor::new(project).unwrap();
    app.session = crate::workspace_state::WorkspaceSession::new(app.editor.project());
    let rows: Vec<_> = results(app.editor.project(), &app.localizer, &name)
        .into_iter()
        .filter(|r| r.group == Group::Boards)
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .any(|r| r.route == ResultRoute::Entity(Destination::Board(first)))
    );
    let selected = rows
        .iter()
        .find(|r| r.route == ResultRoute::Entity(Destination::Board(second)))
        .unwrap();
    assert_eq!(app.invoke_palette(selected).unwrap(), Outcome::Navigated);
    assert_eq!(app.selection.active, Some(second));
    assert_eq!(app.session.inspector, Some(InspectorTarget::Board(second)));
    let mut removed = Project::new("Replacement", app.editor.project().currency);
    removed.id = app.editor.project().id;
    // A previously rendered row must never be resolved by its equal name.
    app.editor = ProjectEditor::new(removed).unwrap();
    let before = app.editor.project().clone();
    assert_eq!(
        app.invoke_palette(selected),
        Err(Unavailable::MissingTarget)
    );
    assert_eq!(app.editor.project(), &before);
}

#[test]
fn equal_material_and_stock_names_keep_distinct_typed_destinations() {
    let mut project = reference_fixture::project();
    let mut material = project.materials[0].clone();
    material.id = uuid::Uuid::new_v4();
    project.materials.push(material.clone());
    let mut stock = project.stock[0].clone();
    stock.id = uuid::Uuid::new_v4();
    project
        .stock_aliases
        .insert(stock.id, format!("S{}", project.next_stock_s_alias));
    project.next_stock_s_alias += 1;
    project.stock.push(stock.clone());
    let mut app = DesktopApp::default();
    app.editor = ProjectEditor::new(project).unwrap();
    app.session = crate::workspace_state::WorkspaceSession::new(app.editor.project());
    let material_rows: Vec<_> = results(app.editor.project(), &app.localizer, &material.name)
        .into_iter()
        .filter(|r| r.group == Group::Materials && r.label == material.name)
        .collect();
    assert_eq!(material_rows.len(), 2);
    let target = material_rows
        .iter()
        .find(|r| r.route == ResultRoute::Entity(Destination::Material(material.id)))
        .unwrap();
    assert_eq!(app.invoke_palette(target), Ok(Outcome::Navigated));
    assert_eq!(
        app.session.inspector,
        Some(InspectorTarget::Material(material.id))
    );
    let stock_rows: Vec<_> = results(app.editor.project(), &app.localizer, &stock.name)
        .into_iter()
        .filter(|r| r.group == Group::Stock && r.label == stock.name)
        .collect();
    assert_eq!(stock_rows.len(), 2);
    let target = stock_rows
        .iter()
        .find(|r| r.route == ResultRoute::Entity(Destination::Sheet(stock.id)))
        .unwrap();
    assert_eq!(app.invoke_palette(target), Ok(Outcome::Navigated));
    assert_eq!(app.session.focused_sheet, Some(stock.id));
    assert_eq!(
        app.session.inspector,
        Some(InspectorTarget::Sheet(stock.id))
    );
    assert_eq!(app.selection.active, None);
}

#[test]
fn preview_navigation_prompts_and_stay_retains_the_preview() {
    let mut app = fixture();
    let board = app.editor.project().boards[0].id;
    app.selection.choose(Some(board), false);
    app.editor.begin_preview();
    let command = results(app.editor.project(), &app.localizer, "NewBoard")
        .into_iter()
        .find(|r| r.route == ResultRoute::Action(Request::new(A::NewBoard)))
        .unwrap();
    assert_eq!(app.invoke_palette(&command), Err(Unavailable::PendingEdit));
    assert!(app.modals.creation().is_none());
    let row = results(
        app.editor.project(),
        &app.localizer,
        &app.editor.project().materials[0].id.to_string(),
    )
    .into_iter()
    .find(|r| r.group == Group::Materials)
    .unwrap();
    let original = app.editor.project().clone();
    assert!(matches!(
        app.invoke_palette(&row),
        Ok(Outcome::Prompt { .. })
    ));
    assert_eq!(app.session.active, Workspace::Design);
    assert!(app.navigation.pending().is_some());
    assert_eq!(
        app.resolve_navigation(crate::NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert!(app.editor.preview().is_some());
    assert_eq!(app.editor.project(), &original);
}

#[test]
fn disabled_actions_modal_and_pending_navigation_cannot_bypass_guards() {
    let mut app = fixture();
    let undo = results(app.editor.project(), &app.localizer, "undo")
        .into_iter()
        .find(|r| r.route == ResultRoute::Action(Request::new(A::Undo)))
        .unwrap();
    assert_eq!(
        app.palette_availability(undo.route),
        Err(Unavailable::NoUndo)
    );
    assert_eq!(app.invoke_palette(&undo), Err(Unavailable::NoUndo));
    let board = app.editor.project().boards[0].id;
    let row = results(app.editor.project(), &app.localizer, &board.to_string())
        .into_iter()
        .find(|r| r.route == ResultRoute::Entity(Destination::Board(board)))
        .unwrap();
    app.invoke(Request::with(A::EditDimensions, Target::Board(board)))
        .unwrap();
    assert_eq!(app.invoke_palette(&row), Err(Unavailable::ModalOpen));
    app.modals.set_board_dimension(None);
    // Pending edit resolution cannot be overridden by another result.
    let edit = crate::pending_navigation::EditBlock {
        kind: crate::pending_navigation::EditKind::Field,
        source: crate::pending_navigation::Revision::of(app.editor.project()),
        workspace: app.session.active,
        target: Some(InspectorTarget::Board(board)),
        can_commit: false,
    };
    assert!(matches!(
        app.navigation.request(
            crate::pending_navigation::NavigationIntent::at(
                app.editor.project(),
                Route::Workspace(Workspace::Stock)
            ),
            Some(edit),
            app.editor.project(),
            &mut app.session,
            &mut app.selection
        ),
        Outcome::Prompt { .. }
    ));
    assert_eq!(app.invoke_palette(&row), Err(Unavailable::ModalOpen));
}

#[test]
fn keyboard_navigation_escape_restores_focus_and_text_shortcuts_do_not_leak() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut app = fixture();
    let mut invoker = Id::NULL;
    let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
        invoker = ui.button("Search").id;
        ui.memory_mut(|m| m.request_focus(invoker));
        app.palette.open(ui.ctx());
        assert!(app.modal_open());
        app.show_palette(ui.ctx());
        assert_eq!(ui.memory(|m| m.focused()), Some(Id::new(QUERY_ID)));
    });
    frame.textures_delta.clear();
    let mut frame = ctx.run_ui(key(egui::Key::ArrowDown, egui::Modifiers::NONE), |ui| {
        app.show_palette(ui.ctx());
        assert_eq!(app.palette.selected, 1);
    });
    frame.textures_delta.clear();
    let mut frame = ctx.run_ui(key(egui::Key::ArrowUp, egui::Modifiers::NONE), |ui| {
        app.show_palette(ui.ctx());
        assert_eq!(app.palette.selected, 0);
    });
    frame.textures_delta.clear();
    let mut frame = ctx.run_ui(key(egui::Key::Escape, egui::Modifiers::NONE), |ui| {
        app.show_palette(ui.ctx());
        assert!(!app.palette.open);
        assert_eq!(ui.memory(|m| m.focused()), Some(invoker));
    });
    frame.textures_delta.clear();
    let text = Id::new("text-input");
    let mut value = String::new();
    let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.add(egui::TextEdit::singleline(&mut value).id(text));
        ui.memory_mut(|m| m.request_focus(text));
    });
    frame.textures_delta.clear();
    let mut frame = ctx.run_ui(key(egui::Key::K, egui::Modifiers::COMMAND), |ui| {
        assert!(!shortcut(ui.ctx(), true));
        ui.add(egui::TextEdit::singleline(&mut value).id(text));
    });
    frame.textures_delta.clear();
    let mut frame = ctx.run_ui(key(egui::Key::K, egui::Modifiers::COMMAND), |ui| {
        ui.memory_mut(|m| m.stop_text_input());
        assert!(shortcut(ui.ctx(), true));
    });
    frame.textures_delta.clear();
}

#[test]
fn enter_activates_the_live_action_and_popup_keys_do_not_escape() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut app = fixture();
    let old_project = app.editor.project().id;
    let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
        app.palette.open(ui.ctx());
        app.palette.query = "NewProject".into();
        app.show_palette(ui.ctx());
    });
    frame.textures_delta.clear();
    assert_eq!(app.palette_results().len(), 1);
    let popup = Id::new("palette-nested-popup");
    egui::Popup::open_id(&ctx, popup);
    let mut frame = ctx.run_ui(key(egui::Key::Enter, egui::Modifiers::NONE), |ui| {
        app.show_palette(ui.ctx());
        egui::Popup::close_id(ui.ctx(), popup);
    });
    frame.textures_delta.clear();
    assert_eq!(app.editor.project().id, old_project);
    assert!(app.palette.open);
    egui::Popup::open_id(&ctx, popup);
    let mut frame = ctx.run_ui(key(egui::Key::Escape, egui::Modifiers::NONE), |ui| {
        app.show_palette(ui.ctx());
        egui::Popup::close_id(ui.ctx(), popup);
    });
    frame.textures_delta.clear();
    assert!(app.palette.open);
    let mut frame = ctx.run_ui(key(egui::Key::Enter, egui::Modifiers::NONE), |ui| {
        app.show_palette(ui.ctx());
    });
    frame.textures_delta.clear();
    assert_ne!(app.editor.project().id, old_project);
    assert!(!app.palette.open);
}

#[test]
fn unavailable_reason_is_localized_without_mutating_project() {
    let mut app = fixture();
    app.palette.query = "Undo".into();
    let row = app
        .palette_results()
        .into_iter()
        .find(|r| r.route == ResultRoute::Action(Request::new(A::Undo)))
        .unwrap();
    let before = app.editor.project().clone();
    assert_eq!(app.invoke_palette(&row), Err(Unavailable::NoUndo));
    assert_eq!(app.editor.project(), &before);
    for language in [Language::En, Language::PtBr] {
        assert!(!Unavailable::NoUndo.reason(language).is_empty());
    }
}
