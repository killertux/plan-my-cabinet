use super::*;

#[test]
fn measure_scope_and_frame_survive_guarded_tool_switches_without_project_edits() {
    let mut app = DesktopApp {
        editor: plan_my_cabinet::commands::ProjectEditor::new(
            plan_my_cabinet::reference_fixture::project(),
        )
        .unwrap(),
        ..Default::default()
    };
    let root = app.editor.project().assemblies[0].id;
    app.selection.choose(Some(root), false);
    let before = app.editor.project().clone();
    let undo = app.editor.can_undo();
    app.invoke(
        Request::new(ActionId::SetMeasurementScope).argument(Argument::Scope(Scope::Overall)),
    )
    .unwrap();
    app.invoke(
        Request::new(ActionId::SetMeasurementFrame).argument(Argument::Frame(Frame::Object(root))),
    )
    .unwrap();
    for (action, expected) in [
        (ActionId::ViewMeasure, viewport::ToolMode::Measure),
        (ActionId::ViewMove, viewport::ToolMode::Move),
        (ActionId::ViewNavigate, viewport::ToolMode::Navigate),
        (ActionId::ViewMeasure, viewport::ToolMode::Measure),
    ] {
        app.invoke(Request::new(action)).unwrap();
        assert_eq!(app.design.move_tool.mode, expected);
        assert_eq!(app.design.measurement_scope, Scope::Overall);
        assert_eq!(app.design.measurement_frame, Frame::Object(root));
    }
    assert_eq!(app.editor.project(), &before);
    assert_eq!(app.editor.can_undo(), undo);
}

#[test]
fn save_undo_redo_settings_shortcuts_do_not_escape_a_text_field_or_modal() {
    let ctx = egui::Context::default();
    let event = |key, shift| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers {
            command: true,
            shift,
            ..Default::default()
        },
    };
    for (key, shift, action) in [
        (egui::Key::S, false, ActionId::SaveProject),
        (egui::Key::Z, false, ActionId::Undo),
        (egui::Key::Z, true, ActionId::Redo),
        (egui::Key::Comma, false, ActionId::OpenSettings),
    ] {
        ctx.run_ui(
            egui::RawInput {
                events: vec![event(key, shift)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(project_shortcut(ui.ctx(), false), Some(action));
                assert_eq!(project_shortcut(ui.ctx(), true), None);
            },
        )
        .drop_without_applying_deltas();
    }
    let mut text = String::from("pending");
    let id = egui::Id::new("project-shortcut-edit");
    ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.add(egui::TextEdit::singleline(&mut text).id(id));
        ui.memory_mut(|m| m.request_focus(id));
    })
    .drop_without_applying_deltas();
    ctx.run_ui(
        egui::RawInput {
            events: vec![event(egui::Key::Z, false)],
            ..Default::default()
        },
        |ui| {
            assert_eq!(project_shortcut(ui.ctx(), false), None);
            ui.add(egui::TextEdit::singleline(&mut text).id(id));
        },
    )
    .drop_without_applying_deltas();
}

#[test]
fn undo_shortcut_resolves_a_dirty_draft_before_touching_history() {
    let mut app = DesktopApp {
        editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
        ..Default::default()
    };
    app.sync_scene_inspector();
    let id = app.editor.project().boards[0].id;
    app.request_scene_selection(Some(id), false);
    let original_spacing = app.editor.project().grid_spacing;
    app.editor
        .set_grid_spacing(plan_my_cabinet::units::Length::from_micrometres(20_000))
        .unwrap();
    let before = app.editor.project().clone();
    app.edit_drafts
        .board(
            &app.editor,
            id,
            plan_my_cabinet::units::Unit::Mm,
            plan_my_cabinet::dimension_input::Locale::En,
        )
        .unwrap()
        .length
        .edit("not a length");
    let ctx = egui::Context::default();
    let command_z = egui::Event::Key {
        key: egui::Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers {
            command: true,
            ..Default::default()
        },
    };
    ctx.run_ui(
        egui::RawInput {
            events: vec![command_z],
            ..Default::default()
        },
        |ui| {
            let action = project_shortcut(ui.ctx(), app.modal_open()).unwrap();
            app.invoke(Request::new(action)).unwrap();
        },
    )
    .drop_without_applying_deltas();
    assert_eq!(app.editor.project(), &before);
    assert!(app.navigation.pending().is_some());
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert_eq!(app.editor.project(), &before);
    assert_eq!(
        app.edit_drafts
            .existing_board(before.id, id)
            .unwrap()
            .length
            .display(),
        "not a length"
    );
    app.invoke(Request::new(ActionId::Undo)).unwrap();
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Abandon),
        Outcome::Navigated
    );
    assert_eq!(app.editor.project().grid_spacing, original_spacing);
}
use std::collections::HashSet;

#[test]
fn existing_capabilities_have_unique_accessible_localized_routes() {
    // Inventory of the pre-relocation controls in main.rs and the *_ui.rs
    // modules. A capability can have several controls; each has a route.
    use ActionId as A;
    use Route as R;
    let routes = [
        (A::NewProject, R::Project),
        (A::OpenWelcome, R::Project),
        (A::OpenProject, R::Project),
        (A::SaveProject, R::Project),
        (A::SaveProjectAs, R::Project),
        (A::OpenSettings, R::Project),
        (A::Undo, R::Design),
        (A::Redo, R::Design),
        (A::NewBoard, R::Design),
        (A::NewMaterial, R::Design),
        (A::EditMaterial, R::Design),
        (A::EditGrid, R::Design),
        (A::EditKerf, R::CutPlan),
        (A::AddCatalog, R::Hardware),
        (A::UpdateCatalog, R::Hardware),
        (A::BatchDimensions, R::Design),
        (A::SelectObject, R::Design),
        (A::ToggleVisibility, R::Design),
        (A::Group, R::Design),
        (A::Reparent, R::Design),
        (A::Ungroup, R::Design),
        (A::DuplicateAssembly, R::Design),
        (A::Transform, R::Design),
        (A::SelectBoard, R::Design),
        (A::AssignMaterial, R::Design),
        (A::PositionBoard, R::Design),
        (A::PlaceFace, R::Design),
        (A::DuplicateBoard, R::Design),
        (A::RenameObject, R::Design),
        (A::EditDimensions, R::Design),
        (A::SetGrain, R::Design),
        (A::NewStock, R::Stock),
        (A::EditStock, R::Stock),
        (A::DuplicateStock, R::Stock),
        (A::DeleteStock, R::Stock),
        (A::StockMove, R::Stock),
        (A::EditCutFee, R::Stock),
        (A::EditCurrency, R::Stock),
        (A::NewHardware, R::Hardware),
        (A::EditHardware, R::Hardware),
        (A::DuplicateHardware, R::Hardware),
        (A::DeleteHardware, R::Hardware),
        (A::NewHinge, R::Hardware),
        (A::EditHinge, R::Hardware),
        (A::DeleteHinge, R::Hardware),
        (A::NewDoor, R::Hardware),
        (A::EditDoor, R::Hardware),
        (A::DeleteDoor, R::Hardware),
        (A::DeleteObject, R::Design),
        (A::StartMotion, R::Hardware),
        (A::CloseMotion, R::Hardware),
        (A::LocateIssue, R::CutPlan),
        (A::RepairIssue, R::CutPlan),
        (A::AddIssueStock, R::CutPlan),
        (A::BeginRepair, R::CutPlan),
        (A::AcceptRepair, R::CutPlan),
        (A::CancelRepair, R::CutPlan),
        (A::StageRepair, R::CutPlan),
        (A::Unallocate, R::CutPlan),
        (A::ToggleAllocationLock, R::CutPlan),
        (A::SelectSheetBoard, R::CutPlan),
        (A::PlaceUnallocated, R::CutPlan),
        (A::ReplanSheets, R::CutPlan),
        (A::AddSuggestedSheets, R::CutPlan),
        (A::StartOptimization, R::CutPlan),
        (A::CancelOptimization, R::CutPlan),
        (A::AcceptOptimization, R::CutPlan),
        (A::ConfirmKerf, R::Handoff),
        (A::OpenHandoff, R::Handoff),
        (A::ExportPdf, R::Handoff),
        (A::ReplacePdf, R::Dialog),
        (A::CancelExport, R::Dialog),
        (A::DirtySave, R::Dialog),
        (A::DirtyDiscard, R::Dialog),
        (A::OverwriteProject, R::Dialog),
        (A::Recover, R::Dialog),
        (A::DiscardRecovery, R::Dialog),
        (A::DeferRecovery, R::Dialog),
        (A::ConfirmDialog, R::Dialog),
        (A::CancelDialog, R::Dialog),
        (A::SetUiLanguage, R::Design),
        (A::SetMeasurementScope, R::Design),
        (A::SetMeasurementFrame, R::Design),
        (A::SetExportMode, R::Handoff),
        (A::SetExportLanguage, R::Handoff),
        (A::SetExportUnits, R::Handoff),
        (A::SetOptimizerObjective, R::CutPlan),
        (A::SetDoorAngle, R::Hardware),
        (A::ViewNavigate, R::Design),
        (A::ViewMove, R::Design),
        (A::ViewMeasure, R::Design),
        (A::ViewFrame, R::Design),
        (A::ViewPreset, R::Design),
        (A::ViewProjection, R::Design),
    ];
    assert_eq!(
        routes.len(),
        ALL.len(),
        "update the control inventory with each new action"
    );
    let mut seen = HashSet::new();
    let mut stable_ids = HashSet::new();
    let en = include_str!("../../../i18n/en.ftl");
    let pt = include_str!("../../../i18n/pt-BR.ftl");
    // Catch registered commands that have no actual pre-relocation UI route.
    let sources = [
        include_str!("../../main.rs"),
        include_str!("../shell.rs"),
        include_str!("../board_dialogs.rs"),
        include_str!("../export_flow.rs"),
        include_str!("../settings_host.rs"),
        include_str!("../navigation.rs"),
        include_str!("../project_ui.rs"),
        include_str!("../assembly_ui.rs"),
        include_str!("../stock_ui.rs"),
        include_str!("../hardware_ui.rs"),
        include_str!("../hinge_ui.rs"),
        include_str!("../door_joint_ui.rs"),
        include_str!("../sheet_ui.rs"),
        include_str!("../optimization_ui.rs"),
        include_str!("../placement_ui.rs"),
        include_str!("../viewport/controls.rs"),
    ];
    let app = DesktopApp::default();
    for (id, route) in routes {
        assert!(seen.insert(id), "duplicate route: {id:?}");
        let descriptor = id.descriptor();
        assert_eq!(descriptor.route, route);
        assert!(stable_ids.insert(descriptor.stable_id));
        assert!(
            sources
                .iter()
                .any(|source| source.contains(&format!("A::{id:?}"))
                    || source.contains(&format!("ActionId::{id:?}"))),
            "no control invokes {id:?}"
        );
        for resource in [en, pt] {
            assert!(
                resource
                    .lines()
                    .any(|line| line.starts_with(&format!("{} =", descriptor.key))),
                "missing translated action label: {}",
                descriptor.key
            );
        }
        for language in [Language::En, Language::PtBr] {
            let localizer = Localizer::new(language);
            assert!(!id.label(&localizer).trim().is_empty());
            assert!(!id.keywords(language).trim().is_empty());
            if let Err(reason) = app.action_availability(Request::new(id)) {
                assert!(
                    !reason.reason(language).trim().is_empty(),
                    "{id:?} lacks a disabled reason"
                );
            }
        }
    }
}

#[test]
fn refused_ui_actions_are_reported_but_an_empty_history_stays_quiet() {
    let mut app = DesktopApp::default();
    app.invoke_or_report(Request::new(ActionId::Undo));
    assert!(app.toasts.texts().is_empty());
    let stale = Request::with(ActionId::EditDimensions, Target::Board(Uuid::new_v4()));
    app.invoke_or_report(stale);
    assert_eq!(app.toasts.texts(), ["The target no longer exists"]);
}

#[test]
fn stale_targets_and_modal_guards_return_reasons_without_editing() {
    let mut app = DesktopApp::default();
    let original = app.editor.project().clone();
    let missing = Uuid::new_v4();
    let stale = Request::with(ActionId::EditDimensions, Target::Board(missing));
    assert_eq!(app.invoke(stale), Err(Unavailable::MissingTarget));
    for (id, target, expected) in [
        (
            ActionId::EditMaterial,
            Target::Material(missing),
            Unavailable::MissingTarget,
        ),
        (
            ActionId::EditStock,
            Target::Stock(missing),
            Unavailable::MissingTarget,
        ),
        (
            ActionId::StockMove,
            Target::Stock(missing),
            Unavailable::MissingTarget,
        ),
        (
            ActionId::UpdateCatalog,
            Target::Catalog(missing),
            Unavailable::MissingTarget,
        ),
        (
            ActionId::EditHinge,
            Target::Hinge(missing),
            Unavailable::MissingTarget,
        ),
        (
            ActionId::EditDoor,
            Target::Door(missing),
            Unavailable::MissingTarget,
        ),
        (
            ActionId::EditHardware,
            Target::Object(missing),
            Unavailable::InvalidHardware,
        ),
    ] {
        assert_eq!(
            app.invoke(Request::with(id, target)),
            Err(expected),
            "{id:?}"
        );
    }
    assert_eq!(
        app.action_availability(Request::new(ActionId::Undo)),
        Err(Unavailable::NoUndo)
    );
    assert_eq!(
        app.action_availability(Request::new(ActionId::PlaceFace)),
        Err(Unavailable::NeedsAnotherBoard)
    );
    assert_eq!(
        app.action_availability(Request::new(ActionId::AssignMaterial)),
        Err(Unavailable::NeedsMaterial)
    );
    assert_eq!(app.editor.project(), &original);

    app.invoke(Request::new(ActionId::NewBoard)).unwrap();
    assert_eq!(
        app.invoke(Request::new(ActionId::NewStock)),
        Err(Unavailable::ModalOpen)
    );
    assert_eq!(app.editor.project(), &original);
    for reason in [
        Unavailable::ModalOpen,
        Unavailable::MissingTarget,
        Unavailable::NoUndo,
        Unavailable::NoRedo,
        Unavailable::Busy,
        Unavailable::NoRepair,
        Unavailable::ExportNotReady,
    ] {
        assert!(!reason.reason(Language::En).is_empty());
        assert!(!reason.reason(Language::PtBr).is_empty());
    }
    let mut called = false;
    assert_eq!(
        contextual(
            Request::new(ActionId::ConfirmDialog),
            Err(Unavailable::ModalOpen),
            || called = true
        ),
        Err(Unavailable::ModalOpen)
    );
    assert!(
        !called,
        "unavailable contextual action must not invoke its handler"
    );
}

#[test]
fn stock_move_requires_a_valid_rank_before_invocation() {
    let mut app = DesktopApp {
        editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
        ..Default::default()
    };
    let original = app.editor.project().clone();
    let stock = original.stock[0].id;
    for argument in [
        Argument::None,
        Argument::StockPriority {
            target: original.stock.len(),
            subset: false,
        },
        Argument::StockPriority {
            target: original.stock.len(),
            subset: true,
        },
    ] {
        assert_eq!(
            app.invoke(Request::with(ActionId::StockMove, Target::Stock(stock)).argument(argument)),
            Err(Unavailable::MissingTarget)
        );
    }
    assert_eq!(app.editor.project(), &original);
}

#[test]
fn header_routes_use_real_history_and_guard_handoff_drafts() {
    let mut app = DesktopApp::default();
    let initial_revision = app.editor.project().revision;
    assert_eq!(
        app.action_availability(Request::new(ActionId::Undo)),
        Err(Unavailable::NoUndo)
    );
    app.editor.confirm_shop_kerf().unwrap();
    assert!(app.editor.is_dirty());
    assert!(app.editor.project().revision > initial_revision);
    app.invoke(Request::new(ActionId::Undo)).unwrap();
    assert!(app.editor.project().confirmed_shop_kerf.is_none());
    app.invoke(Request::new(ActionId::Redo)).unwrap();
    assert_eq!(
        app.editor.project().confirmed_shop_kerf,
        Some(app.editor.project().cutting_kerf)
    );

    app.editor = ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap();
    app.session = WorkspaceSession::new(app.editor.project());
    app.invoke(Request::with(
        ActionId::EditDimensions,
        Target::Board(plan_my_cabinet::reference_fixture::LEFT_SIDE_ID),
    ))
    .unwrap();
    app.modals.board_dimension_mut().unwrap().value.text = "invalid".into();
    assert_eq!(
        app.action_availability(Request::new(ActionId::SaveProject)),
        Ok(())
    );
    app.invoke(Request::new(ActionId::SaveProject)).unwrap();
    assert!(app.navigation.pending().is_some());
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert_eq!(app.modals.board_dimension().unwrap().value.text, "invalid");
    app.invoke(Request::new(ActionId::OpenHandoff)).unwrap();
    assert_eq!(app.session.active, Workspace::Design);
    assert!(app.navigation.pending().is_some());
    assert_eq!(
        app.action_availability(Request::new(ActionId::OpenHandoff)),
        Err(Unavailable::ModalOpen)
    );
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Stay),
        Outcome::Stayed
    );
    assert_eq!(app.modals.board_dimension().unwrap().value.text, "invalid");
    app.invoke(Request::new(ActionId::OpenHandoff)).unwrap();
    assert_eq!(
        app.resolve_navigation(NavigationDecision::Abandon),
        Outcome::Navigated
    );
    assert_eq!(app.session.active, Workspace::Handoff);
    assert!(app.modals.board_dimension().is_none());
    assert!(app.handoff.activity.is_none());
}

#[test]
fn sheet_selection_route_revalidates_identity() {
    let app = DesktopApp::default();
    let mut selection = viewport::Selection::default();
    let missing = Uuid::new_v4();
    assert_eq!(
        select_sheet_board(
            app.editor.project(),
            &mut selection,
            Request::with(ActionId::SelectSheetBoard, Target::Board(missing))
        ),
        Err(Unavailable::MissingTarget)
    );
    assert!(selection.ids.is_empty());
}

#[test]
fn real_entry_points_open_original_forms_and_view_choices_are_not_edits() {
    for id in [
        ActionId::NewBoard,
        ActionId::NewMaterial,
        ActionId::EditGrid,
        ActionId::EditKerf,
        ActionId::NewStock,
        ActionId::EditCutFee,
        ActionId::EditCurrency,
        ActionId::NewHardware,
        ActionId::NewHinge,
        ActionId::NewDoor,
    ] {
        let mut app = DesktopApp::default();
        let original = app.editor.project().clone();
        app.invoke(Request::new(id)).unwrap();
        assert!(app.modal_open(), "{id:?} did not open its real form");
        assert_eq!(
            app.editor.project(),
            &original,
            "{id:?} edited before confirmation"
        );
    }
    let mut app = DesktopApp::default();
    let original = app.editor.project().clone();
    for request in [
        Request::new(ActionId::SetUiLanguage).argument(Argument::Language(Language::PtBr)),
        Request::new(ActionId::SetMeasurementScope).argument(Argument::Scope(Scope::Overall)),
        Request::new(ActionId::SetExportMode).argument(Argument::ExportMode(ExportMode::Draft)),
        Request::new(ActionId::SetExportLanguage).argument(Argument::Language(Language::PtBr)),
        Request::new(ActionId::SetExportUnits).argument(Argument::Unit(Unit::Foot)),
    ] {
        app.invoke(request).unwrap();
    }
    assert_eq!(app.handoff.units, Unit::Foot);
    assert_eq!(app.localizer.language(), Language::PtBr);
    assert_eq!(app.editor.project(), &original);
    assert!(!app.editor.is_dirty());
}

#[test]
fn missing_sheets_are_added_and_parts_placed_in_one_undo_step() {
    use plan_my_cabinet::allocation_diagnostics::{Status, diagnose};
    use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
    let mut app = DesktopApp::default();
    let mm = |n: i64| Length::from_micrometres(n * 1000);
    // Named like a standard preset, so its sheet size is known.
    let material = app
        .editor
        .create_material(NewMaterial {
            name: "White MDF".into(),
            thickness: mm(15),
            grain: BoardGrain::Unrestricted,
        })
        .unwrap();
    for name in ["Side", "Side", "Top", "Bottom", "Shelf"] {
        app.editor
            .create_board(NewBoard {
                name: name.into(),
                material_id: material,
                length: mm(1300),
                width: mm(900),
                pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
            })
            .unwrap();
    }
    assert!(app.editor.project().allocations.is_empty());
    // Nothing to place on yet: the command runs and reports what is waiting.
    app.invoke(Request::new(ActionId::PlaceUnallocated))
        .unwrap();
    assert_eq!(app.toasts.texts().len(), 1);
    let before = app.editor.project().clone();
    app.invoke(
        Request::with(ActionId::AddSuggestedSheets, Target::Material(material))
            .argument(Argument::Length(mm(15))),
    )
    .unwrap();
    let project = app.editor.project();
    assert_eq!(project.stock.len(), 2);
    assert!(
        diagnose(project)
            .iter()
            .all(|d| d.status == Status::AllocatedValid)
    );
    assert_eq!(
        app.toasts.texts().last().copied(),
        Some("Added 2 sheets and placed 5 parts.")
    );
    app.invoke(Request::new(ActionId::ReplanSheets)).unwrap();
    assert!(
        diagnose(app.editor.project())
            .iter()
            .all(|d| d.status == Status::AllocatedValid)
    );
    app.invoke(Request::new(ActionId::Undo)).unwrap();
    app.invoke(Request::new(ActionId::Undo)).unwrap();
    assert_eq!(app.editor.project().stock, before.stock);
    assert_eq!(app.editor.project().allocations, before.allocations);
    // A material with nothing waiting has no suggestion to apply.
    assert_eq!(
        app.invoke(
            Request::with(
                ActionId::AddSuggestedSheets,
                Target::Material(Uuid::new_v4())
            )
            .argument(Argument::Length(mm(15)))
        ),
        Err(Unavailable::MissingTarget)
    );
}
