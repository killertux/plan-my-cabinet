//! Resolution boundary for navigation away from unfinished editing sessions.
//!
//! Integration: keep one `NavigationGuard` for the current project session. All
//! rail, palette, inspector-target, and contextual navigation calls go through
//! `request` with an intent stamped from the project when the action was
//! offered. Provide the current `EditBlock` from the shared field controller or
//! repair/pose session; its `can_commit` must reflect parsing, rounding consent,
//! and preview feasibility. Render `Prompt` as Apply/Discard/Stay for fields or
//! Accept/Cancel/Stay for previews. On a decision, call `resolve` with a freshly
//! calculated block and a closure that performs the actual single transaction
//! or preview cancellation (and clears that edit session) through `ProjectEditor`.
//! A failed closure must leave that edit session available for correction.
//! Only `Navigated` means routing succeeded. In particular, do not change the
//! workspace, inspector, selection, or display-only Hardware motion before it.
//! Reset Hardware motion after successful navigation out of Hardware. Handle
//! unsaved-project replacement *after* edit resolution via its existing prompt.
//! Do not call `request` for panel collapse/reopen: that is view state, so the
//! draft remains owned by the edit controller. Workers are independent of this
//! guard and must not be accepted by a navigation callback.

use crate::viewport::Selection;
use crate::workspace_state::{Destination, InspectorTarget, Workspace, WorkspaceSession};
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::Project;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Revision {
    pub project_id: Uuid,
    pub number: u64,
}

impl Revision {
    pub fn of(project: &Project) -> Self {
        Self {
            project_id: project.id,
            number: project.revision,
        }
    }

    fn matches(self, project: &Project) -> bool {
        self == Self::of(project)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Workspace(Workspace),
    Entity(Destination),
    /// Scene picking and outliner/list selection must use the same edit guard.
    Selection {
        target: Option<Uuid>,
        additive: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NavigationIntent {
    pub source: Revision,
    pub route: Route,
}

impl NavigationIntent {
    pub fn at(project: &Project, route: Route) -> Self {
        Self {
            source: Revision::of(project),
            route,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditKind {
    Field,
    Preview,
}

/// `target` identifies the edit being left, independently from the destination.
/// A preview can be associated with a board, sheet, or other inspector target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EditBlock {
    pub kind: EditKind,
    pub source: Revision,
    pub workspace: Workspace,
    pub target: Option<InspectorTarget>,
    pub can_commit: bool,
}

impl EditBlock {
    fn same_session(self, other: Self) -> bool {
        self.kind == other.kind
            && self.source == other.source
            && self.workspace == other.workspace
            && self.target == other.target
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Commit,  // Apply for a field; Accept for a preview.
    Abandon, // Discard for a field; Cancel for a preview.
    Stay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Blocked {
    PendingDecision,
    StaleSource,
    StaleEdit,
    InvalidCommit,
    MissingDestination,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Prompt { kind: EditKind, can_commit: bool },
    Navigated,
    Stayed,
    Blocked(Blocked),
}

#[derive(Debug)]
pub(crate) enum ResolveError<E> {
    Action(E),
}

#[derive(Clone, Copy, Debug)]
struct Pending {
    intent: NavigationIntent,
    edit: EditBlock,
}

#[derive(Default)]
pub(crate) struct NavigationGuard {
    pending: Option<Pending>,
}

impl NavigationGuard {
    pub fn pending(&self) -> Option<NavigationIntent> {
        self.pending.map(|p| p.intent)
    }

    /// Invalidate queued work when replacing the project/session. A queued
    /// destination never carries over to a new document, even if names match.
    pub fn clear(&mut self) {
        self.pending = None;
    }

    pub fn request(
        &mut self,
        intent: NavigationIntent,
        edit: Option<EditBlock>,
        project: &Project,
        session: &mut WorkspaceSession,
        selection: &mut Selection,
    ) -> Outcome {
        if self.pending.is_some() {
            return Outcome::Blocked(Blocked::PendingDecision);
        }
        if !intent.source.matches(project) || !session.belongs_to(project) {
            return Outcome::Blocked(Blocked::StaleSource);
        }
        if !destination_exists(intent.route, project) {
            return Outcome::Blocked(Blocked::MissingDestination);
        }
        if let Some(edit) = edit {
            if !edit.source.matches(project) || edit.workspace != session.active {
                return Outcome::Blocked(Blocked::StaleEdit);
            }
            self.pending = Some(Pending { intent, edit });
            return Outcome::Prompt {
                kind: edit.kind,
                can_commit: edit.can_commit,
            };
        }
        navigate(intent.route, project, session, selection)
    }

    /// Recheck the original revision *before* invoking the edit action. A
    /// successful transaction may advance it; the destination is checked again
    /// against the resulting project before navigating. The closure must return
    /// an error on failed Apply/Accept (or failed Discard/Cancel), leaving the
    /// draft intact and the prompt pending for retry or Stay.
    pub fn resolve<E>(
        &mut self,
        decision: Decision,
        current_edit: Option<EditBlock>,
        editor: &mut ProjectEditor,
        session: &mut WorkspaceSession,
        selection: &mut Selection,
        action: impl FnOnce(Decision, &mut ProjectEditor) -> Result<(), E>,
    ) -> Result<Outcome, ResolveError<E>> {
        let Some(pending) = self.pending else {
            return Ok(Outcome::Blocked(Blocked::PendingDecision));
        };
        if decision == Decision::Stay {
            self.clear();
            return Ok(Outcome::Stayed);
        }
        if !pending.intent.source.matches(editor.project()) {
            self.clear();
            return Ok(Outcome::Blocked(Blocked::StaleSource));
        }
        let Some(edit) = current_edit.filter(|edit| pending.edit.same_session(*edit)) else {
            self.clear();
            return Ok(Outcome::Blocked(Blocked::StaleEdit));
        };
        if edit.workspace != session.active || !session.belongs_to(editor.project()) {
            self.clear();
            return Ok(Outcome::Blocked(Blocked::StaleEdit));
        }
        if decision == Decision::Commit && !edit.can_commit {
            return Ok(Outcome::Blocked(Blocked::InvalidCommit));
        }
        action(decision, editor).map_err(ResolveError::Action)?;
        self.clear();
        // Never route to an object removed by the accepted edit or to a new
        // project installed during a callback. A source revision change here is
        // expected for a successful Apply/Accept, but a changed ID is not.
        if pending.intent.source.project_id != editor.project().id {
            return Ok(Outcome::Blocked(Blocked::StaleSource));
        }
        Ok(navigate(
            pending.intent.route,
            editor.project(),
            session,
            selection,
        ))
    }
}

fn destination_exists(route: Route, project: &Project) -> bool {
    match route {
        Route::Workspace(_) => true,
        Route::Selection { target: None, .. } => true,
        Route::Selection {
            target: Some(id), ..
        } => {
            project.boards.iter().any(|board| board.id == id)
                || project.assemblies.iter().any(|assembly| assembly.id == id)
                || project.hardware.iter().any(|hardware| hardware.id == id)
        }
        Route::Entity(Destination::Board(id) | Destination::BoardAllocation(id)) => {
            project.boards.iter().any(|board| board.id == id)
        }
        Route::Entity(Destination::Sheet(id)) => project.stock.iter().any(|piece| piece.id == id),
        Route::Entity(Destination::Material(id)) => {
            project.materials.iter().any(|material| material.id == id)
        }
        Route::Entity(Destination::Installation(id)) => project
            .hinge_installations
            .iter()
            .any(|installation| installation.id == id),
    }
}

fn navigate(
    route: Route,
    project: &Project,
    session: &mut WorkspaceSession,
    selection: &mut Selection,
) -> Outcome {
    if !destination_exists(route, project) {
        return Outcome::Blocked(Blocked::MissingDestination);
    }
    match route {
        Route::Workspace(workspace) => session.switch(workspace),
        Route::Selection { target, additive } => {
            selection.choose(target, additive);
            session.inspector = selection.active.and_then(|id| {
                project
                    .boards
                    .iter()
                    .any(|board| board.id == id)
                    .then_some(InspectorTarget::Board(id))
            });
        }
        Route::Entity(destination) => {
            if !session.navigate(project, selection, destination) {
                return Outcome::Blocked(Blocked::MissingDestination);
            }
        }
    }
    Outcome::Navigated
}

#[cfg(test)]
mod tests {
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
        let intent =
            NavigationIntent::at(editor.project(), Route::Entity(Destination::Board(second)));
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
            let source =
                NavigationIntent::at(editor.project(), Route::Selection { target, additive });
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
}
