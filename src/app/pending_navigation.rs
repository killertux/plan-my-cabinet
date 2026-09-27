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
mod tests;
