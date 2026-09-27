//! Nonblocking optimizer controls. This view never writes to an editor except on Accept.
use std::time::Duration;

use crate::actions::{self, ActionId as A, Request, Unavailable};
use crate::icons::Icon;
use crate::modal_chrome::{ModalAction, ModalActions, ModalChrome};
use crate::theme_widgets as tw;
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
    pub(crate) fn acceptance_availability(&self, project: &Project) -> Result<(), Unavailable> {
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

    pub(crate) fn has_result(&self) -> bool {
        self.result.is_some()
    }

    /// Short status for the section header ("current = best found").
    fn status(&self, project: &Project, localizer: &Localizer) -> Option<(String, egui::Color32)> {
        if self.worker.is_some() {
            let mut args = fluent_bundle::FluentArgs::new();
            args.set("count", self.placements);
            return Some((
                localizer.format("optimize-searching", Some(&args)),
                tw::MUTED,
            ));
        }
        let result = self.result.as_ref()?;
        if !result.is_current(project) {
            return Some((localizer.text("optimize-status-stale"), tw::WARN_INK));
        }
        if result.ranking.candidates.is_empty() {
            return Some((localizer.text("optimize-status-none"), tw::WARN_INK));
        }
        if result.placement_changes(0).unwrap_or_default().is_empty() {
            Some((localizer.text("optimize-status-best"), tw::MUTED))
        } else {
            Some((localizer.text("optimize-status-better"), tw::ACCENT_DARK))
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        editor: &mut ProjectEditor,
        localizer: &Localizer,
        blocked: bool,
    ) {
        self.poll(ui.ctx());
        tw::divider(ui);
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 2,
                bottom: 14,
            })
            .show(ui, |ui| {
                self.show_controls(ui, editor, localizer, blocked);
            });
        if self.comparison_open() {
            self.show_comparison(ui.ctx(), editor, localizer, blocked);
        }
    }

    fn show_controls(
        &mut self,
        ui: &mut egui::Ui,
        editor: &mut ProjectEditor,
        localizer: &Localizer,
        blocked: bool,
    ) {
        let status = self.status(editor.project(), localizer);
        let heading = localizer.text("optimize-all-sheets");
        // The status shares the header row only when both fit.
        let heading_width = ui
            .painter()
            .layout_no_wrap(
                heading.to_uppercase(),
                egui::FontId::proportional(11.5),
                tw::FAINT,
            )
            .size()
            .x;
        let status_width = status.as_ref().map_or(0.0, |(text, _)| {
            ui.painter()
                .layout_no_wrap(text.clone(), egui::FontId::proportional(11.0), tw::MUTED)
                .size()
                .x
        });
        let inline = heading_width + status_width + 24.0 <= ui.available_width();
        tw::inspector_heading(ui, &heading, |ui| {
            if inline && let Some((text, color)) = &status {
                ui.label(egui::RichText::new(text).size(11.0).color(*color));
            }
        });
        if !inline && let Some((text, color)) = &status {
            ui.label(egui::RichText::new(text).size(11.0).color(*color));
        }
        ui.add_space(4.0);
        let previous_objective = self.objective;
        let mut objective_choice = self.objective;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.worker.is_some() {
                    if tw::secondary_button(ui, &localizer.text("optimize-cancel")).clicked() {
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
                } else if tw::icon_text_button(
                    ui,
                    Icon::Bolt,
                    &localizer.text("optimize-search"),
                    true,
                    !blocked,
                )
                .on_hover_text(localizer.text("optimize-start"))
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
                let width = ui.available_width().max(80.0);
                ui.add_enabled_ui(!blocked && self.worker.is_none(), |ui| {
                    egui::ComboBox::from_id_salt("optimize-objective")
                        .width(width - 20.0)
                        .height(300.0)
                        .truncate()
                        .selected_text(
                            egui::RichText::new(localizer.text(objective_key(self.objective)))
                                .size(13.0)
                                .color(tw::TEXT),
                        )
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
        ui.add_space(4.0);
        let small = |ui: &mut egui::Ui, text: String, color: egui::Color32| {
            ui.add(egui::Label::new(egui::RichText::new(text).size(11.0).color(color)).wrap());
        };
        if let Some(result) = &self.result {
            if result.exhausted {
                small(ui, localizer.text("optimize-exhausted"), tw::MUTED);
            }
            if result.ranking.candidates.is_empty() {
                small(ui, localizer.text("optimize-no-complete"), tw::WARN_INK);
            }
            if tw::secondary_button_enabled(ui, &localizer.text("optimize-compare"), !blocked)
                .clicked()
            {
                self.open_comparison();
            }
        }
        if let Some(key) = self.notice {
            small(ui, localizer.text(key), tw::MUTED);
        }
        if blocked {
            small(ui, localizer.text("optimize-blocked"), tw::WARN_INK);
        }
        small(ui, localizer.text("optimize-disclaimer"), tw::FAINT);
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
        let mut open = false;
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 0,
                bottom: 14,
            })
            .show(ui, |ui| {
                tw::inspector_heading(ui, &localizer.text("optimize-compare"), |ui| {
                    ui.label(
                        tw::mono(format!("r{}", result.source.revision), 11.0).color(tw::FAINT),
                    )
                    .on_hover_text(localizer.text("optimize-source"));
                });
                let note = |ui: &mut egui::Ui, text: String, color: egui::Color32| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(text).size(11.5).color(color)).wrap(),
                    );
                };
                if !result.is_current(project) {
                    note(ui, localizer.text("optimize-stale"), tw::WARN_INK);
                }
                if result.exhausted {
                    note(ui, localizer.text("optimize-exhausted"), tw::MUTED);
                }
                let Some(best) = result.ranking.candidates.first() else {
                    note(ui, localizer.text("optimize-no-complete"), tw::WARN_INK);
                    return;
                };
                ui.label(
                    tw::medium(ui, localizer.text("optimize-best-found"), 12.5).color(tw::TEXT),
                );
                note(ui, localizer.text("optimize-heuristic"), tw::FAINT);
                if self.objective == Objective::LowestNewSpending
                    && !result.ranking.lowest_spending_claim
                {
                    note(ui, localizer.text("optimize-cost-no-claim"), tw::WARN_INK);
                }
                ui.add_space(4.0);
                tw::card()
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for (index, key) in [
                            "optimize-stock-used",
                            "optimize-cuts-count",
                            "optimize-offcuts",
                            "optimize-unused",
                            "optimize-loss",
                            "optimize-cost",
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            if index > 0 {
                                ui.add_space(4.0);
                            }
                            note(ui, localizer.text(key), tw::MUTED);
                            let current = result.original_plan.as_ref().map_or_else(
                                || current_metric(result, key, localizer),
                                |plan| metric(plan, key, localizer),
                            );
                            ui.add(
                                egui::Label::new(
                                    tw::mono(
                                        format!("{current} → {}", metric(best, key, localizer)),
                                        11.5,
                                    )
                                    .color(tw::TEXT),
                                )
                                .wrap(),
                            )
                            .on_hover_text(format!(
                                "{}: {current}\n{}: {}",
                                localizer.text("optimize-current"),
                                localizer.text("optimize-best-found"),
                                metric(best, key, localizer)
                            ));
                        }
                    });
                if result.original_plan.is_none() {
                    note(ui, localizer.text("optimize-current-unverified"), tw::FAINT);
                }
                let changed = result.placement_changes(0).unwrap_or_default();
                ui.add_space(6.0);
                ui.label(
                    tw::medium(
                        ui,
                        format!("{}: {}", localizer.text("optimize-changes"), changed.len()),
                        12.5,
                    )
                    .color(tw::TEXT),
                );
                if changed.is_empty() {
                    note(ui, localizer.text("optimize-unchanged"), tw::MUTED);
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
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(name).size(12.5).color(tw::TEXT));
                        ui.label(
                            tw::mono(crate::assembly_ui::short_id('b', id), 11.0).color(tw::FAINT),
                        );
                    });
                    note(
                        ui,
                        format!(
                            "{}: {}",
                            localizer.text("optimize-current"),
                            placement(project, old)
                        ),
                        tw::MUTED,
                    );
                    note(
                        ui,
                        format!(
                            "{}: {}",
                            localizer.text("optimize-proposed"),
                            placement(project, new)
                        ),
                        tw::ACCENT_DARK,
                    );
                }
                note(
                    ui,
                    format!(
                        "{}: {}",
                        localizer.text("optimize-locked"),
                        result
                            .original_allocations
                            .iter()
                            .filter(|a| a.locked)
                            .count()
                    ),
                    tw::MUTED,
                );
                ui.add_space(6.0);
                open =
                    tw::primary_button(ui, &localizer.text("optimize-compare"), !blocked).clicked();
            });
        if open {
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
                    ui.colored_label(tw::WARN_INK, localizer.text("optimize-stale"));
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
                    tw::card()
                        .inner_margin(egui::Margin::symmetric(12, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 1.0;
                            for (index, key) in [
                                "optimize-stock-used",
                                "optimize-cuts-count",
                                "optimize-offcuts",
                                "optimize-unused",
                                "optimize-loss",
                                "optimize-cost",
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                if index > 0 {
                                    let (line, _) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), 7.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().hline(
                                        line.x_range(),
                                        line.center().y,
                                        egui::Stroke::new(1.0, tw::BORDER_SOFT),
                                    );
                                }
                                ui.label(
                                    tw::medium(ui, localizer.text(key), 12.5).color(tw::MUTED),
                                );
                                ui.columns(2, |columns| {
                                    columns[0].label(format!(
                                        "{}: {}",
                                        localizer.text("optimize-current"),
                                        result.original_plan.as_ref().map_or_else(
                                            || current_metric(result, key, localizer),
                                            |plan| metric(plan, key, localizer),
                                        )
                                    ));
                                    columns[1].label(
                                        egui::RichText::new(format!(
                                            "{}: {}",
                                            localizer.text("optimize-best-found"),
                                            metric(best, key, localizer)
                                        ))
                                        .color(tw::ACCENT_INK),
                                    );
                                });
                            }
                        });
                    if result.original_plan.is_none() {
                        ui.small(localizer.text("optimize-current-unverified"));
                    }
                    let changed = result.placement_changes(0).unwrap_or_default();
                    ui.add_space(6.0);
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
                        ui.label(format!(
                            "{name} ({})",
                            crate::assembly_ui::short_id('b', id)
                        ));
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
                            crate::assembly_ui::short_id('b', locked.board_id),
                            placement(editor.project(), Some(locked))
                        ));
                        if !preserved {
                            ui.colored_label(
                                tw::WARN_INK,
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
mod tests;
