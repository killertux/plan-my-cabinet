//! Session-only command search. Results carry typed IDs, never display names as
//! destinations; invocation always goes back through the live application guard.
use eframe::egui::{self, Id};
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::i18n::Localizer;

use crate::DesktopApp;
use crate::actions::{ActionId as A, Request, Target, Unavailable};
use crate::pending_navigation::{Outcome, Route};
use crate::theme_widgets as tw;
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
                detail: id.route_name(localizer.language()).into(),
                route: ResultRoute::Action(Request::new(id)),
            });
        }
    }
    // The blank state is a concise list of immediately useful commands. Named
    // entities become relevant as soon as the user starts searching.
    if query.is_empty() {
        return rows;
    }
    let mm = |length: plan_my_cabinet::units::Length| {
        let value = length.micrometres() as f64 / 1000.0;
        let text = format!("{value:.1}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    for board in &project.boards {
        let id = board.id.to_string();
        if matches_query(&query, &[&board.name, &id]) {
            rows.push(ResultRow {
                group: Group::Boards,
                label: board.name.clone(),
                detail: format!(
                    "{} × {} × {}",
                    mm(board.length),
                    mm(board.width),
                    mm(board.thickness)
                ),
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
                detail: format!("{} mm", mm(material.default_thickness)),
                route: ResultRoute::Entity(Destination::Material(material.id)),
            });
        }
    }
    for stock in &project.stock {
        let id = stock.id.to_string();
        let alias = project.stock_alias(stock.id).unwrap_or("");
        // Piece commands appear when the piece or the command itself is searched
        // ("S1", "Spare MDF", "duplicate", "delete sheet").
        for action in [A::DuplicateStock, A::DeleteStock] {
            let label = action.label(localizer);
            if matches_query(&query, &[&stock.name, alias, &id])
                || (query.len() >= 3
                    && matches_query(&query, &[&label, action.keywords(localizer.language())]))
            {
                rows.push(ResultRow {
                    group: Group::Actions,
                    label: format!("{label}: {alias} · {}", stock.name),
                    detail: format!("{}×{}", mm(stock.length), mm(stock.width)),
                    route: ResultRoute::Action(Request::with(action, Target::Stock(stock.id))),
                });
            }
        }
        if matches_query(&query, &[&stock.name, alias, &id]) {
            rows.push(ResultRow {
                group: Group::Stock,
                label: stock.name.clone(),
                detail: format!("{alias} · {}×{}", mm(stock.length), mm(stock.width)),
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
                detail: localizer.text("navigation-hardware"),
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
                detail: localizer.text("navigation-hardware"),
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
                if (self.cut_plan.repair.active()
                    || self.modals.board_dimension().is_some()
                    || self.modals.placement().is_some()
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
                    || self.cut_plan.optimizer.comparison_open()
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
                    || self.cut_plan.optimizer.comparison_open()
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
        let hint = self.localizer.text("palette-hint");
        let mut clicked = None;
        let mut query_changed = false;
        let width = 560.0_f32.min((ctx.content_rect().width() - 32.0).max(1.0));
        let modal = egui::Modal::new(Id::new("command-palette"))
            .backdrop_color(egui::Color32::from_rgba_unmultiplied(42, 37, 32, 64))
            .frame(
                egui::Frame::new()
                    .fill(tw::PANEL)
                    .stroke(egui::Stroke::new(1.0, tw::BORDER))
                    .corner_radius(12)
                    .shadow(egui::Shadow {
                        offset: [0, 18],
                        blur: 44,
                        spread: 0,
                        color: egui::Color32::from_rgba_unmultiplied(60, 45, 25, 46),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(crate::icons::icon(
                                crate::icons::Icon::Search,
                                tw::FAINT,
                                17.0,
                            ));
                            let edit_width = ui.available_width() - 44.0;
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.palette.query)
                                        .id(Id::new(QUERY_ID))
                                        .hint_text(hint)
                                        .frame(egui::Frame::NONE)
                                        .font(egui::FontId::proportional(15.0))
                                        .desired_width(edit_width),
                                )
                                .changed()
                            {
                                self.palette.selected = 0;
                                self.palette.error = None;
                                query_changed = true;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    tw::keycap(ui, "Esc");
                                },
                            );
                        });
                    });
                tw::divider(ui);
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() * 0.6).clamp(120.0, 440.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                if rows.is_empty() {
                                    ui.add_space(18.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new(
                                                self.localizer.text("palette-no-results"),
                                            )
                                            .color(tw::MUTED),
                                        );
                                    });
                                    ui.add_space(18.0);
                                }
                                let mut group = None;
                                for (index, row) in rows.iter().enumerate() {
                                    if group != Some(row.group) {
                                        group = Some(row.group);
                                        ui.add_space(4.0);
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(ui.available_width(), 24.0),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.add_space(8.0);
                                                ui.label(
                                                    tw::semibold(
                                                        ui,
                                                        self.localizer
                                                            .text(row.group.key())
                                                            .to_uppercase(),
                                                        10.5,
                                                    )
                                                    .color(tw::FAINT),
                                                );
                                            },
                                        );
                                    }
                                    let unavailable = self.palette_availability(row.route).err();
                                    let selected = index == self.palette.selected;
                                    let icon = match row.group {
                                        Group::Actions => crate::icons::Icon::Command,
                                        Group::Boards => crate::icons::Icon::Board,
                                        Group::Materials => crate::icons::Icon::Material,
                                        Group::Stock => crate::icons::Icon::Sheet,
                                        Group::Installations => crate::icons::Icon::Hinge,
                                        Group::Relationships => crate::icons::Icon::Door,
                                    };
                                    let (response, ()) = tw::list_row(
                                        ui,
                                        Id::new(("palette-row", index)),
                                        34.0,
                                        if selected {
                                            tw::RowState::Active
                                        } else {
                                            tw::RowState::Normal
                                        },
                                        unavailable.is_none(),
                                        &row.label,
                                        |ui| {
                                            ui.add_space(2.0);
                                            ui.add(crate::icons::icon(
                                                icon,
                                                if selected { tw::ACCENT_DARK } else { tw::MUTED },
                                                15.0,
                                            ));
                                            let ink = if unavailable.is_some() {
                                                tw::FAINT
                                            } else if selected {
                                                tw::ACCENT_INK
                                            } else {
                                                tw::TEXT
                                            };
                                            let label_width =
                                                (ui.available_width() * 0.62).max(80.0);
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(label_width, 28.0),
                                                egui::Layout::left_to_right(egui::Align::Center),
                                                |ui| {
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(&row.label)
                                                                .color(ink),
                                                        )
                                                        .truncate()
                                                        .selectable(false),
                                                    );
                                                },
                                            );
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.add_space(4.0);
                                                    ui.add(
                                                        egui::Label::new(
                                                            tw::mono(&row.detail, 11.5)
                                                                .color(tw::FAINT),
                                                        )
                                                        .truncate()
                                                        .selectable(false),
                                                    );
                                                },
                                            );
                                        },
                                    );
                                    if let Some(reason) = unavailable {
                                        response.on_hover_text(
                                            reason.reason(self.localizer.language()),
                                        );
                                    } else if response.clicked() {
                                        clicked = Some(index);
                                    }
                                }
                            });
                    });
                if let Some(error) = &self.palette.error {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(16, 6))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(error).size(12.0).color(tw::WARN_INK));
                        });
                }
                egui::Frame::new()
                    .fill(tw::APP)
                    .stroke(egui::Stroke::new(1.0, tw::BORDER_SOFT))
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 12,
                        se: 12,
                    })
                    .inner_margin(egui::Margin::symmetric(16, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(self.localizer.text("palette-keys"))
                                .size(11.5)
                                .color(tw::FAINT),
                        );
                    });
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
mod tests;
