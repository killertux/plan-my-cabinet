//! Stock workspace. Sheet coordinates are manufacturing micrometres;
//! selection is shared with the 3D viewport, never stored in the project.
use eframe::egui;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::cut_tree::{
    CutError, Reconstruction, ReconstructionViolation, WitnessError, reconstruct_witness,
};
use plan_my_cabinet::dimension_input::{Locale, format_length, parse_length};
use plan_my_cabinet::domain::{Allocation, Board, Project, Stock, StockGrain};
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::material_changes::{ConflictReason, allocation_conflicts};
use plan_my_cabinet::sheet_edit::SheetEditSession;
use plan_my_cabinet::units::{Conversion, Length, Unit};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::viewport::Selection;

const WITNESS_BUDGET: usize = 20_000;

// Previews can change without advancing the document revision. Comparing their
// manufacturing inputs is cheap relative to reconstructing ten cut witnesses.
pub(crate) type DiagnosticsKey = (Uuid, u64, Option<Vec<u8>>);
type AffectedCache = (DiagnosticsKey, Vec<Uuid>, Vec<(Uuid, &'static str)>);

pub(crate) fn diagnostics_key(project: &Project, preview: bool) -> DiagnosticsKey {
    (
        project.id,
        project.revision,
        preview.then(|| {
            serde_json::to_vec(&(
                &project.materials,
                &project.boards,
                &project.stock,
                &project.allocations,
                project.cutting_kerf,
            ))
            .expect("project diagnostics inputs serialize")
        }),
    )
}

#[derive(Default)]
pub struct RepairUi {
    diagnostics_cache: Option<(DiagnosticsKey, HashMap<Uuid, Vec<Issue>>)>,
    affected_cache: Option<AffectedCache>,
    affected: Option<HashSet<Uuid>>,
    board: Option<Uuid>,
    stock: Option<Uuid>,
    origin: [String; 2],
    consent: [bool; 2],
    quarter_turn: bool,
    error: Option<&'static str>,
    drag: Option<SheetDrag>,
}

struct SheetDrag {
    board: Uuid,
    stock: Uuid,
    origin: [Length; 2],
    turn: bool,
    start: egui::Pos2,
    candidate: Option<([Length; 2], DragStatus)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragStatus {
    Verified,
    Violation(Issue),
    Exhausted,
}

impl DragStatus {
    fn key(self) -> &'static str {
        match self {
            Self::Verified => "sheet-verified",
            Self::Violation(issue) => issue.key(),
            Self::Exhausted => "sheet-feasibility-unknown",
        }
    }

    fn color(self) -> egui::Color32 {
        match self {
            Self::Verified => egui::Color32::GREEN,
            Self::Violation(_) => egui::Color32::RED,
            Self::Exhausted => egui::Color32::from_rgb(220, 155, 36),
        }
    }
}

impl RepairUi {
    pub fn active(&self) -> bool {
        self.affected.is_some()
    }

    pub fn begin(
        &mut self,
        editor: &mut ProjectEditor,
        selection: &Selection,
        locale: Locale,
    ) -> bool {
        if self.active() || editor.preview().is_some() {
            return false;
        }
        editor.begin_preview();
        self.affected = Some(HashSet::new());
        self.select(editor.project(), selection.active, locale);
        true
    }

    fn select(&mut self, project: &Project, id: Option<Uuid>, locale: Locale) {
        if self.board == id {
            return;
        }
        self.board = id;
        self.consent = [false; 2];
        self.error = None;
        if let Some(allocation) = project.allocations.iter().find(|a| Some(a.board_id) == id) {
            self.stock = Some(allocation.stock_id);
            self.quarter_turn = allocation.quarter_turn;
            self.origin = allocation
                .origin
                .map(|v| format_length(v, Unit::Mm, locale, 3));
        } else {
            self.stock = project.ordered_stock().first().map(|s| s.id);
            self.quarter_turn = false;
            self.origin = ["0 mm".into(), "0 mm".into()];
        }
    }
}

fn locale(localizer: &Localizer) -> Locale {
    if localizer.language() == Language::En {
        Locale::En
    } else {
        Locale::PtBr
    }
}

fn coordinate(text: &str, unit: Unit, consent: bool) -> Option<Length> {
    let parsed = parse_length(text, unit).ok()?.conversion;
    match parsed {
        Conversion::Exact(v) => Some(v),
        Conversion::NeedsConfirmation(v) if consent => Some(v),
        _ => None,
    }
}

fn stage(ui_state: &mut RepairUi, editor: &mut ProjectEditor, action: RepairAction) {
    let Some(affected) = ui_state.affected.take() else {
        return;
    };
    let Some(mut session) = SheetEditSession::resume(editor, affected) else {
        return;
    };
    let result = match action {
        RepairAction::Place(id, stock, origin, turn) => session.place(id, stock, origin, turn),
        RepairAction::Unallocate(id) => session.unallocate(id),
        RepairAction::Lock(id, locked) => session.set_lock(id, locked),
    };
    ui_state.error = result.err().map(|err| match err {
        plan_my_cabinet::sheet_edit::SheetEditError::Locked(_) => "sheet-locked-error",
        _ => "sheet-edit-error",
    });
    ui_state.affected = Some(session.pause());
}

enum RepairAction {
    Place(Uuid, Uuid, [Length; 2], bool),
    Unallocate(Uuid),
    Lock(Uuid, bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Issue {
    Material,
    Thickness,
    Grain,
    Bounds,
    Overlap,
    Kerf,
    Cutting,
    Unknown,
}

impl Issue {
    fn key(self) -> &'static str {
        match self {
            Self::Material => "conflict-material-identity",
            Self::Thickness => "conflict-thickness",
            Self::Grain => "conflict-grain",
            Self::Bounds => "conflict-outside-stock",
            Self::Overlap => "conflict-overlap",
            Self::Kerf => "sheet-kerf-conflict",
            Self::Cutting => "sheet-cut-conflict",
            Self::Unknown => "sheet-feasibility-unknown",
        }
    }
}

fn footprint(project: &Project, allocation: &Allocation) -> Option<[i64; 4]> {
    let board = project
        .boards
        .iter()
        .find(|b| b.id == allocation.board_id)?;
    let [length, width] = if allocation.quarter_turn {
        [board.width, board.length]
    } else {
        [board.length, board.width]
    };
    Some([
        allocation.origin[0].micrometres(),
        allocation.origin[1].micrometres(),
        length.micrometres(),
        width.micrometres(),
    ])
}

fn push(issues: &mut HashMap<Uuid, Vec<Issue>>, id: Uuid, issue: Issue) {
    let entries = issues.entry(id).or_default();
    if !entries.contains(&issue) {
        entries.push(issue);
    }
}

/// Recompute when the committed project changes; previews have independent lifetimes.
fn diagnostics(project: &Project) -> HashMap<Uuid, Vec<Issue>> {
    let mut issues = HashMap::new();
    for conflict in allocation_conflicts(project) {
        for reason in conflict.reasons {
            push(
                &mut issues,
                conflict.board_id,
                match reason {
                    ConflictReason::MaterialIdentity => Issue::Material,
                    ConflictReason::EffectiveThickness => Issue::Thickness,
                    ConflictReason::Grain => Issue::Grain,
                    ConflictReason::OutsideStock => Issue::Bounds,
                    ConflictReason::Overlap => Issue::Overlap,
                },
            );
        }
    }
    for stock in &project.stock {
        let allocations: Vec<_> = project
            .allocations
            .iter()
            .filter(|a| a.stock_id == stock.id)
            .collect();
        // An unused sheet has no part to mark. In particular, do not run a
        // bounded guillotine search for every empty inventory item each frame.
        if allocations.is_empty() {
            continue;
        }
        // The geometry validator reports overlap, but a narrow gap also cannot
        // accommodate a full blade. Flag both implicated parts, not just the
        // arbitrary first one returned by witness reconstruction.
        for (index, a) in allocations.iter().enumerate() {
            for b in &allocations[index + 1..] {
                let (Some(a_rect), Some(b_rect)) = (footprint(project, a), footprint(project, b))
                else {
                    continue;
                };
                for axis in 0..2 {
                    let other = 1 - axis;
                    let a0 = i128::from(a_rect[axis]);
                    let b0 = i128::from(b_rect[axis]);
                    let a1 = a0 + i128::from(a_rect[axis + 2]);
                    let b1 = b0 + i128::from(b_rect[axis + 2]);
                    let cross = i128::from(a_rect[other])
                        < i128::from(b_rect[other]) + i128::from(b_rect[other + 2])
                        && i128::from(b_rect[other])
                            < i128::from(a_rect[other]) + i128::from(a_rect[other + 2]);
                    let gap = if a1 <= b0 {
                        b0 - a1
                    } else if b1 <= a0 {
                        a0 - b1
                    } else {
                        -1
                    };
                    if cross && gap >= 0 && gap < i128::from(project.cutting_kerf.micrometres()) {
                        push(&mut issues, a.board_id, Issue::Kerf);
                        push(&mut issues, b.board_id, Issue::Kerf);
                    }
                }
            }
        }
        let result = reconstruct_witness(project, stock.id, project.cutting_kerf, WITNESS_BUDGET);
        let general = match result {
            Reconstruction::Verified { .. } => None,
            Reconstruction::BudgetExhausted => Some(Issue::Unknown),
            Reconstruction::RuleViolation(ReconstructionViolation::Cut(CutError::SubKerfEdge)) => {
                Some(Issue::Kerf)
            }
            Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                WitnessError::MaterialMismatch(id),
            )) => {
                push(&mut issues, id, Issue::Material);
                None
            }
            Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                WitnessError::ThicknessMismatch(id),
            )) => {
                push(&mut issues, id, Issue::Thickness);
                None
            }
            Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                WitnessError::GrainMismatch(id),
            )) => {
                push(&mut issues, id, Issue::Grain);
                None
            }
            Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                WitnessError::PlacementMismatch(id),
            )) => {
                push(&mut issues, id, Issue::Bounds);
                None
            }
            Reconstruction::RuleViolation(_) => Some(Issue::Cutting),
        };
        if let Some(issue) = general {
            for allocation in allocations {
                push(&mut issues, allocation.board_id, issue);
            }
        }
    }
    issues
}

/// One common scale for both axes, with a fixed maximum drawing height.
fn sheet_scale(stock: &Stock, available_width: f32) -> f32 {
    let x = stock.length.micrometres() as f64 / 1000.0;
    let y = stock.width.micrometres() as f64 / 1000.0;
    ((f64::from(available_width.max(1.0)) / x).min(280.0 / y)) as f32
}

fn allocation_rect(origin: egui::Pos2, scale: f32, rect: [i64; 4]) -> egui::Rect {
    let position = egui::pos2(
        origin.x + (rect[0] as f64 / 1000.0 * f64::from(scale)) as f32,
        origin.y + (rect[1] as f64 / 1000.0 * f64::from(scale)) as f32,
    );
    egui::Rect::from_min_size(
        position,
        egui::vec2(
            (rect[2] as f64 / 1000.0 * f64::from(scale)) as f32,
            (rect[3] as f64 / 1000.0 * f64::from(scale)) as f32,
        ),
    )
}

fn drag_origin(origin: [Length; 2], delta: egui::Vec2, scale: f32) -> [Length; 2] {
    let move_um = |current: Length, pixels: f32| {
        let offset = (f64::from(pixels) * 1000.0 / f64::from(scale)).round() as i64;
        Length::from_micrometres(current.micrometres().saturating_add(offset))
    };
    [move_um(origin[0], delta.x), move_um(origin[1], delta.y)]
}

fn drag_status(project: &Project, board: Uuid, stock: Uuid, origin: [Length; 2]) -> DragStatus {
    let mut candidate = project.clone();
    let Some(allocation) = candidate
        .allocations
        .iter_mut()
        .find(|a| a.board_id == board)
    else {
        return DragStatus::Violation(Issue::Cutting);
    };
    allocation.origin = origin;
    // Check geometry first: a witness's generic NoSlicing result may conceal
    // the more useful overlap/bounds reason for the moving board.
    if let Some(conflict) = allocation_conflicts(&candidate)
        .into_iter()
        .find(|conflict| conflict.board_id == board)
        && let Some(reason) = conflict.reasons.first()
    {
        return DragStatus::Violation(match reason {
            ConflictReason::MaterialIdentity => Issue::Material,
            ConflictReason::EffectiveThickness => Issue::Thickness,
            ConflictReason::Grain => Issue::Grain,
            ConflictReason::OutsideStock => Issue::Bounds,
            ConflictReason::Overlap => Issue::Overlap,
        });
    }
    match reconstruct_witness(&candidate, stock, candidate.cutting_kerf, WITNESS_BUDGET) {
        Reconstruction::Verified { .. } => DragStatus::Verified,
        Reconstruction::BudgetExhausted => DragStatus::Exhausted,
        Reconstruction::RuleViolation(ReconstructionViolation::Cut(CutError::SubKerfEdge)) => {
            DragStatus::Violation(Issue::Kerf)
        }
        Reconstruction::RuleViolation(ReconstructionViolation::Witness(
            WitnessError::MaterialMismatch(_),
        )) => DragStatus::Violation(Issue::Material),
        Reconstruction::RuleViolation(ReconstructionViolation::Witness(
            WitnessError::ThicknessMismatch(_),
        )) => DragStatus::Violation(Issue::Thickness),
        Reconstruction::RuleViolation(ReconstructionViolation::Witness(
            WitnessError::GrainMismatch(_),
        )) => DragStatus::Violation(Issue::Grain),
        Reconstruction::RuleViolation(ReconstructionViolation::Witness(
            WitnessError::PlacementMismatch(_),
        )) => DragStatus::Violation(Issue::Bounds),
        Reconstruction::RuleViolation(_) => DragStatus::Violation(Issue::Cutting),
    }
}

impl SheetDrag {
    fn update(&mut self, project: &Project, pointer: egui::Pos2, scale: f32) -> [Length; 2] {
        let origin = drag_origin(self.origin, pointer - self.start, scale);
        if self
            .candidate
            .as_ref()
            .is_none_or(|(previous, _)| *previous != origin)
        {
            self.candidate = Some((origin, drag_status(project, self.board, self.stock, origin)));
        }
        origin
    }
}

fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_owned()
}

fn board_label(board: &Board) -> String {
    format!("{} ({})", board.name, short_id(board.id))
}

fn issue_text(localizer: &Localizer, issues: &[Issue]) -> String {
    issues
        .iter()
        .map(|issue| localizer.text(issue.key()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn choose_sheet_board(selection: &mut Selection, id: Uuid, additive: bool) {
    selection.choose(Some(id), additive);
}

fn paint_allocation(
    painter: &egui::Painter,
    region: egui::Rect,
    label: &str,
    selected: bool,
    in_selection: bool,
    conflict: bool,
) {
    let fill = if conflict {
        egui::Color32::from_rgb(235, 134, 126)
    } else if selected {
        egui::Color32::from_rgb(255, 199, 82)
    } else if in_selection {
        egui::Color32::from_rgb(109, 205, 235)
    } else {
        egui::Color32::from_rgb(160, 192, 148)
    };
    painter.rect_filled(region, 0.0, fill);
    painter.rect_stroke(
        region,
        0.0,
        egui::Stroke::new(
            if selected { 3.0 } else { 1.5 },
            if selected {
                egui::Color32::YELLOW
            } else if conflict {
                egui::Color32::RED
            } else {
                egui::Color32::BLACK
            },
        ),
        egui::StrokeKind::Inside,
    );
    if conflict {
        // Crossed diagonals remain visible on small parts and under active selection.
        painter.line_segment(
            [region.left_top(), region.right_bottom()],
            egui::Stroke::new(2.0, egui::Color32::DARK_RED),
        );
        painter.line_segment(
            [region.right_top(), region.left_bottom()],
            egui::Stroke::new(2.0, egui::Color32::DARK_RED),
        );
    }
    if region.width() > 48.0 && region.height() > 24.0 {
        painter.text(
            region.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            egui::Color32::BLACK,
        );
    }
}

pub fn show(
    ui: &mut egui::Ui,
    editor: &mut ProjectEditor,
    selection: &mut Selection,
    localizer: &Localizer,
    modal: bool,
    repair: &mut RepairUi,
) {
    let project = editor.preview().unwrap_or(editor.project()).clone();
    let project = &project;
    ui.heading(localizer.text("sheet-heading"));
    let mut action = None;
    let mut accept = false;
    let mut cancel = false;
    if !repair.active() {
        ui.small(localizer.text("sheet-read-only"));
        if ui
            .add_enabled(!modal, egui::Button::new(localizer.text("sheet-edit")))
            .clicked()
        {
            repair.begin(editor, selection, locale(localizer));
        }
    } else {
        ui.small(localizer.text("sheet-repair-hint"));
        ui.horizontal(|ui| {
            accept = ui
                .add_enabled(!modal, egui::Button::new(localizer.text("sheet-accept")))
                .clicked();
            cancel = ui
                .add_enabled(!modal, egui::Button::new(localizer.text("sheet-cancel")))
                .clicked();
        });
        if !modal
            && ui.input(|i| i.key_pressed(egui::Key::Escape))
            && !egui::Popup::is_any_open(ui.ctx())
        {
            cancel = true;
            repair.drag = None;
        }
        if modal {
            repair.drag = None;
        }
        repair.select(project, selection.active, locale(localizer));
        if let Some(id) = repair.board {
            let allocated = project.allocations.iter().find(|a| a.board_id == id);
            ui.label(format!(
                "{}: {}",
                localizer.text("sheet-selected"),
                project
                    .boards
                    .iter()
                    .find(|b| b.id == id)
                    .map(board_label)
                    .unwrap_or_default()
            ));
            ui.horizontal(|ui| {
                ui.label(localizer.text("sheet-target"));
                egui::ComboBox::from_id_salt("sheet-target-stock")
                    .selected_text(
                        project
                            .stock
                            .iter()
                            .find(|s| Some(s.id) == repair.stock)
                            .map(|s| format!("{} ({})", s.name, short_id(s.id)))
                            .unwrap_or_default(),
                    )
                    .show_ui(ui, |ui| {
                        for stock in project.ordered_stock() {
                            ui.selectable_value(
                                &mut repair.stock,
                                Some(stock.id),
                                format!("{} ({})", stock.name, short_id(stock.id)),
                            );
                        }
                    });
            });
            let unit = if matches!(project.display_unit, Unit::Foot | Unit::Inch) {
                Unit::Inch
            } else {
                Unit::Mm
            };
            let mut valid = true;
            for axis in 0..2 {
                ui.horizontal(|ui| {
                    ui.label(localizer.text(if axis == 0 {
                        "sheet-origin-x"
                    } else {
                        "sheet-origin-y"
                    }));
                    if ui.text_edit_singleline(&mut repair.origin[axis]).changed() {
                        repair.consent[axis] = false;
                    }
                });
                match parse_length(&repair.origin[axis], unit) {
                    Ok(parsed) => {
                        if let Conversion::NeedsConfirmation(value) = parsed.conversion {
                            let mut args = fluent_bundle::FluentArgs::new();
                            args.set("entered", repair.origin[axis].as_str());
                            args.set(
                                "rounded",
                                format_length(value, Unit::Mm, locale(localizer), 3),
                            );
                            ui.checkbox(
                                &mut repair.consent[axis],
                                localizer.format("rounding-confirmation", Some(&args)),
                            );
                        }
                    }
                    Err(_) => {
                        ui.colored_label(
                            egui::Color32::DARK_RED,
                            localizer.text("sheet-coordinate-error"),
                        );
                    }
                }
                valid &= coordinate(&repair.origin[axis], unit, repair.consent[axis]).is_some();
            }
            ui.checkbox(
                &mut repair.quarter_turn,
                localizer.text("sheet-quarter-turn"),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !modal
                            && valid
                            && repair.stock.is_some()
                            && !allocated.is_some_and(|a| a.locked),
                        egui::Button::new(localizer.text("sheet-stage")),
                    )
                    .clicked()
                {
                    action = Some(RepairAction::Place(
                        id,
                        repair.stock.unwrap(),
                        [
                            coordinate(&repair.origin[0], unit, repair.consent[0]).unwrap(),
                            coordinate(&repair.origin[1], unit, repair.consent[1]).unwrap(),
                        ],
                        repair.quarter_turn,
                    ));
                }
                if let Some(a) = allocated {
                    if ui
                        .add_enabled(
                            !modal,
                            egui::Button::new(localizer.text("sheet-unallocate-action")),
                        )
                        .clicked()
                    {
                        action = Some(RepairAction::Unallocate(id));
                    }
                    if ui
                        .add_enabled(
                            !modal,
                            egui::Button::new(localizer.text(if a.locked {
                                "sheet-unlock"
                            } else {
                                "sheet-lock"
                            })),
                        )
                        .clicked()
                    {
                        action = Some(RepairAction::Lock(id, !a.locked));
                    }
                }
            });
        }
        if let Some(key) = repair.error {
            ui.colored_label(egui::Color32::DARK_RED, localizer.text(key));
        }
        if let Some(affected) = &repair.affected {
            let mut stocks: Vec<_> = affected.iter().copied().collect();
            stocks.sort_unstable();
            let key = diagnostics_key(project, editor.preview().is_some());
            if repair
                .affected_cache
                .as_ref()
                .is_none_or(|(cached, ids, _)| *cached != key || *ids != stocks)
            {
                let statuses = stocks
                    .iter()
                    .copied()
                    .map(|stock_id| {
                        let status = reconstruct_witness(
                            project,
                            stock_id,
                            project.cutting_kerf,
                            WITNESS_BUDGET,
                        );
                        let message = match status {
                            Reconstruction::Verified { .. } => "sheet-verified",
                            Reconstruction::BudgetExhausted => "sheet-feasibility-unknown",
                            Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                                WitnessError::MaterialMismatch(_),
                            )) => "conflict-material-identity",
                            Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                                WitnessError::ThicknessMismatch(_),
                            )) => "conflict-thickness",
                            Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                                WitnessError::GrainMismatch(_),
                            )) => "conflict-grain",
                            Reconstruction::RuleViolation(ReconstructionViolation::Witness(
                                WitnessError::PlacementMismatch(_),
                            )) => "conflict-outside-stock",
                            Reconstruction::RuleViolation(ReconstructionViolation::Cut(
                                CutError::SubKerfEdge,
                            )) => "sheet-kerf-conflict",
                            Reconstruction::RuleViolation(_) => "sheet-cut-conflict",
                        };
                        (stock_id, message)
                    })
                    .collect();
                repair.affected_cache = Some((key, stocks, statuses));
            }
            for &(stock_id, message) in &repair.affected_cache.as_ref().unwrap().2 {
                ui.colored_label(
                    if message == "sheet-verified" {
                        egui::Color32::DARK_GREEN
                    } else {
                        egui::Color32::DARK_RED
                    },
                    format!(
                        "{}: {}",
                        project
                            .stock
                            .iter()
                            .find(|s| s.id == stock_id)
                            .map(|s| s.name.as_str())
                            .unwrap_or("?"),
                        localizer.text(message)
                    ),
                );
            }
        }
    }
    let key = diagnostics_key(project, editor.preview().is_some());
    if repair
        .diagnostics_cache
        .as_ref()
        .is_none_or(|(cached, _)| *cached != key)
    {
        repair.diagnostics_cache = Some((key, diagnostics(project)));
    }
    let issues = repair.diagnostics_cache.as_ref().unwrap().1.clone();
    let mut allocated = HashSet::new();
    egui::ScrollArea::vertical()
        .id_salt("sheet-workspace-scroll")
        .show(ui, |ui| {
            for stock in project.ordered_stock() {
                ui.separator();
                ui.label(format!("{} ({})", stock.name, short_id(stock.id)));
                let scale = sheet_scale(stock, (ui.available_width() - 48.0).max(1.0));
                let sheet_size = egui::vec2(
                    (stock.length.micrometres() as f64 / 1000.0 * f64::from(scale)) as f32,
                    (stock.width.micrometres() as f64 / 1000.0 * f64::from(scale)) as f32,
                );
                // Keep partially outside draft placements visible at the edge.
                let gutter = egui::vec2(24.0, 24.0);
                let (canvas, response) = ui.allocate_exact_size(
                    sheet_size.max(egui::vec2(1.0, 1.0)) + gutter * 2.0,
                    egui::Sense::click_and_drag(),
                );
                let painter = ui.painter().with_clip_rect(canvas);
                let sheet = egui::Rect::from_min_size(canvas.min + gutter, sheet_size);
                painter.rect_filled(sheet, 0.0, egui::Color32::from_rgb(233, 221, 195));
                painter.rect_stroke(
                    sheet,
                    0.0,
                    egui::Stroke::new(2.0, egui::Color32::DARK_GRAY),
                    egui::StrokeKind::Inside,
                );
                let allocations: Vec<_> = project
                    .allocations
                    .iter()
                    .filter(|a| a.stock_id == stock.id)
                    .collect();
                let hit_regions: Vec<_> = allocations
                    .iter()
                    .filter_map(|a| {
                        footprint(project, a)
                            .map(|rect| (allocation_rect(sheet.min, scale, rect), a.board_id))
                    })
                    .collect();
                if repair.active() && !modal && !cancel && !ui.ctx().egui_wants_keyboard_input() {
                    if response.drag_started()
                        && let Some(pointer) = response.interact_pointer_pos()
                        && let Some(id) = hit_board(
                            &hit_regions,
                            canvas,
                            ui.input(|i| i.pointer.press_origin()).unwrap_or(pointer),
                        )
                        && let Some(a) = project.allocations.iter().find(|a| a.board_id == id)
                    {
                        choose_sheet_board(selection, id, false);
                        repair.select(project, Some(id), locale(localizer));
                        if !a.locked {
                            repair.drag = Some(SheetDrag {
                                board: id,
                                stock: stock.id,
                                origin: a.origin,
                                turn: a.quarter_turn,
                                start: ui.input(|i| i.pointer.press_origin()).unwrap_or(pointer),
                                candidate: None,
                            });
                        }
                    }
                    if let Some(drag) = repair.drag.as_mut()
                        && drag.stock == stock.id
                        && let Some(pointer) = response.interact_pointer_pos()
                    {
                        drag.update(project, pointer, scale);
                    }
                    if response.drag_stopped()
                        && let Some(drag) = repair.drag.take()
                    {
                        if drag.stock == stock.id {
                            let pointer = response.interact_pointer_pos().unwrap_or(drag.start);
                            action = Some(RepairAction::Place(
                                drag.board,
                                drag.stock,
                                drag_origin(drag.origin, pointer - drag.start, scale),
                                drag.turn,
                            ));
                        } else {
                            repair.drag = Some(drag);
                        }
                    }
                }
                for allocation in allocations {
                    allocated.insert(allocation.board_id);
                    let (Some(board), Some(rect)) = (
                        project.boards.iter().find(|b| b.id == allocation.board_id),
                        footprint(project, allocation),
                    ) else {
                        continue;
                    };
                    let region = allocation_rect(sheet.min, scale, rect);
                    let selected = selection.active == Some(board.id);
                    let conflict = issues.get(&board.id).is_some_and(|v| !v.is_empty());
                    paint_allocation(
                        &painter,
                        region,
                        &board_label(board),
                        selected,
                        selection.ids.contains(&board.id),
                        conflict,
                    );
                }
                if let Some(drag) = repair.drag.as_ref()
                    && drag.stock == stock.id
                    && let Some((origin, status)) = drag.candidate
                    && let Some(allocation) = project
                        .allocations
                        .iter()
                        .find(|a| a.board_id == drag.board)
                    && let Some(rect) = footprint(project, allocation)
                {
                    let ghost = allocation_rect(
                        sheet.min,
                        scale,
                        [
                            origin[0].micrometres(),
                            origin[1].micrometres(),
                            rect[2],
                            rect[3],
                        ],
                    );
                    painter.rect_stroke(
                        ghost,
                        0.0,
                        egui::Stroke::new(3.0, status.color()),
                        egui::StrokeKind::Outside,
                    );
                    painter.text(
                        canvas.min + egui::vec2(gutter.x, 2.0),
                        egui::Align2::LEFT_TOP,
                        localizer.text(status.key()),
                        egui::FontId::proportional(13.0),
                        status.color(),
                    );
                }
                // Stock grain is in sheet coordinates, independent of assembly rotation.
                let arrow = match stock.grain {
                    StockGrain::AlongX => Some(egui::vec2(34.0, 0.0)),
                    StockGrain::AlongY => Some(egui::vec2(0.0, 34.0)),
                    _ => None,
                };
                if let Some(direction) = arrow {
                    let start = sheet.min + egui::vec2(10.0, 10.0);
                    painter.arrow(
                        start,
                        direction,
                        egui::Stroke::new(3.0, egui::Color32::from_rgb(35, 49, 110)),
                    );
                }
                ui.small(format!(
                    "{}: {}",
                    localizer.text("stock-grain"),
                    localizer.text(match stock.grain {
                        StockGrain::AlongX => "stock-grain-x",
                        StockGrain::AlongY => "stock-grain-y",
                        StockGrain::Nondirectional => "stock-grain-none",
                        StockGrain::Unknown => "stock-grain-unknown",
                    })
                ));
                if !modal
                    && !ui.ctx().egui_wants_keyboard_input()
                    && response.clicked()
                    && let Some(pointer) = response.interact_pointer_pos()
                    && let Some(id) = hit_board(&hit_regions, canvas, pointer)
                {
                    choose_sheet_board(
                        selection,
                        id,
                        ui.input(|i| i.modifiers.command || i.modifiers.shift),
                    );
                }
                for (_, id) in hit_regions {
                    if let Some(board) = project.boards.iter().find(|b| b.id == id) {
                        let warning = issues.get(&id).map(|v| issue_text(localizer, v));
                        ui.horizontal(|ui| {
                            let label = board_label(board);
                            if ui
                                .add_enabled(
                                    !modal,
                                    egui::Button::new(label).selected(selection.active == Some(id)),
                                )
                                .clicked()
                            {
                                choose_sheet_board(
                                    selection,
                                    id,
                                    ui.input(|i| i.modifiers.command || i.modifiers.shift),
                                );
                            }
                            if let Some(warning) = warning {
                                ui.colored_label(egui::Color32::DARK_RED, format!("⚠ {warning}"));
                            }
                        });
                    }
                }
            }
            ui.separator();
            ui.label(localizer.text("sheet-unallocated"));
            for board in &project.boards {
                if !allocated.contains(&board.id)
                    && ui
                        .add_enabled(
                            !modal,
                            egui::Button::new(board_label(board))
                                .selected(selection.active == Some(board.id)),
                        )
                        .clicked()
                {
                    choose_sheet_board(
                        selection,
                        board.id,
                        ui.input(|i| i.modifiers.command || i.modifiers.shift),
                    );
                }
            }
        });
    if let Some(action) = action {
        let moved = match &action {
            RepairAction::Place(id, ..) | RepairAction::Unallocate(id) => Some(*id),
            _ => None,
        };
        stage(repair, editor, action);
        if let Some(id) = moved {
            repair.board = None;
            repair.select(
                editor.preview().unwrap_or(editor.project()),
                Some(id),
                locale(localizer),
            );
        }
    }
    if accept
        && let Some(affected) = repair.affected.take()
        && let Some(mut session) = SheetEditSession::resume(editor, affected)
    {
        match session.accept() {
            Ok(_) => {
                repair.error = None;
                repair.drag = None;
            }
            Err(_) => {
                repair.error = Some("sheet-accept-error");
                repair.affected = Some(session.pause());
            }
        }
    }
    if cancel {
        editor.cancel_preview();
        *repair = RepairUi::default();
    }
}

fn hit_board(
    regions: &[(egui::Rect, Uuid)],
    canvas: egui::Rect,
    pointer: egui::Pos2,
) -> Option<Uuid> {
    regions
        .iter()
        .rev()
        .find(|(rect, _)| rect.intersect(canvas).contains(pointer))
        .map(|(_, id)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::commands::ProjectEditor;
    use plan_my_cabinet::domain::{BoardGrain, Material, StockSource};
    use plan_my_cabinet::money::Currency;
    use plan_my_cabinet::units::{Length, Pose, Quaternion};

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }

    fn fixture() -> Project {
        let mut p = Project::new("sheets", Currency::Brl);
        let material = Uuid::new_v4();
        let stock = Uuid::new_v4();
        p.materials.push(Material {
            id: material,
            name: "ply".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Length,
        });
        p.stock.push(Stock {
            id: stock,
            name: "offcut".into(),
            material_id: material,
            length: mm(205),
            width: mm(50),
            thickness: mm(18),
            grain: StockGrain::AlongX,
            source: StockSource::Owned,
            price: None,
            priority: 0,
            trim: [Length::ZERO; 4],
        });
        for x in [0, 105] {
            let id = Uuid::new_v4();
            p.boards.push(Board {
                id,
                name: "same name".into(),
                material_id: material,
                length: mm(100),
                width: mm(50),
                thickness: mm(18),
                grain_override: None,
                parent_id: None,
                pose: Pose::new([x as f64, 10.0, 0.0], Quaternion::IDENTITY).unwrap(),
            });
            p.allocations.push(Allocation {
                id: Uuid::new_v4(),
                board_id: id,
                stock_id: stock,
                origin: [mm(x), Length::ZERO],
                quarter_turn: false,
                locked: false,
            });
        }
        p
    }

    #[test]
    fn global_diagnostics_find_hidden_board_and_update_after_repair() {
        use plan_my_cabinet::allocation_diagnostics::{Status, diagnose};
        let mut project = fixture();
        let hidden = project.boards[1].id;
        let parent = Uuid::new_v4();
        project.assemblies.push(plan_my_cabinet::domain::Assembly {
            id: parent,
            name: "hidden group".into(),
            parent_id: None,
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        });
        project.boards[1].parent_id = Some(parent);
        project.allocations.pop();
        let mut selection = Selection::default();
        selection.hidden.insert(parent);
        assert!(!selection.visible(&project, hidden));
        let issues = diagnose(&project);
        assert_eq!(issues.len(), project.boards.len());
        assert_eq!(issues.iter().filter(|d| d.board_id == hidden).count(), 1);
        assert_eq!(
            issues.iter().find(|d| d.board_id == hidden).unwrap().status,
            Status::Unallocated
        );
        let revision = project.revision;
        selection.choose(Some(hidden), false);
        selection.reveal(&project, hidden);
        assert!(selection.visible(&project, hidden));
        assert_eq!(project.revision, revision);

        let stock = project.stock[0].id;
        let mut editor = ProjectEditor::new(project).unwrap();
        let mut repair = RepairUi::default();
        assert!(repair.begin(&mut editor, &selection, Locale::En));
        stage(
            &mut repair,
            &mut editor,
            RepairAction::Place(hidden, stock, [mm(105), Length::ZERO], false),
        );
        assert_eq!(
            diagnose(editor.preview().unwrap())
                .iter()
                .find(|d| d.board_id == hidden)
                .unwrap()
                .status,
            Status::AllocatedValid
        );
        let affected = repair.affected.take().unwrap();
        SheetEditSession::resume(&mut editor, affected)
            .unwrap()
            .accept()
            .unwrap();
        assert!(
            diagnose(editor.project())
                .iter()
                .all(|d| d.status == Status::AllocatedValid)
        );
    }

    #[test]
    fn global_diagnostics_have_one_issue_row_despite_multiple_reasons_and_track_edits() {
        use plan_my_cabinet::allocation_diagnostics::{Reason, Status, diagnose};
        let mut project = fixture();
        let id = project.boards[0].id;
        project.allocations[0].origin[0] = mm(200);
        project.stock[0].thickness = mm(12);
        let issues = diagnose(&project);
        assert_eq!(issues.len(), project.boards.len());
        assert_eq!(issues.iter().filter(|d| d.board_id == id).count(), 1);
        let issue = issues.iter().find(|d| d.board_id == id).unwrap();
        assert_eq!(issue.status, Status::Conflicted);
        assert!(issue.reasons.contains(&Reason::Bounds));
        assert!(issue.reasons.contains(&Reason::Thickness));
        project.allocations[0].origin[0] = mm(0);
        project.stock[0].thickness = mm(18);
        assert!(
            diagnose(&project)
                .iter()
                .all(|d| d.status == Status::AllocatedValid)
        );
    }

    #[test]
    fn global_diagnostics_report_missing_and_duplicate_records_once() {
        use plan_my_cabinet::allocation_diagnostics::{Reason, Status, diagnose};
        let mut project = fixture();
        let first = project.boards[0].id;
        let second = project.boards[1].id;
        project.allocations.pop();
        let mut extra = project.allocations[0].clone();
        extra.id = Uuid::new_v4();
        extra.stock_id = Uuid::new_v4();
        project.allocations.push(extra);
        let issues = diagnose(&project);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].board_id, first);
        assert_eq!(issues[0].status, Status::Conflicted);
        assert!(issues[0].reasons.contains(&Reason::DuplicateAllocation));
        assert!(issues[0].reasons.contains(&Reason::MissingStock));
        assert_eq!(issues[1].board_id, second);
        assert_eq!(issues[1].status, Status::Unallocated);
    }

    #[test]
    fn staged_transfer_unlock_and_cancel_leave_identity_and_pose_untouched() {
        let mut p = fixture();
        let board = p.boards[0].id;
        let stock = Uuid::new_v4();
        let mut spare = p.stock[0].clone();
        spare.id = stock;
        p.stock.push(spare);
        let before = p.clone();
        let mut editor = ProjectEditor::new(p).unwrap();
        editor.begin_preview();
        let mut repair = RepairUi {
            affected: Some(HashSet::new()),
            ..Default::default()
        };
        stage(&mut repair, &mut editor, RepairAction::Lock(board, true));
        stage(
            &mut repair,
            &mut editor,
            RepairAction::Place(board, stock, [Length::ZERO; 2], false),
        );
        assert_eq!(repair.error, Some("sheet-locked-error"));
        stage(&mut repair, &mut editor, RepairAction::Lock(board, false));
        stage(
            &mut repair,
            &mut editor,
            RepairAction::Place(board, stock, [Length::ZERO; 2], false),
        );
        assert_eq!(editor.preview().unwrap().allocations[0].stock_id, stock);
        assert_eq!(editor.preview().unwrap().boards, before.boards);
        editor.cancel_preview();
        assert_eq!(editor.project(), &before);
    }

    #[test]
    fn drag_stages_only_on_release_and_coordinates_require_rounding_consent() {
        let ctx = egui::Context::default();
        let p = fixture();
        let id = p.boards[0].id;
        let original = p.allocations[0].origin;
        let mut editor = ProjectEditor::new(p).unwrap();
        editor.begin_preview();
        let mut repair = RepairUi {
            affected: Some(HashSet::new()),
            ..Default::default()
        };
        let mut started = false;
        let mut stopped = false;
        let mut released_delta = egui::Vec2::ZERO;
        let frame = |events: Vec<egui::Event>,
                     started: &mut bool,
                     stopped: &mut bool,
                     released_delta: &mut egui::Vec2| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(200.0, 100.0),
                        egui::Sense::click_and_drag(),
                    );
                    ui.painter().rect_filled(rect, 0.0, egui::Color32::RED);
                    *started |= response.drag_started();
                    *stopped |= response.drag_stopped();
                    if response.drag_stopped() {
                        *released_delta =
                            response.interact_pointer_pos().unwrap() - egui::pos2(40.0, 40.0);
                    }
                },
            );
            output.drop_without_applying_deltas();
        };
        let pointer = egui::pos2(40.0, 40.0);
        frame(vec![], &mut started, &mut stopped, &mut released_delta);
        frame(
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            &mut started,
            &mut stopped,
            &mut released_delta,
        );
        frame(
            vec![egui::Event::PointerMoved(pointer + egui::vec2(30.0, 0.0))],
            &mut started,
            &mut stopped,
            &mut released_delta,
        );
        assert!(started);
        assert!(!stopped);
        assert_eq!(editor.preview().unwrap().allocations[0].origin, original);
        frame(
            vec![egui::Event::PointerButton {
                pos: pointer + egui::vec2(30.0, 0.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut started,
            &mut stopped,
            &mut released_delta,
        );
        assert!(stopped);
        assert_eq!(released_delta.x, 30.0);
        let stock_id = editor_stock(&editor, id);
        stage(
            &mut repair,
            &mut editor,
            RepairAction::Place(
                id,
                stock_id,
                drag_origin(original, released_delta, 2.0),
                false,
            ),
        );
        assert_eq!(editor.preview().unwrap().allocations[0].origin[0], mm(15));
        assert!(coordinate("1/64 in", Unit::Mm, false).is_none());
        assert!(coordinate("1/64 in", Unit::Mm, true).is_some());
    }

    #[test]
    fn sheet_drag_shows_live_conflict_and_valid_witness_without_staging_until_release() {
        let ctx = egui::Context::default();
        let project = fixture();
        let id = project.boards[0].id;
        let original = project.clone();
        let mut editor = ProjectEditor::new(project).unwrap();
        editor.begin_preview();
        let mut repair = RepairUi {
            affected: Some(HashSet::new()),
            ..Default::default()
        };
        let mut selection = Selection::default();
        selection.choose(Some(id), false);
        let localizer = Localizer::new(Language::En);
        macro_rules! frame {
            ($events:expr) => {{
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000.0, 1000.0),
                        )),
                        events: $events,
                        ..Default::default()
                    },
                    |ui| {
                        show(
                            ui,
                            &mut editor,
                            &mut selection,
                            &localizer,
                            false,
                            &mut repair,
                        )
                    },
                );
                let sheet = output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.fill == egui::Color32::from_rgb(233, 221, 195) =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                });
                let ghost_colors: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) if rect.stroke.width == 3.0 => {
                            Some(rect.stroke.color)
                        }
                        _ => None,
                    })
                    .collect();
                output.drop_without_applying_deltas();
                (sheet.unwrap(), ghost_colors)
            }};
        }
        let sheet = frame!(vec![]).0;
        let start = sheet.min + egui::vec2(20.0, 20.0);
        frame!(vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        // At 100 mm the two allocations overlap; at 0 mm the layout is valid.
        let scale = sheet_scale(&editor.project().stock[0], sheet.width());
        let invalid = start + egui::vec2(100.0 * scale, 0.0);
        let (_, colors) = frame!(vec![egui::Event::PointerMoved(invalid)]);
        assert!(colors.contains(&egui::Color32::RED));
        assert_eq!(
            repair.drag.as_ref().unwrap().candidate.unwrap().1,
            DragStatus::Violation(Issue::Overlap)
        );
        assert_eq!(editor.preview().unwrap(), &original);
        assert_eq!(editor.project(), &original);
        let (_, colors) = frame!(vec![egui::Event::PointerMoved(start)]);
        assert!(colors.contains(&egui::Color32::GREEN));
        assert_eq!(
            repair.drag.as_ref().unwrap().candidate.unwrap().1,
            DragStatus::Verified
        );
        assert_eq!(editor.preview().unwrap(), &original);
        frame!(vec![egui::Event::PointerMoved(invalid)]);
        frame!(vec![egui::Event::PointerButton {
            pos: invalid,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(repair.drag.is_none());
        assert_eq!(editor.preview().unwrap().allocations[0].origin[0], mm(100));
        assert_eq!(editor.project(), &original);
        frame!(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(!repair.active());
        assert!(editor.preview().is_none());
        assert_eq!(editor.project(), &original);
    }

    fn editor_stock(editor: &ProjectEditor, id: Uuid) -> Uuid {
        editor
            .preview()
            .unwrap()
            .allocations
            .iter()
            .find(|a| a.board_id == id)
            .unwrap()
            .stock_id
    }

    #[test]
    fn sheet_click_uses_stable_board_identity_in_shared_selection_without_mutating_pose() {
        let mut project = fixture();
        let before = project.clone();
        let scale = sheet_scale(&project.stock[0], 410.0);
        assert_eq!(scale, 2.0);
        let canvas = egui::Rect::from_min_size(egui::pos2(20.0, 30.0), egui::vec2(410.0, 100.0));
        let regions: Vec<_> = project
            .allocations
            .iter()
            .map(|a| {
                (
                    allocation_rect(canvas.min, scale, footprint(&project, a).unwrap()),
                    a.board_id,
                )
            })
            .collect();
        assert_eq!(regions[1].0.min, egui::pos2(230.0, 30.0));
        let id = hit_board(&regions, canvas, egui::pos2(240.0, 45.0)).unwrap();
        let mut selection = Selection::default();
        choose_sheet_board(&mut selection, id, false);
        assert_eq!(selection.active, Some(project.boards[1].id));
        assert!(selection.ids.contains(&id));
        // A sheet-only origin change is independent from the assembly pose.
        project.allocations[1].origin[0] = mm(102);
        assert_eq!(project.boards, before.boards);
        assert_eq!(project.revision, before.revision);
        assert_eq!(
            hit_board(&regions, canvas, egui::pos2(400.0, 45.0)),
            Some(id)
        );
    }

    #[test]
    fn conflicts_are_derived_for_both_overlap_participants_and_current_witness() {
        let mut p = fixture();
        assert!(diagnostics(&p).is_empty());
        p.allocations[1].origin[0] = mm(102);
        let issues = diagnostics(&p);
        for board in &p.boards {
            assert!(issues[&board.id].contains(&Issue::Kerf));
        }
        p.allocations[1].origin[0] = mm(98);
        let issues = diagnostics(&p);
        for board in &p.boards {
            assert!(issues[&board.id].contains(&Issue::Overlap));
        }
        p.allocations[1].origin[0] = mm(105);
        let other_material = Uuid::new_v4();
        p.materials.push(Material {
            id: other_material,
            name: "other".into(),
            default_thickness: mm(12),
            default_grain: BoardGrain::Length,
        });
        p.boards[1].material_id = other_material;
        p.boards[1].thickness = mm(12);
        p.allocations[1].quarter_turn = true;
        let issues = diagnostics(&p);
        assert!(issues[&p.boards[1].id].contains(&Issue::Material));
        assert!(issues[&p.boards[1].id].contains(&Issue::Thickness));
        assert!(issues[&p.boards[1].id].contains(&Issue::Bounds));
        // Restore structural material identity to exercise grain independently.
        p.boards[1].material_id = p.materials[0].id;
        assert!(diagnostics(&p)[&p.boards[1].id].contains(&Issue::Grain));
        p.allocations[1].quarter_turn = false;
        p.boards[1].thickness = mm(18);
        assert!(diagnostics(&p).is_empty());
    }

    #[test]
    fn sub_kerf_edge_and_invalid_trim_are_not_shown_as_valid() {
        let mut p = fixture();
        p.stock[0].length = mm(103);
        p.allocations.truncate(1);
        assert!(diagnostics(&p)[&p.boards[0].id].contains(&Issue::Kerf));
        p.stock[0].length = mm(100);
        p.stock[0].trim[0] = mm(2);
        assert!(diagnostics(&p)[&p.boards[0].id].contains(&Issue::Kerf));
    }

    #[test]
    fn conflicted_selected_allocation_paints_crossed_overlay_and_active_outline() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(Default::default(), |ui| {
            let painter = ui.painter();
            paint_allocation(
                painter,
                egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(80.0, 40.0)),
                "Side (abcdef12)",
                true,
                true,
                true,
            );
        });
        let crosses = output.shapes.iter().filter(|shape| matches!(
            &shape.shape,
            egui::Shape::LineSegment { stroke, .. } if stroke.color == egui::Color32::DARK_RED
        )).count();
        assert_eq!(crosses, 2);
        assert!(output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Rect(rect) if rect.stroke.color == egui::Color32::YELLOW
        )));
        output.drop_without_applying_deltas();
    }

    #[test]
    fn locked_resize_overlay_updates_on_commit_and_disappears_on_undo() {
        use plan_my_cabinet::board_dimensions::BoardDimension;
        use plan_my_cabinet::units::Anchor;

        let mut project = fixture();
        project.allocations[0].locked = true;
        let original = project.allocations.clone();
        let mut editor = ProjectEditor::new(project).unwrap();
        let id = original[0].board_id;
        let ctx = egui::Context::default();
        let localizer = Localizer::new(Language::En);
        let mut selection = Selection::default();
        let mut repair = RepairUi::default();
        let mut frame = |editor: &mut ProjectEditor| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ui| show(ui, editor, &mut selection, &localizer, false, &mut repair),
            );
            let crosses = output.shapes.iter().filter(|shape| matches!(
                &shape.shape,
                egui::Shape::LineSegment { stroke, .. } if stroke.color == egui::Color32::DARK_RED
            )).count();
            output.drop_without_applying_deltas();
            crosses
        };
        assert_eq!(frame(&mut editor), 0);
        let preview = editor
            .preview_board_dimension(id, BoardDimension::Length, mm(108), Anchor::Start)
            .unwrap();
        editor.edit_board_dimension(preview).unwrap();
        assert_eq!(editor.project().allocations, original);
        assert_eq!(frame(&mut editor), 4);
        editor.undo().unwrap();
        assert_eq!(frame(&mut editor), 0);
    }
}
