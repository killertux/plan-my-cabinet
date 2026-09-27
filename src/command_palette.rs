//! Session-only command search. Results carry typed IDs, never display names as
//! destinations; invocation always goes back through the live application guard.
use eframe::egui::{self, Id};
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::i18n::Localizer;

use crate::DesktopApp;
use crate::actions::{ActionId as A, Request, Target, Unavailable};
use crate::pending_navigation::{Outcome, Route};
use crate::workspace_state::{Destination, Workspace};

const QUERY_ID: &str = "command-palette-query";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Group {
    Actions,
    Boards,
    Materials,
    Stock,
    Installations,
    Relationships,
}

impl Group {
    fn key(self) -> &'static str {
        match self {
            Self::Actions => "palette-actions",
            Self::Boards => "palette-boards",
            Self::Materials => "palette-materials",
            Self::Stock => "palette-stock",
            Self::Installations => "palette-installations",
            Self::Relationships => "palette-relationships",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResultRow {
    pub group: Group,
    pub label: String,
    pub detail: String,
    pub route: ResultRoute,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ResultRoute {
    Action(Request),
    Entity(Destination),
    Relationship(uuid::Uuid),
}

// Only commands implemented by DesktopApp::invoke with no additional argument
// or target. Contextual draft buttons remain in their owning workspace.
const COMMANDS: &[A] = &[
    A::NewProject,
    A::OpenProject,
    A::SaveProject,
    A::SaveProjectAs,
    A::Undo,
    A::Redo,
    A::NewBoard,
    A::NewMaterial,
    A::EditGrid,
    A::EditKerf,
    A::NewStock,
    A::EditCutFee,
    A::AddCatalog,
    A::NewHardware,
    A::NewHinge,
    A::NewDoor,
    A::ConfirmKerf,
    A::OpenHandoff,
    A::ExportPdf,
];

fn matches_query(query: &str, parts: &[&str]) -> bool {
    parts.iter().any(|part| part.to_lowercase().contains(query))
}

pub(crate) fn results(project: &Project, localizer: &Localizer, query: &str) -> Vec<ResultRow> {
    let query = query.trim().to_lowercase();
    let mut rows = Vec::new();
    for &id in COMMANDS {
        let label = id.label(localizer);
        let descriptor = id.descriptor();
        if matches_query(
            &query,
            &[
                &label,
                id.keywords(localizer.language()),
                descriptor.stable_id,
            ],
        ) {
            rows.push(ResultRow {
                group: Group::Actions,
                label,
                detail: descriptor.stable_id.into(),
                route: ResultRoute::Action(Request::new(id)),
            });
        }
    }
    // The blank state is a concise list of immediately useful commands. Named
    // entities become relevant as soon as the user starts searching.
    if query.is_empty() {
        return rows;
    }
    for board in &project.boards {
        let id = board.id.to_string();
        if matches_query(&query, &[&board.name, &id]) {
            rows.push(ResultRow {
                group: Group::Boards,
                label: board.name.clone(),
                detail: id,
                route: ResultRoute::Entity(Destination::Board(board.id)),
            });
        }
    }
    for material in &project.materials {
        let id = material.id.to_string();
        if matches_query(&query, &[&material.name, &id]) {
            rows.push(ResultRow {
                group: Group::Materials,
                label: material.name.clone(),
                detail: id,
                route: ResultRoute::Entity(Destination::Material(material.id)),
            });
        }
    }
    for stock in &project.stock {
        let id = stock.id.to_string();
        let alias = project.stock_alias(stock.id).unwrap_or("");
        if matches_query(&query, &[&stock.name, alias, &id]) {
            rows.push(ResultRow {
                group: Group::Stock,
                label: stock.name.clone(),
                detail: format!("{alias} · {id}"),
                route: ResultRoute::Entity(Destination::Sheet(stock.id)),
            });
        }
    }
    for installation in &project.hinge_installations {
        let id = installation.id.to_string();
        let door = project
            .boards
            .iter()
            .find(|b| b.id == installation.door_board_id);
        let mount = project
            .boards
            .iter()
            .find(|b| b.id == installation.mounting_board_id);
        let label = format!(
            "{} → {}",
            door.map_or("?", |b| b.name.as_str()),
            mount.map_or("?", |b| b.name.as_str())
        );
        if matches_query(&query, &[&label, &id]) {
            rows.push(ResultRow {
                group: Group::Installations,
                label,
                detail: id,
                route: ResultRoute::Entity(Destination::Installation(installation.id)),
            });
        }
    }
    for joint in &project.door_joints {
        let id = joint.id.to_string();
        let moving = project
            .boards
            .iter()
            .find(|b| b.id == joint.moving_root_id)
            .map(|b| b.name.as_str())
            .or_else(|| {
                project
                    .assemblies
                    .iter()
                    .find(|a| a.id == joint.moving_root_id)
                    .map(|a| a.name.as_str())
            });
        let mount = project
            .boards
            .iter()
            .find(|b| b.id == joint.mounting_board_id);
        let label = format!(
            "{} → {}",
            moving.unwrap_or("?"),
            mount.map_or("?", |b| b.name.as_str())
        );
        if matches_query(&query, &[&label, &id]) {
            rows.push(ResultRow {
                group: Group::Relationships,
                label,
                detail: id,
                route: ResultRoute::Relationship(joint.id),
            });
        }
    }
    rows
}

#[derive(Default)]
pub(crate) struct Palette {
    pub query: String,
    pub selected: usize,
    pub open: bool,
    needs_focus: bool,
    invoker: Option<Id>,
    pub error: Option<String>,
}

impl Palette {
    pub fn open(&mut self, ctx: &egui::Context) {
        self.query.clear();
        self.selected = 0;
        self.error = None;
        self.invoker = ctx.memory(|m| m.focused());
        self.open = true;
        self.needs_focus = true;
    }

    pub fn close(&mut self, ctx: &egui::Context) {
        self.open = false;
        self.needs_focus = false;
        if let Some(id) = self.invoker.take() {
            ctx.memory_mut(|m| m.request_focus(id));
        } else {
            ctx.memory_mut(|m| m.stop_text_input());
        }
    }

    pub fn step(&mut self, direction: isize, count: usize) {
        if count > 0 {
            self.selected =
                (self.selected as isize + direction).rem_euclid(count as isize) as usize;
        }
    }
}

pub(crate) fn shortcut(ctx: &egui::Context, allowed: bool) -> bool {
    allowed
        && !ctx.egui_wants_keyboard_input()
        && !egui::Popup::is_any_open(ctx)
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K))
}

impl DesktopApp {
    pub(crate) fn palette_results(&self) -> Vec<ResultRow> {
        let rows = results(self.editor.project(), &self.localizer, &self.palette.query);
        if self.palette.query.trim().is_empty() {
            rows.into_iter()
                .filter(|row| self.palette_availability(row.route).is_ok())
                .collect()
        } else {
            rows
        }
    }

    pub(crate) fn palette_availability(&self, route: ResultRoute) -> Result<(), Unavailable> {
        match route {
            ResultRoute::Action(request) => {
                if (self.sheet_repair.active()
                    || self.board_dimension.is_some()
                    || self.placement.is_some()
                    || self.editor.preview().is_some())
                    && request.id != A::OpenHandoff
                {
                    Err(Unavailable::PendingEdit)
                } else {
                    self.action_availability(request)
                }
            }
            ResultRoute::Entity(destination) => {
                if self.project_files.blocking() {
                    Err(Unavailable::Busy)
                } else if self.navigation.pending().is_some()
                    || self.other_modal_open()
                    || self.optimizer.comparison_open()
                {
                    Err(Unavailable::ModalOpen)
                } else if !self.destination_exists(destination) {
                    Err(Unavailable::MissingTarget)
                } else {
                    Ok(())
                }
            }
            ResultRoute::Relationship(id) => {
                if self.project_files.blocking() {
                    Err(Unavailable::Busy)
                } else if self.navigation.pending().is_some()
                    || self.other_modal_open()
                    || self.optimizer.comparison_open()
                {
                    Err(Unavailable::ModalOpen)
                } else {
                    self.action_availability(Request::with(A::EditDoor, Target::Door(id)))
                }
            }
        }
    }

    fn destination_exists(&self, destination: Destination) -> bool {
        let project = self.editor.project();
        match destination {
            Destination::Board(id) | Destination::BoardAllocation(id) => {
                project.boards.iter().any(|b| b.id == id)
            }
            Destination::Sheet(id) => project.stock.iter().any(|s| s.id == id),
            Destination::Material(id) => project.materials.iter().any(|m| m.id == id),
            Destination::Installation(id) => project.hinge_installations.iter().any(|h| h.id == id),
        }
    }

    pub(crate) fn invoke_palette(&mut self, row: &ResultRow) -> Result<Outcome, Unavailable> {
        self.palette_availability(row.route)?;
        match row.route {
            ResultRoute::Action(request) => {
                // Project file actions perform their own busy check. The
                // palette's layer is dismissed for that check, then the caller
                // restores focus on success (or keeps the palette on failure).
                let was_open = std::mem::replace(&mut self.palette.open, false);
                let result = self.invoke(request);
                self.palette.open = was_open;
                result?;
                Ok(Outcome::Navigated)
            }
            ResultRoute::Entity(destination) => {
                let outcome = self.request_navigation(Route::Entity(destination));
                match outcome {
                    Outcome::Blocked(_) => Err(Unavailable::MissingTarget),
                    other => Ok(other),
                }
            }
            ResultRoute::Relationship(id) => {
                let outcome = self.request_navigation(Route::Workspace(Workspace::Hardware));
                match outcome {
                    Outcome::Navigated => {
                        self.invoke(Request::with(A::EditDoor, Target::Door(id)))?;
                    }
                    Outcome::Prompt { .. } => self.palette_relationship_pending = Some(id),
                    Outcome::Blocked(_) | Outcome::Stayed => return Err(Unavailable::ModalOpen),
                }
                Ok(outcome)
            }
        }
    }

    pub(crate) fn show_palette(&mut self, ctx: &egui::Context) {
        if !self.palette.open {
            return;
        }
        // A popup may consume Enter/Escape in the same frame it closes.
        let popup_before = egui::Popup::is_any_open(ctx);
        let rows = self.palette_results();
        self.palette.selected = self.palette.selected.min(rows.len().saturating_sub(1));
        let up = !popup_before
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
        let down = !popup_before
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
        if up {
            self.palette.step(-1, rows.len());
        }
        if down {
            self.palette.step(1, rows.len());
        }
        let enter = !popup_before
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let escape = !popup_before
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let title = self.localizer.text("palette-title");
        let hint = self.localizer.text("palette-hint");
        let mut clicked = None;
        let mut query_changed = false;
        let modal = egui::Modal::new(Id::new("command-palette")).show(ctx, |ui| {
            ui.set_width(520.0_f32.min((ctx.content_rect().width() - 32.0).max(1.0)));
            ui.heading(title);
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.palette.query)
                        .id(Id::new(QUERY_ID))
                        .hint_text(hint),
                )
                .changed()
            {
                self.palette.selected = 0;
                self.palette.error = None;
                query_changed = true;
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 160.0).max(80.0))
                .show(ui, |ui| {
                    if rows.is_empty() {
                        ui.label(self.localizer.text("palette-no-results"));
                    }
                    let mut group = None;
                    for (index, row) in rows.iter().enumerate() {
                        if group != Some(row.group) {
                            group = Some(row.group);
                            ui.strong(self.localizer.text(row.group.key()));
                        }
                        let unavailable = self.palette_availability(row.route).err();
                        let response = ui.add_enabled(
                            unavailable.is_none(),
                            egui::Button::new(format!("{}  ·  {}", row.label, row.detail))
                                .selected(index == self.palette.selected),
                        );
                        if let Some(reason) = unavailable {
                            response
                                .on_disabled_hover_text(reason.reason(self.localizer.language()));
                            ui.small(reason.reason(self.localizer.language()));
                        } else if response.clicked() {
                            clicked = Some(index);
                        }
                    }
                });
            if let Some(error) = &self.palette.error {
                ui.colored_label(crate::theme_widgets::WARN_INK, error);
            }
        });
        if self.palette.needs_focus {
            ctx.memory_mut(|m| m.request_focus(Id::new(QUERY_ID)));
            self.palette.needs_focus = false;
        }
        if !popup_before && (escape || modal.should_close()) {
            self.palette.close(ctx);
            return;
        }
        if (enter && !query_changed) || clicked.is_some() {
            let index = clicked.unwrap_or(self.palette.selected);
            if let Some(row) = rows.get(index) {
                match self.invoke_palette(row) {
                    Ok(_) => self.palette.close(ctx),
                    Err(reason) => {
                        self.palette.error = Some(reason.reason(self.localizer.language()).into())
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_state::InspectorTarget;
    use plan_my_cabinet::commands::ProjectEditor;
    use plan_my_cabinet::i18n::Language;
    use plan_my_cabinet::reference_fixture;

    fn fixture() -> DesktopApp {
        let mut app = DesktopApp::default();
        app.editor = ProjectEditor::new(reference_fixture::project()).unwrap();
        app.session = crate::workspace_state::WorkspaceSession::new(app.editor.project());
        app
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::RawInput {
        egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn groups_localized_case_insensitive_search_and_empty_state() {
        let mut app = fixture();
        let project = app.editor.project();
        for language in [Language::En, Language::PtBr] {
            let localizer = Localizer::new(language);
            assert!(
                results(project, &localizer, "")
                    .iter()
                    .all(|r| r.group == Group::Actions)
            );
            assert!(results(project, &localizer, "impossible-ZZ987").is_empty());
            let keyword = if language == Language::En {
                "NEW BOARD"
            } else {
                "NOVA CHAPA"
            };
            assert!(
                results(project, &localizer, keyword)
                    .iter()
                    .any(|r| r.route == ResultRoute::Action(Request::new(A::NewBoard)))
            );
            assert!(
                results(
                    project,
                    &localizer,
                    &project.boards[0].id.to_string().to_uppercase()
                )
                .iter()
                .any(|r| r.route == ResultRoute::Entity(Destination::Board(project.boards[0].id)))
            );
            let stock = &project.stock[0];
            assert!(
                results(project, &localizer, project.stock_alias(stock.id).unwrap())
                    .iter()
                    .any(|r| r.route == ResultRoute::Entity(Destination::Sheet(stock.id)))
            );
            let installation = &project.hinge_installations[0];
            assert!(
                results(project, &localizer, &installation.id.to_string())
                    .iter()
                    .any(|r| r.route
                        == ResultRoute::Entity(Destination::Installation(installation.id)))
            );
            let joint = &project.door_joints[0];
            assert!(
                results(project, &localizer, &joint.id.to_string())
                    .iter()
                    .any(|r| r.route == ResultRoute::Relationship(joint.id))
            );
        }
        app.palette.query.clear();
        let empty = app.palette_results();
        assert!(!empty.is_empty());
        assert!(
            empty
                .iter()
                .all(|r| app.palette_availability(r.route).is_ok())
        );
        app.palette.query = "no-matching-command-487".into();
        assert!(app.palette_results().is_empty());
    }

    #[test]
    fn equal_names_route_only_the_selected_uuid_and_removed_targets_fail() {
        let mut app = fixture();
        let first = app.editor.project().boards[0].id;
        let second = app.editor.project().boards[1].id;
        let name = app.editor.project().boards[0].name.clone();
        let mut project = app.editor.project().clone();
        project.boards[1].name = name.clone();
        app.editor = ProjectEditor::new(project).unwrap();
        app.session = crate::workspace_state::WorkspaceSession::new(app.editor.project());
        let rows: Vec<_> = results(app.editor.project(), &app.localizer, &name)
            .into_iter()
            .filter(|r| r.group == Group::Boards)
            .collect();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|r| r.detail == first.to_string()));
        let selected = rows
            .iter()
            .find(|r| r.detail == second.to_string())
            .unwrap();
        assert_eq!(app.invoke_palette(selected).unwrap(), Outcome::Navigated);
        assert_eq!(app.selection.active, Some(second));
        assert_eq!(app.session.inspector, Some(InspectorTarget::Board(second)));
        let mut removed = Project::new("Replacement", app.editor.project().currency);
        removed.id = app.editor.project().id;
        // A previously rendered row must never be resolved by its equal name.
        app.editor = ProjectEditor::new(removed).unwrap();
        let before = app.editor.project().clone();
        assert_eq!(
            app.invoke_palette(selected),
            Err(Unavailable::MissingTarget)
        );
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn equal_material_and_stock_names_keep_distinct_typed_destinations() {
        let mut project = reference_fixture::project();
        let mut material = project.materials[0].clone();
        material.id = uuid::Uuid::new_v4();
        project.materials.push(material.clone());
        let mut stock = project.stock[0].clone();
        stock.id = uuid::Uuid::new_v4();
        project
            .stock_aliases
            .insert(stock.id, format!("S{}", project.next_stock_s_alias));
        project.next_stock_s_alias += 1;
        project.stock.push(stock.clone());
        let mut app = DesktopApp::default();
        app.editor = ProjectEditor::new(project).unwrap();
        app.session = crate::workspace_state::WorkspaceSession::new(app.editor.project());
        let material_rows: Vec<_> = results(app.editor.project(), &app.localizer, &material.name)
            .into_iter()
            .filter(|r| r.group == Group::Materials && r.label == material.name)
            .collect();
        assert_eq!(material_rows.len(), 2);
        let target = material_rows
            .iter()
            .find(|r| r.route == ResultRoute::Entity(Destination::Material(material.id)))
            .unwrap();
        assert_eq!(app.invoke_palette(target), Ok(Outcome::Navigated));
        assert_eq!(
            app.session.inspector,
            Some(InspectorTarget::Material(material.id))
        );
        let stock_rows: Vec<_> = results(app.editor.project(), &app.localizer, &stock.name)
            .into_iter()
            .filter(|r| r.group == Group::Stock && r.label == stock.name)
            .collect();
        assert_eq!(stock_rows.len(), 2);
        let target = stock_rows
            .iter()
            .find(|r| r.route == ResultRoute::Entity(Destination::Sheet(stock.id)))
            .unwrap();
        assert_eq!(app.invoke_palette(target), Ok(Outcome::Navigated));
        assert_eq!(app.session.focused_sheet, Some(stock.id));
        assert_eq!(
            app.session.inspector,
            Some(InspectorTarget::Sheet(stock.id))
        );
        assert_eq!(app.selection.active, None);
    }

    #[test]
    fn preview_navigation_prompts_and_stay_retains_the_preview() {
        let mut app = fixture();
        let board = app.editor.project().boards[0].id;
        app.selection.choose(Some(board), false);
        app.editor.begin_preview();
        let command = results(app.editor.project(), &app.localizer, "NewBoard")
            .into_iter()
            .find(|r| r.route == ResultRoute::Action(Request::new(A::NewBoard)))
            .unwrap();
        assert_eq!(app.invoke_palette(&command), Err(Unavailable::PendingEdit));
        assert!(app.dialog.is_none());
        let row = results(
            app.editor.project(),
            &app.localizer,
            &app.editor.project().materials[0].id.to_string(),
        )
        .into_iter()
        .find(|r| r.group == Group::Materials)
        .unwrap();
        let original = app.editor.project().clone();
        assert!(matches!(
            app.invoke_palette(&row),
            Ok(Outcome::Prompt { .. })
        ));
        assert_eq!(app.session.active, Workspace::Design);
        assert!(app.navigation.pending().is_some());
        assert_eq!(
            app.resolve_navigation(crate::NavigationDecision::Stay),
            Outcome::Stayed
        );
        assert!(app.editor.preview().is_some());
        assert_eq!(app.editor.project(), &original);
    }

    #[test]
    fn disabled_actions_modal_and_pending_navigation_cannot_bypass_guards() {
        let mut app = fixture();
        let undo = results(app.editor.project(), &app.localizer, "undo")
            .into_iter()
            .find(|r| r.route == ResultRoute::Action(Request::new(A::Undo)))
            .unwrap();
        assert_eq!(
            app.palette_availability(undo.route),
            Err(Unavailable::NoUndo)
        );
        assert_eq!(app.invoke_palette(&undo), Err(Unavailable::NoUndo));
        let board = app.editor.project().boards[0].id;
        let row = results(app.editor.project(), &app.localizer, &board.to_string())
            .into_iter()
            .find(|r| r.route == ResultRoute::Entity(Destination::Board(board)))
            .unwrap();
        app.invoke(Request::with(A::EditDimensions, Target::Board(board)))
            .unwrap();
        assert_eq!(app.invoke_palette(&row), Err(Unavailable::ModalOpen));
        app.board_dimension = None;
        // Pending edit resolution cannot be overridden by another result.
        let edit = crate::pending_navigation::EditBlock {
            kind: crate::pending_navigation::EditKind::Field,
            source: crate::pending_navigation::Revision::of(app.editor.project()),
            workspace: app.session.active,
            target: Some(InspectorTarget::Board(board)),
            can_commit: false,
        };
        assert!(matches!(
            app.navigation.request(
                crate::pending_navigation::NavigationIntent::at(
                    app.editor.project(),
                    Route::Workspace(Workspace::Stock)
                ),
                Some(edit),
                app.editor.project(),
                &mut app.session,
                &mut app.selection
            ),
            Outcome::Prompt { .. }
        ));
        assert_eq!(app.invoke_palette(&row), Err(Unavailable::ModalOpen));
    }

    #[test]
    fn keyboard_navigation_escape_restores_focus_and_text_shortcuts_do_not_leak() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut app = fixture();
        let mut invoker = Id::NULL;
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            invoker = ui.button("Search").id;
            ui.memory_mut(|m| m.request_focus(invoker));
            app.palette.open(ui.ctx());
            assert!(app.modal_open());
            app.show_palette(ui.ctx());
            assert_eq!(ui.memory(|m| m.focused()), Some(Id::new(QUERY_ID)));
        });
        frame.textures_delta.clear();
        let mut frame = ctx.run_ui(key(egui::Key::ArrowDown, egui::Modifiers::NONE), |ui| {
            app.show_palette(ui.ctx());
            assert_eq!(app.palette.selected, 1);
        });
        frame.textures_delta.clear();
        let mut frame = ctx.run_ui(key(egui::Key::ArrowUp, egui::Modifiers::NONE), |ui| {
            app.show_palette(ui.ctx());
            assert_eq!(app.palette.selected, 0);
        });
        frame.textures_delta.clear();
        let mut frame = ctx.run_ui(key(egui::Key::Escape, egui::Modifiers::NONE), |ui| {
            app.show_palette(ui.ctx());
            assert!(!app.palette.open);
            assert_eq!(ui.memory(|m| m.focused()), Some(invoker));
        });
        frame.textures_delta.clear();
        let text = Id::new("text-input");
        let mut value = String::new();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.add(egui::TextEdit::singleline(&mut value).id(text));
            ui.memory_mut(|m| m.request_focus(text));
        });
        frame.textures_delta.clear();
        let mut frame = ctx.run_ui(key(egui::Key::K, egui::Modifiers::COMMAND), |ui| {
            assert!(!shortcut(ui.ctx(), true));
            ui.add(egui::TextEdit::singleline(&mut value).id(text));
        });
        frame.textures_delta.clear();
        let mut frame = ctx.run_ui(key(egui::Key::K, egui::Modifiers::COMMAND), |ui| {
            ui.memory_mut(|m| m.stop_text_input());
            assert!(shortcut(ui.ctx(), true));
        });
        frame.textures_delta.clear();
    }

    #[test]
    fn enter_activates_the_live_action_and_popup_keys_do_not_escape() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut app = fixture();
        let old_project = app.editor.project().id;
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.palette.open(ui.ctx());
            app.palette.query = "NewProject".into();
            app.show_palette(ui.ctx());
        });
        frame.textures_delta.clear();
        assert_eq!(app.palette_results().len(), 1);
        let popup = Id::new("palette-nested-popup");
        egui::Popup::open_id(&ctx, popup);
        let mut frame = ctx.run_ui(key(egui::Key::Enter, egui::Modifiers::NONE), |ui| {
            app.show_palette(ui.ctx());
            egui::Popup::close_id(ui.ctx(), popup);
        });
        frame.textures_delta.clear();
        assert_eq!(app.editor.project().id, old_project);
        assert!(app.palette.open);
        egui::Popup::open_id(&ctx, popup);
        let mut frame = ctx.run_ui(key(egui::Key::Escape, egui::Modifiers::NONE), |ui| {
            app.show_palette(ui.ctx());
            egui::Popup::close_id(ui.ctx(), popup);
        });
        frame.textures_delta.clear();
        assert!(app.palette.open);
        let mut frame = ctx.run_ui(key(egui::Key::Enter, egui::Modifiers::NONE), |ui| {
            app.show_palette(ui.ctx());
        });
        frame.textures_delta.clear();
        assert_ne!(app.editor.project().id, old_project);
        assert!(!app.palette.open);
    }

    #[test]
    fn unavailable_reason_is_localized_without_mutating_project() {
        let mut app = fixture();
        app.palette.query = "Undo".into();
        let row = app
            .palette_results()
            .into_iter()
            .find(|r| r.route == ResultRoute::Action(Request::new(A::Undo)))
            .unwrap();
        let before = app.editor.project().clone();
        assert_eq!(app.invoke_palette(&row), Err(Unavailable::NoUndo));
        assert_eq!(app.editor.project(), &before);
        for language in [Language::En, Language::PtBr] {
            assert!(!Unavailable::NoUndo.reason(language).is_empty());
        }
    }
}
