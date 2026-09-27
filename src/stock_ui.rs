//! Keyboard-operable stock form and priority controls; drafts never enter the project.
use super::*;
use crate::actions::{ActionId as A, Argument, Request, Target};
use plan_my_cabinet::cost_estimate::Feasibility;
use plan_my_cabinet::domain::{Stock, StockGrain, StockSource};
use plan_my_cabinet::money::{Money, MoneyLocale};
use plan_my_cabinet::stock_commands::{MAX_STOCK_QUANTITY, StockError, StockField, StockInput};
use plan_my_cabinet::stock_read_models::{StockPieceReadModel, StockReadModel};

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

fn show_stock_trim_preview(ui: &mut egui::Ui, piece: &StockPieceReadModel, localizer: &Localizer) {
    let width = ui.available_width().clamp(150.0, 316.0);
    let (area, response) = ui.allocate_exact_size(egui::vec2(width, 120.0), egui::Sense::hover());
    let locale = if localizer.language() == Language::En {
        Locale::En
    } else {
        Locale::PtBr
    };
    response.on_hover_text(format!(
        "{}: {} · {}: {} · {}: {} · {}: {}",
        localizer.text("stock-trim-left"),
        format_length(piece.trim[0], Unit::Mm, locale, 3),
        localizer.text("stock-trim-right"),
        format_length(piece.trim[1], Unit::Mm, locale, 3),
        localizer.text("stock-trim-bottom"),
        format_length(piece.trim[2], Unit::Mm, locale, 3),
        localizer.text("stock-trim-top"),
        format_length(piece.trim[3], Unit::Mm, locale, 3),
    ));
    let measured = egui::Rect::from_min_max(
        area.min + egui::vec2(44.0, 25.0),
        area.max - egui::vec2(44.0, 25.0),
    );
    let usable = trim_preview_rect(measured, piece);
    let painter = ui.painter_at(area);
    painter.rect_filled(measured, 3.0, egui::Color32::from_rgb(232, 224, 211));
    if usable.is_positive() {
        painter.rect_filled(usable, 2.0, egui::Color32::from_rgb(251, 250, 247));
        painter.rect_stroke(
            usable,
            2.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(125, 108, 83)),
            egui::StrokeKind::Inside,
        );
    }
    painter.rect_stroke(
        measured,
        3.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(125, 108, 83)),
        egui::StrokeKind::Inside,
    );
    let value = |length: Length| compact_mm(length, locale);
    for (position, align, label) in [
        (
            egui::pos2(measured.center().x, area.top()),
            egui::Align2::CENTER_TOP,
            value(piece.trim[3]),
        ),
        (
            egui::pos2(area.left(), measured.center().y),
            egui::Align2::LEFT_CENTER,
            value(piece.trim[0]),
        ),
        (
            egui::pos2(area.right(), measured.center().y),
            egui::Align2::RIGHT_CENTER,
            value(piece.trim[1]),
        ),
        (
            egui::pos2(measured.center().x, area.bottom()),
            egui::Align2::CENTER_BOTTOM,
            value(piece.trim[2]),
        ),
    ] {
        painter.text(
            position,
            align,
            label,
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(90, 82, 72),
        );
    }
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
                    .find(|(label, _, _, _)| label.contains("S1 ·"))
                    .expect("first stock row")
                    .1;
                let summary = text
                    .iter()
                    .find(|(label, _, _, _)| *label == app.localizer.text("cost-material"))
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
                assert!(
                    row - title < 220.0,
                    "table must start below title/actions, not below cards: {row} - {title}"
                );
                for (label, _, x, clip) in &text {
                    if ["cost-material", "cost-cutting", "cost-owned-consumed"]
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
            let zero = format_length(Length::ZERO, Unit::Mm, locale(&app), 3);
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
            assert!(labels.contains(long_stock), "{labels}");
            assert!(
                identities
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
        let draft = app.stock_dialog.as_ref().unwrap();
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
    fn global_position_button_moves_selected_piece_across_materials() {
        let project = plan_my_cabinet::reference_fixture::project();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DesktopApp {
            editor: ProjectEditor::new(project).unwrap(),
            ..Default::default()
        };
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
        app.session.stock_piece = Some(chosen);
        app.session.stock_global_target = 1;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| app.show_stock_list(ui));
        let button = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.label() == Some("Move selected globally"))
                    .then(|| node.bounds())
                    .flatten()
            })
            .expect("visible global-position action");
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
                |ui| app.show_stock_list(ui),
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(app.editor.project().ordered_stock()[0].id, chosen);
        assert_eq!(
            app.editor.project().stock_alias(chosen),
            Some(alias.as_str())
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
    chrome: Option<ModalChrome>,
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
            chrome: Some(
                ModalChrome::new(egui::Id::new("stock-dialog"))
                    .first_focus(egui::Id::new("stock-dialog-name"))
                    .width(540.0),
            ),
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
        let blank = self.dimensions[0].text.trim().is_empty() && self.dimensions[1].text.trim().is_empty();
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
            draft.dimensions[2] = DimensionDraft {
                text: format_length(material.default_thickness, Unit::Mm, Locale::En, 3),
                consent: false,
            };
        }
        draft
    }

    pub(super) fn new_for_issue(project: &Project, board_id: Uuid) -> Self {
        let mut draft = Self::new(project);
        if let Some(board) = project.boards.iter().find(|board| board.id == board_id) {
            draft.material_id = Some(board.material_id);
            draft.fill_from_preset(project);
            draft.dimensions[2] = DimensionDraft {
                text: format_length(board.thickness, Unit::Mm, Locale::En, 3),
                consent: false,
            };
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
            dimensions: [piece.length, piece.width, piece.thickness].map(|value| DimensionDraft {
                text: format_length(value, Unit::Mm, locale, 3),
                consent: false,
            }),
            trim: piece.trim.map(|value| DimensionDraft {
                text: format_length(value, Unit::Mm, locale, 3),
                consent: false,
            }),
            grain: piece.grain,
            source: piece.source,
            price: piece.price.map_or(String::new(), |p| {
                format!("{}.{:02}", p.minor_units() / 100, p.minor_units() % 100)
            }),
            quantity: "1".into(),
            error: None,
            chrome: Some(
                ModalChrome::new(egui::Id::new("stock-dialog"))
                    .first_focus(egui::Id::new("stock-dialog-name"))
                    .width(540.0),
            ),
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

fn trim_field(ui: &mut egui::Ui, localizer: &Localizer, label: &str, field: &mut DimensionDraft) {
    ui.horizontal(|ui| {
        ui.label(localizer.text(label));
        if ui.text_edit_singleline(&mut field.text).changed() {
            field.consent = false;
        }
    });
    match parse_length(&field.text, Unit::Mm) {
        Ok(parsed) if parsed.conversion.suggested().micrometres() >= 0 => {
            let value = parsed.conversion.suggested();
            let locale = if localizer.language() == Language::En {
                Locale::En
            } else {
                Locale::PtBr
            };
            if matches!(parsed.conversion, Conversion::NeedsConfirmation(_)) {
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("entered", field.text.as_str());
                args.set("rounded", format_length(value, Unit::Mm, locale, 3));
                ui.checkbox(
                    &mut field.consent,
                    localizer.format("rounding-confirmation", Some(&args)),
                );
            } else {
                ui.small(format_length(value, Unit::Mm, locale, 3));
            }
        }
        Ok(_) | Err(_) => {
            if !field.text.is_empty() {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    localizer.text("stock-invalid-trim"),
                );
            }
        }
    }
}

fn stock_grain_key(grain: StockGrain) -> &'static str {
    match grain {
        StockGrain::AlongX => "stock-grain-x",
        StockGrain::AlongY => "stock-grain-y",
        StockGrain::Nondirectional => "stock-grain-none",
        StockGrain::Unknown => "stock-grain-unknown",
    }
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

    pub(super) fn show_stock_materials(&mut self, ui: &mut egui::Ui) {
        crate::theme_widgets::section_header(ui, &self.localizer.text("stock-materials-heading"));
        ui.add(
            egui::TextEdit::singleline(&mut self.session.stock.filter)
                .hint_text(self.localizer.text("shell-filter-materials")),
        );
        let Some(model) = self.stock_snapshot() else {
            ui.label(self.localizer.text("cost-invalid"));
            return;
        };
        if ui
            .selectable_label(
                self.session.stock_material_filter.is_none(),
                format!(
                    "{} · {} {}",
                    self.localizer.text("stock-all-materials"),
                    model.pieces.len(),
                    self.localizer.text("stock-pieces-count")
                ),
            )
            .clicked()
        {
            self.session.stock_material_filter = None;
            self.session.stock_piece = None;
            self.session.inspector = None;
        }
        let query = self.session.stock.filter.to_lowercase();
        for material in model
            .materials
            .iter()
            .filter(|material| material.name.to_lowercase().contains(&query))
        {
            let selected = self.session.stock_material_filter == Some(material.id);
            egui::Frame::new()
                .fill(if selected {
                    crate::theme_widgets::ACCENT_BG
                } else {
                    crate::theme_widgets::PANEL
                })
                .corner_radius(7)
                .inner_margin(egui::Margin::symmetric(6, 5))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let color = self.editor.project().material_color(material.id);
                        let swatch = egui::Color32::from_rgb(color.0[0], color.0[1], color.0[2]);
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 3.0, swatch);
                        ui.painter().rect_stroke(
                            rect,
                            3.0,
                            egui::Stroke::new(0.8, egui::Color32::GRAY),
                            egui::StrokeKind::Inside,
                        );
                        if ui
                            .selectable_label(selected, &material.name)
                            .on_hover_text(material.id.to_string())
                            .clicked()
                        {
                            self.session.stock_material_filter = Some(material.id);
                            self.session.stock_global_order = false;
                            self.session.stock_piece = None;
                            self.session.inspector = Some(InspectorTarget::Material(material.id));
                        }
                    });
                    ui.small(format!(
                        "{} mm · {} {} · {} {}",
                        compact_mm(material.default_thickness, locale(self)),
                        material.board_count,
                        self.localizer.text("stock-boards-count"),
                        material.stock_piece_count,
                        self.localizer.text("stock-pieces-count")
                    ));
                    if material.board_count > 0 && material.stock_piece_count == 0 {
                        ui.colored_label(
                            egui::Color32::DARK_RED,
                            format!(
                                "{}: {}",
                                self.localizer.text("stock-material-without-stock"),
                                material.unallocated_board_count
                            ),
                        );
                        if ui
                            .add_enabled(
                                !self.modal_open(),
                                egui::Button::new(self.localizer.text("stock-new")),
                            )
                            .clicked()
                        {
                            let _ = self
                                .invoke(Request::with(A::NewStock, Target::Material(material.id)));
                        }
                    }
                    // Material editing remains available without expanding every list row.
                    ui.menu_button("⋯", |ui| {
                        if ui
                            .add_enabled(
                                !self.modal_open(),
                                egui::Button::new(self.localizer.text("material-edit")),
                            )
                            .clicked()
                        {
                            let _ = self.invoke(Request::with(
                                A::EditMaterial,
                                Target::Material(material.id),
                            ));
                            ui.close();
                        }
                    });
                });
        }
        if !model.materials.is_empty()
            && !model
                .materials
                .iter()
                .any(|material| material.name.to_lowercase().contains(&query))
        {
            ui.label(self.localizer.text("stock-filter-no-materials"));
        }
    }

    pub(super) fn show_stock_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading(self.localizer.text("stock-list"));
            if ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(self.localizer.text("stock-new")),
                )
                .clicked()
            {
                let _ = self.invoke(Request::new(A::NewStock));
            }
            let fee = self.editor.project().cut_fee.map_or_else(
                || self.localizer.text("stock-price-unknown"),
                |fee| fee.display(self.stock_money_locale()),
            );
            if ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(format!(
                        "{}: {} · {}",
                        self.localizer.text("cut-fee"),
                        fee,
                        self.localizer.text("cut-fee-edit")
                    )),
                )
                .clicked()
            {
                let _ = self.invoke(Request::new(A::EditCutFee));
            }
            if ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(format!(
                        "{} · {}",
                        self.editor.project().currency.code(),
                        self.localizer.text("currency-change-heading")
                    )),
                )
                .clicked()
            {
                let _ = self.invoke(Request::new(A::EditCurrency));
            }
        });
        let Some(model) = self.stock_snapshot() else {
            ui.colored_label(egui::Color32::DARK_RED, self.localizer.text("cost-invalid"));
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(
                &mut self.session.stock_global_order,
                false,
                self.localizer.text("stock-grouped-view"),
            );
            if ui
                .selectable_value(
                    &mut self.session.stock_global_order,
                    true,
                    self.localizer.text("stock-priority-view"),
                )
                .clicked()
            {
                self.session.stock_material_filter = None;
                self.session.stock.filter.clear();
            }
        });
        let rows = stock_visible_rows(
            &model,
            self.session.stock_material_filter,
            self.session.stock_global_order,
            &self.session.stock.filter,
        );
        let matching_material = model.materials.iter().any(|material| {
            self.session
                .stock_material_filter
                .is_none_or(|id| id == material.id)
                && material
                    .name
                    .to_lowercase()
                    .contains(&self.session.stock.filter.to_lowercase())
        });
        if rows.is_empty() && (!matching_material || self.session.stock_global_order) {
            ui.label(self.localizer.text(if model.materials.is_empty() {
                "stock-no-materials"
            } else if !self.session.stock.filter.is_empty()
                && !model.materials.iter().any(|material| {
                    material
                        .name
                        .to_lowercase()
                        .contains(&self.session.stock.filter.to_lowercase())
                })
            {
                "stock-filter-no-materials"
            } else if model.pieces.is_empty() {
                "stock-no-pieces"
            } else {
                "stock-filter-empty"
            }));
            if let Some(id) = self.session.stock_material_filter
                && ui
                    .add_enabled(
                        !self.modal_open(),
                        egui::Button::new(self.localizer.text("stock-new")),
                    )
                    .clicked()
            {
                let _ = self.invoke(Request::with(A::NewStock, Target::Material(id)));
            }
            self.show_stock_global_move(ui, model.pieces.len());
            self.show_stock_summary(ui, &model);
            return;
        }
        let mut action = None;
        let mut drag_stopped = false;
        let mut drag_targets = Vec::new();
        let groups: Vec<Option<Uuid>> = if self.session.stock_global_order {
            vec![None]
        } else {
            model
                .materials
                .iter()
                .filter(|material| {
                    self.session
                        .stock_material_filter
                        .is_none_or(|id| id == material.id)
                        && material
                            .name
                            .to_lowercase()
                            .contains(&self.session.stock.filter.to_lowercase())
                })
                .map(|material| Some(material.id))
                .collect()
        };
        for group in groups {
            if let Some(id) = group {
                let material = model
                    .materials
                    .iter()
                    .find(|material| material.id == id)
                    .unwrap();
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {} {}",
                        material.name,
                        material.stock_piece_count,
                        self.localizer.text("stock-pieces-count")
                    ))
                    .strong(),
                );
                if material.stock_piece_count == 0 && material.board_count > 0 {
                    ui.colored_label(
                        egui::Color32::DARK_RED,
                        format!(
                            "{} · {} {}",
                            self.localizer.text("stock-material-without-stock"),
                            material.unallocated_board_count,
                            self.localizer.text("stock-unallocated-count")
                        ),
                    );
                    if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("stock-new")),
                        )
                        .clicked()
                    {
                        action = Some(Request::with(A::NewStock, Target::Material(id)));
                    }
                }
            }
            let group_rows: Vec<_> = rows
                .iter()
                .copied()
                .filter(|piece| group.is_none_or(|id| piece.material_id == id))
                .collect();
            if group_rows.is_empty() {
                continue;
            }
            // The table is wider than a compact center pane. Keep its horizontal
            // movement local to the table: the global-move control and spending
            // cards below must remain within the visible center viewport.
            egui::ScrollArea::horizontal()
                .id_salt(("stock-priority-scroll", group))
                .show(ui, |ui| {
                    egui::Grid::new(("stock-priority-table", group))
                        .striped(true)
                        .spacing(egui::vec2(8.0, 3.0))
                        .show(ui, |ui| {
                            for key in [
                                "stock-global-rank",
                                "stock-piece-label",
                                "stock-piece-dimensions",
                                "stock-grain",
                                "stock-source",
                                "stock-price-heading",
                                "stock-parts-heading",
                                "stock-actions-heading",
                            ] {
                                ui.strong(self.localizer.text(key));
                                if key == "stock-grain" {
                                    ui.strong("∑ mm")
                                        .on_hover_text(self.localizer.text("stock-trim-hint"));
                                }
                            }
                            ui.end_row();
                            for (visible_index, piece) in group_rows.iter().enumerate() {
                                let target = if self.session.stock_global_order {
                                    piece.global_rank - 1
                                } else {
                                    visible_index
                                };
                                let rank_cell = ui
                                    .horizontal(|ui| {
                                        let handle = ui
                                            .add_enabled(
                                                !self.modal_open(),
                                                egui::Button::new(
                                                    self.localizer.text("stock-drag-handle"),
                                                )
                                                .sense(egui::Sense::click_and_drag()),
                                            )
                                            .on_hover_text(self.localizer.text("stock-drag-help"));
                                        if handle.drag_started() {
                                            self.session.stock_drag = Some(piece.id);
                                        }
                                        drag_stopped |= handle.drag_stopped();
                                        ui.monospace(format!("#{}", piece.global_rank));
                                    })
                                    .response;
                                drag_targets.push((
                                    piece.id,
                                    target,
                                    piece.global_rank,
                                    rank_cell.rect,
                                ));
                                let label = format!(
                                    "{} · {} ({})",
                                    piece.alias,
                                    piece.name,
                                    &piece.id.to_string()[..8]
                                );
                                let display = format!("{} · {}", piece.alias, piece.name);
                                let response = ui
                                    .selectable_label(
                                        self.session.stock_piece == Some(piece.id),
                                        display,
                                    )
                                    .on_hover_text(format!("{}\n{}", label, piece.id));
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::SelectableLabel,
                                        response.enabled(),
                                        &label,
                                    )
                                });
                                if response.clicked() {
                                    self.session.stock_piece = Some(piece.id);
                                }
                                ui.label(format!(
                                    "{} × {} × {} mm",
                                    compact_mm(piece.length, locale(self)),
                                    compact_mm(piece.width, locale(self)),
                                    compact_mm(piece.measured_thickness, locale(self))
                                ));
                                ui.label(self.localizer.text(stock_grain_key(piece.grain)));
                                ui.monospace(format!(
                                    "{} mm",
                                    compact_mm(
                                        piece.trim.iter().copied().fold(
                                            Length::ZERO,
                                            |sum, trim| {
                                                Length::from_micrometres(
                                                    sum.micrometres() + trim.micrometres(),
                                                )
                                            }
                                        ),
                                        locale(self)
                                    )
                                ));
                                ui.label(self.localizer.text(
                                    if piece.source == StockSource::Owned {
                                        "stock-owned"
                                    } else {
                                        "stock-purchase"
                                    },
                                ));
                                ui.label(piece.price.map_or_else(
                                    || self.localizer.text("stock-price-unknown"),
                                    |p| p.display(self.stock_money_locale()),
                                ));
                                ui.label(piece.parts.len().to_string());
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(
                                            !self.modal_open(),
                                            egui::Button::new(self.localizer.text("stock-edit")),
                                        )
                                        .clicked()
                                    {
                                        action = Some(Request::with(
                                            A::EditStock,
                                            Target::Stock(piece.id),
                                        ));
                                    }
                                    if ui
                                        .add_enabled(
                                            !self.modal_open() && target > 0,
                                            egui::Button::new(self.localizer.text("stock-up")),
                                        )
                                        .clicked()
                                    {
                                        action = Some(
                                            Request::with(A::StockMove, Target::Stock(piece.id))
                                                .argument(Argument::StockPriority {
                                                    target: target - 1,
                                                    subset: !self.session.stock_global_order,
                                                }),
                                        );
                                    }
                                    if ui
                                        .add_enabled(
                                            !self.modal_open()
                                                && target + 1
                                                    < if self.session.stock_global_order {
                                                        model.pieces.len()
                                                    } else {
                                                        group_rows.len()
                                                    },
                                            egui::Button::new(self.localizer.text("stock-down")),
                                        )
                                        .clicked()
                                    {
                                        action = Some(
                                            Request::with(A::StockMove, Target::Stock(piece.id))
                                                .argument(Argument::StockPriority {
                                                    target: target + 1,
                                                    subset: !self.session.stock_global_order,
                                                }),
                                        );
                                    }
                                });
                                ui.end_row();
                            }
                        })
                });
        }
        if let Some(dragged) = self.session.stock_drag
            && let Some(pointer) = ui.ctx().pointer_latest_pos()
        {
            let source_material = model
                .pieces
                .iter()
                .find(|row| row.id == dragged)
                .map(|row| row.material_id);
            if let Some((_, target, rank, rect)) = drag_targets.iter().find(|(id, _, _, rect)| {
                rect.contains(pointer)
                    && (self.session.stock_global_order
                        || model
                            .pieces
                            .iter()
                            .any(|row| row.id == *id && Some(row.material_id) == source_material))
            }) {
                ui.painter().rect_stroke(
                    *rect,
                    2.0,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(174, 113, 31)),
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
                        self.localizer.text(if self.session.stock_global_order {
                            "stock-global-scope"
                        } else {
                            "stock-visible-scope"
                        })
                    ));
                });
                if drag_stopped {
                    action = Some(
                        Request::with(A::StockMove, Target::Stock(dragged)).argument(
                            Argument::StockPriority {
                                target: *target,
                                subset: !self.session.stock_global_order,
                            },
                        ),
                    );
                }
            }
        }
        if drag_stopped {
            self.session.stock_drag = None;
        }
        if let Some(request) = action {
            let _ = self.invoke(request);
        }
        self.show_stock_global_move(ui, model.pieces.len());
        ui.add_space(12.0);
        self.show_stock_summary(ui, &model);
    }

    fn show_stock_global_move(&mut self, ui: &mut egui::Ui, count: usize) {
        ui.horizontal_wrapped(|ui| {
            ui.label(self.localizer.text("stock-global-position"));
            ui.add(
                egui::DragValue::new(&mut self.session.stock_global_target).range(1..=count.max(1)),
            );
            if ui
                .add_enabled(
                    !self.modal_open() && self.session.stock_piece.is_some() && count > 0,
                    egui::Button::new(self.localizer.text("stock-move-global")),
                )
                .on_hover_text(self.localizer.text("stock-priority-distinction"))
                .clicked()
                && let Some(id) = self.session.stock_piece
            {
                let _ = self.invoke(Request::with(A::StockMove, Target::Stock(id)).argument(
                    Argument::StockPriority {
                        target: self.session.stock_global_target - 1,
                        subset: false,
                    },
                ));
            }
        });
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
        let amount = |value: Option<Money>| {
            value.map_or_else(
                || self.localizer.text("stock-price-unknown"),
                |value| value.display(self.stock_money_locale()),
            )
        };
        let usage = model.usage_summary();
        let cards = [
            (
                "cost-material",
                amount(result.material),
                format!(
                    "{} {}",
                    usage.used_purchased_pieces,
                    self.localizer.text("stock-pieces-count")
                ),
            ),
            (
                "cost-cutting",
                amount(result.cutting),
                usage.physical_cuts.map_or_else(
                    || self.localizer.text("stock-cuts-unverified"),
                    |cuts| format!("{} {}", cuts, self.localizer.text("stock-physical-cuts")),
                ),
            ),
            (
                "cost-owned-consumed",
                usage.consumed_owned_pieces.to_string(),
                format!(
                    "{} {}",
                    usage.consumed_owned_pieces,
                    self.localizer.text("stock-pieces-count")
                ),
            ),
        ];
        let columns = if ui.available_width() >= 540.0 { 3 } else { 1 };
        ui.columns(columns, |uis| {
            for (index, (key, value, detail)) in cards.iter().enumerate() {
                crate::theme_widgets::card().show(&mut uis[index % columns], |ui| {
                    ui.set_min_width((ui.available_width() - 2.0).max(80.0));
                    ui.small(self.localizer.text(key));
                    ui.strong(value);
                    ui.small(detail);
                });
            }
        });
        ui.small(format!(
            "{}: {}",
            self.localizer.text("cost-total"),
            result.total.map_or_else(
                || self.localizer.text("cost-incomplete"),
                |value| value.display(self.stock_money_locale()),
            )
        ));
        if result.feasibility != Feasibility::Verified {
            ui.small(self.localizer.text("cost-feasibility"));
        }
        ui.small(self.localizer.text("cost-exclusions"));
        ui.collapsing(self.localizer.text("stock-cost-details"), |ui| {
            ui.small(self.localizer.text("cut-fee-hint"));
            ui.label(format!(
                "{}: {}",
                self.localizer.text("cost-material"),
                amount(result.material)
            ));
            ui.label(format!(
                "{}: {}",
                self.localizer.text("cost-cutting"),
                amount(result.cutting)
            ));
        });
    }

    pub(super) fn show_stock_inspector(&mut self, ui: &mut egui::Ui) {
        crate::theme_widgets::section_header(ui, &self.localizer.text("stock-inspector"));
        let Some(model) = self.stock_snapshot() else {
            ui.label(self.localizer.text("cost-invalid"));
            return;
        };
        let Some(piece) = self
            .session
            .stock_piece
            .and_then(|id| model.pieces.iter().find(|piece| piece.id == id))
        else {
            if let Some(id) = self.session.stock_material_filter
                && let Some(material) = model.materials.iter().find(|material| material.id == id)
            {
                ui.label(&material.name);
                ui.label(format!(
                    "{} {} · {} {}",
                    material.board_count,
                    self.localizer.text("stock-boards-count"),
                    material.stock_piece_count,
                    self.localizer.text("stock-pieces-count")
                ));
                if material.stock_piece_count == 0 && material.board_count > 0 {
                    ui.colored_label(
                        egui::Color32::DARK_RED,
                        self.localizer.text("stock-material-without-stock"),
                    );
                    if ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(self.localizer.text("stock-new")),
                        )
                        .clicked()
                    {
                        let _ = self.invoke(Request::with(A::NewStock, Target::Material(id)));
                    }
                }
                return;
            }
            ui.label(self.localizer.text("stock-inspector-empty"));
            return;
        };
        ui.strong(format!("{} · {}", piece.alias, piece.name));
        ui.small(format!(
            "{}: #{} · {}",
            self.localizer.text("stock-global-rank"),
            piece.global_rank,
            &piece.id.to_string()[..8]
        ));
        ui.separator();
        let mut edit = false;
        ui.small(self.localizer.text("stock-piece-dimensions"));
        egui::Grid::new("stock-inspector-size")
            .num_columns(2)
            .spacing(egui::vec2(8.0, 4.0))
            .show(ui, |ui| {
                for (key, value) in [
                    ("board-length", piece.length),
                    ("board-width", piece.width),
                    ("board-thickness", piece.measured_thickness),
                ] {
                    ui.label(self.localizer.text(key));
                    edit |= ui
                        .add_enabled(
                            !self.modal_open(),
                            egui::Button::new(format_length(value, Unit::Mm, locale(self), 3))
                                .min_size(egui::vec2(112.0, 24.0)),
                        )
                        .on_hover_text(self.localizer.text("stock-edit"))
                        .clicked();
                    ui.end_row();
                }
            });
        ui.small(format!(
            "{}: {} × {}",
            self.localizer.text("stock-usable"),
            format_length(piece.usable_extent[0], Unit::Mm, locale(self), 3),
            format_length(piece.usable_extent[1], Unit::Mm, locale(self), 3)
        ));
        ui.small(self.localizer.text("stock-grain"));
        ui.horizontal_wrapped(|ui| {
            for grain in [
                StockGrain::AlongX,
                StockGrain::AlongY,
                StockGrain::Nondirectional,
                StockGrain::Unknown,
            ] {
                edit |= ui
                    .add_enabled(
                        !self.modal_open(),
                        egui::Button::new(self.localizer.text(stock_grain_key(grain)))
                            .selected(piece.grain == grain),
                    )
                    .on_hover_text(self.localizer.text("stock-edit"))
                    .clicked();
            }
        });
        ui.small(self.localizer.text("stock-trim-hint"));
        show_stock_trim_preview(ui, piece, &self.localizer);
        // A translated trim label and numeric button cannot share a 316-point
        // inspector row. Stack them instead of allowing a grid to push the
        // editable value outside the visible pane at compact widths.
        for (key, trim) in [
            "stock-trim-left",
            "stock-trim-right",
            "stock-trim-top",
            "stock-trim-bottom",
        ]
        .into_iter()
        .zip([piece.trim[0], piece.trim[1], piece.trim[3], piece.trim[2]])
        {
            ui.small(self.localizer.text(key));
            edit |= ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(format_length(trim, Unit::Mm, locale(self), 3))
                        .min_size(egui::vec2(112.0, 24.0)),
                )
                .on_hover_text(self.localizer.text("stock-edit"))
                .clicked();
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(self.localizer.text("stock-source"));
            for (source, key) in [
                (StockSource::Owned, "stock-owned"),
                (StockSource::ToPurchase, "stock-purchase"),
            ] {
                edit |= ui
                    .add_enabled(
                        !self.modal_open(),
                        egui::Button::new(self.localizer.text(key))
                            .selected(piece.source == source),
                    )
                    .on_hover_text(self.localizer.text("stock-edit"))
                    .clicked();
            }
        });
        ui.horizontal(|ui| {
            ui.label(self.localizer.text("stock-price-heading"));
            edit |= ui
                .add_enabled(
                    !self.modal_open(),
                    egui::Button::new(piece.price.map_or_else(
                        || self.localizer.text("stock-price-unknown"),
                        |price| price.display(self.stock_money_locale()),
                    )),
                )
                .on_hover_text(self.localizer.text("stock-edit"))
                .clicked();
        });
        if edit {
            let _ = self.invoke(Request::with(A::EditStock, Target::Stock(piece.id)));
        }
        if ui
            .add_enabled(
                !self.modal_open(),
                egui::Button::new(self.localizer.text("stock-edit")),
            )
            .clicked()
        {
            let _ = self.invoke(Request::with(A::EditStock, Target::Stock(piece.id)));
        }
        if ui
            .button(self.localizer.text("stock-open-cut-plan"))
            .clicked()
        {
            let _ = self.request_navigation(NavigationRoute::Entity(Destination::Sheet(piece.id)));
        }
        ui.separator();
        ui.heading(self.localizer.text("stock-assigned-parts"));
        if piece.parts.is_empty() {
            ui.label(self.localizer.text("stock-no-assigned-parts"));
        }
        for part in &piece.parts {
            if ui
                .button(format!(
                    "{} · {}",
                    part.name,
                    &part.board_id.to_string()[..8]
                ))
                .clicked()
            {
                let _ = self
                    .request_navigation(NavigationRoute::Entity(Destination::Board(part.board_id)));
            }
        }
    }

    pub(super) fn show_cut_fee_dialog(&mut self, ctx: &egui::Context) {
        let state_id = egui::Id::new("cut-fee-modal-controller");
        let Some(mut text) = self.cut_fee_dialog.take() else {
            if let Some(controller) = ctx.data_mut(|data| {
                let controller =
                    data.get_temp::<std::sync::Arc<std::sync::Mutex<ModalChrome>>>(state_id);
                data.remove::<std::sync::Arc<std::sync::Mutex<ModalChrome>>>(state_id);
                controller
            }) {
                controller.lock().expect("fee modal controller").close(ctx);
            }
            return;
        };
        // The fee draft is stored by the host as a String. Keep its controller
        // with the egui context so it survives frames without changing host state.
        let controller = ctx.data_mut(|data| {
            data.get_temp::<std::sync::Arc<std::sync::Mutex<ModalChrome>>>(state_id)
                .unwrap_or_else(|| {
                    let controller = std::sync::Arc::new(std::sync::Mutex::new(
                        ModalChrome::new(egui::Id::new("cut-fee-dialog"))
                            .first_focus(egui::Id::new("cut-fee-amount")),
                    ));
                    data.insert_temp(state_id, controller.clone());
                    controller
                })
        });
        let mut chrome = controller.lock().expect("fee modal controller");
        let result = chrome.show(
            ctx,
            &self.localizer.text("cut-fee-edit"),
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &self.localizer.text("cut-fee-edit"),
            },
            |ui| {
                ui.label(format!(
                    "{} ({})",
                    self.localizer.text("cut-fee"),
                    self.editor.project().currency.code()
                ));
                ui.add(egui::TextEdit::singleline(&mut text).id(egui::Id::new("cut-fee-amount")));
                if ui.button(self.localizer.text("cut-fee-free")).clicked() {
                    text = known_free_fee_text();
                }
                ui.small(self.localizer.text("cut-fee-hint"));
                if !text.trim().is_empty()
                    && Money::parse(self.editor.project().currency, &text).is_err()
                {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text("error-invalid-amount"),
                    );
                }
                (
                    (),
                    text.trim().is_empty()
                        || Money::parse(self.editor.project().currency, &text).is_ok(),
                )
            },
        );
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            chrome.close(ctx);
            ctx.data_mut(|data| {
                data.remove::<std::sync::Arc<std::sync::Mutex<ModalChrome>>>(state_id)
            });
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm) {
            let fee = if text.trim().is_empty() {
                Ok(None)
            } else {
                Money::parse(self.editor.project().currency, &text).map(Some)
            };
            if let Ok(fee) = fee
                && self.editor.set_cut_fee(fee).is_ok()
            {
                chrome.close(ctx);
                ctx.data_mut(|data| {
                    data.remove::<std::sync::Arc<std::sync::Mutex<ModalChrome>>>(state_id)
                });
                return;
            }
        }
        self.cut_fee_dialog = Some(text);
    }

    pub(super) fn show_stock_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.stock_dialog.take() else {
            return;
        };
        let project = self.editor.project();
        let current = draft.project_id == project.id && draft.revision == project.revision;
        let title = self.localizer.text(if draft.edit_id.is_some() {
            "stock-edit"
        } else {
            "stock-new"
        });
        let mut chrome = draft.chrome.take().expect("stock modal controller");
        let result = chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &title,
            },
            |ui| {
                if let Some(id) = draft.edit_id {
                    ui.small(format!(
                        "{} · {}",
                        project.stock_alias(id).unwrap_or("?"),
                        id
                    ));
                }
                ui.horizontal(|ui| {
                    ui.label(self.localizer.text("stock-name"));
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.name)
                            .id(egui::Id::new("stock-dialog-name")),
                    );
                });
                let previous_material = draft.material_id;
                egui::ComboBox::from_label(self.localizer.text("material"))
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
                for (index, key) in ["board-length", "board-width", "board-thickness"]
                    .iter()
                    .enumerate()
                {
                    dimension_field(
                        ui,
                        &self.localizer,
                        key,
                        &mut draft.dimensions[index],
                        project.display_unit,
                    );
                }
                egui::ComboBox::from_label(self.localizer.text("stock-grain"))
                    .selected_text(self.localizer.text(stock_grain_key(draft.grain)))
                    .show_ui(ui, |ui| {
                        for grain in [
                            StockGrain::AlongX,
                            StockGrain::AlongY,
                            StockGrain::Nondirectional,
                            StockGrain::Unknown,
                        ] {
                            combo_option(
                                ui,
                                &mut draft.grain,
                                grain,
                                self.localizer.text(stock_grain_key(grain)),
                            );
                        }
                    });
                egui::ComboBox::from_label(self.localizer.text("stock-source"))
                    .selected_text(self.localizer.text(if draft.source == StockSource::Owned {
                        "stock-owned"
                    } else {
                        "stock-purchase"
                    }))
                    .show_ui(ui, |ui| {
                        for (source, key) in [
                            (StockSource::Owned, "stock-owned"),
                            (StockSource::ToPurchase, "stock-purchase"),
                        ] {
                            combo_option(ui, &mut draft.source, source, self.localizer.text(key));
                        }
                    });
                for (index, key) in [
                    "stock-trim-left",
                    "stock-trim-right",
                    "stock-trim-bottom",
                    "stock-trim-top",
                ]
                .iter()
                .enumerate()
                {
                    trim_field(ui, &self.localizer, key, &mut draft.trim[index]);
                }
                ui.label(self.localizer.text("stock-trim-hint"));
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{} ({})",
                        self.localizer.text("stock-price"),
                        project.currency.code()
                    ));
                    ui.text_edit_singleline(&mut draft.price);
                });
                if draft.edit_id.is_none() {
                    ui.horizontal(|ui| {
                        ui.label(self.localizer.text("stock-quantity"));
                        ui.text_edit_singleline(&mut draft.quantity);
                    });
                }
                let quantity = draft
                    .quantity
                    .parse::<u32>()
                    .ok()
                    .filter(|&n| n > 0 && n <= MAX_STOCK_QUANTITY);
                let parsed = draft.input(project.display_unit, project.currency);
                if !current {
                    ui.colored_label(egui::Color32::LIGHT_RED, self.localizer.text("stock-stale"));
                }
                if let Some(error) = draft.error.or_else(|| parsed.as_ref().err().copied()) {
                    let key = match error {
                        StockError::MissingMaterial(_)
                        | StockError::Invalid(StockField::Material) => "error-material-missing",
                        StockError::Invalid(StockField::Price) => "error-invalid-amount",
                        StockError::Invalid(StockField::Quantity) => "stock-invalid-quantity",
                        StockError::Invalid(StockField::Trim) => "stock-invalid-trim",
                        _ => "stock-invalid",
                    };
                    ui.colored_label(egui::Color32::LIGHT_RED, self.localizer.text(key));
                }
                if draft.edit_id.is_none() && quantity.is_none() {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        self.localizer.text("stock-invalid-quantity"),
                    );
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
        draft.chrome = Some(chrome);
        self.stock_dialog = Some(draft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::board_commands::NewMaterial;

    #[test]
    fn explicit_free_fee_is_known_zero_not_unknown() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = DesktopApp {
            cut_fee_dialog: Some(String::new()),
            ..Default::default()
        };
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
        app.cut_fee_dialog = Some(known_free_fee_text());
        assert_eq!(app.cut_fee_dialog.as_deref(), Some("0"));
        assert_eq!(app.editor.project().cut_fee, None);
        let fee = Money::parse(
            app.editor.project().currency,
            app.cut_fee_dialog.as_deref().unwrap(),
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
        let expected = format!("O1 · Offcut ({})", &id.to_string()[..8]);
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
        assert!(labels.iter().any(|label| label == "#1"), "{labels:?}");
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
        app.stock_dialog = Some(draft);
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
        assert!(app.stock_dialog.is_some());
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
        assert!(app.stock_dialog.is_some());
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
        assert!(app.stock_dialog.is_none());
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
        app.stock_dialog = Some(draft);
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
        assert!(app.stock_dialog.is_some());
        assert_eq!(app.editor.project().revision, before);
        egui::Popup::close_id(&ctx, popup);
        draw(&mut app, vec![]);
        draw(&mut app, enter());
        assert!(app.stock_dialog.is_none());
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
        assert_eq!(edited.trim[0].text, "0.397 mm");
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
        app.stock_dialog = Some(draft);
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.show_stock_dialog(ui.ctx())
        })
        .drop_without_applying_deltas();
        assert!(app.stock_dialog.is_some());
        assert_eq!(app.editor.project(), &initial);
    }
}
