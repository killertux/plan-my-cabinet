//! Keyboard-operable stock form and priority controls; drafts never enter the project.
use super::*;
use crate::actions::{ActionId as A, Argument, Request, Target};
use crate::icons::Icon;
use crate::theme_widgets as tw;
use plan_my_cabinet::cost_estimate::Feasibility;
use plan_my_cabinet::domain::{Stock, StockGrain, StockSource};
use plan_my_cabinet::money::{Money, MoneyLocale};
use plan_my_cabinet::stock_commands::{MAX_STOCK_QUANTITY, StockError, StockField, StockInput};
use plan_my_cabinet::stock_read_models::{
    MaterialStockSummary, StockPieceReadModel, StockReadModel,
};

fn stock_visible_rows<'a>(
    model: &'a StockReadModel,
    filter: Option<Uuid>,
    global_order: bool,
    material_query: &str,
) -> Vec<&'a StockPieceReadModel> {
    let query = material_query.to_lowercase();
    if global_order {
        return model
            .pieces
            .iter()
            .filter(|row| {
                filter.is_none_or(|id| row.material_id == id)
                    && model.materials.iter().any(|material| {
                        material.id == row.material_id
                            && material.name.to_lowercase().contains(&query)
                    })
            })
            .collect();
    }
    model
        .materials
        .iter()
        .filter(|material| {
            filter.is_none_or(|id| material.id == id)
                && material.name.to_lowercase().contains(&query)
        })
        .flat_map(|material| {
            model
                .pieces
                .iter()
                .filter(move |row| row.material_id == material.id)
        })
        .collect()
}

/// The measured outer sheet and its derived usable area. A nonzero trim keeps
/// a visible minimum stroke at small inspector sizes; displayed quantities
/// always come from the exact project lengths rather than this picture.
fn trim_preview_rect(outer: egui::Rect, piece: &StockPieceReadModel) -> egui::Rect {
    let inset = |trim: Length, measured: Length, extent: f32| {
        if trim == Length::ZERO {
            0.0
        } else {
            (trim.micrometres() as f32 / measured.micrometres() as f32 * extent).max(1.0)
        }
    };
    let [left, right, bottom, top] = piece.trim;
    egui::Rect::from_min_max(
        outer.min
            + egui::vec2(
                inset(left, piece.length, outer.width()),
                inset(top, piece.width, outer.height()),
            ),
        outer.max
            - egui::vec2(
                inset(right, piece.length, outer.width()),
                inset(bottom, piece.width, outer.height()),
            ),
    )
}

#[cfg(test)]
mod grouped_stock_tests {
    use super::*;

    #[test]
    fn stock_inventory_precedes_compact_summary_at_reference_and_narrow_widths() {
        for width in [770.0, 510.0] {
            for language in [Language::En, Language::PtBr] {
                let ctx = egui::Context::default();
                let mut app = DesktopApp {
                    editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project())
                        .unwrap(),
                    ..Default::default()
                };
                app.localizer.set_language(language);
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        ui.set_max_width(width);
                        app.show_stock_list(ui);
                    },
                );
                let text = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some((
                            text.galley.text().to_owned(),
                            text.pos.y,
                            text.pos.x,
                            shape.clip_rect,
                        )),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let row = text
                    .iter()
                    .find(|(label, _, _, _)| label == "S1")
                    .expect("first stock row")
                    .1;
                let summary = text
                    .iter()
                    .find(|(label, _, _, _)| *label == app.localizer.text("stock-card-purchase"))
                    .expect("material summary card")
                    .1;
                let title = text
                    .iter()
                    .find(|(label, _, _, _)| *label == app.localizer.text("stock-list"))
                    .expect("stock title")
                    .1;
                assert!(
                    title < row && row < summary,
                    "{language:?} {width}: title {title}, row {row}, summary {summary}"
                );
                // Title, actions, the view toggle and the column header precede
                // the first row; the summary cards must come after the table.
                assert!(
                    row - title < 260.0,
                    "table must start below title/actions, not below cards: {row} - {title}"
                );
                for (label, _, x, clip) in &text {
                    if [
                        "stock-card-purchase",
                        "stock-card-cutting",
                        "stock-card-owned",
                    ]
                    .iter()
                    .any(|key| *label == app.localizer.text(key))
                    {
                        assert!(
                            *x < width && clip.right() <= width,
                            "{language:?} {width}: summary card {label} escaped center pane at {x}, {clip:?}"
                        );
                    }
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn grouped_filter_and_priority_view_preserve_global_ranks_and_identity() {
        let project = plan_my_cabinet::reference_fixture::project();
        let model = StockReadModel::build(&project).unwrap();
        let global = stock_visible_rows(&model, None, true, "");
        assert_eq!(global.len(), project.stock.len());
        assert!(
            global
                .iter()
                .enumerate()
                .all(|(index, row)| row.global_rank == index + 1)
        );
        let grouped = stock_visible_rows(&model, None, false, "");
        let mut global_ids: Vec<_> = global.iter().map(|row| row.id).collect();
        let mut grouped_ids: Vec<_> = grouped.iter().map(|row| row.id).collect();
        global_ids.sort();
        grouped_ids.sort();
        assert_eq!(grouped_ids, global_ids);
        for material in &model.materials {
            let filtered = stock_visible_rows(&model, Some(material.id), false, "");
            assert_eq!(filtered.len(), material.stock_piece_count);
            assert_eq!(
                filtered.iter().map(|row| row.id).collect::<Vec<_>>(),
                stock_visible_rows(&model, Some(material.id), true, "")
                    .iter()
                    .map(|row| row.id)
                    .collect::<Vec<_>>()
            );
            assert!(filtered.iter().all(|row| row.material_id == material.id));
            assert!(filtered.iter().all(|row| {
                global
                    .iter()
                    .any(|same| same.id == row.id && same.global_rank == row.global_rank)
            }));
        }
        let named = &model.materials[0];
        assert_eq!(
            stock_visible_rows(&model, None, false, &named.name.to_uppercase()).len(),
            named.stock_piece_count
        );
        assert!(stock_visible_rows(&model, None, false, "missing material name").is_empty());
        assert_eq!(
            compact_mm(Length::from_micrometres(18_200), Locale::En),
            "18.2"
        );
        assert_eq!(
            compact_mm(Length::from_micrometres(18_200), Locale::PtBr),
            "18,2"
        );
        assert_eq!(
            compact_mm(Length::from_micrometres(900_000), Locale::En),
            "900"
        );
    }

    #[test]
    fn inspector_trim_preview_uses_measured_edges_without_inventing_loss() {
        let mut project = plan_my_cabinet::reference_fixture::project();
        let id = project.stock[0].id;
        let piece = project.stock.iter_mut().find(|p| p.id == id).unwrap();
        piece.trim = [
            Length::from_micrometres(10_000),
            Length::from_micrometres(20_000),
            Length::from_micrometres(30_000),
            Length::from_micrometres(40_000),
        ];
        let model = StockReadModel::build(&project).unwrap();
        let piece = model.pieces.iter().find(|p| p.id == id).unwrap();
        let outer = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(240.0, 120.0));
        let usable = trim_preview_rect(outer, piece);
        let x = piece.length.micrometres() as f32;
        let y = piece.width.micrometres() as f32;
        assert!((usable.left() - (10_000.0 / x * 240.0).max(1.0)).abs() < 0.01);
        assert!((usable.right() - (240.0 - (20_000.0 / x * 240.0).max(1.0))).abs() < 0.01);
        assert!((usable.top() - (40_000.0 / y * 120.0).max(1.0)).abs() < 0.01);
        assert!((usable.bottom() - (120.0 - (30_000.0 / y * 120.0).max(1.0))).abs() < 0.01);
        let mut no_trim = piece.clone();
        no_trim.trim = [Length::ZERO; 4];
        assert_eq!(trim_preview_rect(outer, &no_trim), outer);
        assert_eq!(
            piece.usable_extent[0].micrometres(),
            piece.length.micrometres() - 30_000
        );
        assert_eq!(
            piece.usable_extent[1].micrometres(),
            piece.width.micrometres() - 70_000
        );
    }

    #[test]
    fn inspector_trim_values_stay_in_a_compact_pane_in_both_languages() {
        for language in [Language::En, Language::PtBr] {
            let ctx = egui::Context::default();
            let mut app = DesktopApp {
                editor: ProjectEditor::new(plan_my_cabinet::reference_fixture::project()).unwrap(),
                ..Default::default()
            };
            app.localizer.set_language(language);
            app.session.stock_piece = Some(app.editor.project().stock[0].id);
            let zero = compact_mm(Length::ZERO, locale(&app));
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(316.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    ui.set_max_width(316.0);
                    app.show_stock_inspector(ui);
                },
            );
            let values: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == zero => Some(text.pos.x),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            assert_eq!(
                values.len(),
                4,
                "{language:?}: all trim actions must be present"
            );
            assert!(
                values.iter().all(|x| *x < 316.0),
                "{language:?}: {values:?}"
            );
        }
    }

    #[test]
    fn long_names_and_empty_filter_keep_material_identity_and_table_rows_in_both_locales() {
        use plan_my_cabinet::i18n::Language;
        let mut project = plan_my_cabinet::reference_fixture::project();
        let material_id = project.stock[0].material_id;
        let stock_id = project.stock[0].id;
        let long_material = "Compensado marinho certificado lote número 2026 seção norte";
        let long_stock = "Chapa física identificada no depósito do fundo — bancada leste";
        project
            .materials
            .iter_mut()
            .find(|m| m.id == material_id)
            .unwrap()
            .name = long_material.into();
        project
            .stock
            .iter_mut()
            .find(|p| p.id == stock_id)
            .unwrap()
            .name = long_stock.into();
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        for language in [Language::En, Language::PtBr] {
            app.localizer.set_language(language);
            app.session.stock_material_filter = Some(material_id);
            app.session.stock.filter = "MARINHO".into();
            let model = StockReadModel::build(app.editor.project()).unwrap();
            assert!(
                stock_visible_rows(&model, Some(material_id), false, "MARINHO")
                    .iter()
                    .any(|piece| piece.id == stock_id)
            );
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    app.show_stock_materials(ui);
                    app.show_stock_list(ui);
                },
            );
            let labels = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            let identities = output
                .platform_output
                .accesskit_update
                .as_ref()
                .map(|update| {
                    update
                        .nodes
                        .iter()
                        .filter_map(|(_, node)| node.label().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            output.drop_without_applying_deltas();
            assert!(labels.contains(long_material), "{labels}");
            // The table shows the alias; the piece name is the row's accessible
            // identity and tooltip (never its UUID).
            assert!(
                identities.iter().any(|label| label.contains(long_stock)),
                "{identities:?}"
            );
            assert!(
                !identities
                    .iter()
                    .any(|label| label.contains(&stock_id.to_string()[..8])),
                "{identities:?}"
            );
        }
        app.session.stock.filter = "does not exist".into();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| app.show_stock_list(ui));
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        output.drop_without_applying_deltas();
        assert!(labels.contains(&app.localizer.text("stock-filter-no-materials")));
    }

    #[test]
    fn empty_material_still_has_filter_and_correct_stock_prefill() {
        let mut project = plan_my_cabinet::reference_fixture::project();
        let empty = project.materials[0].id;
        project.stock.retain(|piece| piece.material_id != empty);
        project
            .stock_aliases
            .retain(|id, _| project.stock.iter().any(|piece| piece.id == *id));
        project.allocations.retain(|allocation| {
            project
                .stock
                .iter()
                .any(|piece| piece.id == allocation.stock_id)
        });
        let model = StockReadModel::build(&project).unwrap();
        assert!(stock_visible_rows(&model, Some(empty), false, "").is_empty());
        let summary = model
            .materials
            .iter()
            .find(|material| material.id == empty)
            .unwrap();
        assert_eq!(summary.stock_piece_count, 0);
        let dialog = StockDialog::new_for_material(&project, empty);
        assert_eq!(dialog.material_id, Some(empty));
        assert!(dialog.dimensions[2].value(Unit::Mm).is_ok());
        assert_eq!(
            dialog.dimensions[2].value(Unit::Mm).unwrap(),
            summary.default_thickness
        );
    }

    #[test]
    fn issue_stock_prefill_uses_board_thickness_not_later_material_default() {
        let mut project = plan_my_cabinet::reference_fixture::project();
        let board = project.boards[0].clone();
        let default_thickness = Length::from_micrometres(board.thickness.micrometres() + 1_000);
        project
            .materials
            .iter_mut()
            .find(|material| material.id == board.material_id)
            .unwrap()
            .default_thickness = default_thickness;
        let draft = StockDialog::new_for_issue(&project, board.id);
        assert_eq!(draft.material_id, Some(board.material_id));
        assert_eq!(draft.dimensions[2].value(Unit::Mm), Ok(board.thickness));
        assert_ne!(draft.dimensions[2].value(Unit::Mm), Ok(default_thickness));
    }

    #[test]
    fn needs_stock_action_opens_board_specific_draft_without_mutation() {
        let mut project = plan_my_cabinet::reference_fixture::project();
        let board = project.boards[0].clone();
        project
            .materials
            .iter_mut()
            .find(|material| material.id == board.material_id)
            .unwrap()
            .default_thickness = Length::from_micrometres(board.thickness.micrometres() + 1_000);
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        let before = app.editor.project().clone();
        app.invoke(Request::with(A::AddIssueStock, Target::Board(board.id)))
            .unwrap();
        let draft = app.modals.stock().unwrap();
        assert_eq!(draft.material_id, Some(board.material_id));
        assert_eq!(draft.dimensions[2].value(Unit::Mm), Ok(board.thickness));
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn filtered_and_global_stock_move_actions_preserve_hidden_slots_and_allocations() {
        let project = plan_my_cabinet::reference_fixture::project();
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        app.session = WorkspaceSession::new(app.editor.project());
        app.session.active = Workspace::Stock;
        let initial = app.editor.project().clone();
        let material_id = initial.stock[0].material_id;
        let visible: Vec<_> = initial
            .ordered_stock()
            .iter()
            .filter(|row| row.material_id == material_id)
            .map(|row| row.id)
            .collect();
        assert!(visible.len() >= 2);
        let source = *visible.last().unwrap();
        let global_before: Vec<_> = initial.ordered_stock().iter().map(|row| row.id).collect();
        assert_eq!(
            app.invoke(Request::with(A::StockMove, Target::Stock(source)).argument(
                Argument::StockPriority {
                    target: 0,
                    subset: true
                }
            )),
            Ok(())
        );
        let after: Vec<_> = app
            .editor
            .project()
            .ordered_stock()
            .iter()
            .map(|row| row.id)
            .collect();
        let visible_slots: Vec<_> = global_before
            .iter()
            .enumerate()
            .filter_map(|(index, id)| visible.contains(id).then_some(index))
            .collect();
        assert_eq!(after[visible_slots[0]], source);
        assert!(
            global_before
                .iter()
                .enumerate()
                .all(|(index, id)| visible_slots.contains(&index) || after[index] == *id)
        );
        assert_eq!(app.editor.project().allocations, initial.allocations);
        app.editor.undo().unwrap();
        assert_eq!(
            app.editor
                .project()
                .ordered_stock()
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            global_before
        );
        assert_eq!(
            app.invoke(Request::with(A::StockMove, Target::Stock(source)).argument(
                Argument::StockPriority {
                    target: 0,
                    subset: false
                }
            )),
            Ok(())
        );
        assert_eq!(app.editor.project().ordered_stock()[0].id, source);
        let unchanged = app.editor.project().clone();
        assert!(
            app.invoke(Request::with(A::StockMove, Target::Stock(source)).argument(
                Argument::StockPriority {
                    target: 99,
                    subset: true
                }
            ))
            .is_err()
        );
        assert_eq!(app.editor.project(), &unchanged);
    }

    #[test]
    fn stock_drag_handle_is_exposed_to_accessibility() {
        let project = plan_my_cabinet::reference_fixture::project();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        let output = ctx.run_ui(egui::RawInput::default(), |ui| app.show_stock_list(ui));
        let handles: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                (node.label() == Some("Drag"))
                    .then(|| node.bounds())
                    .flatten()
            })
            .collect();
        output.drop_without_applying_deltas();
        assert_eq!(handles.len(), app.editor.project().stock.len());
    }

    #[test]
    fn pointer_drag_reorders_global_priority_without_moving_allocations() {
        let project = plan_my_cabinet::reference_fixture::project();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        app.session.stock_global_order = true;
        let original = app.editor.project().clone();
        let first = original.ordered_stock()[0].id;
        let second = original.ordered_stock()[1].id;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| app.show_stock_list(ui));
        let mut handles: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                (node.label() == Some("Drag"))
                    .then(|| node.bounds())
                    .flatten()
                    .map(|bounds| {
                        egui::Rect::from_min_max(
                            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                        )
                    })
            })
            .collect();
        output.drop_without_applying_deltas();
        handles.sort_by(|a, b| a.center().y.total_cmp(&b.center().y));
        assert!(handles.len() >= 2);
        let source = handles[1].center();
        let target = handles[0].center();
        let frame = |app: &mut DesktopApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| app.show_stock_list(ui),
            )
            .drop_without_applying_deltas();
        };
        frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(source),
                egui::Event::PointerButton {
                    pos: source,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            &mut app,
            vec![egui::Event::PointerMoved(source + egui::vec2(16.0, 0.0))],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(target)]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.editor.project().ordered_stock()[0].id, second);
        assert_eq!(app.editor.project().ordered_stock()[1].id, first);
        assert_eq!(app.editor.project().allocations, original.allocations);
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().ordered_stock()[0].id, first);
    }

    #[test]
    fn filtered_pointer_drag_permutates_only_visible_global_slots() {
        let project = plan_my_cabinet::reference_fixture::project();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        let original = app.editor.project().clone();
        let material = original.stock[0].material_id;
        let ordered: Vec<_> = original.ordered_stock().iter().map(|row| row.id).collect();
        let visible: Vec<_> = original
            .ordered_stock()
            .iter()
            .filter(|row| row.material_id == material)
            .map(|row| row.id)
            .collect();
        assert!(visible.len() >= 2);
        app.session.stock_material_filter = Some(material);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| app.show_stock_list(ui));
        let mut handles: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                (node.label() == Some("Drag"))
                    .then(|| node.bounds())
                    .flatten()
                    .map(|bounds| {
                        egui::Rect::from_min_max(
                            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                        )
                    })
            })
            .collect();
        output.drop_without_applying_deltas();
        handles.sort_by(|a, b| a.center().y.total_cmp(&b.center().y));
        assert_eq!(handles.len(), visible.len());
        let source = handles[1].center();
        let target = handles[0].center();
        let frame = |app: &mut DesktopApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| app.show_stock_list(ui),
            )
            .drop_without_applying_deltas();
        };
        frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(source),
                egui::Event::PointerButton {
                    pos: source,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            &mut app,
            vec![egui::Event::PointerMoved(source + egui::vec2(16.0, 0.0))],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(target)]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let after: Vec<_> = app
            .editor
            .project()
            .ordered_stock()
            .iter()
            .map(|row| row.id)
            .collect();
        let slots: Vec<_> = ordered
            .iter()
            .enumerate()
            .filter_map(|(index, id)| visible.contains(id).then_some(index))
            .collect();
        assert_eq!(after[slots[0]], visible[1]);
        assert!(
            ordered
                .iter()
                .enumerate()
                .all(|(index, id)| slots.contains(&index) || after[index] == *id)
        );
        assert_eq!(app.editor.project().allocations, original.allocations);
    }

    #[test]
    fn context_menu_moves_piece_to_top_of_global_priority_across_materials() {
        let project = plan_my_cabinet::reference_fixture::project();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        app.session.stock_global_order = true;
        let order: Vec<_> = app
            .editor
            .project()
            .ordered_stock()
            .iter()
            .map(|row| row.id)
            .collect();
        let chosen = order
            .iter()
            .copied()
            .find(|id| {
                app.editor.project().stock.iter().any(|piece| {
                    piece.id == *id
                        && piece.material_id != app.editor.project().stock[0].material_id
                })
            })
            .unwrap();
        let alias = app.editor.project().stock_alias(chosen).unwrap().to_owned();
        let name = app
            .editor
            .project()
            .stock
            .iter()
            .find(|piece| piece.id == chosen)
            .unwrap()
            .name
            .clone();
        let row_label = format!("{alias} · {name}");
        let find = |output: &egui::FullOutput, label: &str| {
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.label() == Some(label))
                        .then(|| node.bounds())
                        .flatten()
                })
                .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
        };
        let screen = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 800.0),
        ));
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: screen,
                ..Default::default()
            },
            |ui| app.show_stock_list(ui),
        );
        let row = find(&output, &row_label).expect("visible stock row");
        output.drop_without_applying_deltas();
        let click = |app: &mut DesktopApp, point: egui::Pos2, button| {
            let mut last = None;
            for events in [
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                vec![egui::Event::PointerButton {
                    pos: point,
                    button,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                vec![],
            ] {
                if let Some(output) = last.take() {
                    egui::FullOutput::drop_without_applying_deltas(output);
                }
                last = Some(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: screen,
                        events,
                        ..Default::default()
                    },
                    |ui| app.show_stock_list(ui),
                ));
            }
            last.unwrap()
        };
        let output = click(&mut app, row, egui::PointerButton::Secondary);
        let item = find(&output, "Move to top").expect("context menu action");
        output.drop_without_applying_deltas();
        click(&mut app, item, egui::PointerButton::Primary).drop_without_applying_deltas();
        assert_eq!(app.editor.project().ordered_stock()[0].id, chosen);
        assert_eq!(
            app.editor.project().stock_alias(chosen),
            Some(alias.as_str())
        );
        app.editor.undo().unwrap();
        assert_eq!(
            app.editor
                .project()
                .ordered_stock()
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            order
        );
    }

    #[test]
    fn stock_inspector_open_in_cut_plan_routes_exact_piece_without_selecting_its_parts() {
        let project = plan_my_cabinet::reference_fixture::project();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        app.session = WorkspaceSession::new(app.editor.project());
        app.session.active = Workspace::Stock;
        let stock = app.editor.project().stock[0].id;
        let board = app.editor.project().boards[0].id;
        app.selection.choose(Some(board), false);
        app.session.stock_piece = Some(stock);
        let revision = app.editor.project().revision;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| app.show_stock_inspector(ui));
        let button = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.label() == Some("Open in cut plan"))
                    .then(|| node.bounds())
                    .flatten()
            })
            .unwrap();
        output.drop_without_applying_deltas();
        let point = egui::pos2(
            ((button.x0 + button.x1) / 2.0) as f32,
            ((button.y0 + button.y1) / 2.0) as f32,
        );
        for events in [
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| app.show_stock_inspector(ui),
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(app.session.active, Workspace::CutPlan);
        assert_eq!(app.session.focused_sheet, Some(stock));
        assert_eq!(app.selection.ids.len(), 1);
        assert_eq!(app.selection.active, Some(board));
        assert_eq!(app.editor.project().revision, revision);
    }
}

pub(super) struct StockDialog {
    project_id: Uuid,
    revision: u64,
    edit_id: Option<Uuid>,
    name: String,
    material_id: Option<Uuid>,
    dimensions: [DimensionDraft; 3],
    trim: [DimensionDraft; 4],
    grain: StockGrain,
    source: StockSource,
    price: String,
    quantity: String,
    error: Option<StockError>,
    chrome: ModalChrome,
}

/// The cut fee draft and its dialog controller.
pub(crate) struct CutFeeDialog {
    pub(crate) text: String,
    chrome: ModalChrome,
}

impl CutFeeDialog {
    pub(crate) fn new(text: String) -> Self {
        Self {
            text,
            chrome: ModalChrome::new(egui::Id::new("cut-fee-dialog"))
                .first_focus(egui::Id::new("cut-fee-amount")),
        }
    }
}

fn locale(app: &DesktopApp) -> Locale {
    if app.localizer.language() == Language::En {
        Locale::En
    } else {
        Locale::PtBr
    }
}

fn compact_mm(value: Length, locale: Locale) -> String {
    let formatted = format_length(value, Unit::Mm, locale, 3);
    let digits = formatted
        .strip_suffix(" mm")
        .expect("millimetre formatter suffix");
    let separator = if locale == Locale::En { '.' } else { ',' };
    digits
        .trim_end_matches('0')
        .trim_end_matches(separator)
        .to_owned()
}

/// Dialog prefill for a stored length. In a millimetre project the field
/// already shows the "mm" suffix, so the text is the bare number.
fn draft_length(project: &Project, value: Length, locale: Locale) -> DimensionDraft {
    DimensionDraft {
        text: if project.display_unit == Unit::Mm {
            compact_mm(value, locale)
        } else {
            format_length(value, Unit::Mm, locale, 3)
        },
        consent: false,
    }
}

fn known_free_fee_text() -> String {
    String::from("0")
}

impl StockDialog {
    pub(super) fn new(project: &Project) -> Self {
        let mut draft = Self::blank(project);
        draft.fill_from_preset(project);
        draft
    }

    fn blank(project: &Project) -> Self {
        Self {
            project_id: project.id,
            revision: project.revision,
            edit_id: None,
            name: String::new(),
            material_id: project.materials.first().map(|m| m.id),
            dimensions: std::array::from_fn(|_| DimensionDraft::new()),
            trim: std::array::from_fn(|_| DimensionDraft {
                text: String::from("0"),
                consent: false,
            }),
            grain: StockGrain::Unknown,
            source: StockSource::Owned,
            price: String::new(),
            quantity: "1".into(),
            error: None,
            chrome: ModalChrome::new(egui::Id::new("stock-dialog"))
                .first_focus(egui::Id::new("stock-dialog-name"))
                .width(540.0),
        }
    }

    /// Pre-fill blank sheet size, grain and ownership from the standard
    /// material preset this material came from, when there is one.
    fn fill_from_preset(&mut self, project: &Project) {
        let Some(material) = self
            .material_id
            .and_then(|id| project.materials.iter().find(|m| m.id == id))
        else {
            return;
        };
        let Some(preset) = plan_my_cabinet::material_presets::preset_for(material) else {
            return;
        };
        let blank =
            self.dimensions[0].text.trim().is_empty() && self.dimensions[1].text.trim().is_empty();
        let from_other_preset = plan_my_cabinet::material_presets::BR_STANDARD
            .iter()
            .any(|p| {
                self.dimensions[0].text == p.sheet_mm[0].to_string()
                    && self.dimensions[1].text == p.sheet_mm[1].to_string()
            });
        if blank || from_other_preset {
            self.dimensions[0] = DimensionDraft {
                text: preset.sheet_mm[0].to_string(),
                consent: false,
            };
            self.dimensions[1] = DimensionDraft {
                text: preset.sheet_mm[1].to_string(),
                consent: false,
            };
            self.dimensions[2] = DimensionDraft {
                text: preset.thickness_mm.to_string(),
                consent: false,
            };
            self.grain = if preset.grain == BoardGrain::Length {
                StockGrain::AlongX
            } else {
                StockGrain::Nondirectional
            };
            self.source = StockSource::ToPurchase;
        }
    }

    pub(super) fn new_for_material(project: &Project, material_id: Uuid) -> Self {
        let mut draft = Self::new(project);
        if let Some(material) = project.materials.iter().find(|m| m.id == material_id) {
            draft.material_id = Some(material.id);
            draft.fill_from_preset(project);
            draft.dimensions[2] = draft_length(project, material.default_thickness, Locale::En);
        }
        draft
    }

    pub(super) fn new_for_issue(project: &Project, board_id: Uuid) -> Self {
        let mut draft = Self::new(project);
        if let Some(board) = project.boards.iter().find(|board| board.id == board_id) {
            draft.material_id = Some(board.material_id);
            draft.fill_from_preset(project);
            draft.dimensions[2] = draft_length(project, board.thickness, Locale::En);
        }
        draft
    }

    pub(super) fn edit(project: &Project, piece: &Stock, locale: Locale) -> Self {
        Self {
            project_id: project.id,
            revision: project.revision,
            edit_id: Some(piece.id),
            name: piece.name.clone(),
            material_id: Some(piece.material_id),
            dimensions: [piece.length, piece.width, piece.thickness]
                .map(|value| draft_length(project, value, locale)),
            // Trims are always entered in millimetres.
            trim: piece.trim.map(|value| DimensionDraft {
                text: compact_mm(value, locale),
                consent: false,
            }),
            grain: piece.grain,
            source: piece.source,
            price: piece.price.map_or(String::new(), |p| {
                format!("{}.{:02}", p.minor_units() / 100, p.minor_units() % 100)
            }),
            quantity: "1".into(),
            error: None,
            chrome: ModalChrome::new(egui::Id::new("stock-dialog"))
                .first_focus(egui::Id::new("stock-dialog-name"))
                .width(540.0),
        }
    }

    fn input(&self, unit: Unit, currency: Currency) -> Result<StockInput, StockError> {
        let fields = [StockField::Length, StockField::Width, StockField::Thickness];
        let values = std::array::from_fn::<_, 3, _>(|index| self.dimensions[index].value(unit));
        let dimensions = std::array::from_fn::<_, 3, _>(|index| {
            values[index].as_ref().copied().unwrap_or(Length::ZERO)
        });
        for (index, value) in values.iter().enumerate() {
            if value.is_err() {
                return Err(StockError::Invalid(fields[index]));
            }
        }
        let mut trim = [Length::ZERO; 4];
        for (index, field) in self.trim.iter().enumerate() {
            trim[index] = trim_value(field).map_err(|_| StockError::Invalid(StockField::Trim))?;
        }
        let price = if self.price.trim().is_empty() {
            None
        } else {
            Some(
                Money::parse(currency, &self.price)
                    .map_err(|_| StockError::Invalid(StockField::Price))?,
            )
        };
        Ok(StockInput {
            name: self.name.clone(),
            material_id: self
                .material_id
                .ok_or(StockError::Invalid(StockField::Material))?,
            length: dimensions[0],
            width: dimensions[1],
            thickness: dimensions[2],
            grain: self.grain,
            source: self.source,
            price,
            trim,
        })
    }
}

// Trims allow zero, unlike board dimensions, but still require consent for grid rounding.
fn trim_value(field: &DimensionDraft) -> Result<Length, InputError> {
    let parsed = parse_length(&field.text, Unit::Mm)?;
    let value = match parsed.conversion {
        Conversion::Exact(value) => value,
        Conversion::NeedsConfirmation(value) if field.consent => value,
        Conversion::NeedsConfirmation(_) => return Err(InputError::Unit(UnitError::InvalidNumber)),
    };
    if value.micrometres() < 0 {
        return Err(InputError::Unit(UnitError::InvalidNumber));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// Presentation helpers (private to the Stock workspace).
// ---------------------------------------------------------------------------

const CHIP_PURCHASE_INK: egui::Color32 = egui::Color32::from_rgb(122, 68, 16);
const WARN_TEXT: egui::Color32 = egui::Color32::from_rgb(92, 62, 16);
const SELECTED_SOFT_INK: egui::Color32 = egui::Color32::from_rgb(138, 84, 24);
const HANDLE_IDLE: egui::Color32 = egui::Color32::from_rgb(191, 181, 165);
const HANDLE_ACTIVE: egui::Color32 = egui::Color32::from_rgb(176, 106, 28);
const ALL_STOCK_SWATCH: egui::Color32 = egui::Color32::from_rgb(225, 219, 207);
const HATCH_FILL: egui::Color32 = egui::Color32::from_rgb(239, 235, 227);
const HATCH_LINE: egui::Color32 = egui::Color32::from_rgb(231, 225, 213);
const HATCH_STROKE: egui::Color32 = egui::Color32::from_rgb(156, 144, 126);
const TRIM_LOSS: egui::Color32 = egui::Color32::from_rgb(226, 218, 203);

/// Table columns: handle, #, ID, measured (fluid), grain, trims, ownership,
/// price, on plan. The fluid column never shrinks below `MIN_FLUID`.
const COLUMNS: [f32; 9] = [30.0, 40.0, 60.0, 0.0, 84.0, 60.0, 108.0, 84.0, 110.0];
const TABLE_PAD: f32 = 10.0;
const MIN_FLUID: f32 = 150.0;
const MATERIAL_FILTER_THRESHOLD: usize = 8;

fn table_min_width() -> f32 {
    COLUMNS.iter().sum::<f32>() + MIN_FLUID + 2.0 * TABLE_PAD
}

fn column_rects(row: egui::Rect) -> [egui::Rect; 9] {
    let fixed: f32 = COLUMNS.iter().sum();
    let fluid = (row.width() - 2.0 * TABLE_PAD - fixed).max(MIN_FLUID);
    let mut x = row.left() + TABLE_PAD;
    std::array::from_fn(|index| {
        let width = if index == 3 { fluid } else { COLUMNS[index] };
        let rect = egui::Rect::from_min_max(
            egui::pos2(x, row.top()),
            egui::pos2(x + width, row.bottom()),
        );
        x += width;
        rect
    })
}

fn paint_in(
    painter: &egui::Painter,
    cell: egui::Rect,
    align: egui::Align2,
    text: impl ToString,
    font: egui::FontId,
    color: egui::Color32,
) {
    let anchor = match align.x() {
        egui::Align::Min => cell.left_center(),
        egui::Align::Center => cell.center(),
        egui::Align::Max => cell.right_center() - egui::vec2(8.0, 0.0),
    };
    painter
        .with_clip_rect(cell.intersect(painter.clip_rect()))
        .text(anchor, align, text, font, color);
}

/// Two columns of three dots (the ⋮⋮ drag grip).
fn paint_grip(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    for dx in [-2.5, 2.5] {
        for dy in [-4.0, 0.0, 4.0] {
            painter.circle_filled(center + egui::vec2(dx, dy), 1.2, color);
        }
    }
}

fn paint_chip(
    painter: &egui::Painter,
    left_center: egui::Pos2,
    text: &str,
    fill: egui::Color32,
    ink: egui::Color32,
) {
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(11.0), ink);
    let size = galley.size() + egui::vec2(16.0, 5.0);
    let rect = egui::Rect::from_min_size(
        egui::pos2(left_center.x, left_center.y - size.y / 2.0),
        size,
    );
    painter.rect_filled(rect, 9.0, fill);
    painter.galley(rect.center() - galley.size() / 2.0, galley, ink);
}

fn tracked_job(
    ui: &egui::Ui,
    text: &str,
    size: f32,
    color: egui::Color32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: tw::weighted_font(ui, size, crate::theme::Typeface::SansSemibold),
            color,
            extra_letter_spacing: size * 0.06,
            ..Default::default()
        },
    );
    job
}

fn money_amount(money: Money, locale: MoneyLocale) -> String {
    let text = money.display(locale);
    text.split_once(' ')
        .map_or_else(|| text.clone(), |(_, amount)| amount.to_owned())
}

fn usage_fraction(piece: &StockPieceReadModel) -> Option<f32> {
    piece
        .proof
        .utilization()
        .filter(|(_, root)| *root > 0)
        .map(|(part, root)| (part as f64 / root as f64).clamp(0.0, 1.0) as f32)
}

fn sum_trims(piece: &StockPieceReadModel) -> Length {
    piece.trim.iter().copied().fold(Length::ZERO, |sum, trim| {
        Length::from_micrometres(sum.micrometres() + trim.micrometres())
    })
}

fn stock_grain_short_key(grain: StockGrain) -> &'static str {
    match grain {
        StockGrain::AlongX => "stock-grain-short-x",
        StockGrain::AlongY => "stock-grain-short-y",
        StockGrain::Nondirectional => "stock-grain-short-none",
        StockGrain::Unknown => "stock-grain-short-unknown",
    }
}

fn stock_grain_column_key(grain: StockGrain) -> &'static str {
    match grain {
        StockGrain::AlongX => "stock-grain-col-x",
        StockGrain::AlongY => "stock-grain-col-y",
        StockGrain::Nondirectional => "stock-grain-short-none",
        StockGrain::Unknown => "stock-grain-short-unknown",
    }
}

/// Equal-width segmented control. Returns the newly picked value, if any;
/// committing it is the caller's decision.
fn full_segmented<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: egui::Id,
    value: T,
    options: &[(T, String)],
    enabled: bool,
) -> Option<T> {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 7.0, tw::VIEWPORT);
    let inner = rect.shrink(2.0);
    let count = options.len().max(1) as f32;
    let segment = (inner.width() - 2.0 * (count - 1.0)) / count;
    let mut picked = None;
    for (index, (candidate, label)) in options.iter().enumerate() {
        let r = egui::Rect::from_min_size(
            inner.min + egui::vec2(index as f32 * (segment + 2.0), 0.0),
            egui::vec2(segment, inner.height()),
        );
        let response = ui.interact(
            r,
            id.with(index),
            if enabled {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        let selected = *candidate == value;
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label)
        });
        let painter = ui.painter();
        if selected {
            painter.rect_filled(r.translate(egui::vec2(0.0, 1.0)), 5.0, tw::BORDER_SOFT);
            painter.rect_filled(r, 5.0, tw::PANEL);
        } else if enabled && response.hovered() {
            painter.rect_filled(r, 5.0, egui::Color32::from_rgb(228, 223, 214));
        }
        if response.has_focus() {
            painter.rect_stroke(
                r,
                5.0,
                egui::Stroke::new(1.0, tw::FOCUS),
                egui::StrokeKind::Inside,
            );
        }
        let font = if selected {
            tw::weighted_font(ui, 12.0, crate::theme::Typeface::SansMedium)
        } else {
            egui::FontId::proportional(12.0)
        };
        let color = if !enabled {
            tw::DISABLED
        } else if selected {
            tw::TEXT
        } else {
            tw::SECONDARY
        };
        paint_in(painter, r, egui::Align2::CENTER_CENTER, label, font, color);
        if response.clicked() && !selected {
            picked = Some(*candidate);
        }
    }
    picked
}

/// Read-only value box styled like an input; clicking opens the edit dialog.
#[allow(clippy::too_many_arguments)]
fn value_box(
    ui: &mut egui::Ui,
    id: egui::Id,
    accessible_name: &str,
    value: &str,
    muted_value: bool,
    tail: Option<&str>,
    centered: bool,
    width: f32,
    enabled: bool,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::hover());
    let response = ui.interact(
        rect,
        id,
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, accessible_name)
    });
    let hovered = enabled && response.hovered();
    let painter = ui.painter();
    painter.rect(
        rect,
        6.0,
        if hovered { tw::CARD } else { tw::APP },
        egui::Stroke::new(
            1.0,
            if response.has_focus() {
                tw::FOCUS
            } else if hovered {
                tw::BORDER_STRONG
            } else {
                tw::BORDER_SOFT
            },
        ),
        egui::StrokeKind::Inside,
    );
    let value_color = if muted_value { tw::FAINT } else { tw::TEXT };
    let value_font = egui::FontId::monospace(12.5);
    if centered {
        let value_galley = painter.layout_no_wrap(value.to_owned(), value_font, value_color);
        let tail_galley = tail.map(|tail| {
            painter.layout_no_wrap(tail.to_owned(), egui::FontId::proportional(11.0), tw::FAINT)
        });
        let total = value_galley.size().x
            + tail_galley
                .as_ref()
                .map_or(0.0, |galley| galley.size().x + 4.0);
        let mut x = rect.center().x - total / 2.0;
        let y = rect.center().y;
        let vw = value_galley.size();
        painter.galley(egui::pos2(x, y - vw.y / 2.0), value_galley, value_color);
        x += vw.x + 4.0;
        if let Some(galley) = tail_galley {
            let size = galley.size();
            painter.galley(egui::pos2(x, y - size.y / 2.0), galley, tw::FAINT);
        }
    } else {
        let clip = rect.shrink2(egui::vec2(8.0, 0.0));
        paint_in(
            painter,
            clip,
            egui::Align2::LEFT_CENTER,
            value,
            value_font,
            value_color,
        );
        if let Some(tail) = tail {
            painter.text(
                rect.right_center() - egui::vec2(8.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                tail,
                egui::FontId::proportional(12.0),
                tw::FAINT,
            );
        }
    }
    if enabled {
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(accessible_name)
    } else {
        response
    }
}

fn field_label(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.label(egui::RichText::new(text).size(12.0).color(tw::MUTED))
}

/// `unit_field` sizes its text to the available width and then adds its frame
/// margins; keep the whole field inside narrow dialog columns.
fn fitted_unit_field(
    ui: &mut egui::Ui,
    id: egui::Id,
    label: &str,
    text: &mut String,
    unit: &str,
    error: Option<&str>,
) -> egui::Response {
    ui.scope(|ui| {
        ui.set_max_width((ui.available_width() - 18.0).max(40.0));
        tw::unit_field(ui, id, label, text, unit, error)
    })
    .inner
}

/// Length input used by the stock dialog: label above, unit suffix, inline
/// error and an explicit rounding consent when the value needs it.
#[allow(clippy::too_many_arguments)]
fn stock_length_field(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    id: &'static str,
    label: &str,
    field: &mut DimensionDraft,
    unit: Unit,
    suffix: &str,
    trim: bool,
) {
    field_label(ui, label);
    let parsed: Result<Conversion, InputError> = if trim {
        parse_length(&field.text, Unit::Mm).and_then(|parsed| {
            if parsed.conversion.suggested().micrometres() < 0 {
                Err(InputError::Unit(UnitError::InvalidNumber))
            } else {
                Ok(parsed.conversion)
            }
        })
    } else {
        parse_length(&field.text, unit)
            .and_then(|parsed| dimension(parsed.conversion).map_err(InputError::Unit))
    };
    let error = match (&parsed, trim) {
        (Err(error), false) if !field.text.is_empty() => Some(localizer.text(error_key(*error))),
        _ => None,
    };
    if fitted_unit_field(
        ui,
        egui::Id::new(id),
        label,
        &mut field.text,
        suffix,
        error.as_deref(),
    )
    .changed()
    {
        field.consent = false;
    }
    if let Ok(Conversion::NeedsConfirmation(value)) = parsed {
        let locale = if localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let mut args = FluentArgs::new();
        args.set("entered", field.text.as_str());
        args.set("rounded", format_length(value, Unit::Mm, locale, 3));
        ui.checkbox(
            &mut field.consent,
            egui::RichText::new(localizer.format("rounding-confirmation", Some(&args))).size(11.5),
        );
    }
}

/// A deferred UI decision; applied after the frame's widgets are drawn.
enum StockUiAction {
    Request(Request),
    Duplicate(Uuid),
    Delete(Uuid),
    Grain(Uuid, StockGrain),
    Source(Uuid, StockSource),
    OpenSheet(Uuid),
    OpenBoard(Uuid),
}

enum TableItem<'a> {
    Header(&'a MaterialStockSummary),
    Row {
        piece: &'a StockPieceReadModel,
        /// Zero-based target in the move scope (material subset or global).
        target: usize,
        scope_len: usize,
        shown_rank: usize,
    },
    Warn(&'a MaterialStockSummary),
}

impl DesktopApp {
    fn stock_snapshot(&mut self) -> Option<StockReadModel> {
        let project = self.editor.project();
        let key = (project.id, project.revision);
        if self
            .design_stock_snapshot
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            self.design_stock_snapshot = StockReadModel::build(project)
                .ok()
                .map(|model| (key, model));
        }
        self.design_stock_snapshot
            .as_ref()
            .map(|(_, model)| model.clone())
    }

    fn apply_stock_action(&mut self, action: StockUiAction) {
        match action {
            StockUiAction::Request(request) => {
                self.invoke_or_report(request);
            }
            StockUiAction::Duplicate(id) => {
                self.invoke_or_report(Request::with(A::DuplicateStock, Target::Stock(id)));
            }
            StockUiAction::Delete(id) => {
                self.invoke_or_report(Request::with(A::DeleteStock, Target::Stock(id)));
            }
            StockUiAction::Grain(id, grain) => {
                self.commit_stock_field(id, |input| input.grain = grain)
            }
            StockUiAction::Source(id, source) => {
                self.commit_stock_field(id, |input| input.source = source)
            }
            StockUiAction::OpenSheet(id) => {
                let _ = self.request_navigation(NavigationRoute::Entity(Destination::Sheet(id)));
            }
            StockUiAction::OpenBoard(id) => {
                let _ = self.request_navigation(NavigationRoute::Entity(Destination::Board(id)));
            }
        }
    }

    /// One atomic, undoable stock edit through the same command the dialog uses.
    fn commit_stock_field(&mut self, id: Uuid, change: impl FnOnce(&mut StockInput)) {
        let Some(stock) = self.editor.project().stock.iter().find(|s| s.id == id) else {
            return;
        };
        let mut input = StockInput::from(stock);
        change(&mut input);
        if self.editor.edit_stock(id, input).is_ok() {
            self.material_conflicts = allocation_conflicts(self.editor.project());
        }
    }

    fn stock_edit_allowed(&self, id: Uuid) -> bool {
        self.action_availability(Request::with(A::EditStock, Target::Stock(id)))
            .is_ok()
    }

    fn material_line(&self, material: &MaterialStockSummary) -> String {
        let mut args = FluentArgs::new();
        args.set(
            "thickness",
            compact_mm(material.default_thickness, locale(self)),
        );
        args.set("boards", material.board_count);
        args.set("pieces", material.stock_piece_count);
        self.localizer.format("stock-material-line", Some(&args))
    }

    // -----------------------------------------------------------------------
    // Left pane: materials
    // -----------------------------------------------------------------------

    pub(super) fn show_stock_materials(&mut self, ui: &mut egui::Ui) {
        let modal = self.modal_open();
        let Some(model) = self.stock_snapshot() else {
            ui.label(self.localizer.text("cost-invalid"));
            return;
        };
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 6,
                right: 0,
                top: 0,
                bottom: 0,
            })
            .show(ui, |ui| {
                tw::section_bar(ui, &self.localizer.text("stock-materials-heading"), |ui| {
                    let request = Request::new(A::NewMaterial);
                    if tw::ghost_icon_sized(
                        ui,
                        Icon::Plus,
                        &A::NewMaterial.label(&self.localizer),
                        tw::MUTED,
                        15.0,
                        24.0,
                        self.action_availability(request).is_ok(),
                        false,
                    )
                    .clicked()
                    {
                        self.invoke_or_report(request);
                    }
                });
            });
        if model.materials.len() > MATERIAL_FILTER_THRESHOLD
            || !self.session.stock.filter.is_empty()
        {
            egui::Frame::new()
                .fill(tw::APP)
                .stroke(egui::Stroke::new(1.0, tw::BORDER_SOFT))
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(8, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(icons::icon(Icon::Search, tw::FAINT, 13.0));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.session.stock.filter)
                                .frame(egui::Frame::NONE)
                                .hint_text(self.localizer.text("shell-filter-materials"))
                                .desired_width(ui.available_width()),
                        );
                    });
                });
            ui.add_space(4.0);
        }
        ui.spacing_mut().item_spacing.y = 2.0;
        let all_selected = self.session.stock_material_filter.is_none();
        let all_label = self.localizer.text("stock-all-materials");
        let mut args = FluentArgs::new();
        args.set("pieces", model.pieces.len());
        args.set("materials", model.materials.len());
        let all_detail = self.localizer.format("stock-all-line", Some(&args));
        let (all, ()) = tw::list_row(
            ui,
            egui::Id::new("stock-material-all"),
            44.0,
            if all_selected {
                tw::RowState::Active
            } else {
                tw::RowState::Normal
            },
            !modal,
            &all_label,
            |ui| {
                tw::swatch(ui, ALL_STOCK_SWATCH, egui::vec2(10.0, 28.0));
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(tw::medium(ui, &all_label, 13.0).color(if all_selected {
                        tw::ACCENT_INK
                    } else {
                        tw::TEXT
                    }));
                    ui.label(egui::RichText::new(&all_detail).size(11.0).color(tw::FAINT));
                });
            },
        );
        if all.clicked() {
            self.session.stock_material_filter = None;
            self.session.inspector = None;
        }
        let query = self.session.stock.filter.to_lowercase();
        let mut pending = None;
        for material in model
            .materials
            .iter()
            .filter(|material| material.name.to_lowercase().contains(&query))
        {
            let selected = self.session.stock_material_filter == Some(material.id);
            let needs_stock = material.board_count > 0 && material.stock_piece_count == 0;
            let color = self.editor.project().material_color(material.id);
            let swatch = egui::Color32::from_rgb(color.0[0], color.0[1], color.0[2]);
            let line = self.material_line(material);
            let (row, ()) = tw::list_row(
                ui,
                egui::Id::new(("stock-material-row", material.id)),
                44.0,
                if selected {
                    tw::RowState::Active
                } else {
                    tw::RowState::Normal
                },
                !modal,
                &material.name,
                |ui| {
                    tw::swatch(ui, swatch, egui::vec2(10.0, 28.0));
                    ui.add_space(4.0);
                    let text_width =
                        (ui.available_width() - if needs_stock { 22.0 } else { 0.0 }).max(40.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(text_width, 34.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.set_width(text_width);
                            ui.add(
                                egui::Label::new(
                                    tw::medium(ui, &material.name, 13.0).color(if selected {
                                        tw::ACCENT_INK
                                    } else {
                                        tw::TEXT
                                    }),
                                )
                                .truncate()
                                .selectable(false),
                            );
                            ui.add(
                                egui::Label::new(tw::mono(&line, 11.0).color(if selected {
                                    SELECTED_SOFT_INK
                                } else {
                                    tw::FAINT
                                }))
                                .truncate()
                                .selectable(false),
                            );
                        },
                    );
                    if needs_stock {
                        ui.add(icons::icon(Icon::Warning, tw::WARN, 14.0))
                            .on_hover_text(self.localizer.text("stock-material-without-stock"));
                    }
                },
            );
            if row.clicked() {
                self.session.stock_material_filter = Some(material.id);
                self.session.stock_global_order = false;
                self.session.inspector = Some(InspectorTarget::Material(material.id));
                if self.session.stock_piece.is_some_and(|id| {
                    model
                        .pieces
                        .iter()
                        .any(|piece| piece.id == id && piece.material_id != material.id)
                }) {
                    self.session.stock_piece = None;
                }
            }
            row.context_menu(|ui| {
                if ui
                    .add_enabled(
                        !modal,
                        egui::Button::new(self.localizer.text("material-edit-ellipsis")),
                    )
                    .clicked()
                {
                    pending = Some(Request::with(
                        A::EditMaterial,
                        Target::Material(material.id),
                    ));
                    ui.close();
                }
                if ui
                    .add_enabled(!modal, egui::Button::new(self.localizer.text("stock-new")))
                    .clicked()
                {
                    pending = Some(Request::with(A::NewStock, Target::Material(material.id)));
                    ui.close();
                }
            });
        }
        if let Some(request) = pending {
            self.invoke_or_report(request);
        }
        if !model.materials.is_empty()
            && !model
                .materials
                .iter()
                .any(|material| material.name.to_lowercase().contains(&query))
        {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(self.localizer.text("stock-filter-no-materials"))
                    .size(12.0)
                    .color(tw::FAINT),
            );
        }
        ui.add_space(12.0);
        tw::divider(ui);
        let summary = self.session.stock_material_filter.or_else(|| {
            self.session.stock_piece.and_then(|id| {
                model
                    .pieces
                    .iter()
                    .find(|piece| piece.id == id)
                    .map(|piece| piece.material_id)
            })
        });
        if let Some(material) =
            summary.and_then(|id| model.materials.iter().find(|material| material.id == id))
        {
            self.show_material_summary(ui, material, modal);
        }
    }

    fn show_material_summary(
        &mut self,
        ui: &mut egui::Ui,
        material: &MaterialStockSummary,
        modal: bool,
    ) {
        let grain = self
            .editor
            .project()
            .materials
            .iter()
            .find(|m| m.id == material.id)
            .map(|m| m.default_grain);
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 6,
                right: 4,
                top: 4,
                bottom: 12,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                tw::inspector_heading(
                    ui,
                    &format!("{} · {}", self.localizer.text("material"), material.name),
                    |_| {},
                );
                tw::prop_row(ui, &self.localizer.text("board-thickness"), 96.0, |ui| {
                    ui.label(
                        tw::mono(
                            format!(
                                "{} mm",
                                compact_mm(material.default_thickness, locale(self))
                            ),
                            12.5,
                        )
                        .color(tw::TEXT),
                    );
                });
                if let Some(grain) = grain {
                    tw::prop_row(
                        ui,
                        &self.localizer.text("material-default-grain"),
                        96.0,
                        |ui| {
                            ui.label(self.localizer.text(match grain {
                                BoardGrain::Length => "grain-length",
                                BoardGrain::Width => "grain-width",
                                BoardGrain::Unrestricted => "grain-unrestricted",
                            }));
                        },
                    );
                }
                let mut args = FluentArgs::new();
                args.set("count", material.board_count);
                tw::prop_row(ui, &self.localizer.text("material-used-by"), 96.0, |ui| {
                    ui.label(self.localizer.format("stock-boards-n", Some(&args)));
                });
                ui.add_space(8.0);
                let request = Request::with(A::EditMaterial, Target::Material(material.id));
                let label = self.localizer.text("material-edit-ellipsis");
                if ui
                    .with_layout(
                        egui::Layout::top_down_justified(egui::Align::Center),
                        |ui| {
                            ui.add_enabled(
                                !modal && self.action_availability(request).is_ok(),
                                egui::Button::new(tw::medium(ui, &label, 13.0).color(tw::TEXT))
                                    .fill(tw::VIEWPORT)
                                    .stroke(egui::Stroke::new(1.0, tw::BORDER_SOFT))
                                    .corner_radius(7)
                                    .min_size(egui::vec2(ui.available_width(), 30.0)),
                            )
                        },
                    )
                    .inner
                    .clicked()
                {
                    self.invoke_or_report(request);
                }
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(self.localizer.text("material-edit-note"))
                        .size(11.0)
                        .color(tw::FAINT),
                );
            });
    }

    // -----------------------------------------------------------------------
    // Centre: title, stock table, spending summary
    // -----------------------------------------------------------------------

    fn show_stock_title(&mut self, ui: &mut egui::Ui, modal: bool) {
        let title = |app: &Self, ui: &mut egui::Ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.label(tw::semibold(ui, app.localizer.text("stock-list"), 20.0).color(tw::TEXT));
            ui.label(
                egui::RichText::new(app.localizer.text("stock-subtitle"))
                    .size(12.5)
                    .color(tw::MUTED),
            );
        };
        let mut action = None;
        let actions = |app: &Self, ui: &mut egui::Ui, action: &mut Option<Request>| {
            // Right-to-left: the primary action sits at the far right.
            if tw::icon_text_button(
                ui,
                Icon::Plus,
                &app.localizer.text("stock-new"),
                true,
                !modal && app.action_availability(Request::new(A::NewStock)).is_ok(),
            )
            .clicked()
            {
                *action = Some(Request::new(A::NewStock));
            }
            if app.fee_chip(ui, modal).clicked() {
                *action = Some(Request::new(A::EditCutFee));
            }
        };
        if ui.available_width() >= 640.0 {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 52.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    actions(self, ui, &mut action);
                    ui.add_space(6.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 52.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| title(self, ui),
                    );
                },
            );
        } else {
            ui.vertical(|ui| title(self, ui));
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 34.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        actions(self, ui, &mut action)
                    });
                },
            );
        }
        if let Some(request) = action {
            self.invoke_or_report(request);
        }
    }

    fn fee_chip(&self, ui: &mut egui::Ui, modal: bool) -> egui::Response {
        let fee = self.editor.project().cut_fee;
        let enabled = !modal
            && self
                .action_availability(Request::new(A::EditCutFee))
                .is_ok();
        let painter = ui.painter().clone();
        let label = painter.layout_no_wrap(
            self.localizer.text("stock-cut-fee-chip"),
            egui::FontId::proportional(13.0),
            tw::MUTED,
        );
        let value = match fee {
            Some(fee) => painter.layout_no_wrap(
                fee.display(self.stock_money_locale()),
                egui::FontId::monospace(12.5),
                tw::TEXT,
            ),
            None => painter.layout_no_wrap(
                self.localizer.text("stock-fee-unknown"),
                egui::FontId::proportional(13.0),
                tw::WARN_INK,
            ),
        };
        let link = painter.layout_no_wrap(
            self.localizer.text(if fee.is_some() {
                "stock-fee-change"
            } else {
                "stock-fee-set"
            }),
            egui::FontId::proportional(12.0),
            if enabled {
                tw::ACCENT_DARK
            } else {
                tw::DISABLED
            },
        );
        let width = 12.0 + label.size().x + 8.0 + value.size().x + 8.0 + link.size().x + 12.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::hover());
        let name = A::EditCutFee.label(&self.localizer);
        let response = ui.interact(
            rect,
            egui::Id::new("stock-cut-fee-chip"),
            if enabled {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            },
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &name));
        painter.rect(
            rect,
            8.0,
            if enabled && response.hovered() {
                tw::CARD
            } else {
                tw::PANEL
            },
            egui::Stroke::new(
                1.0,
                if enabled && response.hovered() {
                    tw::BORDER_STRONG
                } else {
                    tw::BORDER_SOFT
                },
            ),
            egui::StrokeKind::Inside,
        );
        let mut x = rect.left() + 12.0;
        for galley in [label, value, link] {
            let size = galley.size();
            painter.galley(
                egui::pos2(x, rect.center().y - size.y / 2.0),
                galley,
                tw::TEXT,
            );
            x += size.x + 8.0;
        }
        response
            .on_hover_text(self.localizer.text("cut-fee-hint"))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    fn missing_stock_reason(&self, material: &MaterialStockSummary) -> String {
        let project = self.editor.project();
        let waiting: Vec<_> = project
            .boards
            .iter()
            .filter(|board| {
                board.material_id == material.id
                    && !project.allocations.iter().any(|a| a.board_id == board.id)
            })
            .collect();
        let mut args = FluentArgs::new();
        args.set("material", material.name.as_str());
        if let [board] = waiting.as_slice() {
            let loc = locale(self);
            args.set("board", board.name.as_str());
            args.set(
                "size",
                format!(
                    "{} × {} × {}",
                    compact_mm(board.length, loc),
                    compact_mm(board.width, loc),
                    compact_mm(board.thickness, loc)
                ),
            );
            self.localizer.format("stock-missing-one", Some(&args))
        } else {
            args.set("count", waiting.len().max(material.board_count));
            self.localizer.format("stock-missing-many", Some(&args))
        }
    }

    pub(super) fn show_stock_list(&mut self, ui: &mut egui::Ui) {
        let modal = self.modal_open();
        self.show_stock_title(ui, modal);
        ui.add_space(14.0);
        let Some(model) = self.stock_snapshot() else {
            ui.label(egui::RichText::new(self.localizer.text("cost-invalid")).color(tw::DANGER));
            return;
        };
        let loc = locale(self);
        let money_locale = self.stock_money_locale();
        let query = self.session.stock.filter.to_lowercase();
        let filter = self.session.stock_material_filter;
        let global = self.session.stock_global_order;
        let rows = stock_visible_rows(&model, filter, global, &query);
        let visible_materials: Vec<_> = model
            .materials
            .iter()
            .filter(|material| {
                filter.is_none_or(|id| id == material.id)
                    && material.name.to_lowercase().contains(&query)
            })
            .collect();
        let needs_stock = |m: &MaterialStockSummary| m.board_count > 0 && m.stock_piece_count == 0;
        let mut items = Vec::new();
        if global {
            for &piece in &rows {
                items.push(TableItem::Row {
                    piece,
                    target: piece.global_rank - 1,
                    scope_len: model.pieces.len(),
                    shown_rank: piece.global_rank,
                });
            }
            for &material in visible_materials.iter().filter(|m| needs_stock(m)) {
                items.push(TableItem::Header(material));
                items.push(TableItem::Warn(material));
            }
        } else {
            for &material in &visible_materials {
                items.push(TableItem::Header(material));
                let group: Vec<&StockPieceReadModel> = rows
                    .iter()
                    .copied()
                    .filter(|piece| piece.material_id == material.id)
                    .collect();
                for (index, &piece) in group.iter().enumerate() {
                    items.push(TableItem::Row {
                        piece,
                        target: index,
                        scope_len: group.len(),
                        shown_rank: index + 1,
                    });
                }
                if needs_stock(material) {
                    items.push(TableItem::Warn(material));
                }
            }
        }
        let empty_message = if model.materials.is_empty() {
            Some("stock-no-materials")
        } else if visible_materials.is_empty() {
            Some("stock-filter-no-materials")
        } else if model.pieces.is_empty() {
            Some("stock-no-pieces")
        } else if rows.is_empty() && global {
            Some("stock-filter-empty")
        } else {
            None
        };

        let drag_label = self.localizer.text("stock-drag-handle");
        let drag_help = self.localizer.text("stock-drag-help");
        let mut action: Option<StockUiAction> = None;
        let mut drag_stopped = false;
        let mut drag_targets: Vec<(Uuid, Uuid, usize, usize, egui::Rect)> = Vec::new();
        let width = ui.available_width();
        let table_width = width.max(table_min_width());
        let item_count = items.len();
        let mut toggled_global = None;
        egui::ScrollArea::horizontal()
            .id_salt(("stock-table-scroll", self.editor.project().id))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Frame::new()
                    .fill(tw::PANEL)
                    .stroke(egui::Stroke::new(1.0, tw::BORDER_SOFT))
                    .corner_radius(10)
                    .show(ui, |ui| {
                        ui.set_width(table_width - 2.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let w = ui.available_width();
                        // Toolbar: view mode as a compact segmented control.
                        ui.allocate_ui_with_layout(
                            egui::vec2(w, 40.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add_space(10.0);
                                let mut value = global;
                                let grouped = self.localizer.text("stock-view-grouped");
                                let priority = self.localizer.text("stock-view-priority");
                                let response = tw::segmented(
                                    ui,
                                    &mut value,
                                    &[(false, grouped.as_str()), (true, priority.as_str())],
                                );
                                response.on_hover_text(
                                    self.localizer.text("stock-priority-distinction"),
                                );
                                if value != global {
                                    toggled_global = Some(value);
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add_space(12.0);
                                        ui.label(
                                            egui::RichText::new(
                                                self.localizer.text("stock-drag-hint"),
                                            )
                                            .size(11.5)
                                            .color(tw::FAINT),
                                        );
                                    },
                                );
                            },
                        );
                        let (line, _) =
                            ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
                        ui.painter().hline(
                            line.x_range(),
                            line.center().y,
                            egui::Stroke::new(1.0, tw::BORDER_SOFT),
                        );
                        // Column header.
                        let (header, _) =
                            ui.allocate_exact_size(egui::vec2(w, 34.0), egui::Sense::hover());
                        let cols = column_rects(header);
                        for (index, key) in [
                            "",
                            "stock-col-rank",
                            "stock-col-id",
                            "stock-col-measured",
                            "stock-col-grain",
                            "stock-col-trims",
                            "stock-col-ownership",
                            "stock-price-heading",
                            "stock-col-on-plan",
                        ]
                        .iter()
                        .enumerate()
                        {
                            if key.is_empty() {
                                continue;
                            }
                            let job = tracked_job(ui, &self.localizer.text(key), 10.5, tw::FAINT);
                            let galley = ui.painter().layout_job(job);
                            let cell = cols[index];
                            let pos = if index == 7 {
                                egui::pos2(
                                    cell.right() - 8.0 - galley.size().x,
                                    cell.center().y - galley.size().y / 2.0,
                                )
                            } else if index == 8 {
                                egui::pos2(
                                    cell.left() + 14.0,
                                    cell.center().y - galley.size().y / 2.0,
                                )
                            } else {
                                egui::pos2(cell.left(), cell.center().y - galley.size().y / 2.0)
                            };
                            ui.painter()
                                .with_clip_rect(cell)
                                .galley(pos, galley, tw::FAINT);
                        }
                        ui.painter().hline(
                            header.x_range(),
                            header.bottom() - 0.5,
                            egui::Stroke::new(1.0, tw::BORDER_SOFT),
                        );
                        if let Some(key) = empty_message {
                            egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(16, 14))
                                .show(ui, |ui| {
                                    ui.set_width(w - 32.0);
                                    ui.label(
                                        egui::RichText::new(self.localizer.text(key))
                                            .size(12.5)
                                            .color(tw::MUTED),
                                    );
                                    if key == "stock-no-materials"
                                        && tw::icon_text_button(
                                            ui,
                                            Icon::Plus,
                                            &A::NewMaterial.label(&self.localizer),
                                            false,
                                            !modal,
                                        )
                                        .clicked()
                                    {
                                        action = Some(StockUiAction::Request(Request::new(
                                            A::NewMaterial,
                                        )));
                                    }
                                });
                        }
                        for (index, item) in items.iter().enumerate() {
                            let last = index + 1 == item_count;
                            match item {
                                TableItem::Header(material) => {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(w, 30.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(rect, 0.0, tw::APP);
                                    ui.painter().hline(
                                        rect.x_range(),
                                        rect.bottom() - 0.5,
                                        egui::Stroke::new(1.0, tw::RULE),
                                    );
                                    let color = self.editor.project().material_color(material.id);
                                    let swatch = egui::Rect::from_center_size(
                                        egui::pos2(rect.left() + 19.0, rect.center().y),
                                        egui::vec2(10.0, 10.0),
                                    );
                                    ui.painter().rect(
                                        swatch,
                                        2.0,
                                        egui::Color32::from_rgb(color.0[0], color.0[1], color.0[2]),
                                        egui::Stroke::new(1.0, egui::Color32::from_black_alpha(28)),
                                        egui::StrokeKind::Inside,
                                    );
                                    let name_font = tw::weighted_font(
                                        ui,
                                        12.0,
                                        crate::theme::Typeface::SansSemibold,
                                    );
                                    let name = ui.painter().layout_no_wrap(
                                        material.name.clone(),
                                        name_font,
                                        tw::TEXT,
                                    );
                                    let name_width = name.size().x.min(w - 200.0);
                                    let text_rect = egui::Rect::from_min_max(
                                        egui::pos2(rect.left() + 32.0, rect.top()),
                                        egui::pos2(rect.right() - 40.0, rect.bottom()),
                                    );
                                    ui.painter().with_clip_rect(text_rect).galley(
                                        egui::pos2(
                                            text_rect.left(),
                                            rect.center().y - name.size().y / 2.0,
                                        ),
                                        name,
                                        tw::TEXT,
                                    );
                                    let detail = if material.stock_piece_count == 0 {
                                        self.localizer.text("stock-no-stock")
                                    } else {
                                        let mut args = FluentArgs::new();
                                        args.set(
                                            "thickness",
                                            compact_mm(material.default_thickness, loc),
                                        );
                                        args.set("count", material.stock_piece_count);
                                        self.localizer.format("stock-group-detail", Some(&args))
                                    };
                                    ui.painter().with_clip_rect(text_rect).text(
                                        egui::pos2(
                                            text_rect.left() + name_width + 8.0,
                                            rect.center().y,
                                        ),
                                        egui::Align2::LEFT_CENTER,
                                        detail,
                                        egui::FontId::proportional(12.0),
                                        tw::FAINT,
                                    );
                                    let icon_rect = egui::Rect::from_center_size(
                                        egui::pos2(rect.right() - 22.0, rect.center().y),
                                        egui::vec2(24.0, 24.0),
                                    );
                                    let mut child = ui.new_child(
                                        egui::UiBuilder::new().max_rect(icon_rect).layout(
                                            egui::Layout::left_to_right(egui::Align::Center),
                                        ),
                                    );
                                    let mut args = FluentArgs::new();
                                    args.set("material", material.name.as_str());
                                    if tw::ghost_icon_sized(
                                        &mut child,
                                        Icon::Plus,
                                        &self
                                            .localizer
                                            .format("stock-add-material-sheet", Some(&args)),
                                        tw::MUTED,
                                        13.0,
                                        22.0,
                                        !modal,
                                        false,
                                    )
                                    .clicked()
                                    {
                                        action = Some(StockUiAction::Request(Request::with(
                                            A::NewStock,
                                            Target::Material(material.id),
                                        )));
                                    }
                                }
                                TableItem::Warn(material) => {
                                    let reason = self.missing_stock_reason(material);
                                    let mut args = FluentArgs::new();
                                    args.set("material", material.name.as_str());
                                    let button = self
                                        .localizer
                                        .format("stock-add-material-sheet", Some(&args));
                                    let radius = if last {
                                        egui::CornerRadius {
                                            nw: 0,
                                            ne: 0,
                                            sw: 9,
                                            se: 9,
                                        }
                                    } else {
                                        egui::CornerRadius::ZERO
                                    };
                                    egui::Frame::new()
                                        .fill(tw::WARN_BG)
                                        .corner_radius(radius)
                                        .inner_margin(egui::Margin::symmetric(16, 10))
                                        .show(ui, |ui| {
                                            ui.set_width(w - 32.0);
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 12.0;
                                                ui.add(icons::icon(Icon::Warning, tw::WARN, 16.0));
                                                let button_width = ui
                                                    .painter()
                                                    .layout_no_wrap(
                                                        button.clone(),
                                                        egui::FontId::proportional(12.0),
                                                        tw::PANEL,
                                                    )
                                                    .size()
                                                    .x
                                                    + 44.0;
                                                let text_width =
                                                    (ui.available_width() - button_width - 12.0)
                                                        .max(120.0);
                                                ui.allocate_ui_with_layout(
                                                    egui::vec2(text_width, 28.0),
                                                    egui::Layout::left_to_right(
                                                        egui::Align::Center,
                                                    )
                                                    .with_main_wrap(true),
                                                    |ui| {
                                                        ui.set_width(text_width);
                                                        ui.add(
                                                            egui::Label::new(
                                                                egui::RichText::new(&reason)
                                                                    .size(13.0)
                                                                    .color(WARN_TEXT),
                                                            )
                                                            .wrap(),
                                                        );
                                                    },
                                                );
                                                if ui
                                                    .add_enabled(
                                                        !modal,
                                                        egui::Button::image_and_text(
                                                            icons::icon(
                                                                Icon::Plus,
                                                                tw::PANEL,
                                                                13.0,
                                                            ),
                                                            tw::medium(ui, &button, 12.0)
                                                                .color(tw::PANEL),
                                                        )
                                                        .fill(tw::TEXT)
                                                        .corner_radius(6)
                                                        .min_size(egui::vec2(0.0, 28.0)),
                                                    )
                                                    .clicked()
                                                {
                                                    action = Some(StockUiAction::Request(
                                                        Request::with(
                                                            A::NewStock,
                                                            Target::Material(material.id),
                                                        ),
                                                    ));
                                                }
                                            });
                                        });
                                }
                                TableItem::Row {
                                    piece,
                                    target,
                                    scope_len,
                                    shown_rank,
                                } => {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(w, 40.0),
                                        egui::Sense::hover(),
                                    );
                                    let label = format!("{} · {}", piece.alias, piece.name);
                                    let selected = self.session.stock_piece == Some(piece.id);
                                    let response = ui.interact(
                                        rect,
                                        egui::Id::new(("stock-row", piece.id)),
                                        if modal {
                                            egui::Sense::hover()
                                        } else {
                                            egui::Sense::click()
                                        },
                                    );
                                    response.widget_info(|| {
                                        egui::WidgetInfo::selected(
                                            egui::WidgetType::SelectableLabel,
                                            !modal,
                                            selected,
                                            &label,
                                        )
                                    });
                                    let cols = column_rects(rect);
                                    let handle = ui.interact(
                                        cols[0],
                                        egui::Id::new(("stock-drag", piece.id)),
                                        if modal {
                                            egui::Sense::hover()
                                        } else {
                                            egui::Sense::click_and_drag()
                                        },
                                    );
                                    handle.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            !modal,
                                            &drag_label,
                                        )
                                    });
                                    if handle.drag_started() {
                                        self.session.stock_drag = Some(piece.id);
                                    }
                                    drag_stopped |= handle.drag_stopped();
                                    let dragging = self.session.stock_drag == Some(piece.id);
                                    let hovered =
                                        !modal && (response.hovered() || handle.hovered());
                                    let radius = if last {
                                        egui::CornerRadius {
                                            nw: 0,
                                            ne: 0,
                                            sw: 9,
                                            se: 9,
                                        }
                                    } else {
                                        egui::CornerRadius::ZERO
                                    };
                                    let painter = ui.painter().clone();
                                    if selected || dragging {
                                        painter.rect_filled(rect, radius, tw::ACCENT_BG);
                                    } else if hovered {
                                        painter.rect_filled(rect, radius, tw::HOVER_ROW);
                                    }
                                    if !last {
                                        painter.hline(
                                            rect.x_range(),
                                            rect.bottom() - 0.5,
                                            egui::Stroke::new(
                                                1.0,
                                                if selected { tw::WARN_STROKE } else { tw::RULE },
                                            ),
                                        );
                                    }
                                    let handle_color = if selected || dragging || handle.hovered() {
                                        HANDLE_ACTIVE
                                    } else {
                                        HANDLE_IDLE
                                    };
                                    paint_grip(&painter, cols[0].center(), handle_color);
                                    let soft = if selected {
                                        SELECTED_SOFT_INK
                                    } else {
                                        tw::FAINT
                                    };
                                    let ink = if selected {
                                        egui::Color32::from_rgb(62, 35, 5)
                                    } else {
                                        tw::TEXT
                                    };
                                    let mono = egui::FontId::monospace(12.5);
                                    paint_in(
                                        &painter,
                                        cols[1],
                                        egui::Align2::LEFT_CENTER,
                                        shown_rank,
                                        mono.clone(),
                                        soft,
                                    );
                                    paint_in(
                                        &painter,
                                        cols[2],
                                        egui::Align2::LEFT_CENTER,
                                        &piece.alias,
                                        egui::FontId::monospace(12.0),
                                        ink,
                                    );
                                    paint_in(
                                        &painter,
                                        cols[3],
                                        egui::Align2::LEFT_CENTER,
                                        format!(
                                            "{} × {} × {}",
                                            compact_mm(piece.length, loc),
                                            compact_mm(piece.width, loc),
                                            compact_mm(piece.measured_thickness, loc)
                                        ),
                                        mono.clone(),
                                        ink,
                                    );
                                    paint_in(
                                        &painter,
                                        cols[4],
                                        egui::Align2::LEFT_CENTER,
                                        self.localizer.text(stock_grain_column_key(piece.grain)),
                                        egui::FontId::proportional(13.0),
                                        if selected { ink } else { tw::SECONDARY },
                                    );
                                    paint_in(
                                        &painter,
                                        cols[5],
                                        egui::Align2::LEFT_CENTER,
                                        compact_mm(sum_trims(piece), loc),
                                        mono.clone(),
                                        soft,
                                    );
                                    let (chip_text, chip_fill, chip_ink) = if piece.source
                                        == StockSource::Owned
                                    {
                                        (self.localizer.text("stock-owned"), tw::OK_BG, tw::OK_INK)
                                    } else {
                                        (
                                            self.localizer.text("stock-purchase"),
                                            if selected { tw::PANEL } else { tw::ACCENT_BG },
                                            CHIP_PURCHASE_INK,
                                        )
                                    };
                                    paint_chip(
                                        &painter,
                                        cols[6].left_center(),
                                        &chip_text,
                                        chip_fill,
                                        chip_ink,
                                    );
                                    match piece.price {
                                        Some(price) => paint_in(
                                            &painter,
                                            cols[7],
                                            egui::Align2::RIGHT_CENTER,
                                            money_amount(price, money_locale),
                                            mono.clone(),
                                            ink,
                                        ),
                                        None => paint_in(
                                            &painter,
                                            cols[7],
                                            egui::Align2::RIGHT_CENTER,
                                            "—",
                                            mono.clone(),
                                            tw::FAINT,
                                        ),
                                    }
                                    let plan = cols[8].with_min_x(cols[8].left() + 14.0);
                                    if piece.parts.is_empty() {
                                        paint_in(
                                            &painter,
                                            plan,
                                            egui::Align2::LEFT_CENTER,
                                            self.localizer.text(
                                                if piece.source == StockSource::Owned {
                                                    "stock-unused"
                                                } else {
                                                    "stock-spare"
                                                },
                                            ),
                                            egui::FontId::proportional(11.5),
                                            tw::FAINT,
                                        );
                                    } else {
                                        let bar = egui::Rect::from_min_size(
                                            egui::pos2(plan.left(), plan.center().y - 2.0),
                                            egui::vec2(40.0, 4.0),
                                        );
                                        painter.rect_filled(
                                            bar,
                                            2.0,
                                            if selected {
                                                tw::WARN_STROKE
                                            } else {
                                                tw::VIEWPORT
                                            },
                                        );
                                        if let Some(fraction) = usage_fraction(piece) {
                                            painter.rect_filled(
                                                bar.with_max_x(bar.left() + bar.width() * fraction),
                                                2.0,
                                                if selected { HANDLE_ACTIVE } else { tw::FAINT },
                                            );
                                        }
                                        let mut args = FluentArgs::new();
                                        args.set("count", piece.parts.len());
                                        paint_in(
                                            &painter,
                                            plan.with_min_x(bar.right() + 6.0),
                                            egui::Align2::LEFT_CENTER,
                                            self.localizer.format("stock-parts-n", Some(&args)),
                                            egui::FontId::proportional(11.5),
                                            if selected { ink } else { tw::SECONDARY },
                                        );
                                    }
                                    drag_targets.push((
                                        piece.id,
                                        piece.material_id,
                                        *target,
                                        piece.global_rank,
                                        rect,
                                    ));
                                    if response.clicked() {
                                        self.session.stock_piece = Some(piece.id);
                                    }
                                    response.context_menu(|ui| {
                                        ui.set_min_width(180.0);
                                        let item = |ui: &mut egui::Ui, key: &str, enabled: bool| {
                                            ui.add_enabled(
                                                enabled && !modal,
                                                egui::Button::new(self.localizer.text(key)),
                                            )
                                            .clicked()
                                        };
                                        let movement = |to: usize| {
                                            StockUiAction::Request(
                                                Request::with(
                                                    A::StockMove,
                                                    Target::Stock(piece.id),
                                                )
                                                .argument(Argument::StockPriority {
                                                    target: to,
                                                    subset: !global,
                                                }),
                                            )
                                        };
                                        let mut chosen = None;
                                        if item(ui, "stock-edit-ellipsis", true) {
                                            chosen = Some(StockUiAction::Request(Request::with(
                                                A::EditStock,
                                                Target::Stock(piece.id),
                                            )));
                                        }
                                        ui.separator();
                                        if item(ui, "stock-move-top", *target > 0) {
                                            chosen = Some(movement(0));
                                        }
                                        if item(ui, "stock-up", *target > 0) {
                                            chosen = Some(movement(target - 1));
                                        }
                                        if item(ui, "stock-down", target + 1 < *scope_len) {
                                            chosen = Some(movement(target + 1));
                                        }
                                        if item(ui, "stock-move-bottom", target + 1 < *scope_len) {
                                            chosen = Some(movement(scope_len - 1));
                                        }
                                        ui.separator();
                                        if item(ui, "stock-duplicate", true) {
                                            chosen = Some(StockUiAction::Duplicate(piece.id));
                                        }
                                        if item(ui, "stock-delete", piece.parts.is_empty()) {
                                            chosen = Some(StockUiAction::Delete(piece.id));
                                        }
                                        if chosen.is_some() {
                                            action = chosen;
                                            ui.close();
                                        }
                                    });
                                    response.on_hover_text(label.as_str());
                                    handle
                                        .on_hover_text(drag_help.as_str())
                                        .on_hover_cursor(egui::CursorIcon::Grab);
                                }
                            }
                        }
                    });
            });
        if let Some(value) = toggled_global {
            self.session.stock_global_order = value;
            if value {
                self.session.stock_material_filter = None;
                self.session.stock.filter.clear();
            }
        }
        if let Some(dragged) = self.session.stock_drag
            && let Some(pointer) = ui.ctx().pointer_latest_pos()
        {
            let source_material = model
                .pieces
                .iter()
                .find(|row| row.id == dragged)
                .map(|row| row.material_id);
            if let Some((_, _, target, rank, rect)) =
                drag_targets.iter().find(|(_, material, _, _, rect)| {
                    rect.contains(pointer) && (global || Some(*material) == source_material)
                })
            {
                ui.painter().rect_stroke(
                    *rect,
                    0.0,
                    egui::Stroke::new(1.5, tw::ACCENT),
                    egui::StrokeKind::Inside,
                );
                egui::Tooltip::always_open(
                    ui.ctx().clone(),
                    ui.layer_id(),
                    egui::Id::new("stock-drag-target"),
                    egui::PopupAnchor::Pointer,
                )
                .show(|ui| {
                    ui.label(format!(
                        "{}: #{} · {}",
                        self.localizer.text("stock-target-rank"),
                        rank,
                        self.localizer.text(if global {
                            "stock-global-scope"
                        } else {
                            "stock-visible-scope"
                        })
                    ));
                });
                if drag_stopped {
                    action = Some(StockUiAction::Request(
                        Request::with(A::StockMove, Target::Stock(dragged)).argument(
                            Argument::StockPriority {
                                target: *target,
                                subset: !global,
                            },
                        ),
                    ));
                }
            }
        }
        if drag_stopped {
            self.session.stock_drag = None;
        }
        if let Some(action) = action {
            self.apply_stock_action(action);
        }
        ui.add_space(14.0);
        self.show_stock_summary(ui, &model);
    }

    fn stock_money_locale(&self) -> MoneyLocale {
        if locale(self) == Locale::En {
            MoneyLocale::English
        } else {
            MoneyLocale::PortugueseBrazil
        }
    }

    fn show_stock_summary(&self, ui: &mut egui::Ui, model: &StockReadModel) {
        let result = &model.estimate;
        let money_locale = self.stock_money_locale();
        let usage = model.usage_summary();
        let alias = |id: Uuid| {
            model
                .pieces
                .iter()
                .find(|piece| piece.id == id)
                .map(|piece| piece.alias.clone())
        };
        let used: Vec<Uuid> = result
            .used_stock
            .iter()
            .map(|entry| entry.stock_id)
            .collect();
        let purchase_used: Vec<String> = result
            .used_stock
            .iter()
            .filter(|entry| entry.source == StockSource::ToPurchase)
            .filter_map(|entry| alias(entry.stock_id))
            .collect();
        let purchase_unused: Vec<String> = model
            .pieces
            .iter()
            .filter(|piece| piece.source == StockSource::ToPurchase && !used.contains(&piece.id))
            .map(|piece| piece.alias.clone())
            .collect();
        let owned_total = model
            .pieces
            .iter()
            .filter(|piece| piece.source == StockSource::Owned)
            .count();
        let owned_unused: Vec<String> = model
            .pieces
            .iter()
            .filter(|piece| piece.source == StockSource::Owned && !used.contains(&piece.id))
            .map(|piece| piece.alias.clone())
            .collect();
        let unknown = self.localizer.text("stock-fee-unknown");

        let purchase_value = result
            .material
            .map_or_else(|| unknown.clone(), |value| value.display(money_locale));
        let mut purchase_detail = if purchase_used.is_empty() {
            self.localizer.text("stock-card-none-used")
        } else {
            purchase_used.join(", ")
        };
        if !purchase_unused.is_empty() {
            let mut args = FluentArgs::new();
            args.set("aliases", purchase_unused.join(", "));
            purchase_detail = format!(
                "{} · {}",
                purchase_detail,
                self.localizer.format("stock-card-unused", Some(&args))
            );
        }
        let (cutting_value, cutting_warn, cutting_detail) =
            match (model.cut_fee, usage.physical_cuts) {
                (_, None) => (
                    "?".to_owned(),
                    true,
                    self.localizer.text("stock-cuts-unverified"),
                ),
                (None, Some(cuts)) => {
                    let mut args = FluentArgs::new();
                    args.set("cuts", cuts);
                    args.set("fee", "?");
                    (
                        self.localizer.format("stock-cuts-times", Some(&args)),
                        true,
                        self.localizer.text("stock-card-set-fee"),
                    )
                }
                (Some(fee), Some(cuts)) => {
                    let mut args = FluentArgs::new();
                    args.set("cuts", cuts);
                    args.set("fee", money_amount(fee, money_locale));
                    (
                        result
                            .cutting
                            .map_or_else(|| unknown.clone(), |value| value.display(money_locale)),
                        false,
                        self.localizer.format("stock-cuts-times", Some(&args)),
                    )
                }
            };
        let mut args = FluentArgs::new();
        args.set("used", usage.consumed_owned_pieces);
        args.set("total", owned_total);
        let owned_value = self.localizer.format("stock-owned-of", Some(&args));
        let owned_detail = if owned_unused.is_empty() {
            String::new()
        } else {
            let mut args = FluentArgs::new();
            args.set("aliases", owned_unused.join(", "));
            self.localizer.format("stock-owned-free", Some(&args))
        };
        let cards = [
            (
                "stock-card-purchase",
                purchase_value,
                result.material.is_none(),
                purchase_detail,
            ),
            (
                "stock-card-cutting",
                cutting_value,
                cutting_warn,
                cutting_detail,
            ),
            ("stock-card-owned", owned_value, false, owned_detail),
        ];
        let columns = if ui.available_width() >= 540.0 { 3 } else { 1 };
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.columns(columns, |uis| {
            for (index, (key, value, warn, detail)) in cards.iter().enumerate() {
                let ui = &mut uis[index % columns];
                egui::Frame::new()
                    .fill(tw::PANEL)
                    .stroke(egui::Stroke::new(1.0, tw::BORDER_SOFT))
                    .corner_radius(10)
                    .inner_margin(egui::Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.set_min_height(58.0);
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.label(
                            egui::RichText::new(self.localizer.text(key))
                                .size(11.5)
                                .color(tw::MUTED),
                        );
                        ui.add(
                            egui::Label::new(tw::mono(value, 17.0).color(if *warn {
                                tw::WARN_INK
                            } else {
                                tw::TEXT
                            }))
                            .truncate(),
                        );
                        ui.label(egui::RichText::new(detail).size(11.0).color(tw::FAINT));
                    });
                if columns == 1 {
                    ui.add_space(8.0);
                }
            }
        });
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(self.localizer.text("stock-disclaimer"))
                .size(11.5)
                .color(tw::FAINT),
        );
        egui::CollapsingHeader::new(
            egui::RichText::new(self.localizer.text("stock-details"))
                .size(11.5)
                .color(tw::MUTED),
        )
        .id_salt("stock-estimate-details")
        .show(ui, |ui| {
            let small = |text: String| egui::RichText::new(text).size(11.5).color(tw::MUTED);
            ui.label(small(format!(
                "{}: {}",
                self.localizer.text("cost-total"),
                result.total.map_or_else(
                    || self.localizer.text("cost-incomplete"),
                    |value| value.display(money_locale),
                )
            )));
            if result.feasibility != Feasibility::Verified {
                ui.label(small(self.localizer.text("cost-feasibility")));
            }
            ui.label(small(self.localizer.text("cut-fee-hint")));
        });
    }

    // -----------------------------------------------------------------------
    // Right: piece inspector
    // -----------------------------------------------------------------------

    fn show_stock_inspector_empty(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(14, 16))
            .show(ui, |ui| {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.add(icons::icon(Icon::Sheet, tw::DISABLED, 28.0));
                    ui.add_space(8.0);
                    ui.label(
                        tw::medium(ui, self.localizer.text("stock-inspector-none"), 13.0)
                            .color(tw::MUTED),
                    );
                    ui.label(
                        egui::RichText::new(self.localizer.text("stock-inspector-empty"))
                            .size(12.0)
                            .color(tw::FAINT),
                    );
                });
            });
    }

    pub(super) fn show_stock_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(model) = self.stock_snapshot() else {
            ui.label(self.localizer.text("cost-invalid"));
            return;
        };
        let Some(piece) = self
            .session
            .stock_piece
            .and_then(|id| model.pieces.iter().find(|piece| piece.id == id))
            .cloned()
        else {
            self.show_stock_inspector_empty(ui);
            return;
        };
        let loc = locale(self);
        let money_locale = self.stock_money_locale();
        let can_edit = !self.modal_open() && self.stock_edit_allowed(piece.id);
        let edit_label = self.localizer.text("stock-edit");
        let mut action: Option<StockUiAction> = None;
        let edit = || StockUiAction::Request(Request::with(A::EditStock, Target::Stock(piece.id)));

        // Header: icon tile, name, alias/priority, duplicate + delete.
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 14,
                right: 10,
                top: 14,
                bottom: 12,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (tile, _) =
                        ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                    ui.painter().rect_filled(tile, 8.0, tw::ACCENT_BG);
                    icons::icon(Icon::Sheet, tw::ACCENT_DARK, 18.0).paint_at(
                        ui,
                        egui::Rect::from_center_size(tile.center(), egui::vec2(18.0, 18.0)),
                    );
                    let text_width = (ui.available_width() - 70.0).max(60.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(text_width, 36.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(text_width);
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.add(
                                egui::Label::new(
                                    tw::semibold(ui, &piece.name, 15.0).color(tw::TEXT),
                                )
                                .truncate(),
                            );
                            let mut args = FluentArgs::new();
                            args.set("rank", piece.global_rank);
                            ui.label(
                                tw::mono(
                                    format!(
                                        "{} · {}",
                                        piece.alias,
                                        self.localizer.format("stock-priority", Some(&args))
                                    ),
                                    11.0,
                                )
                                .color(tw::FAINT),
                            );
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let deletable = piece.parts.is_empty();
                        let delete_label = self.localizer.text(if deletable {
                            "stock-delete"
                        } else {
                            "stock-delete-blocked"
                        });
                        if tw::ghost_icon_sized(
                            ui,
                            Icon::Trash,
                            &delete_label,
                            tw::MUTED,
                            15.0,
                            28.0,
                            can_edit && deletable,
                            false,
                        )
                        .clicked()
                        {
                            action = Some(StockUiAction::Delete(piece.id));
                        }
                        if tw::ghost_icon_sized(
                            ui,
                            Icon::Duplicate,
                            &self.localizer.text("stock-duplicate"),
                            tw::MUTED,
                            15.0,
                            28.0,
                            can_edit,
                            false,
                        )
                        .clicked()
                        {
                            action = Some(StockUiAction::Duplicate(piece.id));
                        }
                    });
                });
            });
        tw::divider(ui);
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 2,
                bottom: 16,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                tw::inspector_heading(ui, &self.localizer.text("stock-measured-size"), |ui| {
                    if tw::text_button(
                        ui,
                        &self.localizer.text("stock-edit-short"),
                        tw::ACCENT_DARK,
                        can_edit,
                    )
                    .on_hover_text(edit_label.as_str())
                    .clicked()
                    {
                        action = Some(edit());
                    }
                });
                for (key, value, salt) in [
                    ("stock-length-x", piece.length, "length"),
                    ("stock-width-y", piece.width, "width"),
                    ("board-thickness", piece.measured_thickness, "thickness"),
                ] {
                    let label = self.localizer.text(key);
                    tw::prop_row(ui, &label, 88.0, |ui| {
                        if value_box(
                            ui,
                            egui::Id::new(("stock-inspector-size", salt)),
                            &format!("{label} · {edit_label}"),
                            &compact_mm(value, loc),
                            false,
                            Some("mm"),
                            false,
                            ui.available_width(),
                            can_edit,
                        )
                        .clicked()
                        {
                            action = Some(edit());
                        }
                    });
                }

                tw::inspector_heading(ui, &self.localizer.text("stock-grain-heading"), |_| {});
                let grains = [
                    StockGrain::AlongX,
                    StockGrain::AlongY,
                    StockGrain::Nondirectional,
                    StockGrain::Unknown,
                ]
                .map(|grain| (grain, self.localizer.text(stock_grain_short_key(grain))));
                if let Some(grain) = full_segmented(
                    ui,
                    egui::Id::new("stock-inspector-grain"),
                    piece.grain,
                    &grains,
                    can_edit,
                ) {
                    action = Some(StockUiAction::Grain(piece.id, grain));
                }

                tw::inspector_heading(ui, &self.localizer.text("stock-edge-trims"), |ui| {
                    ui.label(
                        egui::RichText::new(self.localizer.text("stock-trims-note"))
                            .size(11.0)
                            .color(tw::MUTED),
                    )
                    .on_hover_text(self.localizer.text("stock-trim-hint"));
                });
                ui.add_space(2.0);
                if self.show_trim_diagram(ui, &piece, can_edit) {
                    action = Some(edit());
                }

                tw::inspector_heading(ui, &self.localizer.text("stock-cost"), |_| {});
                let sources = [
                    (StockSource::Owned, self.localizer.text("stock-owned")),
                    (
                        StockSource::ToPurchase,
                        self.localizer.text("stock-purchase"),
                    ),
                ];
                if let Some(source) = full_segmented(
                    ui,
                    egui::Id::new("stock-inspector-source"),
                    piece.source,
                    &sources,
                    can_edit,
                ) {
                    action = Some(StockUiAction::Source(piece.id, source));
                }
                ui.add_space(2.0);
                let mut args = FluentArgs::new();
                args.set("currency", self.editor.project().currency.code());
                let per_piece = self.localizer.format("stock-per-piece", Some(&args));
                let (price, unknown) = piece.price.map_or_else(
                    || (self.localizer.text("stock-fee-unknown"), true),
                    |price| (money_amount(price, money_locale), false),
                );
                if value_box(
                    ui,
                    egui::Id::new("stock-inspector-price"),
                    &format!(
                        "{} · {edit_label}",
                        self.localizer.text("stock-price-heading")
                    ),
                    &price,
                    unknown,
                    Some(&per_piece),
                    false,
                    ui.available_width(),
                    can_edit,
                )
                .clicked()
                {
                    action = Some(edit());
                }

                ui.add_space(16.0);
                tw::card().inner_margin(12).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.horizontal(|ui| {
                        ui.label(
                            tw::semibold(ui, self.localizer.text("stock-on-plan-title"), 13.0)
                                .color(tw::TEXT),
                        );
                        let mut stats = Vec::new();
                        if let Some(fraction) = usage_fraction(&piece) {
                            stats.push(format!("{:.0}%", fraction * 100.0));
                        }
                        if let Some(cuts) = piece.proof.cut_count() {
                            let mut args = FluentArgs::new();
                            args.set("cuts", cuts);
                            stats.push(self.localizer.format("stock-cuts-short", Some(&args)));
                        }
                        if !stats.is_empty() {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(tw::mono(stats.join(" · "), 11.0).color(tw::MUTED));
                                },
                            );
                        }
                    });
                    if piece.parts.is_empty() {
                        ui.label(
                            egui::RichText::new(self.localizer.text("stock-no-assigned-parts"))
                                .size(12.0)
                                .color(tw::FAINT),
                        );
                    } else {
                        ui.spacing_mut().interact_size.y = 18.0;
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
                            ui.spacing_mut().button_padding = egui::vec2(0.0, 1.0);
                            let count = piece.parts.len();
                            for (index, part) in piece.parts.iter().enumerate() {
                                let name = if index + 1 < count {
                                    format!("{},\u{a0}", part.name)
                                } else {
                                    part.name.clone()
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(name).size(12.0).color(tw::MUTED),
                                        )
                                        .frame(false),
                                    )
                                    .on_hover_text(self.localizer.text("stock-show-part"))
                                    .clicked()
                                {
                                    action = Some(StockUiAction::OpenBoard(part.board_id));
                                }
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.spacing_mut().button_padding = egui::vec2(0.0, 1.0);
                        if tw::text_button(
                            ui,
                            &self.localizer.text("stock-open-cut-plan"),
                            tw::ACCENT_DARK,
                            true,
                        )
                        .clicked()
                        {
                            action = Some(StockUiAction::OpenSheet(piece.id));
                        }
                        ui.label(egui::RichText::new("→").size(12.0).color(tw::ACCENT_DARK));
                    });
                });
            });
        if let Some(action) = action {
            self.apply_stock_action(action);
        }
    }

    /// Four trim values around a hatched "usable" rectangle. Returns whether
    /// any trim value was clicked (to open the edit dialog).
    fn show_trim_diagram(
        &self,
        ui: &mut egui::Ui,
        piece: &StockPieceReadModel,
        enabled: bool,
    ) -> bool {
        let loc = locale(self);
        let side = 62.0;
        let gap = 6.0;
        let width = ui.available_width();
        let center_width = (width - 2.0 * side - 2.0 * gap).max(60.0);
        let (area, _) = ui.allocate_exact_size(
            egui::vec2(width, 28.0 + 70.0 + 28.0 + 2.0 * gap),
            egui::Sense::hover(),
        );
        let middle = egui::Rect::from_min_size(
            area.min + egui::vec2(side + gap, 28.0 + gap),
            egui::vec2(center_width, 70.0),
        );
        let painter = ui.painter().clone();
        let usable = trim_preview_rect(middle, piece);
        if usable != middle {
            painter.rect_filled(middle, 0.0, TRIM_LOSS);
        }
        if usable.is_positive() {
            painter.rect_filled(usable, 0.0, HATCH_FILL);
            let hatch = painter.with_clip_rect(usable);
            let mut x = usable.left() + 8.0;
            while x < usable.right() {
                hatch.vline(x, usable.y_range(), egui::Stroke::new(1.0, HATCH_LINE));
                x += 9.0;
            }
        }
        painter.rect_stroke(
            middle,
            0.0,
            egui::Stroke::new(1.5, HATCH_STROKE),
            egui::StrokeKind::Inside,
        );
        let mut args = FluentArgs::new();
        args.set(
            "size",
            format!(
                "{} × {}",
                compact_mm(piece.usable_extent[0], loc),
                compact_mm(piece.usable_extent[1], loc)
            ),
        );
        paint_in(
            &painter,
            middle,
            egui::Align2::CENTER_CENTER,
            self.localizer.format("stock-usable-size", Some(&args)),
            egui::FontId::proportional(11.0),
            tw::MUTED,
        );
        let edit = self.localizer.text("stock-edit");
        let top = egui::Rect::from_min_size(
            egui::pos2(middle.left(), area.top()),
            egui::vec2(center_width, 28.0),
        );
        let bottom = egui::Rect::from_min_size(
            egui::pos2(middle.left(), middle.bottom() + gap),
            egui::vec2(center_width, 28.0),
        );
        let left = egui::Rect::from_min_size(
            egui::pos2(area.left(), middle.center().y - 14.0),
            egui::vec2(side, 28.0),
        );
        let right = egui::Rect::from_min_size(
            egui::pos2(middle.right() + gap, middle.center().y - 14.0),
            egui::vec2(side, 28.0),
        );
        let [trim_left, trim_right, trim_bottom, trim_top] = piece.trim;
        let mut clicked = false;
        for (rect, value, key, tail) in [
            (
                top,
                trim_top,
                "stock-trim-top",
                Some("stock-trim-short-top"),
            ),
            (left, trim_left, "stock-trim-left", None),
            (right, trim_right, "stock-trim-right", None),
            (
                bottom,
                trim_bottom,
                "stock-trim-bottom",
                Some("stock-trim-short-bottom"),
            ),
        ] {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            let tail = tail.map(|key| self.localizer.text(key));
            clicked |= value_box(
                &mut child,
                egui::Id::new(("stock-inspector-trim", key)),
                &format!("{} · {edit}", self.localizer.text(key)),
                &compact_mm(value, loc),
                false,
                tail.as_deref(),
                true,
                rect.width(),
                enabled,
            )
            .clicked();
        }
        clicked
    }

    // -----------------------------------------------------------------------
    // Dialog bodies (chrome belongs to `modal_chrome`)
    // -----------------------------------------------------------------------

    pub(super) fn show_cut_fee_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.modals.take_cut_fee() else {
            return;
        };
        let CutFeeDialog { text, chrome } = &mut dialog;
        let currency = self.editor.project().currency;
        let result = chrome.show(
            ctx,
            &self.localizer.text("cut-fee-edit"),
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &self.localizer.text("cut-fee-edit"),
            },
            |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let label = self.localizer.text("cut-fee");
                field_label(ui, &label);
                let invalid = !text.trim().is_empty() && Money::parse(currency, text).is_err();
                let mut args = FluentArgs::new();
                args.set("currency", currency.code());
                let error = invalid.then(|| self.localizer.text("error-invalid-amount"));
                tw::unit_field(
                    ui,
                    egui::Id::new("cut-fee-amount"),
                    &label,
                    text,
                    &self.localizer.format("stock-per-cut", Some(&args)),
                    error.as_deref(),
                );
                ui.horizontal(|ui| {
                    ui.spacing_mut().button_padding = egui::vec2(0.0, 2.0);
                    if tw::text_button(
                        ui,
                        &self.localizer.text("cut-fee-free"),
                        tw::ACCENT_DARK,
                        true,
                    )
                    .clicked()
                    {
                        *text = known_free_fee_text();
                    }
                });
                ui.label(
                    egui::RichText::new(self.localizer.text("cut-fee-hint"))
                        .size(11.5)
                        .color(tw::FAINT),
                );
                ((), !invalid)
            },
        );
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm) {
            let fee = if text.trim().is_empty() {
                Ok(None)
            } else {
                Money::parse(self.editor.project().currency, text).map(Some)
            };
            if let Ok(fee) = fee
                && self.editor.set_cut_fee(fee).is_ok()
            {
                chrome.close(ctx);
                return;
            }
        }
        self.modals.set_cut_fee(Some(dialog));
    }

    pub(super) fn show_stock_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_stock() else {
            return;
        };
        let project = self.editor.project();
        let current = draft.project_id == project.id && draft.revision == project.revision;
        let title = self.localizer.text(if draft.edit_id.is_some() {
            "stock-edit"
        } else {
            "stock-new"
        });
        let mut chrome = draft.chrome.detach();
        let result = chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &title,
            },
            |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                let localizer = &self.localizer;
                let gap = |ui: &mut egui::Ui| ui.add_space(8.0);
                // Name (with the fixed alias when editing).
                ui.horizontal(|ui| {
                    field_label(ui, &localizer.text("stock-name"));
                    if let Some(alias) = draft.edit_id.and_then(|id| project.stock_alias(id)) {
                        ui.label(tw::mono(alias, 11.0).color(tw::FAINT));
                    }
                });
                ui.add(
                    egui::TextEdit::singleline(&mut draft.name)
                        .id(egui::Id::new("stock-dialog-name"))
                        .desired_width(f32::INFINITY),
                );
                gap(ui);
                field_label(ui, &localizer.text("material"));
                let previous_material = draft.material_id;
                egui::ComboBox::from_id_salt("stock-dialog-material")
                    .width(ui.available_width())
                    .selected_text(
                        project
                            .materials
                            .iter()
                            .find(|m| Some(m.id) == draft.material_id)
                            .map_or("—", |m| m.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        for material in &project.materials {
                            combo_option(
                                ui,
                                &mut draft.material_id,
                                Some(material.id),
                                format!(
                                    "{} · {} mm",
                                    material.name,
                                    compact_mm(material.default_thickness, locale(self))
                                ),
                            );
                        }
                    });
                if draft.material_id != previous_material && draft.edit_id.is_none() {
                    draft.fill_from_preset(project);
                }
                gap(ui);
                let unit = project.display_unit;
                let unit_text = localizer.text(unit_key(unit));
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.columns(3, |columns| {
                    for (index, (key, id)) in [
                        ("stock-length-x", "stock-dialog-length"),
                        ("stock-width-y", "stock-dialog-width"),
                        ("board-thickness", "stock-dialog-thickness"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        stock_length_field(
                            &mut columns[index],
                            localizer,
                            id,
                            &localizer.text(key),
                            &mut draft.dimensions[index],
                            unit,
                            &unit_text,
                            false,
                        );
                    }
                });
                gap(ui);
                field_label(ui, &localizer.text("stock-grain-heading"));
                let grains = [
                    StockGrain::AlongX,
                    StockGrain::AlongY,
                    StockGrain::Nondirectional,
                    StockGrain::Unknown,
                ]
                .map(|grain| (grain, localizer.text(stock_grain_short_key(grain))));
                if let Some(grain) = full_segmented(
                    ui,
                    egui::Id::new("stock-dialog-grain"),
                    draft.grain,
                    &grains,
                    true,
                ) {
                    draft.grain = grain;
                }
                gap(ui);
                let price_invalid = !draft.price.trim().is_empty()
                    && Money::parse(project.currency, &draft.price).is_err();
                ui.columns(2, |columns| {
                    field_label(&mut columns[0], &localizer.text("stock-source"));
                    let sources = [
                        (StockSource::Owned, localizer.text("stock-owned")),
                        (StockSource::ToPurchase, localizer.text("stock-purchase")),
                    ];
                    if let Some(source) = full_segmented(
                        &mut columns[0],
                        egui::Id::new("stock-dialog-source"),
                        draft.source,
                        &sources,
                        true,
                    ) {
                        draft.source = source;
                    }
                    let price_label = localizer.text("stock-price-short");
                    field_label(&mut columns[1], &price_label)
                        .on_hover_text(localizer.text("stock-price"));
                    let mut args = FluentArgs::new();
                    args.set("currency", project.currency.code());
                    let error = price_invalid.then(|| localizer.text("error-invalid-amount"));
                    fitted_unit_field(
                        &mut columns[1],
                        egui::Id::new("stock-dialog-price"),
                        &localizer.text("stock-price"),
                        &mut draft.price,
                        &localizer.format("stock-per-piece", Some(&args)),
                        error.as_deref(),
                    );
                });
                if draft.edit_id.is_none() {
                    gap(ui);
                    field_label(ui, &localizer.text("stock-quantity-short"))
                        .on_hover_text(localizer.text("stock-quantity"));
                    ui.allocate_ui_with_layout(
                        egui::vec2(140.0, 30.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            tw::unit_field(
                                ui,
                                egui::Id::new("stock-dialog-quantity"),
                                &localizer.text("stock-quantity"),
                                &mut draft.quantity,
                                &localizer.text("stock-pcs"),
                                None,
                            );
                        },
                    );
                }
                gap(ui);
                field_label(ui, &localizer.text("stock-edge-trims"))
                    .on_hover_text(localizer.text("stock-trim-hint"));
                ui.columns(4, |columns| {
                    for (index, (key, id)) in [
                        ("stock-trim-short-left", "stock-dialog-trim-left"),
                        ("stock-trim-short-right", "stock-dialog-trim-right"),
                        ("stock-trim-short-bottom", "stock-dialog-trim-bottom"),
                        ("stock-trim-short-top", "stock-dialog-trim-top"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        stock_length_field(
                            &mut columns[index],
                            localizer,
                            id,
                            &localizer.text(key),
                            &mut draft.trim[index],
                            Unit::Mm,
                            "mm",
                            true,
                        );
                    }
                });
                let quantity = draft
                    .quantity
                    .parse::<u32>()
                    .ok()
                    .filter(|&n| n > 0 && n <= MAX_STOCK_QUANTITY);
                let parsed = draft.input(project.display_unit, project.currency);
                let error_line = |ui: &mut egui::Ui, text: String| {
                    ui.label(egui::RichText::new(text).size(11.5).color(tw::DANGER));
                };
                if !current
                    || draft.error.is_some()
                    || parsed.is_err()
                    || (draft.edit_id.is_none() && quantity.is_none())
                {
                    ui.add_space(4.0);
                }
                if !current {
                    error_line(ui, localizer.text("stock-stale"));
                }
                if let Some(error) = draft.error.or_else(|| parsed.as_ref().err().copied()) {
                    let key = match error {
                        StockError::MissingMaterial(_)
                        | StockError::Invalid(StockField::Material) => {
                            Some("error-material-missing")
                        }
                        // Shown inline under the price field already.
                        StockError::Invalid(StockField::Price) if draft.error.is_none() => None,
                        StockError::Invalid(StockField::Price) => Some("error-invalid-amount"),
                        StockError::Invalid(StockField::Quantity) => Some("stock-invalid-quantity"),
                        StockError::Invalid(StockField::Trim) => Some("stock-invalid-trim"),
                        // Blank sizes simply keep the confirm button disabled.
                        _ if draft.dimensions.iter().any(|d| d.text.trim().is_empty()) => None,
                        _ => Some("stock-invalid"),
                    };
                    if let Some(key) = key {
                        error_line(ui, localizer.text(key));
                    }
                }
                if draft.edit_id.is_none() && quantity.is_none() {
                    error_line(ui, localizer.text("stock-invalid-quantity"));
                }
                (
                    (),
                    current && parsed.is_ok() && (draft.edit_id.is_some() || quantity.is_some()),
                )
            },
        );
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm) {
            let input = draft
                .input(
                    self.editor.project().display_unit,
                    self.editor.project().currency,
                )
                .expect("validated");
            let result = if let Some(id) = draft.edit_id {
                self.editor.edit_stock(id, input).map(|_| ())
            } else {
                self.editor
                    .create_stock(input, draft.quantity.parse().expect("validated"))
                    .map(|_| ())
            };
            match result {
                Ok(()) => {
                    self.material_conflicts = allocation_conflicts(self.editor.project());
                    chrome.close(ctx);
                    return;
                }
                Err(plan_my_cabinet::commands::EditError::Command(error)) => {
                    draft.error = Some(error)
                }
                Err(_) => draft.error = Some(StockError::Invalid(StockField::Material)),
            }
        }
        draft.chrome = chrome;
        self.modals.set_stock(Some(draft));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::board_commands::NewMaterial;

    #[test]
    fn inspector_grain_and_ownership_commit_atomically_and_undo() {
        let project = plan_my_cabinet::reference_fixture::project();
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
        let id = app.editor.project().stock[0].id;
        let before = app.editor.project().clone();
        let original = before.stock[0].clone();
        let grain = if original.grain == StockGrain::AlongY {
            StockGrain::AlongX
        } else {
            StockGrain::AlongY
        };
        app.apply_stock_action(StockUiAction::Grain(id, grain));
        assert_eq!(app.editor.project().revision, before.revision + 1);
        let edited = &app.editor.project().stock[0];
        assert_eq!(edited.grain, grain);
        assert_eq!(
            (
                edited.length,
                edited.width,
                edited.trim,
                edited.price,
                edited.source
            ),
            (
                original.length,
                original.width,
                original.trim,
                original.price,
                original.source
            )
        );
        app.editor.undo().unwrap();
        assert_eq!(app.editor.project().stock, before.stock);
        let source = if original.source == StockSource::Owned {
            StockSource::ToPurchase
        } else {
            StockSource::Owned
        };
        app.apply_stock_action(StockUiAction::Source(id, source));
        assert_eq!(app.editor.project().stock[0].source, source);
        assert_eq!(app.editor.project().stock[0].grain, original.grain);
    }

    #[test]
    fn explicit_free_fee_is_known_zero_not_unknown() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DesktopApp::default();
        app.modals
            .set_cut_fee(Some(CutFeeDialog::new(String::new())));
        let modal_frame = |app: &mut DesktopApp| {
            ctx.begin_pass(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            });
            app.show_cut_fee_dialog(&ctx);
            ctx.end_pass()
        };
        let output = modal_frame(&mut app);
        let free_action = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, node)| node.label() == Some("Free (0)"));
        output.drop_without_applying_deltas();
        assert!(free_action);
        app.modals
            .set_cut_fee(Some(CutFeeDialog::new(known_free_fee_text())));
        let text = &app.modals.cut_fee().unwrap().text;
        assert_eq!(text, "0");
        assert_eq!(app.editor.project().cut_fee, None);
        let fee = Money::parse(
            app.editor.project().currency,
            text,
        )
        .unwrap();
        app.editor.set_cut_fee(Some(fee)).unwrap();
        assert_eq!(
            app.editor.project().cut_fee,
            Some(Money::new(app.editor.project().currency, 0).unwrap())
        );
    }

    #[test]
    fn stock_list_exposes_alias_name_rank_and_identity_to_accessibility() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let id = app
            .editor
            .create_stock(
                StockInput {
                    name: "Offcut".into(),
                    material_id,
                    length: Length::from_micrometres(900_000),
                    width: Length::from_micrometres(240_000),
                    thickness: Length::from_micrometres(18_000),
                    grain: StockGrain::Unknown,
                    source: StockSource::Owned,
                    price: None,
                    trim: [Length::ZERO; 4],
                },
                1,
            )
            .unwrap()[0];
        let output = ctx.run_ui(egui::RawInput::default(), |ui| app.show_stock_list(ui));
        let expected = String::from("O1 · Offcut");
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
        assert!(labels.iter().any(|label| label == &expected), "{labels:?}");
        assert!(
            !labels
                .iter()
                .any(|label| label.contains(&id.to_string()[..8])),
            "{labels:?}"
        );
        assert!(labels.iter().any(|label| label == "Drag"), "{labels:?}");
    }

    #[test]
    fn modal_cancel_and_invalid_quantity_do_not_commit() {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let initial = app.editor.project().clone();
        let mut draft = StockDialog::new(app.editor.project());
        draft.material_id = Some(material_id);
        draft.dimensions = ["900", "240", "18"].map(|text| DimensionDraft {
            text: text.into(),
            consent: false,
        });
        draft.quantity = "0".into();
        app.modals.set_stock(Some(draft));
        let draw = |app: &mut DesktopApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.show_stock_dialog(ui.ctx());
                },
            )
            .drop_without_applying_deltas();
        };
        draw(&mut app, vec![]);
        assert!(app.modals.stock().is_some());
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(egui::Id::new("stock-dialog-name"))
        );
        assert_eq!(app.editor.project(), &initial);
        draw(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(app.modals.stock().is_some());
        assert_eq!(app.editor.project(), &initial);
        draw(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(app.modals.stock().is_none());
        assert_eq!(app.editor.project(), &initial);
        assert_eq!(app.editor.project().revision, initial.revision);
    }

    #[test]
    fn valid_stock_enter_commits_once_and_popup_enter_keeps_draft() {
        let ctx = egui::Context::default();
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let mut draft = StockDialog::new(app.editor.project());
        draft.name = "Sheet".into();
        draft.material_id = Some(material_id);
        draft.dimensions = ["900", "240", "18"].map(|text| DimensionDraft {
            text: text.into(),
            consent: false,
        });
        app.modals.set_stock(Some(draft));
        let draw = |app: &mut DesktopApp, events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    app.show_stock_dialog(ui.ctx());
                },
            )
            .drop_without_applying_deltas();
        };
        let enter = || {
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        draw(&mut app, vec![]);
        let before = app.editor.project().revision;
        let popup = egui::Id::new("stock-test-popup");
        egui::Popup::open_id(&ctx, popup);
        draw(&mut app, enter());
        assert!(app.modals.stock().is_some());
        assert_eq!(app.editor.project().revision, before);
        egui::Popup::close_id(&ctx, popup);
        draw(&mut app, vec![]);
        draw(&mut app, enter());
        assert!(app.modals.stock().is_none());
        assert_eq!(app.editor.project().revision, before + 1);
        assert_eq!(
            app.editor.project().stock[0].length,
            Length::from_micrometres(900_000)
        );
        assert_eq!(
            app.editor.project().stock[0].width,
            Length::from_micrometres(240_000)
        );
        assert_eq!(
            app.editor.project().stock[0].thickness,
            Length::from_micrometres(18_000)
        );
    }

    #[test]
    fn rounded_trims_each_require_consent_and_edit_preserves_exact_values() {
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let mut draft = StockDialog::new(app.editor.project());
        draft.material_id = Some(material_id);
        draft.dimensions = ["900", "240", "18"].map(|text| DimensionDraft {
            text: text.into(),
            consent: false,
        });
        draft.trim[0].text = "1/64 in".into();
        draft.trim[1].text = "1/64 in".into();
        assert_eq!(
            draft
                .input(Unit::Mm, app.editor.project().currency)
                .unwrap_err(),
            StockError::Invalid(StockField::Trim)
        );
        draft.trim[0].consent = true;
        assert_eq!(
            draft
                .input(Unit::Mm, app.editor.project().currency)
                .unwrap_err(),
            StockError::Invalid(StockField::Trim)
        );
        draft.trim[1].consent = true;
        let input = draft
            .input(Unit::Mm, app.editor.project().currency)
            .unwrap();
        assert_eq!(
            input.trim,
            [
                Length::from_micrometres(397),
                Length::from_micrometres(397),
                Length::ZERO,
                Length::ZERO
            ]
        );
        let id = app.editor.create_stock(input, 1).unwrap()[0];
        let piece = app
            .editor
            .project()
            .stock
            .iter()
            .find(|piece| piece.id == id)
            .unwrap();
        let edited = StockDialog::edit(app.editor.project(), piece, Locale::En);
        assert_eq!(edited.trim[0].text, "0.397");
        assert!(edited.trim.iter().all(|field| !field.consent));
        assert_eq!(
            edited
                .input(Unit::Mm, app.editor.project().currency)
                .unwrap()
                .trim,
            piece.trim
        );
    }

    #[test]
    fn invalid_trim_does_not_modify_project() {
        let mut app = DesktopApp::default();
        let material_id = app
            .editor
            .create_material(NewMaterial {
                name: "Plywood".into(),
                thickness: Length::from_micrometres(18_000),
                grain: BoardGrain::Length,
            })
            .unwrap();
        let initial = app.editor.project().clone();
        let mut draft = StockDialog::new(app.editor.project());
        draft.material_id = Some(material_id);
        draft.dimensions = ["900", "240", "18"].map(|text| DimensionDraft {
            text: text.into(),
            consent: false,
        });
        draft.trim[0].text = "-1 mm".into();
        assert_eq!(
            draft.input(Unit::Mm, initial.currency).unwrap_err(),
            StockError::Invalid(StockField::Trim)
        );
        app.modals.set_stock(Some(draft));
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_stock_dialog(ui.ctx())
        })
        .drop_without_applying_deltas();
        assert!(app.modals.stock().is_some());
        assert_eq!(app.editor.project(), &initial);
    }
}
