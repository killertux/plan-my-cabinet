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
    let fee = Money::parse(app.editor.project().currency, text).unwrap();
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
