//! Pending-edit guard: routes, drafts and previews that must resolve before navigating.
use crate::*;


impl DesktopApp {
    // Every mounted draft is owned by the app session, not either editing surface.
    pub(crate) fn navigation_edit(&mut self) -> Option<EditBlock> {
        let (kind, target, can_commit) = if self.cut_plan.repair.active() {
            (
                EditKind::Preview,
                self.session.focused_sheet.map(InspectorTarget::Sheet),
                self.cut_plan.repair.can_accept(&mut self.editor),
            )
        } else if let Some(draft) = self.modals.board_dimension() {
            let project = self.editor.project();
            let valid = project.id == draft.project_id
                && project.revision == draft.revision
                && draft.value.value(project.display_unit).is_ok_and(|value| {
                    self.editor
                        .preview_board_dimension(
                            draft.board_id,
                            draft.dimension,
                            value,
                            draft.anchor,
                        )
                        .is_ok()
                });
            (
                EditKind::Field,
                Some(InspectorTarget::Board(draft.board_id)),
                valid,
            )
        } else if let Some(placement) = self.modals.placement() {
            (
                EditKind::Preview,
                Some(InspectorTarget::Board(placement.board_id)),
                placement.can_accept_navigation(self.editor.project()),
            )
        } else if self.editor.preview().is_some() {
            (
                EditKind::Preview,
                self.selection.active.map(InspectorTarget::Board),
                self.selection.active.is_some_and(|id| {
                    self.editor
                        .project()
                        .boards
                        .iter()
                        .any(|board| board.id == id)
                }),
            )
        } else if let Some(InspectorTarget::Board(id)) = self.session.inspector {
            if let Some(draft) = self
                .edit_drafts
                .existing_board(self.editor.project().id, id)
                .filter(|draft| draft.dirty())
            {
                (
                    EditKind::Field,
                    Some(InspectorTarget::Board(id)),
                    draft.preview(&self.editor).is_ok(),
                )
            } else if let Some(draft) = self
                .edit_drafts
                .existing_pose(self.editor.project().id, id)
                .filter(|draft| draft.dirty())
            {
                (
                    EditKind::Preview,
                    Some(InspectorTarget::Board(id)),
                    draft.preview(&self.editor).is_ok(),
                )
            } else {
                return None;
            }
        } else {
            return None;
        };
        Some(EditBlock {
            kind,
            source: Revision::of(self.editor.project()),
            workspace: self.session.active,
            target,
            can_commit,
        })
    }

    pub(crate) fn request_navigation(&mut self, route: NavigationRoute) -> Outcome {
        if self.project_files.blocking()
            || self.other_modal_open() && self.modals.placement().is_none() && self.modals.board_dimension().is_none()
        {
            return Outcome::Blocked(pending_navigation::Blocked::PendingDecision);
        }
        let block = self.navigation_edit();
        let outcome = self.navigation.request(
            NavigationIntent::at(self.editor.project(), route),
            block,
            self.editor.project(),
            &mut self.session,
            &mut self.selection,
        );
        self.after_navigation(outcome);
        outcome
    }

    pub(crate) fn request_scene_selection(&mut self, target: Option<Uuid>, additive: bool) -> Outcome {
        if self.navigation.pending().is_none()
            && !self.other_modal_open()
            && !additive
            && self.selection.active == target
            && self.selection.ids.len() == usize::from(target.is_some())
        {
            return Outcome::Navigated;
        }
        self.request_navigation(NavigationRoute::Selection { target, additive })
    }

    pub(crate) fn request_pose_frame(&mut self, frame: CoordinateFrame) {
        if frame == self.design.pose_frame {
            return;
        }
        if self.navigation_edit().is_none() {
            self.design.pose_frame = frame;
        } else if matches!(
            self.request_navigation(NavigationRoute::Workspace(self.session.active)),
            Outcome::Prompt { .. }
        ) {
            self.design.pending_pose_frame = Some(frame);
        }
    }

    /// Defer an incompatible command until the shared board edit has been
    /// explicitly resolved. The command is revalidated by `invoke` afterwards.
    pub(crate) fn resolve_draft_before_action(
        &mut self,
        request: Request,
    ) -> Result<bool, actions::Unavailable> {
        if self.navigation.pending().is_some() {
            return Err(actions::Unavailable::PendingEdit);
        }
        let Some(InspectorTarget::Board(id)) = self.session.inspector else {
            return Ok(false);
        };
        let dirty_dimensions = self
            .edit_drafts
            .existing_board(self.editor.project().id, id)
            .is_some_and(|draft| draft.dirty());
        let dirty_pose = self
            .edit_drafts
            .existing_pose(self.editor.project().id, id)
            .is_some_and(|draft| draft.dirty());
        if !dirty_dimensions && !dirty_pose {
            return Ok(false);
        }
        match self.request_navigation(NavigationRoute::Workspace(self.session.active)) {
            Outcome::Prompt { .. } => {
                self.pending_action = Some(request);
                Ok(true)
            }
            Outcome::Blocked(_) => Err(actions::Unavailable::PendingEdit),
            Outcome::Navigated | Outcome::Stayed => Ok(false),
        }
    }

    pub(crate) fn after_navigation(&mut self, outcome: Outcome) {
        if outcome == Outcome::Navigated {
            self.design.scene_active_seen = self.selection.active;
            if self.session.active != Workspace::Hardware {
                self.hardware.door_motion = None;
            }
            self.navigation_error = None;
            if let Some(id) = self.palette_relationship_pending.take()
                && self.session.active == Workspace::Hardware
            {
                self.invoke_or_report(Request::with(A::EditDoor, Target::Door(id)));
            }
        } else if self.navigation.pending().is_none() {
            self.palette_relationship_pending = None;
        }
    }

    pub(crate) fn resolve_navigation(&mut self, decision: NavigationDecision) -> Outcome {
        let block = self.navigation_edit();
        let draft_board = match block.and_then(|edit| edit.target) {
            Some(InspectorTarget::Board(id))
                if self
                    .edit_drafts
                    .existing_board(self.editor.project().id, id)
                    .is_some_and(|draft| draft.dirty()) =>
            {
                Some(id)
            }
            _ => None,
        };
        let draft_pose = match block {
            Some(EditBlock {
                kind: EditKind::Preview,
                target: Some(InspectorTarget::Board(id)),
                ..
            }) if self
                .edit_drafts
                .existing_pose(self.editor.project().id, id)
                .is_some_and(|draft| draft.dirty()) =>
            {
                Some(id)
            }
            _ => None,
        };
        let return_selection = self
            .modals
            .placement()
            .map(|dialog| (dialog.selection_ids.clone(), dialog.selection_active));
        let workspace_route = matches!(
            self.navigation.pending().map(|intent| intent.route),
            Some(NavigationRoute::Workspace(_))
        );
        let repair = &mut self.cut_plan.repair;
        let modals = &mut self.modals;
        let drafts = &mut self.edit_drafts;
        let mut conflicts = None;
        let outcome = self.navigation.resolve(
            decision,
            block,
            &mut self.editor,
            &mut self.session,
            &mut self.selection,
            |decision, editor| {
                if repair.active() {
                    if decision == NavigationDecision::Commit {
                        repair.accept_navigation(editor)?;
                    } else {
                        repair.cancel_navigation(editor);
                    }
                } else if let Some(draft) = modals.board_dimension_mut() {
                    if decision == NavigationDecision::Commit {
                        let value = draft
                            .value
                            .value(editor.project().display_unit)
                            .map_err(|_| ())?;
                        let preview = editor
                            .preview_board_dimension(
                                draft.board_id,
                                draft.dimension,
                                value,
                                draft.anchor,
                            )
                            .map_err(|error| {
                                draft.error = Some(error);
                            })?;
                        conflicts =
                            Some(editor.edit_board_dimension(preview).map_err(|error| {
                                draft.error = Some(match error {
                                    plan_my_cabinet::commands::EditError::Command(reason) => reason,
                                    _ => DimensionEditError::StalePreview,
                                });
                            })?);
                    }
                    modals.set_board_dimension(None);
                } else if modals.placement().is_some() {
                    if decision == NavigationDecision::Commit && editor.preview().is_some() {
                        editor.commit_preview().map_err(|_| ())?;
                    } else {
                        editor.cancel_preview();
                    }
                    modals.set_placement(None);
                } else if editor.preview().is_some() {
                    if decision == NavigationDecision::Commit {
                        editor.commit_preview().map_err(|_| ())?;
                    } else {
                        editor.cancel_preview();
                    }
                    self.design.move_tool.cancel();
                } else if let Some(id) = draft_board {
                    if decision == NavigationDecision::Commit {
                        drafts
                            .existing_board_mut(editor.project().id, id)
                            .ok_or(())?
                            .accept(editor)
                            .map_err(|_| ())?;
                    }
                    drafts.cancel_board(editor.project().id, id);
                } else if let Some(id) = draft_pose {
                    if decision == NavigationDecision::Commit {
                        drafts
                            .existing_pose_mut(editor.project().id, id)
                            .ok_or(())?
                            .accept(editor)
                            .map_err(|_| ())?;
                    }
                    drafts.cancel_pose(editor.project().id, id);
                }
                Ok::<_, ()>(())
            },
        );
        match outcome {
            Ok(outcome) => {
                if let Some(changes) = conflicts {
                    self.cut_plan.material_conflicts = changes;
                }
                if outcome == Outcome::Navigated
                    && decision == NavigationDecision::Abandon
                    && workspace_route
                    && let Some((ids, active)) = return_selection
                {
                    self.selection.ids = ids;
                    self.selection.active = active;
                }
                self.after_navigation(outcome);
                self.navigation_error = match outcome {
                    Outcome::Blocked(pending_navigation::Blocked::MissingDestination) => {
                        Some("navigation-missing")
                    }
                    Outcome::Blocked(
                        pending_navigation::Blocked::StaleSource
                        | pending_navigation::Blocked::StaleEdit,
                    ) => Some("navigation-stale"),
                    _ => None,
                };
                if outcome == Outcome::Navigated {
                    if let Some(frame) = self.design.pending_pose_frame.take() {
                        self.design.pose_frame = frame;
                    }
                    if let Some(request) = self.pending_action.take()
                        && self.invoke(request).is_err()
                    {
                        self.navigation_error = Some("navigation-accept-error");
                    }
                    if let Some(command) = self.pending_project_command.take() {
                        match command {
                            project_ui::PendingProjectCommand::Action(action) => {
                                self.request_project_action(action);
                            }
                            project_ui::PendingProjectCommand::Save(as_new) => {
                                self.request_save(as_new);
                            }
                        }
                    }
                } else if self.navigation.pending().is_none() {
                    self.pending_action = None;
                    self.pending_project_command = None;
                    self.design.pending_pose_frame = None;
                }
                outcome
            }
            Err(_) => {
                self.navigation_error = Some("navigation-accept-error");
                Outcome::Stayed
            }
        }
    }

    pub(crate) fn show_navigation_prompt(&mut self, ctx: &egui::Context) {
        if self.navigation.pending().is_none() {
            if self.navigation_chrome.is_active() {
                self.navigation_chrome.close(ctx);
            }
            return;
        }
        let block = self.navigation_edit();
        let kind = block.map_or(EditKind::Preview, |edit| edit.kind);
        let title = self.localizer.text("navigation-title");
        let description = self.localizer.text(if kind == EditKind::Field {
            "navigation-field"
        } else {
            "navigation-preview"
        });
        let primary = self.localizer.text(if kind == EditKind::Field {
            "navigation-apply"
        } else {
            "navigation-accept"
        });
        let secondary = self.localizer.text(if kind == EditKind::Field {
            "navigation-discard"
        } else {
            "navigation-cancel"
        });
        let stay = self.localizer.text("navigation-stay");
        let error = self.navigation_error.map(|key| self.localizer.text(key));
        let valid = block.is_some_and(|edit| edit.can_commit);
        let action = self
            .navigation_chrome
            .show_three(
                ctx,
                &title,
                ModalThreeActions {
                    primary: &primary,
                    secondary: &secondary,
                    stay: &stay,
                },
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&description)
                                .size(13.0)
                                .color(theme_widgets::SECONDARY),
                        )
                        .wrap()
                        .selectable(false),
                    );
                    if let Some(error) = &error {
                        modal_chrome::form::error(ui, error);
                    }
                    ((), valid)
                },
            )
            .action;
        let choice = match action {
            ModalThreeAction::None => None,
            ModalThreeAction::Primary => Some(NavigationDecision::Commit),
            ModalThreeAction::Secondary => Some(NavigationDecision::Abandon),
            ModalThreeAction::Stay => Some(NavigationDecision::Stay),
        };
        if let Some(choice) = choice {
            self.resolve_navigation(choice);
            if self.navigation.pending().is_none() {
                self.navigation_chrome.close(ctx);
            }
        }
    }

    pub(crate) fn navigate_session(&mut self, destination: Destination) -> bool {
        matches!(
            self.request_navigation(NavigationRoute::Entity(destination)),
            Outcome::Navigated | Outcome::Prompt { .. }
        )
    }
}
