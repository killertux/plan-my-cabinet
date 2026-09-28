//! Cut plan workspace. Sheet coordinates are manufacturing micrometres;
//! selection is shared with the 3D viewport, never stored in the project.
use eframe::egui::{self, Color32, Margin, RichText, Stroke};
use plan_my_cabinet::allocation_diagnostics::{self, BoardDiagnostic, Reason, Status};
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::cut_tree::{
    Axis, CutError, CutKind, CutOperation, CutTree, Edge, Reconstruction, ReconstructionViolation,
    WitnessError, reconstruct_witness,
};
use plan_my_cabinet::dimension_input::{Locale, format_length, parse_length};
use plan_my_cabinet::domain::{
    Allocation, Board, BoardGrain, Project, Stock, StockGrain, StockSource,
};
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::material_changes::{ConflictReason, allocation_conflicts};
use plan_my_cabinet::money::MoneyLocale;
use plan_my_cabinet::sheet_edit::SheetEditSession;
use plan_my_cabinet::sheet_packer::{SheetSuggestion, Unplaced, suggest_sheets};
use plan_my_cabinet::stock_read_models::{SheetProof, StockPieceReadModel, StockReadModel};
use plan_my_cabinet::units::{Conversion, Length, Unit};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::actions::{self, ActionId as A, Argument, Request, Target, Unavailable};
use crate::icons::{self, Icon};
use crate::theme::Typeface;
use crate::theme_widgets as tw;
use crate::viewport::Selection;

const WITNESS_BUDGET: usize = 20_000;

// Sheet canvas palette from the handoff (Cut plan screen).
const SHEET_FILL: Color32 = Color32::from_rgb(247, 245, 240);
const SHEET_EDGE: Color32 = Color32::from_rgb(191, 181, 165);
const PART_FILL: Color32 = Color32::from_rgb(233, 227, 215);
const PART_EDGE: Color32 = Color32::from_rgb(156, 144, 126);
const SELECTED_FILL: Color32 = Color32::from_rgb(244, 194, 122);
const SELECTED_INK: Color32 = Color32::from_rgb(62, 35, 5);
const MULTI_FILL: Color32 = Color32::from_rgb(248, 222, 184);
const CONFLICT_FILL: Color32 = Color32::from_rgb(251, 227, 224);
const OFFCUT_FILL: Color32 = Color32::from_rgb(239, 235, 227);
const OFFCUT_HATCH: Color32 = Color32::from_rgb(226, 220, 208);
const CUT_INK: Color32 = Color32::from_rgb(165, 54, 44);
const CUT_BADGE: Color32 = Color32::from_rgb(251, 233, 230);
/// A hovered sequence row paints its band and marker in this solid ink.
const CUT_HIGHLIGHT: Color32 = CUT_INK;
const PURCHASE_INK: Color32 = Color32::from_rgb(122, 68, 16);
const THUMB_ACTIVE_EDGE: Color32 = Color32::from_rgb(176, 106, 28);
const THUMB_ACTIVE_PART: Color32 = Color32::from_rgb(233, 196, 142);
const THUMB_EDGE: Color32 = Color32::from_rgb(207, 199, 185);
const THUMB_PART: Color32 = Color32::from_rgb(216, 195, 166);
const CARD_ACTIVE_INK: Color32 = Color32::from_rgb(138, 84, 24);

fn kerf_band() -> Color32 {
    tw::KERF.gamma_multiply(0.85)
}

// Previews change without advancing the document revision; the editor's
// preview generation tells them apart without comparing their contents.
pub(crate) type DiagnosticsKey = (Uuid, u64, Option<u64>);
type AffectedCache = (DiagnosticsKey, Vec<Uuid>, Vec<(Uuid, &'static str)>);

pub(crate) fn diagnostics_key(project: &Project, preview: Option<u64>) -> DiagnosticsKey {
    (project.id, project.revision, preview)
}

#[derive(Default)]
pub struct RepairUi {
    stock_model_cache: Option<(DiagnosticsKey, Option<StockReadModel>)>,
    board_diagnostics_cache: Option<(DiagnosticsKey, Vec<BoardDiagnostic>)>,
    suggestion_cache: Option<(DiagnosticsKey, Vec<SheetSuggestion>)>,
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
        // The selected part always shows its ID and grain; the toggles add
        // them to every part.
        Self {
            cuts: true,
            offcuts: true,
            grain: false,
            ids: false,
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

    fn color(self) -> Color32 {
        match self {
            Self::Verified => tw::OK,
            Self::Violation(_) => tw::KERF,
            Self::Exhausted => tw::WARN,
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
                self.reset_keeping_view();
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
        self.reset_keeping_view();
    }

    /// Ends a repair session but keeps the sheet the user is looking at.
    fn reset_keeping_view(&mut self) {
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
                .map(|v| trimmed_length(format_length(v, Unit::Mm, locale, 3)));
        } else {
            self.stock = project.ordered_stock().first().map(|s| s.id);
            self.quarter_turn = false;
            self.origin = ["0 mm".into(), "0 mm".into()];
        }
    }

    /// Rebuilds the read model and diagnostics only when manufacturing inputs change.
    fn refresh(&mut self, project: &Project, preview: Option<u64>) {
        let key = diagnostics_key(project, preview);
        if self
            .stock_model_cache
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            self.hovered_cut = None;
            self.stock_model_cache = Some((key, StockReadModel::build(project).ok()));
        }
        if self
            .board_diagnostics_cache
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            let diagnostics = self
                .stock_model_cache
                .as_ref()
                .and_then(|(_, model)| model.as_ref())
                .map(|model| model.boards.clone())
                .unwrap_or_else(|| allocation_diagnostics::diagnose(project));
            self.board_diagnostics_cache = Some((key, diagnostics));
        }
        if self
            .suggestion_cache
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            self.suggestion_cache = Some((key, suggest_sheets(project)));
        }
    }

    fn suggestions(&self) -> &[SheetSuggestion] {
        self.suggestion_cache
            .as_ref()
            .map_or(&[], |(_, suggestions)| suggestions.as_slice())
    }

    /// Empty until the first `refresh`.
    fn board_diagnostics(&self) -> &[BoardDiagnostic] {
        self.board_diagnostics_cache
            .as_ref()
            .map_or(&[], |(_, diagnostics)| diagnostics.as_slice())
    }

    fn model(&self) -> Option<&StockReadModel> {
        self.stock_model_cache
            .as_ref()
            .and_then(|(_, model)| model.as_ref())
    }
}

/// "565.000 mm" -> "565 mm", "12.500 mm" -> "12.5 mm" (same value, fewer zeros).
fn trimmed_length(text: String) -> String {
    let Some((number, unit)) = text.split_once(' ') else {
        return text;
    };
    if !number.contains(['.', ',']) {
        return text;
    }
    let number = number.trim_end_matches('0').trim_end_matches(['.', ',']);
    format!("{number} {unit}")
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
    let input = tree_rect(
        origin,
        scale,
        tree.node(operation.input)
            .expect("operations reference their own tree")
            .rectangle,
    );
    let first = tree_rect(
        origin,
        scale,
        tree.node(operation.outputs.first)
            .expect("operations reference their own tree")
            .rectangle,
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

/// Where a cut's numbered marker sits: near the start of its band, or just
/// past its end when the band is too short to carry a 22px circle.
fn marker_position(
    band: egui::Rect,
    axis: Axis,
    sheet: egui::Rect,
    placed: &[egui::Pos2],
) -> egui::Pos2 {
    let center = band.center();
    let inner = sheet.shrink(11.0);
    let clamp = |p: egui::Pos2| {
        if inner.is_positive() {
            p.clamp(inner.min, inner.max)
        } else {
            sheet.center()
        }
    };
    let length = match axis {
        Axis::X => band.height(),
        Axis::Y => band.width(),
    };
    let at = |offset: f32| match axis {
        Axis::X => egui::pos2(center.x, band.top() + offset),
        Axis::Y => egui::pos2(band.left() + offset, center.y),
    };
    let first = if length >= 30.0 {
        at(14.0)
    } else {
        at(length + 13.0)
    };
    let free = |p: egui::Pos2| placed.iter().all(|q| q.distance(p) >= 23.0);
    // Slide along the band, then past its end, until the circle is clear.
    let mut offset = 14.0;
    while offset + 11.0 <= length {
        let candidate = clamp(at(offset));
        if free(candidate) {
            return candidate;
        }
        offset += 24.0;
    }
    let candidate = clamp(at(length + 13.0));
    if free(candidate) {
        candidate
    } else {
        clamp(first)
    }
}

fn paint_offcut(painter: &egui::Painter, rect: egui::Rect, label: &str) {
    painter.rect_filled(rect, 0.0, OFFCUT_FILL);
    let hatch = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let stroke = Stroke::new(1.0, OFFCUT_HATCH);
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        hatch.line_segment(
            [
                egui::pos2(x, rect.bottom()),
                egui::pos2(x + rect.height(), rect.top()),
            ],
            stroke,
        );
        x += 7.0;
    }
    let font = egui::FontId::monospace(11.0);
    let galley = painter.layout_no_wrap(label.to_owned(), font, tw::FAINT);
    let pill = egui::Rect::from_center_size(rect.center(), galley.size() + egui::vec2(12.0, 2.0));
    if rect.width() >= pill.width() + 6.0 && rect.height() >= pill.height() + 6.0 {
        painter.rect_filled(pill, 4.0, SHEET_FILL);
        painter.galley(pill.min + egui::vec2(6.0, 1.0), galley, tw::FAINT);
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
    let painter = painter.with_clip_rect(sheet.intersect(painter.clip_rect()));
    if overlays.offcuts {
        for node in tree.nodes() {
            if node.kind != CutKind::Offcut {
                continue;
            }
            let rect = tree_rect(sheet.min, scale, node.rectangle).intersect(sheet);
            let label = dims_text(
                node.rectangle.extent[0],
                node.rectangle.extent[1],
                Locale::En,
            );
            paint_offcut(&painter, rect, &label);
        }
    }
    if overlays.cuts {
        let operations = tree.operations();
        for operation in &operations {
            let band = kerf_geometry(tree, operation, sheet.min, scale);
            let highlighted = hovered_cut == Some(operation.number);
            let color = if highlighted {
                CUT_HIGHLIGHT
            } else {
                kerf_band()
            };
            if highlighted {
                painter.rect_filled(band.expand(2.0), 1.0, tw::KERF.gamma_multiply(0.22));
            }
            painter.rect_filled(band, 0.0, color);
            let center = band.center();
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
            // A thin band gets a visible stroke, never a fabricated wider physical band.
            if band.width().min(band.height()) < 1.6 {
                painter.line_segment(
                    endpoints,
                    Stroke::new(if highlighted { 3.0 } else { 1.6 }, color),
                );
            }
        }
        // Markers go on top of every band so crossings never hide a number.
        let mut placed = Vec::with_capacity(operations.len());
        for operation in &operations {
            let band = kerf_geometry(tree, operation, sheet.min, scale);
            let highlighted = hovered_cut == Some(operation.number);
            let marker = marker_position(band, operation.axis, sheet, &placed);
            placed.push(marker);
            painter.circle(
                marker,
                11.0,
                if highlighted {
                    CUT_HIGHLIGHT
                } else {
                    tw::PANEL
                },
                Stroke::new(1.5, if highlighted { CUT_HIGHLIGHT } else { tw::KERF }),
            );
            painter.text(
                marker,
                egui::Align2::CENTER_CENTER,
                format!("C{}", operation.number),
                egui::FontId::monospace(10.0),
                if highlighted { tw::PANEL } else { CUT_INK },
            );
        }
    }
}

/// Mono dimension rulers above (length) and left of (width) the sheet.
fn paint_rulers(painter: &egui::Painter, sheet: egui::Rect, stock: &Stock, locale: Locale) {
    let stroke = Stroke::new(1.0, SHEET_EDGE);
    let font = egui::FontId::monospace(11.0);
    let length = painter.layout_no_wrap(mm_text(stock.length, locale), font.clone(), tw::MUTED);
    let y = sheet.top() - 14.0;
    let half = length.size().x / 2.0 + 6.0;
    let mid = sheet.center().x;
    if sheet.width() > half * 2.0 + 8.0 {
        painter.line_segment(
            [egui::pos2(sheet.left(), y), egui::pos2(mid - half, y)],
            stroke,
        );
        painter.line_segment(
            [egui::pos2(mid + half, y), egui::pos2(sheet.right(), y)],
            stroke,
        );
    }
    painter.galley(
        egui::pos2(mid - length.size().x / 2.0, y - length.size().y / 2.0),
        length,
        tw::MUTED,
    );
    let width = painter.layout_no_wrap(mm_text(stock.width, locale), font, tw::MUTED);
    let x = sheet.left() - 16.0;
    let half = width.size().x / 2.0 + 6.0;
    let mid = sheet.center().y;
    if sheet.height() > half * 2.0 + 8.0 {
        painter.line_segment(
            [egui::pos2(x, sheet.top()), egui::pos2(x, mid - half)],
            stroke,
        );
        painter.line_segment(
            [egui::pos2(x, mid + half), egui::pos2(x, sheet.bottom())],
            stroke,
        );
    }
    let size = width.size();
    painter.add(
        egui::epaint::TextShape::new(
            egui::pos2(x - size.y / 2.0, mid + size.x / 2.0),
            width,
            tw::MUTED,
        )
        .with_angle(-std::f32::consts::FRAC_PI_2),
    );
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

/// Millimetres without a unit and without needless zeros (`764`, `18.5`).
fn mm_text(length: Length, locale: Locale) -> String {
    let um = length.micrometres();
    let sign = if um < 0 { "-" } else { "" };
    let um = um.unsigned_abs();
    let (whole, fraction) = (um / 1000, um % 1000);
    if fraction == 0 {
        return format!("{sign}{whole}");
    }
    let mut digits = format!("{fraction:03}");
    while digits.ends_with('0') {
        digits.pop();
    }
    let separator = if locale == Locale::En { '.' } else { ',' };
    format!("{sign}{whole}{separator}{digits}")
}

fn dims_text(a: Length, b: Length, locale: Locale) -> String {
    format!("{} × {}", mm_text(a, locale), mm_text(b, locale))
}

fn compact_dims(values: &[Length], locale: Locale) -> String {
    values
        .iter()
        .map(|v| mm_text(*v, locale))
        .collect::<Vec<_>>()
        .join("×")
}

fn stock_material(project: &Project, stock: &Stock, locale: Locale) -> String {
    let material = project
        .materials
        .iter()
        .find(|m| m.id == stock.material_id)
        .map(|m| m.name.as_str())
        .unwrap_or("?");
    format!("{material} {}", mm_text(stock.thickness, locale))
}

/// Alias and name, as used in the repair target picker. Never a raw UUID.
fn stock_label(project: &Project, stock: &Stock) -> String {
    format!(
        "{} · {}",
        project.stock_alias(stock.id).unwrap_or("?"),
        stock.name
    )
}

fn board_short_id(board: &Board) -> String {
    crate::assembly_ui::short_id('b', board.id)
}

fn issue_text(localizer: &Localizer, issues: &[Issue]) -> String {
    issues
        .iter()
        .map(|issue| localizer.text(issue.key()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn capitalized(text: String) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
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

/// Short area for stat cards: `3.1 m²`, `0.05 m²` (exact value in the tooltip).
fn area_short(square_micrometres: i128, locale: Locale) -> String {
    let value = square_micrometres as f64 / 1_000_000_000_000.0;
    let mut text = if value >= 1.0 {
        format!("{value:.1}")
    } else if value >= 0.01 || value == 0.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.4}")
    };
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if locale == Locale::PtBr {
        text = text.replace('.', ",");
    }
    format!("{text} m²")
}

/// Short, shop-facing name of a witness node: part name, `offcut 531×560`,
/// or an intermediate piece `P4 2025×560`.
fn node_label(tree: &CutTree, id: usize, project: &Project, localizer: &Localizer) -> String {
    let Some(node) = tree.node(id) else {
        return format!("P{id}");
    };
    let dims = compact_dims(&node.rectangle.extent, locale(localizer));
    match node.kind {
        CutKind::Part(board) => project
            .boards
            .iter()
            .find(|b| b.id == board)
            .map_or_else(|| localizer.text("sheet-part"), |b| b.name.clone()),
        CutKind::Offcut => format!("{} {dims}", localizer.text("sheet-offcut-short")),
        CutKind::Waste => format!("{} {dims}", localizer.text("sheet-waste")),
        CutKind::Split { .. } => format!("P{id} {dims}"),
    }
}

/// "Rip full sheet at Y 560" / "Cross-cut P1 at X 720".
fn cut_operation_text(
    tree: &CutTree,
    cut: &CutOperation,
    trim: bool,
    localizer: &Localizer,
) -> String {
    let mut args = fluent_bundle::FluentArgs::new();
    args.set(
        "piece",
        if cut.input == tree.root() {
            localizer.text("sheet-op-full-sheet")
        } else {
            format!("P{}", cut.input)
        },
    );
    args.set("at", mm_text(cut.retained_extent, locale(localizer)));
    args.set(
        "axis",
        match cut.axis {
            Axis::X => "X",
            Axis::Y => "Y",
        },
    );
    let key = if trim {
        "sheet-op-trim"
    } else if cut.axis == Axis::Y {
        "sheet-op-rip"
    } else {
        "sheet-op-crosscut"
    };
    let mut text = localizer.format(key, Some(&args));
    if cut.reference_edge == Edge::High {
        text.push_str(" · ");
        text.push_str(&localizer.text("sheet-op-far-edge"));
    }
    text
}

/// "P0 → P1 2750×560 · P2 2750×1265" with part names in place of piece IDs.
fn cut_detail_text(
    tree: &CutTree,
    cut: &CutOperation,
    project: &Project,
    localizer: &Localizer,
) -> String {
    format!(
        "P{} → {} · {}",
        cut.input,
        node_label(tree, cut.outputs.first, project, localizer),
        node_label(tree, cut.outputs.second, project, localizer),
    )
}

fn piece_identity(tree: &CutTree, id: usize, localizer: &Localizer) -> String {
    let node = tree.node(id).expect("witness operation references a node");
    let kind = match node.kind {
        CutKind::Part(board) => format!(
            " · {} {}",
            localizer.text("sheet-part"),
            crate::assembly_ui::short_id('b', board)
        ),
        CutKind::Offcut => format!(" · {}", localizer.text("sheet-overlay-offcuts")),
        CutKind::Waste => format!(" · {}", localizer.text("sheet-waste")),
        CutKind::Split { .. } => String::new(),
    };
    format!(
        "P{id}{kind} · {} · X {} / Y {}",
        dims_text(
            node.rectangle.extent[0],
            node.rectangle.extent[1],
            locale(localizer)
        ),
        mm_text(node.rectangle.origin[0], locale(localizer)),
        mm_text(node.rectangle.origin[1], locale(localizer)),
    )
}

/// Full shop detail for one cut; shown as the sequence row's tooltip.
fn cut_row(tree: &CutTree, cut: &CutOperation, localizer: &Localizer, trim: bool) -> String {
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
    [
        format!(
            "C{}{}",
            cut.number,
            if trim {
                format!(" ({})", localizer.text("sheet-trim-pass"))
            } else {
                String::new()
            }
        ),
        format!(
            "{}: {}",
            localizer.text("sheet-input"),
            piece_identity(tree, cut.input, localizer)
        ),
        format!(
            "{}: {} {axis}",
            localizer.text("sheet-reference-edge"),
            edge(cut.reference_edge)
        ),
        format!(
            "{}: {}",
            localizer.text("sheet-retained-distance"),
            format_length(cut.retained_extent, Unit::Mm, locale(localizer), 3)
        ),
        format!(
            "{}: {}",
            localizer.text("sheet-kerf-side"),
            edge(cut.kerf_side)
        ),
        format!(
            "{}: {}",
            localizer.text("sheet-retained-output"),
            piece_identity(tree, cut.retained_output, localizer)
        ),
        format!(
            "{}: {}",
            localizer.text("sheet-first-output"),
            piece_identity(tree, cut.outputs.first, localizer)
        ),
        format!(
            "{}: {}",
            localizer.text("sheet-second-output"),
            piece_identity(tree, cut.outputs.second, localizer)
        ),
    ]
    .join("\n")
}

fn source_chip(ui: &mut egui::Ui, source: StockSource, localizer: &Localizer) {
    match source {
        StockSource::Owned => tw::chip(ui, &localizer.text("stock-owned"), tw::OK_BG, tw::OK_INK),
        StockSource::ToPurchase => tw::chip(
            ui,
            &localizer.text("stock-purchase"),
            tw::ACCENT_BG,
            PURCHASE_INK,
        ),
    };
}

/// "MDF White 18 · grain along X · no trims · BRL 289.90"
fn sheet_subline(piece: &StockPieceReadModel, localizer: &Localizer) -> String {
    let locale = locale(localizer);
    let grain = localizer.text(match piece.grain {
        StockGrain::AlongX => "sheet-grain-x",
        StockGrain::AlongY => "sheet-grain-y",
        StockGrain::Nondirectional => "sheet-grain-none",
        StockGrain::Unknown => "sheet-grain-unknown",
    });
    let trims = if piece.trim.iter().all(|t| *t == Length::ZERO) {
        localizer.text("sheet-no-trims")
    } else {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("values", piece.trim.map(|v| mm_text(v, locale)).join(" / "));
        localizer.format("sheet-trims-short", Some(&args))
    };
    let price = piece.price.map_or_else(
        || localizer.text("stock-price-unknown"),
        |price| {
            price.display(if localizer.language() == Language::En {
                MoneyLocale::English
            } else {
                MoneyLocale::PortugueseBrazil
            })
        },
    );
    format!(
        "{} {} · {grain} · {trims} · {price}",
        piece.material_name,
        mm_text(piece.measured_thickness, locale)
    )
}

fn inspector_header(ui: &mut egui::Ui, piece: &StockPieceReadModel, localizer: &Localizer) {
    egui::Frame::new()
        .inner_margin(Margin {
            left: 14,
            right: 14,
            top: 14,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add(icons::icon(Icon::Sheet, tw::ACCENT, 15.0));
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("alias", piece.alias.as_str());
                ui.label(
                    tw::semibold(ui, localizer.format("sheet-title", Some(&args)), 15.0)
                        .color(tw::TEXT),
                )
                .on_hover_text(&piece.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    source_chip(ui, piece.source, localizer);
                });
            });
            ui.add(
                egui::Label::new(
                    RichText::new(sheet_subline(piece, localizer))
                        .size(11.0)
                        .color(tw::MUTED),
                )
                .wrap(),
            )
            .on_hover_text(localizer.text("sheet-trims"));
        });
    tw::divider(ui);
}

fn stat_card(ui: &mut egui::Ui, width: f32, label: &str, value: &str, tooltip: Option<String>) {
    let response = egui::Frame::new()
        .fill(tw::APP)
        .corner_radius(7)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width((width - 20.0).max(10.0));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.add(
                    egui::Label::new(RichText::new(label).size(11.0).color(tw::MUTED)).truncate(),
                );
                ui.add(
                    egui::Label::new(tw::mono(value, 15.0).color(tw::TEXT))
                        .wrap_mode(egui::TextWrapMode::Extend),
                );
            });
        })
        .response;
    if let Some(tooltip) = tooltip {
        response.on_hover_text(tooltip);
    }
}

fn stat_grid(ui: &mut egui::Ui, cards: [(String, String, Option<String>); 4]) {
    egui::Frame::new()
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            let gap = 6.0;
            let width = ((ui.available_width() - gap) / 2.0).floor();
            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
            let [a, b, c, d] = cards;
            for row in [[a, b], [c, d]] {
                ui.horizontal(|ui| {
                    for (label, value, tooltip) in row {
                        stat_card(ui, width, &label, &value, tooltip);
                    }
                });
            }
        });
}

fn unverified_notice(ui: &mut egui::Ui, message: String, localizer: &Localizer) {
    egui::Frame::new()
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            tw::warn_callout().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.add(icons::icon(Icon::Warning, tw::WARN, 14.0));
                    ui.add(
                        egui::Label::new(RichText::new(message).size(12.0).color(tw::WARN_INK))
                            .wrap(),
                    );
                });
            });
            ui.add_space(6.0);
            ui.label(
                RichText::new(localizer.text("sheet-unverified"))
                    .size(11.5)
                    .color(tw::FAINT),
            );
        });
}

/// One sequence row: badge, operation and mono detail. Returns whether the
/// pointer is over it (the canvas highlights that cut).
fn sequence_row(
    ui: &mut egui::Ui,
    tree: &CutTree,
    cut: &CutOperation,
    trim: bool,
    project: &Project,
    localizer: &Localizer,
    highlighted: bool,
) -> bool {
    let background = ui.painter().add(egui::Shape::Noop);
    let inner = egui::Frame::new()
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (badge, _) =
                    ui.allocate_exact_size(egui::vec2(24.0, 20.0), egui::Sense::hover());
                ui.painter().rect_filled(badge, 5.0, CUT_BADGE);
                ui.painter().text(
                    badge.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("C{}", cut.number),
                    tw::weighted_font(ui, 10.5, Typeface::MonoSemibold),
                    CUT_INK,
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.add(
                        egui::Label::new(
                            RichText::new(cut_operation_text(tree, cut, trim, localizer))
                                .size(12.5)
                                .color(tw::TEXT),
                        )
                        .wrap()
                        .selectable(false),
                    );
                    ui.add(
                        egui::Label::new(
                            tw::mono(cut_detail_text(tree, cut, project, localizer), 10.5)
                                .color(tw::FAINT),
                        )
                        .wrap()
                        .selectable(false),
                    );
                });
            });
        });
    let rect = inner.response.rect;
    let hovered = ui.rect_contains_pointer(rect);
    ui.interact(
        rect,
        ui.id().with(("cut-row", cut.number)),
        egui::Sense::hover(),
    )
    .on_hover_text(cut_row(tree, cut, localizer, trim));
    if hovered || highlighted {
        ui.painter().set(
            background,
            egui::epaint::RectShape::filled(rect, 6.0, tw::HOVER_ROW),
        );
    }
    hovered
}

fn sequence_rows(
    ui: &mut egui::Ui,
    tree: &CutTree,
    piece: &StockPieceReadModel,
    project: &Project,
    localizer: &Localizer,
    highlighted: Option<usize>,
) -> Option<usize> {
    let trim_count = piece.trim.iter().filter(|v| **v != Length::ZERO).count();
    let mut hovered = None;
    egui::Frame::new()
        .inner_margin(Margin {
            left: 6,
            right: 6,
            top: 0,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            for cut in tree.operations() {
                if sequence_row(
                    ui,
                    tree,
                    &cut,
                    cut.number <= trim_count,
                    project,
                    localizer,
                    highlighted == Some(cut.number),
                ) {
                    hovered = Some(cut.number);
                }
            }
            if tree.cut_count() == 0 {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(localizer.text("sheet-no-cuts"))
                        .size(12.0)
                        .color(tw::MUTED),
                );
            }
        });
    hovered
}

/// Sheet header, stats and the physical cut sequence. Rows and canvas consume
/// the same cached witness; only pointer state is returned.
fn sheet_inspector(
    ui: &mut egui::Ui,
    piece: Option<&StockPieceReadModel>,
    project: &Project,
    localizer: &Localizer,
    highlighted: Option<usize>,
    reserve: Option<f32>,
) -> Option<usize> {
    let Some(piece) = piece else {
        // Only reached without any sheet; the canvas empty state explains it.
        return None;
    };
    inspector_header(ui, piece, localizer);
    let locale = locale(localizer);
    match &piece.proof {
        SheetProof::Verified { tree, accounting } => {
            let percent = accounting.part_area as f64 * 100.0 / accounting.root_area as f64;
            stat_grid(
                ui,
                [
                    (
                        localizer.text("sheet-utilization"),
                        format!("{}%", card_utilization(Some(&piece.proof)).unwrap_or(0)),
                        Some(format!("{percent:.1}%")),
                    ),
                    (
                        localizer.text("sheet-physical-cuts"),
                        tree.cut_count().to_string(),
                        None,
                    ),
                    (
                        localizer.text("sheet-reusable-offcuts"),
                        area_short(accounting.offcut_area, locale),
                        Some(format!(
                            "{}: {}",
                            localizer.text("sheet-recoverable-area"),
                            area_label(accounting.offcut_area)
                        )),
                    ),
                    (
                        localizer.text("sheet-kerf-loss-short"),
                        area_short(accounting.kerf_loss, locale),
                        Some(format!(
                            "{}: {}",
                            localizer.text("sheet-kerf-loss"),
                            area_label(accounting.kerf_loss)
                        )),
                    ),
                ],
            );
            let extra: Vec<_> = [
                (
                    "sheet-trim-loss-short",
                    "sheet-trim-loss",
                    accounting.trim_loss,
                ),
                (
                    "sheet-waste-short",
                    "sheet-other-waste",
                    accounting.waste_area,
                ),
            ]
            .into_iter()
            .filter(|(_, _, area)| *area != 0)
            .collect();
            if !extra.is_empty() {
                egui::Frame::new()
                    .inner_margin(Margin {
                        left: 14,
                        right: 14,
                        top: 0,
                        bottom: 6,
                    })
                    .show(ui, |ui| {
                        let text = extra
                            .iter()
                            .map(|(key, _, area)| {
                                format!("{} {}", localizer.text(key), area_short(*area, locale))
                            })
                            .collect::<Vec<_>>()
                            .join(" · ");
                        let tooltip = extra
                            .iter()
                            .map(|(_, long, area)| {
                                format!("{}: {}", localizer.text(long), area_label(*area))
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        ui.add(
                            egui::Label::new(RichText::new(text).size(11.5).color(tw::MUTED))
                                .wrap(),
                        )
                        .on_hover_text(tooltip);
                    });
            }
            egui::Frame::new()
                .inner_margin(Margin::symmetric(14, 0))
                .show(ui, |ui| {
                    tw::inspector_heading(ui, &localizer.text("sheet-sequence-heading"), |ui| {
                        if tree.cut_count() > 0 {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            ui.label(
                                RichText::new(localizer.text("sheet-verified-full-span"))
                                    .size(11.0)
                                    .color(tw::OK),
                            );
                            ui.add(icons::icon(Icon::Check, tw::OK, 12.0));
                        }
                    });
                });
            let rows =
                |ui: &mut egui::Ui| sequence_rows(ui, tree, piece, project, localizer, highlighted);
            let budget = reserve
                .map(|reserve| ui.clip_rect().bottom() - ui.cursor().top() - reserve)
                .filter(|budget| *budget >= 140.0);
            match budget {
                Some(budget) => {
                    egui::ScrollArea::vertical()
                        .id_salt(("sheet-sequence-rows", piece.id))
                        .max_height(budget)
                        .auto_shrink([false, true])
                        .show(ui, rows)
                        .inner
                }
                None => rows(ui),
            }
        }
        SheetProof::Unused => {
            egui::Frame::new()
                .inner_margin(Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(localizer.text("sheet-unused"))
                            .size(12.5)
                            .color(tw::SECONDARY),
                    );
                    ui.label(
                        RichText::new(localizer.text("sheet-unverified"))
                            .size(11.5)
                            .color(tw::FAINT),
                    );
                });
            None
        }
        SheetProof::SearchExhausted => {
            unverified_notice(
                ui,
                capitalized(localizer.text("sheet-feasibility-unknown")),
                localizer,
            );
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
            unverified_notice(
                ui,
                format!(
                    "{}: {}",
                    localizer.text("sheet-violation"),
                    localizer.text(key)
                ),
                localizer,
            );
            None
        }
    }
}

const SHEET_INSPECTOR_WIDTH: f32 = 308.0;
const SHEET_PANE_GAP: f32 = 12.0;
const SHEET_SIDE_BY_SIDE_MIN: f32 = 760.0;
/// Space under the sheet for the one-line legend.
const LEGEND_HEIGHT: f32 = 34.0;
/// Canvas padding around the sheet, plus room for the dimension rulers.
const SHEET_PADDING: f32 = 24.0;
const RULER_ROOM: f32 = 22.0;

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

fn paint_dashed_rect(
    painter: &egui::Painter,
    rect: egui::Rect,
    stroke: Stroke,
    dash: f32,
    gap: f32,
) {
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    painter.extend(egui::Shape::dashed_line(&corners, stroke, dash, gap));
}

fn paint_thumbnail(
    painter: &egui::Painter,
    region: egui::Rect,
    project: &Project,
    stock: &Stock,
    selected: bool,
) {
    let parts: Vec<_> = project
        .allocations
        .iter()
        .filter(|a| a.stock_id == stock.id)
        .collect();
    if parts.is_empty() {
        // Dashed outline means inventory without a layout, never a proved blank.
        let blank = egui::Rect::from_center_size(region.center(), egui::vec2(30.0, 20.0));
        painter.rect_filled(blank, 0.0, tw::CARD);
        paint_dashed_rect(
            painter,
            blank.shrink(0.5),
            Stroke::new(1.0, SHEET_EDGE),
            3.0,
            2.0,
        );
        return;
    }
    let (edge, fill) = if selected {
        (THUMB_ACTIVE_EDGE, THUMB_ACTIVE_PART)
    } else {
        (THUMB_EDGE, THUMB_PART)
    };
    painter.rect_filled(region, 0.0, tw::CARD);
    let clipped = painter.with_clip_rect(region.intersect(painter.clip_rect()));
    let sx = region.width() / stock.length.micrometres().max(1) as f32;
    let sy = region.height() / stock.width.micrometres().max(1) as f32;
    for allocation in parts {
        if let Some([x, y, length, width]) = footprint(project, allocation) {
            let rect = egui::Rect::from_min_max(
                region.min + egui::vec2(x as f32 * sx, y as f32 * sy),
                region.min + egui::vec2((x + length) as f32 * sx, (y + width) as f32 * sy),
            );
            clipped.rect_filled(rect.shrink(0.5), 0.0, fill);
        }
    }
    painter.rect_stroke(
        region,
        0.0,
        Stroke::new(1.0, edge),
        egui::StrokeKind::Inside,
    );
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

/// Paints one allocated part with its label. `grain` is the drawn grain
/// direction (if restricted); `ui` supplies fonts and icon painting and its
/// painter must already be clipped to the sheet.
fn paint_allocation(
    ui: &egui::Ui,
    region: egui::Rect,
    board: &Board,
    localizer: &Localizer,
    overlays: SheetOverlays,
    grain: Option<egui::Vec2>,
    emphasis: PartEmphasis,
) {
    let painter = ui.painter();
    let PartEmphasis {
        selected,
        in_selection,
        conflict,
    } = emphasis;
    let fill = if conflict {
        CONFLICT_FILL
    } else if selected {
        SELECTED_FILL
    } else if in_selection {
        MULTI_FILL
    } else {
        PART_FILL
    };
    painter.rect_filled(region, 0.0, fill);
    if selected {
        painter.rect_stroke(
            region,
            0.0,
            Stroke::new(2.0, tw::ACCENT_DARK),
            egui::StrokeKind::Inside,
        );
    } else if !conflict {
        painter.rect_stroke(
            region,
            0.0,
            Stroke::new(
                1.0,
                if in_selection {
                    tw::ACCENT_DARK
                } else {
                    PART_EDGE
                },
            ),
            egui::StrokeKind::Inside,
        );
    }
    if conflict {
        // Dashed kerf outline keeps the recorded position readable.
        let inset = if selected { 2.5 } else { 0.5 };
        paint_dashed_rect(
            painter,
            region.shrink(inset),
            Stroke::new(1.0, tw::KERF),
            4.0,
            3.0,
        );
    }
    let locale = locale(localizer);
    let dimensions = dims_text(board.length, board.width, locale);
    let (name_color, dims_color) = if selected {
        (SELECTED_INK, tw::ACCENT_INK)
    } else {
        (tw::TEXT, tw::MUTED)
    };
    let name_font = tw::weighted_font(
        ui,
        if selected { 13.5 } else { 12.5 },
        if selected {
            Typeface::SansSemibold
        } else {
            Typeface::SansMedium
        },
    );
    let dims_font = egui::FontId::monospace(if selected { 11.5 } else { 11.0 });
    let name = painter.layout_no_wrap(board.name.clone(), name_font, name_color);
    let dims = painter.layout_no_wrap(dimensions, dims_font, dims_color);
    let show_id = selected || overlays.ids;
    let id = painter.layout_no_wrap(
        board_short_id(board),
        egui::FontId::monospace(10.5),
        if selected { tw::ACCENT_DARK } else { tw::FAINT },
    );
    let stacked =
        name.size().y + dims.size().y + 2.0 + if show_id { id.size().y + 2.0 } else { 0.0 };
    let mut label_bottom = region.center().y;
    if full_part_label_fits(region.size(), name.size().x, dims.size().x)
        && region.height() >= stacked + 6.0
    {
        let mut y = region.center().y - stacked / 2.0;
        label_bottom = y + stacked;
        for galley in [Some(name), Some(dims), show_id.then_some(id)]
            .into_iter()
            .flatten()
        {
            let size = galley.size();
            painter.galley(
                egui::pos2(region.center().x - size.x / 2.0, y),
                galley,
                name_color,
            );
            y += size.y + 2.0;
        }
    } else if region.width() >= name.size().x + dims.size().x + 8.0 + 10.0
        && region.height() >= name.size().y + 2.0
    {
        // Short parts: name and dimensions on one line.
        let total = name.size().x + 8.0 + dims.size().x;
        let x = region.center().x - total / 2.0;
        let dims_x = x + name.size().x + 8.0;
        label_bottom = region.center().y + name.size().y / 2.0;
        painter.galley(
            egui::pos2(x, region.center().y - name.size().y / 2.0),
            name,
            name_color,
        );
        painter.galley(
            egui::pos2(dims_x, region.center().y - dims.size().y / 2.0),
            dims,
            dims_color,
        );
    } else if region.width() >= name.size().x + 8.0 && region.height() >= name.size().y + 2.0 {
        painter.galley(region.center() - name.size() / 2.0, name, name_color);
    }
    // Small grain glyph in the bottom-right corner.
    if let Some(direction) = grain
        && (selected || overlays.grain)
        && region.width() >= 40.0
        && region.bottom() - 20.0 >= label_bottom
    {
        let color = if selected { tw::ACCENT_DARK } else { tw::FAINT };
        let mut right = region.right() - 8.0;
        if selected && region.width() >= 110.0 {
            let text = painter.layout_no_wrap(
                localizer.text("sheet-overlay-grain").to_lowercase(),
                egui::FontId::proportional(10.5),
                color,
            );
            let size = text.size();
            painter.galley(
                egui::pos2(right - size.x, region.bottom() - 6.0 - size.y),
                text,
                color,
            );
            right -= size.x + 4.0;
        }
        let rect = egui::Rect::from_center_size(
            egui::pos2(right - 6.0, region.bottom() - 12.0),
            egui::Vec2::splat(12.0),
        );
        let mut image = icons::icon(Icon::Grain, color, 12.0);
        if direction.y.abs() > direction.x.abs() {
            image = image.rotate(std::f32::consts::FRAC_PI_2, egui::Vec2::splat(0.5));
        }
        image.paint_at(ui, rect);
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

/// Compact button used on issue cards (28 high, radius 6, 12px text).
fn small_button(
    ui: &mut egui::Ui,
    icon: Option<Icon>,
    text: &str,
    primary: bool,
    enabled: bool,
) -> egui::Response {
    let (fill, ink) = if primary {
        (tw::TEXT, tw::PANEL)
    } else {
        (tw::VIEWPORT, tw::TEXT)
    };
    let label = tw::medium(ui, text, 12.0).color(ink);
    let button = match icon {
        Some(icon) => egui::Button::image_and_text(icons::icon(icon, ink, 13.0), label),
        None => egui::Button::new(label),
    };
    ui.add_enabled(
        enabled,
        button
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(6)
            .min_size(egui::vec2(0.0, 28.0)),
    )
}

/// A sheet card in the priority list. The whole card is the click target.
fn sheet_card(
    ui: &mut egui::Ui,
    project: &Project,
    stock: &Stock,
    piece: Option<&StockPieceReadModel>,
    selected: bool,
    enabled: bool,
    localizer: &Localizer,
) -> egui::Response {
    let locale = locale(localizer);
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 58.0), egui::Sense::hover());
    let response = ui.interact(
        rect,
        ui.id().with(("sheet-card", stock.id)),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let alias = project.stock_alias(stock.id).unwrap_or("?");
    let material = stock_material(project, stock, locale);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            enabled,
            selected,
            format!("{alias} {material}"),
        )
    });
    let fill = if selected {
        tw::ACCENT_BG
    } else if enabled && response.hovered() {
        tw::HOVER_ROW
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 8.0, fill);
    let thumb = egui::Rect::from_min_size(rect.min + egui::vec2(8.0, 9.0), egui::vec2(60.0, 40.0));
    paint_thumbnail(ui.painter(), thumb, project, stock, selected);
    let used = project.allocations.iter().any(|a| a.stock_id == stock.id);
    let count = project
        .allocations
        .iter()
        .filter(|a| a.stock_id == stock.id)
        .count();
    let (alias_ink, name_ink, meta_ink) = if selected {
        (tw::ACCENT_DARK, tw::ACCENT_INK, CARD_ACTIVE_INK)
    } else if used {
        (tw::MUTED, tw::TEXT, tw::FAINT)
    } else {
        (tw::FAINT, tw::SECONDARY, tw::FAINT)
    };
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(thumb.right() + 10.0, rect.top() + 6.0),
        egui::pos2(rect.right() - 8.0, rect.bottom() - 4.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing.y = 2.0;
    let row = |ui: &mut egui::Ui, height: f32, add: &mut dyn FnMut(&mut egui::Ui)| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| add(ui),
        );
    };
    let owned = stock.source == StockSource::Owned;
    let chip_text = localizer.text("sheet-owned-chip");
    let chip_width = if owned {
        child
            .painter()
            .layout_no_wrap(
                chip_text.clone(),
                egui::FontId::proportional(10.5),
                tw::OK_INK,
            )
            .size()
            .x
            + 12.0
            + 6.0
    } else {
        0.0
    };
    row(&mut child, 19.0, &mut |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            tw::mono(alias, 12.0)
                .font(tw::weighted_font(ui, 12.0, Typeface::MonoSemibold))
                .color(alias_ink),
        );
        let name_width = (ui.available_width() - chip_width).max(20.0);
        ui.scope(|ui| {
            ui.set_max_width(name_width);
            ui.add(
                egui::Label::new(tw::medium(ui, material.clone(), 13.0).color(name_ink))
                    .truncate()
                    .selectable(false),
            );
        });
        if owned {
            egui::Frame::new()
                .fill(tw::OK_BG)
                .corner_radius(8)
                .inner_margin(Margin::symmetric(6, 0))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&chip_text).size(10.5).color(tw::OK_INK))
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        }
    });
    let dims = compact_dims(&[stock.length, stock.width], locale);
    let meta = if used {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("count", count);
        format!(
            "{dims} · {}",
            localizer.format("sheet-card-parts", Some(&args))
        )
    } else {
        format!("{dims} · {}", localizer.text("sheet-card-unused"))
    };
    child.add(
        egui::Label::new(tw::mono(meta, 11.0).color(meta_ink))
            .truncate()
            .selectable(false),
    );
    let proof = piece.map(|p| &p.proof);
    if let Some(percent) = card_utilization(proof) {
        row(&mut child, 14.0, &mut |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let bar_width = (ui.available_width() - 34.0).max(20.0);
            let (bar, _) =
                ui.allocate_exact_size(egui::vec2(bar_width, 14.0), egui::Sense::hover());
            let track = egui::Rect::from_center_size(bar.center(), egui::vec2(bar.width(), 4.0));
            ui.painter().rect_filled(
                track,
                2.0,
                if selected {
                    tw::WARN_STROKE
                } else {
                    tw::VIEWPORT
                },
            );
            let mut filled = track;
            filled.set_width(track.width() * percent.min(100) as f32 / 100.0);
            ui.painter().rect_filled(
                filled,
                2.0,
                if selected {
                    THUMB_ACTIVE_EDGE
                } else {
                    tw::FAINT
                },
            );
            ui.label(
                RichText::new(format!("{percent}%"))
                    .size(11.0)
                    .color(meta_ink),
            );
        });
    } else if used {
        row(&mut child, 14.0, &mut |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add(icons::icon(Icon::Warning, tw::WARN, 11.0));
            ui.add(
                egui::Label::new(
                    RichText::new(localizer.text(match proof {
                        Some(SheetProof::SearchExhausted) => "sheet-card-unknown",
                        _ => "sheet-card-conflict",
                    }))
                    .size(11.0)
                    .color(tw::WARN_INK),
                )
                .truncate(),
            );
        });
    }
    response.on_hover_text(format!(
        "{} · {}",
        stock.name,
        card_status(proof, localizer)
    ))
}

/// Short reason for a Needs stock card.
fn issue_reason(
    project: &Project,
    board: &Board,
    entry: &BoardDiagnostic,
    localizer: &Localizer,
) -> String {
    if entry.status == Status::Unallocated {
        if needs_new_stock(project, entry) {
            let material = project
                .materials
                .iter()
                .find(|m| m.id == board.material_id)
                .map_or("?", |m| m.name.as_str());
            let mut args = fluent_bundle::FluentArgs::new();
            args.set("material", material);
            args.set(
                "thickness",
                format!("{} mm", mm_text(board.thickness, locale(localizer))),
            );
            return localizer.format("sheet-issue-no-stock", Some(&args));
        }
        return localizer.text("sheet-issue-unplaced");
    }
    capitalized(
        entry
            .reasons
            .iter()
            .filter(|r| **r != Reason::MissingAllocation)
            .map(|reason| localizer.text(reason.key()))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

#[allow(clippy::too_many_arguments)]
fn issue_card(
    ui: &mut egui::Ui,
    project: &Project,
    model: Option<&StockReadModel>,
    entry: &BoardDiagnostic,
    board: &Board,
    hidden: bool,
    focused: bool,
    modal: bool,
    repairing: bool,
    localizer: &Localizer,
) -> Option<Request> {
    let mut request = None;
    let add_stock = needs_new_stock(project, entry);
    let locale = locale(localizer);
    egui::Frame::new()
        .fill(tw::CARD)
        .stroke(Stroke::new(
            1.0,
            if focused { tw::FOCUS } else { tw::WARN_STROKE },
        ))
        .corner_radius(8)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 7.0;
                ui.add(icons::icon(Icon::Warning, tw::WARN, 14.0));
                let dims = compact_dims(&[board.length, board.width, board.thickness], locale);
                let dims_width = ui
                    .painter()
                    .layout_no_wrap(dims.clone(), egui::FontId::monospace(11.0), tw::FAINT)
                    .size()
                    .x;
                let name_width =
                    (ui.available_width() - dims_width - if hidden { 24.0 } else { 6.0 }).max(30.0);
                ui.scope(|ui| {
                    ui.set_max_width(name_width);
                    ui.add(
                        egui::Label::new(tw::medium(ui, board.name.clone(), 13.0).color(tw::TEXT))
                            .truncate(),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(tw::mono(dims, 11.0).color(tw::FAINT));
                    if hidden {
                        ui.add(icons::icon(Icon::EyeOff, tw::FAINT, 13.0))
                            .on_hover_text(localizer.text("global-hidden"));
                    }
                });
            });
            let mut reason = issue_reason(project, board, entry, localizer);
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
                reason.push_str(". ");
                reason.push_str(&localizer.text("sheet-trim-conflict"));
            }
            ui.add(egui::Label::new(RichText::new(reason).size(12.0).color(tw::MUTED)).wrap());
            let primary_text = if add_stock {
                let material = project
                    .materials
                    .iter()
                    .find(|m| m.id == board.material_id)
                    .map_or("?", |m| m.name.as_str());
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("material", material);
                localizer.format("sheet-add-named", Some(&args))
            } else {
                localizer.text("global-repair")
            };
            let reveal_text = localizer.text("sheet-reveal");
            let font = tw::weighted_font(ui, 12.0, Typeface::SansMedium);
            let text_width = |text: &str| {
                ui.painter()
                    .layout_no_wrap(text.to_owned(), font.clone(), tw::TEXT)
                    .size()
                    .x
            };
            let padding = ui.spacing().button_padding.x * 2.0;
            // Reveal moves into the overflow menu when the card is too narrow.
            let reveal_inline = text_width(&primary_text)
                + padding
                + 13.0
                + ui.spacing().icon_spacing
                + text_width(&reveal_text)
                + padding
                + 22.0
                + 8.0
                <= ui.available_width();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let primary = small_button(
                    ui,
                    Some(if add_stock { Icon::Plus } else { Icon::Move }),
                    &primary_text,
                    true,
                    if add_stock {
                        !modal && !repairing
                    } else {
                        !modal
                    },
                );
                let primary = if add_stock {
                    primary.on_hover_text(localizer.text("sheet-add-material"))
                } else {
                    primary
                };
                if primary.clicked() {
                    request = Some(issue_resolution(project, entry));
                }
                if reveal_inline
                    && small_button(ui, None, &reveal_text, false, !modal && !repairing)
                        .on_hover_text(localizer.text("global-locate"))
                        .clicked()
                {
                    request = Some(Request::with(A::LocateIssue, Target::Board(board.id)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let more = tw::ghost_icon_sized(
                        ui,
                        Icon::Dots,
                        &localizer.text("sheet-more-actions"),
                        tw::SECONDARY,
                        15.0,
                        22.0,
                        !modal,
                        false,
                    );
                    egui::Popup::menu(&more)
                        .align(egui::RectAlign::BOTTOM_END)
                        .show(|ui| {
                            ui.set_min_width(180.0);
                            if !reveal_inline
                                && ui
                                    .add_enabled(!repairing, egui::Button::new(&reveal_text))
                                    .clicked()
                            {
                                request =
                                    Some(Request::with(A::LocateIssue, Target::Board(board.id)));
                            }
                            if add_stock {
                                if ui.button(localizer.text("global-repair")).clicked() {
                                    request = Some(Request::with(
                                        A::RepairIssue,
                                        Target::Board(board.id),
                                    ));
                                }
                            } else if ui
                                .add_enabled(
                                    !repairing,
                                    egui::Button::new(localizer.text("stock-new")),
                                )
                                .clicked()
                            {
                                request =
                                    Some(Request::with(A::AddIssueStock, Target::Board(board.id)));
                            }
                        });
                });
            });
        });
    request
}

/// Automatic placement: fill gaps, re-plan, and per-material advice on what
/// is missing, each with a one-click fix. Every button is one undoable edit.
fn place_panel(
    ui: &mut egui::Ui,
    project: &Project,
    suggestions: &[SheetSuggestion],
    waiting: bool,
    disabled: bool,
    focus: SheetFocus,
    localizer: &Localizer,
) -> Option<Request> {
    if project.boards.is_empty() {
        return None;
    }
    let mut request = None;
    let has_stock = !project.stock.is_empty();
    let unallocated = project
        .boards
        .iter()
        .any(|b| !project.allocations.iter().any(|a| a.board_id == b.id));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        if small_button(
            ui,
            Some(Icon::Place),
            &localizer.text("sheet-place-unallocated"),
            unallocated,
            !disabled && has_stock && unallocated,
        )
        .on_hover_text(localizer.text("sheet-place-hint"))
        .on_disabled_hover_text(localizer.text(if has_stock {
            "sheet-place-none"
        } else {
            "sheet-place-no-stock"
        }))
        .clicked()
        {
            request = Some(Request::new(A::PlaceUnallocated));
        }
        if small_button(
            ui,
            Some(Icon::Layers),
            &localizer.text("sheet-replan"),
            !unallocated,
            !disabled && has_stock,
        )
        .on_hover_text(localizer.text("sheet-replan-hint"))
        .clicked()
        {
            request = Some(Request::new(A::ReplanSheets));
        }
    });
    if !waiting {
        return request;
    }
    let locale = locale(localizer);
    let names = |ids: &[(Uuid, Unplaced)], reason: Unplaced| -> Vec<String> {
        ids.iter()
            .filter(|(_, r)| *r == reason)
            .filter_map(|(id, _)| project.boards.iter().find(|b| b.id == *id))
            .map(|b| b.name.clone())
            .collect()
    };
    for suggestion in suggestions {
        let material = project
            .materials
            .iter()
            .find(|m| m.id == suggestion.material_id)
            .map_or("?", |m| m.name.as_str());
        let declared = project.stock.iter().any(|s| {
            s.material_id == suggestion.material_id && s.thickness == suggestion.thickness
        });
        egui::Frame::new()
            .fill(tw::CARD)
            .stroke(Stroke::new(1.0, tw::WARN_STROKE))
            .corner_radius(8)
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 5.0;
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("material", material);
                args.set(
                    "thickness",
                    format!("{} mm", mm_text(suggestion.thickness, locale)),
                );
                args.set("count", suggestion.waiting);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    ui.add(icons::icon(Icon::Sheet, tw::WARN, 14.0));
                    ui.add(
                        egui::Label::new(
                            tw::medium(
                                ui,
                                localizer.format("sheet-group-waiting", Some(&args)),
                                13.0,
                            )
                            .color(tw::TEXT),
                        )
                        .wrap(),
                    );
                });
                let note = |ui: &mut egui::Ui, text: String| {
                    ui.add(
                        egui::Label::new(RichText::new(text).size(12.0).color(tw::MUTED)).wrap(),
                    );
                };
                let fixable = suggestion.waiting - suggestion.blocked.len();
                if fixable > 0 {
                    note(
                        ui,
                        localizer.text(if declared {
                            "sheet-group-full"
                        } else {
                            "sheet-group-no-stock"
                        }),
                    );
                }
                for (reason, key) in [
                    (Unplaced::TooLarge, "sheet-group-too-large"),
                    (Unplaced::Grain, "sheet-group-grain"),
                ] {
                    let list = names(&suggestion.blocked, reason);
                    if !list.is_empty() {
                        let mut args = fluent_bundle::FluentArgs::new();
                        args.set("count", list.len());
                        args.set("names", list.join(", "));
                        note(ui, localizer.format(key, Some(&args)));
                    }
                }
                if let Some(next) =
                    waiting_parts(ui, project, suggestion, disabled, focus, localizer)
                {
                    request = Some(next);
                }
                match &suggestion.sheet {
                    Some(sheet) if sheet.count > 0 && fixable > 0 => {
                        let mut args = fluent_bundle::FluentArgs::new();
                        args.set("count", sheet.count);
                        args.set("size", compact_dims(&[sheet.length, sheet.width], locale));
                        if small_button(
                            ui,
                            Some(Icon::Plus),
                            &localizer.format("sheet-group-add", Some(&args)),
                            true,
                            !disabled,
                        )
                        .on_hover_text(localizer.text("sheet-group-add-hint"))
                        .clicked()
                        {
                            request = Some(
                                Request::with(
                                    A::AddSuggestedSheets,
                                    Target::Material(suggestion.material_id),
                                )
                                .argument(Argument::Length(suggestion.thickness)),
                            );
                        }
                    }
                    _ => {
                        if small_button(
                            ui,
                            Some(Icon::Plus),
                            &localizer.format("sheet-add-named", Some(&args)),
                            fixable > 0,
                            !disabled,
                        )
                        .on_hover_text(localizer.text("sheet-group-no-size"))
                        .clicked()
                        {
                            request = Some(Request::with(
                                A::NewStock,
                                Target::Material(suggestion.material_id),
                            ));
                        }
                    }
                }
            });
    }
    request
}

/// Collapsible list of one material's waiting parts, each with Reveal. It
/// opens itself when an issue link targets one of them.
fn waiting_parts(
    ui: &mut egui::Ui,
    project: &Project,
    suggestion: &SheetSuggestion,
    disabled: bool,
    focus: SheetFocus,
    localizer: &Localizer,
) -> Option<Request> {
    let mut request = None;
    let locale = locale(localizer);
    let targeted = focus.issue.filter(|id| suggestion.boards.contains(id));
    let mut args = fluent_bundle::FluentArgs::new();
    args.set("count", suggestion.boards.len());
    let header = egui::CollapsingHeader::new(
        RichText::new(localizer.format("sheet-group-parts", Some(&args)))
            .size(12.0)
            .color(tw::SECONDARY),
    )
    .id_salt((
        "waiting-parts",
        suggestion.material_id,
        suggestion.thickness,
    ))
    .open((targeted.is_some() && focus.scroll_to_target).then_some(true));
    header.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for id in &suggestion.boards {
            let Some(board) = project.boards.iter().find(|b| b.id == *id) else {
                continue;
            };
            let focused = targeted == Some(board.id);
            let row = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let dims = compact_dims(&[board.length, board.width], locale);
                let name_width = (ui.available_width() - 130.0).max(40.0);
                ui.scope(|ui| {
                    ui.set_max_width(name_width);
                    ui.add(
                        egui::Label::new(RichText::new(&board.name).size(12.0).color(if focused {
                            tw::FOCUS
                        } else {
                            tw::TEXT
                        }))
                        .truncate(),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !disabled,
                            egui::Button::new(
                                RichText::new(localizer.text("sheet-reveal")).size(11.5),
                            )
                            .small(),
                        )
                        .on_hover_text(localizer.text("global-locate"))
                        .clicked()
                    {
                        request = Some(Request::with(A::LocateIssue, Target::Board(board.id)));
                    }
                    ui.label(tw::mono(dims, 11.0).color(tw::FAINT));
                });
            });
            if focused && focus.scroll_to_target {
                row.response.scroll_to_me(Some(egui::Align::Center));
            }
        }
    });
    request
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
    repair.refresh(project, editor.preview_generation());
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
    let repairing = repair.active();
    let focused_sheet = repair.focused_sheet;
    let model = repair.model();
    let diagnostics = repair.board_diagnostics();
    let mut request = None;
    let mut chosen = None;
    egui::Frame::new()
        .inner_margin(Margin {
            left: 14,
            right: 10,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            tw::section_bar(ui, &localizer.text("sheet-priority-cards"), |ui| {
                if tw::ghost_icon_sized(
                    ui,
                    Icon::Plus,
                    &localizer.text("stock-new"),
                    tw::SECONDARY,
                    15.0,
                    26.0,
                    !modal && !repairing,
                    false,
                )
                .clicked()
                {
                    request = Some(Request::new(A::NewStock));
                }
            });
        });
    egui::Frame::new()
        .inner_margin(Margin::symmetric(8, 0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            if ordered.is_empty() {
                ui.add(
                    egui::Label::new(
                        RichText::new(localizer.text("design-no-stock"))
                            .size(12.0)
                            .color(tw::MUTED),
                    )
                    .wrap(),
                );
            }
            for stock in &ordered {
                let piece = model.and_then(|model| model.miniature(stock.id));
                let response = sheet_card(
                    ui,
                    project,
                    stock,
                    piece,
                    focused_sheet == Some(stock.id),
                    !modal,
                    localizer,
                );
                if response.clicked() {
                    chosen = Some(stock.id);
                }
                response.context_menu(|ui| {
                    if ui
                        .add_enabled(
                            !modal && !repairing,
                            egui::Button::new(localizer.text("sheet-edit-stock")),
                        )
                        .clicked()
                    {
                        request = Some(Request::with(A::EditStock, Target::Stock(stock.id)));
                        ui.close();
                    }
                });
            }
        });
    ui.add_space(10.0);
    tw::divider(ui);
    let issues: Vec<_> = needs_stock(diagnostics).collect();
    egui::Frame::new()
        .inner_margin(Margin::symmetric(14, 0))
        .show(ui, |ui| {
            tw::section_bar(ui, &localizer.text("sheet-needs-stock"), |ui| {
                if !issues.is_empty() {
                    ui.label(
                        RichText::new(issues.len().to_string())
                            .font(tw::weighted_font(ui, 11.0, Typeface::MonoSemibold))
                            .color(tw::WARN),
                    );
                }
            });
        });
    egui::Frame::new()
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 0,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            if let Some(next) = place_panel(
                ui,
                project,
                repair.suggestions(),
                !issues.is_empty(),
                modal || repairing,
                focus,
                localizer,
            ) {
                request = Some(next);
            }
            if issues.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.add(icons::icon(Icon::Check, tw::OK, 13.0));
                    ui.add(
                        egui::Label::new(
                            RichText::new(localizer.text("sheet-no-issues"))
                                .size(12.0)
                                .color(tw::MUTED),
                        )
                        .wrap(),
                    );
                });
            }
            // Waiting parts are listed under their material card instead.
            let grouped: HashSet<Uuid> = repair
                .suggestions()
                .iter()
                .flat_map(|s| s.boards.iter().copied())
                .collect();
            for entry in issues {
                if entry.status == Status::Unallocated && grouped.contains(&entry.board_id) {
                    continue;
                }
                let Some(board) = project.boards.iter().find(|b| b.id == entry.board_id) else {
                    continue;
                };
                let focused = focus.issue == Some(board.id);
                if let Some(next) = issue_card(
                    ui,
                    project,
                    model,
                    entry,
                    board,
                    !selection.visible(project, board.id),
                    focused,
                    modal,
                    repairing,
                    localizer,
                ) {
                    request = Some(next);
                }
                if focus.scroll_to_target && focused {
                    ui.scroll_to_cursor(Some(egui::Align::TOP));
                }
            }
        });
    if let Some(id) = chosen {
        repair.focused_sheet = Some(id);
    }
    request
}

/// Pinned footer of the Cut plan controls pane: "+ Sheet or offcut".
pub fn show_sheet_footer(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    repair: &RepairUi,
    modal: bool,
) -> Option<Request> {
    let enabled = !modal && !repair.active();
    let width = ui.available_width();
    ui.add_enabled(
        enabled,
        egui::Button::image_and_text(
            icons::icon(Icon::Plus, tw::TEXT, 14.0),
            tw::medium(ui, localizer.text("sheet-footer-add"), 13.0).color(tw::TEXT),
        )
        .fill(tw::VIEWPORT)
        .stroke(Stroke::NONE)
        .corner_radius(7)
        .min_size(egui::vec2(width, 32.0)),
    )
    .on_hover_text(localizer.text("stock-new"))
    .clicked()
    .then(|| Request::new(A::NewStock))
}

/// Render into the host's Cut plan inspector ScrollArea. Hover is
/// session-only and read on the next canvas frame.
#[cfg(test)]
pub fn show_focused_inspector(
    ui: &mut egui::Ui,
    editor: &ProjectEditor,
    localizer: &Localizer,
    repair: &mut RepairUi,
) {
    show_focused_inspector_with_reserve(ui, editor, localizer, repair, None);
}

/// Like `show_focused_inspector`; with `reserve`, the cut sequence scrolls on
/// its own so that much height stays free below it (for the optimizer).
pub fn show_focused_inspector_with_reserve(
    ui: &mut egui::Ui,
    editor: &ProjectEditor,
    localizer: &Localizer,
    repair: &mut RepairUi,
    reserve: Option<f32>,
) {
    let project = editor.preview().unwrap_or(editor.project());
    repair.refresh(project, editor.preview_generation());
    let stock = repair
        .focused_sheet
        .and_then(|id| project.stock.iter().find(|s| s.id == id));
    let highlighted = repair
        .hovered_cut
        .zip(stock)
        .and_then(|((sheet, number), stock)| (sheet == stock.id).then_some(number));
    let hovered = sheet_inspector(
        ui,
        stock.and_then(|stock| repair.model()?.miniature(stock.id)),
        project,
        localizer,
        highlighted,
        reserve,
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

fn tool_group() -> egui::Frame {
    egui::Frame::new()
        .fill(tw::PANEL)
        .stroke(Stroke::new(1.0, tw::BORDER_SOFT))
        .corner_radius(9)
        .inner_margin(3)
}

/// 28×28 icon button (optionally mirrored) with an accessible name.
fn nav_button(ui: &mut egui::Ui, mirrored: bool, label: &str, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(28.0, 28.0),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    let response = response.on_hover_text(label);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if enabled && response.hovered() {
        ui.painter().rect_filled(rect, 6.0, tw::VIEWPORT);
    }
    let color = if enabled { tw::SECONDARY } else { tw::DISABLED };
    let mut image = icons::icon(Icon::ChevRight, color, 15.0);
    if mirrored {
        image = image.rotate(std::f32::consts::PI, egui::Vec2::splat(0.5));
    }
    image.paint_at(
        ui,
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(15.0)),
    );
    response
}

/// Segment or toggle button inside a toolbar group.
fn tool_toggle(
    ui: &mut egui::Ui,
    text: &str,
    icon: Option<Icon>,
    selected: bool,
    dark: bool,
    enabled: bool,
) -> egui::Response {
    let (fill, ink) = match (selected, dark) {
        (true, true) => (tw::TEXT, tw::PANEL),
        (true, false) => (tw::VIEWPORT, tw::TEXT),
        (false, _) => (Color32::TRANSPARENT, tw::SECONDARY),
    };
    let label = if selected {
        tw::medium(ui, text, 12.5).color(ink)
    } else {
        RichText::new(text).size(12.5).color(ink)
    };
    let button = match icon {
        Some(icon) => egui::Button::image_and_text(icons::icon(icon, ink, 14.0), label),
        None => egui::Button::new(label),
    };
    ui.add_enabled(
        enabled,
        button
            .selected(selected)
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(6)
            .min_size(egui::vec2(0.0, 28.0)),
    )
}

#[derive(Default)]
struct ToolbarOutput {
    focus: Option<Uuid>,
    begin: bool,
    accept: bool,
    cancel: bool,
    fit: bool,
}

fn show_toolbar(
    ui: &mut egui::Ui,
    project: &Project,
    ordered: &[&Stock],
    repair: &mut RepairUi,
    can_accept: bool,
    modal: bool,
    localizer: &Localizer,
) -> ToolbarOutput {
    let mut out = ToolbarOutput::default();
    let locale = locale(localizer);
    let index = ordered
        .iter()
        .position(|s| Some(s.id) == repair.focused_sheet);
    let right_width_id = ui.id().with("sheet-toolbar-right-width");
    let right_width: f32 = ui.data(|d| d.get_temp(right_width_id)).unwrap_or(330.0);
    let mut measured = right_width;
    let inline = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if let Some(index) = index {
                let stock = ordered[index];
                tool_group().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if nav_button(
                            ui,
                            true,
                            &localizer.text("sheet-previous"),
                            !modal && index > 0,
                        )
                        .clicked()
                        {
                            out.focus = Some(ordered[index - 1].id);
                        }
                        ui.label(
                            tw::mono(project.stock_alias(stock.id).unwrap_or("?"), 12.5)
                                .font(tw::weighted_font(ui, 12.5, Typeface::MonoSemibold))
                                .color(tw::TEXT),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!(
                                    "{} · {}",
                                    stock_material(project, stock, locale),
                                    dims_text(stock.length, stock.width, locale)
                                ))
                                .size(13.0)
                                .color(tw::TEXT),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        )
                        .on_hover_text(&stock.name);
                        if nav_button(
                            ui,
                            false,
                            &localizer.text("sheet-next"),
                            !modal && index + 1 < ordered.len(),
                        )
                        .clicked()
                        {
                            out.focus = Some(ordered[index + 1].id);
                        }
                    });
                });
            }
            let active = repair.active();
            tool_group().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let view = tool_toggle(
                        ui,
                        &localizer.text("sheet-mode-view"),
                        None,
                        !active,
                        true,
                        !modal,
                    );
                    if active {
                        view.on_hover_text(localizer.text("sheet-mode-view-hint"));
                    }
                    let edit_label = localizer.text("sheet-edit");
                    let repair_button = tool_toggle(
                        ui,
                        &localizer.text("sheet-mode-repair"),
                        Some(Icon::Move),
                        active,
                        true,
                        !modal,
                    );
                    repair_button.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            !modal,
                            active,
                            &edit_label,
                        )
                    });
                    if repair_button
                        .on_hover_text(localizer.text("sheet-repair-hint"))
                        .clicked()
                        && !active
                    {
                        out.begin = true;
                    }
                });
            });
            if active {
                out.accept = tw::icon_text_button(
                    ui,
                    Icon::Check,
                    &localizer.text("sheet-accept"),
                    true,
                    !modal && can_accept,
                )
                .clicked();
                out.cancel =
                    tw::secondary_button_enabled(ui, &localizer.text("sheet-cancel"), !modal)
                        .on_hover_text("Esc")
                        .clicked();
            }
            let remaining = ui.available_width();
            if remaining >= right_width + 1.0 {
                ui.add_space(remaining - right_width - 1.0);
                measured = toolbar_view_controls(ui, repair, &mut out, localizer);
                true
            } else {
                false
            }
        })
        .inner;
    if !inline {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            measured = toolbar_view_controls(ui, repair, &mut out, localizer);
        });
    }
    ui.data_mut(|d| d.insert_temp(right_width_id, measured));
    out
}

/// Overlay toggles and Fit/zoom. Returns the width they used.
fn toolbar_view_controls(
    ui: &mut egui::Ui,
    repair: &mut RepairUi,
    out: &mut ToolbarOutput,
    localizer: &Localizer,
) -> f32 {
    let toggles = tool_group()
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let overlays = &mut repair.overlays;
                for (value, key, hint) in [
                    (
                        &mut overlays.cuts,
                        "sheet-overlay-cuts",
                        "sheet-overlay-cuts-hint",
                    ),
                    (
                        &mut overlays.offcuts,
                        "sheet-overlay-offcuts",
                        "sheet-overlay-offcuts-hint",
                    ),
                    (
                        &mut overlays.grain,
                        "sheet-overlay-grain",
                        "sheet-overlay-grain-hint",
                    ),
                    (
                        &mut overlays.ids,
                        "sheet-overlay-ids",
                        "sheet-overlay-ids-hint",
                    ),
                ] {
                    if tool_toggle(ui, &localizer.text(key), None, *value, false, true)
                        .on_hover_text(localizer.text(hint))
                        .clicked()
                    {
                        *value = !*value;
                    }
                }
            });
        })
        .response
        .rect;
    let zoom = tool_group()
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let fit_label = localizer.text("sheet-fit");
                let fit = ui
                    .add(
                        egui::Button::image_and_text(
                            icons::icon(Icon::Frame, tw::SECONDARY, 15.0),
                            tw::mono(localizer.text("sheet-fit-short"), 12.0).color(tw::TEXT),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE)
                        .corner_radius(6)
                        .min_size(egui::vec2(0.0, 28.0)),
                    )
                    .on_hover_text(&fit_label);
                fit.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &fit_label)
                });
                if fit.clicked() {
                    out.fit = true;
                }
                for (symbol, key, factor) in [
                    ("−", "sheet-zoom-out", 1.0 / 1.25_f32),
                    ("+", "sheet-zoom-in", 1.25),
                ] {
                    let label = localizer.text(key);
                    let response = ui
                        .add(
                            egui::Button::new(tw::mono(symbol, 14.0).color(tw::SECONDARY))
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::NONE)
                                .corner_radius(6)
                                .min_size(egui::vec2(26.0, 28.0)),
                        )
                        .on_hover_text(&label);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label)
                    });
                    if response.clicked() {
                        repair.zoom = (repair.zoom.max(0.1) * factor).clamp(0.25, 4.0);
                    }
                }
            });
        })
        .response
        .rect;
    toggles.width() + zoom.width() + ui.spacing().item_spacing.x
}

fn show_legend(ui: &egui::Ui, origin: egui::Pos2, kerf: Length, localizer: &Localizer) {
    let painter = ui.painter();
    let font = egui::FontId::proportional(11.5);
    let mut x = origin.x;
    let y = origin.y;
    let swatch = |x: f32| egui::Rect::from_min_size(egui::pos2(x, y - 5.0), egui::vec2(14.0, 10.0));
    let mut args = fluent_bundle::FluentArgs::new();
    args.set("kerf", format!("{} mm", mm_text(kerf, locale(localizer))));
    for (index, text) in [
        localizer.text("sheet-legend-part"),
        localizer.text("sheet-legend-offcut"),
        localizer.format("sheet-legend-kerf", Some(&args)),
        localizer.text("sheet-legend-conflict"),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = swatch(x);
        match index {
            0 => {
                painter.rect_filled(rect, 0.0, PART_FILL);
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0, PART_EDGE),
                    egui::StrokeKind::Inside,
                );
            }
            1 => {
                painter.rect_filled(rect, 0.0, OFFCUT_FILL);
                let hatch = painter.with_clip_rect(rect);
                let mut hx = rect.left() - rect.height();
                while hx < rect.right() {
                    hatch.line_segment(
                        [
                            egui::pos2(hx, rect.bottom()),
                            egui::pos2(hx + rect.height(), rect.top()),
                        ],
                        Stroke::new(1.0, Color32::from_rgb(216, 209, 196)),
                    );
                    hx += 4.0;
                }
            }
            2 => {
                painter.rect_filled(
                    egui::Rect::from_center_size(rect.center(), egui::vec2(14.0, 3.0)),
                    0.0,
                    tw::KERF,
                );
            }
            _ => {
                painter.rect_filled(rect, 0.0, CONFLICT_FILL);
                paint_dashed_rect(
                    painter,
                    rect.shrink(0.5),
                    Stroke::new(1.0, tw::KERF),
                    2.0,
                    2.0,
                );
            }
        }
        let galley = painter.layout_no_wrap(text, font.clone(), tw::MUTED);
        let width = galley.size().x;
        painter.galley(
            egui::pos2(x + 20.0, y - galley.size().y / 2.0),
            galley,
            tw::MUTED,
        );
        x += 20.0 + width + 16.0;
    }
}

/// Repair controls for the selected part, shown as a strip under the toolbar.
fn show_repair_strip(
    ui: &mut egui::Ui,
    project: &Project,
    repair: &mut RepairUi,
    modal: bool,
    preview: Option<u64>,
    localizer: &Localizer,
) -> Option<RepairAction> {
    let mut action = None;
    let locale = locale(localizer);
    egui::Frame::new()
        .inner_margin(Margin {
            left: 14,
            right: 14,
            top: 0,
            bottom: 8,
        })
        .show(ui, |ui| {
            tw::floating_frame()
                .inner_margin(Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                    let Some(id) = repair.board else {
                        ui.horizontal_wrapped(|ui| {
                            ui.add(icons::icon(Icon::Move, tw::ACCENT, 14.0));
                            ui.label(
                                RichText::new(localizer.text("sheet-repair-pick"))
                                    .size(12.5)
                                    .color(tw::SECONDARY),
                            )
                            .on_hover_text(localizer.text("sheet-repair-hint"));
                        });
                        return;
                    };
                    let allocated = project.allocations.iter().find(|a| a.board_id == id);
                    ui.horizontal_wrapped(|ui| {
                        ui.add(icons::icon(Icon::Move, tw::ACCENT, 14.0));
                        let name = project
                            .boards
                            .iter()
                            .find(|b| b.id == id)
                            .map(|b| b.name.clone())
                            .unwrap_or_default();
                        ui.label(tw::medium(ui, name, 13.0).color(tw::TEXT))
                            .on_hover_text(localizer.text("sheet-selected"));
                        if allocated.is_some_and(|a| a.locked) {
                            ui.add(icons::icon(Icon::Lock, tw::FAINT, 13.0))
                                .on_hover_text(localizer.text("sheet-locked-error"));
                        }
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(localizer.text("sheet-target"))
                                .size(12.0)
                                .color(tw::MUTED),
                        );
                        let previous = repair.stock;
                        egui::ComboBox::from_id_salt("sheet-target-stock")
                            .width(150.0)
                            .truncate()
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
                                    );
                                }
                            });
                        if repair.stock != previous {
                            repair.placement_dirty = true;
                        }
                    });
                    let unit = repair.entry_unit.unwrap_or_else(|| numeric_unit(project));
                    let mut valid = true;
                    let mut notes = Vec::new();
                    ui.horizontal_wrapped(|ui| {
                        for axis in 0..2 {
                            let label = localizer.text(if axis == 0 {
                                "sheet-origin-x"
                            } else {
                                "sheet-origin-y"
                            });
                            ui.label(RichText::new(&label).size(12.0).color(tw::MUTED));
                            let invalid = parse_length(&repair.origin[axis], unit).is_err();
                            if tw::value_field(
                                ui,
                                ui.id().with(("sheet-origin", axis)),
                                &label,
                                &mut repair.origin[axis],
                                96.0,
                                None,
                                Some(if axis == 0 {
                                    tw::KERF
                                } else {
                                    Color32::from_rgb(78, 154, 87)
                                }),
                                !modal,
                                invalid,
                            )
                            .changed()
                            {
                                repair.entry_unit.get_or_insert(unit);
                                repair.consent[axis] = false;
                                repair.placement_dirty = true;
                            }
                            match parse_length(&repair.origin[axis], unit) {
                                Ok(parsed) => {
                                    if let Conversion::NeedsConfirmation(value) = parsed.conversion
                                    {
                                        notes.push((axis, Some(value)));
                                    }
                                }
                                Err(_) => notes.push((axis, None)),
                            }
                            valid &= coordinate(&repair.origin[axis], unit, repair.consent[axis])
                                .is_some();
                        }
                        if ui
                            .checkbox(
                                &mut repair.quarter_turn,
                                RichText::new(localizer.text("sheet-quarter-turn")).size(12.5),
                            )
                            .changed()
                        {
                            repair.placement_dirty = true;
                        }
                    });
                    for (axis, rounded) in notes {
                        match rounded {
                            Some(value) => {
                                let mut args = fluent_bundle::FluentArgs::new();
                                args.set("entered", repair.origin[axis].as_str());
                                args.set("rounded", format_length(value, Unit::Mm, locale, 3));
                                ui.checkbox(
                                    &mut repair.consent[axis],
                                    localizer.format("rounding-confirmation", Some(&args)),
                                );
                            }
                            None => {
                                ui.label(
                                    RichText::new(localizer.text("sheet-coordinate-error"))
                                        .size(11.5)
                                        .color(tw::DANGER),
                                );
                            }
                        }
                    }
                    // The staged placement exists only when every input parses,
                    // so the button's state and its action share one source.
                    let staged = valid
                        .then(|| {
                            Some((
                                repair.stock?,
                                [
                                    coordinate(&repair.origin[0], unit, repair.consent[0])?,
                                    coordinate(&repair.origin[1], unit, repair.consent[1])?,
                                ],
                            ))
                        })
                        .flatten();
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if small_button(
                            ui,
                            Some(Icon::Place),
                            &localizer.text("sheet-stage"),
                            true,
                            !modal && staged.is_some() && !allocated.is_some_and(|a| a.locked),
                        )
                        .clicked()
                            && let Some((stock, origin)) = staged
                        {
                            action =
                                Some(RepairAction::Place(id, stock, origin, repair.quarter_turn));
                        }
                        if let Some(a) = allocated {
                            if small_button(
                                ui,
                                None,
                                &localizer.text("sheet-unallocate-action"),
                                false,
                                !modal && !repair.placement_dirty,
                            )
                            .clicked()
                            {
                                action = Some(RepairAction::Unallocate(id));
                            }
                            if small_button(
                                ui,
                                Some(Icon::Lock),
                                &localizer.text(if a.locked {
                                    "sheet-unlock"
                                } else {
                                    "sheet-lock"
                                }),
                                false,
                                !modal && !repair.placement_dirty,
                            )
                            .clicked()
                            {
                                action = Some(RepairAction::Lock(id, !a.locked));
                            }
                        }
                    });
                    if let Some(key) = repair.error {
                        ui.add(
                            egui::Label::new(
                                RichText::new(localizer.text(key))
                                    .size(12.0)
                                    .color(tw::DANGER),
                            )
                            .wrap(),
                        );
                    }
                    show_affected_statuses(ui, project, repair, preview, localizer);
                });
        });
    action
}

fn show_affected_statuses(
    ui: &mut egui::Ui,
    project: &Project,
    repair: &mut RepairUi,
    preview: Option<u64>,
    localizer: &Localizer,
) {
    let Some(affected) = &repair.affected else {
        return;
    };
    let mut stocks: Vec<_> = affected.iter().copied().collect();
    if stocks.is_empty() {
        return;
    }
    stocks.sort_unstable();
    let key = diagnostics_key(project, preview);
    if repair
        .affected_cache
        .as_ref()
        .is_some_and(|(cached, ids, _)| *cached != key || *ids != stocks)
    {
        repair.affected_cache = None;
    }
    let (_, _, statuses) = repair.affected_cache.get_or_insert_with(|| {
        let statuses = stocks
            .iter()
            .copied()
            .map(|stock_id| {
                let status =
                    reconstruct_witness(project, stock_id, project.cutting_kerf, WITNESS_BUDGET);
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
        (key, stocks, statuses)
    });
    let statuses = statuses.clone();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        for &(stock_id, message) in &statuses {
            let verified = message == "sheet-verified";
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add(icons::icon(
                    if verified { Icon::Check } else { Icon::Warning },
                    if verified { tw::OK } else { tw::KERF },
                    12.0,
                ));
                ui.label(
                    RichText::new(format!(
                        "{}: {}",
                        project.stock_alias(stock_id).unwrap_or("?"),
                        localizer.text(message)
                    ))
                    .size(11.5)
                    .color(if verified { tw::OK_INK } else { tw::DANGER }),
                );
            });
        }
    });
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
    let preview = editor.preview_generation();
    let loc = locale(localizer);
    let mut action = None;
    let mut selection_request = None;
    repair.refresh(project, preview);
    let model = repair.model().cloned();
    let board_diagnostics = repair.board_diagnostics().to_vec();
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
    let can_accept = repair.active() && repair.can_accept(editor);
    let toolbar = egui::Frame::new()
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            show_toolbar(ui, project, &ordered, repair, can_accept, modal, localizer)
        })
        .inner;
    if let Some(id) = toolbar.focus {
        repair.focused_sheet = Some(id);
    }
    let accept = toolbar.accept;
    let mut cancel = toolbar.cancel;
    if toolbar.fit {
        repair.zoom = 1.0;
    }
    if toolbar.begin {
        let _ = actions::contextual(Request::new(A::BeginRepair), Ok(()), || {
            repair.begin(editor, selection, loc)
        });
    }
    if repair.active() {
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
        repair.select(project, selection.active, loc);
        action = show_repair_strip(ui, project, repair, modal, preview, localizer);
    }
    let issues = diagnostic_issues(&board_diagnostics);
    let canvas_sheet = repair.focused_sheet;
    egui::ScrollArea::vertical()
        .id_salt("sheet-workspace-scroll")
        .show(ui, |ui| {
            let Some(stock) = ordered.iter().find(|s| Some(s.id) == canvas_sheet).copied() else {
                // Offer the missing prerequisite: boards to cut, then sheets to cut them from.
                let (icon, title, detail, label, next) = if project.boards.is_empty() {
                    (
                        Icon::Board,
                        "empty-cut-title",
                        "empty-cut-detail",
                        "board-new",
                        A::NewBoard,
                    )
                } else {
                    (
                        Icon::Sheet,
                        "empty-sheets-title",
                        "empty-sheets-detail",
                        "stock-new",
                        A::NewStock,
                    )
                };
                ui.set_min_height(ui.clip_rect().height() - 40.0);
                if tw::empty_state(
                    ui,
                    icon,
                    &localizer.text(title),
                    &localizer.text(detail),
                    Some((Icon::Plus, &localizer.text(label))),
                ) && !modal
                {
                    selection_request = Some(Request::new(next));
                }
                return;
            };
            let width = ui.available_width();
            let height = ui.available_height().max(280.0);
            let size = sheet_panes(egui::Pos2::ZERO, width, height, separate_inspector).bounds;
            let (allocated, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let layout = sheet_panes(allocated.min, width, height, separate_inspector);
            let piece = model.as_ref().and_then(|m| m.miniature(stock.id));
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
                inspector_ui
                    .painter()
                    .rect_filled(layout.inspector, 9.0, tw::PANEL);
                egui::ScrollArea::vertical()
                    .id_salt(("sheet-sequence-scroll", stock.id))
                    .max_height(layout.inspector.height())
                    .show(&mut inspector_ui, |ui| {
                        ui.set_width(layout.inspector.width());
                        sheet_inspector(ui, piece, project, localizer, None, None)
                    })
                    .inner
            };
            let mut canvas_ui = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("sheet-canvas-pane", stock.id))
                    .max_rect(layout.canvas),
            );
            canvas_ui.set_clip_rect(layout.canvas.intersect(ui.clip_rect()));
            let view = egui::Rect::from_min_max(
                layout.canvas.min,
                egui::pos2(
                    layout.canvas.right(),
                    (layout.canvas.bottom() - LEGEND_HEIGHT).max(layout.canvas.top() + 60.0),
                ),
            );
            show_legend(
                &canvas_ui,
                egui::pos2(
                    layout.canvas.left() + 14.0,
                    view.bottom() + LEGEND_HEIGHT / 2.0,
                ),
                project.cutting_kerf,
                localizer,
            );
            let mut scroll_ui = canvas_ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("sheet-canvas-view", stock.id))
                    .max_rect(view),
            );
            let mut scroll = egui::ScrollArea::both()
                .id_salt(("sheet-canvas-scroll", stock.id))
                .max_height(view.height())
                .max_width(view.width())
                .auto_shrink([false, false]);
            if toolbar.fit {
                scroll = scroll.scroll_offset(egui::Vec2::ZERO);
            }
            scroll.show(&mut scroll_ui, |ui| {
                let margin = SHEET_PADDING * 2.0 + RULER_ROOM;
                let scale = sheet_scale(
                    stock,
                    (view.width() - margin).max(1.0),
                    (view.height() - margin).max(1.0),
                ) * repair.zoom.max(0.1);
                let sheet_size = egui::vec2(
                    (stock.length.micrometres() as f64 / 1000.0 * f64::from(scale)) as f32,
                    (stock.width.micrometres() as f64 / 1000.0 * f64::from(scale)) as f32,
                )
                .max(egui::vec2(1.0, 1.0));
                let content = egui::vec2(
                    view.width().max(sheet_size.x + margin),
                    view.height().max(sheet_size.y + margin),
                );
                let (canvas, response) =
                    ui.allocate_exact_size(content, egui::Sense::click_and_drag());
                let free = content - sheet_size - egui::vec2(margin, margin);
                let sheet = egui::Rect::from_min_size(
                    canvas.min
                        + egui::vec2(
                            SHEET_PADDING + RULER_ROOM + (free.x / 2.0).max(0.0),
                            SHEET_PADDING + RULER_ROOM + (free.y / 2.0).max(0.0),
                        ),
                    sheet_size,
                );
                if response.hovered() {
                    let zoom = ui.input(|i| i.zoom_delta());
                    if zoom != 1.0 {
                        repair.zoom = (repair.zoom * zoom).clamp(0.25, 4.0);
                    }
                }
                let painter = ui
                    .painter()
                    .with_clip_rect(canvas.intersect(ui.clip_rect()));
                painter.add(
                    egui::Shadow {
                        offset: [0, 6],
                        blur: 20,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(60, 45, 25, 36),
                    }
                    .as_shape(sheet, 0.0),
                );
                painter.rect_filled(sheet, 0.0, SHEET_FILL);
                paint_rulers(&painter, sheet, stock, loc);
                let witness = piece.and_then(|p| p.proof.verified().map(|(tree, _)| tree));
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
                        footprint(project, a)
                            .map(|rect| (allocation_rect(sheet.min, scale, rect), a.board_id))
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
                        && let Some(a) = project.allocations.iter().find(|a| a.board_id == id)
                    {
                        choose_sheet_board(project, selection, id, false);
                        repair.select(project, Some(id), loc);
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
                let mut parts_ui = ui.new_child(egui::UiBuilder::new().max_rect(sheet));
                parts_ui.set_clip_rect(sheet.intersect(ui.clip_rect()));
                for allocation in &allocations {
                    let (Some(board), Some(rect)) = (
                        project.boards.iter().find(|b| b.id == allocation.board_id),
                        footprint(project, allocation),
                    ) else {
                        continue;
                    };
                    let region = allocation_rect(sheet.min, scale, rect);
                    let grain = project
                        .materials
                        .iter()
                        .find(|m| m.id == board.material_id)
                        .and_then(|material| {
                            grain_direction(
                                board.effective_grain(material),
                                allocation.quarter_turn,
                            )
                        });
                    paint_allocation(
                        &parts_ui,
                        region,
                        board,
                        localizer,
                        repair.overlays,
                        grain,
                        PartEmphasis {
                            selected: selection.active == Some(board.id),
                            in_selection: selection.ids.contains(&board.id),
                            conflict: issues.get(&board.id).is_some_and(|v| !v.is_empty()),
                        },
                    );
                    if allocation.locked && region.width() >= 24.0 && region.height() >= 20.0 {
                        icons::icon(Icon::Lock, tw::FAINT, 11.0).paint_at(
                            &parts_ui,
                            egui::Rect::from_min_size(
                                egui::pos2(region.right() - 16.0, region.top() + 5.0),
                                egui::Vec2::splat(11.0),
                            ),
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
                painter.rect_stroke(
                    sheet,
                    0.0,
                    Stroke::new(1.0, SHEET_EDGE),
                    egui::StrokeKind::Outside,
                );
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
                    painter.rect_filled(ghost, 0.0, status.color().gamma_multiply(0.16));
                    painter.rect_stroke(
                        ghost,
                        0.0,
                        Stroke::new(3.0, status.color()),
                        egui::StrokeKind::Outside,
                    );
                    let text = painter.layout_no_wrap(
                        localizer.text(status.key()),
                        egui::FontId::proportional(12.0),
                        tw::PANEL,
                    );
                    let pill = egui::Rect::from_min_size(
                        ghost.left_top() - egui::vec2(0.0, text.size().y + 10.0),
                        text.size() + egui::vec2(12.0, 6.0),
                    );
                    painter.rect_filled(pill, 5.0, status.color());
                    painter.galley(pill.min + egui::vec2(6.0, 3.0), text, tw::PANEL);
                }
                if allocations.is_empty() {
                    let mut empty = ui.new_child(egui::UiBuilder::new().max_rect(sheet).layout(
                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                    ));
                    empty.label(
                        RichText::new(localizer.text("sheet-no-placements"))
                            .size(12.5)
                            .color(tw::FAINT),
                    );
                }
                let pointer_part = response
                    .hover_pos()
                    .and_then(|pointer| hit_board(&hit_regions, canvas, pointer));
                if repair.drag.is_none()
                    && let Some(id) = pointer_part
                    && let Some(board) = project.boards.iter().find(|b| b.id == id)
                {
                    let warning = issues.get(&id).map(|v| issue_text(localizer, v));
                    let allocation = project.allocations.iter().find(|a| a.board_id == id);
                    response.clone().on_hover_ui_at_pointer(|ui| {
                        ui.label(tw::medium(ui, board.name.clone(), 13.0).color(tw::TEXT));
                        ui.label(
                            tw::mono(
                                format!(
                                    "{} · {}",
                                    dims_text(board.length, board.width, loc),
                                    board_short_id(board)
                                ),
                                11.5,
                            )
                            .color(tw::MUTED),
                        );
                        if let Some(allocation) = allocation {
                            ui.label(
                                tw::mono(
                                    format!(
                                        "X {} · Y {}",
                                        mm_text(allocation.origin[0], loc),
                                        mm_text(allocation.origin[1], loc)
                                    ),
                                    11.5,
                                )
                                .color(tw::FAINT),
                            );
                        }
                        if let Some(warning) = warning {
                            ui.label(
                                RichText::new(capitalized(warning))
                                    .size(12.0)
                                    .color(tw::DANGER),
                            );
                        }
                    });
                }
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
            });
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
            repair.select(editor.preview().unwrap_or(editor.project()), Some(id), loc);
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
        repair.cancel_navigation(editor);
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
mod tests;
