use super::*;
use plan_my_cabinet::reference_fixture;

fn field(project: &Project, target: Uuid, can_commit: bool) -> EditBlock {
    EditBlock {
        kind: EditKind::Field,
        source: Revision::of(project),
        workspace: Workspace::Design,
        target: Some(InspectorTarget::Board(target)),
        can_commit,
    }
}

#[test]
fn invalid_field_apply_stays_and_discard_routes_to_the_actual_target() {
    let project = reference_fixture::project();
    let first = project.boards[0].id;
    let second = project.boards[1].id;
    let mut editor = ProjectEditor::new(project).unwrap();
    let mut session = WorkspaceSession::new(editor.project());
    let mut selection = Selection::default();
    session.navigate(editor.project(), &mut selection, Destination::Board(first));
    let mut guard = NavigationGuard::default();
    let block = field(editor.project(), first, false);
    let intent = NavigationIntent::at(editor.project(), Route::Entity(Destination::Board(second)));
    assert_eq!(
        guard.request(
            intent,
            Some(block),
            editor.project(),
            &mut session,
            &mut selection
        ),
        Outcome::Prompt {
            kind: EditKind::Field,
            can_commit: false
        }
    );
    assert_eq!(guard.pending(), Some(intent));
    assert_eq!(
        guard
            .resolve(
                Decision::Commit,
                Some(block),
                &mut editor,
                &mut session,
                &mut selection,
                |_, _| -> Result<(), ()> { panic!("invalid apply must not run") }
            )
            .unwrap(),
        Outcome::Blocked(Blocked::InvalidCommit)
    );
    assert_eq!(selection.active, Some(first));
    assert_eq!(
        guard
            .resolve(
                Decision::Abandon,
                Some(block),
                &mut editor,
                &mut session,
                &mut selection,
                |_, _| Ok::<_, ()>(())
            )
            .unwrap(),
        Outcome::Navigated
    );
    assert_eq!(selection.active, Some(second));
    assert_eq!(guard.pending(), None);
}

#[test]
fn selecting_or_deselecting_a_scene_target_waits_for_the_edit_decision() {
    let project = reference_fixture::project();
    let first = project.boards[0].id;
    let second = project.boards[1].id;
    let mut editor = ProjectEditor::new(project).unwrap();
    let mut session = WorkspaceSession::new(editor.project());
    let mut selection = Selection::default();
    session.navigate(editor.project(), &mut selection, Destination::Board(first));
    let mut guard = NavigationGuard::default();
    for (target, additive) in [(Some(second), false), (None, false), (Some(first), true)] {
        let source = NavigationIntent::at(editor.project(), Route::Selection { target, additive });
        assert!(matches!(
            guard.request(
                source,
                Some(field(editor.project(), first, false)),
                editor.project(),
                &mut session,
                &mut selection
            ),
            Outcome::Prompt {
                can_commit: false,
                ..
            }
        ));
        assert_eq!(selection.active, Some(first));
        assert_eq!(session.inspector, Some(InspectorTarget::Board(first)));
        assert!(matches!(
            guard.resolve(
                Decision::Stay,
                Some(field(editor.project(), first, false)),
                &mut editor,
                &mut session,
                &mut selection,
                |_, _| Ok::<_, ()>(())
            ),
            Ok(Outcome::Stayed)
        ));
    }
    let intent = NavigationIntent::at(
        editor.project(),
        Route::Selection {
            target: Some(second),
            additive: false,
        },
    );
    guard.request(
        intent,
        Some(field(editor.project(), first, false)),
        editor.project(),
        &mut session,
        &mut selection,
    );
    assert!(matches!(
        guard.resolve(
            Decision::Abandon,
            Some(field(editor.project(), first, false)),
            &mut editor,
            &mut session,
            &mut selection,
            |_, _| Ok::<_, ()>(())
        ),
        Ok(Outcome::Navigated)
    ));
    assert_eq!(selection.active, Some(second));
    assert_eq!(session.inspector, Some(InspectorTarget::Board(second)));
}

#[test]
fn collapse_does_not_resolve_or_lose_a_draft_and_stay_preserves_location() {
    let project = reference_fixture::project();
    let target = project.boards[0].id;
    let mut editor = ProjectEditor::new(project).unwrap();
    let mut session = WorkspaceSession::new(editor.project());
    let mut selection = Selection::default();
    session.navigate(editor.project(), &mut selection, Destination::Board(target));
    let block = field(editor.project(), target, false);
    let mut guard = NavigationGuard::default();
    session.design.inspector_open = false;
    session.design.inspector_open = true;
    assert_eq!(guard.pending(), None);
    assert_eq!(editor.project().revision, block.source.number);
    guard.request(
        NavigationIntent::at(editor.project(), Route::Workspace(Workspace::Stock)),
        Some(block),
        editor.project(),
        &mut session,
        &mut selection,
    );
    assert_eq!(
        guard
            .resolve(
                Decision::Stay,
                Some(block),
                &mut editor,
                &mut session,
                &mut selection,
                |_, _| -> Result<(), ()> { panic!("stay must not run a callback") }
            )
            .unwrap(),
        Outcome::Stayed
    );
    assert_eq!(session.active, Workspace::Design);
    assert_eq!(session.inspector, Some(InspectorTarget::Board(target)));
    assert_eq!(selection.active, Some(target));
}

#[test]
fn preview_accept_must_validate_and_failed_commit_keeps_the_prompt() {
    let project = reference_fixture::project();
    let mut editor = ProjectEditor::new(project).unwrap();
    let mut session = WorkspaceSession::new(editor.project());
    session.switch(Workspace::CutPlan);
    let mut selection = Selection::default();
    let mut guard = NavigationGuard::default();
    let mut preview = EditBlock {
        kind: EditKind::Preview,
        source: Revision::of(editor.project()),
        workspace: Workspace::CutPlan,
        target: Some(InspectorTarget::Sheet(editor.project().stock[0].id)),
        can_commit: false,
    };
    editor.begin_preview();
    assert_eq!(
        guard.request(
            NavigationIntent::at(editor.project(), Route::Workspace(Workspace::Design)),
            Some(preview),
            editor.project(),
            &mut session,
            &mut selection
        ),
        Outcome::Prompt {
            kind: EditKind::Preview,
            can_commit: false
        }
    );
    assert_eq!(
        guard
            .resolve(
                Decision::Commit,
                Some(preview),
                &mut editor,
                &mut session,
                &mut selection,
                |_, _| -> Result<(), &'static str> { panic!("invalid repair") }
            )
            .unwrap(),
        Outcome::Blocked(Blocked::InvalidCommit)
    );
    preview.can_commit = true;
    assert!(matches!(
        guard.resolve(
            Decision::Commit,
            Some(preview),
            &mut editor,
            &mut session,
            &mut selection,
            |_, _| Err("witness failed")
        ),
        Err(ResolveError::Action("witness failed"))
    ));
    assert_eq!(session.active, Workspace::CutPlan);
    assert!(editor.preview().is_some());
    assert!(guard.pending().is_some());
    assert_eq!(
        guard
            .resolve(
                Decision::Abandon,
                Some(preview),
                &mut editor,
                &mut session,
                &mut selection,
                |_, editor| {
                    editor.cancel_preview();
                    Ok::<_, ()>(())
                }
            )
            .unwrap(),
        Outcome::Navigated
    );
    assert_eq!(session.active, Workspace::Design);
    assert!(editor.preview().is_none());
}

#[test]
fn stale_queued_revision_and_changed_edit_target_cannot_resume() {
    let project = reference_fixture::project();
    let first = project.boards[0].id;
    let second = project.boards[1].id;
    let mut editor = ProjectEditor::new(project).unwrap();
    let mut session = WorkspaceSession::new(editor.project());
    let mut selection = Selection::default();
    let mut guard = NavigationGuard::default();
    let block = field(editor.project(), first, true);
    let route = Route::Entity(Destination::Board(second));
    guard.request(
        NavigationIntent::at(editor.project(), route),
        Some(block),
        editor.project(),
        &mut session,
        &mut selection,
    );
    let changed = field(editor.project(), second, true);
    assert_eq!(
        guard
            .resolve(
                Decision::Commit,
                Some(changed),
                &mut editor,
                &mut session,
                &mut selection,
                |_, _| -> Result<(), ()> { panic!("changed target") }
            )
            .unwrap(),
        Outcome::Blocked(Blocked::StaleEdit)
    );
    guard.request(
        NavigationIntent::at(editor.project(), route),
        Some(block),
        editor.project(),
        &mut session,
        &mut selection,
    );
    editor
        .transact(|project| {
            project.name.push('!');
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(
        guard
            .resolve(
                Decision::Commit,
                Some(block),
                &mut editor,
                &mut session,
                &mut selection,
                |_, _| -> Result<(), ()> { panic!("changed revision") }
            )
            .unwrap(),
        Outcome::Blocked(Blocked::StaleSource)
    );
    assert_eq!(session.active, Workspace::Design);
    assert_eq!(selection.active, None);
}

#[test]
fn apply_rechecks_destination_after_transaction_and_does_not_route_removed_objects() {
    let project = reference_fixture::project();
    let first = project.boards[0].id;
    let mut editor = ProjectEditor::new(project).unwrap();
    let second = Uuid::new_v4();
    editor
        .transact(|project| {
            let mut board = project.boards[0].clone();
            board.id = second;
            board.name = "Temporary target".into();
            project.boards.push(board);
            Ok::<_, ()>(())
        })
        .unwrap();
    let mut session = WorkspaceSession::new(editor.project());
    let mut selection = Selection::default();
    let mut guard = NavigationGuard::default();
    let block = field(editor.project(), first, true);
    guard.request(
        NavigationIntent::at(editor.project(), Route::Entity(Destination::Board(second))),
        Some(block),
        editor.project(),
        &mut session,
        &mut selection,
    );
    assert_eq!(
        guard
            .resolve(
                Decision::Commit,
                Some(block),
                &mut editor,
                &mut session,
                &mut selection,
                |_, editor| {
                    editor
                        .transact(|project| {
                            project.boards.retain(|board| board.id != second);
                            Ok::<_, ()>(())
                        })
                        .map(|_| ())
                }
            )
            .unwrap(),
        Outcome::Blocked(Blocked::MissingDestination)
    );
    assert_eq!(session.active, Workspace::Design);
    assert_eq!(selection.active, None);
}

#[test]
fn successful_apply_can_advance_revision_before_routing() {
    let project = reference_fixture::project();
    let first = project.boards[0].id;
    let second = project.boards[1].id;
    let mut editor = ProjectEditor::new(project).unwrap();
    let mut session = WorkspaceSession::new(editor.project());
    let mut selection = Selection::default();
    let mut guard = NavigationGuard::default();
    let block = field(editor.project(), first, true);
    let start = editor.project().revision;
    guard.request(
        NavigationIntent::at(editor.project(), Route::Entity(Destination::Board(second))),
        Some(block),
        editor.project(),
        &mut session,
        &mut selection,
    );
    assert_eq!(
        guard
            .resolve(
                Decision::Commit,
                Some(block),
                &mut editor,
                &mut session,
                &mut selection,
                |_, editor| {
                    editor
                        .transact(|project| {
                            project.name.push('!');
                            Ok::<_, ()>(())
                        })
                        .map(|_| ())
                }
            )
            .unwrap(),
        Outcome::Navigated
    );
    assert_eq!(editor.project().revision, start + 1);
    assert_eq!(selection.active, Some(second));
}

#[test]
fn project_replacement_during_resolution_cannot_resume_a_queued_route() {
    use plan_my_cabinet::money::Currency;

    let project = reference_fixture::project();
    let first = project.boards[0].id;
    let mut editor = ProjectEditor::new(project).unwrap();
    let mut session = WorkspaceSession::new(editor.project());
    let mut selection = Selection::default();
    let mut guard = NavigationGuard::default();
    let block = field(editor.project(), first, true);
    guard.request(
        NavigationIntent::at(editor.project(), Route::Workspace(Workspace::Stock)),
        Some(block),
        editor.project(),
        &mut session,
        &mut selection,
    );
    assert_eq!(
        guard
            .resolve(
                Decision::Abandon,
                Some(block),
                &mut editor,
                &mut session,
                &mut selection,
                |_, editor| {
                    *editor =
                        ProjectEditor::new(Project::new("replacement", Currency::Brl)).unwrap();
                    Ok::<_, ()>(())
                }
            )
            .unwrap(),
        Outcome::Blocked(Blocked::StaleSource)
    );
    assert_eq!(session.active, Workspace::Design);
    assert_eq!(guard.pending(), None);
}
