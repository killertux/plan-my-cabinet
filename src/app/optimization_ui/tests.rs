use super::*;
use plan_my_cabinet::domain::{Board, BoardGrain, Material, Stock, StockGrain, StockSource};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::units::{Length, Pose, Quaternion};
use uuid::Uuid;

fn fixture() -> ProjectEditor {
    let mut project = Project::new("ui", Currency::Brl);
    let material = Uuid::new_v4();
    let mm = |n: i64| Length::from_micrometres(n * 1000);
    project.materials.push(Material {
        id: material,
        name: "ply".into(),
        default_thickness: mm(18),
        default_grain: BoardGrain::Unrestricted,
    });
    project.boards.push(Board {
        id: Uuid::new_v4(),
        name: "part".into(),
        material_id: material,
        length: mm(100),
        width: mm(50),
        thickness: mm(18),
        grain_override: None,
        parent_id: None,
        pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
    });
    project.stock.push(Stock {
        id: Uuid::new_v4(),
        name: "sheet".into(),
        material_id: material,
        length: mm(100),
        width: mm(50),
        thickness: mm(18),
        grain: StockGrain::Nondirectional,
        source: StockSource::Owned,
        price: None,
        priority: 0,
        trim: [Length::ZERO; 4],
    });
    ProjectEditor::new(project).unwrap()
}

fn frame_in_language(
    ctx: &egui::Context,
    state: &mut OptimizeUi,
    editor: &mut ProjectEditor,
    events: Vec<egui::Event>,
    language: Language,
) -> Vec<(String, egui::Pos2)> {
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 900.),
            )),
            events,
            ..Default::default()
        },
        |ui| state.show(ui, editor, &Localizer::new(language), false),
    );
    let text = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(t) => Some((t.galley.text().to_owned(), t.pos)),
            _ => None,
        })
        .collect();
    output.drop_without_applying_deltas();
    text
}

fn frame(
    ctx: &egui::Context,
    state: &mut OptimizeUi,
    editor: &mut ProjectEditor,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Pos2)> {
    frame_in_language(ctx, state, editor, events, Language::En)
}

fn click(ctx: &egui::Context, state: &mut OptimizeUi, editor: &mut ProjectEditor, label: &str) {
    let shapes = frame(ctx, state, editor, vec![]);
    let pos = shapes
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("missing {label}: {shapes:?}"))
        .1
        + egui::vec2(10., 8.);
    frame(
        ctx,
        state,
        editor,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    frame(
        ctx,
        state,
        editor,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}

fn finish(ctx: &egui::Context, state: &mut OptimizeUi, editor: &mut ProjectEditor) {
    for _ in 0..1000 {
        frame(ctx, state, editor, vec![]);
        if state.worker.is_none() {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("search timed out");
}

#[test]
fn synthetic_start_cancel_stale_preview_and_atomic_accept() {
    let ctx = egui::Context::default();
    let mut editor = fixture();
    let original = editor.project().clone();
    let mut ui = OptimizeUi::default();
    assert!(!ui.start(&editor, true)); // modal or sheet repair
    click(&ctx, &mut ui, &mut editor, "Search");
    assert!(ui.worker.is_some());
    // The tiny fixture may finish before a second pointer event; cancellation
    // still invalidates an already delivered proposal as well as an active run.
    ui.cancel();
    assert!(ui.worker.is_none() && ui.result.is_none());
    assert_eq!(editor.project(), &original);
    assert!(!editor.can_undo());

    click(&ctx, &mut ui, &mut editor, "Search");
    finish(&ctx, &mut ui, &mut editor);
    assert!(
        ui.result
            .as_ref()
            .is_some_and(|r| !r.ranking.candidates.is_empty())
    );
    assert!(ui.accept(&mut editor, true).is_err());
    assert_eq!(editor.project(), &original); // preview is read-only
    assert!(!editor.can_undo());
    editor
        .transact(|p| -> Result<(), ()> {
            p.boards[0].length = Length::from_micrometres(99_000);
            Ok(())
        })
        .unwrap();
    frame(&ctx, &mut ui, &mut editor, vec![]);
    assert!(!ui.result.as_ref().unwrap().is_current(editor.project()));
    assert!(matches!(
        ui.accept(&mut editor, false),
        Err(ApplyError::Stale { .. })
    ));
    editor.undo().unwrap();
    click(&ctx, &mut ui, &mut editor, "Search");
    finish(&ctx, &mut ui, &mut editor);
    click(&ctx, &mut ui, &mut editor, "Review current vs best");
    assert!(ui.comparison_open());
    click(&ctx, &mut ui, &mut editor, "Accept best found");
    assert!(!ui.comparison_open());
    assert_eq!(editor.project().allocations.len(), 1);
    editor.undo().unwrap();
    assert_eq!(editor.project().allocations, original.allocations);
}

#[test]
fn compact_inspector_exposes_metrics_and_placements_without_accepting() {
    let mut editor = fixture();
    let mut state = OptimizeUi::default();
    let ctx = egui::Context::default();
    assert!(state.start(&editor, false));
    finish(&ctx, &mut state, &mut editor);
    let original = editor.project().clone();
    for language in [Language::En, Language::PtBr] {
        let localizer = Localizer::new(language);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(308.0, 650.0),
                )),
                ..Default::default()
            },
            |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    state.show_inspector_comparison(ui, editor.project(), &localizer, false);
                });
            },
        );
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for key in [
            "optimize-stock-used",
            "optimize-cuts-count",
            "optimize-changes",
        ] {
            assert!(
                labels
                    .iter()
                    .any(|label| label.contains(&localizer.text(key))),
                "{labels:?}"
            );
        }
        assert!(
            labels.iter().any(|label| label.contains("part")),
            "{labels:?}"
        );
        output.drop_without_applying_deltas();
    }
    assert_eq!(editor.project(), &original);
    assert!(!editor.can_undo());
    assert!(!state.comparison_open());
}

#[test]
fn host_modal_guard_blocks_palette_and_project_actions_during_comparison() {
    use crate::DesktopApp;
    let ctx = egui::Context::default();
    let mut app = DesktopApp {
        editor: fixture(),
        ..Default::default()
    };
    click(&ctx, &mut app.cut_plan.optimizer, &mut app.editor, "Search");
    finish(&ctx, &mut app.cut_plan.optimizer, &mut app.editor);
    click(
        &ctx,
        &mut app.cut_plan.optimizer,
        &mut app.editor,
        "Review current vs best",
    );
    assert!(app.cut_plan.optimizer.comparison_open());
    assert!(app.modal_open());
    let before = app.editor.project().clone();
    for action in [A::NewBoard, A::OpenHandoff, A::NewProject] {
        assert!(app.invoke(Request::new(action)).is_err(), "{action:?}");
    }
    assert_eq!(app.editor.project(), &before);
    assert!(app.cut_plan.optimizer.comparison_open());
}

#[test]
fn palette_optimizer_actions_start_cancel_and_route_to_review_without_auto_apply() {
    use crate::{
        DesktopApp,
        workspace_state::{Workspace, WorkspaceSession},
    };
    let mut app = DesktopApp {
        editor: fixture(),
        ..Default::default()
    };
    app.session = WorkspaceSession::new(app.editor.project());
    let before = app.editor.project().clone();
    app.invoke(Request::new(A::StartOptimization)).unwrap();
    assert!(app.cut_plan.optimizer.running());
    assert_eq!(app.editor.project(), &before);
    app.invoke(Request::new(A::CancelOptimization)).unwrap();
    assert!(!app.cut_plan.optimizer.running());
    assert_eq!(app.editor.project(), &before);

    app.invoke(Request::new(A::StartOptimization)).unwrap();
    let ctx = egui::Context::default();
    for _ in 0..1000 {
        app.cut_plan.optimizer.poll(&ctx);
        if !app.cut_plan.optimizer.running() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        app.cut_plan
            .optimizer
            .acceptance_availability(app.editor.project())
            .is_ok()
    );
    app.invoke(Request::new(A::AcceptOptimization)).unwrap();
    assert_eq!(app.session.active, Workspace::CutPlan);
    assert!(app.cut_plan.optimizer.comparison_open());
    assert_eq!(app.editor.project(), &before);
    assert!(!app.editor.can_undo());
}

#[test]
fn palette_objective_route_exposes_controls_without_changing_objective_or_project() {
    use crate::{
        DesktopApp, workspace_shell,
        workspace_state::{Workspace, WorkspaceSession},
    };
    let mut app = DesktopApp {
        editor: fixture(),
        ..Default::default()
    };
    app.session = WorkspaceSession::new(app.editor.project());
    let project = app.editor.project().clone();
    let objective = app.cut_plan.optimizer.objective;
    app.invoke(Request::new(A::SetOptimizerObjective)).unwrap();
    assert_eq!(app.session.active, Workspace::CutPlan);
    assert_eq!(app.open_drawer, Some(workspace_shell::Drawer::Controls));
    assert_eq!(app.cut_plan.optimizer.objective, objective);
    assert_eq!(app.editor.project(), &project);
}

#[test]
fn running_search_reports_activity_across_workspaces_without_mutating_project() {
    use crate::{
        DesktopApp,
        workspace_state::{Workspace, WorkspaceSession},
    };
    let mut app = DesktopApp {
        editor: fixture(),
        ..Default::default()
    };
    app.session = WorkspaceSession::new(app.editor.project());
    let before = app.editor.project().clone();
    app.invoke(Request::new(A::StartOptimization)).unwrap();
    let ctx = egui::Context::default();
    for workspace in [Workspace::Design, Workspace::Hardware, Workspace::Handoff] {
        app.session.switch(workspace);
        let (issues, total, invalid) = app.shell_facts();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_shell_status(ui, issues, total, invalid)
        });
        assert!(output.shapes.iter().any(|shape| {
            match &shape.shape {
                egui::Shape::Text(text) => text
                    .galley
                    .text()
                    .contains(&app.localizer.text("shell-search-active")),
                _ => false,
            }
        }));
        output.drop_without_applying_deltas();
    }
    assert_eq!(app.editor.project(), &before);
    app.invoke(Request::new(A::CancelOptimization)).unwrap();
    assert_eq!(app.editor.project(), &before);
}

#[test]
fn bilingual_preview_labels_best_found_and_unknown_cost_without_zero() {
    let ctx = egui::Context::default();
    let mut editor = fixture();
    editor
        .transact(|p| -> Result<(), ()> {
            p.stock[0].source = StockSource::ToPurchase;
            p.stock[0].price = None;
            p.cut_fee = Some(plan_my_cabinet::money::Money::new(Currency::Brl, 0).unwrap());
            Ok(())
        })
        .unwrap();
    let mut ui = OptimizeUi::default();
    assert!(ui.start(&editor, false));
    finish(&ctx, &mut ui, &mut editor);
    click(&ctx, &mut ui, &mut editor, "Review current vs best");
    assert_eq!(
        ui.result.as_ref().unwrap().ranking.candidates[0].new_spending,
        None
    );
    for (language, best, unknown, warning) in [
        (
            Language::En,
            "Best found (complete, verified)",
            "Incomplete (unknown price or cut fee)",
            "Some costs are unknown; no lowest-spending claim is possible.",
        ),
        (
            Language::PtBr,
            "Melhor encontrado (completo e verificado)",
            "Incompleta (preço ou tarifa desconhecidos)",
            "Alguns custos são desconhecidos; não é possível afirmar a menor despesa.",
        ),
    ] {
        let labels: Vec<_> = frame_in_language(&ctx, &mut ui, &mut editor, vec![], language)
            .into_iter()
            .map(|(text, _)| text)
            .collect();
        assert!(labels.iter().any(|text| text == best), "{labels:?}");
        assert!(labels.iter().any(|text| text == warning), "{labels:?}");
        let cost = Localizer::new(language).text("optimize-best-found");
        assert!(
            labels
                .iter()
                .any(|text| text == &format!("{cost}: {unknown}")),
            "{labels:?}"
        );
        assert!(
            !labels
                .iter()
                .any(|text| text.starts_with(&format!("{cost}: BRL 0"))),
            "{labels:?}"
        );
    }
}

#[test]
fn comparison_is_wide_at_reference_and_compact_sizes_and_blocks_stale_acceptance() {
    for (language, size) in [Language::En, Language::PtBr]
        .into_iter()
        .flat_map(|language| {
            [egui::vec2(1440., 900.), egui::vec2(900., 650.)].map(|size| (language, size))
        })
    {
        let ctx = egui::Context::default();
        let mut editor = fixture();
        let mut state = OptimizeUi::default();
        assert!(state.start(&editor, false));
        for _ in 0..1000 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    ui.set_max_width(256.);
                    state.show(ui, &mut editor, &Localizer::new(language), false);
                },
            );
            output.drop_without_applying_deltas();
            if state.worker.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(state.worker.is_none());
        state.comparison = Some(ModalChrome::new(egui::Id::new("optimize-comparison")).width(800.));
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| {
                ui.set_max_width(256.);
                state.show(ui, &mut editor, &Localizer::new(language), false);
            },
        );
        output.drop_without_applying_deltas(); // modal opens after the controls pane this frame
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| {
                ui.set_max_width(256.);
                state.show(ui, &mut editor, &Localizer::new(language), false);
            },
        );
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        let rect = ctx
            .memory(|m| m.area_rect(egui::Id::new("optimize-comparison")))
            .unwrap();
        assert!(rect.width() >= 650., "modal width: {rect:?}");
        assert!(rect.height() <= size.y, "modal height: {rect:?}");
        assert!(
            rect.min.x >= 0. && rect.max.x <= size.x,
            "modal bounds: {rect:?}"
        );
        assert!(
            texts
                .iter()
                .any(|s| s == &Localizer::new(language).text("optimize-cost")),
            "{texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|s| s == &Localizer::new(language).text("optimize-current-unverified")),
            "{texts:?}"
        );
        let mut texts = Vec::new();
        for scroll in [true, false] {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events: if scroll {
                        vec![
                            egui::Event::PointerMoved(rect.center()),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0., -900.),
                                phase: egui::TouchPhase::Move,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
                |ui| {
                    ui.set_max_width(256.);
                    state.show(ui, &mut editor, &Localizer::new(language), false);
                },
            );
            texts = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
        }
        assert!(
            texts
                .iter()
                .any(|s| s.starts_with(&Localizer::new(language).text("optimize-proposed"))),
            "{texts:?}"
        );
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].length = Length::from_micrometres(99_000);
                Ok(())
            })
            .unwrap();
        assert!(matches!(
            state.acceptance_availability(editor.project()),
            Err(Unavailable::StaleOptimization)
        ));
        assert!(matches!(
            state.accept(&mut editor, false),
            Err(ApplyError::Stale { .. })
        ));
        state.close_comparison(&ctx);
    }
}

#[test]
fn all_objectives_no_result_and_locked_unchanged_layout() {
    let ctx = egui::Context::default();
    let mut editor = fixture();
    let mut state = OptimizeUi::default();
    for objective in [
        Objective::LowestNewSpending,
        Objective::FewestCuts,
        Objective::LeastUnusedStockArea,
    ] {
        state.objective = objective;
        assert!(state.start(&editor, false));
        finish(&ctx, &mut state, &mut editor);
        assert_eq!(state.objective, objective);
        assert!(!state.result.as_ref().unwrap().ranking.candidates.is_empty());
    }
    assert!(state.accept(&mut editor, false).unwrap());
    editor
        .transact(|p| -> Result<(), ()> {
            p.allocations[0].locked = true;
            Ok(())
        })
        .unwrap();
    assert!(state.start(&editor, false));
    finish(&ctx, &mut state, &mut editor);
    let result = state.result.as_ref().unwrap();
    let locked = &result.original_allocations[0];
    assert!(locked.locked);
    let proposed = &result.ranking.candidates[0].candidate.allocations[0];
    assert_eq!(locked, proposed);
    assert!(result.placement_changes(0).unwrap().is_empty());
    let before = editor.project().clone();
    assert!(!state.accept(&mut editor, false).unwrap());
    assert_eq!(editor.project(), &before);

    assert!(state.start(&editor, false));
    finish(&ctx, &mut state, &mut editor);
    state.result.as_mut().unwrap().ranking.candidates[0]
        .candidate
        .allocations[0]
        .locked = false;
    state.comparison = Some(ModalChrome::new(egui::Id::new("optimize-comparison")).width(800.));
    frame(&ctx, &mut state, &mut editor, vec![]);
    click(&ctx, &mut state, &mut editor, "Accept best found");
    assert_eq!(editor.project(), &before);
    assert!(state.comparison_open());
    state.close_comparison(&ctx);

    assert!(state.start(&editor, false));
    finish(&ctx, &mut state, &mut editor);
    state.result.as_mut().unwrap().ranking.candidates.clear();
    assert_eq!(
        state.acceptance_availability(editor.project()),
        Err(Unavailable::NoOptimization)
    );
    state.comparison = Some(ModalChrome::new(egui::Id::new("optimize-comparison")).width(800.));
    let labels = frame(&ctx, &mut state, &mut editor, vec![]);
    assert!(labels.iter().any(|(s, _)| s == "No complete verified plan found within the search budget. The current layout is unchanged."));
    assert!(matches!(
        state.accept(&mut editor, false),
        Err(ApplyError::UnknownCandidate)
    ));
}

#[test]
fn visible_cut_plan_searches_once_per_revision_after_a_quiet_moment() {
    let ctx = egui::Context::default();
    let mut editor = fixture();
    let mut state = OptimizeUi::default();
    let settle = |state: &mut OptimizeUi, editor: &ProjectEditor, visible, blocked| {
        state.auto_run(&ctx, editor, visible, blocked);
        if let Some((key, _)) = state.waiting {
            state.waiting = Some((key, Instant::now() - AUTO_DELAY));
        }
        state.auto_run(&ctx, editor, visible, blocked);
    };
    settle(&mut state, &editor, false, false);
    assert!(!state.running(), "hidden cut plan does not search");
    settle(&mut state, &editor, true, true);
    assert!(!state.running(), "an open dialog blocks the search");
    state.auto_run(&ctx, &editor, true, false);
    assert!(
        !state.running(),
        "the first sighting only starts the quiet timer"
    );
    settle(&mut state, &editor, true, false);
    assert!(state.running());
    state.cancel();
    settle(&mut state, &editor, true, false);
    assert!(
        !state.running(),
        "a cancelled revision is not searched again"
    );
    editor
        .transact(|p| -> Result<(), ()> {
            p.name = "edited".into();
            Ok(())
        })
        .unwrap();
    settle(&mut state, &editor, true, false);
    assert!(state.running(), "a new revision searches again");
    state.cancel();
    assert_eq!(
        editor.project().name,
        "edited",
        "searching never edits the project"
    );
}
