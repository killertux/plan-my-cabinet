//! Nonblocking optimizer controls. This view never writes to an editor except on Accept.
use std::time::Duration;

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
        }
    }
}

impl OptimizeUi {
    pub fn running(&self) -> bool {
        self.worker.is_some()
    }
    fn start(&mut self, editor: &ProjectEditor, blocked: bool) -> bool {
        if blocked || self.worker.is_some() || editor.preview().is_some() {
            return false;
        }
        self.result = None;
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

    fn cancel(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.cancel();
        }
        self.result = None;
        self.notice = Some("optimize-cancelled");
    }

    fn poll(&mut self, ctx: &egui::Context) {
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
        ui.heading(localizer.text("optimize-heading"));
        let previous_objective = self.objective;
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
                            &mut self.objective,
                            objective,
                            localizer.text(objective_key(objective)),
                        );
                    }
                });
        });
        if self.objective != previous_objective {
            // The completed ranking belongs to the objective selected at search start.
            self.result = None;
            self.notice = None;
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !blocked && self.worker.is_none(),
                    egui::Button::new(localizer.text("optimize-start")),
                )
                .clicked()
            {
                self.start(editor, blocked);
            }
            if ui
                .add_enabled(
                    self.worker.is_some(),
                    egui::Button::new(localizer.text("optimize-cancel")),
                )
                .clicked()
            {
                self.cancel();
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
            if let Some(best) = result.ranking.candidates.first() {
                ui.label(localizer.text("optimize-best-found"));
                ui.small(localizer.text("optimize-heuristic"));
                if !result.ranking.lowest_spending_claim
                    && self.objective == Objective::LowestNewSpending
                {
                    ui.small(localizer.text("optimize-cost-no-claim"));
                }
                ui.columns(2, |columns| {
                    columns[0].strong(localizer.text("optimize-current"));
                    columns[1].strong(localizer.text("optimize-best-found"));
                    if let Some(current) = &result.original_plan {
                        summary(&mut columns[0], localizer, current);
                    } else {
                        columns[0].label(localizer.text("optimize-current-unverified"));
                        columns[0].label(format!(
                            "{}: {}",
                            localizer.text("optimize-placements"),
                            result.original_allocations.len()
                        ));
                        let sheets: std::collections::HashSet<_> = result
                            .original_allocations
                            .iter()
                            .map(|a| a.stock_id)
                            .collect();
                        columns[0].label(format!(
                            "{}: {}",
                            localizer.text("optimize-stock-used"),
                            sheets.len()
                        ));
                        for key in [
                            "optimize-cuts-count",
                            "optimize-offcuts",
                            "optimize-unused",
                            "optimize-loss",
                            "optimize-cost",
                        ] {
                            columns[0].label(format!(
                                "{}: {}",
                                localizer.text(key),
                                localizer.text("optimize-unverified")
                            ));
                        }
                    }
                    summary(&mut columns[1], localizer, best);
                });
                let changed = result.placement_changes(0).unwrap_or_default();
                ui.label(format!(
                    "{}: {}",
                    localizer.text("optimize-changes"),
                    changed.len()
                ));
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
                    ui.small(format!(
                        "{name} ({}) · {} → {}",
                        &id.to_string()[..8],
                        placement(editor.project(), old),
                        placement(editor.project(), new)
                    ));
                }
                if ui
                    .add_enabled(
                        !blocked && !stale && editor.preview().is_none(),
                        egui::Button::new(localizer.text("optimize-accept")),
                    )
                    .clicked()
                    && self.accept(editor, blocked).is_err()
                {
                    self.notice = Some("optimize-apply-error");
                }
            } else {
                ui.label(localizer.text("optimize-no-complete"));
            }
        }
        if let Some(key) = self.notice {
            ui.label(localizer.text(key));
        }
        if blocked {
            ui.small(localizer.text("optimize-blocked"));
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
            format!(
                "{stock} ({}, {} mm; {}°)",
                a.origin[0].micrometres() as f64 / 1000.0,
                a.origin[1].micrometres() as f64 / 1000.0,
                if a.quarter_turn { 90 } else { 0 }
            )
        },
    )
}

fn summary(ui: &mut egui::Ui, localizer: &Localizer, plan: &RankedCandidate) {
    ui.label(format!(
        "{}: {}",
        localizer.text("optimize-stock-used"),
        plan.candidate.witnesses.len()
    ));
    ui.label(format!(
        "{}: {}",
        localizer.text("optimize-cuts-count"),
        plan.cuts
    ));
    ui.label(format!(
        "{}: {} mm² ({} {})",
        localizer.text("optimize-offcuts"),
        plan.utilization.recoverable_offcut_area / 1_000_000,
        plan.offcuts.len(),
        localizer.text("optimize-rectangles")
    ));
    ui.label(format!(
        "{}: {} mm²",
        localizer.text("optimize-unused"),
        plan.utilization.unused_stock_area / 1_000_000
    ));
    ui.label(format!(
        "{}: {} mm²",
        localizer.text("optimize-loss"),
        plan.utilization.irreversible_loss_area() / 1_000_000
    ));
    ui.label(format!(
        "{}: {}",
        localizer.text("optimize-cost"),
        plan.new_spending.map_or_else(
            || localizer.text("optimize-unknown"),
            |money| format!(
                "{} {}.{:02}",
                money.currency().code(),
                money.minor_units() / 100,
                money.minor_units() % 100
            )
        )
    ));
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
        click(&ctx, &mut ui, &mut editor, "Accept best found");
        assert_eq!(editor.project().allocations.len(), 1);
        editor.undo().unwrap();
        assert_eq!(editor.project().allocations, original.allocations);
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
            let cost = Localizer::new(language).text("optimize-cost");
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
}
