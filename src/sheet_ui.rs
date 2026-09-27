//! Cut plan workspace. Sheet coordinates are manufacturing micrometres;
//! selection is shared with the 3D viewport, never stored in the project.
use eframe::egui;
use plan_my_cabinet::allocation_diagnostics::{self, BoardDiagnostic, Reason, Status};
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::cut_tree::{
    Axis, CutError, CutKind, CutOperation, CutTree, Edge, Reconstruction, ReconstructionViolation,
    WitnessError, reconstruct_witness,
};
use plan_my_cabinet::dimension_input::{Locale, format_length, parse_length};
use plan_my_cabinet::domain::{Allocation, Board, BoardGrain, Project, Stock, StockGrain};
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::material_changes::{ConflictReason, allocation_conflicts};
use plan_my_cabinet::money::MoneyLocale;
use plan_my_cabinet::sheet_edit::SheetEditSession;
use plan_my_cabinet::stock_read_models::{SheetProof, StockPieceReadModel, StockReadModel};
use plan_my_cabinet::units::{Conversion, Length, Unit};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::actions::{self, ActionId as A, Argument, Request, Target, Unavailable};
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
    stock_model_cache: Option<(DiagnosticsKey, Option<StockReadModel>)>,
    board_diagnostics_cache: Option<(DiagnosticsKey, Vec<BoardDiagnostic>)>,
    focused_sheet: Option<Uuid>,
    hovered_cut: Option<(Uuid, usize)>,
    affected_cache: Option<AffectedCache>,
    affected: Option<HashSet<Uuid>>,
    board: Option<Uuid>,
    stock: Option<Uuid>,
    origin: [String; 2],
    consent: [bool; 2],
    entry_unit: Option<Unit>,
    quarter_turn: bool,
    placement_dirty: bool,
    error: Option<&'static str>,
    drag: Option<SheetDrag>,
    overlays: SheetOverlays,
    zoom: f32,
}

#[derive(Clone, Copy)]
struct SheetOverlays {
    cuts: bool,
    offcuts: bool,
    grain: bool,
    ids: bool,
}

impl Default for SheetOverlays {
    fn default() -> Self {
        Self {
            cuts: true,
            offcuts: true,
            grain: true,
            ids: true,
        }
    }
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

    pub fn can_accept(&self, editor: &mut ProjectEditor) -> bool {
        if self.placement_dirty {
            return false;
        }
        let Some(affected) = &self.affected else {
            return false;
        };
        let Some(session) = SheetEditSession::resume(editor, affected.clone()) else {
            return false;
        };
        let valid = session.diagnostics().iter().all(|diagnostic| {
            matches!(
                diagnostic.status,
                plan_my_cabinet::sheet_edit::SheetStatus::Verified(_)
            )
        });
        session.pause();
        valid
    }

    pub fn accept_navigation(&mut self, editor: &mut ProjectEditor) -> Result<(), ()> {
        if !self.can_accept(editor) {
            return Err(());
        }
        let affected = self.affected.clone().ok_or(())?;
        let mut session = SheetEditSession::resume(editor, affected).ok_or(())?;
        match session.accept() {
            Ok(_) => {
                let focused_sheet = self.focused_sheet;
                let overlays = self.overlays;
                let zoom = self.zoom;
                *self = Self::default();
                self.focused_sheet = focused_sheet;
                self.overlays = overlays;
                self.zoom = zoom;
                Ok(())
            }
            Err(_) => {
                self.error = Some("sheet-accept-error");
                session.pause();
                Err(())
            }
        }
    }

    pub fn cancel_navigation(&mut self, editor: &mut ProjectEditor) {
        editor.cancel_preview();
        let focused_sheet = self.focused_sheet;
        let overlays = self.overlays;
        let zoom = self.zoom;
        *self = Self::default();
        self.focused_sheet = focused_sheet;
        self.overlays = overlays;
        self.zoom = zoom;
    }

    pub fn stage_placement(
        &mut self,
        editor: &mut ProjectEditor,
        board: Uuid,
        stock: Uuid,
        origin: [Length; 2],
        turn: bool,
    ) -> bool {
        let succeeded = stage(
            self,
            editor,
            RepairAction::Place(board, stock, origin, turn),
        );
        if succeeded {
            self.placement_dirty = false;
            self.entry_unit = None;
        }
        succeeded
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
        // A routed selection may change before the host resolves navigation.
        // Keep the numeric draft attached to its original board until it is
        // staged or the repair is explicitly cancelled.
        if self.placement_dirty && self.board.is_some() {
            return;
        }
        self.board = id;
        self.placement_dirty = false;
        self.entry_unit = None;
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

fn numeric_unit(project: &Project) -> Unit {
    if matches!(project.display_unit, Unit::Foot | Unit::Inch) {
        Unit::Inch
    } else {
        Unit::Mm
    }
}

fn stage(ui_state: &mut RepairUi, editor: &mut ProjectEditor, action: RepairAction) -> bool {
    let Some(affected) = ui_state.affected.take() else {
        return false;
    };
    let Some(mut session) = SheetEditSession::resume(editor, affected) else {
        return false;
    };
    let result = match action {
        RepairAction::Place(id, stock, origin, turn) => session.place(id, stock, origin, turn),
        RepairAction::Unallocate(id) => session.unallocate(id),
        RepairAction::Lock(id, locked) => session.set_lock(id, locked),
    };
    let succeeded = result.is_ok();
    ui_state.error = result.err().map(|err| match err {
        plan_my_cabinet::sheet_edit::SheetEditError::Locked(_) => "sheet-locked-error",
        _ => "sheet-edit-error",
    });
    ui_state.affected = Some(session.pause());
    succeeded
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

/// All drawing conflicts use the same project-wide diagnosis as Needs stock.
#[cfg(test)]
fn diagnostics(project: &Project) -> HashMap<Uuid, Vec<Issue>> {
    diagnostic_issues(&allocation_diagnostics::diagnose(project))
}

fn diagnostic_issues(entries: &[BoardDiagnostic]) -> HashMap<Uuid, Vec<Issue>> {
    entries
        .iter()
        .filter_map(|entry| {
            let mapped: Vec<_> = entry
                .reasons
                .iter()
                .filter_map(|reason| match reason {
                    Reason::Material => Some(Issue::Material),
                    Reason::Thickness => Some(Issue::Thickness),
                    Reason::Grain => Some(Issue::Grain),
                    Reason::Bounds => Some(Issue::Bounds),
                    Reason::Overlap => Some(Issue::Overlap),
                    Reason::Kerf => Some(Issue::Kerf),
                    Reason::Cutting | Reason::DuplicateAllocation | Reason::MissingStock => {
                        Some(Issue::Cutting)
                    }
                    Reason::SearchBudget => Some(Issue::Unknown),
                    Reason::MissingAllocation => None,
                })
                .collect();
            (!mapped.is_empty()).then_some((entry.board_id, mapped))
        })
        .collect()
}

/// One common scale for both axes, bounded by the available canvas height.
fn sheet_scale(stock: &Stock, available_width: f32, available_height: f32) -> f32 {
    let x = stock.length.micrometres() as f64 / 1000.0;
    let y = stock.width.micrometres() as f64 / 1000.0;
    ((f64::from(available_width.max(1.0)) / x).min(f64::from(available_height.max(1.0)) / y)) as f32
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

fn tree_rect(
    origin: egui::Pos2,
    scale: f32,
    rect: plan_my_cabinet::cut_tree::Rectangle,
) -> egui::Rect {
    allocation_rect(
        origin,
        scale,
        [
            rect.origin[0].micrometres(),
            rect.origin[1].micrometres(),
            rect.extent[0].micrometres(),
            rect.extent[1].micrometres(),
        ],
    )
}

/// The mathematical band is never enlarged: only its visible stroke has a
/// minimum, so a narrow kerf remains discoverable at low zoom.
fn kerf_geometry(
    tree: &CutTree,
    operation: &plan_my_cabinet::cut_tree::CutOperation,
    origin: egui::Pos2,
    scale: f32,
) -> egui::Rect {
    let input = tree_rect(origin, scale, tree.node(operation.input).unwrap().rectangle);
    let first = tree_rect(
        origin,
        scale,
        tree.node(operation.outputs.first).unwrap().rectangle,
    );
    let width = (tree.kerf().micrometres() as f64 / 1000.0 * f64::from(scale)) as f32;
    match operation.axis {
        Axis::X => egui::Rect::from_min_max(
            egui::pos2(first.right(), input.top()),
            egui::pos2(first.right() + width, input.bottom()),
        ),
        Axis::Y => egui::Rect::from_min_max(
            egui::pos2(input.left(), first.bottom()),
            egui::pos2(input.right(), first.bottom() + width),
        ),
    }
}

fn paint_witness(
    painter: &egui::Painter,
    sheet: egui::Rect,
    tree: &CutTree,
    scale: f32,
    overlays: SheetOverlays,
    hovered_cut: Option<usize>,
) {
    let painter = painter.with_clip_rect(sheet);
    if overlays.offcuts {
        for node in tree.nodes() {
            if node.kind != CutKind::Offcut {
                continue;
            }
            let rect = tree_rect(sheet.min, scale, node.rectangle).intersect(sheet);
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(232, 239, 226));
            let hatch = painter.with_clip_rect(rect);
            let mut x = rect.left() - rect.height();
            while x < rect.right() {
                hatch.line_segment(
                    [
                        egui::pos2(x, rect.bottom()),
                        egui::pos2(x + rect.height(), rect.top()),
                    ],
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(145, 167, 134)),
                );
                x += 12.0;
            }
            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(115, 140, 108)),
                egui::StrokeKind::Inside,
            );
            let label = format!(
                "{} × {} mm",
                node.rectangle.extent[0].micrometres() as f64 / 1000.0,
                node.rectangle.extent[1].micrometres() as f64 / 1000.0
            );
            if rect.width() > label.len() as f32 * 7.0 + 8.0 && rect.height() > 18.0 {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(10.0),
                    egui::Color32::from_rgb(40, 68, 39),
                );
            }
        }
    }
    if overlays.cuts {
        for operation in tree.operations() {
            let band = kerf_geometry(tree, &operation, sheet.min, scale);
            let highlighted = hovered_cut == Some(operation.number);
            let color = if highlighted {
                egui::Color32::from_rgb(25, 85, 190)
            } else {
                egui::Color32::from_rgb(174, 95, 55)
            };
            painter.rect_filled(
                band,
                0.0,
                color.linear_multiply(if highlighted { 0.9 } else { 0.45 }),
            );
            let center = band.center();
            let stroke = egui::Stroke::new(
                band.width()
                    .min(band.height())
                    .max(if highlighted { 3.0 } else { 1.0 }),
                color,
            );
            let endpoints = match operation.axis {
                Axis::X => [
                    egui::pos2(center.x, band.top()),
                    egui::pos2(center.x, band.bottom()),
                ],
                Axis::Y => [
                    egui::pos2(band.left(), center.y),
                    egui::pos2(band.right(), center.y),
                ],
            };
            // A subpixel band gets a visible stroke, never a fabricated wider physical band.
            if band.width().min(band.height()) < 1.0 {
                painter.line_segment(endpoints, stroke);
            }
            let marker = match operation.axis {
                Axis::X => egui::pos2(center.x, band.top() + 9.0),
                Axis::Y => egui::pos2(band.left() + 9.0, center.y),
            };
            painter.circle_filled(
                marker,
                9.0,
                if highlighted {
                    color
                } else {
                    egui::Color32::from_rgb(109, 58, 31)
                },
            );
            painter.text(
                marker,
                egui::Align2::CENTER_CENTER,
                operation.number.to_string(),
                egui::FontId::proportional(10.0),
                egui::Color32::WHITE,
            );
        }
    }
}

fn ruler_step(scale: f32) -> i64 {
    let mut magnitude = 1_i64;
    loop {
        for factor in [1, 2, 5] {
            let step = magnitude * factor;
            if step as f32 * scale >= 42.0 {
                return step;
            }
        }
        magnitude = magnitude.saturating_mul(10);
        if magnitude > 1_000_000_000_000 {
            return magnitude;
        }
    }
}

fn paint_rulers(painter: &egui::Painter, sheet: egui::Rect, stock: &Stock, scale: f32) {
    let step = ruler_step(scale);
    let color = egui::Color32::from_rgb(91, 82, 72);
    for axis in 0..2 {
        let length = if axis == 0 { stock.length } else { stock.width };
        for mm in (0..=length.micrometres() / 1000).step_by(step as usize) {
            let coordinate = mm as f32 * scale;
            let (start, end, text) = if axis == 0 {
                (
                    sheet.left_top() + egui::vec2(coordinate, -4.0),
                    sheet.left_top() + egui::vec2(coordinate, -11.0),
                    sheet.left_top() + egui::vec2(coordinate + 2.0, -13.0),
                )
            } else {
                (
                    sheet.left_top() + egui::vec2(-4.0, coordinate),
                    sheet.left_top() + egui::vec2(-11.0, coordinate),
                    sheet.left_top() + egui::vec2(-13.0, coordinate + 2.0),
                )
            };
            painter.line_segment([start, end], egui::Stroke::new(1.0, color));
            painter.text(
                text,
                if axis == 0 {
                    egui::Align2::LEFT_BOTTOM
                } else {
                    egui::Align2::RIGHT_TOP
                },
                mm.to_string(),
                egui::FontId::monospace(10.0),
                color,
            );
        }
    }
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

fn stock_label(project: &Project, stock: &Stock) -> String {
    format!(
        "{} · {} ({})",
        project.stock_alias(stock.id).unwrap_or("?"),
        stock.name,
        short_id(stock.id)
    )
}

fn issue_text(localizer: &Localizer, issues: &[Issue]) -> String {
    issues
        .iter()
        .map(|issue| localizer.text(issue.key()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn needs_stock(diagnostics: &[BoardDiagnostic]) -> impl Iterator<Item = &BoardDiagnostic> {
    diagnostics
        .iter()
        .filter(|entry| entry.status != Status::AllocatedValid)
}

// Adding a blank is useful when no existing piece can contain the part with
// compatible material, thickness and grain. Occupied candidates can be repaired.
fn needs_new_stock(project: &Project, diagnostic: &BoardDiagnostic) -> bool {
    if diagnostic.status != Status::Unallocated {
        return false;
    }
    let Some(board) = project
        .boards
        .iter()
        .find(|board| board.id == diagnostic.board_id)
    else {
        return false;
    };
    let Some(material) = project.materials.iter().find(|m| m.id == board.material_id) else {
        return false;
    };
    let required_grain = board.effective_grain(material);
    !project.stock.iter().any(|piece| {
        if piece.material_id != board.material_id || piece.thickness != board.thickness {
            return false;
        }
        let usable = [
            i128::from(piece.length.micrometres())
                - i128::from(piece.trim[0].micrometres())
                - i128::from(piece.trim[1].micrometres()),
            i128::from(piece.width.micrometres())
                - i128::from(piece.trim[2].micrometres())
                - i128::from(piece.trim[3].micrometres()),
        ];
        [false, true].into_iter().any(|turned| {
            let [length, width] = if turned {
                [board.width, board.length]
            } else {
                [board.length, board.width]
            };
            let expected = match required_grain {
                BoardGrain::Unrestricted => None,
                BoardGrain::Length => Some(if turned {
                    StockGrain::AlongY
                } else {
                    StockGrain::AlongX
                }),
                BoardGrain::Width => Some(if turned {
                    StockGrain::AlongX
                } else {
                    StockGrain::AlongY
                }),
            };
            i128::from(length.micrometres()) <= usable[0]
                && i128::from(width.micrometres()) <= usable[1]
                && expected.is_none_or(|axis| {
                    piece.grain == StockGrain::Nondirectional || piece.grain == axis
                })
        })
    })
}

fn issue_resolution(project: &Project, diagnostic: &BoardDiagnostic) -> Request {
    if needs_new_stock(project, diagnostic) {
        Request::with(A::AddIssueStock, Target::Board(diagnostic.board_id))
    } else {
        Request::with(A::RepairIssue, Target::Board(diagnostic.board_id))
    }
}

fn card_status(proof: Option<&SheetProof>, localizer: &Localizer) -> String {
    match proof {
        Some(SheetProof::Unused) => localizer.text("sheet-unused"),
        Some(SheetProof::Verified { .. }) => localizer.text("sheet-verified"),
        Some(SheetProof::SearchExhausted) => localizer.text("sheet-feasibility-unknown"),
        Some(SheetProof::Violation(_)) => localizer.text("sheet-cut-conflict"),
        None => localizer.text("sheet-unverified"),
    }
}

fn card_utilization(proof: Option<&SheetProof>) -> Option<u32> {
    let (part, root) = proof?.utilization()?;
    (root > 0).then(|| ((part * 100 + root / 2) / root) as u32)
}

fn area_label(square_micrometres: i128) -> String {
    format!("{:.6} m²", square_micrometres as f64 / 1_000_000_000_000.0)
}

// Witness node numbers are local to this reconstruction. The root keeps the
// actual stock UUID; child UUIDs are deterministic sheet-scoped view identities.
fn witness_piece_uuid(stock_id: Uuid, id: usize) -> Uuid {
    let n = id as u128;
    Uuid::from_u128(stock_id.as_u128() ^ (n << 64) ^ n)
}

fn piece_identity(stock_id: Uuid, tree: &CutTree, id: usize, localizer: &Localizer) -> String {
    let node = tree.node(id).expect("witness operation references a node");
    let kind = match node.kind {
        CutKind::Part(board) => format!(" · {} {board}", localizer.text("sheet-part")),
        CutKind::Offcut => format!(" · {}", localizer.text("sheet-overlay-offcuts")),
        CutKind::Waste => format!(" · {}", localizer.text("sheet-waste")),
        CutKind::Split { .. } => String::new(),
    };
    format!(
        "#{id} ({}){kind} · {} × {} · X {} / Y {}",
        witness_piece_uuid(stock_id, id),
        format_length(node.rectangle.extent[0], Unit::Mm, locale(localizer), 3),
        format_length(node.rectangle.extent[1], Unit::Mm, locale(localizer), 3),
        format_length(node.rectangle.origin[0], Unit::Mm, locale(localizer), 3),
        format_length(node.rectangle.origin[1], Unit::Mm, locale(localizer), 3),
    )
}

fn cut_row(
    stock_id: Uuid,
    tree: &CutTree,
    cut: &CutOperation,
    localizer: &Localizer,
    trim: bool,
) -> String {
    let axis = match cut.axis {
        Axis::X => "X",
        Axis::Y => "Y",
    };
    let edge = |edge| {
        localizer.text(match edge {
            Edge::Low => "sheet-edge-low",
            Edge::High => "sheet-edge-high",
        })
    };
    format!(
        "#{}{} · {}: {} · {}: {} {axis} · {}: {} · {}: {} · {}: {} · {}: {} · {}: {}",
        cut.number,
        if trim {
            format!(" ({})", localizer.text("sheet-trim-pass"))
        } else {
            String::new()
        },
        localizer.text("sheet-input"),
        piece_identity(stock_id, tree, cut.input, localizer),
        localizer.text("sheet-reference-edge"),
        edge(cut.reference_edge),
        localizer.text("sheet-retained-distance"),
        format_length(cut.retained_extent, Unit::Mm, locale(localizer), 3),
        localizer.text("sheet-kerf-side"),
        edge(cut.kerf_side),
        localizer.text("sheet-retained-output"),
        piece_identity(stock_id, tree, cut.retained_output, localizer),
        localizer.text("sheet-first-output"),
        piece_identity(stock_id, tree, cut.outputs.first, localizer),
        localizer.text("sheet-second-output"),
        piece_identity(stock_id, tree, cut.outputs.second, localizer),
    )
}

/// Rows and canvas consume the same cached witness; only pointer state is returned.
fn sheet_inspector(
    ui: &mut egui::Ui,
    piece: Option<&StockPieceReadModel>,
    localizer: &Localizer,
    compact: bool,
) -> Option<usize> {
    let Some(piece) = piece else {
        ui.label(localizer.text("sheet-unverified"));
        return None;
    };
    ui.strong(localizer.text("sheet-inspector"));
    if compact {
        ui.push_id(("sheet-inspector-details", piece.id), |ui| {
            ui.collapsing(localizer.text("shell-advanced"), |ui| {
                sheet_piece_details(ui, piece, localizer);
            });
        });
    } else {
        sheet_piece_details(ui, piece, localizer);
    }
    match &piece.proof {
        SheetProof::Verified { tree, accounting } => {
            ui.label(format!(
                "{}: {:.1}% · {}: {}",
                localizer.text("sheet-utilization"),
                accounting.part_area as f64 * 100.0 / accounting.root_area as f64,
                localizer.text("sheet-physical-cuts"),
                tree.cut_count()
            ));
            for (key, area) in [
                ("sheet-recoverable-area", accounting.offcut_area),
                ("sheet-kerf-loss", accounting.kerf_loss),
                ("sheet-trim-loss", accounting.trim_loss),
                ("sheet-other-waste", accounting.waste_area),
            ] {
                if !compact || area != 0 || key == "sheet-kerf-loss" {
                    ui.label(format!("{}: {}", localizer.text(key), area_label(area)));
                }
            }
            ui.strong(localizer.text("sheet-cut-sequence"));
            if !compact {
                ui.small(localizer.text("sheet-piece-ids-hint"));
            }
            let trim_count = piece.trim.iter().filter(|v| **v != Length::ZERO).count();
            let mut hovered = None;
            for cut in tree.operations() {
                let row = cut_row(piece.id, tree, &cut, localizer, cut.number <= trim_count);
                if compact {
                    let axis = match cut.axis {
                        Axis::X => "X",
                        Axis::Y => "Y",
                    };
                    let label = format!(
                        "#{} · {axis} {}",
                        cut.number,
                        format_length(cut.retained_extent, Unit::Mm, locale(localizer), 0)
                    );
                    let response = ui
                        .collapsing(label, |ui| {
                            ui.small(&row);
                        })
                        .header_response;
                    if response.on_hover_text(&row).hovered() {
                        hovered = Some(cut.number);
                    }
                } else if ui
                    .add(egui::Label::new(row).sense(egui::Sense::hover()))
                    .hovered()
                {
                    hovered = Some(cut.number);
                }
            }
            if tree.cut_count() == 0 {
                ui.small(localizer.text("sheet-no-cuts"));
            }
            hovered
        }
        SheetProof::Unused => {
            ui.label(localizer.text("sheet-unused"));
            ui.label(localizer.text("sheet-unverified"));
            None
        }
        SheetProof::SearchExhausted => {
            ui.colored_label(
                egui::Color32::from_rgb(140, 95, 20),
                localizer.text("sheet-feasibility-unknown"),
            );
            ui.label(localizer.text("sheet-unverified"));
            None
        }
        SheetProof::Violation(reason) => {
            let key = match reason {
                ReconstructionViolation::Cut(CutError::InvalidTrim) => "sheet-trim-conflict",
                ReconstructionViolation::Cut(CutError::SubKerfEdge) => "sheet-kerf-conflict",
                ReconstructionViolation::Witness(WitnessError::PlacementMismatch(_)) => {
                    "conflict-outside-stock"
                }
                ReconstructionViolation::Witness(WitnessError::MaterialMismatch(_)) => {
                    "conflict-material-identity"
                }
                ReconstructionViolation::Witness(WitnessError::ThicknessMismatch(_)) => {
                    "conflict-thickness"
                }
                ReconstructionViolation::Witness(WitnessError::GrainMismatch(_)) => {
                    "conflict-grain"
                }
                _ => "sheet-cut-conflict",
            };
            ui.colored_label(
                egui::Color32::DARK_RED,
                format!(
                    "{}: {}",
                    localizer.text("sheet-violation"),
                    localizer.text(key)
                ),
            );
            ui.label(localizer.text("sheet-unverified"));
            None
        }
    }
}

fn sheet_piece_details(ui: &mut egui::Ui, piece: &StockPieceReadModel, localizer: &Localizer) {
    ui.label(format!(
        "{}: {}",
        localizer.text("stock-source"),
        localizer.text(match piece.source {
            plan_my_cabinet::domain::StockSource::Owned => "stock-owned",
            plan_my_cabinet::domain::StockSource::ToPurchase => "stock-purchase",
        })
    ));
    ui.label(format!(
        "{}: {} · {}",
        localizer.text("sheet-material-thickness"),
        piece.material_name,
        format_length(piece.measured_thickness, Unit::Mm, locale(localizer), 3)
    ));
    ui.label(format!(
        "{}: {}",
        localizer.text("stock-grain"),
        localizer.text(match piece.grain {
            StockGrain::AlongX => "stock-grain-x",
            StockGrain::AlongY => "stock-grain-y",
            StockGrain::Nondirectional => "stock-grain-none",
            StockGrain::Unknown => "stock-grain-unknown",
        })
    ));
    ui.label(format!(
        "{}: {}",
        localizer.text("sheet-trims"),
        piece
            .trim
            .map(|v| format_length(v, Unit::Mm, locale(localizer), 3))
            .join(" / ")
    ));
    // The order above is left, right, bottom, top, including the blade allowance.
    ui.label(format!(
        "{}: {}",
        localizer.text("stock-price-heading"),
        piece.price.map_or_else(
            || localizer.text("stock-price-unknown"),
            |price| price.display(if localizer.language() == Language::En {
                MoneyLocale::English
            } else {
                MoneyLocale::PortugueseBrazil
            }),
        )
    ));
}

const SHEET_INSPECTOR_WIDTH: f32 = 308.0;
const SHEET_PANE_GAP: f32 = 12.0;
const SHEET_SIDE_BY_SIDE_MIN: f32 = 760.0;

#[derive(Clone, Copy)]
struct SheetPaneLayout {
    canvas: egui::Rect,
    inspector: egui::Rect,
    bounds: egui::Vec2,
}

fn sheet_panes(
    origin: egui::Pos2,
    width: f32,
    height: f32,
    separate_inspector: bool,
) -> SheetPaneLayout {
    if separate_inspector {
        SheetPaneLayout {
            canvas: egui::Rect::from_min_size(origin, egui::vec2(width, height)),
            inspector: egui::Rect::NOTHING,
            bounds: egui::vec2(width, height),
        }
    } else if width >= SHEET_SIDE_BY_SIDE_MIN {
        let canvas_width = width - SHEET_INSPECTOR_WIDTH - SHEET_PANE_GAP;
        SheetPaneLayout {
            canvas: egui::Rect::from_min_size(origin, egui::vec2(canvas_width, height)),
            inspector: egui::Rect::from_min_size(
                origin + egui::vec2(canvas_width + SHEET_PANE_GAP, 0.0),
                egui::vec2(SHEET_INSPECTOR_WIDTH, height),
            ),
            bounds: egui::vec2(width, height),
        }
    } else {
        let canvas_height = height.min(320.0);
        let inspector_height = height.min(280.0);
        SheetPaneLayout {
            canvas: egui::Rect::from_min_size(origin, egui::vec2(width, canvas_height)),
            inspector: egui::Rect::from_min_size(
                origin + egui::vec2(0.0, canvas_height + SHEET_PANE_GAP),
                egui::vec2(width, inspector_height),
            ),
            bounds: egui::vec2(width, canvas_height + SHEET_PANE_GAP + inspector_height),
        }
    }
}

fn sheet_thumbnail(ui: &mut egui::Ui, project: &Project, stock: &Stock) {
    let (region, _) = ui.allocate_exact_size(egui::vec2(60.0, 40.0), egui::Sense::hover());
    let painter = ui.painter().with_clip_rect(region);
    painter.rect_filled(region, 3.0, egui::Color32::from_rgb(247, 245, 240));
    let parts: Vec<_> = project
        .allocations
        .iter()
        .filter(|a| a.stock_id == stock.id)
        .collect();
    if parts.is_empty() {
        // Dashed outline means inventory without a layout, never a proved blank.
        for edge in 0..2 {
            let y = if edge == 0 {
                region.top()
            } else {
                region.bottom()
            };
            for n in 0..6 {
                painter.line_segment(
                    [
                        egui::pos2(region.left() + n as f32 * 10.0, y),
                        egui::pos2(region.left() + n as f32 * 10.0 + 6.0, y),
                    ],
                    egui::Stroke::new(1.0, egui::Color32::GRAY),
                );
            }
        }
    } else {
        painter.rect_stroke(
            region,
            3.0,
            egui::Stroke::new(1.0, egui::Color32::GRAY),
            egui::StrokeKind::Inside,
        );
        let sx = region.width() / stock.length.micrometres() as f32;
        let sy = region.height() / stock.width.micrometres() as f32;
        for allocation in parts {
            if let Some([x, y, length, width]) = footprint(project, allocation) {
                let rect = egui::Rect::from_min_max(
                    region.min + egui::vec2(x as f32 * sx, y as f32 * sy),
                    region.min + egui::vec2((x + length) as f32 * sx, (y + width) as f32 * sy),
                );
                painter.rect_filled(rect, 1.0, egui::Color32::from_rgb(226, 219, 207));
                painter.rect_stroke(
                    rect,
                    1.0,
                    egui::Stroke::new(0.8, egui::Color32::from_rgb(156, 144, 126)),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
}

fn choose_sheet_board(project: &Project, selection: &mut Selection, id: Uuid, additive: bool) {
    let _ = actions::select_sheet_board(
        project,
        selection,
        Request::with(A::SelectSheetBoard, Target::Board(id))
            .argument(Argument::Additive(additive)),
    );
}

#[derive(Clone, Copy)]
struct PartEmphasis {
    selected: bool,
    in_selection: bool,
    conflict: bool,
}

fn paint_allocation(
    painter: &egui::Painter,
    region: egui::Rect,
    board: &Board,
    localizer: &Localizer,
    overlays: SheetOverlays,
    callout: usize,
    emphasis: PartEmphasis,
) {
    let PartEmphasis {
        selected,
        in_selection,
        conflict,
    } = emphasis;
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
    let dimensions = format!(
        "{} × {}",
        format_length(board.length, Unit::Mm, locale(localizer), 0),
        format_length(board.width, Unit::Mm, locale(localizer), 0)
    );
    let name = if overlays.ids {
        board_label(board)
    } else {
        board.name.clone()
    };
    let font = egui::FontId::proportional(11.0);
    let name_width = painter
        .layout_no_wrap(name.clone(), font.clone(), egui::Color32::BLACK)
        .size()
        .x;
    let dimensions_width = painter
        .layout_no_wrap(dimensions.clone(), font.clone(), egui::Color32::BLACK)
        .size()
        .x;
    if full_part_label_fits(region.size(), name_width, dimensions_width) {
        painter.text(
            region.center() - egui::vec2(0.0, 8.0),
            egui::Align2::CENTER_CENTER,
            name,
            font.clone(),
            egui::Color32::BLACK,
        );
        painter.text(
            region.center() + egui::vec2(0.0, 8.0),
            egui::Align2::CENTER_CENTER,
            dimensions,
            font,
            egui::Color32::BLACK,
        );
    } else {
        // Compact identity refers to the associated full label below the sheet.
        let center = region.center();
        let anchor = center;
        let radius = (region.width().min(region.height()) * 0.45).clamp(2.0, 9.0);
        painter.circle_filled(anchor, radius, egui::Color32::from_rgb(48, 57, 49));
        painter.text(
            anchor,
            egui::Align2::CENTER_CENTER,
            callout.to_string(),
            egui::FontId::proportional(10.0),
            egui::Color32::WHITE,
        );
    }
}

fn full_part_label_fits(size: egui::Vec2, name_width: f32, dimensions_width: f32) -> bool {
    size.x >= name_width.max(dimensions_width) + 8.0 && size.y >= 35.0
}

fn grain_direction(grain: BoardGrain, quarter_turn: bool) -> Option<egui::Vec2> {
    match (grain, quarter_turn) {
        (BoardGrain::Length, false) | (BoardGrain::Width, true) => Some(egui::vec2(16.0, 0.0)),
        (BoardGrain::Length, true) | (BoardGrain::Width, false) => Some(egui::vec2(0.0, 16.0)),
        _ => None,
    }
}

#[derive(Clone, Copy, Default)]
pub struct SheetFocus {
    pub sheet: Option<Uuid>,
    pub issue: Option<Uuid>,
    pub scroll_to_target: bool,
}

/// The host renders this in the Cut plan controls pane. Selection and repair
/// requests use the same actions as the canvas, without borrowing its height.
pub fn show_sheet_list(
    ui: &mut egui::Ui,
    editor: &ProjectEditor,
    selection: &Selection,
    localizer: &Localizer,
    repair: &mut RepairUi,
    modal: bool,
    focus: SheetFocus,
) -> Option<Request> {
    let project = editor.preview().unwrap_or(editor.project());
    let key = diagnostics_key(project, editor.preview().is_some());
    if repair
        .stock_model_cache
        .as_ref()
        .is_none_or(|(cached, _)| *cached != key)
    {
        repair.hovered_cut = None;
        repair.stock_model_cache = Some((key.clone(), StockReadModel::build(project).ok()));
    }
    if repair
        .board_diagnostics_cache
        .as_ref()
        .is_none_or(|(cached, _)| *cached != key)
    {
        repair.board_diagnostics_cache = Some((
            key,
            repair
                .stock_model_cache
                .as_ref()
                .and_then(|(_, model)| model.as_ref())
                .map(|model| model.boards.clone())
                .unwrap_or_else(|| allocation_diagnostics::diagnose(project)),
        ));
    }
    let model = repair
        .stock_model_cache
        .as_ref()
        .and_then(|(_, model)| model.as_ref());
    let diagnostics = &repair.board_diagnostics_cache.as_ref().unwrap().1;
    let ordered = project.ordered_stock();
    if focus.scroll_to_target
        && let Some(id) = focus.sheet
        && ordered.iter().any(|stock| stock.id == id)
    {
        repair.focused_sheet = Some(id);
    }
    if !ordered
        .iter()
        .any(|stock| Some(stock.id) == repair.focused_sheet)
    {
        repair.focused_sheet = ordered.first().map(|stock| stock.id);
    }
    let mut request = None;
    ui.horizontal(|ui| {
        ui.strong(localizer.text("sheet-priority-cards"));
        if ui
            .add_enabled(
                !modal && !repair.active(),
                egui::Button::new("+").min_size(egui::vec2(24.0, 24.0)),
            )
            .on_hover_text(localizer.text("stock-new"))
            .clicked()
        {
            request = Some(Request::new(A::NewStock));
        }
    });
    for (rank, stock) in ordered.into_iter().enumerate() {
        let piece = model.and_then(|model| model.miniature(stock.id));
        let count = project
            .allocations
            .iter()
            .filter(|a| a.stock_id == stock.id)
            .count();
        let material = project
            .materials
            .iter()
            .find(|m| m.id == stock.material_id)
            .map(|m| m.name.as_str())
            .unwrap_or("?");
        let title = format!(
            "{}  {}",
            project.stock_alias(stock.id).unwrap_or("?"),
            material
        );
        let description = format!(
            "{} × {} · {count} {}",
            format_length(stock.length, Unit::Mm, locale(localizer), 0),
            format_length(stock.width, Unit::Mm, locale(localizer), 0),
            localizer.text("sheet-parts")
        );
        ui.horizontal(|ui| {
            sheet_thumbnail(ui, project, stock);
            ui.vertical(|ui| {
                if ui
                    .add_enabled(
                        !modal,
                        egui::Button::new(title).selected(repair.focused_sheet == Some(stock.id)),
                    )
                    .clicked()
                {
                    repair.focused_sheet = Some(stock.id);
                }
                ui.small(description);
                ui.small(format!(
                    "#{} · {} · {}{}",
                    piece.map_or(rank + 1, |p| p.global_rank),
                    localizer.text(match stock.source {
                        plan_my_cabinet::domain::StockSource::Owned => "stock-owned",
                        plan_my_cabinet::domain::StockSource::ToPurchase => "stock-purchase",
                    }),
                    card_status(piece.map(|p| &p.proof), localizer),
                    card_utilization(piece.map(|p| &p.proof))
                        .map_or(String::new(), |n| format!(" · {n}%")),
                ));
            });
        });
    }
    ui.separator();
    ui.strong(localizer.text("sheet-needs-stock"));
    if !needs_stock(diagnostics).any(|_| true) {
        ui.small(localizer.text("sheet-no-issues"));
    }
    for entry in needs_stock(diagnostics) {
        let Some(board) = project.boards.iter().find(|b| b.id == entry.board_id) else {
            continue;
        };
        ui.group(|ui| {
            ui.strong(format!(
                "{}{}",
                board_label(board),
                if selection.visible(project, board.id) {
                    String::new()
                } else {
                    format!(" · {}", localizer.text("global-hidden"))
                }
            ));
            ui.small(format!(
                "{} × {} × {}",
                format_length(board.length, Unit::Mm, locale(localizer), 3),
                format_length(board.width, Unit::Mm, locale(localizer), 3),
                format_length(board.thickness, Unit::Mm, locale(localizer), 3)
            ));
            for reason in &entry.reasons {
                ui.colored_label(egui::Color32::DARK_RED, localizer.text(reason.key()));
            }
            if project.allocations.iter().any(|allocation| {
                allocation.board_id == board.id
                    && matches!(
                        model
                            .and_then(|m| m.miniature(allocation.stock_id))
                            .map(|piece| &piece.proof),
                        Some(SheetProof::Violation(ReconstructionViolation::Cut(
                            CutError::InvalidTrim
                        )))
                    )
            }) {
                ui.colored_label(
                    egui::Color32::DARK_RED,
                    localizer.text("sheet-trim-conflict"),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !modal && !repair.active(),
                        egui::Button::new(localizer.text("global-locate")),
                    )
                    .clicked()
                {
                    request = Some(Request::with(A::LocateIssue, Target::Board(board.id)));
                }
                if ui
                    .add_enabled(
                        !modal && !repair.active(),
                        egui::Button::new(localizer.text(if needs_new_stock(project, entry) {
                            "sheet-add-material"
                        } else {
                            "global-repair"
                        })),
                    )
                    .clicked()
                {
                    request = Some(issue_resolution(project, entry));
                }
            });
        });
        if focus.scroll_to_target && focus.issue == Some(board.id) {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
    }
    request
}

/// Render into the host's existing Cut plan inspector ScrollArea, after the
/// central sheet. Hover is session-only and read on the next canvas frame.
pub fn show_focused_inspector(
    ui: &mut egui::Ui,
    editor: &ProjectEditor,
    localizer: &Localizer,
    repair: &mut RepairUi,
) {
    let project = editor.preview().unwrap_or(editor.project());
    let key = diagnostics_key(project, editor.preview().is_some());
    if repair
        .stock_model_cache
        .as_ref()
        .is_none_or(|(cached, _)| *cached != key)
    {
        repair.hovered_cut = None;
        repair.stock_model_cache = Some((key, StockReadModel::build(project).ok()));
    }
    ui.heading(localizer.text("sheet-heading"));
    let stock = repair
        .focused_sheet
        .and_then(|id| project.stock.iter().find(|s| s.id == id));
    if let Some(stock) = stock {
        ui.small(stock_label(project, stock));
    }
    let hovered = sheet_inspector(
        ui,
        stock.and_then(|stock| {
            repair
                .stock_model_cache
                .as_ref()?
                .1
                .as_ref()?
                .miniature(stock.id)
        }),
        localizer,
        true,
    );
    let next = stock.and_then(|stock| hovered.map(|number| (stock.id, number)));
    if repair.hovered_cut != next {
        repair.hovered_cut = next;
        ui.ctx().request_repaint();
    }
}

/// Existing hosts retain the in-canvas fallback until they opt into the
/// dedicated pane with `show_with_layout(..., true)`.
#[cfg(test)]
pub fn show(
    ui: &mut egui::Ui,
    editor: &mut ProjectEditor,
    selection: &mut Selection,
    localizer: &Localizer,
    modal: bool,
    repair: &mut RepairUi,
    focus: SheetFocus,
) -> Option<Request> {
    show_with_layout(
        ui, editor, selection, localizer, modal, repair, focus, false,
    )
}

#[allow(clippy::too_many_arguments)] // The host passes the same workspace inputs plus its pane availability.
pub fn show_with_layout(
    ui: &mut egui::Ui,
    editor: &mut ProjectEditor,
    selection: &mut Selection,
    localizer: &Localizer,
    modal: bool,
    repair: &mut RepairUi,
    focus: SheetFocus,
    separate_inspector: bool,
) -> Option<Request> {
    if repair.zoom == 0.0 {
        repair.zoom = 1.0;
    }
    if !separate_inspector {
        repair.hovered_cut = None;
    }
    let project = editor.preview().unwrap_or(editor.project()).clone();
    let project = &project;
    if !separate_inspector {
        ui.heading(localizer.text("sheet-heading"));
    }
    let mut action = None;
    let mut selection_request = None;
    let mut accept = false;
    let mut cancel = false;
    let key = diagnostics_key(project, editor.preview().is_some());
    if repair
        .stock_model_cache
        .as_ref()
        .is_none_or(|(cached, _)| *cached != key)
    {
        repair.hovered_cut = None;
        repair.stock_model_cache = Some((key.clone(), StockReadModel::build(project).ok()));
    }
    if repair
        .board_diagnostics_cache
        .as_ref()
        .is_none_or(|(cached, _)| *cached != key)
    {
        repair.board_diagnostics_cache = Some((
            key.clone(),
            repair
                .stock_model_cache
                .as_ref()
                .unwrap()
                .1
                .as_ref()
                .map(|model| model.boards.clone())
                .unwrap_or_else(|| allocation_diagnostics::diagnose(project)),
        ));
    }
    let model = repair.stock_model_cache.as_ref().unwrap().1.clone();
    let board_diagnostics = repair.board_diagnostics_cache.as_ref().unwrap().1.clone();
    let ordered = project.ordered_stock();
    if focus.scroll_to_target && focus.sheet.is_some() {
        repair.focused_sheet = focus.sheet;
    }
    if !ordered.iter().any(|s| Some(s.id) == repair.focused_sheet) {
        repair.focused_sheet = focus
            .sheet
            .filter(|id| ordered.iter().any(|s| s.id == *id))
            .or_else(|| ordered.first().map(|s| s.id));
    }
    if !separate_inspector {
        ui.horizontal(|ui| {
            ui.strong(localizer.text("sheet-priority-cards"));
            if ui
                .add_enabled(
                    !modal && !repair.active(),
                    egui::Button::new(localizer.text("stock-new")),
                )
                .clicked()
            {
                selection_request = Some(Request::new(A::NewStock));
            }
        });
        egui::ScrollArea::horizontal()
            .id_salt("sheet-cards")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for stock in &ordered {
                        let piece = model.as_ref().and_then(|m| m.miniature(stock.id));
                        let proof = piece.map(|p| &p.proof);
                        let count = project
                            .allocations
                            .iter()
                            .filter(|a| a.stock_id == stock.id)
                            .count();
                        let material = project
                            .materials
                            .iter()
                            .find(|m| m.id == stock.material_id)
                            .map(|m| m.name.as_str())
                            .unwrap_or("?");
                        let title = format!(
                            "{} · {} · {}",
                            project.stock_alias(stock.id).unwrap_or("?"),
                            material,
                            stock.name
                        );
                        let description = format!(
                            "{} × {} · {} {}",
                            format_length(stock.length, Unit::Mm, locale(localizer), 0),
                            format_length(stock.width, Unit::Mm, locale(localizer), 0),
                            count,
                            localizer.text("sheet-parts")
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(230.0, 150.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                sheet_thumbnail(ui, project, stock);
                                if ui
                                    .add_enabled(
                                        !modal,
                                        egui::Button::new(title)
                                            .min_size(egui::vec2(220.0, 36.0))
                                            .selected(repair.focused_sheet == Some(stock.id)),
                                    )
                                    .clicked()
                                {
                                    repair.focused_sheet = Some(stock.id);
                                }
                                ui.small(description);
                                ui.small(format!(
                                    "#{} · {} · {}",
                                    piece.map(|p| p.global_rank).unwrap_or_else(|| ordered
                                        .iter()
                                        .position(|s| s.id == stock.id)
                                        .unwrap()
                                        + 1),
                                    localizer.text(match stock.source {
                                        plan_my_cabinet::domain::StockSource::Owned =>
                                            "stock-owned",
                                        plan_my_cabinet::domain::StockSource::ToPurchase =>
                                            "stock-purchase",
                                    }),
                                    card_status(proof, localizer)
                                ));
                                if let Some(percent) = card_utilization(proof) {
                                    ui.small(format!("{percent}%"));
                                }
                            },
                        );
                    }
                });
            });
        ui.separator();
        ui.strong(localizer.text("sheet-needs-stock"));
        if !needs_stock(&board_diagnostics).any(|_| true) {
            ui.small(localizer.text("sheet-no-issues"));
        }
        for entry in needs_stock(&board_diagnostics) {
            let Some(board) = project.boards.iter().find(|b| b.id == entry.board_id) else {
                continue;
            };
            ui.group(|ui| {
                let hidden = !selection.visible(project, board.id);
                ui.strong(format!(
                    "{}{}",
                    board_label(board),
                    if hidden {
                        format!(" · {}", localizer.text("global-hidden"))
                    } else {
                        String::new()
                    }
                ));
                ui.small(format!(
                    "{} × {} × {}",
                    format_length(board.length, Unit::Mm, locale(localizer), 3),
                    format_length(board.width, Unit::Mm, locale(localizer), 3),
                    format_length(board.thickness, Unit::Mm, locale(localizer), 3)
                ));
                for reason in &entry.reasons {
                    ui.colored_label(egui::Color32::DARK_RED, localizer.text(reason.key()));
                }
                if project.allocations.iter().any(|allocation| {
                    allocation.board_id == board.id
                        && matches!(
                            model
                                .as_ref()
                                .and_then(|m| m.miniature(allocation.stock_id))
                                .map(|piece| &piece.proof),
                            Some(SheetProof::Violation(ReconstructionViolation::Cut(
                                CutError::InvalidTrim
                            )))
                        )
                }) {
                    ui.colored_label(
                        egui::Color32::DARK_RED,
                        localizer.text("sheet-trim-conflict"),
                    );
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !modal && !repair.active(),
                            egui::Button::new(localizer.text("global-locate")),
                        )
                        .clicked()
                    {
                        selection_request =
                            Some(Request::with(A::LocateIssue, Target::Board(board.id)));
                    }
                    if needs_new_stock(project, entry) {
                        if ui
                            .add_enabled(
                                !modal && !repair.active(),
                                egui::Button::new(localizer.text("sheet-add-material")),
                            )
                            .clicked()
                        {
                            selection_request = Some(issue_resolution(project, entry));
                        }
                    } else if ui
                        .add_enabled(
                            !modal && !repair.active(),
                            egui::Button::new(localizer.text("global-repair")),
                        )
                        .clicked()
                    {
                        selection_request = Some(issue_resolution(project, entry));
                    }
                });
            });
            if focus.scroll_to_target && focus.issue == Some(board.id) {
                ui.scroll_to_cursor(Some(egui::Align::TOP));
            }
        }
    }
    if let Some(index) = ordered
        .iter()
        .position(|s| Some(s.id) == repair.focused_sheet)
    {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!modal && index > 0, egui::Button::new("‹"))
                .on_hover_text(localizer.text("sheet-previous"))
                .clicked()
            {
                repair.focused_sheet = Some(ordered[index - 1].id);
            }
            let name = stock_label(project, ordered[index]);
            ui.strong(if separate_inspector {
                name.clone()
            } else {
                format!("{} · {name}", localizer.text("sheet-target"))
            })
            .on_hover_text(ordered[index].id.to_string());
            if ui
                .add_enabled(!modal && index + 1 < ordered.len(), egui::Button::new("›"))
                .on_hover_text(localizer.text("sheet-next"))
                .clicked()
            {
                repair.focused_sheet = Some(ordered[index + 1].id);
            }
        });
        let stock = ordered
            .iter()
            .find(|s| Some(s.id) == repair.focused_sheet)
            .copied()
            .unwrap();
        let piece = model.as_ref().and_then(|m| m.miniature(stock.id));
        if !separate_inspector {
            ui.small(card_status(piece.map(|p| &p.proof), localizer));
            if let Some(piece) = piece
                && let Some(percent) = card_utilization(Some(&piece.proof))
            {
                ui.small(format!(
                    "{}: {percent}% · {}: {}",
                    localizer.text("sheet-utilization"),
                    localizer.text("sheet-physical-cuts"),
                    piece.proof.cut_count().unwrap()
                ));
            }
        }
    } else {
        ui.small(localizer.text("sheet-no-pieces"));
    }
    if !repair.active() {
        if !separate_inspector {
            ui.small(localizer.text("sheet-read-only"));
        }
        if ui
            .add_enabled(!modal, egui::Button::new(localizer.text("sheet-edit")))
            .clicked()
        {
            let _ = actions::contextual(Request::new(A::BeginRepair), Ok(()), || {
                repair.begin(editor, selection, locale(localizer))
            });
        }
    } else {
        if !separate_inspector {
            ui.small(localizer.text("sheet-repair-hint"));
        }
        ui.horizontal(|ui| {
            accept = ui
                .add_enabled(
                    !modal && repair.can_accept(editor),
                    egui::Button::new(localizer.text("sheet-accept")),
                )
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
                let previous = repair.stock;
                egui::ComboBox::from_id_salt("sheet-target-stock")
                    .selected_text(
                        project
                            .stock
                            .iter()
                            .find(|s| Some(s.id) == repair.stock)
                            .map(|s| stock_label(project, s))
                            .unwrap_or_default(),
                    )
                    .show_ui(ui, |ui| {
                        for stock in project.ordered_stock() {
                            ui.selectable_value(
                                &mut repair.stock,
                                Some(stock.id),
                                stock_label(project, stock),
                            )
                            .on_hover_text(stock.id.to_string());
                        }
                    });
                if repair.stock != previous {
                    repair.placement_dirty = true;
                }
            });
            let unit = repair.entry_unit.unwrap_or_else(|| numeric_unit(project));
            let mut valid = true;
            for axis in 0..2 {
                ui.horizontal(|ui| {
                    ui.label(localizer.text(if axis == 0 {
                        "sheet-origin-x"
                    } else {
                        "sheet-origin-y"
                    }));
                    if ui.text_edit_singleline(&mut repair.origin[axis]).changed() {
                        repair.entry_unit.get_or_insert(unit);
                        repair.consent[axis] = false;
                        repair.placement_dirty = true;
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
            if ui
                .checkbox(
                    &mut repair.quarter_turn,
                    localizer.text("sheet-quarter-turn"),
                )
                .changed()
            {
                repair.placement_dirty = true;
            }
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
                            !modal && !repair.placement_dirty,
                            egui::Button::new(localizer.text("sheet-unallocate-action")),
                        )
                        .clicked()
                    {
                        action = Some(RepairAction::Unallocate(id));
                    }
                    if ui
                        .add_enabled(
                            !modal && !repair.placement_dirty,
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
                            .map(|s| stock_label(project, s))
                            .unwrap_or_else(|| stock_id.to_string()),
                        localizer.text(message)
                    ),
                );
            }
        }
    }
    let issues = diagnostic_issues(&board_diagnostics);
    let canvas_sheet = repair.focused_sheet;
    ui.horizontal_wrapped(|ui| {
        if ui.button(localizer.text("sheet-fit")).clicked() {
            repair.zoom = 1.0;
        }
        if ui
            .button("−")
            .on_hover_text(localizer.text("sheet-zoom-out"))
            .clicked()
        {
            repair.zoom = (repair.zoom.max(0.1) / 1.25).max(0.25);
        }
        if ui
            .button("+")
            .on_hover_text(localizer.text("sheet-zoom-in"))
            .clicked()
        {
            repair.zoom = (repair.zoom.max(0.1) * 1.25).min(4.0);
        }
        ui.checkbox(
            &mut repair.overlays.cuts,
            localizer.text("sheet-overlay-cuts"),
        );
        ui.checkbox(
            &mut repair.overlays.offcuts,
            localizer.text("sheet-overlay-offcuts"),
        );
        ui.checkbox(
            &mut repair.overlays.grain,
            localizer.text("sheet-overlay-grain"),
        );
        ui.checkbox(
            &mut repair.overlays.ids,
            localizer.text("sheet-overlay-ids"),
        );
    });
    ui.small(localizer.text("sheet-legend"));
    egui::ScrollArea::vertical()
        .id_salt("sheet-workspace-scroll")
        .show(ui, |ui| {
            for stock in project
                .ordered_stock()
                .into_iter()
                .filter(|s| Some(s.id) == canvas_sheet)
            {
                let width = ui.available_width();
                let height =
                    ui.available_height().max(280.0) - if separate_inspector { 25.0 } else { 0.0 };
                let size = sheet_panes(egui::Pos2::ZERO, width, height, separate_inspector).bounds;
                let (allocated, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let layout = sheet_panes(allocated.min, width, height, separate_inspector);
                // Inspect first, even though it is on the right: pointer state
                // reaches the canvas painter during this same frame.
                let hovered_cut = if separate_inspector {
                    repair
                        .hovered_cut
                        .and_then(|(id, number)| (id == stock.id).then_some(number))
                } else {
                    let mut inspector_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt(("sheet-inspector-pane", stock.id))
                            .max_rect(layout.inspector),
                    );
                    inspector_ui.set_clip_rect(layout.inspector.intersect(ui.clip_rect()));
                    egui::ScrollArea::vertical()
                        .id_salt(("sheet-sequence-scroll", stock.id))
                        .max_height(layout.inspector.height())
                        .show(&mut inspector_ui, |ui| {
                            sheet_inspector(
                                ui,
                                model.as_ref().and_then(|m| m.miniature(stock.id)),
                                localizer,
                                false,
                            )
                        })
                        .inner
                };
                let mut canvas_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt(("sheet-canvas-pane", stock.id))
                        .max_rect(layout.canvas),
                );
                canvas_ui.set_clip_rect(layout.canvas.intersect(ui.clip_rect()));
                egui::ScrollArea::both()
                    .id_salt(("sheet-canvas-scroll", stock.id))
                    .max_height(layout.canvas.height())
                    .show(&mut canvas_ui, |ui| {
                        ui.separator();
                        let heading = ui.strong(stock_label(project, stock));
                        heading.on_hover_text(stock.id.to_string());
                        let scale = sheet_scale(
                            stock,
                            (layout.canvas.width() - 72.0).max(1.0),
                            (layout.canvas.height() - 115.0).max(1.0),
                        ) * repair.zoom.max(0.1);
                        let sheet_size = egui::vec2(
                            (stock.length.micrometres() as f64 / 1000.0 * f64::from(scale)) as f32,
                            (stock.width.micrometres() as f64 / 1000.0 * f64::from(scale)) as f32,
                        );
                        // Keep partially outside draft placements visible at the edge.
                        let gutter = egui::vec2(34.0, 28.0);
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
                        paint_rulers(&painter, sheet, stock, scale);
                        let proof = model
                            .as_ref()
                            .and_then(|m| m.miniature(stock.id))
                            .map(|p| &p.proof);
                        let witness = proof.and_then(|p| p.verified().map(|(tree, _)| tree));
                        if let Some(tree) = witness {
                            paint_witness(
                                &painter,
                                sheet,
                                tree,
                                scale,
                                SheetOverlays {
                                    cuts: false,
                                    ..repair.overlays
                                },
                                None,
                            );
                        }
                        let allocations: Vec<_> = project
                            .allocations
                            .iter()
                            .filter(|a| a.stock_id == stock.id)
                            .collect();
                        let hit_regions: Vec<_> = allocations
                            .iter()
                            .filter_map(|a| {
                                footprint(project, a).map(|rect| {
                                    (allocation_rect(sheet.min, scale, rect), a.board_id)
                                })
                            })
                            .collect();
                        if repair.active()
                            && !modal
                            && !cancel
                            && !repair.placement_dirty
                            && !ui.ctx().egui_wants_keyboard_input()
                        {
                            if response.drag_started()
                                && let Some(pointer) = response.interact_pointer_pos()
                                && let Some(id) = hit_board(
                                    &hit_regions,
                                    canvas,
                                    ui.input(|i| i.pointer.press_origin()).unwrap_or(pointer),
                                )
                                && let Some(a) =
                                    project.allocations.iter().find(|a| a.board_id == id)
                            {
                                choose_sheet_board(project, selection, id, false);
                                repair.select(project, Some(id), locale(localizer));
                                if !a.locked {
                                    repair.drag = Some(SheetDrag {
                                        board: id,
                                        stock: stock.id,
                                        origin: a.origin,
                                        turn: a.quarter_turn,
                                        start: ui
                                            .input(|i| i.pointer.press_origin())
                                            .unwrap_or(pointer),
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
                                    let pointer =
                                        response.interact_pointer_pos().unwrap_or(drag.start);
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
                        for (index, allocation) in allocations.iter().enumerate() {
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
                                board,
                                localizer,
                                repair.overlays,
                                index + 1,
                                PartEmphasis {
                                    selected,
                                    in_selection: selection.ids.contains(&board.id),
                                    conflict,
                                },
                            );
                            if repair.overlays.grain
                                && let Some(material) =
                                    project.materials.iter().find(|m| m.id == board.material_id)
                                && let Some(direction) = grain_direction(
                                    board.effective_grain(material),
                                    allocation.quarter_turn,
                                )
                                && region.width() >= 42.0
                                && region.height() >= 42.0
                            {
                                painter.with_clip_rect(region).arrow(
                                    region.left_top() + egui::vec2(7.0, 7.0),
                                    direction,
                                    egui::Stroke::new(1.5, egui::Color32::from_rgb(35, 49, 110)),
                                );
                            }
                        }
                        if let Some(tree) = witness {
                            paint_witness(
                                &painter,
                                sheet,
                                tree,
                                scale,
                                SheetOverlays {
                                    offcuts: false,
                                    ..repair.overlays
                                },
                                hovered_cut,
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
                        if repair.overlays.grain
                            && let Some(direction) = arrow
                        {
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
                            && (!repair.active() || !repair.placement_dirty)
                            && let Some(pointer) = response.interact_pointer_pos()
                            && let Some(id) = hit_board(&hit_regions, canvas, pointer)
                        {
                            selection_request = Some(
                                Request::with(A::SelectSheetBoard, Target::Board(id)).argument(
                                    Argument::Additive(
                                        ui.input(|i| i.modifiers.command || i.modifiers.shift),
                                    ),
                                ),
                            );
                        }
                        for (_, id) in hit_regions {
                            if let Some(board) = project.boards.iter().find(|b| b.id == id) {
                                let warning = issues.get(&id).map(|v| issue_text(localizer, v));
                                ui.horizontal(|ui| {
                                    let label = board_label(board);
                                    if ui
                                        .add_enabled(
                                            !modal && (!repair.active() || !repair.placement_dirty),
                                            egui::Button::new(label)
                                                .selected(selection.active == Some(id)),
                                        )
                                        .clicked()
                                    {
                                        selection_request = Some(
                                            Request::with(A::SelectSheetBoard, Target::Board(id))
                                                .argument(Argument::Additive(ui.input(|i| {
                                                    i.modifiers.command || i.modifiers.shift
                                                }))),
                                        );
                                    }
                                    if let Some(warning) = warning {
                                        ui.colored_label(
                                            egui::Color32::DARK_RED,
                                            format!("⚠ {warning}"),
                                        );
                                    }
                                });
                            }
                        }
                        for (index, allocation) in project
                            .allocations
                            .iter()
                            .filter(|a| a.stock_id == stock.id)
                            .enumerate()
                        {
                            if let Some(board) =
                                project.boards.iter().find(|b| b.id == allocation.board_id)
                            {
                                ui.small(format!(
                                    "{} · {} · {} × {} · X {} / Y {}{}",
                                    index + 1,
                                    if repair.overlays.ids {
                                        board_label(board)
                                    } else {
                                        board.name.clone()
                                    },
                                    format_length(board.length, Unit::Mm, locale(localizer), 3),
                                    format_length(board.width, Unit::Mm, locale(localizer), 3),
                                    format_length(
                                        allocation.origin[0],
                                        Unit::Mm,
                                        locale(localizer),
                                        3
                                    ),
                                    format_length(
                                        allocation.origin[1],
                                        Unit::Mm,
                                        locale(localizer),
                                        3
                                    ),
                                    issues
                                        .get(&board.id)
                                        .map(|items| format!(
                                            " · ⚠ {}",
                                            issue_text(localizer, items)
                                        ))
                                        .unwrap_or_default(),
                                ));
                            }
                        }
                        if repair.overlays.offcuts
                            && let Some(tree) = witness
                        {
                            for (id, node) in tree
                                .nodes()
                                .iter()
                                .enumerate()
                                .filter(|(_, node)| node.kind == CutKind::Offcut)
                            {
                                ui.small(format!(
                                    "{} #{id} · {} × {} · X {} / Y {}",
                                    localizer.text("sheet-overlay-offcuts"),
                                    format_length(
                                        node.rectangle.extent[0],
                                        Unit::Mm,
                                        locale(localizer),
                                        3
                                    ),
                                    format_length(
                                        node.rectangle.extent[1],
                                        Unit::Mm,
                                        locale(localizer),
                                        3
                                    ),
                                    format_length(
                                        node.rectangle.origin[0],
                                        Unit::Mm,
                                        locale(localizer),
                                        3
                                    ),
                                    format_length(
                                        node.rectangle.origin[1],
                                        Unit::Mm,
                                        locale(localizer),
                                        3
                                    ),
                                ));
                            }
                        }
                    });
            }
            if repair.focused_sheet.is_some()
                && !project
                    .allocations
                    .iter()
                    .any(|a| Some(a.stock_id) == repair.focused_sheet)
            {
                ui.small(localizer.text("sheet-no-placements"));
            }
        });
    if let Some(action) = action {
        let moved = match &action {
            RepairAction::Place(id, ..) | RepairAction::Unallocate(id) => Some(*id),
            _ => None,
        };
        let id = match &action {
            RepairAction::Place(..) => A::StageRepair,
            RepairAction::Unallocate(..) => A::Unallocate,
            RepairAction::Lock(..) => A::ToggleAllocationLock,
        };
        let succeeded = actions::contextual(
            Request::new(id),
            if repair.active() {
                Ok(())
            } else {
                Err(Unavailable::NoRepair)
            },
            || match action {
                RepairAction::Place(board, stock, origin, turn) => {
                    repair.stage_placement(editor, board, stock, origin, turn)
                }
                other => stage(repair, editor, other),
            },
        )
        .is_ok_and(|succeeded| succeeded);
        if succeeded && let Some(id) = moved {
            repair.board = None;
            repair.select(
                editor.preview().unwrap_or(editor.project()),
                Some(id),
                locale(localizer),
            );
        }
    }
    if accept
        && actions::contextual(
            Request::new(A::AcceptRepair),
            if repair.active() {
                Ok(())
            } else {
                Err(Unavailable::NoRepair)
            },
            || (),
        )
        .is_ok()
    {
        let _ = repair.accept_navigation(editor);
    }
    if cancel
        && actions::contextual(
            Request::new(A::CancelRepair),
            if repair.active() {
                Ok(())
            } else {
                Err(Unavailable::NoRepair)
            },
            || (),
        )
        .is_ok()
    {
        editor.cancel_preview();
        let focused_sheet = repair.focused_sheet;
        let overlays = repair.overlays;
        let zoom = repair.zoom;
        *repair = RepairUi::default();
        repair.focused_sheet = focused_sheet;
        repair.overlays = overlays;
        repair.zoom = zoom;
    }
    selection_request
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
    fn unused_focus_has_no_inherited_witness_metrics_or_placements() {
        let mut project = fixture();
        let mut spare = project.stock[0].clone();
        spare.id = Uuid::new_v4();
        spare.name = "spare blank".into();
        spare.priority = 1;
        let spare_id = spare.id;
        project.stock.push(spare);
        let snapshot = StockReadModel::build(&project).unwrap();
        assert!(snapshot.sheet_cards()[0].proof.cut_count().is_some());
        assert_eq!(snapshot.sheet_cards()[1].proof, SheetProof::Unused);
        assert_eq!(
            card_utilization(Some(&snapshot.sheet_cards()[1].proof)),
            None
        );

        let mut editor = ProjectEditor::new(project).unwrap();
        let mut repair = RepairUi::default();
        let mut selection = Selection::default();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(
                ui,
                &mut editor,
                &mut selection,
                &Localizer::new(Language::En),
                false,
                &mut repair,
                SheetFocus {
                    sheet: Some(spare_id),
                    issue: None,
                    scroll_to_target: true,
                },
            );
        });
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
            .collect();
        assert_eq!(repair.focused_sheet, Some(spare_id));
        assert!(
            labels
                .iter()
                .any(|text| text.contains("No placements on this piece")),
            "{labels:?}"
        );
        assert!(
            !labels.iter().any(|text| text.contains("Physical cuts")),
            "{labels:?}"
        );
        assert!(
            labels.iter().any(|text| text.contains("Unused")),
            "{labels:?}"
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn hidden_multi_reason_board_has_one_contextual_issue_and_repair_route() {
        let mut project = fixture();
        let hidden = project.boards[0].id;
        project.stock[0].thickness = mm(12);
        project.allocations[1].origin[0] = mm(50);
        let mut selection = Selection::default();
        selection.hidden.insert(hidden);
        assert!(!selection.visible(&project, hidden));
        let issues = allocation_diagnostics::diagnose(&project);
        assert_eq!(issues.iter().filter(|d| d.board_id == hidden).count(), 1);
        let issue = issues.iter().find(|d| d.board_id == hidden).unwrap();
        assert!(issue.reasons.contains(&Reason::Thickness));
        assert!(issue.reasons.contains(&Reason::Overlap));
        assert_eq!(
            needs_stock(&issues)
                .filter(|d| d.board_id == hidden)
                .count(),
            1
        );
        assert_eq!(
            issue_resolution(&project, issue),
            Request::with(A::RepairIssue, Target::Board(hidden))
        );
        // The existing LocateIssue route explicitly reveals without moving the board.
        let pose = project.boards[0].pose;
        selection.choose(Some(hidden), false);
        selection.reveal(&project, hidden);
        assert!(selection.visible(&project, hidden));
        assert_eq!(project.boards[0].pose, pose);
    }

    #[test]
    fn stock_shortage_uses_board_effective_thickness_prefill_but_existing_stock_uses_repair() {
        let mut project = fixture();
        let id = project.boards[0].id;
        project.allocations.retain(|a| a.board_id != id);
        project.stock.clear();
        let entry = allocation_diagnostics::diagnose(&project)
            .into_iter()
            .find(|d| d.board_id == id)
            .unwrap();
        assert_eq!(
            issue_resolution(&project, &entry),
            Request::with(A::AddIssueStock, Target::Board(id))
        );
        let mut spare = fixture().stock.remove(0);
        spare.material_id = project.boards[0].material_id;
        project.stock.push(spare);
        assert_eq!(
            issue_resolution(&project, &entry),
            Request::with(A::RepairIssue, Target::Board(id))
        );
        project.stock[0].width = mm(20);
        assert_eq!(
            issue_resolution(&project, &entry),
            Request::with(A::AddIssueStock, Target::Board(id))
        );
        project.stock[0].width = mm(50);
        project.stock[0].grain = StockGrain::Unknown;
        assert!(needs_new_stock(&project, &entry));
    }

    #[test]
    fn repair_target_and_sheet_label_expose_alias_with_uuid_identity() {
        let project = fixture();
        let id = project.stock[0].id;
        let mut editor = ProjectEditor::new(project).unwrap();
        assert_eq!(
            stock_label(editor.project(), &editor.project().stock[0]),
            format!("O1 · offcut ({})", short_id(id))
        );
        let mut selection = Selection::default();
        selection.choose(Some(editor.project().boards[0].id), false);
        let mut repair = RepairUi::default();
        assert!(repair.begin(&mut editor, &selection, Locale::En));
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(
                ui,
                &mut editor,
                &mut selection,
                &Localizer::new(Language::En),
                false,
                &mut repair,
                SheetFocus::default(),
            );
        });
        let expected = stock_label(editor.project(), &editor.project().stock[0]);
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
            .collect();
        output.drop_without_applying_deltas();
        assert!(
            labels.iter().any(|label| label.contains(&expected)),
            "{labels:?}"
        );
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
        let mut editor = ProjectEditor::new(p).unwrap();
        let before = editor.project().clone();
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
    fn transfer_turn_unallocate_and_lock_require_every_affected_sheet_before_one_undo() {
        let mut project = fixture();
        let first = project.boards[0].id;
        let second = project.boards[1].id;
        let source = project.stock[0].id;
        let mut spare = project.stock[0].clone();
        spare.id = Uuid::new_v4();
        spare.priority = 1;
        spare.width = mm(120);
        spare.grain = StockGrain::AlongY;
        let destination = spare.id;
        project.stock.push(spare);
        let mut editor = ProjectEditor::new(project).unwrap();
        let original = editor.project().clone();
        let mut selection = Selection::default();
        selection.choose(Some(first), false);
        let mut repair = RepairUi::default();
        assert!(repair.begin(&mut editor, &selection, Locale::En));

        stage(&mut repair, &mut editor, RepairAction::Lock(first, true));
        assert!(!stage(
            &mut repair,
            &mut editor,
            RepairAction::Place(first, destination, [Length::ZERO; 2], true),
        ));
        assert_eq!(repair.error, Some("sheet-locked-error"));
        stage(&mut repair, &mut editor, RepairAction::Lock(first, false));
        assert!(stage(
            &mut repair,
            &mut editor,
            RepairAction::Place(first, destination, [Length::ZERO; 2], true),
        ));
        stage(&mut repair, &mut editor, RepairAction::Lock(first, true));
        assert!(stage(
            &mut repair,
            &mut editor,
            RepairAction::Place(second, source, [mm(109), Length::ZERO], false),
        ));
        assert_eq!(editor.project(), &original);
        assert_eq!(repair.affected.as_ref().unwrap().len(), 2);
        assert!(!repair.can_accept(&mut editor));
        assert!(repair.accept_navigation(&mut editor).is_err());
        assert!(repair.active());
        assert_eq!(editor.project(), &original);

        assert!(stage(
            &mut repair,
            &mut editor,
            RepairAction::Unallocate(second)
        ));
        assert!(repair.can_accept(&mut editor));
        repair.accept_navigation(&mut editor).unwrap();
        assert!(!repair.active());
        let placed = editor
            .project()
            .allocations
            .iter()
            .find(|a| a.board_id == first)
            .unwrap();
        assert_eq!(placed.stock_id, destination);
        assert!(placed.quarter_turn && placed.locked);
        assert!(
            !editor
                .project()
                .allocations
                .iter()
                .any(|a| a.board_id == second)
        );
        assert_eq!(editor.project().boards, original.boards);
        editor.undo().unwrap();
        assert_eq!(editor.project().allocations, original.allocations);
    }

    #[test]
    fn pending_numeric_input_and_consent_survive_selection_and_block_navigation_accept() {
        let mut project = fixture();
        let first = project.boards[0].id;
        let other = project.boards[1].id;
        let mut spare = project.stock[0].clone();
        spare.id = Uuid::new_v4();
        spare.priority = 1;
        spare.length = mm(250);
        let destination = spare.id;
        project.stock.push(spare);
        let mut editor = ProjectEditor::new(project).unwrap();
        let original = editor.project().clone();
        let mut selection = Selection::default();
        selection.choose(Some(first), false);
        let mut repair = RepairUi::default();
        assert!(repair.begin(&mut editor, &selection, Locale::En));
        repair.stock = Some(destination);
        repair.origin[0] = "1 1/64 in".into();
        repair.placement_dirty = true;
        assert_eq!(coordinate(&repair.origin[0], Unit::Mm, false), None);
        repair.consent[0] = true;
        let rounded = coordinate(&repair.origin[0], Unit::Mm, true).unwrap();
        assert_eq!(rounded.micrometres(), 25_797);
        selection.choose(Some(other), false);
        repair.select(editor.preview().unwrap(), selection.active, Locale::En);
        assert_eq!(repair.board, Some(first));
        assert_eq!(repair.origin[0], "1 1/64 in");
        assert!(repair.consent[0]);
        assert!(!repair.can_accept(&mut editor));
        assert!(repair.accept_navigation(&mut editor).is_err());
        assert_eq!(editor.project(), &original);
        assert!(repair.stage_placement(
            &mut editor,
            first,
            destination,
            [rounded, Length::ZERO],
            false,
        ));
        assert!(!repair.placement_dirty);
        repair.select(editor.preview().unwrap(), selection.active, Locale::En);
        assert_eq!(repair.board, Some(other));
        assert!(repair.can_accept(&mut editor));
        repair.cancel_navigation(&mut editor);
        assert!(!repair.active());
        assert_eq!(editor.project(), &original);
    }

    #[test]
    fn pointer_accept_is_disabled_during_invalid_stage_and_commits_after_repair() {
        let project = fixture();
        let first = project.boards[0].id;
        let source = project.stock[0].id;
        let mut editor = ProjectEditor::new(project).unwrap();
        let original = editor.project().clone();
        let mut selection = Selection::default();
        selection.choose(Some(first), false);
        let mut repair = RepairUi::default();
        assert!(repair.begin(&mut editor, &selection, Locale::En));
        stage(
            &mut repair,
            &mut editor,
            RepairAction::Place(first, source, [mm(100), Length::ZERO], false),
        );
        assert!(!repair.can_accept(&mut editor));
        let ctx = egui::Context::default();
        let localizer = Localizer::new(Language::En);
        let frame = |events,
                     editor: &mut ProjectEditor,
                     repair: &mut RepairUi,
                     selection: &mut Selection| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 1000.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    show(
                        ui,
                        editor,
                        selection,
                        &localizer,
                        false,
                        repair,
                        SheetFocus::default(),
                    );
                },
            );
            let accept = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Accept repair" => {
                    Some(text.pos + egui::vec2(5.0, 5.0))
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
            accept.expect("repair accept button drawn")
        };
        let mut accept = frame(vec![], &mut editor, &mut repair, &mut selection);
        let click = |at, pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        accept = frame(
            vec![egui::Event::PointerMoved(accept), click(accept, true)],
            &mut editor,
            &mut repair,
            &mut selection,
        );
        frame(
            vec![click(accept, false)],
            &mut editor,
            &mut repair,
            &mut selection,
        );
        assert!(repair.active());
        assert_eq!(editor.project(), &original);
        assert_eq!(editor.preview().unwrap().allocations[0].origin[0], mm(100));

        stage(
            &mut repair,
            &mut editor,
            RepairAction::Unallocate(original.boards[1].id),
        );
        stage(
            &mut repair,
            &mut editor,
            RepairAction::Place(first, source, [Length::ZERO; 2], false),
        );
        assert!(repair.can_accept(&mut editor));
        accept = frame(vec![], &mut editor, &mut repair, &mut selection);
        accept = frame(
            vec![egui::Event::PointerMoved(accept), click(accept, true)],
            &mut editor,
            &mut repair,
            &mut selection,
        );
        frame(
            vec![click(accept, false)],
            &mut editor,
            &mut repair,
            &mut selection,
        );
        assert!(!repair.active());
        assert!(editor.preview().is_none());
        assert!(
            !editor
                .project()
                .allocations
                .iter()
                .any(|a| a.board_id == original.boards[1].id)
        );
        editor.undo().unwrap();
        assert_eq!(editor.project().allocations, original.allocations);
    }

    #[test]
    fn escape_cancels_dirty_numeric_repair_and_captured_unit_keeps_its_meaning() {
        let mut project = fixture();
        project.display_unit = Unit::Inch;
        let mut editor = ProjectEditor::new(project).unwrap();
        let original = editor.project().clone();
        let mut selection = Selection::default();
        selection.choose(Some(original.boards[0].id), false);
        let mut repair = RepairUi::default();
        assert!(repair.begin(&mut editor, &selection, Locale::En));
        repair.entry_unit = Some(numeric_unit(editor.project()));
        repair.origin[0] = "1.5".into();
        repair.placement_dirty = true;
        assert_eq!(
            coordinate(&repair.origin[0], repair.entry_unit.unwrap(), false),
            Some(Length::from_micrometres(38_100))
        );
        editor.set_display_unit(Unit::Mm);
        assert_eq!(repair.entry_unit, Some(Unit::Inch));
        assert_eq!(
            coordinate(&repair.origin[0], repair.entry_unit.unwrap(), false),
            Some(Length::from_micrometres(38_100))
        );
        assert_ne!(
            coordinate(&repair.origin[0], numeric_unit(editor.project()), false),
            Some(Length::from_micrometres(38_100))
        );
        let ctx = egui::Context::default();
        let frame = |events,
                     editor: &mut ProjectEditor,
                     repair: &mut RepairUi,
                     selection: &mut Selection| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 1000.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    show(
                        ui,
                        editor,
                        selection,
                        &Localizer::new(Language::En),
                        false,
                        repair,
                        SheetFocus::default(),
                    );
                },
            );
            output.drop_without_applying_deltas();
        };
        frame(vec![], &mut editor, &mut repair, &mut selection);
        frame(
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut editor,
            &mut repair,
            &mut selection,
        );
        assert!(!repair.active());
        assert!(editor.preview().is_none());
        assert_eq!(editor.project().allocations, original.allocations);
        assert_eq!(editor.project().revision, original.revision);
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
        let mut editor = ProjectEditor::new(project).unwrap();
        let original = editor.project().clone();
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
                            SheetFocus::default(),
                        );
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
        let scale = sheet_scale(&editor.project().stock[0], sheet.width(), 280.0);
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
        let scale = sheet_scale(&project.stock[0], 410.0, 280.0);
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
        choose_sheet_board(&project, &mut selection, id, false);
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
        let project = fixture();
        let output = ctx.run_ui(Default::default(), |ui| {
            let painter = ui.painter();
            paint_allocation(
                painter,
                egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(80.0, 40.0)),
                &project.boards[0],
                &Localizer::new(Language::En),
                SheetOverlays::default(),
                1,
                PartEmphasis {
                    selected: true,
                    in_selection: true,
                    conflict: true,
                },
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
    fn witness_projection_uses_exact_input_span_and_kerf_at_low_zoom() {
        let project = fixture();
        let model = StockReadModel::build(&project).unwrap();
        let (tree, _) = model
            .miniature(project.stock[0].id)
            .unwrap()
            .proof
            .verified()
            .unwrap();
        let operations = tree.operations();
        assert_eq!(operations.len(), tree.cut_count());
        let sheet = egui::Rect::from_min_size(egui::pos2(40.0, 40.0), egui::vec2(205.0, 50.0));
        for (index, operation) in operations.iter().enumerate() {
            assert_eq!(operation.number, index + 1);
            let band = kerf_geometry(tree, operation, sheet.min, 1.0);
            let input = tree_rect(
                sheet.min,
                1.0,
                tree.node(operation.input).unwrap().rectangle,
            );
            let blade = tree.kerf().micrometres() as f32 / 1000.0;
            match operation.axis {
                Axis::X => {
                    assert!((band.width() - blade).abs() < 0.001);
                    assert_eq!((band.top(), band.bottom()), (input.top(), input.bottom()));
                }
                Axis::Y => {
                    assert!((band.height() - blade).abs() < 0.001);
                    assert_eq!((band.left(), band.right()), (input.left(), input.right()));
                }
            }
        }
        let ctx = egui::Context::default();
        let output = ctx.run_ui(Default::default(), |ui| {
            let overlays = SheetOverlays::default();
            paint_witness(ui.painter(), sheet, tree, 0.1, overlays, None);
            paint_allocation(
                ui.painter(),
                egui::Rect::from_min_size(sheet.min, egui::vec2(8.0, 5.0)),
                &project.boards[0],
                &Localizer::new(Language::En),
                overlays,
                1,
                PartEmphasis {
                    selected: false,
                    in_selection: false,
                    conflict: false,
                },
            );
        });
        let markers = output.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Circle(circle) if circle.fill == egui::Color32::from_rgb(109, 58, 31))).count();
        assert_eq!(markers, operations.len());
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::LineSegment { stroke, .. } if stroke.color == egui::Color32::from_rgb(174, 95, 55) && stroke.width >= 1.0)));
        output.drop_without_applying_deltas();
    }

    #[test]
    fn reference_inspector_accounts_for_witness_and_names_real_cut_outputs() {
        use plan_my_cabinet::reference_fixture::{self, WHITE_STOCK_ID};
        let project = reference_fixture::project();
        let model = StockReadModel::build(&project).unwrap();
        let piece = model.miniature(WHITE_STOCK_ID).unwrap();
        let (tree, accounting) = piece.proof.verified().unwrap();
        let localizer = Localizer::new(Language::En);
        assert_eq!(tree.cut_count(), 9);
        assert_eq!(
            accounting.root_area,
            accounting.part_area
                + accounting.offcut_area
                + accounting.waste_area
                + accounting.kerf_loss
                + accounting.trim_loss
        );
        let crosscut = tree
            .operations()
            .into_iter()
            .find(|cut| cut.axis == Axis::Y && cut.input != tree.root())
            .unwrap();
        let row = cut_row(WHITE_STOCK_ID, tree, &crosscut, &localizer, false);
        assert!(row.contains(&format!("Input piece: #{}", crosscut.input)));
        assert!(row.contains(&witness_piece_uuid(WHITE_STOCK_ID, crosscut.input).to_string()));
        assert!(
            row.contains(&witness_piece_uuid(WHITE_STOCK_ID, crosscut.outputs.first).to_string())
        );
        assert!(row.contains(&format!("Low-side output: #{}", crosscut.outputs.first)));
        assert!(row.contains(&format!("High-side output: #{}", crosscut.outputs.second)));
        assert!(row.contains("Reference edge:") && row.contains("Kerf side:"));
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run_ui(Default::default(), |ui| {
            sheet_inspector(ui, Some(piece), &localizer, false);
        });
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
            .collect();
        for expected in [
            "MDF White",
            "18.000 mm",
            "BRL 289.90",
            "Recoverable offcut area",
            "Kerf loss",
            "Trim loss",
            "#1",
            &crosscut.input.to_string(),
        ] {
            assert!(
                labels.iter().any(|label| label.contains(expected)),
                "missing {expected}: {labels:?}"
            );
        }
        output.drop_without_applying_deltas();
    }

    #[test]
    fn trim_loss_is_separate_and_unproved_states_have_no_numeric_metrics() {
        let mut project = fixture();
        project.stock[0].length = mm(210);
        project.stock[0].trim = [mm(5), Length::ZERO, Length::ZERO, Length::ZERO];
        project.allocations[0].origin[0] = mm(5);
        project.allocations[1].origin[0] = mm(110);
        let model = StockReadModel::build(&project).unwrap();
        let mut piece = model.pieces[0].clone();
        let (tree, accounting) = piece.proof.verified().unwrap();
        assert_eq!(tree.operations()[0].reference_edge, Edge::High);
        assert_eq!(accounting.trim_loss, 5_000 * 50_000);
        assert_eq!(accounting.kerf_loss, 5_000 * 50_000);
        assert_eq!(area_label(accounting.trim_loss), "0.000250 m²");
        let localizer = Localizer::new(Language::En);
        assert!(
            cut_row(piece.id, tree, &tree.operations()[0], &localizer, true).contains("trim pass")
        );
        for proof in [
            SheetProof::SearchExhausted,
            SheetProof::Violation(ReconstructionViolation::NoSlicing),
        ] {
            piece.proof = proof.clone();
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let output = ctx.run_ui(Default::default(), |ui| {
                sheet_inspector(ui, Some(&piece), &localizer, false);
            });
            let labels: Vec<_> = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
                .collect();
            assert!(
                labels.iter().any(|label| label.contains(
                    if matches!(proof, SheetProof::SearchExhausted) {
                        "search limit"
                    } else {
                        "Cutting rule violation"
                    }
                )),
                "{labels:?}"
            );
            assert!(
                labels
                    .iter()
                    .any(|label| label.contains("Metrics unverified"))
            );
            assert!(
                !labels
                    .iter()
                    .any(|label| label.contains("Physical cuts:") || label.contains("Kerf loss:"))
            );
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn hovering_sequence_row_highlights_only_its_exact_band_without_selecting() {
        use plan_my_cabinet::reference_fixture::{self, WHITE_STOCK_ID};
        let project = reference_fixture::project();
        let model = StockReadModel::build(&project).unwrap();
        let piece = model.miniature(WHITE_STOCK_ID).unwrap();
        let tree = piece.proof.verified().unwrap().0;
        let localizer = Localizer::new(Language::En);
        let ctx = egui::Context::default();
        let input = |events| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(2000.0, 1600.0),
            )),
            events,
            ..Default::default()
        };
        let first = ctx.run_ui(input(vec![]), |ui| {
            sheet_inspector(ui, Some(piece), &localizer, false);
        });
        let pointer = first
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().starts_with("#2") => {
                    Some(text.pos + egui::vec2(5.0, 5.0))
                }
                _ => None,
            })
            .expect("second row rendered");
        first.drop_without_applying_deltas();
        let sheet = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(275.0, 183.0));
        let mut hovered = None;
        let second = ctx.run_ui(input(vec![egui::Event::PointerMoved(pointer)]), |ui| {
            hovered = sheet_inspector(ui, Some(piece), &localizer, false);
            paint_witness(
                ui.painter(),
                sheet,
                tree,
                0.1,
                SheetOverlays::default(),
                hovered,
            );
        });
        assert_eq!(hovered, Some(2));
        let cut = &tree.operations()[1];
        let band = kerf_geometry(tree, cut, sheet.min, 0.1);
        assert!(second.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(rect) if rect.rect == band && rect.fill == egui::Color32::from_rgb(25, 85, 190).linear_multiply(0.9))));
        assert_eq!(project, reference_fixture::project());
        second.drop_without_applying_deltas();
    }

    #[test]
    fn reference_layout_keeps_hovered_sequence_and_matching_marker_in_one_visible_frame() {
        use plan_my_cabinet::reference_fixture::{self, WHITE_STOCK_ID};
        let mut editor = ProjectEditor::new(reference_fixture::project()).unwrap();
        let original = editor.project().clone();
        let model = StockReadModel::build(editor.project()).unwrap();
        let tree = model
            .miniature(WHITE_STOCK_ID)
            .unwrap()
            .proof
            .verified()
            .unwrap()
            .0;
        let mut repair = RepairUi::default();
        let mut selection = Selection::default();
        let localizer = Localizer::new(Language::En);
        let ctx = egui::Context::default();
        let canvas_width = crate::workspace_shell::PaneLayout::for_width(
            crate::workspace_state::Workspace::CutPlan,
            1440.0 - crate::workspace_shell::RAIL_WIDTH,
        )
        .canvas;
        assert!(canvas_width >= SHEET_SIDE_BY_SIDE_MIN);
        let frame = |events,
                     editor: &mut ProjectEditor,
                     repair: &mut RepairUi,
                     selection: &mut Selection| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(
                            canvas_width,
                            900.0
                                - crate::workspace_shell::HEADER_HEIGHT
                                - crate::workspace_shell::STATUS_HEIGHT,
                        ),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            show(
                                ui,
                                editor,
                                selection,
                                &localizer,
                                false,
                                repair,
                                SheetFocus::default(),
                            );
                        },
                    );
                },
            )
        };
        let first = frame(vec![], &mut editor, &mut repair, &mut selection);
        let row = first
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.text().starts_with("#1")
                        && text.galley.text().contains("Input piece")
                        && shape.clip_rect.contains(text.pos + egui::vec2(5.0, 5.0)) =>
                {
                    Some(text.pos + egui::vec2(5.0, 5.0))
                }
                _ => None,
            })
            .expect("first sequence row visible at reference size");
        let marker_visible = |shapes: &[egui::epaint::ClippedShape], color| {
            shapes.iter().any(|shape| match &shape.shape {
                egui::Shape::Circle(circle) => {
                    circle.fill == color && shape.clip_rect.contains(circle.center)
                }
                _ => false,
            })
        };
        assert!(marker_visible(
            &first.shapes,
            egui::Color32::from_rgb(109, 58, 31)
        ));
        first.drop_without_applying_deltas();
        let second = frame(
            vec![egui::Event::PointerMoved(row)],
            &mut editor,
            &mut repair,
            &mut selection,
        );
        let marker = marker_visible(&second.shapes, egui::Color32::from_rgb(25, 85, 190));
        let cut = tree.operations()[0];
        let band = second.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(rect) if rect.fill == egui::Color32::from_rgb(25, 85, 190).linear_multiply(0.9)
                && shape.clip_rect.intersects(rect.rect) && cut.number == 1));
        assert_eq!(selection.active, None);
        assert_eq!(editor.project(), &original);
        second.drop_without_applying_deltas();
        assert!(
            marker && band,
            "row {row:?}; expected matching cut marker and band in visible canvas"
        );

        let wheel = frame(
            vec![
                egui::Event::PointerMoved(row),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -1500.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            &mut editor,
            &mut repair,
            &mut selection,
        );
        wheel.drop_without_applying_deltas();
        let mut last_row = None;
        for _ in 0..20 {
            let scrolled = frame(
                vec![
                    egui::Event::PointerMoved(egui::pos2(canvas_width - 80.0, 600.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -400.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                &mut editor,
                &mut repair,
                &mut selection,
            );
            last_row = scrolled.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.text().starts_with("#9")
                        && text.galley.text().contains("Input piece")
                        && shape.clip_rect.contains(text.pos + egui::vec2(5.0, 5.0)) =>
                {
                    Some(text.pos + egui::vec2(5.0, 5.0))
                }
                _ => None,
            });
            scrolled.drop_without_applying_deltas();
            if last_row.is_some() {
                break;
            }
        }
        let last_row = last_row.expect("last row reachable by scrolling only the inspector");
        let last = frame(
            vec![egui::Event::PointerMoved(last_row)],
            &mut editor,
            &mut repair,
            &mut selection,
        );
        let linked = marker_visible(&last.shapes, egui::Color32::from_rgb(25, 85, 190));
        last.drop_without_applying_deltas();
        assert!(
            linked,
            "last witness row highlights a still-visible canvas marker"
        );
    }

    #[test]
    fn host_inspector_hover_links_next_frame_without_shrinking_canvas_or_rebuilding_witness() {
        use plan_my_cabinet::reference_fixture::{self, WHITE_STOCK_ID};
        let mut editor = ProjectEditor::new(reference_fixture::project()).unwrap();
        let original = editor.project().clone();
        let mut selection = Selection::default();
        let mut repair = RepairUi::default();
        let ctx = egui::Context::default();
        let localizer = Localizer::new(Language::En);
        let width = crate::workspace_shell::PaneLayout::for_width(
            crate::workspace_state::Workspace::CutPlan,
            1440.0 - crate::workspace_shell::RAIL_WIDTH,
        )
        .canvas;
        assert_eq!(
            sheet_panes(egui::Pos2::ZERO, width, 400.0, true)
                .canvas
                .width(),
            width
        );
        let frame = |events,
                     editor: &mut ProjectEditor,
                     repair: &mut RepairUi,
                     selection: &mut Selection| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, 828.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                show_with_layout(
                                    ui,
                                    editor,
                                    selection,
                                    &localizer,
                                    false,
                                    repair,
                                    SheetFocus::default(),
                                    true,
                                );
                            },
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(SHEET_INSPECTOR_WIDTH, 828.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt("host-cut-inspector")
                                    .show(ui, |ui| {
                                        show_focused_inspector(ui, editor, &localizer, repair);
                                    });
                            },
                        );
                    });
                },
            )
        };
        let first = frame(vec![], &mut editor, &mut repair, &mut selection);
        let row = first.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if text.galley.text().starts_with("#1")
                    && text.pos.x >= width
                    && shape.clip_rect.contains(text.pos + egui::vec2(5.0, 5.0)) =>
            {
                Some(text.pos + egui::vec2(5.0, 5.0))
            }
            _ => None,
        });
        let inspector_headings: Vec<_> = first
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().contains("SHEET INSPECTOR") => {
                    Some(text.pos.x)
                }
                _ => None,
            })
            .collect();
        first.drop_without_applying_deltas();
        let row = row.expect("host sequence row is visible beside the canvas");
        assert_eq!(inspector_headings.len(), 1);
        assert!(
            inspector_headings[0] >= width,
            "inspector renders only in the host pane"
        );
        let key = repair.stock_model_cache.as_ref().unwrap().0.clone();
        let cached_model = repair
            .stock_model_cache
            .as_ref()
            .unwrap()
            .1
            .as_ref()
            .unwrap() as *const StockReadModel;
        let second = frame(
            vec![egui::Event::PointerMoved(row)],
            &mut editor,
            &mut repair,
            &mut selection,
        );
        second.drop_without_applying_deltas();
        assert_eq!(repair.hovered_cut, Some((WHITE_STOCK_ID, 1)));
        let third = frame(vec![], &mut editor, &mut repair, &mut selection);
        let band = third.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(rect) if rect.fill == egui::Color32::from_rgb(25, 85, 190).linear_multiply(0.9)
                && shape.clip_rect.intersects(rect.rect)));
        third.drop_without_applying_deltas();
        assert!(
            band,
            "host row hover highlights the corresponding full-span band on the next frame"
        );
        assert_eq!(repair.stock_model_cache.as_ref().unwrap().0, key);
        assert_eq!(
            repair
                .stock_model_cache
                .as_ref()
                .unwrap()
                .1
                .as_ref()
                .unwrap() as *const StockReadModel,
            cached_model
        );
        assert_eq!(selection.active, None);
        assert_eq!(editor.project(), &original);
        repair.focused_sheet = Some(reference_fixture::OAK_STOCK_ID);
        let other = frame(vec![], &mut editor, &mut repair, &mut selection);
        let stale_band = other.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(rect) if rect.fill == egui::Color32::from_rgb(25, 85, 190).linear_multiply(0.9)));
        other.drop_without_applying_deltas();
        assert!(!stale_band, "a cut hover cannot leak to another sheet");
        assert!(
            repair
                .hovered_cut
                .is_none_or(|(id, _)| id == reference_fixture::OAK_STOCK_ID)
        );
    }

    #[test]
    fn focused_sheet_fits_available_pane_and_low_zoom_uses_identity_callouts() {
        let project = plan_my_cabinet::reference_fixture::project();
        let stock = project
            .stock
            .iter()
            .find(|s| s.id == plan_my_cabinet::reference_fixture::WHITE_STOCK_ID)
            .unwrap();
        for (width, height) in [(780.0, 565.0), (420.0, 320.0)] {
            let layout = sheet_panes(egui::pos2(340.0, 180.0), width, height, true);
            let scale = sheet_scale(
                stock,
                layout.canvas.width() - 72.0,
                layout.canvas.height() - 115.0,
            );
            let drawing = egui::vec2(
                stock.length.micrometres() as f32 / 1000.0 * scale,
                stock.width.micrometres() as f32 / 1000.0 * scale,
            );
            assert!(drawing.x + 68.0 <= layout.canvas.width() + 1.0);
            assert!(drawing.y + 56.0 + 35.0 <= layout.canvas.height() + 1.0);
        }
        assert!(full_part_label_fits(egui::vec2(175.0, 80.0), 62.0, 100.0));
        assert!(!full_part_label_fits(egui::vec2(45.0, 18.0), 62.0, 100.0));
        assert!(ruler_step(0.08) as f32 * 0.08 >= 42.0);
    }

    #[test]
    fn compact_panes_stack_without_overlap_and_keep_both_scroll_targets_reachable() {
        let layout = sheet_panes(egui::pos2(10.0, 20.0), 650.0, 280.0, false);
        assert_eq!(layout.canvas.width(), 650.0);
        assert_eq!(layout.inspector.width(), 650.0);
        assert!(layout.canvas.bottom() < layout.inspector.top());
        assert_eq!(layout.bounds.y, layout.inspector.bottom() - 20.0);
        let wide = sheet_panes(egui::pos2(10.0, 20.0), 1120.0, 300.0, false);
        assert_eq!(wide.inspector.width(), SHEET_INSPECTOR_WIDTH);
        assert!(wide.canvas.right() < wide.inspector.left());

        let project = fixture();
        let mut editor = ProjectEditor::new(project.clone()).unwrap();
        let original = editor.project().clone();
        let mut repair = RepairUi::default();
        let mut selection = Selection::default();
        let ctx = egui::Context::default();
        let frame = |events,
                     editor: &mut ProjectEditor,
                     repair: &mut RepairUi,
                     selection: &mut Selection| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(650.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    show_with_layout(
                        ui,
                        editor,
                        selection,
                        &Localizer::new(Language::En),
                        false,
                        repair,
                        SheetFocus::default(),
                        false,
                    );
                },
            )
        };
        let output = frame(vec![], &mut editor, &mut repair, &mut selection);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(rect) if rect.fill == egui::Color32::from_rgb(233, 221, 195))));
        output.drop_without_applying_deltas();
        let wheel = vec![
            egui::Event::PointerMoved(egui::pos2(630.0, 480.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -400.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        let output = frame(wheel, &mut editor, &mut repair, &mut selection);
        output.drop_without_applying_deltas();
        let output = frame(vec![], &mut editor, &mut repair, &mut selection);
        let inspector_visible = output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text().contains("SHEET INSPECTOR") && shape.clip_rect.contains(text.pos)));
        output.drop_without_applying_deltas();
        assert!(
            inspector_visible,
            "compact stacked inspector must be reachable by workspace scrolling"
        );
        assert_eq!(editor.project(), &original);
    }

    #[test]
    fn toggles_only_filter_verified_projection_and_conflicts_retain_positions() {
        let mut project = fixture();
        let original = project.clone();
        let model = StockReadModel::build(&project).unwrap();
        let tree = model
            .miniature(project.stock[0].id)
            .unwrap()
            .proof
            .verified()
            .unwrap()
            .0;
        let ctx = egui::Context::default();
        let output = ctx.run_ui(Default::default(), |ui| {
            paint_witness(
                ui.painter(),
                egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(205.0, 50.0)),
                tree,
                1.0,
                SheetOverlays {
                    cuts: false,
                    offcuts: false,
                    grain: false,
                    ids: false,
                },
                None,
            );
        });
        assert!(output.shapes.is_empty());
        output.drop_without_applying_deltas();
        assert_eq!(project, original);
        project.allocations[1].origin[0] = mm(102);
        let invalid = StockReadModel::build(&project).unwrap();
        assert!(
            invalid
                .miniature(project.stock[0].id)
                .unwrap()
                .proof
                .verified()
                .is_none()
        );
        let mut editor = ProjectEditor::new(project.clone()).unwrap();
        let before = editor.project().clone();
        let mut repair = RepairUi::default();
        repair.overlays.cuts = false;
        repair.overlays.ids = false;
        let mut selection = Selection::default();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(
                ui,
                &mut editor,
                &mut selection,
                &Localizer::new(Language::En),
                false,
                &mut repair,
                SheetFocus::default(),
            );
        });
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
            .collect();
        let has_conflict_position = labels
            .iter()
            .any(|label| label.contains("X 102.000 mm / Y 0.000 mm") && label.contains('⚠'));
        assert_eq!(editor.project(), &before);
        output.drop_without_applying_deltas();
        assert!(has_conflict_position, "{labels:?}");
    }

    #[test]
    fn only_verified_reusable_leaf_offcuts_are_hatched() {
        let mut project = fixture();
        project.stock[0].length = mm(240);
        let model = StockReadModel::build(&project).unwrap();
        let tree = model
            .miniature(project.stock[0].id)
            .unwrap()
            .proof
            .verified()
            .unwrap()
            .0;
        let offcuts: Vec<_> = tree
            .nodes()
            .iter()
            .filter(|node| node.kind == CutKind::Offcut)
            .collect();
        assert!(!offcuts.is_empty());
        let ctx = egui::Context::default();
        let sheet = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(240.0, 50.0));
        let output = ctx.run_ui(Default::default(), |ui| {
            paint_witness(
                ui.painter(),
                sheet,
                tree,
                1.0,
                SheetOverlays {
                    cuts: false,
                    ..SheetOverlays::default()
                },
                None,
            );
        });
        let filled: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == egui::Color32::from_rgb(232, 239, 226) => {
                    Some(rect.rect)
                }
                _ => None,
            })
            .collect();
        assert_eq!(filled.len(), offcuts.len());
        for node in offcuts {
            assert!(filled.contains(&tree_rect(sheet.min, 1.0, node.rectangle)));
        }
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
                |ui| {
                    show(
                        ui,
                        editor,
                        &mut selection,
                        &localizer,
                        false,
                        &mut repair,
                        SheetFocus::default(),
                    );
                },
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
