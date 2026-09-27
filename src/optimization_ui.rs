//! Nonblocking optimizer controls. This view never writes to an editor except on Accept.
use std::time::Duration;

use crate::actions::{self, ActionId as A, Request, Unavailable};
use crate::modal_chrome::{ModalAction, ModalActions, ModalChrome};
use eframe::egui;
use plan_my_cabinet::candidate_generation::SearchBudget;
use plan_my_cabinet::candidate_ranking::{Objective, RankedCandidate};
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::i18n::Localizer;
use plan_my_cabinet::optimization_worker::{
    ApplyError, CompletedSearch, OptimizationWorker, WorkerError, WorkerMessage,
};

const BUDGET: SearchBudget = SearchBudget {
    placements: 10_000,
    witness_states: 20_000,
    beam_width: 8,
};

pub struct OptimizeUi {
    objective: Objective,
    worker: Option<OptimizationWorker>,
    result: Option<CompletedSearch>,
    started_revision: u64,
    placements: usize,
    notice: Option<&'static str>,
    comparison: Option<ModalChrome>,
}

impl Default for OptimizeUi {
    fn default() -> Self {
        Self {
            objective: Objective::LowestNewSpending,
            worker: None,
            result: None,
            started_revision: 0,
            placements: 0,
            notice: None,
            comparison: None,
        }
    }
}

impl OptimizeUi {
    pub fn running(&self) -> bool {
        self.worker.is_some()
    }
    /// The host must include this in its modal/shortcut guard, including while
    /// the Cut plan pane is hidden by a responsive drawer or workspace change.
    pub(crate) fn comparison_open(&self) -> bool {
        self.comparison.is_some()
    }
    pub(super) fn acceptance_availability(&self, project: &Project) -> Result<(), Unavailable> {
        let Some(result) = &self.result else {
            return Err(Unavailable::NoOptimization);
        };
        if !result.is_current(project) {
            return Err(Unavailable::StaleOptimization);
        }
        if result.ranking.candidates.is_empty() {
            return Err(Unavailable::NoOptimization);
        }
        Ok(())
    }
    pub(crate) fn start(&mut self, editor: &ProjectEditor, blocked: bool) -> bool {
        if blocked || self.worker.is_some() || editor.preview().is_some() {
            return false;
        }
        self.result = None;
        self.comparison = None;
        self.notice = None;
        self.placements = 0;
        self.started_revision = editor.project().revision;
        self.worker = Some(OptimizationWorker::start(
            editor,
            self.objective,
            BUDGET,
            Duration::from_secs(5),
        ));
        true
    }

    pub(crate) fn cancel(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.cancel();
        }
        self.result = None;
        self.comparison = None;
        self.notice = Some("optimize-cancelled");
    }

    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
        let Some(worker) = self.worker.as_mut() else {
            return;
        };
        // Bound work per frame even if a fast worker queued thousands of updates.
        for _ in 0..256 {
            match worker.try_receive() {
                Ok(Some(WorkerMessage::Progress { placements })) => self.placements = placements,
                Ok(Some(WorkerMessage::Completed(Ok(result)))) => {
                    self.notice = None;
                    self.result = Some(*result);
                    self.worker = None;
                    break;
                }
                Ok(Some(WorkerMessage::Completed(Err(error)))) | Err(error) => {
                    self.notice = Some(match error {
                        WorkerError::Cancelled => "optimize-cancelled",
                        _ => "optimize-error",
                    });
                    self.worker = None;
                    break;
                }
                Ok(None) => break,
            }
        }
        if self.worker.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn accept(&mut self, editor: &mut ProjectEditor, blocked: bool) -> Result<bool, ApplyError> {
        if blocked || editor.preview().is_some() {
            return Err(ApplyError::Stale {
                started: self.started_revision,
                current: editor.project().revision,
            });
        }
        let result = self.result.as_ref().ok_or(ApplyError::UnknownCandidate)?;
        if !result.is_current(editor.project()) {
            return Err(ApplyError::Stale {
                started: result.source.revision,
                current: editor.project().revision,
            });
        }
        let applied = OptimizationWorker::apply(editor, result, 0)?;
        self.result = None;
        self.notice = Some(if applied {
            "optimize-applied"
        } else {
            "optimize-unchanged"
        });
        Ok(applied)
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        editor: &mut ProjectEditor,
        localizer: &Localizer,
        blocked: bool,
    ) {
        self.poll(ui.ctx());
        ui.add(
            egui::Label::new(egui::RichText::new(localizer.text("optimize-heading")).heading())
                .wrap(),
        );
        let previous_objective = self.objective;
        let mut objective_choice = self.objective;
        ui.add_enabled_ui(!blocked && self.worker.is_none(), |ui| {
            egui::ComboBox::from_id_salt("optimize-objective")
                .selected_text(localizer.text(objective_key(self.objective)))
                .show_ui(ui, |ui| {
                    for objective in [
                        Objective::LowestNewSpending,
                        Objective::FewestCuts,
                        Objective::LeastUnusedStockArea,
                    ] {
                        ui.selectable_value(
                            &mut objective_choice,
                            objective,
                            localizer.text(objective_key(objective)),
                        );
                    }
                });
        });
        if objective_choice != previous_objective
            && actions::contextual(
                Request::new(A::SetOptimizerObjective),
                if blocked || self.worker.is_some() {
                    Err(Unavailable::ModalOpen)
                } else {
                    Ok(())
                },
                || (),
            )
            .is_ok()
        {
            self.objective = objective_choice;
            // The completed ranking belongs to the objective selected at search start.
            self.result = None;
            self.comparison = None;
            self.notice = None;
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !blocked && self.worker.is_none(),
                    egui::Button::new(localizer.text("optimize-start")),
                )
                .clicked()
            {
                let _ = actions::contextual(
                    Request::new(A::StartOptimization),
                    if blocked {
                        Err(Unavailable::ModalOpen)
                    } else {
                        Ok(())
                    },
                    || self.start(editor, blocked),
                );
            }
            if ui
                .add_enabled(
                    self.worker.is_some(),
                    egui::Button::new(localizer.text("optimize-cancel")),
                )
                .clicked()
            {
                let _ = actions::contextual(
                    Request::new(A::CancelOptimization),
                    if self.worker.is_some() {
                        Ok(())
                    } else {
                        Err(Unavailable::NoOptimization)
                    },
                    || self.cancel(),
                );
            }
        });
        if self.worker.is_some() {
            ui.label(format!(
                "{}: {} · {}: {}",
                localizer.text("optimize-source"),
                self.started_revision,
                localizer.text("optimize-progress"),
                self.placements
            ));
        }
        if let Some(result) = &self.result {
            ui.label(format!(
                "{}: {}",
                localizer.text("optimize-source"),
                result.source.revision
            ));
            let stale = !result.is_current(editor.project());
            if stale {
                ui.colored_label(egui::Color32::LIGHT_RED, localizer.text("optimize-stale"));
            }
            if result.exhausted {
                ui.small(localizer.text("optimize-exhausted"));
            }
            if result.ranking.candidates.is_empty() {
                ui.label(localizer.text("optimize-no-complete"));
            }
            if ui
                .add_enabled(
                    !blocked,
                    egui::Button::new(localizer.text("optimize-compare")),
                )
                .clicked()
            {
                self.open_comparison();
            }
        }
        if let Some(key) = self.notice {
            ui.label(localizer.text(key));
        }
        if blocked {
            ui.small(localizer.text("optimize-blocked"));
        }
        if self.comparison_open() {
            self.show_comparison(ui.ctx(), editor, localizer, blocked);
        }
    }

    /// Compact inspector counterpart to the expanded, scrollable comparison.
    /// The shell supplies the inspector's own scroll container; a modal is an
    /// optional enlarged view, not the only place the comparison can be read.
    pub(crate) fn show_inspector_comparison(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        localizer: &Localizer,
        blocked: bool,
    ) {
        let Some(result) = &self.result else {
            return;
        };
        ui.separator();
        ui.heading(localizer.text("optimize-compare"));
        ui.small(format!(
            "{}: {}",
            localizer.text("optimize-source"),
            result.source.revision
        ));
        if !result.is_current(project) {
            ui.colored_label(egui::Color32::LIGHT_RED, localizer.text("optimize-stale"));
        }
        if result.exhausted {
            ui.small(localizer.text("optimize-exhausted"));
        }
        if let Some(best) = result.ranking.candidates.first() {
            ui.strong(localizer.text("optimize-best-found"));
            ui.small(localizer.text("optimize-heuristic"));
            if self.objective == Objective::LowestNewSpending
                && !result.ranking.lowest_spending_claim
            {
                ui.small(localizer.text("optimize-cost-no-claim"));
            }
            for key in [
                "optimize-stock-used",
                "optimize-cuts-count",
                "optimize-offcuts",
                "optimize-unused",
                "optimize-loss",
                "optimize-cost",
            ] {
                ui.group(|ui| {
                    ui.strong(localizer.text(key));
                    ui.small(format!(
                        "{}: {}",
                        localizer.text("optimize-current"),
                        result.original_plan.as_ref().map_or_else(
                            || current_metric(result, key, localizer),
                            |plan| metric(plan, key, localizer),
                        )
                    ));
                    ui.small(format!(
                        "{}: {}",
                        localizer.text("optimize-best-found"),
                        metric(best, key, localizer)
                    ));
                });
            }
            if result.original_plan.is_none() {
                ui.small(localizer.text("optimize-current-unverified"));
            }
            let changed = result.placement_changes(0).unwrap_or_default();
            ui.strong(format!(
                "{}: {}",
                localizer.text("optimize-changes"),
                changed.len()
            ));
            if changed.is_empty() {
                ui.label(localizer.text("optimize-unchanged"));
            }
            for id in changed {
                let name = project
                    .boards
                    .iter()
                    .find(|board| board.id == id)
                    .map_or("?", |board| board.name.as_str());
                let old = result
                    .original_allocations
                    .iter()
                    .find(|a| a.board_id == id);
                let new = best.candidate.allocations.iter().find(|a| a.board_id == id);
                ui.label(format!("{name} ({})", &id.to_string()[..8]));
                ui.indent(id, |ui| {
                    ui.small(format!(
                        "{}: {}",
                        localizer.text("optimize-current"),
                        placement(project, old)
                    ));
                    ui.small(format!(
                        "{}: {}",
                        localizer.text("optimize-proposed"),
                        placement(project, new)
                    ));
                });
            }
            ui.small(format!(
                "{}: {}",
                localizer.text("optimize-locked"),
                result
                    .original_allocations
                    .iter()
                    .filter(|a| a.locked)
                    .count()
            ));
        } else {
            ui.label(localizer.text("optimize-no-complete"));
        }
        if ui
            .add_enabled(
                !blocked,
                egui::Button::new(localizer.text("optimize-compare")),
            )
            .clicked()
        {
            self.open_comparison();
        }
    }

    pub(crate) fn open_comparison(&mut self) {
        if self.result.is_some() {
            self.comparison =
                Some(ModalChrome::new(egui::Id::new("optimize-comparison")).width(800.0));
        }
    }

    /// Call from the host when the Cut plan controls pane is collapsed but the
    /// review remains open, with the same external-dialog/repair guard as `show`.
    pub(crate) fn show_comparison(
        &mut self,
        ctx: &egui::Context,
        editor: &mut ProjectEditor,
        localizer: &Localizer,
        blocked: bool,
    ) {
        let Some(chrome) = &mut self.comparison else {
            return;
        };
        let Some(result) = &self.result else {
            self.comparison = None;
            return;
        };
        let stale = !result.is_current(editor.project());
        let locks_preserved = result.ranking.candidates.first().is_some_and(|best| {
            result
                .original_allocations
                .iter()
                .filter(|a| a.locked)
                .all(|old| {
                    best.candidate.allocations.iter().any(|a| {
                        a.board_id == old.board_id
                            && a.stock_id == old.stock_id
                            && a.origin == old.origin
                            && a.quarter_turn == old.quarter_turn
                            && a.locked
                    })
                })
        });
        let valid = !blocked && !stale && editor.preview().is_none() && locks_preserved;
        let review = chrome.show(
            ctx,
            &localizer.text("optimize-compare"),
            ModalActions {
                cancel: &localizer.text("optimize-close"),
                confirm: &localizer.text("optimize-accept"),
            },
            |ui| {
                ui.label(format!(
                    "{}: {}",
                    localizer.text("optimize-source"),
                    result.source.revision
                ));
                if stale {
                    ui.colored_label(egui::Color32::LIGHT_RED, localizer.text("optimize-stale"));
                }
                if result.exhausted {
                    ui.small(localizer.text("optimize-exhausted"));
                }
                if let Some(best) = result.ranking.candidates.first() {
                    ui.strong(localizer.text("optimize-best-found"));
                    ui.small(localizer.text("optimize-heuristic"));
                    if !result.ranking.lowest_spending_claim
                        && self.objective == Objective::LowestNewSpending
                    {
                        ui.small(localizer.text("optimize-cost-no-claim"));
                    }
                    // Metric labels span the review width; values have two real
                    // columns and wrap independently even in compact pt-BR layouts.
                    ui.label(format!(
                        "{}: {}",
                        localizer.text("optimize-placements"),
                        result.original_allocations.len()
                    ));
                    for key in [
                        "optimize-stock-used",
                        "optimize-cuts-count",
                        "optimize-offcuts",
                        "optimize-unused",
                        "optimize-loss",
                        "optimize-cost",
                    ] {
                        ui.separator();
                        ui.strong(localizer.text(key));
                        ui.columns(2, |columns| {
                            columns[0].label(format!(
                                "{}: {}",
                                localizer.text("optimize-current"),
                                result.original_plan.as_ref().map_or_else(
                                    || current_metric(result, key, localizer),
                                    |plan| metric(plan, key, localizer),
                                )
                            ));
                            columns[1].label(format!(
                                "{}: {}",
                                localizer.text("optimize-best-found"),
                                metric(best, key, localizer)
                            ));
                        });
                    }
                    if result.original_plan.is_none() {
                        ui.small(localizer.text("optimize-current-unverified"));
                    }
                    let changed = result.placement_changes(0).unwrap_or_default();
                    ui.separator();
                    ui.strong(format!(
                        "{}: {}",
                        localizer.text("optimize-changes"),
                        changed.len()
                    ));
                    if changed.is_empty() {
                        ui.label(localizer.text("optimize-unchanged"));
                    }
                    for id in changed {
                        let name = editor
                            .project()
                            .boards
                            .iter()
                            .find(|b| b.id == id)
                            .map_or("?", |b| b.name.as_str());
                        let old = result
                            .original_allocations
                            .iter()
                            .find(|a| a.board_id == id);
                        let new = best.candidate.allocations.iter().find(|a| a.board_id == id);
                        ui.label(format!("{name} ({})", &id.to_string()[..8]));
                        ui.indent(id, |ui| {
                            ui.label(format!(
                                "{}: {}",
                                localizer.text("optimize-current"),
                                placement(editor.project(), old)
                            ));
                            ui.label(format!(
                                "{}: {}",
                                localizer.text("optimize-proposed"),
                                placement(editor.project(), new)
                            ));
                        });
                    }
                    let locks: Vec<_> = result
                        .original_allocations
                        .iter()
                        .filter(|a| a.locked)
                        .collect();
                    ui.label(format!(
                        "{}: {}",
                        localizer.text("optimize-locked"),
                        locks.len()
                    ));
                    for locked in locks {
                        let proposed = best
                            .candidate
                            .allocations
                            .iter()
                            .find(|a| a.board_id == locked.board_id);
                        let preserved = proposed.is_some_and(|a| {
                            a.stock_id == locked.stock_id
                                && a.origin == locked.origin
                                && a.quarter_turn == locked.quarter_turn
                                && a.locked
                        });
                        let name = editor
                            .project()
                            .boards
                            .iter()
                            .find(|b| b.id == locked.board_id)
                            .map_or("?", |b| b.name.as_str());
                        ui.label(format!(
                            "{name} ({}): {}",
                            &locked.board_id.to_string()[..8],
                            placement(editor.project(), Some(locked))
                        ));
                        if !preserved {
                            ui.colored_label(
                                egui::Color32::LIGHT_RED,
                                localizer.text("optimize-lock-mismatch"),
                            );
                        }
                    }
                } else {
                    ui.label(localizer.text("optimize-no-complete"));
                }
                ((), valid)
            },
        );
        match review.action {
            ModalAction::Cancel => self.close_comparison(ctx),
            ModalAction::Confirm if valid => {
                if actions::contextual(Request::new(A::AcceptOptimization), Ok(()), || {
                    self.accept(editor, blocked)
                })
                .map_or(true, |r| r.is_err())
                {
                    self.notice = Some("optimize-apply-error");
                } else {
                    self.close_comparison(ctx);
                }
            }
            _ => {}
        }
    }

    fn close_comparison(&mut self, ctx: &egui::Context) {
        if let Some(mut chrome) = self.comparison.take() {
            chrome.close(ctx);
        }
    }
}

fn objective_key(objective: Objective) -> &'static str {
    match objective {
        Objective::LowestNewSpending => "optimize-spending",
        Objective::FewestCuts => "optimize-cuts",
        Objective::LeastUnusedStockArea => "optimize-area",
    }
}

fn placement(
    project: &Project,
    allocation: Option<&plan_my_cabinet::domain::Allocation>,
) -> String {
    allocation.map_or_else(
        || "—".into(),
        |a| {
            let stock = project
                .stock
                .iter()
                .find(|s| s.id == a.stock_id)
                .map_or("?", |s| s.name.as_str());
            let alias = project.stock_alias(a.stock_id).unwrap_or("?");
            format!(
                "{alias} · {stock} ({}, {} mm; {}°)",
                a.origin[0].micrometres() as f64 / 1000.0,
                a.origin[1].micrometres() as f64 / 1000.0,
                if a.quarter_turn { 90 } else { 0 }
            )
        },
    )
}

fn current_metric(result: &CompletedSearch, key: &str, localizer: &Localizer) -> String {
    match key {
        "optimize-stock-used" => result
            .original_allocations
            .iter()
            .map(|a| a.stock_id)
            .collect::<std::collections::HashSet<_>>()
            .len()
            .to_string(),
        _ => localizer.text("optimize-unverified"),
    }
}

fn metric(plan: &RankedCandidate, key: &str, localizer: &Localizer) -> String {
    match key {
        "optimize-stock-used" => plan.candidate.witnesses.len().to_string(),
        "optimize-cuts-count" => plan.cuts.to_string(),
        "optimize-offcuts" => format!(
            "{} mm² ({} {})",
            plan.utilization.recoverable_offcut_area / 1_000_000,
            plan.offcuts.len(),
            localizer.text("optimize-rectangles")
        ),
        "optimize-unused" => format!("{} mm²", plan.utilization.unused_stock_area / 1_000_000),
        "optimize-loss" => format!(
            "{} mm²",
            plan.utilization.irreversible_loss_area() / 1_000_000
        ),
        "optimize-cost" => plan.new_spending.map_or_else(
            || localizer.text("optimize-unknown"),
            |money| {
                format!(
                    "{} {}.{:02}",
                    money.currency().code(),
                    money.minor_units() / 100,
                    money.minor_units() % 100
                )
            },
        ),
        _ => unreachable!("comparison metric"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::domain::{Board, BoardGrain, Material, Stock, StockGrain, StockSource};
    use plan_my_cabinet::i18n::Language;
    use plan_my_cabinet::money::Currency;
    use plan_my_cabinet::units::{Length, Pose, Quaternion};
    use uuid::Uuid;

    fn fixture() -> ProjectEditor {
        let mut project = Project::new("ui", Currency::Brl);
        let material = Uuid::new_v4();
        let mm = |n: i64| Length::from_micrometres(n * 1000);
        project.materials.push(Material {
            id: material,
            name: "ply".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Unrestricted,
        });
        project.boards.push(Board {
            id: Uuid::new_v4(),
            name: "part".into(),
            material_id: material,
            length: mm(100),
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
        });
        project.stock.push(Stock {
            id: Uuid::new_v4(),
            name: "sheet".into(),
            material_id: material,
            length: mm(100),
            width: mm(50),
            thickness: mm(18),
            grain: StockGrain::Nondirectional,
            source: StockSource::Owned,
            price: None,
            priority: 0,
            trim: [Length::ZERO; 4],
        });
        ProjectEditor::new(project).unwrap()
    }

    fn frame_in_language(
        ctx: &egui::Context,
        state: &mut OptimizeUi,
        editor: &mut ProjectEditor,
        events: Vec<egui::Event>,
        language: Language,
    ) -> Vec<(String, egui::Pos2)> {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 900.),
                )),
                events,
                ..Default::default()
            },
            |ui| state.show(ui, editor, &Localizer::new(language), false),
        );
        let text = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(t) => Some((t.galley.text().to_owned(), t.pos)),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        text
    }

    fn frame(
        ctx: &egui::Context,
        state: &mut OptimizeUi,
        editor: &mut ProjectEditor,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Pos2)> {
        frame_in_language(ctx, state, editor, events, Language::En)
    }

    fn click(ctx: &egui::Context, state: &mut OptimizeUi, editor: &mut ProjectEditor, label: &str) {
        let shapes = frame(ctx, state, editor, vec![]);
        let pos = shapes
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing {label}: {shapes:?}"))
            .1
            + egui::vec2(10., 8.);
        frame(
            ctx,
            state,
            editor,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            ctx,
            state,
            editor,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }

    fn finish(ctx: &egui::Context, state: &mut OptimizeUi, editor: &mut ProjectEditor) {
        for _ in 0..1000 {
            frame(ctx, state, editor, vec![]);
            if state.worker.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("search timed out");
    }

    #[test]
    fn synthetic_start_cancel_stale_preview_and_atomic_accept() {
        let ctx = egui::Context::default();
        let mut editor = fixture();
        let original = editor.project().clone();
        let mut ui = OptimizeUi::default();
        assert!(!ui.start(&editor, true)); // modal or sheet repair
        click(&ctx, &mut ui, &mut editor, "Start optimization");
        assert!(ui.worker.is_some());
        // The tiny fixture may finish before a second pointer event; cancellation
        // still invalidates an already delivered proposal as well as an active run.
        ui.cancel();
        assert!(ui.worker.is_none() && ui.result.is_none());
        assert_eq!(editor.project(), &original);
        assert!(!editor.can_undo());

        click(&ctx, &mut ui, &mut editor, "Start optimization");
        finish(&ctx, &mut ui, &mut editor);
        assert!(
            ui.result
                .as_ref()
                .is_some_and(|r| !r.ranking.candidates.is_empty())
        );
        assert!(ui.accept(&mut editor, true).is_err());
        assert_eq!(editor.project(), &original); // preview is read-only
        assert!(!editor.can_undo());
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].length = Length::from_micrometres(99_000);
                Ok(())
            })
            .unwrap();
        frame(&ctx, &mut ui, &mut editor, vec![]);
        assert!(!ui.result.as_ref().unwrap().is_current(editor.project()));
        assert!(matches!(
            ui.accept(&mut editor, false),
            Err(ApplyError::Stale { .. })
        ));
        editor.undo().unwrap();
        click(&ctx, &mut ui, &mut editor, "Start optimization");
        finish(&ctx, &mut ui, &mut editor);
        click(&ctx, &mut ui, &mut editor, "Review current vs best");
        assert!(ui.comparison_open());
        click(&ctx, &mut ui, &mut editor, "Accept best found");
        assert!(!ui.comparison_open());
        assert_eq!(editor.project().allocations.len(), 1);
        editor.undo().unwrap();
        assert_eq!(editor.project().allocations, original.allocations);
    }

    #[test]
    fn compact_inspector_exposes_metrics_and_placements_without_accepting() {
        let mut editor = fixture();
        let mut state = OptimizeUi::default();
        let ctx = egui::Context::default();
        assert!(state.start(&editor, false));
        finish(&ctx, &mut state, &mut editor);
        let original = editor.project().clone();
        for language in [Language::En, Language::PtBr] {
            let localizer = Localizer::new(language);
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(308.0, 650.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        state.show_inspector_comparison(ui, editor.project(), &localizer, false);
                    });
                },
            );
            let labels = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            for key in [
                "optimize-stock-used",
                "optimize-cuts-count",
                "optimize-changes",
            ] {
                assert!(
                    labels
                        .iter()
                        .any(|label| label.contains(&localizer.text(key))),
                    "{labels:?}"
                );
            }
            assert!(
                labels.iter().any(|label| label.contains("part")),
                "{labels:?}"
            );
            output.drop_without_applying_deltas();
        }
        assert_eq!(editor.project(), &original);
        assert!(!editor.can_undo());
        assert!(!state.comparison_open());
    }

    #[test]
    fn host_modal_guard_blocks_palette_and_project_actions_during_comparison() {
        use crate::DesktopApp;
        let ctx = egui::Context::default();
        let mut app = DesktopApp {
            editor: fixture(),
            ..Default::default()
        };
        click(
            &ctx,
            &mut app.optimizer,
            &mut app.editor,
            "Start optimization",
        );
        finish(&ctx, &mut app.optimizer, &mut app.editor);
        click(
            &ctx,
            &mut app.optimizer,
            &mut app.editor,
            "Review current vs best",
        );
        assert!(app.optimizer.comparison_open());
        assert!(app.modal_open());
        let before = app.editor.project().clone();
        for action in [A::NewBoard, A::OpenHandoff, A::NewProject] {
            assert!(app.invoke(Request::new(action)).is_err(), "{action:?}");
        }
        assert_eq!(app.editor.project(), &before);
        assert!(app.optimizer.comparison_open());
    }

    #[test]
    fn palette_optimizer_actions_start_cancel_and_route_to_review_without_auto_apply() {
        use crate::{
            DesktopApp,
            workspace_state::{Workspace, WorkspaceSession},
        };
        let mut app = DesktopApp {
            editor: fixture(),
            ..Default::default()
        };
        app.session = WorkspaceSession::new(app.editor.project());
        let before = app.editor.project().clone();
        app.invoke(Request::new(A::StartOptimization)).unwrap();
        assert!(app.optimizer.running());
        assert_eq!(app.editor.project(), &before);
        app.invoke(Request::new(A::CancelOptimization)).unwrap();
        assert!(!app.optimizer.running());
        assert_eq!(app.editor.project(), &before);

        app.invoke(Request::new(A::StartOptimization)).unwrap();
        let ctx = egui::Context::default();
        for _ in 0..1000 {
            app.optimizer.poll(&ctx);
            if !app.optimizer.running() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            app.optimizer
                .acceptance_availability(app.editor.project())
                .is_ok()
        );
        app.invoke(Request::new(A::AcceptOptimization)).unwrap();
        assert_eq!(app.session.active, Workspace::CutPlan);
        assert!(app.optimizer.comparison_open());
        assert_eq!(app.editor.project(), &before);
        assert!(!app.editor.can_undo());
    }

    #[test]
    fn palette_objective_route_exposes_controls_without_changing_objective_or_project() {
        use crate::{
            DesktopApp, workspace_shell,
            workspace_state::{Workspace, WorkspaceSession},
        };
        let mut app = DesktopApp {
            editor: fixture(),
            ..Default::default()
        };
        app.session = WorkspaceSession::new(app.editor.project());
        let project = app.editor.project().clone();
        let objective = app.optimizer.objective;
        app.invoke(Request::new(A::SetOptimizerObjective)).unwrap();
        assert_eq!(app.session.active, Workspace::CutPlan);
        assert_eq!(app.open_drawer, Some(workspace_shell::Drawer::Controls));
        assert_eq!(app.optimizer.objective, objective);
        assert_eq!(app.editor.project(), &project);
    }

    #[test]
    fn running_search_reports_activity_across_workspaces_without_mutating_project() {
        use crate::{
            DesktopApp,
            workspace_state::{Workspace, WorkspaceSession},
        };
        let mut app = DesktopApp {
            editor: fixture(),
            ..Default::default()
        };
        app.session = WorkspaceSession::new(app.editor.project());
        let before = app.editor.project().clone();
        app.invoke(Request::new(A::StartOptimization)).unwrap();
        let ctx = egui::Context::default();
        for workspace in [Workspace::Design, Workspace::Hardware, Workspace::Handoff] {
            app.session.switch(workspace);
            let (issues, total, invalid) = app.shell_facts();
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                app.show_shell_status(ui, issues, total, invalid)
            });
            assert!(output.shapes.iter().any(|shape| {
                match &shape.shape {
                    egui::Shape::Text(text) => text
                        .galley
                        .text()
                        .contains(&app.localizer.text("shell-search-active")),
                    _ => false,
                }
            }));
            output.drop_without_applying_deltas();
        }
        assert_eq!(app.editor.project(), &before);
        app.invoke(Request::new(A::CancelOptimization)).unwrap();
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn bilingual_preview_labels_best_found_and_unknown_cost_without_zero() {
        let ctx = egui::Context::default();
        let mut editor = fixture();
        editor
            .transact(|p| -> Result<(), ()> {
                p.stock[0].source = StockSource::ToPurchase;
                p.stock[0].price = None;
                p.cut_fee = Some(plan_my_cabinet::money::Money::new(Currency::Brl, 0).unwrap());
                Ok(())
            })
            .unwrap();
        let mut ui = OptimizeUi::default();
        assert!(ui.start(&editor, false));
        finish(&ctx, &mut ui, &mut editor);
        click(&ctx, &mut ui, &mut editor, "Review current vs best");
        assert_eq!(
            ui.result.as_ref().unwrap().ranking.candidates[0].new_spending,
            None
        );
        for (language, best, unknown, warning) in [
            (
                Language::En,
                "Best found (complete, verified)",
                "Incomplete (unknown price or cut fee)",
                "Some costs are unknown; no lowest-spending claim is possible.",
            ),
            (
                Language::PtBr,
                "Melhor encontrado (completo e verificado)",
                "Incompleta (preço ou tarifa desconhecidos)",
                "Alguns custos são desconhecidos; não é possível afirmar a menor despesa.",
            ),
        ] {
            let labels: Vec<_> = frame_in_language(&ctx, &mut ui, &mut editor, vec![], language)
                .into_iter()
                .map(|(text, _)| text)
                .collect();
            assert!(labels.iter().any(|text| text == best), "{labels:?}");
            assert!(labels.iter().any(|text| text == warning), "{labels:?}");
            let cost = Localizer::new(language).text("optimize-best-found");
            assert!(
                labels
                    .iter()
                    .any(|text| text == &format!("{cost}: {unknown}")),
                "{labels:?}"
            );
            assert!(
                !labels
                    .iter()
                    .any(|text| text.starts_with(&format!("{cost}: BRL 0"))),
                "{labels:?}"
            );
        }
    }

    #[test]
    fn comparison_is_wide_at_reference_and_compact_sizes_and_blocks_stale_acceptance() {
        for (language, size) in [Language::En, Language::PtBr]
            .into_iter()
            .flat_map(|language| {
                [egui::vec2(1440., 900.), egui::vec2(900., 650.)].map(|size| (language, size))
            })
        {
            let ctx = egui::Context::default();
            let mut editor = fixture();
            let mut state = OptimizeUi::default();
            assert!(state.start(&editor, false));
            for _ in 0..1000 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| {
                        ui.set_max_width(256.);
                        state.show(ui, &mut editor, &Localizer::new(language), false);
                    },
                );
                output.drop_without_applying_deltas();
                if state.worker.is_none() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(state.worker.is_none());
            state.comparison =
                Some(ModalChrome::new(egui::Id::new("optimize-comparison")).width(800.));
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    ui.set_max_width(256.);
                    state.show(ui, &mut editor, &Localizer::new(language), false);
                },
            );
            output.drop_without_applying_deltas(); // modal opens after the controls pane this frame
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    ui.set_max_width(256.);
                    state.show(ui, &mut editor, &Localizer::new(language), false);
                },
            );
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            let rect = ctx
                .memory(|m| m.area_rect(egui::Id::new("optimize-comparison")))
                .unwrap();
            assert!(rect.width() >= 650., "modal width: {rect:?}");
            assert!(rect.height() <= size.y, "modal height: {rect:?}");
            assert!(
                rect.min.x >= 0. && rect.max.x <= size.x,
                "modal bounds: {rect:?}"
            );
            assert!(
                texts
                    .iter()
                    .any(|s| s == &Localizer::new(language).text("optimize-cost")),
                "{texts:?}"
            );
            assert!(
                texts
                    .iter()
                    .any(|s| s == &Localizer::new(language).text("optimize-current-unverified")),
                "{texts:?}"
            );
            let mut texts = Vec::new();
            for scroll in [true, false] {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        events: if scroll {
                            vec![
                                egui::Event::PointerMoved(rect.center()),
                                egui::Event::MouseWheel {
                                    unit: egui::MouseWheelUnit::Point,
                                    delta: egui::vec2(0., -900.),
                                    phase: egui::TouchPhase::Move,
                                    modifiers: egui::Modifiers::NONE,
                                },
                            ]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    },
                    |ui| {
                        ui.set_max_width(256.);
                        state.show(ui, &mut editor, &Localizer::new(language), false);
                    },
                );
                texts = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                        _ => None,
                    })
                    .collect();
                output.drop_without_applying_deltas();
            }
            assert!(
                texts
                    .iter()
                    .any(|s| s.starts_with(&Localizer::new(language).text("optimize-proposed"))),
                "{texts:?}"
            );
            editor
                .transact(|p| -> Result<(), ()> {
                    p.boards[0].length = Length::from_micrometres(99_000);
                    Ok(())
                })
                .unwrap();
            assert!(matches!(
                state.acceptance_availability(editor.project()),
                Err(Unavailable::StaleOptimization)
            ));
            assert!(matches!(
                state.accept(&mut editor, false),
                Err(ApplyError::Stale { .. })
            ));
            state.close_comparison(&ctx);
        }
    }

    #[test]
    fn all_objectives_no_result_and_locked_unchanged_layout() {
        let ctx = egui::Context::default();
        let mut editor = fixture();
        let mut state = OptimizeUi::default();
        for objective in [
            Objective::LowestNewSpending,
            Objective::FewestCuts,
            Objective::LeastUnusedStockArea,
        ] {
            state.objective = objective;
            assert!(state.start(&editor, false));
            finish(&ctx, &mut state, &mut editor);
            assert_eq!(state.objective, objective);
            assert!(!state.result.as_ref().unwrap().ranking.candidates.is_empty());
        }
        assert!(state.accept(&mut editor, false).unwrap());
        editor
            .transact(|p| -> Result<(), ()> {
                p.allocations[0].locked = true;
                Ok(())
            })
            .unwrap();
        assert!(state.start(&editor, false));
        finish(&ctx, &mut state, &mut editor);
        let result = state.result.as_ref().unwrap();
        let locked = &result.original_allocations[0];
        assert!(locked.locked);
        let proposed = &result.ranking.candidates[0].candidate.allocations[0];
        assert_eq!(locked, proposed);
        assert!(result.placement_changes(0).unwrap().is_empty());
        let before = editor.project().clone();
        assert!(!state.accept(&mut editor, false).unwrap());
        assert_eq!(editor.project(), &before);

        assert!(state.start(&editor, false));
        finish(&ctx, &mut state, &mut editor);
        state.result.as_mut().unwrap().ranking.candidates[0]
            .candidate
            .allocations[0]
            .locked = false;
        state.comparison = Some(ModalChrome::new(egui::Id::new("optimize-comparison")).width(800.));
        frame(&ctx, &mut state, &mut editor, vec![]);
        click(&ctx, &mut state, &mut editor, "Accept best found");
        assert_eq!(editor.project(), &before);
        assert!(state.comparison_open());
        state.close_comparison(&ctx);

        assert!(state.start(&editor, false));
        finish(&ctx, &mut state, &mut editor);
        state.result.as_mut().unwrap().ranking.candidates.clear();
        assert_eq!(
            state.acceptance_availability(editor.project()),
            Err(Unavailable::NoOptimization)
        );
        state.comparison = Some(ModalChrome::new(egui::Id::new("optimize-comparison")).width(800.));
        let labels = frame(&ctx, &mut state, &mut editor, vec![]);
        assert!(labels.iter().any(|(s, _)| s == "No complete verified plan found within the search budget. The current layout is unchanged."));
        assert!(matches!(
            state.accept(&mut editor, false),
            Err(ApplyError::UnknownCandidate)
        ));
    }
}
