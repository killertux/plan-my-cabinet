use super::*;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::{BoardGrain, Material, StockSource};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::units::{Length, Pose, Quaternion};

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}

fn fixture() -> Project {
    let mut p = Project::new("sheets", Currency::Brl);
    let material = Uuid::new_v4();
    let stock = Uuid::new_v4();
    p.materials.push(Material {
        default_band: None,
        kind: Default::default(),
        id: material,
        name: "ply".into(),
        default_thickness: mm(18),
        default_grain: BoardGrain::Length,
    });
    p.stock.push(Stock {
        id: stock,
        name: "offcut".into(),
        material_id: material,
        length: mm(205),
        width: mm(50),
        thickness: mm(18),
        grain: StockGrain::AlongX,
        source: StockSource::Owned,
        price: None,
        priority: 0,
        trim: [Length::ZERO; 4],
    });
    for x in [0, 105] {
        let id = Uuid::new_v4();
        p.boards.push(Board {
            banding: Default::default(),
            id,
            name: "same name".into(),
            material_id: material,
            length: mm(100),
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([x as f64, 10.0, 0.0], Quaternion::IDENTITY).unwrap(),
        });
        p.allocations.push(Allocation {
            id: Uuid::new_v4(),
            board_id: id,
            stock_id: stock,
            origin: [mm(x), Length::ZERO],
            quarter_turn: false,
            locked: false,
        });
    }
    p
}

#[test]
fn unused_focus_has_no_inherited_witness_metrics_or_placements() {
    let mut project = fixture();
    let mut spare = project.stock[0].clone();
    spare.id = Uuid::new_v4();
    spare.name = "spare blank".into();
    spare.priority = 1;
    let spare_id = spare.id;
    project.stock.push(spare);
    let snapshot = StockReadModel::build(&project).unwrap();
    assert!(snapshot.sheet_cards()[0].proof.cut_count().is_some());
    assert_eq!(snapshot.sheet_cards()[1].proof, SheetProof::Unused);
    assert_eq!(
        card_utilization(Some(&snapshot.sheet_cards()[1].proof)),
        None
    );

    let mut editor = ProjectEditor::new(project).unwrap();
    let mut repair = RepairUi::default();
    let mut selection = Selection::default();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        show(
            ui,
            &mut editor,
            &mut selection,
            &Localizer::new(Language::En),
            false,
            &mut repair,
            SheetFocus {
                sheet: Some(spare_id),
                issue: None,
                scroll_to_target: true,
            },
        );
    });
    let labels: Vec<_> = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
        .collect();
    assert_eq!(repair.focused_sheet, Some(spare_id));
    assert!(
        labels
            .iter()
            .any(|text| text.contains("No placements on this piece")),
        "{labels:?}"
    );
    assert!(
        !labels.iter().any(|text| text.contains("Physical cuts")),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|text| text.contains("Unused")),
        "{labels:?}"
    );
    output.drop_without_applying_deltas();
}

#[test]
fn hidden_multi_reason_board_has_one_contextual_issue_and_repair_route() {
    let mut project = fixture();
    let hidden = project.boards[0].id;
    project.stock[0].thickness = mm(12);
    project.allocations[1].origin[0] = mm(50);
    let mut selection = Selection::default();
    selection.hidden.insert(hidden);
    assert!(!selection.visible(&project, hidden));
    let issues = allocation_diagnostics::diagnose(&project);
    assert_eq!(issues.iter().filter(|d| d.board_id == hidden).count(), 1);
    let issue = issues.iter().find(|d| d.board_id == hidden).unwrap();
    assert!(issue.reasons.contains(&Reason::Thickness));
    assert!(issue.reasons.contains(&Reason::Overlap));
    assert_eq!(
        needs_stock(&issues)
            .filter(|d| d.board_id == hidden)
            .count(),
        1
    );
    assert_eq!(
        issue_resolution(&project, issue),
        Request::with(A::RepairIssue, Target::Board(hidden))
    );
    // The existing LocateIssue route explicitly reveals without moving the board.
    let pose = project.boards[0].pose;
    selection.choose(Some(hidden), false);
    selection.reveal(&project, hidden);
    assert!(selection.visible(&project, hidden));
    assert_eq!(project.boards[0].pose, pose);
}

#[test]
fn stock_shortage_uses_board_effective_thickness_prefill_but_existing_stock_uses_repair() {
    let mut project = fixture();
    let id = project.boards[0].id;
    project.allocations.retain(|a| a.board_id != id);
    project.stock.clear();
    let entry = allocation_diagnostics::diagnose(&project)
        .into_iter()
        .find(|d| d.board_id == id)
        .unwrap();
    assert_eq!(
        issue_resolution(&project, &entry),
        Request::with(A::AddIssueStock, Target::Board(id))
    );
    let mut spare = fixture().stock.remove(0);
    spare.material_id = project.boards[0].material_id;
    project.stock.push(spare);
    assert_eq!(
        issue_resolution(&project, &entry),
        Request::with(A::RepairIssue, Target::Board(id))
    );
    project.stock[0].width = mm(20);
    assert_eq!(
        issue_resolution(&project, &entry),
        Request::with(A::AddIssueStock, Target::Board(id))
    );
    project.stock[0].width = mm(50);
    project.stock[0].grain = StockGrain::Unknown;
    assert!(needs_new_stock(&project, &entry));
}

#[test]
fn repair_target_and_sheet_label_expose_alias_with_uuid_identity() {
    let project = fixture();
    let id = project.stock[0].id;
    let mut editor = ProjectEditor::new(project).unwrap();
    assert_eq!(
        stock_label(editor.project(), &editor.project().stock[0]),
        "O1 · offcut"
    );
    let mut selection = Selection::default();
    selection.choose(Some(editor.project().boards[0].id), false);
    let mut repair = RepairUi::default();
    assert!(repair.begin(&mut editor, &selection, Locale::En));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        show(
            ui,
            &mut editor,
            &mut selection,
            &Localizer::new(Language::En),
            false,
            &mut repair,
            SheetFocus::default(),
        );
    });
    let expected = stock_label(editor.project(), &editor.project().stock[0]);
    let labels: Vec<_> = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
        .collect();
    output.drop_without_applying_deltas();
    assert!(
        labels.iter().any(|label| label.contains(&expected)),
        "{labels:?}"
    );
    // Raw UUIDs never reach the surface; the alias is the identity.
    let raw = id.to_string();
    assert!(
        !labels.iter().any(|label| label.contains(&raw[..8])),
        "{labels:?}"
    );
}

#[test]
fn global_diagnostics_find_hidden_board_and_update_after_repair() {
    use plan_my_cabinet::allocation_diagnostics::{Status, diagnose};
    let mut project = fixture();
    let hidden = project.boards[1].id;
    let parent = Uuid::new_v4();
    project.assemblies.push(plan_my_cabinet::domain::Assembly {
        id: parent,
        name: "hidden group".into(),
        parent_id: None,
        pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
    });
    project.boards[1].parent_id = Some(parent);
    project.allocations.pop();
    let mut selection = Selection::default();
    selection.hidden.insert(parent);
    assert!(!selection.visible(&project, hidden));
    let issues = diagnose(&project);
    assert_eq!(issues.len(), project.boards.len());
    assert_eq!(issues.iter().filter(|d| d.board_id == hidden).count(), 1);
    assert_eq!(
        issues.iter().find(|d| d.board_id == hidden).unwrap().status,
        Status::Unallocated
    );
    let revision = project.revision;
    selection.choose(Some(hidden), false);
    selection.reveal(&project, hidden);
    assert!(selection.visible(&project, hidden));
    assert_eq!(project.revision, revision);

    let stock = project.stock[0].id;
    let mut editor = ProjectEditor::new(project).unwrap();
    let mut repair = RepairUi::default();
    assert!(repair.begin(&mut editor, &selection, Locale::En));
    stage(
        &mut repair,
        &mut editor,
        RepairAction::Place(hidden, stock, [mm(105), Length::ZERO], false),
    );
    assert_eq!(
        diagnose(editor.preview().unwrap())
            .iter()
            .find(|d| d.board_id == hidden)
            .unwrap()
            .status,
        Status::AllocatedValid
    );
    let affected = repair.affected.take().unwrap();
    SheetEditSession::resume(&mut editor, affected)
        .unwrap()
        .accept()
        .unwrap();
    assert!(
        diagnose(editor.project())
            .iter()
            .all(|d| d.status == Status::AllocatedValid)
    );
}

#[test]
fn global_diagnostics_have_one_issue_row_despite_multiple_reasons_and_track_edits() {
    use plan_my_cabinet::allocation_diagnostics::{Reason, Status, diagnose};
    let mut project = fixture();
    let id = project.boards[0].id;
    project.allocations[0].origin[0] = mm(200);
    project.stock[0].thickness = mm(12);
    let issues = diagnose(&project);
    assert_eq!(issues.len(), project.boards.len());
    assert_eq!(issues.iter().filter(|d| d.board_id == id).count(), 1);
    let issue = issues.iter().find(|d| d.board_id == id).unwrap();
    assert_eq!(issue.status, Status::Conflicted);
    assert!(issue.reasons.contains(&Reason::Bounds));
    assert!(issue.reasons.contains(&Reason::Thickness));
    project.allocations[0].origin[0] = mm(0);
    project.stock[0].thickness = mm(18);
    assert!(
        diagnose(&project)
            .iter()
            .all(|d| d.status == Status::AllocatedValid)
    );
}

#[test]
fn global_diagnostics_report_missing_and_duplicate_records_once() {
    use plan_my_cabinet::allocation_diagnostics::{Reason, Status, diagnose};
    let mut project = fixture();
    let first = project.boards[0].id;
    let second = project.boards[1].id;
    project.allocations.pop();
    let mut extra = project.allocations[0].clone();
    extra.id = Uuid::new_v4();
    extra.stock_id = Uuid::new_v4();
    project.allocations.push(extra);
    let issues = diagnose(&project);
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].board_id, first);
    assert_eq!(issues[0].status, Status::Conflicted);
    assert!(issues[0].reasons.contains(&Reason::DuplicateAllocation));
    assert!(issues[0].reasons.contains(&Reason::MissingStock));
    assert_eq!(issues[1].board_id, second);
    assert_eq!(issues[1].status, Status::Unallocated);
}

#[test]
fn staged_transfer_unlock_and_cancel_leave_identity_and_pose_untouched() {
    let mut p = fixture();
    let board = p.boards[0].id;
    let stock = Uuid::new_v4();
    let mut spare = p.stock[0].clone();
    spare.id = stock;
    p.stock.push(spare);
    let mut editor = ProjectEditor::new(p).unwrap();
    let before = editor.project().clone();
    editor.begin_preview();
    let mut repair = RepairUi {
        affected: Some(HashSet::new()),
        ..Default::default()
    };
    stage(&mut repair, &mut editor, RepairAction::Lock(board, true));
    stage(
        &mut repair,
        &mut editor,
        RepairAction::Place(board, stock, [Length::ZERO; 2], false),
    );
    assert_eq!(repair.error, Some("sheet-locked-error"));
    stage(&mut repair, &mut editor, RepairAction::Lock(board, false));
    stage(
        &mut repair,
        &mut editor,
        RepairAction::Place(board, stock, [Length::ZERO; 2], false),
    );
    assert_eq!(editor.preview().unwrap().allocations[0].stock_id, stock);
    assert_eq!(editor.preview().unwrap().boards, before.boards);
    editor.cancel_preview();
    assert_eq!(editor.project(), &before);
}

#[test]
fn transfer_turn_unallocate_and_lock_require_every_affected_sheet_before_one_undo() {
    let mut project = fixture();
    let first = project.boards[0].id;
    let second = project.boards[1].id;
    let source = project.stock[0].id;
    let mut spare = project.stock[0].clone();
    spare.id = Uuid::new_v4();
    spare.priority = 1;
    spare.width = mm(120);
    spare.grain = StockGrain::AlongY;
    let destination = spare.id;
    project.stock.push(spare);
    let mut editor = ProjectEditor::new(project).unwrap();
    let original = editor.project().clone();
    let mut selection = Selection::default();
    selection.choose(Some(first), false);
    let mut repair = RepairUi::default();
    assert!(repair.begin(&mut editor, &selection, Locale::En));

    stage(&mut repair, &mut editor, RepairAction::Lock(first, true));
    assert!(!stage(
        &mut repair,
        &mut editor,
        RepairAction::Place(first, destination, [Length::ZERO; 2], true),
    ));
    assert_eq!(repair.error, Some("sheet-locked-error"));
    stage(&mut repair, &mut editor, RepairAction::Lock(first, false));
    assert!(stage(
        &mut repair,
        &mut editor,
        RepairAction::Place(first, destination, [Length::ZERO; 2], true),
    ));
    stage(&mut repair, &mut editor, RepairAction::Lock(first, true));
    assert!(stage(
        &mut repair,
        &mut editor,
        RepairAction::Place(second, source, [mm(109), Length::ZERO], false),
    ));
    assert_eq!(editor.project(), &original);
    assert_eq!(repair.affected.as_ref().unwrap().len(), 2);
    assert!(!repair.can_accept(&mut editor));
    assert!(repair.accept_navigation(&mut editor).is_err());
    assert!(repair.active());
    assert_eq!(editor.project(), &original);

    assert!(stage(
        &mut repair,
        &mut editor,
        RepairAction::Unallocate(second)
    ));
    assert!(repair.can_accept(&mut editor));
    repair.accept_navigation(&mut editor).unwrap();
    assert!(!repair.active());
    let placed = editor
        .project()
        .allocations
        .iter()
        .find(|a| a.board_id == first)
        .unwrap();
    assert_eq!(placed.stock_id, destination);
    assert!(placed.quarter_turn && placed.locked);
    assert!(
        !editor
            .project()
            .allocations
            .iter()
            .any(|a| a.board_id == second)
    );
    assert_eq!(editor.project().boards, original.boards);
    editor.undo().unwrap();
    assert_eq!(editor.project().allocations, original.allocations);
}

#[test]
fn pending_numeric_input_and_consent_survive_selection_and_block_navigation_accept() {
    let mut project = fixture();
    let first = project.boards[0].id;
    let other = project.boards[1].id;
    let mut spare = project.stock[0].clone();
    spare.id = Uuid::new_v4();
    spare.priority = 1;
    spare.length = mm(250);
    let destination = spare.id;
    project.stock.push(spare);
    let mut editor = ProjectEditor::new(project).unwrap();
    let original = editor.project().clone();
    let mut selection = Selection::default();
    selection.choose(Some(first), false);
    let mut repair = RepairUi::default();
    assert!(repair.begin(&mut editor, &selection, Locale::En));
    repair.stock = Some(destination);
    repair.origin[0] = "1 1/64 in".into();
    repair.placement_dirty = true;
    assert_eq!(coordinate(&repair.origin[0], Unit::Mm, false), None);
    repair.consent[0] = true;
    let rounded = coordinate(&repair.origin[0], Unit::Mm, true).unwrap();
    assert_eq!(rounded.micrometres(), 25_797);
    selection.choose(Some(other), false);
    repair.select(editor.preview().unwrap(), selection.active, Locale::En);
    assert_eq!(repair.board, Some(first));
    assert_eq!(repair.origin[0], "1 1/64 in");
    assert!(repair.consent[0]);
    assert!(!repair.can_accept(&mut editor));
    assert!(repair.accept_navigation(&mut editor).is_err());
    assert_eq!(editor.project(), &original);
    assert!(repair.stage_placement(
        &mut editor,
        first,
        destination,
        [rounded, Length::ZERO],
        false,
    ));
    assert!(!repair.placement_dirty);
    repair.select(editor.preview().unwrap(), selection.active, Locale::En);
    assert_eq!(repair.board, Some(other));
    assert!(repair.can_accept(&mut editor));
    repair.cancel_navigation(&mut editor);
    assert!(!repair.active());
    assert_eq!(editor.project(), &original);
}

#[test]
fn pointer_accept_is_disabled_during_invalid_stage_and_commits_after_repair() {
    let project = fixture();
    let first = project.boards[0].id;
    let source = project.stock[0].id;
    let mut editor = ProjectEditor::new(project).unwrap();
    let original = editor.project().clone();
    let mut selection = Selection::default();
    selection.choose(Some(first), false);
    let mut repair = RepairUi::default();
    assert!(repair.begin(&mut editor, &selection, Locale::En));
    stage(
        &mut repair,
        &mut editor,
        RepairAction::Place(first, source, [mm(100), Length::ZERO], false),
    );
    assert!(!repair.can_accept(&mut editor));
    let ctx = egui::Context::default();
    let localizer = Localizer::new(Language::En);
    let frame =
        |events, editor: &mut ProjectEditor, repair: &mut RepairUi, selection: &mut Selection| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 1000.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    show(
                        ui,
                        editor,
                        selection,
                        &localizer,
                        false,
                        repair,
                        SheetFocus::default(),
                    );
                },
            );
            let accept = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Accept repair" => {
                    Some(text.pos + egui::vec2(5.0, 5.0))
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
            accept.expect("repair accept button drawn")
        };
    let mut accept = frame(vec![], &mut editor, &mut repair, &mut selection);
    let click = |at, pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    accept = frame(
        vec![egui::Event::PointerMoved(accept), click(accept, true)],
        &mut editor,
        &mut repair,
        &mut selection,
    );
    frame(
        vec![click(accept, false)],
        &mut editor,
        &mut repair,
        &mut selection,
    );
    assert!(repair.active());
    assert_eq!(editor.project(), &original);
    assert_eq!(editor.preview().unwrap().allocations[0].origin[0], mm(100));

    stage(
        &mut repair,
        &mut editor,
        RepairAction::Unallocate(original.boards[1].id),
    );
    stage(
        &mut repair,
        &mut editor,
        RepairAction::Place(first, source, [Length::ZERO; 2], false),
    );
    assert!(repair.can_accept(&mut editor));
    accept = frame(vec![], &mut editor, &mut repair, &mut selection);
    accept = frame(
        vec![egui::Event::PointerMoved(accept), click(accept, true)],
        &mut editor,
        &mut repair,
        &mut selection,
    );
    frame(
        vec![click(accept, false)],
        &mut editor,
        &mut repair,
        &mut selection,
    );
    assert!(!repair.active());
    assert!(editor.preview().is_none());
    assert!(
        !editor
            .project()
            .allocations
            .iter()
            .any(|a| a.board_id == original.boards[1].id)
    );
    editor.undo().unwrap();
    assert_eq!(editor.project().allocations, original.allocations);
}

#[test]
fn escape_cancels_dirty_numeric_repair_and_captured_unit_keeps_its_meaning() {
    let mut project = fixture();
    project.display_unit = Unit::Inch;
    let mut editor = ProjectEditor::new(project).unwrap();
    let original = editor.project().clone();
    let mut selection = Selection::default();
    selection.choose(Some(original.boards[0].id), false);
    let mut repair = RepairUi::default();
    assert!(repair.begin(&mut editor, &selection, Locale::En));
    repair.entry_unit = Some(numeric_unit(editor.project()));
    repair.origin[0] = "1.5".into();
    repair.placement_dirty = true;
    assert_eq!(
        coordinate(&repair.origin[0], repair.entry_unit.unwrap(), false),
        Some(Length::from_micrometres(38_100))
    );
    editor.set_display_unit(Unit::Mm);
    assert_eq!(repair.entry_unit, Some(Unit::Inch));
    assert_eq!(
        coordinate(&repair.origin[0], repair.entry_unit.unwrap(), false),
        Some(Length::from_micrometres(38_100))
    );
    assert_ne!(
        coordinate(&repair.origin[0], numeric_unit(editor.project()), false),
        Some(Length::from_micrometres(38_100))
    );
    let ctx = egui::Context::default();
    let frame =
        |events, editor: &mut ProjectEditor, repair: &mut RepairUi, selection: &mut Selection| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 1000.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    show(
                        ui,
                        editor,
                        selection,
                        &Localizer::new(Language::En),
                        false,
                        repair,
                        SheetFocus::default(),
                    );
                },
            );
            output.drop_without_applying_deltas();
        };
    frame(vec![], &mut editor, &mut repair, &mut selection);
    frame(
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        &mut editor,
        &mut repair,
        &mut selection,
    );
    assert!(!repair.active());
    assert!(editor.preview().is_none());
    assert_eq!(editor.project().allocations, original.allocations);
    assert_eq!(editor.project().revision, original.revision);
}

#[test]
fn drag_stages_only_on_release_and_coordinates_require_rounding_consent() {
    let ctx = egui::Context::default();
    let p = fixture();
    let id = p.boards[0].id;
    let original = p.allocations[0].origin;
    let mut editor = ProjectEditor::new(p).unwrap();
    editor.begin_preview();
    let mut repair = RepairUi {
        affected: Some(HashSet::new()),
        ..Default::default()
    };
    let mut started = false;
    let mut stopped = false;
    let mut released_delta = egui::Vec2::ZERO;
    let frame = |events: Vec<egui::Event>,
                 started: &mut bool,
                 stopped: &mut bool,
                 released_delta: &mut egui::Vec2| {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(200.0, 100.0), egui::Sense::click_and_drag());
                ui.painter().rect_filled(rect, 0.0, egui::Color32::RED);
                *started |= response.drag_started();
                *stopped |= response.drag_stopped();
                if response.drag_stopped() {
                    *released_delta =
                        response.interact_pointer_pos().unwrap() - egui::pos2(40.0, 40.0);
                }
            },
        );
        output.drop_without_applying_deltas();
    };
    let pointer = egui::pos2(40.0, 40.0);
    frame(vec![], &mut started, &mut stopped, &mut released_delta);
    frame(
        vec![
            egui::Event::PointerMoved(pointer),
            egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        &mut started,
        &mut stopped,
        &mut released_delta,
    );
    frame(
        vec![egui::Event::PointerMoved(pointer + egui::vec2(30.0, 0.0))],
        &mut started,
        &mut stopped,
        &mut released_delta,
    );
    assert!(started);
    assert!(!stopped);
    assert_eq!(editor.preview().unwrap().allocations[0].origin, original);
    frame(
        vec![egui::Event::PointerButton {
            pos: pointer + egui::vec2(30.0, 0.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        &mut started,
        &mut stopped,
        &mut released_delta,
    );
    assert!(stopped);
    assert_eq!(released_delta.x, 30.0);
    let stock_id = editor_stock(&editor, id);
    stage(
        &mut repair,
        &mut editor,
        RepairAction::Place(
            id,
            stock_id,
            drag_origin(original, released_delta, 2.0),
            false,
        ),
    );
    assert_eq!(editor.preview().unwrap().allocations[0].origin[0], mm(15));
    assert!(coordinate("1/64 in", Unit::Mm, false).is_none());
    assert!(coordinate("1/64 in", Unit::Mm, true).is_some());
}

#[test]
fn sheet_drag_shows_live_conflict_and_valid_witness_without_staging_until_release() {
    let ctx = egui::Context::default();
    let project = fixture();
    let id = project.boards[0].id;
    let mut editor = ProjectEditor::new(project).unwrap();
    let original = editor.project().clone();
    editor.begin_preview();
    let mut repair = RepairUi {
        affected: Some(HashSet::new()),
        ..Default::default()
    };
    let mut selection = Selection::default();
    selection.choose(Some(id), false);
    let localizer = Localizer::new(Language::En);
    macro_rules! frame {
        ($events:expr) => {{
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 1000.0),
                    )),
                    events: $events,
                    ..Default::default()
                },
                |ui| {
                    show(
                        ui,
                        &mut editor,
                        &mut selection,
                        &localizer,
                        false,
                        &mut repair,
                        SheetFocus::default(),
                    );
                },
            );
            let sheet = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == SHEET_FILL => Some(rect.rect),
                _ => None,
            });
            let ghost_colors: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.stroke.width == 3.0 => Some(rect.stroke.color),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            (sheet.unwrap(), ghost_colors)
        }};
    }
    let sheet = frame!(vec![]).0;
    let start = sheet.min + egui::vec2(20.0, 20.0);
    frame!(vec![
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    // At 100 mm the two allocations overlap; at 0 mm the layout is valid.
    let scale = sheet_scale(&editor.project().stock[0], sheet.width(), 280.0);
    let invalid = start + egui::vec2(100.0 * scale, 0.0);
    let (_, colors) = frame!(vec![egui::Event::PointerMoved(invalid)]);
    assert!(colors.contains(&tw::KERF));
    assert_eq!(
        repair.drag.as_ref().unwrap().candidate.unwrap().1,
        DragStatus::Violation(Issue::Overlap)
    );
    assert_eq!(editor.preview().unwrap(), &original);
    assert_eq!(editor.project(), &original);
    let (_, colors) = frame!(vec![egui::Event::PointerMoved(start)]);
    assert!(colors.contains(&tw::OK));
    assert_eq!(
        repair.drag.as_ref().unwrap().candidate.unwrap().1,
        DragStatus::Verified
    );
    assert_eq!(editor.preview().unwrap(), &original);
    frame!(vec![egui::Event::PointerMoved(invalid)]);
    frame!(vec![egui::Event::PointerButton {
        pos: invalid,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(repair.drag.is_none());
    assert_eq!(editor.preview().unwrap().allocations[0].origin[0], mm(100));
    assert_eq!(editor.project(), &original);
    frame!(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(!repair.active());
    assert!(editor.preview().is_none());
    assert_eq!(editor.project(), &original);
}

fn editor_stock(editor: &ProjectEditor, id: Uuid) -> Uuid {
    editor
        .preview()
        .unwrap()
        .allocations
        .iter()
        .find(|a| a.board_id == id)
        .unwrap()
        .stock_id
}

#[test]
fn sheet_click_uses_stable_board_identity_in_shared_selection_without_mutating_pose() {
    let mut project = fixture();
    let before = project.clone();
    let scale = sheet_scale(&project.stock[0], 410.0, 280.0);
    assert_eq!(scale, 2.0);
    let canvas = egui::Rect::from_min_size(egui::pos2(20.0, 30.0), egui::vec2(410.0, 100.0));
    let regions: Vec<_> = project
        .allocations
        .iter()
        .map(|a| {
            (
                allocation_rect(canvas.min, scale, footprint(&project, a).unwrap()),
                a.board_id,
            )
        })
        .collect();
    assert_eq!(regions[1].0.min, egui::pos2(230.0, 30.0));
    let id = hit_board(&regions, canvas, egui::pos2(240.0, 45.0)).unwrap();
    let mut selection = Selection::default();
    choose_sheet_board(&project, &mut selection, id, false);
    assert_eq!(selection.active, Some(project.boards[1].id));
    assert!(selection.ids.contains(&id));
    // A sheet-only origin change is independent from the assembly pose.
    project.allocations[1].origin[0] = mm(102);
    assert_eq!(project.boards, before.boards);
    assert_eq!(project.revision, before.revision);
    assert_eq!(
        hit_board(&regions, canvas, egui::pos2(400.0, 45.0)),
        Some(id)
    );
}

#[test]
fn conflicts_are_derived_for_both_overlap_participants_and_current_witness() {
    let mut p = fixture();
    assert!(diagnostics(&p).is_empty());
    p.allocations[1].origin[0] = mm(102);
    let issues = diagnostics(&p);
    for board in &p.boards {
        assert!(issues[&board.id].contains(&Issue::Kerf));
    }
    p.allocations[1].origin[0] = mm(98);
    let issues = diagnostics(&p);
    for board in &p.boards {
        assert!(issues[&board.id].contains(&Issue::Overlap));
    }
    p.allocations[1].origin[0] = mm(105);
    let other_material = Uuid::new_v4();
    p.materials.push(Material {
        default_band: None,
        kind: Default::default(),
        id: other_material,
        name: "other".into(),
        default_thickness: mm(12),
        default_grain: BoardGrain::Length,
    });
    p.boards[1].material_id = other_material;
    p.boards[1].thickness = mm(12);
    p.allocations[1].quarter_turn = true;
    let issues = diagnostics(&p);
    assert!(issues[&p.boards[1].id].contains(&Issue::Material));
    assert!(issues[&p.boards[1].id].contains(&Issue::Thickness));
    assert!(issues[&p.boards[1].id].contains(&Issue::Bounds));
    // Restore structural material identity to exercise grain independently.
    p.boards[1].material_id = p.materials[0].id;
    assert!(diagnostics(&p)[&p.boards[1].id].contains(&Issue::Grain));
    p.allocations[1].quarter_turn = false;
    p.boards[1].thickness = mm(18);
    assert!(diagnostics(&p).is_empty());
}

#[test]
fn sub_kerf_edge_and_invalid_trim_are_not_shown_as_valid() {
    let mut p = fixture();
    p.stock[0].length = mm(103);
    p.allocations.truncate(1);
    assert!(diagnostics(&p)[&p.boards[0].id].contains(&Issue::Kerf));
    p.stock[0].length = mm(100);
    p.stock[0].trim[0] = mm(2);
    assert!(diagnostics(&p)[&p.boards[0].id].contains(&Issue::Kerf));
}

#[test]
fn conflicted_selected_allocation_paints_dashed_conflict_and_active_outline() {
    let ctx = egui::Context::default();
    let project = fixture();
    let output = ctx.run_ui(Default::default(), |ui| {
        paint_allocation(
            ui,
            egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(80.0, 40.0)),
            &project.boards[0],
            &Localizer::new(Language::En),
            SheetOverlays::default(),
            None,
            PartEmphasis {
                selected: true,
                in_selection: true,
                conflict: true,
            },
        );
    });
    // Conflict: #FBE3E0 fill with a dashed kerf outline at the recorded position.
    assert!(output.shapes.iter().any(|shape| matches!(
        &shape.shape,
        egui::Shape::Rect(rect) if rect.fill == CONFLICT_FILL
            && rect.rect == egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(80.0, 40.0))
    )));
    let dashes = output
        .shapes
        .iter()
        .filter(|shape| {
            matches!(
                &shape.shape,
                egui::Shape::LineSegment { stroke, .. } if stroke.color == tw::KERF
            )
        })
        .count();
    assert!(dashes >= 8, "dashed conflict outline: {dashes}");
    // The active selection keeps its 2px accent outline on top.
    assert!(output.shapes.iter().any(|shape| matches!(
        &shape.shape,
        egui::Shape::Rect(rect) if rect.stroke.color == tw::ACCENT_DARK && rect.stroke.width == 2.0
    )));
    output.drop_without_applying_deltas();
}

#[test]
fn witness_projection_uses_exact_input_span_and_kerf_at_low_zoom() {
    let project = fixture();
    let model = StockReadModel::build(&project).unwrap();
    let (tree, _) = model
        .miniature(project.stock[0].id)
        .unwrap()
        .proof
        .verified()
        .unwrap();
    let operations = tree.operations();
    assert_eq!(operations.len(), tree.cut_count());
    let sheet = egui::Rect::from_min_size(egui::pos2(40.0, 40.0), egui::vec2(205.0, 50.0));
    for (index, operation) in operations.iter().enumerate() {
        assert_eq!(operation.number, index + 1);
        let band = kerf_geometry(tree, operation, sheet.min, 1.0);
        let input = tree_rect(
            sheet.min,
            1.0,
            tree.node(operation.input).unwrap().rectangle,
        );
        let blade = tree.kerf().micrometres() as f32 / 1000.0;
        match operation.axis {
            Axis::X => {
                assert!((band.width() - blade).abs() < 0.001);
                assert_eq!((band.top(), band.bottom()), (input.top(), input.bottom()));
            }
            Axis::Y => {
                assert!((band.height() - blade).abs() < 0.001);
                assert_eq!((band.left(), band.right()), (input.left(), input.right()));
            }
        }
    }
    let ctx = egui::Context::default();
    let output = ctx.run_ui(Default::default(), |ui| {
        let overlays = SheetOverlays::default();
        paint_witness(ui.painter(), sheet, tree, 0.1, overlays, None);
        paint_allocation(
            ui,
            egui::Rect::from_min_size(sheet.min, egui::vec2(8.0, 5.0)),
            &project.boards[0],
            &Localizer::new(Language::En),
            overlays,
            None,
            PartEmphasis {
                selected: false,
                in_selection: false,
                conflict: false,
            },
        );
    });
    let markers = output.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Circle(circle) if circle.fill == tw::PANEL && circle.stroke.color == tw::KERF)).count();
    assert_eq!(markers, operations.len());
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::LineSegment { stroke, .. } if stroke.color == kerf_band() && stroke.width >= 1.6)));
    output.drop_without_applying_deltas();
}

#[test]
fn reference_inspector_accounts_for_witness_and_names_real_cut_outputs() {
    use plan_my_cabinet::reference_fixture::{self, WHITE_STOCK_ID};
    let project = reference_fixture::project();
    let model = StockReadModel::build(&project).unwrap();
    let piece = model.miniature(WHITE_STOCK_ID).unwrap();
    let (tree, accounting) = piece.proof.verified().unwrap();
    let localizer = Localizer::new(Language::En);
    assert_eq!(tree.cut_count(), 9);
    assert_eq!(
        accounting.root_area,
        accounting.part_area
            + accounting.offcut_area
            + accounting.waste_area
            + accounting.kerf_loss
            + accounting.trim_loss
    );
    let crosscut = tree
        .operations()
        .into_iter()
        .find(|cut| cut.axis == Axis::Y && cut.input != tree.root())
        .unwrap();
    let row = cut_row(tree, &crosscut, &localizer, false);
    assert!(row.contains(&format!("Input piece: P{}", crosscut.input)));
    assert!(row.contains(&format!("Low-side output: P{}", crosscut.outputs.first)));
    assert!(row.contains(&format!("High-side output: P{}", crosscut.outputs.second)));
    assert!(row.contains("Reference edge:") && row.contains("Kerf side:"));
    // Shop rows never expose raw UUIDs.
    assert!(!row.contains(&WHITE_STOCK_ID.to_string()[..8]));
    let first = &tree.operations()[0];
    let first_op = cut_operation_text(tree, first, false, &localizer);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = ctx.run_ui(Default::default(), |ui| {
        sheet_inspector(ui, Some(piece), &project, &localizer, None, None);
    });
    let labels: Vec<_> = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
        .collect();
    for expected in [
        "Sheet S1",
        "MDF White 18",
        "BRL 289.90",
        "Utilization",
        "Physical cuts",
        "Reusable offcuts",
        "Kerf loss",
        first_op.as_str(),
        &format!("P{}", crosscut.input),
    ] {
        assert!(
            labels.iter().any(|label| label.contains(expected)),
            "missing {expected}: {labels:?}"
        );
    }
    output.drop_without_applying_deltas();
}

#[test]
fn trim_loss_is_separate_and_unproved_states_have_no_numeric_metrics() {
    let mut project = fixture();
    project.stock[0].length = mm(210);
    project.stock[0].trim = [mm(5), Length::ZERO, Length::ZERO, Length::ZERO];
    project.allocations[0].origin[0] = mm(5);
    project.allocations[1].origin[0] = mm(110);
    let model = StockReadModel::build(&project).unwrap();
    let mut piece = model.pieces[0].clone();
    let (tree, accounting) = piece.proof.verified().unwrap();
    assert_eq!(tree.operations()[0].reference_edge, Edge::High);
    assert_eq!(accounting.trim_loss, 5_000 * 50_000);
    assert_eq!(accounting.kerf_loss, 5_000 * 50_000);
    assert_eq!(area_label(accounting.trim_loss), "0.000250 m²");
    let localizer = Localizer::new(Language::En);
    assert!(cut_row(tree, &tree.operations()[0], &localizer, true).contains("trim pass"));
    for proof in [
        SheetProof::SearchExhausted,
        SheetProof::Violation(ReconstructionViolation::NoSlicing),
    ] {
        piece.proof = proof.clone();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run_ui(Default::default(), |ui| {
            sheet_inspector(ui, Some(&piece), &project, &localizer, None, None);
        });
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
            .collect();
        assert!(
            labels.iter().any(|label| label.contains(
                if matches!(proof, SheetProof::SearchExhausted) {
                    "search limit"
                } else {
                    "Cutting rule violation"
                }
            )),
            "{labels:?}"
        );
        assert!(
            labels
                .iter()
                .any(|label| label.contains("Metrics unverified"))
        );
        assert!(
            !labels
                .iter()
                .any(|label| label.contains("Physical cuts:") || label.contains("Kerf loss:"))
        );
        output.drop_without_applying_deltas();
    }
}

#[test]
fn hovering_sequence_row_highlights_only_its_exact_band_without_selecting() {
    use plan_my_cabinet::reference_fixture::{self, WHITE_STOCK_ID};
    let project = reference_fixture::project();
    let model = StockReadModel::build(&project).unwrap();
    let piece = model.miniature(WHITE_STOCK_ID).unwrap();
    let tree = piece.proof.verified().unwrap().0;
    let localizer = Localizer::new(Language::En);
    let ctx = egui::Context::default();
    let input = |events| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(2000.0, 1600.0),
        )),
        events,
        ..Default::default()
    };
    let second_op = cut_operation_text(tree, &tree.operations()[1], false, &localizer);
    let first = ctx.run_ui(input(vec![]), |ui| {
        sheet_inspector(ui, Some(piece), &project, &localizer, None, None);
    });
    let pointer = first
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == second_op => {
                Some(text.pos + egui::vec2(5.0, 5.0))
            }
            _ => None,
        })
        .expect("second row rendered");
    first.drop_without_applying_deltas();
    let sheet = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(275.0, 183.0));
    let mut hovered = None;
    let second = ctx.run_ui(input(vec![egui::Event::PointerMoved(pointer)]), |ui| {
        hovered = sheet_inspector(ui, Some(piece), &project, &localizer, None, None);
        paint_witness(
            ui.painter(),
            sheet,
            tree,
            0.1,
            SheetOverlays::default(),
            hovered,
        );
    });
    assert_eq!(hovered, Some(2));
    let cut = &tree.operations()[1];
    let band = kerf_geometry(tree, cut, sheet.min, 0.1);
    assert!(second.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Rect(rect) if rect.rect == band && rect.fill == CUT_HIGHLIGHT)));
    assert_eq!(project, reference_fixture::project());
    second.drop_without_applying_deltas();
}

#[test]
fn reference_layout_keeps_hovered_sequence_and_matching_marker_in_one_visible_frame() {
    use plan_my_cabinet::reference_fixture::{self, WHITE_STOCK_ID};
    let mut editor = ProjectEditor::new(reference_fixture::project()).unwrap();
    let original = editor.project().clone();
    let model = StockReadModel::build(editor.project()).unwrap();
    let tree = model
        .miniature(WHITE_STOCK_ID)
        .unwrap()
        .proof
        .verified()
        .unwrap()
        .0;
    let mut repair = RepairUi::default();
    let mut selection = Selection::default();
    let localizer = Localizer::new(Language::En);
    let ctx = egui::Context::default();
    let canvas_width = crate::workspace_shell::PaneLayout::for_width(
        crate::workspace_state::Workspace::CutPlan,
        1440.0 - crate::workspace_shell::RAIL_WIDTH,
    )
    .canvas;
    assert!(canvas_width >= SHEET_SIDE_BY_SIDE_MIN);
    let frame =
        |events, editor: &mut ProjectEditor, repair: &mut RepairUi, selection: &mut Selection| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(
                            canvas_width,
                            900.0
                                - crate::workspace_shell::HEADER_HEIGHT
                                - crate::workspace_shell::STATUS_HEIGHT,
                        ),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            show(
                                ui,
                                editor,
                                selection,
                                &localizer,
                                false,
                                repair,
                                SheetFocus::default(),
                            );
                        },
                    );
                },
            )
        };
    let first_op = cut_operation_text(tree, &tree.operations()[0], false, &localizer);
    let last_op = cut_operation_text(tree, &tree.operations()[8], false, &localizer);
    let first = frame(vec![], &mut editor, &mut repair, &mut selection);
    let row = first
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if text.galley.text() == first_op
                    && shape.clip_rect.contains(text.pos + egui::vec2(5.0, 5.0)) =>
            {
                Some(text.pos + egui::vec2(5.0, 5.0))
            }
            _ => None,
        })
        .expect("first sequence row visible at reference size");
    let marker_visible = |shapes: &[egui::epaint::ClippedShape], color| {
        shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Circle(circle) => {
                circle.fill == color && shape.clip_rect.contains(circle.center)
            }
            _ => false,
        })
    };
    assert!(marker_visible(&first.shapes, tw::PANEL));
    first.drop_without_applying_deltas();
    let second = frame(
        vec![egui::Event::PointerMoved(row)],
        &mut editor,
        &mut repair,
        &mut selection,
    );
    let marker = marker_visible(&second.shapes, CUT_HIGHLIGHT);
    let cut = tree.operations()[0];
    let band = second.shapes.iter().any(|shape| {
        matches!(&shape.shape,
        egui::Shape::Rect(rect) if rect.fill == CUT_HIGHLIGHT
            && shape.clip_rect.intersects(rect.rect) && cut.number == 1)
    });
    assert_eq!(selection.active, None);
    assert_eq!(editor.project(), &original);
    second.drop_without_applying_deltas();
    assert!(
        marker && band,
        "row {row:?}; expected matching cut marker and band in visible canvas"
    );

    let wheel = frame(
        vec![
            egui::Event::PointerMoved(row),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -1500.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        &mut editor,
        &mut repair,
        &mut selection,
    );
    wheel.drop_without_applying_deltas();
    let mut last_row = None;
    for _ in 0..20 {
        let scrolled = frame(
            vec![
                egui::Event::PointerMoved(egui::pos2(canvas_width - 80.0, 600.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -400.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            &mut editor,
            &mut repair,
            &mut selection,
        );
        last_row = scrolled.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if text.galley.text() == last_op
                    && shape.clip_rect.contains(text.pos + egui::vec2(5.0, 5.0)) =>
            {
                Some(text.pos + egui::vec2(5.0, 5.0))
            }
            _ => None,
        });
        scrolled.drop_without_applying_deltas();
        if last_row.is_some() {
            break;
        }
    }
    let last_row = last_row.expect("last row reachable by scrolling only the inspector");
    let last = frame(
        vec![egui::Event::PointerMoved(last_row)],
        &mut editor,
        &mut repair,
        &mut selection,
    );
    let linked = marker_visible(&last.shapes, CUT_HIGHLIGHT);
    last.drop_without_applying_deltas();
    assert!(
        linked,
        "last witness row highlights a still-visible canvas marker"
    );
}

#[test]
fn host_inspector_hover_links_next_frame_without_shrinking_canvas_or_rebuilding_witness() {
    use plan_my_cabinet::reference_fixture::{self, WHITE_STOCK_ID};
    let mut editor = ProjectEditor::new(reference_fixture::project()).unwrap();
    let original = editor.project().clone();
    let mut selection = Selection::default();
    let mut repair = RepairUi::default();
    let ctx = egui::Context::default();
    let localizer = Localizer::new(Language::En);
    let width = crate::workspace_shell::PaneLayout::for_width(
        crate::workspace_state::Workspace::CutPlan,
        1440.0 - crate::workspace_shell::RAIL_WIDTH,
    )
    .canvas;
    assert_eq!(
        sheet_panes(egui::Pos2::ZERO, width, 400.0, true)
            .canvas
            .width(),
        width
    );
    let frame =
        |events, editor: &mut ProjectEditor, repair: &mut RepairUi, selection: &mut Selection| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, 828.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                show_with_layout(
                                    ui,
                                    editor,
                                    selection,
                                    &localizer,
                                    false,
                                    repair,
                                    SheetFocus::default(),
                                    true,
                                );
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(SHEET_INSPECTOR_WIDTH, 828.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt("host-cut-inspector")
                                    .show(ui, |ui| {
                                        show_focused_inspector(ui, editor, &localizer, repair);
                                    });
                            },
                        );
                    });
                },
            )
        };
    let tree = StockReadModel::build(editor.project())
        .unwrap()
        .miniature(WHITE_STOCK_ID)
        .unwrap()
        .proof
        .verified()
        .unwrap()
        .0
        .clone();
    let first_op = cut_operation_text(&tree, &tree.operations()[0], false, &localizer);
    let heading = localizer.text("sheet-sequence-heading").to_uppercase();
    let first = frame(vec![], &mut editor, &mut repair, &mut selection);
    let row = first.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text)
            if text.galley.text() == first_op
                && text.pos.x >= width
                && shape.clip_rect.contains(text.pos + egui::vec2(5.0, 5.0)) =>
        {
            Some(text.pos + egui::vec2(5.0, 5.0))
        }
        _ => None,
    });
    let inspector_headings: Vec<_> = first
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == heading => Some(text.pos.x),
            _ => None,
        })
        .collect();
    first.drop_without_applying_deltas();
    let row = row.expect("host sequence row is visible beside the canvas");
    assert_eq!(inspector_headings.len(), 1);
    assert!(
        inspector_headings[0] >= width,
        "inspector renders only in the host pane"
    );
    let key = repair.stock_model_cache.as_ref().unwrap().0;
    let cached_model = repair
        .stock_model_cache
        .as_ref()
        .unwrap()
        .1
        .as_ref()
        .unwrap() as *const StockReadModel;
    let second = frame(
        vec![egui::Event::PointerMoved(row)],
        &mut editor,
        &mut repair,
        &mut selection,
    );
    second.drop_without_applying_deltas();
    assert_eq!(repair.hovered_cut, Some((WHITE_STOCK_ID, 1)));
    let third = frame(vec![], &mut editor, &mut repair, &mut selection);
    let band = third.shapes.iter().any(|shape| {
        matches!(&shape.shape,
        egui::Shape::Rect(rect) if rect.fill == CUT_HIGHLIGHT
            && shape.clip_rect.intersects(rect.rect))
    });
    third.drop_without_applying_deltas();
    assert!(
        band,
        "host row hover highlights the corresponding full-span band on the next frame"
    );
    assert_eq!(repair.stock_model_cache.as_ref().unwrap().0, key);
    assert_eq!(
        repair
            .stock_model_cache
            .as_ref()
            .unwrap()
            .1
            .as_ref()
            .unwrap() as *const StockReadModel,
        cached_model
    );
    assert_eq!(selection.active, None);
    assert_eq!(editor.project(), &original);
    repair.focused_sheet = Some(reference_fixture::OAK_STOCK_ID);
    let other = frame(vec![], &mut editor, &mut repair, &mut selection);
    let stale_band = other.shapes.iter().any(|shape| {
        matches!(&shape.shape,
        egui::Shape::Rect(rect) if rect.fill == CUT_HIGHLIGHT)
    });
    other.drop_without_applying_deltas();
    assert!(!stale_band, "a cut hover cannot leak to another sheet");
    assert!(
        repair
            .hovered_cut
            .is_none_or(|(id, _)| id == reference_fixture::OAK_STOCK_ID)
    );
}

#[test]
fn focused_sheet_fits_available_pane_and_low_zoom_uses_identity_callouts() {
    let project = plan_my_cabinet::reference_fixture::project();
    let stock = project
        .stock
        .iter()
        .find(|s| s.id == plan_my_cabinet::reference_fixture::WHITE_STOCK_ID)
        .unwrap();
    for (width, height) in [(780.0, 565.0), (420.0, 320.0)] {
        let layout = sheet_panes(egui::pos2(340.0, 180.0), width, height, true);
        let scale = sheet_scale(
            stock,
            layout.canvas.width() - 72.0,
            layout.canvas.height() - 115.0,
        );
        let drawing = egui::vec2(
            stock.length.micrometres() as f32 / 1000.0 * scale,
            stock.width.micrometres() as f32 / 1000.0 * scale,
        );
        assert!(drawing.x + 68.0 <= layout.canvas.width() + 1.0);
        assert!(drawing.y + 56.0 + 35.0 <= layout.canvas.height() + 1.0);
    }
    assert!(full_part_label_fits(egui::vec2(175.0, 80.0), 62.0, 100.0));
    assert!(!full_part_label_fits(egui::vec2(45.0, 18.0), 62.0, 100.0));
}

#[test]
fn compact_panes_stack_without_overlap_and_keep_both_scroll_targets_reachable() {
    let layout = sheet_panes(egui::pos2(10.0, 20.0), 650.0, 280.0, false);
    assert_eq!(layout.canvas.width(), 650.0);
    assert_eq!(layout.inspector.width(), 650.0);
    assert!(layout.canvas.bottom() < layout.inspector.top());
    assert_eq!(layout.bounds.y, layout.inspector.bottom() - 20.0);
    let wide = sheet_panes(egui::pos2(10.0, 20.0), 1120.0, 300.0, false);
    assert_eq!(wide.inspector.width(), SHEET_INSPECTOR_WIDTH);
    assert!(wide.canvas.right() < wide.inspector.left());

    let project = fixture();
    let mut editor = ProjectEditor::new(project.clone()).unwrap();
    let original = editor.project().clone();
    let mut repair = RepairUi::default();
    let mut selection = Selection::default();
    let ctx = egui::Context::default();
    let frame =
        |events, editor: &mut ProjectEditor, repair: &mut RepairUi, selection: &mut Selection| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(650.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    show_with_layout(
                        ui,
                        editor,
                        selection,
                        &Localizer::new(Language::En),
                        false,
                        repair,
                        SheetFocus::default(),
                        false,
                    );
                },
            )
        };
    let output = frame(vec![], &mut editor, &mut repair, &mut selection);
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Rect(rect) if rect.fill == SHEET_FILL)));
    output.drop_without_applying_deltas();
    let wheel = vec![
        egui::Event::PointerMoved(egui::pos2(630.0, 480.0)),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -400.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        },
    ];
    let output = frame(wheel, &mut editor, &mut repair, &mut selection);
    output.drop_without_applying_deltas();
    let output = frame(vec![], &mut editor, &mut repair, &mut selection);
    let inspector_visible = output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text() == "CUT SEQUENCE" && shape.clip_rect.contains(text.pos)));
    output.drop_without_applying_deltas();
    assert!(
        inspector_visible,
        "compact stacked inspector must be reachable by workspace scrolling"
    );
    assert_eq!(editor.project(), &original);
}

#[test]
fn toggles_only_filter_verified_projection_and_conflicts_retain_positions() {
    let mut project = fixture();
    let original = project.clone();
    let model = StockReadModel::build(&project).unwrap();
    let tree = model
        .miniature(project.stock[0].id)
        .unwrap()
        .proof
        .verified()
        .unwrap()
        .0;
    let ctx = egui::Context::default();
    let output = ctx.run_ui(Default::default(), |ui| {
        paint_witness(
            ui.painter(),
            egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(205.0, 50.0)),
            tree,
            1.0,
            SheetOverlays {
                cuts: false,
                offcuts: false,
                grain: false,
                ids: false,
            },
            None,
        );
    });
    assert!(output.shapes.is_empty());
    output.drop_without_applying_deltas();
    assert_eq!(project, original);
    project.allocations[1].origin[0] = mm(102);
    let invalid = StockReadModel::build(&project).unwrap();
    assert!(
        invalid
            .miniature(project.stock[0].id)
            .unwrap()
            .proof
            .verified()
            .is_none()
    );
    let mut editor = ProjectEditor::new(project.clone()).unwrap();
    let before = editor.project().clone();
    let mut repair = RepairUi::default();
    repair.overlays.cuts = false;
    repair.overlays.ids = false;
    let mut selection = Selection::default();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        show(
            ui,
            &mut editor,
            &mut selection,
            &Localizer::new(Language::En),
            false,
            &mut repair,
            SheetFocus::default(),
        );
    });
    let sheet = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == SHEET_FILL => Some(rect.rect),
            _ => None,
        })
        .expect("sheet painted");
    let scale = sheet.width() / 205.0;
    let conflicts: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == CONFLICT_FILL && rect.rect.height() > 11.0 => {
                Some(rect.rect)
            }
            _ => None,
        })
        .collect();
    assert_eq!(editor.project(), &before);
    output.drop_without_applying_deltas();
    // Both overlap participants are drawn as conflicts at their recorded origins.
    assert_eq!(conflicts.len(), 2, "{conflicts:?}");
    assert!(
        conflicts
            .iter()
            .any(|rect| (rect.min.x - (sheet.min.x + 102.0 * scale)).abs() < 0.5),
        "{conflicts:?}"
    );
}

#[test]
fn only_verified_reusable_leaf_offcuts_are_hatched() {
    let mut project = fixture();
    project.stock[0].length = mm(240);
    let model = StockReadModel::build(&project).unwrap();
    let tree = model
        .miniature(project.stock[0].id)
        .unwrap()
        .proof
        .verified()
        .unwrap()
        .0;
    let offcuts: Vec<_> = tree
        .nodes()
        .iter()
        .filter(|node| node.kind == CutKind::Offcut)
        .collect();
    assert!(!offcuts.is_empty());
    let ctx = egui::Context::default();
    let sheet = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(240.0, 50.0));
    let output = ctx.run_ui(Default::default(), |ui| {
        paint_witness(
            ui.painter(),
            sheet,
            tree,
            1.0,
            SheetOverlays {
                cuts: false,
                ..SheetOverlays::default()
            },
            None,
        );
    });
    let filled: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == OFFCUT_FILL => Some(rect.rect),
            _ => None,
        })
        .collect();
    assert_eq!(filled.len(), offcuts.len());
    for node in offcuts {
        assert!(filled.contains(&tree_rect(sheet.min, 1.0, node.rectangle)));
    }
    output.drop_without_applying_deltas();
}

#[test]
fn locked_resize_overlay_updates_on_commit_and_disappears_on_undo() {
    use plan_my_cabinet::board_dimensions::BoardDimension;
    use plan_my_cabinet::units::Anchor;

    let mut project = fixture();
    project.allocations[0].locked = true;
    let original = project.allocations.clone();
    let mut editor = ProjectEditor::new(project).unwrap();
    let id = original[0].board_id;
    let ctx = egui::Context::default();
    let localizer = Localizer::new(Language::En);
    let mut selection = Selection::default();
    let mut repair = RepairUi::default();
    let mut frame = |editor: &mut ProjectEditor| {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| {
                show(
                    ui,
                    editor,
                    &mut selection,
                    &localizer,
                    false,
                    &mut repair,
                    SheetFocus::default(),
                );
            },
        );
        let conflicts = output.shapes.iter().filter(|shape| matches!(
            &shape.shape,
            egui::Shape::Rect(rect) if rect.fill == CONFLICT_FILL && rect.rect.height() > 11.0
        )).count();
        output.drop_without_applying_deltas();
        conflicts
    };
    assert_eq!(frame(&mut editor), 0);
    let preview = editor
        .preview_board_dimension(id, BoardDimension::Length, mm(108), Anchor::Start)
        .unwrap();
    editor.edit_board_dimension(preview).unwrap();
    assert_eq!(editor.project().allocations, original);
    assert_eq!(frame(&mut editor), 2);
    editor.undo().unwrap();
    assert_eq!(frame(&mut editor), 0);
}
