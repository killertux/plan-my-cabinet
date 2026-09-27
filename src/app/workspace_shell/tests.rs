use super::*;
use plan_my_cabinet::allocation_diagnostics::diagnose;
use plan_my_cabinet::cost_estimate::estimate;
use plan_my_cabinet::reference_fixture::{BACK_ID, LEFT_SIDE_ID, RIGHT_SIDE_ID};

#[test]
fn global_issues_include_hidden_unallocated_and_conflicting_placements() {
    let mut project = plan_my_cabinet::reference_fixture::project();
    let overlapping = project
        .allocations
        .iter_mut()
        .find(|a| a.board_id == RIGHT_SIDE_ID)
        .unwrap();
    overlapping.origin = [plan_my_cabinet::units::Length::ZERO; 2];
    let diagnostics = diagnose(&project);
    let counts =
        IssueCounts::from_diagnostics(&diagnostics, |id| id == BACK_ID || id == LEFT_SIDE_ID);
    assert_eq!(counts.unallocated, 1);
    assert!(counts.conflicted >= 2);
    assert_eq!(counts.hidden, 2);
    assert_eq!(
        complete_spending(&project, counts, estimate(&project).ok().as_ref()),
        None
    );
}

#[test]
fn unknown_price_or_fee_cannot_be_shown_as_a_total() {
    let project = Project::new("Empty", plan_my_cabinet::money::Currency::Brl);
    let counts = IssueCounts::from_diagnostics(&diagnose(&project), |_| false);
    assert_eq!(
        complete_spending(&project, counts, estimate(&project).ok().as_ref())
            .map(|v| v.minor_units()),
        Some(0)
    );
    // A used sheet with a missing fee or price is incomplete even if all
    // allocations are otherwise valid; unknown is distinct from free.
    let mut project = plan_my_cabinet::reference_fixture::project();
    project.boards.retain(|board| board.id != BACK_ID);
    let counts = IssueCounts::from_diagnostics(&diagnose(&project), |_| false);
    assert_eq!(counts.total(), 0);
    project.cut_fee = None;
    assert!(complete_spending(&project, counts, estimate(&project).ok().as_ref()).is_none());
    project.cut_fee = Some(plan_my_cabinet::money::Money::new(project.currency, 0).unwrap());
    assert!(complete_spending(&project, counts, estimate(&project).ok().as_ref()).is_some());
    project
        .stock
        .iter_mut()
        .find(|s| s.id == project.allocations[0].stock_id)
        .unwrap()
        .price = None;
    assert!(complete_spending(&project, counts, estimate(&project).ok().as_ref()).is_none());
}

#[test]
fn shell_labels_exist_in_both_languages() {
    for language in [
        plan_my_cabinet::i18n::Language::En,
        plan_my_cabinet::i18n::Language::PtBr,
    ] {
        let locale = Localizer::new(language);
        assert!(!more_label(language).is_empty());
        assert!(!inspector_label(language).is_empty());
        for action in OVERFLOW_ACTIONS {
            let label = action.label(&locale);
            assert!(!label.trim().is_empty(), "{language:?} {action:?}");
        }
        for key in [
            "shell-projects",
            "shell-export",
            "shell-unsaved",
            "shell-proof-unknown",
            "shell-search-active",
            "shell-export-active",
            "cost-incomplete",
            "global-conflicted",
            "global-hidden",
        ] {
            let label = locale.text(key);
            assert!(
                !label.trim().is_empty() && label != key,
                "{language:?}: {key}"
            );
        }
    }
}

#[test]
fn logical_widths_collapse_at_minimum_canvas_without_shrinking_panes() {
    let baseline = PaneLayout::for_width(Workspace::Design, 1440.0 - RAIL_WIDTH);
    assert_eq!((baseline.controls, baseline.inspector), (256.0, 292.0));
    for window in [(1440.0, 900.0), (1100.0, 700.0), (900.0, 650.0)] {
        for scale in [0.90, 1.0, 1.15, 1.30] {
            let width = window.0 / scale - RAIL_WIDTH;
            for (workspace, _, _) in ENTRIES {
                let layout = PaneLayout::for_width(workspace, width);
                assert!(
                    layout.canvas >= MIN_CANVAS,
                    "{workspace:?} {window:?} {scale}"
                );
                assert!(layout.controls == 0.0 || layout.controls >= 256.0);
                assert!(layout.inspector == 0.0 || layout.inspector >= 292.0);
                assert!(layout.controls + layout.inspector + layout.canvas <= width);
            }
        }
    }
    let narrow = PaneLayout::for_width(Workspace::Design, 900.0 / 1.30 - RAIL_WIDTH);
    assert!(narrow.collapsed(Drawer::Controls));
    assert!(narrow.collapsed(Drawer::Inspector));
    let medium = PaneLayout::for_width(Workspace::Design, 1100.0 / 1.15 - RAIL_WIDTH);
    assert!(!medium.collapsed(Drawer::Controls));
    assert!(medium.collapsed(Drawer::Inspector));
}

#[test]
fn five_unique_entries_and_command_shortcuts_obey_modal_and_text_focus() {
    assert_eq!(
        ENTRIES.map(|entry| entry.0),
        [
            Workspace::Design,
            Workspace::Stock,
            Workspace::CutPlan,
            Workspace::Hardware,
            Workspace::Handoff
        ]
    );
    let ctx = egui::Context::default();
    let modifiers = egui::Modifiers {
        command: true,
        ..Default::default()
    };
    let key_event = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    for (index, key) in [
        egui::Key::Num1,
        egui::Key::Num2,
        egui::Key::Num3,
        egui::Key::Num4,
        egui::Key::Num5,
    ]
    .into_iter()
    .enumerate()
    {
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![key_event(key, modifiers)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(shortcut(ui.ctx(), false), Some(ENTRIES[index].0));
                assert_eq!(shortcut(ui.ctx(), true), None);
            },
        );
        output.textures_delta.clear();
    }
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![key_event(egui::Key::Num1, egui::Modifiers::default())],
            ..Default::default()
        },
        |ui| assert_eq!(shortcut(ui.ctx(), false), None),
    );
    output.textures_delta.clear();

    let id = egui::Id::new("shell-shortcut-text-focus");
    let mut query = String::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.add(egui::TextEdit::singleline(&mut query).id(id));
        ui.memory_mut(|memory| memory.request_focus(id));
    });
    output.textures_delta.clear();
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![key_event(egui::Key::Num4, modifiers)],
            ..Default::default()
        },
        |ui| {
            assert_eq!(shortcut(ui.ctx(), false), None);
            ui.add(egui::TextEdit::singleline(&mut query).id(id));
        },
    );
    output.textures_delta.clear();
}
