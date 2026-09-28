//! Constructive guillotine packing. Every placement splits a free offcut of a
//! sheet's cutting tree, so a full-span cut sequence exists by construction
//! and no placement has to be proposed and then proven.
//!
//! Boards are packed per material and thickness. Several sort orders, fit
//! rules and split rules run on each group; the best group result is kept.
//! A board that cannot be placed gets a reason, and `suggest_sheets` works out
//! how many more sheets of a material would let the waiting boards fit.
use std::collections::{BTreeMap, HashMap, HashSet};

use uuid::Uuid;

use crate::candidate_generation::{Candidate, StockWitness, allocation_id, stock_order};
use crate::cut_tree::{
    Axis, CutKind, CutTree, PieceId, Reconstruction, reconstruct_witness_ordered, validate_witness,
};
use crate::domain::{
    Allocation, Board, BoardGrain, Material, Project, Stock, StockGrain, StockSource,
};
use crate::material_presets::preset_for;
use crate::money::Money;
use crate::units::Length;

const WITNESS_BUDGET: usize = 20_000;
/// Upper bound on sheets `suggest_sheets` will propose for one material.
const MAX_SUGGESTED: usize = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackMode {
    /// Keep every existing placement; only unallocated boards are placed.
    FillGaps,
    /// Keep locked placements; every other board may move.
    Replan,
}

/// Why a board is still waiting after packing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Unplaced {
    /// No declared sheet has this board's material and thickness.
    NoStock,
    /// No sheet of the material is big enough, in any orientation.
    TooLarge,
    /// It would fit only turned, and the grain forbids that.
    Grain,
    /// The declared sheets are full.
    NoRoom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackResult {
    /// The complete allocation list for the project.
    pub allocations: Vec<Allocation>,
    pub unplaced: Vec<(Uuid, Unplaced)>,
    /// Boards that were placed or moved by this pack.
    pub changed: Vec<Uuid>,
    /// Trees for the sheets this pack built or extended.
    witnesses: Vec<(Uuid, CutTree)>,
}

impl PackResult {
    pub fn is_complete(&self) -> bool {
        self.unplaced.is_empty()
    }

    /// Stock pieces that hold at least one board.
    pub fn sheets_used(&self) -> usize {
        self.allocations
            .iter()
            .map(|a| a.stock_id)
            .collect::<HashSet<_>>()
            .len()
    }

    /// A complete result as an optimizer candidate, with each tree checked
    /// against the resulting project.
    pub fn candidate(&self, project: &Project) -> Option<Candidate> {
        if !self.is_complete() {
            return None;
        }
        let mut copy = project.clone();
        copy.allocations = self.allocations.clone();
        let used: HashSet<_> = self.allocations.iter().map(|a| a.stock_id).collect();
        let mut witnesses = Vec::new();
        for stock in project.ordered_stock() {
            if !used.contains(&stock.id) {
                continue;
            }
            let tree = match self.witnesses.iter().find(|(id, _)| *id == stock.id) {
                Some((_, tree)) => tree.clone(),
                None => proven_tree(&copy, stock.id)?,
            };
            let accounting = validate_witness(&tree, &copy, stock.id).ok()?;
            witnesses.push(StockWitness {
                stock_id: stock.id,
                tree,
                accounting,
            });
        }
        Some(Candidate {
            allocations: self.allocations.clone(),
            witnesses,
        })
    }
}

/// A sheet the user could add, and how many would let the waiting boards fit.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetSuggestion {
    pub material_id: Uuid,
    pub thickness: Length,
    /// Boards of this material and thickness that are still waiting.
    pub waiting: usize,
    /// Those boards, in pack order.
    pub boards: Vec<Uuid>,
    /// Of those, boards that more sheets of this size would not help.
    pub blocked: Vec<(Uuid, Unplaced)>,
    pub sheet: Option<SuggestedSheet>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SuggestedSheet {
    pub name: String,
    pub length: Length,
    pub width: Length,
    pub grain: StockGrain,
    pub trim: [Length; 4],
    pub price: Option<Money>,
    /// Zero when the waiting boards cannot be placed on sheets of this size.
    pub count: usize,
}

type Group = (Uuid, Length);

#[derive(Clone, Copy)]
enum Order {
    Area,
    LongSide,
    Length,
    Width,
    Perimeter,
}

#[derive(Clone, Copy)]
enum Fit {
    ShortSide,
    Area,
}

#[derive(Clone, Copy)]
enum SplitRule {
    XFirst,
    YFirst,
    KeepLargest,
    KeepSquare,
}

#[derive(Clone, Copy)]
struct Strategy {
    order: Order,
    fit: Fit,
    split: SplitRule,
}

fn strategies() -> Vec<Strategy> {
    let mut all = Vec::new();
    for order in [
        Order::Area,
        Order::LongSide,
        Order::Length,
        Order::Width,
        Order::Perimeter,
    ] {
        for fit in [Fit::ShortSide, Fit::Area] {
            for split in [
                SplitRule::KeepLargest,
                SplitRule::XFirst,
                SplitRule::YFirst,
                SplitRule::KeepSquare,
            ] {
                all.push(Strategy { order, fit, split });
            }
        }
    }
    all
}

fn um(length: Length) -> i64 {
    length.micrometres()
}

/// Whether the board may lie on this stock in the given orientation.
pub(crate) fn grain_allows(grain: BoardGrain, stock: StockGrain, turn: bool) -> bool {
    let required = match grain {
        BoardGrain::Unrestricted => return true,
        BoardGrain::Length => {
            if turn {
                StockGrain::AlongY
            } else {
                StockGrain::AlongX
            }
        }
        BoardGrain::Width => {
            if turn {
                StockGrain::AlongX
            } else {
                StockGrain::AlongY
            }
        }
    };
    stock == StockGrain::Nondirectional || stock == required
}

/// A remainder is either absent or wide enough to hold the blade band.
fn cuttable(room: i64, need: i64, kerf: i64) -> bool {
    room == need || room - need >= kerf
}

fn fits(rect: [i64; 2], part: [i64; 2], kerf: i64) -> bool {
    part[0] <= rect[0]
        && part[1] <= rect[1]
        && cuttable(rect[0], part[0], kerf)
        && cuttable(rect[1], part[1], kerf)
}

fn proven_tree(project: &Project, stock_id: Uuid) -> Option<CutTree> {
    for axes in [[Axis::X, Axis::Y], [Axis::Y, Axis::X]] {
        if let Reconstruction::Verified { tree, .. } = reconstruct_witness_ordered(
            project,
            stock_id,
            project.cutting_kerf,
            WITNESS_BUDGET,
            axes,
        ) {
            return Some(tree);
        }
    }
    None
}

struct Sheet {
    stock_id: Uuid,
    grain: StockGrain,
    tree: CutTree,
    /// Holds a part already; open sheets are filled before new ones.
    open: bool,
    /// The tree changed during this pack.
    touched: bool,
}

struct Piece<'a> {
    board: &'a Board,
    grain: BoardGrain,
}

fn extent(board: &Board, turn: bool) -> [i64; 2] {
    if turn {
        [um(board.width), um(board.length)]
    } else {
        [um(board.length), um(board.width)]
    }
}

fn sort_pieces(pieces: &mut [Piece<'_>], order: Order) {
    let key = |p: &Piece<'_>| -> i128 {
        let (l, w) = (
            i128::from(um(p.board.length)),
            i128::from(um(p.board.width)),
        );
        match order {
            Order::Area => l * w,
            Order::LongSide => l.max(w),
            Order::Length => l,
            Order::Width => w,
            Order::Perimeter => l + w,
        }
    };
    pieces.sort_by(|a, b| {
        key(b)
            .cmp(&key(a))
            .then(b.board.length.cmp(&a.board.length))
            .then(a.board.id.cmp(&b.board.id))
    });
}

/// The best free offcut on one sheet: (score, leaf, turn).
fn best_leaf(
    sheet: &Sheet,
    piece: &Piece<'_>,
    fit: Fit,
    kerf: i64,
) -> Option<((i128, i128), PieceId, bool)> {
    let mut best = None;
    for (index, node) in sheet.tree.nodes().iter().enumerate() {
        if node.kind != CutKind::Offcut {
            continue;
        }
        let rect = node.rectangle.extent.map(um);
        for turn in [false, true] {
            if turn && piece.board.length == piece.board.width {
                continue;
            }
            if !grain_allows(piece.grain, sheet.grain, turn) {
                continue;
            }
            let part = extent(piece.board, turn);
            if !fits(rect, part, kerf) {
                continue;
            }
            let dx = i128::from(rect[0] - part[0]);
            let dy = i128::from(rect[1] - part[1]);
            let score = match fit {
                Fit::ShortSide => (dx.min(dy), dx.max(dy)),
                Fit::Area => (
                    i128::from(rect[0]) * i128::from(rect[1])
                        - i128::from(part[0]) * i128::from(part[1]),
                    dx.min(dy),
                ),
            };
            if best.as_ref().is_none_or(|(s, _, _)| score < *s) {
                best = Some((score, index, turn));
            }
        }
    }
    best
}

/// Cut the part out of the leaf's low corner and return its origin.
fn place(
    tree: &mut CutTree,
    leaf: PieceId,
    part: [i64; 2],
    board: Uuid,
    rule: SplitRule,
) -> Option<[Length; 2]> {
    let rect = tree.node(leaf)?.rectangle;
    let whole = rect.extent.map(um);
    let kerf = um(tree.kerf());
    let x_first = match rule {
        SplitRule::XFirst => true,
        SplitRule::YFirst => false,
        SplitRule::KeepLargest | SplitRule::KeepSquare => {
            // X first leaves a full-height strip on the right; Y first a
            // full-width strip on top.
            let right = [(whole[0] - part[0] - kerf).max(0), whole[1]];
            let top = [whole[0], (whole[1] - part[1] - kerf).max(0)];
            let area = |r: [i64; 2]| i128::from(r[0]) * i128::from(r[1]);
            let short = |r: [i64; 2]| r[0].min(r[1]);
            if matches!(rule, SplitRule::KeepLargest) {
                area(right) >= area(top)
            } else {
                short(right) >= short(top)
            }
        }
    };
    let axes = if x_first {
        [Axis::X, Axis::Y]
    } else {
        [Axis::Y, Axis::X]
    };
    let mut piece = leaf;
    for axis in axes {
        let index = if axis == Axis::X { 0 } else { 1 };
        if whole[index] > part[index] {
            piece = tree
                .split(piece, axis, Length::from_micrometres(part[index]))
                .ok()?
                .first;
        }
    }
    tree.finish_part(piece, board, part.map(Length::from_micrometres))
        .ok()?;
    Some(rect.origin)
}

struct GroupOutcome {
    placed: Vec<Allocation>,
    unplaced: Vec<Uuid>,
    sheets: Vec<Sheet>,
}

impl GroupOutcome {
    /// Lower is better: fewer waiting boards, fewer sheets, earlier sheets in
    /// the user's priority order, fewer cuts, then one big offcut over many small.
    fn score(&self, order: &HashMap<Uuid, usize>) -> (usize, usize, usize, usize, i128) {
        let used: Vec<_> = self.sheets.iter().filter(|s| s.open).collect();
        let cuts = used.iter().map(|s| s.tree.cut_count()).sum();
        let largest = used
            .iter()
            .flat_map(|s| s.tree.nodes())
            .filter(|n| n.kind == CutKind::Offcut)
            .map(|n| i128::from(um(n.rectangle.extent[0])) * i128::from(um(n.rectangle.extent[1])))
            .max()
            .unwrap_or(0);
        (
            self.unplaced.len(),
            used.len(),
            used.iter().map(|s| order[&s.stock_id]).sum(),
            cuts,
            -largest,
        )
    }
}

fn pack_group(
    kerf: i64,
    base: &[Sheet],
    pieces: &mut [Piece<'_>],
    strategy: Strategy,
) -> GroupOutcome {
    let mut sheets: Vec<Sheet> = base
        .iter()
        .map(|s| Sheet {
            stock_id: s.stock_id,
            grain: s.grain,
            tree: s.tree.clone(),
            open: s.open,
            touched: false,
        })
        .collect();
    sort_pieces(pieces, strategy.order);
    let mut placed = Vec::new();
    let mut unplaced = Vec::new();
    for piece in pieces.iter() {
        // Fill sheets already in use before opening the next one.
        let mut choice: Option<((i128, i128), usize, PieceId, bool)> = None;
        for (index, sheet) in sheets.iter().enumerate().filter(|(_, s)| s.open) {
            if let Some((score, leaf, turn)) = best_leaf(sheet, piece, strategy.fit, kerf)
                && choice.as_ref().is_none_or(|(s, ..)| score < *s)
            {
                choice = Some((score, index, leaf, turn));
            }
        }
        if choice.is_none() {
            choice =
                sheets
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| !s.open)
                    .find_map(|(index, sheet)| {
                        best_leaf(sheet, piece, strategy.fit, kerf)
                            .map(|(score, leaf, turn)| (score, index, leaf, turn))
                    });
        }
        let Some((_, index, leaf, turn)) = choice else {
            unplaced.push(piece.board.id);
            continue;
        };
        let sheet = &mut sheets[index];
        let part = extent(piece.board, turn);
        let Some(origin) = place(&mut sheet.tree, leaf, part, piece.board.id, strategy.split)
        else {
            unplaced.push(piece.board.id);
            continue;
        };
        sheet.open = true;
        sheet.touched = true;
        placed.push(Allocation {
            id: allocation_id(piece.board.id, sheet.stock_id, origin, turn),
            board_id: piece.board.id,
            stock_id: sheet.stock_id,
            origin,
            quarter_turn: turn,
            locked: false,
        });
    }
    GroupOutcome {
        placed,
        unplaced,
        sheets,
    }
}

fn material(project: &Project, id: Uuid) -> Option<&Material> {
    project.materials.iter().find(|m| m.id == id)
}

fn usable(stock: &Stock) -> [i64; 2] {
    let t = stock.trim.map(um);
    [
        um(stock.length) - t[0] - t[1],
        um(stock.width) - t[2] - t[3],
    ]
}

/// Why a board that did not fit is waiting.
fn reason(project: &Project, board: &Board, grain: BoardGrain) -> Unplaced {
    let kerf = um(project.cutting_kerf);
    let group: Vec<_> = project
        .stock
        .iter()
        .filter(|s| s.material_id == board.material_id && s.thickness == board.thickness)
        .collect();
    if group.is_empty() {
        return Unplaced::NoStock;
    }
    let mut any_size = false;
    for stock in &group {
        for turn in [false, true] {
            if fits(usable(stock), extent(board, turn), kerf) {
                any_size = true;
                if grain_allows(grain, stock.grain, turn) {
                    return Unplaced::NoRoom;
                }
            }
        }
    }
    if any_size {
        Unplaced::Grain
    } else {
        Unplaced::TooLarge
    }
}

/// Pack every waiting board (or, with `Replan`, every unlocked board) onto the
/// declared stock. Existing placements the mode keeps are never moved. Pure:
/// the caller decides whether to apply `allocations`.
pub fn pack(project: &Project, mode: PackMode) -> PackResult {
    pack_with(project, mode, None, &strategies(), false)
}

/// Distinct complete plans for the optimizer to rank: the best packing, each
/// single strategy (they trade sheets, cuts and offcuts differently) and, when
/// boards are already placed, one that keeps them. Empty when some board
/// cannot be placed at all; `unreachable` then says the search cannot help.
pub(crate) fn candidates(project: &Project) -> (Vec<Candidate>, bool) {
    let all = strategies();
    let best = pack_with(project, PackMode::Replan, None, &all, false);
    let unreachable = best.unplaced.iter().any(|(_, r)| *r != Unplaced::NoRoom);
    let mut results = vec![best];
    // Owned and cheaper sheets first, for the lowest-spending objective.
    results.push(pack_with(project, PackMode::Replan, None, &all, true));
    if !project.allocations.is_empty() {
        results.push(pack_with(project, PackMode::FillGaps, None, &all, false));
    }
    for strategy in all {
        results.push(pack_with(
            project,
            PackMode::Replan,
            None,
            &[strategy],
            false,
        ));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for result in results {
        let mut key: Vec<_> = result
            .allocations
            .iter()
            .map(|a| (a.board_id, a.stock_id, a.origin.map(um), a.quarter_turn))
            .collect();
        key.sort();
        if seen.insert(key)
            && let Some(candidate) = result.candidate(project)
        {
            out.push(candidate);
        }
    }
    (out, unreachable)
}

fn pack_with(
    project: &Project,
    mode: PackMode,
    only: Option<Group>,
    strategies: &[Strategy],
    cost_aware: bool,
) -> PackResult {
    let kerf = um(project.cutting_kerf);
    let kept: Vec<Allocation> = project
        .allocations
        .iter()
        .filter(|a| match mode {
            PackMode::FillGaps => true,
            PackMode::Replan => a.locked,
        })
        .cloned()
        .collect();
    let kept_boards: HashSet<_> = kept.iter().map(|a| a.board_id).collect();
    let mut kept_project = project.clone();
    kept_project.allocations = kept.clone();

    let mut groups: BTreeMap<Group, Vec<Piece<'_>>> = BTreeMap::new();
    let mut unplaced = Vec::new();
    for board in &project.boards {
        if kept_boards.contains(&board.id) {
            continue;
        }
        let key = (board.material_id, board.thickness);
        if only.is_some_and(|g| g != key) {
            continue;
        }
        let Some(material) = material(project, board.material_id) else {
            unplaced.push((board.id, Unplaced::NoStock));
            continue;
        };
        groups.entry(key).or_default().push(Piece {
            board,
            grain: board.effective_grain(material),
        });
    }

    let ordered = stock_order(project, cost_aware);
    let order: HashMap<Uuid, usize> = ordered
        .iter()
        .enumerate()
        .map(|(index, s)| (s.id, index))
        .collect();
    let mut allocations = kept;
    let mut changed = Vec::new();
    let mut witnesses = Vec::new();
    for ((material_id, thickness), mut pieces) in groups {
        let mut base = Vec::new();
        for stock in &ordered {
            if stock.material_id != material_id || stock.thickness != thickness {
                continue;
            }
            let open = kept_project
                .allocations
                .iter()
                .any(|a| a.stock_id == stock.id);
            // A sheet whose kept placements cannot be proven takes no more parts.
            let Some(tree) = proven_tree(&kept_project, stock.id) else {
                continue;
            };
            base.push(Sheet {
                stock_id: stock.id,
                grain: stock.grain,
                tree,
                open,
                touched: false,
            });
        }
        let mut best: Option<GroupOutcome> = None;
        for &strategy in strategies {
            let outcome = pack_group(kerf, &base, &mut pieces, strategy);
            let better = best
                .as_ref()
                .is_none_or(|b| outcome.score(&order) < b.score(&order));
            if better {
                best = Some(outcome);
            }
            if best.as_ref().is_some_and(|b| {
                b.unplaced.is_empty() && b.sheets.iter().filter(|s| s.open).count() <= 1
            }) {
                break;
            }
        }
        let Some(best) = best else {
            continue;
        };
        for board_id in &best.unplaced {
            let piece = pieces
                .iter()
                .find(|p| p.board.id == *board_id)
                .expect("unplaced board came from this group");
            unplaced.push((*board_id, reason(project, piece.board, piece.grain)));
        }
        for allocation in best.placed {
            let moved = !project.allocations.iter().any(|a| {
                a.board_id == allocation.board_id
                    && a.stock_id == allocation.stock_id
                    && a.origin == allocation.origin
                    && a.quarter_turn == allocation.quarter_turn
            });
            if moved {
                changed.push(allocation.board_id);
            }
            allocations.push(allocation);
        }
        witnesses.extend(
            best.sheets
                .into_iter()
                .filter(|s| s.touched)
                .map(|s| (s.stock_id, s.tree)),
        );
    }
    unplaced.sort_by_key(|(id, reason)| (*reason, *id));
    PackResult {
        allocations,
        unplaced,
        changed,
        witnesses,
    }
}

fn template(project: &Project, group: Group) -> Option<SuggestedSheet> {
    let (material_id, thickness) = group;
    let material = material(project, material_id)?;
    // The biggest declared sheet of the group is the best guide to what the
    // user buys; otherwise the standard size of a preset material.
    let declared = project
        .stock
        .iter()
        .filter(|s| s.material_id == material_id && s.thickness == thickness)
        .max_by_key(|s| {
            (
                i128::from(um(s.length)) * i128::from(um(s.width)),
                s.priority,
            )
        });
    if let Some(stock) = declared {
        return Some(SuggestedSheet {
            name: stock.name.clone(),
            length: stock.length,
            width: stock.width,
            grain: stock.grain,
            trim: stock.trim,
            price: (stock.source == StockSource::ToPurchase)
                .then_some(stock.price)
                .flatten(),
            count: 0,
        });
    }
    let preset = preset_for(material).filter(|p| p.thickness() == thickness)?;
    let [length, width] = preset.sheet_size();
    Some(SuggestedSheet {
        name: material.name.clone(),
        length,
        width,
        grain: match material.default_grain {
            BoardGrain::Unrestricted => StockGrain::Nondirectional,
            BoardGrain::Length | BoardGrain::Width => StockGrain::AlongX,
        },
        trim: [Length::ZERO; 4],
        price: None,
        count: 0,
    })
}

/// The stock pieces `add_suggested` would create.
pub fn suggested_stock(project: &Project, group: Group, sheet: &SuggestedSheet) -> Vec<Stock> {
    let first = project
        .stock
        .iter()
        .map(|s| s.priority + 1)
        .max()
        .unwrap_or(0);
    (0..sheet.count)
        .map(|index| Stock {
            id: Uuid::new_v4(),
            name: sheet.name.clone(),
            material_id: group.0,
            length: sheet.length,
            width: sheet.width,
            thickness: group.1,
            grain: sheet.grain,
            source: StockSource::ToPurchase,
            price: sheet.price,
            priority: first + index as u32,
            trim: sheet.trim,
        })
        .collect()
}

/// For each material with waiting boards: why they wait, and how many more
/// sheets (like the biggest one declared, or the material's standard size)
/// would let them fit around the current placements.
pub fn suggest_sheets(project: &Project) -> Vec<SheetSuggestion> {
    let current = pack(project, PackMode::FillGaps);
    let mut waiting: BTreeMap<Group, Vec<(Uuid, Unplaced)>> = BTreeMap::new();
    for (board_id, reason) in &current.unplaced {
        if let Some(board) = project.boards.iter().find(|b| b.id == *board_id) {
            waiting
                .entry((board.material_id, board.thickness))
                .or_default()
                .push((*board_id, *reason));
        }
    }
    let mut suggestions = Vec::new();
    for (group, boards) in waiting {
        let mut sheet = template(project, group);
        let mut blocked = Vec::new();
        if let Some(sheet) = sheet.as_mut() {
            let size = [um(sheet.length), um(sheet.width)];
            let t = sheet.trim.map(um);
            let room = [size[0] - t[0] - t[1], size[1] - t[2] - t[3]];
            let kerf = um(project.cutting_kerf);
            let material = material(project, group.0);
            let mut area = 0i128;
            for (board_id, _) in &boards {
                let Some(board) = project.boards.iter().find(|b| b.id == *board_id) else {
                    continue;
                };
                let grain = material.map_or(BoardGrain::Unrestricted, |m| board.effective_grain(m));
                let size_ok = [false, true]
                    .into_iter()
                    .any(|turn| fits(room, extent(board, turn), kerf));
                let grain_ok = [false, true].into_iter().any(|turn| {
                    fits(room, extent(board, turn), kerf) && grain_allows(grain, sheet.grain, turn)
                });
                if !size_ok {
                    blocked.push((*board_id, Unplaced::TooLarge));
                } else if !grain_ok {
                    blocked.push((*board_id, Unplaced::Grain));
                } else {
                    area += i128::from(um(board.length)) * i128::from(um(board.width));
                }
            }
            if blocked.len() < boards.len() {
                let per_sheet = (i128::from(room[0]) * i128::from(room[1])).max(1);
                let mut count = usize::try_from((area + per_sheet - 1) / per_sheet)
                    .unwrap_or(MAX_SUGGESTED)
                    .max(1);
                // Room left on the declared sheets may already cover part of it.
                count = count.saturating_sub(1).max(1);
                while count <= MAX_SUGGESTED {
                    sheet.count = count;
                    let mut trial = project.clone();
                    trial.stock.extend(suggested_stock(project, group, sheet));
                    let result = pack_with(
                        &trial,
                        PackMode::FillGaps,
                        Some(group),
                        &strategies(),
                        false,
                    );
                    let still = result
                        .unplaced
                        .iter()
                        .filter(|(id, _)| !blocked.iter().any(|(b, _)| b == id))
                        .count();
                    if still == 0 {
                        break;
                    }
                    count += 1;
                }
                if count > MAX_SUGGESTED {
                    sheet.count = 0;
                }
            }
        } else {
            blocked = boards
                .iter()
                .filter(|(_, r)| matches!(r, Unplaced::TooLarge | Unplaced::Grain))
                .copied()
                .collect();
        }
        suggestions.push(SheetSuggestion {
            material_id: group.0,
            thickness: group.1,
            waiting: boards.len(),
            boards: boards.iter().map(|(id, _)| *id).collect(),
            blocked,
            sheet,
        });
    }
    suggestions
}

#[cfg(test)]
mod tests;
