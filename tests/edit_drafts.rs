use plan_my_cabinet::board_dimensions::{BoardDimension, BoardSelection, SelectionValue};
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::dimension_input::Locale;
use plan_my_cabinet::domain::{Board, BoardGrain, Material, Project};
use plan_my_cabinet::edit_drafts::{BatchDraft, BoardDraft, DraftError, EditDrafts, PoseDraft};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::placement::CoordinateFrame;
use plan_my_cabinet::units::{Anchor, Length, Pose, Quaternion, Unit};
use uuid::Uuid;

fn mm(value: i64) -> Length {
    Length::from_micrometres(value * 1000)
}

fn fixture() -> (ProjectEditor, Uuid, Uuid) {
    let mut project = Project::new("Draft", Currency::Brl);
    let material_id = Uuid::new_v4();
    project.materials.push(Material {
        default_band: None,
        kind: Default::default(),
        id: material_id,
        name: "Plywood".into(),
        default_thickness: mm(18),
        default_grain: BoardGrain::Unrestricted,
    });
    let ids = [Uuid::new_v4(), Uuid::new_v4()];
    for (index, id) in ids.into_iter().enumerate() {
        project.boards.push(Board {
            banding: Default::default(),
            id,
            name: format!("Board {index}"),
            material_id,
            length: if index == 0 { mm(100) } else { mm(120) },
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([index as f64 * 200.0, 0.0, 0.0], Quaternion::IDENTITY).unwrap(),
        });
    }
    (ProjectEditor::new(project).unwrap(), ids[0], ids[1])
}

#[test]
fn inspector_and_hud_share_exact_original_and_one_undo() {
    let (mut editor, id, _) = fixture();
    // The canonical original is more precise than a two-decimal field.
    let preview = editor
        .preview_board_dimension(
            id,
            BoardDimension::Length,
            Length::from_micrometres(12_345),
            Anchor::Start,
        )
        .unwrap();
    editor.edit_board_dimension(preview).unwrap();
    let revision = editor.project().revision;
    let mut drafts = EditDrafts::default();
    let first = drafts.board(&editor, id, Unit::Mm, Locale::En).unwrap();
    assert_eq!(first.length.committed, Length::from_micrometres(12_345));
    assert_eq!(first.length.display(), "12.35 mm");
    assert!(!first.dirty());
    assert!(!first.accept(&mut editor).unwrap());
    assert_eq!(editor.project().revision, revision);
    drafts
        .board(&editor, id, Unit::Mm, Locale::En)
        .unwrap()
        .length
        .edit("invalid");
    assert!(matches!(
        drafts
            .board(&editor, id, Unit::Mm, Locale::En)
            .unwrap()
            .values(),
        Err(DraftError::InvalidField { axis: 0, .. })
    ));
    assert_eq!(editor.project().revision, revision);
    let hud = drafts.board(&editor, id, Unit::Mm, Locale::En).unwrap();
    hud.length.edit("20 mm");
    hud.width.edit("60 mm");
    hud.anchor = Anchor::End;
    assert!(hud.accept(&mut editor).unwrap());
    assert_eq!(editor.project().revision, revision + 1);
    let board = editor.project().boards.iter().find(|b| b.id == id).unwrap();
    assert_eq!([board.length, board.width], [mm(20), mm(60)]);
    editor.undo().unwrap();
    let board = editor.project().boards.iter().find(|b| b.id == id).unwrap();
    assert_eq!(
        [board.length, board.width],
        [Length::from_micrometres(12_345), mm(50)]
    );
}

#[test]
fn rounding_consent_resets_and_cancel_never_mutates() {
    let (mut editor, id, _) = fixture();
    let original = editor.project().clone();
    let mut draft = BoardDraft::new(&editor, id, Unit::Inch, Locale::En).unwrap();
    draft.length.edit("1/64 in");
    assert!(matches!(
        draft.values(),
        Err(DraftError::RoundingConsent { axis: 0, .. })
    ));
    draft.length.consent = true;
    assert_eq!(draft.values().unwrap()[0], Length::from_micrometres(397));
    draft.length.edit("3/64 in");
    assert!(matches!(
        draft.accept(&mut editor),
        Err(DraftError::RoundingConsent { axis: 0, .. })
    ));
    draft.cancel();
    assert_eq!(editor.project(), &original);
    assert!(!editor.can_undo());
}

#[test]
fn invalid_second_axis_does_not_commit_valid_first_axis() {
    let (mut editor, id, _) = fixture();
    let before = editor.project().clone();
    let mut draft = BoardDraft::new(&editor, id, Unit::Mm, Locale::PtBr).unwrap();
    draft.length.edit("110 mm");
    draft.width.edit("0 mm");
    assert!(matches!(
        draft.accept(&mut editor),
        Err(DraftError::InvalidField { axis: 1, .. })
    ));
    assert_eq!(editor.project(), &before);
    assert!(!editor.can_undo());
    draft.width.edit("52,5 mm");
    assert!(draft.accept(&mut editor).unwrap());
    assert_eq!(
        editor.project().boards[0].width,
        Length::from_micrometres(52_500)
    );
}

#[test]
fn mixed_batch_previews_and_commits_once_and_rejects_stale_source() {
    let (mut editor, first, second) = fixture();
    let selection = vec![BoardSelection::Board(first), BoardSelection::Board(second)];
    let mut draft = BatchDraft::new(
        &editor,
        selection.clone(),
        BoardDimension::Length,
        Unit::Mm,
        Locale::En,
    )
    .unwrap();
    assert_eq!(draft.original, SelectionValue::Mixed);
    assert_eq!(draft.display(), None);
    assert!(!draft.accept(&mut editor).unwrap());
    draft.anchors[0].1 = Anchor::Start;
    draft.anchors[1].1 = Anchor::End;
    draft.edit("140 mm");
    let preview = draft.before_after(&editor).unwrap();
    assert_eq!(preview[0].1, mm(100));
    assert_eq!(preview[1].1, mm(120));
    assert!(draft.accept(&mut editor).unwrap());
    assert_eq!(editor.project().revision, 1);
    editor.undo().unwrap();
    assert_eq!(editor.project().boards[0].length, mm(100));
    assert_eq!(editor.project().boards[1].length, mm(120));
    let stale = BatchDraft::new(
        &editor,
        selection,
        BoardDimension::Length,
        Unit::Mm,
        Locale::En,
    )
    .unwrap();
    editor.set_grid_spacing(mm(20)).unwrap();
    assert!(matches!(
        stale.before_after(&editor),
        Err(DraftError::Stale)
    ));
}

#[test]
fn stale_project_and_pose_cancel_or_accept() {
    let (mut editor, id, _) = fixture();
    let draft = BoardDraft::new(&editor, id, Unit::Mm, Locale::En).unwrap();
    editor.set_grid_spacing(mm(20)).unwrap();
    assert!(matches!(draft.preview(&editor), Err(DraftError::Stale)));

    let mut pose = PoseDraft::new(
        &editor,
        id,
        CoordinateFrame::LocalParent,
        Unit::Mm,
        Locale::En,
    )
    .unwrap();
    pose.position[0].edit("12.5 mm");
    assert_eq!(pose.preview(&editor).unwrap().translation_mm[0], 12.5);
    let original = editor.project().clone();
    pose.cancel();
    assert_eq!(editor.project(), &original);
    pose.rotation[2] = Some("90".into());
    assert!(pose.accept(&mut editor).unwrap());
    editor.undo().unwrap();
    assert_eq!(
        editor
            .project()
            .boards
            .iter()
            .find(|b| b.id == id)
            .unwrap()
            .pose,
        original.boards[0].pose
    );
}

#[test]
fn pristine_draft_refreshes_but_stale_pending_text_is_never_replaced() {
    let (mut editor, id, _) = fixture();
    let mut drafts = EditDrafts::default();
    drafts.board(&editor, id, Unit::Mm, Locale::En).unwrap();
    editor.set_grid_spacing(mm(20)).unwrap();
    assert!(
        !drafts
            .board(&editor, id, Unit::Cm, Locale::PtBr)
            .unwrap()
            .dirty()
    );
    drafts
        .board(&editor, id, Unit::Cm, Locale::PtBr)
        .unwrap()
        .length
        .edit("bad");
    editor.set_grid_spacing(mm(30)).unwrap();
    assert!(matches!(
        drafts.board(&editor, id, Unit::Mm, Locale::En),
        Err(DraftError::Stale)
    ));
    assert_eq!(
        drafts
            .existing_board(editor.project().id, id)
            .unwrap()
            .length
            .display(),
        "bad"
    );
    drafts.cancel_board(editor.project().id, id);
    assert!(
        !drafts
            .board(&editor, id, Unit::Mm, Locale::En)
            .unwrap()
            .dirty()
    );
}

#[test]
fn pose_position_preserves_exact_quaternion_and_requires_its_own_consent() {
    let (mut editor, id, _) = fixture();
    let rotation = Quaternion::normalized(1.0, 0.2, 0.3, 0.4).unwrap();
    editor
        .transact(|project| {
            project.boards[0].pose.rotation = rotation;
            Ok::<_, ()>(())
        })
        .unwrap();
    let before = editor.project().boards[0].pose;
    let revision = editor.project().revision;
    let mut drafts = EditDrafts::default();
    let pose = drafts
        .pose(
            &editor,
            id,
            CoordinateFrame::LocalParent,
            Unit::Inch,
            Locale::En,
        )
        .unwrap();
    pose.position[0].edit("1/64 in");
    assert!(matches!(
        pose.accept(&mut editor),
        Err(DraftError::RoundingConsent { axis: 0, .. })
    ));
    assert_eq!(editor.project().revision, revision);
    let pose = drafts
        .pose(
            &editor,
            id,
            CoordinateFrame::LocalParent,
            Unit::Inch,
            Locale::En,
        )
        .unwrap();
    pose.position[0].consent = true;
    assert!(pose.accept(&mut editor).unwrap());
    assert_eq!(editor.project().boards[0].pose.rotation, before.rotation);
    editor.undo().unwrap();
    assert_eq!(editor.project().boards[0].pose, before);
}

#[test]
fn pose_invalid_rotation_never_mutates_and_entry_units_survive_presentation_switches() {
    let (mut editor, id, _) = fixture();
    let initial = editor.project().clone();
    let mut drafts = EditDrafts::default();
    let pose = drafts
        .pose(
            &editor,
            id,
            CoordinateFrame::LocalParent,
            Unit::Inch,
            Locale::En,
        )
        .unwrap();
    pose.position[0].edit("1/64");
    pose.rotation[2] = Some("NaN".into());
    let pose = drafts
        .pose(
            &editor,
            id,
            CoordinateFrame::LocalParent,
            Unit::Mm,
            Locale::PtBr,
        )
        .unwrap();
    assert_eq!(pose.position[0].parsed().unwrap().unwrap().unit, Unit::Inch);
    assert!(matches!(
        pose.accept(&mut editor),
        Err(DraftError::RoundingConsent { axis: 0, .. })
    ));
    pose.position[0].consent = true;
    assert!(matches!(
        pose.accept(&mut editor),
        Err(DraftError::InvalidRotation(2))
    ));
    assert_eq!(editor.project(), &initial);
    pose.rotation[2] = Some("90".into());
    assert!(pose.accept(&mut editor).unwrap());
    assert_eq!(editor.project().boards[0].pose.translation_mm[0], 0.397);
    editor.undo().unwrap();
    assert_eq!(editor.project().boards[0].pose, initial.boards[0].pose);
}

#[test]
fn pending_unsuffixed_input_freezes_entry_unit_and_locale_until_resolution() {
    let (mut editor, id, _) = fixture();
    let revision = editor.project().revision;
    let mut drafts = EditDrafts::default();
    let draft = drafts.board(&editor, id, Unit::Mm, Locale::PtBr).unwrap();
    draft.length.edit("1,5");
    draft.width.edit("bad");
    let draft = drafts.board(&editor, id, Unit::Cm, Locale::En).unwrap();
    assert_eq!(draft.length.display(), "1,5");
    assert_eq!(draft.length.parsed().unwrap().unwrap().unit, Unit::Mm);
    assert!(matches!(
        draft.values(),
        Err(DraftError::InvalidField { axis: 1, .. })
    ));
    assert_eq!(editor.project().revision, revision);
    draft.width.cancel();
    assert_eq!(draft.width.display(), "5.00 cm");
    assert_eq!(draft.values().unwrap()[0], Length::from_micrometres(1_500));
    draft.width.edit("6");
    assert_eq!(draft.values().unwrap()[1], mm(60));
    assert!(draft.accept(&mut editor).unwrap());
    assert_eq!(
        editor.project().boards[0].length,
        Length::from_micrometres(1_500)
    );
    assert_eq!(editor.project().boards[0].width, mm(60));
    assert_eq!(editor.project().revision, revision + 1);
    editor.undo().unwrap();
    assert_eq!(editor.project().boards[0].length, mm(100));
}

#[test]
fn explicit_suffix_consent_survives_preferences_but_not_text_edits() {
    let (editor, id, _) = fixture();
    let mut drafts = EditDrafts::default();
    let draft = drafts.board(&editor, id, Unit::Inch, Locale::PtBr).unwrap();
    draft.length.edit("1/64 in");
    draft.length.consent = true;
    let draft = drafts.board(&editor, id, Unit::Cm, Locale::En).unwrap();
    assert_eq!(draft.length.parsed().unwrap().unwrap().unit, Unit::Inch);
    assert_eq!(draft.values().unwrap()[0], Length::from_micrometres(397));
    draft.length.edit("3/64 in");
    assert!(
        matches!(draft.values(), Err(DraftError::RoundingConsent { axis: 0, rounded_mm, .. }) if rounded_mm.contains(','))
    );
    draft.length.cancel();
    assert_eq!(draft.length.display(), "10.00 cm");
}

#[test]
fn unsuffixed_inch_consent_and_proposal_survive_unit_and_language_switch() {
    let (mut editor, id, _) = fixture();
    let mut drafts = EditDrafts::default();
    let draft = drafts.board(&editor, id, Unit::Inch, Locale::En).unwrap();
    draft.length.edit("1/64");
    assert!(matches!(
        draft.values(),
        Err(DraftError::RoundingConsent { axis: 0, .. })
    ));
    draft.length.consent = true;
    let draft = drafts.board(&editor, id, Unit::Mm, Locale::PtBr).unwrap();
    assert_eq!(draft.length.display(), "1/64");
    assert_eq!(draft.length.parsed().unwrap().unwrap().unit, Unit::Inch);
    assert!(draft.length.consent);
    assert_eq!(draft.values().unwrap()[0], Length::from_micrometres(397));
    assert!(draft.accept(&mut editor).unwrap());
    assert_eq!(
        editor.project().boards[0].length,
        Length::from_micrometres(397)
    );
}

#[test]
fn batch_and_pose_fields_keep_pending_entry_unit() {
    let (editor, first, second) = fixture();
    let mut batch = BatchDraft::new(
        &editor,
        vec![BoardSelection::Board(first), BoardSelection::Board(second)],
        BoardDimension::Length,
        Unit::Mm,
        Locale::PtBr,
    )
    .unwrap();
    batch.edit("140");
    batch.set_presentation(Unit::Cm, Locale::En);
    assert!(
        batch
            .before_after(&editor)
            .unwrap()
            .iter()
            .all(|(_, _, after, _)| *after == mm(140))
    );
    batch.cancel();
    batch.edit("14");
    assert!(
        batch
            .before_after(&editor)
            .unwrap()
            .iter()
            .all(|(_, _, after, _)| *after == mm(140))
    );

    let mut drafts = EditDrafts::default();
    drafts
        .pose(
            &editor,
            first,
            CoordinateFrame::LocalParent,
            Unit::Mm,
            Locale::PtBr,
        )
        .unwrap()
        .position[0]
        .edit("12,5");
    let pose = drafts
        .pose(
            &editor,
            first,
            CoordinateFrame::LocalParent,
            Unit::Cm,
            Locale::En,
        )
        .unwrap();
    assert_eq!(pose.preview(&editor).unwrap().translation_mm[0], 12.5);
    pose.cancel();
    assert_eq!(pose.position[0].display(), "0.00 cm");
}
